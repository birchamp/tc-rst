//! The count rows of B2 and of C1a's D7: Home reads each project's derived files once, off the
//! request, never walks a project's history, and with its summaries kept a launch's Home costs
//! the projects, not their history (§8.2).

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tc_api::Reply;
use tc_api::home::{Card, List};
use tc_api::shown::Known;
use tc_api::test::{Quiet, QuietView};
use tc_app::App;
use tc_app::config::ProjectsFolder;
use tc_app::prefs::AppDataPath;
use tc_store::FakePlatform;

use super::corpus::{FOLDER, corpora, store_corpora};
use super::harness::{Backing, OnStore, Scale, cost, costs_what_it_touches, scale};
use crate::common::{Parts, TestApp, folder};
use crate::store_world::{APP, PROJECTS, backend_over, files};

async fn list(app: &App) -> tc_api::home::HomeView {
    let Reply::Ok(view) = app.request(&List {}).await.unwrap() else {
        panic!("home/list")
    };
    view
}

/// One Home visit: the list, then each card's own read, as the cards do when they mount.
async fn visit_app(app: &App) {
    let view = list(app).await;
    for card in view.cards {
        let Reply::Ok(_) = app
            .request(&Card {
                project: card.project,
            })
            .await
            .unwrap()
        else {
            panic!("home/card")
        };
    }
    settle(app).await;
}

async fn visit(s: &Scale) {
    visit_app(&s.app).await;
}

/// Until the summary jobs the visit queued are done.
async fn settle(app: &App) {
    for _ in 0..2_000 {
        let Reply::Ok(QuietView { busy }) = app.request(&Quiet {}).await.unwrap() else {
            panic!("test/quiet")
        };
        if !busy {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("the app did not settle");
}

/// A second Home visit renders nothing: its summaries are kept under their stamps.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_home_visit_renders_nothing() {
    let (memory, store) =
        costs_what_it_touches("a second Home visit", OnStore::Everything, visit, visit).await;
    for cost in [memory.0, memory.1, store.0, store.1] {
        let app = cost.app;
        assert_eq!(
            (
                app.book_usfm,
                app.books_rendered,
                app.files_rendered,
                app.conflict_walks,
                app.keys_probed,
                app.summaries_computed
            ),
            (0, 0, 0, 0, 0, 0),
            "{cost:?}"
        );
        assert_eq!(cost.fold.changes_visited, 0, "{cost:?}");
        assert_eq!(cost.fold.records_listed, 0, "{cost:?}");
    }
}

/// The first visit computes one summary, in the background, and walks no history at any size.
/// On the memory backend it renders each book once, so its book count grows with the books of
/// a project by design; the store reads the files it wrote and renders nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn home_walks_no_history() {
    let (memory, disk) = (corpora(), store_corpora());
    for backing in [Backing::Memory, Backing::Store] {
        for (project, on_disk) in [
            (&memory.psalms, &disk.psalms),
            (&memory.larger, &disk.larger),
        ] {
            let s = scale(backing, project, on_disk).await;
            let cost = cost(&s, visit).await;
            let Reply::Ok(card) = s
                .app
                .request(&Card {
                    project: s.folder.clone(),
                })
                .await
                .unwrap()
            else {
                panic!("home/card")
            };
            let Known::Known(books) = card.books else {
                panic!("books unknown")
            };
            assert!(books.iter().all(|b| matches!(b.drafted, Known::Known(_))));
            let rendered = match backing {
                Backing::Memory => books.len() as u64,
                Backing::Store => 0,
            };
            assert_eq!(cost.app.book_usfm, rendered, "{backing:?}: {cost:?}");
            assert_eq!(cost.app.summaries_computed, 1, "{cost:?}");
            assert_eq!(cost.fold.changes_visited, 0, "{cost:?}");
            assert_eq!(cost.fold.records_listed, 0, "{cost:?}");
        }
    }
}

/// An app on `fs`'s projects folder, with app data `parts` keeps across launches.
async fn launch(dir: &Path, parts: &Parts, fs: &tc_store::fault::FaultFs) -> TestApp {
    let backend = Arc::new(backend_over(
        fs,
        APP,
        FakePlatform::default(),
        Arc::default(),
    ));
    let projects = dir.join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    TestApp::start_with(dir, parts, |config, deps| {
        config.projects = ProjectsFolder::Given(projects.clone());
        deps.store = tc_app::StoreChoice::Given(backend);
    })
    .await
}

/// §8.2's Home row on the store (C1a's D7, A-25): with its summaries kept, a launch's Home of
/// ten cards opens no project, reads no pack and no pending save, opens at most five files per
/// card, and costs the same whether one card is Psalms or the corpus ten times larger. A card
/// whose kept summary is gone starts exactly one summary, which the next launch reads.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn home_costs_projects_not_history() {
    let disk = store_corpora();
    let small = files(&disk.psalms, &Path::new(PROJECTS).join(FOLDER));
    let mut opened_per_corpus = Vec::new();
    for on_disk in [&disk.psalms, &disk.larger] {
        // Ten cards: the corpus project and nine small ones (copies of Psalms).
        let fs = on_disk.copy();
        for i in 1..10 {
            for (rel, bytes) in &small {
                fs.put(
                    &Path::new(PROJECTS).join(format!("small_{i}")).join(rel),
                    bytes,
                    None,
                );
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let parts = Parts::new();
        let first = launch(dir.path(), &parts, &fs).await;
        visit_app(&first.app).await;
        assert_eq!(first.app.counters().summaries_computed, 10);
        first.app.quit().await;
        drop(first);

        // The next launch, with the summaries app data kept.
        let next_dir = tempfile::tempdir().unwrap();
        let next = launch(next_dir.path(), &parts, &fs).await;
        fs.clear_log();
        visit_app(&next.app).await;
        let counts = next.app.counters();
        assert_eq!(counts.summaries_computed, 0, "fresh summaries start no job");
        let opened = fs.opened();
        let history: Vec<_> = opened
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .filter(|p| p.contains("/ingredients/tc/changes/") || p.contains("/.tc/pending/"))
            .collect();
        assert!(history.is_empty(), "Home read history: {history:?}");
        assert!(
            opened.len() <= 5 * 10,
            "{} files opened for ten cards: {opened:?}",
            opened.len()
        );
        assert!(
            next.app.sessions().open_sessions().is_empty(),
            "no project is opened"
        );
        opened_per_corpus.push(opened.len());

        next.app.quit().await;
        drop(next);

        // A card whose kept summary is gone starts exactly one job, and the next launch
        // reads the new one.
        tc_app::prefs::AppDataFiles::remove(&*parts.files, &AppDataPath::summary(&folder(FOLDER)))
            .unwrap();
        for (expected, launch_dir) in [
            (1, tempfile::tempdir().unwrap()),
            (0, tempfile::tempdir().unwrap()),
        ] {
            let again = launch(launch_dir.path(), &parts, &fs).await;
            visit_app(&again.app).await;
            assert_eq!(again.app.counters().summaries_computed, expected);
            again.app.quit().await;
        }
    }
    assert_eq!(
        opened_per_corpus[0], opened_per_corpus[1],
        "Home opens as many files with the Psalms card as with the ten-times-larger one"
    );
}
