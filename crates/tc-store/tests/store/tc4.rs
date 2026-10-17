//! tC4's tests ported (`docs/TC4-DELTA-2026-10-16.md` §3.10 and §4 "tc-store"): the staging
//! rename, the start cleanup, the training snapshot, and the open's power-loss scenarios that
//! need no git (the rest are unit S2's).

use crate::common::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tc_journal::command::{Command, Draft, DraftField};
use tc_journal::fold::{Notice, ProjectKind};
use tc_journal::record::Scope;
use tc_store::fault::FaultFs;
use tc_store::{CreateError, EditorText, Hooks, NewProject, OpenOptions, Point};

fn titus_project(name: &str) -> NewProject {
    NewProject {
        name: name.into(),
        kind: ProjectKind::Bible,
        records: first_records(),
        books: vec![(
            TIT,
            Scope::Whole,
            tc_journal::command::SeedSource { usfm: ult("TIT") },
        )],
    }
}

/// `test/sync/openFromDoor43.test.ts:1000-1025`, `syncRig.integration.test.ts:134` (#725).
#[test]
fn a_collision_at_the_rename_keeps_the_destination_byte_for_byte() {
    let hooks = Arc::new(Hooks::default());
    let rig = Arc::new(rig_with_hooks(FaultFs::new(), hooks.clone()));
    rig.store.wait_cleanup();
    hooks.hold(Point::BeforeStagingRename);
    let creating = std::thread::spawn({
        let rig = rig.clone();
        move || rig.store.create(&titus_project("Titus"))
    });
    hooks.wait_reached(Point::BeforeStagingRename, 1);
    // Another project appears at the name between the check and the rename.
    let theirs = Path::new(PROJECTS).join("Titus");
    rig.fs
        .put(&theirs.join("notes.txt"), b"someone else's work", None);
    let before = files(&rig.fs, &theirs);
    hooks.release(Point::BeforeStagingRename);
    let result = creating.join().unwrap();
    assert!(
        matches!(result, Err(CreateError::NameExists(_))),
        "{result:?}"
    );
    assert_eq!(
        files(&rig.fs, &theirs),
        before,
        "the destination keeps every byte"
    );
    assert!(
        !files(&rig.fs, Path::new(PROJECTS))
            .keys()
            .any(|p| p.starts_with(".staging-")),
        "only the staging folder is removed"
    );
}

/// tC4 #676: the import shell waits for `recoverySettled()`.
#[test]
fn an_import_started_while_the_start_cleanup_runs_keeps_its_staging_folder_and_finishes() {
    let fs = FaultFs::new();
    let stale = Path::new(PROJECTS).join(".staging-0123456789abcdef");
    fs.put(&stale.join("ingredients/tc/project.json"), b"{}", None);
    let hooks = Arc::new(Hooks::default());
    hooks.hold(Point::BeforeStagingRemoval);
    let rig = Arc::new(rig_with_hooks(fs.clone(), hooks.clone()));
    hooks.wait_reached(Point::BeforeStagingRemoval, 1);
    let creating = std::thread::spawn({
        let rig = rig.clone();
        move || rig.store.create(&titus_project("Titus"))
    });
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !creating.is_finished(),
        "a create waits for the start cleanup"
    );
    assert!(
        !files(&fs, Path::new(PROJECTS))
            .keys()
            .any(|p| p.starts_with(".staging-") && !p.starts_with(".staging-0123456789abcdef")),
        "no staging folder of the create exists while the cleanup runs"
    );
    hooks.release(Point::BeforeStagingRemoval);
    let created: PathBuf = creating.join().unwrap().unwrap();
    assert!(
        fs.peek(&stale.join("ingredients/tc/project.json"))
            .is_none()
    );
    assert!(fs.peek(&created.join("metadata.json")).is_some());
}

/// `test/trainingBookLoading.test.ts`: training during a book switch never pairs this book's
/// alignments with another book's text.
#[test]
fn training_takes_text_and_alignments_from_one_snapshot_and_this_books_editor_text_only() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    let snapshot = handle.snapshot();
    // A later save is not in the snapshot the training took.
    save_verse(&handle, 1, 1, "changed after the snapshot").unwrap();
    let other_book = EditorText {
        book: tc_journal::BookId::Jonah,
        chapter: tc_journal::ChapterNo::new(1).unwrap(),
        verse: tc_journal::VerseNo::new(1).unwrap(),
        words: "Jonah's unsaved words".into(),
    };
    let input = snapshot.training_input(TIT, Some(&other_book));
    let first = input
        .iter()
        .find(|v| v.chapter.get() == 1 && v.verse.get() == 1)
        .unwrap();
    assert_eq!(first.words, "Paul, a servant of God");
    let this_book = EditorText {
        book: TIT,
        ..other_book
    };
    let input = snapshot.training_input(TIT, Some(&this_book));
    let first = input
        .iter()
        .find(|v| v.chapter.get() == 1 && v.verse.get() == 1)
        .unwrap();
    assert_eq!(first.words, "Jonah's unsaved words");
    handle.close().unwrap();
}

/// `test/sync/syncOperation.test.ts:1178-1476`: a torn, missing or empty `metadata.json`
/// never locks the project.
#[test]
fn a_torn_missing_or_empty_metadata_json_never_locks_the_project() {
    let (rig, folder) = titus();
    let metadata = folder.join("metadata.json");
    let whole = rig.fs.peek(&metadata).unwrap();
    for broken in [
        Some(whole[..whole.len() / 2].to_vec()),
        Some(Vec::new()),
        None,
    ] {
        let fs = rig.fs.copy();
        match &broken {
            Some(bytes) => fs.put(&metadata, bytes, None),
            None => fs.delete(&metadata).unwrap(),
        }
        let rig2 = rig_as(fs.clone(), APP, "host-1");
        let handle = rig2.store.open(&folder, &OpenOptions::default()).unwrap();
        assert_eq!(handle.facts().mode, tc_store::Mode::Writable);
        save_verse(&handle, 1, 1, "still works").unwrap();
        handle.wait_idle();
        handle.close().unwrap();
        assert_valid_burrito(&fs, &folder);
    }
}

/// `LICENSE.md` not UTF-8: the project still opens.
#[test]
fn a_file_that_is_not_utf8_never_stops_an_open() {
    let (rig, folder) = titus();
    rig.fs.put(
        &folder.join("ingredients/LICENSE.md"),
        &[0xff, 0xfe, 0x00, 0x80],
        None,
    );
    rig.fs.put(
        &folder.join("ingredients/TIT.usfm"),
        &[0xc3, 0x28, 0xa0],
        None,
    );
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    assert!(handle.snapshot().fold().book_usfm(TIT).is_some());
    handle.wait_idle();
    handle.close().unwrap();
}

/// Packs written and derived files stale, then a kill or a power cut: the reopen
/// materializes forward and records no outside edit.
#[test]
fn a_reopen_after_packs_and_stale_files_materializes_forward_and_holds_nothing() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    save_verse(&handle, 2, 1, "speak what fits sound teaching").unwrap();
    // A checkpoint whose seal ran and whose materialization did not: kill before the
    // first write of the materialization, found by its intent file.
    handle.wait_idle();
    let probe = rig.fs.copy();
    let probe_rig = rig_as(probe.clone(), APP, "host-1");
    let probe_handle = probe_rig
        .store
        .open(&folder, &OpenOptions::default())
        .unwrap();
    probe_handle.wait_idle();
    probe.clear_log();
    let start = probe.ops();
    probe_handle
        .checkpoint(tc_store::CheckpointReason::LeavingProject)
        .wait()
        .unwrap();
    let intent = probe
        .log()
        .iter()
        .find(|op| op.path.to_string_lossy().contains("materialize.intent"))
        .map(|op| op.index - start)
        .unwrap();
    drop(probe_handle);
    rig.fs.kill_before(intent, false);
    let _ = handle
        .checkpoint(tc_store::CheckpointReason::LeavingProject)
        .wait();
    drop(handle);
    for power_cut in [false, true] {
        let rig2 = rig_as(rig.fs.crash(power_cut), APP, "host-1");
        assert!(!packs(&rig2.fs, &folder).is_empty());
        let handle = rig2.store.open(&folder, &OpenOptions::default()).unwrap();
        handle.wait_idle();
        let notices = handle.notices();
        assert!(
            !notices.iter().any(|n| matches!(
                n.notice,
                Notice::OutsideEditHeld { .. } | Notice::StorageDamage { .. }
            )),
            "{notices:?}"
        );
        let usfm =
            String::from_utf8(rig2.fs.peek(&folder.join("ingredients/TIT.usfm")).unwrap()).unwrap();
        assert!(usfm.contains("speak what fits sound teaching"));
        handle.close().unwrap();
        assert_valid_burrito(&rig2.fs, &folder);
    }
}

/// APP-DESIGN R22: a draft write never delays a command.
#[test]
fn a_held_draft_write_never_delays_a_save() {
    let hooks = Arc::new(Hooks::default());
    let rig = rig_with_hooks(FaultFs::new(), hooks.clone());
    rig.store.wait_cleanup();
    let folder = create_titus(&rig, "Titus");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    hooks.hold(Point::BeforeDraftWrite);
    let shown = handle.mint(&verse(1, 1), None).unwrap();
    handle
        .submit(Command::Draft(Draft {
            field: DraftField::Words,
            shown: vec![shown],
            text: "typing".into(),
        }))
        .wait()
        .unwrap();
    hooks.wait_reached(Point::BeforeDraftWrite, 1);
    let started = std::time::Instant::now();
    save_verse(&handle, 1, 2, "saved while a draft write waits").unwrap();
    assert!(started.elapsed() < Duration::from_secs(2));
    hooks.release(Point::BeforeDraftWrite);
    handle.close().unwrap();
}

/// `docs/TC4-DELTA-2026-10-16.md` §3.3, N1 for a join: the start cleanup says which staging
/// folders it removed and what each was for, so the app can tell the person that opening a
/// project from Door43 did not finish (`refusal.open-door43.removed`). A staging folder for
/// an import or a join is handed out only after the cleanup, so the cleanup never removes
/// one that is in use.
#[test]
fn the_start_cleanup_reports_a_join_it_removed_and_hands_out_no_staging_folder_before_it_ends() {
    use tc_store::{RemovedStaging, StagingKind};
    let fs = FaultFs::new();
    let join = Path::new(PROJECTS).join(".staging-join-00112233445566ff");
    fs.put(&join.join(".git/HEAD"), b"ref: refs/heads/main\n", None);
    let old = Path::new(PROJECTS).join(".staging-0123456789abcdef");
    fs.put(&old.join("x"), b"x", None);
    let hooks = Arc::new(Hooks::default());
    hooks.hold(Point::BeforeStagingRemoval);
    let rig = Arc::new(rig_with_hooks(fs.clone(), hooks.clone()));
    hooks.wait_reached(Point::BeforeStagingRemoval, 1);
    let asking = std::thread::spawn({
        let rig = rig.clone();
        move || rig.store.staging(StagingKind::Import)
    });
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !asking.is_finished(),
        "a staging folder waits for the cleanup"
    );
    hooks.release(Point::BeforeStagingRemoval);
    let staging = asking.join().unwrap();
    assert!(
        staging
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(".staging-import-")
    );
    let removed = rig.store.removed_at_start();
    assert_eq!(
        removed,
        vec![
            RemovedStaging {
                folder: ".staging-0123456789abcdef".into(),
                kind: None,
            },
            RemovedStaging {
                folder: ".staging-join-00112233445566ff".into(),
                kind: Some(StagingKind::Join),
            },
        ]
    );
    assert!(fs.peek(&join.join(".git/HEAD")).is_none());
}

/// `test/sync/syncOperation.test.ts` power-loss scenarios: packs written and derived files
/// stale, then a crash, plus a real edit of the book made outside the app before the reopen.
/// The reopen materializes forward everything but that file, which it holds unchanged with a
/// notice (unit S1 holds rather than records an outside edit, §6.2, §11.1): the edit is never
/// written over, and the save stays in the project.
#[test]
fn a_reopen_after_packs_and_stale_files_keeps_an_outside_edit_of_the_book() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    save_verse(&handle, 2, 1, "speak what fits sound teaching").unwrap();
    handle.wait_idle();
    // Killed before the materialization: the save is in a pending file, TIT.usfm is stale.
    let fs = rig.fs.crash(true);
    drop(handle);
    let usfm = folder.join("ingredients/TIT.usfm");
    let mut edited = fs.peek(&usfm).unwrap();
    let at = edited.windows(3).position(|w| w == b"___").unwrap();
    edited[at..at + 3].copy_from_slice(b"Pau");
    fs.put(&usfm, &edited, None);
    let rig2 = rig_as(fs.clone(), APP, "host-1");
    let handle = rig2.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert!(
        handle.notices().iter().any(|n| matches!(
            &n.notice,
            Notice::OutsideEditHeld { path, .. } | Notice::StorageDamage { path, .. }
                if path.as_str() == "ingredients/TIT.usfm"
        )),
        "{:?}",
        handle.notices()
    );
    assert_eq!(
        fs.peek(&usfm).as_deref(),
        Some(edited.as_slice()),
        "never written over"
    );
    assert_eq!(
        words(&handle, 2, 1).as_deref(),
        Some("speak what fits sound teaching")
    );
    handle.close().unwrap();
    assert_valid_burrito(&fs, &folder);
}

/// `syncOperation.test.ts`, a file that is not UTF-8: a derived file replaced by bytes that
/// are not text still lets the project open and save, and is held, not read as an edit.
#[test]
fn a_derived_file_that_is_not_utf8_never_stops_the_project() {
    let (rig, folder) = titus();
    let fs = rig.fs.crash(false);
    let usfm = folder.join("ingredients/TIT.usfm");
    let garbage: Vec<u8> = (0..300u32).map(|i| (i % 7 * 37 + 128) as u8).collect();
    fs.put(&usfm, &garbage, None);
    let rig2 = rig_as(fs.clone(), APP, "host-1");
    let handle = rig2.store.open(&folder, &OpenOptions::default()).unwrap();
    assert_eq!(handle.facts().mode, tc_store::Mode::Writable);
    save_verse(&handle, 1, 1, "still saves").unwrap();
    handle.leave(tc_store::Leave::Book(TIT)).wait().unwrap();
    assert_eq!(
        fs.peek(&usfm),
        Some(garbage),
        "held: never written over unread"
    );
    assert!(!handle.notices().is_empty());
    handle.close().unwrap();
    assert_valid_burrito(&fs, &folder);
}
