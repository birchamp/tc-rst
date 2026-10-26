//! The scripted ports of the tests and journeys: the outcomes a test set, in place of Door43
//! and of the importer. They send nothing; a scripted sync writes its teammates' changes
//! through the app.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tc_api::door43::{ProjectName, RepoName, RepoSource, ShareOwner, SharedRepo};
use tc_api::error::{ApiError, Feature};
use tc_api::ids::ProjectRef;
use tc_api::import::{ImportDetails, ImportFound, UpdateReport, VersionSlot, VersionsView};
use tc_api::jobs::{Phase, SharePhase, SyncResultView};
use tc_core::BookId;
use tc_import::SourceFile;

use super::{ImportPort, SyncPort};
use crate::app::{App, lock};
use crate::backend::BoxFuture;
use crate::jobs::{JobCtx, JobEnd, ProgressSink};
use crate::session::ProjectSession;

#[derive(Default)]
pub struct ScriptedSyncPort {
    /// How the next sync of each project ends: each setting replaces the one before, as a
    /// change on Door43 changes what the next sync finds.
    syncs: Mutex<HashMap<ProjectRef, tc_api::test::ScriptedSync>>,
    /// The outcomes of the next opens, in order; `Unavailable(Sync)` when none is left.
    opens: Mutex<VecDeque<Result<ProjectRef, ApiError>>>,
    /// The outcomes of the next shares, in order; `Unavailable(Sync)` when none is left.
    shares: Mutex<VecDeque<Result<SharedRepo, ApiError>>>,
    calls: AtomicUsize,
    /// The next sync waits for [`ScriptedSyncPort::release`] before it ends.
    hold: AtomicBool,
    released: Arc<tokio::sync::Notify>,
}

impl ScriptedSyncPort {
    pub fn script_sync(&self, project: ProjectRef, outcome: tc_api::test::ScriptedSync) {
        lock(&self.syncs).insert(project, outcome);
    }

    pub fn script_open(&self, outcome: Result<ProjectRef, ApiError>) {
        lock(&self.opens).push_back(outcome);
    }

    pub fn script_share(&self, outcome: Result<SharedRepo, ApiError>) {
        lock(&self.shares).push_back(outcome);
    }

    /// Forgets everything scripted (a test reset).
    pub fn clear(&self) {
        lock(&self.syncs).clear();
        lock(&self.opens).clear();
        lock(&self.shares).clear();
    }

    /// How many syncs, shares and opens reached the port.
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    /// The next sync waits, once it reached the port, until [`Self::release`].
    pub fn hold_next(&self) {
        self.hold.store(true, Ordering::SeqCst);
    }

    pub fn release(&self) {
        self.released.notify_one();
    }
}

impl SyncPort for ScriptedSyncPort {
    fn sync(&self, app: App, folder: ProjectRef) -> BoxFuture<Result<SyncResultView, JobEnd>> {
        let next = lock(&self.syncs).remove(&folder);
        let held = self.hold.swap(false, Ordering::SeqCst);
        let released = self.released.clone();
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if held {
                released.notified().await;
            }
            match next {
                Some(outcome) => app.scripted_sync(folder, outcome).await,
                None => Err(JobEnd::Failed(ApiError::Unavailable(Feature::Sync))),
            }
        })
    }

    fn share(
        &self,
        _app: App,
        _folder: ProjectRef,
        _owner: ShareOwner,
        _name: RepoName,
        progress: ProgressSink,
    ) -> BoxFuture<Result<SharedRepo, JobEnd>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let next = lock(&self.shares)
            .pop_front()
            .unwrap_or(Err(ApiError::Unavailable(Feature::Sync)));
        Box::pin(async move {
            // A scripted share that Door43 took went through both steps the dialog names.
            if next.is_ok() {
                progress.phase(Phase::Share(SharePhase::Create), None);
                progress.phase(Phase::Share(SharePhase::Push), None);
            }
            next.map_err(JobEnd::Failed)
        })
    }

    fn open(
        &self,
        _app: App,
        _repo: RepoSource,
        _name: ProjectName,
    ) -> BoxFuture<Result<ProjectRef, ApiError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let next = lock(&self.opens)
            .pop_front()
            .unwrap_or(Err(ApiError::Unavailable(Feature::Sync)));
        Box::pin(async move { next })
    }

    fn scripted(&self) -> Option<&ScriptedSyncPort> {
        Some(self)
    }
}

/// The scripted import port of the tests: each call answers the next outcome a test set for
/// it, in order, and `Unavailable(Import)` when none is left. It reads no file and writes
/// nothing; the handlers' checks before it are the app's own.
#[derive(Default)]
pub struct ScriptedImportPort {
    reviews: Mutex<VecDeque<Result<ImportFound, ApiError>>>,
    versions: Mutex<VecDeque<Result<VersionsView, ApiError>>>,
    carry_overs: Mutex<VecDeque<Result<tc_api::import::CarryOver, ApiError>>>,
    runs: Mutex<VecDeque<ScriptedRun>>,
    updates: Mutex<VecDeque<Result<UpdateReport, ApiError>>>,
    calls: AtomicUsize,
}

/// How a scripted run ends.
enum ScriptedRun {
    Answer(Result<ProjectRef, ApiError>),
    /// A long run: it ends only when it is cancelled.
    UntilCancelled,
}

fn next<T>(queue: &Mutex<VecDeque<Result<T, ApiError>>>) -> Result<T, ApiError> {
    lock(queue)
        .pop_front()
        .unwrap_or(Err(ApiError::Unavailable(Feature::Import)))
}

impl ScriptedImportPort {
    pub fn script_review(&self, outcome: Result<ImportFound, ApiError>) {
        lock(&self.reviews).push_back(outcome);
    }

    pub fn script_versions(&self, outcome: Result<VersionsView, ApiError>) {
        lock(&self.versions).push_back(outcome);
    }

    pub fn script_carry_over(&self, outcome: Result<tc_api::import::CarryOver, ApiError>) {
        lock(&self.carry_overs).push_back(outcome);
    }

    /// The next run's project, which the test's store already holds, or its refusal.
    pub fn script_run(&self, outcome: Result<ProjectRef, ApiError>) {
        lock(&self.runs).push_back(ScriptedRun::Answer(outcome));
    }

    /// The next run works until it is cancelled, then ends with nothing made.
    pub fn script_run_until_cancelled(&self) {
        lock(&self.runs).push_back(ScriptedRun::UntilCancelled);
    }

    pub fn script_update(&self, outcome: Result<UpdateReport, ApiError>) {
        lock(&self.updates).push_back(outcome);
    }

    /// How many calls reached the port.
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ImportPort for ScriptedImportPort {
    fn review(&self, _files: Vec<SourceFile>) -> BoxFuture<Result<ImportFound, ApiError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let outcome = next(&self.reviews);
        Box::pin(async move { outcome })
    }

    fn versions(
        &self,
        _app: App,
        _files: Vec<SourceFile>,
    ) -> BoxFuture<Result<VersionsView, JobEnd>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let outcome = next(&self.versions);
        Box::pin(async move { outcome.map_err(JobEnd::Failed) })
    }

    fn carry_over(
        &self,
        _app: App,
        _files: Vec<SourceFile>,
        _found: Vec<(VersionSlot, tc_resources::ResourcePin)>,
    ) -> BoxFuture<Result<tc_api::import::CarryOver, ApiError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let outcome = next(&self.carry_overs);
        Box::pin(async move { outcome })
    }

    /// A cancel asked before the run answers ends it with nothing made, as the real port's
    /// cancel before its create does.
    fn run(
        &self,
        _app: App,
        _files: Vec<SourceFile>,
        _details: ImportDetails,
        _found: Vec<(VersionSlot, tc_resources::ResourcePin)>,
        _license: String,
        _folder: ProjectRef,
        ctx: JobCtx,
    ) -> BoxFuture<Result<ProjectRef, JobEnd>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let script = lock(&self.runs).pop_front();
        Box::pin(async move {
            match script {
                Some(ScriptedRun::UntilCancelled) => {
                    ctx.cancel.cancelled().await;
                    Err(JobEnd::Cancelled)
                }
                Some(ScriptedRun::Answer(outcome)) => {
                    ctx.cancel.check()?;
                    outcome.map_err(JobEnd::Failed)
                }
                None => Err(JobEnd::Failed(ApiError::Unavailable(Feature::Import))),
            }
        })
    }

    fn update_book(
        &self,
        _session: Arc<ProjectSession>,
        _book: BookId,
        _file: SourceFile,
        ctx: JobCtx,
    ) -> BoxFuture<Result<UpdateReport, JobEnd>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let outcome = next(&self.updates);
        Box::pin(async move {
            ctx.cancel.check()?;
            outcome.map_err(JobEnd::Failed)
        })
    }
}
