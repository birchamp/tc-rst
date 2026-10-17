//! The two lexicon archives in `bundle/en/`, checked with `verify_archive` against the
//! trees their zips hold. Door43 has not confirmed those trees yet (branch `door43-facts`,
//! `.audit/tasks/door43-facts.md`), so they serve tests of the repackaging only and are
//! never a lock entry.

use std::path::PathBuf;
use std::sync::OnceLock;

use tc_door43::{Limits, VerifiedArchive, verify_archive};
use tc_resources::strongs::LexiconId;
use tc_resources::{
    FetchedLock, Lexicon, LockEntry, ResourcePin, Sha256, repackage_lexicon, suite,
};

pub struct BundledLexicon {
    pub id: LexiconId,
    pub file: &'static str,
    /// The tree the zip's files hash to, which Door43 must confirm.
    pub zip_tree: &'static str,
    pub entries: usize,
    /// The repackaged file's SHA-256 and size, recorded in `bundle/en/README.md`.
    /// Repackaging is deterministic, so a change here is a change of the format or of its
    /// writer, made on purpose.
    pub json_sha256: &'static str,
    pub json_size: usize,
}

pub const UGL: BundledLexicon = BundledLexicon {
    id: LexiconId::Ugl,
    file: "en_ugl-d9d29e2d589258ce27f92b59f753a3af03ab7a72.zip",
    zip_tree: "4d24ed9c8ceb768df1069feed0cc375d32cd93ce",
    entries: 5408,
    json_sha256: "7798835a14559f03b3d3e4b804b6d88f6118923fd07ff39c44e14258a9f3f094",
    json_size: 500_021,
};

pub const UHL: BundledLexicon = BundledLexicon {
    id: LexiconId::Uhl,
    file: "en_uhl-72df5ac25acf9d51e826b20e3ad883a5a657ef4e.zip",
    zip_tree: "ad07f5e41f7052598d90eb8dc2b9ecad811b0b17",
    entries: 8674,
    json_sha256: "b3a002bbfe52720624b9e97a1130b138cfb4ecfc30a5d4111e7fa1f1581cc258",
    json_size: 2_055_993,
};

pub fn bundle() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bundle/en")
}

impl BundledLexicon {
    /// The bundled suite's pin: `lexicon.nt` for `ugl`, `lexicon.ot` for `uhl`.
    pub fn pin(&self) -> ResourcePin {
        suite::project_pins()
            .lexicon
            .get(self.id.testament())
            .unwrap()
            .clone()
    }

    pub fn zip(&self) -> Vec<u8> {
        std::fs::read(bundle().join(self.file)).unwrap()
    }

    pub fn archive(&self, zip: &[u8]) -> VerifiedArchive {
        verify_archive(
            zip,
            &self.pin().sha,
            &self.zip_tree.parse().unwrap(),
            Limits::default().max_unpacked_bytes,
        )
        .unwrap()
    }

    /// The repackaged file, made once per test process.
    pub fn repackaged(&self) -> &'static [u8] {
        static UGL_FILE: OnceLock<Vec<u8>> = OnceLock::new();
        static UHL_FILE: OnceLock<Vec<u8>> = OnceLock::new();
        let cell = match self.id {
            LexiconId::Ugl => &UGL_FILE,
            LexiconId::Uhl => &UHL_FILE,
        };
        cell.get_or_init(|| {
            let zip = self.zip();
            repackage_lexicon(
                self.pin().repo.repo(),
                &self.archive(&zip),
                Sha256::of(&zip),
            )
            .unwrap()
        })
    }

    pub fn lexicon(&self) -> Lexicon {
        Lexicon::parse(self.repackaged()).unwrap()
    }
}

/// A lock that names the tree each zip itself holds, for tests that need the real lexicons
/// installed. Door43 has not confirmed these trees, so this stands in for
/// `bundle/en/fetched.lock` only in memory, and is never written anywhere.
pub fn lock_from_the_zips() -> FetchedLock {
    let entries = [UGL, UHL]
        .iter()
        .map(|lexicon| {
            let zip = lexicon.zip();
            let pin = lexicon.pin();
            LockEntry {
                repo: pin.repo.repo().clone(),
                commit: pin.sha.clone(),
                tree: lexicon.zip_tree.parse().unwrap(),
                file: Some(lexicon.file.to_owned()),
                sha256: Sha256::of(&zip),
                size: zip.len() as u64,
                lexicon_sha256: Some(lexicon.json_sha256.parse().unwrap()),
                checked: "2026-10-16".to_owned(),
                how: "tests only: the tree the zip holds, not confirmed by Door43".to_owned(),
            }
        })
        .collect();
    FetchedLock { entries }
}
