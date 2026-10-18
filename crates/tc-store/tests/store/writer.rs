//! The writer thread (`docs/STORAGE.md` §5.0 "One writer per project", §5.10; owner ruling
//! 2026-10-18 in PERFORMANCE.md): derived files are written off the actor, from a snapshot, so
//! a save never waits for them, and the compare-and-swap looks at the file's bytes at the
//! moment it matters.

use crate::common::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tc_journal::fold::Notice;
use tc_store::fault::FaultFs;
use tc_store::{Hooks, Leave, OpenOptions, Point, ProjectHandle, Stored};

fn open(rig: &Rig, folder: &Path) -> ProjectHandle {
    let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle
}

fn usfm(folder: &Path) -> PathBuf {
    folder.join("ingredients/TIT.usfm")
}

fn held_notice(handle: &ProjectHandle) -> Option<(bool, String)> {
    handle.notices().into_iter().find_map(|n| match n.notice {
        Notice::StorageDamage { path, copy } if path.as_str() == "ingredients/TIT.usfm" => {
            Some((true, copy.to_string()))
        }
        Notice::OutsideEditHeld { path, copy } if path.as_str() == "ingredients/TIT.usfm" => {
            Some((false, copy.to_string()))
        }
        _ => None,
    })
}

/// The task's test: a materialization held on the writer does not hold a save.
#[test]
fn a_save_is_durable_while_a_materialize_is_held() {
    let hooks = Arc::new(Hooks::default());
    let rig = rig_with_hooks(FaultFs::new(), hooks.clone());
    rig.store.wait_cleanup();
    let folder = create_titus(&rig, "Titus");
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    hooks.hold(Point::BeforeMaterialize);
    let left = handle.leave(Leave::Book(TIT));
    hooks.wait_reached(Point::BeforeMaterialize, 1);
    for v in 2..=16 {
        let reply = save_verse(&handle, 1, v, &format!("saved while held {v}")).unwrap();
        assert_eq!(reply.stored, Stored::Durable);
    }
    assert!(!left.is_ready(), "the writer is still held");
    hooks.release(Point::BeforeMaterialize);
    left.wait().unwrap();
    // The saves made while it was held are written by the next leave.
    handle.leave(Leave::Book(TIT)).wait().unwrap();
    let text = String::from_utf8(rig.fs.peek(&usfm(&folder)).unwrap()).unwrap();
    assert!(text.contains("saved while held 16"), "{text}");
    assert!(text.contains("Paul, a servant"));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

#[test]
fn a_save_is_durable_while_a_checkpoint_is_held() {
    let hooks = Arc::new(Hooks::default());
    let rig = rig_with_hooks(FaultFs::new(), hooks.clone());
    rig.store.wait_cleanup();
    let folder = create_titus(&rig, "Titus");
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "before the checkpoint").unwrap();
    hooks.hold(Point::BeforeCheckpoint);
    let checkpoint = handle.checkpoint(tc_store::CheckpointReason::LeavingTranslate);
    hooks.wait_reached(Point::BeforeCheckpoint, 1);
    for v in 2..=10 {
        let reply = save_verse(&handle, 1, v, &format!("during {v}")).unwrap();
        assert_eq!(reply.stored, Stored::Durable);
    }
    assert!(!checkpoint.is_ready());
    hooks.release(Point::BeforeCheckpoint);
    checkpoint.wait().unwrap();
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// O10: the open's materialization is the writer's first job, so the open returns, and
/// saves go on, while it waits.
#[test]
fn the_opens_first_materialization_runs_after_the_open_returns() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    // The files as a stopped process leaves them: the save is in a pending file, the
    // derived files do not show it yet.
    let fs = rig.fs.crash(false);
    drop(handle);
    let hooks = Arc::new(Hooks::default());
    let rig = rig_with_hooks(fs, hooks.clone());
    hooks.hold(Point::BeforeMaterialize);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    hooks.wait_reached(Point::BeforeMaterialize, 1);
    let reply = save_verse(&handle, 1, 2, "saved while the open's write waits").unwrap();
    assert_eq!(reply.stored, Stored::Durable);
    hooks.release(Point::BeforeMaterialize);
    handle.wait_idle();
    let text = String::from_utf8(rig.fs.peek(&usfm(&folder)).unwrap()).unwrap();
    assert!(text.contains("Paul, a servant"), "{text}");
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// Owner ruling 2026-10-18: the compare-and-swap reads the path's bytes before it
/// overwrites. An outside edit that keeps the file's size and modification time, found by no
/// stat, is read, held as storage damage with a copy of its bytes, and not written over.
#[test]
fn the_compare_and_swap_reads_a_file_whose_size_and_mtime_are_unchanged() {
    let (rig, folder) = titus();
    // An open settles the entries once their time stamp's tick is over (O14).
    rig.fs.advance(Duration::from_secs(10));
    let handle = open(&rig, &folder);
    handle.close().unwrap();
    rig.fs.advance(Duration::from_secs(10));
    let handle = open(&rig, &folder);
    handle.wait_idle();
    let path = usfm(&folder);
    let mut edited = rig.fs.peek(&path).unwrap();
    let at = edited.iter().position(|b| *b == b'T').unwrap();
    edited[at] = b't';
    let mtime = tc_store::Vfs::metadata(&rig.fs, &path).unwrap().mtime_ns;
    rig.fs.put(&path, &edited, Some(mtime));
    let meta = tc_store::Vfs::metadata(&rig.fs, &path).unwrap();
    assert_eq!(meta.len as usize, edited.len());
    assert_eq!(
        meta.mtime_ns, mtime,
        "size and mtime are as the app left them"
    );

    let reads_before = handle.counters().derived_read;
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    handle.leave(Leave::Book(TIT)).wait().unwrap();
    assert!(handle.counters().derived_read > reads_before, "it was read");
    assert_eq!(
        rig.fs.peek(&path),
        Some(edited.clone()),
        "never written over"
    );
    let (damage, copy) = held_notice(&handle).expect("a hold is listed");
    assert!(damage, "the bytes changed although size and time did not");
    assert_eq!(rig.fs.peek(&folder.join(&copy)), Some(edited));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// I8 at the rename: the file is looked at again once the temp file is complete. A program
/// that saves the file in the window between the compare-and-swap and the rename is not
/// written over: its bytes are kept whole in a copy before anything replaces them.
#[test]
fn an_outside_save_during_the_rename_window_is_never_overwritten() {
    let hooks = Arc::new(Hooks::default());
    let rig = rig_with_hooks(FaultFs::new(), hooks.clone());
    rig.store.wait_cleanup();
    let folder = create_titus(&rig, "Titus");
    let handle = open(&rig, &folder);
    let path = usfm(&folder);
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    hooks.hold(Point::BeforeRename);
    let left = handle.leave(Leave::Book(TIT));
    hooks.wait_reached(Point::BeforeRename, 1);
    let outside = b"\\id TIT saved by Paratext in the window\n".to_vec();
    rig.fs.put(&path, &outside, None);
    hooks.release(Point::BeforeRename);
    left.wait().unwrap();
    handle.wait_idle();
    let copies = copies_of(&handle, "ingredients/TIT.usfm");
    assert_eq!(copies.len(), 1, "{:?}", handle.notices());
    assert_eq!(rig.fs.peek(&folder.join(&copies[0])), Some(outside));
    let written = String::from_utf8(rig.fs.peek(&path).unwrap()).unwrap();
    assert!(written.contains("Paul, a servant"), "{written}");
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// R21 and the owner's ruling: the seal is the one long step of a checkpoint on the actor, and
/// a save does not wait for all of it. A save that is waiting when a step of the seal ends is
/// served before the next step: it is durable, and its pending file is one this seal never saw,
/// so it stays.
#[test]
fn a_save_is_served_between_the_steps_of_a_seal() {
    use std::sync::Mutex;
    let (rig, folder) = titus();
    let handle = Arc::new(open(&rig, &folder));
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    let steps = Arc::new(Mutex::new(0));
    let saved = Arc::new(Mutex::new(None));
    {
        let (handle, steps, saved) = (handle.clone(), steps.clone(), saved.clone());
        rig.store.hooks().at_seal_step(move || {
            // On the actor, between two steps: the save is queued, and served next.
            let mut steps = steps.lock().unwrap();
            *steps += 1;
            if *steps == 2 {
                let shown = handle.mint(&verse(1, 2), None).unwrap();
                *saved.lock().unwrap() =
                    Some(handle.submit(tc_journal::command::Command::SaveVerse {
                        shown,
                        words: "saved while the seal runs".into(),
                    }));
            }
        });
    }
    let report = handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert!(*steps.lock().unwrap() >= 3, "the seal has steps");
    let reply = saved.lock().unwrap().take().expect("a save was queued");
    // Its reply came while the seal ran, and its pending file was not sealed with the others.
    assert!(reply.is_ready(), "the save waited for the seal");
    let reply = reply.wait().unwrap();
    assert_eq!(reply.stored, Stored::Durable);
    assert_eq!(report.sealed.len(), 1, "{report:?}");
    handle.wait_idle();
    assert_eq!(
        pending_files(&rig.fs, &folder).len(),
        1,
        "the new save stays"
    );
    rig.store.hooks().at_seal_step(|| {});
    let handle = Arc::try_unwrap(handle).unwrap_or_else(|_| panic!("no other owner"));
    handle.close().unwrap();
    let handle = open(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 2).as_deref(),
        Some("saved while the seal runs")
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}
