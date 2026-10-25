//! "Get a project from Door43" (`docs/STORAGE.md` §9.6 Join): the repository is fetched
//! into a new history in a staging folder, every file of its tree is written with the
//! store's atomic writes (never a checkout, so no line ending is touched), and the folder is
//! placed among the projects. The open that follows mints this computer's device and folds.

use std::path::{Path, PathBuf};
use tc_api::refusal::{SyncBlock, SyncRefusal};
use tc_door43::{Door43Client, GitRemote, RepoRef, Session};
use tc_store::{StagingKind, Store, SyncDir, SyncFile, atomic_create_dirs, atomic_write};

#[derive(Debug, thiserror::Error)]
pub enum JoinError {
    #[error("the repository cannot be joined: {0:?}")]
    Blocked(SyncBlock),
    #[error("the repository could not be read: {0:?}")]
    Refused(SyncRefusal),
    #[error("{0}")]
    Io(String),
    #[error(transparent)]
    Create(#[from] tc_store::CreateError),
}

/// Fetches `repo` and places it as the project `name`. Returns its folder.
pub async fn join(
    store: &Store,
    client: &Door43Client,
    session: Option<&Session>,
    repo: &RepoRef,
    name: &str,
) -> Result<PathBuf, JoinError> {
    if !client.internet().is_on() {
        return Err(JoinError::Refused(SyncRefusal::Offline));
    }
    let info = client
        .repo(repo, session)
        .await
        .map_err(|e| JoinError::Refused(crate::sync::refusal_of(&e)))?;
    let branch = info.default_branch.clone();
    let staging = store.staging(StagingKind::Join);
    let vfs = store.vfs().clone();
    atomic_create_dirs(&*vfs, &staging).map_err(|e| JoinError::Io(e.to_string()))?;
    let outside = vfs
        .outside_dir(&staging)
        .map_err(|e| JoinError::Io(e.to_string()))?;
    let remote = GitRemote {
        server: client.server().clone(),
        repo: repo.clone(),
        session: session.cloned(),
    };
    let http = client.http().clone();
    let fetch_branch = branch.clone();
    let fetched =
        tokio::task::spawn_blocking(move || fetch(&http, &outside, &remote, &fetch_branch))
            .await
            .map_err(|e| JoinError::Io(e.to_string()))?;
    // A failure leaves the staging folder, which the store removes at its next start (§6.4 N1).
    let (url, files) = fetched?;
    let written = write_tree(&*vfs, &staging, &files).and_then(|()| {
        let mut bytes = serde_json::to_vec_pretty(&SyncFile {
            remote_url: url,
            branch: branch.clone(),
            confirmed: None,
            username_device: None,
        })
        .map_err(|e| JoinError::Io(e.to_string()))?;
        bytes.push(b'\n');
        write_file(&*vfs, &staging, ".tc/sync.json", &bytes)
    });
    written?;
    Ok(store.place(&staging, name)?)
}

/// The fetched repository's address, and the files of its tree by path.
type Fetched = (String, Vec<(String, Vec<u8>)>);

/// The fetch into a new history at `outside`, the branch set to what it fetched, and the
/// files to write: every regular file of the tree but foreign dot paths, which stay in the
/// history only (§9.4).
fn fetch(
    http: &tc_door43::Http,
    outside: &Path,
    remote: &GitRemote,
    branch: &str,
) -> Result<Fetched, JoinError> {
    let git = |e: git2::Error| JoinError::Io(e.message().to_owned());
    let mut options = git2::RepositoryInitOptions::new();
    options
        .initial_head(branch)
        .mkpath(true)
        .external_template(false);
    let repo = git2::Repository::init_opts(outside, &options).map_err(git)?;
    let url = remote.url().to_string();
    repo.remote("origin", &url).map_err(git)?;
    repo.config()
        .and_then(|mut c| c.set_str("tc.branch", branch))
        .map_err(git)?;
    let tracking = format!("refs/remotes/origin/{branch}");
    let refspec = format!("+refs/heads/{branch}:{tracking}");
    tc_door43::git::fetch(http, &repo, remote, &[refspec.as_str()])
        .map_err(|e| JoinError::Refused(crate::sync::refusal_of(&e)))?;
    let commit = repo
        .refname_to_id(&tracking)
        .map_err(|_| JoinError::Refused(SyncRefusal::RemoteMissing))?;
    let tree = repo
        .find_commit(commit)
        .and_then(|c| c.tree())
        .map_err(git)?;
    if let Some(block) = crate::sync::gate_of(&repo, &tree) {
        return Err(JoinError::Blocked(block));
    }
    let mut files = Vec::new();
    tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
        let path = format!("{dir}{}", entry.name().unwrap_or_default());
        let foreign = path.split('/').any(|part| part.starts_with('.'))
            && !matches!(path.as_str(), ".gitignore" | ".gitattributes");
        if entry.kind() == Some(git2::ObjectType::Blob)
            && !foreign
            && let Ok(blob) = repo.find_blob(entry.id())
        {
            files.push((path, blob.content().to_vec()));
        }
        git2::TreeWalkResult::Ok
    })
    .map_err(git)?;
    repo.reference(
        &format!("refs/heads/{branch}"),
        commit,
        true,
        "joined by tC",
    )
    .map_err(git)?;
    let mut index = repo.index().map_err(git)?;
    index.read_tree(&tree).map_err(git)?;
    index.write().map_err(git)?;
    Ok((url, files))
}

fn write_tree(
    vfs: &dyn tc_store::Vfs,
    root: &Path,
    files: &[(String, Vec<u8>)],
) -> Result<(), JoinError> {
    for (path, bytes) in files {
        write_file(vfs, root, path, bytes)?;
    }
    Ok(())
}

fn write_file(
    vfs: &dyn tc_store::Vfs,
    root: &Path,
    path: &str,
    bytes: &[u8],
) -> Result<(), JoinError> {
    let full = root.join(path);
    let (Some(dir), Some(name)) = (full.parent(), full.file_name().and_then(|n| n.to_str())) else {
        return Err(JoinError::Io(format!("{path} has no name")));
    };
    atomic_create_dirs(vfs, dir).map_err(|e| JoinError::Io(e.to_string()))?;
    atomic_write(vfs, dir, name, bytes, SyncDir::Now)
        .map_err(|e| JoinError::Io(e.into_inner().to_string()))
}
