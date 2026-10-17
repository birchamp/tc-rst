//! Determinate progress (PERFORMANCE.md rule 4, `docs/STORAGE.md` §6.1 "Progress").
//!
//! The working threads count into shared counters; a reporter thread, never the caller's or
//! the screen's, sends the first report 300 ms after the action started, whatever a read is
//! still waiting for, then at most every 100 ms while a count moves. An action that ends
//! sooner sends nothing, so a quick action never flashes an indicator. Counts only grow
//! within a phase. [`Tracker::finish`] sends the last report, with every count of the last
//! phase at its total, before it returns, so the last report always precedes "usable"; an
//! action that fails ends the same way before its error is returned.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The open's phases, in order (§6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Phase {
    /// O2 to O7: temporary files, the lock, the format gate, the device.
    Finishing,
    /// O8: files and bytes read, of the totals the listing gave.
    Reading,
    /// The fold's stages after the last file.
    Folding,
    /// O9: paths classified.
    Checking,
    /// O10: paths written.
    Writing,
    /// Any other background action: a seal, a materialization.
    Working,
}

/// One report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub phase: Phase,
    pub done: u64,
    pub total: u64,
    /// For [`Phase::Reading`]: bytes read, of the bytes listed.
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// When the report was made, after the action started.
    pub at: Duration,
}

/// Where reports go. Called on the reporter thread.
pub type ProgressSink = Arc<dyn Fn(Progress) + Send + Sync>;

/// A sink that drops every report.
pub fn no_progress() -> ProgressSink {
    Arc::new(|_| {})
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Counts {
    phase: Phase,
    done: u64,
    total: u64,
    bytes_done: u64,
    bytes_total: u64,
}

#[derive(Debug)]
struct Shared {
    counts: Mutex<Counts>,
    finished: AtomicBool,
    wake: Condvar,
    wake_lock: Mutex<()>,
}

/// The counters an action's threads count into (see the module documentation).
#[derive(Debug, Clone)]
pub struct Tracker {
    shared: Arc<Shared>,
}

/// The first report comes this long after the action started.
pub const FIRST_REPORT: Duration = Duration::from_millis(300);
/// Then at most this often.
pub const REPORT_EVERY: Duration = Duration::from_millis(100);

/// Starts counting an action that began at `started`, with a reporter thread sending to
/// `sink`.
pub fn start(sink: ProgressSink, started: Instant) -> (Tracker, Reporter) {
    let shared = Arc::new(Shared {
        counts: Mutex::new(Counts {
            phase: Phase::Finishing,
            done: 0,
            total: 1,
            bytes_done: 0,
            bytes_total: 0,
        }),
        finished: AtomicBool::new(false),
        wake: Condvar::new(),
        wake_lock: Mutex::new(()),
    });
    let thread_shared = shared.clone();
    let thread = std::thread::Builder::new()
        .name("tc-store progress".into())
        .spawn(move || report_loop(&thread_shared, &sink, started))
        .ok();
    (Tracker { shared }, Reporter { thread })
}

fn report_loop(shared: &Shared, sink: &ProgressSink, started: Instant) {
    let wait_until = |deadline: Instant| {
        let mut guard = shared.wake_lock.lock().unwrap_or_else(|e| e.into_inner());
        while !shared.finished.load(Ordering::Acquire) {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            guard = shared
                .wake
                .wait_timeout(guard, deadline - now)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    };
    wait_until(started + FIRST_REPORT);
    if shared.finished.load(Ordering::Acquire) && started.elapsed() < FIRST_REPORT {
        return;
    }
    let mut last: Option<Counts> = None;
    loop {
        let finished = shared.finished.load(Ordering::Acquire);
        let mut counts = *shared.counts.lock().unwrap_or_else(|e| e.into_inner());
        if finished {
            // The last report: the last phase complete.
            counts.done = counts.total;
            counts.bytes_done = counts.bytes_total;
        }
        if let Some(before) = last
            && before.phase != counts.phase
            && before.done < before.total
        {
            // A phase that ended between two reports is reported complete before the next.
            sink(Progress {
                phase: before.phase,
                done: before.total,
                total: before.total,
                bytes_done: before.bytes_total.max(before.bytes_done),
                bytes_total: before.bytes_total.max(before.bytes_done),
                at: started.elapsed(),
            });
        }
        if last != Some(counts) {
            sink(Progress {
                phase: counts.phase,
                done: counts.done,
                total: counts.total,
                bytes_done: counts.bytes_done,
                bytes_total: counts.bytes_total,
                at: started.elapsed(),
            });
            last = Some(counts);
        }
        if finished {
            return;
        }
        wait_until(Instant::now() + REPORT_EVERY);
    }
}

impl Tracker {
    /// A tracker whose counts nobody reads (an action with no indicator).
    pub fn none() -> Self {
        Self {
            shared: Arc::new(Shared {
                counts: Mutex::new(Counts {
                    phase: Phase::Working,
                    done: 0,
                    total: 1,
                    bytes_done: 0,
                    bytes_total: 0,
                }),
                finished: AtomicBool::new(true),
                wake: Condvar::new(),
                wake_lock: Mutex::new(()),
            }),
        }
    }

    /// Starts phase `phase` of `total` steps (and `bytes_total` bytes for reading).
    pub fn phase(&self, phase: Phase, total: u64, bytes_total: u64) {
        let mut counts = self.shared.counts.lock().unwrap_or_else(|e| e.into_inner());
        *counts = Counts {
            phase,
            done: 0,
            total: total.max(1),
            bytes_done: 0,
            bytes_total,
        };
    }

    /// `n` more steps and `bytes` more bytes of the current phase are done.
    pub fn add(&self, n: u64, bytes: u64) {
        let mut counts = self.shared.counts.lock().unwrap_or_else(|e| e.into_inner());
        counts.done = (counts.done + n).min(counts.total);
        // A file that grew after the listing never takes the count past the listed total.
        counts.bytes_done = (counts.bytes_done + bytes).min(counts.bytes_total);
    }

    /// `done` steps of the current phase are done (never fewer than counted before).
    pub fn set(&self, done: u64) {
        let mut counts = self.shared.counts.lock().unwrap_or_else(|e| e.into_inner());
        counts.done = counts.done.max(done.min(counts.total));
    }
}

/// The reporter thread of an action.
#[derive(Debug)]
pub struct Reporter {
    thread: Option<JoinHandle<()>>,
}

impl Reporter {
    /// Ends the action: the reporter sends its last report, if it reported at all, and
    /// stops before this returns.
    pub fn finish(mut self, tracker: &Tracker) {
        tracker.shared.finished.store(true, Ordering::Release);
        {
            let _guard = tracker
                .shared
                .wake_lock
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            tracker.shared.wake.notify_all();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Seen = Arc<Mutex<Vec<(Progress, std::thread::ThreadId)>>>;

    fn collecting() -> (ProgressSink, Seen) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let into = seen.clone();
        let sink: ProgressSink = Arc::new(move |p| {
            into.lock().unwrap().push((p, std::thread::current().id()));
        });
        (sink, seen)
    }

    #[test]
    fn a_quick_action_reports_nothing() {
        let (sink, seen) = collecting();
        let (tracker, reporter) = start(sink, Instant::now());
        tracker.phase(Phase::Reading, 10, 0);
        tracker.add(10, 0);
        reporter.finish(&tracker);
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn a_slow_action_reports_from_300_ms_and_last_before_it_ends() {
        let (sink, seen) = collecting();
        let started = Instant::now();
        let (tracker, reporter) = start(sink, started);
        tracker.phase(Phase::Reading, 4, 400);
        for _ in 0..4 {
            std::thread::sleep(Duration::from_millis(150));
            tracker.add(1, 100);
        }
        tracker.phase(Phase::Folding, 2, 0);
        tracker.add(1, 0);
        reporter.finish(&tracker);
        let seen = seen.lock().unwrap();
        assert!(!seen.is_empty());
        let first = &seen[0].0;
        assert!(first.at >= FIRST_REPORT && first.at < Duration::from_millis(400));
        assert!(
            seen.iter()
                .all(|(_, id)| *id != std::thread::current().id())
        );
        let last = &seen.last().unwrap().0;
        assert_eq!((last.phase, last.done, last.total), (Phase::Folding, 2, 2));
        for pair in seen.windows(2) {
            let (a, b) = (&pair[0].0, &pair[1].0);
            assert!(a.phase < b.phase || (a.phase == b.phase && a.done <= b.done));
        }
    }
}
