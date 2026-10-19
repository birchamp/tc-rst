//! A large batch is written off the actor (`docs/STORAGE.md` §5.0 "A large batch", §5.8,
//! §6.4; PERFORMANCE.md: a save waits at most 25 ms p95 while an upgrade write runs). The
//! batches here are the carry-over of the decisions of Psalms, more than the 2,048 changes
//! past which the actor hands a batch to a worker thread.

use crate::common::*;
use crate::corpus::{OnDisk, psalms};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tc_journal::command::{Command, PlannedChange, UpgradePlan};
use tc_journal::fold::FoldState;
use tc_journal::keys::ToolId;
use tc_journal::pins::Collection;
use tc_journal::record::CheckValue;
use tc_journal::{BookId, Cause, GitOid, PinGroup, Record, RecordKey};
use tc_store::fault::FaultFs;
use tc_store::{
    CommandError, Hooks, OpenOptions, Point, ProjectHandle, StoreEvent, StoreReply, Stored, Work,
};

const PSALMS: BookId = BookId::Psalms;

/// The decisions of Psalms carried to another commit of their resource, `n` of them (at most).
fn upgrade_plan(fold: &FoldState, n: usize) -> UpgradePlan {
    upgrade_plan_to(fold, n, '9')
}

/// The same, to the commit whose id is `digit` forty times.
fn upgrade_plan_to(fold: &FoldState, n: usize, digit: char) -> UpgradePlan {
    let sha = GitOid::parse(&digit.to_string().repeat(40)).unwrap();
    let mut changes = Vec::new();
    for tool in [ToolId::TranslationNotes, ToolId::TranslationWords] {
        for decision in fold.decisions(tool, Collection::Book(PSALMS)) {
            if changes.len() >= n {
                break;
            }
            let key = RecordKey::Check(decision.key.clone());
            let mut record = decision.record;
            record.resource.sha = sha;
            changes.push(PlannedChange {
                record: Record::Check {
                    key: decision.key,
                    value: CheckValue::Decision(record),
                },
                parents: fold.heads(&key),
                cause: Cause::CarryOver {
                    group: PinGroup::Primary,
                },
            });
        }
    }
    UpgradePlan { changes }
}

fn start(hooks: Arc<Hooks>) -> (FaultFs, Rig, ProjectHandle, &'static OnDisk) {
    let corpus = psalms();
    let fs = corpus.fs.copy();
    let rig = rig_with_hooks(fs.clone(), hooks);
    let handle = rig
        .store
        .open(&corpus.folder, &OpenOptions::default())
        .unwrap();
    handle.wait_idle();
    (fs, rig, handle, corpus)
}

/// A verse of Psalms that no decision of the plan is about, saved.
fn save_psalm_verse(handle: &ProjectHandle, chapter: u16, verse: u16, words: &str) -> StoreReply {
    let key = RecordKey::Verse(
        PSALMS,
        tc_journal::ChapterNo::new(chapter).unwrap(),
        tc_journal::VerseNo::new(verse).unwrap(),
    );
    let shown = handle.mint(&key, None).unwrap();
    handle
        .submit(Command::SaveVerse {
            shown,
            words: words.into(),
        })
        .wait()
        .unwrap()
}

fn carried(fold: &FoldState) -> usize {
    fold.decisions(ToolId::TranslationWords, Collection::Book(PSALMS))
        .into_iter()
        .chain(fold.decisions(ToolId::TranslationNotes, Collection::Book(PSALMS)))
        .filter(|d| d.record.resource.sha == GitOid::parse(&"9".repeat(40)).unwrap())
        .count()
}

fn staged_dir_is_empty(fs: &FaultFs, corpus: &OnDisk) -> bool {
    let dir = corpus.folder.join(".tc/staged");
    tc_store::Vfs::read_dir(fs, &dir).map_or(true, |entries| entries.is_empty())
}

#[test]
fn a_save_is_durable_while_a_large_batch_is_written() {
    let hooks = Arc::new(Hooks::default());
    let (fs, _rig, handle, corpus) = start(hooks.clone());
    let plan = upgrade_plan(handle.snapshot().fold(), 2_300);
    assert!(
        plan.changes.len() > 2_048,
        "{} decisions",
        plan.changes.len()
    );
    let n = plan.changes.len();
    hooks.hold(Point::BeforeStagedReady);
    let upgrade = handle.submit(Command::ApplyUpgrade { plan });
    hooks.wait_reached(Point::BeforeStagedReady, 1);
    // The worker has written and folded the batch and waits; the actor is free.
    for v in 1..=5 {
        let reply = save_psalm_verse(&handle, 23, v, &format!("saved while the batch waits {v}"));
        assert_eq!(reply.stored, Stored::Durable);
    }
    assert!(!upgrade.is_ready(), "the batch is not answered yet");
    assert_eq!(carried(handle.snapshot().fold()), 0, "nor in the fold");
    hooks.release(Point::BeforeStagedReady);
    let reply = upgrade.wait().unwrap();
    assert_eq!(reply.changes.len(), n);
    assert_eq!(reply.stored, Stored::Durable);
    let fold = handle.snapshot().fold().clone();
    assert!(carried(&fold) >= n.min(2_048));
    assert_eq!(
        words_of(&fold, 23, 5).as_deref(),
        Some("saved while the batch waits 5"),
        "the saves made meanwhile are in the fold"
    );
    assert!(staged_dir_is_empty(&fs, corpus));
    handle.close().unwrap();
    assert_valid_burrito(&fs, &corpus.folder);
    // Reopened: both are there.
    let rig = rig_as(fs.crash(false), APP, "host-1");
    let reopened = rig
        .store
        .open(&corpus.folder, &OpenOptions::default())
        .unwrap();
    reopened.wait_idle();
    let fold = reopened.snapshot().fold().clone();
    assert!(carried(&fold) >= n.min(2_048));
    assert_eq!(
        words_of(&fold, 23, 3).as_deref(),
        Some("saved while the batch waits 3")
    );
    reopened.close().unwrap();
}

fn words_of(fold: &FoldState, chapter: u16, verse: u16) -> Option<String> {
    match fold.projected(&RecordKey::Verse(
        PSALMS,
        tc_journal::ChapterNo::new(chapter).unwrap(),
        tc_journal::VerseNo::new(verse).unwrap(),
    )) {
        Some(Record::Verse {
            value: tc_journal::record::VerseValue::Text { words, .. },
            ..
        }) => Some(words),
        _ => None,
    }
}

#[test]
fn a_large_batch_whose_records_moved_is_refused_and_leaves_no_file() {
    use tc_journal::command::{DecisionChange, DecisionEdit};
    let hooks = Arc::new(Hooks::default());
    let (fs, _rig, handle, corpus) = start(hooks.clone());
    let plan = upgrade_plan(handle.snapshot().fold(), 2_100);
    let target = plan.changes[7].record.key();
    let RecordKey::Check(check) = &target else {
        panic!("a decision");
    };
    let tc_journal::keys::CheckTarget::Verse {
        book,
        chapter,
        verse,
    } = &check.target
    else {
        panic!("a verse check");
    };
    let verse_key = RecordKey::Verse(
        *book,
        tc_journal::ChapterNo::new(chapter.number().unwrap()).unwrap(),
        tc_journal::VerseNo::new(verse.number().unwrap()).unwrap(),
    );
    let packs_before = packs(&fs, &corpus.folder);
    hooks.hold(Point::BeforeStagedReady);
    let upgrade = handle.submit(Command::ApplyUpgrade { plan });
    hooks.wait_reached(Point::BeforeStagedReady, 1);
    // The person changes one of the decisions the plan carries while the batch is written.
    let Some(Record::Check {
        value: CheckValue::Decision(d),
        ..
    }) = handle.snapshot().fold().projected(&target)
    else {
        panic!("a decision");
    };
    let shown = handle.mint(&target, None).unwrap();
    let verse_shown = handle.mint(&verse_key, None).unwrap();
    handle
        .submit(Command::SaveDecision {
            shown,
            verse_shown,
            value: DecisionEdit {
                basis: None,
                resource: d.resource.clone(),
                changes: vec![DecisionChange::Comment(Some("my own words".into()))],
            },
        })
        .wait()
        .unwrap();
    hooks.release(Point::BeforeStagedReady);
    assert!(matches!(upgrade.wait(), Err(CommandError::PlanStale)));
    handle.wait_idle();
    assert_eq!(carried(handle.snapshot().fold()), 0, "nothing was carried");
    assert!(staged_dir_is_empty(&fs, corpus), "no staged file is left");
    assert_eq!(
        packs(&fs, &corpus.folder),
        packs_before,
        "no pack was added"
    );
    let Some(Record::Check {
        value: CheckValue::Decision(kept),
        ..
    }) = handle.snapshot().fold().projected(&target)
    else {
        panic!("a decision");
    };
    assert_eq!(
        kept.comment.as_deref(),
        Some("my own words"),
        "the save is kept"
    );
    handle.close().unwrap();
    assert_valid_burrito(&fs, &corpus.folder);
}

#[test]
fn a_large_batch_shows_determinate_progress_by_300_ms() {
    let hooks = Arc::new(Hooks::default());
    let (_fs, _rig, handle, _corpus) = start(hooks.clone());
    let events = handle.subscribe();
    // A batch that ends at once shows nothing.
    let quick = upgrade_plan(handle.snapshot().fold(), 2_100);
    handle
        .submit(Command::ApplyUpgrade { plan: quick })
        .wait()
        .unwrap();
    let plan = upgrade_plan_to(handle.snapshot().fold(), 2_200, '8');
    let n = plan.changes.len() as u64;
    assert!(
        !std::iter::from_fn(|| events.try_recv().ok()).any(|e| matches!(
            e,
            StoreEvent::Progress {
                work: Work::Batch,
                ..
            }
        )),
        "a batch that ends in under 300 ms shows no progress"
    );
    // One that takes longer shows determinate progress from 300 ms on.
    hooks.hold(Point::BeforeStagedReady);
    let started = Instant::now();
    let upgrade = handle.submit(Command::ApplyUpgrade { plan });
    hooks.wait_reached(Point::BeforeStagedReady, 1);
    std::thread::sleep(Duration::from_millis(450));
    hooks.release(Point::BeforeStagedReady);
    let _ = upgrade.wait();
    let reports: Vec<_> = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|e| match e {
            StoreEvent::Progress {
                work: Work::Batch,
                progress,
            } => Some(progress),
            _ => None,
        })
        .collect();
    assert!(!reports.is_empty(), "progress after 300 ms");
    assert!(
        reports[0].at >= Duration::from_millis(290),
        "{:?}",
        reports[0]
    );
    assert!(
        reports[0].at < Duration::from_millis(450),
        "{:?}",
        reports[0]
    );
    for pair in reports.windows(2) {
        assert!(pair[0].done <= pair[1].done, "only grows: {pair:?}");
    }
    let last = reports.last().unwrap();
    assert_eq!((last.done, last.total), (n, n));
    assert!(started.elapsed() < Duration::from_secs(30));
    handle.close().unwrap();
}

fn packs(fs: &FaultFs, folder: &std::path::Path) -> Vec<String> {
    crate::common::packs(fs, folder)
}

#[test]
fn a_large_batch_keeps_its_values_in_the_packs() {
    let hooks = Arc::new(Hooks::default());
    let (_fs, _rig, handle, _corpus) = start(hooks);
    let plan = upgrade_plan(handle.snapshot().fold(), 2_100);
    handle
        .submit(Command::ApplyUpgrade { plan })
        .wait()
        .unwrap();
    // A fork that counts its own work: projecting a carried decision reads its value from
    // the pack, so the batch does not hold it in memory.
    let mut fork = handle.snapshot().fold().clone();
    fork.own_counters();
    let before = fork.counters();
    let decisions = fork.decisions(ToolId::TranslationWords, Collection::Book(PSALMS));
    assert!(!decisions.is_empty());
    let read = fork.counters().since(&before);
    assert!(read.values_read > 0, "{read:?}");
    handle.close().unwrap();
}

/// The ledger (K5) names a sequence number a durable file holds. A batch that is refused
/// holds numbers that are in no file, and nothing is saved after it: the next open must not
/// take the missing numbers for a restored backup and mint a new device.
#[test]
fn a_refused_large_batch_never_rotates_the_device() {
    use tc_journal::command::{DecisionChange, DecisionEdit};
    let hooks = Arc::new(Hooks::default());
    let (fs, _rig, handle, corpus) = start(hooks);
    let device = handle.facts().device;
    let plan = upgrade_plan(handle.snapshot().fold(), 2_100);
    // A decision of the plan changes after the plan was made: the batch is refused at once.
    let target = plan.changes[0].record.key();
    let RecordKey::Check(check) = &target else {
        panic!("a decision");
    };
    let tc_journal::keys::CheckTarget::Verse {
        book,
        chapter,
        verse,
    } = &check.target
    else {
        panic!("a verse check");
    };
    let verse_key = RecordKey::Verse(
        *book,
        tc_journal::ChapterNo::new(chapter.number().unwrap()).unwrap(),
        tc_journal::VerseNo::new(verse.number().unwrap()).unwrap(),
    );
    let Some(Record::Check {
        value: CheckValue::Decision(d),
        ..
    }) = handle.snapshot().fold().projected(&target)
    else {
        panic!("a decision");
    };
    let shown = handle.mint(&target, None).unwrap();
    let verse_shown = handle.mint(&verse_key, None).unwrap();
    handle
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
    assert!(matches!(
        handle.submit(Command::ApplyUpgrade { plan }).wait(),
        Err(CommandError::PlanStale)
    ));
    // A checkpoint writes the ledger; the close and the next open then look at it.
    handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    handle.close().unwrap();
    let rig = rig_as(fs.crash(false), APP, "host-1");
    let reopened = rig
        .store
        .open(&corpus.folder, &OpenOptions::default())
        .unwrap();
    reopened.wait_idle();
    assert_eq!(reopened.facts().device, device, "the device is the same");
    assert!(
        reopened
            .notices()
            .iter()
            .all(|n| !matches!(n.notice, tc_journal::fold::Notice::RestoredBackup { .. })),
        "{:?}",
        reopened.notices()
    );
    reopened.close().unwrap();
}

/// A checkpoint made while the worker writes the batch finds nothing wrong with the folder:
/// the batch's files are outside the journal folders it lists.
#[test]
fn a_checkpoint_while_a_large_batch_is_written_finds_no_second_writer() {
    let hooks = Arc::new(Hooks::default());
    let (fs, _rig, handle, corpus) = start(hooks.clone());
    let plan = upgrade_plan(handle.snapshot().fold(), 2_100);
    let n = plan.changes.len();
    save_psalm_verse(&handle, 1, 1, "a save the checkpoint seals");
    hooks.hold(Point::BeforeStagedReady);
    let upgrade = handle.submit(Command::ApplyUpgrade { plan });
    hooks.wait_reached(Point::BeforeStagedReady, 1);
    let report = handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert_eq!(report.skipped, None, "{report:?}");
    assert!(report.commit.is_some());
    hooks.release(Point::BeforeStagedReady);
    let reply = upgrade.wait().unwrap();
    assert_eq!(reply.changes.len(), n);
    assert!(matches!(handle.facts().mode, tc_store::Mode::Writable));
    // The next checkpoint commits the batch\'s packs and the files that show it.
    let report = handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert!(report.commit.is_some(), "{report:?}");
    handle.close().unwrap();
    assert_valid_burrito(&fs, &corpus.folder);
    assert_commits_valid(&fs, &corpus.folder);
}

/// 2,100 new comments on a verse of Titus: a batch of records nobody has yet, so no parents.
fn comment_plan(n: usize) -> UpgradePlan {
    use tc_journal::record::{CommentTarget, CommentValue};
    UpgradePlan {
        changes: (0..n)
            .map(|i| PlannedChange {
                record: Record::Comment {
                    id: tc_journal::CommentId::from_bytes({
                        let mut id = [0u8; 16];
                        id[..8].copy_from_slice(&(i as u64 + 1).to_be_bytes());
                        id[15] = 0x7c;
                        id
                    }),
                    value: CommentValue {
                        target: CommentTarget::Verse {
                            book: TIT,
                            chapter: tc_journal::ChapterNo::new(1).unwrap(),
                            verse: tc_journal::VerseNo::new(1).unwrap(),
                        },
                        text: format!("a carried note {i}"),
                    },
                },
                parents: Vec::new(),
                cause: Cause::Edit,
            })
            .collect(),
    }
}

fn comments(fold: &FoldState) -> usize {
    fold.record_keys()
        .iter()
        .filter(|k| matches!(k, RecordKey::Comment(_)))
        .count()
}

/// Whatever write point the writer stops at, with or without a power cut, the batch is all
/// there or not at all, every save the person was told was saved is there, nothing of the
/// batch is left half-moved, and the folder is a valid burrito. (Titus holds no decision, so
/// the batch is 2,100 new comments: the staged path does not care what the records are.)
#[test]
fn crash_during_a_large_batch() {
    let (base_rig, folder) = titus();
    let base = base_rig.fs.clone();
    // The write points of a clean run, to cover every one.
    let total = {
        let probe = base.copy();
        let rig = rig_as(probe.clone(), APP, "host-1");
        let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
        handle.wait_idle();
        let before = probe.ops();
        handle
            .submit(Command::ApplyUpgrade {
                plan: comment_plan(2_100),
            })
            .wait()
            .unwrap();
        let ops = probe.ops() - before;
        handle.close().unwrap();
        ops
    };
    assert!(total > 10, "{total} write points");
    let (mut seen_all, mut seen_none, mut rolled_forward) = (false, false, false);
    for k in 0..=total {
        for cut in [false, true] {
            let fs = base.copy();
            let rig = rig_as(fs.clone(), APP, "host-1");
            let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
            handle.wait_idle();
            let saved = save_verse(&handle, 1, 1, &format!("saved before {k} {cut}")).unwrap();
            assert_eq!(saved.stored, Stored::Durable);
            fs.kill_before(k, false);
            let result = handle
                .submit(Command::ApplyUpgrade {
                    plan: comment_plan(2_100),
                })
                .wait();
            let after = fs.crash(cut);
            drop(handle);
            drop(rig);
            let rig = rig_as(after.clone(), APP, "host-1");
            let reopened = rig.store.open(&folder, &OpenOptions::default()).unwrap();
            reopened.wait_idle();
            let found = comments(reopened.snapshot().fold());
            assert!(
                found == 0 || found == 2_100,
                "k={k} cut={cut}: {found} of 2100"
            );
            if result.is_ok() && !cut {
                assert_eq!(found, 2_100, "k={k} cut={cut}: told it was applied");
            }
            assert_eq!(
                words(&reopened, 1, 1).as_deref(),
                Some(format!("saved before {k} {cut}").as_str()),
                "k={k} cut={cut}: a save the person was told was saved"
            );
            let staged = folder.join(".tc/staged");
            assert!(
                tc_store::Vfs::read_dir(&after, &staged).map_or(true, |e| e.is_empty()),
                "k={k} cut={cut}: a staged batch is left"
            );
            rolled_forward |= result.is_err() && found == 2_100;
            seen_all |= found == 2_100;
            seen_none |= found == 0;
            reopened.close().unwrap();
            assert_valid_burrito(&after, &folder);
        }
    }
    assert!(seen_all && seen_none, "the points cover both outcomes");
    assert!(
        rolled_forward,
        "a stop after the decision is finished by the next open"
    );
}
