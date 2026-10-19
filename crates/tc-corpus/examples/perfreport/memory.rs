//! The memory of the project store (PERFORMANCE.md: 400 MiB steady, 450 MiB peak, private
//! bytes read a minute after the last action, never just before exit) and its growth with
//! history (300 B a change with one heap, under 350 B with the default Linux heap, at twice
//! today's history).
//!
//! `memory` runs in a child of its own, so its figures are one process's: an open, 200 saves
//! and 200 decisions in Psalms, leaving the book, a minute of waiting, then a whole-project
//! materialize and the close. A thread samples the private bytes (Linux: `Private_Clean` and
//! `Private_Dirty` of `smaps_rollup`) every 50 ms, so the peak is a peak of private bytes
//! and not of the resident set.

use crate::common::{decide_one, decisions_of, ms, open, run_id, save_one, store, verses_of};
use crate::machine::private_mib;
use crate::table::Measured;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tc_journal::command::Command;
use tc_journal::{BookId, RecordKey};
use tc_store::Leave;

/// Samples this process's private bytes until it is dropped.
pub struct PeakSampler {
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicU64>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl PeakSampler {
    pub fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicU64::new(0));
        let (s, p) = (stop.clone(), peak.clone());
        let thread = std::thread::spawn(move || {
            while !s.load(Ordering::Acquire) {
                if let Some(mib) = private_mib() {
                    p.fetch_max((mib * 1024.0) as u64, Ordering::Relaxed);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        Self {
            stop,
            peak,
            thread: Some(thread),
        }
    }

    /// The largest private figure seen so far, in MiB.
    pub fn peak_mib(&self) -> f64 {
        self.peak.load(Ordering::Relaxed) as f64 / 1024.0
    }
}

impl Drop for PeakSampler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn heap_label() -> &'static str {
    if std::env::var("MALLOC_ARENA_MAX").is_ok_and(|v| v == "1") {
        "one heap"
    } else {
        "default heap"
    }
}

/// The first open of a project with no usable fold cache: the peak of that open alone.
pub fn nocache(dir: &Path) -> Vec<Measured> {
    crate::common::header("memory (first open without a cache)");
    let index = crate::common::project_folder(dir).join(".tc/index.bin");
    let _ = std::fs::remove_file(&index);
    let sampler = PeakSampler::start();
    let store = store(dir);
    let started = Instant::now();
    let handle = open(&store, dir);
    let opened = ms(started);
    let steady = private_mib().unwrap_or(f64::NAN);
    handle.close().expect("close");
    let peak = sampler.peak_mib();
    let suffix = if heap_label() == "one heap" {
        ""
    } else {
        "_default_heap"
    };
    vec![Measured::new(
        &format!("mem.peak{suffix}"),
        vec![peak],
        format!(
            "{}: the first open without a cache ({opened:.0} ms), private {steady:.0} MiB when it ends, peak {peak:.0} MiB",
            heap_label()
        ),
    )]
}

/// The app's case: open, 200 saves and 200 decisions in Psalms, leave, a minute, then a
/// whole-project materialize and the close.
pub fn app_case(dir: &Path) -> Vec<Measured> {
    crate::common::header("memory");
    let sampler = PeakSampler::start();
    let store = store(dir);
    let started = Instant::now();
    let handle = open(&store, dir);
    let changes = handle.snapshot().fold().len();
    let after_open = private_mib().unwrap_or(f64::NAN);
    let open_ms = ms(started);
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
    eprintln!("waiting a minute after the last action");
    std::thread::sleep(Duration::from_secs(60));
    let steady = private_mib().unwrap_or(f64::NAN);
    let peak_so_far = sampler.peak_mib();
    let key = RecordKey::Pins(tc_journal::PinGroup::Lexicon);
    let shown = handle.mint(&key, None).expect("a token");
    handle
        .submit(Command::SetPins {
            shown,
            value: crate::r9::pins_value(run, 99),
        })
        .wait()
        .expect("pins");
    handle.leave(Leave::Mode).wait().expect("leave");
    handle.wait_idle();
    let after_whole = private_mib().unwrap_or(f64::NAN);
    handle.close().expect("close");
    let after_close = private_mib().unwrap_or(f64::NAN);
    let peak = sampler.peak_mib();
    let heap = heap_label();
    let suffix = if heap == "one heap" {
        ""
    } else {
        "_default_heap"
    };
    vec![
        Measured::new("mem.changes", vec![changes as f64], ""),
        Measured::new(
            &format!("mem.steady{suffix}"),
            vec![steady],
            format!(
                "{heap}: {changes} changes; private {after_open:.0} MiB after the open ({open_ms:.0} ms), {steady:.0} MiB a minute after 200 saves and 200 decisions, {after_whole:.0} MiB after a whole-project materialize, {after_close:.0} MiB after the close; peak {peak:.0} MiB (before the whole-project job {peak_so_far:.0})"
            ),
        ),
        Measured::new(
            &format!("mem.peak{suffix}"),
            vec![peak],
            format!("{heap}: the open, the saves, the whole-project job and the close"),
        ),
    ]
}
