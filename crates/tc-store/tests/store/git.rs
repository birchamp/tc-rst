//! The project's local repository (`docs/STORAGE.md` §6.1 O2 and O3, §6.4 G4): made at the
//! first open, configured for bytes, cleaned of a stopped process's lock files, never touched
//! by a read-only open, and never rebuilt for a refusal that is not damage.

use crate::common::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tc_journal::fold::Notice;
use tc_store::fault::FaultFs;
use tc_store::{Failure, Hooks, OpenOptions, Point};

fn git_dir(fs: &FaultFs, folder: &Path) -> PathBuf {
    git_dir_of(fs, folder)
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
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle.close().unwrap();
    let fs = rig.fs.clone();
    drop(rig);
    std::fs::write(dir.join("index.lock"), b"another process").unwrap();
    // A configuration a person's tool changed is not set again by a reader.
    let mut config = git2::Config::open(&dir.join("config")).unwrap();
    config.set_bool("core.autocrlf", true).unwrap();
    let before = tree_of_files(&dir);
    // The projects folder is found to be cloud-synced: the next open is read-only.
    let cloud = cloud_synced_rig(fs);
    let reader = cloud.store.open(&folder, &OpenOptions::default()).unwrap();
    reader.wait_idle();
    assert!(matches!(
        reader.facts().mode,
        tc_store::Mode::ReadOnly(tc_store::ReadOnlyReason::UnsafeFolder(_))
    ));
    reader.close().unwrap();
    assert_eq!(tree_of_files(&dir), before);
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

/// G4, L84: a repository that does not open is kept in `.tc/` and a new one takes its place,
/// without losing the carried blobs. Version 1 of a
/// file was committed and version 2 recorded after it; the open copies version 1 from the
/// repository it set aside and stores version 2 again from the file in the folder (its object
/// was lost with the damage), the first commit lists the file, the address of Door43 comes
/// back from `.tc/sync.json`, and *Restore* of version 1 is accepted.
#[test]
fn crash_g4_rebuild_keeps_carried_blobs() {
    use tc_journal::command::Command;
    use tc_journal::{RecordKey, RepoPath};

    const SONG: &str = "ingredients/audio/song.mp3";
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let first = b"my first recording of the song".to_vec();
    let second = b"my second recording of the song, longer".to_vec();
    for (n, bytes) in [&first, &second].into_iter().enumerate() {
        rig.fs.put_unseen(&folder.join(SONG), bytes, None);
        handle.focus().wait().unwrap();
        handle.wait_idle();
        if n == 0 {
            // Version 1 is committed; version 2 is recorded after it.
            handle
                .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
                .wait()
                .unwrap();
            handle.wait_idle();
        }
    }
    handle.close().unwrap();
    let v1 = git2::Oid::hash_object(git2::ObjectType::Blob, &first).unwrap();
    let v2 = git2::Oid::hash_object(git2::ObjectType::Blob, &second).unwrap();

    // The damage: HEAD no longer reads, and version 2's object is gone with it.
    let dir = git_dir(&rig.fs, &folder);
    let loose = |oid: git2::Oid| {
        let hex = oid.to_string();
        dir.join("objects").join(&hex[..2]).join(&hex[2..])
    };
    assert!(loose(v1).exists() && loose(v2).exists());
    std::fs::write(dir.join("HEAD"), b"garbage that is no reference").unwrap();
    std::fs::remove_file(loose(v2)).unwrap();
    rig.fs.put_unseen(
        &folder.join(".tc/sync.json"),
        br#"{"remoteUrl":"https://door43.example/org/project.git","branch":"main"}"#,
        None,
    );

    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let notices: Vec<Notice> = handle.notices().into_iter().map(|n| n.notice).collect();
    let kept = notices.iter().find_map(|n| match n {
        Notice::HistoryRebuilt { broken } => Some(broken.to_string()),
        _ => None,
    });
    let kept = kept.unwrap_or_else(|| panic!("no HistoryRebuilt in {notices:?}"));
    assert_eq!(
        std::fs::read(dir.parent().unwrap().join(&kept).join("HEAD")).unwrap(),
        b"garbage that is no reference",
        "the damaged repository is kept aside"
    );
    assert!(
        !notices
            .iter()
            .any(|n| matches!(n, Notice::CarriedBytesMissing { .. })),
        "no blob is missing: {notices:?}"
    );
    let repo = repo_of(&rig.fs, &folder);
    assert!(
        repo.odb().unwrap().exists(v1),
        "copied from the old objects"
    );
    assert!(repo.odb().unwrap().exists(v2), "stored again from the file");
    let remote = repo.find_remote("origin").unwrap();
    assert_eq!(
        remote.url().unwrap(),
        "https://door43.example/org/project.git"
    );
    assert_eq!(
        repo.config().unwrap().get_string("tc.branch").unwrap(),
        "main"
    );
    let commits = commits_of(&repo);
    assert_eq!(commits.len(), 1);
    let first = repo.find_commit(commits[0]).unwrap();
    assert_eq!(first.parent_count(), 0);
    assert!(
        first
            .message()
            .unwrap()
            .starts_with("Recovered history (tC)")
    );
    drop(first);
    assert_eq!(
        commit_files(&repo, commits[0]).get(SONG),
        Some(&second),
        "the first commit lists the file as the project has it"
    );
    assert!(commit_problems(&repo, commits[0]).is_empty());

    // A checkpoint finds nothing to add, and the earlier version can come back.
    let report = handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert_eq!(report.skipped, Some(tc_store::CheckpointSkip::NoChanges));
    let key = RecordKey::CarriedFile(RepoPath::parse(SONG).unwrap());
    let history = handle.snapshot().fold().history(&key);
    assert_eq!(history.len(), 2);
    let shown = handle.mint(&key, None).unwrap();
    handle
        .submit(Command::Restore {
            shown,
            from: history[0],
        })
        .wait()
        .unwrap();
    handle.wait_idle();
    let placed = handle.snapshot().fold().carried_files();
    assert_eq!(placed.len(), 1);
    assert_eq!(
        placed[0].oid,
        tc_journal::GitOid::parse(&v1.to_string()).unwrap()
    );
    handle.close().unwrap();
    assert_commits_valid(&rig.fs, &folder);
    assert_valid_burrito(&rig.fs, &folder);
}
