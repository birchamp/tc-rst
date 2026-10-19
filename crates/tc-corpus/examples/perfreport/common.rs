//! What every case shares: the folders of a report directory, how a store is started, how a
//! project is set up, the helpers that save a verse or a decision and time it, and the
//! protocol with the child processes (one line `ROW <json>` per measured row).

use crate::machine;
use crate::table::Measured;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tc_journal::command::{AlignmentEdit, Command, DecisionChange, DecisionEdit};
use tc_journal::keys::{CheckTarget, ToolId};
use tc_journal::record::CheckValue;
use tc_journal::{BookId, ChapterNo, Record, RecordKey, VerseNo};
use tc_store::{
    Clock, OpenOptions, ProjectHandle, StdFs, StdPlatform, Store, StoreConfig, SystemClock,
};

/// The reading threads of a store: the reference laptop has 4 cores.
pub const STORE_THREADS: usize = 4;

/// The project the cases open: `bible`, or the one `PERFREPORT_PROJECT` names (the growth
/// figure compares two projects of different history).
pub fn project_folder(dir: &Path) -> PathBuf {
    let name = std::env::var("PERFREPORT_PROJECT").unwrap_or_else(|_| "bible".into());
    dir.join("projects").join(name)
}

/// The folder of the project called `name`.
pub fn project_folder_named(dir: &Path, name: &str) -> PathBuf {
    dir.join("projects").join(name)
}

/// A store over `dir/app` and `dir/projects`, the file watcher on with its real debounce.
pub fn store_with(dir: &Path, clock: Arc<dyn Clock>) -> Store {
    let store = Store::start(StoreConfig {
        app_data: dir.join("app"),
        projects: dir.join("projects"),
        vfs: Arc::new(StdFs),
        platform: Arc::new(StdPlatform),
        clock,
        threads: STORE_THREADS,
        watch_debounce: Duration::from_secs(1),
    })
    .expect("a store starts");
    store.wait_cleanup();
    store
}

pub fn store(dir: &Path) -> Store {
    store_with(dir, Arc::new(SystemClock))
}

pub fn open(store: &Store, dir: &Path) -> ProjectHandle {
    let handle = store
        .open(&project_folder(dir), &OpenOptions::default())
        .expect("the project opens");
    handle.wait_idle();
    handle
}

pub fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

/// A number no earlier run used, so every save writes (an unchanged value writes nothing).
pub fn run_id() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

/// The first `n` verse keys of a book, spread over its first chapters.
pub fn verses_of(handle: &ProjectHandle, book: BookId, n: usize) -> Vec<RecordKey> {
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

/// Verse keys spread over the whole of a book (every chapter contributes), for rows that
/// must not hit the same chapter again and again.
pub fn verses_across(handle: &ProjectHandle, book: BookId, n: usize) -> Vec<RecordKey> {
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    let chapters = fold.chapters_of(book);
    let mut out = Vec::new();
    let mut round = 0usize;
    while out.len() < n {
        let before = out.len();
        for chapter in &chapters {
            if let Some(key) = fold.chapter_keys(book, *chapter).verses.get(round) {
                out.push(key.clone());
                if out.len() >= n {
                    break;
                }
            }
        }
        if out.len() == before {
            break;
        }
        round += 1;
    }
    out
}

/// A save of one verse; its time until the durable reply, in ms.
pub fn save_one(handle: &ProjectHandle, key: &RecordKey, words: String) -> f64 {
    let shown = handle.mint(key, None).expect("a token");
    let started = Instant::now();
    let reply = handle.submit(Command::SaveVerse { shown, words }).wait();
    let took = ms(started);
    let reply = reply.expect("the save is accepted");
    assert!(!reply.changes.is_empty(), "every save writes");
    took
}

/// The decisions of a tool in a book that a bookmark can be toggled on.
pub fn decisions_of(handle: &ProjectHandle, book: BookId, n: usize) -> Vec<RecordKey> {
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    fold.decisions_of(ToolId::TranslationWords, book)
        .into_iter()
        .filter(|key| decision_target(key).is_some())
        .take(n)
        .collect()
}

pub fn decision_target(key: &RecordKey) -> Option<RecordKey> {
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
        ChapterNo::new(chapter.number()?)?,
        VerseNo::new(verse.number()?)?,
    ))
}

/// A bookmark toggled on a decided check; its time until the durable reply, in ms.
pub fn decide_one(handle: &ProjectHandle, key: &RecordKey) -> f64 {
    let verse_key = decision_target(key).expect("a verse check");
    let Some(Record::Check {
        value: CheckValue::Decision(d),
        ..
    }) = handle.snapshot().fold().projected(key)
    else {
        panic!("a decided check");
    };
    let shown = handle.mint(key, None).expect("a token");
    let verse_shown = handle.mint(&verse_key, None).expect("a token");
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
    let took = ms(started);
    reply.expect("the decision is accepted");
    took
}

/// An alignment toggled between done and not done; its time until the durable reply, in ms.
pub fn align_one(handle: &ProjectHandle, key: &RecordKey) -> f64 {
    let Some(Record::Alignment {
        book,
        chapter,
        verse,
        value,
    }) = handle.snapshot().fold().projected(key)
    else {
        panic!("an alignment");
    };
    let edit = AlignmentEdit {
        alignments: value.alignments.clone(),
        word_bank: value.word_bank.clone(),
        done: !value.done,
    };
    let shown = handle.mint(key, None).expect("a token");
    let verse_key = RecordKey::Verse(book, chapter, verse);
    let verse_shown = handle.mint(&verse_key, None).expect("a token");
    let started = Instant::now();
    let reply = handle
        .submit(Command::SaveAlignment {
            shown,
            verse_shown,
            value: edit,
        })
        .wait();
    let took = ms(started);
    reply.expect("the alignment is accepted");
    took
}

/// The alignment keys of a book, spread over its chapters.
pub fn alignments_of(handle: &ProjectHandle, book: BookId, n: usize) -> Vec<RecordKey> {
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    let mut out = Vec::new();
    for chapter in fold.chapters_of(book) {
        for key in fold.chapter_keys(book, chapter).alignments {
            out.push(key);
            if out.len() >= n {
                return out;
            }
        }
    }
    out
}

/// What one action did, from the store's counters.
pub fn cost(delta: &tc_store::StoreCounters) -> String {
    format!(
        "opened {}, created {}, projected {}, written {}, read {} B, hashed {} B, fsyncs {}, \
         yielded {} ms",
        delta.files_opened,
        delta.files_created,
        delta.projected,
        delta.derived_written,
        delta.bytes_read,
        delta.bytes_hashed,
        delta.fsyncs,
        delta.writer_yield_ns / 1_000_000
    )
}

/// A row line a child prints and the parent collects.
pub fn emit(rows: &[Measured]) {
    for m in rows {
        let json = serde_json::json!({
            "id": m.id,
            "samples": m.samples,
            "note": m.note,
            "fsyncs": m.fsyncs,
        });
        println!("ROW\t{json}");
    }
}

/// Parses the `ROW` lines of a child's output.
pub fn collect(output: &str) -> Vec<Measured> {
    output
        .lines()
        .filter_map(|line| line.strip_prefix("ROW\t"))
        .filter_map(|json| serde_json::from_str::<serde_json::Value>(json).ok())
        .filter_map(|v| {
            Some(Measured {
                id: v.get("id")?.as_str()?.to_owned(),
                samples: v
                    .get("samples")?
                    .as_array()?
                    .iter()
                    .filter_map(serde_json::Value::as_f64)
                    .collect(),
                note: v.get("note")?.as_str()?.to_owned(),
                fsyncs: v.get("fsyncs").and_then(serde_json::Value::as_f64),
            })
        })
        .collect()
}

/// Adds `more` to `into`, row by row: samples are appended, the last note stays.
pub fn merge(into: &mut Vec<Measured>, more: Vec<Measured>) {
    for m in more {
        match into.iter_mut().find(|e| e.id == m.id) {
            Some(e) => {
                e.samples.extend(m.samples);
                e.note = m.note;
                if m.fsyncs.is_some() {
                    e.fsyncs = m.fsyncs;
                }
            }
            None => into.push(m),
        }
    }
}

/// Runs this binary again as `child <case> <dir> <args...>` and returns its rows, passing
/// everything else the child prints on to this process's output.
pub fn run_child(case: &str, dir: &Path, args: &[&str]) -> Result<Vec<Measured>, String> {
    run_child_env(case, dir, args, &[])
}

/// [`run_child`] with environment variables set for the child only.
pub fn run_child_env(
    case: &str,
    dir: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Result<Vec<Measured>, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let output = std::process::Command::new(exe)
        .arg("child")
        .arg(case)
        .arg(dir)
        .args(args)
        .envs(envs.iter().copied())
        .output()
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines().filter(|l| !l.starts_with("ROW\t")) {
        println!("  [{case}] {line}");
    }
    let err = String::from_utf8_lossy(&output.stderr);
    for line in err.lines().filter(|l| !l.trim().is_empty()) {
        eprintln!("  [{case}] {line}");
    }
    if !output.status.success() {
        return Err(format!("{case}: the child failed with {}", output.status));
    }
    Ok(collect(&text))
}

/// Tells the kernel to drop the cached pages of every file under `root` (Linux:
/// `posix_fadvise(DONTNEED)` after a `sync`, which needs no privilege). Directory entries and
/// inodes stay cached, so "cold" is approximate. Returns the files advised.
#[cfg(unix)]
pub fn evict_page_cache(root: &Path) -> usize {
    use std::os::fd::AsFd;
    rustix::fs::sync();
    let mut stack = vec![root.to_path_buf()];
    let mut n = 0;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file()
                && let Ok(file) = std::fs::File::open(&path)
            {
                let _ = rustix::fs::fadvise(file.as_fd(), 0, None, rustix::fs::Advice::DontNeed);
                n += 1;
            }
        }
    }
    n
}

#[cfg(not(unix))]
pub fn evict_page_cache(_root: &Path) -> usize {
    0
}

/// Loose objects of the project's repository: their number and bytes on disk.
pub fn loose_objects(root: &Path) -> (u64, u64) {
    let objects = root.join(".git/objects");
    let (mut n, mut bytes) = (0, 0);
    let Ok(dirs) = std::fs::read_dir(&objects) else {
        return (0, 0);
    };
    for dir in dirs.flatten() {
        let name = dir.file_name();
        let name = name.to_string_lossy();
        if name.len() != 2 {
            continue;
        }
        if let Ok(files) = std::fs::read_dir(dir.path()) {
            for f in files.flatten() {
                n += 1;
                bytes += f.metadata().map_or(0, |m| m.len());
            }
        }
    }
    (n, bytes)
}

pub fn dir_size(root: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push(entry.path()),
                Ok(t) if t.is_file() => total += entry.metadata().map_or(0, |m| m.len()),
                _ => {}
            }
        }
    }
    total
}

pub fn header(case: &str) {
    println!("{case}: {}", machine::machine_line());
}
