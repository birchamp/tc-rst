//! Progress, the rail's chips and its order, from tC4's suites (`check-filters`,
//! `checkSessionMerge` `railGroupsOf`, `checkStartIndex`) and the real Titus lists.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{eng, notes, pin, word_links};
use tc_check::derive::{notes_for_book, word_links_for_book};
use tc_check::item::{Category, NoteCategory};
use tc_check::rail::{
    Filter, GroupLabel, Position, Progress, SortMode, counts, groups, next_undecided, progress,
    start_index,
};
use tc_check::status::{Edit, Triage, apply};
use tc_check::{
    CheckItem, CheckKey, CheckState, CheckTarget, Decision, RefPart, Selection, ToolId,
};
use tc_core::BookId;
use tc_helps::{HelpItemId, HelpRef, Quote, RowId, WordCategory};

fn resource() -> tc_check::ResourceRef {
    pin("unfoldingWord/en_tn", "v86", &[]).resource
}

fn decision(edits: &[Edit]) -> Decision {
    let mut d: Option<Decision> = None;
    for edit in edits {
        d = Some(apply(d.as_ref(), edit.clone(), "Dios", &resource()));
    }
    d.unwrap()
}

fn state(edits: &[Edit]) -> CheckState {
    if edits.is_empty() {
        return CheckState::undecided();
    }
    CheckState::of(Some(&decision(edits)), Some(&resource()), Some("Dios"))
}

fn dios() -> Vec<Selection> {
    vec![Selection {
        text: "Dios".into(),
        occurrence: 1,
        occurrences: 1,
    }]
}

#[test]
fn chip_counts_and_rows_come_from_one_predicate_each() {
    // tC4 test/check-filters.test.ts.
    let states = [
        state(&[]),
        state(&[Edit::Bookmark(true)]),
        state(&[Edit::Comment("ask the team".into())]),
        state(&[
            Edit::Comment("done".into()),
            Edit::Bookmark(true),
            Edit::Triage(Triage::Valid(dios())),
        ]),
        state(&[Edit::Comment(String::new())]),
        state(&[Edit::Triage(Triage::Invalid)]),
    ];
    let got: Vec<(Filter, usize)> = counts(&states).to_vec();
    assert_eq!(
        got,
        [
            (Filter::All, 6),
            (Filter::ToDo, 4),
            (Filter::Invalid, 1),
            (Filter::Bookmarked, 2),
            (Filter::Commented, 2)
        ]
    );
    assert_eq!(
        progress(&states),
        Progress {
            decided: 2,
            total: 6
        }
    );
    // A bookmark or a comment never moves progress.
    assert!(!states[1].is_decided() && !states[2].is_decided());
}

#[test]
fn an_invalidated_check_is_to_do_and_invalid() {
    let d = decision(&[Edit::Triage(Triage::Valid(dios()))]);
    let s = CheckState::of(Some(&d), Some(&resource()), Some("Señor"));
    assert!(Filter::ToDo.matches(&s));
    assert!(Filter::Invalid.matches(&s));
    assert_eq!(
        progress(&[s]),
        Progress {
            decided: 0,
            total: 1
        }
    );
}

fn item(chapter: &str, verse: &str, category: Category, id: &str) -> CheckItem {
    CheckItem {
        key: CheckKey {
            tool: ToolId::TranslationNotes,
            check_id: tc_check::CheckId::parse(id).unwrap(),
            target: CheckTarget::Verse {
                book: BookId::Titus,
                chapter: RefPart::new(chapter).unwrap(),
                verse: RefPart::new(verse).unwrap(),
            },
            occurrence: 1,
        },
        group_id: "g".into(),
        category,
        quote: Quote::new("q"),
        note: None,
        source: HelpRef::Bible(tc_helps::BibleRef::verse(
            BookId::Titus,
            tc_helps::ChapterVerse::new(1, 1).unwrap(),
        )),
        help_id: HelpItemId::Row(RowId::parse(id).unwrap()),
    }
}

#[test]
fn text_references_sort_ahead_of_numbers() {
    // tC4 test/checkSessionMerge.test.tsx, railGroupsOf.
    let kt = Category::Word(WordCategory::KeyTerm);
    let other = Category::Note(NoteCategory::Other);
    let items = [
        item("1", "3", kt, "a"),
        item("front", "intro", other, "b"),
        item("1", "1-2", kt, "c"),
        item("1", "1", kt, "d"),
    ];
    let states = vec![CheckState::undecided(); items.len()];
    let by_verse = groups(&items, &states, Filter::All, SortMode::ByVerse);
    let order: Vec<String> = by_verse[0]
        .rows
        .iter()
        .map(|&i| items[i].key.target.locator())
        .collect();
    assert_eq!(order, ["front:intro", "1:1-2", "1:1", "1:3"]);
    assert_eq!(by_verse[0].label, GroupLabel::Unit);
    let by_category = groups(&items, &states, Filter::All, SortMode::ByCategory);
    let labels: Vec<GroupLabel> = by_category.into_iter().map(|g| g.label).collect();
    assert_eq!(
        labels,
        [GroupLabel::Category(other), GroupLabel::Category(kt)]
    );
}

#[test]
fn tw_groups_by_term_alphabetically_and_tn_lists_by_verse() {
    assert_eq!(
        SortMode::default_for(ToolId::TranslationWords),
        SortMode::ByWord
    );
    assert_eq!(
        SortMode::default_for(ToolId::TranslationNotes),
        SortMode::ByVerse
    );
    let list = word_links_for_book(
        &word_links("en_twl@v86/TIT.tsv", BookId::Titus),
        None,
        &eng(BookId::Titus),
    );
    let states = vec![CheckState::undecided(); list.items.len()];
    let by_word = groups(&list.items, &states, Filter::All, SortMode::ByWord);
    let terms: Vec<String> = by_word
        .iter()
        .map(|g| match &g.label {
            GroupLabel::Term(t) => t.clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    let mut sorted = terms.clone();
    sorted.sort();
    assert_eq!(terms, sorted, "Titus terms are plain lower-case slugs");
    assert_eq!(by_word.iter().map(|g| g.rows.len()).sum::<usize>(), 188);
    for g in &by_word {
        let GroupLabel::Term(term) = &g.label else {
            unreachable!()
        };
        assert!(g.rows.iter().all(|&i| &list.items[i].group_id == term));
        // Within a term, rows follow the verse order.
        let verses: Vec<(u16, u16)> = g
            .rows
            .iter()
            .map(|&i| {
                let (c, v) = list.items[i].key.target.parts();
                (c.parse().unwrap(), v.parse().unwrap())
            })
            .collect();
        assert!(verses.windows(2).all(|w| w[0] <= w[1]));
    }
    let god = by_word
        .iter()
        .find(|g| g.label == GroupLabel::Term("god".into()))
        .unwrap();
    assert!(god.rows.len() > 5);
}

#[test]
fn a_filter_narrows_the_rail_and_its_keyboard_order() {
    let list = notes_for_book(
        &notes("en_tn@v86/TIT.tsv", BookId::Titus),
        &eng(BookId::Titus),
    );
    let mut states = vec![CheckState::undecided(); list.items.len()];
    for i in [3, 10, 42] {
        states[i] = state(&[Edit::Bookmark(true)]);
    }
    let shown = groups(
        &list.items,
        &states,
        Filter::Bookmarked,
        SortMode::ByCategory,
    );
    let mut rows: Vec<usize> = shown.iter().flat_map(|g| g.rows.clone()).collect();
    rows.sort_unstable();
    assert_eq!(rows, [3, 10, 42]);
    let none = groups(&list.items, &states, Filter::Commented, SortMode::ByVerse);
    assert!(none[0].rows.is_empty());
}

#[test]
fn a_tool_opens_where_the_checker_was_else_at_the_first_undecided() {
    let list = notes_for_book(
        &notes("en_tn@v86/TIT.tsv", BookId::Titus),
        &eng(BookId::Titus),
    );
    let decided = state(&[Edit::Triage(Triage::Valid(dios()))]);
    let mut states = vec![decided.clone(); list.items.len()];
    states[5] = CheckState::undecided();
    assert_eq!(start_index(&list.items, &states, None), 5);
    // A position is a place, a group id and an occurrence, which two notes can share
    // (tC4 then opens at the first); take one no other check shares.
    let positions: Vec<Position> = list.items.iter().map(Position::of).collect();
    let unique = (20..positions.len())
        .find(|&i| positions.iter().filter(|p| **p == positions[i]).count() == 1)
        .unwrap();
    let remembered = positions[unique].clone();
    assert_eq!(start_index(&list.items, &states, Some(&remembered)), unique);
    let shared = (0..positions.len())
        .find(|&i| positions[i + 1..].contains(&positions[i]))
        .unwrap();
    let last_twin = positions
        .iter()
        .rposition(|p| *p == positions[shared])
        .unwrap();
    assert_eq!(
        start_index(&list.items, &states, Some(&positions[last_twin])),
        shared
    );
    let gone = Position {
        group_id: "nowhere".into(),
        ..remembered
    };
    assert_eq!(start_index(&list.items, &states, Some(&gone)), 5);
    let all = vec![decided; list.items.len()];
    assert_eq!(start_index(&list.items, &all, None), 0);
    assert_eq!(next_undecided(&all), None);
}

#[test]
fn to_do_after_valid_still_counts_as_decided_as_in_tc4() {
    // tC4's To do button changes only the status, and its counting rule reads the
    // selections first: the check stays decided.
    let s = state(&[
        Edit::Triage(Triage::Valid(dios())),
        Edit::Triage(Triage::Todo),
    ]);
    assert!(s.is_decided());
    assert_eq!(s.dot(), tc_check::Dot::Todo);
}
