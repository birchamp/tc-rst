//! A test-only watchdog for APP-DESIGN §8.3: it asks the runtime, every few milliseconds,
//! how late a sleep woke, and keeps the worst. A task that blocks a worker thread (a
//! synchronous disk read, a lock held across a long call) shows as a late wake.
//!
//! It also times a plain thread's sleep, as a control: that lateness is the machine's, not
//! the server's, and a test allows it on top of its bound, so a loaded computer does not fail
//! a test for what the operating system did.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub struct Watchdog {
    /// Microseconds.
    worst_runtime: Arc<AtomicU64>,
    worst_os: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
}

fn micros(late: Duration) -> u64 {
    u64::try_from(late.as_micros()).unwrap_or(u64::MAX)
}

impl Watchdog {
    /// Pings `runtime` every `every` from a task, and the operating system from a plain thread,
    /// and keeps the worst lateness of each: how much later than due a sleep of `every` woke.
    pub fn start(runtime: &tokio::runtime::Handle, every: Duration) -> Watchdog {
        let worst_runtime = Arc::new(AtomicU64::new(0));
        let worst_os = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));

        let (worst, halt) = (Arc::clone(&worst_runtime), Arc::clone(&stop));
        runtime.spawn(async move {
            while !halt.load(Ordering::Relaxed) {
                let due = Instant::now() + every;
                tokio::time::sleep(every).await;
                worst.fetch_max(
                    micros(Instant::now().saturating_duration_since(due)),
                    Ordering::Relaxed,
                );
            }
        });

        let (worst, halt) = (Arc::clone(&worst_os), Arc::clone(&stop));
        std::thread::spawn(move || {
            while !halt.load(Ordering::Relaxed) {
                let due = Instant::now() + every;
                std::thread::sleep(every);
                worst.fetch_max(
                    micros(Instant::now().saturating_duration_since(due)),
                    Ordering::Relaxed,
                );
            }
        });

        Watchdog {
            worst_runtime,
            worst_os,
            stop,
        }
    }

    /// The worst lateness of the runtime's sleeps so far.
    pub fn worst_runtime(&self) -> Duration {
        Duration::from_micros(self.worst_runtime.load(Ordering::Relaxed))
    }

    /// The control: how late a plain thread's sleep woke, which the machine alone causes.
    pub fn worst_os(&self) -> Duration {
        Duration::from_micros(self.worst_os.load(Ordering::Relaxed))
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
