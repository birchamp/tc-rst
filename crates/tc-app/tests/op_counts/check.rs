//! The count rows of B5a (§8.2), on Psalms and on the corpus ten times larger: opening a tool
//! derives its list once, a cached open derives nothing and sends only the rows changed since
//! the view the UI holds, and the UI's grouping of the Psalms rails equals tc-check's.

use tc_api::Reply;
use tc_api::check::{
    CheckListRef, CheckSessionView, CheckUnit, Open, RailRow, RailRows, RailSince, Tool,
};
use tc_app::App;
use tc_journal::RecordKey;
use tc_journal::ids::{ChapterNo, VerseNo};

use super::corpus::{PSALMS, corpora, psalms_resources};
use super::harness::{Backing, OnStore, Scale, cost, costs_what_it_touches, opened, scale};

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
    let ((small, large), (store, _)) = costs_what_it_touches(
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
