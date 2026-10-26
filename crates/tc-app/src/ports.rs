//! The outside jobs' ports (UI-2, APP-DESIGN 3.x "ports"): sync, share and open from Door43,
//! exchange and import each reach the outside through one port in [`crate::app::AppDeps`].
//! Release builds get ports that answer `Unavailable`; tests and journeys get a scripted one,
//! so release and tests run the same business code.

use std::sync::Arc;

use tc_api::door43::{ProjectName, RepoSource};
use tc_api::error::{ApiError, Feature};
use tc_api::ids::ProjectRef;
use tc_api::import::{ImportDetails, ImportFound, UpdateReport, VersionSlot, VersionsView};
use tc_api::jobs::SyncResultView;
use tc_core::BookId;
use tc_import::SourceFile;

use crate::app::App;
use crate::backend::BoxFuture;
use crate::jobs::{JobCtx, JobEnd};
use crate::session::ProjectSession;

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

/// Import of a translationCore 3 project, and Update a book from USFM (F1). The handlers check
/// the details, the names and the file before a port is called, and hold the uploads; the
/// port reads and writes. [`crate::import::TcImport`] is the real one.
pub trait ImportPort: Send + Sync {
    /// What the files hold, for the review step. Nothing is written.
    fn review(&self, files: Vec<SourceFile>) -> BoxFuture<Result<ImportFound, ApiError>>;

    /// The resource versions the files' projects name, looked up on Door43.
    fn versions(&self, app: App, files: Vec<SourceFile>)
    -> BoxFuture<Result<VersionsView, JobEnd>>;

    /// What "Use installed versions" does to the decisions of the slots not in `found`,
    /// before anything is written.
    fn carry_over(
        &self,
        app: App,
        files: Vec<SourceFile>,
        found: Vec<(VersionSlot, tc_resources::ResourcePin)>,
    ) -> BoxFuture<Result<tc_api::import::CarryOver, ApiError>>;

    /// Makes the project `folder` from the files with `details`: one whole project, or none.
    /// `found` are the versions the person accepted from the lookup, one per slot; `license`
    /// is one the files name, `""` for none.
    #[expect(
        clippy::too_many_arguments,
        reason = "each is one thing the person chose or the job holds; a struct of them would \
                  be built once and taken apart once"
    )]
    fn run(
        &self,
        app: App,
        files: Vec<SourceFile>,
        details: ImportDetails,
        found: Vec<(VersionSlot, tc_resources::ResourcePin)>,
        license: String,
        folder: ProjectRef,
        ctx: JobCtx,
    ) -> BoxFuture<Result<ProjectRef, JobEnd>>;

    /// Update a book from USFM on the open project: the `Fill`, then the `Review` (STORAGE
    /// §13.6 U5), and the report of both.
    fn update_book(
        &self,
        session: Arc<ProjectSession>,
        book: BookId,
        file: SourceFile,
        ctx: JobCtx,
    ) -> BoxFuture<Result<UpdateReport, JobEnd>>;
}

/// The release exchange until E2.
pub struct UnavailableExchange;

impl ExchangePort for UnavailableExchange {}

/// An import that answers `Unavailable`, for a test that needs none.
pub struct UnavailableImport;

fn no_import<T: Send + 'static>() -> BoxFuture<Result<T, JobEnd>> {
    Box::pin(async { Err(JobEnd::Failed(ApiError::Unavailable(Feature::Import))) })
}

impl ImportPort for UnavailableImport {
    fn review(&self, _files: Vec<SourceFile>) -> BoxFuture<Result<ImportFound, ApiError>> {
        Box::pin(async { Err(ApiError::Unavailable(Feature::Import)) })
    }

    fn versions(
        &self,
        _app: App,
        _files: Vec<SourceFile>,
    ) -> BoxFuture<Result<VersionsView, JobEnd>> {
        no_import()
    }

    fn carry_over(
        &self,
        _app: App,
        _files: Vec<SourceFile>,
        _found: Vec<(VersionSlot, tc_resources::ResourcePin)>,
    ) -> BoxFuture<Result<tc_api::import::CarryOver, ApiError>> {
        Box::pin(async { Err(ApiError::Unavailable(Feature::Import)) })
    }

    fn run(
        &self,
        _app: App,
        _files: Vec<SourceFile>,
        _details: ImportDetails,
        _found: Vec<(VersionSlot, tc_resources::ResourcePin)>,
        _license: String,
        _folder: ProjectRef,
        _ctx: JobCtx,
    ) -> BoxFuture<Result<ProjectRef, JobEnd>> {
        no_import()
    }

    fn update_book(
        &self,
        _session: Arc<ProjectSession>,
        _book: BookId,
        _file: SourceFile,
        _ctx: JobCtx,
    ) -> BoxFuture<Result<UpdateReport, JobEnd>> {
        no_import()
    }
}
