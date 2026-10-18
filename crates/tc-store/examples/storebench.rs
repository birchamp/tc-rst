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

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tc_corpus::{Corpus, Spec};
use tc_journal::command::{Command, DecisionChange, DecisionEdit};
use tc_journal::format::FormatDoc;
use tc_journal::keys::{CheckTarget, ToolId};
use tc_journal::record::CheckValue;
use tc_journal::{ChapterNo, Record, RecordKey, VerseNo};
use tc_store::layout::{Folder, GITATTRIBUTES_BYTES, GITIGNORE_BYTES};
use tc_store::{
    OpenOptions, Progress, ProgressSink, StdFs, StdPlatform, Store, StoreConfig, SystemClock,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = PathBuf::from(args.get(1).expect("DIR"));
    match args.first().map(String::as_str) {
        Some("setup") => setup(&dir),
        Some("open") => open_once(&dir, args.get(2).map_or("warm", String::as_str)),
        Some("run") => run(&dir, args.get(2).and_then(|n| n.parse().ok()).unwrap_or(7)),
        Some("saves") => saves(&dir),
        _ => {
            eprintln!("storebench setup DIR | open DIR CASE | run DIR [runs] | saves DIR");
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
    })
    .expect("store");
    store.wait_cleanup();
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
