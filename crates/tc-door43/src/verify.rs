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
//! A commit can hold symbolic links, so a verified archive can too: see
//! [`FileMode::Symlink`] before writing one to disk.
//!
//! # The DCS Scripture Burrito export (`/<owner>/<repo>/sb/<tag>.zip`)
//!
//! DCS builds this zip on each request from the tag's commit. [`verify_sb_export`] accepts
//! it only when its `metadata.json` declares exactly one DCS revision
//! (`identification.primary.dcs["<owner>/<repo>"].revision`), that revision equals the
//! expected sha, every ingredient the zip holds has the md5 and size the metadata gives,
//! and no entry is a symbolic link. The expected sha must come from outside the zip: from
//! the pin, or on a first install from the tags API, so the archive never certifies itself.
//! A matching revision does not prove that every byte comes from that commit: the `en_tw`
//! export takes its TWL files from `en_twl` master (D96). Use the archive when the bytes
//! must be reproducible.
//!
//! Files the metadata does not list were checked by nothing, so they come back apart, in
//! [`VerifiedSbExport::unlisted`]. The same checks suit a burrito zip a person made: the
//! debris macOS Finder adds is dropped, and a `metadata.json` may start with a UTF-8 BOM.

use crate::error::VerificationError;
use crate::git_tree::{ObjectId, tree_id};
use crate::names::CommitSha;
use crate::unpack::{FileMode, Layout, UnpackedFile, unpack_from};
use md5::{Digest, Md5};
use serde_json::Value;
use std::io::{Cursor, Read, Seek};

/// A Gitea archive whose files are the content of `commit`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedArchive {
    pub commit: CommitSha,
    pub tree: ObjectId,
    /// Sorted by path, relative to the repository root. A symbolic link in the commit is
    /// here too, as its target text: never create it as a link ([`FileMode::Symlink`]).
    pub files: Vec<UnpackedFile>,
}

/// A DCS burrito export whose `metadata.json` declares the expected revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedSbExport {
    pub revision: CommitSha,
    /// `metadata.json` and every ingredient it lists that the zip holds, each matching its
    /// md5 and size. Sorted by path, relative to the burrito root. None is a symbolic link.
    pub files: Vec<UnpackedFile>,
    /// Files the zip holds that `metadata.json` does not list, so nothing checked them.
    /// They are not part of the verified burrito; the caller decides whether to keep them
    /// (the `uW` lexicon burritos leave their payload out of the ingredient table). The
    /// export's `.git/` and `.DS_Store` entries and macOS Finder's `__MACOSX/` and `._*`
    /// entries are dropped and appear nowhere.
    pub unlisted: Vec<UnpackedFile>,
    /// Ingredients `metadata.json` lists that the zip does not hold. tC4's evidence says
    /// some exports list articles they do not ship; the caller decides what that means.
    pub missing_ingredients: Vec<String>,
}

/// Check that `zip` is the content of `commit`, whose git tree is `commit_tree`. See the
/// module documentation for the checks.
pub fn verify_archive(
    zip: &[u8],
    commit: &CommitSha,
    commit_tree: &ObjectId,
    max_unpacked: u64,
) -> Result<VerifiedArchive, VerificationError> {
    verify_archive_from(Cursor::new(zip), commit, commit_tree, max_unpacked)
}

/// [`verify_archive`] of a zip read from `zip`, such as a downloaded file.
pub(crate) fn verify_archive_from<R: Read + Seek>(
    zip: R,
    commit: &CommitSha,
    commit_tree: &ObjectId,
    max_unpacked: u64,
) -> Result<VerifiedArchive, VerificationError> {
    let unpacked = unpack_from(zip, Layout::GitArchive, max_unpacked)?;
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

/// Check that `zip` is a burrito whose metadata declares `expected` as its DCS revision.
/// See the module documentation for the checks.
pub fn verify_sb_export(
    zip: &[u8],
    expected: &CommitSha,
    max_unpacked: u64,
) -> Result<VerifiedSbExport, VerificationError> {
    verify_sb_export_from(Cursor::new(zip), expected, max_unpacked)
}

/// [`verify_sb_export`] of a zip read from `zip`, such as a downloaded file.
pub(crate) fn verify_sb_export_from<R: Read + Seek>(
    zip: R,
    expected: &CommitSha,
    max_unpacked: u64,
) -> Result<VerifiedSbExport, VerificationError> {
    let unpacked = unpack_from(zip, Layout::Burrito, max_unpacked)?;
    // A burrito holds no links, and an installer that made one could write anywhere.
    if let Some(link) = unpacked.files.iter().find(|f| f.mode == FileMode::Symlink) {
        return Err(VerificationError::SymlinkInBurrito(link.path.clone()));
    }
    let metadata = unpacked
        .files
        .iter()
        .find(|f| f.path == "metadata.json")
        .ok_or(VerificationError::MetadataMissing)?;
    if !unpacked
        .files
        .iter()
        .any(|f| f.path.starts_with("ingredients/"))
    {
        return Err(VerificationError::NoIngredients);
    }
    // Hand-made metadata may start with a BOM, which JSON does not allow. Only the parse
    // skips it; the file's bytes come back unchanged.
    let json = metadata
        .bytes
        .strip_prefix(b"\xEF\xBB\xBF")
        .unwrap_or(&metadata.bytes);
    let metadata: Value = serde_json::from_slice(json)
        .map_err(|e| VerificationError::MetadataUnreadable(e.to_string()))?;

    let declared = declared_revision(&metadata)?;
    if !declared.eq_ignore_ascii_case(expected.as_str()) {
        return Err(VerificationError::RevisionMismatch {
            expected: expected.clone(),
            declared: declared.chars().take(80).collect(),
        });
    }
    let ingredients = ingredient_table(&metadata)?;
    let missing_ingredients = check_ingredients(ingredients, &unpacked.files)?;
    let (files, unlisted) = unpacked
        .files
        .into_iter()
        .partition(|f| f.path == "metadata.json" || ingredients.contains_key(&f.path));
    Ok(VerifiedSbExport {
        revision: expected.clone(),
        files,
        unlisted,
        missing_ingredients,
    })
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

fn ingredient_table(
    metadata: &Value,
) -> Result<&serde_json::Map<String, Value>, VerificationError> {
    metadata
        .get("ingredients")
        .and_then(Value::as_object)
        .ok_or_else(|| VerificationError::MetadataUnreadable("it has no ingredients table".into()))
}

/// Check every listed ingredient the zip holds; return the listed ones it does not hold.
fn check_ingredients(
    ingredients: &serde_json::Map<String, Value>,
    files: &[UnpackedFile],
) -> Result<Vec<String>, VerificationError> {
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

    #[test]
    fn a_symlink_is_refused_even_when_its_target_matches_the_metadata() {
        let target = b"../../../../home/user/.ssh/id_rsa";
        let zip = zip_of(
            &[
                (
                    "en_tn/metadata.json",
                    FileMode::Regular,
                    &metadata(&[("unfoldingWord/en_tn", SHA)], target),
                ),
                ("en_tn/ingredients/TIT.tsv", FileMode::Symlink, target),
            ],
            "",
        );
        assert_eq!(
            verify_sb_export(&zip, &sha(SHA), 1 << 20).unwrap_err(),
            VerificationError::SymlinkInBurrito("ingredients/TIT.tsv".into())
        );
        let unlisted_link = zip_of(
            &[
                (
                    "en_tn/metadata.json",
                    FileMode::Regular,
                    &metadata(&[("unfoldingWord/en_tn", SHA)], b"x"),
                ),
                ("en_tn/ingredients/TIT.tsv", FileMode::Regular, b"x"),
                ("en_tn/ingredients/link", FileMode::Symlink, b"/etc/passwd"),
            ],
            "",
        );
        assert_eq!(
            verify_sb_export(&unlisted_link, &sha(SHA), 1 << 20).unwrap_err(),
            VerificationError::SymlinkInBurrito("ingredients/link".into())
        );
    }

    #[test]
    fn files_the_metadata_does_not_list_come_back_apart() {
        let tsv = b"Reference\tID\n";
        let zip = zip_of(
            &[
                (
                    "en_tn/metadata.json",
                    FileMode::Regular,
                    &metadata(&[("unfoldingWord/en_tn", SHA)], tsv),
                ),
                ("en_tn/ingredients/TIT.tsv", FileMode::Regular, tsv),
                (
                    "en_tn/ingredients/EXTRA.tsv",
                    FileMode::Regular,
                    b"never checked",
                ),
                ("en_tn/README.md", FileMode::Regular, b"# en_tn"),
            ],
            "",
        );
        let verified = verify_sb_export(&zip, &sha(SHA), 1 << 20).unwrap();
        let paths = |files: &[UnpackedFile]| -> Vec<String> {
            files.iter().map(|f| f.path.clone()).collect()
        };
        assert_eq!(
            paths(&verified.files),
            ["ingredients/TIT.tsv", "metadata.json"]
        );
        assert_eq!(
            paths(&verified.unlisted),
            ["README.md", "ingredients/EXTRA.tsv"]
        );
        assert_eq!(verified.unlisted[1].bytes, b"never checked");
    }

    #[test]
    fn a_burrito_zipped_by_macos_finder_with_a_bom_is_accepted_unchanged() {
        let tsv = b"Reference\tID\n";
        let mut meta = b"\xEF\xBB\xBF".to_vec();
        meta.extend_from_slice(&metadata(&[("unfoldingWord/en_tn", SHA)], tsv));
        let zip = zip_of(
            &[
                ("en_tn/", FileMode::Regular, b""),
                ("en_tn/metadata.json", FileMode::Regular, &meta),
                ("en_tn/ingredients/TIT.tsv", FileMode::Regular, tsv),
                ("__MACOSX/", FileMode::Regular, b""),
                ("__MACOSX/en_tn/", FileMode::Regular, b""),
                (
                    "__MACOSX/en_tn/._metadata.json",
                    FileMode::Regular,
                    b"\0\x05\x16\x07",
                ),
                (
                    "__MACOSX/en_tn/ingredients/._TIT.tsv",
                    FileMode::Regular,
                    b"\0\x05\x16\x07",
                ),
            ],
            "",
        );
        let verified = verify_sb_export(&zip, &sha(SHA), 1 << 20).unwrap();
        let metadata_file = verified
            .files
            .iter()
            .find(|f| f.path == "metadata.json")
            .unwrap();
        assert_eq!(metadata_file.bytes, meta, "the BOM stays in the bytes");
        assert!(verified.unlisted.is_empty(), "{:?}", verified.unlisted);
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
