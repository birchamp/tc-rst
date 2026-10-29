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
    /// A file of the repository has a name that could leave the project folder, or a file
    /// the app owns has a name Windows cannot write.
    #[error("the repository holds a file named {path:?}: {problem:?}")]
    UnsafePath {
        path: String,
        problem: tc_door43::NameProblem,
    },
    /// Two files the app owns that Windows or macOS would store as one file.
    #[error("the repository holds {first:?} and {second:?}, one file on Windows or macOS")]
    CollidingPaths { first: String, second: String },
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

/// A project fetched from Door43 and placed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Joined {
    pub folder: PathBuf,
    /// Foreign files of the repository that were not written: Windows cannot write the name
    /// as it is, or Windows or macOS would store two of them as one file. The history keeps
    /// them, and a checkpoint keeps them in every commit (§9.4), on every computer alike.
    pub not_written: Vec<String>,
}

/// Fetches `repo` and places it as the project `name`.
pub async fn join(
    store: &Store,
    client: &Door43Client,
    session: Option<&Session>,
    repo: &RepoRef,
    name: &str,
) -> Result<Joined, JoinError> {
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
    let (url, TreeFiles { files, not_written }) = fetched?;
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
    Ok(Joined {
        folder: store.place(&staging, name)?,
        not_written,
    })
}

/// The fetched repository's address, and the files of its tree by path.
type Fetched = (String, TreeFiles);

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

/// The files of `tree` to write, and the foreign files not written on disk
/// ([`crate::names::not_written`]). Foreign dot paths stay in the history only (§9.4). A
/// name that could land outside the staging folder refuses the join, and so does an owned
/// file Windows cannot write: libgit2 lets `\\` and `:` through on every platform.
fn tree_files(repo: &git2::Repository, tree: &git2::Tree<'_>) -> Result<TreeFiles, JoinError> {
    let mut wanted = Vec::new();
    let mut unsafe_path = None;
    let walked = tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
        if entry.kind() != Some(git2::ObjectType::Blob) {
            return git2::TreeWalkResult::Ok;
        }
        let path = format!("{dir}{}", entry.name().unwrap_or_default());
        let foreign_dot = path.split('/').any(|part| part.starts_with('.'))
            && !matches!(path.as_str(), ".gitignore" | ".gitattributes");
        if !foreign_dot {
            wanted.push((path, entry.id()));
        } else if let Some(problem @ tc_door43::NameProblem::EscapesRoot(_)) =
            tc_door43::name_problem(&path)
        {
            unsafe_path = Some(JoinError::UnsafePath { path, problem });
            return git2::TreeWalkResult::Abort;
        }
        git2::TreeWalkResult::Ok
    });
    if let Some(refusal) = unsafe_path {
        return Err(refusal);
    }
    walked?;
    let not_written =
        crate::names::not_written(wanted.iter().map(|(p, _)| p.as_str())).map_err(|refusal| {
            match refusal {
                crate::names::NameRefusal::Unsafe { path, problem } => {
                    JoinError::UnsafePath { path, problem }
                }
                crate::names::NameRefusal::Colliding { first, second } => {
                    JoinError::CollidingPaths { first, second }
                }
            }
        })?;
    let mut files = Vec::new();
    for (path, oid) in wanted {
        if !not_written.contains(&path)
            && let Ok(blob) = repo.find_blob(oid)
        {
            files.push((path, blob.content().to_vec()));
        }
    }
    Ok(TreeFiles {
        files,
        not_written: not_written.into_iter().collect(),
    })
}

#[derive(Debug)]
struct TreeFiles {
    files: Vec<(String, Vec<u8>)>,
    not_written: Vec<String>,
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

    /// A tree of files at `paths`, each holding `text`, written as Door43 could send it:
    /// libgit2's tree builder refuses such names, the object database does not.
    fn tree_of<'r>(repo: &'r git2::Repository, paths: &[&str]) -> git2::Tree<'r> {
        fn write(repo: &git2::Repository, paths: &[&str]) -> git2::Oid {
            let mut children: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
            let mut raw = Vec::new();
            let blob = repo.blob(b"text").unwrap();
            let mut entries: Vec<(String, &str, git2::Oid)> = Vec::new();
            for path in paths {
                match path.split_once('/') {
                    Some((dir, rest)) => children.entry(dir).or_default().push(rest),
                    None => entries.push(((*path).to_owned(), "100644", blob)),
                }
            }
            for (dir, rest) in children {
                entries.push((format!("{dir}/"), "40000", write(repo, &rest)));
            }
            // Git orders a tree's entries by name, a folder's name with a '/' after it.
            entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            for (name, mode, oid) in entries {
                let name = name.strip_suffix('/').unwrap_or(&name);
                raw.extend_from_slice(format!("{mode} {name}\0").as_bytes());
                raw.extend_from_slice(oid.as_bytes());
            }
            repo.odb()
                .unwrap()
                .write(git2::ObjectType::Tree, &raw)
                .unwrap()
        }
        repo.find_tree(write(repo, paths)).unwrap()
    }

    fn tree_named<'r>(repo: &'r git2::Repository, name: &str) -> git2::Tree<'r> {
        tree_of(repo, &[name])
    }

    #[test]
    fn a_file_name_that_leaves_the_folder_refuses_the_join() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        for name in [
            "ingredients\\..\\..\\evil.usfm",
            "c:evil.usfm",
            "a\\b.md",
            "..",
            ".. ",
        ] {
            let refused = tree_files(&repo, &tree_named(&repo, name));
            assert!(
                matches!(
                    &refused,
                    Err(JoinError::UnsafePath { path, problem: tc_door43::NameProblem::EscapesRoot(_) })
                        if path == name
                ),
                "{name:?}: {refused:?}"
            );
        }
    }

    #[test]
    fn an_owned_file_windows_cannot_write_refuses_the_join() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        for name in [
            "ingredients/con.usfm",
            "ingredients/book.usfm ",
            "ingredients/book.usfm.",
            "ingredients/PROGRA~1",
        ] {
            let refused = tree_files(&repo, &tree_named(&repo, name));
            assert!(
                matches!(
                    &refused,
                    Err(JoinError::UnsafePath { path, problem: tc_door43::NameProblem::NotPortable { .. } })
                        if path == name
                ),
                "{name:?}: {refused:?}"
            );
        }
        let refused = tree_files(
            &repo,
            &tree_of(&repo, &["ingredients/A.usfm", "ingredients/a.usfm"]),
        );
        assert!(
            matches!(&refused, Err(JoinError::CollidingPaths { first, second })
                if first == "ingredients/A.usfm" && second == "ingredients/a.usfm"),
            "{refused:?}"
        );
    }

    /// Owner ruling "carry, skip on disk": the join succeeds, the file stays in the history,
    /// and it is listed.
    #[test]
    fn a_foreign_file_windows_cannot_write_is_listed_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let joined = tree_files(
            &repo,
            &tree_of(
                &repo,
                &[
                    "A.md",
                    "README.md",
                    "a.md",
                    "aux.md",
                    "book.md ",
                    "ingredients/TIT.usfm",
                    "notes~1.md",
                ],
            ),
        )
        .unwrap();
        let written: Vec<&str> = joined.files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(written, ["README.md", "ingredients/TIT.usfm"]);
        assert_eq!(
            joined.not_written,
            ["A.md", "a.md", "aux.md", "book.md ", "notes~1.md"]
        );
    }

    #[test]
    fn ordinary_names_are_written_and_foreign_dot_paths_stay_in_the_history() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        for (name, kept) in [
            ("LICENSE copy.md", true),
            (".gitignore", true),
            (".DS_Store", false),
            (".gitea/con.txt", false),
        ] {
            let joined = tree_files(&repo, &tree_named(&repo, name)).unwrap();
            let names: Vec<&str> = joined.files.iter().map(|(n, _)| n.as_str()).collect();
            assert_eq!(names, if kept { vec![name] } else { vec![] }, "{name:?}");
            assert!(joined.not_written.is_empty(), "{name:?}");
        }
    }
}
