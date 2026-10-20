//! The suggestion engine's server side (tC4 D72, D93, #516): one model per testament, trained
//! in the background from one snapshot of the project's own alignments, and asked for one verse
//! at a time. A suggestion is a proposal: nothing here writes the project.
//!
//! The model of a testament is `Reading` while its `Train` job runs, then `Ready` (or `None`
//! when no verse of the testament has a link yet), or `Error` when the training stopped. Each
//! change is announced as a `Suggest` event, and every aligner view carries the state again, so
//! a gap in the event stream cannot leave "Reading" standing (§5.4).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tc_align::suggest::{Model, TrainingVerse, training_verse};
use tc_align::{Alignment, TargetVerse};
use tc_api::align::{SuggestStatus, Testament};
use tc_api::event::{Event, SessionEvent};
use tc_api::ids::JobId;
use tc_api::jobs::{JobKind, JobOutcome, Phase};
use tc_core::BookId;
use tc_journal::record::VerseValue;
use tc_journal::{Record, RecordKey};

use super::open::anchor;
use crate::app::App;
use crate::counters::CountingSnapshot;
use crate::jobs::{CancelToken, JobCtx, JobEnd, JobSpec};
use crate::session::ProjectSession;

enum Slot {
    Reading(JobId),
    Ready(Arc<Model>),
    Failed(String),
}

/// A session's models, by testament.
#[derive(Default)]
pub(crate) struct SuggestHub {
    slots: Mutex<HashMap<Testament, Slot>>,
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
    /// What the engine knows of `testament`; `None` until a training has begun.
    pub(crate) fn status(&self, testament: Testament) -> Option<SuggestStatus> {
        Some(match lock(&self.slots).get(&testament)? {
            Slot::Reading(_) => SuggestStatus::Reading,
            Slot::Ready(model) => status_of(model),
            Slot::Failed(reason) => SuggestStatus::Error {
                reason: reason.clone(),
            },
        })
    }

    /// The trained model, when there is one.
    pub(crate) fn model(&self, testament: Testament) -> Option<Arc<Model>> {
        match lock(&self.slots).get(&testament)? {
            Slot::Ready(model) => Some(model.clone()),
            Slot::Reading(_) | Slot::Failed(_) => None,
        }
    }

    /// Whether a training of `testament` is needed and none runs: with `again`, whatever is
    /// held is trained anew; without, only a testament nothing was tried for.
    fn wants_training(&self, testament: Testament, again: bool) -> bool {
        match lock(&self.slots).get(&testament) {
            Some(Slot::Reading(_)) => false,
            Some(_) => again,
            None => true,
        }
    }

    fn set(&self, testament: Testament, slot: Slot) {
        lock(&self.slots).insert(testament, slot);
    }

    /// The job of a training that is still reading.
    fn attach(&self, testament: Testament, job: JobId) {
        let mut slots = lock(&self.slots);
        if matches!(slots.get(&testament), Some(Slot::Reading(_))) {
            slots.insert(testament, Slot::Reading(job));
        }
    }

    /// A training that was asked to stop leaves nothing held for its testament.
    fn forget_reading(&self, testament: Testament) {
        let mut slots = lock(&self.slots);
        if matches!(slots.get(&testament), Some(Slot::Reading(_))) {
            slots.remove(&testament);
        }
    }

    /// The switch went off: what was learned is let go. The jobs that run are asked to stop.
    pub(crate) fn forget(&self) -> Vec<JobId> {
        let mut slots = lock(&self.slots);
        let jobs = slots
            .values()
            .filter_map(|slot| match slot {
                Slot::Reading(job) => Some(*job),
                Slot::Ready(_) | Slot::Failed(_) => None,
            })
            .collect();
        slots.clear();
        jobs
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
    /// Starts the `Train` job of `testament` when one is needed: a testament nothing was tried
    /// for, or, with `again`, every time (the switch went on).
    pub(crate) fn train_suggestions(
        &self,
        session: &Arc<ProjectSession>,
        testament: Testament,
        again: bool,
    ) {
        let hub = session.align().hub().clone();
        if !hub.wants_training(testament, again) {
            return;
        }
        // Held before the job starts: a training that ends at once must not be overwritten.
        hub.set(testament, Slot::Reading(JobId::new(0)));
        let spec =
            JobSpec::new(JobKind::Train, Phase::Train).project(session.scope().project.clone());
        let read = session.read();
        let (bus, scope) = (self.inner.bus.clone(), session.scope().clone());
        let announce = move |status: SuggestStatus| {
            bus.publish(Event::Session {
                at: scope.clone(),
                event: SessionEvent::Suggest { testament, status },
            });
        };
        let (done_hub, done_announce) = (hub.clone(), announce.clone());
        let job = self.inner.jobs.start_blocking(spec, move |ctx: JobCtx| {
            let verses = match corpus(&read, testament, &ctx.cancel) {
                Ok(verses) => verses,
                Err(end) => {
                    // The switch went off or the project closed: nothing is held.
                    done_hub.forget_reading(testament);
                    return Err(end);
                }
            };
            let engine = match testament {
                Testament::Nt => tc_core::Testament::New,
                Testament::Ot => tc_core::Testament::Old,
            };
            // A training that stops on a bug is stated, never left as "Reading" for ever.
            let trained = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                Model::train(engine, &verses, 0)
            }));
            let status = match trained {
                Ok(model) => {
                    let status = status_of(&model);
                    done_hub.set(testament, Slot::Ready(Arc::new(model)));
                    status
                }
                Err(_) => {
                    let reason = "the engine stopped while it learned from your verses".to_owned();
                    done_hub.set(testament, Slot::Failed(reason.clone()));
                    SuggestStatus::Error { reason }
                }
            };
            done_announce(status);
            Ok(JobOutcome::Finished)
        });
        hub.attach(testament, job);
        announce(SuggestStatus::Reading);
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
