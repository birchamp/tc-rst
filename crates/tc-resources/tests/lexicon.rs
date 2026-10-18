//! Lexicons repackaged as one JSON file each (`bundle/en/README.md`): the two bundled
//! archives, the check against `fetched.lock` that confirms an archive before it is
//! installed, and reading an installed lexicon back.

// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::fs;
use std::io::{Cursor, Read};
use std::path::PathBuf;

use common::bundled::{UGL, UHL, bundle};
use common::nothing_installed;
use serde_json::Value;
use tc_door43::VerificationError;
use tc_resources::strongs::LexiconId;
use tc_resources::{
    FetchedLock, InstallError, LEXICON_FILE, Lexicon, LexiconError, LexiconInstall,
    LexiconUnavailable, Lexicons, Origin, Packaging, PartEntry, Refusal, RepoPath, ResourcePin,
    ResourceStore, Sha256, Testaments, TreeCheck, content_files_opened, install_lexicon_zip,
    open_lexicon,
};

const MAX: u64 = 1 << 30;

/// Every file of a zip below its top folder, by path.
fn unzipped(zip: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip)).unwrap();
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        if file.is_dir() {
            continue;
        }
        let path = file.name().split_once('/').unwrap().1.to_owned();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        out.push((path, bytes));
    }
    out
}

#[test]
fn the_bundled_archives_hold_the_trees_door43_must_confirm() {
    for lexicon in [UGL, UHL] {
        let archive = lexicon.archive(&lexicon.zip());
        assert_eq!(
            archive.tree.to_string(),
            lexicon.zip_tree,
            "{}",
            lexicon.file
        );
        assert_eq!(archive.commit, lexicon.pin().sha);
    }
}

#[test]
fn repackaging_a_bundled_lexicon_gives_the_same_bytes_every_time() {
    for lexicon in [UGL, UHL] {
        let zip = lexicon.zip();
        let again = tc_resources::repackage_lexicon(
            lexicon.pin().repo.repo(),
            &lexicon.archive(&zip),
            Sha256::of(&zip),
        )
        .unwrap();
        assert_eq!(again, lexicon.repackaged(), "{}", lexicon.file);
        assert_eq!(
            (Sha256::of(&again).to_hex().as_str(), again.len()),
            (lexicon.json_sha256, lexicon.json_size),
            "{}",
            lexicon.file
        );
    }
}

#[test]
fn every_entry_is_kept_unchanged_and_keyed_in_numeric_order() {
    for lexicon in [UGL, UHL] {
        let file: Value = serde_json::from_slice(lexicon.repackaged()).unwrap();
        let entries = file["entries"].as_object().unwrap();
        assert_eq!(entries.len(), lexicon.entries, "{}", lexicon.file);
        let numbers: Vec<u32> = entries.keys().map(|k| k.parse().unwrap()).collect();
        assert!(numbers.windows(2).all(|w| w[0] < w[1]), "ascending");

        let mut compared = 0;
        for (path, bytes) in unzipped(&lexicon.zip()) {
            let Some(number) = path
                .strip_prefix("ingredients/content/")
                .and_then(|p| p.strip_suffix(".json"))
            else {
                continue;
            };
            let original: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(entries[number], original, "{path}");
            assert_eq!(
                serde_json::to_string(&entries[number]).unwrap(),
                serde_json::to_string(&original).unwrap(),
                "{path} keeps its key order"
            );
            compared += 1;
        }
        assert_eq!(compared, lexicon.entries);

        let read = lexicon.lexicon();
        assert_eq!(read.len(), lexicon.entries);
        assert_eq!(read.numbers(), numbers);
        assert!(read.numbers().iter().all(|&n| {
            let entry = read.entry(n).unwrap();
            entry.brief.is_some() && entry.long.is_some() && entry.other.is_empty()
        }));
    }
}

#[test]
fn metadata_index_and_other_files_are_kept() {
    for lexicon in [UGL, UHL] {
        let files = unzipped(&lexicon.zip());
        let original = |name: &str| {
            files
                .iter()
                .find(|(path, _)| path == name)
                .map(|(_, bytes)| bytes.clone())
        };
        let read = lexicon.lexicon();
        let metadata: Value = serde_json::from_slice(&original("metadata.json").unwrap()).unwrap();
        assert_eq!(read.metadata(), &metadata);
        assert_eq!(
            serde_json::to_string(read.metadata()).unwrap(),
            serde_json::to_string(&metadata).unwrap(),
            "the metadata keeps its key order"
        );
        let ingredients: Vec<&String> = read.metadata()["ingredients"]
            .as_object()
            .unwrap()
            .keys()
            .collect();
        assert_eq!(
            ingredients,
            ["README.md"],
            "the table that vouches for nothing is kept"
        );

        let index = original("ingredients/index.json")
            .map(|bytes| serde_json::from_slice::<Value>(&bytes).unwrap());
        assert_eq!(read.index(), index.as_ref());
        let readme = String::from_utf8(original("ingredients/README.md").unwrap()).unwrap();
        assert_eq!(
            read.files().iter().collect::<Vec<_>>(),
            [(&"ingredients/README.md".to_owned(), &readme)]
        );
        assert_eq!(read.source().commit, lexicon.pin().sha);
        assert_eq!(read.source().repo, *lexicon.pin().repo.repo());
        assert_eq!(read.source().archive_sha256, Sha256::of(&lexicon.zip()));
    }
    let uhl = UHL.lexicon();
    assert_eq!(
        uhl.metadata()
            .pointer("/identification/primary/uWBurritos/TA/timestamp")
            .and_then(Value::as_str),
        Some("2025-11-24T55:20:15.878Z"),
        "the invalid timestamp is kept as it is"
    );
    assert_eq!(
        uhl.index().and_then(Value::as_array).map(Vec::len),
        Some(8674)
    );
    assert_eq!(UGL.lexicon().index(), None);
}

/// `bundle/en/fetched.lock`, or a lock without entries while the file does not exist.
fn bundle_lock() -> FetchedLock {
    match fs::read(bundle().join("fetched.lock")) {
        Ok(bytes) => FetchedLock::parse(&bytes).unwrap(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => FetchedLock {
            entries: Vec::new(),
        },
        Err(e) => panic!("fetched.lock: {e}"),
    }
}

#[test]
fn a_bundled_lexicon_is_unconfirmed_until_fetched_lock_has_its_entry() {
    let lock = bundle_lock();
    let dir = tempfile::tempdir().unwrap();
    let store = ResourceStore::open(dir.path()).unwrap();
    for lexicon in [UGL, UHL] {
        let pin = lexicon.pin();
        let zip = lexicon.zip();
        let check = lock.check(pin.repo.repo(), &pin.sha, &zip, MAX).unwrap();
        let installed = install_lexicon_zip(&store, &pin, &lock, &zip, MAX).unwrap();
        match lock.entry(pin.repo.repo(), &pin.sha) {
            None => {
                assert_eq!(check, TreeCheck::Unconfirmed, "{}", lexicon.file);
                assert!(matches!(installed, LexiconInstall::Unconfirmed));
                assert!(!store.has_dcs(&pin), "nothing unconfirmed is installed");
                assert!(matches!(
                    open_lexicon(&store, &pin),
                    Err(LexiconUnavailable::NotInstalled(_))
                ));
            }
            Some(entry) => {
                let TreeCheck::Confirmed(archive) = check else {
                    panic!("{}: the lock's entry must confirm it", lexicon.file);
                };
                assert_eq!(archive.tree, entry.tree);
                let LexiconInstall::Installed(installed) = installed else {
                    panic!("{}: installed", lexicon.file);
                };
                assert_eq!(installed.read(LEXICON_FILE).unwrap(), lexicon.repackaged());
            }
        }
    }
}

fn fixture_dir() -> PathBuf {
    common::fixtures().join("lexicon")
}

const EXCERPT: &str = "en_uhl_excerpt-7791a4fdd72748b5d59ed8744c1a9ccc9ed9543a.zip";

fn excerpt_lock() -> FetchedLock {
    FetchedLock::parse(&fs::read(fixture_dir().join("fetched.lock")).unwrap()).unwrap()
}

fn excerpt_pin() -> ResourcePin {
    ResourcePin::new(
        "git.door43.org/fixture/en_uhl_excerpt"
            .parse::<RepoPath>()
            .unwrap(),
        "7791a4fdd72748b5d59ed8744c1a9ccc9ed9543a".parse().unwrap(),
    )
}

#[test]
fn an_archive_its_lock_confirms_installs_as_one_lexicon_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = ResourceStore::open(dir.path()).unwrap();
    let zip = fs::read(fixture_dir().join(EXCERPT)).unwrap();
    let pin = excerpt_pin();
    let LexiconInstall::Installed(installed) =
        install_lexicon_zip(&store, &pin, &excerpt_lock(), &zip, MAX).unwrap()
    else {
        panic!("the fixture lock has the archive's entry");
    };
    installed.verify().unwrap();
    assert_eq!(installed.files().collect::<Vec<_>>(), [LEXICON_FILE]);
    let Origin::Dcs {
        pin: held,
        packaging,
        ..
    } = installed.origin()
    else {
        panic!("a Door43 install");
    };
    assert_eq!(*packaging, Packaging::Lexicon);
    assert_eq!(held.flavor.as_deref(), Some("peripheral/x-lexicon"));

    let lexicon = open_lexicon(&store, &pin).unwrap();
    assert_eq!(lexicon.numbers(), [1, 2, 10]);
    assert_eq!(lexicon.entry(1).unwrap().brief.as_deref(), Some("father"));
    assert_eq!(
        lexicon.files().keys().collect::<Vec<_>>(),
        ["ingredients/README.md"]
    );
    assert_eq!(lexicon.source().archive_sha256, Sha256::of(&zip));
    let the_same: Vec<u8> = installed.read(LEXICON_FILE).unwrap();
    assert_eq!(
        Lexicon::parse(&the_same).unwrap(),
        lexicon,
        "the installed file is the one read"
    );

    // Installed once: a second call neither checks nor writes anything.
    let LexiconInstall::Installed(again) =
        install_lexicon_zip(&store, &pin, &FetchedLock { entries: vec![] }, b"", MAX).unwrap()
    else {
        panic!("already installed");
    };
    assert_eq!(again.dir(), installed.dir());
}

#[test]
fn an_archive_that_does_not_match_its_lock_entry_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = ResourceStore::open(dir.path()).unwrap();
    let zip = fs::read(fixture_dir().join(EXCERPT)).unwrap();
    let pin = excerpt_pin();

    let mut changed = zip.clone();
    changed[zip.len() / 2] ^= 1;
    let err = install_lexicon_zip(&store, &pin, &excerpt_lock(), &changed, MAX).unwrap_err();
    assert!(
        matches!(
            err,
            InstallError::Refused {
                refusal: Refusal::Sha256Mismatch { .. },
                ..
            }
        ),
        "{err}"
    );
    nothing_installed(&store);

    // A lock whose tree is not the one the files hash to.
    let mut lock = excerpt_lock();
    lock.entries[0].tree = UHL.zip_tree.parse().unwrap();
    let err = install_lexicon_zip(&store, &pin, &lock, &zip, MAX).unwrap_err();
    assert!(
        matches!(
            err,
            InstallError::Refused {
                refusal: Refusal::Door43(VerificationError::TreeMismatch { .. }),
                ..
            }
        ),
        "{err}"
    );
    nothing_installed(&store);
}

#[test]
fn the_repackaged_file_must_be_the_one_the_lock_records() {
    let dir = tempfile::tempdir().unwrap();
    let store = ResourceStore::open(dir.path()).unwrap();
    let zip = fs::read(fixture_dir().join(EXCERPT)).unwrap();
    let pin = excerpt_pin();
    let recorded = excerpt_lock().entries[0].lexicon_sha256.unwrap();

    let mut unrecorded = excerpt_lock();
    unrecorded.entries[0].lexicon_sha256 = None;
    let err = install_lexicon_zip(&store, &pin, &unrecorded, &zip, MAX).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!(
            "fixture/en_uhl_excerpt (7791a4f…) cannot be installed as a lexicon: the repackaged \
             file's SHA-256 is {recorded}, but fetched.lock records none (record it as \
             lexiconSha256)"
        )
    );
    nothing_installed(&store);

    let mut other = excerpt_lock();
    other.entries[0].lexicon_sha256 = Some(Sha256::of(b"another writer"));
    let err = install_lexicon_zip(&store, &pin, &other, &zip, MAX).unwrap_err();
    assert!(
        matches!(
            &err,
            InstallError::Lexicon {
                problem: LexiconError::NotTheRecordedFile { found, .. },
                ..
            } if *found == recorded
        ),
        "{err}"
    );
    nothing_installed(&store);
}

#[test]
fn lexicons_open_once_and_a_lookup_reads_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = ResourceStore::open(dir.path()).unwrap();
    let zip = fs::read(fixture_dir().join(EXCERPT)).unwrap();
    let pin = excerpt_pin();
    install_lexicon_zip(&store, &pin, &excerpt_lock(), &zip, MAX).unwrap();
    let pins = Testaments {
        ot: Some(pin.clone()),
        ..Testaments::default()
    };

    let before = content_files_opened();
    let lexicons = Lexicons::open(&store, &pins);
    assert_eq!(
        content_files_opened() - before,
        1,
        "one file for the one pinned lexicon"
    );
    assert_eq!(
        lexicons.get(LexiconId::Ugl).unwrap_err(),
        &LexiconUnavailable::NotPinned(LexiconId::Ugl)
    );

    let before = content_files_opened();
    let found = lexicons.lookup("c:H1");
    let entries: Vec<PartEntry> = found.iter().map(|p| p.entry).collect();
    let father = lexicons.get(LexiconId::Uhl).unwrap().entry(1).unwrap();
    assert_eq!(entries, [PartEntry::NoNumber, PartEntry::Found(father)]);
    assert_eq!(
        lexicons.lookup("H3")[0].entry,
        PartEntry::NotInLexicon,
        "the excerpt holds 1, 2 and 10"
    );
    assert!(matches!(
        lexicons.lookup("G35880")[0].entry,
        PartEntry::Unavailable(LexiconUnavailable::NotPinned(LexiconId::Ugl))
    ));
    assert_eq!(content_files_opened(), before, "lookups read no file");
}

#[test]
fn a_lexicon_that_cannot_be_shown_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let store = ResourceStore::open(dir.path()).unwrap();
    let pin = UHL.pin();
    assert_eq!(
        open_lexicon(&store, &pin).unwrap_err().to_string(),
        "uW/en_uhl (72df5ac…) is not on this computer"
    );
    assert_eq!(
        LexiconUnavailable::NotPinned(LexiconId::Ugl).to_string(),
        "the project names no Greek lexicon"
    );

    // A store an earlier build wrote, with the commit's files: kept, and not read.
    let folder = dir
        .path()
        .join("dcs/uw/en_uhl/72df5ac25acf9d51e826b20e3ad883a5a657ef4e");
    fs::create_dir_all(folder.join("content")).unwrap();
    let receipt = serde_json::json!({
        "receiptVersion": 1,
        "source": "dcs",
        "pin": pin,
        "packaging": "archive",
        "files": [],
    });
    fs::write(folder.join("receipt.json"), receipt.to_string()).unwrap();
    assert_eq!(
        open_lexicon(&store, &pin).unwrap_err(),
        LexiconUnavailable::NotRepackaged(pin.clone())
    );
    assert!(folder.join("receipt.json").exists());
}

#[test]
fn a_seeded_lexicon_is_read_from_the_bundled_store_unchanged() {
    let bundled_dir = tempfile::tempdir().unwrap();
    let bundled = ResourceStore::open(bundled_dir.path()).unwrap();
    let zip = fs::read(fixture_dir().join(EXCERPT)).unwrap();
    let pin = excerpt_pin();
    install_lexicon_zip(&bundled, &pin, &excerpt_lock(), &zip, MAX).unwrap();

    let machine_dir = tempfile::tempdir().unwrap();
    let machine = ResourceStore::open(machine_dir.path()).unwrap();
    let report = machine.seed_from(&bundled).unwrap();
    assert_eq!(report.added.len(), 1);
    assert_eq!(
        open_lexicon(&machine, &pin).unwrap(),
        open_lexicon(&bundled, &pin).unwrap()
    );
}
