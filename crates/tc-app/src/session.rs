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
use tc_api::event::{CheckpointOutcome, Event, SessionEvent, SessionMode};
use tc_api::ids::{Epoch, ProjectId, ProjectRef, Scope, ViewSeq, WallMs};
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
    /// The books whose resources this open has read (B3).
    books: crate::understand::BookCache,
    /// The check lists this open has derived (B5a).
    checks: crate::check::CheckCache,
    /// The open aligner (B6).
    pub(crate) align: crate::align::AlignCache,
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

    pub(crate) fn books(&self) -> &crate::understand::BookCache {
        &self.books
    }

    pub(crate) fn checks(&self) -> &crate::check::CheckCache {
        &self.checks
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

/// Who has a folder's screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    /// No one: the person is on Home or in another project.
    Free,
    /// This many screen opens wait for the folder's open, or have its outcome and have not
    /// returned yet, each with an armed guard. One that goes away hands the screen to those still waiting; the last one takes
    /// it with it, so a cancelled open never closes a session another screen open is about
    /// to return.
    Waiting(u32),
    /// A screen open returned: the screen is the person's until `leave_screen`, a screen
    /// open of another folder, or `close_all`.
    Held,
}

struct Slot {
    /// This slot's own number: a lease of an earlier slot of the folder changes nothing here.
    id: u64,
    phase: Phase,
    screen: Screen,
    side: u32,
    writes: u32,
    /// The person went Home from this session's screen. A read-only session that was left is
    /// never joined by a screen open again: the person may have dealt with what made it
    /// read-only ("That computer is closed", C1b), and only a fresh open looks again.
    left: bool,
}

/// A leave of the screen that has begun: the "leaving the project" checkpoint is to run
/// ([`SessionRegistry::run_leave`]).
struct Leave {
    session: Arc<ProjectSession>,
    slot: u64,
    /// `true` when the checkpoint ends.
    done: watch::Sender<bool>,
}

impl Slot {
    fn session(&self) -> Option<&Arc<ProjectSession>> {
        match &self.phase {
            Phase::Open { session, .. } => Some(session),
            Phase::Opening(_) | Phase::Closing(_) => None,
        }
    }

    fn has_screen(&self) -> bool {
        self.screen != Screen::Free
    }

    /// The screen's session at `epoch`, which answers project-scoped requests.
    fn screen_session(&self, epoch: Epoch) -> Option<&Arc<ProjectSession>> {
        self.session()
            .filter(|session| self.has_screen() && session.scope.epoch == epoch)
    }

    fn unused(&self) -> bool {
        !self.has_screen() && self.side == 0 && self.writes == 0
    }

    /// A read-only session the person left: a screen open waits for its close and opens the
    /// folder afresh, never joins it.
    fn left_read_only(&self) -> bool {
        self.left
            && self.session().is_some_and(|session| {
                matches!(session.handle.facts().mode, SessionMode::ReadOnly(_))
            })
    }

    /// The leave checkpoint or the close a caller waits for before it can join the folder.
    fn settling(&self) -> Option<watch::Receiver<bool>> {
        match &self.phase {
            Phase::Open {
                leaving: Some(done),
                ..
            }
            | Phase::Closing(done) => Some(done.clone()),
            Phase::Opening(_) | Phase::Open { leaving: None, .. } => None,
        }
    }

    /// The close at Home begins: the screen is no one's from now on, and an open session's
    /// leave checkpoint is to run. `None` while the folder is still opening: the open closes
    /// it if no one else uses it.
    fn begin_leave(&mut self) -> Option<Leave> {
        self.screen = Screen::Free;
        let Phase::Open {
            session,
            leaving: None,
        } = &self.phase
        else {
            return None;
        };
        self.left = true;
        let session = session.clone();
        let (done, rx) = watch::channel(false);
        self.phase = Phase::Open {
            session: session.clone(),
            leaving: Some(rx),
        };
        Some(Leave {
            session,
            slot: self.id,
            done,
        })
    }
}

/// When a mode switch checkpoints (D1, Q-5): every switch (R17, once P measures R9), or at most
/// once in a span, the fallback used until then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointPolicy {
    EveryModeSwitch,
    AtMostEvery(std::time::Duration),
}

/// The policy in force: at most once every two minutes, so a checker who switches mode after
/// every decision does not wait behind a checkpoint each time (Q-5's fallback).
pub const MODE_SWITCH_POLICY: CheckpointPolicy =
    CheckpointPolicy::AtMostEvery(std::time::Duration::from_secs(120));

/// Whether a checkpoint for `reason` runs now: leaving the project always checks for changes;
/// a mode switch follows `policy`, from the start of the folder's last mode-switch checkpoint.
pub fn checkpoint_due(
    policy: CheckpointPolicy,
    last: Option<WallMs>,
    now: WallMs,
    reason: &CheckpointReason,
) -> bool {
    match reason {
        CheckpointReason::LeavingProject | CheckpointReason::Other(_) => true,
        CheckpointReason::LeavingTranslate | CheckpointReason::LeavingMode(_) => match policy {
            CheckpointPolicy::EveryModeSwitch => true,
            CheckpointPolicy::AtMostEvery(span) => last.is_none_or(|last| {
                u128::try_from(now.0.saturating_sub(last.0)).unwrap_or(0) >= span.as_millis()
            }),
        },
    }
}

/// Where a folder's last checkpoint stands (D1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckpointState {
    /// None has run in this server run.
    Never,
    Running,
    Ended(CheckpointOutcome),
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
    /// Each folder's last checkpoint, kept after its session closes so a reload finds it (D1).
    checkpoints: Mutex<BTreeMap<ProjectRef, CheckpointState>>,
    /// When each folder's last mode-switch checkpoint started (D1, the policy's clock).
    switched: Mutex<BTreeMap<ProjectRef, WallMs>>,
    /// Woken whenever an open, a leave checkpoint or a close ends.
    changed: tokio::sync::Notify,
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

/// What one look at a folder under the registry's lock gives a caller who wants to join it.
enum Step {
    /// The session is open: the caller is one of its users.
    Joined(Arc<ProjectSession>, UserGuard),
    /// The folder's open runs: the caller is counted and waits for its outcome.
    Open(watch::Receiver<OpenResult>, UserGuard),
    /// A leave checkpoint or a close of the folder runs: the caller waits, then looks again.
    Settle(watch::Receiver<bool>),
    /// Another folder's screen open runs: a screen open waits for it, then leaves it.
    OtherScreen(ProjectRef),
    /// A read-only session the person left still has users: a screen open waits until it
    /// closes, then looks again.
    Closing,
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
                checkpoints: Mutex::new(BTreeMap::new()),
                switched: Mutex::new(BTreeMap::new()),
                changed: tokio::sync::Notify::new(),
            }),
        }
    }

    /// INV-A-LEAVE-WITH-CHANGES-MAKES-ONE-COMMIT (D1): the one caller of
    /// `ProjectHandle::checkpoint`. It keeps the folder's state, reports a failure, and tells
    /// the screens the outcome.
    pub async fn checkpoint(
        &self,
        session: &ProjectSession,
        reason: CheckpointReason,
    ) -> CheckpointOutcome {
        let folder = session.scope.project.clone();
        let label = match &reason {
            CheckpointReason::LeavingProject => "leavingProject",
            CheckpointReason::LeavingTranslate | CheckpointReason::LeavingMode(_) => "leavingMode",
            CheckpointReason::Other(_) => "other",
        };
        let before = lock(&self.inner.checkpoints).insert(folder.clone(), CheckpointState::Running);
        let started = self.inner.clock.now();
        let mut outcome = session.handle.checkpoint(reason).await;
        // A read-only project (a second writer appeared) skips the checkpoint without looking:
        // an earlier failure's work may still be missing from the history, so it stays shown.
        if let (
            CheckpointOutcome::Unchanged,
            Some(CheckpointState::Ended(failed @ CheckpointOutcome::Failed { .. })),
        ) = (&outcome, before)
            && matches!(session.handle.facts().mode, SessionMode::ReadOnly(_))
        {
            outcome = failed;
        }
        lock(&self.inner.checkpoints)
            .insert(folder.clone(), CheckpointState::Ended(outcome.clone()));
        if let CheckpointOutcome::Failed { reason } = &outcome {
            self.inner.reports.add(
                ReportOp::Checkpoint,
                ReportOutcome::Failed {
                    reason: reason.clone(),
                },
                facts(json!({"project": folder.as_str(), "reason": label})),
                started,
                self.inner.clock.now(),
            );
        }
        self.inner.bus.publish(Event::Session {
            at: session.scope.clone(),
            event: SessionEvent::Checkpoint(outcome.clone()),
        });
        outcome
    }

    /// A mode switch (`session/leave`): the checkpoint runs when the policy says it is due.
    pub async fn mode_switch(
        &self,
        session: &ProjectSession,
        reason: CheckpointReason,
    ) -> Option<CheckpointOutcome> {
        let folder = session.scope.project.clone();
        let now = self.inner.clock.now();
        {
            let mut switched = lock(&self.inner.switched);
            let last = switched.get(&folder).copied();
            if !checkpoint_due(MODE_SWITCH_POLICY, last, now, &reason) {
                return None;
            }
            switched.insert(folder, now);
        }
        Some(self.checkpoint(session, reason).await)
    }

    /// The session's outside edits as the store lists them now, told to every page (D1).
    pub fn publish_outside(&self, session: &ProjectSession) -> tc_api::session::OutsideEdits {
        let outside = session.handle.outside_edits();
        self.inner.bus.publish(Event::Session {
            at: session.scope.clone(),
            event: SessionEvent::OutsideEdits(outside.clone()),
        });
        outside
    }

    /// The projects whose last checkpoint failed, for `hello`.
    pub fn failed_checkpoints(&self) -> Vec<tc_api::app::FailedCheckpoint> {
        lock(&self.inner.checkpoints)
            .iter()
            .filter_map(|(project, state)| match state {
                CheckpointState::Ended(CheckpointOutcome::Failed { reason }) => {
                    Some(tc_api::app::FailedCheckpoint {
                        project: project.clone(),
                        reason: reason.clone(),
                    })
                }
                _ => None,
            })
            .collect()
    }

    /// Where `folder`'s last checkpoint stands.
    pub fn checkpoint_state(&self, folder: &ProjectRef) -> CheckpointState {
        lock(&self.inner.checkpoints)
            .get(folder)
            .cloned()
            .unwrap_or(CheckpointState::Never)
    }

    /// The project store every session is opened from (Home lists its projects through it).
    pub fn store(&self) -> &Arc<dyn ProjectStore> {
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
    /// is still running. A scope it returns may already be stale, when a screen open of
    /// another folder or a quit came while it waited.
    pub async fn open_screen(
        &self,
        folder: &ProjectRef,
        progress: ProgressSink,
    ) -> Result<OpenSession, OpenFailure> {
        let (session, mut guard) = self.join(folder, Role::Screen, progress).await?;
        self.claim_screen(folder, guard.slot);
        // The screen stays a user until it leaves: `leave_screen`, a screen open elsewhere,
        // or `close_all`.
        guard.armed = false;
        Ok(OpenSession {
            at: session.scope.clone(),
            id: session.id.clone(),
            mode: session.handle.facts().mode,
            role: SessionRole::Screen,
            outside: session.handle.outside_edits(),
        })
    }

    /// A screen open returns: the screen is the person's, unless it was given away while the
    /// open waited.
    fn claim_screen(&self, folder: &ProjectRef, slot_id: u64) {
        let mut slots = lock(&self.inner.slots);
        if let Some(slot) = slots.get_mut(folder).filter(|slot| slot.id == slot_id)
            && matches!(slot.screen, Screen::Waiting(_))
        {
            slot.screen = Screen::Held;
        }
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
            let mut leaves = Vec::new();
            // Armed before the look, so a close that ends right after it still wakes this
            // caller.
            let changed = self.inner.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let step = {
                let mut slots = lock(&self.inner.slots);
                self.enter(&mut slots, folder, role, &progress, &mut leaves)
            };
            for leave in leaves {
                self.run_leave(leave);
            }
            match step {
                Step::Joined(session, guard) => return Ok((session, guard)),
                Step::Settle(mut done) => {
                    // A closed channel means the step ended too.
                    let _ = done.wait_for(|done| *done).await;
                }
                Step::OtherScreen(other) => self.settled(&other).await,
                Step::Closing => changed.await,
                Step::Open(mut rx, guard) => {
                    let outcome = match rx.wait_for(Option::is_some).await {
                        Ok(outcome) => outcome.clone(),
                        Err(_) => Some(Err(OpenFailure::Unreadable {
                            reason: "the open stopped without an outcome".to_owned(),
                        })),
                    };
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

    /// One look at `folder` under the registry's lock. A screen open checks the other folders
    /// and takes the screen in this same look, so two screen opens of two folders at once
    /// never both find no other screen: the second sees the first. The leaves it begins are
    /// left in `leaves`, to run once the lock is let go.
    fn enter(
        &self,
        slots: &mut BTreeMap<ProjectRef, Slot>,
        folder: &ProjectRef,
        role: Role,
        progress: &ProgressSink,
        leaves: &mut Vec<Leave>,
    ) -> Step {
        if let Some(done) = slots.get(folder).and_then(Slot::settling) {
            return Step::Settle(done);
        }
        if role == Role::Screen {
            // Another folder's screen open is running: it ends first, then it is left.
            if let Some(other) = slots
                .iter()
                .find(|(other, slot)| {
                    *other != folder && slot.has_screen() && matches!(slot.phase, Phase::Opening(_))
                })
                .map(|(other, _)| other.clone())
            {
                return Step::OtherScreen(other);
            }
            leaves.extend(
                slots
                    .iter_mut()
                    .filter(|(other, slot)| *other != folder && slot.has_screen())
                    .filter_map(|(_, slot)| slot.begin_leave()),
            );
        }
        match slots.get_mut(folder) {
            None => {
                let id = self.inner.slot_ids.fetch_add(1, Ordering::SeqCst);
                let (tx, rx) = watch::channel(None);
                let mut slot = Slot {
                    id,
                    phase: Phase::Opening(rx.clone()),
                    screen: Screen::Free,
                    side: 0,
                    writes: 0,
                    left: false,
                };
                let guard = self.register(&mut slot, folder, role);
                slots.insert(folder.clone(), slot);
                self.spawn_open(folder.clone(), id, tx, progress.clone());
                Step::Open(rx, guard)
            }
            Some(slot) => match &slot.phase {
                Phase::Opening(rx) => {
                    let rx = rx.clone();
                    Step::Open(rx, self.register(slot, folder, role))
                }
                Phase::Open { leaving: None, .. }
                    if role == Role::Screen && slot.left_read_only() =>
                {
                    Step::Closing
                }
                Phase::Open {
                    session,
                    leaving: None,
                } => {
                    let session = session.clone();
                    Step::Joined(session, self.register(slot, folder, role))
                }
                Phase::Open {
                    leaving: Some(done),
                    ..
                }
                | Phase::Closing(done) => Step::Settle(done.clone()),
            },
        }
    }

    /// Counts a user in `slot` and hands back the guard that counts it out.
    fn register(&self, slot: &mut Slot, folder: &ProjectRef, role: Role) -> UserGuard {
        let mut guard = self.guard(folder, slot.id, role);
        match (role, slot.screen) {
            // The screen is the person's already: this caller joins it and owns nothing.
            (Role::Screen, Screen::Held) => guard.armed = false,
            // The session is open, so the caller returns without waiting: the screen is the
            // person's now. Only screen opens that wait for the folder's open are counted, so
            // the guard of one whose screen was given away never counts out a later one.
            (Role::Screen, _) if slot.session().is_some() => {
                slot.screen = Screen::Held;
                guard.armed = false;
            }
            (Role::Screen, Screen::Free) => slot.screen = Screen::Waiting(1),
            (Role::Screen, Screen::Waiting(n)) => slot.screen = Screen::Waiting(n + 1),
            (Role::Side, _) => slot.side += 1,
            (Role::Write, _) => slot.writes += 1,
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
            registry.inner.changed.notify_waiters();
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
            books: Default::default(),
            checks: Default::default(),
            align: Default::default(),
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
        let leave = {
            let mut slots = lock(&self.inner.slots);
            let slot = slots.get_mut(&at.project).ok_or(StaleScope)?;
            slot.screen_session(at.epoch).ok_or(StaleScope)?;
            slot.begin_leave().ok_or(StaleScope)?
        };
        self.run_leave(leave);
        Ok(())
    }

    /// Runs a leave's checkpoint behind the caller, then closes the session if no one uses it.
    fn run_leave(&self, leave: Leave) {
        let Leave {
            session,
            slot: slot_id,
            done,
        } = leave;
        let registry = self.clone();
        self.inner.runtime.spawn(async move {
            // The links of an open aligner are saved first: the leave checkpoint (and the quit's)
            // must hold them (AD18).
            session
                .align
                .finish(
                    &registry.inner.reports,
                    registry.inner.clock.now(),
                    session.scope.project.as_str(),
                )
                .await;
            registry
                .checkpoint(&session, CheckpointReason::LeavingProject)
                .await;
            // Home is already showing: its card was read before this checkpoint wrote the
            // files its counts come from (a kept verse's conflict among them, J18).
            registry.inner.bus.publish(Event::HomeCard {
                project: session.scope.project.clone(),
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
            registry.inner.changed.notify_waiters();
            registry.maybe_close(&session.scope.project, slot_id);
        });
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
                .filter(|_| slot.has_screen())
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
            .filter(|slot| slot.has_screen() || slot.side > 0)
            .filter_map(|slot| {
                let session = slot.session()?;
                Some(OpenSession {
                    at: session.scope.clone(),
                    id: session.id.clone(),
                    mode: session.handle.facts().mode,
                    role: if slot.has_screen() {
                        SessionRole::Screen
                    } else {
                        SessionRole::Side
                    },
                    outside: session.handle.outside_edits(),
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
    /// go, and every session closes once its writes are done. Resolves when no session is
    /// left, an open that was running included.
    pub async fn close_all(&self) {
        loop {
            let changed = self.inner.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let (screens, folders) = {
                let mut slots = lock(&self.inner.slots);
                for slot in slots.values_mut() {
                    // Their leases count out of a slot that no longer counts them.
                    slot.side = 0;
                }
                let screens: Vec<Scope> = slots
                    .values()
                    .filter(|slot| slot.has_screen())
                    .filter_map(|slot| slot.session().map(|s| s.scope.clone()))
                    .collect();
                let folders: Vec<(ProjectRef, u64)> = slots
                    .iter()
                    .map(|(folder, slot)| (folder.clone(), slot.id))
                    .collect();
                (screens, folders)
            };
            if folders.is_empty() {
                return;
            }
            for scope in &screens {
                let _ = self.leave_screen(scope);
            }
            for (folder, slot_id) in &folders {
                self.maybe_close(folder, *slot_id);
            }
            // An open, a checkpoint, a write or a close ends: look again.
            changed.await;
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
                // A screen open that went away before it returned.
                Role::Screen => match slot.screen {
                    Screen::Waiting(n) if n > 1 => {
                        slot.screen = Screen::Waiting(n - 1);
                        None
                    }
                    // The last one: no one has the screen, and an open session is left.
                    Screen::Waiting(_) => slot.begin_leave(),
                    // Another screen open returned, or the screen was given away: none of
                    // it was this caller's.
                    Screen::Held | Screen::Free => None,
                },
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
        if let Some(leave) = leave {
            self.run_leave(leave);
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
            registry.inner.changed.notify_waiters();
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
    session.checks().begin(previous.seq());
    while let Some(event) = events.recv().await {
        let event = match event {
            StoreEvent::Committed { seq, delta, cause } => {
                // Logged before the UI hears of it, so the rows it asks for re-assess only
                // what changed.
                session.checks().note(seq, &delta);
                let newest = session.read();
                let me = session.handle.facts().device;
                let labels = std::cell::OnceCell::new();
                let label = |device: &tc_journal::DeviceId| {
                    if *device == me {
                        Some(wire::scope::person_label(
                            &tc_journal::fold::PersonLabel::Mine,
                        ))
                    } else {
                        // Local saves need no other device's profile. Numbering an unknown
                        // teammate is project-wide work, deferred until a change names one.
                        labels
                            .get_or_init(|| newest.person_labels(me))
                            .get(device)
                            .map(wire::scope::person_label)
                    }
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
            // A file was held, or a hold ended: the question and the kept copies change (D1).
            StoreEvent::Notice(tc_store::StoreNotice {
                notice: tc_journal::fold::Notice::OutsideEditHeld { .. },
                ..
            }) => SessionEvent::OutsideEdits(session.handle.outside_edits()),
            // The conflict review's notices read the store's list; a skipped event never ends
            // the pump.
            StoreEvent::Notice(_) | StoreEvent::Materialized { .. } => continue,
        };
        bus.publish(Event::Session {
            at: session.scope.clone(),
            event,
        });
    }
}
