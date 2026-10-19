//! The operation-count harness (PERFORMANCE rule 2, APP-DESIGN §8.1, AD32): an action runs
//! through a real `App` over the memory backend on Psalms and on the corpus ten times larger,
//! and fails when any count of the app's reads or of the fold's work grows.

use std::sync::Arc;

use tc_api::ids::{ProjectRef, Scope};
use tc_app::App;
use tc_app::backend::memory::{MemoryBackend, MemoryProject};
use tc_app::config::ProjectsFolder;
use tc_app::counters::AppCounts;
use tc_journal::fold::Counters;
use tempfile::TempDir;

use super::corpus::corpora;
use crate::common::{Parts, TestApp, folder, open_progress};

/// What an action cost: the app's counts and the project's fold work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cost {
    pub app: AppCounts,
    pub fold: Counters,
}

/// The fold's counters by name.
fn fold_fields(c: &Counters) -> [(&'static str, u64); 15] {
    [
        ("changes_indexed", c.changes_indexed),
        ("keys_updated", c.keys_updated),
        ("keys_recomputed", c.keys_recomputed),
        ("changes_visited", c.changes_visited),
        ("values_decoded", c.values_decoded),
        ("groups_built", c.groups_built),
        ("books_checked", c.books_checked),
        ("chapters_cohered", c.chapters_cohered),
        ("verses_cohered", c.verses_cohered),
        ("attached_placed", c.attached_placed),
        ("conflicts_built", c.conflicts_built),
        ("records_projected", c.records_projected),
        ("values_read", c.values_read),
        ("anomalies_touched", c.anomalies_touched),
        ("records_listed", c.records_listed),
    ]
}

/// A fresh app over a memory backend that holds one corpus project as `psalms`.
pub struct Scale {
    pub app: App,
    pub backend: MemoryBackend,
    pub folder: ProjectRef,
    _t: TestApp,
    _dir: TempDir,
}

impl Scale {
    /// Opens `psalms` for the screen.
    pub async fn open(&self) -> Scope {
        self.app
            .sessions()
            .open_screen(&self.folder, open_progress())
            .await
            .unwrap()
            .at
    }

    /// The screen session's scope, once [`Self::open`] ran.
    pub fn scope(&self) -> Scope {
        self.app.sessions().open_sessions()[0].at.clone()
    }

    fn fold_counters(&self) -> Counters {
        self.backend.fold(&self.folder).unwrap().counters()
    }
}

/// An app on temporary folders (never the person's own projects folder) over `project`,
/// which counts its own work.
pub async fn scale(project: &MemoryProject) -> Scale {
    let backend = MemoryBackend::new();
    let folder = folder("psalms");
    backend.add(folder.clone(), project.clone()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join("projects");
    std::fs::create_dir(&projects).unwrap();
    let store = Arc::new(backend.clone());
    let t = TestApp::start_with(dir.path(), &Parts::new(), |config, deps| {
        config.projects = ProjectsFolder::Given(projects.clone());
        deps.store = tc_app::StoreChoice::Given(store);
    })
    .await;
    Scale {
        app: t.app.clone(),
        backend,
        folder,
        _t: t,
        _dir: dir,
    }
}

/// What `action` cost on `scale`.
pub async fn cost(scale: &Scale, action: impl AsyncFnOnce(&Scale)) -> Cost {
    let app = scale.app.counters();
    let fold = scale.fold_counters();
    action(scale).await;
    Cost {
        app: scale.app.counters().since(&app),
        fold: scale.fold_counters().since(&fold),
    }
}

/// The fields that grew from Psalms to ten times Psalms, with both values; empty when none
/// did.
pub fn growth(small: &Cost, large: &Cost) -> Vec<String> {
    let app = large
        .app
        .exceeding(&small.app)
        .into_iter()
        .map(|(name, big, little)| {
            format!("app {name}: {little} on Psalms, {big} ten times larger")
        });
    let fold = fold_fields(&large.fold)
        .into_iter()
        .zip(fold_fields(&small.fold))
        .filter(|((_, big), (_, little))| big > little)
        .map(|((name, big), (_, little))| {
            format!("fold {name}: {little} on Psalms, {big} ten times larger")
        });
    app.chain(fold).collect()
}

/// The cost of `action` on Psalms and on the corpus ten times larger, each on a fresh app.
/// `before` runs first on each, uncounted: the open an action needs.
pub async fn costs_on_both(
    before: impl AsyncFn(&Scale),
    action: impl AsyncFn(&Scale),
) -> (Cost, Cost) {
    let corpora = corpora();
    let mut out = Vec::new();
    for project in [&corpora.psalms, &corpora.larger] {
        let scale = scale(project).await;
        before(&scale).await;
        out.push(cost(&scale, async |s| action(s).await).await);
    }
    (out[0], out[1])
}

/// Runs `action` on both corpora and fails, naming every count that grew.
pub async fn costs_what_it_touches(
    name: &str,
    before: impl AsyncFn(&Scale),
    action: impl AsyncFn(&Scale),
) -> (Cost, Cost) {
    let (small, large) = costs_on_both(before, action).await;
    let grew = growth(&small, &large);
    assert!(
        grew.is_empty(),
        "{name} grows with the project:\n{}",
        grew.join("\n")
    );
    (small, large)
}

/// Nothing before the action.
pub async fn nothing(_: &Scale) {}

/// The screen session open before the action.
pub async fn opened(scale: &Scale) {
    scale.open().await;
}
