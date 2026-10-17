//! The author's clock (`docs/STORAGE.md` §2.1 `WallTime`): it stamps `at` on changes and
//! `sealed` on packs, and never decides what survives (I3).

use std::sync::atomic::{AtomicI64, Ordering};
use tc_journal::WallTime;

pub trait Clock: Send + Sync + std::fmt::Debug {
    /// Milliseconds since the Unix epoch, UTC.
    fn now(&self) -> WallTime;
}

/// This computer's clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> WallTime {
        let ms = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
            Err(e) => -i64::try_from(e.duration().as_millis()).unwrap_or(i64::MAX),
        };
        WallTime(ms)
    }
}

/// A clock a test sets: it moves only when told, and one millisecond at each reading, so
/// two changes never share a time by accident.
#[derive(Debug, Default)]
pub struct ManualClock(AtomicI64);

impl ManualClock {
    pub fn at(ms: i64) -> Self {
        Self(AtomicI64::new(ms))
    }

    pub fn advance(&self, ms: i64) {
        self.0.fetch_add(ms, Ordering::Relaxed);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> WallTime {
        WallTime(self.0.fetch_add(1, Ordering::Relaxed))
    }
}
