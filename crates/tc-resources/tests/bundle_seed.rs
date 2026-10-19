//! First-launch seeding of the real bundled English suite, against tC4's start counts
//! (docs/PERFORMANCE.md "Start-up", tC4 `docs/evidence/startup-740.md`): a first launch reads
//! each shipped archive once, writes each installed file once and reads none of them back;
//! a repeat launch reads no archive and reads or writes no resource content file. The
//! rebuild ships exports, not tC4's archives, so what is asserted is the rule per file, not
//! tC4's totals (16 archives, 3,739 resource files, 3,792 files in all), which the OBS
//! pictures will change.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;
use std::path::PathBuf;
use tc_resources::{
    ResourceStore, SeedWatch, Seeding, content_files_opened, content_files_written, install_bundle,
    with_opened_paths,
};

use common::real::bundle;

struct Quiet;

impl SeedWatch for Quiet {
    fn done(&self, _files: u64) {}

    fn cancelled(&self) -> bool {
        false
    }
}

fn under(paths: &[PathBuf], root: &std::path::Path) -> usize {
    paths.iter().filter(|p| p.starts_with(root)).count()
}

#[test]
fn the_bundled_english_suite_seeds_as_tc4_counts() {
    let bundled_dir = tempfile::tempdir().unwrap();
    let writer = ResourceStore::open(bundled_dir.path()).unwrap();
    let installed = install_bundle(&bundle(), &writer, &|_| true).unwrap();
    // The suite of bundle/en: the Bible and OBS exports and the two lexicons; the OBS
    // pictures are fetched when the app is packaged.
    assert_eq!(installed.installed.len(), 15, "{:?}", installed.installed);
    assert_eq!(installed.skipped.len(), 1, "{:?}", installed.skipped);
    let bundled = ResourceStore::open_read_only(bundled_dir.path()).unwrap();

    let machine_dir = tempfile::tempdir().unwrap();
    let machine = ResourceStore::open(machine_dir.path()).unwrap();
    // Planning reads receipts: it opens and writes no content file.
    let (written_before, opened_before) = (content_files_written(), content_files_opened());
    let (plan, planning) = with_opened_paths(|| machine.seed_plan(&bundled).unwrap());
    assert!(planning.is_empty(), "planning opened {planning:?}");
    assert_eq!(content_files_written(), written_before);
    assert_eq!(content_files_opened(), opened_before);
    let files = plan.files();
    assert_eq!(
        files, installed.files,
        "the plan names every content file of the bundle"
    );

    // The first launch: each content file is opened once (to be copied), each is written
    // once, none is read back.
    let (seeding, opened) = with_opened_paths(|| machine.seed(plan, &Quiet).unwrap());
    let Seeding::Finished(report) = seeding else {
        panic!("the seeding finished");
    };
    assert_eq!(report.added.len(), 15, "{report:?}");
    assert!(report.refused.is_empty(), "{:?}", report.refused);
    assert_eq!(opened.len() as u64, files, "each file is opened once");
    let distinct: BTreeSet<&PathBuf> = opened.iter().collect();
    assert_eq!(distinct.len(), opened.len(), "no file is opened twice");
    assert_eq!(
        under(&opened, bundled_dir.path()),
        opened.len(),
        "every file read is the bundle's"
    );
    assert_eq!(
        under(&opened, machine_dir.path()),
        0,
        "none of the files written is read back"
    );
    assert_eq!(content_files_written() - written_before, files);
    eprintln!(
        "first launch: 15 installs, {files} content files opened once and written once \
         (tC4: 16 archives, 3,739 resource files, 3,792 files in all)"
    );

    // The repeat launch: nothing to plan, nothing opened, nothing written.
    let (written, opened_count) = (content_files_written(), content_files_opened());
    let (plan, planning) = with_opened_paths(|| machine.seed_plan(&bundled).unwrap());
    assert_eq!(plan.files(), 0, "everything is installed and checked");
    assert!(planning.is_empty());
    let (_, opened) = with_opened_paths(|| machine.seed(plan, &Quiet).unwrap());
    assert!(opened.is_empty(), "a repeat launch opens no content file");
    assert_eq!(content_files_written(), written);
    assert_eq!(content_files_opened(), opened_count);
}
