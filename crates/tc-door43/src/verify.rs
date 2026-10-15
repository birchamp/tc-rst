//! Checking that downloaded bytes are the commit they claim to be. Pure functions over
//! bytes, so an app can also check a zip a person picked from a USB drive.
//!
//! # A Gitea archive at a commit (`/archive/<sha>.zip`)
//!
//! [`verify_archive`] accepts the bytes only when all of these hold:
//!
//! 1. The request named the full commit sha, so no tag or branch was resolved on the way.
//! 2. The zip's end-of-central-directory comment is that sha. Gitea runs `git archive`,
//!    which writes the archived commit's id there. This catches an archive of another
//!    commit (tC4 `resourceFetch.ts` verified the comment on Door43 with `unzip -z`).
//! 3. Every entry decompresses and passes its CRC-32, no entry name can escape the
//!    folder, and every entry sits in one top folder (Gitea's `<repo>/` prefix).
//! 4. The unpacked files hash to the commit's git tree id. The client reads that id from
//!    `GET /repos/{owner}/{repo}/git/commits/{sha}`. This is the content check: equal tree
//!    ids mean every path, mode and byte equals the commit's, so the same sha gives the
//!    same bytes on every download (decisions D96/D97). It refuses an archive that
//!    `.gitattributes` changed on the way out (`export-ignore`, `export-subst`, line-ending
//!    filters) and one with a submodule, because those bytes are not the commit's.
//!
//! The sha and the tree id both come from the server the bytes come from. The checks bind
//! the bytes to the commit the server names; they do not prove the server honest.
//!
//! # The DCS Scripture Burrito export (`/<owner>/<repo>/sb/<tag>.zip`)
//!
//! DCS builds this zip on each request from the tag's commit. [`verify_sb_export`] accepts
//! it only when its `metadata.json` declares exactly one DCS revision
//! (`identification.primary.dcs["<owner>/<repo>"].revision`), that revision equals the
//! expected sha, and every ingredient the zip holds has the md5 and size the metadata
//! gives. The expected sha must come from outside the zip: from the pin, or on a first
//! install from the tags API, so the archive never certifies itself. A matching revision
//! does not prove that every byte comes from that commit: the `en_tw` export takes its TWL
//! files from `en_twl` master (D96). Use the archive when the bytes must be reproducible.

use crate::error::VerificationError;
use crate::git_tree::{ObjectId, tree_id};
use crate::names::CommitSha;
use crate::unpack::{UnpackedFile, Wrapping, unpack};
use md5::{Digest, Md5};
use serde_json::Value;

/// A Gitea archive whose files are the content of `commit`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedArchive {
    pub commit: CommitSha,
    pub tree: ObjectId,
    /// Sorted by path, relative to the repository root.
    pub files: Vec<UnpackedFile>,
}

/// A DCS burrito export whose `metadata.json` declares the expected revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedSbExport {
    pub revision: CommitSha,
    /// Sorted by path, relative to the burrito root (`metadata.json`, `ingredients/...`).
    /// The export's `.git/` and `.DS_Store` entries are dropped.
    pub files: Vec<UnpackedFile>,
    /// Ingredients `metadata.json` lists that the zip does not hold. tC4's evidence says
    /// some exports list articles they do not ship; the caller decides what that means.
    pub missing_ingredients: Vec<String>,
}

pub fn verify_archive(
    zip: &[u8],
    commit: &CommitSha,
    commit_tree: &ObjectId,
    max_unpacked: u64,
) -> Result<VerifiedArchive, VerificationError> {
    let unpacked = unpack(zip, Wrapping::Required, max_unpacked)?;
    let comment = String::from_utf8_lossy(&unpacked.comment);
    let comment = comment.trim();
    if comment.is_empty() {
        return Err(VerificationError::ArchiveCommentMissing);
    }
    if !comment.eq_ignore_ascii_case(commit.as_str()) {
        return Err(VerificationError::ArchiveCommentMismatch {
            expected: commit.clone(),
            found: comment.chars().take(80).collect(),
        });
    }
    let computed = tree_id(&unpacked.files, &unpacked.folders)?;
    if computed != *commit_tree {
        return Err(VerificationError::TreeMismatch {
            commit: commit.clone(),
            expected: *commit_tree,
            computed,
        });
    }
    Ok(VerifiedArchive {
        commit: commit.clone(),
        tree: computed,
        files: unpacked.files,
    })
}

pub fn verify_sb_export(
    zip: &[u8],
    expected: &CommitSha,
    max_unpacked: u64,
) -> Result<VerifiedSbExport, VerificationError> {
    let unpacked = unpack(zip, Wrapping::IfSingleTopFolder, max_unpacked)?;
    let files: Vec<UnpackedFile> = unpacked
        .files
        .into_iter()
        .filter(|f| !is_export_debris(&f.path))
        .collect();
    let metadata = files
        .iter()
        .find(|f| f.path == "metadata.json")
        .ok_or(VerificationError::MetadataMissing)?;
    if !files.iter().any(|f| f.path.starts_with("ingredients/")) {
        return Err(VerificationError::NoIngredients);
    }
    let metadata: Value = serde_json::from_slice(&metadata.bytes)
        .map_err(|e| VerificationError::MetadataUnreadable(e.to_string()))?;

    let declared = declared_revision(&metadata)?;
    if !declared.eq_ignore_ascii_case(expected.as_str()) {
        return Err(VerificationError::RevisionMismatch {
            expected: expected.clone(),
            declared: declared.chars().take(80).collect(),
        });
    }
    let missing_ingredients = check_ingredients(&metadata, &files)?;
    Ok(VerifiedSbExport {
        revision: expected.clone(),
        files,
        missing_ingredients,
    })
}

/// The export ships the repository's `.git/` folder and macOS `.DS_Store` files; neither
/// belongs to the burrito.
fn is_export_debris(path: &str) -> bool {
    path == ".git"
        || path.starts_with(".git/")
        || path == ".DS_Store"
        || path.ends_with("/.DS_Store")
}

fn declared_revision(metadata: &Value) -> Result<&str, VerificationError> {
    let dcs = metadata
        .pointer("/identification/primary/dcs")
        .and_then(Value::as_object)
        .ok_or(VerificationError::RevisionMissing)?;
    let mut revisions = dcs
        .values()
        .map(|v| v.get("revision").and_then(Value::as_str));
    match (revisions.next(), dcs.len()) {
        (Some(Some(revision)), 1) => Ok(revision),
        (Some(None), 1) | (None, _) => Err(VerificationError::RevisionMissing),
        (_, count) => Err(VerificationError::RevisionAmbiguous(count)),
    }
}

fn check_ingredients(
    metadata: &Value,
    files: &[UnpackedFile],
) -> Result<Vec<String>, VerificationError> {
    let Some(ingredients) = metadata.get("ingredients").and_then(Value::as_object) else {
        return Err(VerificationError::MetadataUnreadable(
            "it has no ingredients table".into(),
        ));
    };
    let mut missing = Vec::new();
    for (path, entry) in ingredients {
        let Some(file) = files.iter().find(|f| &f.path == path) else {
            missing.push(path.clone());
            continue;
        };
        let mismatch = |detail: String| VerificationError::IngredientMismatch {
            path: path.clone(),
            detail,
        };
        let size = entry
            .get("size")
            .and_then(Value::as_u64)
            .ok_or_else(|| mismatch("the metadata gives no size".into()))?;
        if size != file.bytes.len() as u64 {
            return Err(mismatch(format!(
                "size {} where the metadata says {size}",
                file.bytes.len()
            )));
        }
        let md5 = entry
            .pointer("/checksum/md5")
            .and_then(Value::as_str)
            .ok_or_else(|| mismatch("the metadata gives no md5".into()))?;
        let actual = hex(&Md5::digest(&file.bytes));
        if !md5.eq_ignore_ascii_case(&actual) {
            return Err(mismatch(format!(
                "md5 {actual} where the metadata says {md5}"
            )));
        }
    }
    Ok(missing)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unpack::FileMode;
    use crate::unpack::tests::zip_of;

    const SHA: &str = "c354b8ae66a23c485bf6f38fd35bd8f7ef81e4e5";
    const OTHER: &str = "570e76d0024c847689e48a20e2ac1a1d2c6eb6e3";

    fn sha(s: &str) -> CommitSha {
        s.parse().unwrap()
    }

    fn metadata(revisions: &[(&str, &str)], ingredient: &[u8]) -> Vec<u8> {
        let dcs: serde_json::Map<String, Value> = revisions
            .iter()
            .map(|(k, r)| ((*k).to_owned(), serde_json::json!({ "revision": r })))
            .collect();
        serde_json::to_vec(&serde_json::json!({
            "format": "scripture burrito",
            "identification": { "primary": { "dcs": dcs } },
            "ingredients": {
                "ingredients/TIT.tsv": {
                    "checksum": { "md5": hex(&Md5::digest(ingredient)) },
                    "size": ingredient.len(),
                },
                "ingredients/JON.tsv": { "checksum": { "md5": "00" }, "size": 1 },
            }
        }))
        .unwrap()
    }

    fn export(meta: &[u8], tsv: &[u8]) -> Vec<u8> {
        zip_of(
            &[
                ("en_tn/", FileMode::Regular, b""),
                ("en_tn/.DS_Store", FileMode::Regular, b"junk"),
                (
                    "en_tn/.git/HEAD",
                    FileMode::Regular,
                    b"ref: refs/heads/master",
                ),
                ("en_tn/metadata.json", FileMode::Regular, meta),
                ("en_tn/ingredients/TIT.tsv", FileMode::Regular, tsv),
            ],
            "",
        )
    }

    #[test]
    fn an_export_with_the_expected_revision_is_accepted() {
        let tsv = b"Reference\tID\n1:1\tabcd\n";
        let zip = export(&metadata(&[("unfoldingWord/en_tn", SHA)], tsv), tsv);
        let verified = verify_sb_export(&zip, &sha(SHA), 1 << 20).unwrap();
        let paths: Vec<_> = verified.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["ingredients/TIT.tsv", "metadata.json"]);
        assert_eq!(verified.missing_ingredients, ["ingredients/JON.tsv"]);
        assert_eq!(verified.revision, sha(SHA));
    }

    #[test]
    fn an_export_of_another_commit_is_refused() {
        let tsv = b"x";
        let zip = export(&metadata(&[("unfoldingWord/en_tn", OTHER)], tsv), tsv);
        assert_eq!(
            verify_sb_export(&zip, &sha(SHA), 1 << 20).unwrap_err(),
            VerificationError::RevisionMismatch {
                expected: sha(SHA),
                declared: OTHER.into()
            }
        );
    }

    #[test]
    fn a_missing_or_doubled_revision_is_refused() {
        let tsv = b"x";
        let none = export(&metadata(&[], tsv), tsv);
        assert_eq!(
            verify_sb_export(&none, &sha(SHA), 1 << 20).unwrap_err(),
            VerificationError::RevisionMissing
        );
        let two = export(&metadata(&[("a/b", SHA), ("c/d", SHA)], tsv), tsv);
        assert_eq!(
            verify_sb_export(&two, &sha(SHA), 1 << 20).unwrap_err(),
            VerificationError::RevisionAmbiguous(2)
        );
    }

    #[test]
    fn an_ingredient_that_differs_from_the_metadata_is_refused() {
        let zip = export(
            &metadata(&[("unfoldingWord/en_tn", SHA)], b"original"),
            b"tampered",
        );
        assert!(matches!(
            verify_sb_export(&zip, &sha(SHA), 1 << 20),
            Err(VerificationError::IngredientMismatch { path, .. }) if path == "ingredients/TIT.tsv"
        ));
    }

    #[test]
    fn a_zip_without_burrito_files_is_refused() {
        let no_meta = zip_of(&[("r/ingredients/TIT.tsv", FileMode::Regular, b"x")], "");
        assert_eq!(
            verify_sb_export(&no_meta, &sha(SHA), 1 << 20).unwrap_err(),
            VerificationError::MetadataMissing
        );
        let no_ingredients = zip_of(&[("r/metadata.json", FileMode::Regular, b"{}")], "");
        assert_eq!(
            verify_sb_export(&no_ingredients, &sha(SHA), 1 << 20).unwrap_err(),
            VerificationError::NoIngredients
        );
    }

    fn archive(comment: &str, readme: &[u8]) -> Vec<u8> {
        zip_of(
            &[
                ("en_ugl/", FileMode::Regular, b""),
                ("en_ugl/README.md", FileMode::Regular, readme),
            ],
            comment,
        )
    }

    fn tree_of(readme: &[u8]) -> ObjectId {
        let files = [UnpackedFile {
            path: "README.md".into(),
            mode: FileMode::Regular,
            bytes: readme.to_vec(),
        }];
        tree_id(&files, &Default::default()).unwrap()
    }

    #[test]
    fn an_archive_of_the_commit_is_accepted() {
        let verified =
            verify_archive(&archive(SHA, b"hi"), &sha(SHA), &tree_of(b"hi"), 1 << 20).unwrap();
        assert_eq!(verified.files.len(), 1);
        assert_eq!(verified.tree, tree_of(b"hi"));
    }

    #[test]
    fn an_archive_with_another_or_no_comment_is_refused() {
        assert!(matches!(
            verify_archive(&archive(OTHER, b"hi"), &sha(SHA), &tree_of(b"hi"), 1 << 20),
            Err(VerificationError::ArchiveCommentMismatch { .. })
        ));
        assert_eq!(
            verify_archive(&archive("", b"hi"), &sha(SHA), &tree_of(b"hi"), 1 << 20).unwrap_err(),
            VerificationError::ArchiveCommentMissing
        );
    }

    #[test]
    fn an_archive_whose_content_differs_from_the_commit_is_refused() {
        assert!(matches!(
            verify_archive(
                &archive(SHA, b"changed"),
                &sha(SHA),
                &tree_of(b"hi"),
                1 << 20
            ),
            Err(VerificationError::TreeMismatch { .. })
        ));
    }
}
