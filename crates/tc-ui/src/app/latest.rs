//! Which attempt is the newest (UI-1, INV-A-STALE-REPLY-NEVER-SHOWS): a screen begins an
//! attempt for each read or open it asks for, and a reply lands only while its attempt is
//! still the newest, so a slow reply to an older click never replaces a newer one.

use std::cell::Cell;

/// The newest attempt of one kind of request.
#[derive(Debug, Default)]
pub struct Latest(Cell<u64>);

/// One attempt; only [`Latest`] makes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attempt(u64);

impl Latest {
    pub const fn new() -> Self {
        Self(Cell::new(0))
    }

    /// A new attempt, which makes every earlier one stale.
    pub fn begin(&self) -> Attempt {
        let next = self.0.get().wrapping_add(1);
        self.0.set(next);
        Attempt(next)
    }

    /// The newest attempt, for work that belongs to it without replacing it.
    pub fn current(&self) -> Attempt {
        Attempt(self.0.get())
    }

    pub fn is_current(&self, attempt: Attempt) -> bool {
        self.0.get() == attempt.0
    }
}
