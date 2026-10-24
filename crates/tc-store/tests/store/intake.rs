//! A sync's merge (`docs/STORAGE.md` §9.2 to §9.4): the packs fetched from Door43 are taken
//! in whole beside the project's own, the merge commit has Door43's commit as its second
//! parent, and no entry of Door43's tree is dropped.

use crate::common::*;
use std::path::Path;
use tc_store::{CheckpointReason, OpenOptions, ProjectHandle, ReceivedPack};

fn open(rig: &Rig, folder: &Path) -> ProjectHandle {
    let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle
}

fn checkpoint(handle: &ProjectHandle) -> git2::Oid {
    let report = handle
        .checkpoint(CheckpointReason::BeforeSync)
        .wait()
        .unwrap();
    git2::Oid::from_bytes(report.commit.expect("a commit").as_bytes()).unwrap()
}

/// What a sync's fetch gives: `from`'s branch in `into`'s repository, and the packs of its
/// tree that `into`'s HEAD lacks.
fn fetch(into: &git2::Repository, from: &Path) -> (git2::Oid, Vec<ReceivedPack>) {
    let mut remote = into.remote_anonymous(from.to_str().unwrap()).unwrap();
    remote
        .fetch(&["+refs/heads/main:refs/remotes/origin/main"], None, None)
        .unwrap();
    let theirs = into.refname_to_id("refs/remotes/origin/main").unwrap();
    let ours = commit_files(into, into.head().unwrap().target().unwrap());
    let packs = commit_files(into, theirs)
        .into_iter()
        .filter(|(path, _)| path.starts_with("ingredients/tc/changes/") && !ours.contains_key(path))
        .map(|(path, bytes)| ReceivedPack {
            blob: tc_journal::GitOid::from_bytes(
                git2::Oid::hash_object(git2::ObjectType::Blob, &bytes)
                    .unwrap()
                    .as_bytes()
                    .try_into()
                    .unwrap(),
            ),
            path,
            bytes,
        })
        .collect();
    (theirs, packs)
}

#[test]
fn a_sync_merge_takes_a_teammates_packs_and_commits_with_both_parents() {
    let (rig_a, folder) = titus();
    let a = open(&rig_a, &folder);
    save_verse(&a, 1, 1, "Paul, as Ana has it").unwrap();
    let ours = checkpoint(&a);
    a.close().unwrap();
    // Another computer with a copy of the project.
    let rig_b = rig_as(rig_a.fs.copy(), APP, "host-2");
    let b = open(&rig_b, &folder);
    save_verse(&b, 1, 2, "in hope, as Juan has it").unwrap();
    checkpoint(&b);
    b.close().unwrap();

    let repo_a = repo_of(&rig_a.fs, &folder);
    let (theirs, packs) = fetch(&repo_a, &git_dir_of(&rig_b.fs, &folder));
    assert!(!packs.is_empty(), "Juan's computer sealed a pack");
    let a = open(&rig_a, &folder);
    let remote = tc_journal::GitOid::from_bytes(theirs.as_bytes().try_into().unwrap());
    let report = a.sync_merge(remote, packs.clone()).wait().unwrap();
    assert_eq!(report.intake.packs as usize, packs.len(), "{report:?}");
    assert_eq!(words(&a, 1, 2).as_deref(), Some("in hope, as Juan has it"));
    assert_eq!(words(&a, 1, 1).as_deref(), Some("Paul, as Ana has it"));
    let merge = git2::Oid::from_bytes(report.commit.expect("a merge commit").as_bytes()).unwrap();
    let parents: Vec<git2::Oid> = repo_a.find_commit(merge).unwrap().parent_ids().collect();
    assert_eq!(parents, [ours, theirs]);
    let merged = commit_files(&repo_a, merge);
    for path in commit_files(&repo_a, theirs).keys() {
        if path.starts_with("ingredients/tc/changes/") {
            assert!(merged.contains_key(path), "{path} left the tip");
        }
    }
    a.close().unwrap();
    assert_commits_valid(&rig_a.fs, &folder);
    assert_valid_burrito(&rig_a.fs, &folder);
}

/// A pack the project already holds is not taken twice, and a commit the history holds is
/// not merged again.
#[test]
fn a_sync_merge_of_held_packs_takes_nothing_and_still_commits() {
    let (rig_a, folder) = titus();
    let a = open(&rig_a, &folder);
    save_verse(&a, 1, 1, "Paul").unwrap();
    checkpoint(&a);
    a.close().unwrap();
    let rig_b = rig_as(rig_a.fs.copy(), APP, "host-2");
    let b = open(&rig_b, &folder);
    save_verse(&b, 1, 2, "in hope").unwrap();
    checkpoint(&b);
    b.close().unwrap();
    let repo_a = repo_of(&rig_a.fs, &folder);
    let (theirs, packs) = fetch(&repo_a, &git_dir_of(&rig_b.fs, &folder));
    let remote = tc_journal::GitOid::from_bytes(theirs.as_bytes().try_into().unwrap());
    let a = open(&rig_a, &folder);
    a.sync_merge(remote, packs.clone()).wait().unwrap();
    let again = a.sync_merge(remote, packs).wait().unwrap();
    assert_eq!(again.intake.packs, 0, "{again:?}");
    assert!(again.intake.held > 0, "{again:?}");
    assert_eq!(
        again.skipped,
        Some(tc_store::CheckpointSkip::NoChanges),
        "{again:?}"
    );
    a.close().unwrap();
    assert_commits_valid(&rig_a.fs, &folder);
}

/// §9.3 T1 and §5.10 M1: a pack path where another program wrote other bytes while the
/// project is open is never written over. A file appearing in the journal folders is another
/// installation saving into this folder (§5.0), so the merge is not made at all, and the next
/// sync takes the teammates' packs once the folder is quiet.
#[test]
fn intake_never_writes_over_a_file_another_program_wrote() {
    let (rig_a, folder) = titus();
    let a = open(&rig_a, &folder);
    save_verse(&a, 1, 1, "Paul").unwrap();
    checkpoint(&a);
    a.close().unwrap();
    let rig_b = rig_as(rig_a.fs.copy(), APP, "host-2");
    let b = open(&rig_b, &folder);
    save_verse(&b, 1, 2, "in hope").unwrap();
    checkpoint(&b);
    b.close().unwrap();
    let repo_a = repo_of(&rig_a.fs, &folder);
    let (theirs, packs) = fetch(&repo_a, &git_dir_of(&rig_b.fs, &folder));
    let taken = &packs[0].path;
    let other = b"another program's bytes".to_vec();
    let a = open(&rig_a, &folder);
    let head = repo_a.head().unwrap().target();
    rig_a.fs.put_unseen(&folder.join(taken), &other, None);
    let remote = tc_journal::GitOid::from_bytes(theirs.as_bytes().try_into().unwrap());
    let report = a.sync_merge(remote, packs.clone()).wait().unwrap();
    assert_eq!(
        report.skipped,
        Some(tc_store::CheckpointSkip::SecondWriter),
        "{report:?}"
    );
    assert_eq!(report.intake, tc_store::IntakeReport::default());
    assert_eq!(rig_a.fs.peek(&folder.join(taken)), Some(other));
    assert_eq!(repo_a.head().unwrap().target(), head, "no merge commit");
    assert_eq!(
        words(&a, 1, 2).as_deref(),
        Some("___"),
        "Juan's verse is not taken"
    );
    damaged_on_purpose(&rig_a);
    a.close().unwrap();
}

/// §9.2 N3: a damaged copy of a pack this computer holds is repaired from Door43's valid
/// copy, and the damaged bytes are kept aside.
#[test]
fn a_damaged_local_pack_is_repaired_from_door43s_copy() {
    let (rig_a, folder) = titus();
    let a = open(&rig_a, &folder);
    save_verse(&a, 1, 1, "Paul").unwrap();
    checkpoint(&a);
    a.close().unwrap();
    let rig_b = rig_as(rig_a.fs.copy(), APP, "host-2");
    let b = open(&rig_b, &folder);
    save_verse(&b, 1, 2, "in hope").unwrap();
    checkpoint(&b);
    b.close().unwrap();
    let repo_a = repo_of(&rig_a.fs, &folder);
    let (theirs, packs) = fetch(&repo_a, &git_dir_of(&rig_b.fs, &folder));
    let taken = packs[0].path.clone();
    // The pack reached this computer once, then a disk error cut it.
    let cut = packs[0].bytes[..packs[0].bytes.len() / 2].to_vec();
    rig_a.fs.put_unseen(&folder.join(&taken), &cut, None);
    let a = open(&rig_a, &folder);
    let remote = tc_journal::GitOid::from_bytes(theirs.as_bytes().try_into().unwrap());
    let report = a.sync_merge(remote, packs.clone()).wait().unwrap();
    assert_eq!(report.intake.packs as usize, packs.len(), "{report:?}");
    assert_eq!(
        rig_a.fs.peek(&folder.join(&taken)),
        Some(packs[0].bytes.clone())
    );
    assert_eq!(words(&a, 1, 2).as_deref(), Some("in hope"));
    let kept: Vec<Vec<u8>> = files(&rig_a.fs, &folder)
        .into_iter()
        .filter(|(rel, _)| rel.starts_with(".tc/quarantine/"))
        .map(|(_, bytes)| bytes)
        .collect();
    assert!(kept.contains(&cut), "the damaged bytes are kept");
    a.close().unwrap();
    assert_commits_valid(&rig_a.fs, &folder);
}

/// §9.2 N3: a damaged pack received from Door43 is salvaged (its readable changes are
/// taken), and its blob stays in the merge tree, so no device drops it from the tip.
#[test]
fn a_damaged_remote_pack_is_salvaged_and_kept_in_the_tree() {
    let (rig_a, folder) = titus();
    let a = open(&rig_a, &folder);
    save_verse(&a, 1, 1, "Paul").unwrap();
    checkpoint(&a);
    a.close().unwrap();
    let rig_b = rig_as(rig_a.fs.copy(), APP, "host-2");
    let b = open(&rig_b, &folder);
    save_verse(&b, 1, 2, "in hope").unwrap();
    checkpoint(&b);
    b.close().unwrap();
    let repo_a = repo_of(&rig_a.fs, &folder);
    let (theirs, packs) = fetch(&repo_a, &git_dir_of(&rig_b.fs, &folder));
    // Door43's copy of Juan's pack lost its last bytes.
    let path = packs[0].path.clone();
    let damaged = packs[0].bytes[..packs[0].bytes.len() - 7].to_vec();
    let blob = repo_a.blob(&damaged).unwrap();
    let base = repo_a.find_commit(theirs).unwrap();
    let mut update = git2::build::TreeUpdateBuilder::new();
    update.upsert(&path, blob, git2::FileMode::Blob);
    let tree = update
        .create_updated(&repo_a, &base.tree().unwrap())
        .unwrap();
    let tree = repo_a.find_tree(tree).unwrap();
    let sig = git2::Signature::now("Door43", "door43@example.org").unwrap();
    let cut = repo_a
        .commit(None, &sig, &sig, "a damaged copy", &tree, &[&base])
        .unwrap();
    let received = vec![ReceivedPack {
        path: path.clone(),
        bytes: damaged.clone(),
        blob: tc_journal::GitOid::from_bytes(blob.as_bytes().try_into().unwrap()),
    }];
    let a = open(&rig_a, &folder);
    let remote = tc_journal::GitOid::from_bytes(cut.as_bytes().try_into().unwrap());
    let report = a.sync_merge(remote, received).wait().unwrap();
    assert_eq!(report.intake.salvaged, 1, "{report:?}");
    let merge = git2::Oid::from_bytes(report.commit.expect("a merge commit").as_bytes()).unwrap();
    assert_eq!(
        commit_files(&repo_a, merge).get(&path),
        Some(&damaged),
        "the damaged pack stays at the tip"
    );
    a.close().unwrap();
}
