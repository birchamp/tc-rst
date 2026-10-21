//! Placement preserves outside versions before replacing or removing their files.

use crate::common::*;
use std::io::ErrorKind;
use std::path::Path;
use tc_journal::command::Command;
use tc_journal::fold::{ConflictKind, Notice};
use tc_journal::keys::SettingKey;
use tc_journal::record::CarriedValue;
use tc_journal::{Json, JsonMap, Md5, MetaField, Record, RecordKey, RepoPath};
use tc_store::{Leave, OpenOptions, Point, ProjectHandle, Vfs};

const METADATA: &str = "metadata.json";
const SONG: &str = "ingredients/audio/song.mp3";

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
