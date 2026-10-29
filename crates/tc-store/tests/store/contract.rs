//! The handle's contract: create, open, save, reopen; tokens; read-only refusals; drafts;
//! notices; events.

use crate::common::*;
use tc_journal::command::{Command, Draft, DraftField};
use tc_store::fault::FaultFs;
use tc_store::{CommandError, Mode, OpenError, OpenOptions, Refusal, StoreEvent, Stored};

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
    assert!(
        handle.snapshot().seq() >= reply.seq,
        "the snapshot reads the save"
    );
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

/// O1, tC4 D39: translationCore runs once per computer, so a project whose `.tc/lock`
/// another process holds is not opened at all. The open is refused with the holder the lock
/// file names, writes nothing, and leaves the holder's session as it was.
#[test]
fn a_second_open_of_a_project_is_refused_and_writes_nothing() {
    let (rig, folder) = titus();
    let first = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    first.wait_idle();
    let before = files(&rig.fs, &folder);
    rig.fs.clear_log();
    let Err(refused) = rig.store.open(&folder, &OpenOptions::default()) else {
        panic!("a second open of a project that is open is refused");
    };
    match &refused {
        OpenError::LockHeld {
            holder: Some(holder),
        } => {
            assert_eq!(holder.host, "host-1");
            assert_eq!(holder.pid, 4242, "the pid the lock file names");
        }
        other => panic!("{other:?}"),
    }
    let message = refused.to_string();
    assert!(
        message.contains("host-1") && message.contains("4242"),
        "the error names the holder: {message}"
    );
    assert_eq!(
        files(&rig.fs, &folder),
        before,
        "a refused open writes nothing"
    );
    assert!(
        rig.fs.log().is_empty(),
        "a refused open changed the folder: {:?}",
        rig.fs.log()
    );
    // The first session is as it was, and a close frees the project for the next open.
    save_verse(&first, 1, 1, "still writable").unwrap();
    first.close().unwrap();
    let again = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    assert_eq!(again.facts().mode, Mode::Writable);
    assert_eq!(words(&again, 1, 1).as_deref(), Some("still writable"));
    again.close().unwrap();
}

/// O1: a program that holds the lock without writing its name (or a platform that does not
/// let another handle read a locked file) still refuses the open; the error says no one by
/// name.
#[test]
fn a_lock_another_program_holds_refuses_the_open_without_a_name() {
    let (rig, folder) = titus();
    let held = tc_store::Vfs::lock(&rig.fs, &folder.join(".tc/lock"))
        .unwrap()
        .expect("the lock is free");
    rig.fs.clear_log();
    let Err(refused) = rig.store.open(&folder, &OpenOptions::default()) else {
        panic!("the open is refused");
    };
    assert!(
        matches!(refused, OpenError::LockHeld { holder: None }),
        "{refused:?}"
    );
    assert!(refused.to_string().contains("another program"));
    assert!(rig.fs.log().is_empty(), "{:?}", rig.fs.log());
    drop(held);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.close().unwrap();
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
        .submit(Command::Draft {
            draft: Draft {
                field: DraftField::Words,
                shown: vec![shown],
                text: "unsaved words".into(),
            },
            at: tc_journal::WallTime(0),
        })
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
        .submit(Command::Draft {
            draft: Draft {
                field: DraftField::Words,
                shown: vec![shown],
                text: "a lover of good".into(),
            },
            at: tc_journal::WallTime(0),
        })
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

/// APP-DESIGN §4.2 `minted_token_parses`, `forged_token_is_refused`: a screen cannot make a
/// token the store did not mint (§2.7).
#[test]
fn a_minted_token_parses_and_a_forged_one_is_refused() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let minted = handle.mint(&verse(1, 1), None).unwrap();
    let text = minted.to_text();
    assert_eq!(handle.parse_edit(&text).unwrap(), minted);
    // One character changed anywhere is refused.
    for at in [0, text.len() / 2, text.len() - 1] {
        let mut forged: Vec<char> = text.chars().collect();
        forged[at] = if forged[at] == 'A' { 'B' } else { 'A' };
        let forged: String = forged.into_iter().collect();
        assert!(handle.parse_edit(&forged).is_err(), "changed at {at}");
    }
    // A token another installation minted for the same records is refused.
    let other = rig_as(rig.fs.copy(), "/other-app", "host-1");
    let copy = other.store.open(&folder, &OpenOptions::default()).unwrap();
    let theirs = copy.mint(&verse(1, 1), None).unwrap();
    assert!(handle.parse_edit(&theirs.to_text()).is_err());
    let refused = handle
        .submit(Command::SaveVerse {
            shown: theirs,
            words: "forged".into(),
        })
        .wait();
    assert!(
        matches!(refused, Err(CommandError::BadToken(_))),
        "{refused:?}"
    );
    copy.close().unwrap();
    handle.close().unwrap();
}

/// APP-DESIGN §4.2 `review_token_lists_every_head`, and an edit token of a record in
/// conflict lists only the displayed candidate (§2.7).
#[test]
fn a_review_token_lists_every_head_and_an_edit_token_only_the_shown_one() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let shown = handle.mint(&verse(1, 3), None).unwrap();
    for words in ["one", "two"] {
        handle
            .submit(Command::SaveVerse {
                shown: shown.clone(),
                words: words.into(),
            })
            .wait()
            .unwrap();
    }
    let mut heads = handle.snapshot().fold().heads(&verse(1, 3));
    heads.sort();
    assert_eq!(heads.len(), 2);
    let review = handle.mint_review(&verse(1, 3)).unwrap();
    assert_eq!(review.heads(), heads.as_slice());
    let edit = handle.mint(&verse(1, 3), None).unwrap();
    assert_eq!(edit.heads().len(), 1);
    handle.close().unwrap();
}

/// APP-DESIGN §4.2 `reply_token_lists_only_the_new_change` and `read_your_writes`: the
/// reply's token continues the saved version, and the snapshot its `seq` names holds it.
#[test]
fn a_reply_token_lists_only_the_new_change_and_its_seq_reads_the_write() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let reply = save_verse(&handle, 1, 7, "an overseer").unwrap();
    let (key, token) = reply.tokens.first().unwrap();
    assert_eq!(*key, verse(1, 7));
    assert_eq!(token.heads(), reply.changes.as_slice());
    let snapshot = handle.snapshot();
    assert!(snapshot.seq() >= reply.seq);
    assert!(snapshot.fold().body(&reply.changes[0]).is_some());
    // Saving again from the reply's token continues it: one head, no conflict.
    let next = handle
        .submit(Command::SaveVerse {
            shown: token.clone(),
            words: "an overseer must be".into(),
        })
        .wait()
        .unwrap();
    assert!(next.new_conflicts.is_empty());
    assert_eq!(handle.snapshot().fold().heads(&verse(1, 7)), next.changes);
    handle.close().unwrap();
}

/// APP-DESIGN §4.2 `snapshot_clone_costs_nothing` (§2.6, R5): taking a snapshot is an `Arc`
/// clone of what the actor published, not a copy of the fold.
#[test]
fn a_snapshot_costs_nothing_to_take() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let (first, second) = (handle.snapshot(), handle.snapshot());
    assert!(
        std::ptr::eq(first.fold(), second.fold()),
        "two snapshots of one published state share its fold"
    );
    handle.close().unwrap();
}

/// APP-DESIGN §4.2 `reads_never_wait_behind_a_long_command`, as the owner ruled on 2026-10-18
/// (PERFORMANCE.md): the long part of a checkpoint runs on the writer, so with it held,
/// snapshots and book reads still come back and a save is durable at once. (Before the
/// ruling the save waited its turn behind the checkpoint.)
#[test]
fn reads_never_wait_behind_a_held_checkpoint() {
    let hooks = std::sync::Arc::new(tc_store::Hooks::default());
    let rig = rig_with_hooks(FaultFs::new(), hooks.clone());
    rig.store.wait_cleanup();
    let folder = create_titus(&rig, "Titus");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    hooks.hold(tc_store::Point::BeforeCheckpoint);
    let checkpoint = handle.checkpoint(tc_store::CheckpointReason::LeavingTranslate);
    hooks.wait_reached(tc_store::Point::BeforeCheckpoint, 1);
    let save = handle.submit(Command::SaveVerse {
        shown: handle.mint(&verse(1, 8), None).unwrap(),
        words: "hospitable".into(),
    });
    assert!(handle.snapshot().fold().book_usfm(TIT).is_some());
    assert!(handle.read_book(TIT).wait().usfm.is_some());
    // The hold is released even if an assertion below fails, so the test ends.
    let saved = save.wait();
    let more: Vec<_> = (9..=16)
        .map(|v| save_verse(&handle, 1, v, &format!("during {v}")))
        .collect();
    let still_held = !checkpoint.is_ready();
    hooks.release(tc_store::Point::BeforeCheckpoint);
    assert!(still_held, "the checkpoint is still held on the writer");
    checkpoint.wait().unwrap();
    assert_eq!(saved.unwrap().stored, Stored::Durable);
    for reply in more {
        assert_eq!(reply.unwrap().stored, Stored::Durable);
    }
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// APP-DESIGN §4.2 `section_save_checks_only_displayed_values` (§5.2 step 2, L74): a join
/// made after the section editor drew its screen does not refuse the editor's save; the two
/// are one visible passage conflict.
#[test]
fn a_section_save_checks_only_what_it_displayed() {
    use tc_journal::command::SectionValue;
    use tc_journal::record::VerseValue;
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let text = |v: u16| match handle.snapshot().fold().projected(&verse(1, v)) {
        Some(tc_journal::Record::Verse { value, .. }) => value,
        other => panic!("{other:?}"),
    };
    // The section editor shows 1:5 and 1:6.
    let shown5 = handle.mint(&verse(1, 5), None).unwrap();
    let shown6 = handle.mint(&verse(1, 6), None).unwrap();
    let (five, six) = (text(5), text(6));
    // Meanwhile 6 and 7 are joined.
    let VerseValue::Text { marker, layout, .. } = six.clone() else {
        panic!()
    };
    handle
        .submit(Command::SaveSection {
            book: TIT,
            shown: vec![
                (
                    handle.mint(&verse(1, 6), None).unwrap(),
                    Some(SectionValue::Verse(VerseValue::Text {
                        marker: marker.clone(),
                        words: "six and seven".into(),
                        layout: layout.clone(),
                        last: tc_journal::VerseNo::new(7).unwrap(),
                        from: None,
                    })),
                ),
                (
                    handle.mint(&verse(1, 7), None).unwrap(),
                    Some(SectionValue::Verse(VerseValue::Covered {
                        by: tc_journal::VerseNo::new(6).unwrap(),
                    })),
                ),
            ],
            layout: None,
        })
        .wait()
        .unwrap();
    // The editor's own edit of 5 and 6, from what it displayed.
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
        .submit(Command::SaveSection {
            book: TIT,
            shown: vec![
                (
                    shown5,
                    Some(SectionValue::Verse(edited(five, "five, edited"))),
                ),
                (
                    shown6,
                    Some(SectionValue::Verse(edited(six, "six, edited"))),
                ),
            ],
            layout: None,
        })
        .wait()
        .unwrap();
    assert_eq!(reply.stored, Stored::Durable);
    assert!(!reply.new_conflicts.is_empty(), "a visible conflict");
    let conflict = handle
        .snapshot()
        .fold()
        .conflicts()
        .find(|c| reply.new_conflicts.contains(&c.id))
        .cloned()
        .unwrap();
    assert_eq!(conflict.kind, tc_journal::fold::ConflictKind::Passage);
    handle.close().unwrap();
}

/// APP-DESIGN §4.2 `not_written_keeps_seq_and_retry_uses_the_same_token` (§5.0 B3): a save
/// the disk refused keeps the editor's text and the device's counter, and a retry with the
/// same token writes the change the first attempt would have written.
#[test]
fn a_save_not_written_keeps_its_seq_and_a_retry_with_the_same_token_saves() {
    let (rig, folder) = titus();
    let fs = rig.fs.crash(false);
    let rig = rig_as(fs.clone(), APP, "host-1");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let shown = handle.mint(&verse(1, 9), None).unwrap();
    let before = handle.snapshot().fold().clone();
    let device = handle.facts().device;
    let next_seq = before
        .device_stats()
        .get(&device)
        .map_or(1, |s| s.max_seq.0 + 1);
    // The pending folder exists, so the save's first write point is the temp file's
    // creation.
    fs.fail_op(0, std::io::ErrorKind::StorageFull);
    let refused = handle
        .submit(Command::SaveVerse {
            shown: shown.clone(),
            words: "holding firm".into(),
        })
        .wait();
    assert!(
        matches!(refused, Err(CommandError::NotWritten(_))),
        "{refused:?}"
    );
    assert_eq!(words(&handle, 1, 9), before_words(&before, 9));
    let reply = handle
        .submit(Command::SaveVerse {
            shown,
            words: "holding firm".into(),
        })
        .wait()
        .unwrap();
    let body = handle.snapshot().fold().body(&reply.changes[0]).unwrap();
    assert_eq!(body.seq.0, next_seq, "the refused save used no seq");
    assert_eq!(words(&handle, 1, 9).as_deref(), Some("holding firm"));
    handle.close().unwrap();
}

fn before_words(fold: &tc_journal::fold::FoldState, v: u16) -> Option<String> {
    match fold.projected(&verse(1, v)) {
        Some(tc_journal::Record::Verse {
            value: tc_journal::record::VerseValue::Text { words, .. },
            ..
        }) => Some(words),
        _ => None,
    }
}
