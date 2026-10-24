//! The real sync port (E1a): sync and open from Door43 through unit X's `tc-sync`.

use tc_api::door43::{ProjectName, RepoSource};
use tc_api::error::ApiError;
use tc_api::ids::ProjectRef;
use tc_api::jobs::SyncResultView;

use super::SyncPort;
use crate::app::App;
use crate::backend::BoxFuture;
use crate::jobs::JobEnd;

pub struct Door43Sync;

impl SyncPort for Door43Sync {
    fn sync(&self, app: App, folder: ProjectRef) -> BoxFuture<Result<SyncResultView, JobEnd>> {
        Box::pin(async move { app.door43_sync(folder).await })
    }

    fn open(
        &self,
        app: App,
        repo: RepoSource,
        name: ProjectName,
    ) -> BoxFuture<Result<ProjectRef, ApiError>> {
        Box::pin(async move { app.door43_open(repo, name).await })
    }
}
