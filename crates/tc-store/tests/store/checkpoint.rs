//! The checkpoint's commit (`docs/STORAGE.md` §5.13, §9.4, §16.6; APP-DESIGN R21): one commit
//! when something changed and none when nothing did, a tree built from the project and never
//! from a file another program holds, the expected packs restored or the commit refused, bytes
//! never converted, foreign entries kept, and a save never waiting for any of it.

use crate::common::*;
use crate::corpus::{OnDisk, psalms, psalms_larger};
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
    // A changed book file is held and asked about, whatever its size and time say
    // (delta §2.5: R10's own hold goes).
    assert!(
        notices(&handle).iter().any(|n| matches!(
            n,
            Notice::OutsideEditHeld { path, .. } if path.as_str() == "ingredients/TIT.usfm"
        )),
        "the storage check holds the file"
    );
    assert_eq!(handle.waiting_books().len(), 1);
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
    // The saves made meanwhile are not in that commit: the next checkpoint has them.
    let repo = repo_of(&rig.fs, &folder);
    let titus_of = |commit: Option<tc_journal::GitOid>| {
        let commit = git2::Oid::from_bytes(commit.expect("a commit").as_bytes()).unwrap();
        String::from_utf8(commit_files(&repo, commit)["ingredients/TIT.usfm"].clone()).unwrap()
    };
    let first = titus_of(report.commit);
    assert!(first.contains("Paul, a servant of God"), "{first}");
    let next = titus_of(checkpoint_again(&handle).commit);
    for v in 2..8 {
        let saved = format!("saved while the tree was built {v}");
        assert!(
            !first.contains(&saved),
            "{v} is in the tree built before it"
        );
        assert!(next.contains(&saved), "{v} never reached a commit: {next}");
    }
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
    let costs: Vec<CheckpointCost> = [psalms(), psalms_larger()]
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
            let text = slashed(path);
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
    // The outside edit waits for the person's answer; *Import* brings it in, and the file
    // shows both.
    assert_eq!(handle.waiting_books().len(), 1);
    assert_eq!(counts(&import_waiting(&handle)), vec![(1, 0)]);
    assert_eq!(
        words(&handle, 1, 2).as_deref(),
        Some("to Titus, edited by a program")
    );
    assert_eq!(words(&handle, 1, 1).as_deref(), Some("saved in the app"));
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
/// save of this session, which the fold holds in memory: the seal packs it from there, so
/// the commit shows nothing a pack lacks, and the file stays until it can be read. Damage to
/// the file meanwhile (another program cut it) can no longer take the save with it.
#[test]
fn a_save_whose_pending_file_the_seal_cannot_read_is_sealed_from_memory() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 3, "my true child").unwrap();
    assert!(checkpoint(&handle).commit.is_some());
    let repo = repo_of(&rig.fs, &folder);
    save_verse(&handle, 1, 1, "saved while an antivirus holds the file").unwrap();
    let pending = pending_files(&rig, &folder);
    assert_eq!(pending.len(), 1, "{pending:?}");
    rig.fs.set_unreadable(&pending[0], true);
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
    assert_eq!(
        pending_files(&rig, &folder),
        pending,
        "a file the seal cannot read is never removed"
    );
    // The program cuts the file it held: the save survives in the pack.
    let bytes = rig.fs.peek(&pending[0]).unwrap();
    rig.fs.set_unreadable(&pending[0], false);
    rig.fs.cut_in_place(&pending[0], bytes.len() / 2);
    handle.close().unwrap();
    let handle = open(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved while an antivirus holds the file")
    );
    let report = checkpoint(&handle);
    assert_ne!(
        report.skipped,
        Some(CheckpointSkip::SealFailed),
        "{report:?}"
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

/// A pending file of this session that no longer verifies (something else changed a byte of
/// its last line): the seal cannot take that line from the file, and takes it from the
/// session's memory, checked against its id. The commit's tree is what its packs hold, the
/// changed bytes are kept in `.tc/quarantine/`, and the person is told.
#[test]
fn a_pending_file_changed_inside_a_line_is_sealed_from_memory_and_kept_in_quarantine() {
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
    assert!(
        String::from_utf8(commit_files(&repo, commit)["ingredients/TIT.usfm"].clone())
            .unwrap()
            .contains("saved before the file was damaged")
    );
    assert_eq!(written_again(&handle).len(), 1, "{:?}", notices(&handle));
    assert_eq!(quarantined(&handle).len(), 1, "{:?}", notices(&handle));
    assert!(
        rig.fs
            .files_under(&folder.join(".tc/quarantine"))
            .values()
            .any(|kept| *kept == bytes),
        "the changed bytes are kept"
    );
    handle.close().unwrap();
    let handle = open(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before the file was damaged")
    );
    handle.close().unwrap();
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

/// A pending file a crash left: the next open seals it before it returns, so no later change
/// to the file can take the save, however old it is (ruling 2026-10-22-acceptable-loss,
/// LOSS-2), and the next checkpoint commits it.
#[test]
fn a_pending_file_a_crash_left_is_sealed_at_the_open() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "saved before a crash").unwrap();
    let fs = rig.fs.crash(false);
    drop(handle);
    let rig = rig_as(fs, APP, "host-1");
    assert_eq!(pending_files(&rig, &folder).len(), 1);
    let repo = repo_of(&rig.fs, &folder);
    let commits = commits_of(&repo).len();
    let handle = open(&rig, &folder);
    assert_eq!(
        pending_files(&rig, &folder),
        Vec::<std::path::PathBuf>::new()
    );
    assert!(a_version_holds(&handle, 1, 1, "saved before a crash"));
    assert!(checkpoint(&handle).commit.is_some());
    assert_eq!(commits_of(&repo).len(), commits + 1);
    handle.close().unwrap();
    assert_commits_valid(&rig.fs, &folder);
}

/// The open's seal of what a crash left fails (the disk refuses the pack's rename): the
/// pending file is kept, the open goes on writable, the save shows, and the folder is a valid
/// Scripture Burrito once the open's materialization is done (§6.1 O15, LOSS-2).
#[test]
fn a_seal_that_fails_at_the_open_keeps_the_project_writable() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "saved before a crash").unwrap();
    let fs = rig.fs.crash(false);
    drop(handle);
    // The same open on a copy tells which write point the pack's rename is.
    let probe = fs.copy();
    let probe_rig = rig_as(probe.clone(), APP, "host-1");
    probe.clear_log();
    let start = probe.ops();
    let probed = open(&probe_rig, &folder);
    let changes = folder.join("ingredients/tc/changes");
    let rename = probe
        .log()
        .iter()
        .find(|op| op.kind == tc_store::fault::OpKind::Rename && op.path.starts_with(&changes))
        .map(|op| op.index - start)
        .expect("the open seals into a pack");
    drop(probed);
    drop(probe_rig);
    let rig = rig_as(fs, APP, "host-1");
    rig.fs.fail_op(rename, std::io::ErrorKind::Other);
    let handle = open(&rig, &folder);
    assert_eq!(handle.facts().mode, tc_store::Mode::Writable);
    assert_eq!(
        pending_files(&rig, &folder).len(),
        1,
        "the pending file is kept"
    );
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before a crash")
    );
    handle.wait_idle();
    assert_valid_burrito(&rig.fs, &folder);
    save_verse(&handle, 1, 2, "a later save").unwrap();
    handle.close().unwrap();
    let handle = open(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before a crash")
    );
    assert_eq!(words(&handle, 1, 2).as_deref(), Some("a later save"));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// §5.11 K4: a pending file another program removes after K1 read it and before K4 removes it.
/// The verified pack holds every change of it already, so the seal goes on as if K4 had
/// removed it: the checkpoint commits, and the next checkpoint writes no second pack of the
/// same lines.
#[test]
fn a_pending_file_removed_outside_during_the_seal_ends_the_seal_as_removed() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 3, "my true child").unwrap();
    assert!(checkpoint(&handle).commit.is_some());
    save_verse(&handle, 1, 1, "saved before the file was removed").unwrap();
    let pending = pending_files(&rig, &folder);
    assert_eq!(pending.len(), 1, "{pending:?}");
    let packs_before = packs(&rig.fs, &folder).len();
    {
        let (fs, file) = (rig.fs.clone(), pending[0].clone());
        let steps = std::sync::Arc::new(std::sync::Mutex::new(0));
        rig.store.hooks().at_seal_step(move || {
            let mut steps = steps.lock().unwrap();
            *steps += 1;
            // After the pack is built, before it is written and before K4.
            if *steps == 2 {
                fs.delete(&file).unwrap();
            }
        });
    }
    let report = handle.checkpoint(CheckpointReason::LeavingTranslate).wait();
    rig.store.hooks().at_seal_step(|| {});
    let report = report.expect("the seal ends without an error");
    assert_eq!(report.skipped, None, "{report:?}");
    assert_eq!(report.sealed.len(), 1, "{report:?}");
    assert!(report.commit.is_some(), "{report:?}");
    assert_eq!(packs(&rig.fs, &folder).len(), packs_before + 1);
    let again = checkpoint(&handle);
    assert_eq!(again.sealed, Vec::<String>::new(), "{again:?}");
    assert_eq!(again.skipped, Some(CheckpointSkip::NoChanges), "{again:?}");
    assert_eq!(packs(&rig.fs, &folder).len(), packs_before + 1);
    handle.close().unwrap();
    let handle = open(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before the file was removed")
    );
    handle.close().unwrap();
    assert_commits_valid(&rig.fs, &folder);
}

/// The notices that say a pending file was moved to `.tc/quarantine/`.
fn quarantined(handle: &ProjectHandle) -> Vec<(String, u32)> {
    notices(handle)
        .into_iter()
        .filter_map(|n| match n {
            Notice::QuarantinedPending { file, salvaged, .. } => Some((file.to_string(), salvaged)),
            _ => None,
        })
        .collect()
}

/// §5.11 K1: a pending file of this session cut short inside a line (another program wrote it
/// back torn) no longer verifies, which is as likely as a cut at a line. The session holds the
/// save's line in memory, so the seal writes it into its pack as for a file cut at a line, the
/// torn file goes to `.tc/quarantine/` with its bytes, and the save is in the project and in the
/// book file after the next open.
#[test]
fn a_pending_file_cut_inside_a_line_is_sealed_from_memory() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 3, "my true child").unwrap();
    assert!(checkpoint(&handle).commit.is_some());
    save_verse(&handle, 1, 1, "saved before the file was torn").unwrap();
    handle.leave(tc_store::Leave::Mode).wait().unwrap();
    handle.wait_idle();
    let pending = pending_files(&rig, &folder).remove(0);
    let bytes = rig.fs.peek(&pending).unwrap();
    let torn = bytes[..bytes.len() - 20].to_vec();
    rig.fs.put_unseen(&pending, &torn, None);
    let report = checkpoint(&handle);
    assert_eq!(report.skipped, None, "{report:?}: {:?}", notices(&handle));
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
    assert_eq!(written_again(&handle).len(), 1, "{:?}", notices(&handle));
    let kept = quarantined(&handle);
    assert_eq!(kept.len(), 1, "{:?}", notices(&handle));
    assert!(pending.ends_with(&kept[0].0), "{kept:?} {pending:?}");
    assert!(
        rig.fs.peek(&pending).is_none(),
        "the torn file left .tc/pending"
    );
    assert!(
        rig.fs
            .files_under(&folder.join(".tc/quarantine"))
            .values()
            .any(|held| *held == torn),
        "the torn bytes are kept"
    );
    handle.close().unwrap();
    let handle = open(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before the file was torn")
    );
    let usfm =
        String::from_utf8(rig.fs.peek(&folder.join("ingredients/TIT.usfm")).unwrap()).unwrap();
    assert!(usfm.contains("saved before the file was torn"), "{usfm}");
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}

/// K1: another program puts a pending file of a newer format at the name of one of this
/// session's saves. The fold took the save from the file this session wrote, and holds its
/// line in memory: the seal writes it into its pack, and the newer file stays as it is, never
/// removed and never read (§4.2).
#[test]
fn a_pending_file_replaced_by_one_of_a_newer_format_is_sealed_from_memory_and_kept() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 3, "my true child").unwrap();
    assert!(checkpoint(&handle).commit.is_some());
    save_verse(&handle, 1, 1, "saved before the file was replaced").unwrap();
    let pending = pending_files(&rig, &folder).remove(0);
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
    rig.fs.put_unseen(&pending, newer.as_bytes(), None);
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
    assert_eq!(rig.fs.peek(&pending), Some(newer.clone().into_bytes()));
    // The next checkpoint leaves it alone too, and finds no second writer in it.
    save_verse(&handle, 1, 2, "a later save").unwrap();
    let again = checkpoint(&handle);
    assert!(again.commit.is_some(), "{again:?}");
    assert_eq!(rig.fs.peek(&pending), Some(newer.into_bytes()));
    handle.close().unwrap();
    let handle = open(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before the file was replaced")
    );
    handle.close().unwrap();
    assert_commits_valid(&rig.fs, &folder);
}

/// Whether a version of the verse `c:v` in the project holds `text`.
fn a_version_holds(handle: &ProjectHandle, c: u16, v: u16, text: &str) -> bool {
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    fold.history(&verse(c, v)).iter().any(|id| {
        matches!(
            fold.body(id).map(|b| b.record),
            Some(Record::Verse {
                value: tc_journal::record::VerseValue::Text { words, .. },
                ..
            }) if words == text
        )
    })
}

/// The notices of an outside edit of `TIT.usfm` recorded, with the copy each keeps.
fn recorded_edits_of_titus(handle: &ProjectHandle) -> Vec<(u32, bool, Option<String>)> {
    handle
        .notices()
        .into_iter()
        .filter_map(|n| match n.notice {
            Notice::OutsideEditRecorded {
                path,
                changes,
                unknown_base,
                ..
            } if path.as_str() == "ingredients/TIT.usfm" => Some((changes, unknown_base, n.copy)),
            _ => None,
        })
        .collect()
}

/// §6.2: the book file is the only copy of a save whose pending file lost it. The open must not
/// take it for the app's own stale bytes: its heads file names the save, which the fold no longer
/// holds, so a copy is kept with `why: lost` and nothing comes back on its own (H22 part 1).
/// *Bring it back* imports the copy, and the save is a version of its verse again. For a record
/// whose head the journal lost, the base is the heads the fold held when the copy was kept, so
/// the save comes back over them, and a version saved since is in conflict with it.
fn assert_the_book_file_gave_the_save_back(
    rig: &Rig,
    folder: &Path,
    handle: &ProjectHandle,
    text: &str,
) {
    handle.wait_idle();
    let lost: Vec<tc_store::Kept> = handle
        .kept_copies()
        .into_iter()
        .filter(|k| k.why == tc_store::KeptWhy::Lost)
        .collect();
    assert_eq!(
        lost.len(),
        1,
        "{:?} {:?}",
        handle.kept_copies(),
        notices(handle)
    );
    let kept = rig
        .fs
        .peek(&folder.join(&lost[0].copy))
        .expect("the copy is there");
    assert!(String::from_utf8_lossy(&kept).contains(text));
    assert!(
        recorded_edits_of_titus(handle).is_empty(),
        "nothing comes back on its own: {:?}",
        notices(handle)
    );
    // *Bring it back* (H22 part 1): the normal import of the copy.
    handle.import_kept(&lost[0].copy).wait().unwrap();
    handle.wait_idle();
    assert!(
        a_version_holds(handle, 1, 1, text),
        "the save is lost: {:?}, {:?}",
        words(handle, 1, 1),
        notices(handle)
    );
    assert!(
        handle.kept_copies().is_empty(),
        "{:?}",
        handle.kept_copies()
    );
}

fn verse_text_conflicts(handle: &ProjectHandle) -> usize {
    handle
        .snapshot()
        .fold()
        .conflicts()
        .filter(|c| matches!(c.kind, tc_journal::fold::ConflictKind::VerseText))
        .count()
}

/// §6.2: a pending file a crash left, cut short in place by another program before the next
/// open, so that the save it held is in no journal file. The open finds the book file showing
/// the save, and keeps a copy to bring back instead of writing the book over without it.
#[test]
fn a_book_file_holding_a_save_its_pending_file_lost_in_place_is_recorded_at_the_next_open() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "saved before a crash").unwrap();
    handle.leave(tc_store::Leave::Mode).wait().unwrap();
    handle.wait_idle();
    let fs = rig.fs.crash(false);
    drop(handle);
    let rig = rig_as(fs, APP, "host-1");
    let usfm = folder.join("ingredients/TIT.usfm");
    assert!(String::from_utf8_lossy(&rig.fs.peek(&usfm).unwrap()).contains("saved before a crash"));
    let pending = pending_files(&rig, &folder).remove(0);
    let bytes = rig.fs.peek(&pending).unwrap();
    let header = bytes.iter().position(|b| *b == b'\n').unwrap() + 1;
    rig.fs.cut_in_place(&pending, header);
    let handle = open(&rig, &folder);
    assert_the_book_file_gave_the_save_back(&rig, &folder, &handle, "saved before a crash");
    // The verse had only the seed's text before, which a person's text replaces.
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before a crash")
    );
    let usfm = String::from_utf8(rig.fs.peek(&usfm).unwrap()).unwrap();
    assert!(usfm.contains("saved before a crash"), "{usfm}");
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// §6.2: a save is written to the book file, another program deletes its pending file, and the
/// process is killed before any seal. The next open finds the book file showing a save no journal
/// file holds, and keeps a copy to bring back instead of writing the book over without
/// it.
#[test]
fn a_book_file_holding_a_save_whose_pending_file_was_deleted_before_a_crash_is_recorded() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "the text before").unwrap();
    assert!(checkpoint(&handle).commit.is_some());
    save_verse(&handle, 1, 1, "saved before the file was deleted").unwrap();
    handle.leave(tc_store::Leave::Mode).wait().unwrap();
    handle.wait_idle();
    let usfm = folder.join("ingredients/TIT.usfm");
    assert!(
        String::from_utf8_lossy(&rig.fs.peek(&usfm).unwrap())
            .contains("saved before the file was deleted")
    );
    let pending = pending_files(&rig, &folder);
    assert_eq!(pending.len(), 1, "{pending:?}");
    rig.fs.delete(&pending[0]).unwrap();
    let fs = rig.fs.crash(false);
    drop(handle);
    let rig = rig_as(fs, APP, "host-1");
    let handle = open(&rig, &folder);
    assert_the_book_file_gave_the_save_back(
        &rig,
        &folder,
        &handle,
        "saved before the file was deleted",
    );
    // Nothing was saved over the verse since: the save comes back over the text before, with
    // no question, and the text before stays in History.
    assert!(a_version_holds(&handle, 1, 1, "the text before"));
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before the file was deleted")
    );
    assert_eq!(verse_text_conflicts(&handle), 0);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// §6.2: a save lost from its pending file, and the heads file of the book file that shows it
/// put back by another program as it was before the save, so the ledger keeps no heads of the
/// bytes in the folder: the next open cannot say the fold holds what they show, so it records
/// them (a copy kept), never writing the book over without the save. Once recorded, a later
/// open records nothing again.
#[test]
fn a_book_file_whose_heads_were_put_back_is_recorded_not_written_over() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    let heads = folder.join(tc_store::layout::heads_file("ingredients/TIT.usfm"));
    let older = rig.fs.peek(&heads).expect("a heads file");
    save_verse(&handle, 1, 1, "saved before a crash").unwrap();
    handle.leave(tc_store::Leave::Mode).wait().unwrap();
    handle.wait_idle();
    let fs = rig.fs.crash(false);
    drop(handle);
    let rig = rig_as(fs, APP, "host-1");
    let pending = pending_files(&rig, &folder).remove(0);
    let bytes = rig.fs.peek(&pending).unwrap();
    let header = bytes.iter().position(|b| *b == b'\n').unwrap() + 1;
    rig.fs.cut_in_place(&pending, header);
    rig.fs.put_unseen(&heads, &older, None);
    let handle = open(&rig, &folder);
    assert_the_book_file_gave_the_save_back(&rig, &folder, &handle, "saved before a crash");
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before a crash")
    );
    handle.close().unwrap();
    let handle = open(&rig, &folder);
    assert_eq!(recorded_edits_of_titus(&handle), Vec::new());
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before a crash")
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// Whether a version of the project's name holds `text`.
fn a_name_holds(handle: &ProjectHandle, text: &str) -> bool {
    let key = RecordKey::ProjectMeta(tc_journal::MetaField::Name);
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    fold.history(&key).iter().any(|id| {
        matches!(
            fold.body(id).map(|b| b.record),
            Some(Record::ProjectMeta { value: Some(value), .. })
                if format!("{value:?}").contains(text)
        )
    })
}

/// §6.2 for `metadata.json`: a project detail saved, written to `metadata.json`, and lost from
/// the journal (another program deleted its pending file before a crash). The open must not
/// write `metadata.json` over as stale: it holds the only copy of the detail, which is recorded
/// (a copy kept, the person told) and stands beside the name before it.
#[test]
fn metadata_json_holding_a_project_detail_whose_pending_file_was_deleted_is_recorded() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 3, "my true child").unwrap();
    assert!(checkpoint(&handle).commit.is_some());
    let name = RecordKey::ProjectMeta(tc_journal::MetaField::Name);
    handle
        .submit(Command::SetMeta {
            shown: handle.mint(&name, None).unwrap(),
            value: Some(tc_journal::Json::Obj(tc_journal::JsonMap::from([(
                "en".to_owned(),
                tc_journal::Json::from("Titus, a name saved before a crash"),
            )]))),
        })
        .wait()
        .unwrap();
    handle.leave(tc_store::Leave::Mode).wait().unwrap();
    handle.wait_idle();
    let meta = folder.join("metadata.json");
    let written = rig.fs.peek(&meta).unwrap();
    assert!(String::from_utf8_lossy(&written).contains("Titus, a name saved before a crash"));
    let pending = pending_files(&rig, &folder);
    assert_eq!(pending.len(), 1, "{pending:?}");
    rig.fs.delete(&pending[0]).unwrap();
    let fs = rig.fs.crash(false);
    drop(handle);
    let rig = rig_as(fs, APP, "host-1");
    let handle = open(&rig, &folder);
    handle.wait_idle();
    assert!(
        a_name_holds(&handle, "Titus, a name saved before a crash"),
        "the detail is lost: {:?}",
        notices(&handle)
    );
    let copies: Vec<String> = handle
        .notices()
        .into_iter()
        .filter(|n| matches!(n.notice, Notice::MetadataEditedOutside { .. }))
        .filter_map(|n| n.copy)
        .collect();
    assert_eq!(copies.len(), 1, "{:?}", notices(&handle));
    assert_eq!(rig.fs.peek(&folder.join(&copies[0])), Some(written));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// The same loss, and the book file cannot be read when the open classifies it (a program holds
/// it): the compare-and-swap that reads it later asks what the open would have asked, and
/// keeps a copy to bring back instead of writing the book over without the save.
#[test]
fn a_book_file_the_open_could_not_read_is_asked_the_same_at_its_compare_and_swap() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "the text before").unwrap();
    assert!(checkpoint(&handle).commit.is_some());
    save_verse(&handle, 1, 1, "saved before the file was deleted").unwrap();
    handle.leave(tc_store::Leave::Mode).wait().unwrap();
    handle.wait_idle();
    let pending = pending_files(&rig, &folder);
    rig.fs.delete(&pending[0]).unwrap();
    let fs = rig.fs.crash(false);
    drop(handle);
    let rig = rig_as(fs, APP, "host-1");
    let usfm = folder.join("ingredients/TIT.usfm");
    rig.fs.set_unreadable(&usfm, true);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    assert!(
        notices(&handle).iter().any(
            |n| matches!(n, Notice::FileUnreadable { path, .. } if path.ends_with("TIT.usfm"))
        ),
        "{:?}",
        notices(&handle)
    );
    rig.fs.set_unreadable(&usfm, false);
    let started = std::time::Instant::now();
    while handle.kept_copies().is_empty() {
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "the writer's retry never kept the book file: {:?} {:?}",
            words(&handle, 1, 1),
            notices(&handle)
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    handle.wait_idle();
    assert_the_book_file_gave_the_save_back(
        &rig,
        &folder,
        &handle,
        "saved before the file was deleted",
    );
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before the file was deleted")
    );
    assert_eq!(verse_text_conflicts(&handle), 0);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// INV-VALID-BURRITO at a full disk during a checkpoint: a pack renamed into place whose folder
/// flush then fails stays listed by `metadata.json` once the group ends, and this session's
/// own pack is never taken for a second writer's (the checks lane's repro, 2026-10-25). A
/// failure after the book file's rename (while `metadata.json` is written or the folder
/// flushed) is retried, and the retry's commit holds the file as saved, not the blob of the
/// last commit (a stale `blob` in the manifest; D1's loss lens, 2026-10-25): Door43 and
/// teammates would otherwise read an old book file until it changed again.
#[test]
fn a_checkpoint_that_fails_on_a_full_disk_is_retried_whole() {
    let mut reached = Vec::new();
    for n in 0..46u64 {
        let (rig, folder) = titus();
        let handle = open(&rig, &folder);
        save_verse(&handle, 1, 2, "MUST BE COMMITTED").unwrap();
        handle.wait_idle();
        rig.fs.fail_op(n, std::io::ErrorKind::StorageFull);
        let first = handle.checkpoint(CheckpointReason::LeavingTranslate).wait();
        handle.wait_idle();
        if first.is_ok() {
            handle.close().unwrap();
            continue;
        }
        let retried = handle
            .checkpoint(CheckpointReason::Other("retry".into()))
            .wait();
        handle.wait_idle();
        let problems = rig.store.hooks().burrito_problems();
        assert!(
            problems.is_empty(),
            "fail point {n}: INV-VALID-BURRITO: {problems:?}; retry {retried:?}, mode {:?}",
            handle.facts().mode
        );
        assert!(
            !matches!(handle.facts().mode, tc_store::Mode::ReadOnly(_)),
            "fail point {n}: this session's own pack was taken for a second writer's"
        );
        assert!(
            retried.as_ref().is_ok_and(|r| r.commit.is_some()),
            "fail point {n}: the retry commits the save: {retried:?}"
        );
        let repo = repo_of(&rig.fs, &folder);
        let head = repo.head().unwrap().peel_to_commit().unwrap().id();
        let committed = String::from_utf8_lossy(&commit_files(&repo, head)["ingredients/TIT.usfm"])
            .into_owned();
        assert!(
            committed.contains("MUST BE COMMITTED"),
            "fail point {n}: HEAD's TIT.usfm lacks the save"
        );
        reached.push(n);
        handle.close().unwrap();
    }
    assert!(
        !reached.is_empty(),
        "no fail point failed the first checkpoint"
    );
}
