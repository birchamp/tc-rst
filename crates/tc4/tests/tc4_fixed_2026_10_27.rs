//! tC4 issues fixed upstream in October 2026, replayed on the rebuild (report 2026-10-27, area
//! helps-check): #769, folding the Check sidebar's groups, through the app the `tc4` program
//! serves, restarted on the same app data.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tc_api::app::BuildInfo;
use tc_api::check::{CheckListRef, CheckSessionView, CheckUnit, Open, RailRows, Tool};
use tc_api::ids::{ProjectRef, RequestId, Scope};
use tc_api::jobs::{JobOutcome, JobState};
use tc_api::model::rail::{
    Filter, GroupLabel, SortMode, fold_all, folds, group_id, groups, open_groups, term_progress,
};
use tc_api::prefs::SetRailFolded;
use tc_api::session::{Open as OpenProject, OpenUnit};
use tc_api::{Endpoint, Reply};
use tc_app::account::MemKeychain;
use tc_app::backend::memory::MemoryBackend;
use tc_app::backend::scenario::{ScenarioState, install_resources_into};
use tc_app::clock::ManualClock;
use tc_app::config::{AppConfig, HostEnv, HostOs, ProjectsFolder};
use tc_app::folders::HostFolders;
use tc_app::hooks::RecordingEnder;
use tc_app::prefs::MemoryFiles;
use tc_app::{App, AppDeps, FilesChoice, StoreChoice};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// An app over `dir`, its projects in `store` and its app-data files in `files`, as `tc4
/// serve --scenario` starts it.
async fn start_app(dir: &Path, store: Arc<MemoryBackend>, files: Arc<MemoryFiles>) -> App {
    let (home, local) = (dir.join("home"), dir.join("local"));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&local).unwrap();
    let host = HostEnv::new(
        HostOs::Linux,
        [
            ("HOME", &home),
            ("USERPROFILE", &home),
            ("XDG_DATA_HOME", &local),
        ]
        .map(|(k, v)| (k.to_owned(), OsString::from(v))),
    );
    let keychain = Arc::new(MemKeychain::new());
    let config = AppConfig {
        app_data: dir.join("app-data"),
        projects: ProjectsFolder::Default,
        door43: "http://127.0.0.1:9".parse().unwrap(),
        build: BuildInfo {
            version: "4.0.0-test".to_owned(),
            commit: "0123abc".to_owned(),
        },
        bundle: None,
        test_hooks: true,
        seed_pace: None,
    };
    let deps = AppDeps {
        clock: Arc::new(ManualClock::new(1_000_000)),
        keychain: keychain.clone(),
        folders: Arc::new(HostFolders::new(host.clone())),
        host,
        files: FilesChoice::Given(files),
        store: StoreChoice::Given(store),
        sync: Arc::new(tc_app::ports::scripted::ScriptedSyncPort::default()),
        exchange: Arc::new(tc_app::ports::scripted::ScriptedExchangePort::default()),
        import: Arc::new(tc_app::ports::UnavailableImport),
        ender: Arc::new(RecordingEnder::default()),
        test_keychain: Some(keychain),
    };
    App::start(config, deps).await.unwrap()
}

async fn ok<E: Endpoint>(app: &App, req: &E) -> E::Ok {
    match app.request(req).await.unwrap() {
        Reply::Ok(ok) => ok,
        Reply::Err(e) => panic!("{}: {e:?}", E::PATH),
    }
}

fn rid(n: u8) -> RequestId {
    RequestId::from_bytes([n; 16])
}

/// Opens the sample Bible project at Titus, then its Translation Notes tool.
async fn open_titus_notes(app: &App) -> (Scope, CheckSessionView) {
    let job = ok(
        app,
        &OpenProject {
            request: rid(40),
            project: ProjectRef::parse("sample_burrito").unwrap(),
            unit: Some(OpenUnit::Book("TIT".parse().unwrap())),
        },
    )
    .await;
    let mut watch = app.jobs().watch(job).unwrap();
    let state = tokio::time::timeout(Duration::from_secs(60), async {
        watch
            .wait_for(tc_app::jobs::JobRegistry::is_ended)
            .await
            .unwrap()
            .clone()
    })
    .await
    .expect("the open ends");
    let JobState::Done(JobOutcome::Opened(opened)) = state else {
        panic!("{state:?}");
    };
    let at = opened.session.at;
    let session = ok(
        app,
        &Open {
            at: at.clone(),
            list: CheckListRef {
                tool: Tool::TranslationNotes,
                unit: CheckUnit::Book("TIT".parse().unwrap()),
            },
            since: None,
        },
    )
    .await;
    (at, session)
}

/// tC4 #769 "Let a checker fold and unfold the groups in the Check sidebar".
///
/// Steps: open Check → Translation Notes for Titus (the en-pins scenario: en_tn and en_ta),
/// sort By figure: one group per Translation Academy module, headed by the module's title
/// ("Abstract Nouns") with its resolved/total count. Fold a group: its rows hide, but the
/// group holding the active check stays open. Fold all folds every other group, then reads
/// Unfold all. By verse is one group with no fold control. Restart the app and open the tool
/// again: the group is still folded.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc4_769_by_figure_groups_fold_and_the_fold_survives_a_restart() {
    let state = ScenarioState::from_repository(&repo_root(), "en-pins").unwrap();
    let (backend, _) = MemoryBackend::from_scenario(&state).unwrap();
    let backend = Arc::new(backend);
    let files = Arc::new(MemoryFiles::new());
    let dir = tempfile::tempdir().unwrap();
    install_resources_into(&dir.path().join("app-data"), &state.resources().unwrap()).unwrap();

    let app = start_app(dir.path(), backend.clone(), files.clone()).await;
    let (at, session) = open_titus_notes(&app).await;
    let RailRows::Full(rows) = &session.rows else {
        panic!("{:?}", session.rows);
    };
    let terms = &session.terms;
    assert!(session.folded.is_empty());

    // Notes sort By verse or By figure; only By figure folds, and By verse is one group.
    assert_eq!(
        SortMode::modes(Tool::TranslationNotes),
        [SortMode::ByVerse, SortMode::ByFigure]
    );
    assert!(folds(SortMode::ByFigure) && !folds(SortMode::ByVerse));
    let by_verse = groups(rows, terms.len(), Filter::All, SortMode::ByVerse);
    assert_eq!(by_verse.len(), 1);
    assert_eq!(group_id(by_verse[0].label, terms), None, "no fold control");

    // One group per tA module, headed by the module's title, with resolved/total.
    let figures = groups(rows, terms.len(), Filter::All, SortMode::ByFigure);
    let slugs: BTreeSet<&str> = rows
        .iter()
        .filter_map(|r| terms.get(r.term? as usize).map(String::as_str))
        .collect();
    assert!(figures.len() > 1);
    assert_eq!(figures.len(), slugs.len());
    assert!(
        figures
            .iter()
            .all(|g| matches!(g.label, GroupLabel::Figure(_)))
    );
    let abstract_nouns = terms
        .iter()
        .position(|t| t == "figs-abstractnouns")
        .unwrap();
    assert_eq!(session.group_titles[abstract_nouns], "Abstract Nouns");
    let group = figures
        .iter()
        .find(|g| g.label == GroupLabel::Figure(u32::try_from(abstract_nouns).unwrap()))
        .unwrap();
    let decided = group
        .rows
        .iter()
        .filter(|&&r| rows[r].flags.decided)
        .count();
    assert_eq!(
        term_progress(rows, terms.len())[abstract_nouns],
        (
            u32::try_from(decided).unwrap(),
            u32::try_from(group.rows.len()).unwrap()
        )
    );

    // Fold another group and the active one: only the other closes.
    let active_row = rows.iter().find(|r| r.index == session.start).unwrap();
    let active = terms[active_row.term.unwrap() as usize].clone();
    let other = figures
        .iter()
        .filter_map(|g| group_id(g.label, terms))
        .find(|id| *id != active)
        .unwrap()
        .to_owned();
    let both: BTreeSet<String> = [other.clone(), active.clone()].into();
    let open = open_groups(&figures, terms, &both, Some(&active), SortMode::ByFigure);
    for (g, open) in figures.iter().zip(open) {
        let id = group_id(g.label, terms).unwrap();
        assert_eq!(open, id != other, "{id}");
    }
    // Fold all leaves the active group open; then the control reads Unfold all.
    let (ids, fold) = fold_all(&figures, terms, &BTreeSet::new(), Some(&active));
    assert!(fold && !ids.contains(&active) && ids.len() == figures.len() - 1);
    let all: BTreeSet<String> = ids.into_iter().collect();
    let (ids, fold) = fold_all(&figures, terms, &all, Some(&active));
    assert!(!fold && ids.len() == figures.len());

    // The fold is kept on this computer: after a restart the tool opens with it folded.
    ok(
        &app,
        &SetRailFolded {
            at,
            request: rid(41),
            tool: Tool::TranslationNotes,
            groups: vec![other.clone()],
            folded: true,
        },
    )
    .await;
    // Quit as the app does: close the project, write what is pending.
    app.quit().await;
    drop(app);
    let app = start_app(dir.path(), backend, files).await;
    let (_, again) = open_titus_notes(&app).await;
    assert_eq!(again.folded, [other]);
}
