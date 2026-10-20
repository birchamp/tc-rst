//! The server's save of an open aligner's links as a state machine (APP-DESIGN AD18, A-4).
//!
//! The links are the translator's work, so the rules are in one place and proved by a
//! model-based test (`tests/app/align_save.rs`): a change is never forgotten, a save that
//! fails leaves the links unsaved and is tried again, a change made while a save runs is saved
//! after it, and only a save that stored the newest version makes the links "Saved".
//!
//! [`next`] is pure. The actor (`actor.rs`) carries the effects out: it keeps the timer, sends
//! the command to the store and feeds the outcome back as an event.
//!
//! *Versions.* The actor counts a version for every edit that changes the links; the machine
//! never reads the links.

use std::time::Duration;

use tc_api::align::AlignSave;
use tc_api::write::NotSaved;

/// How long after the last change the links are saved (tC4's align scheduler; AD18).
pub(super) const SAVE_AFTER: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum State {
    /// The store holds what the aligner shows.
    Clean,
    /// The aligner opened with a lossless carry-over of the stored links (AD19): it is saved
    /// when the verse is left or flushed, not on a timer, and it shows as saved, because the
    /// translator has changed nothing. Version `at` is the one to save.
    Carried { at: u64 },
    /// Changed up to version `latest`; the timer of generation `timer` is running.
    Dirty { latest: u64, timer: u64 },
    /// The save of version `sent` is on its way; `latest` is the newest version.
    Saving { sent: u64, latest: u64 },
    /// The last save failed. The links stay unsaved; the next change, a flush or leaving the
    /// verse tries again.
    Failed { latest: u64, reason: NotSaved },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Machine {
    pub state: State,
    /// Numbers the timers, so a tick of a timer that was restarted is ignored.
    pub timer: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Event {
    /// The links changed; `version` is the new version, larger than every earlier one.
    Changed { version: u64 },
    /// The timer of this generation ran out.
    Tick { timer: u64 },
    /// Save now: the navigation gate, leaving the verse, replacing or closing the session,
    /// the quit.
    Flush,
    /// The save that was started ended.
    Done(Result<(), NotSaved>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Effect {
    /// Start the timer of this generation, in place of any that runs.
    Arm {
        timer: u64,
    },
    Disarm,
    /// Send the links as of this version to the store.
    Start {
        version: u64,
    },
    /// The store holds the newest version: the aligner no longer counts as changed.
    MarkSaved,
}

impl Machine {
    pub(super) fn clean() -> Self {
        Self {
            state: State::Clean,
            timer: 0,
        }
    }

    /// A verse that opened with a carry-over to save when it is left.
    pub(super) fn carried(version: u64) -> Self {
        Self {
            state: State::Carried { at: version },
            timer: 0,
        }
    }

    /// What the indicator and the close guard count.
    pub(super) fn visible(&self) -> AlignSave {
        match &self.state {
            State::Clean | State::Carried { .. } => AlignSave::Saved,
            State::Dirty { .. } => AlignSave::Unsaved,
            State::Saving { .. } => AlignSave::Saving,
            State::Failed { reason, .. } => AlignSave::Failed(reason.clone()),
        }
    }

    /// Whether a flush has finished: nothing waits to be saved, or the last save failed and
    /// the answer is that failure.
    pub(super) fn settled(&self) -> Option<AlignSave> {
        match &self.state {
            State::Clean => Some(AlignSave::Saved),
            State::Failed { reason, .. } => Some(AlignSave::Failed(reason.clone())),
            State::Carried { .. } | State::Dirty { .. } | State::Saving { .. } => None,
        }
    }
}

/// The machine after `event`, and what to do about it.
pub(super) fn next(machine: Machine, event: Event) -> (Machine, Vec<Effect>) {
    let Machine { state, timer } = machine;
    match (state, event) {
        // A change restarts the timer, unless a save is on its way: that one ends first.
        (State::Saving { sent, .. }, Event::Changed { version }) => (
            Machine {
                state: State::Saving {
                    sent,
                    latest: version,
                },
                timer,
            },
            vec![],
        ),
        (
            State::Clean | State::Carried { .. } | State::Dirty { .. } | State::Failed { .. },
            Event::Changed { version },
        ) => {
            let timer = timer + 1;
            (
                Machine {
                    state: State::Dirty {
                        latest: version,
                        timer,
                    },
                    timer,
                },
                vec![Effect::Arm { timer }],
            )
        }

        (
            State::Dirty {
                latest,
                timer: armed,
            },
            Event::Tick { timer: ticked },
        ) if armed == ticked => (
            Machine {
                state: State::Saving {
                    sent: latest,
                    latest,
                },
                timer,
            },
            vec![Effect::Start { version: latest }],
        ),
        // A tick of a timer that was restarted, or after the save began.
        (state, Event::Tick { .. }) => (Machine { state, timer }, vec![]),

        (State::Carried { at: latest }, Event::Flush)
        | (State::Failed { latest, .. }, Event::Flush) => (
            Machine {
                state: State::Saving {
                    sent: latest,
                    latest,
                },
                timer,
            },
            vec![Effect::Start { version: latest }],
        ),
        (State::Dirty { latest, .. }, Event::Flush) => (
            Machine {
                state: State::Saving {
                    sent: latest,
                    latest,
                },
                timer,
            },
            vec![Effect::Disarm, Effect::Start { version: latest }],
        ),
        (state @ (State::Clean | State::Saving { .. }), Event::Flush) => {
            (Machine { state, timer }, vec![])
        }

        (State::Saving { sent, latest }, Event::Done(Ok(()))) if sent == latest => (
            Machine {
                state: State::Clean,
                timer,
            },
            vec![Effect::MarkSaved],
        ),
        // Changed while the save ran: what was sent is stored, the newer links wait.
        (State::Saving { latest, .. }, Event::Done(Ok(()))) => {
            let timer = timer + 1;
            (
                Machine {
                    state: State::Dirty { latest, timer },
                    timer,
                },
                vec![Effect::Arm { timer }],
            )
        }
        (State::Saving { latest, .. }, Event::Done(Err(reason))) => (
            Machine {
                state: State::Failed { latest, reason },
                timer,
            },
            vec![],
        ),
        // An outcome with no save on its way: the actor starts one save at a time.
        (state, Event::Done(_)) => (Machine { state, timer }, vec![]),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    //! A model of the store against the machine, over random histories of edits, ticks,
    //! flushes and outcomes: whatever the order, a change is never forgotten, a failed save
    //! is tried again, and "Saved" means the store holds the newest links. Each bug the
    //! machine could have is put back to show the test catches it.

    use proptest::prelude::*;
    use proptest::test_runner::{Config, TestError, TestRunner};

    use super::*;

    type Next = fn(Machine, Event) -> (Machine, Vec<Effect>);

    #[derive(Debug, Clone)]
    enum Act {
        /// The person changes the links.
        Edit,
        /// A timer runs out: the newest one, or an older one that was restarted.
        Tick {
            stale: bool,
        },
        Flush,
        /// The save in flight ends: the store has it, or the disk refuses.
        Finish {
            ok: bool,
        },
        /// The app is killed: the machine is gone, the store keeps what it has.
        Crash,
    }

    fn acts() -> impl Strategy<Value = Vec<Act>> {
        let act = prop_oneof![
            4 => Just(Act::Edit),
            2 => any::<bool>().prop_map(|stale| Act::Tick { stale }),
            2 => Just(Act::Flush),
            4 => any::<bool>().prop_map(|ok| Act::Finish { ok }),
            1 => Just(Act::Crash),
        ];
        proptest::collection::vec(act, 0..60)
    }

    /// The world the machine acts in.
    struct World {
        machine: Machine,
        /// The newest version of the links.
        latest: u64,
        /// The version the store holds.
        stored: u64,
        /// The save in flight, by version.
        inflight: Option<u64>,
        /// The version at which the aligner opened with a carry-over to save, if any.
        carried: bool,
    }

    impl World {
        fn new(carried: bool) -> Self {
            Self {
                machine: if carried {
                    Machine::carried(0)
                } else {
                    Machine::clean()
                },
                latest: 0,
                stored: 0,
                inflight: None,
                carried,
            }
        }

        fn feed(&mut self, next: Next, event: Event) -> Result<(), String> {
            let (machine, effects) = next(self.machine.clone(), event);
            self.machine = machine;
            for effect in effects {
                match effect {
                    Effect::Start { version } => {
                        if self.inflight.is_some() {
                            return Err("a second save started while one ran".into());
                        }
                        if version != self.latest {
                            return Err(format!(
                                "a save of version {version} started when {} was the newest",
                                self.latest
                            ));
                        }
                        self.inflight = Some(version);
                    }
                    Effect::Arm { .. } | Effect::Disarm | Effect::MarkSaved => {}
                }
            }
            self.check()
        }

        fn check(&self) -> Result<(), String> {
            let saved = self.machine.visible() == AlignSave::Saved;
            let held = self.stored == self.latest;
            let untouched = self.carried && self.latest == 0;
            if matches!(self.machine.state, State::Clean) && !held && !untouched {
                return Err(format!(
                    "Clean, but the store holds {} of {}",
                    self.stored, self.latest
                ));
            }
            if saved && !held && !untouched {
                return Err(format!(
                    "shown as Saved, but the store holds {} of {}",
                    self.stored, self.latest
                ));
            }
            Ok(())
        }

        fn act(&mut self, next: Next, act: &Act) -> Result<(), String> {
            match act {
                Act::Edit => {
                    self.latest += 1;
                    self.feed(
                        next,
                        Event::Changed {
                            version: self.latest,
                        },
                    )
                }
                Act::Tick { stale } => {
                    let timer = if *stale {
                        self.machine.timer.saturating_sub(1)
                    } else {
                        self.machine.timer
                    };
                    let before = self.inflight;
                    self.feed(next, Event::Tick { timer })?;
                    // The timer of an older change must not save what a newer change is still
                    // waiting 2 s for.
                    if *stale && self.inflight != before {
                        return Err("the tick of a restarted timer started a save".into());
                    }
                    Ok(())
                }
                Act::Flush => self.feed(next, Event::Flush),
                Act::Finish { ok } => {
                    let Some(version) = self.inflight.take() else {
                        return Ok(());
                    };
                    if *ok {
                        self.stored = self.stored.max(version);
                        self.feed(next, Event::Done(Ok(())))
                    } else {
                        self.feed(next, Event::Done(Err(NotSaved::Disk("full".into()))))
                    }
                }
                Act::Crash => {
                    // What the machine knew is gone; the links that were unsaved are lost, as
                    // a crash loses them (C5's draft is what narrows this window). The new
                    // aligner opens from the store.
                    self.machine = Machine::clean();
                    self.inflight = None;
                    self.latest = self.stored;
                    self.carried = false;
                    Ok(())
                }
            }
        }

        /// A flush that waits, as the actor's, and the person's retry after a failure: save,
        /// let each save end well, until settled as Saved.
        fn drain(&mut self, next: Next) -> Result<(), String> {
            for _ in 0..50 {
                match self.machine.settled() {
                    Some(AlignSave::Saved) => {
                        return if self.stored == self.latest {
                            Ok(())
                        } else {
                            Err(format!(
                                "a flush ended Saved with {} of {} stored",
                                self.stored, self.latest
                            ))
                        };
                    }
                    // A failure is answered as one, and a retry saves.
                    Some(_) | None if self.inflight.is_none() => {
                        let before = self.machine.clone();
                        self.feed(next, Event::Flush)?;
                        if self.inflight.is_none() && self.machine == before {
                            return Err(format!("a flush did nothing in {:?}", before.state));
                        }
                    }
                    Some(_) | None => self.act(next, &Act::Finish { ok: true })?,
                }
            }
            Err("a flush never ended".into())
        }
    }

    fn history(next: Next, carried: bool, acts: &[Act]) -> Result<(), String> {
        let mut world = World::new(carried);
        for act in acts {
            world.act(next, act)?;
        }
        // Whatever happened, a flush that waits saves the newest links.
        world.drain(next)?;
        if world.stored != world.latest {
            return Err(format!(
                "after a flush the store holds {} of {}",
                world.stored, world.latest
            ));
        }
        Ok(())
    }

    fn holds(next: Next) -> Result<(), TestError<(bool, Vec<Act>)>> {
        let mut runner = TestRunner::new(Config {
            cases: 4096,
            ..Config::default()
        });
        runner.run(&(any::<bool>(), acts()), |(carried, acts)| {
            history(next, carried, &acts).map_err(TestCaseError::fail)
        })
    }

    #[test]
    fn no_change_is_forgotten_and_saved_means_stored() {
        holds(next).unwrap();
    }

    /// A save that ends well while newer links wait must not make the machine clean.
    fn forgets_edits_made_during_a_save(machine: Machine, event: Event) -> (Machine, Vec<Effect>) {
        let (mut out, effects) = next(machine, event.clone());
        if matches!(event, Event::Done(Ok(()))) && matches!(out.state, State::Dirty { .. }) {
            out.state = State::Clean;
        }
        (out, effects)
    }

    /// A save that fails must not make the machine clean.
    fn forgets_a_failed_save(machine: Machine, event: Event) -> (Machine, Vec<Effect>) {
        let (mut out, effects) = next(machine, event.clone());
        if matches!(event, Event::Done(Err(_))) {
            out.state = State::Clean;
        }
        (out, effects)
    }

    /// The tick of a timer that was restarted must not start a save.
    fn obeys_a_stale_tick(machine: Machine, event: Event) -> (Machine, Vec<Effect>) {
        if let (State::Dirty { latest, .. }, Event::Tick { .. }) = (&machine.state, &event) {
            let latest = *latest;
            return (
                Machine {
                    state: State::Saving {
                        sent: latest,
                        latest,
                    },
                    timer: machine.timer,
                },
                vec![Effect::Start { version: latest }],
            );
        }
        next(machine, event)
    }

    /// A flush while a save runs must not start a second one.
    fn starts_a_second_save(machine: Machine, event: Event) -> (Machine, Vec<Effect>) {
        if let (State::Saving { sent, latest }, Event::Flush) = (&machine.state, &event) {
            let (sent, latest) = (*sent, *latest);
            return (
                Machine {
                    state: State::Saving { sent, latest },
                    timer: machine.timer,
                },
                vec![Effect::Start { version: latest }],
            );
        }
        next(machine, event)
    }

    /// A save that sent version `sent` marks everything saved, though `latest` is newer.
    fn marks_the_newest_saved(machine: Machine, event: Event) -> (Machine, Vec<Effect>) {
        if let (State::Saving { .. }, Event::Done(Ok(()))) = (&machine.state, &event) {
            return (
                Machine {
                    state: State::Clean,
                    timer: machine.timer,
                },
                vec![Effect::MarkSaved],
            );
        }
        next(machine, event)
    }

    #[test]
    fn each_bug_put_back_is_caught() {
        for (name, buggy) in [
            (
                "forgetting edits made during a save",
                forgets_edits_made_during_a_save as Next,
            ),
            ("forgetting a failed save", forgets_a_failed_save as Next),
            ("obeying a stale tick", obeys_a_stale_tick as Next),
            ("starting a second save", starts_a_second_save as Next),
            ("marking the newest saved", marks_the_newest_saved as Next),
        ] {
            assert!(holds(buggy).is_err(), "the model missed: {name}");
        }
    }

    #[test]
    fn a_lossless_carry_over_is_saved_by_a_flush_and_never_by_a_timer() {
        let (m, effects) = next(Machine::carried(0), Event::Tick { timer: 0 });
        assert_eq!(m, Machine::carried(0));
        assert!(effects.is_empty());
        assert_eq!(m.visible(), AlignSave::Saved);
        let (m, effects) = next(m, Event::Flush);
        assert_eq!(effects, [Effect::Start { version: 0 }]);
        let (m, effects) = next(m, Event::Done(Ok(())));
        assert_eq!(m.state, State::Clean);
        assert_eq!(effects, [Effect::MarkSaved]);
    }
}
