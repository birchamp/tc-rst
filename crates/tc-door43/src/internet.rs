//! The app's internet switch (decision D95).
//!
//! The internet is one on/off state for the app session. Each launch starts with it off,
//! and nothing about it is stored: the type has no serde impls and no constructor that
//! starts it on. Every request a network client makes reads the switch at the moment it is
//! made, so turning it off refuses the next request before anything is sent.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The refusal for a request made while the internet is off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the internet is off for this session")]
pub struct Offline;

/// The session's internet switch. Clones share one state, so the app holds one switch and
/// hands clones to every network client.
#[derive(Debug, Clone, Default)]
pub struct InternetSwitch {
    on: Arc<AtomicBool>,
}

impl InternetSwitch {
    /// A new session's switch. It is off.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn turn_on(&self) {
        self.on.store(true, Ordering::SeqCst);
    }

    pub fn turn_off(&self) {
        self.on.store(false, Ordering::SeqCst);
    }

    pub fn is_on(&self) -> bool {
        self.on.load(Ordering::SeqCst)
    }

    /// `Ok` while the switch is on. A network client calls this before it sends anything.
    pub fn check(&self) -> Result<(), Offline> {
        if self.is_on() { Ok(()) } else { Err(Offline) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_switch_is_off_and_refuses() {
        let switch = InternetSwitch::new();
        assert!(!switch.is_on());
        assert_eq!(switch.check(), Err(Offline));
    }

    #[test]
    fn clones_share_one_state() {
        let switch = InternetSwitch::new();
        let held_by_client = switch.clone();
        switch.turn_on();
        assert_eq!(held_by_client.check(), Ok(()));
        switch.turn_off();
        assert_eq!(held_by_client.check(), Err(Offline));
    }
}
