//! The project's local git repository (`docs/STORAGE.md` §9, §5.13, §6.1 O3).
//!
//! Git is a backup and the transport to Door43, never the source of truth (I-rules of §1): the
//! packs are. This module is the one place libgit2 is called for a project's own repository.
//! It runs outside the [`crate::vfs::Vfs`] (libgit2 does its own file I/O in `.git`), in the
//! folder [`crate::vfs::Vfs::outside_dir`] names, which is the project folder itself on a real
//! filesystem and a real mirror folder beside the model of [`crate::fault::FaultFs`].
//!
//! Every tree is built in memory from the fold's projection and written as objects; the
//! working tree is never checked out into, so libgit2 never rewrites a line ending or touches a
//! file the person may have open. A commit object is written raw, because libgit2's signature
//! refuses the empty email D85 requires.
//!
//! A [`git2::Repository`] is not `Sync`: every thread that needs one opens its own [`Git`].

use crate::counters::Counters;
use crate::hooks::{Failure, Hooks, Point};
use git2::build::TreeUpdateBuilder;
use git2::{
    ConfigLevel, ErrorCode, FileMode, ObjectType, Oid, Repository, RepositoryInitOptions,
    RepositoryOpenFlags, TreeWalkMode, TreeWalkResult,
};
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use tc_journal::GitOid;

/// The branch a new repository starts on.
pub const BRANCH: &str = "main";
/// The reference that keeps every carried-file version recorded on this computer reachable,
/// so a repack never prunes one that was superseded before a checkpoint committed it (§9.8).
pub const CARRIED_REF: &str = "refs/tc/carried";
/// The trailers an app commit carries (§5.13 C5).
pub const TRAILER_FRONTIER: &str = "Tc-Frontier";
pub const TRAILER_FORMAT: &str = "Tc-Format";
pub const TRAILER_DEVICE: &str = "Tc-Device";

/// What the history could not do, in words.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("the project's history could not {what}: {source}")]
    Git {
        what: &'static str,
        #[source]
        source: git2::Error,
    },
    #[error("the project's history: {0}")]
    Other(String),
    /// The branch is not where the checkpoint read it: someone else moved it.
    #[error("the project's history moved on while a checkpoint was made")]
    HeadMoved,
}

impl GitError {
    /// libgit2 refused the folder because another account owns it (not damage, §6.1 O3).
    pub fn is_owner(&self) -> bool {
        matches!(self, Self::Git { source, .. } if source.code() == ErrorCode::Owner)
    }
}

impl From<GitError> for crate::error::StoreError {
    fn from(e: GitError) -> Self {
        crate::error::StoreError::Other(e.to_string())
    }
}

pub(crate) fn at<T>(what: &'static str, result: Result<T, git2::Error>) -> Result<T, GitError> {
    result.map_err(|source| GitError::Git { what, source })
}

pub(crate) fn to_oid(oid: GitOid) -> Result<Oid, GitError> {
    at("read an object id", Oid::from_bytes(oid.as_bytes()))
}

fn from_oid(oid: Oid) -> Result<GitOid, GitError> {
    let bytes: [u8; 20] = oid
        .as_bytes()
        .try_into()
        .map_err(|_| GitError::Other(format!("{oid} is not a SHA-1 object id")))?;
    Ok(GitOid::from_bytes(bytes))
}

/// What opening the repository found (§6.1 O3).
pub enum Opened {
    /// A repository that opens and whose HEAD reads.
    Existing(Git),
    /// There was none; an empty one was made.
    Created(Git),
    /// A read-only open found none, and makes none.
    Absent,
    /// libgit2 cannot open it or HEAD does not read: damage (§6.4 G4).
    Broken { reason: String },
    /// libgit2 refuses the folder (another account owns it). Never rebuilt: the session runs
    /// without history and says so.
    Unavailable(String),
}

/// HEAD's commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Head {
    pub commit: GitOid,
    pub tree: GitOid,
    /// It carries the `Tc-Frontier` and `Tc-Format` trailers: a commit this app made (§6.2).
    pub app_commit: bool,
}

/// A blob or submodule entry of a tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeEntry {
    pub oid: GitOid,
    /// The git file mode (`0o100644`, `0o100755`, `0o120000`, `0o160000`).
    pub mode: u32,
}

/// The mode of an ordinary file.
pub const MODE_BLOB: u32 = 0o100_644;

/// Who a commit is by and when (the email is always empty, D85).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub name: String,
    pub time_ms: i64,
}

/// The commits being made from objects the database may hold unreferenced (§9.8), shared by
/// every handle on the repository ([`Git::another`]) so a repack on its own thread sees them.
#[derive(Debug, Default)]
struct Making {
    /// The commits between [`Git::commit_begin`] and [`Git::commit_end`] now.
    active: u32,
    /// Every [`Git::commit_begin`] there has been.
    begun: u64,
}

/// What a repack saw of the commits when it started: it prunes only if nothing changed since
/// ([`Git::while_no_commit`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommitMarks {
    active: u32,
    begun: u64,
}

/// See the module documentation.
pub struct Git {
    repo: Repository,
    counters: Arc<Counters>,
    making: Arc<Mutex<Making>>,
}

impl std::fmt::Debug for Git {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Git")
            .field("path", &self.repo.path())
            .finish()
    }
}

/// The values every repository of a project is configured with (§6.1 O3): bytes are never
/// converted, objects and references are flushed, and no reflog keeps the machine's git
/// identity in the project (D85).
const CONFIG: [(&str, bool); 3] = [
    ("core.autocrlf", false),
    ("core.fsyncObjectFiles", true),
    ("core.logAllRefUpdates", false),
];

fn open_repo(outside: &Path) -> Result<Repository, git2::Error> {
    Repository::open_ext(
        outside,
        RepositoryOpenFlags::NO_SEARCH,
        std::iter::empty::<&std::ffi::OsStr>(),
    )
}

impl Git {
    /// Opens the repository in `outside`, making one when there is none (§6.1 O3). A
    /// read-only open never makes, configures or rebuilds anything.
    pub fn open(outside: &Path, read_only: bool, counters: Arc<Counters>, hooks: &Hooks) -> Opened {
        if let Err(Failure::Owner) = hooks.check(Point::GitOpen) {
            return Opened::Unavailable("another account owns the folder".into());
        }
        let git_dir = outside.join(".git");
        if std::fs::symlink_metadata(&git_dir).is_err() {
            if read_only {
                return Opened::Absent;
            }
            return match Self::init(outside, counters) {
                Ok(git) => Opened::Created(git),
                Err(e) if e.is_owner() => Opened::Unavailable(e.to_string()),
                Err(e) => Opened::Broken {
                    reason: e.to_string(),
                },
            };
        }
        let repo = match open_repo(outside) {
            Ok(repo) => repo,
            Err(e) if e.code() == ErrorCode::Owner => {
                return Opened::Unavailable(e.message().to_owned());
            }
            Err(e) => {
                return Opened::Broken {
                    reason: e.message().to_owned(),
                };
            }
        };
        let mut git = Git {
            repo,
            counters,
            making: Arc::default(),
        };
        // HEAD must read, as an unborn branch or as a commit that exists.
        if let Err(e) = git.head() {
            return Opened::Broken {
                reason: e.to_string(),
            };
        }
        if !read_only {
            match git.ensure_config() {
                // The repository caches its configuration: open it again after a change.
                Ok(true) => match open_repo(outside) {
                    Ok(repo) => git.repo = repo,
                    Err(e) => {
                        return Opened::Broken {
                            reason: e.message().to_owned(),
                        };
                    }
                },
                Ok(false) => {}
                Err(e) => {
                    return Opened::Broken {
                        reason: e.to_string(),
                    };
                }
            }
        }
        Opened::Existing(git)
    }

    /// A new, empty repository on [`BRANCH`], configured.
    pub fn init(outside: &Path, counters: Arc<Counters>) -> Result<Git, GitError> {
        let mut options = RepositoryInitOptions::new();
        options
            .initial_head(BRANCH)
            .mkpath(true)
            // No hooks, description or `info/exclude` from the machine's template folder.
            .external_template(false);
        at(
            "make a repository",
            Repository::init_opts(outside, &options),
        )?;
        let git = Git {
            repo: at("open the new repository", open_repo(outside))?,
            counters: counters.clone(),
            making: Arc::default(),
        };
        git.ensure_config()?;
        Ok(Git {
            repo: at("open the new repository", open_repo(outside))?,
            counters,
            making: git.making.clone(),
        })
    }

    /// Sets the configuration of §6.1 O3 where the repository's own file lacks it. Returns
    /// whether anything was written.
    pub fn ensure_config(&self) -> Result<bool, GitError> {
        let config = at("read its configuration", self.repo.config())?;
        let mut local = at(
            "read its own configuration",
            config.open_level(ConfigLevel::Local),
        )?;
        let mut changed = false;
        for (name, value) in CONFIG {
            if local.get_bool(name).ok() != Some(value) {
                at("write its configuration", local.set_bool(name, value))?;
                changed = true;
            }
        }
        Ok(changed)
    }

    /// The value of a boolean in the repository's own configuration.
    pub fn local_bool(&self, name: &str) -> Option<bool> {
        let config = self.repo.config().ok()?;
        config
            .open_level(ConfigLevel::Local)
            .ok()?
            .get_bool(name)
            .ok()
    }

    /// Adds `origin` and the branch the project syncs (`tc.branch`) to a repository made after
    /// the old one was lost (§6.4 G4, §9.6). The address carries no credentials.
    pub fn set_origin(&self, url: &str, branch: &str) -> Result<(), GitError> {
        if self.repo.find_remote("origin").is_err() {
            at("add the remote", self.repo.remote("origin", url))?;
        }
        let config = at("read its configuration", self.repo.config())?;
        let mut local = at(
            "read its own configuration",
            config.open_level(ConfigLevel::Local),
        )?;
        at(
            "write its configuration",
            local.set_str("tc.branch", branch),
        )
    }

    /// Copies the blobs of `wanted` that this repository lacks from the `objects` folder of
    /// another one (a damaged repository set aside, §6.4 G4). The folder is read through an
    /// object database of its own, never attached to this repository: libgit2 treats an
    /// object an attached alternate holds as already stored and writes nothing. A loose blob
    /// streams; a packed one is read whole (libgit2 streams loose objects only). Returns how
    /// many blobs were copied.
    pub fn adopt_blobs(&self, from_objects: &Path, wanted: &[GitOid]) -> Result<u32, GitError> {
        let missing: Vec<Oid> = wanted
            .iter()
            .filter(|oid| !self.has(**oid))
            .map(|oid| to_oid(*oid))
            .collect::<Result<_, _>>()?;
        let Some(from) = from_objects.to_str().filter(|_| !missing.is_empty()) else {
            return Ok(0);
        };
        let source = at("read the damaged repository", git2::Odb::new())?;
        at(
            "read the damaged repository",
            source.add_disk_alternate(from),
        )?;
        let odb = at("open its objects", self.repo.odb())?;
        let mut copied = 0;
        for id in missing {
            if self.adopt_one(&source, &odb, id)? {
                copied += 1;
            }
        }
        Ok(copied)
    }

    /// One blob of `source` into `odb`. `false`: the source has no such blob, or one that does
    /// not hash to its name.
    fn adopt_one(
        &self,
        source: &git2::Odb<'_>,
        odb: &git2::Odb<'_>,
        id: Oid,
    ) -> Result<bool, GitError> {
        if let Ok((mut reader, len, ObjectType::Blob)) = source.reader(id) {
            let mut writer = at("store a file", odb.writer(len, ObjectType::Blob))?;
            io::copy(&mut reader, &mut writer).map_err(|e| GitError::Other(e.to_string()))?;
            let stored = at("store a file", writer.finalize())?;
            self.count_blob(len as u64);
            return Ok(stored == id);
        }
        // A packed object has no stream.
        match source.read(id) {
            Ok(object) if object.kind() == ObjectType::Blob => {
                let stored = at("store a file", odb.write(ObjectType::Blob, object.data()))?;
                self.count_blob(object.len() as u64);
                Ok(stored == id)
            }
            Ok(_) => Ok(false),
            Err(e) if e.code() == ErrorCode::NotFound => Ok(false),
            Err(source) => Err(GitError::Git {
                what: "read the damaged repository",
                source,
            }),
        }
    }

    /// Another handle on the same repository, for another thread.
    pub fn another(&self) -> Result<Git, GitError> {
        let folder = self.repo.workdir().unwrap_or_else(|| self.repo.path());
        Ok(Git {
            repo: at("open the repository", open_repo(folder))?,
            counters: self.counters.clone(),
            making: self.making.clone(),
        })
    }

    fn lock_making(&self) -> MutexGuard<'_, Making> {
        self.making.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A commit is about to be made from a tree that may name objects the database already
    /// holds and nothing references (a blob an abandoned checkpoint stored, older than the
    /// 14 days a repack waits): until [`Git::commit_end`], which follows the move of the
    /// branch, a repack removes no unreachable object (§9.8). It waits for the batch of
    /// removals a repack is making this moment, never for more.
    pub fn commit_begin(&self) {
        let mut making = self.lock_making();
        making.active += 1;
        making.begun += 1;
    }

    /// The commit made since [`Git::commit_begin`] is referenced, or was given up.
    pub fn commit_end(&self) {
        let mut making = self.lock_making();
        making.active = making.active.saturating_sub(1);
    }

    /// Where the commits are now.
    pub(crate) fn commit_marks(&self) -> CommitMarks {
        let making = self.lock_making();
        CommitMarks {
            active: making.active,
            begun: making.begun,
        }
    }

    /// Runs `remove` if no commit is being made, none was begun since `marks`, and none was
    /// being made then; `None` otherwise. No commit begins while it runs: `commit_begin` waits
    /// for it, so an object a commit has freshened or named is never removed by a decision
    /// taken before (§9.8).
    pub(crate) fn while_no_commit<T>(
        &self,
        marks: CommitMarks,
        remove: impl FnOnce() -> T,
    ) -> Option<T> {
        let making = self.lock_making();
        let quiet = marks.active == 0 && making.active == 0 && making.begun == marks.begun;
        quiet.then(remove)
    }

    /// The repository's `.git` folder.
    pub fn git_dir(&self) -> &Path {
        self.repo.path()
    }

    /// HEAD's commit; `None` on a branch with no commit yet.
    pub fn head(&self) -> Result<Option<Head>, GitError> {
        let reference = match self.repo.head() {
            Ok(reference) => reference,
            Err(e) if e.code() == ErrorCode::UnbornBranch || e.code() == ErrorCode::NotFound => {
                return Ok(None);
            }
            Err(source) => {
                return Err(GitError::Git {
                    what: "read HEAD",
                    source,
                });
            }
        };
        let commit = at("read HEAD's commit", reference.peel_to_commit())?;
        let message = commit.message().unwrap_or_default();
        let has = |trailer: &str| {
            message.lines().any(|line| {
                line.strip_prefix(trailer)
                    .is_some_and(|r| r.starts_with(':'))
            })
        };
        Ok(Some(Head {
            commit: from_oid(commit.id())?,
            tree: from_oid(commit.tree_id())?,
            app_commit: has(TRAILER_FRONTIER) && has(TRAILER_FORMAT),
        }))
    }

    /// The commit's message and the commit's parents (first parent first).
    pub fn commit_facts(&self, commit: GitOid) -> Result<(String, Vec<GitOid>), GitError> {
        let commit = at("read a commit", self.repo.find_commit(to_oid(commit)?))?;
        let parents = commit
            .parent_ids()
            .map(from_oid)
            .collect::<Result<Vec<_>, _>>()?;
        Ok((commit.message().unwrap_or_default().to_owned(), parents))
    }

    /// Every blob (and submodule) path of a tree, recursively, with its id and mode.
    pub fn tree_entries(&self, tree: GitOid) -> Result<BTreeMap<String, TreeEntry>, GitError> {
        let tree = at("read a tree", self.repo.find_tree(to_oid(tree)?))?;
        let mut out = BTreeMap::new();
        let mut failed: Option<GitError> = None;
        at(
            "walk a tree",
            tree.walk(TreeWalkMode::PreOrder, |root, entry| {
                if entry.kind() == Some(ObjectType::Tree) {
                    return TreeWalkResult::Ok;
                }
                let name = String::from_utf8_lossy(entry.name_bytes());
                match from_oid(entry.id()) {
                    Ok(oid) => {
                        out.insert(
                            format!("{root}{name}"),
                            TreeEntry {
                                oid,
                                mode: entry.filemode() as u32,
                            },
                        );
                        TreeWalkResult::Ok
                    }
                    Err(e) => {
                        failed = Some(e);
                        TreeWalkResult::Abort
                    }
                }
            }),
        )
        .or_else(|e| failed.take().map_or(Err(e), Err))?;
        Ok(out)
    }

    /// The bytes of the file at `path` in a commit's tree; `None` when the tree has none
    /// there or the object database lacks the blob.
    pub fn read_path(&self, commit: GitOid, path: &str) -> Result<Option<Vec<u8>>, GitError> {
        let commit = at("read a commit", self.repo.find_commit(to_oid(commit)?))?;
        let tree = at("read a tree", commit.tree())?;
        let entry = match tree.get_path(Path::new(path)) {
            Ok(entry) => entry,
            Err(e) if e.code() == ErrorCode::NotFound => return Ok(None),
            Err(source) => {
                return Err(GitError::Git {
                    what: "read a tree",
                    source,
                });
            }
        };
        self.read_blob(from_oid(entry.id())?)
    }

    /// Whether the object database holds `oid`.
    pub fn has(&self, oid: GitOid) -> bool {
        match (to_oid(oid), self.repo.odb()) {
            (Ok(oid), Ok(odb)) => odb.exists(oid),
            _ => false,
        }
    }

    /// The bytes of a blob; `None` when the object database lacks it.
    pub fn read_blob(&self, oid: GitOid) -> Result<Option<Vec<u8>>, GitError> {
        let odb = at("open its objects", self.repo.odb())?;
        match odb.read(to_oid(oid)?) {
            Ok(object) => Ok(Some(object.data().to_vec())),
            Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
            Err(source) => Err(GitError::Git {
                what: "read a file's bytes",
                source,
            }),
        }
    }

    /// Streams a blob to `out` in chunks; `false` when the object database lacks it. A loose
    /// object streams; one in a pack (after a repack) is read whole, because libgit2 streams
    /// loose objects only.
    pub fn copy_blob_to(
        &self,
        oid: GitOid,
        out: &mut dyn FnMut(&[u8]) -> io::Result<()>,
    ) -> Result<bool, GitError> {
        let odb = at("open its objects", self.repo.odb())?;
        let id = to_oid(oid)?;
        match odb.reader(id) {
            Ok((mut reader, _, _)) => {
                let mut buffer = vec![0u8; 64 * 1024];
                loop {
                    let n = io::Read::read(&mut reader, &mut buffer)
                        .map_err(|e| GitError::Other(e.to_string()))?;
                    if n == 0 {
                        return Ok(true);
                    }
                    out(&buffer[..n]).map_err(|e| GitError::Other(e.to_string()))?;
                }
            }
            // A packed object has no stream and libgit2 says "not found" for it too: read it
            // whole, which says whether it is there.
            Err(_) => match self.read_blob(oid)? {
                Some(bytes) => {
                    for chunk in bytes.chunks(64 * 1024) {
                        out(chunk).map_err(|e| GitError::Other(e.to_string()))?;
                    }
                    Ok(true)
                }
                None => Ok(false),
            },
        }
    }

    fn count_blob(&self, bytes: u64) {
        self.counters.add(&self.counters.git_blobs_written, 1);
        self.counters.add(&self.counters.git_blob_bytes, bytes);
    }

    /// Stores `bytes` as a blob (a loose object, flushed).
    pub fn write_blob(&self, bytes: &[u8]) -> Result<GitOid, GitError> {
        let oid = at("store a file", self.repo.blob(bytes))?;
        self.count_blob(bytes.len() as u64);
        from_oid(oid)
    }

    /// The blob id `bytes` would have, without storing anything.
    pub fn blob_id(&self, bytes: &[u8]) -> Result<GitOid, GitError> {
        from_oid(at(
            "hash a file",
            Oid::hash_object(ObjectType::Blob, bytes),
        )?)
    }

    /// The blob of `bytes`, stored only if the object database lacks it (a checkpoint that
    /// finds a file unchanged writes nothing).
    pub fn ensure_blob(&self, bytes: &[u8]) -> Result<GitOid, GitError> {
        let oid = self.blob_id(bytes)?;
        if self.has(oid) {
            // Writing an object the database holds only brings its time up to date (libgit2
            // freshens it, as git does), which keeps a repack from removing an old object
            // nothing references that the commit being made names now (§9.8).
            let odb = at("open its objects", self.repo.odb())?;
            at("keep a stored file", odb.write(ObjectType::Blob, bytes))?;
            Ok(oid)
        } else {
            self.write_blob(bytes)
        }
    }

    /// Every blob below `prefix` (a directory path such as `ingredients/tc/changes`) in a
    /// tree, with its full path; empty when the tree has no such directory.
    pub fn subtree_entries(
        &self,
        tree: GitOid,
        prefix: &str,
    ) -> Result<BTreeMap<String, TreeEntry>, GitError> {
        let root = at("read a tree", self.repo.find_tree(to_oid(tree)?))?;
        let sub = match root.get_path(Path::new(prefix)) {
            Ok(entry) => at("read a tree", self.repo.find_tree(entry.id()))?,
            Err(e) if e.code() == ErrorCode::NotFound => return Ok(BTreeMap::new()),
            Err(source) => {
                return Err(GitError::Git {
                    what: "read a tree",
                    source,
                });
            }
        };
        let mut out = BTreeMap::new();
        let mut failed: Option<GitError> = None;
        at(
            "walk a tree",
            sub.walk(TreeWalkMode::PreOrder, |dir, entry| {
                if entry.kind() == Some(ObjectType::Tree) {
                    return TreeWalkResult::Ok;
                }
                let name = String::from_utf8_lossy(entry.name_bytes());
                match from_oid(entry.id()) {
                    Ok(oid) => {
                        out.insert(
                            format!("{prefix}/{dir}{name}"),
                            TreeEntry {
                                oid,
                                mode: entry.filemode() as u32,
                            },
                        );
                        TreeWalkResult::Ok
                    }
                    Err(e) => {
                        failed = Some(e);
                        TreeWalkResult::Abort
                    }
                }
            }),
        )
        .or_else(|e| failed.take().map_or(Err(e), Err))?;
        Ok(out)
    }

    /// Stores a blob read from `read` until it returns 0, without holding it in memory.
    pub fn write_blob_from(
        &self,
        read: &mut dyn FnMut(&mut [u8]) -> io::Result<usize>,
    ) -> Result<GitOid, GitError> {
        let mut writer = at("store a file", self.repo.blob_writer(None))?;
        let mut buffer = vec![0u8; 64 * 1024];
        let mut total = 0u64;
        loop {
            let n = read(&mut buffer).map_err(|e| GitError::Other(e.to_string()))?;
            if n == 0 {
                break;
            }
            io::Write::write_all(&mut writer, &buffer[..n])
                .map_err(|e| GitError::Other(e.to_string()))?;
            total += n as u64;
        }
        let oid = at("store a file", writer.commit())?;
        self.count_blob(total);
        from_oid(oid)
    }

    /// The tree `base` with `upserts` (path, blob, mode) set and `removes` taken out. Only the
    /// directories on those paths are written again; the rest of `base` is shared. Every
    /// removed path must be in `base`, and no path may be both removed and set.
    pub fn update_tree(
        &self,
        base: Option<GitOid>,
        upserts: &[(String, GitOid, u32)],
        removes: &[String],
    ) -> Result<GitOid, GitError> {
        let base = match base {
            Some(oid) => at("read a tree", self.repo.find_tree(to_oid(oid)?))?,
            None => {
                let empty = at("make a tree", self.repo.treebuilder(None))?;
                let oid = at("make a tree", empty.write())?;
                at("read a tree", self.repo.find_tree(oid))?
            }
        };
        let mut update = TreeUpdateBuilder::new();
        let mut folders: BTreeSet<&str> = BTreeSet::from([""]);
        for path in removes {
            update.remove(path.as_str());
            folders.extend(prefixes(path));
        }
        for (path, oid, mode) in upserts {
            update.upsert(path.as_str(), to_oid(*oid)?, file_mode(*mode));
            folders.extend(prefixes(path));
        }
        let tree = at("build a tree", update.create_updated(&self.repo, &base))?;
        self.counters
            .add(&self.counters.git_trees_written, folders.len() as u64);
        from_oid(tree)
    }

    /// Writes a commit object by hand: libgit2's signature refuses the empty email D85
    /// requires. The name loses `<`, `>` and line ends so it cannot break the object.
    pub fn write_commit(
        &self,
        tree: GitOid,
        parents: &[GitOid],
        author: &Author,
        message: &str,
    ) -> Result<GitOid, GitError> {
        let name: String = author
            .name
            .chars()
            .filter(|c| !matches!(c, '<' | '>' | '\n' | '\r'))
            .collect();
        let name = name.trim();
        let secs = author.time_ms.div_euclid(1000);
        let mut object = format!("tree {}\n", tree.to_hex());
        for parent in parents {
            object.push_str(&format!("parent {}\n", parent.to_hex()));
        }
        object.push_str(&format!("author {name} <> {secs} +0000\n"));
        object.push_str(&format!("committer {name} <> {secs} +0000\n\n"));
        object.push_str(message.trim_end_matches('\n'));
        object.push('\n');
        let odb = at("open its objects", self.repo.odb())?;
        let oid = at(
            "write a commit",
            odb.write(ObjectType::Commit, object.as_bytes()),
        )?;
        self.counters.add(&self.counters.git_commits_written, 1);
        from_oid(oid)
    }

    /// The reference HEAD points at.
    fn branch_ref(&self) -> Result<String, GitError> {
        let head = at("read HEAD", self.repo.find_reference("HEAD"))?;
        match at("read HEAD", head.symbolic_target())? {
            Some(name) => Ok(name.to_owned()),
            None => Err(GitError::Other("HEAD is detached".into())),
        }
    }

    /// Moves the branch HEAD names to `new`, if it is still at `expected` (`None`: the branch
    /// has no commit yet). libgit2 takes the reference's lock file, writes, flushes and
    /// renames (`core.fsyncObjectFiles`).
    pub fn move_branch(&self, expected: Option<GitOid>, new: GitOid) -> Result<(), GitError> {
        let name = self.branch_ref()?;
        let result = match expected {
            Some(expected) => self
                .repo
                .reference_matching(&name, to_oid(new)?, true, to_oid(expected)?, "")
                .map(|_| ()),
            None => self
                .repo
                .reference(&name, to_oid(new)?, false, "")
                .map(|_| ()),
        };
        match result {
            Ok(()) => Ok(()),
            Err(e) if matches!(e.code(), ErrorCode::Modified | ErrorCode::Exists) => {
                Err(GitError::HeadMoved)
            }
            Err(source) => Err(GitError::Git {
                what: "move the branch",
                source,
            }),
        }
    }

    /// Makes `git status` in a terminal show a clean tree: the index holds `tree` (§5.13 C6).
    pub fn write_index(&self, tree: GitOid) -> Result<(), GitError> {
        let tree = at("read a tree", self.repo.find_tree(to_oid(tree)?))?;
        let mut index = at("open the index", self.repo.index())?;
        at("fill the index", index.read_tree(&tree))?;
        at("write the index", index.write())
    }

    /// Points `name` at `commit` (`refs/tc/carried`).
    pub fn set_ref(&self, name: &str, commit: GitOid) -> Result<(), GitError> {
        at(
            "write a reference",
            self.repo.reference(name, to_oid(commit)?, true, ""),
        )
        .map(|_| ())
    }

    /// The commit a reference names, if it exists.
    pub fn find_ref(&self, name: &str) -> Result<Option<GitOid>, GitError> {
        match self.repo.find_reference(name) {
            Ok(reference) => Ok(Some(from_oid(
                at("read a reference", reference.peel_to_commit())?.id(),
            )?)),
            Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
            Err(source) => Err(GitError::Git {
                what: "read a reference",
                source,
            }),
        }
    }

    /// The tree of a commit.
    pub fn tree_of(&self, commit: GitOid) -> Result<GitOid, GitError> {
        let commit = at("read a commit", self.repo.find_commit(to_oid(commit)?))?;
        from_oid(commit.tree_id())
    }

    /// How many loose objects the repository holds, and their bytes (§9.8).
    pub fn loose_objects(&self) -> (u64, u64) {
        let mut count = 0;
        let mut bytes = 0;
        let Ok(folders) = std::fs::read_dir(self.repo.path().join("objects")) else {
            return (0, 0);
        };
        for folder in folders.flatten() {
            let name = folder.file_name();
            let name = name.to_string_lossy();
            if name.len() != 2 || !name.bytes().all(|b| b.is_ascii_hexdigit()) {
                continue;
            }
            let Ok(files) = std::fs::read_dir(folder.path()) else {
                continue;
            };
            for file in files.flatten() {
                count += 1;
                bytes += file.metadata().map_or(0, |m| m.len());
            }
        }
        (count, bytes)
    }

    /// The underlying repository, for the rebuild and the repack of this crate.
    pub(crate) fn repo(&self) -> &Repository {
        &self.repo
    }

    /// The store's counters, which this repository's work is counted in.
    pub(crate) fn counters(&self) -> &Counters {
        &self.counters
    }
}

/// Every directory above a path: `a/b/c` gives `a` and `a/b`.
fn prefixes(path: &str) -> impl Iterator<Item = &str> {
    path.match_indices('/').map(move |(i, _)| &path[..i])
}

fn file_mode(mode: u32) -> FileMode {
    match mode {
        0o100_755 => FileMode::BlobExecutable,
        0o120_000 => FileMode::Link,
        0o160_000 => FileMode::Commit,
        _ => FileMode::Blob,
    }
}

/// O2: removes the lock files and half-written packs a stopped process left under `.git`
/// (`*.lock`, `objects/pack/tmp_*`). Safe because the project lock is ours and the app is
/// single-instance. Returns how many were removed.
pub fn remove_stale_locks(outside: &Path) -> io::Result<u32> {
    let git = outside.join(".git");
    let mut removed = 0;
    let mut stack: Vec<PathBuf> = vec![git.clone()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                // The loose objects are many and never locked.
                let loose = dir.ends_with("objects") && name.len() == 2;
                if !loose {
                    stack.push(path);
                }
            } else if name.ends_with(".lock")
                || (name.starts_with("tmp_") && dir.ends_with("objects/pack"))
            {
                std::fs::remove_file(&path)?;
                removed += 1;
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn fresh() -> (tempfile::TempDir, Git) {
        let dir = tempfile::tempdir().unwrap();
        let git = Git::init(dir.path(), Arc::default()).unwrap();
        (dir, git)
    }

    fn author(secs: i64) -> Author {
        Author {
            name: "translationCore user".into(),
            time_ms: secs * 1000,
        }
    }

    /// Makes the loose file of `oid` look `days` old.
    fn age(git: &Git, oid: GitOid, days: u64) {
        let hex = oid.to_hex();
        let path = git
            .git_dir()
            .join("objects")
            .join(&hex[..2])
            .join(&hex[2..]);
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        std::fs::set_permissions(&path, permissions).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(
                std::time::SystemTime::now() - std::time::Duration::from_secs(days * 86_400),
            )
            .unwrap();
    }

    fn loose_age(git: &Git, oid: GitOid) -> std::time::Duration {
        let hex = oid.to_hex();
        let path = git
            .git_dir()
            .join("objects")
            .join(&hex[..2])
            .join(&hex[2..]);
        std::fs::metadata(path)
            .unwrap()
            .modified()
            .unwrap()
            .elapsed()
            .unwrap_or_default()
    }

    /// A blob an abandoned checkpoint stored long ago, named again by the commit being made,
    /// is brought up to date: a repack that decided it was unreachable earlier does not remove
    /// it before the branch has moved (§9.8).
    #[test]
    fn a_blob_a_new_commit_names_again_is_kept_young() {
        let (_dir, git) = fresh();
        let oid = git
            .write_blob(b"stored by an abandoned checkpoint")
            .unwrap();
        age(&git, oid, 20);
        assert!(loose_age(&git, oid) > std::time::Duration::from_secs(19 * 86_400));
        assert_eq!(
            git.ensure_blob(b"stored by an abandoned checkpoint")
                .unwrap(),
            oid
        );
        assert!(
            loose_age(&git, oid) < std::time::Duration::from_secs(3600),
            "the object is as old as before"
        );
        assert_eq!(
            git.counters.read().git_blobs_written,
            1,
            "nothing was written twice"
        );
    }

    /// A repack removes no unreachable object while a commit is being made, or when one was
    /// made during its run, whatever the objects' age (§9.8).
    #[test]
    fn a_repack_prunes_nothing_while_a_commit_is_being_made() {
        let (_dir, git) = fresh();
        let orphan = git.write_blob(b"old, unreferenced").unwrap();
        age(&git, orphan, 20);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let repack = || {
            git.repack(
                std::time::SystemTime::now(),
                &Hooks::default(),
                &stop,
                crate::repack::MAX_RAW,
            )
            .unwrap()
        };
        git.commit_begin();
        assert_eq!(repack().pruned, 0, "a commit is being made");
        assert!(git.has(orphan));
        git.commit_end();
        assert_eq!(
            repack().pruned,
            1,
            "the commit is referenced; nothing names it"
        );
        assert!(!git.has(orphan));
    }

    /// Unreachable objects old enough to prune, each stored and aged, and a repack that runs
    /// `between` before each batch of removals (with the number removed so far). The
    /// repack's result, the objects and what `between` did are returned.
    fn orphans_and_a_repack(
        count: usize,
        between: impl Fn(&Git, &[(Vec<u8>, GitOid)], u64) + Send + Sync + 'static,
    ) -> (Git, Vec<(Vec<u8>, GitOid)>, crate::repack::Repacked) {
        let (dir, git) = fresh();
        // The folder lives as long as the handle that points into it.
        std::mem::forget(dir);
        let orphans: Vec<(Vec<u8>, GitOid)> = (0..count)
            .map(|i| {
                let bytes = format!("an orphan, number {i}").into_bytes();
                let oid = git.write_blob(&bytes).unwrap();
                age(&git, oid, 20);
                (bytes, oid)
            })
            .collect();
        let hooks = Hooks::default();
        let (other, seen) = (git.another().unwrap(), orphans.clone());
        let other = std::sync::Mutex::new(other);
        hooks.at_prune_batch(move |pruned| {
            let other = other.lock().unwrap();
            between(&other, &seen, pruned);
        });
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let done = git
            .repack(
                std::time::SystemTime::now(),
                &hooks,
                &stop,
                crate::repack::MAX_RAW,
            )
            .unwrap();
        (git, orphans, done)
    }

    /// §9.8: a checkpoint that begins its commit while a repack is removing the objects it
    /// found unreachable may name an old one that way (`entry.blob` of an earlier checkpoint
    /// that was abandoned): no object it names is removed, whatever the repack decided
    /// earlier. The batch before it was removed before the commit began, and the commit's tree
    /// build stores those again.
    #[test]
    fn a_commit_that_begins_during_the_removals_loses_no_object() {
        let named = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (git, orphans, done) = {
            let named = named.clone();
            orphans_and_a_repack(300, move |other, orphans, pruned| {
                if pruned == 0 || !named.lock().unwrap().is_empty() {
                    return;
                }
                other.commit_begin();
                let mut named = named.lock().unwrap();
                named.extend(
                    orphans
                        .iter()
                        .map(|(_, oid)| *oid)
                        .filter(|oid| other.has(*oid)),
                );
            })
        };
        let named = named.lock().unwrap().clone();
        assert!(
            !named.is_empty(),
            "the commit began between two batches of removals"
        );
        assert_eq!(done.pruned as usize, orphans.len() - named.len());
        for oid in &named {
            assert!(git.has(*oid), "the commit names an object that was removed");
        }
        // The commit is referenced or given up: the next run may prune what nothing names.
        git.commit_end();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let later = git
            .repack(
                std::time::SystemTime::now(),
                &Hooks::default(),
                &stop,
                crate::repack::MAX_RAW,
            )
            .unwrap();
        assert_eq!(later.pruned as usize, named.len());
    }

    /// No commit begins while a batch of removals runs: `commit_begin` waits for it, so an
    /// object the commit then names is never one the batch was about to remove.
    #[test]
    fn a_commit_cannot_begin_while_a_batch_of_removals_runs() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let (_dir, git) = fresh();
        let other = git.another().unwrap();
        let begun = AtomicBool::new(false);
        let marks = git.commit_marks();
        std::thread::scope(|scope| {
            let begun = &begun;
            let removed = git.while_no_commit(marks, move || {
                scope.spawn(move || {
                    other.commit_begin();
                    begun.store(true, Ordering::SeqCst);
                });
                std::thread::sleep(std::time::Duration::from_millis(200));
                begun.load(Ordering::SeqCst)
            });
            assert_eq!(removed, Some(false), "a commit began during the removals");
        });
        assert!(begun.load(Ordering::SeqCst), "the commit began afterwards");
        // A commit was begun since the marks were taken: nothing is removed.
        assert!(git.while_no_commit(marks, || ()).is_none());
        git.commit_end();
        assert!(git.while_no_commit(marks, || ()).is_none());
        assert!(git.while_no_commit(git.commit_marks(), || ()).is_some());
    }

    /// Whatever a repack decided, an object written again since it listed the loose files
    /// (a blob a command stored outside any commit) is as young as the write made it, and
    /// stays.
    #[test]
    fn an_object_written_again_since_the_listing_is_not_pruned() {
        let kept = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (git, orphans, done) = {
            let kept = kept.clone();
            orphans_and_a_repack(200, move |other, orphans, pruned| {
                if pruned == 0 || !kept.lock().unwrap().is_empty() {
                    return;
                }
                let mut kept = kept.lock().unwrap();
                for (bytes, oid) in orphans {
                    if other.has(*oid) {
                        other.ensure_blob(bytes).unwrap();
                        kept.push(*oid);
                    }
                }
            })
        };
        let kept = kept.lock().unwrap().clone();
        assert!(!kept.is_empty());
        assert_eq!(done.pruned as usize, orphans.len() - kept.len());
        assert!(kept.iter().all(|oid| git.has(*oid)));
    }

    /// A repository is made for bytes: nothing is converted, objects and references are
    /// flushed, and no reflog is kept.
    #[test]
    fn a_new_repository_is_configured_for_bytes() {
        let (dir, git) = fresh();
        assert_eq!(git.local_bool("core.autocrlf"), Some(false));
        assert_eq!(git.local_bool("core.fsyncObjectFiles"), Some(true));
        assert_eq!(git.local_bool("core.logAllRefUpdates"), Some(false));
        assert!(git.head().unwrap().is_none(), "an unborn branch");
        // A second open changes nothing in the configuration file.
        let before = std::fs::read(dir.path().join(".git/config")).unwrap();
        let hooks = Hooks::default();
        let Opened::Existing(again) = Git::open(dir.path(), false, Arc::default(), &hooks) else {
            panic!("the repository opens");
        };
        assert_eq!(
            std::fs::read(dir.path().join(".git/config")).unwrap(),
            before
        );
        drop(again);
    }

    #[test]
    fn a_blob_round_trips_whole_and_in_chunks() {
        let (_dir, git) = fresh();
        let bytes: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let oid = git.write_blob(&bytes).unwrap();
        assert_eq!(git.read_blob(oid).unwrap(), Some(bytes.clone()));
        let mut copied = Vec::new();
        assert!(
            git.copy_blob_to(oid, &mut |chunk| {
                copied.extend_from_slice(chunk);
                Ok(())
            })
            .unwrap()
        );
        assert_eq!(copied, bytes);
        // The same bytes streamed in are the same object.
        let mut at = 0;
        let streamed = git
            .write_blob_from(&mut |buf| {
                let n = (bytes.len() - at).min(buf.len()).min(1000);
                buf[..n].copy_from_slice(&bytes[at..at + n]);
                at += n;
                Ok(n)
            })
            .unwrap();
        assert_eq!(streamed, oid);
        let missing = GitOid::from_bytes([7; 20]);
        assert!(!git.has(missing));
        assert_eq!(git.read_blob(missing).unwrap(), None);
    }

    /// A blob in a pack streams too (libgit2 streams loose objects only).
    #[test]
    fn a_blob_in_a_pack_is_copied_whole() {
        let (_dir, git) = fresh();
        let oid = git.write_blob(b"in a pack").unwrap();
        let repo = git.repo();
        let mut builder = repo.packbuilder().unwrap();
        builder.insert_object(to_oid(oid).unwrap(), None).unwrap();
        builder
            .write(&repo.path().join("objects/pack"), 0o644)
            .unwrap();
        let loose = repo
            .path()
            .join("objects")
            .join(&oid.to_hex()[..2])
            .join(&oid.to_hex()[2..]);
        std::fs::remove_file(loose).unwrap();
        repo.odb().unwrap().refresh().unwrap();
        let mut copied = Vec::new();
        assert!(
            git.copy_blob_to(oid, &mut |c| {
                copied.extend_from_slice(c);
                Ok(())
            })
            .unwrap()
        );
        assert_eq!(copied, b"in a pack");
    }

    /// `update_tree` shares what it does not change and writes only the directories on the
    /// paths it touches.
    #[test]
    fn a_tree_update_writes_only_the_changed_directories() {
        let (_dir, git) = fresh();
        let counters = git.counters.clone();
        let a = git.write_blob(b"a").unwrap();
        let b = git.write_blob(b"b").unwrap();
        let c = git.write_blob(b"c").unwrap();
        let first = git
            .update_tree(
                None,
                &[
                    ("ingredients/x/1.txt".into(), a, MODE_BLOB),
                    ("ingredients/y/2.txt".into(), b, MODE_BLOB),
                    ("metadata.json".into(), a, MODE_BLOB),
                ],
                &[],
            )
            .unwrap();
        let before = counters.read();
        let second = git
            .update_tree(
                Some(first),
                &[("ingredients/x/1.txt".into(), c, MODE_BLOB)],
                &["ingredients/y/2.txt".into()],
            )
            .unwrap();
        let written = counters.read().since(&before).git_trees_written;
        assert_eq!(
            written, 4,
            "the root, ingredients, ingredients/x and ingredients/y"
        );
        let entries = git.tree_entries(second).unwrap();
        assert_eq!(
            entries.keys().map(String::as_str).collect::<Vec<_>>(),
            ["ingredients/x/1.txt", "metadata.json"]
        );
        assert_eq!(entries["ingredients/x/1.txt"].oid, c);
        assert_eq!(entries["metadata.json"].oid, a, "shared, not rewritten");
    }

    /// The commit object is written by hand with an empty email, and libgit2 reads it.
    #[test]
    fn a_commit_has_no_email_and_parses() {
        let (_dir, git) = fresh();
        let blob = git.write_blob(b"x").unwrap();
        let tree = git
            .update_tree(None, &[("f".into(), blob, MODE_BLOB)], &[])
            .unwrap();
        let message =
            "Checkpoint, created: TIT (tC)\n\nTc-Device: d1\nTc-Format: 1.2\nTc-Frontier: 00\n";
        let commit = git
            .write_commit(tree, &[], &author(1_760_000_000), message)
            .unwrap();
        git.move_branch(None, commit).unwrap();
        let head = git.head().unwrap().unwrap();
        assert_eq!(head.commit, commit);
        assert_eq!(head.tree, tree);
        assert!(head.app_commit);
        let found = git.repo().find_commit(to_oid(commit).unwrap()).unwrap();
        assert_eq!(found.author().email().unwrap(), "");
        assert_eq!(found.author().name().unwrap(), "translationCore user");
        assert_eq!(found.time().seconds(), 1_760_000_000);
        assert_eq!(found.message().unwrap(), message);
        // A second commit on top, and the branch only moves from where it was read.
        let child = git
            .write_commit(tree, &[commit], &author(1_760_000_001), "second\n")
            .unwrap();
        assert!(matches!(
            git.move_branch(None, child),
            Err(GitError::HeadMoved)
        ));
        assert!(matches!(
            git.move_branch(Some(child), child),
            Err(GitError::HeadMoved)
        ));
        git.move_branch(Some(commit), child).unwrap();
        assert_eq!(git.head().unwrap().unwrap().commit, child);
        assert!(!git.head().unwrap().unwrap().app_commit);
        git.write_index(tree).unwrap();
        // No reflog keeps the machine's identity in the project.
        assert!(!git.git_dir().join("logs").exists());
    }

    /// §6.4 G4: blobs a lost repository held, loose or packed, are copied into a new one from
    /// its objects, and only into the new repository's own objects.
    #[test]
    fn blobs_are_adopted_from_a_repositorys_objects() {
        let (old_dir, old) = fresh();
        let loose = old.write_blob(b"a loose recording").unwrap();
        let packed = old.write_blob(b"a packed recording").unwrap();
        let mut builder = old.repo().packbuilder().unwrap();
        builder
            .insert_object(to_oid(packed).unwrap(), None)
            .unwrap();
        builder
            .write(&old_dir.path().join(".git/objects/pack"), 0o644)
            .unwrap();
        let hex = packed.to_hex();
        std::fs::remove_file(
            old_dir
                .path()
                .join(".git/objects")
                .join(&hex[..2])
                .join(&hex[2..]),
        )
        .unwrap();
        let (new_dir, new) = fresh();
        let unknown = GitOid::from_bytes([9; 20]);
        assert!(!new.has(loose) && !new.has(packed));
        let copied = new
            .adopt_blobs(
                &old_dir.path().join(".git/objects"),
                &[loose, packed, unknown],
            )
            .unwrap();
        assert_eq!(copied, 2, "the unknown blob is not there to copy");
        for (oid, bytes) in [
            (loose, &b"a loose recording"[..]),
            (packed, b"a packed recording"),
        ] {
            let hex = oid.to_hex();
            assert!(
                new_dir
                    .path()
                    .join(".git/objects")
                    .join(&hex[..2])
                    .join(&hex[2..])
                    .exists(),
                "written into the new repository's own objects"
            );
            assert_eq!(new.read_blob(oid).unwrap(), Some(bytes.to_vec()));
        }
        // Nothing is left to copy the second time.
        assert_eq!(
            new.adopt_blobs(&old_dir.path().join(".git/objects"), &[loose, packed])
                .unwrap(),
            0
        );
    }

    /// A name cannot break the object it is in.
    #[test]
    fn a_name_with_angle_brackets_and_line_ends_is_cleaned() {
        let (_dir, git) = fresh();
        let tree = git.update_tree(None, &[], &[]).unwrap();
        let weird = Author {
            name: "A <b@c>\nparent 0000".into(),
            time_ms: 5_000,
        };
        let commit = git.write_commit(tree, &[], &weird, "m").unwrap();
        let found = git.repo().find_commit(to_oid(commit).unwrap()).unwrap();
        assert_eq!(found.parent_count(), 0);
        assert_eq!(found.author().email().unwrap(), "");
    }

    /// O2 removes the lock files and half-written packs a stopped process left, and nothing
    /// else.
    #[test]
    fn stale_locks_and_temp_packs_are_removed() {
        let (dir, git) = fresh();
        let blob = git.write_blob(b"keep").unwrap();
        let git_dir = dir.path().join(".git");
        for rel in [
            "config.lock",
            "index.lock",
            "HEAD.lock",
            "refs/heads/main.lock",
            "objects/pack/tmp_pack_x",
        ] {
            std::fs::create_dir_all(git_dir.join(rel).parent().unwrap()).unwrap();
            std::fs::write(git_dir.join(rel), b"x").unwrap();
        }
        std::fs::write(git_dir.join("objects/pack/pack-keep.pack"), b"x").unwrap();
        assert_eq!(remove_stale_locks(dir.path()).unwrap(), 5);
        assert!(git_dir.join("objects/pack/pack-keep.pack").exists());
        assert!(git.has(blob));
        assert_eq!(remove_stale_locks(dir.path()).unwrap(), 0);
    }

    /// A `.git` that does not open, or whose HEAD names a commit that is gone, is damage; a
    /// read-only open finds nothing and makes nothing.
    #[test]
    fn a_damaged_repository_is_reported_and_a_read_only_open_makes_none() {
        let dir = tempfile::tempdir().unwrap();
        let hooks = Hooks::default();
        assert!(matches!(
            Git::open(dir.path(), true, Arc::default(), &hooks),
            Opened::Absent
        ));
        assert!(!dir.path().join(".git").exists());
        let (_keep, git) = fresh();
        let blob = git.write_blob(b"x").unwrap();
        let tree = git
            .update_tree(None, &[("f".into(), blob, MODE_BLOB)], &[])
            .unwrap();
        let commit = git.write_commit(tree, &[], &author(1), "m").unwrap();
        git.move_branch(None, commit).unwrap();
        let damaged = git.git_dir().to_owned();
        let hex = commit.to_hex();
        std::fs::remove_file(damaged.join("objects").join(&hex[..2]).join(&hex[2..])).unwrap();
        let outside = damaged.parent().unwrap().to_owned();
        assert!(matches!(
            Git::open(&outside, false, Arc::default(), &hooks),
            Opened::Broken { .. }
        ));
        std::fs::write(damaged.join("HEAD"), b"garbage").unwrap();
        assert!(matches!(
            Git::open(&outside, true, Arc::default(), &hooks),
            Opened::Broken { .. }
        ));
    }
}
