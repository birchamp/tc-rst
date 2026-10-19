//! Open projects (APP-DESIGN §4.3, AD38, A-8, A-25): one session per open folder, shared by
//! every caller that needs the project, each open with an epoch that is never used again.
//!
//! - **One open per folder.** `ProjectStore::open` runs at most once per folder at a time.
//!   A caller that arrives while it runs waits for it and becomes another user of the same
//!   session: it never opens again, which would find the folder's lock held by this same
//!   `tc4` (STORAGE §6.1 O1). A failure reaches every waiter, uses no epoch and leaves nothing.
//! - **Users.** A session's users are the screen (at most one folder has it), side users (a
//!   sync, a USB export or a Settings-from-Home save, each a [`SideLease`]) and the writes in
//!   flight (each a [`WriteLease`], so a session never closes under a running write, even when
//!   the HTTP caller went away). Only the screen answers project-scoped requests: once the
//!   person has gone Home, a request with that scope is stale, whoever else holds the
//!   session.
//! - **The close at Home**, as tC4's `backToProjects` and `startLeaveCheckpoint`
//!   (`tC4:src/state.jsx:7286-7328, 1918-1945`): [`SessionRegistry::leave_screen`] returns
//!   at once, the "leaving the project" checkpoint runs behind it, and the session closes when
//!   its last user and its last write are gone. A later open of that folder, by any role,
//!   waits for the checkpoint and the close (`tC4:src/state.jsx:2177, 6203`). A checkpoint
//!   that fails is reported and the session still closes: every save is durable in the
//!   journal, and nothing is lost.
//! - **Epochs** count from 1 for the whole server run and are never reset, not by
//!   `test/reset`, so a page from an earlier open never reaches another open's session.
//! - **Events.** `Opened` once per epoch; each fold step of the store becomes `Changed` with
//!   its scope ([`crate::wire::scope::change_scope`]), and a mode switch `Mode`; then
//!   `Checkpoint` and `Closed`.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use serde_json::json;
use tc_api::app::{OpenSession, SessionRole};
use tc_api::event::{CheckpointOutcome, Event, SessionEvent};
use tc_api::ids::{Epoch, ProjectId, ProjectRef, Scope, ViewSeq};
use tc_api::{ApiError, Message};
use tc_journal::record::CommentTarget;
use tc_journal::{Record, RecordKey};
use tokio::sync::{mpsc, watch};

use crate::app::lock;
use crate::backend::{CheckpointReason, OpenFailure, ProjectHandle, ProjectStore, StoreEvent};
use crate::clock::Clock;
use crate::counters::{AppCounters, CountingSnapshot};
use crate::events::EventBus;
use crate::jobs::ProgressSink;
use crate::replies::ReplyCache;
use crate::reports::{ReportLog, ReportOp, ReportOutcome, facts};
use crate::wire;

/// The project behind an open scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    /// The folder the scope names.
    pub project: ProjectRef,
    /// The project's id, which keys its preferences (`prefs/<projectId>.json`).
    pub id: ProjectId,
}

/// A scope that is not the screen's open session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the project was closed or opened again")]
pub struct StaleScope;

/// One open of one folder.
pub struct ProjectSession {
    scope: Scope,
    id: ProjectId,
    handle: Arc<dyn ProjectHandle>,
    replies: Arc<ReplyCache>,
    counters: Arc<AppCounters>,
    /// The newest store sequence this session's events have told the UI about.
    view: AtomicU64,
}

impl ProjectSession {
    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    pub fn id(&self) -> &ProjectId {
        &self.id
    }

    pub fn handle(&self) -> &Arc<dyn ProjectHandle> {
        &self.handle
    }

    /// The newest snapshot, with its reads counted.
    pub fn read(&self) -> CountingSnapshot {
        CountingSnapshot::new(self.handle.snapshot(), self.counters.clone())
    }

    pub fn view(&self) -> ViewSeq {
        wire::scope::view(self.view.load(Ordering::SeqCst))
    }

    /// This session's reply cache: a retry of a project write is answered from it, and it
    /// goes with the session (AD4).
    pub fn replies(&self) -> &Arc<ReplyCache> {
        &self.replies
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Screen,
    Side,
    Write,
}

type OpenResult = Option<Result<Arc<ProjectSession>, OpenFailure>>;

enum Phase {
    /// `ProjectStore::open` runs; the outcome comes on the channel.
    Opening(watch::Receiver<OpenResult>),
    Open {
        session: Arc<ProjectSession>,
        /// The leave checkpoint while it runs; `true` on the channel when it ends.
        leaving: Option<watch::Receiver<bool>>,
    },
    /// The store closes the project; `true` on the channel when it is closed.
    Closing(watch::Receiver<bool>),
}

struct Slot {
    /// This slot's own number: a lease of an earlier slot of the folder changes nothing here.
    id: u64,
    phase: Phase,
    screen: bool,
    side: u32,
    writes: u32,
}

impl Slot {
    fn session(&self) -> Option<&Arc<ProjectSession>> {
        match &self.phase {
            Phase::Open { session, .. } => Some(session),
            Phase::Opening(_) | Phase::Closing(_) => None,
        }
    }

    /// The screen's session at `epoch`, which answers project-scoped requests.
    fn screen_session(&self, epoch: Epoch) -> Option<&Arc<ProjectSession>> {
        self.session()
            .filter(|session| self.screen && session.scope.epoch == epoch)
    }

    fn unused(&self) -> bool {
        !self.screen && self.side == 0 && self.writes == 0
    }
}

struct RegistryInner {
    store: Arc<dyn ProjectStore>,
    bus: EventBus,
    reports: ReportLog,
    clock: Arc<dyn Clock>,
    counters: Arc<AppCounters>,
    runtime: tokio::runtime::Handle,
    /// The next epoch; never reset.
    epochs: AtomicU64,
    slot_ids: AtomicU64,
    slots: Mutex<BTreeMap<ProjectRef, Slot>>,
}

/// Every open project of this server run. Clones share the registry.
#[derive(Clone)]
pub struct SessionRegistry {
    inner: Arc<RegistryInner>,
}

/// What a caller waits for before it can act on a folder.
enum Wait {
    Open(watch::Receiver<OpenResult>),
    Settle(watch::Receiver<bool>),
}

/// A user of a slot that leaves the slot when dropped, unless it was handed on.
struct UserGuard {
    registry: Weak<RegistryInner>,
    folder: ProjectRef,
    slot: u64,
    role: Role,
    armed: bool,
}

impl Drop for UserGuard {
    fn drop(&mut self) {
        if self.armed
            && let Some(inner) = self.registry.upgrade()
        {
            SessionRegistry { inner }.release(&self.folder, self.slot, self.role);
        }
    }
}

/// A side user of a session (a sync, a USB export, a Settings-from-Home save): it shares the
/// screen's session, or keeps one open of its own, and leaves when dropped.
pub struct SideLease {
    session: Arc<ProjectSession>,
    _guard: UserGuard,
}

impl SideLease {
    pub fn scope(&self) -> &Scope {
        &self.session.scope
    }

    pub fn session(&self) -> &Arc<ProjectSession> {
        &self.session
    }
}

/// One write in flight: the session stays open until it is dropped.
pub struct WriteLease {
    session: Arc<ProjectSession>,
    _guard: UserGuard,
}

impl WriteLease {
    pub fn session(&self) -> &Arc<ProjectSession> {
        &self.session
    }
}

impl SessionRegistry {
    pub(crate) fn new(
        store: Arc<dyn ProjectStore>,
        bus: EventBus,
        reports: ReportLog,
        clock: Arc<dyn Clock>,
        counters: Arc<AppCounters>,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        Self {
            inner: Arc::new(RegistryInner {
                store,
                bus,
                reports,
                clock,
                counters,
                runtime,
                epochs: AtomicU64::new(1),
                slot_ids: AtomicU64::new(1),
                slots: Mutex::new(BTreeMap::new()),
            }),
        }
    }

    pub(crate) fn store(&self) -> &Arc<dyn ProjectStore> {
        &self.inner.store
    }

    fn guard(&self, folder: &ProjectRef, slot: u64, role: Role) -> UserGuard {
        UserGuard {
            registry: Arc::downgrade(&self.inner),
            folder: folder.clone(),
            slot,
            role,
            armed: true,
        }
    }

    /// Opens `folder` for the screen, or joins the session a side user holds or is opening.
    /// At most one folder has the screen: a screen open of another folder first leaves the
    /// old one, as going Home would. Waits for a leave checkpoint or a close of `folder` that
    /// is still running.
    pub async fn open_screen(
        &self,
        folder: &ProjectRef,
        progress: ProgressSink,
    ) -> Result<OpenSession, OpenFailure> {
        let elsewhere: Vec<Scope> = {
            let slots = lock(&self.inner.slots);
            slots
                .iter()
                .filter(|(other, slot)| *other != folder && slot.screen)
                .filter_map(|(_, slot)| slot.session().map(|s| s.scope.clone()))
                .collect()
        };
        for scope in elsewhere {
            // Its screen user may have just left; then there is nothing to leave.
            let _ = self.leave_screen(&scope);
        }
        let (session, mut guard) = self.join(folder, Role::Screen, progress).await?;
        // The screen stays a user until it leaves: `leave_screen`, a screen open elsewhere,
        // or `close_all`.
        guard.armed = false;
        Ok(OpenSession {
            at: session.scope.clone(),
            id: session.id.clone(),
            mode: session.handle.facts().mode,
            role: SessionRole::Screen,
        })
    }

    /// A side user: shares the session, or opens one of its own; leaves when dropped.
    pub async fn open_side(
        &self,
        folder: &ProjectRef,
        progress: ProgressSink,
    ) -> Result<SideLease, OpenFailure> {
        let (session, guard) = self.join(folder, Role::Side, progress).await?;
        Ok(SideLease {
            session,
            _guard: guard,
        })
    }

    /// Becomes a user of `folder`'s session in `role`, opening it if no one has.
    async fn join(
        &self,
        folder: &ProjectRef,
        role: Role,
        progress: ProgressSink,
    ) -> Result<(Arc<ProjectSession>, UserGuard), OpenFailure> {
        loop {
            let (wait, guard) = {
                let mut slots = lock(&self.inner.slots);
                match slots.get_mut(folder) {
                    None => {
                        let id = self.inner.slot_ids.fetch_add(1, Ordering::SeqCst);
                        let (tx, rx) = watch::channel(None);
                        let mut slot = Slot {
                            id,
                            phase: Phase::Opening(rx.clone()),
                            screen: false,
                            side: 0,
                            writes: 0,
                        };
                        let guard = self.register(&mut slot, folder, role);
                        slots.insert(folder.clone(), slot);
                        self.spawn_open(folder.clone(), id, tx, progress.clone());
                        (Wait::Open(rx), Some(guard))
                    }
                    Some(slot) => match &slot.phase {
                        Phase::Opening(rx) => {
                            let rx = rx.clone();
                            (Wait::Open(rx), Some(self.register(slot, folder, role)))
                        }
                        Phase::Open {
                            leaving: Some(done),
                            ..
                        } => (Wait::Settle(done.clone()), None),
                        Phase::Closing(done) => (Wait::Settle(done.clone()), None),
                        Phase::Open {
                            session,
                            leaving: None,
                        } => {
                            let session = session.clone();
                            let guard = self.register(slot, folder, role);
                            return Ok((session, guard));
                        }
                    },
                }
            };
            match wait {
                Wait::Settle(mut done) => {
                    // A closed channel means the step ended too.
                    let _ = done.wait_for(|done| *done).await;
                }
                Wait::Open(mut rx) => {
                    let outcome = match rx.wait_for(Option::is_some).await {
                        Ok(outcome) => outcome.clone(),
                        Err(_) => Some(Err(OpenFailure::Unreadable {
                            reason: "the open stopped without an outcome".to_owned(),
                        })),
                    };
                    let guard = guard.ok_or(OpenFailure::Unavailable)?;
                    return match outcome {
                        Some(Ok(session)) => Ok((session, guard)),
                        Some(Err(failure)) => {
                            let mut guard = guard;
                            // The slot is gone with the failure.
                            guard.armed = false;
                            Err(failure)
                        }
                        None => Err(OpenFailure::Unavailable),
                    };
                }
            }
        }
    }

    /// Counts a user in `slot` and hands back the guard that counts it out.
    fn register(&self, slot: &mut Slot, folder: &ProjectRef, role: Role) -> UserGuard {
        let mut guard = self.guard(folder, slot.id, role);
        match role {
            Role::Screen if slot.screen => {
                // The screen is a user already; this caller does not own it.
                guard.armed = false;
            }
            Role::Screen => slot.screen = true,
            Role::Side => slot.side += 1,
            Role::Write => slot.writes += 1,
        }
        guard
    }

    /// Runs the store's open once, on the blocking pool, whoever is still waiting for it.
    fn spawn_open(
        &self,
        folder: ProjectRef,
        slot_id: u64,
        tx: watch::Sender<OpenResult>,
        progress: ProgressSink,
    ) {
        let registry = self.clone();
        self.inner.runtime.spawn(async move {
            let store = registry.inner.store.clone();
            let started = registry.inner.clock.now();
            let opened = {
                let folder = folder.clone();
                tokio::task::spawn_blocking(move || store.open(&folder, progress)).await
            };
            let opened = match opened {
                Ok(opened) => opened,
                Err(stopped) => Err(OpenFailure::Unreadable {
                    reason: format!("the open stopped: {stopped}"),
                }),
            };
            let outcome = match opened {
                Ok(handle) => Ok(registry.opened(&folder, slot_id, handle)),
                Err(failure) => {
                    registry.refused(&folder, slot_id, &failure, started);
                    Err(failure)
                }
            };
            tx.send_replace(Some(outcome));
        });
    }

    /// The store opened `folder`: the session gets its epoch, its event pump and its users.
    fn opened(
        &self,
        folder: &ProjectRef,
        slot_id: u64,
        handle: Arc<dyn ProjectHandle>,
    ) -> Arc<ProjectSession> {
        let epoch = Epoch::new(self.inner.epochs.fetch_add(1, Ordering::SeqCst));
        let first = handle.snapshot();
        let session = Arc::new(ProjectSession {
            scope: Scope {
                project: folder.clone(),
                epoch,
            },
            id: wire::scope::project_id(handle.facts().id),
            handle: handle.clone(),
            replies: ReplyCache::new(ReplyCache::FINISHED),
            counters: self.inner.counters.clone(),
            view: AtomicU64::new(first.seq()),
        });
        // Subscribed before the slot is open, so no step of the store is missed.
        let events = handle.subscribe();
        self.inner.bus.publish(Event::Session {
            at: session.scope.clone(),
            event: SessionEvent::Opened,
        });
        let close_now = {
            let mut slots = lock(&self.inner.slots);
            match slots.get_mut(folder).filter(|slot| slot.id == slot_id) {
                Some(slot) => {
                    slot.phase = Phase::Open {
                        session: session.clone(),
                        leaving: None,
                    };
                    // Every waiter may have gone away meanwhile.
                    slot.unused()
                }
                None => false,
            }
        };
        self.inner
            .runtime
            .spawn(pump(self.inner.bus.clone(), session.clone(), events));
        if close_now {
            self.maybe_close(folder, slot_id);
        }
        session
    }

    /// The store refused `folder`: one report, and the slot is gone, with no epoch used.
    fn refused(
        &self,
        folder: &ProjectRef,
        slot_id: u64,
        failure: &OpenFailure,
        started: tc_api::ids::WallMs,
    ) {
        {
            let mut slots = lock(&self.inner.slots);
            if slots.get(folder).is_some_and(|slot| slot.id == slot_id) {
                slots.remove(folder);
            }
        }
        let outcome = match wire::error::open_failure(failure.clone()) {
            ApiError::Refused(refusal) => ReportOutcome::Refused {
                code: refusal.code().map(str::to_owned),
                key: refusal.key().to_owned(),
            },
            other => ReportOutcome::Failed {
                reason: other.to_string(),
            },
        };
        self.inner.reports.add(
            ReportOp::Open,
            outcome,
            facts(json!({"project": folder.as_str()})),
            started,
            self.inner.clock.now(),
        );
    }

    /// The close at Home: the screen user leaves now (the caller goes Home at once), the
    /// "leaving the project" checkpoint runs behind it, and the session closes when its last
    /// user and its last write are gone. A scope that is not the screen's is stale.
    pub fn leave_screen(&self, at: &Scope) -> Result<(), StaleScope> {
        let (session, slot_id, done) = {
            let mut slots = lock(&self.inner.slots);
            let slot = slots.get_mut(&at.project).ok_or(StaleScope)?;
            let session = slot.screen_session(at.epoch).ok_or(StaleScope)?.clone();
            let (done, rx) = watch::channel(false);
            slot.screen = false;
            slot.phase = Phase::Open {
                session: session.clone(),
                leaving: Some(rx),
            };
            (session, slot.id, done)
        };
        let registry = self.clone();
        self.inner.runtime.spawn(async move {
            let started = registry.inner.clock.now();
            let outcome = session
                .handle
                .checkpoint(CheckpointReason::LeavingProject)
                .await;
            if let CheckpointOutcome::Failed { reason } = &outcome {
                registry.inner.reports.add(
                    ReportOp::Checkpoint,
                    ReportOutcome::Failed {
                        reason: reason.clone(),
                    },
                    facts(json!({"project": session.scope.project.as_str(), "reason": "leavingProject"})),
                    started,
                    registry.inner.clock.now(),
                );
            }
            registry.inner.bus.publish(Event::Session {
                at: session.scope.clone(),
                event: SessionEvent::Checkpoint(outcome),
            });
            {
                let mut slots = lock(&registry.inner.slots);
                if let Some(slot) = slots
                    .get_mut(&session.scope.project)
                    .filter(|slot| slot.id == slot_id)
                {
                    slot.phase = Phase::Open {
                        session: session.clone(),
                        leaving: None,
                    };
                }
            }
            done.send_replace(true);
            registry.maybe_close(&session.scope.project, slot_id);
        });
        Ok(())
    }

    /// Holds the screen's session open for one write. `None`: the scope is closed, replaced,
    /// or no longer the screen's.
    pub fn writer(&self, at: &Scope) -> Option<WriteLease> {
        let mut slots = lock(&self.inner.slots);
        let slot = slots.get_mut(&at.project)?;
        let session = slot.screen_session(at.epoch)?.clone();
        let guard = self.register(slot, &at.project, Role::Write);
        Some(WriteLease {
            session,
            _guard: guard,
        })
    }

    /// The screen's session of a query; `Err(current)` names the folder's screen epoch, if
    /// it has one.
    pub fn reader(&self, at: &Scope) -> Result<Arc<ProjectSession>, Option<Epoch>> {
        let slots = lock(&self.inner.slots);
        let Some(slot) = slots.get(&at.project) else {
            return Err(None);
        };
        match slot.screen_session(at.epoch) {
            Some(session) => Ok(session.clone()),
            None => Err(slot
                .session()
                .filter(|_| slot.screen)
                .map(|session| session.scope.epoch)),
        }
    }

    /// The project behind an open scope, for a handler that runs inside a write's lease: the
    /// session is open while any user holds it.
    pub fn resolve(&self, at: &Scope) -> Option<SessionInfo> {
        let slots = lock(&self.inner.slots);
        let session = slots
            .get(&at.project)?
            .session()
            .filter(|session| session.scope.epoch == at.epoch)?;
        Some(SessionInfo {
            project: session.scope.project.clone(),
            id: session.id.clone(),
        })
    }

    /// The sessions with a screen or a side user, by folder: what `hello` lists, so a reload
    /// continues without opening the project again (AD22).
    pub fn open_sessions(&self) -> Vec<OpenSession> {
        let slots = lock(&self.inner.slots);
        slots
            .values()
            .filter(|slot| slot.screen || slot.side > 0)
            .filter_map(|slot| {
                let session = slot.session()?;
                Some(OpenSession {
                    at: session.scope.clone(),
                    id: session.id.clone(),
                    mode: session.handle.facts().mode,
                    role: if slot.screen {
                        SessionRole::Screen
                    } else {
                        SessionRole::Side
                    },
                })
            })
            .collect()
    }

    /// Resolves when no open, leave checkpoint or close of `folder` is running.
    pub async fn settled(&self, folder: &ProjectRef) {
        loop {
            let wait = {
                let slots = lock(&self.inner.slots);
                match slots.get(folder).map(|slot| &slot.phase) {
                    None | Some(Phase::Open { leaving: None, .. }) => return,
                    Some(Phase::Opening(rx)) => Wait::Open(rx.clone()),
                    Some(Phase::Open {
                        leaving: Some(done),
                        ..
                    })
                    | Some(Phase::Closing(done)) => Wait::Settle(done.clone()),
                }
            };
            match wait {
                Wait::Open(mut rx) => {
                    let _ = rx.wait_for(Option::is_some).await;
                }
                Wait::Settle(mut done) => {
                    let _ = done.wait_for(|done| *done).await;
                }
            }
        }
    }

    /// `app/quit` and `test/reset`: the screen leaves with its checkpoint, side users are let
    /// go, and every session closes once its writes are done.
    pub async fn close_all(&self) {
        let screens: Vec<Scope> = {
            let mut slots = lock(&self.inner.slots);
            for slot in slots.values_mut() {
                // Their leases count out of a slot that no longer counts them.
                slot.side = 0;
            }
            slots
                .values()
                .filter(|slot| slot.screen)
                .filter_map(|slot| slot.session().map(|s| s.scope.clone()))
                .collect()
        };
        for scope in &screens {
            let _ = self.leave_screen(scope);
        }
        let folders: Vec<(ProjectRef, u64)> = lock(&self.inner.slots)
            .iter()
            .map(|(folder, slot)| (folder.clone(), slot.id))
            .collect();
        for (folder, slot_id) in &folders {
            self.maybe_close(folder, *slot_id);
        }
        for (folder, _) in &folders {
            self.settled(folder).await;
        }
    }

    /// An open, a leave checkpoint or a close is running, or a session's write is
    /// (`test/quiet`).
    pub fn busy(&self) -> bool {
        lock(&self.inner.slots).values().any(|slot| {
            slot.writes > 0
                || match &slot.phase {
                    Phase::Opening(_) | Phase::Closing(_) => true,
                    Phase::Open { leaving, .. } => leaving.is_some(),
                }
        })
    }

    /// A user counts out of slot `slot_id` of `folder`.
    fn release(&self, folder: &ProjectRef, slot_id: u64, role: Role) {
        let leave = {
            let mut slots = lock(&self.inner.slots);
            let Some(slot) = slots.get_mut(folder).filter(|slot| slot.id == slot_id) else {
                return;
            };
            match role {
                Role::Screen => {
                    // A screen caller that went away: a leave of the screen, if it is open.
                    if matches!(slot.phase, Phase::Opening(_)) {
                        slot.screen = false;
                        None
                    } else {
                        slot.session().map(|s| s.scope.clone())
                    }
                }
                Role::Side => {
                    slot.side = slot.side.saturating_sub(1);
                    None
                }
                Role::Write => {
                    slot.writes = slot.writes.saturating_sub(1);
                    None
                }
            }
        };
        if let Some(scope) = leave {
            let _ = self.leave_screen(&scope);
        }
        self.maybe_close(folder, slot_id);
    }

    /// Closes the session of slot `slot_id` when it has no user, no write and no leave
    /// checkpoint running: the store's close, then the slot is gone, then `Closed`.
    fn maybe_close(&self, folder: &ProjectRef, slot_id: u64) {
        let (session, done) = {
            let mut slots = lock(&self.inner.slots);
            let Some(slot) = slots.get_mut(folder).filter(|slot| slot.id == slot_id) else {
                return;
            };
            let session = match &slot.phase {
                Phase::Open {
                    session,
                    leaving: None,
                } if slot.unused() => session.clone(),
                _ => return,
            };
            let (done, rx) = watch::channel(false);
            slot.phase = Phase::Closing(rx);
            (session, done)
        };
        let registry = self.clone();
        let folder = folder.clone();
        self.inner.runtime.spawn(async move {
            let started = registry.inner.clock.now();
            let closed = session.handle.clone().close().await;
            if let Err(error) = closed {
                registry.inner.reports.add(
                    ReportOp::Close,
                    ReportOutcome::Failed {
                        reason: error.to_string(),
                    },
                    facts(json!({"project": folder.as_str()})),
                    started,
                    registry.inner.clock.now(),
                );
            }
            {
                let mut slots = lock(&registry.inner.slots);
                if slots.get(&folder).is_some_and(|slot| slot.id == slot_id) {
                    slots.remove(&folder);
                }
            }
            registry.inner.bus.publish(Event::Session {
                at: session.scope.clone(),
                event: SessionEvent::Closed,
            });
            done.send_replace(true);
        });
    }
}

/// Where a comment is: its value says, not its key.
fn comment_target(read: &CountingSnapshot, key: &RecordKey) -> Option<CommentTarget> {
    match read.projected(key)? {
        Record::Comment { value, .. } => Some(value.target),
        _ => None,
    }
}

/// Turns the store's events of one session into the UI's, until the handle closes.
async fn pump(
    bus: EventBus,
    session: Arc<ProjectSession>,
    mut events: mpsc::UnboundedReceiver<StoreEvent>,
) {
    let mut previous = session.read();
    while let Some(event) = events.recv().await {
        let event = match event {
            StoreEvent::Committed { seq, delta, cause } => {
                let newest = session.read();
                let me = session.handle.facts().device;
                let labels = newest.person_labels(me);
                let label = |device: &tc_journal::DeviceId| {
                    labels.get(device).map(wire::scope::person_label)
                };
                let conflicts_changed = previous.conflict_counts() != newest.conflict_counts();
                let scope = wire::scope::change_scope(
                    &delta,
                    conflicts_changed,
                    |key| newest.writer_of(key).and_then(|device| label(&device)),
                    |key| comment_target(&newest, key),
                );
                let cause = wire::scope::change_cause(&cause, label);
                session.view.fetch_max(seq, Ordering::SeqCst);
                previous = newest;
                SessionEvent::Changed {
                    view: wire::scope::view(seq),
                    scope,
                    cause,
                }
            }
            StoreEvent::Mode(mode) => SessionEvent::Mode(mode),
        };
        bus.publish(Event::Session {
            at: session.scope.clone(),
            event,
        });
    }
}
