//! The outside jobs' ports (UI-2, APP-DESIGN 3.x "ports"): sync, share and open from Door43,
//! exchange and import each reach the outside through one port in [`crate::app::AppDeps`].
//! Release builds get ports that answer `Unavailable`; tests and journeys get a scripted one,
//! so release and tests run the same business code.

use tc_api::door43::{ProjectName, RepoSource};
use tc_api::error::{ApiError, Feature};
use tc_api::ids::ProjectRef;
use tc_api::jobs::SyncResultView;

use crate::app::App;
use crate::backend::BoxFuture;
use crate::jobs::JobEnd;

#[cfg(feature = "test-hooks")]
pub mod scripted;

/// Sync, share and open from Door43. E1a adds the real sync and open; E1b adds share's push.
pub trait SyncPort: Send + Sync {
    /// One sync of `folder`, after its checkpoint. `app` is the app the sync's intake writes
    /// the received changes through.
    fn sync(&self, app: App, folder: ProjectRef) -> BoxFuture<Result<SyncResultView, JobEnd>>;

    /// The open of Door43's `repo` as the project `name`: the project it added.
    fn open(
        &self,
        app: App,
        repo: RepoSource,
        name: ProjectName,
    ) -> BoxFuture<Result<ProjectRef, ApiError>>;

    /// The scripted port, for the test hooks that script it.
    #[cfg(feature = "test-hooks")]
    fn scripted(&self) -> Option<&scripted::ScriptedSyncPort> {
        None
    }
}

/// USB exchange. E2 adds its methods and its real port.
pub trait ExchangePort: Send + Sync {}

/// Import of other projects. F1 adds its methods and its real port.
pub trait ImportPort: Send + Sync {}

/// The release sync until E1a: nothing to sync with yet.
pub struct UnavailableSync;

impl SyncPort for UnavailableSync {
    fn sync(&self, _app: App, _folder: ProjectRef) -> BoxFuture<Result<SyncResultView, JobEnd>> {
        Box::pin(async { Err(JobEnd::Failed(ApiError::Unavailable(Feature::Sync))) })
    }

    fn open(
        &self,
        _app: App,
        _repo: RepoSource,
        _name: ProjectName,
    ) -> BoxFuture<Result<ProjectRef, ApiError>> {
        Box::pin(async { Err(ApiError::Unavailable(Feature::Sync)) })
    }
}

/// The release exchange until E2.
pub struct UnavailableExchange;

impl ExchangePort for UnavailableExchange {}

/// The release import until F1.
pub struct UnavailableImport;

impl ImportPort for UnavailableImport {}
