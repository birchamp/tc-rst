//! The reply cache (APP-DESIGN §4.4, AD4, A-2): a retry of a write gets the first attempt's
//! reply instead of running again.
//!
//! A write carries a [`RequestId`]. The cache keys each id with a digest of the request, and
//! runs the write's work once per id, on a task of its own:
//!
//! - a retry of a finished request gets the first reply;
//! - a retry of a request that is still running waits for the first attempt's reply, so a
//!   reload that retries while a save waits behind a checkpoint never saves twice;
//! - a retry whose request differs from the first is [`CacheAnswer::Reused`], never the first
//!   reply: the UI marks a field clean only when the reply's text equals its own, and a reply
//!   to other text would mark newer typing as saved;
//! - the work runs to its end when its caller goes away (a reload, a dropped connection), so a
//!   save is never stopped half way;
//! - a request can be refused before its work starts, but only when its id is new: an id the
//!   cache knows was sent, so its retry gets its own reply and never a refusal that says
//!   nothing was sent;
//! - a write whose caller says [`Keep::Nothing`] leaves no reply behind: it is waited for
//!   while it runs, and its id is free the moment it ends.
//!
//! Replies are kept for the last [`ReplyCache::FINISHED`] requests; a request in flight is
//! never dropped.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ring::digest;
use tc_api::ApiError;
use tc_api::ids::RequestId;
use tokio::sync::watch;

/// SHA-256 of an endpoint path and its request, as serde writes the request again after it
/// was parsed. The same request digests the same whatever its spacing or key order in the
/// body, and one id used for another endpoint differs.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BodyDigest([u8; 32]);

impl BodyDigest {
    pub fn of(path: &str, canonical_body: &[u8]) -> Self {
        let mut context = digest::Context::new(&digest::SHA256);
        context.update(path.as_bytes());
        // A path never holds a zero byte, so this separates "a"+"bc" from "ab"+"c".
        context.update(&[0]);
        context.update(canonical_body);
        let mut out = [0u8; 32];
        out.copy_from_slice(context.finish().as_ref());
        Self(out)
    }
}

/// What the cache keeps of a write once it has finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keep {
    /// The reply, for the retries of its id: what every write needs, because a retry after a
    /// lost reply must not run the write again.
    Reply,
    /// Nothing, for a write that does the same when it runs again, such as `test/reset`. A
    /// kept reply would answer the next request that carries the id, without running it, and
    /// a harness restarts its ids with every scenario.
    Nothing,
}

/// What a write produced.
#[derive(Clone, Debug)]
pub enum Outcome {
    /// The bytes of the HTTP body: a serialized `Reply`.
    Reply(Arc<[u8]>),
    /// A test hook ended the process before the reply: there is none (D5).
    #[cfg(feature = "test-hooks")]
    Ended,
}

#[derive(Clone, Debug)]
pub enum CacheAnswer {
    /// This call ran the work.
    Fresh(Outcome),
    /// A retry: the first attempt's outcome, finished or waited for.
    Repeat(Outcome),
    /// The id came with another request. Nothing ran.
    Reused,
    /// The id is new and the caller had a refusal for it. Nothing ran and nothing is kept
    /// under the id: the UI sends the same request again once the refusal's cause is gone.
    Refused(ApiError),
    /// The task that ran the work ended without an outcome. The request may have been
    /// partly carried out, so it is not run again under this id.
    Lost,
}

type Waiting = watch::Receiver<Option<Outcome>>;

enum Slot {
    Running(Waiting),
    Finished(Outcome),
}

struct Entry {
    digest: BodyDigest,
    slot: Slot,
}

#[derive(Default)]
struct CacheState {
    entries: HashMap<RequestId, Entry>,
    /// Finished ids, oldest first.
    finished: VecDeque<RequestId>,
}

pub struct ReplyCache {
    state: Mutex<CacheState>,
    finished_cap: usize,
}

enum Plan {
    Reused,
    Refused(ApiError),
    Done(Outcome),
    Wait(Waiting, bool),
}

impl ReplyCache {
    /// The finished replies kept (AD4).
    pub const FINISHED: usize = 1024;

    pub fn new(finished_cap: usize) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(CacheState::default()),
            finished_cap,
        })
    }

    fn state(&self) -> MutexGuard<'_, CacheState> {
        // Every change below is a few map operations that cannot panic part way.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `work` once for `id`, or answers from the first attempt. The work runs on its own
    /// task, so a caller that goes away does not stop it; a second caller with the same id
    /// and digest waits for it.
    ///
    /// `keep` is what stays of the write when it ends.
    ///
    /// `refuse` is the answer for a request that cannot start now, for example because the
    /// internet switch is off. It applies only when `id` is new: the cache looks the id up
    /// first, so a request that was sent (finished, or still running) is answered from its
    /// first attempt, and one id used for another body is [`CacheAnswer::Reused`] whatever
    /// `refuse` says. The lookup and the decision are one step under the cache's lock, so an
    /// id that falls out of the cache between them cannot start work that `refuse` forbade.
    pub async fn run<W>(
        self: &Arc<Self>,
        keep: Keep,
        id: RequestId,
        digest: BodyDigest,
        refuse: Option<ApiError>,
        work: W,
    ) -> CacheAnswer
    where
        W: Future<Output = Outcome> + Send + 'static,
    {
        let plan = {
            let mut state = self.state();
            match state.entries.get(&id) {
                Some(entry) if entry.digest != digest => Plan::Reused,
                Some(Entry {
                    slot: Slot::Finished(outcome),
                    ..
                }) => Plan::Done(outcome.clone()),
                Some(Entry {
                    slot: Slot::Running(waiting),
                    ..
                }) => Plan::Wait(waiting.clone(), false),
                None => match refuse {
                    Some(error) => Plan::Refused(error),
                    None => {
                        let (sender, waiting) = watch::channel(None);
                        state.entries.insert(
                            id,
                            Entry {
                                digest,
                                slot: Slot::Running(waiting.clone()),
                            },
                        );
                        let cache = Arc::clone(self);
                        tokio::spawn(async move {
                            let outcome = work.await;
                            cache.finish(id, keep, &outcome);
                            sender.send_replace(Some(outcome));
                        });
                        Plan::Wait(waiting, true)
                    }
                },
            }
        };
        match plan {
            Plan::Reused => CacheAnswer::Reused,
            Plan::Refused(error) => CacheAnswer::Refused(error),
            Plan::Done(outcome) => CacheAnswer::Repeat(outcome),
            Plan::Wait(mut waiting, fresh) => {
                let outcome = match waiting.wait_for(Option::is_some).await {
                    Ok(ready) => ready.clone(),
                    Err(_) => return CacheAnswer::Lost,
                };
                match outcome {
                    Some(outcome) if fresh => CacheAnswer::Fresh(outcome),
                    Some(outcome) => CacheAnswer::Repeat(outcome),
                    None => CacheAnswer::Lost,
                }
            }
        }
    }

    /// Records the outcome, and forgets the oldest finished replies past the cap.
    fn finish(&self, id: RequestId, keep: Keep, outcome: &Outcome) {
        let mut state = self.state();
        if keep == Keep::Nothing {
            // Callers already waiting hold the write's channel, so they still get the outcome.
            state.entries.remove(&id);
            return;
        }
        if let Some(entry) = state.entries.get_mut(&id) {
            entry.slot = Slot::Finished(outcome.clone());
        }
        state.finished.push_back(id);
        while state.finished.len() > self.finished_cap {
            let Some(oldest) = state.finished.pop_front() else {
                break;
            };
            if matches!(
                state.entries.get(&oldest),
                Some(Entry {
                    slot: Slot::Finished(_),
                    ..
                })
            ) {
                state.entries.remove(&oldest);
            }
        }
    }

    /// Forgets every kept reply and every write that ended without one, and leaves the writes
    /// that are still running, so a retry of one of them still waits for it instead of running
    /// it again. This is what a new launch finds, for `test/reset`: a harness that starts the
    /// next scenario may use a request id again.
    pub fn forget_settled(&self) {
        let mut state = self.state();
        state.entries.retain(|_, entry| is_running(&entry.slot));
        state.finished.clear();
    }

    /// The writes still running, for `test/quiet`. A write whose task ended without an outcome
    /// is not running: it stays in the cache so a retry gets [`CacheAnswer::Lost`] instead of
    /// running again, but `test/quiet` must not wait for it until the process ends.
    pub fn running(&self) -> usize {
        self.state()
            .entries
            .values()
            .filter(|entry| is_running(&entry.slot))
            .count()
    }
}

fn is_running(slot: &Slot) -> bool {
    match slot {
        // `has_changed` fails only once the sender is dropped, which the task does when it
        // ends; a running write's value is `None` too, so that cannot tell.
        Slot::Running(waiting) => waiting.has_changed().is_ok(),
        Slot::Finished(_) => false,
    }
}
