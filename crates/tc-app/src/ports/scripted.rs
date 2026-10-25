//! The scripted sync port of the tests and journeys: the outcomes a test set, in place of
//! Door43. It sends nothing; a scripted sync writes its teammates' changes through the app.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use std::path::PathBuf;
use std::sync::Arc;

use tc_api::door43::{ProjectName, RepoSource};
use tc_api::error::{ApiError, Feature};
use tc_api::exchange::{ExchangeFile, ExportedView, HandEdited, IdentifyView, ReceivedView};
use tc_api::ids::ProjectRef;
use tc_api::jobs::SyncResultView;
use tc_api::refusal::ExchangeRefusal;

use super::{ExchangePort, SyncPort};
use crate::app::{App, lock};
use crate::backend::BoxFuture;
use crate::jobs::JobEnd;

#[derive(Default)]
pub struct ScriptedSyncPort {
    /// How the next sync of each project ends: each setting replaces the one before, as a
    /// change on Door43 changes what the next sync finds.
    syncs: Mutex<HashMap<ProjectRef, tc_api::test::ScriptedSync>>,
    /// The outcomes of the next opens, in order; `Unavailable(Sync)` when none is left.
    opens: Mutex<VecDeque<Result<ProjectRef, ApiError>>>,
    calls: AtomicUsize,
}

impl ScriptedSyncPort {
    pub fn script_sync(&self, project: ProjectRef, outcome: tc_api::test::ScriptedSync) {
        lock(&self.syncs).insert(project, outcome);
    }

    pub fn script_open(&self, outcome: Result<ProjectRef, ApiError>) {
        lock(&self.opens).push_back(outcome);
    }

    /// Forgets everything scripted (a test reset).
    pub fn clear(&self) {
        lock(&self.syncs).clear();
        lock(&self.opens).clear();
    }

    /// How many syncs and opens reached the port.
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl SyncPort for ScriptedSyncPort {
    fn sync(&self, app: App, folder: ProjectRef) -> BoxFuture<Result<SyncResultView, JobEnd>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let next = lock(&self.syncs).remove(&folder);
        Box::pin(async move {
            match next {
                Some(outcome) => app.scripted_sync(folder, outcome).await,
                None => Err(JobEnd::Failed(ApiError::Unavailable(Feature::Sync))),
            }
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

/// How a scripted receive ends: the teammates' changes the file holds, written through the
/// app as a receive's intake writes them, and the files inside it that were edited by hand.
#[derive(Debug, Clone)]
pub enum ScriptedReceive {
    Received {
        changes: tc_api::test::ScriptedSync,
        hand_edited: Vec<HandEdited>,
    },
    Failed(ExchangeRefusal),
}

/// The scripted exchange port of the tests and journeys: no file is written or read; a
/// scripted receive writes its teammates' changes through the app, as the scripted sync does.
#[derive(Default)]
pub struct ScriptedExchangePort {
    exports: Mutex<HashMap<ProjectRef, Result<String, ExchangeRefusal>>>,
    receives: Mutex<HashMap<ProjectRef, ScriptedReceive>>,
    identify: Mutex<Option<Result<IdentifyView, ExchangeRefusal>>>,
    /// A receive of the project waits here before it runs, so a test can post a sync meanwhile.
    holds: Mutex<HashMap<ProjectRef, Arc<tokio::sync::Notify>>>,
    calls: AtomicUsize,
}

impl ScriptedExchangePort {
    pub fn script_export(&self, project: ProjectRef, outcome: Result<String, ExchangeRefusal>) {
        lock(&self.exports).insert(project, outcome);
    }

    pub fn script_receive(&self, project: ProjectRef, outcome: ScriptedReceive) {
        lock(&self.receives).insert(project, outcome);
    }

    pub fn script_identify(&self, outcome: Result<IdentifyView, ExchangeRefusal>) {
        *lock(&self.identify) = Some(outcome);
    }

    /// The next receive of `project` waits until the returned handle is notified.
    pub fn hold_receive(&self, project: ProjectRef) -> Arc<tokio::sync::Notify> {
        let release = Arc::new(tokio::sync::Notify::new());
        lock(&self.holds).insert(project, release.clone());
        release
    }

    pub fn clear(&self) {
        lock(&self.exports).clear();
        lock(&self.receives).clear();
        *lock(&self.identify) = None;
        lock(&self.holds).clear();
    }

    /// How many exports, identifies and receives reached the port.
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

fn refused(refusal: ExchangeRefusal) -> JobEnd {
    JobEnd::Failed(ApiError::Refused(tc_api::Refusal::Exchange(refusal)))
}

impl ExchangePort for ScriptedExchangePort {
    fn export(
        &self,
        _app: App,
        project: ProjectRef,
        _destination: PathBuf,
    ) -> BoxFuture<Result<ExportedView, JobEnd>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let next = lock(&self.exports).remove(&project);
        Box::pin(async move {
            match next {
                Some(Ok(path)) => Ok(ExportedView { path }),
                Some(Err(refusal)) => Err(refused(refusal)),
                None => Err(JobEnd::Failed(ApiError::Unavailable(Feature::Exchange))),
            }
        })
    }

    fn identify(
        &self,
        _app: App,
        _file: ExchangeFile,
    ) -> BoxFuture<Result<IdentifyView, ApiError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let next = lock(&self.identify).take();
        Box::pin(async move {
            match next {
                Some(Ok(view)) => Ok(view),
                Some(Err(refusal)) => Err(ApiError::Refused(tc_api::Refusal::Exchange(refusal))),
                None => Err(ApiError::Unavailable(Feature::Exchange)),
            }
        })
    }

    fn receive(
        &self,
        app: App,
        project: ProjectRef,
        _file: ExchangeFile,
    ) -> BoxFuture<Result<ReceivedView, JobEnd>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let hold = lock(&self.holds).remove(&project);
        let next = lock(&self.receives).remove(&project);
        Box::pin(async move {
            if let Some(release) = hold {
                release.notified().await;
            }
            match next {
                Some(ScriptedReceive::Received {
                    changes,
                    hand_edited,
                }) => app.scripted_receive(project, changes, hand_edited).await,
                Some(ScriptedReceive::Failed(refusal)) => Err(refused(refusal)),
                None => Err(JobEnd::Failed(ApiError::Unavailable(Feature::Exchange))),
            }
        })
    }

    fn scripted(&self) -> Option<&ScriptedExchangePort> {
        Some(self)
    }
}
