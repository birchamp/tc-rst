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
/// taken), and its blob stays in the merge tree, so no device drops it from the tip. L77
/// (§5.0): the salvaged lines are kept as the teammate made them: the same change, device,
/// number, time and batch, never this device's, after the salvage, after this device seals
/// them into a pack of its own, and after a reopen.
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
    save_verse(&b, 1, 3, "in his time").unwrap();
    checkpoint(&b);
    b.close().unwrap();
    let repo_a = repo_of(&rig_a.fs, &folder);
    let (theirs, packs) = fetch(&repo_a, &git_dir_of(&rig_b.fs, &folder));
    assert_eq!(packs.len(), 1);
    // Door43's copy of Juan's pack lost its last bytes.
    let path = packs[0].path.clone();
    let project = project_of(&rig_a.fs, &folder);
    let whole = tc_store::read_received_pack(&path, &packs[0].bytes, project).changes;
    let damaged = packs[0].bytes[..packs[0].bytes.len() - 7].to_vec();
    let readable = tc_store::read_received_pack(&path, &damaged, project).changes;
    assert!(!readable.is_empty() && readable.len() < whole.len());
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
    let me = a.facts().device;
    let remote = tc_journal::GitOid::from_bytes(cut.as_bytes().try_into().unwrap());
    let report = a.sync_merge(remote, received).wait().unwrap();
    assert_eq!(report.intake.salvaged, 1, "{report:?}");
    let merge = git2::Oid::from_bytes(report.commit.expect("a merge commit").as_bytes()).unwrap();
    assert_eq!(
        commit_files(&repo_a, merge).get(&path),
        Some(&damaged),
        "the damaged pack stays at the tip"
    );
    let as_made = |handle: &ProjectHandle| {
        let snapshot = handle.snapshot();
        for change in &readable {
            let body = change.body();
            let view = snapshot
                .fold()
                .change(&change.id())
                .expect("the salvaged change, by its own id");
            assert_ne!(view.device, me);
            assert_eq!(
                (view.device, view.seq, view.at, view.batch),
                (body.device, body.seq, body.at, body.batch.id)
            );
        }
    };
    as_made(&a);
    checkpoint(&a);
    as_made(&a);
    a.close().unwrap();
    let a = open(&rig_a, &folder);
    as_made(&a);
    a.close().unwrap();
}

/// §6.4 Y2 to Y4: a sync's merge killed at any write point, with or without a power cut,
/// leaves this computer's own work, a teammate's verse whole or absent (never half a batch),
/// a valid folder, and a retry that takes the teammate's work and commits.
#[test]
fn crash_intake_folds_whole_batches_only_and_commits_on_retry() {
    use crate::crash::{Outcome, kill_everywhere};
    let (rig_a, folder) = titus();
    let a = open(&rig_a, &folder);
    save_verse(&a, 1, 1, "Paul, as Ana has it").unwrap();
    checkpoint(&a);
    a.close().unwrap();
    let rig_b = rig_as(rig_a.fs.copy(), APP, "host-2");
    let b = open(&rig_b, &folder);
    save_verse(&b, 1, 2, "in hope, as Juan has it").unwrap();
    save_verse(&b, 1, 3, "in his time, as Juan has it").unwrap();
    checkpoint(&b);
    b.close().unwrap();
    let repo_a = repo_of(&rig_a.fs, &folder);
    let (theirs, packs) = fetch(&repo_a, &git_dir_of(&rig_b.fs, &folder));
    let remote = tc_journal::GitOid::from_bytes(theirs.as_bytes().try_into().unwrap());
    let base = rig_a.fs.crash(false);
    drop(rig_a);
    let action = |handle: &ProjectHandle| {
        let _ = handle.sync_merge(remote, packs.clone()).wait();
        Outcome::NotSaved
    };
    let points = kill_everywhere(&base, &folder, &action, &|handle, _| {
        assert_eq!(
            words(handle, 1, 1).as_deref(),
            Some("Paul, as Ana has it"),
            "INV-OWN-WORK-NEVER-REVERTED"
        );
        let two = words(handle, 1, 2);
        let three = words(handle, 1, 3);
        let whole = two.as_deref() == Some("in hope, as Juan has it")
            && three.as_deref() == Some("in his time, as Juan has it");
        let absent = two.as_deref() == Some("___") && three.as_deref() == Some("___");
        assert!(
            whole || absent,
            "INV-RECEIVED-WORK-IS-KEPT-WHOLE-OR-ABSENT: {two:?} {three:?}"
        );
        let retry = handle.sync_merge(remote, packs.clone()).wait().unwrap();
        assert!(
            retry.skipped.is_none() || retry.commit.is_some() || whole,
            "{retry:?}"
        );
        assert_eq!(
            words(handle, 1, 2).as_deref(),
            Some("in hope, as Juan has it")
        );
    });
    assert!(points > 5, "{points}");
}

/// §6.4 I1 and I2: receiving an exchange file killed at any write point keeps this computer's
/// work and the teammate's batch whole or absent; receiving the file again takes the rest.
#[test]
#[ignore = "nightly: receive_packs runs the same intake and checkpoint as sync_merge; crash_intake_folds_whole_batches_only_and_commits_on_retry catches the bug in the gate"]
fn crash_usb_receive_takes_whole_batches_and_the_rest_on_retry_nightly() {
    use crate::crash::{Outcome, kill_everywhere};
    let (rig_a, folder) = titus();
    let a = open(&rig_a, &folder);
    save_verse(&a, 1, 1, "Paul, as Ana has it").unwrap();
    checkpoint(&a);
    a.close().unwrap();
    let rig_b = rig_as(rig_a.fs.copy(), APP, "host-2");
    let b = open(&rig_b, &folder);
    save_verse(&b, 1, 2, "in hope, as Juan has it").unwrap();
    save_verse(&b, 1, 3, "in his time, as Juan has it").unwrap();
    checkpoint(&b);
    b.close().unwrap();
    let repo_a = repo_of(&rig_a.fs, &folder);
    let (_, packs) = fetch(&repo_a, &git_dir_of(&rig_b.fs, &folder));
    let base = rig_a.fs.crash(false);
    drop(rig_a);
    let action = |handle: &ProjectHandle| {
        let _ = handle.receive_packs("Juan".into(), packs.clone()).wait();
        Outcome::NotSaved
    };
    let points = kill_everywhere(&base, &folder, &action, &|handle, _| {
        assert_eq!(words(handle, 1, 1).as_deref(), Some("Paul, as Ana has it"));
        let two = words(handle, 1, 2);
        let three = words(handle, 1, 3);
        let whole = two.as_deref() == Some("in hope, as Juan has it")
            && three.as_deref() == Some("in his time, as Juan has it");
        let absent = two.as_deref() == Some("___") && three.as_deref() == Some("___");
        assert!(whole || absent, "{two:?} {three:?}");
        handle
            .receive_packs("Juan".into(), packs.clone())
            .wait()
            .unwrap();
        assert_eq!(
            words(handle, 1, 3).as_deref(),
            Some("in his time, as Juan has it")
        );
    });
    assert!(points > 5, "{points}");
}

/// A received pack as two packs of the same sealer, the first holding `first` of its lines: a
/// seal writes an action larger than a pack that way (§4.6), so neither half is whole alone.
fn split(pack: &ReceivedPack, project: tc_journal::ProjectId, first: usize) -> Vec<ReceivedPack> {
    use tc_journal::pack::{PackSpec, encode_pack, read_pack};
    let read = read_pack(&pack.path, &pack.bytes, project);
    let header = read.header.expect("a valid pack");
    let spec = PackSpec {
        project,
        sealer: header.sealer,
        sealed: header.sealed,
    };
    let (a, b) = read.entries.split_at(first);
    [a, b]
        .into_iter()
        .map(|lines| {
            let encoded = encode_pack(&spec, lines, &[]).unwrap();
            let oid = git2::Oid::hash_object(git2::ObjectType::Blob, &encoded.bytes).unwrap();
            ReceivedPack {
                path: encoded.path.to_string(),
                blob: tc_journal::GitOid::from_bytes(oid.as_bytes().try_into().unwrap()),
                bytes: encoded.bytes,
            }
        })
        .collect()
}

/// §9.5: a commit whose new blobs pass the bound is a chain. Juan's new book arrives as two
/// packs; the commit that takes it in has a link before it that adds the half that changes
/// nothing yet, with the derived files of the commit before, so each link is a valid burrito
/// whose derived files are the fold of its own packs, and a push can send one link at a time.
#[test]
fn a_large_commit_is_a_chain_of_valid_commits() {
    use tc_journal::command::{Command, SeedSource};
    let (rig_a, folder) = titus();
    let a = open(&rig_a, &folder);
    save_verse(&a, 1, 1, "Paul, as Ana has it").unwrap();
    let before = checkpoint(&a);
    a.close().unwrap();
    let rig_b = rig_as(rig_a.fs.copy(), APP, "host-2");
    let b = open(&rig_b, &folder);
    b.submit(Command::AddBook {
        book: tc_journal::BookId::Philemon,
        scope: tc_journal::record::Scope::Whole,
        seed: SeedSource {
            usfm: "\\id PHM\n\\usfm 3.0\n\\h PHM\n\\mt PHM\n\\c 1\n\\p\n\\v 1 One.\n\\v 2 Two.\n\\v 3 Three.\n"
                .into(),
        },
    })
    .wait()
    .unwrap();
    checkpoint(&b);
    b.close().unwrap();
    let repo_a = repo_of(&rig_a.fs, &folder);
    let (_, packs) = fetch(&repo_a, &git_dir_of(&rig_b.fs, &folder));
    assert_eq!(packs.len(), 1, "Juan's book is one pack");
    let packs = split(&packs[0], project_of(&rig_a.fs, &folder), 2);

    rig_a.store.hooks().chain_after(1);
    let a = open(&rig_a, &folder);
    let report = a
        .receive_packs("Juan".into(), packs.clone())
        .wait()
        .unwrap();
    let tip = git2::Oid::from_bytes(report.commit.expect("a commit").as_bytes()).unwrap();
    let parents: Vec<git2::Oid> = repo_a.find_commit(tip).unwrap().parent_ids().collect();
    assert_eq!(parents.len(), 1);
    let link = parents[0];
    assert_ne!(link, before, "the commit is a chain");
    let link_parents: Vec<git2::Oid> = repo_a.find_commit(link).unwrap().parent_ids().collect();
    assert_eq!(link_parents, [before]);

    let held = commit_files(&repo_a, link);
    let in_link: Vec<&ReceivedPack> = packs
        .iter()
        .filter(|p| held.contains_key(&p.path))
        .collect();
    assert_eq!(in_link.len(), 1, "the link adds one of the two packs");
    let derived = |files: &std::collections::BTreeMap<String, Vec<u8>>| {
        files
            .iter()
            .filter(|(path, _)| {
                !path.starts_with("ingredients/tc/changes/") && path.as_str() != "metadata.json"
            })
            .map(|(path, bytes)| (path.clone(), bytes.clone()))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    assert_eq!(derived(&held), derived(&commit_files(&repo_a, before)));
    let project = project_of(&rig_a.fs, &folder);
    for commit in [link, tip] {
        assert_eq!(
            tree_differs_from_its_packs(
                &repo_a,
                commit,
                project,
                tc_journal::fold::ProjectKind::Bible
            ),
            Vec::<String>::new(),
            "{commit}"
        );
    }
    let tipped = commit_files(&repo_a, tip);
    assert!(packs.iter().all(|p| tipped.contains_key(&p.path)));
    assert!(
        tipped.contains_key("ingredients/PHM.usfm"),
        "{:?}",
        tipped.keys()
    );
    a.close().unwrap();
    assert_commits_valid(&rig_a.fs, &folder);
    assert_valid_burrito(&rig_a.fs, &folder);
}

/// §9.5 for a merge (S7): the links grow from Door43's commit, so each is a fast-forward of
/// it, and the merge's second parent is the last link. Each is a valid burrito whose derived
/// files are the fold of its own packs, and the teammate's verse on Door43 is kept.
#[test]
fn a_large_merge_is_a_chain_grown_from_door43s_commit() {
    use tc_journal::command::{Command, SeedSource};
    let (rig_a, folder) = titus();
    let a = open(&rig_a, &folder);
    save_verse(&a, 1, 1, "Paul, as Ana has it").unwrap();
    checkpoint(&a);
    a.close().unwrap();
    let rig_b = rig_as(rig_a.fs.copy(), APP, "host-2");
    let rig_c = rig_as(rig_a.fs.copy(), APP, "host-3");
    let b = open(&rig_b, &folder);
    b.submit(Command::AddBook {
        book: tc_journal::BookId::Philemon,
        scope: tc_journal::record::Scope::Whole,
        seed: SeedSource {
            usfm: "\\id PHM\n\\usfm 3.0\n\\h PHM\n\\mt PHM\n\\c 1\n\\p\n\\v 1 One.\n\\v 2 Two.\n\\v 3 Three.\n"
                .into(),
        },
    })
    .wait()
    .unwrap();
    checkpoint(&b);
    b.close().unwrap();
    let c = open(&rig_c, &folder);
    save_verse(&c, 1, 2, "in hope, as Lia has it").unwrap();
    checkpoint(&c);
    c.close().unwrap();

    let repo_a = repo_of(&rig_a.fs, &folder);
    let (_, juans) = fetch(&repo_a, &git_dir_of(&rig_b.fs, &folder));
    let juans = split(&juans[0], project_of(&rig_a.fs, &folder), 2);
    let a = open(&rig_a, &folder);
    a.receive_packs("Juan".into(), juans.clone())
        .wait()
        .unwrap();
    let ours = repo_a.head().unwrap().target().unwrap();
    let (theirs, packs) = fetch(&repo_a, &git_dir_of(&rig_c.fs, &folder));
    rig_a.store.hooks().chain_after(1);
    let remote = tc_journal::GitOid::from_bytes(theirs.as_bytes().try_into().unwrap());
    let report = a.sync_merge(remote, packs).wait().unwrap();
    let merge = git2::Oid::from_bytes(report.commit.expect("a merge commit").as_bytes()).unwrap();
    let parents: Vec<git2::Oid> = repo_a.find_commit(merge).unwrap().parent_ids().collect();
    assert_eq!(parents.len(), 2);
    assert_eq!(parents[0], ours);
    let link = parents[1];
    assert_ne!(link, theirs, "the merge is a chain");
    let link_parents: Vec<git2::Oid> = repo_a.find_commit(link).unwrap().parent_ids().collect();
    assert_eq!(
        link_parents,
        [theirs],
        "the link is a fast-forward of Door43's commit"
    );
    let project = project_of(&rig_a.fs, &folder);
    for commit in [link, merge] {
        assert_eq!(
            tree_differs_from_its_packs(
                &repo_a,
                commit,
                project,
                tc_journal::fold::ProjectKind::Bible
            ),
            Vec::<String>::new(),
            "{commit}"
        );
    }
    assert_eq!(words(&a, 1, 2).as_deref(), Some("in hope, as Lia has it"));
    assert_eq!(words(&a, 1, 1).as_deref(), Some("Paul, as Ana has it"));
    a.close().unwrap();
    assert_commits_valid(&rig_a.fs, &folder);
    assert_valid_burrito(&rig_a.fs, &folder);
}

/// The storage lane's attack on *Bring it back*: Ana's save S reaches the book file while only
/// a pending file holds it, and Juan's change T of the same verse, made over the same text,
/// arrives by an exchange file. The receive is killed at a write point; at a point after
/// T's pack is written and before S is sealed, another program then deletes S's pending file
/// while the app is closed. The reopened project has T, and the book file shows S, which is
/// kept to bring back. T is newer than what S was made over: *Bring it back* never puts S
/// over T, T stays a version shown or in conflict.
struct BringItBack {
    base: tc_store::fault::FaultFs,
    folder: std::path::PathBuf,
    packs: Vec<ReceivedPack>,
    before: Vec<String>,
}

fn bring_it_back_attack() -> BringItBack {
    let (rig_a, folder) = titus();
    let a = open(&rig_a, &folder);
    save_verse(&a, 1, 1, "the text before").unwrap();
    checkpoint(&a);
    a.close().unwrap();
    let rig_b = rig_as(rig_a.fs.copy(), APP, "host-2");
    let b = open(&rig_b, &folder);
    save_verse(&b, 1, 1, "Juan's verse").unwrap();
    checkpoint(&b);
    b.close().unwrap();
    let repo_a = repo_of(&rig_a.fs, &folder);
    let (_, packs) = fetch(&repo_a, &git_dir_of(&rig_b.fs, &folder));
    let base = rig_a.fs.crash(false);
    drop(rig_a);
    let before = pending_files(&base, &folder);
    BringItBack {
        base,
        folder,
        packs,
        before,
    }
}

impl BringItBack {
    fn save(&self, handle: &ProjectHandle) {
        save_verse(handle, 1, 1, "Ana's save").unwrap();
        handle.leave(tc_store::Leave::Mode).wait().unwrap();
        handle.wait_idle();
    }

    fn receive(&self, handle: &ProjectHandle) -> crate::crash::Outcome {
        let _ = handle
            .receive_packs("Juan".into(), self.packs.clone())
            .wait();
        crate::crash::Outcome::NotSaved
    }

    fn delete_anas_pending(&self, fs: &tc_store::fault::FaultFs) {
        for pending in pending_files(fs, &self.folder) {
            if !self.before.contains(&pending) {
                fs.delete(&self.folder.join(pending)).unwrap();
            }
        }
    }

    /// Whether the reopened project is the attack's state; if it is, *Bring it back* keeps
    /// Juan's verse.
    fn bring_it_back_keeps_juans_verse(&self, handle: &ProjectHandle) -> bool {
        let juan_folded = {
            let snapshot = handle.snapshot();
            let fold = snapshot.fold();
            let key = verse(1, 1);
            fold.history(&key).iter().any(|id| {
                fold.body(id)
                    .is_some_and(|b| format!("{:?}", b.record).contains("Juan's verse"))
            })
        };
        let lost: Vec<_> = handle
            .kept_copies()
            .into_iter()
            .filter(|k| k.why == tc_store::KeptWhy::Lost)
            .collect();
        if !juan_folded || lost.is_empty() {
            return false;
        }
        handle.import_kept(&lost[0].copy).wait().unwrap();
        handle.wait_idle();
        let shown = words(handle, 1, 1);
        let snapshot = handle.snapshot();
        let in_conflict = snapshot.fold().conflicts().any(|c| {
            c.candidates
                .iter()
                .any(|v| format!("{:?}", v.value).contains("Juan's verse"))
        });
        assert!(
            shown.as_deref() == Some("Juan's verse") || in_conflict,
            "Ana's lost save replaced Juan's verse: {shown:?}"
        );
        true
    }
}

/// The attack at the first write point that gives it, after a killed process (no power cut):
/// the gate's copy of the nightly sweep, whose other checks the intake crash tests make.
#[test]
fn bring_it_back_never_replaces_a_teammates_verse_received_after_the_save() {
    let attack = bring_it_back_attack();
    for k in 0..500 {
        let fs = attack.base.copy();
        let rig = rig_as(fs.clone(), APP, "host-1");
        let handle = open(&rig, &attack.folder);
        attack.save(&handle);
        fs.kill_before(k, false);
        attack.receive(&handle);
        let dead = fs.is_dead();
        drop(handle);
        drop(rig);
        let after = fs.crash(false);
        attack.delete_anas_pending(&after);
        let rig = rig_as(after, APP, "host-1");
        let handle = open(&rig, &attack.folder);
        if attack.bring_it_back_keeps_juans_verse(&handle) {
            return;
        }
        assert!(dead, "no kill point gave the attack");
    }
    panic!("the receive never ends");
}

/// Every write point, both power-loss models, torn or not, with the crash sweep's checks.
#[test]
#[ignore = "nightly: bring_it_back_never_replaces_a_teammates_verse_received_after_the_save runs the attack at its first kill point in the gate"]
fn bring_it_back_never_replaces_a_teammates_verse_received_after_the_save_nightly() {
    let attack = bring_it_back_attack();
    let attacked = std::cell::Cell::new(0);
    let points = crate::crash::kill_everywhere_between(
        &attack.base,
        &attack.folder,
        &|_, handle| attack.save(handle),
        &|handle| attack.receive(handle),
        &|fs| attack.delete_anas_pending(fs),
        &|handle, _, _| {
            if attack.bring_it_back_keeps_juans_verse(handle) {
                attacked.set(attacked.get() + 1);
            }
        },
    );
    assert!(points > 5, "{points}");
    assert!(attacked.get() > 0, "no kill point gave the attack");
}
