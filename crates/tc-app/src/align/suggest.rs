//! Per-testament suggestion memory. Confirmed saves queue only their own verse; appending,
//! cloning model snapshots and training all run on the blocking pool. Initial training uses
//! one project snapshot. Retraining attaches its booster to the current memory, so saves
//! that arrived while it ran stay learned. Slot and training generations fence late results.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

use tc_align::suggest::{Model, TrainingVerse, crosses_retrain_budget, training_verse};
use tc_align::{Alignment, TargetVerse};
use tc_api::align::{SuggestStatus, Testament};
use tc_api::event::{Event, SessionEvent};
use tc_api::ids::JobId;
use tc_api::jobs::{JobKind, JobOutcome, Phase};
use tc_core::BookId;
use tc_journal::record::{AlignmentValue, VerseValue};
use tc_journal::{Record, RecordKey};

use super::open::anchor;
use crate::app::App;
use crate::counters::CountingSnapshot;
use crate::jobs::{CancelToken, JobCtx, JobEnd, JobSpec};
use crate::session::ProjectSession;

#[derive(Clone)]
struct Memory {
    model: Arc<Model>,
    references: HashSet<String>,
}

enum State {
    Reading,
    Ready(Arc<Memory>),
    Failed(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Ticket {
    slot: u64,
    train: u64,
}

struct Training {
    ticket: Ticket,
    job: Option<JobId>,
    cancel: Option<CancelToken>,
}

enum Update {
    Saved(TrainingVerse),
    Trained {
        ticket: Ticket,
        result: Result<Memory, String>,
        cancel: CancelToken,
    },
}

#[derive(Default)]
enum Worker {
    #[default]
    Idle,
    Running,
}

#[derive(Default)]
enum Retrain {
    #[default]
    Current,
    Due,
}

struct Slot {
    generation: u64,
    state: State,
    training: Option<Training>,
    pending: VecDeque<Update>,
    worker: Worker,
    retrain: Retrain,
}

/// A session's models, by testament. Only queue and snapshot swaps hold this lock.
#[derive(Default)]
pub(crate) struct SuggestHub {
    slots: Mutex<HashMap<Testament, Slot>>,
    next: AtomicU64,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Every change under this lock is a store of a whole value.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn testament_of(book: BookId) -> Testament {
    match book.testament() {
        tc_core::Testament::Old => Testament::Ot,
        tc_core::Testament::New => Testament::Nt,
    }
}

fn engine(testament: Testament) -> tc_core::Testament {
    match testament {
        Testament::Nt => tc_core::Testament::New,
        Testament::Ot => tc_core::Testament::Old,
    }
}

fn status_of(model: &Model) -> SuggestStatus {
    if model.verses() == 0 {
        SuggestStatus::None
    } else {
        SuggestStatus::Ready {
            verses: u32::try_from(model.verses()).unwrap_or(u32::MAX),
            boosted: model.boosted() > 0,
        }
    }
}

impl SuggestHub {
    pub(crate) fn status(&self, testament: Testament) -> Option<SuggestStatus> {
        Some(match &lock(&self.slots).get(&testament)?.state {
            State::Reading => SuggestStatus::Reading,
            State::Ready(memory) => status_of(&memory.model),
            State::Failed(reason) => SuggestStatus::Error {
                reason: reason.clone(),
            },
        })
    }

    pub(crate) fn model(&self, testament: Testament) -> Option<Arc<Model>> {
        // The model itself is behind an Arc too: a prediction takes no corpus-sized copy.
        match &lock(&self.slots).get(&testament)?.state {
            State::Ready(memory) => Some(memory.model.clone()),
            State::Reading | State::Failed(_) => None,
        }
    }

    fn begin(
        &self,
        testament: Testament,
        again: bool,
        expected: Option<u64>,
        enabled: impl FnOnce() -> bool,
    ) -> Option<(Ticket, Option<JobId>)> {
        let mut slots = lock(&self.slots);
        // The switch is read while holding the lock Off uses to invalidate this slot.
        if !enabled()
            || expected.is_some_and(|generation| {
                !slots
                    .get(&testament)
                    .is_some_and(|slot| slot.generation == generation)
            })
        {
            return None;
        }
        if !again && slots.contains_key(&testament) {
            return None;
        }
        let slot = slots.entry(testament).or_insert_with(|| Slot {
            generation: self.next.fetch_add(1, Ordering::Relaxed),
            state: State::Reading,
            training: None,
            pending: VecDeque::new(),
            worker: Worker::Idle,
            retrain: Retrain::Current,
        });
        let previous = slot.training.take();
        if let Some(cancel) = previous.as_ref().and_then(|t| t.cancel.as_ref()) {
            cancel.cancel();
        }
        let ticket = Ticket {
            slot: slot.generation,
            train: self.next.fetch_add(1, Ordering::Relaxed),
        };
        slot.training = Some(Training {
            ticket,
            job: None,
            cancel: None,
        });
        slot.retrain = Retrain::Current;
        if matches!(slot.state, State::Failed(_)) {
            slot.state = State::Reading;
        }
        Some((ticket, previous.and_then(|t| t.job)))
    }

    fn attach(&self, testament: Testament, ticket: Ticket, job: JobId) {
        if let Some(training) = lock(&self.slots)
            .get_mut(&testament)
            .and_then(|s| s.training.as_mut())
            .filter(|t| t.ticket == ticket)
        {
            training.job = Some(job);
        }
    }

    fn attach_cancel(&self, testament: Testament, ticket: Ticket, cancel: &CancelToken) -> bool {
        let mut slots = lock(&self.slots);
        let Some(training) = slots
            .get_mut(&testament)
            .and_then(|s| s.training.as_mut())
            .filter(|t| t.ticket == ticket)
        else {
            cancel.cancel();
            return false;
        };
        training.cancel = Some(cancel.clone());
        true
    }

    /// Reserve one worker. Saves queued during the initial train wait for its model.
    fn wake(slot: &mut Slot) -> Option<u64> {
        let ready = matches!(slot.state, State::Ready(_))
            || matches!(slot.pending.front(), Some(Update::Trained { .. }));
        if matches!(slot.worker, Worker::Idle) && ready {
            slot.worker = Worker::Running;
            Some(slot.generation)
        } else {
            None
        }
    }

    fn saved(&self, testament: Testament, verse: TrainingVerse) -> Option<u64> {
        let mut slots = lock(&self.slots);
        let slot = slots.get_mut(&testament)?;
        slot.pending.push_back(Update::Saved(verse));
        Self::wake(slot)
    }

    fn trained(
        &self,
        testament: Testament,
        ticket: Ticket,
        result: Result<Memory, String>,
        cancel: CancelToken,
    ) -> Option<u64> {
        let mut slots = lock(&self.slots);
        let slot = slots
            .get_mut(&testament)
            .filter(|s| s.training.as_ref().is_some_and(|t| t.ticket == ticket))?;
        let update = Update::Trained {
            ticket,
            result,
            cancel,
        };
        if matches!(slot.state, State::Ready(_)) {
            slot.pending.push_back(update);
        } else {
            // The initial snapshot must precede all saves that arrived while it was read.
            slot.pending.push_front(update);
        }
        Self::wake(slot)
    }

    fn next_update(
        &self,
        testament: Testament,
        generation: u64,
    ) -> Option<(Option<Arc<Memory>>, Update)> {
        let mut slots = lock(&self.slots);
        let slot = slots
            .get_mut(&testament)
            .filter(|s| s.generation == generation)?;
        let memory = match &slot.state {
            State::Ready(memory) => Some(memory.clone()),
            State::Reading | State::Failed(_) => None,
        };
        if memory.is_none() && !matches!(slot.pending.front(), Some(Update::Trained { .. })) {
            slot.worker = Worker::Idle;
            return None;
        }
        let Some(update) = slot.pending.pop_front() else {
            slot.worker = Worker::Idle;
            return None;
        };
        Some((memory, update))
    }

    /// Off and project close invalidate every completion, including a train already computing.
    pub(crate) fn forget(&self) -> Vec<JobId> {
        let slots = std::mem::take(&mut *lock(&self.slots));
        slots
            .into_values()
            .filter_map(|slot| {
                let training = slot.training?;
                if let Some(cancel) = training.cancel {
                    cancel.cancel();
                }
                training.job
            })
            .collect()
    }
}

/// The actor captures this before submitting, never from its later edited workspace.
pub(super) fn saved_verse(
    key: &RecordKey,
    value: &AlignmentValue,
    target: &TargetVerse,
) -> Option<TrainingVerse> {
    let RecordKey::Alignment(book, chapter, verse) = key else {
        return None;
    };
    training_verse(
        format!("{} {chapter}:{}", book.code(), verse.get()),
        value,
        target,
    )
}

#[derive(Clone)]
pub(super) struct Learner {
    app: App,
    session: Weak<ProjectSession>,
}

impl Learner {
    pub(super) fn new(app: App, session: &Arc<ProjectSession>) -> Self {
        Self {
            app,
            session: Arc::downgrade(session),
        }
    }

    pub(super) fn saved(&self, testament: Testament, verse: TrainingVerse) {
        let Some(session) = self.session.upgrade() else {
            return;
        };
        if !self
            .app
            .inner
            .prefs
            .align_suggestions(&session.scope().project)
            .unwrap_or(false)
        {
            return;
        }
        if let Some(generation) = session.align().hub().saved(testament, verse) {
            self.work(&session, testament, generation);
        }
    }

    fn work(&self, session: &Arc<ProjectSession>, testament: Testament, generation: u64) {
        let hub = session.align().hub().clone();
        let (bus, scope) = (self.app.inner.bus.clone(), session.scope().clone());
        let learner = self.clone();
        tokio::task::spawn_blocking(move || {
            let announce = |status| {
                bus.publish(Event::Session {
                    at: scope.clone(),
                    event: SessionEvent::Suggest { testament, status },
                });
            };
            while let Some(retrain) = hub.apply_update(testament, generation, &announce) {
                if retrain && let Some(session) = learner.session.upgrade() {
                    learner
                        .app
                        .start_training(&session, testament, true, Some(generation));
                }
            }
        });
    }
}

impl SuggestHub {
    /// The caller runs one queue item on the blocking pool. Returning None releases its worker.
    fn apply_update(
        &self,
        testament: Testament,
        generation: u64,
        announce: &impl Fn(SuggestStatus),
    ) -> Option<bool> {
        let (current, update) = self.next_update(testament, generation)?;
        let (memory, completion, grew) = match update {
            Update::Saved(verse) => {
                let Some(current) = current else {
                    return Some(false);
                };
                let mut memory = (*current).clone();
                let before = memory.model.verses();
                let is_new = memory.references.insert(verse.reference().to_owned());
                Arc::make_mut(&mut memory.model).append_verse(&verse, is_new);
                let grew = crosses_retrain_budget(before, memory.model.verses());
                (Ok(memory), None, grew)
            }
            Update::Trained {
                ticket,
                result,
                cancel,
            } => {
                let result = match (current, result) {
                    (Some(current), Ok(trained)) => {
                        let mut memory = (*current).clone();
                        if let Some(booster) = trained.model.booster() {
                            Arc::make_mut(&mut memory.model)
                                .attach_booster(booster.clone(), trained.model.boosted());
                        }
                        Ok(memory)
                    }
                    (Some(current), Err(_)) => Ok((*current).clone()),
                    (None, result) => result,
                };
                (result, Some((ticket, cancel)), false)
            }
        };
        let (retrain, previous) = {
            let mut slots = lock(&self.slots);
            let slot = slots
                .get_mut(&testament)
                .filter(|s| s.generation == generation)?;
            if let Some((ticket, cancel)) = completion {
                if !slot.training.as_ref().is_some_and(|t| t.ticket == ticket) {
                    return Some(false);
                }
                slot.training = None;
                if cancel.is_cancelled() {
                    let status = match &slot.state {
                        State::Ready(memory) => status_of(&memory.model),
                        State::Reading | State::Failed(_) => {
                            let reason = "the suggestion training was cancelled".to_owned();
                            slot.state = State::Failed(reason.clone());
                            SuggestStatus::Error { reason }
                        }
                    };
                    announce(status);
                    return Some(false);
                }
            }
            let (state, status) = match memory {
                Ok(memory) => {
                    let status = status_of(&memory.model);
                    (State::Ready(Arc::new(memory)), status)
                }
                Err(reason) => (
                    State::Failed(reason.clone()),
                    SuggestStatus::Error { reason },
                ),
            };
            let previous = std::mem::replace(&mut slot.state, state);
            if grew {
                slot.retrain = Retrain::Due;
            }
            // Publishing under the same short lock orders this before a later Off.
            announce(status);
            let retrain = slot.training.is_none() && matches!(slot.retrain, Retrain::Due);
            if retrain {
                slot.retrain = Retrain::Current;
            }
            (retrain, previous)
        };
        // Freeing the previous corpus can be large too; request threads never wait for it.
        drop(previous);
        Some(retrain)
    }
}

/// Every verse of the project's books of `testament` that has a link, as the engine's training
/// data (tC4's `collectTrainingVerses`), from one snapshot.
fn corpus(
    read: &CountingSnapshot,
    testament: Testament,
    cancel: &CancelToken,
) -> Result<Vec<TrainingVerse>, JobEnd> {
    let mut out = Vec::new();
    for book in read
        .projected_books()
        .into_iter()
        .filter(|b| testament_of(*b) == testament)
    {
        cancel.check()?;
        for chapter in read.chapters_of(book) {
            for key in read.chapter_keys(book, chapter).verses {
                let Some(Record::Verse {
                    chapter: c,
                    verse,
                    value:
                        VerseValue::Text {
                            words,
                            layout,
                            from,
                            ..
                        },
                    ..
                }) = read.projected(&key)
                else {
                    continue;
                };
                let at = anchor(c, verse, from);
                let Some(Record::Alignment { value, .. }) =
                    read.projected(&RecordKey::Alignment(book, at.chapter, at.verse))
                else {
                    continue;
                };
                if value.placed() == 0 {
                    continue;
                }
                let target = TargetVerse::new(&format!("{words}{layout}"));
                let reference = format!("{} {}:{}", book.code(), chapter, verse.get());
                out.extend(training_verse(reference, &value, &target));
            }
        }
    }
    Ok(out)
}

impl App {
    pub(crate) fn train_suggestions(
        &self,
        session: &Arc<ProjectSession>,
        testament: Testament,
        again: bool,
    ) {
        self.start_training(session, testament, again, None);
    }

    fn start_training(
        &self,
        session: &Arc<ProjectSession>,
        testament: Testament,
        again: bool,
        expected: Option<u64>,
    ) {
        let hub = session.align().hub().clone();
        let Some((ticket, previous)) = hub.begin(testament, again, expected, || {
            self.inner
                .prefs
                .align_suggestions(&session.scope().project)
                .unwrap_or(false)
        }) else {
            return;
        };
        if let Some(previous) = previous {
            self.inner.jobs.cancel(previous);
        }
        let spec =
            JobSpec::new(JobKind::Train, Phase::Train).project(session.scope().project.clone());
        // This immutable snapshot pairs each alignment with the exact text it was read with.
        let read = session.read();
        let learner = Learner::new(self.clone(), session);
        let weak_session = Arc::downgrade(session);
        let (done_hub, bus, scope) = (hub.clone(), self.inner.bus.clone(), session.scope().clone());
        // Announce before starting and under Off's lock: no late Reading can follow Off.
        {
            let slots = lock(&hub.slots);
            if slots.get(&testament).is_some_and(|slot| {
                matches!(slot.state, State::Reading)
                    && slot.training.as_ref().is_some_and(|t| t.ticket == ticket)
            }) {
                bus.publish(Event::Session {
                    at: scope,
                    event: SessionEvent::Suggest {
                        testament,
                        status: SuggestStatus::Reading,
                    },
                });
            }
        }
        #[cfg(feature = "test-hooks")]
        let test_app = self.clone();
        let job = self.inner.jobs.start_blocking(spec, move |ctx: JobCtx| {
            if !done_hub.attach_cancel(testament, ticket, &ctx.cancel) {
                return Err(JobEnd::Cancelled);
            }
            #[cfg(feature = "test-hooks")]
            {
                tokio::runtime::Handle::current().block_on(async {
                    tokio::select! {
                        () = test_app.hold_at(tc_api::test::HoldPoint::Train) => {}
                        () = ctx.cancel.cancelled() => {}
                    }
                });
                if ctx.cancel.is_cancelled() {
                    return Err(JobEnd::Cancelled);
                }
            }
            let trained = match corpus(&read, testament, &ctx.cancel) {
                Ok(verses) => {
                    ctx.progress
                        .total(u64::try_from(verses.len()).unwrap_or(u64::MAX));
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        Memory {
                            model: Arc::new(Model::train(engine(testament), &verses, 0)),
                            references: verses.iter().map(|v| v.reference().to_owned()).collect(),
                        }
                    }))
                    .map_err(|_| "the engine stopped while it learned from your verses".to_owned());
                    ctx.progress
                        .reach(u64::try_from(verses.len()).unwrap_or(u64::MAX));
                    result
                }
                Err(_) => Err("the suggestion training was cancelled".to_owned()),
            };
            if let Some(generation) =
                done_hub.trained(testament, ticket, trained, ctx.cancel.clone())
                && let Some(session) = weak_session.upgrade()
            {
                learner.work(&session, testament, generation);
            }
            if ctx.cancel.is_cancelled() {
                Err(JobEnd::Cancelled)
            } else {
                Ok(JobOutcome::Finished)
            }
        });
        hub.attach(testament, ticket, job);
    }
}

impl App {
    /// The switch changed: on trains the open aligner's testament anew, off lets go of what was
    /// learned and says so.
    pub(crate) async fn suggestions_switched(
        &self,
        at: &tc_api::ids::Scope,
        on: bool,
    ) -> Result<(), tc_api::ApiError> {
        let session = self.read(at)?;
        let hub = session.align().hub().clone();
        if on {
            if let Some(book) = session.align().open_book().await {
                self.train_suggestions(&session, testament_of(book), true);
            }
            return Ok(());
        }
        let learned: Vec<Testament> = [Testament::Nt, Testament::Ot]
            .into_iter()
            .filter(|t| hub.status(*t).is_some())
            .collect();
        for job in hub.forget() {
            self.inner.jobs.cancel(job);
        }
        for testament in learned {
            self.inner.bus.publish(Event::Session {
                at: at.clone(),
                event: SessionEvent::Suggest {
                    testament,
                    status: SuggestStatus::Off,
                },
            });
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "suggest_tests.rs"]
mod tests;
