//! A story's checks are written as a book's are (C4, B7 D7, J22): `check/item`, `check/decide`
//! and `check/article` over a story's list, on the real store with the en-pins scenario's OBS
//! sample.

use std::path::Path;
use std::sync::Arc;

use tc_api::check::{
    CheckArticleRead, CheckItemView, CheckListRef, CheckSessionView, CheckSources, CheckUnit,
    Decide, DecideEdit, DecisionStatus, Item, Open, RailRow, RailRows, Tool,
};
use tc_api::ids::{Scope, StoryNo};
use tc_app::StoreChoice;
use tc_app::backend::scenario::install_resources_into;
use tc_app::backend::scenario_store::ScenarioStore;
use tc_app::config::ProjectsFolder;
use tc_journal::RecordKey;
use tc_journal::ids::FrameNo;
use tc_store::FakePlatform;
use tc_store::fault::FaultFs;

use super::common::{Parts, TestApp, folder, open_progress, post, rid};
use super::scenario::scenario;
use super::understand::ok;
use crate::store_world::{APP, PROJECTS, assert_valid_burrito, backend_over, files};

const OBS: &str = "sample_burrito_obs";

fn list(tool: Tool) -> CheckListRef {
    CheckListRef {
        tool,
        unit: CheckUnit::Story(StoryNo::new(1).unwrap()),
    }
}

/// The en-pins scenario on the real store, with the store kept for a teammate's edits.
async fn on_the_store() -> (tempfile::TempDir, TestApp, FaultFs, Arc<ScenarioStore>) {
    let fs = FaultFs::new();
    let backend = backend_over(&fs, APP, FakePlatform::default(), Arc::default());
    let (store, _) = ScenarioStore::write(backend, scenario("en-pins")).unwrap();
    let store = Arc::new(store);
    let dir = tempfile::tempdir().unwrap();
    install_resources_into(
        &dir.path().join("app-data"),
        &scenario("en-pins").resources().unwrap(),
    )
    .unwrap();
    let projects = dir.path().join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    let given = store.clone();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |config, deps| {
        config.projects = ProjectsFolder::Given(projects.clone());
        deps.store = StoreChoice::Given(given);
    })
    .await;
    (dir, t, fs, store)
}

async fn open_obs(t: &TestApp) -> Scope {
    t.app
        .sessions()
        .open_screen(&folder(OBS), open_progress())
        .await
        .unwrap()
        .at
}

/// Leaves the story project and waits until no session holds it: a side user (a background
/// read) keeps it open after the screen leaves, and its close lets go of the project's lock,
/// which a teammate's edit through the store needs.
async fn leave(t: &TestApp, at: &Scope) {
    t.app.sessions().leave_screen(at).unwrap();
    // A left screen's session closes after its checkpoint; the store opens the folder next in
    // these tests, so wait for the close itself, not only for the screen to go.
    t.app.sessions().close_all().await;
}

async fn rail(t: &TestApp, at: &Scope, tool: Tool) -> Vec<RailRow> {
    let view: CheckSessionView = ok(
        &t.app,
        &Open {
            at: at.clone(),
            list: list(tool),
            since: None,
        },
    )
    .await;
    match view.rows {
        RailRows::Full(rows) => rows,
        other => panic!("{other:?}"),
    }
}

async fn item(t: &TestApp, at: &Scope, tool: Tool, index: u32) -> CheckItemView {
    ok(
        &t.app,
        &Item {
            at: at.clone(),
            list: list(tool),
            index,
        },
    )
    .await
}

fn decide(at: &Scope, request: u8, tool: Tool, view: &CheckItemView, edit: DecideEdit) -> Decide {
    Decide {
        at: at.clone(),
        request: rid(request),
        list: list(tool),
        place: view.place.clone(),
        check_id: view.check_id.clone(),
        decision: view.decision.token.clone(),
        verse: view.target.verse.clone(),
        edit,
    }
}

/// The first check of frame 1 on `tool`'s rail.
fn frame_one(rows: &[RailRow]) -> u32 {
    rows.iter()
        .find(|row| row.place.to_string() == "OBS 1:1")
        .expect("a check of frame 1")
        .index
}

/// The project's files under its folder, by path in the folder.
fn project_files(fs: &FaultFs) -> std::collections::BTreeMap<String, Vec<u8>> {
    files(fs, &Path::new(PROJECTS).join(OBS))
}

/// J22 (tC4 `e2e/j22-obs-check.spec.ts`): a note and a word link of frame 1, each marked valid
/// with one tapped word, are stored under the story and the frame with that word, judged
/// against the frame's text; the rail and the item follow; the story file is the same bytes
/// after the checkpoint; and the project stays a valid Burrito.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_story_mark_is_stored_under_its_frame_with_the_tapped_word() {
    let (_dir, t, fs, _store) = on_the_store().await;
    let at = open_obs(&t).await;
    let before = project_files(&fs);
    let story_file = before
        .keys()
        .find(|path| path.ends_with("01.md"))
        .expect("story 1's file")
        .clone();
    for (n, tool) in [Tool::TranslationNotes, Tool::TranslationWords]
        .into_iter()
        .enumerate()
    {
        let index = frame_one(&rail(&t, &at, tool).await);
        let view = item(&t, &at, tool, index).await;
        assert!(matches!(view.sources, CheckSources::Story { .. }));
        let word = view
            .target
            .words
            .iter()
            .position(|w| w == "principio")
            .expect("the frame's words") as u32;
        let ack = post(
            &t.app,
            &decide(
                &at,
                10 + n as u8,
                tool,
                &view,
                DecideEdit::Valid { words: vec![word] },
            ),
        )
        .await
        .unwrap();
        assert_eq!(ack.rows.len(), 1, "the check's row");
        assert!(ack.rows[0].row.flags.decided);
        let marked = item(&t, &at, tool, index).await;
        assert_eq!(marked.decision.value.status, Some(DecisionStatus::Valid));
        assert_eq!(marked.target.selected, [word]);
        assert!(
            !marked.decision.value.invalidated,
            "judged against the frame's own text"
        );
        let row = &rail(&t, &at, tool).await[index as usize];
        assert!(row.flags.decided && !row.flags.invalidated);
    }
    // Each decision was judged against the frame's text, so the frame shows no edits since.
    {
        let session = t.app.read(&at).unwrap();
        let read = session.read();
        for tool in [
            tc_check::ToolId::TranslationNotes,
            tc_check::ToolId::TranslationWords,
        ] {
            let decisions = read.check_decisions(tool, tc_check::Collection::Stories);
            assert_eq!(decisions.len(), 1);
            assert!(!decisions[0].assessment.verse_edits, "{tool:?}");
        }
    }
    leave(&t, &at).await;
    let after = project_files(&fs);
    assert_eq!(
        after.get(&story_file),
        before.get(&story_file),
        "checking wrote nothing into the story"
    );
    for tool in ["translationNotes", "translationWords"] {
        let path = format!("ingredients/checking/{tool}/OBS.json");
        let file: serde_json::Value =
            serde_json::from_slice(after.get(&path).unwrap_or_else(|| panic!("{path}"))).unwrap();
        let decisions = file["decisions"].as_array().expect("decisions");
        assert_eq!(decisions.len(), 1, "{tool}: {file}");
        let decision = &decisions[0];
        assert_eq!(
            decision["contextId"]["reference"],
            serde_json::json!({ "story": 1, "frame": 1 })
        );
        assert_eq!(decision["status"], "valid");
        assert_eq!(
            decision["selections"],
            serde_json::json!([{ "text": "principio", "occurrence": 1, "occurrences": 1 }])
        );
    }
    assert_valid_burrito(&fs, &Path::new(PROJECTS).join(OBS));
}

/// D36: a teammate's edit of frame 1 that drops the word a decision selected flags the
/// decision invalidated at the next open, and keeps it with its words.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_frame_edit_that_drops_the_tapped_word_invalidates_the_mark_and_keeps_it() {
    let (_dir, t, _fs, store) = on_the_store().await;
    let at = open_obs(&t).await;
    let index = frame_one(&rail(&t, &at, Tool::TranslationNotes).await);
    let view = item(&t, &at, Tool::TranslationNotes, index).await;
    let word = view
        .target
        .words
        .iter()
        .position(|w| w == "principio")
        .unwrap() as u32;
    post(
        &t.app,
        &decide(
            &at,
            20,
            Tool::TranslationNotes,
            &view,
            DecideEdit::Valid { words: vec![word] },
        ),
    )
    .await
    .unwrap();
    leave(&t, &at).await;
    let frame = RecordKey::Frame(tc_journal::StoryNo::new(1).unwrap(), FrameNo::new(1));
    store
        .teammates_edit_verse(
            &folder(OBS),
            &frame,
            &["Así fue como el Señor hizo todo al comienzo."],
        )
        .unwrap();
    let at = open_obs(&t).await;
    let rows = rail(&t, &at, Tool::TranslationNotes).await;
    assert!(
        rows[index as usize].flags.invalidated,
        "{:?}",
        rows[index as usize]
    );
    let view = item(&t, &at, Tool::TranslationNotes, index).await;
    assert!(view.decision.value.invalidated);
    assert!(
        view.notices
            .iter()
            .any(|n| matches!(n, tc_api::check::CheckNotice::Invalidated { .. })),
        "{:?}",
        view.notices
    );
    let session = t.app.read(&at).unwrap();
    let read = session.read();
    let kept = read.check_decisions(
        tc_check::ToolId::TranslationNotes,
        tc_check::Collection::Stories,
    );
    assert_eq!(kept.len(), 1, "the decision is kept");
    assert_eq!(
        kept[0].record.judgement.selections.as_ref().map(Vec::len),
        Some(1),
        "with its word"
    );
}

/// A story check's Learn more reads its article from the story's helps.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_story_checks_article_is_read_from_the_story_helps() {
    let (_dir, t, _fs, _store) = on_the_store().await;
    let at = open_obs(&t).await;
    let view = item(&t, &at, Tool::TranslationWords, 0).await;
    assert_eq!(view.article.title.as_deref(), Some("God"));
    let article: tc_api::helps::ArticleView = ok(
        &t.app,
        &CheckArticleRead {
            at: at.clone(),
            list: list(Tool::TranslationWords),
            index: 0,
        },
    )
    .await;
    assert!(
        matches!(&article, tc_api::helps::ArticleView::Found { title, .. } if title == "God"),
        "{article:?}"
    );
}
