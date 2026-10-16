//! Release benchmarks of the fold (docs/PERFORMANCE.md rule 5) on a saved corpus.
//!
//! `cargo run --release -p tc-corpus --example foldbench -- save bible DIR` generates a
//! corpus (`psalms`, `scaled <factor>`, `bible`, or `bible-share <share>` for less history)
//! and saves its packs and pending files under DIR; `... -- run DIR [threads]` reads them
//! back the way an open does (verify and prepare on `threads` threads, keeping only where
//! each value lies, then fold) and times the actions the fold owns: the full fold, one
//! incremental step after a verse save, a decision, an alignment and a resolution, the
//! conflict list from scratch, and the Psalms projection, whose values are read from the
//! files on demand. Times are medians and p95 over repeated runs. `FOLDBENCH_RUNS` sets the
//! runs of the open, `FOLDBENCH_FOLD_ONLY` stops after the open and its parts, and
//! `FOLDBENCH_STREAM_ONLY` after the open alone (for a heap profile of it).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tc_corpus::{Corpus, Spec};
use tc_journal::fold::{DerivedFile, FoldInput, FoldState, ProjectKind, ValueAt, ValueSource};
use tc_journal::record::{CheckValue, VerseValue};
use tc_journal::{
    BatchId, BatchRef, BookId, Cause, Change, ChangeBody, DeviceId, DeviceSeq, ProjectId, Record,
    RecordKey, WallTime,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("save") => save(&args[1..]),
        Some("run") => run(&args[1..]),
        _ => {
            eprintln!(
                "foldbench save <psalms|scaled N|bible|bible-share S> DIR | run DIR [threads]"
            );
            std::process::exit(2);
        }
    }
}

fn save(args: &[String]) {
    let (spec, dir) = match args.first().map(String::as_str) {
        Some("scaled") => (Spec::scaled(1, args[1].parse().expect("factor")), &args[2]),
        Some("bible") => (Spec::whole_bible(1), &args[1]),
        Some("bible-share") => {
            let share: f64 = args[1].parse().expect("share");
            (Spec::whole_bible(1).with_history(750, share), &args[2])
        }
        _ => (Spec::psalms(1), &args[1]),
    };
    let started = Instant::now();
    let corpus = Corpus::generate(&spec).expect("corpus");
    eprintln!(
        "generated {} changes in {:?}",
        corpus.journal.stats.changes,
        started.elapsed()
    );
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(dir.join("packs")).expect("dir");
    std::fs::create_dir_all(dir.join("pending")).expect("dir");
    std::fs::write(dir.join("project"), corpus.journal.project.to_string()).expect("write");
    for pack in &corpus.journal.packs {
        let path = pack
            .path
            .strip_prefix("ingredients/tc/changes/")
            .expect("a pack path");
        let name = path.replace('/', "__");
        std::fs::write(dir.join("packs").join(name), &pack.bytes).expect("write");
    }
    for (i, file) in corpus.journal.pending.iter().enumerate() {
        std::fs::write(
            dir.join("pending").join(format!("{i:04}.batch")),
            &file.bytes,
        )
        .expect("write");
    }
}

fn cpu_ns() -> u64 {
    std::fs::read_to_string("/proc/thread-self/schedstat")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse().ok()))
        .unwrap_or(0)
}

/// CPU time of the whole process, user and system, in ms (10 ms resolution).
fn process_cpu_ms() -> u64 {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let rest = stat.rsplit(')').next().unwrap_or("");
    let field = |n: usize| -> u64 {
        rest.split_whitespace()
            .nth(n)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };
    // utime and stime, in clock ticks of 10 ms.
    (field(11) + field(12)) * 10
}

fn minflt() -> u64 {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let rest = stat.rsplit(')').next().unwrap_or("");
    rest.split_whitespace()
        .nth(7)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

/// A line of `/proc/self/status` in MiB: `VmHWM` is the peak resident memory, `VmRSS` the
/// current.
fn status_mib(field: &str) -> f64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status
        .lines()
        .find(|l| l.starts_with(field))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|kb| kb.parse::<f64>().ok())
        .map_or(0.0, |kb| kb / 1024.0)
}

fn peak_rss_mib() -> f64 {
    status_mib("VmHWM:")
}

fn rss_mib() -> f64 {
    status_mib("VmRSS:")
}

/// The packs and pending files of a saved corpus, read again by span: what `tc-store` does
/// with the files of an open project.
#[derive(Debug)]
struct Files(Vec<File>);

impl ValueSource for Files {
    fn read(&self, at: ValueAt) -> Option<Vec<u8>> {
        let file = self.0.get(at.file as usize)?;
        let mut bytes = vec![0; at.len as usize];
        read_at(file, &mut bytes, at.offset).ok()?;
        Some(bytes)
    }
}

#[cfg(unix)]
fn read_at(file: &File, bytes: &mut [u8], offset: u64) -> std::io::Result<()> {
    std::os::unix::fs::FileExt::read_exact_at(file, bytes, offset)
}

#[cfg(windows)]
fn read_at(file: &File, mut bytes: &mut [u8], mut offset: u64) -> std::io::Result<()> {
    while !bytes.is_empty() {
        let n = std::os::windows::fs::FileExt::seek_read(file, bytes, offset)?;
        if n == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        bytes = &mut bytes[n..];
        offset += n as u64;
    }
    Ok(())
}

fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    out.sort();
    out
}

/// Reads, verifies and prepares every line on `threads` threads, as an open does, keeping
/// only where each value lies, and hands each file's changes over as soon as they are
/// prepared, so the fold indexes them while the threads read the rest.
fn load_streamed(
    dir: &Path,
    threads: usize,
) -> (
    std::sync::mpsc::Receiver<Vec<FoldInput>>,
    Arc<dyn ValueSource>,
) {
    let project = ProjectId::parse(
        std::fs::read_to_string(dir.join("project"))
            .expect("project")
            .trim(),
    )
    .expect("project id");
    let mut work: Vec<(bool, PathBuf)> = files(&dir.join("packs"))
        .into_iter()
        .map(|p| (true, p))
        .collect();
    work.extend(files(&dir.join("pending")).into_iter().map(|p| (false, p)));
    let opened: Vec<File> = work
        .iter()
        .map(|(_, path)| File::open(path).expect("open"))
        .collect();
    let mut queue: Vec<(u32, bool, PathBuf)> = work
        .into_iter()
        .enumerate()
        .map(|(n, (is_pack, path))| (n as u32, is_pack, path))
        .collect();
    // Largest files first, each to the next free thread, as a work-stealing pool would.
    queue.sort_by_key(|(_, _, path)| {
        std::cmp::Reverse(std::fs::metadata(path).map_or(0, |m| m.len()))
    });
    let queue = Arc::new(queue);
    let next = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    // A few files in flight per thread at most: memory holds what is being folded, not
    // the whole project's prepared changes.
    let (send, receive) = std::sync::mpsc::sync_channel::<Vec<FoldInput>>(threads * 2);
    for _ in 0..threads {
        let (queue, next, send) = (queue.clone(), next.clone(), send.clone());
        std::thread::spawn(move || {
            while let Some((file, is_pack, path)) =
                queue.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
            {
                let bytes = std::fs::read(path).expect("read");
                let (pack, entries, spans) = if *is_pack {
                    let name = pack_path(path);
                    let read = tc_journal::pack::read_pack(&name, &bytes, project);
                    assert!(read.is_valid(), "{name}: {:?}", read.status);
                    (Some(read.id), read.entries, read.spans)
                } else {
                    let read = tc_journal::pending::read_pending(&bytes, project);
                    (None, read.entries, read.spans)
                };
                drop(bytes);
                let inputs: Vec<FoldInput> = entries
                    .into_iter()
                    .zip(spans)
                    .map(|(entry, span)| {
                        FoldInput::new(entry, pack).prepare_at(ValueAt::new(*file, span))
                    })
                    .collect();
                if send.send(inputs).is_err() {
                    return;
                }
            }
        });
    }
    (receive, Arc::new(Files(opened)))
}

/// [`load_streamed`] collected: every prepared change, for a fold timed on its own.
fn load(dir: &Path, threads: usize) -> (Vec<FoldInput>, Arc<dyn ValueSource>) {
    let (receive, source) = load_streamed(dir, threads);
    (receive.into_iter().flatten().collect(), source)
}

/// The repository path of a saved pack, which names its sealer and month.
fn pack_path(path: &Path) -> String {
    let name = path.file_name().unwrap().to_string_lossy();
    let (sealer, rest) = name.split_once("__").expect("sealer__month__name");
    let (month, file) = rest.split_once("__").expect("month__name");
    format!("ingredients/tc/changes/{sealer}/{month}/{file}")
}

struct Stats {
    name: &'static str,
    times: Vec<Duration>,
}

impl Stats {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            times: Vec::new(),
        }
    }

    fn report(&mut self) {
        self.times.sort();
        let n = self.times.len();
        let median = self.times[n / 2];
        let p95 = self.times[(n * 95 / 100).min(n - 1)];
        println!(
            "| {} | {} | {:.3} ms | {:.3} ms |",
            self.name,
            n,
            median.as_secs_f64() * 1e3,
            p95.as_secs_f64() * 1e3
        );
    }
}

/// A device that writes the benchmark's own changes.
struct Writer {
    device: DeviceId,
    seq: u64,
    at: i64,
}

impl Writer {
    fn write(
        &mut self,
        record: Record,
        parents: Vec<tc_journal::ChangeId>,
        cause: Cause,
    ) -> Change {
        self.seq += 1;
        self.at += 1_000;
        let mut id = [0u8; 16];
        id[..8].copy_from_slice(&self.seq.to_be_bytes());
        id[15] = 0xbe;
        let mut parents = parents;
        parents.sort();
        Change::new(ChangeBody {
            v: 0,
            record,
            parents,
            device: self.device,
            seq: DeviceSeq(self.seq),
            at: WallTime(self.at),
            batch: BatchRef::new(BatchId::from_bytes(id), 1).expect("batch"),
            cause,
        })
        .expect("change")
    }
}

fn run(args: &[String]) {
    let dir = PathBuf::from(&args[0]);
    let threads: usize = args.get(1).and_then(|t| t.parse().ok()).unwrap_or(4);
    // The schema validator is compiled once per process; not part of any action.
    let _ = tc_burrito::check_field(
        tc_burrito::ProjectKind::BibleText,
        "identification.name",
        &serde_json::json!({"en": "x"}),
    );
    let mut open = Stats::new(
        "Cold open: read, verify, prepare and fold, the fold indexing as files are read",
    );
    let mut fold_only = Stats::new("Full fold of the prepared changes");
    let mut after_read =
        Stats::new("Cold open: the fold's work after the last file is read (stages F2 to F10)");
    let mut load_only = Stats::new("Read, verify and prepare");
    let mut state = None;
    let mut changes = 0;
    let runs: usize = std::env::var("FOLDBENCH_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    // The cold open: the fold indexes each file's changes as the reading threads hand
    // them over, then runs its stages once the last arrives.
    for _ in 0..runs {
        drop(state.take());
        let started = Instant::now();
        let cpu = process_cpu_ms();
        let (receive, source) = load_streamed(&dir, threads);
        let last_read = std::cell::Cell::new(None);
        let inputs = receive.into_iter().flatten().chain(std::iter::from_fn(|| {
            last_read.set(Some(Instant::now()));
            None
        }));
        let folded = FoldState::fold_from(ProjectKind::Bible, Some(source), inputs);
        open.times.push(started.elapsed());
        let tail = last_read.get().map(|t| t.elapsed()).unwrap_or_default();
        after_read.times.push(tail);
        eprintln!(
            "cold open {:?}, the fold after the last file {tail:?}, cpu over all threads {} ms",
            started.elapsed(),
            process_cpu_ms() - cpu
        );
        changes = folded.len();
        state = Some(folded);
    }
    let open_peak = peak_rss_mib();
    if std::env::var("FOLDBENCH_STREAM_ONLY").is_ok() {
        open.report();
        after_read.report();
        println!(
            "peak RSS during the streamed cold opens {open_peak:.0} MiB, RSS with the fold \
             open {:.0} MiB",
            rss_mib()
        );
        return;
    }
    // The parts on their own: reading every file, then folding what was read.
    for _ in 0..runs {
        drop(state.take());
        let started = Instant::now();
        let load_cpu = process_cpu_ms();
        let (inputs, source) = load(&dir, threads);
        let loaded = started.elapsed();
        eprintln!(
            "load {loaded:?}, cpu over all threads {} ms",
            process_cpu_ms() - load_cpu
        );
        let t = Instant::now();
        let f0 = minflt();
        let c0 = cpu_ns();
        let folded = FoldState::fold_from(ProjectKind::Bible, Some(source), inputs);
        fold_only.times.push(t.elapsed());
        eprintln!(
            "fold {:?}, cpu {} ms, minor faults {}",
            t.elapsed(),
            (cpu_ns() - c0) / 1_000_000,
            minflt() - f0
        );
        load_only.times.push(loaded);
        state = Some(folded);
    }
    println!("peak RSS during the streamed cold opens {open_peak:.0} MiB");
    let mut state = state.expect("folded");
    if std::env::var("FOLDBENCH_FOLD_ONLY").is_ok() {
        load_only.report();
        fold_only.report();
        open.report();
        after_read.report();
        println!("peak RSS {:.0} MiB", peak_rss_mib());
        return;
    }
    println!(
        "corpus: {} lines, {} records, {} conflicts, {} anomalies; peak RSS {:.0} MiB, \
         RSS with the fold open {:.0} MiB",
        changes,
        state.record_keys().len(),
        state.conflict_count(),
        state.anomalies().count(),
        peak_rss_mib(),
        rss_mib()
    );
    println!("| Action | Runs | Median | p95 |");
    println!("|---|---:|---:|---:|");
    load_only.report();
    // What SHA-256 alone costs: one pass over every file's bytes on the same threads (P2
    // hashes each file once and P4 each line once, so a load hashes every byte twice).
    let mut sha = Stats::new("SHA-256 of every file once (one of the load's two passes)");
    let all: Vec<PathBuf> = files(&dir.join("packs"))
        .into_iter()
        .chain(files(&dir.join("pending")))
        .collect();
    let bytes: Vec<Vec<u8>> = all
        .iter()
        .map(|p| std::fs::read(p).expect("read"))
        .collect();
    for _ in 0..3 {
        let t = Instant::now();
        std::thread::scope(|scope| {
            for t in 0..threads {
                let bytes = &bytes;
                scope.spawn(move || {
                    for b in bytes.iter().skip(t).step_by(threads) {
                        std::hint::black_box(tc_journal::PackId::of(b));
                    }
                });
            }
        });
        sha.times.push(t.elapsed());
    }
    drop(bytes);
    sha.report();
    fold_only.report();
    open.report();
    after_read.report();

    let keys = state.record_keys();
    let psalms_verses: Vec<RecordKey> = keys
        .iter()
        .filter(|k| matches!(k, RecordKey::Verse(BookId::Psalms, ..)))
        .cloned()
        .collect();
    let psalms_checks: Vec<RecordKey> = keys
        .iter()
        .filter(|k| matches!(k, RecordKey::Check(c) if c.book() == Some(BookId::Psalms)))
        .cloned()
        .collect();
    let psalms_alignments: Vec<RecordKey> = keys
        .iter()
        .filter(|k| matches!(k, RecordKey::Alignment(BookId::Psalms, ..)))
        .cloned()
        .collect();
    let mut writer = Writer {
        device: DeviceId::local([0xbe; 15]),
        seq: 0,
        at: 1_900_000_000_000,
    };
    let mut verse = Stats::new("Incremental step: save one Psalms verse");
    let mut decision = Stats::new("Incremental step: record one Psalms decision");
    let mut alignment = Stats::new("Incremental step: save one Psalms alignment");
    let mut counts = Stats::new("Conflict counts after a save (incremental)");
    for i in 0..200 {
        let key = &psalms_verses[(i * 7919) % psalms_verses.len()];
        if let Some(Record::Verse {
            book,
            chapter,
            verse: v,
            value:
                VerseValue::Text {
                    marker,
                    layout,
                    last,
                    from,
                    ..
                },
        }) = state.projected(key)
        {
            let change = writer.write(
                Record::Verse {
                    book,
                    chapter,
                    verse: v,
                    value: VerseValue::Text {
                        marker,
                        words: format!("bench words {i}"),
                        layout,
                        last,
                        from,
                    },
                },
                state.shown_heads(key, None),
                Cause::Edit,
            );
            let t = Instant::now();
            state.add([FoldInput::from(change)]);
            verse.times.push(t.elapsed());
            let t = Instant::now();
            let _ = state.conflict_counts().len();
            counts.times.push(t.elapsed());
        }
        let key = &psalms_checks[(i * 104_729) % psalms_checks.len()];
        if let Some(Record::Check {
            key: check,
            value: CheckValue::Decision(mut value),
        }) = state.projected(key)
        {
            value.bookmarked = !value.bookmarked;
            let change = writer.write(
                Record::Check {
                    key: check,
                    value: CheckValue::Decision(value),
                },
                state.shown_heads(key, None),
                Cause::Edit,
            );
            let t = Instant::now();
            state.add([FoldInput::from(change)]);
            decision.times.push(t.elapsed());
        }
        if !psalms_alignments.is_empty() {
            let key = &psalms_alignments[(i * 31) % psalms_alignments.len()];
            if let Some(Record::Alignment {
                book,
                chapter,
                verse: v,
                mut value,
            }) = state.projected(key)
            {
                value.done = !value.done;
                let change = writer.write(
                    Record::Alignment {
                        book,
                        chapter,
                        verse: v,
                        value,
                    },
                    state.shown_heads(key, None),
                    Cause::Edit,
                );
                let t = Instant::now();
                state.add([FoldInput::from(change)]);
                alignment.times.push(t.elapsed());
            }
        }
    }
    verse.report();
    decision.report();
    alignment.report();
    counts.report();

    // The same steps while a snapshot is alive (a sync, an export or an upgrade plan holds
    // one, §7.8): the first write to each shared part of the state copies it.
    let mut verse = Stats::new("Incremental step with a live snapshot: save one Psalms verse");
    let mut decision =
        Stats::new("Incremental step with a live snapshot: record one Psalms decision");
    for i in 0..200 {
        let key = &psalms_verses[(i * 6151) % psalms_verses.len()];
        if let Some(Record::Verse {
            book,
            chapter,
            verse: v,
            value:
                VerseValue::Text {
                    marker,
                    layout,
                    last,
                    from,
                    ..
                },
        }) = state.projected(key)
        {
            let change = writer.write(
                Record::Verse {
                    book,
                    chapter,
                    verse: v,
                    value: VerseValue::Text {
                        marker,
                        words: format!("snapshot words {i}"),
                        layout,
                        last,
                        from,
                    },
                },
                state.shown_heads(key, None),
                Cause::Edit,
            );
            let snapshot = state.clone();
            let t = Instant::now();
            state.add([FoldInput::from(change)]);
            verse.times.push(t.elapsed());
            drop(snapshot);
        }
        let key = &psalms_checks[(i * 7_793) % psalms_checks.len()];
        if let Some(Record::Check {
            key: check,
            value: CheckValue::Decision(mut value),
        }) = state.projected(key)
        {
            value.bookmarked = !value.bookmarked;
            let change = writer.write(
                Record::Check {
                    key: check,
                    value: CheckValue::Decision(value),
                },
                state.shown_heads(key, None),
                Cause::Edit,
            );
            let snapshot = state.clone();
            let t = Instant::now();
            state.add([FoldInput::from(change)]);
            decision.times.push(t.elapsed());
            drop(snapshot);
        }
    }
    verse.report();
    decision.report();

    // A resolution: make a conflict on a Psalms verse, then resolve it.
    let mut resolve = Stats::new("Incremental step: resolve one Psalms conflict");
    let mut other = Writer {
        device: DeviceId::local([0xbf; 15]),
        seq: 0,
        at: 1_900_000_000_500,
    };
    for i in 0..100 {
        let key = &psalms_verses[(i * 613 + 5) % psalms_verses.len()];
        let Some(Record::Verse {
            book,
            chapter,
            verse: v,
            value:
                VerseValue::Text {
                    marker,
                    layout,
                    last,
                    from,
                    ..
                },
        }) = state.projected(key)
        else {
            continue;
        };
        let heads = state.shown_heads(key, None);
        let make = |words: String| Record::Verse {
            book,
            chapter,
            verse: v,
            value: VerseValue::Text {
                marker: marker.clone(),
                words,
                layout: layout.clone(),
                last,
                from,
            },
        };
        let a = writer.write(make(format!("one side {i}")), heads.clone(), Cause::Edit);
        let b = other.write(make(format!("other side {i}")), heads, Cause::Edit);
        state.add([FoldInput::from(a), FoldInput::from(b.clone())]);
        let Some(conflict) = state
            .conflicts()
            .find(|c| c.key == *key || c.also_keys.contains(key))
            .cloned()
        else {
            continue;
        };
        let Ok(planned) = state.resolution(
            &conflict.id,
            &tc_journal::command::Choice::Candidate { head: b.id() },
        ) else {
            continue;
        };
        let resolved: Vec<FoldInput> = planned
            .into_iter()
            .map(|p| FoldInput::from(writer.write(p.record, p.parents, Cause::Resolve)))
            .collect();
        let t = Instant::now();
        state.add(resolved);
        resolve.times.push(t.elapsed());
    }
    resolve.report();

    let mut scratch =
        Stats::new("Conflict list and counts from scratch (every record's conflict built again)");
    for _ in 0..20 {
        let t = Instant::now();
        let (list, counts) = state.conflicts_rebuilt();
        scratch.times.push(t.elapsed());
        assert_eq!(list.len(), state.conflict_count());
        assert_eq!(&counts, state.conflict_counts());
    }
    scratch.report();

    let projection_runs: usize = std::env::var("FOLDBENCH_PROJECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let mut projection =
        Stats::new("Psalms projection: every file of the book (values read from disk)");
    let mut usfm = Stats::new("Psalms USFM alone");
    let mut chapter = Stats::new("One Psalms chapter's USFM");
    let mut files = 0;
    let mut bytes = 0;
    let mut cpu = Vec::new();
    for run in 0..projection_runs {
        let t = Instant::now();
        let c = process_cpu_ms();
        let rendered = state.render_book(BookId::Psalms);
        projection.times.push(t.elapsed());
        cpu.push(process_cpu_ms() - c);
        files = rendered.len();
        bytes = rendered.iter().map(|(_, b)| b.len()).sum::<usize>();
        let t = Instant::now();
        std::hint::black_box(state.render(&DerivedFile::Usfm(BookId::Psalms)));
        usfm.times.push(t.elapsed());
        let t = Instant::now();
        std::hint::black_box(state.chapter_usfm(BookId::Psalms, (run % 150 + 1) as u16));
        chapter.times.push(t.elapsed());
    }
    projection.report();
    usfm.report();
    chapter.report();
    cpu.sort_unstable();
    println!(
        "Psalms: {files} files, {bytes} bytes; CPU time of a projection, all threads: median {} ms",
        cpu[cpu.len() / 2]
    );
    // To compare the bytes of two builds: FOLDBENCH_DUMP=DIR writes every file of Psalms.
    if let Ok(dump) = std::env::var("FOLDBENCH_DUMP") {
        let dump = PathBuf::from(dump);
        for (file, bytes) in state.render_book(BookId::Psalms) {
            let path = dump.join(file.path());
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("dir");
            std::fs::write(path, bytes).expect("write");
        }
    }
    let mut snapshot = Stats::new("Snapshot (clone of the fold)");
    for _ in 0..20 {
        let t = Instant::now();
        let s = state.clone();
        snapshot.times.push(t.elapsed());
        drop(s);
    }
    snapshot.report();
    println!("peak RSS {:.0} MiB", peak_rss_mib());
}
