//! Seeding at every start: a copy is checked once, and a repeat start opens no resource
//! file (tC4 #740, the verify-once cases of `scripts/desktop-resources.test.cjs:554-605`,
//! as count tests).

// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use common::install_fixture;
use tc_resources::{Installed, Receipt, ResourcePin, ResourceStore, Sha256, content_files_opened};
use tempfile::TempDir;

/// A bundled store of three real fixture exports, as an installer ships it.
fn bundle() -> (TempDir, Vec<ResourcePin>) {
    let dir = tempfile::tempdir().unwrap();
    let store = ResourceStore::open(dir.path()).unwrap();
    let pins = vec![
        install_fixture(
            &store,
            "en_tn@v91",
            "unfoldingWord/en_tn",
            "v91",
            &[("TIT.tsv", "ingredients/TIT.tsv")],
        ),
        install_fixture(
            &store,
            "en_tw@v87",
            "unfoldingWord/en_tw",
            "v87",
            &[
                ("TIT.tsv", "ingredients/TIT.tsv"),
                ("payload-kt-god.md", "ingredients/payload/kt/god.md"),
            ],
        ),
        install_fixture(
            &store,
            "en_ult@v89",
            "unfoldingWord/en_ult",
            "v89",
            &[("TIT.usfm", "ingredients/TIT.usfm")],
        ),
    ];
    (dir, pins)
}

struct Seeded {
    _bundled_dir: TempDir,
    bundled: ResourceStore,
    pins: Vec<ResourcePin>,
    _machine_dir: TempDir,
    machine: ResourceStore,
}

/// A first start: the bundle seeded into an empty store.
fn first_start() -> Seeded {
    let (bundled_dir, pins) = bundle();
    let bundled = ResourceStore::open_read_only(bundled_dir.path()).unwrap();
    let machine_dir = tempfile::tempdir().unwrap();
    let machine = ResourceStore::open(machine_dir.path()).unwrap();
    assert_eq!(machine.seed_from(&bundled).unwrap().added.len(), 3);
    Seeded {
        _bundled_dir: bundled_dir,
        bundled,
        pins,
        _machine_dir: machine_dir,
        machine,
    }
}

/// Runs `action` and returns how many content files it opened.
fn opened<T>(action: impl FnOnce() -> T) -> (u64, T) {
    let before = content_files_opened();
    let out = action();
    (content_files_opened() - before, out)
}

/// Every file under `dir` with its bytes and modification time.
fn tree(dir: &Path) -> BTreeMap<PathBuf, (Vec<u8>, SystemTime)> {
    let mut out = BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let modified = fs::metadata(&path).unwrap().modified().unwrap();
                out.insert(path.clone(), (fs::read(&path).unwrap(), modified));
            }
        }
    }
    out
}

fn file_count(install: &Installed) -> u64 {
    install.files().count() as u64
}

/// Rewrites a copy's receipt entry for `path` with `size` and `sha256`.
fn set_receipt_entry(install: &Installed, path: &str, size: u64, sha256: Sha256) {
    let file = install.dir().join("receipt.json");
    let mut receipt: Receipt = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    let entry = receipt.files.iter_mut().find(|f| f.path == path).unwrap();
    entry.size = size;
    entry.sha256 = sha256;
    fs::write(&file, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
}

#[test]
fn c29_an_install_opens_each_bundled_file_once_and_a_repeat_start_opens_none() {
    let (bundled_dir, pins) = bundle();
    let bundled = ResourceStore::open_read_only(bundled_dir.path()).unwrap();
    let bundled_files: u64 = bundled
        .installed()
        .unwrap()
        .installs
        .iter()
        .map(file_count)
        .sum();
    let machine_dir = tempfile::tempdir().unwrap();
    let machine = ResourceStore::open(machine_dir.path()).unwrap();

    let (first_opens, first) = opened(|| machine.seed_from(&bundled).unwrap());
    assert_eq!(first.added.len(), 3);
    assert_eq!(
        first_opens, bundled_files,
        "each bundled file is opened once, checked in memory and written once"
    );

    let written = tree(machine_dir.path());
    let (opens, again) = opened(|| machine.seed_from(&bundled).unwrap());
    assert_eq!(opens, 0, "a repeat start opens no resource file");
    assert_eq!(again.already_installed.len(), 3);
    assert!(again.added.is_empty() && again.checked_again.is_empty());
    assert!(again.damaged.is_empty() && again.refused.is_empty());
    assert_eq!(
        tree(machine_dir.path()),
        written,
        "a repeat start writes nothing"
    );
    for pin in &pins {
        machine.dcs(pin).unwrap().unwrap().verify().unwrap();
    }
}

#[test]
fn c29_a_changed_receipt_makes_that_copy_be_checked_again_against_its_own_receipt() {
    let s = first_start();
    let tn = s.machine.dcs(&s.pins[0]).unwrap().unwrap();
    // The copy's own inventory changes (tC4's #640 shape): one more row, and a receipt
    // that agrees with it.
    let tsv = tn.dir().join("content/ingredients/TIT.tsv");
    let mut drifted = fs::read(&tsv).unwrap();
    drifted.extend_from_slice(b"TIT 3:16\tdrift\t\tdrift\t1\t\tdrift\n");
    fs::write(&tsv, &drifted).unwrap();
    set_receipt_entry(
        &tn,
        "ingredients/TIT.tsv",
        drifted.len() as u64,
        Sha256::of(&drifted),
    );

    let (opens, report) = opened(|| s.machine.seed_from(&s.bundled).unwrap());
    assert_eq!(
        opens,
        file_count(&tn),
        "the copy is checked again, each of its files once, and the bundle is not opened"
    );
    assert_eq!(report.checked_again, [s.pins[0].to_string()]);
    assert_eq!(report.already_installed.len(), 3);
    assert!(report.added.is_empty() && report.damaged.is_empty());
    assert_eq!(fs::read(&tsv).unwrap(), drifted, "the copy keeps its bytes");
    let (opens, report) = opened(|| s.machine.seed_from(&s.bundled).unwrap());
    assert_eq!(opens, 0, "the new receipt is recorded as checked");
    assert!(report.checked_again.is_empty());

    // Negative control: a receipt that disagrees with its copy is damaged. The copy is
    // left as it is, and no record vouches for it at the next start either.
    set_receipt_entry(
        &tn,
        "ingredients/TIT.tsv",
        drifted.len() as u64 + 1,
        Sha256::of(&drifted),
    );
    for _ in 0..2 {
        let report = s.machine.seed_from(&s.bundled).unwrap();
        assert_eq!(report.damaged.len(), 1);
        assert_eq!(report.damaged[0].0, s.pins[0].to_string());
        assert!(report.checked_again.is_empty() && report.added.is_empty());
        assert_eq!(report.already_installed.len(), 2);
        assert_eq!(fs::read(&tsv).unwrap(), drifted);
    }
}

#[test]
fn c29_a_missing_or_unreadable_check_record_is_no_check() {
    let s = first_start();
    let tn = s.machine.dcs(&s.pins[0]).unwrap().unwrap();
    let record = tn.dir().join("checked.json");
    let partial = tn.dir().join("checked.json.partial");
    let written = fs::read(&record).unwrap();
    let another_receipt = format!(r#"{{"receiptSha256":"{}"}}"#, Sha256::of(b"another"));
    for wrong in [
        None,
        Some(""),
        Some("checked"),
        Some("[]"),
        Some("{}"),
        Some(r#"{"receiptSha256":42}"#),
        Some(another_receipt.as_str()),
    ] {
        match wrong {
            None => fs::remove_file(&record).unwrap(),
            Some(text) => fs::write(&record, text).unwrap(),
        }
        // What a record write stopped by a crash leaves beside it.
        fs::write(&partial, b"{").unwrap();
        let (opens, report) = opened(|| s.machine.seed_from(&s.bundled).unwrap());
        assert_eq!(opens, file_count(&tn), "{wrong:?}");
        assert_eq!(report.checked_again, [s.pins[0].to_string()], "{wrong:?}");
        assert_eq!(fs::read(&record).unwrap(), written, "{wrong:?}");
        assert!(!partial.exists(), "{wrong:?}");
        let (opens, _) = opened(|| s.machine.seed_from(&s.bundled).unwrap());
        assert_eq!(opens, 0, "{wrong:?}");
    }
    // A store written before copies were recorded has no records: its first start
    // checks each copy once.
    for pin in &s.pins {
        let copy = s.machine.dcs(pin).unwrap().unwrap();
        fs::remove_file(copy.dir().join("checked.json")).unwrap();
    }
    let report = s.machine.seed_from(&s.bundled).unwrap();
    assert_eq!(report.checked_again.len(), 3);
}
