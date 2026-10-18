//! The project's local repository (`docs/STORAGE.md` §6.1 O2 and O3, §6.4 G4): made at the
//! first open, configured for bytes, cleaned of a stopped process's lock files, never touched
//! by a read-only open, and never rebuilt for a refusal that is not damage.

use crate::common::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tc_journal::fold::Notice;
use tc_store::fault::FaultFs;
use tc_store::{Failure, Hooks, OpenOptions, Point};

/// The repository of the project at `folder`, as git itself reads it.
pub fn repo_of(fs: &FaultFs, folder: &Path) -> git2::Repository {
    use tc_store::Vfs;
    git2::Repository::open(fs.outside_dir(folder).unwrap()).unwrap()
}

fn git_dir(fs: &FaultFs, folder: &Path) -> PathBuf {
    use tc_store::Vfs;
    fs.outside_dir(folder).unwrap().join(".git")
}

/// Every file under `dir`, by relative path, with its bytes.
fn tree_of_files(dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut out = std::collections::BTreeMap::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap().flatten() {
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                stack.push(path);
            } else {
                let rel = path
                    .strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.insert(rel, std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

/// A project made by the create path has a repository that converts no bytes, flushes
/// what it writes and keeps no reflog (so the machine's git identity stays out of it, D85).
#[test]
fn a_new_project_has_a_repository_configured_for_bytes() {
    let (rig, folder) = titus();
    let repo = repo_of(&rig.fs, &folder);
    let config = repo
        .config()
        .unwrap()
        .open_level(git2::ConfigLevel::Local)
        .unwrap();
    assert!(!config.get_bool("core.autocrlf").unwrap());
    assert!(config.get_bool("core.fsyncObjectFiles").unwrap());
    assert!(!config.get_bool("core.logAllRefUpdates").unwrap());
    let head = repo.find_reference("HEAD").unwrap();
    assert_eq!(head.symbolic_target().unwrap(), Some("refs/heads/main"));
    assert!(!git_dir(&rig.fs, &folder).join("logs").exists());
}

/// O2: the lock files and the half-written pack a stopped process left are removed at the
/// next open (L87), and the repository works.
#[test]
fn an_open_removes_stale_git_lock_files() {
    let (rig, folder) = titus();
    let dir = git_dir(&rig.fs, &folder);
    let stale = [
        "config.lock",
        "index.lock",
        "refs/heads/main.lock",
        "objects/pack/tmp_pack_stopped",
    ];
    for rel in stale {
        std::fs::create_dir_all(dir.join(rel).parent().unwrap()).unwrap();
        std::fs::write(dir.join(rel), b"left by a stopped process").unwrap();
    }
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    for rel in stale {
        assert!(!dir.join(rel).exists(), "{rel} was left");
    }
    handle.close().unwrap();
}

/// L87: a read-only open reads only; it can never delete a lock file another process is
/// using, change the configuration or rebuild the repository.
#[test]
fn a_read_only_open_writes_nothing_in_git() {
    let (rig, folder) = titus();
    let dir = git_dir(&rig.fs, &folder);
    // The open on this computer holds the project; the other computer's open is read-only.
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    std::fs::write(dir.join("index.lock"), b"another process").unwrap();
    // A configuration a person's tool changed is not set again by a reader.
    let mut config = git2::Config::open(&dir.join("config")).unwrap();
    config.set_bool("core.autocrlf", true).unwrap();
    let before = tree_of_files(&dir);
    let other = rig_as(rig.fs.other_machine(), "/app-desktop", "host-2");
    let reader = other.store.open(&folder, &OpenOptions::default()).unwrap();
    reader.wait_idle();
    assert!(matches!(
        reader.facts().mode,
        tc_store::Mode::ReadOnly(tc_store::ReadOnlyReason::OpenOnAnotherComputer { .. })
    ));
    reader.close().unwrap();
    assert_eq!(tree_of_files(&dir), before);
    handle.close().unwrap();
}

/// L87 (G4's guard): libgit2 refuses a folder another account owns. That is not damage: the
/// repository is neither rebuilt nor moved, the person is told, and the session works
/// without history.
#[test]
fn a_repository_another_account_owns_is_never_rebuilt() {
    let (rig, folder) = titus();
    handle_closed(&rig, &folder);
    let before = tree_of_files(&git_dir(&rig.fs, &folder));
    let hooks = Arc::new(Hooks::default());
    hooks.fail(Point::GitOpen, Failure::Owner);
    let other = rig_with_hooks(rig.fs.copy(), hooks.clone());
    // `copy` made a mirror of the same files for the copy of the filesystem.
    let handle = other.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let notices: Vec<Notice> = handle.notices().into_iter().map(|n| n.notice).collect();
    assert!(
        notices
            .iter()
            .any(|n| matches!(n, Notice::HistoryUnavailable { .. })),
        "{notices:?}"
    );
    assert!(
        !notices
            .iter()
            .any(|n| matches!(n, Notice::HistoryRebuilt { .. }))
    );
    // The session still saves.
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    handle.close().unwrap();
    assert_eq!(tree_of_files(&git_dir(&other.fs, &folder)), before);
    assert!(!folder.join(".tc/broken-git").exists());
}

fn handle_closed(rig: &Rig, folder: &Path) {
    let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle.close().unwrap();
}

/// G4: a repository that does not open is kept in `.tc/` and a new one takes its place; the
/// project loses nothing, because the packs never depended on git.
#[test]
fn a_damaged_repository_is_kept_aside_and_replaced() {
    let (rig, folder) = titus();
    handle_closed(&rig, &folder);
    let dir = git_dir(&rig.fs, &folder);
    std::fs::write(dir.join("HEAD"), b"garbage that is no reference").unwrap();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let notices: Vec<Notice> = handle.notices().into_iter().map(|n| n.notice).collect();
    let kept = notices.iter().find_map(|n| match n {
        Notice::HistoryRebuilt { broken } => Some(broken.to_string()),
        _ => None,
    });
    let kept = kept.unwrap_or_else(|| panic!("no HistoryRebuilt in {notices:?}"));
    let outside = dir.parent().unwrap();
    assert_eq!(
        std::fs::read(outside.join(&kept).join("HEAD")).unwrap(),
        b"garbage that is no reference"
    );
    let repo = repo_of(&rig.fs, &folder);
    assert_eq!(
        repo.head().err().map(|e| e.code()),
        Some(git2::ErrorCode::UnbornBranch)
    );
    handle.close().unwrap();
}
