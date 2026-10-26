//! Release benchmarks of the fold (docs/PERFORMANCE.md rule 5) on a saved corpus.
//!
//! `cargo run --release -p tc-corpus --example foldbench -- save bible DIR` generates a
//! corpus (`psalms`, `scaled <factor>`, `bible`, or `bible-share <share>` for less history)
//! and saves its packs and pending files under DIR; `... -- run DIR [threads]` reads them
//! back the way an open does and times the actions the fold owns. The cold open is timed
//! with the fold cache (`DIR/index.bin`, `.tc/index.bin` in a project; the first run writes
//! it): each file is read and hashed, and its prepared changes taken from the cache; and
//! without it: every line verified, parsed and prepared on `threads` threads, keeping only
//! where each value lies. Either way the open keeps each file's facts and, once the last file
//! is read, runs O7's rules on them (§6.1). Then the full fold; and, in a process of its own
//! (`... -- app DIR [threads]`, which `run` starts last), the app's case: one open with the
//! cache, then one incremental step after a verse save, a decision, an alignment and a
//! resolution, the conflict list from scratch, the Psalms projection, whose values are read
//! from the files on demand, and snapshots, with the memory that leaves resident. Times are
//! medians and p95 over repeated runs. `FOLDBENCH_RUNS` sets the runs of the open,
//! `FOLDBENCH_FIRST_ONLY` stops after the first open when there is no cache yet,
//! `FOLDBENCH_CACHED_ONLY` stops after the opens with the cache, `FOLDBENCH_STREAM_ONLY` after
//! the opens without it too (for a heap profile), and `FOLDBENCH_FOLD_ONLY` after the open's
//! parts.
//! `FOLDBENCH_PSALMS_ONLY` projects Psalms right after the opens with the cache and stops: the
//! whole Bible open, Psalms in use, without the steps `app` adds. `FOLDBENCH_NO_O7` drops each
//! file's facts as it is read instead of holding them for O7's rules, to measure what holding
//! costs.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tc_corpus::{Corpus, Spec};
use tc_journal::consolidate::redundancy;
use tc_journal::fold::{DerivedFile, FoldInput, FoldState, ProjectKind, ValueAt, ValueSource};
use tc_journal::index::{Facts, FileKey, Fingerprinter, Index, Section};
use tc_journal::pack::PackFacts;
use tc_journal::pending::PendingFacts;
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
        Some("app") => app(&args[1..]),
        _ => {
            eprintln!(
                "foldbench save <psalms|scaled N|bible|bible-share S> DIR | run DIR [threads] | \
                 app DIR [threads]"
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

// Every reading below comes from Linux's `/proc`. Where it cannot be read (macOS, Windows,
// a container without `/proc`), the figure is `None` and prints as "unavailable", so a zero
// is never taken for a measurement (docs/LOCAL-BENCH-FINDINGS-2026-10-16.md F4).

/// CPU time of the calling thread, in ns.
fn cpu_ns() -> Option<u64> {
    std::fs::read_to_string("/proc/thread-self/schedstat")
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// A field of `/proc/self/stat` after the command name.
fn stat_field(n: usize) -> Option<u64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    stat.rsplit(')')
        .next()?
        .split_whitespace()
        .nth(n)?
        .parse()
        .ok()
}

/// CPU time of the whole process, user and system, in ms (10 ms resolution).
fn process_cpu_ms() -> Option<u64> {
    // utime and stime, in clock ticks of 10 ms.
    Some((stat_field(11)? + stat_field(12)?) * 10)
}

/// The process CPU time spent since `start`, a reading of [`process_cpu_ms`].
fn cpu_since(start: Option<u64>) -> Option<u64> {
    process_cpu_ms()?.checked_sub(start?)
}

fn minflt() -> Option<u64> {
    stat_field(7)
}

/// A line of `/proc/self/status` in MiB: `VmHWM` is the peak resident memory, `VmRSS` the
/// current.
fn status_mib(field: &str) -> Option<f64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let kb: f64 = status
        .lines()
        .find(|l| l.starts_with(field))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    Some(kb / 1024.0)
}

fn peak_rss_mib() -> Option<f64> {
    status_mib("VmHWM:")
}

/// Starts the peak resident size again from the current one (Linux 4.0 and later), so each
/// open's own peak can be read.
fn reset_peak_rss() {
    let _ = std::fs::write("/proc/self/clear_refs", "5");
}

fn rss_mib() -> Option<f64> {
    status_mib("VmRSS:")
}

/// Where the resident memory is (`/proc/self/smaps`): the main thread's heap, which holds the
/// fold's tables; other anonymous memory, which is the heaps of worker threads and large
/// blocks; and mapped files, the program's code among them.
fn resident_by_kind() -> String {
    let Ok(text) = std::fs::read_to_string("/proc/self/smaps") else {
        return "resident memory by kind unavailable".to_owned();
    };
    let (mut heap, mut anonymous, mut files) = (0u64, 0u64, 0u64);
    let mut kind = 0;
    for line in text.lines() {
        let mut words = line.split_whitespace();
        let first = words.next().unwrap_or_default();
        if let Some(kb) = line.strip_prefix("Rss:") {
            let kb: u64 = kb.trim().trim_end_matches("kB").trim().parse().unwrap_or(0);
            match kind {
                0 => heap += kb,
                1 => anonymous += kb,
                _ => files += kb,
            }
        } else if first.contains('-') && !first.ends_with(':') {
            // A mapping's first line: range, permissions, offset, device, inode, path.
            kind = match words.nth(4) {
                Some("[heap]") => 0,
                None => 1,
                Some(path) if path.starts_with('[') => 1,
                Some(_) => 2,
            };
        }
    }
    format!(
        "resident: the main heap {} MiB, other anonymous memory {} MiB, mapped files {} MiB",
        heap / 1024,
        anonymous / 1024,
        files / 1024
    )
}

/// A time in ms, or "unavailable".
fn ms(reading: Option<u64>) -> String {
    reading.map_or_else(|| "unavailable".to_owned(), |v| format!("{v} ms"))
}

/// A size in MiB, or "unavailable".
fn mib(reading: Option<f64>) -> String {
    reading.map_or_else(|| "unavailable".to_owned(), |v| format!("{v:.0} MiB"))
}

/// A count, or "unavailable".
fn count(reading: Option<u64>) -> String {
    reading.map_or_else(|| "unavailable".to_owned(), |v| v.to_string())
}

/// The machine line: the CPU features that move the budgets (SHA instructions make SHA-256
/// 6.5 times faster, docs/LOCAL-BENCH-FINDINGS-2026-10-16.md F1), the threads and the load.
fn machine_line(threads: usize) -> String {
    #[cfg(target_arch = "x86_64")]
    let features = format!(
        "sha {}, avx2 {}, avx512f {}, avx512bw {}, avx512vl {}",
        std::is_x86_feature_detected!("sha"),
        std::is_x86_feature_detected!("avx2"),
        std::is_x86_feature_detected!("avx512f"),
        std::is_x86_feature_detected!("avx512bw"),
        std::is_x86_feature_detected!("avx512vl"),
    );
    #[cfg(not(target_arch = "x86_64"))]
    let features = "not x86, so no SHA or AVX-512 bits to read".to_owned();
    let load = std::fs::read_to_string("/proc/loadavg")
        .map_or_else(|_| "unavailable".to_owned(), |l| l.trim().to_owned());
    format!("cpu: {features}; {threads} threads; load {load}")
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
/// thread has them, the source the fold reads values from, and each file's key, with the
/// section made of it when the cache did not hold it, and its facts for O7's rules.
struct Streamed {
    inputs: std::sync::mpsc::Receiver<Vec<FoldInput>>,
    source: Arc<dyn ValueSource>,
    sections: std::sync::mpsc::Receiver<(FileKey, Option<Section>, Option<Facts>)>,
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
    let (send_section, sections) =
        std::sync::mpsc::channel::<(FileKey, Option<Section>, Option<Facts>)>();
    // To measure what holding every file's facts until O7 costs, `FOLDBENCH_NO_O7` drops them.
    let keep_facts = std::env::var_os("FOLDBENCH_NO_O7").is_none();
    // The readers run on the worker pool the fold's own parallel stages use, so the open's
    // threads are the pool's, not four more each open; each reuses its buffers from file to
    // file.
    for _ in 0..threads {
        let (queue, next, send) = (queue.clone(), next.clone(), send.clone());
        let (send_section, index) = (send_section.clone(), index.clone());
        rayon::spawn(move || {
            let mut buffer = vec![0u8; 1 << 20];
            let mut whole: Vec<u8> = Vec::new();
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
                    let loaded = index.section(&key)?.load(*file).ok()?;
                    Some((key, loaded.inputs, loaded.facts))
                });
                let (key, inputs, section, facts) = match cached {
                    Some((key, inputs, facts)) => (key, inputs, None, facts),
                    None => {
                        // Read whole, and keyed by the bytes read, which may differ from
                        // those fingerprinted if another program replaced the file since.
                        whole.clear();
                        {
                            use std::io::Read;
                            File::open(path)
                                .and_then(|mut f| f.read_to_end(&mut whole))
                                .expect("read");
                        }
                        let bytes = &whole;
                        let loaded = if *is_pack {
                            let loaded =
                                tc_journal::index::load_pack(&cache_path, bytes, project, *file);
                            assert!(
                                matches!(&loaded.facts, Facts::Pack(pack) if pack.is_valid()),
                                "{cache_path}: {:?}",
                                loaded.facts
                            );
                            loaded
                        } else {
                            tc_journal::index::load_pending(&cache_path, bytes, project, *file)
                        };
                        (loaded.key, loaded.inputs, loaded.section, loaded.facts)
                    }
                };
                let _ = send_section.send((key, section, keep_facts.then_some(facts)));
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

/// One streamed open: the fold indexes each file's changes as they arrive. Once the last file
/// is read, O7's rules run on the files' facts (§6.1): the pack rule (§4.7) needs only the
/// packs' facts, so it runs then and the packs' change ids are dropped before the fold's
/// stages allocate; the pending rule asks the fold, so it runs after them. Returns the fold,
/// the time after the last file was read (the fold's stages and the rules), the rules' time,
/// and the files read.
fn open_streamed(
    dir: &Path,
    threads: usize,
    index: Option<Arc<Index>>,
) -> (FoldState, Duration, Duration, Read) {
    let streamed = load_streamed(dir, threads, index);
    let sections = streamed.sections;
    let last_read = std::cell::Cell::new(None);
    let after_read = std::cell::RefCell::new(None);
    let inputs = streamed
        .inputs
        .into_iter()
        .flatten()
        .chain(std::iter::from_fn(|| {
            if last_read.get().is_none() {
                last_read.set(Some(Instant::now()));
                // Every reading thread has ended, so every file's facts were sent.
                *after_read.borrow_mut() = Some(pack_rules(sections.iter()));
            }
            None
        }));
    let folded = FoldState::fold_from(ProjectKind::Bible, Some(streamed.source), inputs);
    let (read, pending, mut rules) = after_read.into_inner().expect("the last file was read");
    if let Some(pending) = pending {
        let started = Instant::now();
        pending_rule(&folded, &pending.0, &pending.1);
        rules += started.elapsed();
    }
    let tail = last_read.get().map(|t| t.elapsed()).unwrap_or_default();
    (folded, tail, rules, read)
}

/// The facts the pending rule needs: the valid packs' ids and every pending file's facts.
type PendingInput = (
    std::collections::HashSet<tc_journal::PackId>,
    Vec<PendingFacts>,
);

/// The files read and O7's pack rule: the packs a valid pack replaces (§4.7). Returns the
/// files, what the pending rule needs (`None` when the facts were dropped as files were
/// read), and the rule's time. The saved corpus has no replaced pack.
fn pack_rules(
    files: impl Iterator<Item = (FileKey, Option<Section>, Option<Facts>)>,
) -> (Read, Option<PendingInput>, Duration) {
    let started = Instant::now();
    let mut read = Read::new();
    let mut packs: Vec<PackFacts> = Vec::new();
    let mut pending: Vec<PendingFacts> = Vec::new();
    let mut kept = true;
    for (key, section, facts) in files {
        read.push((key, section));
        match facts {
            Some(Facts::Pack(facts)) => packs.push(facts),
            Some(Facts::Pending(facts)) => pending.push(facts),
            None => kept = false,
        }
    }
    if !kept {
        return (read, None, started.elapsed());
    }
    let result = redundancy(&packs);
    assert!(
        result.redundant.is_empty() && result.kept.is_empty(),
        "{result:?}"
    );
    let valid = packs
        .iter()
        .filter(|pack| pack.is_valid())
        .map(|pack| pack.id)
        .collect();
    drop(packs);
    (read, Some((valid, pending)), started.elapsed())
}

/// O7's pending rule: a pending file every change of which a valid pack holds is removed. It
/// asks the fold which packs hold each change, as `tc-store` may. The saved corpus has none.
fn pending_rule(
    folded: &FoldState,
    valid: &std::collections::HashSet<tc_journal::PackId>,
    pending: &[PendingFacts],
) {
    let held = pending
        .iter()
        .filter(|file| {
            file.ids.iter().all(|id| {
                folded
                    .change(id)
                    .is_some_and(|change| change.packs.iter().any(|p| valid.contains(p)))
            })
        })
        .count();
    assert_eq!(held, 0, "no pending file of the corpus is sealed");
}

/// Whether the cache an open used still stands for every file it read: no file was read
/// in full, and no file the cache holds is gone. Otherwise `tc-store` writes it again.
fn cache_unchanged(held: usize, read: &Read) -> bool {
    read.iter().all(|(_, section)| section.is_none()) && read.len() == held
}

/// Reads the fold cache of a saved corpus, as an open reads `.tc/index.bin` before the
/// files; `None` when there is none, or it is of another cache version or format minor.
fn read_index(dir: &Path) -> Option<Arc<Index>> {
    let bytes = std::fs::read(index_path(dir)).ok()?;
    match Index::parse(bytes, project_of(dir)) {
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
    let bytes = Index::encode(project_of(dir), sections);
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
    println!("{}", machine_line(threads));
    // The schema validator is compiled once per process; not part of any action.
    let _ = tc_burrito::check_field(
        tc_burrito::ProjectKind::BibleText,
        "identification.name",
        &serde_json::json!({"en": "x"}),
    );
    // Every open of a process pays the probe stamp once (§3.6); the opens below do not.
    let t = Instant::now();
    let stamp = tc_journal::index::stamp();
    println!("probe stamp {stamp:#x} computed in {:?}", t.elapsed());
    println!("RSS before the first open {}", mib(rss_mib()));
    let runs: usize = std::env::var("FOLDBENCH_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let file_count = files(&dir.join("packs")).len() + files(&dir.join("pending")).len();
    // The first open of a project, or the first after an update that changed the cache's
    // encoding, has no cache: it reads every file and writes the cache for the next.
    if read_index(&dir).is_none() {
        reset_peak_rss();
        let started = Instant::now();
        let cpu = process_cpu_ms();
        let (folded, tail, rules, read) = open_streamed(&dir, threads, None);
        let opened = started.elapsed();
        let cpu = cpu_since(cpu);
        let peak = peak_rss_mib();
        let t = Instant::now();
        let size = write_index(&dir, &read, None);
        println!(
            "first open without a cache {:.0} ms, the fold after the last file {:.0} ms, cpu over \
             all threads {}, peak RSS {}; the cache of {} files written in {:?}, {size} bytes; \
             O7's rules {rules:?}",
            opened.as_secs_f64() * 1e3,
            tail.as_secs_f64() * 1e3,
            ms(cpu),
            mib(peak),
            read.len(),
            t.elapsed()
        );
        drop(folded);
        if std::env::var("FOLDBENCH_FIRST_ONLY").is_ok() {
            return;
        }
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
        let (folded, tail, rules, read) = open_streamed(&dir, threads, Some(index));
        cached.times.push(started.elapsed());
        cached_tail.times.push(tail);
        assert_eq!(read.len(), file_count);
        assert!(
            cache_unchanged(held, &read),
            "every file is taken from the cache"
        );
        eprintln!(
            "cold open with the cache {:?}, the fold after the last file {tail:?}, cpu over all \
             threads {}, O7's rules {rules:?}",
            started.elapsed(),
            ms(cpu_since(cpu))
        );
        changes = folded.len();
        state = Some(folded);
        peaks.push(peak_rss_mib());
    }
    println!(
        "fold cache: {index_bytes} bytes for {file_count} files; peak RSS of each cold open with \
         the cache {:?}, RSS with the fold open {}",
        peaks.iter().map(|p| mib(*p)).collect::<Vec<_>>(),
        mib(rss_mib())
    );
    if std::env::var("FOLDBENCH_CACHED_ONLY").is_ok() {
        cached.report();
        cached_tail.report();
        return;
    }
    // The app's case: the whole Bible opened with the cache, then Psalms in use.
    if std::env::var("FOLDBENCH_PSALMS_ONLY").is_ok() {
        cached.report();
        cached_tail.report();
        let state = state.expect("folded");
        println!("RSS with the fold open {}", mib(rss_mib()));
        reset_peak_rss();
        project_psalms(&state);
        println!("{}", resident_by_kind());
        println!(
            "peak RSS while Psalms was projected {}; RSS with the fold open after Psalms was \
             projected {}",
            mib(peak_rss_mib()),
            mib(rss_mib())
        );
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
    reset_peak_rss();
    for _ in 0..runs {
        drop(state.take());
        let started = Instant::now();
        let cpu = process_cpu_ms();
        let (folded, tail, _, _) = open_streamed(&dir, threads, None);
        open.times.push(started.elapsed());
        after_read.times.push(tail);
        eprintln!(
            "cold open {:?}, the fold after the last file {tail:?}, cpu over all threads {}",
            started.elapsed(),
            ms(cpu_since(cpu))
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
            "peak RSS during the streamed cold opens {}, RSS with the fold open {}",
            mib(open_peak),
            mib(rss_mib())
        );
        return;
    }
    // The parts on their own: reading every file, then folding what was read. No open
    // holds every prepared change at once like this; its peak is printed on its own.
    reset_peak_rss();
    for _ in 0..runs {
        drop(state.take());
        let started = Instant::now();
        let load_cpu = process_cpu_ms();
        let (inputs, source) = load(&dir, threads);
        let loaded = started.elapsed();
        eprintln!(
            "load {loaded:?}, cpu over all threads {}",
            ms(cpu_since(load_cpu))
        );
        let t = Instant::now();
        let f0 = minflt();
        let c0 = cpu_ns();
        let folded = FoldState::fold_from(ProjectKind::Bible, Some(source), inputs);
        fold_only.times.push(t.elapsed());
        eprintln!(
            "fold {:?}, cpu {}, minor faults {}",
            t.elapsed(),
            ms(cpu_ns()
                .zip(c0)
                .and_then(|(now, start)| now.checked_sub(start))
                .map(|ns| ns / 1_000_000)),
            count(
                minflt()
                    .zip(f0)
                    .and_then(|(now, start)| now.checked_sub(start))
            )
        );
        load_only.times.push(loaded);
        state = Some(folded);
    }
    println!(
        "peak RSS during the streamed cold opens {}; while every prepared change was collected \
         and folded {}",
        mib(open_peak),
        mib(peak_rss_mib())
    );
    let state = state.expect("folded");
    if std::env::var("FOLDBENCH_FOLD_ONLY").is_ok() {
        load_only.report();
        fold_only.report();
        cached.report();
        cached_tail.report();
        open.report();
        after_read.report();
        println!("peak RSS {}", mib(peak_rss_mib()));
        return;
    }
    println!(
        "corpus: {} lines, {} records, {} conflicts, {} anomalies; RSS with the fold open {}",
        changes,
        state.record_keys().len(),
        state.conflict_count(),
        state.anomalies().count(),
        mib(rss_mib())
    );
    println!("| Action | Runs | Median | p95 |");
    println!("|---|---:|---:|---:|");
    load_only.report();
    // What SHA-256 alone costs: one pass over every file's bytes on the same threads (P2
    // hashes each file once and P4 each line once, so a load hashes every byte twice). Each
    // thread reads its files one at a time into one buffer and times only the hashing, so
    // the pass holds a few files, not the project's 700 MB; the slowest thread's hashing
    // time is the pass's.
    let mut sha = Stats::new("SHA-256 of every file once (one of the load's two passes)");
    let all: Vec<PathBuf> = files(&dir.join("packs"))
        .into_iter()
        .chain(files(&dir.join("pending")))
        .collect();
    for _ in 0..3 {
        let busy = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..threads)
                .map(|t| {
                    let all = &all;
                    scope.spawn(move || {
                        use std::io::Read;
                        let mut buffer = Vec::new();
                        let mut busy = Duration::ZERO;
                        for path in all.iter().skip(t).step_by(threads) {
                            buffer.clear();
                            File::open(path)
                                .and_then(|mut f| f.read_to_end(&mut buffer))
                                .expect("read");
                            let started = Instant::now();
                            std::hint::black_box(tc_journal::PackId::of(&buffer));
                            busy += started.elapsed();
                        }
                        busy
                    })
                })
                .collect();
            workers
                .into_iter()
                .map(|w| w.join().expect("a hashing thread"))
                .max()
                .unwrap_or_default()
        });
        sha.times.push(busy);
    }
    sha.report();
    fold_only.report();
    cached.report();
    cached_tail.report();
    open.report();
    after_read.report();

    println!(
        "this process, after its opens and the parts: {}",
        resident_by_kind()
    );
    // The app's case runs in a process of its own: an app opens a project once, and this
    // process has opened it many times and held every prepared change at once, memory the
    // allocator may keep after it is freed.
    drop(state);
    use std::io::Write;
    std::io::stdout().flush().expect("flush");
    let status = std::process::Command::new(std::env::current_exe().expect("this program"))
        .arg("app")
        .arg(&dir)
        .arg(threads.to_string())
        .status()
        .expect("the app's case");
    assert!(status.success(), "the app's case failed");
}

/// The app's case: one open (with the fold cache, or the first, which writes it), then a
/// person working in Psalms (saves,
/// decisions, alignments and resolutions, the conflict list, the book's projection and
/// snapshots), in a process that has done nothing else.
fn app(args: &[String]) {
    let dir = PathBuf::from(&args[0]);
    let threads: usize = args.get(1).and_then(|t| t.parse().ok()).unwrap_or(4);
    let _ = tc_burrito::check_field(
        tc_burrito::ProjectKind::BibleText,
        "identification.name",
        &serde_json::json!({"en": "x"}),
    );
    let _ = tc_journal::index::stamp();
    println!(
        "the app's case, in a process of its own: RSS before the open {}",
        mib(rss_mib())
    );
    reset_peak_rss();
    let started = Instant::now();
    // Without a cache this is the first open of a project, which writes the cache.
    let index = read_index(&dir);
    let cached = index.is_some();
    let (state, _, _, read) = open_streamed(&dir, threads, index);
    if !cached {
        write_index(&dir, &read, None);
    }
    drop(read);
    println!(
        "open {} the fold cache {:?}, peak RSS {}; RSS with the fold open {}",
        if cached { "with" } else { "without" },
        started.elapsed(),
        mib(peak_rss_mib()),
        mib(rss_mib())
    );
    println!("{}", resident_by_kind());
    reset_peak_rss();
    let mut state = state;
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

    project_psalms(&state);
    let mut snapshot = Stats::new("Snapshot (clone of the fold)");
    for _ in 0..20 {
        let t = Instant::now();
        let s = state.clone();
        snapshot.times.push(t.elapsed());
        drop(s);
    }
    snapshot.report();
    println!("{}", resident_by_kind());
    println!(
        "peak RSS of the incremental steps, conflict lists, Psalms projections and snapshots \
         {}; RSS with the fold open after Psalms was projected {}",
        mib(peak_rss_mib()),
        mib(rss_mib())
    );
}

/// Renders every file of Psalms `FOLDBENCH_PROJECTIONS` times (20 by default), with its
/// values read from the files, as materializing the book does.
fn project_psalms(state: &FoldState) {
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
        cpu.push(cpu_since(c));
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
    // One unreadable run makes the median unavailable rather than skew it.
    let mut cpu: Option<Vec<u64>> = cpu.into_iter().collect();
    let median = cpu.as_mut().and_then(|cpu| {
        cpu.sort_unstable();
        cpu.get(cpu.len() / 2).copied()
    });
    println!(
        "Psalms: {files} files, {bytes} bytes; CPU time of a projection, all threads: median {}",
        ms(median)
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
}
