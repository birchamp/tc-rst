//! Repack (`docs/STORAGE.md` §9.8): the loose objects the branches and `refs/tc/` reach are
//! written into a pack and their loose files removed; objects nothing reaches go only when
//! they are older than 14 days; a carried version replaced between two checkpoints is kept
//! by `refs/tc/carried`; and a repack that stops after its pack is written loses nothing and
//! leaves nothing twice once the next one has run.

use crate::common::*;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tc_store::{CheckpointReason, OpenOptions, Point, ProjectHandle};

fn open(rig: &Rig, folder: &Path) -> ProjectHandle {
    let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle
}

fn checkpoint(handle: &ProjectHandle) {
    handle
        .checkpoint(CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    handle.wait_idle();
}

/// Makes a repack due at once, however few objects are loose.
fn repack_always(rig: &Rig) {
    rig.store.hooks().repack_after(0, 0);
}

/// The loose object files of the repository.
fn loose(rig: &Rig, folder: &Path) -> Vec<PathBuf> {
    let objects = git_dir_of(&rig.fs, folder).join("objects");
    let mut out = Vec::new();
    for folder in std::fs::read_dir(objects).unwrap().flatten() {
        let name = folder.file_name().to_string_lossy().into_owned();
        if name.len() == 2 && name.bytes().all(|b| b.is_ascii_hexdigit()) {
            for file in std::fs::read_dir(folder.path()).unwrap().flatten() {
                out.push(file.path());
            }
        }
    }
    out
}

/// The object ids each pack index lists, one list per pack.
fn packed(rig: &Rig, folder: &Path) -> Vec<Vec<[u8; 20]>> {
    let dir = git_dir_of(&rig.fs, folder).join("objects/pack");
    let mut out = Vec::new();
    for file in std::fs::read_dir(dir).unwrap().flatten() {
        if file.path().extension().is_some_and(|e| e == "idx") {
            let bytes = std::fs::read(file.path()).unwrap();
            assert_eq!(&bytes[..8], &[0xff, b't', b'O', b'c', 0, 0, 0, 2]);
            let count = u32::from_be_bytes(bytes[8 + 1020..8 + 1024].try_into().unwrap()) as usize;
            let names = &bytes[8 + 1024..8 + 1024 + count * 20];
            out.push(names.as_chunks::<20>().0.to_vec());
        }
    }
    out
}

/// Makes a loose file look as old as `days` days.
fn age(path: &Path, days: u64) {
    let file = std::fs::File::options().write(true).open(path);
    let file = match file {
        Ok(file) => file,
        Err(_) => {
            // Loose objects are read-only; the time is set through a writable copy of the mode.
            let mut permissions = std::fs::metadata(path).unwrap().permissions();
            #[allow(
                clippy::permissions_set_readonly_false,
                reason = "a test file, read by this test process alone"
            )]
            permissions.set_readonly(false);
            std::fs::set_permissions(path, permissions).unwrap();
            std::fs::File::options().write(true).open(path).unwrap()
        }
    };
    file.set_modified(SystemTime::now() - Duration::from_secs(days * 24 * 3600))
        .unwrap();
}

fn loose_path(rig: &Rig, folder: &Path, oid: git2::Oid) -> PathBuf {
    let hex = oid.to_string();
    git_dir_of(&rig.fs, folder)
        .join("objects")
        .join(&hex[..2])
        .join(&hex[2..])
}

fn oid_of(loose: &Path) -> git2::Oid {
    let hex = format!(
        "{}{}",
        loose
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy(),
        loose.file_name().unwrap().to_string_lossy()
    );
    git2::Oid::from_str(&hex).unwrap()
}

/// Every commit, tree and blob the branch reaches.
fn reachable_oids(repo: &git2::Repository) -> BTreeSet<git2::Oid> {
    let mut out = BTreeSet::new();
    for commit in commits_of(repo) {
        out.insert(commit);
        let tree = repo.find_commit(commit).unwrap().tree().unwrap();
        out.insert(tree.id());
        tree.walk(git2::TreeWalkMode::PreOrder, |_, entry| {
            out.insert(entry.id());
            git2::TreeWalkResult::Ok
        })
        .unwrap();
    }
    out
}

/// Every object a commit reaches, read back.
fn read_everything(repo: &git2::Repository, commit: git2::Oid) {
    let commit = repo.find_commit(commit).unwrap();
    let tree = commit.tree().unwrap();
    tree.walk(git2::TreeWalkMode::PreOrder, |_, entry| {
        if entry.kind() == Some(git2::ObjectType::Blob) {
            repo.find_blob(entry.id()).unwrap();
        }
        git2::TreeWalkResult::Ok
    })
    .unwrap();
}

/// §9.8: after checkpoints the loose objects of every commit are in one pack, the loose files
/// are gone, and every commit's tree reads back.
#[test]
fn repack_keeps_every_reachable_object() {
    let (rig, folder) = titus();
    repack_always(&rig);
    let handle = open(&rig, &folder);
    for (verse, text) in [(1, "Paul, a servant"), (2, "in hope of eternal life")] {
        save_verse(&handle, 1, verse, text).unwrap();
        checkpoint(&handle);
    }
    assert!(handle.counters().git_repacks >= 1, "a repack ran");
    let repo = repo_of(&rig.fs, &folder);
    let commits = commits_of(&repo);
    assert_eq!(commits.len(), 3);
    for commit in &commits {
        read_everything(&repo, *commit);
    }
    let reached = reachable_oids(&repo);
    let left: Vec<String> = loose(&rig, &folder)
        .iter()
        .map(|p| oid_of(p))
        .filter(|oid| reached.contains(oid))
        .map(|oid| oid.to_string())
        .collect();
    assert!(
        left.is_empty(),
        "what a branch reaches is packed: {left:#?}"
    );
    let packs = packed(&rig, &folder);
    assert!(!packs.is_empty());
    let all: Vec<[u8; 20]> = packs.into_iter().flatten().collect();
    let unique: BTreeSet<[u8; 20]> = all.iter().copied().collect();
    assert_eq!(all.len(), unique.len(), "no object is in two packs");
    handle.close().unwrap();
    assert_commits_valid(&rig.fs, &folder);
    assert_valid_burrito(&rig.fs, &folder);
}

/// An object only a remote branch or `refs/tc/` reaches survives a repack although it is
/// older than 14 days; an unreachable one that old goes, and a young one stays.
#[test]
fn repack_prunes_unreachable_objects_by_age_and_keeps_what_a_ref_reaches() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    handle.close().unwrap();
    let repo = repo_of(&rig.fs, &folder);
    // A commit only `refs/remotes/origin/side` reaches.
    let blob = repo.blob(b"a teammate's note, fetched").unwrap();
    let mut builder = repo.treebuilder(None).unwrap();
    builder.insert("note.txt", blob, 0o100_644).unwrap();
    let tree = builder.write().unwrap();
    let signature =
        git2::Signature::new("Juan", "juan@example.org", &git2::Time::new(0, 0)).unwrap();
    let commit = repo
        .commit(
            None,
            &signature,
            &signature,
            "side",
            &repo.find_tree(tree).unwrap(),
            &[],
        )
        .unwrap();
    repo.reference("refs/remotes/origin/side", commit, true, "")
        .unwrap();
    // Two objects nothing reaches.
    let old = repo.blob(b"nothing reaches this, and it is old").unwrap();
    let young = repo.blob(b"nothing reaches this, and it is new").unwrap();
    for oid in [blob, tree, commit, old] {
        age(&loose_path(&rig, &folder, oid), 15);
    }
    drop(builder);
    drop(repo);

    repack_always(&rig);
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    checkpoint(&handle);
    assert!(handle.counters().git_repacks >= 1);
    let repo = repo_of(&rig.fs, &folder);
    let odb = repo.odb().unwrap();
    assert!(odb.exists(blob), "the remote branch's blob is kept");
    assert!(odb.exists(tree));
    assert!(odb.exists(commit));
    assert!(!odb.exists(old), "an unreachable object past 14 days goes");
    assert!(odb.exists(young), "a young one stays");
    assert!(loose_path(&rig, &folder, young).exists());
    assert!(
        !loose_path(&rig, &folder, blob).exists(),
        "what the remote branch reaches is in the pack"
    );
    handle.close().unwrap();
}

/// §9.8 and R1: a repack never delays the close. The one that runs when the project closes is
/// stopped after its pack is written and removes no loose object, so the close ends without
/// waiting for the rest of the work; the next open's repack removes the duplicates without
/// packing them twice, and every object is still there.
#[test]
fn a_close_stops_the_repack_that_runs_and_the_next_repack_finishes_it() {
    let (rig, folder) = titus();
    repack_always(&rig);
    rig.store.hooks().hold(Point::AfterPackWritten);
    // No `wait_idle`: the repack of the open is held.
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    rig.store.hooks().wait_reached(Point::AfterPackWritten, 1);
    let releaser = {
        let hooks = rig.store.hooks().clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            hooks.release(Point::AfterPackWritten);
        })
    };
    handle.close().unwrap();
    releaser.join().unwrap();
    let packs = packed(&rig, &folder);
    assert_eq!(packs.len(), 1, "the pack was written");
    let in_pack: BTreeSet<[u8; 20]> = packs.iter().flatten().copied().collect();
    let still_loose: Vec<git2::Oid> = loose(&rig, &folder)
        .iter()
        .map(|p| oid_of(p))
        .filter(|oid| in_pack.contains(<&[u8; 20]>::try_from(oid.as_bytes()).unwrap()))
        .collect();
    assert!(!still_loose.is_empty(), "a stopped run removed nothing");

    let handle = open(&rig, &folder);
    assert!(handle.counters().git_loose_removed > 0);
    let repo = repo_of(&rig.fs, &folder);
    let reached = reachable_oids(&repo);
    let left: Vec<String> = loose(&rig, &folder)
        .iter()
        .map(|p| oid_of(p))
        .filter(|oid| reached.contains(oid))
        .map(|oid| oid.to_string())
        .collect();
    assert!(left.is_empty(), "the next repack removed them: {left:#?}");
    let all: Vec<[u8; 20]> = packed(&rig, &folder).into_iter().flatten().collect();
    let unique: BTreeSet<[u8; 20]> = all.iter().copied().collect();
    assert_eq!(all.len(), unique.len(), "no object is in two packs");
    for oid in &still_loose {
        assert!(repo.odb().unwrap().exists(*oid), "{oid} is still there");
    }
    for commit in commits_of(&repo) {
        read_everything(&repo, commit);
    }
    handle.close().unwrap();
    assert_commits_valid(&rig.fs, &folder);
}

/// A closing project starts no repack at all: what the close's own checkpoint wrote stays
/// loose until the next open.
#[test]
fn a_closing_project_starts_no_repack() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    repack_always(&rig);
    handle.close().unwrap();
    assert!(
        packed(&rig, &folder).is_empty(),
        "nothing was packed while closing"
    );
    assert!(!loose(&rig, &folder).is_empty());
}
