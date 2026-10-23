//! The first-launch state `hello` answers and `Startup` events carry (APP-DESIGN §3.6, unit
//! A9).
//!
//! The state only moves forward: a `Preparing` count never falls, and `Ready` or `Failed` is
//! final for the run. A job's reporter sends its last `Running` frame after the job's own
//! end, so a late report can arrive after the state was set to `Ready`; moving only forward
//! is what keeps the window from showing "preparing" again after it was told to leave.

use std::sync::{Mutex, MutexGuard, PoisonError};

use tc_api::event::{Event, StartupProgress, StartupState};

use crate::events::EventBus;

/// How preparation ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StartupEnd {
    Ready,
    Failed(String),
}

/// What changed in the preparation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Update {
    Progress { done: u64, total: u64 },
    End(StartupEnd),
}

/// The next state, or `None` when the update changes nothing. Pure: `Preparing` counts never
/// fall, and `Ready` or `Failed` is final, so a late progress report never shows preparing
/// again after it ended.
pub(crate) fn advance(now: &StartupState, update: Update) -> Option<StartupState> {
    let StartupState::Preparing(preparing) = now else {
        return None;
    };
    match update {
        Update::Progress { done, total } => {
            let next = StartupProgress { done, total };
            (done >= preparing.done && next != *preparing).then_some(StartupState::Preparing(next))
        }
        Update::End(StartupEnd::Ready) => Some(StartupState::Ready),
        Update::End(StartupEnd::Failed(reason)) => Some(StartupState::Failed { reason }),
    }
}

/// The state, kept under one lock with its publication, so the last `Startup` frame always
/// equals what `hello` answers.
pub(crate) struct Startup {
    state: Mutex<StartupState>,
    bus: EventBus,
}

impl Startup {
    /// Starts in `state` and publishes nothing: [`Startup::announce`] does that for a state
    /// a page must see when it subscribes from the start.
    pub(crate) fn new(state: StartupState, bus: EventBus) -> Self {
        Self {
            state: Mutex::new(state),
            bus,
        }
    }

    fn locked(&self) -> MutexGuard<'_, StartupState> {
        // A store of a whole value is the only change made under this lock.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn get(&self) -> StartupState {
        self.locked().clone()
    }

    /// Publishes the state as it is now.
    pub(crate) fn announce(&self) {
        let state = self.locked();
        self.bus.publish(Event::Startup(state.clone()));
    }

    /// Applies `update` and publishes the new state when it changed anything. The bus never
    /// calls back, so it is safe to publish under the lock.
    pub(crate) fn update(&self, update: Update) -> bool {
        let mut state = self.locked();
        let Some(next) = advance(&state, update) else {
            return false;
        };
        *state = next.clone();
        self.bus.publish(Event::Startup(next));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preparing(done: u64, total: u64) -> StartupState {
        StartupState::Preparing(StartupProgress { done, total })
    }

    fn progress(done: u64, total: u64) -> Update {
        Update::Progress { done, total }
    }

    #[test]
    fn progress_only_grows() {
        let now = preparing(5, 10);
        assert_eq!(advance(&now, progress(6, 10)), Some(preparing(6, 10)));
        assert_eq!(advance(&now, progress(10, 10)), Some(preparing(10, 10)));
        assert_eq!(advance(&now, progress(4, 10)), None, "a count never falls");
        assert_eq!(advance(&now, progress(0, 10)), None);
    }

    #[test]
    fn ready_and_failed_are_final() {
        let failed = StartupState::Failed {
            reason: "disk full".to_owned(),
        };
        for end in [StartupState::Ready, failed] {
            for update in [
                progress(1, 10),
                Update::End(StartupEnd::Ready),
                Update::End(StartupEnd::Failed("again".to_owned())),
            ] {
                assert_eq!(advance(&end, update.clone()), None, "{end:?} {update:?}");
            }
        }
    }

    #[test]
    fn the_first_end_wins() {
        let now = preparing(3, 10);
        assert_eq!(
            advance(&now, Update::End(StartupEnd::Ready)),
            Some(StartupState::Ready)
        );
        let failed = advance(
            &now,
            Update::End(StartupEnd::Failed("it stopped".to_owned())),
        );
        assert_eq!(
            failed,
            Some(StartupState::Failed {
                reason: "it stopped".to_owned()
            })
        );
        // A job's last progress report, sent after the end, changes nothing.
        let ended = failed.unwrap();
        assert_eq!(advance(&ended, progress(10, 10)), None);
        assert_eq!(advance(&ended, Update::End(StartupEnd::Ready)), None);
    }

    #[test]
    fn a_repeated_count_changes_nothing() {
        let now = preparing(5, 10);
        assert_eq!(advance(&now, progress(5, 10)), None);
    }
}
