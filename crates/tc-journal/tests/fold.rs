//! The fold's rules (§7.2, §7.3): heads, merges, stub rules, passages, batches, books.

// Helpers outside `#[test]` functions build known-valid values and may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::*;
use tc_journal::fold::{
    AutoRule, BookView, ConflictKind, FoldInput, FoldState, MergeHow, ProjectKind, Resolved,
};
use tc_journal::record::{PresentHow, UserStatus};
use tc_journal::{Cause, RecordKey};

#[test]
fn one_edit_settles_and_the_next_replaces_it() {
    let mut maria = Dev::new(1);
    let seed = seed_book(&mut maria, &[3]);
    let stub = on(&seed, &verse_key(1, 1));
    let edit = maria.one(verse(1, 1, text("Paul, a servant", 1)), vec![stub.id()]);
    let mut all = seed.clone();
    all.push(edit.clone());
    let state = fold(&all);
    assert_eq!(state.heads(&verse_key(1, 1)), vec![edit.id()]);
    assert_eq!(
        projected_words(&state, 1, 1).as_deref(),
        Some("Paul, a servant")
    );
    assert_eq!(state.conflicts().count(), 0);
    assert!(matches!(state.book(BOOK), BookView::Present { .. }));
}

#[test]
fn concurrent_rewordings_conflict_and_the_newest_is_provisional() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let seed = seed_book(&mut maria, &[3]);
    let stub = on(&seed, &verse_key(1, 1));
    let base = maria.one(verse(1, 1, text("Paul, a servant", 1)), vec![stub.id()]);
    let a = maria.one(verse(1, 1, text("Paul, a slave", 1)), vec![base.id()]);
    juan.at = maria.at + 1;
    let b = juan.one(
        verse(1, 1, text("Paul, a servant of God", 1)),
        vec![base.id()],
    );
    let mut all = seed.clone();
    all.extend([base.clone(), a.clone(), b.clone()]);
    let state = fold(&all);
    let conflicts: Vec<_> = state.conflicts().collect();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].kind, ConflictKind::VerseText);
    assert_eq!(conflicts[0].bases, vec![base.id()]);
    assert_eq!(
        conflicts[0].provisional,
        b.id(),
        "the newest by (rank, at, id)"
    );
    assert_eq!(conflicts[0].candidates.len(), 2);
    assert_eq!(
        projected_words(&state, 1, 1).as_deref(),
        Some("Paul, a servant of God")
    );
}

#[test]
fn a_paragraph_break_and_a_rewording_merge_by_units() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let seed = seed_book(&mut maria, &[3]);
    let stub = on(&seed, &verse_key(1, 2));
    let base = maria.one(
        verse(1, 2, text_layout("in hope", "\n", 2)),
        vec![stub.id()],
    );
    let a = maria.one(
        verse(1, 2, text_layout("in the hope", "\n", 2)),
        vec![base.id()],
    );
    let b = juan.one(
        verse(1, 2, text_layout("in hope", "\n\\p\n", 2)),
        vec![base.id()],
    );
    let mut all = seed.clone();
    all.extend([base, a, b]);
    let state = fold(&all);
    assert_eq!(state.conflicts().count(), 0);
    let ks = state.key_state(&verse_key(1, 2)).expect("state");
    assert!(matches!(
        ks.state,
        Resolved::Merged {
            how: MergeHow::Units(_),
            ..
        }
    ));
    let merged = state.projected(&verse_key(1, 2)).expect("merged");
    assert_eq!(
        merged,
        verse(1, 2, text_layout("in the hope", "\n\\p\n", 2))
    );
}

#[test]
fn bookmark_and_status_merge_and_of_two_statuses_the_newest_wins() {
    let mut ana = Dev::new(1);
    let mut pedro = Dev::new(2);
    let key = check_key("abcd", 1, 1);
    let base = ana.one(
        check(&key, decision(Some(UserStatus::Valid), false, None)),
        vec![],
    );
    let bookmark = ana.one(
        check(&key, decision(Some(UserStatus::Valid), true, None)),
        vec![base.id()],
    );
    let status = pedro.one(
        check(&key, decision(Some(UserStatus::Invalid), false, None)),
        vec![base.id()],
    );
    let state = fold(&[base.clone(), bookmark.clone(), status.clone()]);
    assert_eq!(state.conflicts().count(), 0, "different units merge");
    assert_eq!(
        state.projected(&RecordKey::Check(key.clone())),
        Some(check(&key, decision(Some(UserStatus::Invalid), true, None)))
    );
    let other = ana.one(
        check(&key, decision(Some(UserStatus::Todo), false, None)),
        vec![base.id()],
    );
    let state = fold(&[base, status.clone(), other.clone()]);
    // Ruling H5: no conflict; the newest wins and the other stays in History.
    assert_eq!(state.conflicts().count(), 0);
    let newest = [&status, &other]
        .into_iter()
        .max_by_key(|c| (c.body().at, c.id()))
        .unwrap();
    let older = if newest.id() == status.id() {
        &other
    } else {
        &status
    };
    let key = RecordKey::Check(key);
    assert_eq!(
        state.key_state(&key).map(|k| k.state),
        Some(Resolved::SettledByRule {
            head: newest.id(),
            rule: AutoRule::Newest
        })
    );
    assert_eq!(state.projected(&key), Some(newest.body().record.clone()));
    assert!(state.history(&key).contains(&older.id()));
}

#[test]
fn seed_stubs_are_absorbed_and_two_seeds_settle() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let a = seed_book(&mut maria, &[2]);
    let mut b = seed_book(&mut juan, &[2]);
    // Juan's source had a paragraph break after verse 2: his stub differs.
    b.push(juan.with(
        verse(1, 2, text_layout("___", "\n\\p\n", 2)),
        vec![on(&b, &verse_key(1, 2)).id()],
        Cause::Seed,
    ));
    let draft = maria.one(
        verse(1, 1, text("Paul", 1)),
        vec![on(&a, &verse_key(1, 1)).id()],
    );
    let mut all = a.clone();
    all.extend(b.clone());
    all.push(draft.clone());
    let state = fold(&all);
    // Two presences with equal values merge; two stubs settle; a draft absorbs a stub.
    assert_eq!(
        state.conflicts().count(),
        0,
        "{:?}",
        state.conflicts().collect::<Vec<_>>()
    );
    assert_eq!(projected_words(&state, 1, 1).as_deref(), Some("Paul"));
    assert!(matches!(
        state.key_state(&verse_key(1, 2)).map(|k| k.state),
        Some(Resolved::SettledByRule { .. })
    ));
}

#[test]
fn a_join_against_an_edit_of_a_joined_verse_is_one_passage_conflict() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[6]);
    let v4 = on(&book, &verse_key(1, 4));
    let v5 = on(&book, &verse_key(1, 5));
    // Maria joins 4 and 5; Juan edits 5 from before the join.
    let join = maria.batch(
        vec![
            (
                verse(1, 4, text("words of 1:4 words of 1:5", 5)),
                vec![v4.id()],
            ),
            (verse(1, 5, covered(4)), vec![v5.id()]),
        ],
        Cause::Edit,
    );
    juan.at = maria.at + 10;
    let edit = juan.one(verse(1, 5, text("Juan's verse five", 5)), vec![v5.id()]);
    let mut all = book.clone();
    all.extend(join.clone());
    all.push(edit.clone());
    let state = fold(&all);
    let conflicts: Vec<_> = state.conflicts().collect();
    assert_eq!(conflicts.len(), 1, "{conflicts:#?}");
    assert_eq!(conflicts[0].kind, ConflictKind::Passage);
    assert_eq!(conflicts[0].key, verse_key(1, 4));
    assert_eq!(conflicts[0].also_keys, vec![verse_key(1, 5)]);
    // Juan's side is newer: the join is rewound, so 4 and 5 show apart, each once.
    assert_eq!(
        projected_words(&state, 1, 4).as_deref(),
        Some("words of 1:4")
    );
    assert_eq!(
        projected_words(&state, 1, 5).as_deref(),
        Some("Juan's verse five")
    );
}

#[test]
fn removal_hides_nothing_unseen() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let mut book = imported_book(&mut maria, &[3]);
    // Another book stays, so the last-book rule does not keep Titus.
    book.push(maria.one(
        tc_journal::Record::BookPresence {
            book: tc_journal::BookId::Ruth,
            value: tc_journal::record::BookPresence::Present {
                scope: tc_journal::record::Scope::Whole,
                how: PresentHow::Added,
            },
        },
        vec![],
    ));
    let presence = on(&book, &RecordKey::BookPresence(BOOK));
    // Juan edits verse 2; Maria removes the book without having seen it.
    let edit = juan.one(
        verse(1, 2, text("Juan's", 2)),
        vec![on(&book, &verse_key(1, 2)).id()],
    );
    let held: Vec<_> = ids(&book);
    let removal = maria.one(removed(&held), vec![presence.id()]);
    let mut all = book.clone();
    all.push(removal.clone());
    let state = fold(&all);
    assert_eq!(state.book(BOOK), BookView::Removed);
    let mut all_with = all.clone();
    all_with.push(edit);
    let state = fold(&all_with);
    assert!(matches!(
        state.book(BOOK),
        BookView::RemovedWhileEdited { unseen: 1, .. }
    ));
    assert!(
        state
            .conflicts()
            .any(|c| matches!(c.kind, ConflictKind::BookRemovedWhileEdited { unseen: 1 }))
    );
}

#[test]
fn incremental_steps_equal_a_fold_from_scratch() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[6]);
    let v4 = on(&book, &verse_key(1, 4));
    let v5 = on(&book, &verse_key(1, 5));
    let join = maria.batch(
        vec![
            (verse(1, 4, text("joined", 5)), vec![v4.id()]),
            (verse(1, 5, covered(4)), vec![v5.id()]),
        ],
        Cause::Edit,
    );
    let edit = juan.one(verse(1, 5, text("five", 5)), vec![v5.id()]);
    let fix = maria.one(verse(1, 4, text("joined again", 5)), vec![join[0].id()]);
    let mut all = book.clone();
    all.extend(join);
    all.push(edit);
    all.push(fix);
    let full = summary(&fold(&all));
    // One at a time, in reverse: parents arrive after children, batches late.
    let mut state = FoldState::new(ProjectKind::Bible);
    for change in all.iter().rev() {
        state.add([FoldInput::from(change.clone())]);
    }
    assert_eq!(summary(&state), full);
    let mut state = FoldState::new(ProjectKind::Bible);
    for change in &all {
        state.add([FoldInput::from(change.clone())]);
    }
    assert_eq!(summary(&state), full);
}

#[test]
fn restore_text_against_start_empty_is_a_presence_conflict() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[2]);
    let presence = on(&book, &RecordKey::BookPresence(BOOK));
    let removal = maria.one(removed(&ids(&book)), vec![presence.id()]);
    let restore = maria.one(present(PresentHow::RestoredText), vec![removal.id()]);
    juan.at = maria.at - 100_000;
    let empty = juan.batch(
        vec![
            (present(PresentHow::StartedEmpty), vec![removal.id()]),
            (
                verse(1, 1, text("___", 1)),
                vec![on(&book, &verse_key(1, 1)).id()],
            ),
            (
                verse(1, 2, text("___", 2)),
                vec![on(&book, &verse_key(1, 2)).id()],
            ),
        ],
        Cause::StartOver,
    );
    let mut all = book.clone();
    all.extend([removal, restore]);
    all.extend(empty);
    let state = fold(&all);
    assert!(
        state
            .conflicts()
            .any(|c| c.kind == ConflictKind::BookPresence)
    );
    assert!(matches!(state.book(BOOK), BookView::Present { .. }));
}
