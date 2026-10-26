//! The count rows of B5a and C4 (§8.2), on Psalms and on the corpus ten times larger: opening a
//! tool derives its list once, a cached open derives nothing and sends only the rows changed
//! since the view the UI holds, the UI's grouping of the Psalms rails equals tc-check's, a
//! decision serializes its one row and the `check/rows` after it no rail, and a verse edit
//! re-assesses the decided checks of its chapter only.

use std::sync::Mutex;
use tc_api::Reply;

use tc_api::check::{
    CheckItemView, CheckListRef, CheckPlace, CheckSessionView, CheckUnit, Decide, DecideEdit, Item,
    Open, RailRow, RailRows, RailSince, Rows, RowsView, Tool,
};
use tc_api::editor::SaveVerse;
use tc_app::App;
use tc_app::backend::memory::Scripted;
use tc_app::wire::token::shown;
use tc_journal::Record;
use tc_journal::RecordKey;
use tc_journal::ids::{ChapterNo, VerseNo};
use tc_journal::record::VerseValue;

use super::corpus::{PSALMS, corpora, psalms_resources};
use super::harness::{
    Backing, OnStore, Scale, book_costs_what_it_touches, cost, costs_what_it_touches, opened, scale,
};
use crate::common::rid;

async fn ok<E: tc_api::Endpoint>(app: &App, req: &E) -> E::Ok {
    match app.request(req).await.unwrap() {
        Reply::Ok(ok) => ok,
        Reply::Err(e) => panic!("{}: {e:?}", E::PATH),
    }
}

fn list(tool: Tool) -> CheckListRef {
    CheckListRef {
        tool,
        unit: CheckUnit::Book(PSALMS),
    }
}

async fn open_tool(s: &Scale, tool: Tool, since: Option<RailSince>) -> CheckSessionView {
    ok(
        &s.app,
        &Open {
            at: s.scope(),
            list: list(tool),
            since,
        },
    )
    .await
}

fn full(view: &CheckSessionView) -> &[RailRow] {
    match &view.rows {
        RailRows::Full(rows) => rows,
        RailRows::Since(_) => panic!("a first open sends the whole rail"),
    }
}

/// The first open of a tool derives its list, Psalms' alone, whatever else the project holds.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_open_derives_one_unit() {
    let ((small, large), (store, _)) = book_costs_what_it_touches(
        "opening the Psalms notes",
        OnStore::AppCounts,
        opened,
        async |s: &Scale| {
            let view = open_tool(s, Tool::TranslationNotes, None).await;
            assert!(full(&view).len() > 7_000, "{}", full(&view).len());
        },
    )
    .await;
    assert!(small.app.items_derived > 7_000, "{small:?}");
    assert_eq!(large.app.items_derived, small.app.items_derived);
    assert_eq!(small.app.book_resource_loads, 1);
    assert_eq!(store.app.items_derived, small.app.items_derived);
}

/// A second open with the rail the UI holds derives nothing, loads nothing, reads nothing and
/// sends no row (PERF:77, 20 ms from the cache).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_open_cached_derives_nothing() {
    let ((small, large), (store, _)) = costs_what_it_touches(
        "opening the Psalms words again",
        // Each open reads the pins to know the tool is still ready: their values come from
        // the packs that hold them, whose framing is the store's own budget.
        OnStore::AppCounts,
        async |s: &Scale| {
            opened(s).await;
            open_tool(s, Tool::TranslationWords, None).await;
        },
        async |s: &Scale| {
            let held = open_tool(s, Tool::TranslationWords, None).await;
            let since = RailSince {
                session: held.session,
                view: held.view,
            };
            let again = open_tool(s, Tool::TranslationWords, Some(since)).await;
            assert_eq!(again.rows, RailRows::Since(vec![]));
        },
    )
    .await;
    for cost in [small, large, store] {
        assert_eq!(cost.app.items_derived, 0, "{cost:?}");
        assert_eq!(cost.app.book_resource_loads, 0, "{cost:?}");
    }
    // The pins, the numbering and the book's scope, read for each of the two opens.
    assert!(small.app.records_read <= 2 * 7, "{small:?}");
    // The first of the two opens was a UI without a rail: it is sent whole, once.
    assert_eq!(small.app.rows_serialized, large.app.rows_serialized);
}

/// B5b: showing a check of a list already open costs the check: its verse's chapter keys and
/// records, never the list, the book's text or the helps again, and the same on the larger
/// corpus.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_item_costs_its_verse_not_the_book() {
    let ((small, large), (store, _)) = costs_what_it_touches(
        "showing a check of the Psalms notes",
        OnStore::AppCounts,
        async |s: &Scale| {
            opened(s).await;
            open_tool(s, Tool::TranslationNotes, None).await;
        },
        async |s: &Scale| {
            let item = ok(
                &s.app,
                &Item {
                    at: s.scope(),
                    list: list(Tool::TranslationNotes),
                    index: 0,
                },
            )
            .await;
            assert!(!item.title.is_empty());
        },
    )
    .await;
    for cost in [small, large, store] {
        assert_eq!(cost.app.items_derived, 0, "{cost:?}");
        assert_eq!(cost.app.rows_serialized, 0, "{cost:?}");
    }
    // The pins and the numbering, the verse's chapter (Psalm 119 has 176 verses) and its record.
    assert!(small.app.keys_probed <= 200, "{small:?}");
    assert!(small.app.records_read <= 20, "{small:?}");
}

/// A teammate's edit of a verse arrives by sync and takes the words of its decided checks
/// away: the open that follows sends exactly the rows that changed (A-12).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_open_cached_sends_changed_rows_only() {
    let corpora = corpora();
    for project in [&corpora.psalms, &corpora.larger] {
        let s = scale(
            Backing::Memory,
            project,
            &super::corpus::store_corpora().psalms,
        )
        .await;
        s.open().await;
        let held = open_tool(&s, Tool::TranslationNotes, None).await;
        let before = full(&held).to_vec();
        let decided = before.iter().find(|r| r.flags.decided).unwrap().clone();
        let tc_api::check::CheckPlace::Verse { chapter, verse, .. } = &decided.place else {
            panic!("{:?}", decided.place);
        };
        let key = RecordKey::Verse(
            PSALMS,
            ChapterNo::new(chapter.as_str().parse().unwrap()).unwrap(),
            VerseNo::new(verse.as_str().parse().unwrap()).unwrap(),
        );
        let memory = s.memory().unwrap();
        let edits = memory
            .teammates_edit_verse(&s.folder, &key, &["Otras palabras del todo."])
            .unwrap();
        memory.teammate_edits(&s.folder, edits).unwrap();
        let since = RailSince {
            session: held.session,
            view: held.view,
        };
        let spent = cost(&s, async |s| {
            let again = open_tool(s, Tool::TranslationNotes, Some(since)).await;
            assert!(matches!(again.rows, RailRows::Since(_)));
        })
        .await;
        let after = open_tool(&s, Tool::TranslationNotes, None).await;
        let mut changed: Vec<u32> = full(&after)
            .iter()
            .filter(|row| before.iter().find(|r| r.index == row.index) != Some(row))
            .map(|row| row.index)
            .collect();
        changed.sort_unstable();
        assert!(changed.contains(&decided.index));
        assert_eq!(spent.app.rows_serialized, changed.len() as u64);
        assert_eq!(spent.app.items_derived, 0);
    }
}

/// The rail differential test on Psalms: the UI's groups over the served rows equal tc-check's
/// for every filter and mode of both tools.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_rail_groups_equal_tc_checks_on_psalms() {
    let corpora = corpora();
    let s = scale(
        Backing::Memory,
        &corpora.psalms,
        &super::corpus::store_corpora().psalms,
    )
    .await;
    s.open().await;
    for tool in [Tool::TranslationNotes, Tool::TranslationWords] {
        let view = open_tool(&s, tool, None).await;
        let (items, states) = crate::rail_differential::tc_check_side(
            psalms_resources(),
            &corpora.psalms.fold,
            tool,
            PSALMS,
        );
        let rows = crate::rail_differential::assert_groups_agree(
            tool,
            full(&view),
            &view.terms,
            &items,
            &states,
        );
        assert!(rows > 7_000, "{tool:?}: {rows}");
    }
}

/// The check of the Psalms notes in the chapter alike in both corpora (`corpora().chapter`) that
/// `pick` chooses among the rail's rows there.
fn in_the_quiet_chapter(rows: &[RailRow], pick: impl Fn(&RailRow) -> bool) -> RailRow {
    let chapter = corpora().chapter.to_string();
    rows.iter()
        .find(|row| {
            matches!(&row.place, CheckPlace::Verse { chapter: c, .. } if c.as_str() == chapter)
                && pick(row)
        })
        .cloned()
        .expect("a check of the quiet chapter")
}

async fn shown_item(s: &Scale, index: u32) -> CheckItemView {
    ok(
        &s.app,
        &Item {
            at: s.scope(),
            list: list(Tool::TranslationNotes),
            index,
        },
    )
    .await
}

fn decide(s: &Scale, view: &CheckItemView) -> Decide {
    Decide {
        at: s.scope(),
        request: rid(30),
        list: list(Tool::TranslationNotes),
        place: view.place.clone(),
        check_id: view.check_id.clone(),
        decision: view.decision.token.clone(),
        verse: view.target.verse.clone(),
        edit: DecideEdit::Invalid,
    }
}

/// PERF:79, AD17: a mark on a check of the open rail writes one decision, serializes its one
/// row into the `Ack`, re-assesses that check alone and derives nothing, on Psalms as on the
/// corpus ten times larger.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn decision_serializes_one_row() {
    let shown = Mutex::new(Vec::new());
    let ((small, large), (store, store_large)) = costs_what_it_touches(
        "a mark on a check",
        OnStore::AppCounts,
        async |s: &Scale| {
            opened(s).await;
            let view = open_tool(s, Tool::TranslationNotes, None).await;
            let row = in_the_quiet_chapter(full(&view), |r| !r.flags.decided);
            let item = shown_item(s, row.index).await;
            shown.lock().unwrap().push(item);
            if let Some(memory) = s.memory() {
                memory.script(&s.folder, Scripted::Written).unwrap();
            }
        },
        async |s: &Scale| {
            let view = shown.lock().unwrap().pop().unwrap();
            let ack = ok(&s.app, &decide(s, &view)).await;
            assert_eq!(ack.rows.len(), 1, "the check's row");
            assert_eq!(ack.rows[0].row.index, view.index);
        },
    )
    .await;
    for cost in [small, large, store, store_large] {
        assert_eq!(cost.app.rows_serialized, 1, "{cost:?}");
        assert_eq!(cost.app.items_derived, 0, "{cost:?}");
        assert_eq!(cost.app.book_usfm, 0, "{cost:?}");
    }
    // The memory backend's scripted write folds nothing; the store's folds the decision.
    assert_eq!(small.app.checks_assessed, 0, "{small:?}");
    assert_eq!(store.app.checks_assessed, 1, "{store:?}");
}

/// AD17: the `Changed` a mark makes names the list, and the `check/rows { since }` the page asks
/// then sends the one row the mark changed, which the `Ack` already brought: no rail, and
/// nothing re-assessed again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn decision_refetches_no_rail() {
    let held = Mutex::new(Vec::new());
    let ((small, _), (store, store_large)) = costs_what_it_touches(
        "the rows after a mark",
        OnStore::AppCounts,
        async |s: &Scale| {
            opened(s).await;
            let view = open_tool(s, Tool::TranslationNotes, None).await;
            let row = in_the_quiet_chapter(full(&view), |r| !r.flags.decided);
            let item = shown_item(s, row.index).await;
            if let Some(memory) = s.memory() {
                memory.script(&s.folder, Scripted::Written).unwrap();
            }
            ok(&s.app, &decide(s, &item)).await;
            held.lock().unwrap().push((
                RailSince {
                    session: view.session,
                    view: view.view,
                },
                row.index,
            ));
        },
        async |s: &Scale| {
            let (since, index) = held.lock().unwrap().pop().unwrap();
            let rows = ok(
                &s.app,
                &Rows {
                    at: s.scope(),
                    list: list(Tool::TranslationNotes),
                    since,
                },
            )
            .await;
            let RowsView::Changed { rows, .. } = rows else {
                panic!("the list is the one the page holds: {rows:?}");
            };
            if s.memory().is_none() {
                assert_eq!(rows.len(), 1, "{rows:?}");
                assert_eq!(rows[0].row.index, index);
            }
        },
    )
    .await;
    for cost in [small, store, store_large] {
        assert_eq!(cost.app.checks_assessed, 0, "{cost:?}");
        assert_eq!(cost.app.items_derived, 0, "{cost:?}");
        assert_eq!(cost.app.book_usfm, 0, "{cost:?}");
    }
    assert_eq!(store.app.rows_serialized, 1, "{store:?}");
}

/// PERF:80: a verse of the quiet chapter edited (a teammate's change received on the memory
/// backend, a save through Translate on the store): the rows the page asks for then
/// re-assess the checks of that chapter that hold a decision, never the list's other seven
/// thousand, read that chapter's text, and derive nothing, on Psalms as on the corpus ten
/// times larger.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn verse_edit_reassesses_its_checks_only() {
    let held = Mutex::new(Vec::new());
    let ((small, large), (store, store_large)) = costs_what_it_touches(
        "the rows after a verse edit",
        OnStore::AppCounts,
        async |s: &Scale| {
            opened(s).await;
            let view = open_tool(s, Tool::TranslationNotes, None).await;
            let in_chapter = full(&view)
                .iter()
                .filter(|r| {
                    matches!(&r.place, CheckPlace::Verse { chapter, .. }
                        if chapter.as_str() == corpora().chapter.to_string())
                })
                .count();
            let row = in_the_quiet_chapter(full(&view), |r| {
                r.flags.decided
                    && matches!(&r.place, CheckPlace::Verse { verse, .. }
                        if verse.as_str().parse::<u16>().is_ok())
            });
            let item = shown_item(s, row.index).await;
            let session = s.app.read(&s.scope()).unwrap();
            let read = session.read();
            let CheckPlace::Verse { chapter, verse, .. } = &item.place else {
                panic!("{:?}", item.place);
            };
            let key = RecordKey::Verse(
                PSALMS,
                ChapterNo::new(chapter.as_str().parse().unwrap()).unwrap(),
                VerseNo::new(verse.as_str().parse().unwrap()).unwrap(),
            );
            // Words added keep every selection: the states stay, so what is counted is the
            // re-assessment alone, the same on both corpora.
            let Some(Record::Verse {
                value: VerseValue::Text { words, .. },
                ..
            }) = read.projected(&key)
            else {
                panic!("a drafted verse");
            };
            let words = format!("{words} y más");
            match s.memory() {
                Some(memory) => {
                    let edits = memory
                        .teammates_edit_verse(&s.folder, &key, &[&words])
                        .unwrap();
                    memory.teammate_edits(&s.folder, edits).unwrap();
                }
                None => {
                    let token = shown(&read.mint(&key, None).unwrap()).unwrap();
                    ok(
                        &s.app,
                        &SaveVerse {
                            at: s.scope(),
                            request: rid(31),
                            shown: token,
                            words,
                        },
                    )
                    .await;
                }
            }
            held.lock().unwrap().push((
                RailSince {
                    session: view.session,
                    view: view.view,
                },
                in_chapter,
            ));
        },
        async |s: &Scale| {
            let (since, in_chapter) = held.lock().unwrap().pop().unwrap();
            let before = s.app.counters();
            let rows = ok(
                &s.app,
                &Rows {
                    at: s.scope(),
                    list: list(Tool::TranslationNotes),
                    since,
                },
            )
            .await;
            let spent = s.app.counters().since(&before);
            assert!(matches!(rows, RowsView::Changed { .. }), "{rows:?}");
            // The checks of the chapter that hold a decision, never the list's others.
            assert!(
                spent.checks_assessed <= in_chapter as u64,
                "{spent:?} of {in_chapter}"
            );
        },
    )
    .await;
    for cost in [small, large, store, store_large] {
        assert!(cost.app.checks_assessed > 0, "{cost:?}");
        assert_eq!(cost.app.items_derived, 0, "{cost:?}");
        assert_eq!(cost.app.book_usfm, 0, "{cost:?}");
        assert_eq!(cost.app.chapters_projected, 1, "{cost:?}");
    }
}
