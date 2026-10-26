//! The outside jobs' ports (UI-2, APP-DESIGN 3.x "ports"): sync, share and open from Door43,
//! exchange and import each reach the outside through one port in [`crate::app::AppDeps`].
//! Release builds get ports that answer `Unavailable`; tests and journeys get a scripted one,
//! so release and tests run the same business code.

use tc_api::door43::{ProjectName, RepoSource};
use tc_api::error::ApiError;
use tc_api::ids::ProjectRef;
use tc_api::jobs::SyncResultView;

use crate::app::App;
use crate::backend::BoxFuture;
use crate::jobs::JobEnd;

pub mod door43;
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

/// The release exchange until E2.
pub struct UnavailableExchange;

impl ExchangePort for UnavailableExchange {}

/// The release import until F1.
pub struct UnavailableImport;

impl ImportPort for UnavailableImport {}
