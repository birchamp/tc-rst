//! The silent-loss cases of §15 that the fold decides, one test per scenario id. Each
//! asserts what a person sees (the conflict list, the projected verse, the placement of
//! attached records, the anomalies) and the derived bytes the projection would write; the
//! same names in `tc-store` and `tc-sync` add the files on disk.

// Helpers outside `#[test]` functions build known-valid values and may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::{BTreeMap, BTreeSet};
use support::*;
use tc_journal::command::Choice;
use tc_journal::fold::{
    Anomaly, BookView, ConflictKind, DerivedFile, FoldInput, FoldState, MergeHow, Placement,
    ProjectKind, Resolved, Side, decode_decisions,
};
use tc_journal::json::Json;
use tc_journal::record::{
    BatchReleaseValue, ChapterFront, LayoutItem, PresentHow, UserStatus, UsfmBytes, VerseValue,
};
use tc_journal::{
    BookId, Cause, Change, ChangeId, MetaField, MetaPath, PinGroup, Record, RecordKey, ToolId,
};

fn all_of(parts: &[&[Change]]) -> Vec<Change> {
    parts.iter().flat_map(|p| p.iter().cloned()).collect()
}

fn the_conflict(state: &FoldState) -> tc_journal::fold::Conflict {
    let conflicts: Vec<_> = state.conflicts().cloned().collect();
    assert_eq!(conflicts.len(), 1, "{conflicts:#?}");
    conflicts.into_iter().next().expect("one")
}

/// Every word of a book's original verses `c:1..=n`, as `imported_book` wrote them.
fn original_words(chapters: &[u16]) -> Vec<String> {
    let mut texts = Vec::new();
    for (i, &count) in chapters.iter().enumerate() {
        for v in 1..=count {
            texts.push(format!("words of {}:{v}", i + 1));
        }
    }
    words(&texts.iter().map(String::as_str).collect::<Vec<_>>())
}

/// Each projected `Text` value of a verse has its words in the USFM exactly once.
fn every_text_once(state: &FoldState) {
    let text = usfm(state);
    for key in state.record_keys() {
        if let RecordKey::Verse(..) = key
            && let Some(words) = state.projected(&key).as_ref().and_then(words_of)
            && words != "___"
        {
            assert_eq!(
                text.matches(words.as_str()).count(),
                1,
                "{key:?}: {words:?} in\n{text}"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------
// 15.1
// ---------------------------------------------------------------------------------------

fn two_spans(juan_first: bool) -> Vec<Change> {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[6]);
    juan.at = maria.at;
    if juan_first {
        maria.at += 3_600_000;
    } else {
        juan.at += 3_600_000;
    }
    let a = join(&mut maria, &book, 1, 1, 2);
    let b = join(&mut juan, &book, 1, 4, 5);
    // Each keeps working in the book after the span.
    let maria_later = maria.one(
        verse(1, 3, text("Maria after her span", 3)),
        vec![on(&book, &verse_key(1, 3)).id()],
    );
    let juan_later = juan.one(
        verse(1, 6, text("Juan after his span", 6)),
        vec![on(&book, &verse_key(1, 6)).id()],
    );
    all_of(&[&book, &a, &b, &[maria_later, juan_later]])
}

fn assert_two_spans(state: &FoldState) {
    assert_eq!(
        state.conflict_count(),
        0,
        "{:#?}",
        state.conflicts().collect::<Vec<_>>()
    );
    let text = usfm(state);
    for line in [
        "\\v 1-2 words of 1:1 words of 1:2\n",
        "\\v 3 Maria after her span\n",
        "\\v 4-5 words of 1:4 words of 1:5\n",
        "\\v 6 Juan after his span\n",
    ] {
        assert!(text.contains(line), "{line:?} in\n{text}");
    }
}

/// L1: two concurrent spans in one book write different keys; neither is pended and every
/// later edit by either author projects.
#[test]
fn loss_e1_two_spans_one_book() {
    let state = assert_converges(&two_spans(false));
    assert_two_spans(&state);
}

/// L1, E1 reversed: the same with the other span made first and every fold order.
#[test]
fn loss_e1_reversed_order() {
    let mut changes = two_spans(true);
    changes.reverse();
    let state = assert_converges(&changes);
    assert_two_spans(&state);
}

/// L2: spans that share a verse are one conflict over both spans, described in words,
/// and the provisional text shows every word once.
#[test]
fn loss_e1_overlapping_spans_conflict() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[6]);
    juan.at = maria.at + 1_000;
    let a = join(&mut maria, &book, 1, 1, 2);
    let b = join(&mut juan, &book, 1, 2, 3);
    let state = assert_converges(&all_of(&[&book, &a, &b]));
    let conflict = the_conflict(&state);
    assert_eq!(conflict.kind, ConflictKind::Passage);
    let keys: BTreeSet<RecordKey> = std::iter::once(conflict.key.clone())
        .chain(conflict.also_keys.iter().cloned())
        .collect();
    assert_eq!(
        keys,
        (1..=3).map(|v| verse_key(1, v)).collect::<BTreeSet<_>>()
    );
    let sides: BTreeSet<Side> = conflict.candidates.iter().filter_map(|c| c.side).collect();
    assert_eq!(sides.len(), 2, "one side per span");
    assert_eq!(usfm_words(&usfm(&state)), original_words(&[6]));
    every_text_once(&state);
}

/// L3 (E2a): a span while a teammate edits another verse with an older clock: no
/// pending state, no conflict, both project.
#[test]
fn loss_e2a_span_vs_edit_elsewhere() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[6]);
    juan.at = maria.at - 10 * 86_400_000;
    let edit = juan.one(
        verse(1, 6, text("Juan's six", 6)),
        vec![on(&book, &verse_key(1, 6)).id()],
    );
    let span = join(&mut maria, &book, 1, 1, 2);
    let later = maria.one(
        verse(1, 4, text("Maria's four", 4)),
        vec![on(&book, &verse_key(1, 4)).id()],
    );
    let state = assert_converges(&all_of(&[&book, &span, &[edit, later]]));
    assert_eq!(state.conflict_count(), 0);
    assert_eq!(projected_words(&state, 1, 6).as_deref(), Some("Juan's six"));
    assert_eq!(
        projected_words(&state, 1, 4).as_deref(),
        Some("Maria's four")
    );
    assert!(usfm(&state).contains("\\v 1-2 words of 1:1 words of 1:2\n"));
}

/// L4 (E2b): a span writes only the verses of its range, so no one gets a conflict they
/// did not cause, now or after the next edits on either side.
#[test]
fn loss_e2b_no_phantom_conflict() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[6]);
    let span = join(&mut maria, &book, 1, 1, 2);
    let written: BTreeSet<RecordKey> = span.iter().map(|c| c.body().record.key()).collect();
    assert_eq!(written, [verse_key(1, 1), verse_key(1, 2)].into());
    let three = juan.one(
        verse(1, 3, text("Juan's three", 3)),
        vec![on(&book, &verse_key(1, 3)).id()],
    );
    let five = juan.one(
        verse(1, 5, text("Juan's five", 5)),
        vec![on(&book, &verse_key(1, 5)).id()],
    );
    let mut all = all_of(&[&book, &span, &[three, five]]);
    let state = fold(&all);
    assert_eq!(state.conflict_count(), 0);
    // Each continues from what the screen shows.
    let maria_five = maria.one(
        verse(1, 5, text("Maria's five", 5)),
        state.shown_heads(&verse_key(1, 5), None),
    );
    let juan_one = juan.one(
        verse(1, 1, text("Juan's span text", 2)),
        state.shown_heads(&verse_key(1, 1), None),
    );
    all.extend([maria_five, juan_one]);
    let state = assert_converges(&all);
    assert_eq!(state.conflict_count(), 0);
    for v in 3..=6 {
        assert!(matches!(
            state.key_state(&verse_key(1, v)).map(|k| k.state),
            Some(Resolved::Settled { .. })
        ));
    }
}

fn join_vs_edit() -> (Vec<Change>, Change) {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[6]);
    juan.at = maria.at - 30_000;
    let edit = juan.one(
        verse(1, 5, text("Juan's verse five", 5)),
        vec![on(&book, &verse_key(1, 5)).id()],
    );
    let span = join(&mut maria, &book, 1, 4, 5);
    (all_of(&[&book, std::slice::from_ref(&edit), &span]), edit)
}

/// L5 (E3, F2): an edit of a verse a teammate merged away is a conflict, never retained
/// out of sight; the provisional text keeps every word of the passage once.
#[test]
fn loss_e3_edit_vs_merge_away() {
    let (all, edit) = join_vs_edit();
    let state = assert_converges(&all);
    let conflict = the_conflict(&state);
    assert_eq!(conflict.kind, ConflictKind::Passage);
    let juan = conflict
        .candidates
        .iter()
        .find(|c| c.head == edit.id())
        .expect("Juan's edit is a candidate");
    assert_eq!(juan.side, Some(Side::Others));
    assert_eq!(words_of(&juan.value).as_deref(), Some("Juan's verse five"));
    assert!(usfm(&state).contains("\\v 4-5 words of 1:4 words of 1:5\n"));
    assert_eq!(usfm_words(&usfm(&state)), original_words(&[6]));
    let listed = rendered(&state, &DerivedFile::Conflicts).expect("conflicts.json");
    assert_eq!(listed["conflicts"].as_array().map(Vec::len), Some(1));
}

/// L5: *Keep 4 and 5 separate* restores both verses in one action.
#[test]
fn loss_span_resolution_restores_both_verses() {
    let (mut all, _) = join_vs_edit();
    let state = fold(&all);
    let id = the_conflict(&state).id;
    let mut ana = Dev::new(3);
    let planned = state.keep_separate(&id, &[]).expect("offered");
    assert_eq!(planned.len(), 2, "one batch writes both verses");
    all.extend(ana.apply(planned));
    let state = assert_converges(&all);
    assert_eq!(state.conflict_count(), 0);
    let text = usfm(&state);
    assert!(
        text.contains("\\v 4 words of 1:4\n\\v 5 Juan's verse five\n"),
        "{text}"
    );
}

/// Maria renumbers 1:6–2:4 one number later: 1:6 moves to 2:1, and so on, and 2:5 is new.
fn renumber(dev: &mut Dev, view: &[Change]) -> Vec<Change> {
    let h = |c, v| latest(view, &verse_key(c, v)).id();
    dev.batch(
        vec![
            (verse(1, 6, absent(Some((2, 1)))), vec![h(1, 6)]),
            (
                verse(2, 1, text_from("words of 1:6", 1, (1, 6))),
                vec![h(2, 1)],
            ),
            (
                verse(2, 2, text_from("words of 2:1", 2, (2, 1))),
                vec![h(2, 2)],
            ),
            (
                verse(2, 3, text_from("words of 2:2", 3, (2, 2))),
                vec![h(2, 3)],
            ),
            (
                verse(2, 4, text_from("words of 2:3", 4, (2, 3))),
                vec![h(2, 4)],
            ),
            (verse(2, 5, text_from("words of 2:4", 5, (2, 4))), vec![]),
        ],
        Cause::Edit,
    )
}

/// The renumber against Juan's edit of 2:2 by number (the text that is now at 2:3).
fn renumber_vs_edit(juan_newer: bool) -> (Vec<Change>, Vec<Change>, Change) {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[6, 4]);
    juan.at = maria.at + if juan_newer { 3_600_000 } else { -3_600_000 };
    let edit = juan.one(
        verse(2, 2, text("words of 2:2 as Juan has it", 2)),
        vec![on(&book, &verse_key(2, 2)).id()],
    );
    let moved = renumber(&mut maria, &book);
    (
        all_of(&[&book, &moved, std::slice::from_ref(&edit)]),
        moved,
        edit,
    )
}

/// L6 (F12): a renumber against an edit of a verse it moved is one `Passage` conflict;
/// the newer side shows whole and a resolution needs no ancestry condition.
#[test]
fn loss_renumber_vs_edit() {
    let (mut all, moved, edit) = renumber_vs_edit(true);
    let state = assert_converges(&all);
    let conflict = the_conflict(&state);
    assert_eq!(conflict.kind, ConflictKind::Passage);
    assert_eq!(conflict.also_keys.len(), 5, "the whole passage, once");
    // Juan's side is newer: the renumber is rewound, so the old numbering shows with
    // Juan's edit, 2:5 does not exist, and every word shows once.
    assert_eq!(
        projected_words(&state, 1, 6).as_deref(),
        Some("words of 1:6")
    );
    assert_eq!(
        projected_words(&state, 2, 2).as_deref(),
        Some("words of 2:2 as Juan has it")
    );
    assert_eq!(state.projected(&verse_key(2, 5)), None);
    every_text_once(&state);
    // *Keep Maria's passage*.
    let pick = conflict
        .candidates
        .iter()
        .find(|c| moved.iter().any(|m| m.id() == c.head))
        .expect("Maria's side");
    let mut ana = Dev::new(3);
    let planned = state
        .resolution(&conflict.id, &Choice::Candidate { head: pick.head })
        .expect("always possible");
    all.extend(ana.apply(planned));
    let state = assert_converges(&all);
    assert_eq!(state.conflict_count(), 0);
    assert_eq!(
        projected_words(&state, 2, 3).as_deref(),
        Some("words of 2:2")
    );
    assert!(state.history(&verse_key(2, 2)).contains(&edit.id()));
}

/// L6, L68: a renumber that shifts verses against an edit by verse number never shows one
/// verse twice or drops another, and *Combine* applies the edit where the text now is.
#[test]
fn loss_renumber_shift_vs_edit_by_number() {
    let (mut all, _, edit) = renumber_vs_edit(false);
    let state = assert_converges(&all);
    let conflict = the_conflict(&state);
    assert!(conflict.candidates.iter().any(|c| c.head == edit.id()));
    // Maria's side is newer: the renumbered text, each word once.
    let text = usfm(&state);
    assert!(text.contains("\\v 3 words of 2:2\n"), "{text}");
    assert!(!text.contains("\\v 6"), "{text}");
    assert_eq!(usfm_words(&text), original_words(&[6, 4]));
    every_text_once(&state);
    let combined = state
        .combine_passage(&conflict.id)
        .expect("disjoint changes");
    let at_23 = combined
        .iter()
        .find(|r| r.key() == verse_key(2, 3))
        .expect("2:3");
    assert_eq!(
        words_of(at_23).as_deref(),
        Some("words of 2:2 as Juan has it")
    );
    let mut ana = Dev::new(3);
    let planned = state
        .resolution(&conflict.id, &Choice::Values(combined))
        .expect("values of the conflict's records");
    all.extend(ana.apply(planned));
    let state = assert_converges(&all);
    assert_eq!(state.conflict_count(), 0);
    let text = usfm(&state);
    assert!(
        text.contains("\\v 3 words of 2:2 as Juan has it\n"),
        "{text}"
    );
    every_text_once(&state);
}

/// L68: alignments and comments made for a verse follow its text through a renumber.
#[test]
fn loss_renumber_alignment_and_comments_follow_text() {
    let mut maria = Dev::new(1);
    let mut ana = Dev::new(3);
    let book = imported_book(&mut maria, &[6, 4]);
    let aligned = ana.one(alignment(2, 2, 2, "words of 2:2"), vec![]);
    let note = ana.one(comment(9, verse_target(2, 2), "check this"), vec![]);
    let moved = renumber(&mut maria, &book);
    let state = assert_converges(&all_of(&[&book, &[aligned, note], &moved]));
    assert_eq!(state.conflict_count(), 0);
    assert_eq!(
        state.placement(&RecordKey::Alignment(BOOK, ch(2), vn(2))),
        Placement::Run {
            chapter: 2,
            verse: 3
        }
    );
    assert_eq!(
        state.placement(&RecordKey::Comment(tc_journal::CommentId::from_bytes(
            [9; 16]
        ))),
        Placement::Run {
            chapter: 2,
            verse: 3
        }
    );
    assert_eq!(state.detached().count(), 0);
    let comments = rendered(&state, &DerivedFile::Comments(BOOK)).expect("comments");
    assert_eq!(
        comments["comments"][0]["placed"],
        serde_json::json!({"chapter": 2, "verse": 3})
    );
    let alignments = rendered(&state, &DerivedFile::Alignments(BOOK)).expect("alignments");
    assert_eq!(
        alignments["chapters"]["2"]["3"]["invalid"], false,
        "made for this text: {alignments}"
    );
}

/// L7 (J18): the same verse edited two ways is a conflict with the base text shown.
#[test]
fn loss_same_verse_two_ways() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[3]);
    let base = latest(&book, &verse_key(1, 1));
    let a = maria.one(verse(1, 1, text("Paul, a slave", 1)), vec![base.id()]);
    let b = juan.one(
        verse(1, 1, text("Paul, a servant of God", 1)),
        vec![base.id()],
    );
    let state = assert_converges(&all_of(&[&book, &[a.clone(), b.clone()]]));
    let conflict = the_conflict(&state);
    assert_eq!(conflict.kind, ConflictKind::VerseText);
    assert_eq!(conflict.bases, vec![base.id()]);
    let shown_base = state.body(&conflict.bases[0]).expect("base").record;
    assert_eq!(words_of(&shown_base).as_deref(), Some("words of 1:1"));
    let values: BTreeSet<String> = conflict
        .candidates
        .iter()
        .filter_map(|c| words_of(&c.value))
        .collect();
    assert_eq!(
        values,
        [
            "Paul, a slave".to_owned(),
            "Paul, a servant of God".to_owned()
        ]
        .into()
    );
    assert!(conflict.candidates.iter().any(|c| c.head == a.id()));
    assert!(conflict.candidates.iter().any(|c| c.head == b.id()));
}

/// L8 (E4, F3, G7): two alignments of one verse are a listed conflict, not newest-wins.
#[test]
fn loss_e4_alignment_conflict_visible() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[3]);
    let a = maria.one(alignment(1, 1, 1, "words of 1:1"), vec![]);
    let b = juan.one(alignment(1, 1, 1, "words of"), vec![]);
    let state = assert_converges(&all_of(&[&book, &[a, b]]));
    let conflict = the_conflict(&state);
    assert_eq!(conflict.kind, ConflictKind::Alignment);
    assert_eq!(conflict.candidates.len(), 2);
}

fn statuses(first: UserStatus, second: UserStatus, second_bookmarks: bool) -> FoldState {
    let mut ana = Dev::new(1);
    let mut pedro = Dev::new(2);
    let book = imported_book(&mut ana, &[1]);
    let key = check_key("abcd", 1, 1);
    let base = ana.one(
        check(&key, decision(Some(UserStatus::Todo), false, None)),
        vec![],
    );
    let a = ana.one(
        check(&key, decision(Some(first), false, None)),
        vec![base.id()],
    );
    let b = pedro.one(
        check(&key, decision(Some(second), second_bookmarks, None)),
        vec![base.id()],
    );
    assert_converges(&all_of(&[&book, &[base, a, b]]))
}

/// L8: two check results are a listed conflict on the disputed unit.
#[test]
fn loss_check_two_ways_visible() {
    let state = statuses(UserStatus::Valid, UserStatus::Invalid, false);
    let conflict = the_conflict(&state);
    assert_eq!(conflict.kind, ConflictKind::Check);
    assert_eq!(conflict.disputed.len(), 1);
    assert_eq!(&*conflict.disputed[0].0, "/decision/judgement");
}

/// L8: a bookmark against a status is no conflict: decisions merge by unit first.
#[test]
fn merge_bookmark_and_status() {
    let state = statuses(UserStatus::Valid, UserStatus::Todo, true);
    assert_eq!(state.conflict_count(), 0);
    let key = RecordKey::Check(check_key("abcd", 1, 1));
    assert!(matches!(
        state.key_state(&key).map(|k| k.state),
        Some(Resolved::Merged {
            how: MergeHow::Units(_),
            ..
        })
    ));
    assert_eq!(
        state.projected(&key),
        Some(check(
            &check_key("abcd", 1, 1),
            decision(Some(UserStatus::Valid), true, None)
        ))
    );
}

fn gateway(language: &str, with_words: bool) -> Record {
    let slot = |name: &str| {
        obj(&[
            ("languageId", Json::from(language)),
            (
                "repoPath",
                Json::from(format!("git.door43.org/org/{language}_{name}")),
            ),
            ("sha", Json::from(oid(language.len() as u8).to_hex())),
        ])
    };
    let mut slots = vec![
        ("gatewayLanguage", Json::from(language)),
        ("translationNotes", slot("tn")),
    ];
    if with_words {
        slots.push(("translationWords", slot("tw")));
    }
    pins(PinGroup::Primary, &slots)
}

/// L9: two gateway changes conflict as one pin group; the projection never mixes slots
/// of two heads.
#[test]
fn loss_two_gateway_changes_never_mix() {
    for juan_newer in [true, false] {
        let mut maria = Dev::new(1);
        let mut juan = Dev::new(2);
        let book = imported_book(&mut maria, &[1]);
        let base = maria.one(gateway("en", true), vec![]);
        juan.at = maria.at + if juan_newer { 1_000 } else { -1_000 };
        let es = maria.one(gateway("es", true), vec![base.id()]);
        let fr = juan.one(gateway("fr", false), vec![base.id()]);
        let state = assert_converges(&all_of(&[&book, &[base, es.clone(), fr.clone()]]));
        let conflict = the_conflict(&state);
        assert_eq!(conflict.kind, ConflictKind::Pins);
        let newer = if juan_newer { &fr } else { &es };
        assert_eq!(
            state.projected(&RecordKey::Pins(PinGroup::Primary)),
            Some(newer.body().record.clone()),
            "the provisional head's whole value"
        );
        let resources = String::from_utf8(
            state
                .render(&DerivedFile::Resources)
                .expect("resources.json"),
        )
        .expect("utf-8");
        let (shown, hidden) = if juan_newer {
            ("fr", "es")
        } else {
            ("es", "fr")
        };
        assert!(resources.contains(&format!("{shown}_tn")));
        assert!(!resources.contains(&format!("{hidden}_")), "{resources}");
        assert_eq!(resources.contains("_tw"), !juan_newer, "{resources}");
    }
}

/// L10: two people add one book with different payloads: stubs settle by rule, real
/// differences are conflicts per verse.
#[test]
fn loss_two_book_adds() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let seeded = seed_book(&mut maria, &[3]);
    let draft = maria.one(
        verse(1, 1, text("Maria's draft", 1)),
        vec![on(&seeded, &verse_key(1, 1)).id()],
    );
    let imported = imported_book(&mut juan, &[3]);
    let state = assert_converges(&all_of(&[&seeded, std::slice::from_ref(&draft), &imported]));
    let conflict = the_conflict(&state);
    assert_eq!(conflict.key, verse_key(1, 1));
    assert_eq!(conflict.kind, ConflictKind::VerseText);
    assert_eq!(conflict.candidates.len(), 2);
    for v in 2..=3 {
        assert_eq!(
            projected_words(&state, 1, v),
            Some(format!("words of 1:{v}"))
        );
    }
    assert!(matches!(state.book(BOOK), BookView::Present { .. }));
}

fn two_resolutions(same: bool) -> (FoldState, Change, Change) {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[3]);
    let base = latest(&book, &verse_key(1, 1));
    let a = maria.one(verse(1, 1, text("Maria's", 1)), vec![base.id()]);
    let b = juan.one(verse(1, 1, text("Juan's", 1)), vec![base.id()]);
    let mut all = all_of(&[&book, &[a.clone(), b.clone()]]);
    let state = fold(&all);
    let id = the_conflict(&state).id;
    let keep = |head: &Change| {
        state
            .resolution(&id, &Choice::Candidate { head: head.id() })
            .expect("possible")
    };
    all.extend(maria.apply(keep(&a)));
    all.extend(juan.apply(keep(if same { &a } else { &b })));
    (assert_converges(&all), a, b)
}

/// L11: two people resolve one conflict differently: a new conflict over the two
/// resolutions, whose bases are the old heads; equal resolutions merge.
#[test]
fn resolve_differently_reconflicts() {
    let (state, a, b) = two_resolutions(false);
    let conflict = the_conflict(&state);
    let mut bases = vec![a.id(), b.id()];
    bases.sort();
    assert_eq!(conflict.bases, bases);
    assert!(
        conflict
            .candidates
            .iter()
            .all(|c| matches!(c.cause, Cause::Resolve)),
        "labelled resolved differently"
    );
    let (state, _, _) = two_resolutions(true);
    assert_eq!(state.conflict_count(), 0);
    assert!(matches!(
        state.key_state(&verse_key(1, 1)).map(|k| k.state),
        Some(Resolved::Merged {
            how: MergeHow::EqualValues,
            ..
        })
    ));
    assert_eq!(projected_words(&state, 1, 1).as_deref(), Some("Maria's"));
}

/// L12 (E5, F6): a change that passes the hash check but breaks a rule is quarantined
/// and listed; its batch waits; everything else folds and projects.
#[test]
fn loss_e5_poison_never_blocks() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[3]);
    let section = juan.batch(
        vec![
            (
                verse(1, 1, text("held one", 1)),
                vec![on(&book, &verse_key(1, 1)).id()],
            ),
            (
                verse(1, 2, text("poisoned two", 2)),
                vec![on(&book, &verse_key(1, 2)).id()],
            ),
        ],
        Cause::Edit,
    );
    // The second line repeats a parent: rule V5.
    let poison = forged(&section[1], |map| {
        let parent = on(&book, &verse_key(1, 2)).id().to_hex();
        map.insert(
            "parents".into(),
            Json::Arr(vec![Json::from(parent.clone()), Json::from(parent)]),
        );
    });
    let poison_id = poison.id();
    let garbage = FoldInput::new(
        tc_journal::change::parse_change(
            ChangeId::of(b"{\"not\":\"a change\"}"),
            std::sync::Arc::from(&b"{\"not\":\"a change\"}"[..]),
        ),
        None,
    );
    let fine = juan.one(
        verse(1, 3, text("Juan's three", 3)),
        vec![on(&book, &verse_key(1, 3)).id()],
    );
    let inputs: Vec<FoldInput> = book
        .iter()
        .cloned()
        .map(FoldInput::from)
        .chain([
            FoldInput::from(section[0].clone()),
            poison,
            garbage,
            FoldInput::from(fine),
        ])
        .collect();
    let state = FoldState::fold(ProjectKind::Bible, inputs);
    assert!(
        state.anomalies().any(
            |a| matches!(a, Anomaly::QuarantinedChange { change, .. } if *change == poison_id)
        )
    );
    assert_eq!(
        state
            .anomalies()
            .filter(|a| matches!(a, Anomaly::QuarantinedChange { .. }))
            .count(),
        2
    );
    assert_eq!(
        projected_words(&state, 1, 1).as_deref(),
        Some("words of 1:1")
    );
    assert_eq!(
        projected_words(&state, 1, 3).as_deref(),
        Some("Juan's three")
    );
    derived_bytes(&state);
}

/// L13 (E6): a fast clock wins no rule that decides survival; it only picks the
/// provisional text, and nothing ratchets.
#[test]
fn loss_e6_fast_clock_cannot_hide() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[3]);
    juan.at = maria.at + 365 * 86_400_000;
    let base = latest(&book, &verse_key(1, 1));
    let fast = juan.one(verse(1, 1, text("Juan, a year ahead", 1)), vec![base.id()]);
    let slow = maria.one(verse(1, 1, text("Maria, on time", 1)), vec![base.id()]);
    let mut all = all_of(&[&book, &[fast.clone(), slow.clone()]]);
    let state = fold(&all);
    let conflict = the_conflict(&state);
    assert_eq!(conflict.provisional, fast.id(), "the clock picks only this");
    assert!(conflict.candidates.iter().any(|c| c.head == slow.id()));
    // Maria keeps her version with a clock a year behind Juan's change: it wins.
    let keep = state
        .resolution(&conflict.id, &Choice::Candidate { head: slow.id() })
        .expect("possible");
    all.extend(maria.apply(keep));
    // An edit after seeing Juan's fast change replaces it, whatever the clocks say.
    let fast_two = juan.one(
        verse(1, 2, text("Juan's two", 2)),
        vec![on(&book, &verse_key(1, 2)).id()],
    );
    let after = maria.one(verse(1, 2, text("Maria's two", 2)), vec![fast_two.id()]);
    all.extend([fast_two, after]);
    let state = assert_converges(&all);
    assert_eq!(state.conflict_count(), 0);
    assert_eq!(
        projected_words(&state, 1, 1).as_deref(),
        Some("Maria, on time")
    );
    assert_eq!(
        projected_words(&state, 1, 2).as_deref(),
        Some("Maria's two")
    );
}

/// L15 (F5): a stale screen names the heads it showed, so its save is a conflict with
/// the teammate's newer text, never a revert.
#[test]
fn loss_f5_stale_buffer_never_reverts() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[3]);
    let shown = fold(&book).shown_heads(&verse_key(1, 1), None);
    let newer = juan.one(verse(1, 1, text("Juan's newer text", 1)), shown.clone());
    let stale = maria.one(verse(1, 1, text("words of 1:1, retyped", 1)), shown);
    let state = assert_converges(&all_of(&[&book, &[newer.clone(), stale]]));
    let conflict = the_conflict(&state);
    assert!(conflict.candidates.iter().any(|c| c.head == newer.id()));
}

/// L16 (F7): two machines with one identity: concurrent edits are a conflict, nothing
/// collides, and the duplicate device is listed.
#[test]
fn loss_f7_one_identity_two_machines() {
    let mut desk = Dev::new(1);
    let book = imported_book(&mut desk, &[3]);
    let mut laptop = desk.twin();
    let base = latest(&book, &verse_key(1, 1));
    let a = desk.one(verse(1, 1, text("from the desk", 1)), vec![base.id()]);
    let b = laptop.one(verse(1, 1, text("from the laptop", 1)), vec![base.id()]);
    assert_eq!(a.body().seq, b.body().seq);
    let state = assert_converges(&all_of(&[&book, &[a.clone(), b.clone()]]));
    let conflict = the_conflict(&state);
    assert_eq!(conflict.candidates.len(), 2);
    assert!(state.anomalies().any(|x| matches!(
        x,
        Anomaly::DuplicateDevice { changes, .. } if changes.contains(&a.id()) && changes.contains(&b.id())
    )));
}

/// L25: a record kind from a newer minor is kept, listed and carried, and two values of
/// it are a conflict for the newer app to resolve.
#[test]
fn loss_newer_minor_carried() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[3]);
    let newer = |dev: &mut Dev, n: u8| {
        let carrier = dev.one(verse(1, 1, text("carrier", 1)), vec![]);
        forged(&carrier, |map| {
            map.insert("v".into(), Json::from(tc_journal::FORMAT_MINOR + 1));
            map.insert(
                "record".into(),
                obj(&[(
                    "verseAudio",
                    obj(&[
                        ("book", Json::from("TIT")),
                        ("chapter", Json::from(1u16)),
                        ("verse", Json::from(1u16)),
                        ("value", obj(&[("oid", Json::from(oid(n).to_hex()))])),
                    ]),
                )]),
            );
        })
    };
    let a = newer(&mut maria, 1);
    let b = newer(&mut juan, 2);
    let inputs: Vec<FoldInput> = book
        .iter()
        .cloned()
        .map(FoldInput::from)
        .chain([a, b])
        .collect();
    let state = FoldState::fold(ProjectKind::Bible, inputs);
    assert!(state.anomalies().any(|x| matches!(
        x,
        Anomaly::UnknownKinds { kinds, changes: 2 } if kinds.iter().any(|k| &**k == "verseAudio")
    )));
    let conflict = the_conflict(&state);
    assert_eq!(conflict.kind, ConflictKind::Unknown);
    // Nothing of it reaches the projection.
    assert_eq!(state.book_usfm(BOOK), fold(&book).book_usfm(BOOK));
}

/// L27: decision files read only the projected pins with their recorded coverage, so the
/// same changes give the same bytes on every computer.
#[test]
fn projection_ignores_installed_resources() {
    let notes = |repo: &str, n: u8, books: Option<&[&str]>| {
        let mut fields = vec![
            ("repoPath", Json::from(repo)),
            ("sha", Json::from(oid(n).to_hex())),
            ("version", Json::from(format!("v{n}"))),
        ];
        if let Some(books) = books {
            fields.push((
                "books",
                Json::Arr(books.iter().map(|b| Json::from(*b)).collect()),
            ));
        }
        obj(&fields)
    };
    let mut ana = Dev::new(1);
    let book = imported_book(&mut ana, &[3]);
    let primary = ana.one(
        pins(
            PinGroup::Primary,
            &[(
                "translationNotes",
                notes("git.door43.org/unfoldingWord/en_tn", 7, Some(&["RUT"])),
            )],
        ),
        vec![],
    );
    let fallback = ana.one(
        pins(
            PinGroup::Fallback,
            &[(
                "translationNotes",
                notes("git.door43.org/unfoldingWord/fr_tn", 8, None),
            )],
        ),
        vec![],
    );
    let key = check_key("abcd", 1, 1);
    let made = ana.one(
        check(&key, decision(Some(UserStatus::Valid), false, None)),
        vec![],
    );
    let mut all = all_of(&[&book, &[primary.clone(), fallback, made]]);
    let file = DerivedFile::Decisions(ToolId::TranslationNotes, BOOK);
    let state = assert_converges(&all);
    let doc = rendered(&state, &file).expect("decisions");
    assert_eq!(
        doc["resource"]["repoPath"], "git.door43.org/unfoldingWord/fr_tn",
        "the primary pin does not cover Titus"
    );
    assert_eq!(doc["decisions"][0]["invalidated"], true);
    all.push(ana.one(
        pins(
            PinGroup::Primary,
            &[(
                "translationNotes",
                notes(
                    "git.door43.org/unfoldingWord/en_tn",
                    7,
                    Some(&["RUT", "TIT"]),
                ),
            )],
        ),
        vec![primary.id()],
    ));
    let state = assert_converges(&all);
    let doc = rendered(&state, &file).expect("decisions");
    assert_eq!(
        doc["resource"]["repoPath"],
        "git.door43.org/unfoldingWord/en_tn"
    );
}

// ---------------------------------------------------------------------------------------
// 15.2
// ---------------------------------------------------------------------------------------

/// Ana's carry-over moves check K1 to K2 while Pedro changes K1 offline.
fn carry_over() -> (Vec<Change>, tc_journal::CheckKey, tc_journal::CheckKey) {
    let mut ana = Dev::new(1);
    let mut pedro = Dev::new(2);
    let old = check_key("abcd", 1, 1);
    let new = check_key("efgh", 1, 1);
    let base = ana.one(
        check(
            &old,
            decision(Some(UserStatus::Valid), true, Some("keep me")),
        ),
        vec![],
    );
    let upgrade = ana.batch(
        vec![
            (moved(&old, &new), vec![base.id()]),
            (
                check(
                    &new,
                    decision(Some(UserStatus::Valid), true, Some("keep me")),
                ),
                vec![],
            ),
        ],
        Cause::Edit,
    );
    let offline = pedro.one(
        check(
            &old,
            decision(Some(UserStatus::Invalid), true, Some("Pedro's note")),
        ),
        vec![base.id()],
    );
    (all_of(&[&[base], &upgrade, &[offline]]), old, new)
}

/// L35: a carry-over against an offline check is a `CheckMoved` conflict, and *Apply
/// Pedro's change to the moved check* lands his verdict on the live key.
#[test]
fn loss_carryover_move_vs_offline_check() {
    let (mut all, old, new) = carry_over();
    let mut maria = Dev::new(4);
    all.extend(imported_book(&mut maria, &[3]));
    let state = assert_converges(&all);
    let conflict = the_conflict(&state);
    assert_eq!(conflict.kind, ConflictKind::CheckMoved);
    assert_eq!(conflict.key, RecordKey::Check(old.clone()));
    let planned = state.apply_to_moved_check(&conflict.id).expect("offered");
    let mut ana = Dev::new(1);
    ana.at += 10_000_000;
    all.extend(ana.apply(planned));
    let state = assert_converges(&all);
    assert_eq!(state.conflict_count(), 0);
    assert_eq!(
        state.projected(&RecordKey::Check(new.clone())),
        Some(check(
            &new,
            decision(Some(UserStatus::Invalid), true, Some("Pedro's note"))
        ))
    );
    assert_eq!(
        state.projected(&RecordKey::Check(old.clone())),
        Some(moved(&old, &new))
    );
}

/// L36: a re-key moves the whole decision, comment and bookmark included, and a comment
/// on the old check shows on the new one.
#[test]
fn loss_carryover_keeps_comment_and_bookmark() {
    let mut ana = Dev::new(1);
    let book = imported_book(&mut ana, &[3]);
    let old = check_key("abcd", 1, 1);
    let new = check_key("efgh", 1, 1);
    let base = ana.one(
        check(
            &old,
            decision(Some(UserStatus::Valid), true, Some("keep me")),
        ),
        vec![],
    );
    let note = ana.one(
        comment(
            5,
            tc_journal::record::CommentTarget::Check(old.clone()),
            "about this check",
        ),
        vec![],
    );
    let upgrade = ana.batch(
        vec![
            (moved(&old, &new), vec![base.id()]),
            (
                check(
                    &new,
                    decision(Some(UserStatus::Valid), true, Some("keep me")),
                ),
                vec![],
            ),
        ],
        Cause::Edit,
    );
    let state = assert_converges(&all_of(&[&book, &[base, note], &upgrade]));
    let file = DerivedFile::Decisions(ToolId::TranslationNotes, BOOK);
    let bytes = state.render(&file).expect("decisions");
    let decoded = decode_decisions(ToolId::TranslationNotes, BOOK, &bytes).expect("decodes");
    assert_eq!(decoded.len(), 1, "the moved check projects nothing");
    assert_eq!(decoded[0].key(), RecordKey::Check(new.clone()));
    let doc: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(doc["decisions"][0]["comments"], "keep me");
    assert_eq!(doc["decisions"][0]["reminders"], true);
    let comments = rendered(&state, &DerivedFile::Comments(BOOK)).expect("comments");
    assert_eq!(
        comments["comments"][0]["placed"]["check"]["checkId"],
        "efgh"
    );
}

fn titus_and_ruth(dev: &mut Dev, chapters: &[u16]) -> Vec<Change> {
    let mut book = imported_book(dev, chapters);
    book.extend(other_book(dev, BookId::Ruth));
    book
}

fn other_book(dev: &mut Dev, book: BookId) -> Vec<Change> {
    dev.batch(
        vec![
            (presence_of(book, PresentHow::Added), vec![]),
            (
                Record::BookHeader {
                    book,
                    value: UsfmBytes(format!("\\id {}\n", book.code())),
                },
                vec![],
            ),
            (
                Record::ChapterFront {
                    book,
                    chapter: ch(1),
                    value: ChapterFront::Present(UsfmBytes("\\c 1\n\\p\n".into())),
                },
                vec![],
            ),
            (
                Record::Verse {
                    book,
                    chapter: ch(1),
                    verse: vn(1),
                    value: text("In the days", 1),
                },
                vec![],
            ),
        ],
        Cause::Edit,
    )
}

/// L37: a book removed while a teammate edits it stays projected with the edits, until a
/// person decides; either choice resolves it.
#[test]
fn loss_book_removed_while_edited() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = titus_and_ruth(&mut maria, &[3]);
    let edit = juan.one(
        verse(1, 2, text("Juan's two", 2)),
        vec![on(&book, &verse_key(1, 2)).id()],
    );
    let removal = maria.one(
        removed(&ids(&book)),
        vec![on(&book, &RecordKey::BookPresence(BOOK)).id()],
    );
    let all = all_of(&[&book, &[edit.clone(), removal]]);
    let state = assert_converges(&all);
    let conflict = the_conflict(&state);
    assert_eq!(
        conflict.kind,
        ConflictKind::BookRemovedWhileEdited { unseen: 1 }
    );
    assert!(usfm(&state).contains("\\v 2 Juan's two\n"));
    // *Keep it removed*: the unseen work is now seen and stays in History.
    let mut kept = all.clone();
    kept.extend(maria.apply(vec![state.keep_removed(BOOK)]));
    let after = assert_converges(&kept);
    assert_eq!(after.conflict_count(), 0);
    assert_eq!(after.book(BOOK), BookView::Removed);
    assert!(after.history(&verse_key(1, 2)).contains(&edit.id()));
    // *Restore Titus*.
    let mut restored = all.clone();
    restored.extend(maria.apply(state.restore_book(conflict.id).expect("offered")));
    let after = assert_converges(&restored);
    assert_eq!(after.conflict_count(), 0);
    assert!(usfm(&after).contains("\\v 2 Juan's two\n"));
}

fn conflicted_verse() -> (Vec<Change>, Change, Change, Dev) {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[3]);
    let base = latest(&book, &verse_key(1, 1));
    juan.at = maria.at + 1_000;
    let a = maria.one(verse(1, 1, text("Maria's", 1)), vec![base.id()]);
    let b = juan.one(verse(1, 1, text("Juan's", 1)), vec![base.id()]);
    (all_of(&[&book, &[a.clone(), b.clone()]]), a, b, maria)
}

/// L38: typing into a conflicted verse continues the displayed version; the conflict
/// stays open with the other one.
#[test]
fn loss_edit_conflicted_verse_keeps_conflict() {
    let (mut all, a, b, mut maria) = conflicted_verse();
    let state = fold(&all);
    assert_eq!(state.shown_heads(&verse_key(1, 1), None), vec![b.id()]);
    assert_eq!(
        state.shown_heads(&verse_key(1, 1), Some(a.id())),
        vec![a.id()]
    );
    let typed = maria.one(
        verse(1, 1, text("Juan's, retyped", 1)),
        state.shown_heads(&verse_key(1, 1), None),
    );
    all.push(typed.clone());
    let state = assert_converges(&all);
    let conflict = the_conflict(&state);
    let heads: BTreeSet<ChangeId> = conflict.candidates.iter().map(|c| c.head).collect();
    assert_eq!(heads, [a.id(), typed.id()].into());
}

/// L38: a resolution is refused when the heads moved after the screen was drawn.
#[test]
fn loss_resolution_refused_when_heads_moved() {
    let (mut all, a, b, mut maria) = conflicted_verse();
    let state = fold(&all);
    let id = the_conflict(&state).id;
    let more = maria.one(verse(1, 1, text("Maria's, more", 1)), vec![a.id()]);
    all.push(more);
    let state = fold(&all);
    assert_eq!(
        state.resolution(&id, &Choice::Candidate { head: b.id() }),
        Err(tc_journal::fold::ResolveError::UnknownConflict)
    );
    assert_ne!(the_conflict(&state).id, id);
}

/// L39: two rewordings of different poetry lines of one verse are never combined into a
/// verse neither wrote.
#[test]
fn loss_poetry_lines_never_auto_merged() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[3]);
    let base = maria.one(
        verse(
            1,
            1,
            text("The LORD is my shepherd;\n\\q2 I shall not want.", 1),
        ),
        vec![on(&book, &verse_key(1, 1)).id()],
    );
    let a = maria.one(
        verse(
            1,
            1,
            text("The LORD shepherds me;\n\\q2 I shall not want.", 1),
        ),
        vec![base.id()],
    );
    let b = juan.one(
        verse(
            1,
            1,
            text("The LORD is my shepherd;\n\\q2 I lack nothing.", 1),
        ),
        vec![base.id()],
    );
    let state = assert_converges(&all_of(&[&book, &[base, a, b]]));
    let conflict = the_conflict(&state);
    assert_eq!(conflict.kind, ConflictKind::VerseText);
    assert_eq!(&*conflict.disputed[0].0, "/text/words");
}

/// L44: two versions of a carried file: a conflict, and both sets of bytes are placed.
#[test]
fn loss_carried_file_two_versions_both_kept() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[1]);
    let base = maria.one(carried("audio/01.mp3", 1), vec![]);
    let a = maria.one(carried("audio/01.mp3", 2), vec![base.id()]);
    let b = juan.one(carried("audio/01.mp3", 3), vec![base.id()]);
    let state = assert_converges(&all_of(&[&book, &[base, a, b]]));
    assert_eq!(the_conflict(&state).kind, ConflictKind::CarriedFile);
    let placed = state.carried_files();
    let oids: BTreeSet<_> = placed.iter().map(|p| p.oid).collect();
    assert_eq!(oids, [oid(2), oid(3)].into());
    let paths: BTreeSet<_> = placed.iter().map(|p| p.path.clone()).collect();
    assert!(paths.contains("audio/01.mp3"));
    assert!(
        paths
            .iter()
            .any(|p| p.starts_with("audio/01.conflict-") && p.ends_with(".mp3")),
        "{paths:?}"
    );
}

/// L50: open span conflicts never drop a verse's words from the provisional USFM.
#[test]
fn loss_span_conflict_never_drops_words() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let mut ana = Dev::new(3);
    let book = imported_book(&mut maria, &[6]);
    let a = join(&mut maria, &book, 1, 2, 3);
    let b = join(&mut juan, &book, 1, 3, 4);
    let c = ana.one(
        verse(1, 3, text("Ana's three", 3)),
        vec![on(&book, &verse_key(1, 3)).id()],
    );
    for order in [0, 1, 2] {
        let mut all = all_of(&[&book, &a, &b, std::slice::from_ref(&c)]);
        all.rotate_left(order * 3);
        let state = assert_converges(&all);
        assert_eq!(state.conflicts_of(ConflictKind::Passage).count(), 1);
        every_text_once(&state);
        let shown = usfm_words(&usfm(&state));
        let mut unique = shown.clone();
        unique.dedup();
        assert_eq!(
            unique.len(),
            shown.iter().collect::<BTreeSet<_>>().len(),
            "no word twice"
        );
        for v in [1, 5, 6] {
            assert!(usfm(&state).contains(&format!("words of 1:{v}")));
        }
    }
}

/// L51: restoring a book projects every record again: the same checks, alignments and
/// comments, byte for byte.
#[test]
fn loss_restore_book_keeps_checks() {
    let mut ana = Dev::new(1);
    let book = titus_and_ruth(&mut ana, &[3]);
    let key = check_key("abcd", 1, 2);
    let work = vec![
        ana.one(
            check(&key, decision(Some(UserStatus::Valid), true, None)),
            vec![],
        ),
        ana.one(alignment(1, 1, 1, "words of 1:1"), vec![]),
        ana.one(comment(3, verse_target(1, 3), "a note"), vec![]),
    ];
    let mut all = all_of(&[&book, &work]);
    let before = derived_bytes(&fold(&all));
    let removal = ana.one(
        removed(&ids(&all)),
        vec![on(&book, &RecordKey::BookPresence(BOOK)).id()],
    );
    all.push(removal.clone());
    let state = assert_converges(&all);
    assert_eq!(state.book(BOOK), BookView::Removed);
    assert!(
        !state
            .derived_files()
            .iter()
            .any(|f| f.path().contains("TIT"))
    );
    all.push(ana.one(present(PresentHow::RestoredText), vec![removal.id()]));
    let state = assert_converges(&all);
    let after = derived_bytes(&state);
    for path in before.keys().filter(|p| p.contains("TIT")) {
        assert_eq!(after.get(path), before.get(path), "{path}");
    }
}

/// L53: a text edit never conflicts with a check: invalidation is computed, never
/// written, and restoring the text makes the decision valid again.
#[test]
fn merge_text_edit_never_conflicts_with_check() {
    let mut ana = Dev::new(1);
    let mut maria = Dev::new(2);
    let book = imported_book(&mut ana, &[3]);
    let key = check_key("abcd", 1, 2);
    let mut judged = decision(Some(UserStatus::Valid), false, None);
    judged.judgement.verse_text_md5 = md5(&tc_usfm::plain_text_v1("words of 1:2\n"));
    let made = ana.one(check(&key, judged), vec![]);
    let edit = maria.one(
        verse(1, 2, text("new words", 2)),
        vec![on(&book, &verse_key(1, 2)).id()],
    );
    let mut all = all_of(&[&book, &[made, edit.clone()]]);
    let file = DerivedFile::Decisions(ToolId::TranslationNotes, BOOK);
    let state = assert_converges(&all);
    assert_eq!(state.conflict_count(), 0);
    assert_eq!(state.history(&RecordKey::Check(key.clone())).len(), 1);
    assert_eq!(
        rendered(&state, &file).expect("doc")["decisions"][0]["verseEdits"],
        true
    );
    all.push(maria.one(verse(1, 2, text("words of 1:2", 2)), vec![edit.id()]));
    let state = assert_converges(&all);
    assert_eq!(
        rendered(&state, &file).expect("doc")["decisions"][0]["verseEdits"],
        false
    );
}

/// L55: concurrent removals of the last books keep one book, listed, so the burrito
/// stays valid.
#[test]
fn loss_last_book_removed_concurrently() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = titus_and_ruth(&mut maria, &[2]);
    juan.at = maria.at + 1_000;
    let titus = maria.one(
        removed(&ids(&book)),
        vec![on(&book, &RecordKey::BookPresence(BOOK)).id()],
    );
    let ruth = juan.one(
        removed_of(BookId::Ruth, &ids(&book)),
        vec![on(&book, &RecordKey::BookPresence(BookId::Ruth)).id()],
    );
    let mut all = all_of(&[&book, &[titus, ruth]]);
    let state = assert_converges(&all);
    let conflict = the_conflict(&state);
    assert_eq!(conflict.kind, ConflictKind::LastBookRemoved);
    assert_eq!(conflict.key, RecordKey::BookPresence(BookId::Ruth));
    assert_eq!(state.projected_books(), vec![BookId::Ruth]);
    all.extend(juan.apply(state.restore_book(conflict.id).expect("offered")));
    let state = assert_converges(&all);
    assert_eq!(state.conflict_count(), 0);
    assert_eq!(state.projected_books(), vec![BookId::Ruth]);
}

fn tn_slot(version: &str, n: u8) -> Json {
    obj(&[
        ("repoPath", Json::from("git.door43.org/unfoldingWord/en_tn")),
        ("sha", Json::from(oid(n).to_hex())),
        ("version", Json::from(version)),
    ])
}

/// L58: two upgrades of one resource to different versions: a pins conflict and the
/// check conflicts it caused share their batches, so they are one row, resolved in one
/// batch.
#[test]
fn loss_upgrade_two_versions_grouped() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[2]);
    let key = check_key("abcd", 1, 1);
    let base_pins = maria.one(
        pins(
            PinGroup::Primary,
            &[("translationNotes", tn_slot("v91", 91))],
        ),
        vec![],
    );
    let base_check = maria.one(
        check(&key, decision(Some(UserStatus::Valid), false, None)),
        vec![],
    );
    let upgrade = |dev: &mut Dev, version: &str, n: u8| {
        let mut moved_on = decision(Some(UserStatus::Valid), false, None);
        moved_on.resource.sha = oid(n);
        dev.batch(
            vec![
                (
                    pins(
                        PinGroup::Primary,
                        &[("translationNotes", tn_slot(version, n))],
                    ),
                    vec![base_pins.id()],
                ),
                (check(&key, moved_on), vec![base_check.id()]),
            ],
            Cause::Edit,
        )
    };
    let v92 = upgrade(&mut maria, "v92", 92);
    let v93 = upgrade(&mut juan, "v93", 93);
    let mut all = all_of(&[&book, &[base_pins.clone(), base_check.clone()], &v92, &v93]);
    let state = assert_converges(&all);
    let conflicts: Vec<_> = state.conflicts().cloned().collect();
    assert_eq!(conflicts.len(), 2, "{conflicts:#?}");
    let kinds: BTreeSet<u8> = conflicts.iter().map(|c| c.kind.order()).collect();
    assert_eq!(
        kinds,
        [ConflictKind::Pins.order(), ConflictKind::Check.order()].into()
    );
    assert_eq!(conflicts[0].batches, conflicts[1].batches, "one row");
    let ids: Vec<_> = conflicts.iter().map(|c| c.id).collect();
    let (resolved, skipped) = state.bulk_resolution(&[maria.id].into(), &ids);
    assert!(skipped.is_empty());
    let planned: Vec<_> = resolved.into_iter().flat_map(|(_, p)| p).collect();
    all.extend(maria.apply(planned));
    let state = assert_converges(&all);
    assert_eq!(state.conflict_count(), 0);
    let resources = rendered(&state, &DerivedFile::Resources).expect("resources");
    assert!(resources.to_string().contains("v92"), "{resources}");
}

/// L59, L78: a bulk choice by computer skips conflicts whose candidates were all made on
/// the chosen computers (one person's laptop and desktop).
#[test]
fn bulk_choice_by_computer_skips_shared_username() {
    let mut laptop = Dev::new(1);
    let mut juan = Dev::new(2);
    let mut desktop = Dev::new(3);
    let book = imported_book(&mut laptop, &[3]);
    let v = |n| latest(&book, &verse_key(1, n)).id();
    let changes = vec![
        laptop.one(verse(1, 1, text("laptop one", 1)), vec![v(1)]),
        juan.one(verse(1, 1, text("Juan one", 1)), vec![v(1)]),
        laptop.one(verse(1, 2, text("laptop two", 2)), vec![v(2)]),
        desktop.one(verse(1, 2, text("desktop two", 2)), vec![v(2)]),
    ];
    let mut all = all_of(&[&book, &changes]);
    let state = assert_converges(&all);
    let ids: Vec<_> = state.conflicts().map(|c| c.id).collect();
    assert_eq!(ids.len(), 2);
    let between_mine = state
        .conflicts()
        .find(|c| c.key == verse_key(1, 2))
        .expect("laptop against desktop")
        .id;
    let (resolved, skipped) = state.bulk_resolution(&[laptop.id, desktop.id].into(), &ids);
    assert_eq!(skipped, vec![between_mine]);
    assert_eq!(resolved.len(), 1);
    all.extend(laptop.apply(resolved.into_iter().flat_map(|(_, p)| p).collect()));
    let state = assert_converges(&all);
    assert_eq!(projected_words(&state, 1, 1).as_deref(), Some("laptop one"));
    assert_eq!(state.conflict_count(), 1);
}

/// L63: half of an action that arrived is held whole and listed; the rest completes it.
#[test]
fn loss_incomplete_batch_held_and_listed() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[3]);
    let section = juan.batch(
        vec![
            (
                verse(1, 1, text("one", 1)),
                vec![on(&book, &verse_key(1, 1)).id()],
            ),
            (
                verse(1, 2, text("two", 2)),
                vec![on(&book, &verse_key(1, 2)).id()],
            ),
        ],
        Cause::Edit,
    );
    let mut all = all_of(&[&book, &section[..1]]);
    let state = assert_converges(&all);
    assert_eq!(
        projected_words(&state, 1, 1).as_deref(),
        Some("words of 1:1")
    );
    assert!(state.anomalies().any(|a| matches!(
        a,
        Anomaly::IncompleteBatch { have: 1, of: 2, device, .. } if *device == juan.id
    )));
    all.push(section[1].clone());
    let state = assert_converges(&all);
    assert_eq!(projected_words(&state, 1, 1).as_deref(), Some("one"));
    assert_eq!(projected_words(&state, 1, 2).as_deref(), Some("two"));
    assert!(
        !state
            .anomalies()
            .any(|a| matches!(a, Anomaly::IncompleteBatch { .. }))
    );
}

/// L67: a book file added on Door43 after a removal is a visible presence conflict, and
/// the book projects meanwhile.
#[test]
fn loss_web_added_book_after_removal_conflict() {
    let mut maria = Dev::new(1);
    let mut door43 = Dev::new(9);
    let book = titus_and_ruth(&mut maria, &[2]);
    let removal = maria.one(
        removed(&ids(&book)),
        vec![on(&book, &RecordKey::BookPresence(BOOK)).id()],
    );
    let added = door43.one(present(PresentHow::Added), vec![]);
    let state = assert_converges(&all_of(&[&book, &[removal, added]]));
    let conflict = the_conflict(&state);
    assert_eq!(conflict.kind, ConflictKind::BookPresence);
    assert!(state.book(BOOK).projects());
}

// ---------------------------------------------------------------------------------------
// 15.3
// ---------------------------------------------------------------------------------------

fn section_move(juan_newer: bool, juan_value: VerseValue) -> (Vec<Change>, FoldState) {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[6]);
    let base = maria.batch(
        vec![
            (
                verse(1, 4, text("Alpha Beta", 4)),
                vec![on(&book, &verse_key(1, 4)).id()],
            ),
            (
                verse(1, 5, text("Gamma Delta Epsilon", 5)),
                vec![on(&book, &verse_key(1, 5)).id()],
            ),
        ],
        Cause::Edit,
    );
    juan.at = maria.at + if juan_newer { 3_600_000 } else { -3_600_000 };
    // Maria drags verse 5's marker past "Gamma".
    let drag = maria.batch(
        vec![
            (verse(1, 4, text("Alpha Beta Gamma", 4)), vec![base[0].id()]),
            (verse(1, 5, text("Delta Epsilon", 5)), vec![base[1].id()]),
        ],
        Cause::Edit,
    );
    let edit = juan.one(verse(1, 4, juan_value), vec![base[0].id()]);
    let all = all_of(&[&book, &base, &drag, &[edit]]);
    let state = assert_converges(&all);
    (all, state)
}

/// L69: a section save that moves a word against an edit of the verse it moved into is
/// one grouped conflict; the provisional text takes one side whole, and *Combine* puts
/// both changes where they belong.
#[test]
fn loss_section_move_vs_verse_edit() {
    for juan_newer in [false, true] {
        let (mut all, state) = section_move(juan_newer, text("Alpha Bravo", 4));
        let conflict = the_conflict(&state);
        assert_eq!(conflict.kind, ConflictKind::Passage);
        let (four, five) = if juan_newer {
            ("Alpha Bravo", "Gamma Delta Epsilon")
        } else {
            ("Alpha Beta Gamma", "Delta Epsilon")
        };
        assert_eq!(projected_words(&state, 1, 4).as_deref(), Some(four));
        assert_eq!(projected_words(&state, 1, 5).as_deref(), Some(five));
        let combined = state.combine_passage(&conflict.id).expect("disjoint");
        let mut ana = Dev::new(3);
        all.extend(
            ana.apply(
                state
                    .resolution(&conflict.id, &Choice::Values(combined))
                    .expect("values"),
            ),
        );
        let state = assert_converges(&all);
        assert_eq!(state.conflict_count(), 0);
        assert_eq!(
            projected_words(&state, 1, 4).as_deref(),
            Some("Alpha Bravo Gamma")
        );
        assert_eq!(
            projected_words(&state, 1, 5).as_deref(),
            Some("Delta Epsilon")
        );
    }
}

/// L69: a paragraph break made concurrently with a word move is not merged by units: it
/// could land on the wrong words.
#[test]
fn loss_section_move_vs_paragraph_break_not_merged() {
    for juan_newer in [false, true] {
        let (_, state) = section_move(juan_newer, text_layout("Alpha Beta", "\n\\p\n", 4));
        let conflict = the_conflict(&state);
        assert_eq!(conflict.kind, ConflictKind::Passage);
        assert!(matches!(
            state.key_state(&verse_key(1, 4)).map(|k| k.state),
            Some(Resolved::Conflict(_))
        ));
        let four = state.projected(&verse_key(1, 4)).expect("verse 4");
        let expected = if juan_newer {
            verse(1, 4, text_layout("Alpha Beta", "\n\\p\n", 4))
        } else {
            verse(1, 4, text("Alpha Beta Gamma", 4))
        };
        assert_eq!(four, expected, "one side whole");
    }
}

fn restore_race(restore_newer: bool) -> FoldState {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = titus_and_ruth(&mut maria, &[2]);
    let presence = on(&book, &RecordKey::BookPresence(BOOK));
    let removal = maria.one(removed(&ids(&book)), vec![presence.id()]);
    juan.at = maria.at + if restore_newer { -100_000 } else { 100_000 };
    let restore = maria.one(present(PresentHow::RestoredText), vec![removal.id()]);
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
    assert_converges(&all_of(&[&book, &[removal, restore], &empty]))
}

/// L70: *Start empty* never silently beats a concurrent *Restore the text*: a presence
/// conflict, and the provisional presence decides what shows.
#[test]
fn loss_restore_text_vs_start_empty_conflict() {
    let state = restore_race(true);
    assert!(state.conflicts_of(ConflictKind::BookPresence).count() == 1);
    assert_eq!(
        projected_words(&state, 1, 1).as_deref(),
        Some("words of 1:1")
    );
    let state = restore_race(false);
    assert!(state.conflicts_of(ConflictKind::BookPresence).count() == 1);
    assert_eq!(projected_words(&state, 1, 1).as_deref(), Some("___"));
}

/// L70: *Start empty* against a concurrent edit is a passage conflict: a person's choice
/// is never absorbed as a seed stub, and the edit is never absorbed by it.
#[test]
fn loss_start_empty_vs_concurrent_edit_conflict() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[3]);
    let empty = maria.batch(
        (1..=3)
            .map(|v| {
                (
                    verse(1, v, text("___", v)),
                    vec![on(&book, &verse_key(1, v)).id()],
                )
            })
            .collect(),
        Cause::StartOver,
    );
    juan.at = maria.at + 1_000;
    let edit = juan.one(
        verse(1, 2, text("Juan's two", 2)),
        vec![on(&book, &verse_key(1, 2)).id()],
    );
    let state = assert_converges(&all_of(&[&book, &empty, std::slice::from_ref(&edit)]));
    let conflict = the_conflict(&state);
    assert_eq!(conflict.kind, ConflictKind::Passage);
    assert!(conflict.candidates.iter().any(|c| c.head == edit.id()));
    assert!(
        conflict
            .candidates
            .iter()
            .any(|c| empty.iter().any(|e| e.id() == c.head))
    );
}

/// L70: the stub rules are for one head plus stubs: one real head settles by rule; two
/// real heads conflict among themselves, the stub absorbed.
#[test]
fn stub_rules_one_head_plus_stubs() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let mut ana = Dev::new(3);
    let m = seed_book(&mut maria, &[2]);
    let j = seed_book(&mut juan, &[2]);
    let real = ana.one(
        verse(1, 1, text("Ana's one", 1)),
        vec![on(&m, &verse_key(1, 1)).id()],
    );
    let state = assert_converges(&all_of(&[&m, &j, std::slice::from_ref(&real)]));
    assert_eq!(state.conflict_count(), 0);
    assert!(matches!(
        state.key_state(&verse_key(1, 1)).map(|k| k.state),
        Some(Resolved::SettledByRule { head, .. }) if head == real.id()
    ));
    let other = juan.one(
        verse(1, 1, text("Juan's one", 1)),
        vec![on(&j, &verse_key(1, 1)).id()],
    );
    let state = assert_converges(&all_of(&[&m, &j, &[real, other]]));
    let conflict = the_conflict(&state);
    assert_eq!(conflict.candidates.len(), 2, "the stub is absorbed");
}

/// L71: a comment made concurrently with a carry-over shows on the moved check.
#[test]
fn loss_comment_follows_moved_check() {
    let (mut all, old, _) = carry_over();
    let mut maria = Dev::new(4);
    all.extend(imported_book(&mut maria, &[3]));
    let mut pedro = Dev::new(2);
    all.push(pedro.one(
        comment(
            7,
            tc_journal::record::CommentTarget::Check(old.clone()),
            "while it moved",
        ),
        vec![],
    ));
    let state = assert_converges(&all);
    let comments = rendered(&state, &DerivedFile::Comments(BOOK)).expect("comments");
    assert_eq!(
        comments["comments"][0]["placed"]["check"]["checkId"],
        "efgh"
    );
}

/// L71: a comment on a verse follows its text through a renumber.
#[test]
fn loss_comment_follows_renumber() {
    let mut maria = Dev::new(1);
    let mut ana = Dev::new(3);
    let book = imported_book(&mut maria, &[6, 4]);
    let moved = renumber(&mut maria, &book);
    // Ana comments on 2:2 by number, before she sees the renumber.
    let note = ana.one(comment(4, verse_target(2, 2), "on old 2:2"), vec![]);
    let state = assert_converges(&all_of(&[&book, &moved, &[note]]));
    assert_eq!(
        state.placement(&RecordKey::Comment(tc_journal::CommentId::from_bytes(
            [4; 16]
        ))),
        Placement::Run {
            chapter: 2,
            verse: 3
        }
    );
}

/// L71: an alignment of a verse joined into a span is placed at the run's aligner and
/// kept in the sidecar, never dropped.
#[test]
fn loss_alignment_inside_join_reachable() {
    let mut maria = Dev::new(1);
    let mut ana = Dev::new(3);
    let book = imported_book(&mut maria, &[6]);
    let aligned = ana.one(alignment(1, 5, 5, "words of 1:5"), vec![]);
    let span = join(&mut maria, &book, 1, 4, 5);
    let state = assert_converges(&all_of(&[&book, &[aligned], &span]));
    assert_eq!(
        state.placement(&RecordKey::Alignment(BOOK, ch(1), vn(5))),
        Placement::Run {
            chapter: 1,
            verse: 4
        }
    );
    assert_eq!(state.detached().count(), 0);
    let doc = rendered(&state, &DerivedFile::Alignments(BOOK)).expect("alignments");
    assert_eq!(doc["chapters"]["1"]["5"]["invalid"], true, "{doc}");
}

/// L72: a field from a newer minor keeps the change on its own key, is compared, and is
/// never projected.
#[test]
fn loss_newer_field_keeps_known_key() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[3]);
    let base = latest(&book, &verse_key(1, 1));
    let plain = maria.one(verse(1, 1, text("Paul", 1)), vec![base.id()]);
    let newer = forged(&plain, |map| {
        map.insert("v".into(), Json::from(tc_journal::FORMAT_MINOR + 1));
        let Some(Json::Obj(record)) = map.get_mut("record") else {
            panic!("record")
        };
        let Some(Json::Obj(inner)) = record.get_mut("verse") else {
            panic!("verse")
        };
        let Some(Json::Obj(value)) = inner.get_mut("value") else {
            panic!("value")
        };
        let Some(Json::Obj(text)) = value.get_mut("text") else {
            panic!("text")
        };
        text.insert("tone".into(), Json::from("soft"));
    });
    let same_words = juan.one(verse(1, 1, text("Paul", 1)), vec![base.id()]);
    let inputs: Vec<FoldInput> = book
        .iter()
        .cloned()
        .map(FoldInput::from)
        .chain([newer, FoldInput::from(same_words)])
        .collect();
    let state = FoldState::fold(ProjectKind::Bible, inputs);
    assert_eq!(
        state.heads(&verse_key(1, 1)).len(),
        2,
        "both on the verse's key"
    );
    assert!(
        matches!(
            state.key_state(&verse_key(1, 1)).map(|k| k.state),
            Some(Resolved::Merged { how: MergeHow::Units(ref units), .. })
                if units.iter().any(|u| &*u.0 == "/text/tone")
        ),
        "{:?}",
        state.key_state(&verse_key(1, 1))
    );
    assert!(usfm(&state).contains("\\v 1 Paul\n"));
    assert!(!usfm(&state).contains("soft"));
}

/// L74: a section save from a screen drawn before a teammate's join arrived is a
/// `Passage` conflict, never a refusal, and shows every word once.
#[test]
fn loss_section_save_after_teammate_join_conflicts() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[6]);
    let screen = fold(&book);
    let tokens: BTreeMap<u16, Vec<ChangeId>> = (4..=6)
        .map(|v| (v, screen.shown_heads(&verse_key(1, v), None)))
        .collect();
    let span = join(&mut juan, &book, 1, 4, 5);
    let save = maria.batch(
        (4..=6)
            .map(|v| {
                (
                    verse(1, v, text(&format!("Maria's {v}"), v)),
                    tokens[&v].clone(),
                )
            })
            .collect(),
        Cause::Edit,
    );
    let state = assert_converges(&all_of(&[&book, &span, &save]));
    let conflict = the_conflict(&state);
    assert_eq!(conflict.kind, ConflictKind::Passage);
    every_text_once(&state);
}

/// L75: a verse added to a book with a `BookLayout` projects after its numeric
/// predecessor.
#[test]
fn loss_booklayout_new_verse_projected() {
    let mut ana = Dev::new(1);
    let book = imported_book(&mut ana, &[3]);
    let layout = ana.one(
        Record::BookLayout {
            book: BOOK,
            value: Some(vec![
                LayoutItem::Chapter(ch(1)),
                LayoutItem::Verse(ch(1), vn(1)),
                LayoutItem::Verse(ch(1), vn(3)),
                LayoutItem::Verse(ch(1), vn(2)),
            ]),
        },
        vec![],
    );
    let added = ana.one(verse(1, 4, text("words of 1:4", 4)), vec![]);
    let state = assert_converges(&all_of(&[&book, &[layout, added]]));
    let text = usfm(&state);
    let at = |v: u16| text.find(&format!("\\v {v} ")).expect("projected");
    assert!(at(1) < at(3) && at(3) < at(4) && at(4) < at(2), "{text}");
}

/// L76: *Apply what was recovered* releases what arrived of a batch that can never be
/// completed; a later arrival applies the rest.
#[test]
fn loss_unrecoverable_batch_release() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[3]);
    let section = juan.batch(
        (1..=3)
            .map(|v| {
                (
                    verse(1, v, text(&format!("Juan's {v}"), v)),
                    vec![on(&book, &verse_key(1, v)).id()],
                )
            })
            .collect(),
        Cause::Edit,
    );
    let batch = section[0].body().batch.id;
    let mut recovered = vec![section[0].id(), section[1].id()];
    recovered.sort();
    let release = maria.one(
        Record::BatchRelease {
            batch,
            value: BatchReleaseValue { changes: recovered },
        },
        vec![],
    );
    let mut all = all_of(&[&book, &section[..2], &[release]]);
    let state = assert_converges(&all);
    assert_eq!(projected_words(&state, 1, 1).as_deref(), Some("Juan's 1"));
    assert_eq!(projected_words(&state, 1, 2).as_deref(), Some("Juan's 2"));
    assert_eq!(
        projected_words(&state, 1, 3).as_deref(),
        Some("words of 1:3")
    );
    all.push(section[2].clone());
    let state = assert_converges(&all);
    assert_eq!(projected_words(&state, 1, 3).as_deref(), Some("Juan's 3"));
    assert!(
        !state
            .anomalies()
            .any(|a| matches!(a, Anomaly::IncompleteBatch { .. }))
    );
}

/// L76: a conflict whose head continues a change that has not arrived is marked as
/// incomplete history.
#[test]
fn incomplete_history_conflict_labelled() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[3]);
    let base = latest(&book, &verse_key(1, 1));
    let lost = juan.batch(
        vec![
            (verse(1, 1, text("Juan's first", 1)), vec![base.id()]),
            (
                verse(1, 2, text("Juan's two", 2)),
                vec![on(&book, &verse_key(1, 2)).id()],
            ),
        ],
        Cause::Edit,
    );
    let continued = juan.one(verse(1, 1, text("Juan's second", 1)), vec![lost[0].id()]);
    let mine = maria.one(verse(1, 1, text("Maria's", 1)), vec![base.id()]);
    // Only the first change of Juan's two-change batch arrived.
    let state = assert_converges(&all_of(&[&book, &lost[..1], &[continued, mine]]));
    let conflict = the_conflict(&state);
    assert!(conflict.incomplete_history);
}

/// L79: carried paths that differ only in letter case are both kept, one at a `.case-`
/// path, and listed.
#[test]
fn loss_case_colliding_carried_paths_kept() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[1]);
    let a = maria.one(carried("audio/Intro.mp3", 1), vec![]);
    let b = juan.one(carried("audio/intro.mp3", 2), vec![]);
    let state = assert_converges(&all_of(&[&book, &[a, b]]));
    let conflict = the_conflict(&state);
    assert_eq!(conflict.kind, ConflictKind::PathCase);
    let placed = state.carried_files();
    assert_eq!(placed.len(), 2);
    let folded: BTreeSet<String> = placed.iter().map(|p| p.path.to_lowercase()).collect();
    assert_eq!(folded.len(), 2, "no two paths collide: {placed:?}");
    assert!(placed.iter().any(|p| p.path.contains(".case-")));
}

/// L81: a metadata value valid as JSON but invalid in a burrito never blocks a commit:
/// the newest valid value is used and the invalid one is listed.
#[test]
fn loss_invalid_meta_value_never_blocks_sync() {
    let mut ana = Dev::new(1);
    let mut door43 = Dev::new(9);
    let book = imported_book(&mut ana, &[1]);
    let good = ana.one(
        Record::ProjectMeta {
            field: MetaField::Copyright,
            value: Some(obj(&[(
                "shortStatements",
                Json::Arr(vec![obj(&[("statement", Json::from("CC BY-SA 4.0"))])]),
            )])),
        },
        vec![],
    );
    let bad = door43.one(
        Record::ProjectMeta {
            field: MetaField::Copyright,
            value: Some(Json::from("All rights reserved")),
        },
        vec![good.id()],
    );
    let state = assert_converges(&all_of(&[&book, &[good, bad.clone()]]));
    assert!(
        state
            .anomalies()
            .any(|a| matches!(a, Anomaly::InvalidForBurrito { change, .. } if *change == bad.id()))
    );
    let metadata = derived_bytes(&state)
        .remove("metadata.json")
        .expect("built");
    let doc: serde_json::Value = serde_json::from_slice(&metadata).expect("json");
    assert_eq!(
        doc["copyright"]["shortStatements"][0]["statement"],
        "CC BY-SA 4.0"
    );
}

/// L92: metadata fields the app does not model are records, written back verbatim.
#[test]
fn loss_metadata_unmodelled_fields_kept() {
    let mut ana = Dev::new(1);
    let book = imported_book(&mut ana, &[1]);
    let names = obj(&[(
        "book-tit",
        obj(&[
            ("short", obj(&[("en", Json::from("Titus"))])),
            ("abbr", obj(&[("en", Json::from("Tit"))])),
            ("long", obj(&[("en", Json::from("The Letter to Titus"))])),
        ]),
    )]);
    let field = ana.one(
        Record::ProjectMeta {
            field: MetaField::Extra(MetaPath::parse("localizedNames").expect("path")),
            value: Some(names),
        },
        vec![],
    );
    let state = assert_converges(&all_of(&[&book, &[field]]));
    let metadata = derived_bytes(&state)
        .remove("metadata.json")
        .expect("built");
    let doc: serde_json::Value = serde_json::from_slice(&metadata).expect("json");
    assert_eq!(
        doc["localizedNames"]["book-tit"]["long"]["en"],
        "The Letter to Titus"
    );
}

// ---------------------------------------------------------------------------------------
// Cases found by the review of the fold (§15.4)
// ---------------------------------------------------------------------------------------

/// Maria joins 1:4–5 while Juan and Ana each change verse 5 (Ana later) and, when
/// `both_verses`, verse 4 too: one side, "the others", holds two versions of a record.
fn join_vs_two_editors(both_verses: bool) -> (Vec<Change>, FoldState, Vec<Change>, Vec<Change>) {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let mut ana = Dev::new(3);
    let book = imported_book(&mut maria, &[6]);
    juan.at = maria.at + 60_000;
    ana.at = maria.at + 3_600_000;
    let span = join(&mut maria, &book, 1, 4, 5);
    let edits = |dev: &mut Dev, who: &str| -> Vec<Change> {
        let mut out = vec![dev.one(
            verse(1, 5, text(&format!("{who}'s five"), 5)),
            vec![on(&book, &verse_key(1, 5)).id()],
        )];
        if both_verses {
            out.push(dev.one(
                verse(1, 4, text(&format!("{who}'s four"), 4)),
                vec![on(&book, &verse_key(1, 4)).id()],
            ));
        }
        out
    };
    let juans = edits(&mut juan, "Juan");
    let anas = edits(&mut ana, "Ana");
    let all = all_of(&[&book, &span, &juans, &anas]);
    let state = assert_converges(&all);
    (all, state, juans, anas)
}

/// L97: keeping one candidate of a passage conflict writes exactly that version, never
/// another version of the same side that a later clock prefers (I3).
#[test]
fn loss_passage_keeps_the_picked_version() {
    for pick_juan in [true, false] {
        let (mut all, state, juans, anas) = join_vs_two_editors(false);
        let conflict = the_conflict(&state);
        assert_eq!(conflict.kind, ConflictKind::Passage);
        let (picked, words) = if pick_juan {
            (&juans[0], "Juan's five")
        } else {
            (&anas[0], "Ana's five")
        };
        let planned = state
            .resolution(&conflict.id, &Choice::Candidate { head: picked.id() })
            .expect("one version of each record");
        let five = planned
            .iter()
            .find(|p| p.record.key() == verse_key(1, 5))
            .expect("verse 5 is written");
        assert_eq!(five.record, verse(1, 5, text(words, 5)));
        let four = planned
            .iter()
            .find(|p| p.record.key() == verse_key(1, 4))
            .expect("verse 4 is written");
        assert_eq!(four.record, verse(1, 4, text("words of 1:4", 4)));
        all.extend(Dev::new(4).apply(planned));
        let state = assert_converges(&all);
        assert_eq!(state.conflict_count(), 0);
        assert_eq!(projected_words(&state, 1, 5).as_deref(), Some(words));
    }
}

/// L97: where the kept side holds two versions of a record the person did not pick
/// between, the choice asks for one instead of letting the clock pick; picking it keeps it.
#[test]
fn loss_passage_side_with_two_versions_asks() {
    let (mut all, state, juans, anas) = join_vs_two_editors(true);
    let conflict = the_conflict(&state);
    let asked = state.resolution(
        &conflict.id,
        &Choice::Candidate {
            head: juans[0].id(),
        },
    );
    assert_eq!(
        asked,
        Err(tc_journal::fold::ResolveError::MissingValue(verse_key(
            1, 4
        )))
    );
    let planned = state
        .keep_passage(&conflict.id, &[juans[0].id(), anas[1].id()])
        .expect("a version of each record");
    let by_key: BTreeMap<RecordKey, Record> = planned
        .iter()
        .map(|p| (p.record.key(), p.record.clone()))
        .collect();
    assert_eq!(
        by_key[&verse_key(1, 5)],
        verse(1, 5, text("Juan's five", 5))
    );
    assert_eq!(by_key[&verse_key(1, 4)], verse(1, 4, text("Ana's four", 4)));
    all.extend(Dev::new(4).apply(planned));
    let state = assert_converges(&all);
    assert_eq!(state.conflict_count(), 0);
}

/// L97: *Keep 4 and 5 separate* writes back a verse's own text only when it has one;
/// two concurrent texts of it are the person's to pick, never the newest by clock.
#[test]
fn loss_keep_separate_never_picks_by_clock() {
    let (_, state, juans, _) = join_vs_two_editors(false);
    let conflict = the_conflict(&state);
    assert_eq!(
        state.keep_separate(&conflict.id, &[]),
        Err(tc_journal::fold::ResolveError::MissingValue(verse_key(
            1, 5
        )))
    );
    let planned = state
        .keep_separate(&conflict.id, &[juans[0].id()])
        .expect("Juan's verse 5 picked");
    let five = planned
        .iter()
        .find(|p| p.record.key() == verse_key(1, 5))
        .expect("verse 5");
    assert_eq!(five.record, verse(1, 5, text("Juan's five", 5)));
}

/// L97: keeping the joined verses after picking a span keeps that span's words at its own
/// verse, whatever span the range's first verse holds.
#[test]
fn loss_keep_joined_keeps_the_picked_span() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[6]);
    // Two spans written alone by a foreign tool: 3–4 and 4–5, so 3 is short of its end.
    let three = maria.one(
        verse(1, 3, text("words of 1:3 words of 1:4", 4)),
        vec![on(&book, &verse_key(1, 3)).id()],
    );
    let four = juan.one(
        verse(1, 4, text("words of 1:4 words of 1:5", 5)),
        vec![on(&book, &verse_key(1, 4)).id()],
    );
    let state = assert_converges(&all_of(&[&book, &[three, four.clone()]]));
    let conflict = state
        .conflicts()
        .find(|c| c.kind == ConflictKind::StructureIssue && c.key == verse_key(1, 3))
        .cloned()
        .expect("verse 3 spans past a verse it does not cover");
    let planned = state
        .resolution(&conflict.id, &Choice::Candidate { head: four.id() })
        .expect("offered");
    let by_key: BTreeMap<RecordKey, Record> = planned
        .iter()
        .map(|p| (p.record.key(), p.record.clone()))
        .collect();
    assert_eq!(
        by_key.get(&verse_key(1, 4)).and_then(words_of).as_deref(),
        Some("words of 1:4 words of 1:5"),
        "{planned:#?}"
    );
}

/// L100: *Choose per line* keeps every unit that merged cleanly, whichever head has the
/// smallest id, and takes the picked head's value only for the disputed units.
#[test]
fn loss_choose_per_line_keeps_agreed_units() {
    let key = check_key("abcd", 1, 1);
    let mut orders = BTreeSet::new();
    for attempt in 0..32 {
        let mut ana = Dev::new(1);
        let mut pedro = Dev::new(2);
        let mut juan = Dev::new(3);
        pedro.at += attempt * 1_000;
        let book = imported_book(&mut ana, &[1]);
        let base = ana.one(
            check(&key, decision(Some(UserStatus::Todo), false, Some("base"))),
            vec![],
        );
        // Pedro changes the status; Juan the status and the comment.
        let p = pedro.one(
            check(
                &key,
                decision(Some(UserStatus::Invalid), false, Some("base")),
            ),
            vec![base.id()],
        );
        let j = juan.one(
            check(
                &key,
                decision(Some(UserStatus::Valid), false, Some("Juan's note")),
            ),
            vec![base.id()],
        );
        let pedro_first = p.id() < j.id();
        if !orders.insert(pedro_first) {
            continue;
        }
        let state = assert_converges(&all_of(&[&book, &[base, p.clone(), j]]));
        let conflict = the_conflict(&state);
        assert_eq!(
            conflict.disputed,
            vec![tc_journal::fold::UnitName("/decision/judgement".into())]
        );
        let picks = BTreeMap::from([(
            tc_journal::fold::UnitName("/decision/judgement".into()),
            p.id(),
        )]);
        let record = state.per_unit(&conflict.id, &picks).expect("offered");
        assert_eq!(
            record,
            check(
                &key,
                decision(Some(UserStatus::Invalid), false, Some("Juan's note"))
            ),
            "Pedro's head has the smaller id: {pedro_first}"
        );
        if orders.len() == 2 {
            return;
        }
    }
    panic!("both orders of the heads' ids were not reached: {orders:?}");
}

/// L100: *Choose per line* is not offered for a conflict that lists no units, such as a
/// chapter with verses but no chapter marker, whose front has no folded version.
#[test]
fn per_unit_not_offered_without_units() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[1]);
    // Chapter 2's front came in a batch of two whose other change never arrived.
    let held = maria.batch(
        vec![
            (front(2), vec![]),
            (verse(2, 2, text("Maria's 2:2", 2)), vec![]),
        ],
        Cause::Edit,
    );
    let verse_two = juan.one(verse(2, 1, text("Juan's 2:1", 1)), vec![]);
    let state = assert_converges(&all_of(&[&book, &held[..1], &[verse_two]]));
    let conflict = state
        .conflicts()
        .find(|c| c.key == RecordKey::ChapterFront(BOOK, ch(2)))
        .cloned()
        .expect("chapter 2 has no marker");
    assert_eq!(conflict.kind, ConflictKind::StructureIssue);
    assert_eq!(
        state.per_unit(&conflict.id, &BTreeMap::new()),
        Err(tc_journal::fold::ResolveError::NotOffered)
    );
}
