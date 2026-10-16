//! Release benchmarks of the fold (docs/PERFORMANCE.md rule 5) on a saved corpus.
//!
//! `cargo run --release -p tc-corpus --example foldbench -- save bible DIR` generates a
//! corpus (`psalms`, `scaled <factor>`, `bible`, or `bible-share <share>` for less history)
//! and saves its packs and pending files under DIR; `... -- run DIR [threads]` reads them
//! back the way an open does and times the actions the fold owns. The cold open is timed
//! with the fold cache (`DIR/index.bin`, `.tc/index.bin` in a project; the first run writes
//! it): each file is read and hashed, and its prepared changes taken from the cache; and
//! without it: every line verified, parsed and prepared on `threads` threads, keeping only
//! where each value lies. Then the full fold, one incremental step after a verse save, a
//! decision, an alignment and a resolution, the conflict list from scratch, and the Psalms
//! projection, whose values are read from the files on demand. Times are medians and p95
//! over repeated runs. `FOLDBENCH_RUNS` sets the runs of the open, `FOLDBENCH_CACHED_ONLY`
//! stops after the opens with the cache, `FOLDBENCH_STREAM_ONLY` after the opens without
//! it too (for a heap profile), and `FOLDBENCH_FOLD_ONLY` after the open's parts.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tc_corpus::{Corpus, Spec};
use tc_journal::fold::{DerivedFile, FoldInput, FoldState, ProjectKind, ValueAt, ValueSource};
use tc_journal::index::{FileKey, Fingerprinter, Index, Section};
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

/// Starts the peak resident size again from the current one (Linux 4.0 and later), so each
/// open's own peak can be read.
fn reset_peak_rss() {
    let _ = std::fs::write("/proc/self/clear_refs", "5");
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

/// The app build the benchmark's cache is written by (`tc-store` passes the app's own).
const BUILD: &str = concat!("foldbench ", env!("CARGO_PKG_VERSION"));

/// Where the saved corpus keeps its fold cache: `.tc/index.bin` in a project folder.
fn index_path(dir: &Path) -> PathBuf {
    dir.join("index.bin")
}

fn project_of(dir: &Path) -> ProjectId {
    ProjectId::parse(
        std::fs::read_to_string(dir.join("project"))
            .expect("project")
            .trim(),
    )
    .expect("project id")
}

/// What a streamed open hands over: each file's prepared changes as soon as a reading
/// thread has them, the source the fold reads values from, and, once every file is read,
/// each file's key with the section made of it when the cache did not hold it.
struct Streamed {
    inputs: std::sync::mpsc::Receiver<Vec<FoldInput>>,
    source: Arc<dyn ValueSource>,
    sections: std::sync::mpsc::Receiver<(FileKey, Option<Section>)>,
}

/// The key of the file at `path` in the project folder, taken a chunk at a time, so a file
/// the cache holds is never in memory whole.
fn key_of(cache_path: &str, path: &Path, buffer: &mut [u8]) -> FileKey {
    use std::io::Read;
    let mut file = File::open(path).expect("open");
    let mut fingerprint = Fingerprinter::new();
    let mut size = 0;
    loop {
        let n = file.read(buffer).expect("read");
        if n == 0 {
            break;
        }
        fingerprint.update(&buffer[..n]);
        size += n as u64;
    }
    FileKey {
        path: cache_path.into(),
        size,
        fingerprint: fingerprint.finish(),
    }
}

/// Reads every pack and pending file on `threads` threads, as an open does, keeping only
/// where each value lies, and hands each file's changes over as soon as they are prepared,
/// so the fold indexes them while the threads read the rest. With a fold cache, a file
/// whose path, size and fingerprint equal a section's takes that section's changes; every
/// other file is verified and parsed (P1 to P4), and its section is made.
fn load_streamed(dir: &Path, threads: usize, index: Option<Arc<Index>>) -> Streamed {
    let project = project_of(dir);
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
    let (send_section, sections) = std::sync::mpsc::channel::<(FileKey, Option<Section>)>();
    for _ in 0..threads {
        let (queue, next, send) = (queue.clone(), next.clone(), send.clone());
        let (send_section, index) = (send_section.clone(), index.clone());
        std::thread::spawn(move || {
            let mut buffer = vec![0u8; 1 << 20];
            while let Some((file, is_pack, path)) =
                queue.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
            {
                let cache_path = if *is_pack {
                    pack_path(path)
                } else {
                    let name = path.file_name().unwrap().to_string_lossy();
                    format!(".tc/pending/{name}")
                };
                let cached = index.as_ref().and_then(|index| {
                    let key = key_of(&cache_path, path, &mut buffer);
                    let inputs = index.section(&key)?.inputs(*file).ok()?;
                    Some((key, inputs))
                });
                let (key, inputs, section) = match cached {
                    Some((key, inputs)) => (key, inputs, None),
                    None => {
                        // Read whole, and keyed by the bytes read, which may differ from
                        // those fingerprinted if another program replaced the file since.
                        let bytes = std::fs::read(path).expect("read");
                        let loaded = if *is_pack {
                            let (read, loaded) =
                                tc_journal::index::load_pack(&cache_path, &bytes, project, *file);
                            assert!(read.is_valid(), "{cache_path}: {:?}", read.status);
                            loaded
                        } else {
                            tc_journal::index::load_pending(&cache_path, &bytes, project, *file).1
                        };
                        (loaded.key, loaded.inputs, loaded.section)
                    }
                };
                let _ = send_section.send((key, section));
                if send.send(inputs).is_err() {
                    return;
                }
            }
        });
    }
    Streamed {
        inputs: receive,
        source: Arc::new(Files(opened)),
        sections,
    }
}

/// [`load_streamed`] without a cache, collected: every prepared change, for a fold timed
/// on its own.
fn load(dir: &Path, threads: usize) -> (Vec<FoldInput>, Arc<dyn ValueSource>) {
    let streamed = load_streamed(dir, threads, None);
    (
        streamed.inputs.into_iter().flatten().collect(),
        streamed.source,
    )
}

/// The files an open read: each one's key, with the section made of it when the cache did
/// not hold it.
type Read = Vec<(FileKey, Option<Section>)>;

/// One streamed open: the fold indexes each file's changes as they arrive. Returns the
/// fold, the time after the last file was read, and the files read.
fn open_streamed(
    dir: &Path,
    threads: usize,
    index: Option<Arc<Index>>,
) -> (FoldState, Duration, Read) {
    let streamed = load_streamed(dir, threads, index);
    let last_read = std::cell::Cell::new(None);
    let inputs = streamed
        .inputs
        .into_iter()
        .flatten()
        .chain(std::iter::from_fn(|| {
            last_read.set(Some(Instant::now()));
            None
        }));
    let folded = FoldState::fold_from(ProjectKind::Bible, Some(streamed.source), inputs);
    let tail = last_read.get().map(|t| t.elapsed()).unwrap_or_default();
    (folded, tail, streamed.sections.into_iter().collect())
}

/// Whether the cache an open used still stands for every file it read: no file was read
/// in full, and no file the cache holds is gone. Otherwise `tc-store` writes it again.
fn cache_unchanged(held: usize, read: &Read) -> bool {
    read.iter().all(|(_, section)| section.is_none()) && read.len() == held
}

/// Reads the fold cache of a saved corpus, as an open reads `.tc/index.bin` before the
/// files; `None` when there is none or it is not this build's.
fn read_index(dir: &Path) -> Option<Arc<Index>> {
    let bytes = std::fs::read(index_path(dir)).ok()?;
    match Index::parse(bytes, project_of(dir), BUILD) {
        Ok(index) => Some(Arc::new(index)),
        Err(error) => {
            eprintln!("the fold cache is not used: {error}");
            None
        }
    }
}

/// Writes the fold cache as `tc-store` does after an open whose files changed: the sections
/// made of the files read in full, and those of `old` for the files it held, to a temporary
/// file in the same directory, renamed over the old one.
fn write_index(dir: &Path, read: &Read, old: Option<&Index>) -> usize {
    let sections: Vec<&Section> = read
        .iter()
        .filter_map(|(key, made)| made.as_ref().or_else(|| old?.section(key)))
        .collect();
    let bytes = Index::encode(project_of(dir), BUILD, sections);
    let temp = dir.join(".~index.bin.tmp");
    std::fs::write(&temp, &bytes).expect("write the cache");
    std::fs::rename(&temp, index_path(dir)).expect("rename the cache");
    bytes.len()
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
    let runs: usize = std::env::var("FOLDBENCH_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let file_count = files(&dir.join("packs")).len() + files(&dir.join("pending")).len();
    // The first open of a project, or the first after an update of the app, has no cache:
    // it reads every file and writes the cache for the next.
    if read_index(&dir).is_none() {
        let started = Instant::now();
        let (folded, _, read) = open_streamed(&dir, threads, None);
        let opened = started.elapsed();
        let t = Instant::now();
        let size = write_index(&dir, &read, None);
        eprintln!(
            "first open {opened:?}; the cache of {} files written in {:?}, {size} bytes",
            read.len(),
            t.elapsed()
        );
        drop(folded);
    }
    let index_bytes = std::fs::metadata(index_path(&dir)).map_or(0, |m| m.len());
    let mut cached = Stats::new(
        "Cold open with the fold cache: read the cache, read and fingerprint every file, take \
         each file's changes from the cache, fold as they arrive",
    );
    let mut cached_tail =
        Stats::new("Cold open with the fold cache: the fold's work after the last file");
    let mut state = None;
    let mut changes = 0;
    let mut peaks = Vec::new();
    for _ in 0..runs {
        drop(state.take());
        reset_peak_rss();
        let started = Instant::now();
        let cpu = process_cpu_ms();
        let index = read_index(&dir).expect("the fold cache");
        let held = index.len();
        // The reading threads hold the cache, so it is freed once the last file is read.
        let (folded, tail, read) = open_streamed(&dir, threads, Some(index));
        cached.times.push(started.elapsed());
        cached_tail.times.push(tail);
        assert_eq!(read.len(), file_count);
        assert!(
            cache_unchanged(held, &read),
            "every file is taken from the cache"
        );
        eprintln!(
            "cold open with the cache {:?}, the fold after the last file {tail:?}, cpu over all \
             threads {} ms",
            started.elapsed(),
            process_cpu_ms() - cpu
        );
        changes = folded.len();
        state = Some(folded);
        peaks.push(peak_rss_mib());
    }
    println!(
        "fold cache: {index_bytes} bytes for {file_count} files; peak RSS of each cold open with \
         the cache {:?} MiB, RSS with the fold open {:.0} MiB",
        peaks.iter().map(|p| p.round() as u64).collect::<Vec<_>>(),
        rss_mib()
    );
    if std::env::var("FOLDBENCH_CACHED_ONLY").is_ok() {
        cached.report();
        cached_tail.report();
        return;
    }
    let mut open = Stats::new(
        "Cold open without a cache: read, verify, prepare and fold, the fold indexing as files \
         are read",
    );
    let mut fold_only = Stats::new("Full fold of the prepared changes");
    let mut after_read = Stats::new(
        "Cold open without a cache: the fold's work after the last file is read (stages F2 to \
         F10)",
    );
    let mut load_only = Stats::new("Read, verify and prepare");
    // The cold open: the fold indexes each file's changes as the reading threads hand
    // them over, then runs its stages once the last arrives.
    for _ in 0..runs {
        drop(state.take());
        let started = Instant::now();
        let cpu = process_cpu_ms();
        let (folded, tail, _) = open_streamed(&dir, threads, None);
        open.times.push(started.elapsed());
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
        cached.report();
        cached_tail.report();
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
        cached.report();
        cached_tail.report();
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
    cached.report();
    cached_tail.report();
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
