//! Placement preserves outside versions before replacing or removing their files.

use crate::common::*;
use std::collections::BTreeSet;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tc_journal::command::{Command, SeedSource};
use tc_journal::fold::{ConflictKind, Notice};
use tc_journal::keys::SettingKey;
use tc_journal::record::{CarriedValue, Scope, VerseValue};
use tc_journal::{
    BookId, ChapterNo, Json, JsonMap, Md5, MetaField, Record, RecordKey, RepoPath, VerseNo,
};
use tc_store::fault::FaultFs;
use tc_store::vfs::{
    DirEntry, FileMeta, LockHandle, ReadHandle, WatchGuard, WatchSink, WriteHandle,
};
use tc_store::{Leave, OpenOptions, Point, ProjectHandle, Store, Vfs};

const METADATA: &str = "metadata.json";
const SONG: &str = "ingredients/audio/song.mp3";
const BOOK: &str = "ingredients/TIT.usfm";

fn open(rig: &Rig, folder: &Path) -> ProjectHandle {
    let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle
}

fn name_value(name: &str) -> Json {
    Json::Obj(JsonMap::from([("en".to_owned(), Json::from(name))]))
}

fn set_name(handle: &ProjectHandle, name: &str) {
    let key = RecordKey::ProjectMeta(MetaField::Name);
    handle
        .submit(Command::SetMeta {
            shown: handle.mint(&key, None).unwrap(),
            value: Some(name_value(name)),
        })
        .wait()
        .unwrap();
}

fn renamed_metadata(rig: &Rig, folder: &Path, name: &str) -> Vec<u8> {
    let bytes = rig.fs.peek(&folder.join(METADATA)).unwrap();
    let mut document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    document["identification"]["name"]["en"] = name.into();
    serde_json::to_vec_pretty(&document).unwrap()
}

fn assert_name_in_history(handle: &ProjectHandle, name: &str) {
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    let key = RecordKey::ProjectMeta(MetaField::Name);
    let wanted = Record::ProjectMeta {
        field: MetaField::Name,
        value: Some(name_value(name)),
    };
    assert!(
        fold.history(&key)
            .iter()
            .filter_map(|id| fold.body(id))
            .any(|body| body.record == wanted),
        "the project lost the name {name:?}; notices: {:?}",
        handle.notices()
    );
}

fn assert_metadata_copy(rig: &Rig, handle: &ProjectHandle, folder: &Path, bytes: &[u8]) {
    let copies = copies_of(handle, METADATA);
    assert!(
        copies
            .iter()
            .any(|copy| { rig.fs.peek(&folder.join(copy)).as_deref() == Some(bytes) }),
        "no notice leads to a complete copy of the metadata bytes; copies: {copies:?}"
    );
}

fn song_key() -> RecordKey {
    RecordKey::CarriedFile(RepoPath::parse(SONG).unwrap())
}

fn ingest_song(
    rig: &Rig,
    handle: &ProjectHandle,
    folder: &Path,
    bytes: &[u8],
) -> tc_journal::ChangeId {
    rig.fs.put_unseen(&folder.join(SONG), bytes, None);
    handle.focus().wait().unwrap();
    handle.wait_idle();
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    assert_eq!(rig.fs.peek(&folder.join(SONG)).as_deref(), Some(bytes));
    assert_valid_burrito(&rig.fs, folder);
    let snapshot = handle.snapshot();
    let heads = snapshot.fold().heads(&song_key());
    assert_eq!(
        heads.len(),
        1,
        "the setup must have one settled carried version"
    );
    heads[0]
}

/// Read sharing and rename sharing are separate faults. A successful rename must not let
/// the app replace metadata it was unable to read and preserve first.
#[test]
fn unreadable_but_writable_metadata_keeps_the_outside_name_until_it_can_be_read() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let outside_name = "name typed in another editor while unreadable";
    let inside_name = "name saved in translationCore while unreadable";
    let edited = renamed_metadata(&rig, &folder, outside_name);
    let path = folder.join(METADATA);
    let book = folder.join("ingredients/TIT.usfm");
    let book_before = rig.fs.peek(&book).unwrap();
    let saved_words = "words saved while metadata cannot be read";
    rig.fs.put_unseen(&path, &edited, None);
    rig.fs.set_unreadable(&path, true);
    // Do not also set_busy: the regression requires rename to remain possible.
    set_name(&handle, inside_name);
    save_verse(&handle, 1, 1, saved_words).unwrap();
    let _ = handle.leave(Leave::Mode).wait();
    handle.wait_idle();
    let while_unreadable = rig.fs.peek(&path);
    let book_while_unreadable = rig.fs.peek(&book);
    let saved_while_unreadable = words(&handle, 1, 1);
    let problems_while_unreadable = burrito_problems(&rig.fs, &folder);
    rig.fs.set_unreadable(&path, false);
    assert_eq!(
        while_unreadable,
        Some(edited.clone()),
        "the app renamed over bytes it could not read"
    );
    assert_eq!(saved_while_unreadable.as_deref(), Some(saved_words));
    assert_eq!(
        book_while_unreadable,
        Some(book_before),
        "the group must defer ingredient placement with unreadable metadata"
    );
    assert!(
        problems_while_unreadable.is_empty(),
        "the blocked job broke the Burrito: {problems_while_unreadable:#?}"
    );
    handle.focus().wait().unwrap();
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    assert_name_in_history(&handle, outside_name);
    assert_name_in_history(&handle, inside_name);
    assert_eq!(words(&handle, 1, 1).as_deref(), Some(saved_words));
    assert_metadata_copy(&rig, &handle, &folder, &edited);
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
    let reopened = open(&rig, &folder);
    assert_name_in_history(&reopened, outside_name);
    assert_name_in_history(&reopened, inside_name);
    assert_eq!(words(&reopened, 1, 1).as_deref(), Some(saved_words));
    assert_valid_burrito(&rig.fs, &folder);
    reopened.close().unwrap();
}

/// A removal command changes the fold at once, but its group's unlink must wait until
/// metadata can safely replace the listing that still names the recording on disk.
#[test]
fn carried_removal_waits_with_its_old_listing_while_metadata_is_unreadable() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let original = b"the established recording before its removal".to_vec();
    ingest_song(&rig, &handle, &folder, &original);
    let path = folder.join(METADATA);
    let metadata_before = rig.fs.peek(&path).unwrap();
    rig.fs.set_unreadable(&path, true);
    handle
        .submit(Command::RemoveCarriedFile {
            shown: handle.mint(&song_key(), None).unwrap(),
        })
        .wait()
        .unwrap();
    let _ = handle.leave(Leave::Mode).wait();
    handle.wait_idle();
    let removal_is_saved = matches!(
        handle.snapshot().fold().projected(&song_key()),
        Some(Record::CarriedFile {
            value: CarriedValue::Removed,
            ..
        })
    );
    let song_while_unreadable = rig.fs.peek(&folder.join(SONG));
    let metadata_while_unreadable = rig.fs.peek(&path);
    let problems_while_unreadable = burrito_problems(&rig.fs, &folder);
    rig.fs.set_unreadable(&path, false);
    assert!(
        removal_is_saved,
        "deferring placement must not refuse the saved removal"
    );
    assert_eq!(
        song_while_unreadable,
        Some(original),
        "unlink must defer until the group can replace metadata safely"
    );
    assert_eq!(metadata_while_unreadable, Some(metadata_before));
    assert!(
        problems_while_unreadable.is_empty(),
        "the blocked removal broke its old Burrito listing: {problems_while_unreadable:#?}"
    );
    handle.focus().wait().unwrap();
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    assert_eq!(rig.fs.peek(&folder.join(SONG)), None);
    let metadata_after: serde_json::Value =
        serde_json::from_slice(&rig.fs.peek(&path).unwrap()).unwrap();
    assert!(metadata_after["ingredients"].get(SONG).is_none());
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
}

/// Restoring a known recorded blob changes no outside file while metadata is denied.
/// This isolates the placement ordering from outside-edit detection and preservation.
#[test]
fn carried_restore_waits_with_its_old_listing_while_metadata_is_unreadable() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let first = b"the first recording, available in History".to_vec();
    let second = b"the second recording, currently placed, with a different size".to_vec();
    let first_id = ingest_song(&rig, &handle, &folder, &first);
    let second_id = ingest_song(&rig, &handle, &folder, &second);
    assert_ne!(first_id, second_id);
    let path = folder.join(METADATA);
    let metadata_before = rig.fs.peek(&path).unwrap();
    rig.fs.set_unreadable(&path, true);
    handle
        .submit(Command::Restore {
            shown: handle.mint(&song_key(), None).unwrap(),
            from: first_id,
        })
        .wait()
        .unwrap();
    let _ = handle.leave(Leave::Mode).wait();
    handle.wait_idle();
    let restore_is_saved = matches!(
        handle.snapshot().fold().projected(&song_key()),
        Some(Record::CarriedFile {
            value: CarriedValue::Present { md5, .. },
            ..
        }) if md5 == Md5::of(&first)
    );
    let song_while_unreadable = rig.fs.peek(&folder.join(SONG));
    let metadata_while_unreadable = rig.fs.peek(&path);
    let problems_while_unreadable = burrito_problems(&rig.fs, &folder);
    rig.fs.set_unreadable(&path, false);
    assert!(
        restore_is_saved,
        "deferring placement must not refuse the saved restore"
    );
    assert_eq!(
        song_while_unreadable,
        Some(second),
        "carried placement ran before metadata was safe to replace"
    );
    assert_eq!(metadata_while_unreadable, Some(metadata_before));
    assert!(
        problems_while_unreadable.is_empty(),
        "the blocked restore broke its old Burrito listing: {problems_while_unreadable:#?}"
    );
    handle.focus().wait().unwrap();
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    assert_eq!(rig.fs.peek(&folder.join(SONG)), Some(first));
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    assert!(fold.history(&song_key()).contains(&first_id));
    assert!(fold.history(&song_key()).contains(&second_id));
    assert_eq!(fold.conflicts_of(ConflictKind::CarriedFile).count(), 0);
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
}

/// A settled stat is useful when nothing is being replaced. It cannot stand in for the
/// bytes when a new project detail is about to replace the file.
#[test]
fn a_same_size_same_mtime_edit_of_settled_metadata_is_kept_before_replacement() {
    let (rig, folder) = titus();
    settle_entries(&rig, &folder);
    let handle = open(&rig, &folder);
    let path = folder.join(METADATA);
    let manifest: serde_json::Value =
        serde_json::from_slice(&rig.fs.peek(&folder.join(".tc/materialized.json")).unwrap())
            .unwrap();
    assert_eq!(
        manifest["paths"][METADATA]["placed"]["settled"], true,
        "the precondition must exercise the settled fast path"
    );
    let old_stat = rig.fs.metadata(&path).unwrap();
    let original = rig.fs.peek(&path).unwrap();
    let from = "Titus test translation";
    let outside_name = "titus test translation";
    let text = String::from_utf8(original.clone()).unwrap();
    assert!(text.contains(from));
    let edited = text.replacen(from, outside_name, 1).into_bytes();
    assert_eq!(edited.len(), original.len());
    rig.fs.put_unseen(&path, &edited, Some(old_stat.mtime_ns));
    let new_stat = rig.fs.metadata(&path).unwrap();
    assert_eq!(new_stat.len, old_stat.len);
    assert_eq!(new_stat.mtime_ns, old_stat.mtime_ns);
    let inside_name = "name changed in the app after the hidden disk edit";
    set_name(&handle, inside_name);
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    assert_name_in_history(&handle, outside_name);
    assert_name_in_history(&handle, inside_name);
    assert_metadata_copy(&rig, &handle, &folder, &edited);
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
}

/// A failed byte copy has not kept the bytes. Its digest must remain eligible for the
/// next focus, rather than being remembered as an already completed preservation.
#[test]
fn metadata_copy_failure_is_retried_after_writes_resume() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let outside_name = "an outside name whose first byte copy fails";
    let edited = renamed_metadata(&rig, &folder, outside_name);
    let path = folder.join(METADATA);
    rig.fs.put_unseen(&path, &edited, None);
    rig.fs.fail_writes(Some(ErrorKind::StorageFull));
    let first = handle.focus().wait();
    let after_failed_copy = rig.fs.peek(&path);
    let copies_after_failure = copies_of(&handle, METADATA);
    rig.fs.fail_writes(None);
    assert!(first.is_err(), "the first preservation must actually fail");
    assert_eq!(after_failed_copy, Some(edited.clone()));
    assert!(copies_after_failure.is_empty());
    assert_valid_burrito(&rig.fs, &folder);
    handle.focus().wait().unwrap();
    handle.wait_idle();
    assert_metadata_copy(&rig, &handle, &folder, &edited);
    assert_name_in_history(&handle, outside_name);
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
    let reopened = open(&rig, &folder);
    assert_name_in_history(&reopened, outside_name);
    assert_valid_burrito(&rig.fs, &folder);
    reopened.close().unwrap();
}

/// All names the current bounded search can consider are occupied. Refusing the second
/// edit or choosing a fresh name is safe; replacing any existing byte copy is not.
#[test]
fn external_copy_namespace_exhaustion_never_replaces_an_existing_copy() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let first_edit = renamed_metadata(&rig, &folder, "first outside name");
    rig.fs.put_unseen(&folder.join(METADATA), &first_edit, None);
    handle.focus().wait().unwrap();
    handle.wait_idle();
    let copies = copies_of(&handle, METADATA);
    assert_eq!(copies.len(), 1, "the base comes from the real first notice");
    let base = copies[0].clone();
    assert_eq!(rig.fs.peek(&folder.join(&base)), Some(first_edit.clone()));
    let mut occupied = Vec::new();
    for suffix in 2..=999 {
        let rel = format!("{base}-{suffix}");
        let sentinel = format!("earlier translator bytes in occupied copy {suffix}").into_bytes();
        rig.fs.put_unseen(&folder.join(&rel), &sentinel, None);
        occupied.push((rel, sentinel));
    }
    let second_edit = renamed_metadata(&rig, &folder, "second outside name");
    rig.fs
        .put_unseen(&folder.join(METADATA), &second_edit, None);
    let _ = handle.focus().wait();
    handle.wait_idle();
    assert_eq!(rig.fs.peek(&folder.join(&base)), Some(first_edit));
    let overwritten: Vec<_> = occupied
        .iter()
        .filter(|(rel, sentinel)| rig.fs.peek(&folder.join(rel)).as_ref() != Some(sentinel))
        .map(|(rel, _)| rel)
        .collect();
    assert!(
        overwritten.is_empty(),
        "copy-name exhaustion overwrote earlier work: {overwritten:?}"
    );
    assert!(
        rig.fs.peek(&folder.join(METADATA)).as_ref() == Some(&second_edit)
            || files(&rig.fs, &folder)
                .iter()
                .any(|(rel, bytes)| { rel.starts_with(".tc/external/") && bytes == &second_edit }),
        "the second edit must still be on disk, either at its path or in a fresh copy"
    );
    assert_valid_burrito(&rig.fs, &folder);
    // A safe refusal can also refuse close's materialization while the namespace is full.
    let _ = handle.close();
}

/// The settings rename supplies a deterministic pause after M1 selected the carried file
/// for removal and before the unlink loop. No watcher or later focus can rescue bytes
/// already unlinked in that interval.
#[test]
fn a_carried_edit_after_removal_check_is_preserved_before_unlink() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let original = b"the recording the remove command saw".to_vec();
    let edited = b"new recording made externally after M1, before unlink".to_vec();
    let song = folder.join(SONG);
    rig.fs.put_unseen(&song, &original, None);
    handle.focus().wait().unwrap();
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    handle
        .submit(Command::RemoveCarriedFile {
            shown: handle.mint(&song_key(), None).unwrap(),
        })
        .wait()
        .unwrap();
    let setting = RecordKey::Setting(SettingKey::parse("textDirection").unwrap());
    handle
        .submit(Command::SetSetting {
            shown: handle.mint(&setting, None).unwrap(),
            value: Some(Json::from("rtl")),
        })
        .wait()
        .unwrap();
    let hooks = rig.store.hooks().clone();
    hooks.hold(Point::BeforeRename);
    rig.fs.clear_log();
    let materialized = handle.leave(Leave::Mode);
    hooks.wait_reached(Point::BeforeRename, 1);
    let removal_was_read = rig.fs.opened().contains(&song);
    let before_edit = rig.fs.peek(&song);
    rig.fs.put_unseen(&song, &edited, None);
    hooks.release(Point::BeforeRename);
    materialized.wait().unwrap();
    handle.wait_idle();
    assert!(
        removal_was_read,
        "the race must follow M1's read of the removal"
    );
    assert_eq!(before_edit, Some(original));
    assert_eq!(
        rig.fs.peek(&song),
        Some(edited.clone()),
        "the unlink removed bytes that arrived after its comparison"
    );
    handle.focus().wait().unwrap();
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    assert_eq!(copies_of(&handle, SONG).len(), 1);
    assert!(handle.notices().iter().any(|notice| matches!(
        &notice.notice,
        Notice::OutsideEditHeld { path, .. } if path.as_str() == SONG
    )));
    assert!(handle.take_in_held(SONG).wait().unwrap().batch.is_some());
    handle.wait_idle();
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    assert!(
        fold.history(&song_key())
            .iter()
            .filter_map(|id| fold.body(id))
            .any(|body| matches!(
                body.record,
                Record::CarriedFile {
                    value: CarriedValue::Present { md5, .. },
                    ..
                } if md5 == Md5::of(&edited)
            )),
        "the external version is absent from readable history"
    );
    assert_eq!(
        fold.conflicts_of(ConflictKind::CarriedFile).count(),
        0,
        "the explicit take-in choice settles the held removal"
    );
    assert!(
        fold.history(&song_key()).iter().any(|id| matches!(
            fold.body(id).map(|body| body.record),
            Some(Record::CarriedFile {
                value: CarriedValue::Removed,
                ..
            })
        )),
        "the take-in choice must retain the earlier removal in History"
    );
    let repo = repo_of(&rig.fs, &folder);
    let oid = git2::Oid::hash_object(git2::ObjectType::Blob, &edited).unwrap();
    assert_eq!(repo.find_blob(oid).unwrap().content(), edited.as_slice());
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
}

// Drafts to append inside tests/store/placement_regressions.rs; they use that module's
// existing imports and helpers. No implementation changes are included here.

/// A valid old listing and a pending carried restore must stay together until an outside
/// metadata version has been preserved. Exhaustion makes preservation fail deterministically
/// after a carried rename would otherwise have succeeded.
#[test]
fn a_carried_restore_waits_until_outside_metadata_can_be_preserved() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let first = b"the first recording retained in readable History".to_vec();
    let second = b"the second recording currently placed; deliberately a different size".to_vec();
    let first_id = ingest_song(&rig, &handle, &folder, &first);
    let second_id = ingest_song(&rig, &handle, &folder, &second);
    assert_ne!(first_id, second_id);

    let seed_name = "outside name that establishes the preservation basename";
    let seed_edit = renamed_metadata(&rig, &folder, seed_name);
    let metadata_path = folder.join(METADATA);
    rig.fs.put_unseen(&metadata_path, &seed_edit, None);
    handle.focus().wait().unwrap();
    handle.wait_idle();
    assert_name_in_history(&handle, seed_name);
    assert_metadata_copy(&rig, &handle, &folder, &seed_edit);
    let copies = copies_of(&handle, METADATA);
    assert_eq!(
        copies.len(),
        1,
        "the clock must yield one preservation basename"
    );
    let base = copies[0].clone();
    let mut occupied = Vec::new();
    for suffix in 2..=999 {
        let rel = format!("{base}-{suffix}");
        let sentinel =
            format!("earlier translator copy {suffix}, kept before restore").into_bytes();
        rig.fs.put_unseen(&folder.join(&rel), &sentinel, None);
        occupied.push((rel, sentinel));
    }

    let hooks = rig.store.hooks().clone();
    hooks.hold(Point::BeforeMaterialize);
    handle
        .submit(Command::Restore {
            shown: handle.mint(&song_key(), None).unwrap(),
            from: first_id,
        })
        .wait()
        .unwrap();
    let outside_name = "outside metadata name whose preservation is temporarily impossible";
    let outside = renamed_metadata(&rig, &folder, outside_name);
    rig.fs.put_unseen(&metadata_path, &outside, None);
    let materialized = handle.leave(Leave::Mode);
    hooks.wait_reached(Point::BeforeMaterialize, 1);
    hooks.release(Point::BeforeMaterialize);
    let first_attempt = materialized.wait();
    handle.wait_idle();

    let recording_after_refusal = rig.fs.peek(&folder.join(SONG));
    let metadata_after_refusal = rig.fs.peek(&metadata_path);
    let listing_after_refusal = burrito_problems(&rig.fs, &folder);
    assert_eq!(
        recording_after_refusal,
        Some(second.clone()),
        "carried bytes changed before metadata preservation succeeded: {first_attempt:?}"
    );
    assert_eq!(metadata_after_refusal, Some(outside.clone()));
    assert!(
        listing_after_refusal.is_empty(),
        "preservation refusal broke the old ingredient listing: {listing_after_refusal:#?}"
    );
    assert!(
        matches!(
            handle.snapshot().fold().projected(&song_key()),
            Some(Record::CarriedFile {
                value: CarriedValue::Present { md5, .. },
                ..
            }) if md5 == Md5::of(&first)
        ),
        "the restore itself must remain saved while placement waits"
    );
    for (rel, sentinel) in &occupied {
        assert_eq!(rig.fs.peek(&folder.join(rel)).as_ref(), Some(sentinel));
    }

    // Keep the last occupied copy at another name, freeing a candidate for the retry.
    rig.fs
        .rename(
            &folder.join(format!("{base}-999")),
            &folder.join(format!("{base}-1000")),
        )
        .unwrap();
    handle.focus().wait().unwrap();
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    assert_name_in_history(&handle, seed_name);
    assert_name_in_history(&handle, outside_name);
    assert_metadata_copy(&rig, &handle, &folder, &outside);
    assert_eq!(rig.fs.peek(&folder.join(SONG)), Some(first.clone()));
    let snapshot = handle.snapshot();
    let history = snapshot.fold().history(&song_key());
    assert!(history.contains(&first_id));
    assert!(history.contains(&second_id));
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
    let reopened = open(&rig, &folder);
    assert_name_in_history(&reopened, seed_name);
    assert_name_in_history(&reopened, outside_name);
    assert_eq!(rig.fs.peek(&folder.join(SONG)), Some(first));
    assert_valid_burrito(&rig.fs, &folder);
    reopened.close().unwrap();
}

/// Metadata becomes unreadable after the group read it, while an ingredient temp is ready.
/// The final metadata callback must leave its bytes alone and retain enough dirty work for
/// the interrupted listing to become valid when reads resume.
#[test]
fn metadata_denied_after_group_preflight_is_preserved_and_retried() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let metadata_path = folder.join(METADATA);
    let book_path = folder.join("ingredients/TIT.usfm");
    let metadata_before = rig.fs.peek(&metadata_path).unwrap();
    let book_before = rig.fs.peek(&book_path).unwrap();
    let saved_words = "verse saved before metadata read sharing was withdrawn";
    save_verse(&handle, 1, 1, saved_words).unwrap();
    let hooks = rig.store.hooks().clone();
    hooks.hold(Point::BeforeRename);
    rig.fs.clear_log();
    let materialized = handle.leave(Leave::Book(TIT));
    hooks.wait_reached(Point::BeforeRename, 1);
    let metadata_was_read = rig.fs.opened().contains(&metadata_path);
    rig.fs.set_unreadable(&metadata_path, true);
    hooks.release(Point::BeforeRename);
    let first_attempt = materialized.wait();
    handle.wait_idle();
    let metadata_while_denied = rig.fs.peek(&metadata_path);
    let book_while_denied = rig.fs.peek(&book_path);
    let words_while_denied = words(&handle, 1, 1);
    // Restore reads before assertions so a failing regression does not leave a held path.
    rig.fs.set_unreadable(&metadata_path, false);
    assert!(
        metadata_was_read,
        "denial must follow the group's metadata preflight"
    );
    assert_ne!(
        book_while_denied,
        Some(book_before),
        "the scenario must reach an ingredient rename before the final metadata look"
    );
    assert_eq!(metadata_while_denied, Some(metadata_before));
    assert_eq!(words_while_denied.as_deref(), Some(saved_words));
    first_attempt.unwrap();
    // No new save or leave supplies dirty marks: the interrupted group must retry itself.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !burrito_problems(&rig.fs, &folder).is_empty() {
        assert!(
            std::time::Instant::now() < deadline,
            "the interrupted listing never retried after metadata reads resumed: {:?}",
            handle.notices()
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    handle.wait_idle();
    assert_eq!(words(&handle, 1, 1).as_deref(), Some(saved_words));
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
    let reopened = open(&rig, &folder);
    assert_eq!(words(&reopened, 1, 1).as_deref(), Some(saved_words));
    assert_valid_burrito(&rig.fs, &folder);
    reopened.close().unwrap();
}

/// A copy left after placement but before its directory flush is not durable yet.
/// Reusing it must finish the directory-then-file protocol before replacing its source.
#[test]
fn reusing_an_unflushed_metadata_copy_flushes_its_name_before_its_file() {
    use tc_store::fault::OpKind;

    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let metadata_path = folder.join(METADATA);

    // The first real notice supplies this fixed-clock namespace without duplicating the
    // preservation writer's timestamp or escaping rules in the test.
    let first_edit = renamed_metadata(&rig, &folder, "first outside name before copy recovery");
    rig.fs.put_unseen(&metadata_path, &first_edit, None);
    handle.focus().wait().unwrap();
    handle.wait_idle();
    let copies = copies_of(&handle, METADATA);
    assert_eq!(copies.len(), 1, "the setup must supply one real base copy");
    let base = &copies[0];
    assert_eq!(rig.fs.peek(&folder.join(base)), Some(first_edit.clone()));

    let outside_name = "outside name with an already placed but unflushed byte copy";
    let second_edit = renamed_metadata(&rig, &folder, outside_name);
    let recovered_rel = format!("{base}-2");
    let recovered_path = folder.join(&recovered_rel);
    let copy_dir = recovered_path.parent().unwrap();
    let copy_name = recovered_path.file_name().unwrap().to_str().unwrap();
    let mut unflushed_dirs = tc_store::DirSet::new();
    tc_store::atomic::atomic_create(
        &rig.fs,
        copy_dir,
        copy_name,
        &second_edit,
        tc_store::SyncDir::Deferred(&mut unflushed_dirs),
    )
    .unwrap();
    assert!(!unflushed_dirs.is_empty());
    assert_eq!(rig.fs.peek(&recovered_path), Some(second_edit.clone()));
    assert_eq!(
        rig.fs.crash(true).peek(&recovered_path),
        None,
        "the setup must leave the copy's final name unflushed"
    );
    rig.fs.put_unseen(&metadata_path, &second_edit, None);
    rig.fs.clear_log();

    handle.focus().wait().unwrap();
    // Even when the outside document is already byte-identical to its projection, this
    // authored change must eventually replace it after the reuse finished.
    let inside_name = "app name saved after the preservation copy was recovered";
    set_name(&handle, inside_name);
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    let log = rig.fs.log();
    assert!(
        copies_of(&handle, METADATA).contains(&recovered_rel),
        "the second edit must reuse the preexisting identical copy"
    );
    assert!(
        !log.iter()
            .any(|op| op.kind == OpKind::Rename && op.path == recovered_path),
        "the scenario must reuse the existing copy rather than place a new one"
    );

    let replacement = log
        .iter()
        .position(|op| op.kind == OpKind::Rename && op.path == metadata_path)
        .expect("the preserved outside metadata must subsequently be replaced");
    let copy_dir_flush = log[..replacement]
        .iter()
        .rposition(|op| op.kind == OpKind::SyncDir && op.path == copy_dir)
        .expect("the reused copy's containing directory must be flushed before replacement");
    assert!(
        log[copy_dir_flush + 1..replacement]
            .iter()
            .any(|op| op.kind == OpKind::SyncFile && op.path == recovered_path),
        "the reused preservation copy must flush its file after its final directory flush \
         and before metadata replacement; log: {log:#?}"
    );
    assert_eq!(rig.fs.peek(&folder.join(base)), Some(first_edit));
    assert_eq!(rig.fs.peek(&recovered_path), Some(second_edit.clone()));
    assert_eq!(
        rig.fs.crash(true).peek(&recovered_path),
        Some(second_edit.clone()),
        "a reused copy must be durable before it authorizes source replacement"
    );
    assert_name_in_history(&handle, outside_name);
    assert_name_in_history(&handle, inside_name);
    assert_metadata_copy(&rig, &handle, &folder, &second_edit);
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
}

/// An outside edit of `metadata.json` is pending until the batch that records it is durable
/// (§11.1 D7): written over before then, a power cut would leave the folder no longer
/// differing and nothing to record again (§6.4 X1). The person's name must be in the
/// reopened project's history, or a notice on it must lead to the copy that keeps it.
#[test]
fn an_outside_metadata_name_survives_a_power_cut_before_its_batch_is_durable() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let outside_name = "name typed in another editor";
    let edited = renamed_metadata(&rig, &folder, outside_name);
    let path = folder.join(METADATA);
    rig.fs.put_unseen(&path, &edited, None);
    let hooks = rig.store.hooks().clone();
    // The actor is busy: the edit's batch waits before it becomes durable.
    hooks.hold(Point::BeforeDurablePending);
    handle.focus().wait().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while hooks.reached(Point::BeforeDurablePending) < 1 {
        assert!(
            std::time::Instant::now() < deadline,
            "setup: the outside edit was never sent to be recorded"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let before_cut = rig.fs.peek(&path);
    let after = rig.fs.crash(true);
    hooks.release(Point::BeforeDurablePending);
    handle.wait_idle();
    handle.close().unwrap();

    let rig2 = crate::common::rig(after);
    let reopened = open(&rig2, &folder);
    let snapshot = reopened.snapshot();
    let fold = snapshot.fold();
    let key = RecordKey::ProjectMeta(MetaField::Name);
    let wanted = Record::ProjectMeta {
        field: MetaField::Name,
        value: Some(name_value(outside_name)),
    };
    let in_history = fold
        .history(&key)
        .iter()
        .filter_map(|id| fold.body(id))
        .any(|body| body.record == wanted);
    let noticed_copies = copies_of(&reopened, METADATA);
    let noticed_copy_keeps_it = noticed_copies
        .iter()
        .any(|copy| rig2.fs.peek(&folder.join(copy)).as_deref() == Some(edited.as_slice()));
    let unnoticed_copies: Vec<String> = files(&rig2.fs, &folder)
        .into_iter()
        .filter(|(rel, bytes)| rel.starts_with(".tc/external/") && bytes == &edited)
        .map(|(rel, _)| rel)
        .collect();
    let metadata_now = rig2.fs.peek(&path);
    reopened.close().unwrap();
    assert!(
        in_history || noticed_copy_keeps_it,
        "after the power cut the outside name {outside_name:?} is neither in History nor \
         behind a notice; metadata.json holds the edit: {}; noticed metadata copies: \
         {noticed_copies:?}; .tc/external files holding the edited bytes: {unnoticed_copies:?}",
        metadata_now.as_deref() == Some(edited.as_slice())
    );
    assert!(
        before_cut.as_deref() == Some(edited.as_slice()),
        "metadata.json was written over before the batch that records its edit was durable"
    );
}

/// Copying an outside version of `metadata.json` once does not settle those bytes for the
/// session. A person who puts that version back after a later rename has made a new edit:
/// it is found, copied, told and recorded again, never renamed over in silence (§5.10,
/// §11.1 X3).
#[test]
fn metadata_json_put_back_to_an_earlier_outside_version_is_recorded_again() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let path = folder.join(METADATA);
    let key = RecordKey::ProjectMeta(MetaField::Name);
    let outside_name = "Beta";
    let inside_name = "Gamma";
    let edited = renamed_metadata(&rig, &folder, outside_name);
    rig.fs.put_unseen(&path, &edited, None);
    let first = handle.focus().wait().unwrap().found;
    handle.wait_idle();
    assert_eq!(
        first,
        vec![METADATA.to_owned()],
        "the setup's first edit must be found"
    );
    assert_name_in_history(&handle, outside_name);
    assert_metadata_copy(&rig, &handle, &folder, &edited);
    set_name(&handle, inside_name);
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    let placed: serde_json::Value = serde_json::from_slice(&rig.fs.peek(&path).unwrap()).unwrap();
    assert_eq!(
        placed["identification"]["name"]["en"], inside_name,
        "the setup must place the app's later name before the outside version comes back"
    );
    let copies_before = copies_of(&handle, METADATA).len();

    // The editor saves its old buffer again: byte for byte the version copied above.
    rig.fs.put_unseen(&path, &edited, None);
    let found = handle.focus().wait().unwrap().found;
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    // Any later save runs a group that rewrites the listing in `metadata.json`.
    save_verse(&handle, 1, 1, "a verse saved after the put-back").unwrap();
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    let on_disk: serde_json::Value = serde_json::from_slice(&rig.fs.peek(&path).unwrap()).unwrap();
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    let heads: Vec<Record> = fold
        .heads(&key)
        .iter()
        .filter_map(|id| fold.body(id))
        .map(|body| body.record)
        .collect();
    let copies_after = copies_of(&handle, METADATA).len();
    assert_eq!(
        found,
        vec![METADATA.to_owned()],
        "the person's put-back of an earlier outside version was not found; after a later save \
         metadata.json names {:?}, the name's heads are {heads:?}, metadata copies went \
         {copies_before} -> {copies_after}",
        on_disk["identification"]["name"]["en"]
    );
    let beta = Record::ProjectMeta {
        field: MetaField::Name,
        value: Some(name_value(outside_name)),
    };
    assert!(
        heads.contains(&beta),
        "the put-back name is neither shown nor offered in a conflict: heads {heads:?}"
    );
    assert!(
        copies_after > copies_before,
        "no new notice leads to a copy of the put-back bytes"
    );
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
}

/// When the batch that records an outside edit of `metadata.json` is refused, the file was not
/// written over meanwhile (§11.1 D7), and the copy keeps the person's bytes once the file
/// changes again. Neither dismissing its notice nor reopening the project may lose or hide
/// them.
#[test]
fn a_refused_metadata_recording_keeps_the_edit_or_its_copy_after_dismissal() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let hooks = rig.store.hooks().clone();
    let outside_name = "a name typed outside whose recording the full disk refuses";
    let edited = renamed_metadata(&rig, &folder, outside_name);
    let path = folder.join(METADATA);
    rig.fs.put_unseen(&path, &edited, None);
    hooks.hold(Point::BeforeDurablePending);
    handle.focus().wait().unwrap();
    hooks.wait_reached(Point::BeforeDurablePending, 1);
    let while_recorded = rig.fs.peek(&path);
    let copies = copies_of(&handle, METADATA);
    assert_eq!(copies.len(), 1, "the setup needs one copy: {copies:?}");
    let copy = copies[0].clone();
    assert_eq!(rig.fs.peek(&folder.join(&copy)), Some(edited.clone()));
    rig.fs.fail_writes(Some(ErrorKind::StorageFull));
    hooks.release(Point::BeforeDurablePending);
    // The actor answers the refused batch before this sync, and the writer handles the
    // answer before its own.
    handle.wait_idle();
    let after_refusal = rig.fs.peek(&path);
    rig.fs.fail_writes(None);
    handle.wait_idle();
    let recorded = |handle: &ProjectHandle| {
        let snapshot = handle.snapshot();
        let fold = snapshot.fold();
        let key = RecordKey::ProjectMeta(MetaField::Name);
        fold.history(&key)
            .iter()
            .filter_map(|id| fold.body(id))
            .any(|body| {
                body.record
                    == Record::ProjectMeta {
                        field: MetaField::Name,
                        value: Some(name_value(outside_name)),
                    }
            })
    };
    let notices = handle.notices();
    eprintln!(
        "after the refused recording: name recorded = {}, metadata.json is the edit = {}, copy kept = {}, notices = {notices:#?}",
        recorded(&handle),
        rig.fs.peek(&path).as_deref() == Some(edited.as_slice()),
        rig.fs.peek(&folder.join(&copy)).as_deref() == Some(edited.as_slice()),
    );
    let notice = notices
        .iter()
        .find(|n| matches!(n.notice, Notice::MetadataEditedOutside { .. }))
        .expect("the setup raises MetadataEditedOutside")
        .clone();
    let dismissed = handle.dismiss_notice(notice.id).wait();
    handle.wait_idle();
    let copy_after = rig.fs.peek(&folder.join(&copy));
    eprintln!(
        "dismissal: {dismissed:?}; copy present after = {}",
        copy_after.is_some()
    );
    assert!(
        recorded(&handle) || copy_after.as_deref() == Some(edited.as_slice()),
        "the outside name is in no History and its only copy {copy} was removed by the \
         dismissal (dismissal result {dismissed:?}); notices: {:?}",
        handle.notices()
    );
    assert!(
        while_recorded.as_deref() == Some(edited.as_slice()),
        "metadata.json was written over before the batch that records its edit was durable"
    );
    assert!(
        after_refusal.as_deref() == Some(edited.as_slice()),
        "metadata.json was written over after the disk refused the batch that records its edit"
    );
    handle.close().unwrap();
    let reopened = open(&rig, &folder);
    let routed = copies_of(&reopened, METADATA)
        .iter()
        .any(|c| rig.fs.peek(&folder.join(c)).as_deref() == Some(edited.as_slice()));
    assert!(
        recorded(&reopened) || routed,
        "after reopening, the outside name is in no History and no notice leads to its copy; \
         notices: {:?}",
        reopened.notices()
    );
    reopened.close().unwrap();
}

/// The disk refuses the batch of an outside edit of `metadata.json`: the bytes stay in the
/// folder, and the writer's own retry sends them again once the disk may take them, never at
/// once, which would ask the full disk again and again. Once it takes them, they are recorded
/// and the file is written again.
#[test]
fn a_refused_metadata_batch_is_sent_again_by_the_retry_not_at_once() {
    use tc_store::fault::OpKind;

    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let outside_name = "a name whose batch the full disk refuses for a while";
    let edited = renamed_metadata(&rig, &folder, outside_name);
    let path = folder.join(METADATA);
    let pending = folder.join(".tc/pending");
    rig.fs.put_unseen(&path, &edited, None);
    let hooks = rig.store.hooks().clone();
    hooks.hold(Point::BeforeDurablePending);
    handle.focus().wait().unwrap();
    hooks.wait_reached(Point::BeforeDurablePending, 1);
    rig.fs.fail_writes(Some(ErrorKind::StorageFull));
    hooks.release(Point::BeforeDurablePending);
    let batches_tried = || {
        rig.fs
            .log()
            .iter()
            .filter(|op| op.kind == OpKind::Create && op.path.parent() == Some(pending.as_path()))
            .count()
    };
    // No `wait_idle` here: a writer that sends the batch again at once is never idle. Both
    // looks come well before the retry's first 3 s.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while batches_tried() == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "setup: the batch was never tried"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let after_refusal = batches_tried();
    std::thread::sleep(std::time::Duration::from_millis(500));
    let half_a_second_later = batches_tried();
    let on_disk = rig.fs.peek(&path);
    rig.fs.fail_writes(None);
    assert_eq!(
        half_a_second_later, after_refusal,
        "the refused batch was sent again at once"
    );
    assert!(
        on_disk.as_deref() == Some(edited.as_slice()),
        "bytes not recorded were written over"
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while rig.fs.peek(&path) == Some(edited.clone()) {
        assert!(
            std::time::Instant::now() < deadline,
            "the retry never recorded the edit: {:?}",
            handle.notices()
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    handle.wait_idle();
    assert_name_in_history(&handle, outside_name);
    assert_metadata_copy(&rig, &handle, &folder, &edited);
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
}

/// Another program at a moment the store cannot foresee: it saves over a file while the
/// store waits out that file's hold, or the disk stops reading a journal file the open read.
#[derive(Debug)]
struct Interposed {
    fs: FaultFs,
    /// Bytes another program saves over a path, releasing its hold, at the next wait.
    save_on_wait: Mutex<Option<(PathBuf, Vec<u8>)>>,
    saved: AtomicBool,
    /// A handle of this file stops reading once it has read the file whole, until the disk
    /// reads it again (`unreadable` cleared).
    read_once: Mutex<Option<PathBuf>>,
    unreadable: Arc<AtomicBool>,
}

/// A failing sector, or a cloud placeholder whose local bytes were evicted, after the open
/// read the file.
struct ReadOnce {
    inner: Box<dyn ReadHandle>,
    read: AtomicU64,
    unreadable: Arc<AtomicBool>,
}

impl ReadHandle for ReadOnce {
    fn len(&self) -> u64 {
        self.inner.len()
    }
    fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()> {
        if self.unreadable.load(Ordering::SeqCst)
            && self.read.load(Ordering::SeqCst) >= self.inner.len()
        {
            return Err(io::Error::other("the disk can no longer read this file"));
        }
        self.inner.read_at(buf, offset)?;
        self.read.fetch_add(buf.len() as u64, Ordering::SeqCst);
        Ok(())
    }
}

impl Vfs for Interposed {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.fs.read(path)
    }
    fn open(&self, path: &Path) -> io::Result<Box<dyn ReadHandle>> {
        let inner = self.fs.open(path)?;
        if self.read_once.lock().unwrap().as_deref() != Some(path) {
            return Ok(inner);
        }
        Ok(Box::new(ReadOnce {
            inner,
            read: AtomicU64::new(0),
            unreadable: self.unreadable.clone(),
        }))
    }
    fn metadata(&self, path: &Path) -> io::Result<FileMeta> {
        self.fs.metadata(path)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>> {
        self.fs.read_dir(path)
    }
    fn create_new(&self, path: &Path) -> io::Result<Box<dyn WriteHandle>> {
        self.fs.create_new(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.fs.rename(from, to)
    }
    fn rename_file_noreplace(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.fs.rename_file_noreplace(from, to)
    }
    fn rename_dir_noreplace(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.fs.rename_dir_noreplace(from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.fs.remove_file(path)
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        self.fs.remove_dir_all(path)
    }
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        self.fs.create_dir(path)
    }
    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        self.fs.sync_dir(path)
    }
    fn sync_file(&self, path: &Path) -> io::Result<()> {
        self.fs.sync_file(path)
    }
    fn lock(&self, path: &Path) -> io::Result<Option<Box<dyn LockHandle>>> {
        self.fs.lock(path)
    }
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.fs.canonicalize(path)
    }
    fn secure_dir_chain(&self, path: &Path, made: &[PathBuf]) -> io::Result<()> {
        self.fs.secure_dir_chain(path, made)
    }
    fn is_held(&self, error: &io::Error) -> bool {
        self.fs.is_held(error)
    }
    fn wait_held(&self, delay: Duration) {
        self.fs.wait_held(delay);
        if let Some((path, bytes)) = self.save_on_wait.lock().unwrap().take() {
            self.fs.put_unseen(&path, &bytes, None);
            self.fs.set_busy(&path, false);
            self.saved.store(true, Ordering::SeqCst);
        }
    }
    fn outside_dir(&self, path: &Path) -> io::Result<PathBuf> {
        self.fs.outside_dir(path)
    }
    fn watch(
        &self,
        root: &Path,
        recursive: bool,
        sink: WatchSink,
    ) -> io::Result<Option<WatchGuard>> {
        self.fs.watch(root, recursive, sink)
    }
}

fn interposed_rig(fs: FaultFs) -> (Rig, Arc<Interposed>) {
    let interposed = Arc::new(Interposed {
        fs: fs.clone(),
        save_on_wait: Mutex::new(None),
        saved: AtomicBool::new(false),
        read_once: Mutex::new(None),
        unreadable: Arc::new(AtomicBool::new(true)),
    });
    let (mut settings, _) = config(&fs, APP, "host-1");
    settings.vfs = interposed.clone();
    let store = Store::start(settings).unwrap();
    store.wait_cleanup();
    watch_burrito(&store, &fs);
    (Rig { fs, store }, interposed)
}

fn shows(bytes: &[u8], words: &str) -> bool {
    String::from_utf8_lossy(bytes).contains(words)
}

/// A removed book's file is a plain derived file: its last look before the unlink is the
/// only comparison between M1's read and the removal, so a save another program makes while
/// the store waits out a hold on the file must stop the unlink and be kept.
#[test]
fn a_book_file_saved_during_the_sharing_wait_of_its_removal_is_kept() {
    const PHM: BookId = BookId::Philemon;
    let (rig, interposed) = interposed_rig(FaultFs::new());
    let folder = create_titus(&rig, "Titus");
    let handle = open(&rig, &folder);
    handle
        .submit(Command::AddBook {
            book: PHM,
            scope: Scope::Whole,
            seed: SeedSource {
                usfm: "\\id PHM\n\\c 1\n\\p\n\\v 1 One.\n\\v 2 Two.\n".into(),
            },
        })
        .wait()
        .unwrap();
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    let path = folder.join("ingredients/PHM.usfm");
    let placed = rig.fs.peek(&path).expect("the added book is placed");
    let edited = String::from_utf8(placed.clone())
        .unwrap()
        .replacen(
            "\\v 1 ___",
            "\\v 1 Words a colleague saved in another editor.",
            1,
        )
        .into_bytes();
    assert_ne!(edited, placed);

    handle
        .submit(Command::RemoveBook {
            book: PHM,
            presence_shown: handle.mint(&RecordKey::BookPresence(PHM), None).unwrap(),
        })
        .wait()
        .unwrap();
    *interposed.save_on_wait.lock().unwrap() = Some((path.clone(), edited.clone()));
    rig.fs.set_busy(&path, true);
    let attempt = handle.leave(Leave::Mode).wait();
    // Clear the hold even if a regression never reached the planned removal.
    rig.fs.set_busy(&path, false);
    assert!(
        interposed.saved.load(Ordering::SeqCst),
        "the save must come between the removal's first attempt and its retry"
    );
    attempt.unwrap();
    handle.wait_idle();
    let kept = rig.fs.peek(&path).as_deref() == Some(edited.as_slice())
        || files(&rig.fs, &folder)
            .iter()
            .any(|(rel, bytes)| rel.starts_with(".tc/external/") && *bytes == edited);
    assert!(
        kept,
        "the unlink removed bytes saved after its comparison; notices: {:?}",
        handle.notices()
    );
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    let first = RecordKey::Verse(PHM, ChapterNo::new(1).unwrap(), VerseNo::new(1).unwrap());
    assert!(
        fold.history(&first).iter().any(|id| matches!(
            fold.body(id).map(|body| body.record),
            Some(Record::Verse {
                value: VerseValue::Text { words, .. },
                ..
            }) if words == "Words a colleague saved in another editor."
        )),
        "the colleague's save is absent from History; notices: {:?}",
        handle.notices()
    );
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// O9 projects the book file while the value of a save it shows can no longer be read. That
/// rendering is not what the project holds, so the open's job renders again, finds the value
/// still unreadable, and leaves the book file, the last readable copy of the save, as it is.
#[test]
fn a_rendering_that_could_not_read_a_save_never_replaces_its_last_readable_copy() {
    let saved = "words the book file alone can still show";
    let later = "a later save the book file does not show yet";
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let earlier: BTreeSet<String> = pending_files(&rig.fs, &folder).into_iter().collect();
    save_verse(&handle, 1, 1, saved).unwrap();
    let journal: Vec<String> = pending_files(&rig.fs, &folder)
        .into_iter()
        .filter(|rel| !earlier.contains(rel))
        .collect();
    assert_eq!(journal.len(), 1, "{journal:?}");
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    let book = folder.join(BOOK);
    let placed = rig.fs.peek(&book).unwrap();
    assert!(shows(&placed, saved));
    // The process dies after the later save reached the journal, before the book file.
    save_verse(&handle, 1, 2, later).unwrap();
    let fs = rig.fs.crash(false);
    drop(handle);

    let (rig, interposed) = interposed_rig(fs);
    *interposed.read_once.lock().unwrap() = Some(folder.join(&journal[0]));
    let handle = open(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 2).as_deref(),
        Some(later),
        "the open must have read the journal"
    );
    assert_ne!(
        words(&handle, 1, 1).as_deref(),
        Some(saved),
        "the save's value must be unreadable after the open read its file"
    );
    let after_open = rig.fs.peek(&book).unwrap();
    assert!(
        shows(&after_open, saved),
        "a rendering that could not read the save replaced its last readable copy; notices: {:?}",
        handle.notices()
    );
    assert_eq!(after_open, placed);
    assert_valid_burrito(&rig.fs, &folder);

    // The disk reads the file again: the next job writes both saves.
    interposed.unreadable.store(false, Ordering::SeqCst);
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    assert_eq!(words(&handle, 1, 1).as_deref(), Some(saved));
    let written = rig.fs.peek(&book).unwrap();
    assert!(shows(&written, saved) && shows(&written, later));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}
