//! The handle's contract: create, open, save, reopen; tokens; read-only refusals; drafts;
//! notices; events.

use crate::common::*;
use tc_journal::command::{Command, Draft, DraftField};
use tc_store::fault::FaultFs;
use tc_store::{CommandError, Mode, OpenOptions, ReadOnlyReason, Refusal, StoreEvent, Stored};

#[test]
fn a_created_project_is_a_valid_burrito_with_its_book() {
    let (rig, folder) = titus();
    assert_valid_burrito(&rig.fs, &folder);
    let all = files(&rig.fs, &folder);
    for path in [
        "metadata.json",
        "ingredients/TIT.usfm",
        "ingredients/tc/project.json",
        "ingredients/tc/format.json",
        ".gitignore",
        ".gitattributes",
    ] {
        assert!(all.contains_key(path), "{path} missing: {:?}", all.keys());
    }
    assert!(
        pending_files(&rig.fs, &folder).is_empty(),
        "the create seals its batches"
    );
    assert_eq!(packs(&rig.fs, &folder).len(), 1);
    assert!(
        !all.keys().any(|p| p.contains(".staging-")),
        "no staging folder is left"
    );
}

#[test]
fn a_save_is_durable_before_its_reply_and_survives_a_reopen() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let reply = save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    assert_eq!(reply.stored, Stored::Durable);
    assert!(reply.seq >= handle.snapshot().seq() || reply.seq > 0);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("Paul, a servant of God")
    );
    assert_eq!(pending_files(&rig.fs, &folder).len(), 1);
    // A power cut right after the reply keeps the save.
    let after = rig.fs.crash(true);
    drop(handle);
    let rig2 = rig_as(after, APP, "host-1");
    let handle = rig2.store.open(&folder, &OpenOptions::default()).unwrap();
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("Paul, a servant of God")
    );
    handle.close().unwrap();
}

#[test]
fn a_save_of_the_displayed_value_writes_nothing() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    save_verse(&handle, 1, 2, "in hope").unwrap();
    let reply = save_verse(&handle, 1, 2, "in hope").unwrap();
    assert_eq!(reply.stored, Stored::Unchanged);
    assert!(reply.changes.is_empty());
    handle.close().unwrap();
}

#[test]
fn a_stale_token_makes_a_conflict_never_a_revert() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let old = handle.mint(&verse(1, 3), None).unwrap();
    save_verse(&handle, 1, 3, "first").unwrap();
    let reply = handle
        .submit(Command::SaveVerse {
            shown: old,
            words: "second".into(),
        })
        .wait()
        .unwrap();
    assert!(
        !reply.new_conflicts.is_empty(),
        "both versions are kept as a conflict"
    );
    assert_eq!(handle.snapshot().fold().heads(&verse(1, 3)).len(), 2);
    handle.close().unwrap();
}

#[test]
fn a_review_token_cannot_save_and_an_edit_token_cannot_resolve() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let review = handle.mint_review(&verse(1, 4)).unwrap();
    let refused = handle
        .submit(Command::SaveVerse {
            shown: review.clone(),
            words: "x".into(),
        })
        .wait();
    assert_eq!(
        refused.unwrap_err(),
        CommandError::Refused(Refusal::WrongTokenKind)
    );
    assert!(handle.parse_edit(&review.to_text()).is_err());
    assert!(handle.parse_review(&review.to_text()).is_ok());
    handle.close().unwrap();
}

#[test]
fn a_second_open_of_a_project_is_read_only_and_refuses_saves() {
    let (rig, folder) = titus();
    let first = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let second = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    assert_eq!(
        second.facts().mode,
        Mode::ReadOnly(ReadOnlyReason::LockHeld)
    );
    let before = files(&rig.fs, &folder);
    let refused = save_verse(&second, 1, 1, "x");
    assert!(matches!(
        refused,
        Err(CommandError::ReadOnly(ReadOnlyReason::LockHeld))
    ));
    second.close().unwrap();
    assert_eq!(
        files(&rig.fs, &folder),
        before,
        "a read-only open writes nothing"
    );
    first.close().unwrap();
}

#[test]
fn reads_never_wait_behind_a_save() {
    let hooks = std::sync::Arc::new(tc_store::Hooks::default());
    let rig = rig_with_hooks(FaultFs::new(), hooks.clone());
    rig.store.wait_cleanup();
    let folder = create_titus(&rig, "Titus");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    hooks.hold(tc_store::Point::BeforeDurablePending);
    let shown = handle.mint(&verse(1, 5), None).unwrap();
    let pending = handle.submit(Command::SaveVerse {
        shown,
        words: "held".into(),
    });
    hooks.wait_reached(tc_store::Point::BeforeDurablePending, 1);
    // The actor is held inside the save; a snapshot and a book read still come back.
    let snapshot = handle.snapshot();
    assert!(snapshot.fold().book_usfm(TIT).is_some());
    let book = handle.read_book(TIT).wait();
    assert!(book.usfm.is_some());
    assert!(!pending.is_ready(), "no reply before the save is durable");
    hooks.release(tc_store::Point::BeforeDurablePending);
    assert_eq!(pending.wait().unwrap().stored, Stored::Durable);
    handle.close().unwrap();
}

#[test]
fn a_pending_book_read_waits_and_is_never_an_empty_book() {
    let hooks = std::sync::Arc::new(tc_store::Hooks::default());
    let rig = rig_with_hooks(FaultFs::new(), hooks.clone());
    rig.store.wait_cleanup();
    let folder = create_titus(&rig, "Titus");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    hooks.hold(tc_store::Point::BeforeBookRead);
    let read = handle.read_book(TIT);
    hooks.wait_reached(tc_store::Point::BeforeBookRead, 1);
    assert!(!read.is_ready());
    hooks.release(tc_store::Point::BeforeBookRead);
    let book = read.wait();
    assert_eq!(book.book, TIT);
    assert!(book.usfm.unwrap().contains("\\c 1"));
    handle.close().unwrap();
}

#[test]
fn every_committed_batch_is_an_event_with_its_seq() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let events = handle.subscribe();
    let reply = save_verse(&handle, 1, 6, "an elder").unwrap();
    let event = events.recv().unwrap();
    match event {
        StoreEvent::Committed { seq, changes, .. } => {
            assert_eq!(seq, reply.seq);
            assert_eq!(changes, reply.changes);
        }
        other => panic!("{other:?}"),
    }
    assert!(handle.snapshot().seq() >= reply.seq);
    handle.close().unwrap();
}

#[test]
fn a_draft_is_kept_and_offered_at_the_next_open_until_saved() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let shown = handle.mint(&verse(1, 7), None).unwrap();
    handle
        .submit(Command::Draft(Draft {
            field: DraftField::Words,
            shown: vec![shown],
            text: "unsaved words".into(),
        }))
        .wait()
        .unwrap();
    handle.wait_idle();
    drop(handle);
    let crashed = rig.fs.crash(true);
    let rig2 = rig_as(crashed, APP, "host-1");
    let handle = rig2.store.open(&folder, &OpenOptions::default()).unwrap();
    let offered = handle.offered_drafts();
    assert_eq!(offered.len(), 1, "{offered:?}");
    assert_eq!(offered[0].draft.text, "unsaved words");
    assert!(offered[0].verified);
    // Keep saves it with its token.
    let reply = handle
        .submit(Command::KeepDraft {
            draft: offered[0].draft.clone(),
            section: Vec::new(),
        })
        .wait()
        .unwrap();
    assert_eq!(reply.stored, Stored::Durable);
    assert_eq!(words(&handle, 1, 7).as_deref(), Some("unsaved words"));
    handle.wait_idle();
    handle.close().unwrap();
    let rig3 = rig_as(rig2.fs.crash(false), APP, "host-1");
    let handle = rig3.store.open(&folder, &OpenOptions::default()).unwrap();
    assert!(
        handle.offered_drafts().is_empty(),
        "a saved draft is not offered again"
    );
    handle.close().unwrap();
}

#[test]
fn a_draft_whose_token_no_longer_verifies_is_offered_as_text_and_kept_without_parents() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    save_verse(&handle, 1, 8, "hospitable").unwrap();
    let shown = handle.mint(&verse(1, 8), None).unwrap();
    handle
        .submit(Command::Draft(Draft {
            field: DraftField::Words,
            shown: vec![shown],
            text: "a lover of good".into(),
        }))
        .wait()
        .unwrap();
    handle.wait_idle();
    handle.close().unwrap();
    // A new installation secret: the old app data is gone.
    let fs = rig.fs.crash(false);
    fs.delete(std::path::Path::new("/app/installation.json"))
        .unwrap();
    let rig2 = rig_as(fs, APP, "host-1");
    let handle = rig2.store.open(&folder, &OpenOptions::default()).unwrap();
    let offered = handle.offered_drafts();
    assert_eq!(offered.len(), 1);
    assert!(!offered[0].verified);
    let reply = handle
        .submit(Command::KeepDraft {
            draft: offered[0].draft.clone(),
            section: Vec::new(),
        })
        .wait()
        .unwrap();
    assert_eq!(reply.changes.len(), 1);
    let change = handle.snapshot().fold().change(&reply.changes[0]).unwrap();
    assert!(change.parents.is_empty(), "no parents: R20");
    // Both texts are kept: a conflict unless they are equal.
    assert_eq!(handle.snapshot().fold().heads(&verse(1, 8)).len(), 2);
    handle.close().unwrap();
}

#[test]
fn a_project_name_that_exists_is_refused_and_untouched() {
    let (rig, folder) = titus();
    let before = files(&rig.fs, &folder);
    let again = rig.store.create(&tc_store::NewProject {
        name: "Titus".into(),
        kind: tc_journal::fold::ProjectKind::Bible,
        records: first_records(),
        books: Vec::new(),
    });
    assert!(matches!(again, Err(tc_store::CreateError::NameExists(_))));
    assert_eq!(files(&rig.fs, &folder), before);
}

#[test]
fn a_comment_revision_keeps_the_earlier_text_in_history() {
    use tc_journal::record::{CommentTarget, CommentValue};
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let target = CommentTarget::Verse {
        book: TIT,
        chapter: tc_journal::ChapterNo::new(1).unwrap(),
        verse: tc_journal::VerseNo::new(1).unwrap(),
    };
    let added = handle
        .submit(Command::AddComment {
            target: target.clone(),
            text: "Check the word servant".into(),
        })
        .wait()
        .unwrap();
    let key = added.tokens[0].0.clone();
    let revised = handle
        .submit(Command::ReviseComment {
            shown: added.tokens[0].1.clone(),
            text: "Check the word servant: slave?".into(),
        })
        .wait()
        .unwrap();
    assert_eq!(revised.changes.len(), 1);
    let fold = handle.snapshot();
    match fold.fold().projected(&key) {
        Some(tc_journal::Record::Comment {
            value: CommentValue { text, target: t },
            ..
        }) => {
            assert_eq!(text, "Check the word servant: slave?");
            assert_eq!(t, target);
        }
        other => panic!("{other:?}"),
    }
    let history = fold.fold().history(&key);
    assert_eq!(history.len(), 2, "the earlier text is kept in History");
    let empty = handle
        .submit(Command::ReviseComment {
            shown: revised.tokens[0].1.clone(),
            text: "  ".into(),
        })
        .wait();
    assert!(matches!(
        empty,
        Err(CommandError::Refused(Refusal::Text(_)))
    ));
    handle.close().unwrap();
}

#[test]
fn a_bulk_choice_resolves_each_conflict_in_one_batch_and_skips_stale_ones() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    // Two conflicts made on this computer: each verse saved twice from one token.
    for v in [3, 4] {
        let old = handle.mint(&verse(1, v), None).unwrap();
        save_verse(&handle, 1, v, &format!("first {v}")).unwrap();
        handle
            .submit(Command::SaveVerse {
                shown: old,
                words: format!("second {v}"),
            })
            .wait()
            .unwrap();
    }
    let snapshot = handle.snapshot();
    let conflicts: Vec<_> = snapshot.fold().conflicts().map(|c| c.id).collect();
    assert_eq!(conflicts.len(), 2);
    let with_tokens: Vec<_> = conflicts
        .iter()
        .map(|id| {
            let conflict = snapshot.fold().conflict(id).unwrap();
            let tokens = std::iter::once(&conflict.key)
                .chain(&conflict.also_keys)
                .map(|k| snapshot.review_token(k).unwrap())
                .collect();
            (*id, tokens)
        })
        .collect();
    let mine = std::collections::BTreeSet::from([handle.facts().device]);
    let reply = handle
        .submit(Command::ResolveBulk {
            devices: mine,
            conflicts: with_tokens,
        })
        .wait()
        .unwrap();
    // Both candidates of each conflict come from this computer: each is left for a choice
    // of its own, never decided by a guess.
    assert_eq!(reply.skipped.len(), 2);
    assert!(reply.changes.is_empty());
    handle.close().unwrap();
}

#[test]
fn the_last_book_cannot_be_removed() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let presence = handle
        .mint(&tc_journal::RecordKey::BookPresence(TIT), None)
        .unwrap();
    let refused = handle
        .submit(Command::RemoveBook {
            book: TIT,
            presence_shown: presence,
        })
        .wait();
    assert_eq!(
        refused.unwrap_err(),
        CommandError::Refused(Refusal::LastBook)
    );
    handle.close().unwrap();
}
