//! The steps of one sync (`docs/STORAGE.md` §9.1, S0 to S10) as one pure, total function.
//!
//! [`next`] takes the state and what the last action answered, and gives the next state and
//! the actions to take. The state records whether teammates' work was received in this sync,
//! so a failure after intake is [`SyncFailure::AfterIntake`] by construction: the person is
//! never told "nothing changed" once received work is in the project (PLAN Q8). No action
//! pushes with force: [`SyncAction::Push`] carries nothing, and the executor builds the one
//! refspec it may send from the branch name alone.

use tc_api::refusal::{SyncBlock, SyncFailure, SyncRefusal};

/// Pushes Door43 refused because teammates moved the branch first, before the sync stops
/// with [`SyncRefusal::Busy`] (STORAGE §9.5, Q11).
pub const MAX_ATTEMPTS: u8 = 6;

/// Whether this sync has taken teammates' work into the project yet (§9.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Received {
    Nothing,
    Integrated,
}

/// One round of fetch, intake, merge and push. A push that loses a race to a teammate starts
/// the next round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Attempt {
    /// From 1.
    pub n: u8,
    pub received: Received,
}

/// Where a sync is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncState {
    /// Not begun.
    Idle,
    /// S0: waiting for the gate.
    Gate,
    /// S1: waiting for the checkpoint "before sync".
    Checkpoint,
    /// S3: waiting for the fetch.
    Fetch(Attempt),
    /// S5: waiting for the reading of the remote's tree (§9.2).
    Intake(Attempt),
    /// S6: waiting for the store to take the received packs in (§9.3).
    Integrate(Attempt),
    /// S7: waiting for the merge commit.
    Merge(Attempt),
    /// S8: waiting for the push.
    Push(Attempt),
    /// Waiting out the pause before the next round, after Door43's branch moved.
    Backoff(Attempt),
    /// S9: waiting for the confirmation.
    Confirm(Received),
    /// S10: waiting for the repack.
    Repack { received: Received, confirmed: bool },
    /// The sync is over.
    Done(Ended),
}

/// How a sync ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ended {
    /// The project and Door43 hold each other's work. `confirmed`: Door43's branch was read
    /// back at the pushed commit ("Your work up to 14:32 is on Door43"); else "Sent, not
    /// confirmed".
    Synced {
        received: Received,
        confirmed: bool,
    },
    Failed(SyncFailure),
}

/// What the remote branch holds after a fetch, against this project's history (S4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Remote {
    /// No branch on Door43 yet, the same commit, or one this project's history holds.
    NothingNew,
    /// Commits this project does not hold.
    New,
    /// The repository has history but not the branch this project syncs: its owner deleted
    /// it. Nothing is pushed, so no new branch replaces the team's (§9.1, X step 5).
    BranchMissing,
}

/// What the last action answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncEvent {
    /// The person asked for a sync.
    Start,
    GatePassed,
    /// S0 refused: offline, not signed in, read-only, or the project has no repository.
    GateRefused(GateRefusal),
    CheckpointMade,
    CheckpointFailed {
        reason: String,
    },
    Fetched(Remote),
    FetchFailed(SyncRefusal),
    /// §9.2 read the remote's tree and found what to take in.
    RemoteRead,
    /// §9.2 N1 or N2 stopped the sync before anything was taken.
    RemoteBlocked(SyncBlock),
    RemoteUnreadable {
        reason: String,
    },
    TakenIn,
    TakeInFailed {
        reason: String,
    },
    Merged,
    MergeFailed {
        reason: String,
    },
    Pushed,
    /// Door43's branch moved since the fetch (`NotFastForward`, `RefMoved`).
    PushRaced,
    PushFailed(SyncRefusal),
    WaitOver,
    /// Door43's branch read back; `equal`: at the pushed commit.
    Confirmed {
        equal: bool,
    },
    ConfirmFailed,
    Repacked,
}

/// Why the gate refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateRefusal {
    Block(SyncBlock),
    Refusal(SyncRefusal),
}

/// What the executor does next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAction {
    Gate,
    Checkpoint,
    Fetch,
    ReadRemote,
    TakeIn,
    MergeCommit,
    /// A fast-forward push of the branch; never forced.
    Push,
    /// Wait before round `attempt`: `min(2^attempt, 16) s × U(0.7, 1.3)`, at most 30 s in all.
    Wait {
        attempt: u8,
    },
    Confirm,
    Repack,
    Report(Ended),
}

/// The next state, and what to do (STORAGE §9.1). Every state is listed; an answer a state
/// does not wait for ends the sync without a push.
pub fn next(state: &SyncState, event: &SyncEvent) -> (SyncState, Vec<SyncAction>) {
    use SyncEvent as E;
    use SyncState as S;
    match state {
        S::Idle => match event {
            E::Start => (S::Gate, vec![SyncAction::Gate]),
            other => unexpected(Received::Nothing, other),
        },
        S::Gate => match event {
            E::GatePassed => (S::Checkpoint, vec![SyncAction::Checkpoint]),
            E::GateRefused(GateRefusal::Block(block)) => {
                end(Ended::Failed(SyncFailure::Blocked(block.clone())))
            }
            E::GateRefused(GateRefusal::Refusal(refusal)) => {
                fail(Received::Nothing, refusal.clone())
            }
            other => unexpected(Received::Nothing, other),
        },
        S::Checkpoint => match event {
            E::CheckpointMade => fetch(first()),
            E::CheckpointFailed { reason } => fail(
                Received::Nothing,
                SyncRefusal::Io {
                    reason: reason.clone(),
                },
            ),
            other => unexpected(Received::Nothing, other),
        },
        S::Fetch(at) => match event {
            E::Fetched(Remote::NothingNew) => (S::Push(*at), vec![SyncAction::Push]),
            E::Fetched(Remote::New) => (S::Intake(*at), vec![SyncAction::ReadRemote]),
            E::Fetched(Remote::BranchMissing) => match at.received {
                Received::Nothing => end(Ended::Failed(SyncFailure::Blocked(
                    SyncBlock::BranchMissing,
                ))),
                Received::Integrated => fail(
                    Received::Integrated,
                    SyncRefusal::Io {
                        reason: "Door43's branch was deleted during the sync".into(),
                    },
                ),
            },
            E::FetchFailed(refusal) => fail(at.received, refusal.clone()),
            other => unexpected(at.received, other),
        },
        S::Intake(at) => match event {
            E::RemoteRead => (S::Integrate(*at), vec![SyncAction::TakeIn]),
            E::RemoteBlocked(block) => match at.received {
                Received::Nothing => end(Ended::Failed(SyncFailure::Blocked(block.clone()))),
                // An earlier round took work in: it stays, and the person is told so.
                Received::Integrated => fail(
                    Received::Integrated,
                    SyncRefusal::Io {
                        reason: format!("{block:?}"),
                    },
                ),
            },
            E::RemoteUnreadable { reason } => fail(
                at.received,
                SyncRefusal::Io {
                    reason: reason.clone(),
                },
            ),
            other => unexpected(at.received, other),
        },
        // From here the store holds the received work: whatever fails, it was integrated.
        S::Integrate(at) => match event {
            E::TakenIn => {
                let at = Attempt {
                    received: Received::Integrated,
                    ..*at
                };
                (S::Merge(at), vec![SyncAction::MergeCommit])
            }
            // A take-in that failed is whole or absent (§9.3): what an earlier round took
            // stays, and this round's is not in the fold.
            E::TakeInFailed { reason } => fail(
                at.received,
                SyncRefusal::Io {
                    reason: reason.clone(),
                },
            ),
            other => unexpected(at.received, other),
        },
        S::Merge(at) => match event {
            E::Merged => (S::Push(*at), vec![SyncAction::Push]),
            E::MergeFailed { reason } => fail(
                at.received,
                SyncRefusal::Io {
                    reason: reason.clone(),
                },
            ),
            other => unexpected(at.received, other),
        },
        S::Push(at) => match event {
            E::Pushed => (S::Confirm(at.received), vec![SyncAction::Confirm]),
            E::PushRaced if at.n >= MAX_ATTEMPTS => fail(
                at.received,
                SyncRefusal::Busy {
                    attempts: MAX_ATTEMPTS,
                },
            ),
            E::PushRaced => {
                let again = Attempt { n: at.n + 1, ..*at };
                (
                    S::Backoff(again),
                    vec![SyncAction::Wait { attempt: again.n }],
                )
            }
            E::PushFailed(refusal) => fail(at.received, refusal.clone()),
            other => unexpected(at.received, other),
        },
        S::Backoff(at) => match event {
            E::WaitOver => fetch(*at),
            other => unexpected(at.received, other),
        },
        S::Confirm(received) => match event {
            E::Confirmed { equal } => (
                S::Repack {
                    received: *received,
                    confirmed: *equal,
                },
                vec![SyncAction::Repack],
            ),
            E::ConfirmFailed => (
                S::Repack {
                    received: *received,
                    confirmed: false,
                },
                vec![SyncAction::Repack],
            ),
            other => unexpected(*received, other),
        },
        S::Repack {
            received,
            confirmed,
        } => match event {
            E::Repacked => end(Ended::Synced {
                received: *received,
                confirmed: *confirmed,
            }),
            other => unexpected(*received, other),
        },
        S::Done(_) => (state.clone(), Vec::new()),
    }
}

fn first() -> Attempt {
    Attempt {
        n: 1,
        received: Received::Nothing,
    }
}

fn fetch(at: Attempt) -> (SyncState, Vec<SyncAction>) {
    (SyncState::Fetch(at), vec![SyncAction::Fetch])
}

fn end(ended: Ended) -> (SyncState, Vec<SyncAction>) {
    (
        SyncState::Done(ended.clone()),
        vec![SyncAction::Report(ended)],
    )
}

/// A failure, on the side of intake the sync is on.
fn fail(received: Received, refusal: SyncRefusal) -> (SyncState, Vec<SyncAction>) {
    end(Ended::Failed(match received {
        Received::Nothing => SyncFailure::BeforeIntake(refusal),
        Received::Integrated => SyncFailure::AfterIntake(refusal),
    }))
}

/// An answer the state does not wait for is a bug of the executor: the sync stops, nothing
/// more is sent, and the report says what came.
fn unexpected(received: Received, event: &SyncEvent) -> (SyncState, Vec<SyncAction>) {
    fail(
        received,
        SyncRefusal::Io {
            reason: format!("the sync got {event:?} at the wrong step"),
        },
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn attempts() -> Vec<Attempt> {
        let mut out = Vec::new();
        for n in [1, 2, MAX_ATTEMPTS] {
            for received in [Received::Nothing, Received::Integrated] {
                out.push(Attempt { n, received });
            }
        }
        out
    }

    fn states() -> Vec<SyncState> {
        let mut out = vec![SyncState::Idle, SyncState::Gate, SyncState::Checkpoint];
        for at in attempts() {
            out.extend([
                SyncState::Fetch(at),
                SyncState::Intake(at),
                SyncState::Integrate(at),
                SyncState::Merge(at),
                SyncState::Push(at),
                SyncState::Backoff(at),
            ]);
        }
        for received in [Received::Nothing, Received::Integrated] {
            out.push(SyncState::Confirm(received));
            for confirmed in [false, true] {
                out.push(SyncState::Repack {
                    received,
                    confirmed,
                });
            }
            out.push(SyncState::Done(Ended::Synced {
                received,
                confirmed: true,
            }));
        }
        out
    }

    fn events() -> Vec<SyncEvent> {
        let refusal = SyncRefusal::Unreachable;
        vec![
            SyncEvent::Start,
            SyncEvent::GatePassed,
            SyncEvent::GateRefused(GateRefusal::Block(SyncBlock::NotShared)),
            SyncEvent::GateRefused(GateRefusal::Refusal(SyncRefusal::Offline)),
            SyncEvent::CheckpointMade,
            SyncEvent::CheckpointFailed {
                reason: "disk full".into(),
            },
            SyncEvent::Fetched(Remote::NothingNew),
            SyncEvent::Fetched(Remote::New),
            SyncEvent::Fetched(Remote::BranchMissing),
            SyncEvent::FetchFailed(refusal.clone()),
            SyncEvent::RemoteRead,
            SyncEvent::RemoteBlocked(SyncBlock::ProjectMismatch),
            SyncEvent::RemoteUnreadable {
                reason: "bad tree".into(),
            },
            SyncEvent::TakenIn,
            SyncEvent::TakeInFailed {
                reason: "disk full".into(),
            },
            SyncEvent::Merged,
            SyncEvent::MergeFailed {
                reason: "disk full".into(),
            },
            SyncEvent::Pushed,
            SyncEvent::PushRaced,
            SyncEvent::PushFailed(refusal),
            SyncEvent::WaitOver,
            SyncEvent::Confirmed { equal: true },
            SyncEvent::Confirmed { equal: false },
            SyncEvent::ConfirmFailed,
            SyncEvent::Repacked,
        ]
    }

    fn received_of(state: &SyncState) -> Option<Received> {
        match state {
            SyncState::Idle | SyncState::Gate | SyncState::Checkpoint => Some(Received::Nothing),
            SyncState::Fetch(at)
            | SyncState::Intake(at)
            | SyncState::Integrate(at)
            | SyncState::Merge(at)
            | SyncState::Push(at)
            | SyncState::Backoff(at) => Some(at.received),
            SyncState::Confirm(received) | SyncState::Repack { received, .. } => Some(*received),
            SyncState::Done(_) => None,
        }
    }

    /// Every state and every answer: a failure after intake is never "nothing changed", a
    /// failure before it is never "kept", the sync stops after `MAX_ATTEMPTS` pushes, the
    /// report comes exactly once, and a done sync does nothing more.
    #[test]
    fn sync_next_is_total_over_every_state_and_event() {
        for state in states() {
            for event in events() {
                let (after, actions) = next(&state, &event);
                let was = received_of(&state);
                if let SyncState::Done(Ended::Failed(failure)) = &after
                    && !matches!(state, SyncState::Done(_))
                {
                    match (was, failure) {
                        (Some(Received::Integrated), SyncFailure::AfterIntake(_)) => {}
                        (Some(Received::Integrated), other) => panic!(
                            "INV-RECEIVED-WORK-IS-KEPT-WHOLE-OR-ABSENT: {state:?} + {event:?} ended {other:?}, but work was taken in"
                        ),
                        (Some(Received::Nothing), SyncFailure::AfterIntake(_))
                            if !matches!(state, SyncState::Integrate(_)) =>
                        {
                            panic!(
                                "{state:?} + {event:?} says work was kept, but none was taken in"
                            )
                        }
                        _ => {}
                    }
                }
                let reports = actions
                    .iter()
                    .filter(|a| matches!(a, SyncAction::Report(_)))
                    .count();
                match (&state, &after) {
                    (SyncState::Done(_), _) => {
                        assert!(actions.is_empty(), "{state:?} + {event:?} did {actions:?}");
                        assert_eq!(after, state);
                    }
                    (_, SyncState::Done(ended)) => {
                        assert_eq!(actions, vec![SyncAction::Report(ended.clone())]);
                    }
                    _ => assert_eq!(reports, 0, "{state:?} + {event:?}"),
                }
                for action in &actions {
                    if let SyncAction::Wait { attempt } = action {
                        assert!(*attempt <= MAX_ATTEMPTS, "{state:?} + {event:?}");
                    }
                }
                if let SyncState::Backoff(at) | SyncState::Fetch(at) = &after {
                    assert!(at.n <= MAX_ATTEMPTS, "{state:?} + {event:?}");
                }
            }
        }
    }

    /// The sixth race ends the sync as busy, on the side of intake it happened.
    #[test]
    fn six_raced_pushes_end_as_busy() {
        for received in [Received::Nothing, Received::Integrated] {
            let (after, _) = next(
                &SyncState::Push(Attempt {
                    n: MAX_ATTEMPTS,
                    received,
                }),
                &SyncEvent::PushRaced,
            );
            let busy = SyncRefusal::Busy {
                attempts: MAX_ATTEMPTS,
            };
            let want = match received {
                Received::Nothing => SyncFailure::BeforeIntake(busy),
                Received::Integrated => SyncFailure::AfterIntake(busy),
            };
            assert_eq!(after, SyncState::Done(Ended::Failed(want)));
        }
    }
}
