//! A file another program holds (Windows hazard 2 of `docs/evidence/windows-build-2026-10-17.md`):
//! Defender, the Search indexer and OneDrive open a file just written without
//! `FILE_SHARE_DELETE`, and a rename or removal of it fails for a moment. A save retries
//! within a bound (`docs/STORAGE.md` §5.0 step 3); a save that still fails leaves a complete
//! temporary file, and the next open must neither fold it beside the person's later save
//! (a false "another computer was saving as this one" and a conflict with their own text) nor
//! remove it (§6.1 O2, §5.0 B3).

use crate::common::*;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tc_journal::fold::{Anomaly, Notice, RotationReason};
use tc_store::fault::FaultFs;
use tc_store::{CommandError, OpenOptions, ProjectHandle, Stored};

fn open(rig: &Rig, folder: &Path) -> ProjectHandle {
    let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle
}

fn notices(handle: &ProjectHandle) -> Vec<Notice> {
    handle.notices().into_iter().map(|n| n.notice).collect()
}

fn pending_dir(folder: &Path) -> PathBuf {
    folder.join(".tc/pending")
}

/// The temporary files of pending files, by path relative to the project folder.
fn temps(fs: &FaultFs, folder: &Path) -> Vec<String> {
    files(fs, folder)
        .into_keys()
        .filter(|p| p.starts_with(".tc/pending/.~"))
        .collect()
}

/// The copies the open kept of saves that failed.
fn kept(fs: &FaultFs, folder: &Path) -> Vec<(String, Vec<u8>)> {
    files(fs, folder)
        .into_iter()
        .filter(|(p, _)| p.starts_with(".tc/external/") && p.contains("-not-saved-"))
        .collect()
}

fn no_false_other_computer(handle: &ProjectHandle) {
    assert!(
        !handle
            .snapshot()
            .fold()
            .anomalies()
            .any(|a| matches!(a, Anomaly::DuplicateDevice { .. })),
        "the project says another computer saved as this one"
    );
    assert!(
        !notices(handle).iter().any(|n| matches!(
            n,
            Notice::DeviceRotated {
                reason: RotationReason::DuplicateDevice,
                ..
            }
        )),
        "the open started a new device because of a save that failed"
    );
}

/// A scanner that lets go after a few tries: the save is durable, and the waits are bounded.
#[test]
fn a_save_whose_rename_is_held_a_few_times_is_durable() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    rig.fs.hold_new_files(&pending_dir(&folder), Some(3));
    let reply = save_verse(&handle, 1, 1, "held three times").unwrap();
    assert!(matches!(reply.stored, Stored::Durable));
    let waits = rig.fs.held_waits();
    assert_eq!(waits.len(), 3, "{waits:?}");
    assert!(waits.iter().sum::<Duration>() <= Duration::from_millis(2_004));
    assert_eq!(pending_files(&rig.fs, &folder).len(), 1);
    assert!(temps(&rig.fs, &folder).is_empty());
    assert!(handle.counters().held_retries >= 3);
    drop(handle);

    let after = rig_as(rig.fs.crash(false), APP, "host-1");
    let handle = open(&after, &folder);
    assert_eq!(words(&handle, 1, 1).as_deref(), Some("held three times"));
    handle.close().unwrap();
}

/// The task's test: a save that cannot be written leaves its temp (the removal is held too),
/// the person saves again, and no open rolls the failed attempt into the seq the second save
/// used.
#[test]
fn a_held_save_past_the_retries_is_not_saved_and_leaves_no_false_conflict() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    rig.fs.hold_new_files(&pending_dir(&folder), None);
    let failed = save_verse(&handle, 1, 1, "A, the attempt that failed");
    assert!(
        matches!(failed, Err(CommandError::NotWritten(_))),
        "{failed:?}"
    );
    let left = temps(&rig.fs, &folder);
    assert_eq!(left.len(), 1, "the temp stays: {left:?}");
    let attempt = files(&rig.fs, &folder)[&left[0]].clone();
    let waits = rig.fs.held_waits();
    assert_eq!(waits.len(), 20, "10 for the rename and 10 for the removal");
    assert!(waits.iter().sum::<Duration>() <= Duration::from_millis(2 * 2_004));

    rig.fs.release_new_files(&pending_dir(&folder));
    let reply = save_verse(&handle, 1, 1, "B, the save that worked").unwrap();
    assert!(matches!(reply.stored, Stored::Durable));
    handle.close().unwrap();

    let handle = open(&rig, &folder);
    no_false_other_computer(&handle);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("B, the save that worked")
    );
    assert_eq!(
        handle.snapshot().fold().conflict_count(),
        0,
        "no conflict with the person's own text"
    );
    assert!(temps(&rig.fs, &folder).is_empty());
    let copies = kept(&rig.fs, &folder);
    assert_eq!(copies.len(), 1, "{copies:?}");
    assert_eq!(
        copies[0].1, attempt,
        "the copy is the attempt, byte for byte"
    );
    assert!(notices(&handle).iter().any(|n| matches!(
        n,
        Notice::UnsavedAttemptKept { changes: 1, copy, .. } if copy.contains("-not-saved-")
    )));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// The person never saved again: the attempt is the newest text, and the next open folds it
/// without a notice or a new device.
#[test]
fn a_held_save_never_retried_is_saved_at_the_next_open() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let device = handle.facts().device;
    rig.fs.hold_new_files(&pending_dir(&folder), None);
    assert!(save_verse(&handle, 1, 1, "A, never retried").is_err());
    rig.fs.release_new_files(&pending_dir(&folder));
    handle.close().unwrap();

    let handle = open(&rig, &folder);
    assert_eq!(handle.facts().device, device, "no new device");
    no_false_other_computer(&handle);
    assert_eq!(words(&handle, 1, 1).as_deref(), Some("A, never retried"));
    assert_eq!(handle.snapshot().fold().conflict_count(), 0);
    assert!(
        !notices(&handle)
            .iter()
            .any(|n| matches!(n, Notice::UnsavedAttemptKept { .. }))
    );
    assert!(temps(&rig.fs, &folder).is_empty());
    assert!(kept(&rig.fs, &folder).is_empty());
    handle.close().unwrap();
}

/// Two attempts failed, the person gave up: the later is the text, the earlier is the copy.
#[test]
fn of_two_failed_attempts_the_later_one_is_kept() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    rig.fs.hold_new_files(&pending_dir(&folder), None);
    assert!(save_verse(&handle, 1, 1, "A, the earlier attempt").is_err());
    assert!(save_verse(&handle, 1, 1, "B, the later attempt").is_err());
    assert_eq!(temps(&rig.fs, &folder).len(), 2);
    rig.fs.release_new_files(&pending_dir(&folder));
    handle.close().unwrap();

    let handle = open(&rig, &folder);
    no_false_other_computer(&handle);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("B, the later attempt")
    );
    assert_eq!(handle.snapshot().fold().conflict_count(), 0);
    let copies = kept(&rig.fs, &folder);
    assert_eq!(copies.len(), 1, "{copies:?}");
    let text = String::from_utf8_lossy(&copies[0].1).into_owned();
    assert!(text.contains("A, the earlier attempt"), "{text}");
    handle.close().unwrap();
}

/// A complete temp the open cannot rename into place (the scanner still holds it): its seq
/// is not free, so the open starts a new device; when the scanner lets go, the next open
/// folds the text, and nothing says another computer saved as this one.
#[test]
fn a_complete_temp_that_cannot_be_placed_rotates_the_device() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let device = handle.facts().device;
    rig.fs.hold_new_files(&pending_dir(&folder), None);
    assert!(save_verse(&handle, 1, 1, "A, held at the next open").is_err());
    rig.fs.release_new_files(&pending_dir(&folder));
    handle.close().unwrap();
    let left = temps(&rig.fs, &folder);
    assert_eq!(left.len(), 1);
    let abs = folder.join(&left[0]);
    rig.fs.set_busy(&abs, true);

    let handle = open(&rig, &folder);
    assert_ne!(handle.facts().device, device, "a new device");
    assert!(notices(&handle).iter().any(|n| matches!(
        n,
        Notice::DeviceRotated { old, reason: RotationReason::PendingUnreadable, .. } if *old == device
    )));
    assert_ne!(
        words(&handle, 1, 1).as_deref(),
        Some("A, held at the next open"),
        "not folded while it cannot be placed"
    );
    save_verse(&handle, 1, 2, "a save works meanwhile").unwrap();
    handle.close().unwrap();
    assert!(rig.fs.peek(&abs).is_some(), "the temp is still there");

    rig.fs.set_busy(&abs, false);
    let handle = open(&rig, &folder);
    no_false_other_computer(&handle);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("A, held at the next open")
    );
    assert_eq!(
        words(&handle, 1, 2).as_deref(),
        Some("a save works meanwhile")
    );
    handle.close().unwrap();
}

/// What cannot be read is never removed.
#[test]
fn an_open_never_removes_a_temp_it_cannot_read() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    rig.fs.hold_new_files(&pending_dir(&folder), None);
    assert!(save_verse(&handle, 1, 1, "A, in a file nobody can read").is_err());
    rig.fs.release_new_files(&pending_dir(&folder));
    handle.close().unwrap();
    let left = temps(&rig.fs, &folder);
    let abs = folder.join(&left[0]);
    let bytes = rig.fs.peek(&abs).unwrap();
    rig.fs.set_unreadable(&abs, true);

    let handle = open(&rig, &folder);
    handle.close().unwrap();
    assert_eq!(rig.fs.peek(&abs), Some(bytes), "the temp is untouched");

    rig.fs.set_unreadable(&abs, false);
    let handle = open(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("A, in a file nobody can read")
    );
    no_false_other_computer(&handle);
    handle.close().unwrap();
}

/// A seal whose pending file another program holds keeps the file; the session knows it sealed
/// it, so the next checkpoint finds no second writer in it (the project stays writable and
/// commits), and the next open removes it.
#[test]
fn a_pending_file_the_seal_could_not_remove_is_no_second_writer() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "sealed while the file is held").unwrap();
    let held = pending_files(&rig.fs, &folder);
    let abs = folder.join(&held[0]);
    rig.fs.set_busy(&abs, true);
    let first = handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert!(first.commit.is_some(), "{first:?}");
    assert!(rig.fs.peek(&abs).is_some(), "the held file stays");
    save_verse(&handle, 1, 2, "a save after the seal").unwrap();
    let second = handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert_eq!(second.skipped, None, "{second:?}: {:?}", notices(&handle));
    assert!(second.commit.is_some(), "{second:?}");
    assert!(
        !notices(&handle)
            .iter()
            .any(|n| matches!(n, Notice::SecondWriter { .. })),
        "{:?}",
        notices(&handle)
    );
    save_verse(&handle, 1, 3, "the project is still writable").unwrap();
    handle.close().unwrap();
    rig.fs.set_busy(&abs, false);
    let handle = open(&rig, &folder);
    assert!(rig.fs.peek(&abs).is_none(), "the next open removes it");
    assert_eq!(
        words(&handle, 1, 3).as_deref(),
        Some("the project is still writable")
    );
    handle.close().unwrap();
}

/// The same at the open: O7 finds a pending file a valid pack holds every change of, and
/// another program still holds it, so it stays; the session's first checkpoint finds no second
/// writer in it.
#[test]
fn a_pending_file_the_open_could_not_remove_is_no_second_writer() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "sealed while the file is held").unwrap();
    let held = pending_files(&rig.fs, &folder);
    let abs = folder.join(&held[0]);
    rig.fs.set_busy(&abs, true);
    let sealed = handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert!(sealed.commit.is_some(), "{sealed:?}");
    let fs = rig.fs.crash(false);
    drop(handle);
    let rig = rig_as(fs, APP, "host-1");
    rig.fs.set_busy(&abs, true);
    let handle = open(&rig, &folder);
    assert!(rig.fs.peek(&abs).is_some(), "the held file stays");
    save_verse(&handle, 1, 2, "a save after the open").unwrap();
    let report = handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert_eq!(report.skipped, None, "{report:?}: {:?}", notices(&handle));
    assert!(report.commit.is_some(), "{report:?}");
    handle.close().unwrap();
}

/// A seal whose pending file is held keeps the file; the pack has its changes, and the next
/// open removes the file without folding anything twice.
#[test]
fn a_seal_whose_pending_removal_is_held_keeps_the_file() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "sealed while the file is held").unwrap();
    let held = pending_files(&rig.fs, &folder);
    assert_eq!(held.len(), 1);
    let abs = folder.join(&held[0]);
    rig.fs.set_busy(&abs, true);
    handle
        .checkpoint(tc_store::CheckpointReason::LeavingProject)
        .wait()
        .unwrap();
    assert!(rig.fs.peek(&abs).is_some(), "the held file stays");
    assert!(!packs(&rig.fs, &folder).is_empty(), "the pack holds it");
    handle.close().unwrap();
    rig.fs.set_busy(&abs, false);

    let handle = open(&rig, &folder);
    assert!(
        pending_files(&rig.fs, &folder).is_empty(),
        "the next open removes it"
    );
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("sealed while the file is held")
    );
    no_false_other_computer(&handle);
    handle.close().unwrap();
}
