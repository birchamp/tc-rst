//! The contract cases of APP-DESIGN §4.2 that only the store runs (unit C0): writes, their
//! replies and events, the refusals, read-only folders and drafts, each through the seam on
//! its own world.

use std::sync::Arc;
use std::time::Duration;

use tc_api::write::ReadOnlyReason;
use tc_api::{ApiError, NotSaved};
use tc_app::backend::{CommandError, ProjectHandle, StoreCause, StoreCommand, StoreEvent, Stored};
use tc_app::counters::{AppCounters, CountingSnapshot};
use tc_app::wire::error::command_error;
use tc_app::wire::token::{edit_token, shown};
use tc_journal::command::{Choice, DraftField, SectionValue};
use tc_journal::fold::{ConflictKind, Notice, Resolved};
use tc_journal::record::VerseValue;
use tc_journal::{BookId, Record, RecordKey, VerseNo};
use tc_store::{FakePlatform, FolderFacts, Point, UnsafeFolder};

use crate::store_world::{
    APP, backend_over, files, pending_dir_flush, progress, save, store_world, store_world_with,
    tit, token, words_of,
};

/// The value a verse shows, whole.
fn value_of(handle: &dyn ProjectHandle, key: &RecordKey) -> VerseValue {
    match handle.snapshot().fold().projected(key) {
        Some(Record::Verse { value, .. }) => value,
        other => panic!("{other:?}"),
    }
}

/// The conflict a record is in.
fn conflict_of(handle: &dyn ProjectHandle, key: &RecordKey) -> tc_journal::fold::ConflictId {
    match handle.snapshot().fold().key_state(key).unwrap().state {
        Resolved::Conflict(id) => id,
        other => panic!("{other:?}"),
    }
}

/// R5: with the actor held inside a long command (a checkpoint), snapshots, counted reads,
/// minting and parsing all come back, and a save waits its turn.
#[tokio::test]
async fn reads_never_wait_behind_a_long_command() {
    let world = store_world();
    let handle = world.open(&world.bible);
    let before = handle.snapshot().seq();
    world.hooks.hold(Point::BeforeCheckpoint);
    let checkpoint = handle.checkpoint(tc_app::backend::CheckpointReason::LeavingProject);
    world.hooks.wait_reached(Point::BeforeCheckpoint, 1);

    let (tx, rx) = std::sync::mpsc::channel();
    {
        let handle = handle.clone();
        std::thread::spawn(move || {
            let snapshot = handle.snapshot();
            let read = CountingSnapshot::new(snapshot.clone(), Arc::new(AppCounters::default()));
            let chapter = read.chapter_usfm(BookId::Titus, 1);
            let minted = read.mint(&tit(1, 4), None).unwrap();
            let parsed = edit_token(handle.as_ref(), &shown(&minted).unwrap()).unwrap();
            let _ = tx.send((snapshot.seq(), chapter.is_some(), parsed.key().clone()));
        });
    }
    let (seq, chapter, key) = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the reads came back while the actor is held");
    assert_eq!(seq, before);
    assert!(chapter);
    assert_eq!(key, tit(1, 4));

    let saving = handle.submit(StoreCommand::save_verse(
        token(handle.as_ref(), &tit(1, 4)),
        "a word in season".to_owned(),
    ));
    world.hooks.release(Point::BeforeCheckpoint);
    assert_eq!(saving.await.unwrap().stored, Stored::Durable);
    assert!(matches!(
        checkpoint.await,
        tc_api::event::CheckpointOutcome::Committed | tc_api::event::CheckpointOutcome::Unchanged
    ));
}

/// STORAGE:1447: a save of the words the verse already shows writes nothing and announces
/// nothing.
#[tokio::test]
async fn unchanged_save_writes_nothing() {
    let world = store_world();
    let handle = world.open(&world.bible);
    let mut events = handle.subscribe();
    let key = tit(1, 2);
    let words = words_of(&handle.snapshot(), &key).unwrap();
    let seq = handle.snapshot().seq();
    let counters = handle.counters();
    let reply = save(handle.as_ref(), &key, &words).await.unwrap();
    assert_eq!(reply.stored, Stored::Unchanged);
    assert!(reply.changes.is_empty());
    assert!(reply.tokens.is_empty(), "the store gives no token for it");
    assert_eq!(reply.seq, seq);
    let spent = handle.counters().since(&counters);
    assert_eq!(
        (spent.files_created, spent.fsyncs, spent.renames),
        (0, 0, 0),
        "{spent:?}"
    );
    // The next real save's event is the first the subscriber gets.
    let written = save(handle.as_ref(), &key, "a changed verse")
        .await
        .unwrap();
    match events.recv().await.unwrap() {
        StoreEvent::Committed { seq, .. } => assert_eq!(seq, written.seq),
        other => panic!("{other:?}"),
    }
}

/// STORAGE:1455: the reply's token lists the new change only, so the next save continues
/// only what the person saw.
#[tokio::test]
async fn reply_token_lists_only_the_new_change() {
    let world = store_world();
    let handle = world.open(&world.bible);
    let key = tit(1, 3);
    let reply = save(handle.as_ref(), &key, "a third verse").await.unwrap();
    let (_, next) = reply.tokens.iter().find(|(k, _)| *k == key).unwrap();
    assert_eq!(next.heads().len(), 1);
    assert!(reply.changes.contains(&next.heads()[0]));

    let (folder, key) = &world.conflicted;
    let handle = world.open(folder);
    let heads = handle.snapshot().fold().heads(key);
    assert_eq!(heads.len(), 2);
    let displayed = token(handle.as_ref(), key);
    let other: Vec<_> = heads
        .iter()
        .filter(|h| !displayed.heads().contains(h))
        .collect();
    assert_eq!(other.len(), 1);
    let reply = save(handle.as_ref(), key, "a third text").await.unwrap();
    let (_, next) = reply.tokens.iter().find(|(k, _)| k == key).unwrap();
    assert_eq!(next.heads().len(), 1, "only the new change");
    assert!(reply.changes.contains(&next.heads()[0]));
    assert!(
        !next.heads().contains(other[0]),
        "not the head it never saw"
    );
}

/// STORAGE:1449-1453, I4: a save whose token is older than the record's head makes a
/// conflict with the head it never saw, never a revert of it. S1 has no intake that keeps a
/// project writable (a second writer's files turn it read-only), so the unseen head is made
/// by a save through a newer token; E1a runs the case again with a real intake.
#[tokio::test]
async fn stale_token_after_intake_makes_a_conflict_never_a_revert() {
    let world = store_world();
    let handle = world.open(&world.bible);
    let key = tit(1, 2);
    let stale = token(handle.as_ref(), &key);
    let x = save(handle.as_ref(), &key, "the head the screen never saw")
        .await
        .unwrap();
    let y = handle
        .submit(StoreCommand::save_verse(stale, "second".to_owned()))
        .await
        .unwrap();
    assert!(!y.new_conflicts.is_empty(), "a visible conflict");
    let mut heads = handle.snapshot().fold().heads(&key);
    heads.sort();
    let mut expected = vec![x.changes[0], y.changes[0]];
    expected.sort();
    assert_eq!(heads, expected);
    let body = handle.snapshot().fold().body(&x.changes[0]).unwrap();
    match body.record {
        Record::Verse {
            value: VerseValue::Text { words, .. },
            ..
        } => assert_eq!(words, "the head the screen never saw"),
        other => panic!("{other:?}"),
    }
}

/// R16: a read right after the reply sees the save.
#[tokio::test]
async fn read_your_writes() {
    let world = store_world();
    let handle = world.open(&world.bible);
    let key = tit(1, 5);
    let reply = save(handle.as_ref(), &key, "read me back").await.unwrap();
    let snapshot = handle.snapshot();
    assert!(snapshot.seq() >= reply.seq);
    assert_eq!(words_of(&snapshot, &key).as_deref(), Some("read me back"));
}

/// STORAGE:1474-1481 (§5.2 step 2): a join made after the section editor drew its screen does
/// not refuse the editor's save of what it displayed; the two are one visible passage
/// conflict.
#[tokio::test]
async fn section_save_checks_only_displayed_values() {
    let world = store_world();
    let handle = world.open(&world.bible);
    let (five_key, six_key, seven_key) = (tit(1, 5), tit(1, 6), tit(1, 7));
    // The section editor shows 1:5 and 1:6.
    let shown5 = token(handle.as_ref(), &five_key);
    let shown6 = token(handle.as_ref(), &six_key);
    let (five, six) = (
        value_of(handle.as_ref(), &five_key),
        value_of(handle.as_ref(), &six_key),
    );
    // Meanwhile 6 and 7 are joined.
    let VerseValue::Text { marker, layout, .. } = six.clone() else {
        panic!("{six:?}")
    };
    handle
        .submit(StoreCommand::save_section(
            BookId::Titus,
            vec![
                (
                    token(handle.as_ref(), &six_key),
                    Some(SectionValue::Verse(VerseValue::Text {
                        marker,
                        words: "six and seven".into(),
                        layout,
                        last: VerseNo::new(7).unwrap(),
                        from: None,
                    })),
                ),
                (
                    token(handle.as_ref(), &seven_key),
                    Some(SectionValue::Verse(VerseValue::Covered {
                        by: VerseNo::new(6).unwrap(),
                    })),
                ),
            ],
            None,
        ))
        .await
        .unwrap();
    let edited = |v: VerseValue, words: &str| match v {
        VerseValue::Text {
            marker,
            layout,
            last,
            from,
            ..
        } => VerseValue::Text {
            marker,
            words: words.into(),
            layout,
            last,
            from,
        },
        other => other,
    };
    let reply = handle
        .submit(StoreCommand::save_section(
            BookId::Titus,
            vec![
                (
                    shown5,
                    Some(SectionValue::Verse(edited(five, "five, edited"))),
                ),
                (
                    shown6,
                    Some(SectionValue::Verse(edited(six, "six, edited"))),
                ),
            ],
            None,
        ))
        .await
        .unwrap();
    assert_eq!(reply.stored, Stored::Durable, "never refused");
    assert!(!reply.new_conflicts.is_empty(), "a visible conflict");
    let snapshot = handle.snapshot();
    let conflict = snapshot
        .fold()
        .conflict(&reply.new_conflicts[0])
        .unwrap()
        .clone();
    assert_eq!(conflict.kind, ConflictKind::Passage);
}

/// STORAGE:1615-1616: a resolution over heads that moved since the review was shown is
/// refused with "look again", and nothing is written.
#[tokio::test]
async fn resolve_with_moved_heads_is_look_again() {
    let world = store_world();
    let (folder, key) = &world.conflicted;
    let handle = world.open(folder);
    let review = handle.snapshot().mint_review(key).unwrap();
    let conflict = conflict_of(handle.as_ref(), key);
    let head = review.heads()[0];
    save(handle.as_ref(), key, "a newer text").await.unwrap();
    let seq = handle.snapshot().seq();
    let refused = handle
        .submit(StoreCommand::resolve(
            conflict,
            vec![review],
            Choice::Candidate { head },
        ))
        .await
        .unwrap_err();
    assert_eq!(refused, CommandError::LookAgain);
    assert_eq!(handle.snapshot().seq(), seq);
    assert_eq!(
        command_error(refused),
        Ok(ApiError::NotSaved(NotSaved::LookAgain))
    );
}

/// STORAGE:1422-1424: a save the disk refused keeps the verse and the device's counter, and a
/// retry with the same token writes the change the first attempt would have written.
#[tokio::test]
async fn not_written_keeps_seq_and_retry_uses_the_same_token() {
    let world = store_world();
    let handle = world.open(&world.bible);
    let key = tit(1, 9);
    let shown = token(handle.as_ref(), &key);
    let before = words_of(&handle.snapshot(), &key);
    let device = handle.facts().device;
    let next_seq = handle
        .snapshot()
        .fold()
        .device_stats()
        .get(&device)
        .map_or(1, |s| s.max_seq.0 + 1);
    // The pending folder exists, so the save's first write point is its temporary file.
    world.fs.fail_op(0, std::io::ErrorKind::StorageFull);
    let refused = handle
        .submit(StoreCommand::save_verse(
            shown.clone(),
            "holding firm".to_owned(),
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(refused, CommandError::NotWritten(_)),
        "{refused:?}"
    );
    assert_eq!(words_of(&handle.snapshot(), &key), before);
    assert!(matches!(
        command_error(refused),
        Ok(ApiError::NotSaved(NotSaved::Disk(_)))
    ));
    let reply = handle
        .submit(StoreCommand::save_verse(shown, "holding firm".to_owned()))
        .await
        .unwrap();
    assert_eq!(reply.stored, Stored::Durable);
    let body = handle.snapshot().fold().body(&reply.changes[0]).unwrap();
    assert_eq!(body.seq.0, next_seq, "the refused save used no seq");
    assert_eq!(
        words_of(&handle.snapshot(), &key).as_deref(),
        Some("holding firm")
    );
}

/// STORAGE:1424-1428: a save whose file is in place but whose folder flush failed is saved,
/// with the disk's warning, and the notice reaches the seam's subscriber.
#[tokio::test]
async fn written_not_durable_is_saved_with_warning() {
    let world = store_world();
    let key = tit(1, 10);
    let flush = pending_dir_flush(&world, &world.bible, &key).await;
    let handle = world.open(&world.bible);
    let mut events = handle.subscribe();
    world.fs.fail_op(flush, std::io::ErrorKind::Other);
    let reply = save(handle.as_ref(), &key, "probe").await.unwrap();
    let Stored::WrittenNotDurable(reason) = &reply.stored else {
        panic!("{:?}", reply.stored)
    };
    assert_eq!(
        tc_app::wire::error::stored(&reply.stored),
        (tc_api::write::Stored::Written, Some(reason.clone()))
    );
    assert_eq!(words_of(&handle.snapshot(), &key).as_deref(), Some("probe"));
    let mut noticed = false;
    while let Ok(Some(event)) = tokio::time::timeout(Duration::from_secs(5), events.recv()).await {
        match event {
            StoreEvent::Notice(notice) => {
                noticed |= matches!(notice.notice, Notice::SavedWithDiskError { .. });
            }
            StoreEvent::Committed { seq, cause, .. } => {
                assert_eq!(seq, reply.seq);
                assert!(matches!(cause, StoreCause::Command { .. }));
                break;
            }
            other => panic!("{other:?}"),
        }
    }
    assert!(noticed, "the notice came before its batch's event");
}

/// STORAGE:1893-1899: a project in a cloud-synced folder opens read-only, refuses a save, a
/// draft and a resolution with its reason, and writes nothing.
#[tokio::test]
async fn read_only_refuses_every_command_and_writes_nothing() {
    let world = store_world_with(FakePlatform {
        facts: FolderFacts {
            unsafe_folder: Some(UnsafeFolder::CloudSynced {
                client: "OneDrive".into(),
            }),
            network: false,
        },
        ..FakePlatform::default()
    });
    let (folder, conflicted) = &world.conflicted;
    let before = files(&world.fs, &world.folder(folder));
    let handle = world.open(folder);
    assert_eq!(
        handle.facts().mode,
        tc_api::event::SessionMode::ReadOnly(ReadOnlyReason::CloudSynced)
    );
    let counters = handle.counters();
    let read_only = CommandError::ReadOnly(ReadOnlyReason::CloudSynced);

    let saved = save(handle.as_ref(), &tit(1, 2), "not here").await;
    assert_eq!(saved.unwrap_err(), read_only);
    let drafted = handle
        .submit(StoreCommand::draft(
            DraftField::Words,
            vec![token(handle.as_ref(), &tit(1, 3))],
            "typed".to_owned(),
            tc_journal::WallTime(0),
        ))
        .await;
    assert_eq!(drafted.unwrap_err(), read_only);
    let review = handle.snapshot().mint_review(conflicted).unwrap();
    let head = review.heads()[0];
    let resolved = handle
        .submit(StoreCommand::resolve(
            conflict_of(handle.as_ref(), conflicted),
            vec![review],
            Choice::Candidate { head },
        ))
        .await;
    assert_eq!(resolved.unwrap_err(), read_only);
    assert_eq!(
        command_error(read_only),
        Ok(ApiError::NotSaved(NotSaved::ReadOnly(
            ReadOnlyReason::CloudSynced
        )))
    );

    handle.wait_idle();
    let spent = handle.counters().since(&counters);
    assert_eq!(
        (
            spent.files_created,
            spent.renames,
            spent.removes,
            spent.fsyncs
        ),
        (0, 0, 0, 0),
        "{spent:?}"
    );
    handle.close().await.unwrap();
    assert_eq!(
        files(&world.fs, &world.folder(folder)),
        before,
        "a read-only open writes nothing"
    );
}

/// STORAGE:1750-1759: a draft written in one session is offered at the next open while its
/// text differs from the record's.
#[tokio::test]
async fn drafts_offered_after_reopen() {
    let world = store_world();
    let key = tit(1, 7);
    let handle = world.open(&world.bible);
    let reply = handle
        .submit(StoreCommand::draft(
            DraftField::Words,
            vec![token(handle.as_ref(), &key)],
            "typed, not saved".to_owned(),
            tc_journal::WallTime(0),
        ))
        .await
        .unwrap();
    assert_eq!(reply.stored, Stored::Unchanged);
    assert!(reply.changes.is_empty());
    handle.wait_idle();
    handle.close().await.unwrap();

    let handle = world.open(&world.bible);
    let offered = handle.offered_drafts();
    assert_eq!(offered.len(), 1, "{offered:?}");
    assert!(offered[0].verified);
    assert_eq!(offered[0].field, DraftField::Words);
    assert_eq!(offered[0].text, "typed, not saved");
}

/// §6.6, R20: a draft whose token no longer verifies (another installation secret: an
/// app-data reset, a reinstall) is offered as text, never dropped.
#[tokio::test]
async fn draft_with_unverifiable_token_is_offered_as_text() {
    let world = store_world();
    let key = tit(1, 8);
    let handle = world.open(&world.bible);
    handle
        .submit(StoreCommand::draft(
            DraftField::Words,
            vec![token(handle.as_ref(), &key)],
            "hospitable, typed".to_owned(),
            tc_journal::WallTime(0),
        ))
        .await
        .unwrap();
    handle.wait_idle();
    handle.close().await.unwrap();

    let after = world.fs.crash(false);
    after
        .delete(&std::path::Path::new(APP).join("installation.json"))
        .unwrap();
    let backend = backend_over(&after, APP, FakePlatform::default(), Arc::default());
    let handle = tc_app::backend::ProjectStore::open(&backend, &world.bible, progress()).unwrap();
    handle.wait_idle();
    let offered = handle.offered_drafts();
    assert_eq!(offered.len(), 1, "{offered:?}");
    assert!(!offered[0].verified, "another installation's token");
    assert_eq!(offered[0].field, DraftField::Words);
    assert_eq!(offered[0].text, "hospitable, typed");
}

/// R26 (§9.2), STORAGE §5.14: a draft sent while its save waits must survive that save.
/// The store itself keeps it; no browser buffer or post-Ack refresh participates here.
#[tokio::test]
async fn a_save_never_erases_a_newer_draft_of_its_field() {
    let world = store_world();
    let key = tit(1, 12);
    let handle = world.open(&world.bible);
    let shown = token(handle.as_ref(), &key);
    world.hooks.hold(Point::BeforeDurablePending);
    let saving = handle.submit(StoreCommand::save_verse(
        shown.clone(),
        "first words".to_owned(),
    ));
    world.hooks.wait_reached(Point::BeforeDurablePending, 1);
    let draft = handle
        .submit(StoreCommand::draft(
            DraftField::Words,
            vec![shown],
            "first words, then more typed".to_owned(),
            tc_journal::WallTime(0),
        ))
        .await
        .unwrap();
    assert_eq!(draft.stored, Stored::Unchanged);
    world.hooks.release(Point::BeforeDurablePending);
    assert_eq!(saving.await.unwrap().stored, Stored::Durable);
    handle.wait_idle();
    handle.close().await.unwrap();

    let handle = world.open(&world.bible);
    assert_eq!(
        words_of(&handle.snapshot(), &key).as_deref(),
        Some("first words")
    );
    let offered = handle.offered_drafts();
    assert_eq!(
        offered
            .iter()
            .map(|draft| (draft.field.clone(), draft.text.as_str()))
            .collect::<Vec<_>>(),
        vec![(DraftField::Words, "first words, then more typed")],
        "the text typed while the save was in flight"
    );
}
