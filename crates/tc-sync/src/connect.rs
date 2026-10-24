//! Connecting a project to a Door43 repository (`docs/STORAGE.md` §9.6): `origin` and
//! `tc.branch` in its git repository, and `.tc/sync.json`, which rebuilds them if `.git` is
//! lost. The first sync after it joins the two histories.

use tc_door43::{GitRemote, RepoRef, ServerUrl};
use tc_store::{ProjectHandle, StoreError, SyncFile};

#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("the project has no history this session")]
    NoHistory,
    #[error("the project's history could not be set up: {0}")]
    Git(String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// From now on `project` syncs with `repo` on `server`, on `branch`. An `origin` it had is
/// pointed at the new address.
pub async fn connect(
    project: &ProjectHandle,
    server: &ServerUrl,
    repo: &RepoRef,
    branch: &str,
) -> Result<(), ConnectError> {
    let git_dir = project.facts().git_dir.ok_or(ConnectError::NoHistory)?;
    let url = GitRemote {
        server: server.clone(),
        repo: repo.clone(),
        session: None,
    }
    .url()
    .to_string();
    let git = |e: git2::Error| ConnectError::Git(e.message().to_owned());
    let history = git2::Repository::open(git_dir).map_err(git)?;
    match history.find_remote("origin") {
        Ok(_) => history.remote_set_url("origin", &url).map_err(git)?,
        Err(_) => {
            history.remote("origin", &url).map_err(git)?;
        }
    }
    history
        .config()
        .and_then(|mut c| c.set_str("tc.branch", branch))
        .map_err(git)?;
    project
        .set_sync_file(SyncFile {
            remote_url: url,
            branch: branch.to_owned(),
            confirmed: None,
        })
        .await?;
    Ok(())
}
