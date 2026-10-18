//! The app's internet switch (decision D95).
//!
//! The internet is one on/off state for the app session. Each launch starts with it off,
//! and nothing about it is stored: the type has no serde impls and no constructor that
//! starts it on. Every request a network client makes reads the switch at the moment it is
//! made, so turning it off refuses the next request before anything is sent.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// The refusal for a request made while the internet is off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the internet is off for this session")]
pub struct Offline;

/// The session's internet switch. Clones share one state, so the app holds one switch and
/// hands clones to every network client.
#[derive(Debug, Clone, Default)]
pub struct InternetSwitch {
    /// Twice the number of times the switch was turned on, plus one while it is on: the
    /// state and the count change together, so a reader never sees one without the other.
    state: Arc<AtomicU64>,
}

impl InternetSwitch {
    /// A new session's switch. It is off.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn turn_on(&self) {
        self.flip(false);
    }

    pub fn turn_off(&self) {
        self.flip(true);
    }

    pub fn is_on(&self) -> bool {
        self.state.load(Ordering::SeqCst) & 1 == 1
    }

    /// How many times the switch has gone from off to on in this session. A client that
    /// remembers something about the network (which proxy serves a server) asks again when
    /// this has changed, because the computer may have moved to another network meanwhile.
    pub fn times_turned_on(&self) -> u64 {
        self.state.load(Ordering::SeqCst).div_ceil(2)
    }

    /// `Ok` while the switch is on. A network client calls this before it sends anything.
    pub fn check(&self) -> Result<(), Offline> {
        if self.is_on() { Ok(()) } else { Err(Offline) }
    }

    /// Moves to the other state when the switch is in `from_on`'s state; otherwise does
    /// nothing.
    fn flip(&self, from_on: bool) {
        let _ = self
            .state
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |state| {
                ((state & 1 == 1) == from_on).then_some(state + 1)
            });
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
    fn it_counts_each_time_it_goes_from_off_to_on() {
        let switch = InternetSwitch::new();
        assert_eq!(switch.times_turned_on(), 0);
        switch.turn_off();
        switch.turn_on();
        switch.turn_on();
        assert_eq!((switch.is_on(), switch.times_turned_on()), (true, 1));
        switch.turn_off();
        switch.turn_off();
        assert_eq!((switch.is_on(), switch.times_turned_on()), (false, 1));
        let clone = switch.clone();
        clone.turn_on();
        assert_eq!((switch.is_on(), switch.times_turned_on()), (true, 2));
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
