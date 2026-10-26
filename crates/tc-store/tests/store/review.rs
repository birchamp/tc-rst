//! The defects the independent review of unit S1 found, each as a test that fails without
//! its fix: files held or edited outside the app (`docs/STORAGE.md` §5.10, §6.2), an open
//! that never refuses because of content (§6.1), a derived file another program holds, the
//! order of a close (§5.0), and a draft whose token no longer verifies (§5.14, R20).

use crate::common::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tc_journal::command::{Command, Draft, DraftField, PlannedChange, SectionValue, UpgradePlan};
use tc_journal::fold::{Anomaly, Notice, RotationReason};
use tc_journal::record::{CarriedValue, VerseValue};
use tc_journal::{Cause, GitOid, Md5, Record, RecordKey, RepoPath};
use tc_store::fault::{FaultFs, OpKind};
use tc_store::{
    Hooks, Leave, Mode, OpenOptions, Point, ProjectHandle, ReadOnlyReason, StoreNotice,
};

const AUDIO: &str = "ingredients/audio/a.mp3";

fn open(rig: &Rig, folder: &Path) -> ProjectHandle {
    let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle
}

fn notices(handle: &ProjectHandle) -> Vec<StoreNotice> {
    handle.notices()
}

/// The notices that say a file is held, with the path they name.
fn hold_notices(handle: &ProjectHandle) -> Vec<(String, String)> {
    notices(handle)
        .into_iter()
        .filter_map(|n| match n.notice {
            Notice::OutsideEditHeld { path, copy } | Notice::StorageDamage { path, copy } => {
                Some((path.as_str().to_owned(), copy.to_string()))
            }
            Notice::MetadataEditedOutside { copy } => {
                Some(("metadata.json".to_owned(), copy.to_string()))
            }
            _ => None,
        })
        .collect()
}

fn leave(handle: &ProjectHandle) {
    handle.leave(Leave::Book(TIT)).wait().unwrap();
}

/// Saves 1:1 and writes the derived files, then closes: the usual start of a test of a file
/// changed outside the app.
fn saved_and_closed(rig: &Rig, folder: &Path) {
    let handle = open(rig, folder);
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    leave(&handle);
    handle.close().unwrap();
}

fn usfm_path(folder: &Path) -> PathBuf {
    folder.join("ingredients/TIT.usfm")
}

/// Records a carried file the way an import would: its bytes are in the folder, a record
/// says so. Its identity names a real backup, so removal proves preservation of its History.
/// The bytes are put unseen: told of them, the watcher records the file itself (N11) and the
/// plan here is stale.
fn record_carried(rig: &Rig, folder: &Path, handle: &ProjectHandle, bytes: &[u8]) {
    let repo = repo_of(&rig.fs, folder);
    let oid = GitOid::parse(&repo.blob(bytes).unwrap().to_string()).unwrap();
    handle
        .submit(Command::ApplyUpgrade {
            plan: UpgradePlan {
                changes: vec![PlannedChange {
                    record: Record::CarriedFile {
                        path: RepoPath::parse(AUDIO).unwrap(),
                        value: CarriedValue::Present {
                            oid,
                            size: bytes.len() as u64,
                            md5: Md5::of(bytes),
                        },
                    },
                    parents: Vec::new(),
                    cause: Cause::Seed,
                }],
            },
        })
        .wait()
        .unwrap();
}

// ---------------------------------------------------------------------------------------
// §6.2: a held file that is deleted is rebuilt
// ---------------------------------------------------------------------------------------

/// "A deletion ends it too, and the file is rebuilt": the file was held, then deleted while
/// the project was closed.
#[test]
fn a_held_file_deleted_while_the_project_is_closed_is_rebuilt() {
    let (rig, folder) = titus();
    saved_and_closed(&rig, &folder);
    settle_entries(&rig, &folder);
    let usfm = usfm_path(&folder);
    let damaged = damage_file(&rig, &folder, "ingredients/TIT.usfm");
    let handle = open(&rig, &folder);
    assert!(
        hold_notices(&handle)
            .iter()
            .any(|(path, _)| path == "ingredients/TIT.usfm"),
        "held: {:?}",
        notices(&handle)
    );
    handle.close().unwrap();
    rig.fs.delete(&usfm).unwrap();

    let handle = open(&rig, &folder);
    assert!(
        notices(&handle)
            .iter()
            .any(|n| matches!(&n.notice, Notice::MissingFileRebuilt { path } if path.as_str() == "ingredients/TIT.usfm")),
        "{:?}",
        notices(&handle)
    );
    assert!(hold_notices(&handle).is_empty(), "the hold ended");
    save_verse(&handle, 1, 2, "to Titus").unwrap();
    leave(&handle);
    handle.close().unwrap();
    let rebuilt = rig.fs.peek(&usfm).expect("TIT.usfm is back");
    assert_ne!(rebuilt, damaged);
    let rebuilt = String::from_utf8(rebuilt).unwrap();
    assert!(rebuilt.contains("Paul, a servant"), "{rebuilt}");
    assert!(rebuilt.contains("to Titus"));
    assert_valid_burrito(&rig.fs, &folder);
}

/// The same while the project is open: `metadata.json` never lists a file that is not
/// there, and the next write brings the file back.
#[test]
fn a_held_file_deleted_while_the_project_is_open_is_rebuilt() {
    let (rig, folder) = titus();
    saved_and_closed(&rig, &folder);
    settle_entries(&rig, &folder);
    let handle = open(&rig, &folder);
    let usfm = usfm_path(&folder);
    damage_file(&rig, &folder, "ingredients/TIT.usfm");
    save_verse(&handle, 1, 2, "to Titus").unwrap();
    leave(&handle);
    assert!(
        !hold_notices(&handle).is_empty(),
        "held by the compare-and-swap"
    );
    assert_valid_burrito(&rig.fs, &folder);
    // The writer looks at the folder after its last job; on a slow run that look would come
    // after the deletion below and blame the store for the file the test took away.
    handle.wait_idle();
    rig.fs.delete(&usfm).unwrap();
    save_verse(&handle, 1, 3, "grace and peace").unwrap();
    leave(&handle);
    assert_valid_burrito(&rig.fs, &folder);
    let rebuilt = String::from_utf8(rig.fs.peek(&usfm).expect("TIT.usfm is back")).unwrap();
    assert!(rebuilt.contains("to Titus") && rebuilt.contains("grace and peace"));
    assert!(
        hold_notices(&handle).is_empty(),
        "the hold ended with the deletion"
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

// ---------------------------------------------------------------------------------------
// §5.16: carried files
// ---------------------------------------------------------------------------------------

/// "The next materialization removes the file (compare-and-swap first)": the file leaves the
/// folder, and the folder is still a valid burrito.
#[test]
fn a_removed_carried_file_leaves_the_folder_and_the_burrito_stays_valid() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let bytes = b"audio bytes".to_vec();
    rig.fs.put_unseen(&folder.join(AUDIO), &bytes, None);
    record_carried(&rig, &folder, &handle, &bytes);
    leave(&handle);
    assert_valid_burrito(&rig.fs, &folder);
    assert_eq!(rig.fs.peek(&folder.join(AUDIO)), Some(bytes));

    let key = RecordKey::CarriedFile(RepoPath::parse(AUDIO).unwrap());
    handle
        .submit(Command::RemoveCarriedFile {
            shown: handle.mint(&key, None).unwrap(),
        })
        .wait()
        .unwrap();
    leave(&handle);
    assert_eq!(
        rig.fs.peek(&folder.join(AUDIO)),
        None,
        "the file the command removed is gone"
    );
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    let handle = open(&rig, &folder);
    assert_eq!(rig.fs.peek(&folder.join(AUDIO)), None);
    handle.close().unwrap();
}

/// A removal never takes a file the app did not write: a carried file edited outside the
/// app and then removed by the command is held, not deleted.
#[test]
fn a_removed_carried_file_that_was_edited_outside_is_not_deleted() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let bytes = b"audio bytes".to_vec();
    rig.fs.put_unseen(&folder.join(AUDIO), &bytes, None);
    record_carried(&rig, &folder, &handle, &bytes);
    leave(&handle);
    // The writer's jobs are over, and so is the rig's look at the folder after each: the edit
    // below is the next look's to find, not a change under a job that listed the bytes before.
    handle.wait_idle();
    rig.fs
        .put(&folder.join(AUDIO), b"audio bytes, edited", None);
    let key = RecordKey::CarriedFile(RepoPath::parse(AUDIO).unwrap());
    handle
        .submit(Command::RemoveCarriedFile {
            shown: handle.mint(&key, None).unwrap(),
        })
        .wait()
        .unwrap();
    leave(&handle);
    assert_eq!(
        rig.fs.peek(&folder.join(AUDIO)),
        Some(b"audio bytes, edited".to_vec()),
        "the person's bytes stay"
    );
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
}

/// I7 and §11.4: a carried file rewritten outside the app is listed as it is on disk, and the
/// new bytes are recorded as a new version of the file, stored whole as a blob; nothing is
/// written over them.
#[test]
fn a_carried_file_edited_outside_is_recorded_and_listed_as_it_is() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let bytes = b"mp3".to_vec();
    rig.fs.put_unseen(&folder.join(AUDIO), &bytes, None);
    record_carried(&rig, &folder, &handle, &bytes);
    leave(&handle);
    assert_valid_burrito(&rig.fs, &folder);

    let edited = b"sixteen bytes!!!".to_vec();
    // The writer looks at the folder after its last job; a look still running when the
    // file changes below would blame the store for another program's bytes.
    handle.wait_idle();
    // Unseen, as above: the focus below is the look that finds it.
    rig.fs.put_unseen(&folder.join(AUDIO), &edited, None);
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    leave(&handle);
    assert_valid_burrito(&rig.fs, &folder);
    handle.focus().wait().unwrap();
    handle.wait_idle();
    assert_valid_burrito(&rig.fs, &folder);
    assert!(
        notices(&handle).iter().any(|n| matches!(
            &n.notice,
            Notice::OutsideEditRecorded { path, .. } if path.as_str() == AUDIO
        )),
        "{:?}",
        notices(&handle)
    );
    assert_eq!(rig.fs.peek(&folder.join(AUDIO)), Some(edited.clone()));
    let repo = repo_of(&rig.fs, &folder);
    let blob = git2::Oid::hash_object(git2::ObjectType::Blob, &edited).unwrap();
    assert!(
        repo.odb().unwrap().exists(blob),
        "the bytes are stored whole"
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// §3.5: `project.json` is never recorded. A copy of the edited file is kept, the file is
/// written again as the app had it, and the person is told; the burrito lists what is there.
#[test]
fn a_fixed_file_edited_outside_is_restored_and_a_copy_is_kept() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    leave(&handle);
    let rel = "ingredients/tc/project.json";
    let path = folder.join(rel);
    let original = rig.fs.peek(&path).unwrap();
    let mut spaced = original.clone();
    spaced.extend_from_slice(b"\n\n");
    rig.fs.put(&path, &spaced, None);
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    leave(&handle);
    assert_valid_burrito(&rig.fs, &folder);
    assert_eq!(rig.fs.peek(&path), Some(original), "written again");
    let copies = copies_of(&handle, rel);
    assert_eq!(copies.len(), 1, "{:?}", notices(&handle));
    assert_eq!(rig.fs.peek(&folder.join(&copies[0])), Some(spaced));
    assert!(notices(&handle).iter().any(
        |n| matches!(&n.notice, Notice::FixedFileRestored { path, .. } if path.as_str() == rel)
    ));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// A held file that changes again is kept again under its own entry (delta §2.11): the hold
/// stays, its notice names the newest bytes, and both versions of the file stay as copies,
/// each listed until it is imported or dismissed.
#[test]
fn a_held_file_that_changes_again_is_kept_again_and_both_copies_stay() {
    let (rig, folder) = titus();
    saved_and_closed(&rig, &folder);
    settle_entries(&rig, &folder);
    let handle = open(&rig, &folder);
    let usfm = usfm_path(&folder);
    let first = damage_file(&rig, &folder, "ingredients/TIT.usfm");
    save_verse(&handle, 1, 2, "to Titus").unwrap();
    leave(&handle);
    assert_valid_burrito(&rig.fs, &folder);
    assert_eq!(hold_notices(&handle).len(), 1, "{:?}", notices(&handle));
    let mut second = first.clone();
    second.extend_from_slice(b"\\rem second, and longer\n");
    // Unseen: told of it, the watcher may finish another job before it looks, and the burrito
    // check after that job would see another program's bytes the app has not looked at yet.
    // For the same reason the writer's look at its last job ends first.
    handle.wait_idle();
    rig.fs.put_unseen(&usfm, &second, None);
    save_verse(&handle, 1, 3, "grace and peace").unwrap();
    leave(&handle);
    assert_valid_burrito(&rig.fs, &folder);
    handle.wait_idle();
    assert_eq!(hold_notices(&handle).len(), 1, "the hold stays");
    assert_eq!(rig.fs.peek(&usfm), Some(second.clone()), "still held");
    assert_eq!(handle.kept_copies().len(), 2, "each copy has its entry");
    let copies = copies_of(&handle, "ingredients/TIT.usfm");
    assert_eq!(copies.len(), 1, "{:?}", notices(&handle));
    assert_eq!(
        rig.fs.peek(&folder.join(&copies[0])),
        Some(second.clone()),
        "the notice names the newest bytes"
    );
    let kept: Vec<Vec<u8>> = crate::common::files(&rig.fs, &folder)
        .into_iter()
        .filter(|(rel, _)| rel.starts_with(".tc/external/"))
        .map(|(_, bytes)| bytes)
        .collect();
    assert!(kept.contains(&first), "the first copy stays");
    assert!(kept.contains(&second));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// Two outside edits in one second keep two copies: a copy never replaces another.
#[test]
fn two_outside_edits_in_one_second_keep_two_copies() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    let usfm = usfm_path(&folder);
    let original = rig.fs.peek(&usfm).unwrap();
    let edit = |tail: &[u8]| {
        let mut bytes = original.clone();
        bytes.extend_from_slice(tail);
        bytes
    };
    rig.fs.put(&usfm, &edit(b"\\rem one\n"), None);
    leave(&handle);
    rig.fs.put(&usfm, &edit(b"\\rem two\n"), None);
    save_verse(&handle, 1, 2, "to Titus").unwrap();
    leave(&handle);
    let kept = crate::common::files(&rig.fs, &folder)
        .into_iter()
        .filter(|(rel, _)| rel.starts_with(".tc/external/"))
        .count();
    assert_eq!(kept, 2, "both versions of the file are kept");
    handle.close().unwrap();
}

// ---------------------------------------------------------------------------------------
// §5.10: metadata.json edited outside the app
// ---------------------------------------------------------------------------------------

fn edited_metadata(fs: &FaultFs, folder: &Path) -> Vec<u8> {
    let path = folder.join("metadata.json");
    let text = String::from_utf8(fs.peek(&path).unwrap()).unwrap();
    let edited = text.replacen("Titus test translation", "Titus, renamed in an editor", 1);
    assert_ne!(edited, text, "the project name is in metadata.json");
    fs.put(&path, edited.as_bytes(), None);
    edited.into_bytes()
}

/// I8: bytes in `metadata.json` that nothing names are an outside edit, also when the
/// manifest that would name them is gone.
#[test]
fn an_outside_edit_of_metadata_json_is_copied_when_the_manifest_is_gone() {
    let (rig, folder) = titus();
    saved_and_closed(&rig, &folder);
    let edited = edited_metadata(&rig.fs, &folder);
    rig.fs
        .delete(&folder.join(".tc/materialized.json"))
        .unwrap();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 2, "to Titus").unwrap();
    leave(&handle);
    let held = hold_notices(&handle);
    let (_, copy) = held
        .iter()
        .find(|(path, _)| path == "metadata.json")
        .unwrap_or_else(|| panic!("no notice for metadata.json: {:?}", notices(&handle)));
    assert_eq!(rig.fs.peek(&folder.join(copy)), Some(edited));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// The same when the manifest is there but does not parse.
#[test]
fn an_outside_edit_of_metadata_json_is_copied_when_the_manifest_does_not_parse() {
    let (rig, folder) = titus();
    saved_and_closed(&rig, &folder);
    let edited = edited_metadata(&rig.fs, &folder);
    rig.fs
        .put(&folder.join(".tc/materialized.json"), b"{ not json", None);
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 2, "to Titus").unwrap();
    leave(&handle);
    let held = hold_notices(&handle);
    let (_, copy) = held
        .iter()
        .find(|(path, _)| path == "metadata.json")
        .unwrap_or_else(|| panic!("no notice for metadata.json: {:?}", notices(&handle)));
    assert_eq!(rig.fs.peek(&folder.join(copy)), Some(edited));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// Bytes the app wrote itself are not an outside edit: a lost manifest does not copy
/// `metadata.json` when the bytes there are the ones about to be written.
#[test]
fn metadata_json_the_app_wrote_is_not_copied_as_an_outside_edit() {
    let (rig, folder) = titus();
    saved_and_closed(&rig, &folder);
    rig.fs
        .delete(&folder.join(".tc/materialized.json"))
        .unwrap();
    let handle = open(&rig, &folder);
    leave(&handle);
    assert!(
        hold_notices(&handle)
            .iter()
            .all(|(path, _)| path != "metadata.json"),
        "{:?}",
        notices(&handle)
    );
    handle.close().unwrap();
}

// ---------------------------------------------------------------------------------------
// §6.1: an open never refuses because of content
// ---------------------------------------------------------------------------------------

/// A save whose materialization did not run, and a derived file another program holds: the
/// open reports the file and goes on, and the file is written when it can be read.
#[test]
fn an_open_goes_on_when_a_derived_file_cannot_be_read() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    // Held, so the materialization a save starts three seconds later cannot run first.
    rig.store.hooks().hold(tc_store::Point::BeforeMaterialize);
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    // The process stops before the materialization.
    let fs = rig.fs.crash(false);
    rig.store
        .hooks()
        .release(tc_store::Point::BeforeMaterialize);
    drop(handle);
    let rig = rig_as(fs, APP, "host-1");
    let usfm = usfm_path(&folder);
    let stale = rig.fs.peek(&usfm).unwrap();
    rig.fs.set_unreadable(&usfm, true);
    let handle = open(&rig, &folder);
    assert!(
        notices(&handle).iter().any(
            |n| matches!(&n.notice, Notice::FileUnreadable { path, .. } if &**path == "ingredients/TIT.usfm")
        ),
        "{:?}",
        notices(&handle)
    );
    assert_eq!(words(&handle, 1, 1).as_deref(), Some("Paul, a servant"));
    assert_eq!(handle.facts().mode, Mode::Writable);
    assert_eq!(
        rig.fs.peek(&usfm),
        Some(stale),
        "nothing is written over it"
    );
    rig.fs.set_unreadable(&usfm, false);
    leave(&handle);
    let written = String::from_utf8(rig.fs.peek(&usfm).unwrap()).unwrap();
    assert!(written.contains("Paul, a servant"), "{written}");
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// A full disk at the open: the project opens read-only with the reason, shows everything it
/// holds, and writes nothing.
#[test]
fn an_open_on_a_full_disk_is_read_only_and_never_refused() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    let fs = rig.fs.crash(false);
    drop(handle);
    let rig = rig_as(fs, APP, "host-1");
    let before = files(&rig.fs, &folder);
    rig.fs.fail_writes(Some(std::io::ErrorKind::StorageFull));
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert!(
        matches!(
            handle.facts().mode,
            Mode::ReadOnly(ReadOnlyReason::WriteFailed(_))
        ),
        "{:?}",
        handle.facts().mode
    );
    assert_eq!(words(&handle, 1, 1).as_deref(), Some("Paul, a servant"));
    assert!(matches!(
        save_verse(&handle, 1, 2, "x"),
        Err(tc_store::CommandError::ReadOnly(_))
    ));
    handle.close().unwrap();
    assert_eq!(files(&rig.fs, &folder), before, "nothing was written");
    // Space again: the next open is writable and nothing was lost.
    rig.fs.fail_writes(None);
    let handle = open(&rig, &folder);
    assert_eq!(handle.facts().mode, Mode::Writable);
    assert_eq!(words(&handle, 1, 1).as_deref(), Some("Paul, a servant"));
    leave(&handle);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// The disk fills while the open writes the derived files (O10): the project is usable
/// read-only, and the next open finishes the work.
#[test]
fn a_disk_that_fills_during_the_opens_materialization_leaves_a_read_only_project() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    let fs = rig.fs.crash(false);
    drop(handle);
    let hooks = Arc::new(Hooks::default());
    let rig = rig_with_hooks(fs, hooks.clone());
    hooks.hold(Point::BeforeMaterialize);
    let handle = std::thread::scope(|scope| {
        let opening = scope.spawn(|| rig.store.open(&folder, &OpenOptions::default()));
        hooks.wait_reached(Point::BeforeMaterialize, 1);
        rig.fs.fail_writes(Some(std::io::ErrorKind::StorageFull));
        hooks.release(Point::BeforeMaterialize);
        opening.join().unwrap().unwrap()
    });
    handle.wait_idle();
    assert!(
        matches!(
            handle.facts().mode,
            Mode::ReadOnly(ReadOnlyReason::WriteFailed(_))
        ),
        "{:?}",
        handle.facts().mode
    );
    assert_eq!(words(&handle, 1, 1).as_deref(), Some("Paul, a servant"));
    handle.close().unwrap();
    rig.fs.fail_writes(None);
    let rig = rig_as(rig.fs.crash(false), APP, "host-1");
    let handle = open(&rig, &folder);
    assert_eq!(handle.facts().mode, Mode::Writable);
    leave(&handle);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

// ---------------------------------------------------------------------------------------
// §5.10: a derived file another program holds never stalls the actor
// ---------------------------------------------------------------------------------------

/// A rename refused for a sharing violation leaves the path dirty and tried again later:
/// the actor serves the next save at once, and the file is written when the other program
/// lets go.
#[test]
fn a_derived_file_another_program_holds_never_stalls_a_save() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let usfm = usfm_path(&folder);
    rig.fs.set_busy(&usfm, true);
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    leave(&handle);
    // The materialization found the file refused; the next save is not behind a wait.
    let started = Instant::now();
    save_verse(&handle, 1, 2, "to Titus").unwrap();
    let took = started.elapsed();
    assert!(
        took < Duration::from_millis(500),
        "a save waited {took:?} behind a file another program holds"
    );
    let started = Instant::now();
    leave(&handle);
    save_verse(&handle, 1, 3, "my true child").unwrap();
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "a materialization that finds the file still held does not wait"
    );
    assert!(
        !String::from_utf8(rig.fs.peek(&usfm).unwrap())
            .unwrap()
            .contains("Paul, a servant"),
        "nothing was written over the held file"
    );
    rig.fs.set_busy(&usfm, false);
    // The retry timer writes it without anyone asking.
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let text = String::from_utf8(rig.fs.peek(&usfm).unwrap()).unwrap();
        if text.contains("my true child") {
            break;
        }
        assert!(Instant::now() < deadline, "never written: {text}");
        std::thread::sleep(Duration::from_millis(50));
    }
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

// ---------------------------------------------------------------------------------------
// §6.2: the notice of a held file comes back after a restart
// ---------------------------------------------------------------------------------------

/// The notices live in memory; the hold lives in the manifest. After a restart the notice is
/// listed again, with its copy, so *Restore from the project* and *Take in the file* stay
/// reachable, and the file does not silently stop following the project.
#[test]
fn a_held_files_notice_comes_back_after_a_restart() {
    let (rig, folder) = titus();
    saved_and_closed(&rig, &folder);
    settle_entries(&rig, &folder);
    let usfm = usfm_path(&folder);
    let edited = damage_file(&rig, &folder, "ingredients/TIT.usfm");
    let handle = open(&rig, &folder);
    let first = hold_notices(&handle);
    assert_eq!(first.len(), 1, "{first:?}");
    handle.close().unwrap();

    let handle = open(&rig, &folder);
    let again = hold_notices(&handle);
    assert_eq!(again, first, "the same notice, with the same copy");
    save_verse(&handle, 1, 2, "to Titus").unwrap();
    leave(&handle);
    assert_eq!(rig.fs.peek(&usfm), Some(edited.clone()), "still held");
    // Dismissing hides the notice for this session; the copy stays while the hold does.
    let notice = notices(&handle)
        .into_iter()
        .find(|n| n.copy.is_some())
        .unwrap();
    handle.dismiss_notice(notice.id).wait().unwrap();
    assert!(hold_notices(&handle).is_empty());
    assert!(
        rig.fs
            .peek(&folder.join(notice.copy.as_ref().unwrap()))
            .is_some()
    );
    handle.close().unwrap();

    let handle = open(&rig, &folder);
    assert_eq!(hold_notices(&handle).len(), 1, "back at the next open");
    // Restore from the project: the file follows the project again, the hold and its
    // notice end, and the person's version stays in its copy.
    handle.restore_held("ingredients/TIT.usfm").wait().unwrap();
    handle.wait_idle();
    let restored = String::from_utf8(rig.fs.peek(&usfm).unwrap()).unwrap();
    assert!(restored.contains("to Titus"));
    assert_ne!(restored.as_bytes(), edited.as_slice());
    assert!(hold_notices(&handle).is_empty());
    assert!(
        !notices(&handle)
            .iter()
            .any(|n| matches!(n.notice, Notice::OutsideEditRecorded { .. })),
        "the damaged bytes the person gave up are not recorded: {:?}",
        notices(&handle)
    );
    handle.close().unwrap();
    let handle = open(&rig, &folder);
    assert!(hold_notices(&handle).is_empty(), "the hold ended for good");
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

// ---------------------------------------------------------------------------------------
// §6.3: an unreadable pending file
// ---------------------------------------------------------------------------------------

/// A pending file that cannot be read at open may hold sequence numbers this device used.
/// The open starts a new device, so a save cannot reuse one; when the file can be read
/// again, nothing says another computer saved as this one.
#[test]
fn an_unreadable_pending_file_makes_this_device_start_a_new_one() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "first, in a file that gets held").unwrap();
    let device = handle.facts().device;
    let fs = rig.fs.crash(false);
    drop(handle);
    let rig = rig_as(fs, APP, "host-1");
    let held: Vec<String> = pending_files(&rig.fs, &folder);
    assert_eq!(held.len(), 1, "{held:?}");
    let abs = folder.join(&held[0]);
    rig.fs.set_unreadable(&abs, true);

    let handle = open(&rig, &folder);
    assert_ne!(handle.facts().device, device, "a new device");
    assert!(notices(&handle).iter().any(|n| matches!(
        &n.notice,
        Notice::DeviceRotated { old, reason: RotationReason::PendingUnreadable, .. } if *old == device
    )));
    save_verse(&handle, 1, 2, "second").unwrap();
    handle.close().unwrap();

    rig.fs.set_unreadable(&abs, false);
    let handle = open(&rig, &folder);
    assert!(
        !handle
            .snapshot()
            .fold()
            .anomalies()
            .any(|a| matches!(a, Anomaly::DuplicateDevice { .. })),
        "no other computer saved as this one"
    );
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("first, in a file that gets held")
    );
    assert_eq!(words(&handle, 1, 2).as_deref(), Some("second"));
    assert!(!notices(&handle).iter().any(|n| matches!(
        &n.notice,
        Notice::DeviceRotated {
            reason: RotationReason::DuplicateDevice,
            ..
        }
    )));
    handle.close().unwrap();
}

// ---------------------------------------------------------------------------------------
// §5.13: a seal that fails after its pack is renamed
// ---------------------------------------------------------------------------------------

/// The pack is in place, so `metadata.json` must list it (I7) even though the seal then
/// failed: the checkpoint materializes anyway, and the project is a valid burrito at once.
#[test]
fn a_seal_that_fails_after_its_pack_leaves_a_valid_burrito() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    leave(&handle);
    save_verse(&handle, 1, 2, "to Titus").unwrap();
    leave(&handle);
    handle.wait_idle();
    // The flush of `.tc/pending` after the pending files go (K4) is the last flush of the
    // seal's pending files: find it on a copy.
    let probe = rig.fs.copy();
    let probe_rig = rig_as(probe.clone(), APP, "host-1");
    let probe_handle = open(&probe_rig, &folder);
    probe.clear_log();
    let start = probe.ops();
    probe_handle
        .checkpoint(tc_store::CheckpointReason::Periodic)
        .wait()
        .unwrap();
    let pending_dir = folder.join(".tc/pending");
    let k4 = probe
        .log()
        .into_iter()
        .filter(|op| op.kind == OpKind::SyncDir && op.path == pending_dir)
        .map(|op| op.index - start)
        .next_back()
        .expect("the seal flushes .tc/pending");
    drop(probe_handle);

    let packs_before = packs(&rig.fs, &folder).len();
    rig.fs.fail_op(k4, std::io::ErrorKind::Other);
    let result = handle
        .checkpoint(tc_store::CheckpointReason::Periodic)
        .wait();
    assert!(result.is_err(), "the seal reported its failure");
    assert_eq!(
        packs(&rig.fs, &folder).len(),
        packs_before + 1,
        "the new pack is in place"
    );
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

// ---------------------------------------------------------------------------------------
// §5.0: nothing is written after the lock is released
// ---------------------------------------------------------------------------------------

/// The draft writer is a thread beside the actor. A close waits for it before it releases
/// the lock, so the next open of the project (after a restart, or by a program that takes the
/// lock as this session ends) never races a late `drafts.json`.
#[test]
fn nothing_is_written_into_the_folder_after_the_lock_is_released() {
    let (rig, folder) = titus();
    let hooks = Arc::new(Hooks::default());
    let rig2 = rig_with_hooks(rig.fs.crash(false), hooks.clone());
    let handle = rig2.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    hooks.hold(Point::BeforeDraftWrite);
    let shown = handle.mint(&verse(1, 5), None).unwrap();
    handle
        .submit(Command::Draft {
            draft: Draft {
                field: DraftField::Words,
                shown: vec![shown],
                text: "unsaved text".into(),
            },
            at: tc_journal::WallTime(0),
        })
        .wait()
        .unwrap();
    hooks.wait_reached(Point::BeforeDraftWrite, 1);
    rig2.fs.clear_log();
    let releasing = {
        let hooks = hooks.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            hooks.release(Point::BeforeDraftWrite);
        })
    };
    handle.close().unwrap();
    releasing.join().unwrap();
    let log = rig2.fs.log();
    let released = log
        .iter()
        .rposition(|op| op.kind == OpKind::LockWrite)
        .expect("the close clears the lock's content");
    let late: Vec<_> = log[released + 1..].iter().collect();
    assert!(
        late.is_empty(),
        "written after the lock was released: {late:?}"
    );
    assert!(
        log.iter()
            .any(|op| op.kind == OpKind::Rename && op.path.ends_with(".tc/drafts.json")),
        "the draft was written before it"
    );
}

// ---------------------------------------------------------------------------------------
// §5.14, R20: a section draft whose tokens no longer verify
// ---------------------------------------------------------------------------------------

/// *Keep* of an unverified section draft judges the section as the records stand: a span the
/// draft keeps is coherent against the verses it covers, whose values the draft lists as
/// unchanged.
#[test]
fn an_unverified_section_draft_that_holds_a_span_can_be_kept() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    // 1:4-5 is a span: 4 holds the text, 5 is covered by it.
    let text =
        |handle: &ProjectHandle, v: u16| match handle.snapshot().fold().projected(&verse(1, v)) {
            Some(Record::Verse { value, .. }) => value,
            other => panic!("{other:?}"),
        };
    let VerseValue::Text {
        marker,
        layout,
        from,
        ..
    } = text(&handle, 4)
    else {
        panic!("1:4 is text");
    };
    let span = VerseValue::Text {
        marker,
        words: "four and five, as one verse".into(),
        layout,
        last: tc_journal::VerseNo::new(5).unwrap(),
        from,
    };
    let section = |handle: &ProjectHandle, values: Vec<(u16, Option<SectionValue>)>| {
        values
            .into_iter()
            .map(|(v, value)| (handle.mint(&verse(1, v), None).unwrap(), value))
            .collect::<Vec<_>>()
    };
    handle
        .submit(Command::SaveSection {
            book: TIT,
            shown: section(
                &handle,
                vec![
                    (4, Some(SectionValue::Verse(span.clone()))),
                    (
                        5,
                        Some(SectionValue::Verse(VerseValue::Covered {
                            by: tc_journal::VerseNo::new(4).unwrap(),
                        })),
                    ),
                    (6, None),
                ],
            ),
            layout: None,
        })
        .wait()
        .unwrap();
    // The draft is written over those three verses, and the installation secret is lost.
    let keys = [verse(1, 4), verse(1, 5), verse(1, 6)];
    handle
        .submit(Command::Draft {
            draft: Draft {
                field: DraftField::Section { book: TIT },
                shown: keys.iter().map(|k| handle.mint(k, None).unwrap()).collect(),
                text: "\\v 4-5 four and five, typed \\v 6 six".into(),
            },
            at: tc_journal::WallTime(0),
        })
        .wait()
        .unwrap();
    handle.wait_idle();
    handle.close().unwrap();
    let fs = rig.fs.crash(false);
    fs.delete(Path::new("/app/installation.json")).unwrap();
    let rig = rig_as(fs, APP, "host-1");
    let handle = open(&rig, &folder);
    let offered = handle.offered_drafts();
    assert_eq!(offered.len(), 1, "{offered:?}");
    assert!(!offered[0].verified);
    let VerseValue::Text {
        marker,
        layout,
        last,
        from,
        ..
    } = text(&handle, 4)
    else {
        panic!("1:4 is text");
    };
    let typed = VerseValue::Text {
        marker,
        words: "four and five, typed".into(),
        layout,
        last,
        from,
    };
    let draft = offered[0].draft.clone();
    let reply = handle
        .submit(Command::KeepDraft {
            draft: draft.clone(),
            section: vec![
                (draft.shown[0].clone(), Some(SectionValue::Verse(typed))),
                (draft.shown[1].clone(), None),
                (draft.shown[2].clone(), None),
            ],
        })
        .wait()
        .unwrap();
    assert_eq!(reply.changes.len(), 1, "only the verse the draft changed");
    let body = handle.snapshot().fold().body(&reply.changes[0]).unwrap();
    assert!(body.parents.is_empty(), "no parents (R20)");
    assert!(
        matches!(&body.record, Record::Verse { value: VerseValue::Text { words, last, .. }, .. }
            if words == "four and five, typed" && last.get() == 5),
        "{:?}",
        body.record
    );
    // Both texts stand: a conflict with what the record holds, never a revert.
    assert_eq!(handle.snapshot().fold().heads(&verse(1, 4)).len(), 2);
    assert!(handle.offered_drafts().is_empty());
    handle.close().unwrap();
}
