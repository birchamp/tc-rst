//! `fetched.lock`: facts looked up on Door43 once, by someone who could reach it, so that
//! every later check of a bundled or packaged archive runs offline.
//!
//! The lexicons ship as git archives whose burrito metadata vouches for nothing, and the
//! OBS image pack is downloaded when the app is packaged (`bundle/en/README.md`). An entry
//! names a commit, that commit's tree id as Door43 reported it, and the archive's SHA-256
//! and size. [`LockEntry::verify`] accepts an archive only when all four hold. A lexicon's
//! entry also records the SHA-256 of the one JSON file its archive is repackaged into
//! (`lexiconSha256`), which the packaging step checks ([`crate::install_lexicon_zip`]).
//!
//! ```json
//! {
//!   "format": "tc-fetched-lock",
//!   "version": 1,
//!   "entries": [
//!     { "repo": "uW/en_ugl", "commit": "<40 hex>", "tree": "<40 hex>",
//!       "file": "en_ugl-<commit>.zip", "sha256": "<64 hex>", "size": 0,
//!       "lexiconSha256": "<64 hex>",
//!       "checked": "<YYYY-MM-DD>", "how": "<the lookup and the cross-check>" }
//!   ]
//! }
//! ```

use std::collections::HashSet;

use serde::Deserialize;
use tc_door43::{GitOid, ObjectId, RepoRef, VerifiedArchive, verify_archive};

use crate::error::{ParseError, Refusal};
use crate::pin::Sha256;

pub const LOCK_FORMAT: &str = "tc-fetched-lock";
pub const LOCK_VERSION: u64 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedLock {
    pub entries: Vec<LockEntry>,
}

/// The archive of one commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockEntry {
    pub repo: RepoRef,
    pub commit: GitOid,
    /// The commit's git tree id, as Door43 reported it.
    pub tree: ObjectId,
    /// The archive's file name in the lock's folder. `None` for an archive the packaging
    /// step downloads.
    pub file: Option<String>,
    pub sha256: Sha256,
    /// The archive's size in bytes.
    pub size: u64,
    /// For a lexicon: the SHA-256 of the file [`crate::repackage_lexicon`] makes of the
    /// archive. Repackaging gives the same bytes every time, so this changes only when the
    /// format or its writer does, on purpose.
    pub lexicon_sha256: Option<Sha256>,
    /// The day the facts were looked up, `YYYY-MM-DD`.
    pub checked: String,
    /// How the tree id was looked up and cross-checked.
    pub how: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLock {
    format: String,
    version: u64,
    entries: Vec<RawEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RawEntry {
    repo: String,
    commit: String,
    tree: String,
    file: Option<String>,
    sha256: String,
    size: u64,
    #[serde(default)]
    lexicon_sha256: Option<String>,
    checked: String,
    how: String,
}

impl FetchedLock {
    /// Read a lock. A format it does not know, a version other than [`LOCK_VERSION`], an
    /// unknown key, a malformed value or two entries for one commit or one file are
    /// refused whole.
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        let bad = ParseError::FetchedLock;
        let raw: RawLock = serde_json::from_slice(bytes).map_err(|e| bad(e.to_string()))?;
        if raw.format != LOCK_FORMAT {
            return Err(bad(format!(
                "the format is {:?}, not {LOCK_FORMAT:?}",
                raw.format
            )));
        }
        if raw.version != LOCK_VERSION {
            return Err(bad(format!(
                "version {} is not {LOCK_VERSION}, the version this app reads",
                raw.version
            )));
        }
        let entries = raw
            .entries
            .into_iter()
            .enumerate()
            .map(|(i, entry)| {
                entry
                    .parse()
                    .map_err(|e| bad(format!("entry {}: {e}", i + 1)))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut commits = HashSet::new();
        let mut files = HashSet::new();
        for entry in &entries {
            if !commits.insert((&entry.repo, &entry.commit)) {
                return Err(bad(format!(
                    "two entries for {} at {}",
                    entry.repo, entry.commit
                )));
            }
            if let Some(file) = &entry.file
                && !files.insert(file)
            {
                return Err(bad(format!("two entries for the file {file:?}")));
            }
        }
        Ok(Self { entries })
    }

    /// The entry for `commit` of `repo`.
    pub fn entry(&self, repo: &RepoRef, commit: &GitOid) -> Option<&LockEntry> {
        self.entries
            .iter()
            .find(|e| e.repo == *repo && e.commit == *commit)
    }

    /// Checks the archive of `commit` of `repo` against this lock. Without an entry for
    /// that commit, no tree that Door43 reported is known, so the archive is unconfirmed;
    /// with one, it must match it ([`LockEntry::verify`]).
    pub fn check(
        &self,
        repo: &RepoRef,
        commit: &GitOid,
        zip: &[u8],
        max_unpacked: u64,
    ) -> Result<TreeCheck, Refusal> {
        match self.entry(repo, commit) {
            None => Ok(TreeCheck::Unconfirmed),
            Some(entry) => entry.verify(zip, max_unpacked).map(TreeCheck::Confirmed),
        }
    }
}

/// What [`FetchedLock::check`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeCheck {
    /// The archive is the lock entry's download, and its files are the tree Door43
    /// reported for the commit.
    Confirmed(VerifiedArchive),
    /// The lock has no entry for the commit yet: nothing confirms the archive's files.
    Unconfirmed,
}

impl RawEntry {
    fn parse(self) -> Result<LockEntry, String> {
        if let Some(file) = &self.file
            && !is_plain_file_name(file)
        {
            return Err(format!("file {file:?} is not a plain file name"));
        }
        if !is_date(&self.checked) {
            return Err(format!(
                "checked {:?} is not a YYYY-MM-DD date",
                self.checked
            ));
        }
        if self.how.trim().is_empty() {
            return Err("how is empty".to_owned());
        }
        Ok(LockEntry {
            repo: self.repo.parse().map_err(|e| format!("repo: {e}"))?,
            // The lock is written by hand; a commit pasted in upper case is the same commit.
            commit: GitOid::parse_any_case(&self.commit)
                .ok_or_else(|| format!("commit: {:?} is not a 40-digit commit sha", self.commit))?,
            tree: self
                .tree
                .parse()
                .map_err(|()| format!("tree {:?} is not a git object id", self.tree))?,
            file: self.file,
            sha256: self.sha256.parse().map_err(|e| format!("sha256: {e}"))?,
            size: self.size,
            lexicon_sha256: self
                .lexicon_sha256
                .map(|hex| hex.parse())
                .transpose()
                .map_err(|e| format!("lexiconSha256: {e}"))?,
            checked: self.checked,
            how: self.how,
        })
    }
}

impl LockEntry {
    /// Check that `zip` is this entry's archive: its size and SHA-256 are the recorded
    /// download's, and [`verify_archive`] finds the recorded commit's tree in it.
    pub fn verify(&self, zip: &[u8], max_unpacked: u64) -> Result<VerifiedArchive, Refusal> {
        let size = zip.len() as u64;
        if size != self.size {
            return Err(Refusal::SizeMismatch {
                expected: self.size,
                found: size,
            });
        }
        let found = Sha256::of(zip);
        if found != self.sha256 {
            return Err(Refusal::Sha256Mismatch {
                expected: self.sha256,
                found,
            });
        }
        Ok(verify_archive(zip, &self.commit, &self.tree, max_unpacked)?)
    }
}

/// A name that, joined to the lock's folder, names a file in that folder on every
/// platform.
fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':', '\0'])
}

fn is_date(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    let number = |part: &str, len: usize| {
        (part.len() == len && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse::<u32>().ok())
            .flatten()
    };
    match parts.as_slice() {
        [year, month, day] => {
            number(year, 4).is_some()
                && number(month, 2).is_some_and(|m| (1..=12).contains(&m))
                && number(day, 2).is_some_and(|d| (1..=31).contains(&d))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn lock_with(entry: Value) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "format": LOCK_FORMAT,
            "version": LOCK_VERSION,
            "entries": [entry],
        }))
        .unwrap()
    }

    fn entry() -> Value {
        json!({
            "repo": "uW/en_ugl",
            "commit": "D9D29E2D589258CE27F92B59F753A3AF03AB7A72",
            "tree": "6e5802a33cc85441708e9d4dc17c71c2b25b6cc0",
            "file": "en_ugl-d9d29e2d589258ce27f92b59f753a3af03ab7a72.zip",
            "sha256": "ed0c325b58e18976d29b70151f704548df297f285ca0710704cbe1def4f4de51",
            "size": 748,
            "checked": "2026-10-16",
            "how": "GET /api/v1/repos/uW/en_ugl/git/commits/<sha>; git rev-parse <sha>^{tree}",
        })
    }

    fn refusal(bytes: &[u8]) -> String {
        FetchedLock::parse(bytes).unwrap_err().to_string()
    }

    #[test]
    fn an_entry_reads_into_typed_values() {
        let lock = FetchedLock::parse(&lock_with(entry())).unwrap();
        let [entry] = lock.entries.as_slice() else {
            panic!("one entry: {lock:?}")
        };
        assert_eq!(entry.repo.to_string(), "uW/en_ugl");
        assert_eq!(
            &entry.commit.to_hex(),
            "d9d29e2d589258ce27f92b59f753a3af03ab7a72",
            "a commit is kept in lowercase"
        );
        assert_eq!(
            entry.tree.to_string(),
            "6e5802a33cc85441708e9d4dc17c71c2b25b6cc0"
        );
        assert_eq!(entry.size, 748);
        assert_eq!(entry.checked, "2026-10-16");
        assert_eq!(entry.lexicon_sha256, None);
    }

    #[test]
    fn a_lexicon_entry_records_its_repackaged_file() {
        let mut lexicon = entry();
        let hex = "7798835a14559f03b3d3e4b804b6d88f6118923fd07ff39c44e14258a9f3f094";
        lexicon["lexiconSha256"] = json!(hex);
        let lock = FetchedLock::parse(&lock_with(lexicon)).unwrap();
        assert_eq!(
            lock.entries[0]
                .lexicon_sha256
                .map(Sha256::to_hex)
                .as_deref(),
            Some(hex)
        );
    }

    #[test]
    fn an_entry_without_a_file_is_one_the_packaging_step_downloads() {
        let mut fetched = entry();
        fetched["file"] = Value::Null;
        let lock = FetchedLock::parse(&lock_with(fetched)).unwrap();
        assert_eq!(lock.entries[0].file, None);
    }

    #[test]
    fn another_format_or_version_is_refused() {
        let mut other = serde_json::from_slice::<Value>(&lock_with(entry())).unwrap();
        other["format"] = json!("tc-lexicon");
        assert!(refusal(&serde_json::to_vec(&other).unwrap()).contains("\"tc-lexicon\""));
        other["format"] = json!(LOCK_FORMAT);
        other["version"] = json!(2);
        assert!(refusal(&serde_json::to_vec(&other).unwrap()).contains("version 2"));
    }

    #[test]
    fn a_malformed_value_or_an_unknown_key_is_refused() {
        let cases = [
            ("tree", json!("6e5802a3"), "tree"),
            ("commit", json!("v91"), "commit"),
            ("sha256", json!("ed0c"), "sha256"),
            ("repo", json!("en_ugl"), "repo"),
            ("file", json!("../en_ugl.zip"), "plain file name"),
            ("file", json!("C:en_ugl.zip"), "plain file name"),
            ("checked", json!("2026-13-01"), "YYYY-MM-DD"),
            ("checked", json!("16 Oct 2026"), "YYYY-MM-DD"),
            ("how", json!(" "), "how is empty"),
            ("size", json!(-1), "expected u64"),
            ("lexiconSha256", json!("7798"), "lexiconSha256"),
            ("note", json!("extra"), "unknown field"),
        ];
        for (key, value, says) in cases {
            let mut bad = entry();
            bad[key] = value;
            let message = refusal(&lock_with(bad));
            assert!(message.contains(says), "{key}: {message}");
        }
    }

    #[test]
    fn two_entries_for_one_commit_or_one_file_are_refused() {
        let mut same_commit = entry();
        same_commit["file"] = json!("other.zip");
        same_commit["repo"] = json!("UW/EN_UGL");
        let lock =
            json!({ "format": LOCK_FORMAT, "version": 1, "entries": [entry(), same_commit] });
        assert!(refusal(&serde_json::to_vec(&lock).unwrap()).contains("two entries for"));

        let mut same_file = entry();
        same_file["commit"] = json!("72df5ac25acf9d51e826b20e3ad883a5a657ef4e");
        let lock = json!({ "format": LOCK_FORMAT, "version": 1, "entries": [entry(), same_file] });
        assert!(refusal(&serde_json::to_vec(&lock).unwrap()).contains("the file"));
    }
}
