//! Operation counts (`docs/PERFORMANCE.md` rule 2, `docs/STORAGE.md` §16.5a) on the Psalms
//! corpus and on the corpus ten times larger: an action costs what it touches, never the
//! size of the project.

use crate::common::*;
use crate::corpus::{OnDisk, psalms, psalms_x10};
use std::path::PathBuf;
use tc_store::fault::{FaultFs, OpKind};
use tc_store::{OpenOptions, StoreCounters};

/// Opens a copy of a corpus project, lets its background work end, and closes it; the
/// counters and the files opened are of the open and its background work.
fn open_once(fs: &FaultFs, folder: &std::path::Path) -> (StoreCounters, Vec<PathBuf>) {
    let rig = rig_as(fs.clone(), APP, "host-1");
    fs.clear_log();
    let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let counters = handle.counters();
    let opened = fs.opened();
    handle.close().unwrap();
    (counters, opened)
}

fn is_journal(path: &std::path::Path) -> bool {
    let text = path.to_string_lossy();
    text.contains("/ingredients/tc/changes/") || text.contains("/.tc/pending/")
}

/// What an open after no change cost: the counters when the project became usable, those
/// after the background work (O14) ended, and the number of derived files the fold has.
struct OpenCost {
    usable: StoreCounters,
    after_idle: StoreCounters,
    derived_files: usize,
}

/// Opens twice with the clock past the timestamp tick of the last materialization, so every
/// entry is settled by the second open's storage check; returns the second and the third
/// open's costs and the files the third opened.
fn no_change_opens(corpus: &OnDisk) -> (OpenCost, OpenCost) {
    let fs = corpus.fs.copy();
    // Past the timestamp tick of the last materialization.
    fs.advance(std::time::Duration::from_secs(5));
    let second = open_costed(&fs, &corpus.folder);
    fs.advance(std::time::Duration::from_secs(5));
    let third = open_costed(&fs, &corpus.folder);
    (second, third)
}

/// An open whose counters are taken the moment it returns (the project is usable) and again
/// when its background work has ended.
fn open_costed(fs: &FaultFs, folder: &std::path::Path) -> OpenCost {
    let rig = rig_as(fs.clone(), APP, "host-1");
    fs.clear_log();
    let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
    let usable = handle.counters();
    handle.wait_idle();
    let after_idle = handle.counters();
    let derived_files = handle.snapshot().fold().derived_files().len();
    let opened = fs.opened();
    for path in &opened {
        if is_journal(path) {
            assert_eq!(
                opened.iter().filter(|p| *p == path).count(),
                1,
                "{} was opened more than once",
                path.display()
            );
        }
    }
    handle.close().unwrap();
    OpenCost {
        usable,
        after_idle,
        derived_files,
    }
}

/// O9 converges a path without reading or projecting it when it is clean by stat; O14 reads
/// each such path once, in the background, after the project is usable (§6.1). The two reads
/// are counted apart, so an open that read every derived file before it was usable cannot
/// pass for one that left the reading to O14.
#[test]
fn an_open_after_no_change_projects_no_file_and_reads_no_derived_file() {
    for corpus in [psalms(), psalms_x10()] {
        let (second, third) = no_change_opens(corpus);
        assert_eq!(
            second.usable.projected, 0,
            "the second open projects nothing: {:?}",
            second.usable
        );
        assert_eq!(third.usable.projected, 0, "{:?}", third.usable);
        assert_eq!(third.after_idle.projected, 0);
        assert_eq!(third.usable.heads_read, 0);
        assert!(third.derived_files > 0);
        // The entries are settled: the open reads no derived file before it is usable, and
        // none after it either except the storage check's, which reads each once.
        assert_eq!(
            third.usable.derived_read, 0,
            "the open read derived files: {:?}",
            third.usable
        );
        assert_eq!(third.after_idle.derived_read, 0);
        assert_eq!(
            third.after_idle.check_read, third.derived_files as u64,
            "O14 reads each derived file once: {:?}",
            third.after_idle
        );
    }
}

#[test]
fn an_open_reads_each_pack_once() {
    for corpus in [psalms(), psalms_x10()] {
        let fs = corpus.fs.copy();
        // Without the fold cache, then with it.
        fs.delete(&corpus.folder.join(".tc/index.bin")).ok();
        for _ in 0..2 {
            let (_, opened) = open_once(&fs, &corpus.folder);
            let journal: Vec<&PathBuf> = opened.iter().filter(|p| is_journal(p)).collect();
            let unique: std::collections::BTreeSet<&PathBuf> = journal.iter().copied().collect();
            assert_eq!(
                journal.len(),
                unique.len(),
                "a journal file was opened twice"
            );
            assert!(!unique.is_empty());
        }
    }
}

/// A save costs what it touches: one pending file written, no pack read whole (only the
/// values of the records it changes, read where they lie), and a bounded number of write
/// points, the same on both corpora.
#[test]
fn a_save_reads_no_pack_and_writes_a_bounded_number_of_files() {
    let mut writes = Vec::new();
    for corpus in [psalms(), psalms_x10()] {
        let fs = corpus.fs.copy();
        let rig = rig_as(fs.clone(), APP, "host-1");
        let handle = rig
            .store
            .open(&corpus.folder, &OpenOptions::default())
            .unwrap();
        handle.wait_idle();
        let key = handle
            .snapshot()
            .fold()
            .chapter_keys(tc_journal::BookId::Psalms, 23)
            .verses
            .first()
            .cloned()
            .unwrap();
        fs.clear_log();
        let read_before = fs.bytes_read();
        let before = handle.counters();
        let shown = handle.mint(&key, None).unwrap();
        let reply = handle
            .submit(tc_journal::command::Command::SaveVerse {
                shown,
                words: "The LORD is my shepherd, a save".into(),
            })
            .wait()
            .unwrap();
        assert!(!reply.changes.is_empty());
        let log = fs.log();
        let opened = fs.opened();
        let read = fs.bytes_read() - read_before;
        let delta = handle.counters().since(&before);
        assert!(
            opened.is_empty(),
            "a save opened a file the open had not: {opened:?}"
        );
        assert!(read < 64 * 1024, "a save read {read} bytes");
        let created = log.iter().filter(|op| op.kind == OpKind::Create).count();
        assert_eq!(created, 1, "one pending file: {log:?}");
        assert_eq!(delta.fsyncs, 2, "the file and its folder");
        writes.push(log.len());
        handle.close().unwrap();
    }
    assert_eq!(
        writes[0], writes[1],
        "the same write points on both corpora"
    );
}

#[test]
fn an_open_after_one_save_projects_only_that_books_files() {
    let mut counts = Vec::new();
    for corpus in [psalms(), psalms_x10()] {
        let fs = corpus.fs.copy();
        fs.advance(std::time::Duration::from_secs(5));
        let rig = rig_as(fs.clone(), APP, "host-1");
        let handle = rig
            .store
            .open(&corpus.folder, &OpenOptions::default())
            .unwrap();
        handle.wait_idle();
        let key = handle
            .snapshot()
            .fold()
            .chapter_keys(tc_journal::BookId::Psalms, 1)
            .verses
            .first()
            .cloned()
            .unwrap();
        // Saved and materialized before close: the next open projects nothing.
        let shown = handle.mint(&key, None).unwrap();
        handle
            .submit(tc_journal::command::Command::SaveVerse {
                shown,
                words: "Blessed is the one, first saved".into(),
            })
            .wait()
            .unwrap();
        handle
            .leave(tc_store::Leave::Book(tc_journal::BookId::Psalms))
            .wait()
            .unwrap();
        let closed = fs.crash(false);
        closed.advance(std::time::Duration::from_secs(5));
        let (after_materialized, _) = open_once(&closed, &corpus.folder);
        assert_eq!(after_materialized.projected, 0, "{after_materialized:?}");
        let shown = handle.mint(&key, None).unwrap();
        handle
            .submit(tc_journal::command::Command::SaveVerse {
                shown,
                words: "Blessed is the one, saved".into(),
            })
            .wait()
            .unwrap();
        // A crash after B3: the save is durable, its materialization never ran.
        let crashed = fs.crash(true);
        drop(handle);
        crashed.advance(std::time::Duration::from_secs(5));
        let (counters, _) = open_once(&crashed, &corpus.folder);
        let book_files = rig_as(crashed.copy(), APP, "host-1")
            .store
            .open(&corpus.folder, &OpenOptions::default())
            .map(|h| {
                let n = h
                    .snapshot()
                    .fold()
                    .book_files(tc_journal::BookId::Psalms)
                    .len();
                h.close().unwrap();
                n
            })
            .unwrap();
        assert!(
            counters.projected as usize <= book_files + 2,
            "{} projected, the book has {book_files} files",
            counters.projected
        );
        counts.push(counters.projected);
    }
    assert_eq!(counts[0], counts[1], "the same count on both corpora");
}

/// A pack a teammate sealed, holding the first comment on Psalms 23:1 and a new decision on
/// a translationNotes check of Psalms 23:1: records entering the scopes of
/// `comments/PSA.json` and `translationNotes/PSA.json` only.
fn teammate_comment_pack(fs: &FaultFs, folder: &std::path::Path) {
    use tc_journal::keys::{CheckId, CheckKey, CheckTarget, RefPart, ToolId};
    use tc_journal::record::{
        CheckValue, CommentTarget, CommentValue, DecisionRecord, Judgement, ResourceRef, UserStatus,
    };
    use tc_journal::{
        BatchId, BatchRef, Cause, Change, ChangeBody, CommentId, DeviceId, DeviceSeq, FORMAT_MINOR,
        GitOid, Md5, ProjectId, Record, Verified, WallTime,
    };
    let bytes = fs
        .peek(&folder.join("ingredients/tc/project.json"))
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let project = ProjectId::parse(json["projectId"].as_str().unwrap()).unwrap();
    let device = DeviceId::local([7u8; 15]);
    let change = Change::new(ChangeBody {
        v: FORMAT_MINOR,
        record: Record::Comment {
            id: CommentId::from_bytes([9u8; 16]),
            value: CommentValue {
                target: CommentTarget::Verse {
                    book: tc_journal::BookId::Psalms,
                    chapter: tc_journal::ChapterNo::new(23).unwrap(),
                    verse: tc_journal::VerseNo::new(1).unwrap(),
                },
                text: "Check the word shepherd".into(),
            },
        },
        parents: Vec::new(),
        device,
        seq: DeviceSeq(1),
        at: WallTime(1_792_195_200_000),
        batch: BatchRef::new(BatchId::from_bytes([3u8; 16]), 1).unwrap(),
        cause: Cause::Edit,
    })
    .unwrap();
    let decision = Change::new(ChangeBody {
        v: FORMAT_MINOR,
        record: Record::Check {
            key: CheckKey {
                tool: ToolId::TranslationNotes,
                check_id: CheckId::parse("zz99").unwrap(),
                target: CheckTarget::Verse {
                    book: tc_journal::BookId::Psalms,
                    chapter: RefPart::parse("23").unwrap(),
                    verse: RefPart::parse("1").unwrap(),
                },
                occurrence: 1,
            },
            value: CheckValue::Decision(DecisionRecord {
                context: Default::default(),
                category: None,
                judgement: Judgement {
                    selections: None,
                    nothing_to_select: true,
                    status: Some(UserStatus::Valid),
                    verse_text_md5: Md5::of(b""),
                },
                comment: None,
                bookmarked: false,
                resource: ResourceRef {
                    repo_path: "git.door43.org/unfoldingWord/en_tn".into(),
                    sha: GitOid::parse(&"1".repeat(40)).unwrap(),
                },
                imported_invalid: false,
                dismissed: false,
                extra: Default::default(),
            }),
        },
        parents: Vec::new(),
        device,
        seq: DeviceSeq(2),
        at: WallTime(1_792_195_200_000),
        batch: BatchRef::new(BatchId::from_bytes([4u8; 16]), 1).unwrap(),
        cause: Cause::Edit,
    })
    .unwrap();
    let pack = tc_journal::pack::encode_pack(
        &tc_journal::pack::PackSpec {
            project,
            sealer: device,
            sealed: WallTime(1_792_195_200_000),
        },
        &[Verified::from(change), Verified::from(decision)],
        &[],
    )
    .unwrap();
    fs.put(&folder.join(pack.path.to_string()), &pack.bytes, None);
}

#[test]
fn a_record_entering_a_files_scope_projects_it() {
    let mut counts = Vec::new();
    for corpus in [psalms(), psalms_x10()] {
        let fs = corpus.fs.copy();
        fs.advance(std::time::Duration::from_secs(5));
        open_once(&fs, &corpus.folder);
        fs.advance(std::time::Duration::from_secs(5));
        teammate_comment_pack(&fs, &corpus.folder);
        let (counters, _) = open_once(&fs, &corpus.folder);
        assert_eq!(
            counters.projected, 2,
            "only the book's comment and translationNotes files: {counters:?}"
        );
        assert!(
            fs.peek(&corpus.folder.join("ingredients/tc/comments/PSA.json"))
                .is_some_and(|b| String::from_utf8_lossy(&b).contains("Check the word shepherd"))
        );
        assert!(
            fs.peek(
                &corpus
                    .folder
                    .join("ingredients/checking/translationNotes/PSA.json")
            )
            .is_some_and(|b| String::from_utf8_lossy(&b).contains("zz99"))
        );
        counts.push(counters.projected);
    }
    assert_eq!(counts[0], counts[1]);
}

/// `FaultFs` with 2 s modification times (FAT32, §6.2). A same-size edit within the tick of
/// the app's own write is read and recorded; bytes that changed after the entry settled,
/// with the old size and time put back, are held as storage damage. Neither is written over.
#[test]
fn a_racy_or_mtime_restored_file_is_never_overwritten() {
    use tc_journal::fold::Notice;
    let (rig, folder) = titus();
    rig.fs.set_tick(std::time::Duration::from_secs(2));
    let usfm = folder.join("ingredients/TIT.usfm");
    let held = |handle: &tc_store::ProjectHandle| {
        handle.notices().iter().any(|n| {
            matches!(
                &n.notice,
                Notice::OutsideEditHeld { path, .. } | Notice::StorageDamage { path, .. }
                    if path.as_str() == "ingredients/TIT.usfm"
            )
        })
    };
    // 1. A same-size edit within the tick of the app's write is not told by its stat from
    // the app's own write: it is read, and recorded as an edit of the person's.
    let fs = rig.fs.copy();
    let rig1 = rig_as(fs.clone(), APP, "host-1");
    let handle = rig1.store.open(&folder, &OpenOptions::default()).unwrap();
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    handle.leave(tc_store::Leave::Book(TIT)).wait().unwrap();
    handle.close().unwrap();
    let mut edited = fs.peek(&usfm).unwrap();
    let at = edited.iter().position(|b| *b == b'P').unwrap();
    edited[at] = b'p';
    let mtime = tc_store::Vfs::metadata(&fs, &usfm).unwrap().mtime_ns;
    fs.put(&usfm, &edited, Some(mtime));
    let handle = rig1.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert!(!held(&handle), "an edit is recorded, not held");
    assert!(
        handle.notices().iter().any(|n| matches!(
            &n.notice,
            Notice::OutsideEditRecorded { path, .. } if path.as_str() == "ingredients/TIT.usfm"
        )),
        "the racy edit is read and recorded: {:?}",
        handle.notices()
    );
    assert_eq!(words(&handle, 1, 1).as_deref(), Some("paul, a servant"));
    let copies = copies_of(&handle, "ingredients/TIT.usfm");
    assert_eq!(copies.len(), 1);
    assert_eq!(
        fs.peek(&folder.join(&copies[0])).as_deref(),
        Some(edited.as_slice()),
        "the bytes the person saved are kept"
    );
    // The project shows the edit, so the next materialization writes the same bytes.
    save_verse(&handle, 1, 2, "in hope").unwrap();
    handle.leave(tc_store::Leave::Book(TIT)).wait().unwrap();
    let written = String::from_utf8(fs.peek(&usfm).unwrap()).unwrap();
    assert!(written.contains("paul, a servant") && written.contains("in hope"));
    handle.close().unwrap();
    // 2. An edit whose old mtime is put back after its entry settled.
    let fs = rig.fs.copy();
    fs.advance(std::time::Duration::from_secs(10));
    let rig2 = rig_as(fs.clone(), APP, "host-1");
    let handle = rig2.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle.close().unwrap();
    let mut edited = fs.peek(&usfm).unwrap();
    let at = edited.iter().position(|b| *b == b'T').unwrap();
    edited[at] = b't';
    let mtime = tc_store::Vfs::metadata(&fs, &usfm).unwrap().mtime_ns;
    fs.put(&usfm, &edited, Some(mtime));
    let handle = rig2.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert!(held(&handle), "the storage check reads it and holds it");
    assert_eq!(fs.peek(&usfm).as_deref(), Some(edited.as_slice()));
    handle.close().unwrap();
}

/// The fold cache (`.tc/index.bin`, §3.6, §6.1 O8) is stamped with its encoding, the format
/// minor, the probe stamp and the project only, so another open, another installation or an
/// app update that keeps the encoding takes every unchanged file from it; a damaged cache is
/// only a cache that is not there.
#[test]
fn the_fold_cache_is_kept_by_every_open_of_the_same_encoding() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    handle.wait_idle();
    handle.close().unwrap();
    let fs = rig.fs.crash(false);
    let index = folder.join(".tc/index.bin");
    // The close wrote the cache; the next open takes every file from it.
    let (first, _) = open_once(&fs, &folder);
    assert!(first.journal_files_read > 0);
    assert_eq!(
        first.journal_files_cached, first.journal_files_read,
        "{first:?}"
    );
    let written = fs.peek(&index).unwrap();
    // Another installation (other app data, other secret) opening a copy of the folder takes
    // it too: nothing of the build or the installation is in it.
    let copy = fs.copy();
    let other = rig_as(copy.clone(), "/other-app", "host-2");
    let handle = other
        .store
        .open(
            &folder,
            &OpenOptions {
                other_computer_closed: true,
                ..OpenOptions::default()
            },
        )
        .unwrap();
    handle.wait_idle();
    let counters = handle.counters();
    handle.close().unwrap();
    assert_eq!(
        counters.journal_files_cached, counters.journal_files_read,
        "{counters:?}"
    );
    // A damaged cache gives nothing and is written again. (A byte in every section of the
    // base: it may also hold the sections of files that are gone.)
    let damage = |bytes: &[u8]| {
        let mut damaged = bytes.to_vec();
        for at in (128..damaged.len()).step_by(32) {
            damaged[at] ^= 0x40;
        }
        damaged
    };
    fs.put(&index, &damage(&written), None);
    // And the delta file, which holds the sections made since the base was written.
    let delta = folder.join(".tc/index.delta.bin");
    if let Some(bytes) = fs.peek(&delta) {
        fs.put(&delta, &damage(&bytes), None);
    }
    let (after_damage, _) = open_once(&fs, &folder);
    assert!(
        after_damage.journal_files_cached < after_damage.journal_files_read,
        "the damaged section is not used: {after_damage:?}"
    );
    let (again, _) = open_once(&fs, &folder);
    assert_eq!(again.journal_files_cached, again.journal_files_read);
}

// ---------------------------------------------------------------------------------------
// Unit P: the other saves of the editor, the aligner and the Check screen, a resolution, and
// reading a book (docs/PERFORMANCE.md "Editor", "Checking", "Aligning"; §16.5a)
// ---------------------------------------------------------------------------------------

/// What a command that saves did to the folder, taken from the file system's log and the
/// store's counters: no file opened that the open had not, under 64 KiB read, one file
/// created, two flushes (the file and its folder). Returns the number of write points, which
/// must be the same on both corpora.
fn one_pending_file(fs: &FaultFs, handle: &tc_store::ProjectHandle, act: impl FnOnce()) -> usize {
    fs.clear_log();
    let read_before = fs.bytes_read();
    let before = handle.counters();
    act();
    let log = fs.log();
    let opened = fs.opened();
    let read = fs.bytes_read() - read_before;
    let delta = handle.counters().since(&before);
    assert!(
        opened.is_empty(),
        "a save opened a file the open had not: {opened:?}"
    );
    assert!(read < 64 * 1024, "a save read {read} bytes");
    let created = log.iter().filter(|op| op.kind == OpKind::Create).count();
    assert_eq!(created, 1, "one pending file: {log:?}");
    assert_eq!(delta.fsyncs, 2, "the file and its folder: {delta:?}");
    log.len()
}

fn open_psalms(corpus: &OnDisk) -> (FaultFs, Rig, tc_store::ProjectHandle) {
    let fs = corpus.fs.copy();
    let rig = rig_as(fs.clone(), APP, "host-1");
    let handle = rig
        .store
        .open(&corpus.folder, &OpenOptions::default())
        .unwrap();
    handle.wait_idle();
    (fs, rig, handle)
}

/// Recording a decision writes one pending file and reads no pack, whatever the project's
/// size: a bookmark toggled on a decided translationWords check of Psalms.
#[test]
fn a_decision_save_reads_no_pack_and_writes_one_pending_file() {
    use tc_journal::command::{Command, DecisionChange, DecisionEdit};
    use tc_journal::keys::{CheckTarget, ToolId};
    use tc_journal::record::CheckValue;
    let mut writes = Vec::new();
    for corpus in [psalms(), psalms_x10()] {
        let (fs, _rig, handle) = open_psalms(corpus);
        let fold = handle.snapshot().fold().clone();
        let key = fold
            .decisions_of(ToolId::TranslationWords, tc_journal::BookId::Psalms)
            .into_iter()
            .find(|k| {
                matches!(
                    k,
                    tc_journal::RecordKey::Check(c)
                        if matches!(&c.target, CheckTarget::Verse { chapter, verse, .. }
                            if chapter.number().is_some() && verse.number().is_some())
                )
            })
            .expect("a decided check of a verse");
        let Some(tc_journal::Record::Check {
            value: CheckValue::Decision(d),
            ..
        }) = fold.projected(&key)
        else {
            panic!("a decision");
        };
        let tc_journal::RecordKey::Check(check) = &key else {
            unreachable!()
        };
        let CheckTarget::Verse {
            book,
            chapter,
            verse,
        } = &check.target
        else {
            unreachable!()
        };
        let verse_key = tc_journal::RecordKey::Verse(
            *book,
            tc_journal::ChapterNo::new(chapter.number().unwrap()).unwrap(),
            tc_journal::VerseNo::new(verse.number().unwrap()).unwrap(),
        );
        let shown = handle.mint(&key, None).unwrap();
        let verse_shown = handle.mint(&verse_key, None).unwrap();
        writes.push(one_pending_file(&fs, &handle, || {
            let reply = handle
                .submit(Command::SaveDecision {
                    shown,
                    verse_shown,
                    value: DecisionEdit {
                        basis: None,
                        resource: d.resource.clone(),
                        changes: vec![DecisionChange::Bookmarked(!d.bookmarked)],
                    },
                })
                .wait()
                .unwrap();
            assert!(!reply.changes.is_empty());
        }));
        handle.close().unwrap();
    }
    assert_eq!(
        writes[0], writes[1],
        "the same write points on both corpora"
    );
}

/// Saving an alignment writes one pending file and reads no pack: the aligned verse of
/// Psalms marked done or not done.
#[test]
fn an_alignment_save_reads_no_pack_and_writes_one_pending_file() {
    use tc_journal::command::{AlignmentEdit, Command};
    let mut writes = Vec::new();
    for corpus in [psalms(), psalms_x10()] {
        let (fs, _rig, handle) = open_psalms(corpus);
        let fold = handle.snapshot().fold().clone();
        let key = fold
            .chapter_keys(tc_journal::BookId::Psalms, 23)
            .alignments
            .into_iter()
            .next()
            .expect("an alignment in Psalm 23");
        let Some(tc_journal::Record::Alignment {
            book,
            chapter,
            verse,
            value,
        }) = fold.projected(&key)
        else {
            panic!("an alignment");
        };
        let shown = handle.mint(&key, None).unwrap();
        let verse_shown = handle
            .mint(&tc_journal::RecordKey::Verse(book, chapter, verse), None)
            .unwrap();
        writes.push(one_pending_file(&fs, &handle, || {
            let reply = handle
                .submit(Command::SaveAlignment {
                    shown,
                    verse_shown,
                    value: AlignmentEdit {
                        alignments: value.alignments.clone(),
                        word_bank: value.word_bank.clone(),
                        done: !value.done,
                    },
                })
                .wait()
                .unwrap();
            assert!(!reply.changes.is_empty());
        }));
        handle.close().unwrap();
    }
    assert_eq!(
        writes[0], writes[1],
        "the same write points on both corpora"
    );
}

/// Resolving a conflict writes one pending file and reads no pack. The conflict is made the
/// way a teammate makes it: a pack that arrives between two opens holds an edit of the verse
/// that was based on the same text as this computer's own save.
#[test]
fn a_resolution_reads_no_pack_and_writes_one_pending_file() {
    use tc_journal::command::{Choice, Command};
    use tc_journal::record::VerseValue;
    use tc_journal::{
        BatchId, BatchRef, Cause, Change, ChangeBody, DeviceId, DeviceSeq, FORMAT_MINOR, Record,
        Verified, WallTime,
    };
    let mut writes = Vec::new();
    for corpus in [psalms(), psalms_x10()] {
        let fs = corpus.fs.copy();
        fs.advance(std::time::Duration::from_secs(5));
        let rig = rig_as(fs.clone(), APP, "host-1");
        let key = verse_key_for(corpus);
        let handle = rig
            .store
            .open(&corpus.folder, &OpenOptions::default())
            .unwrap();
        handle.wait_idle();
        let before_edit = handle.snapshot().fold().clone();
        let heads = before_edit.heads(&key);
        let Some(Record::Verse {
            book,
            chapter,
            verse,
            value:
                VerseValue::Text {
                    marker,
                    layout,
                    last,
                    from,
                    ..
                },
        }) = before_edit.projected(&key)
        else {
            panic!("a verse with text");
        };
        save_verse_key(&handle, &key, "Mine, saved here");
        handle.wait_idle();
        handle.close().unwrap();
        // The teammate's edit of the same text, made before the two exchanged.
        let project = project_of(&fs, &corpus.folder);
        let device = DeviceId::local([7u8; 15]);
        let theirs = Change::new(ChangeBody {
            v: FORMAT_MINOR,
            record: Record::Verse {
                book,
                chapter,
                verse,
                value: VerseValue::Text {
                    marker,
                    words: "Theirs, saved there".into(),
                    layout,
                    last,
                    from,
                },
            },
            parents: heads,
            device,
            seq: DeviceSeq(1),
            at: WallTime(1_792_195_200_000),
            batch: BatchRef::new(BatchId::from_bytes([5u8; 16]), 1).unwrap(),
            cause: Cause::Edit,
        })
        .unwrap();
        let their_id = theirs.id();
        let pack = tc_journal::pack::encode_pack(
            &tc_journal::pack::PackSpec {
                project,
                sealer: device,
                sealed: WallTime(1_792_195_200_000),
            },
            &[Verified::from(theirs)],
            &[],
        )
        .unwrap();
        fs.put(
            &corpus.folder.join(pack.path.to_string()),
            &pack.bytes,
            None,
        );
        fs.advance(std::time::Duration::from_secs(5));
        let rig = rig_as(fs.clone(), APP, "host-1");
        let handle = rig
            .store
            .open(&corpus.folder, &OpenOptions::default())
            .unwrap();
        handle.wait_idle();
        let conflict = handle
            .snapshot()
            .fold()
            .conflicts()
            .find(|c| c.key == key)
            .cloned()
            .expect("the two edits conflict");
        let theirs_head = conflict
            .candidates
            .iter()
            .find(|c| c.head == their_id)
            .expect("their version is a candidate")
            .head;
        let shown = handle.mint_review(&key).unwrap();
        writes.push(one_pending_file(&fs, &handle, || {
            let reply = handle
                .submit(Command::Resolve {
                    conflict: conflict.id,
                    shown: vec![shown],
                    choice: Choice::Candidate { head: theirs_head },
                })
                .wait()
                .unwrap();
            assert!(!reply.changes.is_empty());
        }));
        assert!(
            handle.snapshot().fold().conflicts().all(|c| c.key != key),
            "the conflict is resolved"
        );
        handle.close().unwrap();
    }
    assert_eq!(
        writes[0], writes[1],
        "the same write points on both corpora"
    );
}

/// A verse of Psalms 23 with text, the same in both corpora.
fn verse_key_for(corpus: &OnDisk) -> tc_journal::RecordKey {
    let fs = corpus.fs.copy();
    let rig = rig_as(fs, APP, "host-1");
    let handle = rig
        .store
        .open(&corpus.folder, &OpenOptions::default())
        .unwrap();
    let key = handle
        .snapshot()
        .fold()
        .chapter_keys(tc_journal::BookId::Psalms, 23)
        .verses
        .into_iter()
        .find(|k| {
            matches!(
                handle.snapshot().fold().projected(k),
                Some(tc_journal::Record::Verse {
                    value: tc_journal::record::VerseValue::Text { .. },
                    ..
                })
            )
        })
        .expect("a verse with text");
    handle.close().unwrap();
    key
}

fn save_verse_key(handle: &tc_store::ProjectHandle, key: &tc_journal::RecordKey, words: &str) {
    let shown = handle.mint(key, None).unwrap();
    handle
        .submit(tc_journal::command::Command::SaveVerse {
            shown,
            words: words.into(),
        })
        .wait()
        .unwrap();
}

/// Reading a book reads that book's values, where they lie, and opens no file the project
/// does not hold open already: the bytes read are about 470 per record of the book's text, the
/// same on the corpus ten times larger, not the packs the values lie in.
#[test]
fn reading_a_book_reads_only_its_values() {
    let mut read = Vec::new();
    for corpus in [psalms(), psalms_x10()] {
        let (fs, _rig, handle) = open_psalms(corpus);
        // The open took the files' prepared changes from the fold cache the last close wrote,
        // so the first read of a value opens its file once; a second read opens nothing.
        let records = handle
            .snapshot()
            .fold()
            .record_keys()
            .iter()
            .filter(|k| {
                matches!(
                    k,
                    tc_journal::RecordKey::Verse(tc_journal::BookId::Psalms, ..)
                        | tc_journal::RecordKey::ChapterFront(tc_journal::BookId::Psalms, _)
                        | tc_journal::RecordKey::BookHeader(tc_journal::BookId::Psalms)
                )
            })
            .count();
        fs.clear_log();
        let before = fs.bytes_read();
        let book = handle.read_book(tc_journal::BookId::Psalms).wait();
        let first = fs.bytes_read() - before;
        assert!(book.usfm.is_some_and(|u| u.contains("\\c 150")));
        let journal_opened: Vec<_> = fs.opened().into_iter().filter(|p| is_journal(p)).collect();
        let unique: std::collections::BTreeSet<_> = journal_opened.iter().collect();
        assert_eq!(journal_opened.len(), unique.len(), "a file opened twice");
        assert!(
            fs.opened().iter().all(|p| is_journal(p)),
            "a book read opened a file that is not a journal file: {:?}",
            fs.opened()
        );
        assert!(
            first < 1024 * records as u64,
            "reading Psalms read {first} bytes for the {records} records of its text"
        );
        fs.clear_log();
        let before = fs.bytes_read();
        let again = handle.read_book(tc_journal::BookId::Psalms).wait();
        assert!(again.usfm.is_some());
        assert!(
            fs.opened().is_empty(),
            "a second read opened {:?}",
            fs.opened()
        );
        read.push((first, fs.bytes_read() - before));
        handle.close().unwrap();
    }
    // The corpus ten times larger has other books, more devices and more history; Psalms
    // is the same book, so its values are about as many bytes.
    assert!(
        read[1].0 <= read[0].0 * 3 / 2 + 64 * 1024,
        "reading Psalms read {} bytes in the corpus and {} in the one ten times larger",
        read[0].0,
        read[1].0
    );
}

/// The close writes the sections made since the fold cache was written, not the cache again
/// (the cache of a whole Bible is 65 MB, and a session adds a few files): `index.bin` keeps its
/// bytes, a small `index.delta.bin` holds the new sections, and the next open takes every file
/// from the two. The same on the corpus ten times larger.
#[test]
fn a_close_writes_the_sections_it_made_and_not_the_cache() {
    let mut sizes = Vec::new();
    for corpus in [psalms(), psalms_x10()] {
        let fs = corpus.fs.copy();
        let rig = rig_as(fs.clone(), APP, "host-1");
        let handle = rig
            .store
            .open(&corpus.folder, &OpenOptions::default())
            .unwrap();
        handle.wait_idle();
        let base_path = corpus.folder.join(".tc/index.bin");
        let delta_path = corpus.folder.join(".tc/index.delta.bin");
        let base = fs.peek(&base_path).expect("the cache");
        let key = handle
            .snapshot()
            .fold()
            .chapter_keys(tc_journal::BookId::Psalms, 23)
            .verses
            .first()
            .cloned()
            .unwrap();
        let shown = handle.mint(&key, None).unwrap();
        handle
            .submit(tc_journal::command::Command::SaveVerse {
                shown,
                words: "A save the close seals".into(),
            })
            .wait()
            .unwrap();
        handle.close().unwrap();
        assert_eq!(
            fs.peek(&base_path).as_deref(),
            Some(base.as_slice()),
            "the cache was not written again"
        );
        let delta = fs
            .peek(&delta_path)
            .expect("the sections made in the session");
        assert!(
            delta.len() < 256 * 1024 && delta.len() * 50 < base.len(),
            "the delta is {} bytes against a cache of {}",
            delta.len(),
            base.len()
        );
        sizes.push(delta.len());
        // The next open takes every file from the base and the delta.
        let (counters, _) = open_once(&fs, &corpus.folder);
        assert_eq!(
            counters.journal_files_cached, counters.journal_files_read,
            "{counters:?}"
        );
    }
    assert!(
        sizes[1] < sizes[0] * 4,
        "the delta follows the session, not the project: {sizes:?}"
    );
}
