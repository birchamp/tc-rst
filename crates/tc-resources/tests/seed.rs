//! Seeding at every start: a copy is checked once, and a repeat start opens no resource
//! file (tC4 #740, the verify-once cases of `scripts/desktop-resources.test.cjs:554-605`,
//! as count tests). A first run can be planned from receipts, watched file by file and
//! stopped between files, and what a stop leaves is whole installs the next run builds on
//! (docs/APP-DESIGN.md §4.6, docs/PERFORMANCE.md "Start-up").

// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use common::install_fixture;
use tc_resources::{
    Installed, Origin, Receipt, ResourcePin, ResourceStore, SeedWatch, Seeding, Sha256, StoreError,
    content_files_opened, content_files_written,
};
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
    assert!(again.refused.is_empty());
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
    assert!(report.added.is_empty());
    assert_eq!(fs::read(&tsv).unwrap(), drifted, "the copy keeps its bytes");
    let (opens, report) = opened(|| s.machine.seed_from(&s.bundled).unwrap());
    assert_eq!(opens, 0, "the new receipt is recorded as checked");
    assert!(report.checked_again.is_empty());

    // Negative control: a receipt that disagrees with its copy still stops the start. The
    // copy is left as it is, and no record vouches for it at the next start either.
    set_receipt_entry(
        &tn,
        "ingredients/TIT.tsv",
        drifted.len() as u64 + 1,
        Sha256::of(&drifted),
    );
    let record = fs::read(tn.dir().join("checked.json")).unwrap();
    for _ in 0..2 {
        let problem = damaged_file(&s, &tn, "ingredients/TIT.tsv");
        assert!(problem.contains("bytes, the receipt says"), "{problem}");
        assert_eq!(fs::read(&tsv).unwrap(), drifted);
        assert_eq!(fs::read(tn.dir().join("checked.json")).unwrap(), record);
    }
}

/// Seeds `s` and expects it to stop on `copy`'s `file`; returns the problem it names.
fn damaged_file(s: &Seeded, copy: &Installed, file: &str) -> String {
    match s.machine.seed_from(&s.bundled) {
        Err(StoreError::Damaged {
            path,
            file: found,
            problem,
        }) if path == copy.dir() && found == file => problem,
        other => panic!(
            "expected {file} of {} damaged: {other:?}",
            copy.dir().display()
        ),
    }
}

#[test]
fn c29_a_copy_that_fails_its_check_stops_the_start_and_is_never_overwritten() {
    // tC4 `scripts/desktop-resources.test.cjs:453-463`: an incomplete or conflicting
    // copy at the bundled resource's place cannot be accepted or silently overwritten.
    let s = first_start();
    let tn = s.machine.dcs(&s.pins[0]).unwrap().unwrap();
    let record = tn.dir().join("checked.json");
    let tsv = tn.dir().join("content/ingredients/TIT.tsv");
    let bytes = fs::read(&tsv).unwrap();

    // A missing file, and no record: the copy is checked, and the bundle is not opened.
    fs::remove_file(&record).unwrap();
    fs::remove_file(&tsv).unwrap();
    let (opens, _) = opened(|| damaged_file(&s, &tn, "ingredients/TIT.tsv"));
    assert!(opens <= file_count(&tn), "{opens}");
    assert!(!tsv.exists(), "the copy is not repaired from the bundle");
    assert!(!record.exists(), "no record vouches for the copy");

    // One more byte.
    let mut longer = bytes.clone();
    longer.extend_from_slice(b"corrupted");
    fs::write(&tsv, &longer).unwrap();
    let problem = damaged_file(&s, &tn, "ingredients/TIT.tsv");
    assert!(problem.contains("bytes, the receipt says"), "{problem}");

    // The same size, one byte changed: this start stops, and so does the next.
    let mut flipped = bytes.clone();
    flipped[0] ^= 1;
    fs::write(&tsv, &flipped).unwrap();
    for _ in 0..2 {
        let problem = damaged_file(&s, &tn, "ingredients/TIT.tsv");
        assert!(problem.contains("SHA-256"), "{problem}");
        assert_eq!(fs::read(&tsv).unwrap(), flipped);
        assert!(!record.exists());
    }

    // A receipt that cannot be read, or that names another resource.
    fs::write(&tsv, &bytes).unwrap();
    let receipt = tn.dir().join("receipt.json");
    let own = fs::read(&receipt).unwrap();
    // Neither opens the bundle: no copy is made over it.
    fs::remove_file(&receipt).unwrap();
    let (opens, seeded) = opened(|| s.machine.seed_from(&s.bundled));
    assert!(
        matches!(&seeded, Err(StoreError::BadReceipt { path, .. }) if path == tn.dir()),
        "{seeded:?}"
    );
    assert_eq!(opens, 0);
    let tw = s.machine.dcs(&s.pins[1]).unwrap().unwrap();
    fs::copy(tw.dir().join("receipt.json"), &receipt).unwrap();
    let (opens, seeded) = opened(|| s.machine.seed_from(&s.bundled));
    assert!(
        matches!(&seeded, Err(StoreError::Occupied { path, .. }) if path == tn.dir()),
        "{seeded:?}"
    );
    assert_eq!(opens, 0);
    assert!(!record.exists());

    // Control: the copy's own receipt back, and the copy is checked again and trusted.
    fs::write(&receipt, &own).unwrap();
    let report = s.machine.seed_from(&s.bundled).unwrap();
    assert_eq!(report.checked_again, [s.pins[0].to_string()]);
    assert_eq!(report.already_installed.len(), 3);
    assert!(record.exists());
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

#[test]
fn c29_a_file_no_receipt_lists_stops_no_start() {
    // tC4's `validateTree` (`scripts/desktop-bootstrap.cjs:164-176`) checks only the files
    // its inventory lists, so a file manager's leftover in a copy never stops a start.
    let s = first_start();
    let tn = s.machine.dcs(&s.pins[0]).unwrap().unwrap();
    let record = tn.dir().join("checked.json");
    let strays = [
        tn.dir().join("content/.DS_Store"),
        tn.dir().join("content/ingredients/Thumbs.db"),
    ];
    for stray in &strays {
        fs::write(stray, b"finder").unwrap();
    }

    // No record: a copy checked again, or a store written before copies were recorded.
    fs::remove_file(&record).unwrap();
    let (opens, report) = opened(|| s.machine.seed_from(&s.bundled).unwrap());
    assert_eq!(opens, file_count(&tn), "only the listed files are opened");
    assert_eq!(report.checked_again, [s.pins[0].to_string()]);
    assert_eq!(report.already_installed.len(), 3);
    assert!(report.added.is_empty() && report.refused.is_empty());
    assert!(record.exists(), "the check is recorded");
    for stray in &strays {
        assert_eq!(
            fs::read(stray).unwrap(),
            b"finder",
            "the file is left as it is"
        );
    }
    let (opens, report) = opened(|| s.machine.seed_from(&s.bundled).unwrap());
    assert_eq!(opens, 0);
    assert!(report.checked_again.is_empty());

    // Negative control: beside such a file, a listed file that fails its check still
    // stops the start.
    fs::remove_file(&record).unwrap();
    let tsv = tn.dir().join("content/ingredients/TIT.tsv");
    let mut flipped = fs::read(&tsv).unwrap();
    flipped[0] ^= 1;
    fs::write(&tsv, &flipped).unwrap();
    let problem = damaged_file(&s, &tn, "ingredients/TIT.tsv");
    assert!(problem.contains("SHA-256"), "{problem}");
}

#[test]
fn c29_a_bundled_install_with_a_file_no_receipt_lists_is_copied_without_it() {
    let (bundled_dir, pins) = bundle();
    let bundled = ResourceStore::open_read_only(bundled_dir.path()).unwrap();
    let bundled_tn = bundled.dcs(&pins[0]).unwrap().unwrap();
    fs::write(bundled_tn.dir().join("content/.DS_Store"), b"finder").unwrap();
    let machine_dir = tempfile::tempdir().unwrap();
    let machine = ResourceStore::open(machine_dir.path()).unwrap();

    let (opens, report) = opened(|| machine.seed_from(&bundled).unwrap());
    assert_eq!(report.added.len(), 3);
    assert!(report.refused.is_empty(), "{:?}", report.refused);
    let bundled_files: u64 = bundled
        .installed()
        .unwrap()
        .installs
        .iter()
        .map(file_count)
        .sum();
    assert_eq!(opens, bundled_files, "only the listed files are opened");
    let tn = machine.dcs(&pins[0]).unwrap().unwrap();
    assert!(!tn.dir().join("content/.DS_Store").exists());
    tn.verify().unwrap();
    let (opens, _) = opened(|| machine.seed_from(&bundled).unwrap());
    assert_eq!(opens, 0);
}

/// A watch that records what it hears, and asks to stop once `stop_after` files are done.
struct Recorder {
    done: Mutex<Vec<u64>>,
    stop_after: Option<u64>,
    asked: AtomicU64,
}

impl Recorder {
    fn new(stop_after: Option<u64>) -> Self {
        Self {
            done: Mutex::new(Vec::new()),
            stop_after,
            asked: AtomicU64::new(0),
        }
    }

    fn heard(&self) -> Vec<u64> {
        self.done.lock().unwrap().clone()
    }
}

impl SeedWatch for Recorder {
    fn done(&self, files: u64) {
        self.done.lock().unwrap().push(files);
    }

    fn cancelled(&self) -> bool {
        self.asked.fetch_add(1, Ordering::Relaxed);
        let last = self.done.lock().unwrap().last().copied().unwrap_or(0);
        self.stop_after.is_some_and(|stop| last >= stop)
    }
}

/// Runs `action` and returns how many content files it opened and how many it wrote.
fn counted<T>(action: impl FnOnce() -> T) -> (u64, u64, T) {
    let (opened, written) = (content_files_opened(), content_files_written());
    let out = action();
    (
        content_files_opened() - opened,
        content_files_written() - written,
        out,
    )
}

/// The name seeding reports an install by.
fn name_of(install: &Installed) -> String {
    match install.origin() {
        Origin::Dcs { pin, .. } => pin.to_string(),
        Origin::Aquifer { pin, .. } => pin.to_string(),
    }
}

/// The files of every install of `store`.
fn files_of(store: &ResourceStore) -> u64 {
    store
        .installed()
        .unwrap()
        .installs
        .iter()
        .map(file_count)
        .sum()
}

/// Every file under `dir` by its path below `dir`, with its bytes.
fn contents(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    tree(dir)
        .into_iter()
        .map(|(path, (bytes, _))| (path.strip_prefix(dir).unwrap().to_path_buf(), bytes))
        .collect()
}

/// The files of the bundled installs that `machine` does not hold.
fn files_missing(machine: &ResourceStore, bundled: &ResourceStore) -> u64 {
    bundled
        .installed()
        .unwrap()
        .installs
        .iter()
        .filter(|install| !machine.has_dcs(&install.dcs_pin().unwrap()))
        .map(file_count)
        .sum()
}

fn finished(seeding: Seeding) -> tc_resources::SeedReport {
    match seeding {
        Seeding::Finished(report) => report,
        Seeding::Cancelled(report) => panic!("expected a finished seeding: {report:?}"),
    }
}

fn cancelled(seeding: Seeding) -> tc_resources::SeedReport {
    match seeding {
        Seeding::Cancelled(report) => report,
        Seeding::Finished(report) => panic!("expected a stopped seeding: {report:?}"),
    }
}

#[test]
fn c29_a_plan_names_every_bundled_file_and_opens_none() {
    let (bundled_dir, _) = bundle();
    let bundled = ResourceStore::open_read_only(bundled_dir.path()).unwrap();
    let machine_dir = tempfile::tempdir().unwrap();
    let machine = ResourceStore::open(machine_dir.path()).unwrap();
    let before = tree(machine_dir.path());

    let (opens, written, plan) = counted(|| machine.seed_plan(&bundled).unwrap());

    assert_eq!(plan.files(), files_of(&bundled));
    assert!(!plan.is_empty());
    assert!(plan.report().already_installed.is_empty() && plan.report().refused.is_empty());
    assert_eq!(opens, 0, "a plan reads receipts and check records only");
    assert_eq!(written, 0);
    assert_eq!(tree(machine_dir.path()), before, "a plan writes nothing");
}

#[test]
fn c29_seeding_reports_each_file_once_up_to_the_plan() {
    let (bundled_dir, pins) = bundle();
    let bundled = ResourceStore::open_read_only(bundled_dir.path()).unwrap();
    let machine_dir = tempfile::tempdir().unwrap();
    let machine = ResourceStore::open(machine_dir.path()).unwrap();
    let plan = machine.seed_plan(&bundled).unwrap();
    let files = plan.files();
    let watch = Recorder::new(None);

    let (opens, written, seeding) = counted(|| machine.seed(plan, &watch).unwrap());

    let report = finished(seeding);
    assert_eq!(report.added.len(), 3);
    assert_eq!(
        watch.heard(),
        (1..=files).collect::<Vec<_>>(),
        "each file is reported once, in order, up to the plan's count"
    );
    assert_eq!(opens, files, "each bundled file is opened once");
    assert_eq!(written, files, "each installed file is written once");
    for pin in &pins {
        machine.dcs(pin).unwrap().unwrap().verify().unwrap();
    }
}

#[test]
fn c29_a_repeat_plan_is_empty_and_seeding_it_does_nothing() {
    let s = first_start();
    let before = tree(s._machine_dir.path());

    let (opens, written, plan) = counted(|| s.machine.seed_plan(&s.bundled).unwrap());
    assert!(plan.is_empty());
    assert_eq!(plan.files(), 0);
    assert_eq!(plan.report().already_installed.len(), 3);
    assert_eq!((opens, written), (0, 0));

    let watch = Recorder::new(None);
    let (opens, written, seeding) = counted(|| s.machine.seed(plan, &watch).unwrap());
    let report = finished(seeding);
    assert_eq!(report.already_installed.len(), 3);
    assert!(report.added.is_empty() && report.checked_again.is_empty());
    assert_eq!((opens, written), (0, 0));
    assert!(watch.heard().is_empty());
    assert_eq!(
        tree(s._machine_dir.path()),
        before,
        "bytes and modification times stay"
    );
}

#[test]
fn c29_a_copy_to_check_again_is_in_the_plan() {
    let s = first_start();
    let tn = s.machine.dcs(&s.pins[0]).unwrap().unwrap();
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

    let plan = s.machine.seed_plan(&s.bundled).unwrap();
    assert_eq!(plan.files(), file_count(&tn));
    assert_eq!(plan.report().already_installed.len(), 2);
    let watch = Recorder::new(None);
    let (opens, written, seeding) = counted(|| s.machine.seed(plan, &watch).unwrap());

    let report = finished(seeding);
    assert_eq!(report.checked_again, [s.pins[0].to_string()]);
    assert_eq!(report.already_installed.len(), 3);
    assert_eq!(watch.heard(), (1..=file_count(&tn)).collect::<Vec<_>>());
    assert_eq!((opens, written), (file_count(&tn), 0));
}

#[test]
fn c29_a_stop_inside_a_check_records_nothing_and_the_next_seed_checks_again() {
    let s = first_start();
    let tn = s.machine.dcs(&s.pins[0]).unwrap().unwrap();
    let record = tn.dir().join("checked.json");
    fs::remove_file(&record).unwrap();
    let watch = Recorder::new(Some(1));

    let plan = s.machine.seed_plan(&s.bundled).unwrap();
    let (opens, written, seeding) = counted(|| s.machine.seed(plan, &watch).unwrap());

    let report = cancelled(seeding);
    assert!(report.checked_again.is_empty());
    assert_eq!(watch.heard(), [1], "the stop comes before the second file");
    assert_eq!((opens, written), (1, 0));
    assert!(
        !record.exists(),
        "a check that did not finish vouches for nothing"
    );

    let plan = s.machine.seed_plan(&s.bundled).unwrap();
    assert_eq!(plan.files(), file_count(&tn));
    let report = finished(s.machine.seed(plan, &Recorder::new(None)).unwrap());
    assert_eq!(report.checked_again, [s.pins[0].to_string()]);
    assert!(record.exists());
}

#[test]
fn c29_a_stop_before_the_first_file_opens_and_writes_nothing() {
    let (bundled_dir, _) = bundle();
    let bundled = ResourceStore::open_read_only(bundled_dir.path()).unwrap();
    let machine_dir = tempfile::tempdir().unwrap();
    let machine = ResourceStore::open(machine_dir.path()).unwrap();
    let plan = machine.seed_plan(&bundled).unwrap();
    let watch = Recorder::new(Some(0));

    let (opens, written, seeding) = counted(|| machine.seed(plan, &watch).unwrap());

    let report = cancelled(seeding);
    assert!(report.added.is_empty());
    assert_eq!((opens, written), (0, 0));
    assert!(watch.heard().is_empty());
    assert!(machine.installed().unwrap().installs.is_empty());
    assert!(
        tree(machine_dir.path()).is_empty(),
        "nothing is left behind"
    );
}

#[test]
fn c29_a_stop_inside_a_copy_keeps_whole_installs_and_the_next_seed_finishes() {
    let (bundled_dir, _) = bundle();
    let bundled = ResourceStore::open_read_only(bundled_dir.path()).unwrap();
    let in_order: Vec<(String, u64)> = bundled
        .installed()
        .unwrap()
        .installs
        .iter()
        .map(|install| (name_of(install), file_count(install)))
        .collect();
    let (first, first_files) = in_order[0].clone();
    let second_files = in_order[1].1;
    assert!(
        second_files > 1,
        "the fixture's second install has several files"
    );
    let machine_dir = tempfile::tempdir().unwrap();
    let machine = ResourceStore::open(machine_dir.path()).unwrap();
    let plan = machine.seed_plan(&bundled).unwrap();
    let total = plan.files();
    // After the first file of the second install, so inside its copy.
    let watch = Recorder::new(Some(first_files + 1));

    let (_, written, seeding) = counted(|| machine.seed(plan, &watch).unwrap());

    let report = cancelled(seeding);
    assert_eq!(
        report.added,
        std::slice::from_ref(&first),
        "only the finished copy is added"
    );
    assert_eq!(written, first_files + 1);
    let installs = machine.installed().unwrap().installs;
    assert_eq!(installs.len(), 1, "the cut copy is not in the store");
    installs[0].verify().unwrap();
    let kept = tree(installs[0].dir());
    let partial = tree(&machine_dir.path().join(".partial"));
    assert_eq!(
        partial.len(),
        1,
        "the cut copy's one written file is staged"
    );

    // The next start clears the staging and copies what is missing.
    let machine = ResourceStore::open(machine_dir.path()).unwrap();
    assert!(!machine_dir.path().join(".partial").exists());
    let plan = machine.seed_plan(&bundled).unwrap();
    assert_eq!(plan.files(), total - first_files);
    assert_eq!(
        plan.report().already_installed,
        std::slice::from_ref(&first)
    );
    let (opens, written, seeding) = counted(|| machine.seed(plan, &Recorder::new(None)).unwrap());
    let report = finished(seeding);
    assert_eq!(report.added.len(), 2);
    assert_eq!(
        opens,
        total - first_files,
        "the earlier copy is not read again"
    );
    assert_eq!(written, total - first_files);
    assert_eq!(
        tree(installs[0].dir()),
        kept,
        "the earlier copy keeps its bytes and times"
    );
    assert_eq!(machine.installed().unwrap().installs.len(), 3);
    for install in machine.installed().unwrap().installs {
        install.verify().unwrap();
    }
}

#[test]
fn c29_a_stop_asked_at_the_last_file_of_a_copy_leaves_that_copy_finished() {
    let (bundled_dir, _) = bundle();
    let bundled = ResourceStore::open_read_only(bundled_dir.path()).unwrap();
    let in_order: Vec<(String, u64)> = bundled
        .installed()
        .unwrap()
        .installs
        .iter()
        .map(|install| (name_of(install), file_count(install)))
        .collect();
    assert_eq!(in_order.len(), 3);

    let mut through = 0;
    for (index, (_, files)) in in_order.iter().enumerate() {
        through += files;
        // The watch asks to stop as soon as this install's last file is done.
        let machine_dir = tempfile::tempdir().unwrap();
        let machine = ResourceStore::open(machine_dir.path()).unwrap();
        let plan = machine.seed_plan(&bundled).unwrap();
        let watch = Recorder::new(Some(through));

        let (opens, written, seeding) = counted(|| machine.seed(plan, &watch).unwrap());

        let last = index + 1 == in_order.len();
        let report = if last {
            // Nothing is left to stop: the seeding is over.
            finished(seeding)
        } else {
            cancelled(seeding)
        };
        let expected: Vec<String> = in_order[..=index]
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
        assert_eq!(
            report.added, expected,
            "the copy whose last file was written is finished, not thrown away (stop at {through})"
        );
        assert_eq!((opens, written), (through, through));
        let installs = machine.installed().unwrap().installs;
        assert_eq!(installs.len(), index + 1);
        for install in &installs {
            install.verify().unwrap();
        }
        assert!(
            tree(&machine_dir.path().join(".partial")).is_empty(),
            "no finished copy is left in the staging folder"
        );
    }
}

/// A folder made read-only, which holds its entries in place; its permissions come back when
/// it is dropped, so the temporary folder can be removed.
#[cfg(unix)]
struct Held(PathBuf);

#[cfg(unix)]
impl Held {
    fn new(dir: PathBuf) -> Self {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
        Self(dir)
    }
}

#[cfg(unix)]
impl Drop for Held {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
    }
}

#[cfg(unix)]
#[test]
fn c29_a_leftover_that_cannot_be_removed_comes_back_and_stops_neither_the_open_nor_a_seed() {
    let (bundled_dir, _) = bundle();
    let bundled = ResourceStore::open_read_only(bundled_dir.path()).unwrap();
    let machine_dir = tempfile::tempdir().unwrap();
    // What a crash left in `.partial/`, with a folder that cannot be emptied (on Windows a
    // scanner holding one of the files does the same).
    let old = machine_dir.path().join(".partial/1-2-3");
    fs::create_dir_all(old.join("content")).unwrap();
    fs::write(old.join("content/a.md"), b"half").unwrap();
    let held = Held::new(old.clone());
    if fs::write(old.join("probe"), b"").is_ok() {
        eprintln!("the permission is not enforced here (running as root): nothing to prove");
        return;
    }

    assert!(
        matches!(
            ResourceStore::open(machine_dir.path()),
            Err(StoreError::Io {
                action: "clear",
                ..
            })
        ),
        "open still refuses, as other callers rely on"
    );
    let (machine, left) = ResourceStore::open_keeping_leftovers(machine_dir.path()).unwrap();
    let Some(StoreError::Io { action, path, .. }) = left else {
        panic!("the leftover was not reported: {left:?}")
    };
    assert_eq!(action, "clear");
    assert_eq!(path, machine_dir.path().join(".partial"));
    assert!(old.is_dir(), "the leftover is still there");

    // The store works beside the leftover: a seed lands whole installs by rename.
    let report = machine.seed_from(&bundled).unwrap();
    assert_eq!(report.added.len(), 3);
    for install in machine.installed().unwrap().installs {
        install.verify().unwrap();
    }
    drop(held);

    // Once the leftover can go, the next open removes it and says nothing.
    let (_, left) = ResourceStore::open_keeping_leftovers(machine_dir.path()).unwrap();
    assert!(left.is_none(), "{left:?}");
    assert!(!machine_dir.path().join(".partial").exists());
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(12))]

    #[test]
    fn c29_any_stops_resume_to_the_store_of_a_seeding_that_never_stopped(
        stops in proptest::collection::vec(0u64..12, 0..4),
    ) {
        let (bundled_dir, _) = bundle();
        let bundled = ResourceStore::open_read_only(bundled_dir.path()).unwrap();
        let reference_dir = tempfile::tempdir().unwrap();
        ResourceStore::open(reference_dir.path()).unwrap().seed_from(&bundled).unwrap();
        let machine_dir = tempfile::tempdir().unwrap();

        // Each run is a new start: the store is opened again, which clears what the
        // earlier stop left in `.partial/`.
        for stop in stops {
            let machine = ResourceStore::open(machine_dir.path()).unwrap();
            let plan = machine.seed_plan(&bundled).unwrap();
            let planned = plan.files();
            proptest::prop_assert_eq!(planned, files_missing(&machine, &bundled));
            let watch = Recorder::new(Some(stop));
            let (opens, written, seeding) = counted(|| machine.seed(plan, &watch).unwrap());
            let heard = watch.heard();
            proptest::prop_assert_eq!(&heard, &(1..=heard.len() as u64).collect::<Vec<_>>());
            proptest::prop_assert_eq!(written, heard.last().copied().unwrap_or(0));
            match seeding {
                Seeding::Finished(_) => proptest::prop_assert_eq!((opens, written), (planned, planned)),
                Seeding::Cancelled(_) => proptest::prop_assert!(written <= planned && stop <= planned),
            }
            for install in machine.installed().unwrap().installs {
                install.verify().unwrap();
            }
        }

        let machine = ResourceStore::open(machine_dir.path()).unwrap();
        let plan = machine.seed_plan(&bundled).unwrap();
        let planned = plan.files();
        proptest::prop_assert_eq!(planned, files_missing(&machine, &bundled));
        let (opens, written, seeding) = counted(|| machine.seed(plan, &Recorder::new(None)).unwrap());
        finished(seeding);
        proptest::prop_assert_eq!((opens, written), (planned, planned));
        proptest::prop_assert_eq!(contents(machine_dir.path()), contents(reference_dir.path()));
    }
}

#[test]
fn c29_an_upgraded_bundle_copies_only_its_new_install() {
    let (bundled_dir, pins) = bundle();
    let bundled = ResourceStore::open_read_only(bundled_dir.path()).unwrap();
    let machine_dir = tempfile::tempdir().unwrap();
    let machine = ResourceStore::open(machine_dir.path()).unwrap();
    machine.seed_from(&bundled).unwrap();
    let before = tree(machine_dir.path());

    // The next release of the installer ships one more install.
    let newer = install_fixture(
        &ResourceStore::open(bundled_dir.path()).unwrap(),
        "en_tn@v86",
        "unfoldingWord/en_tn",
        "v86",
        &[
            ("TIT.tsv", "ingredients/TIT.tsv"),
            ("JON.tsv", "ingredients/JON.tsv"),
        ],
    );
    let new_files = file_count(&bundled.dcs(&newer).unwrap().unwrap());

    let plan = machine.seed_plan(&bundled).unwrap();
    assert_eq!(plan.files(), new_files);
    assert_eq!(plan.report().already_installed.len(), 3);
    let (opens, written, seeding) = counted(|| machine.seed(plan, &Recorder::new(None)).unwrap());

    let report = finished(seeding);
    assert_eq!(report.added, [newer.to_string()]);
    assert_eq!((opens, written), (new_files, new_files));
    for (path, old) in &before {
        assert_eq!(&tree(machine_dir.path())[path], old, "{}", path.display());
    }
    for pin in pins.iter().chain([&newer]) {
        machine.dcs(pin).unwrap().unwrap().verify().unwrap();
    }
}

#[test]
fn c29_a_damaged_bundled_install_still_counts_toward_the_plan() {
    let (bundled_dir, pins) = bundle();
    let bundled = ResourceStore::open_read_only(bundled_dir.path()).unwrap();
    let damaged = bundled.dcs(&pins[1]).unwrap().unwrap();
    let tsv = damaged.dir().join("content/ingredients/TIT.tsv");
    let mut flipped = fs::read(&tsv).unwrap();
    flipped[0] ^= 1;
    fs::write(&tsv, &flipped).unwrap();
    let machine_dir = tempfile::tempdir().unwrap();
    let machine = ResourceStore::open(machine_dir.path()).unwrap();
    let plan = machine.seed_plan(&bundled).unwrap();
    let files = plan.files();
    let watch = Recorder::new(None);

    let report = finished(machine.seed(plan, &watch).unwrap());

    assert_eq!(report.added.len(), 2, "the other installs are copied");
    assert_eq!(report.refused.len(), 1);
    assert!(
        report.refused[0].0.contains("en_tw"),
        "{:?}",
        report.refused
    );
    let heard = watch.heard();
    assert_eq!(
        heard.last(),
        Some(&files),
        "the count still reaches the plan's"
    );
    assert!(heard.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(machine.dcs(&pins[1]).unwrap().is_none());
}
