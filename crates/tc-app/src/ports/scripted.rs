//! The scripted sync port of the tests and journeys: the outcomes a test set, in place of
//! Door43. It sends nothing; a scripted sync writes its teammates' changes through the app.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tc_api::door43::{ProjectName, RepoSource};
use tc_api::error::{ApiError, Feature};
use tc_api::ids::ProjectRef;
use tc_api::jobs::SyncResultView;

use super::SyncPort;
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

    /// Forgets everything scripted (a test reset).
    pub fn clear(&self) {
        lock(&self.syncs).clear();
        lock(&self.opens).clear();
    }

    /// How many syncs and opens reached the port.
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
