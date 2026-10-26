//! Release benchmarks of the store on the whole-Bible corpus (`docs/PERFORMANCE.md` rule 5,
//! `docs/STORAGE.md` §14): the project is written to a real folder through the store's own
//! open, seal, consolidation and materialization, then opened again and again, and saves
//! and decisions are timed until their durable reply with the whole Bible open.
//!
//! `cargo run --release -p tc-store --example storebench -- setup DIR` writes the project
//! (`DIR/projects/bible`, app data in `DIR/app`); `... -- open DIR CASE` opens it once in a
//! process of its own and prints one line (CASE: `warm`, `nocache`, `cold`, which first asks
//! the kernel to drop its page cache); `... -- run DIR [runs]` runs the opens in child
//! processes, then the saves, and prints the report (with `STOREBENCH_OTHER` naming a
//! binary of another build, also the opens that take the fold cache that build wrote);
//! `... -- saves DIR` times the saves alone.
//!
//! Unit S2 adds the cases of the materialize, close and memory budgets
//! (`docs/PERFORMANCE.md`): `materialize DIR` (one book after one save or decision),
//! `whole DIR` (a pins change over the whole Bible, with saves meanwhile), `checkpoint DIR`,
//! `close DIR`, `outside DIR` (three verses of Psalms edited by another program) and
//! `memory DIR` (private bytes a minute after the last action, then a whole-project
//! materialize and the close). Each prints the machine's SHA and AVX-512 flags and the load.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]
#![allow(
    clippy::disallowed_methods,
    reason = "a benchmark that builds and removes its own scratch projects"
)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tc_corpus::{Corpus, Spec};
use tc_journal::command::{Command, DecisionChange, DecisionEdit};
use tc_journal::format::FormatDoc;
use tc_journal::keys::{CheckTarget, ToolId};
use tc_journal::record::CheckValue;
use tc_journal::{BookId, ChapterNo, Record, RecordKey, VerseNo};
use tc_store::layout::{Folder, GITATTRIBUTES_BYTES, GITIGNORE_BYTES};
use tc_store::{
    CheckpointReason, Leave, OpenOptions, Progress, ProgressSink, ProjectHandle, StdFs,
    StdPlatform, Store, StoreConfig, StoreEvent, SystemClock, Work,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // An absolute name: the file watcher reports paths as the system resolves them.
    let dir = PathBuf::from(args.get(1).expect("DIR"));
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    match args.first().map(String::as_str) {
        Some("setup") => setup(&dir),
        Some("open") => open_once(&dir, args.get(2).map_or("warm", String::as_str)),
        Some("run") => run(&dir, args.get(2).and_then(|n| n.parse().ok()).unwrap_or(7)),
        Some("saves") => saves(&dir),
        Some("materialize") => materialize_case(&dir),
        Some("whole") => whole_case(&dir),
        Some("checkpoint") => checkpoint_case(&dir),
        Some("close") => close_case(&dir),
        Some("outside") => outside_case(&dir),
        Some("memory") => memory_case(&dir),
        Some("repack") => repack_case(&dir),
        _ => {
            eprintln!(
                "storebench setup DIR | open DIR CASE | run DIR [runs] | saves DIR | \
                 materialize DIR | whole DIR | checkpoint DIR | close DIR | outside DIR | memory DIR | repack DIR"
            );
            std::process::exit(2);
        }
    }
}

fn store(dir: &Path) -> Store {
    let store = Store::start(StoreConfig {
        app_data: dir.join("app"),
        projects: dir.join("projects"),
        vfs: Arc::new(StdFs),
        platform: Arc::new(StdPlatform),
        clock: Arc::new(SystemClock),
        threads: std::thread::available_parallelism().map_or(2, |n| n.get()),
        // STOREBENCH_NO_WATCH=1: the watcher's word of a change comes an hour late (a
        // comparison of the cases with and without what the watcher makes the writer do).
        watch_debounce: if std::env::var_os("STOREBENCH_NO_WATCH").is_some() {
            std::time::Duration::from_secs(3600)
        } else {
            std::time::Duration::from_secs(1)
        },
    })
    .expect("store");
    store.wait_cleanup();
    // The cases measure one thing each: a repack (docs/STORAGE.md §9.8) runs only in the
    // case that asks for it.
    store.hooks().repack_after(u64::MAX, u64::MAX);
    store
}

fn folder(dir: &Path) -> PathBuf {
    dir.join("projects").join("bible")
}

fn put(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn setup(dir: &Path) {
    let started = Instant::now();
    let corpus = Corpus::generate(&Spec::whole_bible(1)).expect("corpus");
    eprintln!(
        "generated {} changes in {:?}",
        corpus.journal.stats.changes,
        started.elapsed()
    );
    let store = store(dir);
    let root = folder(dir);
    let f = Folder::new(&root);
    let journal = &corpus.journal;
    let project = serde_json::json!({
        "schemaVersion": 1,
        "projectId": journal.project.to_string(),
        "kind": "bible",
        "created": "2026-10-17T00:00:00.000Z",
    });
    put(
        &f.path("ingredients/tc/project.json"),
        project.to_string().as_bytes(),
    );
    put(
        &f.path("ingredients/tc/format.json"),
        &FormatDoc::current().to_bytes(),
    );
    put(&f.path(".gitignore"), GITIGNORE_BYTES);
    put(&f.path(".gitattributes"), GITATTRIBUTES_BYTES);
    for pack in &journal.packs {
        put(&f.path(&pack.path), &pack.bytes);
    }
    for pending in &journal.pending {
        put(
            &f.path(&format!(".tc/pending/{}", pending.name)),
            &pending.bytes,
        );
    }
    let canonical = std::fs::canonicalize(&root).unwrap();
    let print = tc_store::device::fingerprint(
        store.app_data().secret(),
        &tc_store::Platform::machine_id(&StdPlatform),
        &canonical.to_string_lossy(),
    );
    let device = serde_json::json!({
        "device": journal.devices[0].to_string(),
        "fingerprint": print.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        "created": "2026-10-17T00:00:00.000Z",
    });
    put(&f.path(".tc/device.json"), device.to_string().as_bytes());
    drop(corpus);
    let started = Instant::now();
    let handle = store.open(&root, &OpenOptions::default()).expect("open");
    eprintln!(
        "first open (every file projected and written): {:?}",
        started.elapsed()
    );
    let started = Instant::now();
    handle
        .checkpoint(tc_store::CheckpointReason::AfterImport)
        .wait()
        .expect("checkpoint");
    eprintln!(
        "checkpoint (seal, consolidation, materialization): {:?}",
        started.elapsed()
    );
    handle.wait_idle();
    handle.close().expect("close");
    let problems = tc_burrito::validate_dir(&root);
    eprintln!("burrito problems: {}", problems.len());
}

fn status_mib(field: &str) -> f64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status
        .lines()
        .find(|l| l.starts_with(field))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|kb| kb.parse::<f64>().ok())
        .unwrap_or(0.0)
        / 1024.0
}

fn open_once(dir: &Path, case: &str) {
    let root = folder(dir);
    let index = root.join(".tc/index.bin");
    if case == "nocache" {
        let _ = std::fs::remove_file(&index);
    }
    if case == "cold" {
        let dropped = std::fs::write("/proc/sys/vm/drop_caches", "3").is_ok();
        if !dropped {
            println!("cold: the page cache cannot be dropped here (no privilege)");
            return;
        }
    }
    let load_before = loadavg();
    let store = store(dir);
    let reports: Arc<Mutex<Vec<(Duration, Progress)>>> = Arc::default();
    let started = Instant::now();
    let into = reports.clone();
    let sink: ProgressSink = Arc::new(move |p| into.lock().unwrap().push((started.elapsed(), p)));
    let handle = store
        .open(
            &root,
            &OpenOptions {
                progress: sink,
                ..OpenOptions::default()
            },
        )
        .expect("open");
    let usable = started.elapsed();
    let peak = status_mib("VmHWM:");
    // The counters when the project is usable, and again when the open's background work
    // (the fold cache's write and the storage check, O14) has ended: the check reads every
    // derived file once, so the two differ by exactly its cost.
    let counters = handle.counters();
    handle.wait_idle();
    let idle = started.elapsed();
    let after = handle.counters();
    let steady = status_mib("VmRSS:");
    let reports = reports.lock().unwrap().clone();
    let first = reports
        .first()
        .map(|(at, p)| format!("{at:?} {:?} {}/{}", p.phase, p.done, p.total));
    println!(
        "{case}: usable {:.3} s, background done {:.3} s, peak {peak:.0} MiB, steady {steady:.0} MiB, \
         files opened {} (after the background work {}), journal files {} ({} from the cache), \
         bytes read {} ({}), projected {}, derived read {} (storage check read {} files), \
         first report {}, reports {}, load {} before, {} after",
        usable.as_secs_f64(),
        idle.as_secs_f64(),
        counters.files_opened,
        after.files_opened,
        counters.journal_files_read,
        counters.journal_files_cached,
        counters.bytes_read,
        after.bytes_read,
        counters.projected,
        counters.derived_read,
        after.check_read,
        first.unwrap_or_else(|| "none".into()),
        reports.len(),
        load_before,
        loadavg(),
    );
    handle.close().expect("close");
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let at = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[at]
}

fn usable_of(line: &str) -> Option<f64> {
    line.split("usable ")
        .nth(1)?
        .split(' ')
        .next()?
        .parse()
        .ok()
}

fn loadavg() -> String {
    std::fs::read_to_string("/proc/loadavg")
        .unwrap_or_default()
        .trim()
        .to_owned()
}

fn run(dir: &Path, runs: usize) {
    let exe = std::env::current_exe().unwrap();
    println!("load before: {}", loadavg());
    let cpu = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let flags = cpu
        .lines()
        .find(|l| l.starts_with("flags"))
        .unwrap_or_default();
    println!(
        "cpu: {} cores, sha_ni {}, avx512f {}",
        std::thread::available_parallelism().map_or(0, |n| n.get()),
        flags.contains(" sha_ni"),
        flags.contains(" avx512f")
    );
    // `update`: the fold cache written by another build of the same cache encoding (a
    // binary built with other features, named by STOREBENCH_OTHER), as after an app update.
    let other = std::env::var_os("STOREBENCH_OTHER").map(PathBuf::from);
    for case in ["warm", "cold", "nocache", "update"] {
        if case == "update" && other.is_none() {
            println!("update: skipped, STOREBENCH_OTHER names no other build");
            continue;
        }
        let mut times = Vec::new();
        for _ in 0..runs {
            if case == "update"
                && let Some(other) = &other
            {
                let out = std::process::Command::new(other)
                    .args(["open", &dir.to_string_lossy(), "nocache"])
                    .output()
                    .unwrap();
                println!(
                    "  other build wrote the cache: {}",
                    String::from_utf8_lossy(&out.stdout).trim()
                );
            }
            let open_case = if case == "update" { "warm" } else { case };
            let out = std::process::Command::new(&exe)
                .args(["open", &dir.to_string_lossy(), open_case])
                .output()
                .unwrap();
            let line = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            println!("  {line}");
            match usable_of(&line) {
                Some(t) => times.push(t),
                None => break,
            }
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        if !times.is_empty() {
            println!(
                "{case}: {} runs, median {:.3} s, p95 {:.3} s",
                times.len(),
                percentile(&times, 0.5),
                percentile(&times, 0.95)
            );
        }
    }
    saves(dir);
    println!("load after: {}", loadavg());
}

fn saves(dir: &Path) {
    let store = store(dir);
    let root = folder(dir);
    let handle = store.open(&root, &OpenOptions::default()).expect("open");
    handle.wait_idle();
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    // Verses spread over every book.
    let mut verses: Vec<RecordKey> = Vec::new();
    for book in fold.projected_books() {
        for chapter in fold.chapters_of(book).into_iter().take(3) {
            verses.extend(fold.chapter_keys(book, chapter).verses.into_iter().take(2));
        }
    }
    verses.truncate(200);
    // Words no earlier run saved, so every save writes (an unchanged value writes nothing).
    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let mut verse_ms = Vec::new();
    let mut opened = 0u64;
    let mut fsyncs = 0u64;
    for (i, key) in verses.iter().enumerate() {
        let shown = handle.mint(key, None).unwrap();
        let before = handle.counters();
        let started = Instant::now();
        let reply = handle
            .submit(Command::SaveVerse {
                shown,
                words: format!("A saved verse number {i} of run {run}"),
            })
            .wait();
        verse_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        let reply = reply.expect("save");
        assert!(!reply.changes.is_empty(), "every save writes");
        let delta = handle.counters().since(&before);
        opened += delta.files_opened;
        fsyncs += delta.fsyncs;
    }
    // Decisions: a bookmark toggled on decided checks with a plain verse reference.
    let mut decision_ms = Vec::new();
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    let mut decisions = Vec::new();
    for book in fold.projected_books() {
        for key in fold
            .decisions_of(ToolId::TranslationWords, book)
            .into_iter()
            .take(4)
        {
            decisions.push(key);
        }
    }
    decisions.truncate(200);
    for key in decisions {
        let RecordKey::Check(check) = &key else {
            continue;
        };
        let CheckTarget::Verse {
            book,
            chapter,
            verse,
        } = &check.target
        else {
            continue;
        };
        let (Some(c), Some(v)) = (chapter.number(), verse.number()) else {
            continue;
        };
        let Some(Record::Check {
            value: CheckValue::Decision(d),
            ..
        }) = fold.projected(&key)
        else {
            continue;
        };
        let verse_key =
            RecordKey::Verse(*book, ChapterNo::new(c).unwrap(), VerseNo::new(v).unwrap());
        let shown = handle.mint(&key, None).unwrap();
        let verse_shown = handle.mint(&verse_key, None).unwrap();
        let started = Instant::now();
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
            .wait();
        decision_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        let reply = reply.expect("decision");
        assert!(!reply.changes.is_empty(), "every decision writes");
    }
    for (name, mut ms) in [("verse save", verse_ms), ("decision", decision_ms)] {
        ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "{name}: {} until the durable reply, median {:.2} ms, p95 {:.2} ms, max {:.2} ms",
            ms.len(),
            percentile(&ms, 0.5),
            percentile(&ms, 0.95),
            ms.last().copied().unwrap_or(f64::NAN)
        );
    }
    println!(
        "per verse save: {:.2} files opened, {:.2} fsyncs; steady {:.0} MiB, peak {:.0} MiB",
        opened as f64 / 200.0,
        fsyncs as f64 / 200.0,
        status_mib("VmRSS:"),
        status_mib("VmHWM:")
    );
    handle.close().expect("close");
}

// ---------------------------------------------------------------------------------------
// Unit S2: materialize, whole-project materialize, checkpoint, close, outside edits, memory
// ---------------------------------------------------------------------------------------

/// The machine, printed with every S2 case (PERFORMANCE.md rule 5: whether the CPU has SHA
/// and AVX-512 instructions, and how busy it is).
fn machine_line() -> String {
    let cpu = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let model = cpu
        .lines()
        .find(|l| l.starts_with("model name"))
        .and_then(|l| l.split(':').nth(1))
        .unwrap_or("unknown")
        .trim()
        .to_owned();
    let flags = cpu
        .lines()
        .find(|l| l.starts_with("flags"))
        .unwrap_or_default();
    format!(
        "machine: {model}, {} threads, sha_ni {}, avx512f {}, load {}",
        std::thread::available_parallelism().map_or(0, |n| n.get()),
        flags.contains(" sha_ni"),
        flags.contains(" avx512f"),
        loadavg()
    )
}

fn stats(name: &str, mut ms: Vec<f64>) {
    ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "{name}: {} runs, median {:.1} ms, p95 {:.1} ms, max {:.1} ms",
        ms.len(),
        percentile(&ms, 0.5),
        percentile(&ms, 0.95),
        ms.last().copied().unwrap_or(f64::NAN)
    );
}

fn run_id() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

/// The first `n` verse keys of a book, spread over its first chapters.
fn verses_of(handle: &ProjectHandle, book: BookId, n: usize) -> Vec<RecordKey> {
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    let mut out = Vec::new();
    for chapter in fold.chapters_of(book) {
        out.extend(fold.chapter_keys(book, chapter).verses);
        if out.len() >= n {
            break;
        }
    }
    out.truncate(n);
    out
}

/// A save of one verse; its time until the durable reply, in ms.
fn save_one(handle: &ProjectHandle, key: &RecordKey, words: String) -> f64 {
    let shown = handle.mint(key, None).unwrap();
    let started = Instant::now();
    let reply = handle.submit(Command::SaveVerse { shown, words }).wait();
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let reply = reply.expect("save");
    assert!(!reply.changes.is_empty(), "every save writes");
    ms
}

/// The decisions of a tool in a book that a bookmark can be toggled on.
fn decisions_of(handle: &ProjectHandle, book: BookId, n: usize) -> Vec<RecordKey> {
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    fold.decisions_of(ToolId::TranslationWords, book)
        .into_iter()
        .filter(|key| decision_target(key).is_some())
        .take(n)
        .collect()
}

fn decision_target(key: &RecordKey) -> Option<RecordKey> {
    let RecordKey::Check(check) = key else {
        return None;
    };
    let CheckTarget::Verse {
        book,
        chapter,
        verse,
    } = &check.target
    else {
        return None;
    };
    Some(RecordKey::Verse(
        *book,
        ChapterNo::new(chapter.number()?).unwrap(),
        VerseNo::new(verse.number()?).unwrap(),
    ))
}

/// A bookmark toggled on a decided check; its time until the durable reply, in ms.
fn decide_one(handle: &ProjectHandle, key: &RecordKey) -> f64 {
    let verse_key = decision_target(key).expect("a verse check");
    let Some(Record::Check {
        value: CheckValue::Decision(d),
        ..
    }) = handle.snapshot().fold().projected(key)
    else {
        panic!("a decided check");
    };
    let shown = handle.mint(key, None).unwrap();
    let verse_shown = handle.mint(&verse_key, None).unwrap();
    let started = Instant::now();
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
        .wait();
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    reply.expect("decision");
    ms
}

/// The counters a case reports: what one action opened, read, wrote and hashed.
fn cost(label: &str, delta: &tc_store::StoreCounters) -> String {
    format!(
        "{label}: files opened {}, created {}, derived read {}, projected {}, derived written {}, \
         bytes read {}, written {}, hashed {}, digest bytes {}, fsyncs {}, writer yielded {} ms",
        delta.files_opened,
        delta.files_created,
        delta.derived_read,
        delta.projected,
        delta.derived_written,
        delta.bytes_read,
        delta.bytes_written,
        delta.bytes_hashed,
        delta.digest_bytes,
        delta.fsyncs,
        delta.writer_yield_ns / 1_000_000
    )
}

/// "Materialize one book" (PERFORMANCE.md: 1 s p95 on the reference laptop): the wall time
/// from the person leaving the book to the writer being done, after one verse save or one
/// decision, with the whole Bible open.
fn materialize_case(dir: &Path) {
    println!("{}", machine_line());
    let store = store(dir);
    let handle = store
        .open(&folder(dir), &OpenOptions::default())
        .expect("open");
    handle.wait_idle();
    let run = run_id();
    for (name, book, decide) in [
        ("Psalms after one verse save", BookId::Psalms, false),
        ("Psalms after one decision", BookId::Psalms, true),
        ("Jude after one verse save", BookId::Jude, false),
    ] {
        let verses = verses_of(&handle, book, 20);
        let decisions = decisions_of(&handle, book, 20);
        let mut times = Vec::new();
        let mut last = tc_store::StoreCounters::default();
        for i in 0..20 {
            if decide {
                decide_one(&handle, &decisions[i % decisions.len()]);
            } else {
                save_one(
                    &handle,
                    &verses[i % verses.len()],
                    format!("A saved verse {i} of run {run}"),
                );
            }
            let before = handle.counters();
            let started = Instant::now();
            handle.leave(Leave::Book(book)).wait().expect("leave");
            times.push(started.elapsed().as_secs_f64() * 1000.0);
            last = handle.counters().since(&before);
        }
        stats(name, times);
        println!("  {}", cost("the last run", &last));
        handle.wait_idle();
        // The file watcher tells the app of its own writes a second later (§11.1): what that
        // echo costs.
        let before = handle.counters();
        std::thread::sleep(Duration::from_secs(3));
        handle.wait_idle();
        println!(
            "  {}",
            cost(
                "the 3 s after the last run (the watcher's echo)",
                &handle.counters().since(&before)
            )
        );
    }
    handle.close().expect("close");
}

/// What a case that follows progress events records.
struct ProgressLog {
    events: std::sync::Arc<Mutex<Vec<(Instant, Progress)>>>,
}

impl ProgressLog {
    fn of(handle: &ProjectHandle) -> Self {
        let events: std::sync::Arc<Mutex<Vec<(Instant, Progress)>>> = Arc::default();
        let into = events.clone();
        let receiver = handle.subscribe();
        std::thread::spawn(move || {
            for event in receiver {
                if let StoreEvent::Progress {
                    work: Work::Materialize,
                    progress,
                } = event
                {
                    into.lock().unwrap().push((Instant::now(), progress));
                }
            }
        });
        Self { events }
    }
}

fn pins_value(run: u128, n: u32) -> tc_journal::record::PinGroupValue {
    use tc_journal::json::{Json, JsonMap};
    use tc_journal::keys::SlotName;
    use tc_journal::record::{PinGroupValue, SlotValue};
    PinGroupValue(std::collections::BTreeMap::from([(
        SlotName::parse("ugl").unwrap(),
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

/// "Materialize many or all derived files" with saves going on (PERFORMANCE.md: 25 ms p95 for
/// a save, a decision or an alignment save while a whole-project materialize runs; progress
/// from 300 ms on): a pins change over the whole Bible, five times.
fn whole_case(dir: &Path) {
    println!("{}", machine_line());
    let store = store(dir);
    let handle = store
        .open(&folder(dir), &OpenOptions::default())
        .expect("open");
    handle.wait_idle();
    let log = ProgressLog::of(&handle);
    let verses = verses_of(&handle, BookId::Psalms, 100);
    let decisions = decisions_of(&handle, BookId::Psalms, 100);
    let run = run_id();
    let (mut total, mut first, mut saves, mut decides) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut pins_ms, mut first_from_job) = (Vec::new(), Vec::new());
    for n in 0..5u32 {
        log.events.lock().unwrap().clear();
        let key = RecordKey::Pins(tc_journal::PinGroup::Lexicon);
        let shown = handle.mint(&key, None).unwrap();
        let before = handle.counters();
        let started = Instant::now();
        handle
            .submit(Command::SetPins {
                shown,
                value: pins_value(run, n),
            })
            .wait()
            .expect("pins");
        pins_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        let left_at = Instant::now();
        let left = handle.leave(Leave::Mode);
        let mut i = 0usize;
        // A save or a decision every 20 ms while the writer is at work, at most 400.
        while !left.is_ready() && i < 400 {
            if i.is_multiple_of(2) {
                saves.push(save_one(
                    &handle,
                    &verses[(i / 2) % verses.len()],
                    format!("Saved during the whole-project job {n} {i} of run {run}"),
                ));
            } else {
                decides.push(decide_one(&handle, &decisions[(i / 2) % decisions.len()]));
            }
            i += 1;
            std::thread::sleep(Duration::from_millis(20));
        }
        left.wait().expect("leave");
        total.push(started.elapsed().as_secs_f64() * 1000.0);
        handle.wait_idle();
        let events = log.events.lock().unwrap().clone();
        if let Some((at, _)) = events.first() {
            first.push(at.duration_since(started).as_secs_f64() * 1000.0);
            first_from_job.push(at.duration_since(left_at).as_secs_f64() * 1000.0);
        }
        println!(
            "  run {n}: {} progress events, saves and decisions during the job {i}, {}",
            events.len(),
            cost("the job", &handle.counters().since(&before))
        );
    }
    stats("whole-project materialize, command to done", total);
    stats("the pins command until its reply", pins_ms);
    stats("first progress report after the command", first);
    stats(
        "first progress report after the writer was handed the job",
        first_from_job,
    );
    stats(
        "verse saves during the job (until the durable reply)",
        saves,
    );
    stats(
        "decisions during the job (until the durable reply)",
        decides,
    );
    handle.close().expect("close");
}

/// "Checkpoint" (PERFORMANCE.md: 500 ms, whole Bible): after one Psalms save, and after the
/// materialization of that save already ran; and the saves that go on during the writer's
/// part of a checkpoint (APP-DESIGN R21).
fn checkpoint_case(dir: &Path) {
    println!("{}", machine_line());
    let store = store(dir);
    let handle = store
        .open(&folder(dir), &OpenOptions::default())
        .expect("open");
    handle.wait_idle();
    let verses = verses_of(&handle, BookId::Psalms, 40);
    let run = run_id();
    // The first checkpoint commits whatever the previous job changed (a pins change over the
    // whole Bible changes 334 files): it is reported apart and not counted below.
    save_one(
        &handle,
        &verses[0],
        format!("Checkpoint first save of run {run}"),
    );
    let started = Instant::now();
    handle
        .checkpoint(CheckpointReason::LeavingTranslate)
        .wait()
        .expect("checkpoint");
    println!(
        "the first checkpoint, which commits what earlier jobs changed: {:.0} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
    handle.wait_idle();
    let (mut dirty, mut settled, mut during) = (Vec::new(), Vec::new(), Vec::new());
    let mut sealed_bytes = 0u64;
    let mut last = tc_store::StoreCounters::default();
    for i in 0..20usize {
        save_one(
            &handle,
            &verses[i % verses.len()],
            format!("Checkpoint save {i} of run {run}"),
        );
        let before = handle.counters();
        let started = Instant::now();
        let report = handle
            .checkpoint(CheckpointReason::LeavingTranslate)
            .wait()
            .expect("checkpoint");
        dirty.push(started.elapsed().as_secs_f64() * 1000.0);
        last = handle.counters().since(&before);
        for pack in &report.sealed {
            sealed_bytes += std::fs::metadata(folder(dir).join(pack)).map_or(0, |m| m.len());
        }
        assert!(report.commit.is_some(), "{report:?}");
        handle.wait_idle();
    }
    println!("  {}", cost("the last checkpoint with Psalms dirty", &last));
    for i in 0..20usize {
        save_one(
            &handle,
            &verses[(i + 7) % verses.len()],
            format!("Materialized checkpoint save {i} of run {run}"),
        );
        handle
            .leave(Leave::Book(BookId::Psalms))
            .wait()
            .expect("leave");
        let started = Instant::now();
        let report = handle
            .checkpoint(CheckpointReason::LeavingTranslate)
            .wait()
            .expect("checkpoint");
        settled.push(started.elapsed().as_secs_f64() * 1000.0);
        assert!(report.commit.is_some(), "{report:?}");
        handle.wait_idle();
    }
    for i in 0..10usize {
        save_one(
            &handle,
            &verses[(i + 3) % verses.len()],
            format!("Saved before a checkpoint {i} of run {run}"),
        );
        let checkpoint = handle.checkpoint(CheckpointReason::LeavingTranslate);
        let mut j = 0usize;
        while !checkpoint.is_ready() && j < 100 {
            during.push(save_one(
                &handle,
                &verses[(i + j) % verses.len()],
                format!("Saved during a checkpoint {i} {j} of run {run}"),
            ));
            j += 1;
            std::thread::sleep(Duration::from_millis(5));
        }
        checkpoint.wait().expect("checkpoint");
        handle.wait_idle();
    }
    stats(
        "checkpoint with Psalms dirty (seal, files, tree, commit)",
        dirty,
    );
    stats("checkpoint after the materialization already ran", settled);
    stats(
        "verse saves during a checkpoint (until the durable reply)",
        during,
    );
    println!(
        "  packs sealed by the first 20 checkpoints: {sealed_bytes} bytes hashed with SHA-256 by the seal"
    );
    handle.close().expect("close");
}

/// "Close a project" (PERFORMANCE.md: 3 s on the reference laptop, whole Bible, Psalms
/// changed): the time of `close` after a Psalms save and a decision.
fn close_case(dir: &Path) {
    println!("{}", machine_line());
    let store = store(dir);
    let run = run_id();
    let mut closes = Vec::new();
    for i in 0..5usize {
        let started = Instant::now();
        let handle = store
            .open(&folder(dir), &OpenOptions::default())
            .expect("open");
        let usable = started.elapsed().as_secs_f64() * 1000.0;
        handle.wait_idle();
        let verses = verses_of(&handle, BookId::Psalms, 10);
        let decisions = decisions_of(&handle, BookId::Psalms, 10);
        save_one(
            &handle,
            &verses[i % verses.len()],
            format!("Closed after {i} of run {run}"),
        );
        decide_one(&handle, &decisions[i % decisions.len()]);
        let before = handle.counters();
        let started = Instant::now();
        handle.close().expect("close");
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        println!("  run {i}: usable {usable:.0} ms, close {ms:.0} ms");
        closes.push(ms);
        let _ = before;
    }
    stats(
        "close (seal, files, tree, commit, fold cache, drafts)",
        closes,
    );
}

/// Three verses of `PSA.usfm` changed by another program, found by the window focus and
/// recorded (§11.1): the time from the file being written to the notice, and what the
/// examination cost.
fn outside_case(dir: &Path) {
    println!("{}", machine_line());
    let store = store(dir);
    let root = folder(dir);
    let handle = store.open(&root, &OpenOptions::default()).expect("open");
    handle.wait_idle();
    handle.leave(Leave::Mode).wait().expect("leave");
    handle.wait_idle();
    let usfm = root.join("ingredients/PSA.usfm");
    let run = run_id();
    let count = |handle: &ProjectHandle| {
        handle
            .notices()
            .iter()
            .filter(|n| {
                matches!(
                    &n.notice,
                    tc_journal::fold::Notice::OutsideEditRecorded { path, .. }
                        if path.as_str() == "ingredients/PSA.usfm"
                )
            })
            .count()
    };
    let mut times = Vec::new();
    let mut last = tc_store::StoreCounters::default();
    for i in 0..20usize {
        let text = std::fs::read_to_string(&usfm).expect("PSA.usfm");
        let edited = edit_three_verses(&text, &format!("edited {i} of run {run}"));
        let seen = count(&handle);
        let before = handle.counters();
        let started = Instant::now();
        std::fs::write(&usfm, edited).expect("write");
        handle.focus().wait().expect("focus");
        while count(&handle) == seen {
            assert!(
                started.elapsed() < Duration::from_secs(60),
                "never recorded"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        times.push(started.elapsed().as_secs_f64() * 1000.0);
        handle.wait_idle();
        last = handle.counters().since(&before);
    }
    stats(
        "outside edit of three Psalms verses, file written to notice",
        times,
    );
    println!("  {}", cost("the last run", &last));
    println!(
        "  outside decodes {}, keys examined {}",
        last.outside_decodes, last.outside_keys_examined
    );
    handle.close().expect("close");
}

/// `text` with the words of Psalm 1:2, 1:3 and 1:4 replaced (a verse marker can follow a
/// poetry marker on its line).
fn edit_three_verses(text: &str, words: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chapter = 0u32;
    for line in text.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("\\c ") {
            chapter = rest.trim().parse().unwrap_or(0);
        }
        let marker = line.find("\\v ").map(|at| (at, &line[at + 3..]));
        let verse = marker.and_then(|(_, rest)| rest.split(' ').next()?.parse::<u32>().ok());
        match (marker, verse) {
            (Some((at, _)), Some(v @ 2..=4)) if chapter == 1 => {
                let end = line.trim_end_matches(['\r', '\n']).len();
                out.push_str(&line[..at]);
                out.push_str(&format!("\\v {v} {words} {v}"));
                out.push_str(&line[end..]);
            }
            _ => out.push_str(line),
        }
    }
    out
}

/// Private bytes of this process (Linux: `Private_Clean + Private_Dirty` of
/// `/proc/self/smaps_rollup`), and the anonymous resident part, in MiB.
fn private_mib() -> (f64, f64) {
    let rollup = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap_or_default();
    let kb = |field: &str| -> f64 {
        rollup
            .lines()
            .find(|l| l.starts_with(field))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    (
        (kb("Private_Clean:") + kb("Private_Dirty:")) / 1024.0,
        status_mib("RssAnon:"),
    )
}

/// "Memory of the project store" (PERFORMANCE.md: 400 MiB steady, 450 MiB peak, private bytes
/// a minute after the last action): open, 200 saves and 200 decisions in Psalms, leave, wait a
/// minute and read; then a whole-project materialize and the close, reading the peak after
/// each.
fn memory_case(dir: &Path) {
    println!("{}", machine_line());
    println!(
        "allocator: MALLOC_ARENA_MAX={}",
        std::env::var("MALLOC_ARENA_MAX").unwrap_or_else(|_| "unset".into())
    );
    let store = store(dir);
    let started = Instant::now();
    let handle = store
        .open(&folder(dir), &OpenOptions::default())
        .expect("open");
    handle.wait_idle();
    println!(
        "after the open ({:.1} s): private {:.0} MiB, anonymous {:.0} MiB, peak {:.0} MiB",
        started.elapsed().as_secs_f64(),
        private_mib().0,
        private_mib().1,
        status_mib("VmHWM:")
    );
    let verses = verses_of(&handle, BookId::Psalms, 200);
    let decisions = decisions_of(&handle, BookId::Psalms, 200);
    let run = run_id();
    for (i, key) in verses.iter().enumerate() {
        save_one(&handle, key, format!("Memory save {i} of run {run}"));
    }
    for key in &decisions {
        decide_one(&handle, key);
    }
    handle
        .leave(Leave::Book(BookId::Psalms))
        .wait()
        .expect("leave");
    handle.wait_idle();
    println!("waiting a minute after the last action");
    std::thread::sleep(Duration::from_secs(60));
    let (private, anon) = private_mib();
    println!(
        "steady, a minute after 200 saves and {} decisions: private {private:.0} MiB, anonymous {anon:.0} MiB, peak {:.0} MiB (budget 400 steady)",
        decisions.len(),
        status_mib("VmHWM:")
    );
    let key = RecordKey::Pins(tc_journal::PinGroup::Lexicon);
    let shown = handle.mint(&key, None).unwrap();
    let started = Instant::now();
    handle
        .submit(Command::SetPins {
            shown,
            value: pins_value(run, 99),
        })
        .wait()
        .expect("pins");
    handle.leave(Leave::Mode).wait().expect("leave");
    handle.wait_idle();
    println!(
        "after a whole-project materialize ({:.1} s): private {:.0} MiB, peak {:.0} MiB (budget 450 peak)",
        started.elapsed().as_secs_f64(),
        private_mib().0,
        status_mib("VmHWM:")
    );
    let started = Instant::now();
    handle.close().expect("close");
    println!(
        "after the close ({:.1} s): private {:.0} MiB, peak {:.0} MiB",
        started.elapsed().as_secs_f64(),
        private_mib().0,
        status_mib("VmHWM:")
    );
}

/// "Repack" (docs/STORAGE.md §9.8) must never hold the close back: with a repack due at every
/// check, a Psalms save and a checkpoint 20 times, the repack runs on its own thread, saves
/// stay fast, and the close that follows ends within the close budget.
fn repack_case(dir: &Path) {
    println!("{}", machine_line());
    let store = store(dir);
    store.hooks().repack_after(0, 0);
    let started = Instant::now();
    let handle = store
        .open(&folder(dir), &OpenOptions::default())
        .expect("open");
    let usable = started.elapsed().as_secs_f64() * 1000.0;
    handle.wait_idle();
    println!(
        "open: usable {usable:.0} ms, with the repack of the open {:.0} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
    let verses = verses_of(&handle, BookId::Psalms, 40);
    let run = run_id();
    let (mut saves, mut checkpoints) = (Vec::new(), Vec::new());
    for i in 0..20usize {
        save_one(
            &handle,
            &verses[i % verses.len()],
            format!("Repack save {i} of run {run}"),
        );
        let started = Instant::now();
        handle
            .checkpoint(CheckpointReason::LeavingTranslate)
            .wait()
            .expect("checkpoint");
        checkpoints.push(started.elapsed().as_secs_f64() * 1000.0);
        // Saves while the repack runs.
        for j in 0..10usize {
            saves.push(save_one(
                &handle,
                &verses[(i + j) % verses.len()],
                format!("Repack save {i} {j} of run {run}"),
            ));
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    stats("checkpoint with a repack due", checkpoints);
    stats("verse saves beside a repack", saves);
    let c = handle.counters();
    println!(
        "  repacks {}, objects packed {}, loose files removed {}",
        c.git_repacks, c.git_objects_packed, c.git_loose_removed
    );
    // The close with a checkpoint that makes a repack due and one maybe still running.
    save_one(
        &handle,
        &verses[0],
        format!("Repack save last of run {run}"),
    );
    let started = Instant::now();
    handle.close().expect("close");
    println!(
        "close right after a checkpoint with a repack due: {:.0} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
}
