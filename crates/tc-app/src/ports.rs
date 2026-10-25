//! The outside jobs' ports (UI-2, APP-DESIGN 3.x "ports"): sync, share and open from Door43,
//! exchange and import each reach the outside through one port in [`crate::app::AppDeps`].
//! Release builds get ports that answer `Unavailable`; tests and journeys get a scripted one,
//! so release and tests run the same business code.

use std::path::PathBuf;

use tc_api::door43::{ProjectName, RepoSource};
use tc_api::error::{ApiError, Feature};
use tc_api::exchange::{ExchangeFile, ExportedView, IdentifyView, ReceivedView};
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

/// USB exchange (E2; STORAGE §10). The app holds the project's exchange flow while a call runs,
/// so a receive never runs beside a sync of the same project (INV-NEVER-BESIDE-A-SYNC).
pub trait ExchangePort: Send + Sync {
    /// §10.1: checkpoint `project`, write the exchange file into `destination`, read it back
    /// from the device. Ends verified, or failed with the file deleted.
    fn export(
        &self,
        app: App,
        project: ProjectRef,
        destination: PathBuf,
    ) -> BoxFuture<Result<ExportedView, JobEnd>>;

    /// §10.2 R1-R3: the project the file holds, with nothing written.
    fn identify(&self, app: App, file: ExchangeFile) -> BoxFuture<Result<IdentifyView, ApiError>>;

    /// §10.2 R4-R8: the file's packs taken into `project`, whole or not at all, and only
    /// the packs: a file edited by hand inside the zip is listed, never taken in.
    fn receive(
        &self,
        app: App,
        project: ProjectRef,
        file: ExchangeFile,
    ) -> BoxFuture<Result<ReceivedView, JobEnd>>;

    #[cfg(feature = "test-hooks")]
    fn scripted(&self) -> Option<&scripted::ScriptedExchangePort> {
        None
    }
}

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

/// The release exchange until E2's real port lands.
pub struct UnavailableExchange;

impl ExchangePort for UnavailableExchange {
    fn export(
        &self,
        _app: App,
        _project: ProjectRef,
        _destination: PathBuf,
    ) -> BoxFuture<Result<ExportedView, JobEnd>> {
        Box::pin(async { Err(JobEnd::Failed(ApiError::Unavailable(Feature::Exchange))) })
    }

    fn identify(
        &self,
        _app: App,
        _file: ExchangeFile,
    ) -> BoxFuture<Result<IdentifyView, ApiError>> {
        Box::pin(async { Err(ApiError::Unavailable(Feature::Exchange)) })
    }

    fn receive(
        &self,
        _app: App,
        _project: ProjectRef,
        _file: ExchangeFile,
    ) -> BoxFuture<Result<ReceivedView, JobEnd>> {
        Box::pin(async { Err(JobEnd::Failed(ApiError::Unavailable(Feature::Exchange))) })
    }
}

/// The release import until F1.
pub struct UnavailableImport;

impl ImportPort for UnavailableImport {}
