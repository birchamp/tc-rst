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
    /// A file of the repository has a name no project file may have.
    #[error("the repository holds a file named {path:?}: {rule}")]
    UnsafePath {
        path: String,
        rule: tc_burrito::PathRule,
    },
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the repository's history could not be written: {0}")]
    Git(#[from] git2::Error),
    #[error("the work stopped before it finished: {0}")]
    Stopped(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Create(#[from] tc_store::CreateError),
    #[error(transparent)]
    Store(#[from] tc_store::StoreError),
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
    atomic_create_dirs(&*vfs, &staging)?;
    let outside = vfs.outside_dir(&staging).map_err(io(&staging))?;
    let remote = GitRemote {
        server: client.server().clone(),
        repo: repo.clone(),
        session: session.cloned(),
    };
    let http = client.http().clone();
    let fetch_branch = branch.clone();
    let fetched =
        tokio::task::spawn_blocking(move || fetch(&http, &outside, &remote, &fetch_branch)).await?;
    // A failure leaves the staging folder, which the store removes at its next start (§6.4 N1).
    let (url, files) = fetched?;
    let written = write_tree(&*vfs, &staging, &files).and_then(|()| {
        let mut bytes = serde_json::to_vec_pretty(&SyncFile {
            remote_url: url,
            branch: branch.clone(),
            confirmed: None,
            username_device: None,
            chain_failures: None,
            put_back_unsaid: false,
        })
        .map_err(|e| io(&staging.join(".tc/sync.json"))(e.into()))?;
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
    let mut options = git2::RepositoryInitOptions::new();
    options
        .initial_head(branch)
        .mkpath(true)
        .external_template(false);
    let repo = git2::Repository::init_opts(outside, &options)?;
    let url = remote.url().to_string();
    repo.remote("origin", &url)?;
    repo.config()
        .and_then(|mut c| c.set_str("tc.branch", branch))?;
    let tracking = format!("refs/remotes/origin/{branch}");
    let refspec = format!("+refs/heads/{branch}:{tracking}");
    tc_door43::git::fetch(http, &repo, remote, &[refspec.as_str()])
        .map_err(|e| JoinError::Refused(crate::sync::refusal_of(&e)))?;
    let commit = repo
        .refname_to_id(&tracking)
        .map_err(|_| JoinError::Refused(SyncRefusal::RemoteMissing))?;
    let tree = repo.find_commit(commit).and_then(|c| c.tree())?;
    if let Some(block) = crate::sync::gate_of(&repo, &tree) {
        return Err(JoinError::Blocked(block));
    }
    let files = tree_files(&repo, &tree)?;
    repo.reference(
        &format!("refs/heads/{branch}"),
        commit,
        true,
        "joined by tC",
    )?;
    let mut index = repo.index()?;
    index.read_tree(&tree)?;
    index.write()?;
    Ok((url, files))
}

/// Every regular file of `tree` but foreign dot paths. A name that could land outside the
/// staging folder or not be written on Windows refuses the join: libgit2 lets `\\` and `:`
/// through on every platform.
fn tree_files(
    repo: &git2::Repository,
    tree: &git2::Tree<'_>,
) -> Result<Vec<(String, Vec<u8>)>, JoinError> {
    let mut files = Vec::new();
    let mut unsafe_path = None;
    let walked = tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
        let path = format!("{dir}{}", entry.name().unwrap_or_default());
        let foreign = path.split('/').any(|part| part.starts_with('.'))
            && !matches!(path.as_str(), ".gitignore" | ".gitattributes");
        if entry.kind() != Some(git2::ObjectType::Blob) || foreign {
            return git2::TreeWalkResult::Ok;
        }
        if let Err(rule) = tc_burrito::check_project_path(&path) {
            unsafe_path = Some(JoinError::UnsafePath { path, rule });
            return git2::TreeWalkResult::Abort;
        }
        if let Ok(blob) = repo.find_blob(entry.id()) {
            files.push((path, blob.content().to_vec()));
        }
        git2::TreeWalkResult::Ok
    });
    if let Some(refusal) = unsafe_path {
        return Err(refusal);
    }
    walked?;
    Ok(files)
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
        return Err(io(&full)(std::io::ErrorKind::InvalidFilename.into()));
    };
    atomic_create_dirs(vfs, dir)?;
    atomic_write(vfs, dir, name, bytes, SyncDir::Now).map_err(|e| e.into_inner().into())
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> JoinError + '_ {
    move |source| JoinError::Io {
        path: path.to_owned(),
        source,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "test code: a failed unwrap is a failed test"
)]
mod tests {
    use super::*;

    /// A tree of one file named `name`, written as Door43 could send it: libgit2's tree
    /// builder refuses such names, the object database does not.
    fn tree_named<'r>(repo: &'r git2::Repository, name: &str) -> git2::Tree<'r> {
        let blob = repo.blob(b"text").unwrap();
        let mut raw = format!("100644 {name}\0").into_bytes();
        raw.extend_from_slice(blob.as_bytes());
        let oid = repo
            .odb()
            .unwrap()
            .write(git2::ObjectType::Tree, &raw)
            .unwrap();
        repo.find_tree(oid).unwrap()
    }

    #[test]
    fn a_file_name_that_leaves_the_folder_on_windows_refuses_the_join() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        for (name, rule) in [
            (
                "ingredients\\..\\..\\evil.usfm",
                tc_burrito::PathRule::Backslash,
            ),
            ("c:evil.usfm", tc_burrito::PathRule::ForbiddenChar(':')),
            ("aux.usfm", tc_burrito::PathRule::ReservedName),
            ("book.usfm ", tc_burrito::PathRule::TrailingSpace),
            ("book.usfm.", tc_burrito::PathRule::TrailingDot),
            ("PROGRA~1", tc_burrito::PathRule::ShortName),
        ] {
            let refused = tree_files(&repo, &tree_named(&repo, name));
            assert!(
                matches!(&refused, Err(JoinError::UnsafePath { path, rule: r }) if path == name && *r == rule),
                "{name:?}: {refused:?}"
            );
        }
    }

    #[test]
    fn ordinary_names_are_written_and_foreign_dot_paths_stay_in_the_history() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        for (name, kept) in [
            ("LICENSE copy.md", true),
            (".gitignore", true),
            (".DS_Store", false),
            ("..", false),
        ] {
            let files = tree_files(&repo, &tree_named(&repo, name)).unwrap();
            let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
            assert_eq!(names, if kept { vec![name] } else { vec![] }, "{name:?}");
        }
    }
}
