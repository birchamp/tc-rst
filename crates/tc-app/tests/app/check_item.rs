//! The Check screen's item detail and marks (unit B5b) through `App::request`, on the parity
//! scenarios with their resources installed: what a check shows (the note, the panes, the draft
//! words, the decision), that reading writes nothing, and the marks on the real store and on
//! scripted outcomes.

use std::collections::BTreeSet;
use std::sync::Arc;

use proptest::prelude::*;
use tc_api::check::{
    CheckArticleRead, CheckItemView, CheckListRef, CheckSessionView, CheckUnit, Decide, DecideEdit,
    DecisionStatus, GatewayText, GatewayView, Item, Open, OriginalView, RailRow, RailRows,
    Testament, Tool,
};
use tc_api::helps::ArticleView;
use tc_api::ids::Scope;
use tc_api::model::selection::drag_range;
use tc_api::test::SaveVerse;
use tc_api::write::Stored;
use tc_api::{ApiError, NotSaved, Reply};
use tc_app::App;
use tc_app::StoreChoice;
use tc_app::backend::memory::{MemoryBackend, Scripted};
use tc_app::backend::scenario::install_resources_into;
use tc_app::backend::scenario_store::ScenarioStore;
use tc_app::config::ProjectsFolder;
use tc_check::selection::{TargetText, from_indices, indices, target_words};
use tc_core::BookId;
use tc_store::FakePlatform;
use tc_store::fault::FaultFs;

use super::common::{Parts, TestApp, folder, open_progress, post, rid, tit};
use super::scenario::scenario;
use super::understand::{ok, opened, start};
use crate::store_world::{APP, backend_over};

fn list(tool: Tool) -> CheckListRef {
    CheckListRef {
        tool,
        unit: CheckUnit::Book(BookId::Titus),
    }
}

async fn session(app: &App, at: &Scope, tool: Tool) -> CheckSessionView {
    ok(
        app,
        &Open {
            at: at.clone(),
            list: list(tool),
            since: None,
        },
    )
    .await
}

fn rows(session: &CheckSessionView) -> &[RailRow] {
    match &session.rows {
        RailRows::Full(rows) => rows,
        other => panic!("{other:?}"),
    }
}

async fn item(app: &App, at: &Scope, tool: Tool, index: u32) -> CheckItemView {
    ok(
        app,
        &Item {
            at: at.clone(),
            list: list(tool),
            index,
        },
    )
    .await
}

fn the_first(rows: &[RailRow], title: &str) -> u32 {
    rows.iter()
        .find(|r| r.title.starts_with(title))
        .unwrap_or_else(|| panic!("no check titled {title}"))
        .index
}

/// `en-check-tn-session`: item 1 shows its quote as the ULT renders it, the note, the Academy
/// article, the Greek verse, the ULT verse with the quote's words lit, and the es-419 draft.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_note_shows_its_panes_its_article_and_the_draft_words() {
    let (_dir, t) = start("en-pins").await;
    let at = opened(&t.app).await.session.at;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let view = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert_eq!(view.place.to_string(), "TIT 1:1");
    assert!(view.title.starts_with("for the faith of the chosen people"));
    let note: String = view.note.iter().map(|b| b.text()).collect();
    assert!(note.contains("abstract nouns"), "{note}");
    assert_eq!(view.article.title.as_deref(), Some("Abstract Nouns"));
    match &view.original {
        OriginalView::Ready {
            testament: Testament::Nt,
            verse: Some(verse),
        } => assert!(verse.starts_with("Παῦλος"), "{verse}"),
        other => panic!("{other:?}"),
    }
    let GatewayView::Pane {
        pane,
        text: GatewayText::Verse { tokens, lit },
    } = &view.gateway
    else {
        panic!("{:?}", view.gateway);
    };
    assert_eq!(pane, "ult");
    let lit_words: String = lit.iter().map(|&i| tokens[i as usize].as_str()).collect();
    assert!(lit_words.starts_with("forthefaith"), "{lit_words}");
    assert_eq!(view.target.words[..3], ["Pablo", "siervo", "de"]);
    assert!(view.target.selected.is_empty());
    assert!(view.target.verse.is_some());
    assert_eq!(view.decision.value.status, None);
    assert!(view.notices.is_empty(), "{:?}", view.notices);
}

/// `en-check-tw-article`: a decided word check shows its selected word, and its article is
/// read from the resource the list derives from.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_decided_word_check_marks_its_selected_word_and_serves_its_article() {
    let (_dir, t) = start("en-pins").await;
    let at = opened(&t.app).await.session.at;
    let open = session(&t.app, &at, Tool::TranslationWords).await;
    let decided = rows(&open)
        .iter()
        .find(|r| r.title == "of God" && r.flags.decided)
        .expect("the decided “of God”")
        .index;
    let view = item(&t.app, &at, Tool::TranslationWords, decided).await;
    assert_eq!(view.decision.value.status, Some(DecisionStatus::Valid));
    let words: Vec<&str> = view
        .target
        .selected
        .iter()
        .map(|&i| view.target.words[i as usize].as_str())
        .collect();
    assert_eq!(words, ["Dios"]);
    assert_eq!(view.article.title.as_deref(), Some("God"));
    let article = ok(
        &t.app,
        &CheckArticleRead {
            at,
            list: list(Tool::TranslationWords),
            index: decided,
        },
    )
    .await;
    let ArticleView::Found { title, blocks, .. } = article else {
        panic!("{article:?}");
    };
    assert_eq!(title, "God");
    assert!(blocks.len() > 5);
}

/// tC4 #501: showing every check of a list writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn showing_checks_writes_nothing() {
    let state = scenario("en-pins");
    let (backend, _) = MemoryBackend::from_scenario(state).unwrap();
    let backend = Arc::new(backend);
    let dir = tempfile::tempdir().unwrap();
    install_resources_into(&dir.path().join("app-data"), &state.resources().unwrap()).unwrap();
    let store = backend.clone();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |_, deps| {
        deps.store = StoreChoice::Given(store);
    })
    .await;
    let at = opened(&t.app).await.session.at;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    for row in rows(&open).iter().take(30) {
        item(&t.app, &at, Tool::TranslationNotes, row.index).await;
    }
    assert!(backend.submitted(&folder("sample_burrito")).is_empty());
}

/// A check the list does not have is a bad request, never another check.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_index_past_the_list_is_refused() {
    let (_dir, t) = start("en-pins").await;
    let at = opened(&t.app).await.session.at;
    let reply = t
        .app
        .request(&Item {
            at,
            list: list(Tool::TranslationNotes),
            index: 9_999,
        })
        .await
        .unwrap();
    assert!(
        matches!(reply, Reply::Err(ApiError::BadRequest { .. })),
        "{reply:?}"
    );
}

// ---- marks, on the real store ---------------------------------------------------------------

const SAMPLE: &str = "sample_burrito";

/// The app over the `en-pins` scenario written through tc-store, with its resources installed.
async fn on_the_store() -> (tempfile::TempDir, TestApp, FaultFs) {
    let fs = FaultFs::new();
    let backend = backend_over(&fs, APP, FakePlatform::default(), Arc::default());
    let (store, _) = ScenarioStore::write(backend, scenario("en-pins")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    install_resources_into(
        &dir.path().join("app-data"),
        &scenario("en-pins").resources().unwrap(),
    )
    .unwrap();
    let projects = dir.path().join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |config, deps| {
        config.projects = ProjectsFolder::Given(projects.clone());
        deps.store = StoreChoice::Given(Arc::new(store));
    })
    .await;
    (dir, t, fs)
}

async fn open_on_store(t: &TestApp) -> Scope {
    t.app
        .sessions()
        .open_screen(&folder(SAMPLE), open_progress())
        .await
        .unwrap()
        .at
}

fn decide(at: &Scope, request: u8, view: &CheckItemView, edit: DecideEdit) -> Decide {
    Decide {
        at: at.clone(),
        request: rid(request),
        list: list(Tool::TranslationNotes),
        place: view.place.clone(),
        check_id: view.check_id.clone(),
        decision: view.decision.token.clone(),
        verse: view.target.verse.clone(),
        edit,
    }
}

/// Mark valid with two tapped words stores them; the rail's row and the progress follow, and
/// the item read again shows the words, the status and the new tokens.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mark_valid_stores_the_tapped_words_and_the_rail_follows() {
    let (_dir, t, _fs) = on_the_store().await;
    let at = open_on_store(&t).await;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let view = item(&t.app, &at, Tool::TranslationNotes, first).await;
    let tapped: Vec<u32> = vec![11, 12, 13];
    let ack = post(
        &t.app,
        &decide(
            &at,
            1,
            &view,
            DecideEdit::Valid {
                words: tapped.clone(),
            },
        ),
    )
    .await
    .unwrap();
    assert_eq!(ack.stored, Stored::Written);
    assert_eq!(ack.rows.len(), 1, "the check's row");
    assert!(ack.rows[0].row.flags.decided);
    assert_eq!(ack.rows[0].row.index, first);
    let now = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert_eq!(now.decision.value.status, Some(DecisionStatus::Valid));
    assert_eq!(now.target.selected, tapped);
    assert_ne!(now.decision.token, view.decision.token, "a new version");
    let after = session(&t.app, &at, Tool::TranslationNotes).await;
    assert_eq!(after.progress.decided, open.progress.decided + 1);
}

/// Mark valid with no word is "nothing to select"; Invalid clears the words; To do keeps the
/// status apart from the words, as tC4's buttons do.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_triage_buttons_follow_tc4s_rules() {
    let (_dir, t, _fs) = on_the_store().await;
    let at = open_on_store(&t).await;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let read = || item(&t.app, &at, Tool::TranslationNotes, first);
    let view = read().await;
    post(
        &t.app,
        &decide(&at, 2, &view, DecideEdit::Valid { words: vec![] }),
    )
    .await
    .unwrap();
    let valid = read().await;
    assert_eq!(valid.decision.value.status, Some(DecisionStatus::Valid));
    assert!(valid.decision.value.nothing_to_select);
    assert!(valid.target.selected.is_empty());
    post(&t.app, &decide(&at, 3, &valid, DecideEdit::Invalid))
        .await
        .unwrap();
    let invalid = read().await;
    assert_eq!(invalid.decision.value.status, Some(DecisionStatus::Invalid));
    assert!(!invalid.decision.value.nothing_to_select);
    post(&t.app, &decide(&at, 4, &invalid, DecideEdit::Todo))
        .await
        .unwrap();
    assert_eq!(
        read().await.decision.value.status,
        Some(DecisionStatus::Todo)
    );
}

/// `check_comment_saved_on_leaving_the_item`, the server's half: a comment saved with the
/// token of the item shown comes back with the item, a bookmark and the comment change nothing
/// of the decision, and an empty comment removes it. The UI's half, that leaving the item
/// sends it, is `tc_ui`'s `check_comment_saves_on_done_and_on_leaving`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_comment_and_a_bookmark_leave_the_decision_as_it_was() {
    let (_dir, t, _fs) = on_the_store().await;
    let at = open_on_store(&t).await;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let view = item(&t.app, &at, Tool::TranslationNotes, first).await;
    post(
        &t.app,
        &decide(&at, 5, &view, DecideEdit::Valid { words: vec![0] }),
    )
    .await
    .unwrap();
    let valid = item(&t.app, &at, Tool::TranslationNotes, first).await;
    let ack = post(
        &t.app,
        &decide(
            &at,
            6,
            &valid,
            DecideEdit::Comment {
                text: "ask Pablo".into(),
            },
        ),
    )
    .await
    .unwrap();
    assert_eq!(ack.saved[0].text, "ask Pablo");
    let commented = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert_eq!(
        commented.decision.value.comment.as_deref(),
        Some("ask Pablo")
    );
    assert_eq!(commented.decision.value.status, Some(DecisionStatus::Valid));
    assert_eq!(commented.target.selected, [0]);
    post(
        &t.app,
        &decide(&at, 7, &commented, DecideEdit::Bookmark { on: true }),
    )
    .await
    .unwrap();
    let marked = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert!(marked.decision.value.bookmarked);
    assert_eq!(marked.decision.value.comment.as_deref(), Some("ask Pablo"));
    post(
        &t.app,
        &decide(
            &at,
            8,
            &marked,
            DecideEdit::Comment {
                text: String::new(),
            },
        ),
    )
    .await
    .unwrap();
    let cleared = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert_eq!(cleared.decision.value.comment, None);
    assert!(cleared.decision.value.bookmarked);
    // A bookmark or a comment is not a decision: progress is what the one valid mark made.
    let after = session(&t.app, &at, Tool::TranslationNotes).await;
    assert_eq!(after.progress.decided, open.progress.decided + 1);
}

/// The verse changed after the detail showed it: the words the person tapped are not those
/// words any more, so the mark is refused and nothing is written on the wrong text.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn words_tapped_on_a_verse_that_changed_are_not_saved() {
    let (_dir, t, _fs) = on_the_store().await;
    let at = open_on_store(&t).await;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let view = item(&t.app, &at, Tool::TranslationNotes, first).await;
    let minted = t
        .app
        .sessions()
        .reader(&at)
        .unwrap()
        .read()
        .mint(&tit(1, 1), None)
        .unwrap();
    post(
        &t.app,
        &SaveVerse {
            at: at.clone(),
            request: rid(9),
            shown: tc_app::wire::token::shown(&minted).unwrap(),
            words: "Otras palabras del todo.".into(),
        },
    )
    .await
    .unwrap();
    let reply = t
        .app
        .request(&decide(
            &at,
            10,
            &view,
            DecideEdit::Valid { words: vec![0, 1] },
        ))
        .await
        .unwrap();
    assert!(
        matches!(reply, Reply::Err(ApiError::NotSaved(NotSaved::LookAgain))),
        "{reply:?}"
    );
    let now = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert_eq!(now.decision.value.status, None, "nothing was recorded");
}

/// A mark for a check the list no longer has at that place is `LookAgain`, not another check.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_mark_for_another_check_is_look_again() {
    let (_dir, t, _fs) = on_the_store().await;
    let at = open_on_store(&t).await;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let view = item(&t.app, &at, Tool::TranslationNotes, first).await;
    let mut mark = decide(&at, 11, &view, DecideEdit::Todo);
    mark.check_id = "zzzz".into();
    let reply = t.app.request(&mark).await.unwrap();
    assert!(
        matches!(reply, Reply::Err(ApiError::NotSaved(NotSaved::LookAgain))),
        "{reply:?}"
    );
}

// ---- marks, on scripted outcomes ----------------------------------------------------------

/// A disk that refuses the write: the mark says so, keeps nothing, and the item reads as it
/// did (AD6's scripted outcome).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_mark_the_disk_refuses_is_not_saved_and_changes_nothing() {
    let state = scenario("en-pins");
    let (backend, _) = MemoryBackend::from_scenario(state).unwrap();
    let backend = Arc::new(backend);
    let dir = tempfile::tempdir().unwrap();
    install_resources_into(&dir.path().join("app-data"), &state.resources().unwrap()).unwrap();
    let store = backend.clone();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |_, deps| {
        deps.store = StoreChoice::Given(store);
    })
    .await;
    let at = opened(&t.app).await.session.at;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let view = item(&t.app, &at, Tool::TranslationNotes, first).await;
    backend
        .script(
            &folder(SAMPLE),
            Scripted::NotWritten("the disk is full".into()),
        )
        .unwrap();
    let reply = t
        .app
        .request(&decide(
            &at,
            12,
            &view,
            DecideEdit::Comment {
                text: "kept in the editor".into(),
            },
        ))
        .await
        .unwrap();
    assert!(
        matches!(reply, Reply::Err(ApiError::NotSaved(NotSaved::Disk(_)))),
        "{reply:?}"
    );
    assert_eq!(item(&t.app, &at, Tool::TranslationNotes, first).await, view);
    // The write was scripted to succeed once: the same request goes through.
    backend.script(&folder(SAMPLE), Scripted::Written).unwrap();
    let ack = post(
        &t.app,
        &decide(
            &at,
            13,
            &view,
            DecideEdit::Comment {
                text: "kept in the editor".into(),
            },
        ),
    )
    .await
    .unwrap();
    assert_eq!(ack.stored, Stored::Written);
}

// ---- the selection a drag makes, held to the server's ---------------------------------------

proptest! {
    /// `drag_range` against `tc_check::selection::from_indices`: whatever words a press and a
    /// drag select, the server records exactly those words, whole, and reads the same indices
    /// back (#737).
    #[test]
    fn a_drag_selects_whole_words_and_mark_valid_saves_them(
        base in proptest::collection::btree_set(0u32..16, 0..4),
        from in 0u32..16,
        to in 0u32..16,
    ) {
        const VERSE: &str = "Pablo siervo de Dios y apóstol de Jesucristo, según la fe de los escogidos de Dios";
        let words = target_words(VERSE);
        prop_assert_eq!(words.len(), 16);
        let picked = drag_range(&base, from, to);
        let expected: BTreeSet<u32> = base
            .iter()
            .copied()
            .chain(from.min(to)..=from.max(to))
            .collect();
        prop_assert_eq!(&picked, &expected);
        let chosen: Vec<usize> = picked.iter().map(|&i| i as usize).collect();
        let text = TargetText::new(VERSE.to_owned());
        let selections = from_indices(&text, &chosen).unwrap();
        prop_assert_eq!(selections.len(), picked.len());
        let read: BTreeSet<u32> = indices(VERSE, &selections).into_iter().map(|i| i as u32).collect();
        prop_assert_eq!(read, picked);
    }
}
