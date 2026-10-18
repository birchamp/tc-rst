//! Carried files (`docs/STORAGE.md` §11.4, §5.16): files the app keeps verbatim. Their bytes
//! are blobs in the project's object database, found on disk by a look at the folder, placed
//! where the fold says, never read into memory whole, and never written over bytes the app did
//! not place.

use crate::common::*;
use std::path::{Path, PathBuf};
use tc_journal::command::{Choice, Command};
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
    let conflict = handle
        .snapshot()
        .fold()
        .conflicts_of(ConflictKind::CarriedFile)
        .next()
        .cloned()
        .expect("two versions of a file");
    assert_eq!(conflict.candidates.len(), 2);
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
    assert!(places.iter().any(|p| p.path.contains(".conflict-")));
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
        .find(|p| p.path.contains(".conflict-"))
        .expect("a copy of the other version");
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
    handle.close().unwrap();
}

/// *Keep both* (§8.3): the other version becomes a file of its own, and the conflict is gone.
#[test]
fn keep_both_gives_the_other_version_its_own_file() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let mine = b"my recording of the song".to_vec();
    arrive(&rig, &handle, &folder, SONG, &mine);
    handle.close().unwrap();
    let theirs = b"their recording of the song, longer".to_vec();
    let oid = store_blob(&rig, &folder, &theirs);
    teammate_version(&rig, &folder, 5, SONG, oid, &theirs, Vec::new());
    let handle = open(&rig, &folder);
    let conflict = handle
        .snapshot()
        .fold()
        .conflicts_of(ConflictKind::CarriedFile)
        .next()
        .cloned()
        .unwrap();
    let (keep, other) = (
        conflict.provisional,
        conflict
            .candidates
            .iter()
            .map(|c| c.head)
            .find(|h| *h != conflict.provisional)
            .unwrap(),
    );
    let to = "ingredients/audio/song-second.mp3";
    handle
        .submit(Command::Resolve {
            conflict: conflict.id,
            shown: vec![handle.mint_review(&song_key()).unwrap()],
            choice: Choice::KeepBothCarried {
                keep,
                other,
                to: RepoPath::parse(to).unwrap(),
            },
        })
        .wait()
        .unwrap();
    handle.wait_idle();
    leave(&handle);
    assert_eq!(handle.snapshot().fold().conflicts().count(), 0);
    let bytes_at = |rel: &str| rig.fs.peek(&folder.join(rel));
    let kept = placements(&handle);
    assert_eq!(kept.len(), 2);
    assert!(bytes_at(SONG).is_some() && bytes_at(to).is_some());
    let both: Vec<Vec<u8>> = [bytes_at(SONG).unwrap(), bytes_at(to).unwrap()].to_vec();
    assert!(both.contains(&mine) && both.contains(&theirs));
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
fn carried_version_missing_blob_never_blocks_commit() {
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

/// §11.4: a file is never held in memory whole. A 20 MiB file is hashed and stored in chunks
/// (the filesystem is read in pieces of at most 64 KiB), and written from its blob the same
/// way.
#[test]
fn a_large_carried_file_streams() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let size = 20 * 1024 * 1024usize;
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

/// §3.3, §8.3: two files whose names differ by case only cannot both be written on Windows
/// and macOS; the project lists a path-case conflict and places one at a name of its own
/// until a person renames it, which moves the bytes.
#[test]
fn a_path_case_rename_moves_the_bytes() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let (upper, lower) = ("ingredients/audio/Song.mp3", "ingredients/audio/song.mp3");
    arrive(&rig, &handle, &folder, upper, b"the capital letter song");
    arrive(&rig, &handle, &folder, lower, b"the small letter song");
    leave(&handle);
    let conflict = handle
        .snapshot()
        .fold()
        .conflicts_of(ConflictKind::PathCase)
        .next()
        .cloned()
        .expect("a path-case conflict");
    let places = placements(&handle);
    assert_eq!(places.len(), 2);
    assert_valid_burrito(&rig.fs, &folder);
    // The file that was moved aside is renamed.
    let aside = places
        .iter()
        .find(|p| p.path.contains(".case-"))
        .expect("one is placed apart");
    let to = "ingredients/audio/second-song.mp3";
    let shown: Vec<_> = std::iter::once(&conflict.key)
        .chain(&conflict.also_keys)
        .map(|k| handle.mint_review(k).unwrap())
        .collect();
    handle
        .submit(Command::Resolve {
            conflict: conflict.id,
            shown,
            choice: Choice::RenameCarried {
                from: aside.record.clone(),
                to: RepoPath::parse(to).unwrap(),
            },
        })
        .wait()
        .unwrap();
    handle.wait_idle();
    leave(&handle);
    assert_eq!(handle.snapshot().fold().conflicts().count(), 0);
    let moved = rig.fs.peek(&folder.join(to)).expect("the bytes moved");
    let aside_bytes = if aside.record.as_str() == upper {
        b"the capital letter song".to_vec()
    } else {
        b"the small letter song".to_vec()
    };
    assert_eq!(moved, aside_bytes);
    assert!(
        rig.fs.peek(&folder.join(&aside.path)).is_none(),
        "the name it had apart is gone"
    );
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
        copies_of(&handle, SONG).is_empty()
            || !handle.notices().iter().any(|n| matches!(
                &n.notice,
                Notice::OutsideEditHeld { .. } | Notice::StorageDamage { .. }
            )),
        "the hold ended"
    );
    assert!(stored(&rig, &folder, &edited));
    assert_eq!(rig.fs.peek(&folder.join(SONG)), Some(edited));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}
