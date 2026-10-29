//! Carried files (`docs/STORAGE.md` §11.4, §5.16): files the app keeps verbatim. Their bytes
//! are blobs in the project's object database, found on disk by a look at the folder, placed
//! where the fold says, never read into memory whole, and never written over bytes the app did
//! not place.

use crate::common::*;
use std::path::{Path, PathBuf};
use tc_journal::command::Command;
use tc_journal::fold::{ConflictKind, Notice};
use tc_journal::record::CarriedValue;
use tc_journal::{ChangeId, DeviceId, GitOid, Md5, Record, RecordKey, RepoPath};
use tc_store::{CheckpointReason, Leave, OpenOptions, ProjectHandle};

const SONG: &str = "ingredients/audio/song.mp3";

fn open(rig: &Rig, folder: &Path) -> ProjectHandle {
    let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle
}

fn leave(handle: &ProjectHandle) {
    handle.leave(Leave::Mode).wait().unwrap();
}

fn song_key() -> RecordKey {
    RecordKey::CarriedFile(RepoPath::parse(SONG).unwrap())
}

/// Puts a file where another program would and has the window focus find it.
fn arrive(rig: &Rig, handle: &ProjectHandle, folder: &Path, rel: &str, bytes: &[u8]) {
    rig.fs.put_unseen(&folder.join(rel), bytes, None);
    handle.focus().wait().unwrap();
    handle.wait_idle();
}

/// Whether the project's object database holds the bytes.
fn stored(rig: &Rig, folder: &Path, bytes: &[u8]) -> bool {
    let repo = repo_of(&rig.fs, folder);
    let oid = git2::Oid::hash_object(git2::ObjectType::Blob, bytes).unwrap();
    repo.odb().unwrap().exists(oid)
}

/// Stores `bytes` in the object database as a teammate's pull would, and returns its id.
fn store_blob(rig: &Rig, folder: &Path, bytes: &[u8]) -> GitOid {
    let repo = repo_of(&rig.fs, folder);
    let oid = repo.blob(bytes).unwrap();
    GitOid::parse(&oid.to_string()).unwrap()
}

fn placements(handle: &ProjectHandle) -> Vec<tc_journal::fold::CarriedPlacement> {
    handle.snapshot().fold().carried_files()
}

/// A teammate's version of the file at `rel`, in a pending file of the folder.
fn teammate_version(
    rig: &Rig,
    folder: &Path,
    n: u8,
    rel: &str,
    oid: GitOid,
    bytes: &[u8],
    parents: Vec<ChangeId>,
) -> ChangeId {
    let record = Record::CarriedFile {
        path: RepoPath::parse(rel).unwrap(),
        value: CarriedValue::Present {
            oid,
            size: bytes.len() as u64,
            md5: Md5::of(bytes),
        },
    };
    crate::commands::teammate_batch(
        &rig.fs,
        folder,
        DeviceId::local([n; 15]),
        1,
        vec![(record, parents)],
    )[0]
}

/// A new file in the folder is stored whole in the object database, recorded as a version of
/// itself, listed by `metadata.json`, and in the tree of the next commit (L29, local half).
#[test]
fn a_new_carried_file_is_recorded_stored_listed_and_committed() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let bytes: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
    arrive(&rig, &handle, &folder, SONG, &bytes);
    assert!(stored(&rig, &folder, &bytes), "the bytes are a blob");
    let fold = handle.snapshot().fold().clone();
    assert!(matches!(
        fold.projected(&song_key()),
        Some(Record::CarriedFile {
            value: CarriedValue::Present { size: 5000, .. },
            ..
        })
    ));
    assert_eq!(placements(&handle).len(), 1);
    assert_eq!(rig.fs.peek(&folder.join(SONG)), Some(bytes.clone()));
    leave(&handle);
    let metadata = String::from_utf8(rig.fs.peek(&folder.join("metadata.json")).unwrap()).unwrap();
    assert!(metadata.contains(SONG), "{metadata}");
    assert_valid_burrito(&rig.fs, &folder);

    handle
        .checkpoint(CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    handle.wait_idle();
    let repo = repo_of(&rig.fs, &folder);
    let head = commits_of(&repo)[0];
    assert_eq!(commit_files(&repo, head).get(SONG), Some(&bytes));
    assert_commits_valid(&rig.fs, &folder);
    handle.close().unwrap();
}

/// An export reads a carried file's bytes from its blob, read-only: the version the fold
/// shows, and none for a blob this computer lacks.
#[test]
fn a_carried_files_bytes_read_from_its_blob() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let bytes: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
    arrive(&rig, &handle, &folder, SONG, &bytes);
    let Some(Record::CarriedFile {
        value: CarriedValue::Present { oid, .. },
        ..
    }) = handle.snapshot().fold().projected(&song_key())
    else {
        panic!("the file is recorded")
    };
    let before = rig.fs.peek(&folder.join(SONG));
    assert_eq!(handle.read_carried(oid).wait().unwrap(), Some(bytes));
    assert_eq!(
        rig.fs.peek(&folder.join(SONG)),
        before,
        "the read wrote nothing"
    );
    let absent = GitOid::parse("0123456789abcdef0123456789abcdef01234567").unwrap();
    assert_eq!(handle.read_carried(absent).wait().unwrap(), None);
    handle.close().unwrap();
}

/// L43: a carried file deleted outside the app comes back from its blob; a deletion is no
/// removal, whether the app is open or closed.
#[test]
fn loss_carried_file_deletion_not_propagated() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let bytes = b"sixteen bytes!!!".to_vec();
    arrive(&rig, &handle, &folder, SONG, &bytes);
    leave(&handle);
    rig.fs.delete(&folder.join(SONG)).unwrap();
    handle.focus().wait().unwrap();
    handle.wait_idle();
    assert_eq!(rig.fs.peek(&folder.join(SONG)), Some(bytes.clone()));
    assert!(handle.notices().iter().any(|n| matches!(
        &n.notice,
        Notice::MissingFileRebuilt { path } if path.as_str() == SONG
    )));
    handle.close().unwrap();

    // Closed: the next open places it again.
    rig.fs.delete(&folder.join(SONG)).unwrap();
    let handle = open(&rig, &folder);
    assert_eq!(rig.fs.peek(&folder.join(SONG)), Some(bytes));
    assert!(matches!(
        handle.snapshot().fold().projected(&song_key()),
        Some(Record::CarriedFile {
            value: CarriedValue::Present { .. },
            ..
        })
    ));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// L44: two versions of one file, from two computers, are both kept: one at the file's path
/// and the other at a path of its own, both listed by `metadata.json`.
#[test]
fn loss_carried_file_two_versions_both_kept() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let mine = b"my recording of the song".to_vec();
    arrive(&rig, &handle, &folder, SONG, &mine);
    handle.close().unwrap();
    let theirs = b"their recording of the song, longer".to_vec();
    let oid = store_blob(&rig, &folder, &theirs);
    teammate_version(&rig, &folder, 5, SONG, oid, &theirs, Vec::new());
    let handle = open(&rig, &folder);
    leave(&handle);
    // H5: both are kept with no question.
    assert_eq!(handle.snapshot().fold().conflicts().count(), 0);
    assert_eq!(handle.snapshot().fold().heads(&song_key()).len(), 2);
    let places = placements(&handle);
    assert_eq!(places.len(), 2);
    for place in &places {
        let want = if place.oid == oid { &theirs } else { &mine };
        assert_eq!(
            rig.fs.peek(&folder.join(&place.path)).as_ref(),
            Some(want),
            "{} holds its version",
            place.path
        );
    }
    assert!(places.iter().any(|p| p.path == SONG));
    assert!(
        places
            .iter()
            .any(|p| p.path != SONG && p.record.as_str() == SONG)
    );
    let metadata = String::from_utf8(rig.fs.peek(&folder.join("metadata.json")).unwrap()).unwrap();
    for place in &places {
        assert!(metadata.contains(&place.path), "{} is listed", place.path);
    }
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
}

/// L93: the copy of the other version is a file like any other: edited, it is a new version of
/// the file it was made from, whose parent is the version placed there.
#[test]
fn loss_conflict_copy_edit_kept() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    arrive(&rig, &handle, &folder, SONG, b"my recording of the song");
    handle.close().unwrap();
    let theirs = b"their recording of the song, longer".to_vec();
    let oid = store_blob(&rig, &folder, &theirs);
    teammate_version(&rig, &folder, 5, SONG, oid, &theirs, Vec::new());
    let handle = open(&rig, &folder);
    leave(&handle);
    let copy = placements(&handle)
        .into_iter()
        .find(|p| p.path != SONG)
        .expect("a copy of the other version");
    // A checkpoint commits the copy under the name it has now.
    handle
        .checkpoint(CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    handle.wait_idle();
    let edited = b"the copy, trimmed by hand".to_vec();
    arrive(&rig, &handle, &folder, &copy.path, &edited);
    let fold = handle.snapshot().fold().clone();
    let newest: Vec<ChangeId> = fold
        .history(&song_key())
        .into_iter()
        .filter(|id| {
            fold.body(id).is_some_and(|b| {
                matches!(b.cause, tc_journal::Cause::External { .. }) && b.parents == [copy.head]
            })
        })
        .collect();
    assert_eq!(newest.len(), 1, "one version recorded");
    let body = fold.body(&newest[0]).unwrap();
    assert_eq!(
        body.parents,
        vec![copy.head],
        "from the version placed there"
    );
    assert!(matches!(
        body.record,
        Record::CarriedFile { value: CarriedValue::Present { md5, .. }, .. } if md5 == Md5::of(&edited)
    ));
    assert!(stored(&rig, &folder, &edited));
    // The new version takes the place of the one the copy was made from: its bytes are the
    // person's, now at the copy's new name, and nothing else wrote over them.
    leave(&handle);
    let now = placements(&handle)
        .into_iter()
        .find(|p| p.md5 == Md5::of(&edited))
        .expect("the edit is placed");
    assert_eq!(rig.fs.peek(&folder.join(&now.path)), Some(edited));
    assert_valid_burrito(&rig.fs, &folder);
    // The commit holds the files where they are placed now, and no copy at a name the app
    // gave out earlier and no longer places (H5).
    handle
        .checkpoint(CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    handle.wait_idle();
    let repo = repo_of(&rig.fs, &folder);
    let head = commits_of(&repo)[0];
    let committed: Vec<String> = commit_files(&repo, head)
        .into_keys()
        .filter(|p| p.starts_with("ingredients/audio/"))
        .collect();
    let mut placed: Vec<String> = placements(&handle).into_iter().map(|p| p.path).collect();
    placed.sort();
    assert_eq!(
        committed, placed,
        "the copy's earlier name {} is not committed",
        copy.path
    );
    handle.close().unwrap();
}

/// §11.4: an edit of a placed file continues the version the app placed there, not the newest
/// head of the record. A teammate's version the folder never showed is a sibling of the edit,
/// so both are kept (H5: with no question), and neither replaces the other.
#[test]
fn an_edit_of_a_placed_file_does_not_replace_a_teammates_newer_version() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let first = b"first recording of the song".to_vec();
    arrive(&rig, &handle, &folder, SONG, &first);
    let h1 = handle.snapshot().fold().heads(&song_key());
    assert_eq!(h1.len(), 1);
    handle.close().unwrap();
    let theirs = b"a teammate's longer second recording".to_vec();
    let oid = store_blob(&rig, &folder, &theirs);
    let h2 = teammate_version(&rig, &folder, 5, SONG, oid, &theirs, h1.clone());
    // While the project is closed the person edits the file they were shown.
    let edited = b"first recording of the song, trimmed".to_vec();
    rig.fs.put_unseen(&folder.join(SONG), &edited, None);
    let handle = open(&rig, &folder);
    let fold = handle.snapshot().fold().clone();
    let heads = fold.heads(&song_key());
    assert_eq!(heads.len(), 2, "both versions are heads: {heads:?}");
    assert!(
        heads.contains(&h2),
        "the teammate's version is still a head"
    );
    let mine = heads.iter().find(|h| **h != h2).unwrap();
    assert_eq!(
        fold.body(mine).unwrap().parents,
        h1,
        "the edit continues the version the app placed"
    );
    assert_eq!(
        fold.conflicts().count(),
        0,
        "both are kept with no question"
    );
    leave(&handle);
    let held: Vec<Vec<u8>> = placements(&handle)
        .iter()
        .filter_map(|p| rig.fs.peek(&folder.join(&p.path)))
        .collect();
    assert!(
        held.contains(&edited) && held.contains(&theirs),
        "both are on disk"
    );
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
}

/// §5.9: a version whose bytes are not on this computer cannot be restored.
#[test]
fn restoring_a_version_whose_bytes_are_not_here_is_refused() {
    use tc_store::{CommandError, Refusal};
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    arrive(&rig, &handle, &folder, SONG, b"my recording of the song");
    handle.close().unwrap();
    // A teammate's version whose blob this computer never got.
    let theirs = b"their recording of the song, longer".to_vec();
    let missing = GitOid::parse(&"ab".repeat(20)).unwrap();
    let theirs_id = teammate_version(&rig, &folder, 5, SONG, missing, &theirs, Vec::new());
    let handle = open(&rig, &folder);
    let shown = handle.mint(&song_key(), None).unwrap();
    let refused = handle
        .submit(Command::Restore {
            shown,
            from: theirs_id,
        })
        .wait();
    assert!(
        matches!(
            refused,
            Err(CommandError::Refused(Refusal::FileNotOnThisComputer))
        ),
        "{refused:?}"
    );
    handle.close().unwrap();
}

/// L73: a version whose blob is missing here is told and never blocks a commit: the commit
/// is made, valid, with what the object database has.
#[test]
fn loss_carried_version_missing_blob_never_blocks_commit() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    handle.close().unwrap();
    let missing = GitOid::parse(&"cd".repeat(20)).unwrap();
    teammate_version(
        &rig,
        &folder,
        5,
        SONG,
        missing,
        b"a recording this computer never got",
        Vec::new(),
    );
    let handle = open(&rig, &folder);
    leave(&handle);
    let report = handle
        .checkpoint(CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert!(report.commit.is_some(), "{report:?}");
    assert!(handle.notices().iter().any(|n| matches!(
        &n.notice,
        Notice::CarriedBytesMissing { path, .. } if path.as_str() == SONG
    )));
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
    handle.close().unwrap();
}

/// §9.4, §11.4: a placed carried file whose blob the object database lost (a prune, a damaged
/// repository) is stored again from the folder when the file there holds the placement's
/// bytes, so the checkpoint commits it and tells of no missing bytes.
#[test]
fn a_placed_file_whose_blob_is_gone_is_stored_again_from_the_folder() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let bytes = b"my recording of the song".to_vec();
    arrive(&rig, &handle, &folder, SONG, &bytes);
    let oid = git2::Oid::hash_object(git2::ObjectType::Blob, &bytes).unwrap();
    let hex = oid.to_string();
    let loose = git_dir_of(&rig.fs, &folder)
        .join("objects")
        .join(&hex[..2])
        .join(&hex[2..]);
    std::fs::remove_file(&loose).unwrap();
    assert!(!stored(&rig, &folder, &bytes));

    leave(&handle);
    let report = handle
        .checkpoint(CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert!(report.commit.is_some(), "{report:?}");
    let repo = repo_of(&rig.fs, &folder);
    let head = commits_of(&repo)[0];
    assert_eq!(commit_files(&repo, head).get(SONG), Some(&bytes));
    assert!(
        !handle
            .notices()
            .iter()
            .any(|n| matches!(n.notice, Notice::CarriedBytesMissing { .. })),
        "{:?}",
        handle.notices()
    );
    handle.close().unwrap();
    assert_commits_valid(&rig.fs, &folder);
}

/// §11.4: a file is never held in memory whole. A 1 MiB file is hashed and stored in chunks
/// (the filesystem is read in pieces of at most 64 KiB), and written from its blob the same
/// way.
#[test]
fn a_large_carried_file_streams() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let size = 1024 * 1024usize;
    let bytes: Vec<u8> = (0..size)
        .map(|i| (i.wrapping_mul(31) % 253) as u8)
        .collect();
    rig.fs.reset_widest_io();
    arrive(&rig, &handle, &folder, SONG, &bytes);
    assert!(stored(&rig, &folder, &bytes));
    // Gone, then written from the blob.
    rig.fs.delete(&folder.join(SONG)).unwrap();
    handle.focus().wait().unwrap();
    handle.wait_idle();
    assert!(rig.fs.peek(&folder.join(SONG)) == Some(bytes));
    let widest = rig.fs.widest_io();
    assert!(widest <= 64 * 1024, "a read or write of {widest} bytes");
    handle.close().unwrap();
}

/// §3.3: a file whose name cannot be an ingredient's is moved out of the folder with a word,
/// never deleted (L62).
#[test]
fn loss_invalid_name_file_moved_not_deleted() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let name = "ingredients/TIT (conflicted copy).usfm";
    let bytes = b"\\id TIT a copy a cloud client made\n".to_vec();
    rig.fs.put_unseen(&folder.join(name), &bytes, None);
    handle.focus().wait().unwrap();
    assert!(rig.fs.peek(&folder.join(name)).is_none());
    let moved: Vec<PathBuf> = files(&rig.fs, &folder)
        .into_iter()
        .filter(|(rel, b)| rel.starts_with(".tc/external/") && *b == bytes)
        .map(|(rel, _)| PathBuf::from(rel))
        .collect();
    assert_eq!(moved.len(), 1);
    assert!(handle.notices().iter().any(|n| matches!(
        &n.notice,
        Notice::FileMoved { from, .. } if &**from == name
    )));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// §3.3 and H5: two files whose names differ by case only cannot both be written on Windows
/// and macOS. With no question, one keeps the name, by byte order, and the other is placed
/// beside it; a newer version of the one beside never swaps the names, since on a disk that
/// ignores case a read of the moved name would return the other file's bytes.
#[test]
fn a_path_case_clash_keeps_one_name_and_both_files() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let (upper, lower) = ("ingredients/audio/Song.mp3", "ingredients/audio/song.mp3");
    arrive(&rig, &handle, &folder, upper, b"the capital letter song");
    arrive(&rig, &handle, &folder, lower, b"the small letter song");
    leave(&handle);
    assert_eq!(handle.snapshot().fold().conflicts().count(), 0);
    let check = |want_lower: &[u8]| {
        let places = placements(&handle);
        assert_eq!(places.len(), 2);
        let kept = places
            .iter()
            .find(|p| p.path == upper)
            .expect("one keeps its name");
        assert_eq!(
            rig.fs.peek(&folder.join(&kept.path)).unwrap(),
            b"the capital letter song"
        );
        let aside = places
            .iter()
            .find(|p| p.record.as_str() == lower)
            .expect("the other is placed");
        assert!(aside.path.contains(".case-"), "{}", aside.path);
        assert_eq!(rig.fs.peek(&folder.join(&aside.path)).unwrap(), want_lower);
    };
    check(b"the small letter song");
    // A newer version of the one beside keeps the names where they are.
    let aside = placements(&handle)
        .into_iter()
        .find(|p| p.record.as_str() == lower)
        .unwrap();
    arrive(
        &rig,
        &handle,
        &folder,
        &aside.path,
        b"the small letter song, again",
    );
    leave(&handle);
    check(b"the small letter song, again");
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
}

/// *Take in the file as an edit* for a held carried file: a file edited outside and then
/// removed by a command is held (§5.16); taken in, its bytes are a new version of the file.
#[test]
fn a_held_carried_file_can_be_taken_in() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    arrive(&rig, &handle, &folder, SONG, b"the song as it was placed");
    leave(&handle);
    let edited = b"the song, edited in an audio editor".to_vec();
    rig.fs.put_unseen(&folder.join(SONG), &edited, None);
    handle
        .submit(Command::RemoveCarriedFile {
            shown: handle.mint(&song_key(), None).unwrap(),
        })
        .wait()
        .unwrap();
    leave(&handle);
    assert_eq!(rig.fs.peek(&folder.join(SONG)), Some(edited.clone()));
    assert_eq!(copies_of(&handle, SONG).len(), 1, "{:?}", handle.notices());
    let taken = handle.take_in_held(SONG).wait().unwrap();
    assert!(taken.batch.is_some(), "{taken:?}");
    handle.wait_idle();
    assert!(
        !handle.notices().iter().any(|n| matches!(
            &n.notice,
            Notice::OutsideEditHeld { path, .. } | Notice::StorageDamage { path, .. }
                if path.as_str() == SONG
        )),
        "the hold ended: {:?}",
        handle.notices()
    );
    assert!(
        matches!(
            handle.snapshot().fold().projected(&song_key()),
            Some(Record::CarriedFile { value: CarriedValue::Present { md5, .. }, .. })
                if md5 == Md5::of(&edited)
        ),
        "the taken-in bytes are the file's newest version"
    );
    assert!(stored(&rig, &folder, &edited));
    assert_eq!(rig.fs.peek(&folder.join(SONG)), Some(edited));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// A checkpoint whose seal serves a recorded edit of a carried file: V1 is recorded, and while
/// the seal runs the person saves V2 and a look at the folder records it. Returns the bytes in
/// the folder while the seal ran and the checkpoint's report.
fn checkpoint_serving_a_carried_edit(
    rig: &Rig,
    folder: &Path,
    handle: &std::sync::Arc<ProjectHandle>,
    v2: &[u8],
) -> (Option<Vec<u8>>, tc_store::CheckpointReport) {
    checkpoint_serving_a_carried_file(rig, folder, handle, SONG, v2)
}

/// [`checkpoint_serving_a_carried_edit`] for the file at `rel`.
fn checkpoint_serving_a_carried_file(
    rig: &Rig,
    folder: &Path,
    handle: &std::sync::Arc<ProjectHandle>,
    rel: &str,
    v2: &[u8],
) -> (Option<Vec<u8>>, tc_store::CheckpointReport) {
    use std::sync::{Arc, Mutex};
    let steps = Arc::new(Mutex::new(0));
    let during = Arc::new(Mutex::new(None));
    let looked: Arc<Mutex<Vec<tc_store::Pending<_>>>> = Arc::new(Mutex::new(Vec::new()));
    {
        let (handle, steps, fs, during, looked) = (
            handle.clone(),
            steps.clone(),
            rig.fs.clone(),
            during.clone(),
            looked.clone(),
        );
        let (song, v2) = (folder.join(rel), v2.to_vec());
        rig.store.hooks().at_seal_step(move || {
            let mut steps = steps.lock().unwrap();
            *steps += 1;
            match *steps {
                1 => {
                    fs.put_unseen(&song, &v2, None);
                    looked.lock().unwrap().push(handle.focus());
                }
                // The look ends once the writer has handed the actor the new version, so the
                // actor records it after this step.
                2 => {
                    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
                    while !looked.lock().unwrap().iter().all(|look| look.is_ready()) {
                        assert!(std::time::Instant::now() < until, "the look never ended");
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                }
                4 => *during.lock().unwrap() = fs.peek(&song),
                _ => {}
            }
        });
    }
    let report = handle
        .checkpoint(CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    rig.store.hooks().at_seal_step(|| {});
    for look in looked.lock().unwrap().drain(..) {
        look.wait().unwrap();
    }
    let during = during.lock().unwrap().take();
    (during, report)
}

/// The person's program saves `bytes` over the song once the writer has finished the
/// checkpoint's job, whose reply comes before the rig looks at the folder that job left as a
/// burrito, and before any later job could take the version in the folder into the entry
/// itself: the job a save and a leave make waits until the bytes are there.
fn save_after_the_checkpoint_job(rig: &Rig, handle: &ProjectHandle, folder: &Path, bytes: &[u8]) {
    let hooks = rig.store.hooks().clone();
    hooks.hold(tc_store::Point::BeforeMaterialize);
    save_verse(handle, 1, 3, "my true child").unwrap();
    let next = handle.leave(Leave::Mode);
    hooks.wait_reached(tc_store::Point::BeforeMaterialize, 1);
    rig.fs.put_unseen(&folder.join(SONG), bytes, None);
    hooks.release(tc_store::Point::BeforeMaterialize);
    next.wait().unwrap();
}

/// §5.13 C2 for carried files: a checkpoint's job runs on the snapshot its packs stand for, so
/// it never puts an older version of a carried file back over a newer one recorded while its
/// seal ran, and the person's next save continues the version in the folder: no conflict
/// between their own two edits.
#[test]
fn a_checkpoint_job_never_places_an_older_version_over_a_newer_edit() {
    let (rig, folder) = titus();
    let handle = std::sync::Arc::new(open(&rig, &folder));
    let v1 = b"version one of the song".to_vec();
    let v2 = b"version two of the song, longer".to_vec();
    let v3 = b"version three of the song, continuing two".to_vec();
    arrive(&rig, &handle, &folder, SONG, &v1);
    let (during, report) = checkpoint_serving_a_carried_edit(&rig, &folder, &handle, &v2);
    assert_eq!(during.as_ref(), Some(&v2), "the seal served the edit");
    assert_eq!(
        rig.fs.peek(&folder.join(SONG)),
        Some(v2.clone()),
        "the checkpoint left the newer version in the folder"
    );
    // The commit holds what its packs hold: V1.
    let repo = repo_of(&rig.fs, &folder);
    let commit = git2::Oid::from_bytes(report.commit.expect("a commit").as_bytes()).unwrap();
    assert_eq!(commit_files(&repo, commit).get(SONG), Some(&v1));
    // At once, the person's program saves V3 over V2.
    save_after_the_checkpoint_job(&rig, &handle, &folder, &v3);
    handle.focus().wait().unwrap();
    handle.wait_idle();
    let fold = handle.snapshot().fold().clone();
    assert_eq!(
        fold.conflicts_of(ConflictKind::CarriedFile).count(),
        0,
        "V3 continues V2"
    );
    assert_eq!(fold.heads(&song_key()).len(), 1);
    assert_eq!(
        fold.projected(&song_key()).and_then(|r| match r {
            Record::CarriedFile {
                value: CarriedValue::Present { size, .. },
                ..
            } => Some(size),
            _ => None,
        }),
        Some(v3.len() as u64),
        "V3 was recorded"
    );
    assert_eq!(rig.fs.peek(&folder.join(SONG)), Some(v3));
    assert_valid_burrito(&rig.fs, &folder);
    let handle = std::sync::Arc::try_unwrap(handle).unwrap_or_else(|_| panic!("no other owner"));
    handle.close().unwrap();
    assert_commits_valid(&rig.fs, &folder);
}

/// The same with a checkpoint whose marks name only a book: its `metadata.json` lists the
/// carried file as it is in the folder, and its listing never makes the entry name the older
/// version, so the person's next save is no conflict either.
#[test]
fn a_checkpoint_listing_keeps_the_placed_version_of_a_newer_edit() {
    let (rig, folder) = titus();
    let handle = std::sync::Arc::new(open(&rig, &folder));
    let v1 = b"version one of the song".to_vec();
    let v2 = b"version two of the song, longer".to_vec();
    let v3 = b"version three of the song, continuing two".to_vec();
    arrive(&rig, &handle, &folder, SONG, &v1);
    // The carried group was written; the checkpoint's marks are a save of a verse.
    rig.fs.advance(std::time::Duration::from_secs(10));
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    let (during, _report) = checkpoint_serving_a_carried_edit(&rig, &folder, &handle, &v2);
    assert_eq!(during.as_ref(), Some(&v2), "the seal served the edit");
    assert_eq!(rig.fs.peek(&folder.join(SONG)), Some(v2.clone()));
    assert_valid_burrito(&rig.fs, &folder);
    save_after_the_checkpoint_job(&rig, &handle, &folder, &v3);
    handle.focus().wait().unwrap();
    handle.wait_idle();
    let fold = handle.snapshot().fold().clone();
    assert_eq!(
        fold.conflicts_of(ConflictKind::CarriedFile).count(),
        0,
        "V3 continues V2"
    );
    assert_valid_burrito(&rig.fs, &folder);
    let handle = std::sync::Arc::try_unwrap(handle).unwrap_or_else(|_| panic!("no other owner"));
    handle.close().unwrap();
}

/// A carried file new to the project, recorded while a checkpoint's seal runs: the checkpoint's
/// job, whose fold has no place for it, neither removes it from the folder nor leaves it out of
/// `metadata.json`.
#[test]
fn a_checkpoint_job_keeps_a_carried_file_recorded_during_its_seal() {
    let (rig, folder) = titus();
    let handle = std::sync::Arc::new(open(&rig, &folder));
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    let bytes = b"a recording that arrived while the seal ran".to_vec();
    let (during, report) = checkpoint_serving_a_carried_file(&rig, &folder, &handle, SONG, &bytes);
    assert_eq!(during.as_ref(), Some(&bytes), "the seal served the edit");
    assert!(report.commit.is_some(), "{report:?}");
    assert_eq!(
        rig.fs.peek(&folder.join(SONG)),
        Some(bytes.clone()),
        "the checkpoint's job left the new file in the folder"
    );
    assert_valid_burrito(&rig.fs, &folder);
    handle.wait_idle();
    assert_eq!(placements(&handle).len(), 1);
    assert_eq!(rig.fs.peek(&folder.join(SONG)), Some(bytes));
    let handle = std::sync::Arc::try_unwrap(handle).unwrap_or_else(|_| panic!("no other owner"));
    handle.close().unwrap();
    assert_commits_valid(&rig.fs, &folder);
}

/// I7, §11.4: `metadata.json` lists every carried file as it is in the folder. A file waiting
/// for the actor to record it is listed with the size it was stored with (the writer keeps
/// none of its bytes in memory), and a file the fold no longer places is listed until the job
/// of the carried files removes it. Here a look records an edit of a placed file while the
/// person removes the file by the command, and the jobs that follow list it before the actor
/// has recorded the edit.
#[test]
fn a_carried_file_waiting_to_be_recorded_is_listed_as_it_is() {
    use tc_store::Point;
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    arrive(&rig, &handle, &folder, SONG, b"my recording of the song");
    leave(&handle);
    handle.wait_idle();
    let hooks = rig.store.hooks().clone();
    // The writer's next job waits, so the look and the removal's job queue behind it.
    hooks.hold(Point::BeforeMaterialize);
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    let first = handle.leave(Leave::Book(TIT));
    hooks.wait_reached(Point::BeforeMaterialize, 1);
    let edited = b"my recording of the song, edited and longer".to_vec();
    rig.fs.put_unseen(&folder.join(SONG), &edited, None);
    let looked = handle.focus();
    handle
        .submit(Command::RemoveCarriedFile {
            shown: handle.mint(&song_key(), None).unwrap(),
        })
        .wait()
        .unwrap();
    let second = handle.leave(Leave::Book(TIT));
    // The actor waits before it makes the recorded edit durable: the jobs list the file while
    // it waits to be recorded.
    hooks.hold(Point::BeforeDurablePending);
    hooks.release(Point::BeforeMaterialize);
    first.wait().unwrap();
    let found = looked.wait().unwrap().found;
    second.wait().unwrap();
    let problems = burrito_problems(&rig.fs, &folder);
    hooks.release(Point::BeforeDurablePending);
    assert_eq!(found, vec![SONG.to_owned()], "the look found the edit");
    assert!(problems.is_empty(), "{problems:#?}");
    handle.wait_idle();
    assert_eq!(
        rig.fs.peek(&folder.join(SONG)),
        Some(edited),
        "the person's bytes stay"
    );
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
}

/// I7: a carried file the person removed by the command stays in the folder until the job of
/// the carried files removes it, and `metadata.json` lists it until then: a look at the folder
/// that writes another file meanwhile (here a book file that was deleted) never leaves it out.
#[test]
fn a_removed_carried_file_is_listed_until_its_job_removes_it() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let bytes = b"my recording of the song".to_vec();
    arrive(&rig, &handle, &folder, SONG, &bytes);
    leave(&handle);
    handle.wait_idle();
    handle
        .submit(Command::RemoveCarriedFile {
            shown: handle.mint(&song_key(), None).unwrap(),
        })
        .wait()
        .unwrap();
    // Before the job of the removal (3 s after it), a look finds a book file gone and writes it.
    let usfm = folder.join("ingredients/TIT.usfm");
    rig.fs.delete(&usfm).unwrap();
    handle.focus().wait().unwrap();
    assert!(rig.fs.peek(&usfm).is_some(), "the look wrote the book file");
    assert_eq!(rig.fs.peek(&folder.join(SONG)), Some(bytes));
    assert_valid_burrito(&rig.fs, &folder);
    leave(&handle);
    handle.wait_idle();
    assert_eq!(
        rig.fs.peek(&folder.join(SONG)),
        None,
        "the removal's job removed it"
    );
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
}

/// Damages the loose object of `bytes`; returns its path and its bytes before.
fn damage_blob(rig: &Rig, folder: &Path, bytes: &[u8]) -> (PathBuf, Vec<u8>) {
    let oid = git2::Oid::hash_object(git2::ObjectType::Blob, bytes).unwrap();
    let hex = oid.to_string();
    let loose = git_dir_of(&rig.fs, folder)
        .join("objects")
        .join(&hex[..2])
        .join(&hex[2..]);
    let encoded = std::fs::read(&loose).unwrap();
    // External damage leaves the expected name present, with an unreadable object.
    std::fs::remove_file(&loose).unwrap();
    std::fs::write(&loose, b"this is not a zlib-encoded Git object").unwrap();
    assert!(
        stored(rig, folder, bytes),
        "existence alone reports a backup"
    );
    assert!(repo_of(&rig.fs, folder).find_blob(oid).is_err());
    (loose, encoded)
}

/// §11.4: a missing blob never stops a materialization, and neither does one whose name is
/// present but whose bytes cannot be read. A Restore of such a version leaves the folder's
/// file as it is, the job succeeds, and the project opens writable again.
#[test]
fn a_restored_version_whose_blob_is_damaged_waits_and_the_project_opens_writable() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let first = b"take A of the song, whose backup is later damaged";
    let second = b"take B of the song, the one the folder holds";
    arrive(&rig, &handle, &folder, SONG, first);
    leave(&handle);
    let first_heads = handle.snapshot().fold().heads(&song_key());
    assert_eq!(first_heads.len(), 1);
    let first_id = first_heads[0];
    arrive(&rig, &handle, &folder, SONG, second);
    leave(&handle);
    // The damage model of `corrupt_song_blob`, on the version the folder no longer holds.
    let oid = git2::Oid::hash_object(git2::ObjectType::Blob, first).unwrap();
    let hex = oid.to_string();
    let loose = git_dir_of(&rig.fs, &folder)
        .join("objects")
        .join(&hex[..2])
        .join(&hex[2..]);
    std::fs::remove_file(&loose).unwrap();
    std::fs::write(&loose, b"this is not a zlib-encoded Git object").unwrap();
    assert!(
        stored(&rig, &folder, first),
        "existence alone reports a backup"
    );
    assert!(repo_of(&rig.fs, &folder).find_blob(oid).is_err());

    handle
        .submit(Command::Restore {
            shown: handle.mint(&song_key(), None).unwrap(),
            from: first_id,
        })
        .wait()
        .unwrap();
    let left = handle.leave(Leave::Mode).wait();
    assert!(
        left.is_ok(),
        "an unreadable blob stopped the materialization: {left:?}"
    );
    assert_eq!(
        rig.fs.peek(&folder.join(SONG)).as_deref(),
        Some(second.as_slice())
    );
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();

    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert_eq!(
        handle.facts().mode,
        tc_store::Mode::Writable,
        "the open stopped at the unreadable blob"
    );
    handle.close().unwrap();
}

/// §9.4: a placed version whose blob is there and cannot be read never refuses a commit. The
/// commit keeps the version it had at that path, as the folder keeps its file.
#[test]
fn a_checkpoint_keeps_the_committed_version_while_the_placed_one_is_damaged() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let first = b"take A of the song, whose backup is later damaged";
    let second = b"take B of the song, committed and in the folder";
    arrive(&rig, &handle, &folder, SONG, first);
    leave(&handle);
    let first_id = handle.snapshot().fold().heads(&song_key())[0];
    arrive(&rig, &handle, &folder, SONG, second);
    leave(&handle);
    let report = handle
        .checkpoint(CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert!(report.commit.is_some(), "{report:?}");
    damage_blob(&rig, &folder, first);

    handle
        .submit(Command::Restore {
            shown: handle.mint(&song_key(), None).unwrap(),
            from: first_id,
        })
        .wait()
        .unwrap();
    let report = handle.checkpoint(CheckpointReason::LeavingTranslate).wait();
    assert!(
        report.as_ref().is_ok_and(|r| r.commit.is_some()),
        "the damaged version refused the commit: {report:?}"
    );
    let repo = repo_of(&rig.fs, &folder);
    let head = commits_of(&repo)[0];
    assert_eq!(
        commit_files(&repo, head).get(SONG).map(Vec::as_slice),
        Some(second.as_slice())
    );
    assert_eq!(
        rig.fs.peek(&folder.join(SONG)).as_deref(),
        Some(second.as_slice())
    );
    drop(repo);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}

/// Another program keeps a carried file unreadable while the person restores an earlier
/// version of it: the replacement waits without failing the book's files, and once the file
/// can be read again the restored version is placed.
#[test]
fn a_carried_file_that_cannot_be_read_waits_for_its_replacement() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let first = b"the first recording".to_vec();
    let second = b"the second recording, with other bytes".to_vec();
    arrive(&rig, &handle, &folder, SONG, &first);
    leave(&handle);
    handle.wait_idle();
    let first_id = handle.snapshot().fold().heads(&song_key())[0];
    arrive(&rig, &handle, &folder, SONG, &second);
    leave(&handle);
    handle.wait_idle();
    let path = folder.join(SONG);
    rig.fs.set_unreadable(&path, true);
    handle
        .submit(Command::Restore {
            shown: handle.mint(&song_key(), None).unwrap(),
            from: first_id,
        })
        .wait()
        .unwrap();
    leave(&handle);
    handle.wait_idle();
    assert_eq!(
        rig.fs.peek(&path),
        Some(second),
        "an unreadable file is not replaced"
    );
    rig.fs.set_unreadable(&path, false);
    leave(&handle);
    handle.wait_idle();
    assert_eq!(
        rig.fs.peek(&path),
        Some(first),
        "placed once it can be read"
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}
