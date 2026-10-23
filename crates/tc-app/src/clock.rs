//! The wall clock, as an injected value. It stamps what people or reports see (`at`,
//! `startedAt`). Timers (debounces, the job reporter, the retention of a finished job) use
//! `tokio::time` instead, so a test can pause and advance them.

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tc_api::ids::WallMs;

pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> WallMs;
}

/// This computer's clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> WallMs {
        // A clock set before 1970 reads as 1970, and one past `i64::MAX` ms as that maximum:
        // either way a stamp is only shown, never used to order a person's work.
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_millis());
        WallMs(i64::try_from(ms).unwrap_or(i64::MAX))
    }
}

/// A clock a test sets.
#[derive(Debug, Default)]
pub struct ManualClock(AtomicI64);

impl ManualClock {
    pub fn new(ms: i64) -> Self {
        Self(AtomicI64::new(ms))
    }

    pub fn set(&self, ms: i64) {
        self.0.store(ms, Ordering::SeqCst);
    }

    pub fn advance(&self, ms: i64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> WallMs {
        WallMs(self.0.load(Ordering::SeqCst))
    }
}
