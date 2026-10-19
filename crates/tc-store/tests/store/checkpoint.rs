//! The checkpoint's commit (`docs/STORAGE.md` §5.13, §9.4, §16.6; APP-DESIGN R21): one commit
//! when something changed and none when nothing did, a tree built from the project and never
//! from a file another program holds, the expected packs restored or the commit refused, bytes
//! never converted, foreign entries kept, and a save never waiting for any of it.

use crate::common::*;
use crate::corpus::{OnDisk, psalms, psalms_x10};
use sha2::Digest;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tc_journal::command::Command;
use tc_journal::fold::Notice;
use tc_journal::{Cause, FORMAT_MAJOR, FORMAT_MINOR, Record, RecordKey};
use tc_store::fault::FaultFs;
use tc_store::{
    CheckpointReason, CheckpointSkip, Hooks, OpenOptions, Point, ProjectHandle, Stored,
};

fn open(rig: &Rig, folder: &Path) -> ProjectHandle {
    let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle
}

fn notices(handle: &ProjectHandle) -> Vec<Notice> {
    handle.notices().into_iter().map(|n| n.notice).collect()
}

fn checkpoint(handle: &ProjectHandle) -> tc_store::CheckpointReport {
    handle
        .checkpoint(CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap()
}

/// J8: a checkpoint with changes commits once, and one without changes commits nothing.
#[test]
fn j8_a_checkpoint_with_changes_commits_once_and_without_changes_not_at_all() {
    let (rig, folder) = titus();
    let repo = repo_of(&rig.fs, &folder);
    assert_eq!(commits_of(&repo).len(), 1, "the create path commits once");
    let handle = open(&rig, &folder);
    let report = checkpoint(&handle);
    assert_eq!(report.commit, None);
    assert_eq!(report.skipped, Some(CheckpointSkip::NoChanges));
    assert_eq!(commits_of(&repo).len(), 1);
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    let report = checkpoint(&handle);
    let commit = report.commit.expect("a commit");
    assert_eq!(commits_of(&repo).len(), 2);
    let files = commit_files(&repo, git2::Oid::from_bytes(commit.as_bytes()).unwrap());
    let usfm = String::from_utf8(files["ingredients/TIT.usfm"].clone()).unwrap();
    assert!(usfm.contains("Paul, a servant of God"));
    assert!(
        files
            .keys()
            .any(|p| p.starts_with("ingredients/tc/changes/"))
    );
    assert_eq!(files[".gitignore"], tc_store::layout::GITIGNORE_BYTES);
    let again = checkpoint(&handle);
    assert_eq!(again.commit, None);
    assert_eq!(again.skipped, Some(CheckpointSkip::NoChanges));
    assert_eq!(commits_of(&repo).len(), 2);
    handle.close().unwrap();
    assert_commits_valid(&rig.fs, &folder);
}

/// The commit's tree is the project's projection, never the bytes of a file a program changed
/// although its size and time did not (a held file, §6.2): they would reach Door43 as the
/// team's text.
#[test]
fn a_commit_holds_the_folds_projection_not_a_held_file() {
    let (rig, folder) = titus();
    let fs = rig.fs.copy();
    fs.advance(Duration::from_secs(10));
    let rig2 = rig_as(fs.clone(), APP, "host-1");
    let handle = open(&rig2, &folder);
    handle.close().unwrap();
    let usfm = folder.join("ingredients/TIT.usfm");
    let mut edited = fs.peek(&usfm).unwrap();
    let at = edited.iter().position(|b| *b == b'T').unwrap();
    edited[at] = b't';
    let mtime = tc_store::Vfs::metadata(&fs, &usfm).unwrap().mtime_ns;
    fs.put(&usfm, &edited, Some(mtime));
    let handle = open(&rig2, &folder);
    assert!(
        notices(&handle).iter().any(|n| matches!(
            n,
            Notice::StorageDamage { path, .. } if path.as_str() == "ingredients/TIT.usfm"
        )),
        "the storage check holds the file"
    );
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    let report = checkpoint(&handle);
    let commit = report.commit.expect("a commit");
    let repo = repo_of(&fs, &folder);
    let files = commit_files(&repo, git2::Oid::from_bytes(commit.as_bytes()).unwrap());
    let committed = &files["ingredients/TIT.usfm"];
    assert_ne!(committed, &edited, "the held bytes never reach a commit");
    assert!(
        String::from_utf8_lossy(committed).contains("Paul, a servant of God"),
        "the commit holds the project's text"
    );
    assert_eq!(fs.peek(&usfm).as_deref(), Some(edited.as_slice()));
    handle.close().unwrap();
    assert_commits_valid(&fs, &folder);
}

/// The commit's message and author: the trailers name the device, the format and the frontier
/// of the fold the tree was made from, and the author has no email (D85).
#[test]
fn a_commit_trailer_names_device_format_and_frontier() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    let frontier = handle.snapshot().fold().frontier().to_hex();
    let device = handle.facts().device;
    let report = checkpoint(&handle);
    let repo = repo_of(&rig.fs, &folder);
    let commit = repo
        .find_commit(git2::Oid::from_bytes(report.commit.unwrap().as_bytes()).unwrap())
        .unwrap();
    let message = commit.message().unwrap();
    assert_eq!(
        message.lines().next(),
        Some("Checkpoint, leaving Translate: TIT (tC)")
    );
    assert!(
        message.contains(&format!("\nTc-Device: {device}\n")),
        "{message}"
    );
    assert!(message.contains(&format!("\nTc-Format: {FORMAT_MAJOR}.{FORMAT_MINOR}\n")));
    assert!(message.contains(&format!("\nTc-Frontier: {frontier}\n")));
    handle.close().unwrap();
}

#[test]
fn the_author_is_translationcore_user_with_no_email() {
    let (rig, folder) = titus();
    let repo = repo_of(&rig.fs, &folder);
    let first = *commits_of(&repo).last().unwrap();
    let commit = repo.find_commit(first).unwrap();
    assert_eq!(commit.author().name().unwrap(), "translationCore user");
    assert_eq!(commit.author().email().unwrap(), "");
    assert_eq!(commit.committer().email().unwrap(), "");
    // Nothing of this machine's git identity is in the project.
    assert!(!git_dir_of(&rig.fs, &folder).join("logs").exists());
}

/// After the person consented to show their Door43 name (D98), it is the author.
#[test]
fn the_author_is_the_door43_username_after_consent() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let device = handle.facts().device;
    let key = RecordKey::DeviceProfile(device);
    let parents = handle.snapshot().fold().heads(&key);
    handle
        .submit(Command::ApplyUpgrade {
            plan: tc_journal::command::UpgradePlan {
                changes: vec![tc_journal::command::PlannedChange {
                    record: Record::DeviceProfile {
                        device,
                        value: tc_journal::record::DeviceProfileValue {
                            display_name: None,
                            door43_username: Some(
                                tc_journal::record::Door43Username::parse("jdoe").unwrap(),
                            ),
                            legacy: None,
                        },
                    },
                    parents,
                    cause: Cause::Seed,
                }],
            },
        })
        .wait()
        .unwrap();
    let report = checkpoint(&handle);
    let repo = repo_of(&rig.fs, &folder);
    let commit = repo
        .find_commit(git2::Oid::from_bytes(report.commit.unwrap().as_bytes()).unwrap())
        .unwrap();
    assert_eq!(commit.author().name().unwrap(), "jdoe");
    assert_eq!(commit.author().email().unwrap(), "");
    handle.close().unwrap();
}

/// L65: a pack removed outside the app (an antivirus program, a cloud client) while the project
/// is closed is written back from the commit that holds it, its changes are folded, and the
/// person is told; nothing derived from it was removed.
#[test]
fn loss_pack_deleted_outside_app_restored() {
    let (rig, folder) = titus();
    let pack = packs(&rig.fs, &folder).into_iter().next().unwrap();
    let bytes = rig.fs.peek(&folder.join(&pack)).unwrap();
    rig.fs.delete(&folder.join(&pack)).unwrap();
    let handle = open(&rig, &folder);
    assert!(
        notices(&handle).iter().any(|n| matches!(
            n,
            Notice::PackRestored { path, .. } if path.as_str() == pack
        )),
        "{:?}",
        notices(&handle)
    );
    assert!(
        !notices(&handle)
            .iter()
            .any(|n| matches!(n, Notice::PackMissing { .. }))
    );
    assert_eq!(rig.fs.peek(&folder.join(&pack)), Some(bytes));
    assert!(
        words(&handle, 1, 1).is_some(),
        "the pack's changes are folded"
    );
    // Nothing differs from the commit that held the pack.
    let report = checkpoint(&handle);
    assert_eq!(report.skipped, Some(CheckpointSkip::NoChanges));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}

/// L65: a pack removed while the project is open, whose copy in the object database is gone
/// too, stops the commit: a tree without it would remove it from Door43 by fast-forward. The
/// person is told, nothing derived is removed, and the next checkpoint tries again.
#[test]
fn loss_pack_deleted_while_open_never_pushed() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let repo = repo_of(&rig.fs, &folder);
    let before = commits_of(&repo);
    let pack = packs(&rig.fs, &folder).into_iter().next().unwrap();
    // Its blob is gone from the object database, and the file from the working tree.
    let blob = repo
        .find_commit(before[0])
        .unwrap()
        .tree()
        .unwrap()
        .get_path(Path::new(&pack))
        .unwrap()
        .id()
        .to_string();
    std::fs::remove_file(
        git_dir_of(&rig.fs, &folder)
            .join("objects")
            .join(&blob[..2])
            .join(&blob[2..]),
    )
    .unwrap();
    // The folder is no valid burrito from here on, and not by the app's doing.
    damaged_on_purpose(&rig);
    rig.fs.delete(&folder.join(&pack)).unwrap();
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    let before_usfm = rig.fs.peek(&folder.join("ingredients/TIT.usfm")).unwrap();
    let report = checkpoint(&handle);
    assert_eq!(report.commit, None);
    assert_eq!(report.skipped, Some(CheckpointSkip::PackMissing));
    assert_eq!(commits_of(&repo), before, "nothing was committed");
    assert!(
        notices(&handle)
            .iter()
            .any(|n| matches!(n, Notice::PackMissing { .. }))
    );
    assert_eq!(
        rig.fs
            .peek(&folder.join("ingredients/TIT.usfm"))
            .map(|b| b.len() >= before_usfm.len()),
        Some(true),
        "a book whose changes are missing is not removed"
    );
    handle.close().unwrap();
    // HEAD's own tree names the blob that was lost, so its history is not read back here.
}

/// L56: git never converts a byte. A verse whose text holds CRLF line endings is the same
/// bytes in its blob.
#[test]
fn loss_autocrlf_never_rewrites_bytes() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant\r\nof God\r\nand an apostle").unwrap();
    let report = checkpoint(&handle);
    handle.close().unwrap();
    let on_disk = rig.fs.peek(&folder.join("ingredients/TIT.usfm")).unwrap();
    assert!(
        on_disk.windows(2).any(|w| w == b"\r\n"),
        "the file keeps the verse's CRLF"
    );
    let repo = repo_of(&rig.fs, &folder);
    let commit = git2::Oid::from_bytes(report.commit.unwrap().as_bytes()).unwrap();
    assert_eq!(commit_files(&repo, commit)["ingredients/TIT.usfm"], on_disk);
    assert_commits_valid(&rig.fs, &folder);
}

/// L88 (the local half): an entry of HEAD's tree the app does not own, a file at the root whose
/// name breaks the grammar or a dot folder from Door43, stays in every later commit.
#[test]
fn a_checkpoint_keeps_foreign_entries_of_head() {
    let (rig, folder) = titus();
    let repo = repo_of(&rig.fs, &folder);
    let head = repo.find_commit(commits_of(&repo)[0]).unwrap();
    let blob = repo.blob(b"a style guide").unwrap();
    let inner = repo.blob(b"workflow").unwrap();
    let mut gitea = repo.treebuilder(None).unwrap();
    gitea.insert("x", inner, 0o100_644).unwrap();
    let gitea = gitea.write().unwrap();
    let mut root = repo.treebuilder(Some(&head.tree().unwrap())).unwrap();
    root.insert("Style Guide.pdf", blob, 0o100_644).unwrap();
    root.insert(".gitea", gitea, 0o040_000).unwrap();
    let tree = repo.find_tree(root.write().unwrap()).unwrap();
    let sig = git2::Signature::now("A teammate", "a@example.org").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "added on Door43", &tree, &[&head])
        .unwrap();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    let report = checkpoint(&handle);
    let commit = git2::Oid::from_bytes(report.commit.unwrap().as_bytes()).unwrap();
    let files = commit_files(&repo, commit);
    assert_eq!(files["Style Guide.pdf"], b"a style guide");
    assert_eq!(files[".gitea/x"], b"workflow");
    handle.close().unwrap();
    assert_commits_valid(&rig.fs, &folder);
}

/// R21: the files, the tree and the commit object are the writer's; a save made while the
/// checkpoint holds before it moves the branch is durable at once.
#[test]
fn a_save_is_durable_while_the_tree_is_built() {
    let hooks = Arc::new(Hooks::default());
    let rig = rig_with_hooks(FaultFs::new(), hooks.clone());
    let folder = create_titus(&rig, "Titus");
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    hooks.hold(Point::BeforeHeadUpdate);
    let checkpoint = handle.checkpoint(CheckpointReason::LeavingTranslate);
    hooks.wait_reached(Point::BeforeHeadUpdate, 1);
    for v in 2..8 {
        let reply = save_verse(
            &handle,
            1,
            v,
            &format!("saved while the tree was built {v}"),
        );
        assert_eq!(reply.unwrap().stored, Stored::Durable);
    }
    assert!(!checkpoint.is_ready(), "the branch has not moved");
    hooks.release(Point::BeforeHeadUpdate);
    let report = checkpoint.wait().unwrap();
    assert!(report.commit.is_some());
    // The saves made meanwhile are not in that commit: the next checkpoint has them.
    let next = checkpoint_again(&handle);
    assert!(next.commit.is_some());
    handle.close().unwrap();
    assert_commits_valid(&rig.fs, &folder);
}

fn checkpoint_again(handle: &ProjectHandle) -> tc_store::CheckpointReport {
    handle
        .checkpoint(CheckpointReason::Periodic)
        .wait()
        .unwrap()
}

/// What one Psalms save and a checkpoint cost on a copy of a corpus project: the trees and
/// blobs written, and the files read.
struct CheckpointCost {
    counters: tc_store::StoreCounters,
    opened: Vec<std::path::PathBuf>,
    psalms_files: usize,
}

fn psalms_checkpoint(corpus: &OnDisk) -> CheckpointCost {
    let fs = corpus.fs.copy();
    fs.advance(Duration::from_secs(5));
    let rig = rig_as(fs.clone(), APP, "host-1");
    let handle = open(&rig, &corpus.folder);
    // The first checkpoint of a project made without a history stores every pack: not what is
    // measured.
    checkpoint(&handle);
    fs.advance(Duration::from_secs(5));
    let shown = handle
        .mint(
            &RecordKey::Verse(
                tc_journal::BookId::Psalms,
                tc_journal::ChapterNo::new(23).unwrap(),
                tc_journal::VerseNo::new(1).unwrap(),
            ),
            None,
        )
        .unwrap();
    handle
        .submit(Command::SaveVerse {
            shown,
            words: "The LORD is my shepherd, I shall not want".into(),
        })
        .wait()
        .unwrap();
    handle
        .leave(tc_store::Leave::Book(tc_journal::BookId::Psalms))
        .wait()
        .unwrap();
    handle.wait_idle();
    fs.clear_log();
    let before = handle.counters();
    let report = checkpoint(&handle);
    assert!(report.commit.is_some(), "{report:?}");
    let counters = handle.counters().since(&before);
    let opened = fs.opened();
    let psalms_files = handle
        .snapshot()
        .fold()
        .book_files(tc_journal::BookId::Psalms)
        .len();
    handle.close().unwrap();
    CheckpointCost {
        counters,
        opened,
        psalms_files,
    }
}

/// §14: a checkpoint's tree build touches only the changed directories and the changed files,
/// the same on a corpus ten times the size.
#[test]
fn a_checkpoint_after_one_save_writes_only_changed_trees() {
    let costs: Vec<CheckpointCost> = [psalms(), psalms_x10()]
        .into_iter()
        .map(psalms_checkpoint)
        .collect();
    for cost in &costs {
        let c = &cost.counters;
        assert!(
            c.git_trees_written >= 2 && c.git_trees_written <= 10,
            "{c:?}"
        );
        // The book's files that changed, the pack the seal wrote, and metadata.json.
        assert!(
            c.git_blobs_written <= cost.psalms_files as u64 + 3,
            "{c:?} for {} Psalms files",
            cost.psalms_files
        );
        assert_eq!(c.git_commits_written, 1, "{c:?}");
        for path in &cost.opened {
            let text = path.to_string_lossy();
            if let Some(name) = text.rsplit('/').next()
                && name.ends_with(".usfm")
            {
                assert_eq!(name, "PSA.usfm", "an unchanged book was read: {text}");
            }
        }
    }
    // Ten times the project, the same work: a consolidation the larger project's seal does
    // may touch one more month folder of the packs, and nothing else differs.
    let (small, large) = (&costs[0].counters, &costs[1].counters);
    assert!(
        small.git_trees_written.abs_diff(large.git_trees_written) <= 2,
        "{small:?} {large:?}"
    );
    assert!(
        small.git_blobs_written.abs_diff(large.git_blobs_written) <= 2,
        "{small:?} {large:?}"
    );
}

/// `bytes` of a USFM file with the text of TIT c:v replaced.
fn with_verse(bytes: &[u8], c: u16, v: u16, text: &str) -> Vec<u8> {
    let s = String::from_utf8(bytes.to_vec()).unwrap();
    let chapter = s.find(&format!("\n\\c {c}\n")).unwrap();
    let marker = format!("\\v {v} ");
    let at = chapter + s[chapter..].find(&marker).unwrap() + marker.len();
    let end = at + s[at..].find('\n').unwrap();
    format!("{}{}{}", &s[..at], text, &s[end..]).into_bytes()
}

/// §5.13 C3: a file whose outside edit waits to be recorded was not written over, but the
/// tree still takes it from the snapshot, never from an older checkpoint's blob: the commit
/// holds the saves its packs hold.
#[test]
fn a_commit_takes_a_file_waiting_for_an_outside_edit_from_the_snapshot() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 3, "my true child").unwrap();
    assert!(checkpoint(&handle).commit.is_some());
    save_verse(&handle, 1, 1, "saved in the app").unwrap();
    let usfm = folder.join("ingredients/TIT.usfm");
    let edited = with_verse(
        &rig.fs.peek(&usfm).unwrap(),
        1,
        2,
        "to Titus, edited by a program",
    );
    rig.fs.put_unseen(&usfm, &edited, None);
    let report = checkpoint(&handle);
    let commit = git2::Oid::from_bytes(report.commit.expect("a commit").as_bytes()).unwrap();
    let repo = repo_of(&rig.fs, &folder);
    let text =
        String::from_utf8(commit_files(&repo, commit)["ingredients/TIT.usfm"].clone()).unwrap();
    assert!(
        text.contains("saved in the app"),
        "the commit lacks a save its packs hold"
    );
    assert_eq!(
        tree_differs_from_its_packs(
            &repo,
            commit,
            project_of(&rig.fs, &folder),
            tc_journal::fold::ProjectKind::Bible
        ),
        Vec::<String>::new()
    );
    handle.wait_idle();
    // The outside edit is recorded all the same, and the file shows both.
    assert_eq!(
        words(&handle, 1, 2).as_deref(),
        Some("to Titus, edited by a program")
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}

/// The pending files of the project folder, as paths.
fn pending_files(rig: &Rig, folder: &Path) -> Vec<std::path::PathBuf> {
    rig.fs
        .files_under(&folder.join(".tc/pending"))
        .into_keys()
        .collect()
}

/// §5.11 K1, §5.13 C3: a pending file the seal cannot read (a program holds it open) holds a
/// save the snapshot shows and no pack holds, so the checkpoint makes no commit: a tree that
/// shows the save would reach teammates with packs that lack it (§11.2 D1 would take it for
/// a Door43 edit). The person is told; the next checkpoint seals the file and commits.
#[test]
fn a_commit_waits_for_a_pending_file_the_seal_could_not_read() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 3, "my true child").unwrap();
    assert!(checkpoint(&handle).commit.is_some());
    let repo = repo_of(&rig.fs, &folder);
    let commits = commits_of(&repo).len();
    save_verse(&handle, 1, 1, "saved while an antivirus holds the file").unwrap();
    let pending = pending_files(&rig, &folder);
    assert_eq!(pending.len(), 1, "{pending:?}");
    rig.fs.set_unreadable(&pending[0], true);
    let report = checkpoint(&handle);
    assert_eq!(report.commit, None);
    assert_eq!(report.skipped, Some(CheckpointSkip::SealFailed));
    assert_eq!(commits_of(&repo).len(), commits, "no commit was made");
    let told: Vec<String> = notices(&handle)
        .into_iter()
        .filter_map(|n| match n {
            Notice::FileUnreadable { path, .. } => Some(path.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(told.len(), 1, "told once: {told:?}");
    assert!(pending[0].ends_with(&told[0]), "{told:?} {pending:?}");
    // Asking again does not tell again.
    assert_eq!(
        checkpoint(&handle).skipped,
        Some(CheckpointSkip::SealFailed)
    );
    assert_eq!(
        notices(&handle)
            .iter()
            .filter(|n| matches!(n, Notice::FileUnreadable { .. }))
            .count(),
        1
    );
    // The program lets go: the next checkpoint seals the file and commits what its packs hold.
    rig.fs.set_unreadable(&pending[0], false);
    let report = checkpoint(&handle);
    let commit = git2::Oid::from_bytes(report.commit.expect("a commit").as_bytes()).unwrap();
    assert!(
        String::from_utf8(commit_files(&repo, commit)["ingredients/TIT.usfm"].clone())
            .unwrap()
            .contains("saved while an antivirus holds the file")
    );
    assert_eq!(
        tree_differs_from_its_packs(
            &repo,
            commit,
            project_of(&rig.fs, &folder),
            tc_journal::fold::ProjectKind::Bible
        ),
        Vec::<String>::new()
    );
    assert!(
        notices(&handle)
            .iter()
            .all(|n| !matches!(n, Notice::FileUnreadable { .. })),
        "the notice goes when the file is sealed"
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}

/// A pending file a newer app wrote is kept and not read: the fold holds nothing of it, so
/// no commit lacks anything for it, and the seal leaves it alone without stopping one.
#[test]
fn a_pending_file_of_a_newer_format_does_not_stop_a_commit() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 3, "my true child").unwrap();
    let pending = pending_files(&rig, &folder).remove(0);
    // The header of this file with the framing version a newer app writes, hashed again.
    let text = String::from_utf8(rig.fs.peek(&pending).unwrap()).unwrap();
    let header = text.lines().next().unwrap().split('\t').next().unwrap();
    let header = header.replacen("\"pending\":1", "\"pending\":2", 1);
    let newer = format!(
        "{header}\t{}\n{{\"a body this app does not read\":true}}\n",
        sha2::Sha256::digest(header.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    handle.close().unwrap();
    let kept = folder.join(".tc/pending/from-a-newer-app.batch");
    rig.fs.put(&kept, newer.as_bytes(), None);
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "saved beside a newer app's file").unwrap();
    let report = checkpoint(&handle);
    assert!(report.commit.is_some(), "{report:?}");
    assert_eq!(
        rig.fs.peek(&kept),
        Some(newer.into_bytes()),
        "kept as it is"
    );
    handle.close().unwrap();
    assert_commits_valid(&rig.fs, &folder);
}

/// A pending file that was valid when the project opened and no longer verifies (its bytes
/// were changed by something else): the seal cannot take its lines, so no commit shows them.
#[test]
fn a_commit_waits_for_a_pending_file_that_no_longer_verifies() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 3, "my true child").unwrap();
    assert!(checkpoint(&handle).commit.is_some());
    save_verse(&handle, 1, 1, "saved before the file was damaged").unwrap();
    let pending = pending_files(&rig, &folder).remove(0);
    let mut bytes = rig.fs.peek(&pending).unwrap();
    let last = bytes.len() - 3;
    bytes[last] ^= 0x20;
    rig.fs.put_unseen(&pending, &bytes, None);
    let report = checkpoint(&handle);
    assert_eq!(report.commit, None);
    assert_eq!(report.skipped, Some(CheckpointSkip::SealFailed));
    assert!(
        notices(&handle)
            .iter()
            .any(|n| matches!(n, Notice::FileUnreadable { .. }))
    );
    assert_eq!(
        rig.fs.peek(&pending),
        Some(bytes),
        "the file is never removed"
    );
    handle.close().unwrap();
}

/// The batch of an outside edit of `metadata.json` is a pending file whose header names
/// `metadata.json` as the file it reconciles. The seal reads it back like any other file, so the
/// checkpoint seals the edit into a pack and commits a tree that its packs account for.
#[test]
fn an_outside_edit_of_metadata_json_is_sealed_and_committed_with_its_packs() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let metadata = folder.join("metadata.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&rig.fs.peek(&metadata).unwrap()).unwrap();
    document["identification"]["name"]["en"] =
        serde_json::Value::from("Titus, renamed by a text editor");
    rig.fs
        .put_unseen(&metadata, document.to_string().as_bytes(), None);
    handle.focus().wait().unwrap();
    handle.wait_idle();
    assert!(
        notices(&handle)
            .iter()
            .any(|n| matches!(n, Notice::OutsideEditRecorded { .. })),
        "{:?}",
        notices(&handle)
    );
    let report = checkpoint(&handle);
    let commit = git2::Oid::from_bytes(report.commit.expect("a commit").as_bytes()).unwrap();
    assert_eq!(report.sealed.len(), 1, "{report:?}");
    let repo = repo_of(&rig.fs, &folder);
    let files = commit_files(&repo, commit);
    let committed: serde_json::Value = serde_json::from_slice(&files["metadata.json"]).unwrap();
    assert_eq!(
        committed["identification"]["name"]["en"],
        "Titus, renamed by a text editor"
    );
    assert_eq!(
        tree_differs_from_its_packs(
            &repo,
            commit,
            project_of(&rig.fs, &folder),
            tc_journal::fold::ProjectKind::Bible
        ),
        Vec::<String>::new()
    );
    assert!(
        pending_files(&rig, &folder).is_empty(),
        "the batch is sealed"
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}

/// The notices that say a pending file's saves were written again from memory.
fn written_again(handle: &ProjectHandle) -> Vec<(String, u32)> {
    notices(handle)
        .into_iter()
        .filter_map(|n| match n {
            Notice::PendingWrittenAgain { file, changes } => Some((file.to_string(), changes)),
            _ => None,
        })
        .collect()
}

/// §5.11 K1, §5.13 C3: a pending file of this session's save deleted by another program (a
/// cloud client, an antivirus) before the seal. The session holds the save's lines in memory:
/// the seal writes them into its pack, the person is told, the commit's tree is what its packs
/// hold, and the save is there after the next open.
#[test]
fn a_pending_file_deleted_outside_is_sealed_from_memory() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 3, "my true child").unwrap();
    assert!(checkpoint(&handle).commit.is_some());
    save_verse(&handle, 1, 1, "saved before the file was deleted").unwrap();
    let pending = pending_files(&rig, &folder);
    assert_eq!(pending.len(), 1, "{pending:?}");
    rig.fs.delete(&pending[0]).unwrap();
    let report = checkpoint(&handle);
    assert_eq!(report.skipped, None, "{report:?}");
    assert_eq!(report.sealed.len(), 1, "{report:?}");
    let commit = git2::Oid::from_bytes(report.commit.expect("a commit").as_bytes()).unwrap();
    let repo = repo_of(&rig.fs, &folder);
    assert_eq!(
        tree_differs_from_its_packs(
            &repo,
            commit,
            project_of(&rig.fs, &folder),
            tc_journal::fold::ProjectKind::Bible
        ),
        Vec::<String>::new()
    );
    let told = written_again(&handle);
    assert_eq!(told.len(), 1, "{:?}", notices(&handle));
    assert!(pending[0].ends_with(&told[0].0), "{told:?} {pending:?}");
    assert!(told[0].1 >= 1);
    handle.close().unwrap();
    let handle = open(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before the file was deleted")
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}

/// K1 compares the changes it read with the ones the fold took from the file: a pending file
/// of this session cut short at a line boundary still verifies, and the lines it lost are
/// sealed from memory.
#[test]
fn a_pending_file_cut_short_at_a_line_is_sealed_from_memory() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 3, "my true child").unwrap();
    assert!(checkpoint(&handle).commit.is_some());
    save_verse(&handle, 1, 1, "saved before the file was cut short").unwrap();
    let pending = pending_files(&rig, &folder).remove(0);
    let bytes = rig.fs.peek(&pending).unwrap();
    let header = bytes.iter().position(|b| *b == b'\n').unwrap() + 1;
    rig.fs.put_unseen(&pending, &bytes[..header], None);
    let report = checkpoint(&handle);
    let commit = git2::Oid::from_bytes(report.commit.expect("a commit").as_bytes()).unwrap();
    let repo = repo_of(&rig.fs, &folder);
    assert!(
        String::from_utf8(commit_files(&repo, commit)["ingredients/TIT.usfm"].clone())
            .unwrap()
            .contains("saved before the file was cut short")
    );
    assert_eq!(
        tree_differs_from_its_packs(
            &repo,
            commit,
            project_of(&rig.fs, &folder),
            tc_journal::fold::ProjectKind::Bible
        ),
        Vec::<String>::new()
    );
    assert_eq!(written_again(&handle).len(), 1, "{:?}", notices(&handle));
    handle.close().unwrap();
    let handle = open(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before the file was cut short")
    );
    handle.close().unwrap();
    assert_commits_valid(&rig.fs, &folder);
}

/// A pending file the open read, deleted by another program before the seal: the fold reads
/// its values through the handle the open kept, so the seal writes them into its pack, the
/// person is told, and the save is there after the next open.
#[test]
fn a_pending_file_read_at_the_open_and_deleted_is_sealed_from_the_open_handle() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "saved before a crash").unwrap();
    let fs = rig.fs.crash(false);
    drop(handle);
    let rig = rig_as(fs, APP, "host-1");
    let handle = open(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before a crash")
    );
    let pending = pending_files(&rig, &folder);
    assert_eq!(pending.len(), 1, "{pending:?}");
    rig.fs.delete(&pending[0]).unwrap();
    let report = checkpoint(&handle);
    let commit = git2::Oid::from_bytes(report.commit.expect("a commit").as_bytes()).unwrap();
    let repo = repo_of(&rig.fs, &folder);
    assert_eq!(
        tree_differs_from_its_packs(
            &repo,
            commit,
            project_of(&rig.fs, &folder),
            tc_journal::fold::ProjectKind::Bible
        ),
        Vec::<String>::new()
    );
    assert_eq!(written_again(&handle).len(), 1, "{:?}", notices(&handle));
    handle.close().unwrap();
    let handle = open(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before a crash")
    );
    handle.close().unwrap();
    assert_commits_valid(&rig.fs, &folder);
}

/// A pending file the open read, replaced by another program with a copy cut short at a line:
/// the open's handle still reads the lines the new file lost, so the seal writes them into its
/// pack, and the fold reads their values on after the file goes.
#[test]
fn a_pending_file_read_at_the_open_and_replaced_cut_short_is_sealed_from_the_open_handle() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "saved before a crash").unwrap();
    let fs = rig.fs.crash(false);
    drop(handle);
    let rig = rig_as(fs, APP, "host-1");
    let handle = open(&rig, &folder);
    let pending = pending_files(&rig, &folder).remove(0);
    let bytes = rig.fs.peek(&pending).unwrap();
    let header = bytes.iter().position(|b| *b == b'\n').unwrap() + 1;
    rig.fs.put_unseen(&pending, &bytes[..header], None);
    let report = checkpoint(&handle);
    let commit = git2::Oid::from_bytes(report.commit.expect("a commit").as_bytes()).unwrap();
    let repo = repo_of(&rig.fs, &folder);
    assert_eq!(
        tree_differs_from_its_packs(
            &repo,
            commit,
            project_of(&rig.fs, &folder),
            tc_journal::fold::ProjectKind::Bible
        ),
        Vec::<String>::new()
    );
    assert!(rig.fs.peek(&pending).is_none(), "the sealed file goes");
    assert_eq!(written_again(&handle).len(), 1, "{:?}", notices(&handle));
    // The value is read again after the seal (a later save of another verse renders the book).
    save_verse(&handle, 1, 2, "a later save").unwrap();
    handle.leave(tc_store::Leave::Mode).wait().unwrap();
    handle.wait_idle();
    let usfm =
        String::from_utf8(rig.fs.peek(&folder.join("ingredients/TIT.usfm")).unwrap()).unwrap();
    assert!(usfm.contains("saved before a crash"), "{usfm}");
    assert!(
        notices(&handle)
            .iter()
            .all(|n| !matches!(n, Notice::MaterializeBlocked { .. })),
        "{:?}",
        notices(&handle)
    );
    handle.close().unwrap();
    let handle = open(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before a crash")
    );
    handle.close().unwrap();
    assert_commits_valid(&rig.fs, &folder);
}

/// A pending file the open read, cut short in place by another program before the seal: the
/// lines it lost are in neither the file nor memory, so the seal leaves the file as it is, no
/// commit shows the save, and the person is told.
#[test]
fn a_pending_file_read_at_the_open_and_cut_short_in_place_stops_the_commit() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "saved before a crash").unwrap();
    let fs = rig.fs.crash(false);
    drop(handle);
    let rig = rig_as(fs, APP, "host-1");
    let handle = open(&rig, &folder);
    let pending = pending_files(&rig, &folder).remove(0);
    let bytes = rig.fs.peek(&pending).unwrap();
    let header = bytes.iter().position(|b| *b == b'\n').unwrap() + 1;
    rig.fs.cut_in_place(&pending, header);
    let repo = repo_of(&rig.fs, &folder);
    let commits = commits_of(&repo).len();
    let report = checkpoint(&handle);
    assert_eq!(report.commit, None, "{report:?}");
    assert_eq!(report.skipped, Some(CheckpointSkip::SealFailed));
    assert_eq!(commits_of(&repo).len(), commits);
    assert!(
        notices(&handle).iter().any(
            |n| matches!(n, Notice::FileUnreadable { path, .. } if pending.ends_with(&**path))
        ),
        "{:?}",
        notices(&handle)
    );
    assert_eq!(
        rig.fs.peek(&pending).map(|b| b.len()),
        Some(header),
        "the file is never removed"
    );
    drop(handle);
}
