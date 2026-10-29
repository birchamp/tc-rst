//! Repro for #707 and #746 (docs/evidence/fixed-in-4.1-2026-10-29).
//! Goes at crates/tc-store/tests/repro_707.rs. Run:
//!   tc-heavy cargo test -p tc-store --test repro_707 -- --nocapture
//!
//! #746: an import (the tC3 import ends at `Store::create_imported`, tc-app/src/import.rs:1031)
//! started while the start cleanup runs waits for it, then finishes.
//! #707: such an import is never rolled back at the next start: a finished one stays, and one
//! cut off before its rename leaves only a staging folder the next start removes and reports.

#![allow(clippy::unwrap_used, clippy::disallowed_methods)]

#[path = "store/common.rs"]
#[allow(dead_code)]
mod common;

use common::*;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tc_journal::change::{Cause, ImportSource};
use tc_journal::command::SeedSource;
use tc_journal::fold::ProjectKind;
use tc_journal::record::Scope;
use tc_store::fault::FaultFs;
use tc_store::{Hooks, NewProject, OpenOptions, Point, StagingKind};

fn imported(name: &str) -> NewProject {
    NewProject {
        name: name.into(),
        kind: ProjectKind::Bible,
        records: first_records(),
        books: vec![(TIT, Scope::Whole, SeedSource { usfm: ult("TIT") })],
    }
}

fn tc3() -> Cause {
    Cause::Import(ImportSource::Tc3 { user: None })
}

fn stale(fs: &FaultFs) -> std::path::PathBuf {
    let stale = Path::new(PROJECTS).join(".staging-import-0123456789abcdef");
    fs.put(&stale.join("metadata.json"), b"{}", None);
    stale
}

#[test]
fn repro_707_746_an_import_during_the_start_cleanup_waits_and_survives_the_next_start() {
    let fs = FaultFs::new();
    let stale = stale(&fs);
    let hooks = Arc::new(Hooks::default());
    hooks.hold(Point::BeforeStagingRemoval);
    let rig = Arc::new(rig_with_hooks(fs.clone(), hooks.clone()));
    hooks.wait_reached(Point::BeforeStagingRemoval, 1);
    let importing = std::thread::spawn({
        let rig = rig.clone();
        move || rig.store.create_imported(&imported("Titus"), tc3())
    });
    std::thread::sleep(Duration::from_millis(300));
    let waited = !importing.is_finished();
    let own_staging = files(&fs, Path::new(PROJECTS))
        .keys()
        .filter(|p| p.starts_with(".staging-") && !p.starts_with(".staging-import-0123"))
        .count();
    println!("746: import still waiting while cleanup held = {waited}; its staging files = {own_staging}");
    assert!(waited, "the import waits for the start cleanup");
    assert_eq!(own_staging, 0, "no staging folder of the import exists yet");
    hooks.release(Point::BeforeStagingRemoval);
    let folder = importing.join().unwrap().unwrap();
    println!("746: after release the import finished at {}", folder.display());
    assert!(fs.peek(&stale.join("metadata.json")).is_none());

    // The app stops right after the import (power cut), then starts again.
    let after = fs.crash(true);
    let rig2 = rig_as(after.clone(), APP, "host-1");
    let removed = rig2.store.removed_at_start();
    let listed = rig2.store.list().unwrap();
    println!("707: next start removed {removed:?}; projects listed {listed:?}");
    assert!(removed.is_empty(), "nothing is rolled back");
    assert_eq!(listed, vec![folder.clone()]);
    let handle = rig2.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let verse = words(&handle, 1, 1).unwrap_or_default();
    println!("707: TIT 1:1 after the restart = {verse:?}");
    assert!(!verse.is_empty());
    handle.close().unwrap();
}

#[test]
fn repro_707_an_import_cut_off_before_its_rename_leaves_no_project_and_is_reported() {
    let fs = FaultFs::new();
    let rig = Arc::new(rig_with_hooks(fs.clone(), Arc::default()));
    let other = create_titus(&rig, "Other");
    rig.store.wait_cleanup();
    let before = files(&fs, &other);
    rig.store.hooks().hold(Point::BeforeStagingRename);
    let _importing = std::thread::spawn({
        let rig = rig.clone();
        move || rig.store.create_imported(&imported("Titus"), tc3())
    });
    rig.store.hooks().wait_reached(Point::BeforeStagingRename, 1);
    // The app is killed with the import's staging folder full and not yet renamed.
    let after = fs.crash(false);
    let staged: std::collections::BTreeSet<String> = files(&after, Path::new(PROJECTS))
        .keys()
        .filter(|p| p.starts_with(".staging-"))
        .map(|p| p.split('/').next().unwrap().to_owned())
        .collect();
    println!("707: staging folders on disk after the kill = {staged:?}");
    let rig2 = rig_as(after.clone(), APP, "host-1");
    let removed = rig2.store.removed_at_start();
    let listed = rig2.store.list().unwrap();
    println!("707: next start removed {removed:?}; projects listed {listed:?}");
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].kind, Some(StagingKind::Import));
    assert_eq!(listed, vec![other.clone()], "only the other project, untouched");
    assert_eq!(files(&after, &other), before, "the other project keeps every byte");
    assert!(after.files_under(&Path::new(PROJECTS).join("Titus")).is_empty());
}
