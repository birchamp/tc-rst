//! Sharing a project on Door43 for the first time (`docs/STORAGE.md` §9.6 Share): a new
//! repository, the project connected to it, and a sync that pushes its history.

use crate::sync::{Pause, SyncReport, refusal_of};
use tc_api::refusal::SyncRefusal;
use tc_door43::{Door43Client, Door43Error, NewRepoName, NewRepoOwner, Session, Visibility};
use tc_store::ProjectHandle;

#[derive(Debug, thiserror::Error)]
pub enum ShareError {
    /// A repository with this name exists already: the person picks another.
    #[error("a repository with this name exists already")]
    NameExists,
    #[error("the repository could not be made: {0:?}")]
    Refused(SyncRefusal),
    #[error(transparent)]
    Connect(#[from] crate::connect::ConnectError),
}

/// Makes `name` under `owner`, connects `project` to it on `main` and syncs. The sync's
/// report says whether the push was confirmed.
pub async fn share(
    project: &ProjectHandle,
    client: &Door43Client,
    session: &Session,
    owner: &NewRepoOwner,
    name: &NewRepoName,
    visibility: Visibility,
    pause: &impl Pause,
) -> Result<SyncReport, ShareError> {
    if !client.internet().is_on() {
        return Err(ShareError::Refused(SyncRefusal::Offline));
    }
    let created = match client.create_repo(session, owner, name, visibility).await {
        Ok(created) => created,
        Err(Door43Error::NameExists { .. }) => return Err(ShareError::NameExists),
        Err(e) => return Err(ShareError::Refused(refusal_of(&e))),
    };
    crate::connect::connect(project, client.server(), &created.repo, "main").await?;
    Ok(crate::sync::sync(project, client, Some(session), pause).await)
}
