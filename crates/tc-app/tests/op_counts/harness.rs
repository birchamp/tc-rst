//! The operation-count harness (PERFORMANCE rule 2, APP-DESIGN §8.1, AD32): an action runs
//! through a real `App` on Psalms and on the corpus ten times larger, over the memory backend
//! and over the store (from C1a on), and fails when any count of the app's reads, of the fold's
//! work or of the store's file operations grows.

use std::sync::Arc;

use tc_api::ids::{ProjectRef, Scope};
use tc_app::App;
use tc_app::backend::memory::{MemoryBackend, MemoryProject};
use tc_app::backend::{ProjectStore, StoreCounters};
use tc_app::config::ProjectsFolder;
use tc_app::counters::AppCounts;
use tc_journal::fold::Counters;
use tc_store::FakePlatform;
use tempfile::TempDir;

use super::corpus::{FOLDER, corpora, store_corpora};
use crate::common::{Parts, TestApp, folder, open_progress};
use crate::store_world::{APP, backend_over};

/// What holds the corpus project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backing {
    Memory,
    Store,
}

/// What an action cost: the app's counts, the project's fold work and, on the store, its file
/// operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cost {
    pub app: AppCounts,
    pub fold: Counters,
    pub store: StoreCounters,
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

/// The store's counters by name, its queue's wait times left out: they measure the machine.
fn store_fields(c: &StoreCounters) -> [(&'static str, u64); 18] {
    [
        ("files_opened", c.files_opened),
        ("files_created", c.files_created),
        ("bytes_read", c.bytes_read),
        ("bytes_written", c.bytes_written),
        ("fsyncs", c.fsyncs),
        ("renames", c.renames),
        ("removes", c.removes),
        ("listings", c.listings),
        ("stats", c.stats),
        ("derived_read", c.derived_read),
        ("check_read", c.check_read),
        ("projected", c.projected),
        ("derived_written", c.derived_written),
        ("heads_read", c.heads_read),
        ("journal_files_read", c.journal_files_read),
        ("journal_files_cached", c.journal_files_cached),
        ("commands", c.commands),
        ("drafts_written", c.drafts_written),
    ]
}

/// A fresh app over one corpus project as `psalms`.
pub struct Scale {
    pub app: App,
    pub backing: Backing,
    /// The memory backend, when the project is held in memory.
    memory: Option<MemoryBackend>,
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

    /// The app's data folder, where the machine's resources are installed.
    pub fn app_data(&self) -> std::path::PathBuf {
        self._t.app_data.clone()
    }

    /// The screen session's scope, once [`Self::open`] ran.
    pub fn scope(&self) -> Scope {
        self.app.sessions().open_sessions()[0].at.clone()
    }

    /// The open session's handle on the store; `None` before the open.
    fn store_handle(&self) -> Option<Arc<dyn tc_app::backend::ProjectHandle>> {
        let open = self.app.sessions().open_sessions();
        let at = &open.first()?.at;
        Some(self.app.read(at).ok()?.handle().clone())
    }

    /// Waits until the store's queue and background work are done, so the open's storage
    /// check never counts into an action (the fold's counters are shared with it).
    fn settle(&self) {
        if let Some(handle) = self.store_handle() {
            tokio::task::block_in_place(|| handle.wait_idle());
        }
    }

    fn fold_counters(&self) -> Counters {
        match &self.memory {
            Some(backend) => backend.fold(&self.folder).unwrap().counters(),
            None => self
                .store_handle()
                .map(|h| h.snapshot().fold().counters())
                .unwrap_or_default(),
        }
    }

    fn store_counters(&self) -> StoreCounters {
        self.store_handle()
            .filter(|_| self.backing == Backing::Store)
            .map(|h| h.counters())
            .unwrap_or_default()
    }
}

/// An app on temporary folders (never the person's own projects folder) over the corpus
/// `project` (memory) or a copy of `on_disk` (store), which counts its own work.
pub async fn scale(
    backing: Backing,
    project: &MemoryProject,
    on_disk: &tc_store::fault::FaultFs,
) -> Scale {
    let folder = folder(FOLDER);
    let (store, memory): (Arc<dyn ProjectStore>, _) = match backing {
        Backing::Memory => {
            let backend = MemoryBackend::new();
            backend.add(folder.clone(), project.clone()).unwrap();
            (Arc::new(backend.clone()), Some(backend))
        }
        Backing::Store => {
            let fs = on_disk.copy();
            let backend = backend_over(&fs, APP, FakePlatform::default(), Arc::default());
            (Arc::new(backend), None)
        }
    };
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join("projects");
    std::fs::create_dir(&projects).unwrap();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |config, deps| {
        config.projects = ProjectsFolder::Given(projects.clone());
        deps.store = tc_app::StoreChoice::Given(store);
    })
    .await;
    Scale {
        app: t.app.clone(),
        backing,
        memory,
        folder,
        _t: t,
        _dir: dir,
    }
}

/// What `action` cost on `scale`.
pub async fn cost(scale: &Scale, action: impl AsyncFnOnce(&Scale)) -> Cost {
    scale.settle();
    let app = scale.app.counters();
    let fold = scale.fold_counters();
    let store = scale.store_counters();
    action(scale).await;
    scale.settle();
    Cost {
        app: scale.app.counters().since(&app),
        fold: scale.fold_counters().since(&fold),
        store: scale.store_counters().since(&store),
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
    let grew =
        |kind: &'static str, big: Vec<(&'static str, u64)>, little: Vec<(&'static str, u64)>| {
            big.into_iter()
                .zip(little)
                .filter(|((_, big), (_, little))| big > little)
                .map(move |((name, big), (_, little))| {
                    format!("{kind} {name}: {little} on Psalms, {big} ten times larger")
                })
                .collect::<Vec<_>>()
        };
    let fold = grew(
        "fold",
        fold_fields(&large.fold).to_vec(),
        fold_fields(&small.fold).to_vec(),
    );
    let store = grew(
        "store",
        store_fields(&large.store).to_vec(),
        store_fields(&small.store).to_vec(),
    );
    app.chain(fold).chain(store).collect()
}

/// The cost of `action` on Psalms and on the corpus ten times larger over `backing`, each on a
/// fresh app. `before` runs first on each, uncounted: the open an action needs.
pub async fn costs_on_both(
    backing: Backing,
    before: impl AsyncFn(&Scale),
    action: impl AsyncFn(&Scale),
) -> (Cost, Cost) {
    let (memory, disk) = (corpora(), store_corpora());
    let mut out = Vec::new();
    for (project, on_disk) in [
        (&memory.psalms, &disk.psalms),
        (&memory.larger, &disk.larger),
    ] {
        let scale = scale(backing, project, on_disk).await;
        before(&scale).await;
        out.push(cost(&scale, async |s| action(s).await).await);
    }
    (out[0], out[1])
}

/// What a row holds to on the store: everything, or (for an action that opens the project,
/// whose fold and journal reads are the store's own budget, tc-store's `tests/store/opcount.rs`)
/// the app's counts only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnStore {
    Everything,
    AppCounts,
}

/// Runs `action` on both corpora over both backends and fails, naming every count that grew
/// and the backend. Answers each backend's costs: (memory, store).
pub async fn costs_what_it_touches(
    name: &str,
    on_store: OnStore,
    before: impl AsyncFn(&Scale),
    action: impl AsyncFn(&Scale),
) -> ((Cost, Cost), (Cost, Cost)) {
    let mut both = Vec::new();
    for backing in [Backing::Memory, Backing::Store] {
        let (small, large) = costs_on_both(backing, &before, &action).await;
        let mut grew = growth(&small, &large);
        if backing == Backing::Store && on_store == OnStore::AppCounts {
            grew.retain(|g| g.starts_with("app "));
        }
        assert!(
            grew.is_empty(),
            "{name} grows with the project on {backing:?}:\n{}",
            grew.join("\n")
        );
        both.push((small, large));
    }
    (both[0], both[1])
}

/// Nothing before the action.
pub async fn nothing(_: &Scale) {}

/// The screen session open before the action.
pub async fn opened(scale: &Scale) {
    scale.open().await;
}
