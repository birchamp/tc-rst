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

/// §11.4: an edit of a placed file continues the version the app placed there, not the newest
/// head of the record. A teammate's version the folder never showed is a sibling of the edit,
/// so both are kept and listed as a conflict (I4), and neither replaces the other.
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
        fold.conflicts_of(ConflictKind::CarriedFile).count(),
        1,
        "the two versions are listed"
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

/// A path the other version is moved to must be one a carried file may have: the files the
/// app writes, a folder it needs and a path another record holds are refused, and nothing is
/// written (V8, §11.4).
#[test]
fn keep_both_to_a_path_the_app_writes_or_another_file_holds_is_refused() {
    use tc_store::CommandError;
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    arrive(&rig, &handle, &folder, SONG, b"my recording of the song");
    arrive(
        &rig,
        &handle,
        &folder,
        "ingredients/audio/other.mp3",
        b"another song",
    );
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
    let other = conflict
        .candidates
        .iter()
        .map(|c| c.head)
        .find(|h| *h != conflict.provisional)
        .unwrap();
    let before = handle.snapshot().fold().len();
    for to in [
        "ingredients/TIT.usfm",
        "ingredients/tc/conflicts.json",
        "ingredients/audio/other.mp3",
    ] {
        let refused = handle
            .submit(Command::Resolve {
                conflict: conflict.id,
                shown: vec![handle.mint_review(&song_key()).unwrap()],
                choice: Choice::KeepBothCarried {
                    keep: conflict.provisional,
                    other,
                    to: RepoPath::parse(to).unwrap(),
                },
            })
            .wait();
        assert!(
            matches!(refused, Err(CommandError::Refused(_))),
            "{to}: {refused:?}"
        );
    }
    assert_eq!(
        handle.snapshot().fold().len(),
        before,
        "nothing was written"
    );
    assert_eq!(handle.snapshot().fold().conflicts().count(), 1);
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
                // The writer hands the actor the new version, which records it after this step,
                // and the writer takes the actor's answer before the next.
                2 | 3 => std::thread::sleep(std::time::Duration::from_millis(500)),
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

fn remove_song_blob(rig: &Rig, folder: &Path, bytes: &[u8]) -> git2::Oid {
    let oid = git2::Oid::hash_object(git2::ObjectType::Blob, bytes).unwrap();
    let hex = oid.to_string();
    let loose = git_dir_of(&rig.fs, folder)
        .join("objects")
        .join(&hex[..2])
        .join(&hex[2..]);
    std::fs::remove_file(&loose).unwrap();
    assert!(
        !stored(rig, folder, bytes),
        "the old blob was actually lost"
    );
    assert_eq!(
        rig.fs.peek(&folder.join(SONG)).as_deref(),
        Some(bytes),
        "the ingredient is the surviving byte copy"
    );
    oid
}

#[test]
fn a_replaced_carried_file_recovers_its_lost_old_blob_before_overwrite() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let old = b"my first recording, still restorable from History";
    arrive(&rig, &handle, &folder, SONG, old);
    leave(&handle);
    let old_heads = handle.snapshot().fold().heads(&song_key());
    assert_eq!(old_heads.len(), 1);
    let old_id = old_heads[0];
    handle.close().unwrap();

    let newer = b"a teammate's complete replacement recording";
    let newer_oid = store_blob(&rig, &folder, newer);
    let old_oid = remove_song_blob(&rig, &folder, old);
    let newer_id = teammate_version(&rig, &folder, 5, SONG, newer_oid, newer, old_heads);

    let handle = open(&rig, &folder);
    leave(&handle);
    handle.wait_idle();
    assert_eq!(
        rig.fs.peek(&folder.join(SONG)).as_deref(),
        Some(newer.as_slice())
    );
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    assert_eq!(fold.heads(&song_key()), vec![newer_id]);
    assert!(fold.history(&song_key()).contains(&old_id));
    let repo = repo_of(&rig.fs, &folder);
    let blob = repo.find_blob(old_oid).expect(
        "replacement must recover the old History bytes before overwriting their only file",
    );
    assert_eq!(blob.content(), old);
    assert_valid_burrito(&rig.fs, &folder);

    // The proof above precedes a checkpoint: closing must not be what rescues the bytes.
    drop(blob);
    drop(repo);
    handle.close().unwrap();
    let handle = open(&rig, &folder);
    assert!(
        handle
            .snapshot()
            .fold()
            .history(&song_key())
            .contains(&old_id)
    );
    assert_eq!(
        repo_of(&rig.fs, &folder)
            .find_blob(old_oid)
            .unwrap()
            .content(),
        old
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}

#[test]
fn a_removed_carried_file_recovers_its_lost_blob_before_unlink() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let old = b"the recording removed from the folder but retained in History";
    arrive(&rig, &handle, &folder, SONG, old);
    leave(&handle);
    handle.wait_idle();
    let old_heads = handle.snapshot().fold().heads(&song_key());
    assert_eq!(old_heads.len(), 1);
    let old_id = old_heads[0];
    handle.close().unwrap();
    let old_oid = remove_song_blob(&rig, &folder, old);
    let handle = open(&rig, &folder);
    assert!(
        !stored(&rig, &folder, old),
        "opening alone did not repair the missing blob"
    );

    handle
        .submit(Command::RemoveCarriedFile {
            shown: handle.mint(&song_key(), None).unwrap(),
        })
        .wait()
        .unwrap();
    leave(&handle);
    handle.wait_idle();
    assert!(
        rig.fs.peek(&folder.join(SONG)).is_none(),
        "the explicit removal completed"
    );
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    assert!(matches!(
        fold.projected(&song_key()),
        Some(Record::CarriedFile {
            value: CarriedValue::Removed,
            ..
        })
    ));
    assert!(fold.history(&song_key()).contains(&old_id));
    let repo = repo_of(&rig.fs, &folder);
    let blob = repo
        .find_blob(old_oid)
        .expect("removal must recover the History bytes before unlinking their only file");
    assert_eq!(blob.content(), old);
    assert_valid_burrito(&rig.fs, &folder);

    // Again, assert before close's checkpoint and then prove the restored blob survives reopen.
    drop(blob);
    drop(repo);
    handle.close().unwrap();
    let handle = open(&rig, &folder);
    assert!(
        handle
            .snapshot()
            .fold()
            .history(&song_key())
            .contains(&old_id)
    );
    assert_eq!(
        repo_of(&rig.fs, &folder)
            .find_blob(old_oid)
            .unwrap()
            .content(),
        old
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}

#[cfg(unix)]
#[test]
fn a_cached_carried_blob_is_recovered_after_its_pack_disappears() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let first = b"the packed first recording whose cached bytes must not stand for disk";
    let second = b"the loose second recording available to restore";
    arrive(&rig, &handle, &folder, SONG, first);
    leave(&handle);
    let first_heads = handle.snapshot().fold().heads(&song_key());
    assert_eq!(first_heads.len(), 1);
    let first_id = first_heads[0];
    arrive(&rig, &handle, &folder, SONG, second);
    leave(&handle);
    let second_heads = handle.snapshot().fold().heads(&song_key());
    assert_eq!(second_heads.len(), 1);
    let second_id = second_heads[0];

    let first_oid = git2::Oid::hash_object(git2::ObjectType::Blob, first).unwrap();
    let pack_dir = git_dir_of(&rig.fs, &folder).join("objects/pack");
    let before: std::collections::BTreeSet<PathBuf> = std::fs::read_dir(&pack_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    let repo = repo_of(&rig.fs, &folder);
    let mut builder = repo.packbuilder().unwrap();
    builder.insert_object(first_oid, None).unwrap();
    builder.write(&pack_dir, 0o644).unwrap();
    drop(builder);
    drop(repo);
    let pack_files: Vec<PathBuf> = std::fs::read_dir(&pack_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| !before.contains(path))
        .collect();
    assert!(
        pack_files
            .iter()
            .any(|path| path.extension().is_some_and(|e| e == "pack"))
    );
    assert!(
        pack_files
            .iter()
            .any(|path| path.extension().is_some_and(|e| e == "idx"))
    );
    let hex = first_oid.to_string();
    std::fs::remove_file(
        git_dir_of(&rig.fs, &folder)
            .join("objects")
            .join(&hex[..2])
            .join(&hex[2..]),
    )
    .unwrap();
    assert!(
        stored(&rig, &folder, first),
        "the pack is the remaining backup"
    );

    // Placing a packed blob takes copy_blob_to's read_blob fallback and primes the
    // writer's raw ODB cache; it does not create a fresh loose object.
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
        rig.fs.peek(&folder.join(SONG)).as_deref(),
        Some(first.as_slice())
    );
    assert!(
        !git_dir_of(&rig.fs, &folder)
            .join("objects")
            .join(&hex[..2])
            .join(&hex[2..])
            .exists()
    );
    for pack_file in pack_files {
        std::fs::remove_file(pack_file).unwrap();
    }
    assert!(
        !stored(&rig, &folder, first),
        "a fresh repository sees that disk lost the backup"
    );
    assert_eq!(
        rig.fs.peek(&folder.join(SONG)).as_deref(),
        Some(first.as_slice())
    );

    handle
        .submit(Command::Restore {
            shown: handle.mint(&song_key(), None).unwrap(),
            from: second_id,
        })
        .wait()
        .unwrap();
    leave(&handle);
    handle.wait_idle();
    assert_eq!(
        rig.fs.peek(&folder.join(SONG)).as_deref(),
        Some(second.as_slice())
    );
    let repo = repo_of(&rig.fs, &folder);
    assert_eq!(
        repo.find_blob(first_oid)
            .expect("the ingredient's cached old bytes must be restored to physical backup before replacement")
            .content(),
        first
    );
    assert!(
        handle
            .snapshot()
            .fold()
            .history(&song_key())
            .contains(&first_id)
    );
    assert_valid_burrito(&rig.fs, &folder);
    drop(repo);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}

mod backup_model {
    use super::*;
    use proptest::prelude::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Version {
        A,
        B,
    }

    impl Version {
        fn index(self) -> usize {
            match self {
                Self::A => 0,
                Self::B => 1,
            }
        }

        fn other(self) -> Self {
            match self {
                Self::A => Self::B,
                Self::B => Self::A,
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Archive {
        Readable,
        Lost,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Ingredient {
        File(Version),
        Missing,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Access {
        Readable,
        Denied,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Goal {
        Place(Version),
        Remove,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Event {
        LoseCurrentArchive,
        DenyReads,
        AllowReads,
        Request(Goal),
        Materialize,
        Reopen,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Reference {
        ingredient: Ingredient,
        archives: [Archive; 2],
        access: Access,
        goal: Goal,
    }

    /// Replacing a byte copy must leave another byte copy. This model knows only the
    /// two authored versions, not the writer's interpretation of an entry or a cache hit.
    fn materialize(mut state: Reference) -> Reference {
        match (state.access, state.goal, state.ingredient) {
            (Access::Denied, Goal::Place(_) | Goal::Remove, _) => state,
            (Access::Readable, Goal::Remove, Ingredient::Missing) => state,
            (Access::Readable, Goal::Remove, Ingredient::File(old)) => {
                state.archives[old.index()] = Archive::Readable;
                state.ingredient = Ingredient::Missing;
                state
            }
            (Access::Readable, Goal::Place(target), ingredient) => {
                match state.archives[target.index()] {
                    Archive::Lost => state,
                    Archive::Readable => {
                        match ingredient {
                            Ingredient::File(old) if old != target => {
                                state.archives[old.index()] = Archive::Readable;
                            }
                            Ingredient::File(_) | Ingredient::Missing => {}
                        }
                        state.ingredient = Ingredient::File(target);
                        state
                    }
                }
            }
        }
    }

    fn next(mut state: Reference, event: Event) -> Reference {
        match event {
            Event::LoseCurrentArchive => {
                match state.ingredient {
                    Ingredient::File(version) => state.archives[version.index()] = Archive::Lost,
                    Ingredient::Missing => {}
                }
                state
            }
            Event::DenyReads => {
                state.access = Access::Denied;
                state
            }
            Event::AllowReads => {
                state.access = Access::Readable;
                state
            }
            Event::Request(goal) => {
                state.goal = goal;
                state
            }
            Event::Materialize => materialize(state),
            Event::Reopen => materialize(state),
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum Round {
        ReplaceBacked,
        ReplaceAfterLoss,
        ReplaceAfterLossAndReadDenial,
        ReplaceAfterLossAndReopen,
        RemoveBacked,
        RemoveAfterLoss,
        RemoveAfterLossAndReadDenial,
        RemoveAfterLossAndReopen,
    }

    impl Round {
        fn events(self, current: Version) -> Vec<Event> {
            let replace = Event::Request(Goal::Place(current.other()));
            let remove = Event::Request(Goal::Remove);
            match self {
                Self::ReplaceBacked => vec![replace, Event::Materialize],
                Self::ReplaceAfterLoss => {
                    vec![Event::LoseCurrentArchive, replace, Event::Materialize]
                }
                Self::ReplaceAfterLossAndReadDenial => vec![
                    Event::LoseCurrentArchive,
                    Event::DenyReads,
                    replace,
                    Event::Materialize,
                    Event::AllowReads,
                    Event::Materialize,
                ],
                Self::ReplaceAfterLossAndReopen => vec![
                    Event::LoseCurrentArchive,
                    replace,
                    Event::Materialize,
                    Event::Reopen,
                ],
                Self::RemoveBacked => vec![remove, Event::Materialize],
                Self::RemoveAfterLoss => {
                    vec![Event::LoseCurrentArchive, remove, Event::Materialize]
                }
                Self::RemoveAfterLossAndReadDenial => vec![
                    Event::LoseCurrentArchive,
                    Event::DenyReads,
                    remove,
                    Event::Materialize,
                    Event::AllowReads,
                    Event::Materialize,
                ],
                Self::RemoveAfterLossAndReopen => vec![
                    Event::LoseCurrentArchive,
                    remove,
                    Event::Materialize,
                    Event::Reopen,
                ],
            }
        }
    }

    struct Authored {
        id: ChangeId,
        bytes: Vec<u8>,
    }

    struct Run {
        rig: Rig,
        folder: PathBuf,
        handle: Option<ProjectHandle>,
        authored: [Authored; 2],
        reference: Reference,
        log: Vec<Event>,
    }

    impl Run {
        fn new() -> Self {
            let (rig, folder) = titus();
            let handle = open(&rig, &folder);
            let first = b"first authored recording retained in History".to_vec();
            let second = b"second authored recording with different exact bytes".to_vec();
            arrive(&rig, &handle, &folder, SONG, &first);
            leave(&handle);
            handle.wait_idle();
            let first_heads = handle.snapshot().fold().heads(&song_key());
            assert_eq!(first_heads.len(), 1);
            arrive(&rig, &handle, &folder, SONG, &second);
            leave(&handle);
            handle.wait_idle();
            let second_heads = handle.snapshot().fold().heads(&song_key());
            assert_eq!(second_heads.len(), 1);
            let run = Self {
                rig,
                folder,
                handle: Some(handle),
                authored: [
                    Authored {
                        id: first_heads[0],
                        bytes: first,
                    },
                    Authored {
                        id: second_heads[0],
                        bytes: second,
                    },
                ],
                reference: Reference {
                    ingredient: Ingredient::File(Version::B),
                    archives: [Archive::Readable; 2],
                    access: Access::Readable,
                    goal: Goal::Place(Version::B),
                },
                log: Vec::new(),
            };
            run.assert_expected();
            run.assert_conserved();
            run
        }

        fn handle(&self) -> &ProjectHandle {
            self.handle.as_ref().unwrap()
        }

        /// A cached object ID or an exists() result does not stand for bytes on disk.
        fn archive_bytes(&self, version: Version) -> Option<Vec<u8>> {
            let bytes = &self.authored[version.index()].bytes;
            let oid = git2::Oid::hash_object(git2::ObjectType::Blob, bytes).unwrap();
            let repo = git2::Repository::open(git_dir_of(&self.rig.fs, &self.folder)).unwrap();
            match repo.find_blob(oid) {
                Ok(blob) => {
                    assert_eq!(
                        blob.content(),
                        bytes,
                        "wrong archived bytes after {:?}",
                        self.log
                    );
                    Some(blob.content().to_vec())
                }
                Err(error) if error.code() == git2::ErrorCode::NotFound => None,
                Err(error) => panic!("archive read failed after {:?}: {error}", self.log),
            }
        }

        fn assert_conserved(&self) {
            let snapshot = self.handle().snapshot();
            let history = snapshot.fold().history(&song_key());
            let ingredient = self.rig.fs.peek(&self.folder.join(SONG));
            for version in [Version::A, Version::B] {
                let authored = &self.authored[version.index()];
                assert!(
                    history.contains(&authored.id),
                    "History lost {version:?} after {:?}",
                    self.log
                );
                let oid = git2::Oid::hash_object(git2::ObjectType::Blob, &authored.bytes).unwrap();
                let record = Record::CarriedFile {
                    path: RepoPath::parse(SONG).unwrap(),
                    value: CarriedValue::Present {
                        oid: GitOid::parse(&oid.to_string()).unwrap(),
                        size: authored.bytes.len() as u64,
                        md5: Md5::of(&authored.bytes),
                    },
                };
                assert_eq!(
                    snapshot.fold().body(&authored.id).map(|body| body.record),
                    Some(record),
                    "History's {version:?} must name its exact independently authored blob after {:?}",
                    self.log,
                );
                assert!(
                    self.archive_bytes(version).is_some()
                        || ingredient.as_deref() == Some(authored.bytes.as_slice()),
                    "the last exact byte copy of {version:?} was lost after {:?}",
                    self.log
                );
            }
        }

        fn assert_expected(&self) {
            let ingredient = self.rig.fs.peek(&self.folder.join(SONG));
            match self.reference.ingredient {
                Ingredient::Missing => assert!(ingredient.is_none(), "{:?}", self.log),
                Ingredient::File(version) => assert_eq!(
                    ingredient.as_deref(),
                    Some(self.authored[version.index()].bytes.as_slice()),
                    "ingredient differs from reference after {:?}",
                    self.log
                ),
            }
            for version in [Version::A, Version::B] {
                let readable = self.archive_bytes(version).is_some();
                assert_eq!(
                    readable,
                    self.reference.archives[version.index()] == Archive::Readable,
                    "archive differs from reference for {version:?} after {:?}",
                    self.log
                );
            }
        }

        fn step(&mut self, event: Event) {
            self.log.push(event);
            let before = self.reference;
            match event {
                Event::LoseCurrentArchive => {
                    let Ingredient::File(version) = before.ingredient else {
                        panic!("archive loss needs a surviving ingredient");
                    };
                    assert!(self.archive_bytes(version).is_some(), "archive loss setup");
                    remove_song_blob(
                        &self.rig,
                        &self.folder,
                        &self.authored[version.index()].bytes,
                    );
                    assert!(
                        self.archive_bytes(version).is_none(),
                        "archive was not physically lost"
                    );
                }
                Event::DenyReads => self.rig.fs.set_unreadable(&self.folder.join(SONG), true),
                Event::AllowReads => self.rig.fs.set_unreadable(&self.folder.join(SONG), false),
                Event::Request(goal) => {
                    let shown = self.handle().mint(&song_key(), None).unwrap();
                    let command = match goal {
                        Goal::Place(version) => {
                            assert!(
                                self.archive_bytes(version).is_some(),
                                "restore target setup"
                            );
                            Command::Restore {
                                shown,
                                from: self.authored[version.index()].id,
                            }
                        }
                        Goal::Remove => Command::RemoveCarriedFile { shown },
                    };
                    self.handle().submit(command).wait().unwrap();
                }
                Event::Materialize => {
                    leave(self.handle());
                    self.handle().wait_idle();
                }
                Event::Reopen => {
                    // Every destructive round was checked before this checkpoint can rescue it.
                    self.assert_expected();
                    self.assert_conserved();
                    assert_eq!(before.access, Access::Readable);
                    self.handle.take().unwrap().close().unwrap();
                    self.handle = Some(open(&self.rig, &self.folder));
                }
            }
            self.reference = next(before, event);
            self.assert_conserved();
            match event {
                Event::Materialize | Event::Reopen => {
                    self.assert_expected();
                    if let Ingredient::File(old) = before.ingredient
                        && before.ingredient != self.reference.ingredient
                    {
                        assert!(
                            self.archive_bytes(old).is_some(),
                            "destroyed ingredient has no independent History bytes before checkpoint"
                        );
                    }
                    assert_valid_burrito(&self.rig.fs, &self.folder);
                }
                // Request may already have queued a background materialization. Its global
                // conservation obligation holds even before the explicit leave joins it.
                Event::LoseCurrentArchive
                | Event::DenyReads
                | Event::AllowReads
                | Event::Request(_) => {}
            }
        }

        fn rounds(mut self, rounds: &[Round]) {
            for round in rounds {
                let current = match self.reference.ingredient {
                    Ingredient::File(version) => version,
                    Ingredient::Missing => {
                        self.step(Event::Request(Goal::Place(Version::A)));
                        self.step(Event::Materialize);
                        Version::A
                    }
                };
                for event in round.events(current) {
                    self.step(event);
                }
            }
            self.assert_conserved();
            self.assert_expected();
            self.handle.take().unwrap().close().unwrap();
            assert_valid_burrito(&self.rig.fs, &self.folder);
            assert_commits_valid(&self.rig.fs, &self.folder);
        }
    }

    #[test]
    fn every_reference_transition_preserves_the_last_copy() {
        let mut checked = 0;
        for ingredient in [
            Ingredient::File(Version::A),
            Ingredient::File(Version::B),
            Ingredient::Missing,
        ] {
            for a in [Archive::Readable, Archive::Lost] {
                for b in [Archive::Readable, Archive::Lost] {
                    for access in [Access::Readable, Access::Denied] {
                        for goal in [
                            Goal::Place(Version::A),
                            Goal::Place(Version::B),
                            Goal::Remove,
                        ] {
                            let before = Reference {
                                ingredient,
                                archives: [a, b],
                                access,
                                goal,
                            };
                            for event in [
                                Event::LoseCurrentArchive,
                                Event::DenyReads,
                                Event::AllowReads,
                                Event::Request(Goal::Place(Version::A)),
                                Event::Request(Goal::Place(Version::B)),
                                Event::Request(Goal::Remove),
                                Event::Materialize,
                                Event::Reopen,
                            ] {
                                checked += 1;
                                let after = next(before, event);
                                match event {
                                    Event::Materialize | Event::Reopen => {
                                        if access == Access::Denied {
                                            assert_eq!(after, before);
                                        }
                                        if let Ingredient::File(old) = ingredient
                                            && after.ingredient != ingredient
                                        {
                                            assert_eq!(
                                                after.archives[old.index()],
                                                Archive::Readable
                                            );
                                        }
                                    }
                                    Event::LoseCurrentArchive => {
                                        assert_eq!(after.ingredient, ingredient)
                                    }
                                    Event::DenyReads | Event::AllowReads | Event::Request(_) => {
                                        assert_eq!(after.ingredient, ingredient);
                                        assert_eq!(after.archives, before.archives);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(checked, 576);
    }

    fn a_round() -> impl Strategy<Value = Round> {
        prop_oneof![
            Just(Round::ReplaceBacked),
            Just(Round::ReplaceAfterLoss),
            Just(Round::ReplaceAfterLossAndReadDenial),
            Just(Round::ReplaceAfterLossAndReopen),
            Just(Round::RemoveBacked),
            Just(Round::RemoveAfterLoss),
            Just(Round::RemoveAfterLossAndReadDenial),
            Just(Round::RemoveAfterLossAndReopen),
        ]
    }

    #[test]
    fn a_temporarily_unreadable_carried_file_defers_and_retries_its_replacement() {
        Run::new().rounds(&[Round::ReplaceAfterLossAndReadDenial]);
    }

    #[test]
    fn bounded_carried_histories_preserve_independent_old_bytes() {
        // Permanent seeds make the bounded test detect both original destructive paths.
        a_replaced_carried_file_recovers_its_lost_old_blob_before_overwrite();
        a_removed_carried_file_recovers_its_lost_blob_before_unlink();
        // Guarantee every declared event family in both current-version directions;
        // random samples extend these seeds rather than standing for reachability.
        for round in [
            Round::ReplaceBacked,
            Round::ReplaceAfterLoss,
            Round::ReplaceAfterLossAndReadDenial,
            Round::ReplaceAfterLossAndReopen,
            Round::RemoveBacked,
            Round::RemoveAfterLoss,
            Round::RemoveAfterLossAndReadDenial,
            Round::RemoveAfterLossAndReopen,
        ] {
            Run::new().rounds(&[round]);
            Run::new().rounds(&[Round::ReplaceBacked, round]);
        }
        let config = ProptestConfig {
            cases: 32,
            max_shrink_iters: 128,
            failure_persistence: None,
            ..ProptestConfig::default()
        };
        proptest!(config, |(rounds in proptest::collection::vec(a_round(), 1..=4))| {
            Run::new().rounds(&rounds);
        });
    }
}

fn corrupt_song_blob(rig: &Rig, folder: &Path, bytes: &[u8]) -> (PathBuf, Vec<u8>) {
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
    assert_eq!(rig.fs.peek(&folder.join(SONG)).as_deref(), Some(bytes));
    (loose, encoded)
}

#[test]
fn a_carried_replacement_waits_when_the_old_backup_blob_is_corrupt() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let first = b"the valid first recording requested by Restore";
    let old = b"the current recording whose ingredient is its only readable copy";
    arrive(&rig, &handle, &folder, SONG, first);
    leave(&handle);
    let first_heads = handle.snapshot().fold().heads(&song_key());
    assert_eq!(first_heads.len(), 1);
    let first_id = first_heads[0];
    arrive(&rig, &handle, &folder, SONG, old);
    leave(&handle);
    let old_heads = handle.snapshot().fold().heads(&song_key());
    assert_eq!(old_heads.len(), 1);
    let old_id = old_heads[0];
    let (loose, encoded) = corrupt_song_blob(&rig, &folder, old);
    handle
        .submit(Command::Restore {
            shown: handle.mint(&song_key(), None).unwrap(),
            from: first_id,
        })
        .wait()
        .unwrap();
    leave(&handle);
    assert_eq!(
        rig.fs.peek(&folder.join(SONG)).as_deref(),
        Some(old.as_slice()),
        "the valid replacement must wait before destroying the only readable old bytes"
    );
    assert!(
        handle
            .snapshot()
            .fold()
            .history(&song_key())
            .contains(&old_id)
    );
    assert_valid_burrito(&rig.fs, &folder);

    // The refusal above precedes a checkpoint. Repairing the external damage lets the
    // same requested action finish without another command or any loss of History.
    std::fs::write(&loose, &encoded).unwrap();
    leave(&handle);
    handle.wait_idle();
    assert_eq!(
        rig.fs.peek(&folder.join(SONG)).as_deref(),
        Some(first.as_slice())
    );
    let old_oid = git2::Oid::hash_object(git2::ObjectType::Blob, old).unwrap();
    assert_eq!(
        repo_of(&rig.fs, &folder)
            .find_blob(old_oid)
            .unwrap()
            .content(),
        old
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}

#[test]
fn a_carried_removal_waits_when_its_backup_blob_is_corrupt() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let old = b"the recording removed from the folder only after History has readable bytes";
    arrive(&rig, &handle, &folder, SONG, old);
    leave(&handle);
    let old_heads = handle.snapshot().fold().heads(&song_key());
    assert_eq!(old_heads.len(), 1);
    let old_id = old_heads[0];
    let (loose, encoded) = corrupt_song_blob(&rig, &folder, old);
    handle
        .submit(Command::RemoveCarriedFile {
            shown: handle.mint(&song_key(), None).unwrap(),
        })
        .wait()
        .unwrap();
    leave(&handle);
    assert_eq!(
        rig.fs.peek(&folder.join(SONG)).as_deref(),
        Some(old.as_slice()),
        "the explicit removal must wait before unlinking the only readable bytes"
    );
    assert!(matches!(
        handle.snapshot().fold().projected(&song_key()),
        Some(Record::CarriedFile {
            value: CarriedValue::Removed,
            ..
        })
    ));
    assert!(
        handle
            .snapshot()
            .fold()
            .history(&song_key())
            .contains(&old_id)
    );
    assert_valid_burrito(&rig.fs, &folder);

    std::fs::write(&loose, &encoded).unwrap();
    leave(&handle);
    handle.wait_idle();
    assert!(rig.fs.peek(&folder.join(SONG)).is_none());
    let old_oid = git2::Oid::hash_object(git2::ObjectType::Blob, old).unwrap();
    assert_eq!(
        repo_of(&rig.fs, &folder)
            .find_blob(old_oid)
            .unwrap()
            .content(),
        old
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}
