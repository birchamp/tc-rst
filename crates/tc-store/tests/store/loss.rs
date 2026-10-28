//! The silent-loss cases of `docs/STORAGE.md` §15.3 that unit S1 owns: a journal file of
//! another installation appearing in the folder (L64), a lost rename (L80), a disk error after the rename (L86), a read-only open
//! (L87), a pack deleted outside the app (L65, the local half), a rename retry (L82) and
//! storage damage (L83).

use crate::common::*;
use std::path::Path;
use tc_journal::fold::Notice;
use tc_store::fault::{FaultFs, OpKind, PowerLoss};
use tc_store::{
    FakePlatform, FolderFacts, Mode, OpenOptions, ReadOnlyReason, Store, StoreConfig, Stored,
    UnsafeFolder,
};

fn notices_of(handle: &tc_store::ProjectHandle) -> Vec<Notice> {
    handle.notices().into_iter().map(|n| n.notice).collect()
}

/// The second-writer guard, the one test the one-instance ruling keeps until the owner
/// answers its open point (`rulings/2026-10-22-one-instance.md`): a journal file this
/// installation did not write appears while the project is open (a sync tool copying another
/// computer's saves into a folder nobody recognised as synced). The project turns read-only
/// with a notice, and both computers' saves are kept.
#[test]
fn an_unknown_journal_file_appearing_turns_the_project_read_only() {
    let (rig, folder) = titus();
    // Another computer saves into its own copy of the project.
    let theirs_fs = rig.fs.copy();
    let other = rig_as(theirs_fs.clone(), "/app-desktop", "host-2");
    let theirs = other
        .store
        .open(
            &folder,
            &OpenOptions {
                other_computer_closed: true,
                ..OpenOptions::default()
            },
        )
        .unwrap();
    save_verse(&theirs, 1, 13, "also kept").unwrap();
    let theirs_device = theirs.facts().device;
    let brought: Vec<(String, Vec<u8>)> = pending_files(&theirs_fs, &folder)
        .into_iter()
        .map(|rel| {
            let bytes = theirs_fs.peek(&folder.join(&rel)).unwrap();
            (rel, bytes)
        })
        .collect();
    assert_eq!(brought.len(), 1, "{brought:?}");
    drop(theirs);
    let mine = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    save_verse(&mine, 1, 12, "kept").unwrap();
    for (rel, bytes) in &brought {
        rig.fs.put(&folder.join(rel), bytes, None);
    }
    mine.checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    match mine.facts().mode {
        Mode::ReadOnly(ReadOnlyReason::SecondWriter { device }) => {
            assert_eq!(device, theirs_device)
        }
        other => panic!("{other:?}"),
    }
    assert!(notices_of(&mine).contains(&Notice::SecondWriter {
        device: theirs_device
    }));
    // Both computers' work is in the fold, and nothing more is written.
    assert_eq!(words(&mine, 1, 12).as_deref(), Some("kept"));
    assert_eq!(words(&mine, 1, 13).as_deref(), Some("also kept"));
    assert!(matches!(
        save_verse(&mine, 1, 14, "x"),
        Err(tc_store::CommandError::ReadOnly(_))
    ));
    drop(mine);
}

#[test]
fn open_cloud_synced_folder_read_only() {
    let (rig, folder) = titus();
    let fs = rig.fs.crash(false);
    let clock = std::sync::Arc::new(tc_store::ManualClock::at(1_792_195_200_000));
    let store = Store::start(StoreConfig {
        app_data: APP.into(),
        projects: PROJECTS.into(),
        vfs: std::sync::Arc::new(fs.clone()),
        platform: std::sync::Arc::new(FakePlatform {
            host: "host-1".into(),
            machine: "machine-of-host-1".into(),
            facts: FolderFacts {
                unsafe_folder: Some(UnsafeFolder::CloudSynced {
                    client: "OneDrive".into(),
                }),
                network: false,
            },
            ..FakePlatform::default()
        }),
        clock,
        threads: 2,
        watch_debounce: std::time::Duration::from_millis(20),
    })
    .unwrap();
    store.wait_cleanup();
    let before = files(&fs, &folder);
    let handle = store.open(&folder, &OpenOptions::default()).unwrap();
    assert_eq!(
        handle.facts().mode,
        Mode::ReadOnly(ReadOnlyReason::UnsafeFolder(UnsafeFolder::CloudSynced {
            client: "OneDrive".into()
        }))
    );
    assert!(handle.snapshot().fold().book_usfm(TIT).is_some());
    handle.close().unwrap();
    assert_eq!(
        files(&fs, &folder),
        before,
        "a read-only open writes nothing"
    );
    // The copy offer: a copy in the projects folder, every pending save included.
    let copy = store.copy_project(&folder, "Titus copy").unwrap();
    assert!(fs.peek(&copy.join("ingredients/tc/project.json")).is_some());
    assert!(fs.peek(&copy.join(".tc/device.json")).is_none());
}

/// The index, in the write log of a save, of the flush of `.tc/pending/`.
fn pending_dir_flush(base: &FaultFs, folder: &Path) -> u64 {
    let fs = base.copy();
    let rig = rig_as(fs.clone(), APP, "host-1");
    let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let start = fs.ops();
    fs.clear_log();
    save_verse(&handle, 1, 15, "probe").unwrap();
    let log = fs.log();
    handle.close().unwrap();
    log.iter()
        .find(|op| op.kind == OpKind::SyncDir && op.path.ends_with(".tc/pending"))
        .map(|op| op.index - start)
        .unwrap()
}

#[test]
fn crash_lost_rename_rolled_forward() {
    let (rig, folder) = titus();
    let base = rig.fs.crash(false);
    let flush = pending_dir_flush(&base, &folder);
    let fs = base.copy();
    fs.set_power_loss(PowerLoss::KeepSyncedNames);
    let rig = rig_as(fs.clone(), APP, "host-1");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    // The process dies before the flush that makes the rename durable.
    fs.kill_before(flush, false);
    let _ = save_verse(&handle, 1, 15, "lost rename");
    drop(handle);
    // The power cut keeps the complete temp file and drops the rename (Windows).
    let after = fs.crash(true);
    let temps: Vec<String> = files(&after, &folder)
        .into_keys()
        .filter(|p| p.starts_with(".tc/pending/.~"))
        .collect();
    assert_eq!(
        temps.len(),
        1,
        "the scenario is a kept temp and a lost rename"
    );
    let rig2 = rig_as(after, APP, "host-1");
    let handle = rig2.store.open(&folder, &OpenOptions::default()).unwrap();
    assert_eq!(words(&handle, 1, 15).as_deref(), Some("lost rename"));
    handle.close().unwrap();
}

#[test]
fn save_disk_error_after_rename_counts_as_saved() {
    let (rig, folder) = titus();
    let base = rig.fs.crash(false);
    let flush = pending_dir_flush(&base, &folder);
    let fs = base.copy();
    let rig = rig_as(fs.clone(), APP, "host-1");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let events = handle.subscribe();
    fs.fail_op(flush, std::io::ErrorKind::Other);
    let reply = save_verse(&handle, 1, 15, "saved anyway").unwrap();
    assert!(
        matches!(reply.stored, Stored::WrittenNotDurable(_)),
        "{:?}",
        reply.stored
    );
    assert!(
        notices_of(&handle)
            .iter()
            .any(|n| matches!(n, Notice::SavedWithDiskError { .. }))
    );
    assert!(events.try_iter().any(|e| matches!(
        e,
        tc_store::StoreEvent::Notice(n) if matches!(n.notice, Notice::SavedWithDiskError { .. })
    )));
    // The next save takes the next seq: no duplicate device alarm.
    save_verse(&handle, 1, 16, "next").unwrap();
    handle.close().unwrap();
    let rig2 = rig_as(fs.crash(false), APP, "host-1");
    let handle = rig2.store.open(&folder, &OpenOptions::default()).unwrap();
    assert!(
        !handle
            .snapshot()
            .fold()
            .anomalies()
            .any(|a| matches!(a, tc_journal::fold::Anomaly::DuplicateDevice { .. }))
    );
    assert_eq!(words(&handle, 1, 15).as_deref(), Some("saved anyway"));
    handle.close().unwrap();
}

/// A pack deleted outside the app that no copy in the object database can restore (the
/// project's `.git` went with it): nothing derived from it is removed (§4.4). With a copy, the
/// pack is restored (`checkpoint::loss_pack_deleted_outside_app_restored`).
#[test]
fn loss_pack_deleted_with_no_copy_removes_nothing_derived() {
    let (rig, folder) = titus();
    let pack = packs(&rig.fs, &folder).into_iter().next().unwrap();
    rig.fs.delete(&folder.join(&pack)).unwrap();
    let outside = tc_store::Vfs::outside_dir(&rig.fs, &folder).unwrap();
    std::fs::remove_dir_all(outside.join(".git")).unwrap();
    let before_usfm = rig.fs.peek(&folder.join("ingredients/TIT.usfm")).unwrap();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert!(
        notices_of(&handle)
            .iter()
            .any(|n| matches!(n, Notice::PackMissing { .. }))
    );
    assert_eq!(
        rig.fs.peek(&folder.join("ingredients/TIT.usfm")),
        Some(before_usfm),
        "a book whose changes are missing is not removed"
    );
    handle.close().unwrap();
}

/// L82. A rename that another program refuses leaves the path dirty and tried again by the
/// actor's timer, and every try repeats the compare-and-swap: a program that saved the file
/// meanwhile is never written over. Its bytes are kept in a copy and read as an edit.
#[test]
fn loss_rename_retry_never_overwrites_outside_save() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let usfm = folder.join("ingredients/TIT.usfm");
    rig.fs.set_busy(&usfm, true);
    save_verse(&handle, 1, 1, "a new first verse").unwrap();
    handle.leave(tc_store::Leave::Book(TIT)).wait().unwrap();
    // The writer's job is over, and so is the look at the folder that follows it (the rig's):
    // the outside save below belongs to the wait that follows, not to the state that look sees.
    handle.wait_idle();
    // Paratext saves the file while the app waits to write it, then lets go of it.
    let outside = b"\\id TIT saved by Paratext\n".to_vec();
    rig.fs.put(&usfm, &outside, None);
    rig.fs.set_busy(&usfm, false);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let copy = loop {
        if let Some(copy) = copies_of(&handle, "ingredients/TIT.usfm").pop() {
            break copy;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the retry never found the outside save"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    assert_eq!(
        rig.fs.peek(&folder.join(&copy)),
        Some(outside.clone()),
        "the outside save is kept whole"
    );
    handle.wait_idle();
    // The book waits for the person's answer, and is not written over meanwhile.
    assert_eq!(handle.waiting_books().len(), 1);
    assert_eq!(rig.fs.peek(&usfm), Some(outside.clone()));
    handle.not_now("ingredients/TIT.usfm").wait().unwrap();
    handle.wait_idle();
    let written = String::from_utf8(rig.fs.peek(&usfm).unwrap()).unwrap();
    assert!(
        written.contains("a new first verse"),
        "the project's save is written after the copy: {written}"
    );
    assert_eq!(handle.kept_copies().len(), 1, "the copy stays listed");
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// L65. A pack a cloud client holds back (a placeholder whose stat and read both fail) is
/// neither damaged nor gone: the open reports it and goes on, nothing is removed or
/// rewritten because of it, and when it can be read again the project is whole.
#[test]
fn loss_pack_cloud_placeholder_unreadable_not_damaged() {
    let (rig, folder) = titus();
    let pack = packs(&rig.fs, &folder).into_iter().next().unwrap();
    let abs = folder.join(&pack);
    let bytes = rig.fs.peek(&abs).unwrap();
    let before_usfm = rig.fs.peek(&folder.join("ingredients/TIT.usfm")).unwrap();
    rig.fs.set_unreadable(&abs, true);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert!(
        notices_of(&handle)
            .iter()
            .any(|n| matches!(n, Notice::FileUnreadable { path, .. } if **path == *pack)),
        "{:?}",
        notices_of(&handle)
    );
    assert!(
        !notices_of(&handle).iter().any(|n| matches!(
            n,
            Notice::QuarantinedPack { .. } | Notice::PackMissing { .. }
        )),
        "never damaged, never gone"
    );
    handle.close().unwrap();
    assert_eq!(rig.fs.peek(&abs), Some(bytes), "the pack is untouched");
    assert_eq!(
        rig.fs.peek(&folder.join("ingredients/TIT.usfm")),
        Some(before_usfm),
        "nothing derived from it is removed or rewritten"
    );
    rig.fs.set_unreadable(&abs, false);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert!(words(&handle, 1, 1).is_some(), "whole again");
    assert!(
        !notices_of(&handle)
            .iter()
            .any(|n| matches!(n, Notice::FileUnreadable { .. }))
    );
    handle.close().unwrap();
}

#[test]
fn loss_bit_rot_not_recorded_as_edit() {
    let (rig, folder) = titus();
    // Settle the entries: an open that begins 2 s after the files were written.
    rig.fs.advance(std::time::Duration::from_secs(5));
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle.close().unwrap();
    let usfm = folder.join("ingredients/TIT.usfm");
    let mut bytes = rig.fs.peek(&usfm).unwrap();
    let mtime = tc_store::Vfs::metadata(&rig.fs, &usfm).unwrap().mtime_ns;
    let at = bytes.len() / 2;
    bytes[at] ^= 0x20;
    rig.fs.put(&usfm, &bytes, Some(mtime));
    let changes_before = {
        let h = rig.store.open(&folder, &OpenOptions::default()).unwrap();
        h.wait_idle();
        let n = h.snapshot().fold().len();
        let notices = notices_of(&h);
        // Kept and asked about like any change (delta §2.5: R10's own hold goes); nothing
        // enters the project without *Import*.
        assert!(
            notices
                .iter()
                .any(|n| matches!(n, Notice::OutsideEditHeld { .. })),
            "{notices:?}"
        );
        assert_eq!(h.waiting_books().len(), 1);
        h.close().unwrap();
        n
    };
    assert_eq!(rig.fs.peek(&usfm), Some(bytes), "the file is left as it is");
    let h = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    assert_eq!(
        h.snapshot().fold().len(),
        changes_before,
        "nothing is recorded"
    );
    h.close().unwrap();
}

/// The copy offer is for a folder another writer may be using: another computer whose
/// saves a cloud client brings in, or a second installation (§5.0: "the copy keeps every
/// pending save"). That writer's seal between the copy's pending files and its packs moves
/// the saves from the one to the other; the copy has them either way.
#[test]
fn a_copy_made_while_the_other_writer_seals_keeps_every_save() {
    let hooks = std::sync::Arc::new(tc_store::Hooks::default());
    let rig = rig_with_hooks(FaultFs::new(), hooks.clone());
    rig.store.wait_cleanup();
    let folder = create_titus(&rig, "Titus");
    let writer = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    for v in 1..=15 {
        save_verse(&writer, 1, v, &format!("saved {v}")).unwrap();
    }
    writer.wait_idle();
    assert_eq!(pending_files(&rig.fs, &folder).len(), 15);
    hooks.hold(tc_store::Point::CopyBetweenJournalFolders);
    let copy = std::thread::scope(|scope| {
        let copying = scope.spawn(|| rig.store.copy_project(&folder, "Titus copy"));
        hooks.wait_reached(tc_store::Point::CopyBetweenJournalFolders, 1);
        writer
            .checkpoint(tc_store::CheckpointReason::Periodic)
            .wait()
            .unwrap();
        assert!(pending_files(&rig.fs, &folder).is_empty(), "the seal ran");
        hooks.release(tc_store::Point::CopyBetweenJournalFolders);
        copying.join().unwrap().unwrap()
    });
    writer.close().unwrap();
    let handle = rig.store.open(&copy, &OpenOptions::default()).unwrap();
    for v in 1..=15 {
        assert_eq!(
            words(&handle, 1, v).as_deref(),
            Some(format!("saved {v}").as_str()),
            "the copy lost the save of 1:{v}"
        );
    }
    handle.close().unwrap();
}

/// The device fingerprint is of the folder's canonical path (§6.3). A create fingerprints its
/// project before the folder exists, so it must take the same canonical path every later
/// open takes: through a symlinked projects folder (and, on Windows, the `\\?\` prefix of
/// every canonical path) the first open would otherwise mint a second device and tell the
/// person the folder was copied.
#[test]
fn a_project_created_through_a_symlinked_projects_folder_keeps_its_device() {
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    std::fs::create_dir(&real).unwrap();
    #[cfg(unix)]
    let projects = {
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        link
    };
    #[cfg(not(unix))]
    let projects = real.clone();
    let store = Store::start(StoreConfig {
        app_data: dir.path().join("app"),
        projects,
        vfs: Arc::new(tc_store::StdFs),
        platform: Arc::new(FakePlatform::default()),
        clock: Arc::new(tc_store::ManualClock::at(1_792_195_200_000)),
        threads: 2,
        watch_debounce: std::time::Duration::from_millis(20),
    })
    .unwrap();
    store.wait_cleanup();
    watch_real_burrito(&store);
    let folder = store
        .create(&tc_store::NewProject {
            name: "Titus".into(),
            kind: tc_journal::fold::ProjectKind::Bible,
            records: first_records(),
            books: vec![(
                TIT,
                tc_journal::record::Scope::Whole,
                tc_journal::command::SeedSource { usfm: ult("TIT") },
            )],
        })
        .unwrap();
    let device_json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(folder.join(".tc/device.json")).unwrap()).unwrap();
    let handle = store.open(&folder, &OpenOptions::default()).unwrap();
    assert_eq!(
        handle.facts().device.to_string(),
        device_json["device"].as_str().unwrap(),
        "the first open kept the device the create made"
    );
    assert!(
        !notices_of(&handle)
            .iter()
            .any(|n| matches!(n, Notice::DeviceRotated { .. })),
        "{:?}",
        notices_of(&handle)
    );
    handle.close().unwrap();
    assert!(store.hooks().burrito_problems().is_empty());
}

fn open_ok(rig: &Rig, folder: &Path) -> tc_store::ProjectHandle {
    let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle
}

/// L16 (F7): an older backup of the folder is put back over newer work. This computer's ledger
/// says it saved more than the folder holds, so the open starts a new device and says how
/// many changes the copy is missing; the backup's own saves stay, and a new save never
/// reuses a number the lost work used.
#[test]
fn loss_f7_restored_backup_warns() {
    let (rig, folder) = titus();
    let handle = open_ok(&rig, &folder);
    save_verse(&handle, 1, 1, "in the backup").unwrap();
    handle.close().unwrap();
    let backup = rig.fs.copy();
    let handle = open_ok(&rig, &folder);
    let device = handle.facts().device;
    save_verse(&handle, 1, 2, "after the backup").unwrap();
    save_verse(&handle, 1, 3, "after the backup too").unwrap();
    handle.close().unwrap();
    // The folder goes back to the backup; the application data stays this computer's.
    let restored = backup.copy();
    for (path, bytes) in rig.fs.files_under(Path::new(APP)) {
        restored.put(&path, &bytes, None);
    }
    drop(rig);
    let rig = rig_as(restored, APP, "host-1");
    let handle = open_ok(&rig, &folder);
    assert_ne!(handle.facts().device, device, "a new device");
    assert!(
        notices_of(&handle)
            .iter()
            .any(|n| matches!(n, Notice::RestoredBackup { missing: 2, .. })),
        "{:?}",
        notices_of(&handle)
    );
    assert_eq!(words(&handle, 1, 1).as_deref(), Some("in the backup"));
    save_verse(&handle, 1, 4, "after the restore").unwrap();
    assert!(
        !handle
            .snapshot()
            .fold()
            .anomalies()
            .any(|a| matches!(a, tc_journal::fold::Anomaly::DuplicateDevice { .. })),
        "the new save collides with nothing"
    );
    handle.close().unwrap();
}

/// L17 (F8): the installation secret is lost (`installation.json` deleted or damaged).
/// Saves not yet sealed are files in the project folder, so they are all there at the next
/// open, and new saves work.
#[test]
fn loss_f8_secret_lost_pending_survives() {
    let (rig, folder) = titus();
    let handle = open_ok(&rig, &folder);
    save_verse(&handle, 1, 1, "saved before the secret was lost").unwrap();
    save_verse(&handle, 1, 2, "this one too").unwrap();
    let device = handle.facts().device;
    let fs = rig.fs.crash(false);
    drop(handle);
    drop(rig);
    assert!(!pending_files(&fs, &folder).is_empty(), "unsealed saves");
    fs.delete(&Path::new(APP).join("installation.json"))
        .unwrap();
    let rig = rig_as(fs, APP, "host-1");
    let handle = open_ok(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before the secret was lost")
    );
    assert_eq!(words(&handle, 1, 2).as_deref(), Some("this one too"));
    assert_ne!(handle.facts().device, device, "only a new device");
    assert_eq!(handle.facts().mode, Mode::Writable);
    save_verse(&handle, 1, 3, "after").unwrap();
    handle.close().unwrap();
}

/// L19 (F10): no content can lock the project. Every file the open reads is damaged at once
/// (the journal's files, the project and device files, the format, the lock, the derived
/// files); the open still opens it for writing, keeps what can be read, and saves work.
#[test]
fn loss_f10_nothing_can_lock_open() {
    let (rig, folder) = titus();
    let handle = open_ok(&rig, &folder);
    save_verse(&handle, 1, 1, "sealed into a pack").unwrap();
    handle
        .checkpoint(tc_store::CheckpointReason::BeforeSync)
        .wait()
        .unwrap();
    save_verse(&handle, 1, 2, "still pending").unwrap();
    let fs = rig.fs.crash(false);
    drop(handle);
    drop(rig);
    let garbage = b"\x00\xffnot what this file should hold\n{";
    let mut damaged = vec![
        ".tc/device.json".to_owned(),
        ".tc/lock".to_owned(),
        "ingredients/tc/format.json".to_owned(),
        "ingredients/tc/project.json".to_owned(),
        "metadata.json".to_owned(),
    ];
    let pending = pending_files(&fs, &folder);
    assert!(!pending.is_empty(), "a save not sealed yet");
    damaged.extend(pending);
    damaged.extend(packs(&fs, &folder));
    for rel in &damaged {
        fs.put(&folder.join(rel), garbage, None);
    }
    let rig = rig_as(fs, APP, "host-1");
    damaged_on_purpose(&rig);
    let handle = rig
        .store
        .open(&folder, &OpenOptions::default())
        .unwrap_or_else(|e| panic!("damaged content refused the open: {e}"));
    handle.wait_idle();
    assert_eq!(handle.facts().mode, Mode::Writable);
    save_verse(&handle, 1, 3, "a save after the damage").unwrap();
    assert_eq!(
        words(&handle, 1, 3).as_deref(),
        Some("a save after the damage")
    );
    handle.close().unwrap();
    let handle = open_ok(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 3).as_deref(),
        Some("a save after the damage"),
        "and the next open too"
    );
    handle.close().unwrap();
}

/// L25 (F6, RISKS 10): a project of a newer major format is refused in words before the open
/// writes anything into it.
#[test]
fn loss_newer_major_refused_before_write() {
    let (rig, folder) = titus();
    let newer = std::fs::read(repo_root().join("fixtures/journal/format-2.0.json")).unwrap();
    rig.fs
        .put(&folder.join("ingredients/tc/format.json"), &newer, None);
    let before = files(&rig.fs, &folder);
    let outside = tc_store::Vfs::outside_dir(&rig.fs, &folder).unwrap();
    let git_before = dir_bytes(&outside);
    match rig.store.open(&folder, &OpenOptions::default()) {
        Err(tc_store::OpenError::NewerMajor { major }) => assert_eq!(major, "2"),
        Err(other) => panic!("{other}"),
        Ok(_) => panic!("a project of format 2 opened"),
    }
    assert_eq!(files(&rig.fs, &folder), before, "nothing written");
    assert_eq!(dir_bytes(&outside), git_before, "nothing written to git");
}

fn dir_bytes(dir: &Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut out = std::collections::BTreeMap::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(at) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&at) else {
            continue;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.insert(path.clone(), std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

/// Reads every pack with its last byte changed while `garble` is on: a disk that does not
/// give back what was written.
#[derive(Debug, Clone)]
struct GarblesPacks {
    fs: FaultFs,
    garble: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl tc_store::Vfs for GarblesPacks {
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        let mut bytes = self.fs.read(path)?;
        if self.garble.load(std::sync::atomic::Ordering::SeqCst)
            && slashed(path).contains("/ingredients/tc/changes/")
            && let Some(last) = bytes.last_mut()
        {
            *last ^= 0x55;
        }
        Ok(bytes)
    }
    fn open(&self, path: &Path) -> std::io::Result<Box<dyn tc_store::vfs::ReadHandle>> {
        self.fs.open(path)
    }
    fn metadata(&self, path: &Path) -> std::io::Result<tc_store::vfs::FileMeta> {
        self.fs.metadata(path)
    }
    fn read_dir(&self, path: &Path) -> std::io::Result<Vec<tc_store::vfs::DirEntry>> {
        self.fs.read_dir(path)
    }
    fn create_new(&self, path: &Path) -> std::io::Result<Box<dyn tc_store::vfs::WriteHandle>> {
        self.fs.create_new(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        self.fs.rename(from, to)
    }
    fn rename_file_noreplace(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        self.fs.rename_file_noreplace(from, to)
    }
    fn rename_dir_noreplace(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        self.fs.rename_dir_noreplace(from, to)
    }
    fn remove_file(&self, path: &Path) -> std::io::Result<()> {
        self.fs.remove_file(path)
    }
    fn remove_dir_all(&self, path: &Path) -> std::io::Result<()> {
        self.fs.remove_dir_all(path)
    }
    fn create_dir(&self, path: &Path) -> std::io::Result<()> {
        self.fs.create_dir(path)
    }
    fn sync_dir(&self, path: &Path) -> std::io::Result<()> {
        self.fs.sync_dir(path)
    }
    fn sync_file(&self, path: &Path) -> std::io::Result<()> {
        self.fs.sync_file(path)
    }
    fn lock(&self, path: &Path) -> std::io::Result<Option<Box<dyn tc_store::vfs::LockHandle>>> {
        self.fs.lock(path)
    }
    fn canonicalize(&self, path: &Path) -> std::io::Result<std::path::PathBuf> {
        self.fs.canonicalize(path)
    }
    fn secure_dir_chain(&self, path: &Path, made: &[std::path::PathBuf]) -> std::io::Result<()> {
        self.fs.secure_dir_chain(path, made)
    }
    fn outside_dir(&self, path: &Path) -> std::io::Result<std::path::PathBuf> {
        self.fs.outside_dir(path)
    }
    fn is_held(&self, error: &std::io::Error) -> bool {
        self.fs.is_held(error)
    }
    fn watch(
        &self,
        root: &Path,
        recursive: bool,
        sink: tc_store::vfs::WatchSink,
    ) -> std::io::Result<Option<tc_store::vfs::WatchGuard>> {
        self.fs.watch(root, recursive, sink)
    }
    fn wait_held(&self, delay: std::time::Duration) {
        self.fs.wait_held(delay);
    }
}

/// L47, crash rows K2 and K3: a seal reads its new pack back and verifies it before it
/// removes a pending file. A pack that does not read back as written removes nothing, and
/// every save stays.
#[test]
fn crash_seal_verifies_before_delete() {
    let fs = FaultFs::new();
    let garble = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (mut cfg, _) = config(&fs, APP, "host-1");
    cfg.vfs = std::sync::Arc::new(GarblesPacks {
        fs: fs.clone(),
        garble: garble.clone(),
    });
    let store = Store::start(cfg).unwrap();
    store.wait_cleanup();
    let rig = Rig { fs, store };
    damaged_on_purpose(&rig);
    let folder = create_titus(&rig, "Titus");
    let handle = open_ok(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    save_verse(&handle, 1, 2, "in hope of eternal life").unwrap();
    let pending = pending_files(&rig.fs, &folder);
    assert!(!pending.is_empty());
    let packs_before = packs(&rig.fs, &folder);
    garble.store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = handle
        .checkpoint(tc_store::CheckpointReason::BeforeSync)
        .wait();
    garble.store(false, std::sync::atomic::Ordering::SeqCst);
    for rel in &pending {
        assert!(
            rig.fs.peek(&folder.join(rel)).is_some(),
            "{rel} was removed though its pack did not verify"
        );
    }
    assert_eq!(
        packs(&rig.fs, &folder),
        packs_before,
        "no unverified pack is left"
    );
    let fs = rig.fs.crash(false);
    drop(handle);
    drop(rig);
    let rig = rig_as(fs, APP, "host-1");
    let handle = open_ok(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("Paul, a servant of God")
    );
    assert_eq!(
        words(&handle, 1, 2).as_deref(),
        Some("in hope of eternal life")
    );
    handle.close().unwrap();
}

/// L66 (§4.4 "All copies damaged", §9.2 N3): no valid copy of a pack is left. The copy in the
/// folder and the one the last commit holds are damaged in different lines; the open keeps
/// the union of their readable lines, so every save is still there, and seals them again.
#[test]
fn loss_all_copies_damaged_union_salvaged() {
    let (rig, folder) = titus();
    let handle = open_ok(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    save_verse(&handle, 1, 2, "in hope of eternal life").unwrap();
    save_verse(&handle, 1, 3, "at the right time").unwrap();
    handle
        .checkpoint(tc_store::CheckpointReason::BeforeSync)
        .wait()
        .unwrap();
    // The saves themselves, by id: a verse that came back only as an outside edit of the
    // book file (a new change) would not be its save.
    let saves = |handle: &tc_store::ProjectHandle| -> Vec<Vec<tc_journal::ChangeId>> {
        (1..=3)
            .map(|v| handle.snapshot().fold().heads(&verse(1, v)))
            .collect()
    };
    let saved = saves(&handle);
    handle.close().unwrap();
    let rel = packs(&rig.fs, &folder)
        .into_iter()
        .find(|rel| {
            String::from_utf8_lossy(&rig.fs.peek(&folder.join(rel)).unwrap())
                .contains("servant of God")
        })
        .expect("the pack of the saves");
    let good = rig.fs.peek(&folder.join(&rel)).unwrap();
    let damage = |from: &str, to: &str| -> Vec<u8> {
        let text = String::from_utf8(good.clone()).unwrap();
        assert!(text.contains(from));
        text.replacen(from, to, 1).into_bytes()
    };
    let in_the_commit = damage("servant of God", "servant of Gxd");
    let in_the_folder = damage("right time", "right tome");
    // The commit's copy is damaged too, and no other copy of the good bytes is left.
    let outside = tc_store::Vfs::outside_dir(&rig.fs, &folder).unwrap();
    let repo = git2::Repository::open(outside.join(".git")).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let blob = repo.blob(&in_the_commit).unwrap();
    let mut update = git2::build::TreeUpdateBuilder::new();
    update.upsert(&rel, blob, git2::FileMode::Blob);
    let tree = repo
        .find_tree(update.create_updated(&repo, &head.tree().unwrap()).unwrap())
        .unwrap();
    let sig = git2::Signature::now("disk", "disk@example.org").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "a damaged copy", &tree, &[&head])
        .unwrap();
    let good_oid = git2::Oid::hash_object(git2::ObjectType::Blob, &good)
        .unwrap()
        .to_string();
    let loose = outside
        .join(".git/objects")
        .join(&good_oid[..2])
        .join(&good_oid[2..]);
    std::fs::remove_file(&loose).unwrap();
    assert!(
        !repo
            .odb()
            .unwrap()
            .exists(git2::Oid::from_str(&good_oid).unwrap())
    );
    rig.fs.put_unseen(&folder.join(&rel), &in_the_folder, None);
    let handle = open_ok(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("Paul, a servant of God"),
        "the line only the folder's copy still reads"
    );
    assert_eq!(
        words(&handle, 1, 3).as_deref(),
        Some("at the right time"),
        "the line only the commit's copy still reads"
    );
    assert_eq!(
        words(&handle, 1, 2).as_deref(),
        Some("in hope of eternal life")
    );
    assert_eq!(saves(&handle), saved, "each verse is its own save");
    handle
        .checkpoint(tc_store::CheckpointReason::BeforeSync)
        .wait()
        .unwrap();
    handle.close().unwrap();
    let handle = open_ok(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("Paul, a servant of God")
    );
    assert_eq!(words(&handle, 1, 3).as_deref(), Some("at the right time"));
    assert_eq!(saves(&handle), saved);
    handle.close().unwrap();
}

/// A copy whose original device file cannot be removed would write the same (device, seq)
/// pairs as the original: the copy fails, its staging folder goes, and nothing is placed.
#[test]
fn a_copy_that_cannot_drop_the_device_file_fails_and_leaves_nothing() {
    let (rig, folder) = titus();
    rig.store
        .open(&folder, &OpenOptions::default())
        .unwrap()
        .close()
        .unwrap();
    assert!(rig.fs.peek(&folder.join(".tc/device.json")).is_some());
    let projects = folder.parent().unwrap().to_owned();
    // The same copy on a copy of the files tells which write point removes the device file.
    let probe = rig.fs.copy();
    let probe_rig = rig_as(probe.clone(), APP, "host-1");
    probe.clear_log();
    let start = probe.ops();
    probe_rig.store.copy_project(&folder, "Titus copy").unwrap();
    let removal = probe
        .log()
        .into_iter()
        .find(|op| {
            op.kind == OpKind::RemoveFile
                && op.path.ends_with(".tc/device.json")
                && op.path.to_string_lossy().contains(".staging-")
        })
        .expect("the copy removes its device file");
    let k = removal.index - start;
    drop(probe_rig);

    let before = rig.fs.files_under(&projects);
    rig.fs.fail_op(k, std::io::ErrorKind::PermissionDenied);
    let copied = rig.store.copy_project(&folder, "Titus copy");
    assert!(copied.is_err(), "{copied:?}");
    assert_eq!(
        rig.fs.files_under(&projects),
        before,
        "no copy and no staging folder is left"
    );
}
