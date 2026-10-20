//! The Check screen's reads (unit B5a) through `App::request`, on the parity scenarios with
//! their resources installed: the tool picker as tC4's baseline shows it, the guided fix's
//! offer, a tool's rail, the rows a teammate's edit changes, and that none of it writes.

use std::sync::Arc;

use tc_api::check::{
    CardCount, CheckListRef, CheckProgress, CheckSessionView, CheckUnit, Open, Picker, PickerView,
    RailRows, RailSince, Rows, RowsView, Rung, Tool, ToolState,
};
use tc_api::ids::Scope;
use tc_api::resources::{FixState, SetRung};
use tc_api::{ApiError, Reply};
use tc_app::App;
use tc_app::backend::memory::MemoryBackend;
use tc_app::backend::scenario::install_resources_into;
use tc_core::BookId;
use tc_journal::RecordKey;

use super::common::{Parts, TestApp, folder};
use super::scenario::scenario;
use super::understand::{ok, opened, start};

fn list(tool: Tool) -> CheckListRef {
    CheckListRef {
        tool,
        unit: CheckUnit::Book(BookId::Titus),
    }
}

fn fix_state(at: &Scope, tool: Tool) -> FixState {
    FixState {
        at: at.clone(),
        tool,
        unit: CheckUnit::Book(BookId::Titus),
    }
}

async fn picker(app: &App, at: &Scope) -> PickerView {
    ok(
        app,
        &Picker {
            at: at.clone(),
            book: BookId::Titus,
        },
    )
    .await
}

async fn open_tool(
    app: &App,
    at: &Scope,
    tool: Tool,
    since: Option<RailSince>,
) -> CheckSessionView {
    ok(
        app,
        &Open {
            at: at.clone(),
            list: list(tool),
            since,
        },
    )
    .await
}

fn counted(card: &tc_api::check::ToolCard) -> &CheckProgress {
    match &card.progress {
        Some(CardCount::Counted(progress)) => progress,
        other => panic!("{:?}: {other:?}", card.tool),
    }
}

fn place(progress: &CheckProgress) -> String {
    progress.next.as_ref().unwrap().place.to_string()
}

/// `check-picker`: Translation Words needs its es-419 pins downloaded, Translation Notes is
/// at 1 of 112 with "knowledge" next, Align at 0 of 5 verses.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_sample_picker_is_tc4s() {
    let (_dir, t) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let view = picker(&t.app, &at).await;
    let tools: Vec<Tool> = view.tools.iter().map(|c| c.tool).collect();
    assert_eq!(tools, [Tool::TranslationWords, Tool::TranslationNotes]);
    let words = &view.tools[0];
    assert_eq!(words.state, ToolState::Fetch);
    let pin = words.pin.as_ref().unwrap();
    assert_eq!(pin.repo_path, "git.door43.org/es-419_gl/es-419_twl");
    assert_eq!(pin.version.as_deref(), Some("v18"));
    assert_eq!(words.rung, Some(Rung::Primary));
    assert_eq!(words.progress, None, "only a ready tool is counted");
    let notes = &view.tools[1];
    assert_eq!(notes.state, ToolState::Ready);
    let progress = counted(notes);
    assert_eq!((progress.decided, progress.total), (1, 112));
    assert_eq!(progress.next.as_ref().unwrap().title, "knowledge");
    assert_eq!(place(progress), "TIT 1:1");
    assert_eq!(progress.dropped, None);
    match &view.align {
        CardCount::Counted(align) => {
            assert_eq!((align.done, align.total), (0, 5));
            assert_eq!(align.next.as_deref(), Some("1:1"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(view.old_twl, None);
}

/// A tool whose pin this computer lacks does not open: the answer is the typed one the screen
/// turns into the picker and its Fix card, not a bad request.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn opening_a_tool_that_is_not_ready_answers_tool_not_ready() {
    let (_dir, t) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let reply = t
        .app
        .request(&Open {
            at,
            list: list(Tool::TranslationWords),
            since: None,
        })
        .await
        .unwrap();
    assert!(
        matches!(reply, Reply::Err(ApiError::ToolNotReady)),
        "{reply:?}"
    );
}

/// `en-check-picker`: every tool ready, Translation Words at 2 of 182 with "Paul" next,
/// Translation Notes at 0 of 157.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_english_picker_is_tc4s() {
    let (_dir, t) = start("en-pins").await;
    let at = opened(&t.app).await.session.at;
    let view = picker(&t.app, &at).await;
    let words = counted(&view.tools[0]);
    assert_eq!((words.decided, words.total), (2, 182));
    assert_eq!(words.next.as_ref().unwrap().title, "Paul");
    assert_eq!(place(words), "TIT 1:1");
    let notes = counted(&view.tools[1]);
    assert_eq!((notes.decided, notes.total), (0, 157));
    assert!(
        notes
            .next
            .as_ref()
            .unwrap()
            .title
            .starts_with("for the faith of the chosen people of God"),
        "{:?}",
        notes.next
    );
}

/// `guided-fix`: the missing es-419 words links pin, its commit, and no installed version of
/// its repository to move to.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_guided_fix_names_the_missing_pin() {
    let (_dir, t) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let fix = ok(&t.app, &fix_state(&at, Tool::TranslationWords))
        .await
        .expect("Translation Words has a fix");
    assert_eq!(fix.pin.repo_path, "git.door43.org/es-419_gl/es-419_twl");
    assert_eq!(fix.pin.sha, "c2d9547ec95e40c2ba085c597b127c94733feb8a");
    assert_eq!(fix.rung, SetRung::Primary);
    assert!(fix.candidates.is_empty());
    let ready = ok(&t.app, &fix_state(&at, Tool::TranslationNotes)).await;
    assert_eq!(ready, None, "a ready tool needs no fix");
}

/// `check-tn-session` resumes at item 2: the first check not decided.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tool_opens_whole_at_its_first_undecided_check() {
    let (_dir, t) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let session = open_tool(&t.app, &at, Tool::TranslationNotes, None).await;
    let RailRows::Full(rows) = &session.rows else {
        panic!("{:?}", session.rows);
    };
    assert_eq!(rows.len(), 112);
    assert_eq!((session.progress.decided, session.progress.total), (1, 112));
    assert_eq!(session.start, 1);
    assert_eq!(session.empty, None);
    let citation = session.citation.unwrap();
    assert_eq!(citation.pin.repo_path, "git.door43.org/es-419_gl/es-419_tn");
    assert_eq!(citation.rung, Rung::Primary);
    // The rows come in By verse order, each with its place in the list.
    let mut indices: Vec<u32> = rows.iter().map(|r| r.index).collect();
    indices.sort_unstable();
    assert_eq!(indices, (0..112).collect::<Vec<u32>>());
    assert!(
        session.terms.contains(&"figs-abstractnouns".to_owned()),
        "a notes rail names its Academy slugs"
    );
}

/// A teammate's edit of a verse arrives by sync and takes the words of a decided check away:
/// the UI that holds the rail gets that row and no other, and opening the tool again with the
/// rail it holds gets the same row (AD17, A-12).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_teammates_edit_sends_only_the_rows_it_changed() {
    let state = scenario("sample");
    let (backend, _) = MemoryBackend::from_scenario(state).unwrap();
    let backend = Arc::new(backend);
    let dir = tempfile::tempdir().unwrap();
    install_resources_into(&dir.path().join("app-data"), &state.resources().unwrap()).unwrap();
    let store = backend.clone();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |_, deps| {
        deps.store = tc_app::StoreChoice::Given(store);
    })
    .await;
    let at = opened(&t.app).await.session.at;
    let held = open_tool(&t.app, &at, Tool::TranslationNotes, None).await;
    let RailRows::Full(rows) = &held.rows else {
        panic!("{:?}", held.rows);
    };
    let decided = rows.iter().find(|r| r.flags.decided).unwrap().clone();
    let since = RailSince {
        session: held.session,
        view: held.view,
    };
    let tc_api::check::CheckPlace::Verse { chapter, verse, .. } = &decided.place else {
        panic!("{:?}", decided.place);
    };
    let key = RecordKey::Verse(
        BookId::Titus,
        tc_journal::ids::ChapterNo::new(chapter.as_str().parse().unwrap()).unwrap(),
        tc_journal::ids::VerseNo::new(verse.as_str().parse().unwrap()).unwrap(),
    );
    let project = folder("sample_burrito");
    let edits = backend
        .teammates_edit_verse(&project, &key, &["Otras palabras del todo."])
        .unwrap();
    backend.teammate_edits(&project, edits).unwrap();
    let moved = ok(
        &t.app,
        &Rows {
            at: at.clone(),
            list: list(Tool::TranslationNotes),
            since,
        },
    )
    .await;
    let RowsView::Changed {
        rows: changed,
        progress,
        view,
    } = moved
    else {
        panic!("{moved:?}");
    };
    assert!(view > since.view);
    assert_eq!(changed.len(), 1, "{changed:?}");
    assert_eq!(changed[0].row.index, decided.index);
    assert!(!changed[0].row.flags.decided && changed[0].row.flags.invalidated);
    assert_eq!(progress.decided, 0);
    let again = open_tool(&t.app, &at, Tool::TranslationNotes, Some(since)).await;
    assert_eq!(again.session, held.session, "the list is not derived again");
    assert_eq!(again.rows, RailRows::Since(changed.clone()));
    // Asked from the newest view, nothing is sent.
    let quiet = open_tool(
        &t.app,
        &at,
        Tool::TranslationNotes,
        Some(RailSince {
            session: again.session,
            view: again.view,
        }),
    )
    .await;
    assert_eq!(quiet.rows, RailRows::Since(vec![]));
}

/// tC4 #501: the picker, the guided fix's offer and a tool's rail change nothing in the
/// project: no command reaches the store and the pins read the same.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn open_check_preflight_offer_close_leaves_pins_unchanged() {
    let state = scenario("sample");
    let (backend, _) = MemoryBackend::from_scenario(state).unwrap();
    let backend = Arc::new(backend);
    let dir = tempfile::tempdir().unwrap();
    install_resources_into(&dir.path().join("app-data"), &state.resources().unwrap()).unwrap();
    let store = backend.clone();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |_, deps| {
        deps.store = tc_app::StoreChoice::Given(store);
    })
    .await;
    let at = opened(&t.app).await.session.at;
    let project = folder("sample_burrito");
    let pins = |b: &MemoryBackend| {
        let fold = b.fold(&project).unwrap();
        tc_journal::PinGroup::ALL.map(|g| fold.projected(&RecordKey::Pins(g)))
    };
    let before = pins(&backend);
    picker(&t.app, &at).await;
    ok(&t.app, &fix_state(&at, Tool::TranslationWords)).await;
    open_tool(&t.app, &at, Tool::TranslationNotes, None).await;
    picker(&t.app, &at).await;
    assert!(backend.submitted(&project).is_empty());
    assert_eq!(pins(&backend), before);
}

/// The rail differential test on Titus: the UI's groups over the served rows equal
/// tc-check's for every filter and mode of both tools, on both scenarios.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_rail_groups_equal_tc_checks_on_titus() {
    for (name, tools) in [
        ("sample", &[Tool::TranslationNotes][..]),
        (
            "en-pins",
            &[Tool::TranslationNotes, Tool::TranslationWords][..],
        ),
    ] {
        let state = scenario(name);
        let (backend, _) = MemoryBackend::from_scenario(state).unwrap();
        let backend = Arc::new(backend);
        let dir = tempfile::tempdir().unwrap();
        install_resources_into(&dir.path().join("app-data"), &state.resources().unwrap()).unwrap();
        let store = backend.clone();
        let t = TestApp::start_with(dir.path(), &Parts::new(), |_, deps| {
            deps.store = tc_app::StoreChoice::Given(store);
        })
        .await;
        let at = opened(&t.app).await.session.at;
        let fold = backend.fold(&folder("sample_burrito")).unwrap();
        for &tool in tools {
            let session = open_tool(&t.app, &at, tool, None).await;
            let RailRows::Full(rows) = &session.rows else {
                panic!("{:?}", session.rows);
            };
            let (items, states) = crate::rail_differential::tc_check_side(
                &dir.path().join("app-data/resources"),
                &fold,
                tool,
                BookId::Titus,
            );
            crate::rail_differential::assert_groups_agree(
                tool,
                rows,
                &session.terms,
                &items,
                &states,
            );
        }
    }
}
