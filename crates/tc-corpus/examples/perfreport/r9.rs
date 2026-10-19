//! Saves and reads while background work runs (APP-DESIGN §9.2 R9, PERFORMANCE.md: a save
//! waits at most 25 ms p95 while a materialize, a checkpoint, a merge commit or an upgrade
//! write runs), and the session tC4's J8 asks for (a checkpoint at every mode switch).
//!
//! Each case runs a job and, on a second thread, a save every 20 ms (a verse, a decision, an
//! alignment in turn, on records the job does not touch) until the job ends.

use crate::common::{
    align_one, alignments_of, cost, decide_one, decisions_of, ms, open, project_folder, run_id,
    save_one, store, store_with, verses_across,
};
use crate::table::Measured;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tc_journal::command::{Command, PlannedChange, UpgradePlan};
use tc_journal::keys::ToolId;
use tc_journal::pins::Collection;
use tc_journal::record::{CheckValue, PinGroupValue};
use tc_journal::{BookId, Cause, GitOid, Record, RecordKey};
use tc_store::{
    CheckpointReason, Leave, ManualClock, ProjectHandle, StoreCounters, StoreEvent, Work,
};

/// The progress reports of a project's materializations.
#[derive(Clone)]
pub struct ProgressEvents(Arc<Mutex<Vec<Instant>>>);

impl ProgressEvents {
    pub fn clear(&self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    pub fn take(&self) -> Vec<Instant> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

pub fn watch_progress(handle: &ProjectHandle) -> ProgressEvents {
    let events = ProgressEvents(Arc::default());
    let into = events.clone();
    let receiver = handle.subscribe();
    std::thread::spawn(move || {
        for event in receiver {
            if let StoreEvent::Progress {
                work: Work::Materialize,
                ..
            } = event
            {
                into.0
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(Instant::now());
            }
        }
    });
    events
}

pub fn pins_value(run: u128, n: u32) -> PinGroupValue {
    use std::collections::BTreeMap;
    use tc_journal::json::{Json, JsonMap};
    use tc_journal::keys::SlotName;
    use tc_journal::record::SlotValue;
    PinGroupValue(BTreeMap::from([(
        SlotName::parse("ugl").expect("a slot name"),
        SlotValue(Json::Obj(JsonMap::from([
            (
                "repoPath".to_owned(),
                Json::from("git.door43.org/unfoldingWord/en_ugl"),
            ),
            (
                "sha".to_owned(),
                Json::from(format!("{:040x}", run + u128::from(n))),
            ),
            ("version".to_owned(), Json::from(format!("v{n}"))),
        ]))),
    )]))
}

/// What the saving thread targets: records the job does not touch.
struct Targets {
    verses: Vec<RecordKey>,
    decisions: Vec<RecordKey>,
    alignments: Vec<RecordKey>,
}

impl Targets {
    fn of(handle: &ProjectHandle, decisions: bool) -> Self {
        // The job's own saves use the first verses; these start further in.
        let verses = verses_across(handle, BookId::Psalms, 600).split_off(300);
        Self {
            verses,
            decisions: if decisions {
                decisions_of(handle, BookId::Psalms, 700).split_off(300)
            } else {
                Vec::new()
            },
            alignments: alignments_of(handle, BookId::Psalms, 400).split_off(100),
        }
    }
}

/// One save made while a job ran.
#[derive(Clone, Copy)]
pub struct Save {
    pub ms: f64,
}

/// Saves every 20 ms, a verse, a decision and an alignment in turn, until `stop`.
fn save_loop(
    handle: &ProjectHandle,
    targets: &Targets,
    stop: &AtomicBool,
    tag: &str,
    period: Duration,
) -> Vec<Save> {
    let run = run_id();
    let mut saves = Vec::new();
    let (mut v, mut d, mut a) = (0usize, 0usize, 0usize);
    let mut i = 0usize;
    while !stop.load(Ordering::Acquire) {
        let kinds = if targets.decisions.is_empty() { 2 } else { 3 };
        let took = match i % kinds {
            0 => {
                v += 1;
                save_one(
                    handle,
                    &targets.verses[v % targets.verses.len()],
                    format!("Saved during {tag} number {i} of run {run}"),
                )
            }
            1 if kinds == 2 => {
                a += 1;
                align_one(handle, &targets.alignments[a % targets.alignments.len()])
            }
            1 => {
                d += 1;
                decide_one(handle, &targets.decisions[d % targets.decisions.len()])
            }
            _ => {
                a += 1;
                align_one(handle, &targets.alignments[a % targets.alignments.len()])
            }
        };
        saves.push(Save { ms: took });
        i += 1;
        std::thread::sleep(period);
    }
    saves
}

/// Reads a book and a chapter every 100 ms until `stop`: the open-a-book and move-chapter
/// rows while the job runs.
fn read_loop(handle: &ProjectHandle, stop: &AtomicBool) -> (Vec<f64>, Vec<f64>) {
    let (mut books, mut chapters) = (Vec::new(), Vec::new());
    let mut i = 0usize;
    while !stop.load(Ordering::Acquire) {
        let started = Instant::now();
        let read = handle.read_book(BookId::Psalms).wait();
        books.push(ms(started));
        assert!(read.usfm.is_some());
        let snapshot = handle.snapshot();
        let chapter = (i * 37 % 150 + 1) as u16;
        let started = Instant::now();
        let _ = snapshot.fold().chapter_keys(BookId::Psalms, chapter);
        let usfm = snapshot.fold().chapter_usfm(BookId::Psalms, chapter);
        chapters.push(ms(started));
        assert!(usfm.is_some());
        i += 1;
        std::thread::sleep(Duration::from_millis(100));
    }
    (books, chapters)
}

/// What a job did while the saves went on.
struct Window<R> {
    result: R,
    job_ms: f64,
    saves: Vec<Save>,
    books: Vec<f64>,
    chapters: Vec<f64>,
    delta: StoreCounters,
}

fn during<R: Send>(
    handle: &ProjectHandle,
    targets: &Targets,
    tag: &str,
    reads: bool,
    job: impl FnOnce() -> R + Send,
) -> Window<R> {
    let stop = AtomicBool::new(false);
    let before = handle.counters();
    std::thread::scope(|scope| {
        let saves =
            scope.spawn(|| save_loop(handle, targets, &stop, tag, Duration::from_millis(20)));
        let readers = reads.then(|| scope.spawn(|| read_loop(handle, &stop)));
        // Let the saver get into its stride before the job starts.
        std::thread::sleep(Duration::from_millis(100));
        let started = Instant::now();
        let result = job();
        let job_ms = ms(started);
        stop.store(true, Ordering::Release);
        let saves = saves.join().unwrap_or_default();
        let (books, chapters) = readers.and_then(|r| r.join().ok()).unwrap_or_default();
        Window {
            result,
            job_ms,
            saves,
            books,
            chapters,
            delta: handle.counters().since(&before),
        }
    })
}

fn save_row<R>(id: &str, w: &Window<R>, extra: &str) -> Measured {
    let samples: Vec<f64> = w.saves.iter().map(|s| s.ms).collect();
    Measured::new(
        id,
        samples,
        format!(
            "job {:.0} ms; {extra}; queue wait max {:.1} ms; writer yielded {} ms; {}",
            w.job_ms,
            w.delta.queue_wait_max_ns as f64 / 1e6,
            w.delta.writer_yield_ns / 1_000_000,
            cost(&w.delta)
        ),
    )
}

/// A Psalms save, then the book is left and its files are written, 20 times.
pub fn materialize(dir: &Path) -> Vec<Measured> {
    crate::common::header("r9.materialize");
    let store = store(dir);
    let handle = open(&store, dir);
    let targets = Targets::of(&handle, true);
    let own = verses_across(&handle, BookId::Psalms, 40);
    let run = run_id();
    let w = during(&handle, &targets, "a book's materialize", true, || {
        for (i, key) in own.iter().enumerate().take(20) {
            save_one(&handle, key, format!("A job save {i} of run {run}"));
            handle
                .leave(Leave::Book(BookId::Psalms))
                .wait()
                .expect("leave");
        }
    });
    let mut rows = vec![
        save_row("r9.materialize", &w, "20 saves and materializes"),
        Measured::new("read.book_during_materialize", w.books.clone(), ""),
        Measured::new("read.chapter_during_materialize", w.chapters.clone(), ""),
    ];
    rows[0].fsyncs = None;
    handle.close().expect("close");
    rows
}

/// A pins change, then the whole-project materialize, three times.
pub fn whole(dir: &Path) -> Vec<Measured> {
    crate::common::header("r9.whole");
    let store = store(dir);
    let handle = open(&store, dir);
    let targets = Targets::of(&handle, true);
    let run = run_id();
    let w = during(
        &handle,
        &targets,
        "a whole-project materialize",
        true,
        || {
            for n in 0..3u32 {
                let key = RecordKey::Pins(tc_journal::PinGroup::Lexicon);
                let shown = handle.mint(&key, None).expect("a token");
                handle
                    .submit(Command::SetPins {
                        shown,
                        value: pins_value(run, n),
                    })
                    .wait()
                    .expect("pins");
                handle.leave(Leave::Mode).wait().expect("leave");
            }
        },
    );
    let rows = vec![
        save_row("r9.whole", &w, "3 pins changes"),
        Measured::new("read.book_during_whole", w.books.clone(), ""),
        Measured::new("read.chapter_during_whole", w.chapters.clone(), ""),
    ];
    handle.close().expect("close");
    rows
}

/// A Psalms save and a checkpoint, 20 times.
pub fn checkpoint(dir: &Path) -> Vec<Measured> {
    crate::common::header("r9.checkpoint");
    let store = store(dir);
    let handle = open(&store, dir);
    let targets = Targets::of(&handle, true);
    let own = verses_across(&handle, BookId::Psalms, 40);
    let run = run_id();
    let w = during(&handle, &targets, "a checkpoint", false, || {
        for (i, key) in own.iter().enumerate().take(20) {
            save_one(&handle, key, format!("A job save {i} of run {run}"));
            handle
                .checkpoint(CheckpointReason::LeavingTranslate)
                .wait()
                .expect("checkpoint");
        }
    });
    let rows = vec![save_row("r9.checkpoint", &w, "20 checkpoints")];
    handle.close().expect("close");
    rows
}

/// Checkpoints after Psalms saves until the loose objects pass §9.8's threshold, then saves
/// while the repack runs.
pub fn repack(dir: &Path) -> Vec<Measured> {
    crate::common::header("r9.repack");
    let store = store(dir);
    let handle = open(&store, dir);
    let root = project_folder(dir);
    let targets = Targets::of(&handle, true);
    let own = verses_across(&handle, BookId::Psalms, 200);
    let run = run_id();
    let repacks = handle.counters().git_repacks;
    let mut made = 0;
    for (i, key) in own.iter().enumerate().take(200) {
        save_one(&handle, key, format!("A repack save {i} of run {run}"));
        handle
            .checkpoint(CheckpointReason::LeavingTranslate)
            .wait()
            .expect("checkpoint");
        made += 1;
        if handle.counters().git_repacks > repacks {
            break;
        }
        let (loose, bytes) = crate::common::loose_objects(&root);
        if loose >= tc_store::REPACK_OBJECTS || bytes >= tc_store::REPACK_BYTES {
            break;
        }
    }
    let (loose, bytes) = crate::common::loose_objects(&root);
    let w = during(&handle, &targets, "a repack", false, || {
        let started = Instant::now();
        while handle.counters().git_repacks == repacks
            && started.elapsed() < Duration::from_secs(180)
        {
            std::thread::sleep(Duration::from_millis(50));
        }
    });
    let row = save_row(
        "r9.repack",
        &w,
        &format!("{made} checkpoints, then {loose} loose objects ({bytes} B)"),
    );
    handle.close().expect("close");
    vec![row]
}

/// The merge-commit proxy: the first checkpoint after a whole-project materialize (the
/// biggest tree a commit writes) with saves going on. Unit X adds the real merge commit.
pub fn merge_proxy(dir: &Path) -> Vec<Measured> {
    crate::common::header("r9.merge_proxy");
    let store = store(dir);
    let handle = open(&store, dir);
    let targets = Targets::of(&handle, true);
    let run = run_id();
    let key = RecordKey::Pins(tc_journal::PinGroup::Lexicon);
    let shown = handle.mint(&key, None).expect("a token");
    handle
        .submit(Command::SetPins {
            shown,
            value: pins_value(run, 7),
        })
        .wait()
        .expect("pins");
    handle.leave(Leave::Mode).wait().expect("leave");
    handle.wait_idle();
    let w = during(
        &handle,
        &targets,
        "the first checkpoint after a whole-project job",
        false,
        || {
            handle
                .checkpoint(CheckpointReason::BeforeSync)
                .wait()
                .expect("checkpoint")
        },
    );
    let row = save_row(
        "r9.merge_proxy",
        &w,
        &format!("a proxy: commit {:?}", w.result.commit.is_some()),
    );
    handle.close().expect("close");
    vec![row]
}

fn copy_of(dir: &Path, name: &str) -> std::path::PathBuf {
    let store = store(dir);
    let target = dir.join("projects").join(name);
    if target.exists() {
        std::fs::remove_dir_all(&target).expect("an old copy goes");
    }
    store
        .copy_project(&project_folder(dir), name)
        .expect("the project copies");
    target
}

/// The month-end consolidation (§4.7) with saves going on, on a copy of the project: a
/// manual clock lives through 21 working days of 275 verse saves and four checkpoints each, in
/// 2026-11, then moves into December and a checkpoint consolidates November's own packs.
pub fn consolidation(dir: &Path) -> Vec<Measured> {
    crate::common::header("r9.consolidation");
    let copy = copy_of(dir, "bible-r9");
    let clock = Arc::new(ManualClock::at(1_793_606_400_000));
    let store = store_with(dir, clock.clone());
    let handle = store
        .open(&copy, &tc_store::OpenOptions::default())
        .expect("the copy opens");
    handle.wait_idle();
    let own = verses_across(&handle, BookId::Psalms, 600);
    let targets = Targets::of(&handle, false);
    let run = run_id();
    let mut n = 0usize;
    for day in 0..21 {
        clock.advance(86_400_000);
        for quarter in 0..4 {
            for _ in 0..69 {
                save_one(
                    &handle,
                    &own[n % 300],
                    format!("A consolidation save {n} of run {run}"),
                );
                n += 1;
            }
            handle
                .checkpoint(CheckpointReason::LeavingTranslate)
                .wait()
                .expect("checkpoint");
            let _ = (day, quarter);
        }
    }
    eprintln!("{n} saves in 84 checkpoints");
    // December.
    clock.advance(1_796_112_000_000 - 1_793_606_400_000 - 21 * 86_400_000);
    let w = during(
        &handle,
        &targets,
        "the month's consolidation",
        false,
        || {
            save_one(
                &handle,
                &own[0],
                format!("The last save before December {run}"),
            );
            handle
                .checkpoint(CheckpointReason::LeavingTranslate)
                .wait()
                .expect("checkpoint")
        },
    );
    let packs = w.result.sealed.len();
    let row = save_row(
        "r9.consolidation",
        &w,
        &format!(
            "{n} saves over 21 days, then a checkpoint that sealed {packs} packs and consolidated November"
        ),
    );
    handle.close().expect("close");
    let _ = std::fs::remove_dir_all(&copy);
    vec![row]
}

/// The plan of a resource upgrade: every decision of the two tools carried to a new version
/// of its resource, as a person confirming the upgrade of the whole Bible plans it.
fn upgrade_plan(handle: &ProjectHandle) -> UpgradePlan {
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    let mut changes = Vec::new();
    let new_sha: GitOid = format!("{:040x}", run_id())
        .parse()
        .expect("a git object id");
    for tool in [ToolId::TranslationNotes, ToolId::TranslationWords] {
        for book in fold.projected_books() {
            for decision in fold.decisions(tool, Collection::Book(book)) {
                let key = RecordKey::Check(decision.key.clone());
                let mut record = decision.record;
                record.resource.sha = new_sha;
                changes.push(PlannedChange {
                    record: Record::Check {
                        key: decision.key,
                        value: CheckValue::Decision(record),
                    },
                    parents: fold.heads(&key),
                    cause: Cause::CarryOver {
                        group: tc_journal::PinGroup::Primary,
                    },
                });
            }
        }
    }
    UpgradePlan { changes }
}

/// A resource upgrade that carries the decisions of the whole Bible (150 to 260 MB of
/// changes), with saves going on, on a copy of the project; then the whole-project
/// materialize it causes.
pub fn upgrade(dir: &Path) -> Vec<Measured> {
    crate::common::header("r9.upgrade");
    let copy = copy_of(dir, "bible-r9");
    let store = store(dir);
    let handle = store
        .open(&copy, &tc_store::OpenOptions::default())
        .expect("the copy opens");
    handle.wait_idle();
    let before = crate::machine::private_mib().unwrap_or(f64::NAN);
    let targets = Targets::of(&handle, false);
    let started = Instant::now();
    let plan = upgrade_plan(&handle);
    let planned = plan.changes.len();
    let plan_ms = ms(started);
    let w = during(&handle, &targets, "an upgrade write", false, || {
        let started = Instant::now();
        let reply = handle.submit(Command::ApplyUpgrade { plan }).wait();
        (reply.map(|r| r.changes.len()), ms(started))
    });
    let replied = crate::machine::private_mib().unwrap_or(f64::NAN);
    let (result, reply_ms) = &w.result;
    let upgrade_row = save_row(
        "r9.upgrade",
        &w,
        &format!(
            "{planned} planned changes in {plan_ms:.0} ms, reply {reply_ms:.0} ms {result:?}; private {before:.0} MiB before, {replied:.0} MiB at the reply"
        ),
    );
    let w2 = during(
        &handle,
        &targets,
        "the upgrade's materialize",
        false,
        || {
            handle.leave(Leave::Mode).wait().expect("leave");
        },
    );
    let mut rows = vec![upgrade_row];
    let mut after = save_row(
        "r9.upgrade",
        &w2,
        "the whole-project materialize the upgrade causes",
    );
    after.id = "r9.upgrade_materialize".into();
    rows.push(after);
    handle.wait_idle();
    std::thread::sleep(Duration::from_secs(60));
    let minute = crate::machine::private_mib().unwrap_or(f64::NAN);
    rows[0]
        .note
        .push_str(&format!("; {minute:.0} MiB a minute later"));
    handle.close().expect("close");
    let _ = std::fs::remove_dir_all(&copy);
    rows
}

/// tC4's J8 (AD34, Q-5): 400 decisions at one per 300 ms with a mode switch and a
/// checkpoint every 10, against the same session with no checkpoints.
pub fn j8(dir: &Path) -> Vec<Measured> {
    crate::common::header("r9.j8");
    let store = store(dir);
    let handle = open(&store, dir);
    let keys = decisions_of(&handle, BookId::Psalms, 1000);
    let mut rows = Vec::new();
    for (name, checkpoints) in [
        ("with a checkpoint at every mode switch", true),
        ("without checkpoints", false),
    ] {
        let offset = if checkpoints { 0 } else { 500 };
        let before = handle.counters();
        let commits_before = handle.counters().git_commits_written;
        let mut all = Vec::new();
        let mut overlapping = Vec::new();
        let mut pending: Option<
            tc_store::Pending<Result<tc_store::CheckpointReport, tc_store::StoreError>>,
        > = None;
        for i in 0..400usize {
            let busy_before = pending.as_ref().is_some_and(|p| !p.is_ready());
            let took = decide_one(&handle, &keys[offset + i]);
            let busy_after = pending.as_ref().is_some_and(|p| !p.is_ready());
            all.push(took);
            if busy_before || busy_after {
                overlapping.push(took);
            }
            if checkpoints && (i + 1) % 10 == 0 {
                drop(handle.leave(Leave::Mode));
                if pending.as_ref().is_none_or(tc_store::Pending::is_ready) {
                    pending = Some(handle.checkpoint(CheckpointReason::LeavingTranslate));
                }
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        if let Some(p) = pending.take() {
            let _ = p.wait();
        }
        handle.wait_idle();
        let delta = handle.counters().since(&before);
        let commits = handle.counters().git_commits_written - commits_before;
        let id = if checkpoints { "r9.j8" } else { "r9.j8_none" };
        rows.push(Measured::new(
            id,
            all,
            format!(
                "{name}: {} decisions overlapped a checkpoint (p95 {:.1} ms); commits {commits}; blobs {}, trees {}; {}",
                overlapping.len(),
                crate::table::p95(&overlapping),
                delta.git_blobs_written,
                delta.git_trees_written,
                cost(&delta)
            ),
        ));
    }
    handle.close().expect("close");
    rows
}
