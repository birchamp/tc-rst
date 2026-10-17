//! Which queries a change makes stale (APP-DESIGN §3.6, §6.2): a refresh is never missed
//! for a part a change names, and a change elsewhere sends no request.
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tc_api::check::{CheckListRef, Tool};
use tc_api::event::{Aspect, ChangeParts, ChangeScope, ChapterRef, RecordChange};
use tc_api::ids::RecordRef;
use tc_api::query::QueryKey;
use tc_core::BookId;

fn chapter(book: BookId, chapter: u16) -> ChapterRef {
    ChapterRef { book, chapter }
}

fn record(text: &str) -> RecordRef {
    RecordRef::parse(text).unwrap()
}

fn parts(edit: impl FnOnce(&mut ChangeParts)) -> ChangeScope {
    let mut parts = ChangeParts::default();
    edit(&mut parts);
    ChangeScope::Parts(parts)
}

fn tn(book: BookId) -> CheckListRef {
    CheckListRef {
        tool: Tool::TranslationNotes,
        book,
    }
}

fn every_kind_of_key() -> Vec<QueryKey> {
    vec![
        QueryKey::Chapter(chapter(BookId::Titus, 1)),
        QueryKey::Book(BookId::Titus),
        QueryKey::Story(7),
        QueryKey::CheckList(tn(BookId::Titus)),
        QueryKey::Record(record(r#"{"kind":"verse"}"#)),
        QueryKey::Aspect(Aspect::Settings),
    ]
}

#[test]
fn everything_refreshes_every_query_and_nothing_refreshes_none() {
    for key in every_kind_of_key() {
        assert!(key.depends_on(&ChangeScope::Everything), "{key:?}");
        assert!(!key.depends_on(&parts(|_| {})), "{key:?}");
    }
}

#[test]
fn a_chapter_follows_its_own_edits_and_its_book() {
    let key = QueryKey::Chapter(chapter(BookId::Titus, 2));
    assert!(key.depends_on(&parts(|p| p.chapters.push(chapter(BookId::Titus, 2)))));
    assert!(key.depends_on(&parts(|p| p.books.push(BookId::Titus))));
    assert!(!key.depends_on(&parts(|p| p.chapters.push(chapter(BookId::Titus, 1)))));
    assert!(!key.depends_on(&parts(|p| p.chapters.push(chapter(BookId::Ruth, 2)))));
    assert!(!key.depends_on(&parts(|p| p.books.push(BookId::Ruth))));
}

#[test]
fn a_book_follows_any_of_its_chapters() {
    let key = QueryKey::Book(BookId::Psalms);
    assert!(key.depends_on(&parts(|p| p.chapters.push(chapter(BookId::Psalms, 119)))));
    assert!(key.depends_on(&parts(|p| p.books.push(BookId::Psalms))));
    assert!(!key.depends_on(&parts(|p| p.chapters.push(chapter(BookId::Titus, 1)))));
    assert!(!key.depends_on(&parts(|p| p.stories.push(1))));
}

#[test]
fn a_check_list_follows_its_rows_its_book_and_the_pins() {
    let key = QueryKey::CheckList(tn(BookId::Titus));
    assert!(key.depends_on(&parts(|p| p.checks.push(tn(BookId::Titus)))));
    assert!(key.depends_on(&parts(|p| p.books.push(BookId::Titus))));
    assert!(key.depends_on(&parts(|p| p.aspects.push(Aspect::Pins))));
    let tw = CheckListRef {
        tool: Tool::TranslationWords,
        book: BookId::Titus,
    };
    assert!(!key.depends_on(&parts(|p| p.checks.push(tw))));
    assert!(!key.depends_on(&parts(|p| p.checks.push(tn(BookId::Ruth)))));
    // A verse edit re-assesses checks and names their lists itself; the text alone does not
    // re-send a rail (A-12).
    assert!(!key.depends_on(&parts(|p| p.chapters.push(chapter(BookId::Titus, 1)))));
}

#[test]
fn a_story_a_record_and_an_aspect_follow_only_themselves() {
    assert!(QueryKey::Story(7).depends_on(&parts(|p| p.stories.push(7))));
    assert!(!QueryKey::Story(7).depends_on(&parts(|p| p.stories.push(8))));
    let mine = record(r#"{"kind":"decision","n":1}"#);
    let other = record(r#"{"kind":"decision","n":2}"#);
    let change = |r: &RecordRef| RecordChange {
        record: r.clone(),
        by: None,
    };
    let key = QueryKey::Record(mine.clone());
    assert!(key.depends_on(&parts(|p| p.records.push(change(&mine)))));
    assert!(!key.depends_on(&parts(|p| p.records.push(change(&other)))));
    let key = QueryKey::Aspect(Aspect::Conflicts);
    assert!(key.depends_on(&parts(|p| p.aspects.push(Aspect::Conflicts))));
    assert!(!key.depends_on(&parts(|p| p.aspects.push(Aspect::Meta))));
}

#[test]
fn a_scope_over_the_record_cap_is_everything() {
    let change = |n: usize| RecordChange {
        record: record(&format!(r#"{{"n":{n}}}"#)),
        by: None,
    };
    let at_cap = ChangeParts {
        records: (0..ChangeParts::RECORDS_CAP).map(change).collect(),
        ..ChangeParts::default()
    };
    assert!(matches!(
        ChangeScope::bounded(at_cap.clone()),
        ChangeScope::Parts(_)
    ));
    let mut over = at_cap;
    over.records.push(change(ChangeParts::RECORDS_CAP));
    assert_eq!(ChangeScope::bounded(over), ChangeScope::Everything);
}
