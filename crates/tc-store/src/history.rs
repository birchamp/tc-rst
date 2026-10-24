//! Opening the project's repository (`docs/STORAGE.md` §6.1 O2 and O3, §6.4 G4).
//!
//! A repository that cannot be opened is damage and is replaced; one libgit2 refuses because
//! another account owns the folder is not damage, is never touched, and leaves the session
//! without history. Neither ever stops the open: the packs are the source of truth, and
//! history is a backup and a transport.

use crate::appdata::utc_stamp;
use crate::counters::Counters;
use crate::error::StoreError;
use crate::git::{Git, Opened, remove_stale_locks};
use crate::open::Env;
use crate::vfs::Vfs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tc_journal::fold::Notice;

/// What the open found of the repository.
pub(crate) struct History {
    /// The repository, when this session has one.
    pub git: Option<Git>,
    pub notices: Vec<(Notice, Option<String>)>,
    /// A damaged repository was replaced, and set aside here (absolute): the first commit of
    /// the new one is the project as it is (§6.4 G4).
    pub broken: Option<PathBuf>,
}

/// O2's lock files and O3: opens, makes or replaces the repository of the project at `path`.
/// An error is a write that failed (a full disk): the open ends its writes (§6.1). A
/// read-only open (`writable` false) only reads.
pub(crate) fn open(
    env: &Env,
    vfs: &dyn Vfs,
    path: &Path,
    writable: bool,
    counters: &Arc<Counters>,
) -> Result<History, StoreError> {
    let mut notices = Vec::new();
    let unavailable = |reason: String| History {
        git: None,
        notices: vec![(
            Notice::HistoryUnavailable {
                reason: reason.into(),
            },
            None,
        )],
        broken: None,
    };
    let outside = match vfs.outside_dir(path) {
        Ok(outside) => outside,
        Err(e) => return Ok(unavailable(e.to_string())),
    };
    if writable {
        remove_stale_locks(&outside).map_err(|e| StoreError::io("clean", &outside, e))?;
    }
    let mut broken = None;
    let git = match Git::open(&outside, !writable, counters.clone(), &env.hooks) {
        Opened::Existing(git) | Opened::Created(git) => Some(git),
        Opened::Absent => None,
        Opened::Unavailable(reason) => return Ok(unavailable(reason)),
        Opened::Broken { .. } if !writable => None,
        Opened::Broken { reason } => {
            let stamp = utc_stamp(env.clock.now());
            match rebuild(&outside, &stamp, counters) {
                Ok((git, kept, aside)) => {
                    notices.push((
                        Notice::HistoryRebuilt {
                            broken: kept.into(),
                        },
                        None,
                    ));
                    broken = Some(aside);
                    restore_origin(&git, vfs, path);
                    Some(git)
                }
                Err(e) => return Ok(unavailable(format!("{reason}; {e}"))),
            }
        }
    };
    Ok(History {
        git,
        notices,
        broken,
    })
}

/// G4: `origin` and `tc.branch` come back from `.tc/sync.json` when it reads (§3.6); the
/// address in it carries no credentials. A project that was never shared has none, and a
/// file that does not read leaves the new repository as it is.
fn restore_origin(git: &Git, vfs: &dyn Vfs, path: &Path) {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SyncJson {
        remote_url: String,
        branch: String,
    }
    let Ok(bytes) = vfs.read(&path.join(".tc").join("sync.json")) else {
        return;
    };
    if let Ok(sync) = serde_json::from_slice::<SyncJson>(&bytes)
        && !sync.remote_url.is_empty()
        && !sync.branch.is_empty()
    {
        let _ = git.set_origin(&sync.remote_url, &sync.branch);
    }
}

/// G4: the damaged `.git` is kept in `.tc/broken-git-<utc>` and a new repository takes its
/// place. Returns it, where the damaged one is relative to the project folder, and where it
/// is on the disk.
#[expect(
    clippy::disallowed_methods,
    reason = "moves a broken .git aside into .tc (G4 rebuild): git's own files, kept, not a project file"
)]
fn rebuild(
    outside: &Path,
    stamp: &str,
    counters: &Arc<Counters>,
) -> Result<(Git, String, PathBuf), StoreError> {
    let kept = format!(".tc/broken-git-{stamp}");
    let to = outside.join(&kept);
    let tc = outside.join(".tc");
    std::fs::create_dir_all(&tc).map_err(|e| StoreError::io("create", &tc, e))?;
    let from = outside.join(".git");
    // A name taken by an earlier rebuild in the same second: the damaged repository keeps
    // the next free name.
    let mut to = to;
    let mut n = 2;
    while to.exists() {
        to = outside.join(format!("{kept}-{n}"));
        n += 1;
    }
    std::fs::rename(&from, &to).map_err(|e| StoreError::io("move aside", &from, e))?;
    let git = Git::init(outside, counters.clone())?;
    let name = to
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok((git, format!(".tc/{name}"), to))
}
