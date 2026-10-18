//! The surface the application layer uses (APP-DESIGN §4.2, R4, R5, R16): an O(1) snapshot
//! with its sequence number, minting and parsing tokens, an asynchronous submit whose reply
//! comes after the durability point, typed events, drafts, leaving a book, checkpoints,
//! counters and close.
//!
//! Reads never wait behind the actor (R5): the actor publishes a clone of its fold after
//! every batch, which costs a few hundred pointer copies (§2.6), and a read takes the newest
//! one. Commands go to the actor's queue; drafts go to a writer of their own (R22).

use crate::counters::{Counters, StoreCounters};
use crate::drafts::{DraftWriter, OfferedDraft};
use crate::error::StoreError;
use crate::hooks::{Hooks, Point};
use crate::notices::StoreNotice;
use crate::reply::{CommandError, ReadOnlyReason, StoreReply, Stored};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::task::{Context, Poll, Waker};
use std::time::Instant;
use tc_journal::command::Command;
use tc_journal::fold::{Delta, FoldState, ProjectKind};
use tc_journal::token::{TokenError, TokenKind};
use tc_journal::{BookId, ChangeId, DeviceId, GitOid, ProjectId, RecordKey, ShownToken, TokenKey};

// ---------------------------------------------------------------------------------------
// A value that arrives later
// ---------------------------------------------------------------------------------------

enum SlotState<T> {
    Waiting(Option<Waker>),
    Ready(T),
    Taken,
}

struct Slot<T> {
    state: Mutex<SlotState<T>>,
    ready: Condvar,
}

/// A value that arrives later, from another thread: block with [`Pending::wait`] or
/// `.await` it.
pub struct Pending<T> {
    slot: Arc<Slot<T>>,
}

/// The sending half of a [`Pending`]. Dropped unresolved, it resolves with the value given
/// at creation for that case.
pub(crate) struct Resolver<T> {
    slot: Arc<Slot<T>>,
    on_drop: Option<Box<dyn FnOnce() -> T + Send>>,
}

pub(crate) fn pending<T: Send + 'static>(
    on_drop: impl FnOnce() -> T + Send + 'static,
) -> (Pending<T>, Resolver<T>) {
    let slot = Arc::new(Slot {
        state: Mutex::new(SlotState::Waiting(None)),
        ready: Condvar::new(),
    });
    (
        Pending { slot: slot.clone() },
        Resolver {
            slot,
            on_drop: Some(Box::new(on_drop)),
        },
    )
}

/// A value that is there already.
pub(crate) fn ready<T: Send + 'static>(value: T) -> Pending<T> {
    let (pending, resolver) = pending::<T>(|| unreachable_value());
    resolver.resolve(value);
    pending
}

fn unreachable_value<T>() -> T {
    // `ready` resolves at once, so its drop value is never asked for. Proof: `resolve`
    // takes `on_drop` before the resolver is dropped.
    #[allow(clippy::panic)]
    {
        panic!("a ready value has no drop value")
    }
}

impl<T> Resolver<T> {
    pub(crate) fn resolve(mut self, value: T) {
        self.on_drop = None;
        Self::set(&self.slot, value);
    }

    fn set(slot: &Slot<T>, value: T) {
        let mut state = slot.state.lock().unwrap_or_else(|e| e.into_inner());
        let waker = match std::mem::replace(&mut *state, SlotState::Ready(value)) {
            SlotState::Waiting(waker) => waker,
            _ => None,
        };
        drop(state);
        slot.ready.notify_all();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl<T> Drop for Resolver<T> {
    fn drop(&mut self) {
        if let Some(on_drop) = self.on_drop.take() {
            Self::set(&self.slot, on_drop());
        }
    }
}

impl<T> Pending<T> {
    /// Blocks until the value arrives.
    pub fn wait(self) -> T {
        let mut state = self.slot.state.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            match std::mem::replace(&mut *state, SlotState::Taken) {
                SlotState::Ready(value) => return value,
                other => {
                    *state = other;
                    state = self
                        .slot
                        .ready
                        .wait(state)
                        .unwrap_or_else(|e| e.into_inner());
                }
            }
        }
    }

    /// Whether the value has arrived.
    pub fn is_ready(&self) -> bool {
        matches!(
            *self.slot.state.lock().unwrap_or_else(|e| e.into_inner()),
            SlotState::Ready(_)
        )
    }
}

impl<T> Future for Pending<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let mut state = self.slot.state.lock().unwrap_or_else(|e| e.into_inner());
        match std::mem::replace(&mut *state, SlotState::Taken) {
            SlotState::Ready(value) => Poll::Ready(value),
            SlotState::Waiting(_) | SlotState::Taken => {
                *state = SlotState::Waiting(Some(cx.waker().clone()));
                Poll::Pending
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// Snapshots, events, facts
// ---------------------------------------------------------------------------------------

/// The fold at one moment, with the sequence number the actor gave it (R4). Taking one is
/// an `Arc` clone.
#[derive(Clone)]
pub struct Snapshot {
    inner: Arc<SnapshotInner>,
}

struct SnapshotInner {
    fold: FoldState,
    seq: u64,
    signing: Arc<TokenKey>,
}

impl std::fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Snapshot(seq {})", self.inner.seq)
    }
}

impl Snapshot {
    pub fn fold(&self) -> &FoldState {
        &self.inner.fold
    }

    pub fn seq(&self) -> u64 {
        self.inner.seq
    }

    /// An edit token for `key` as this snapshot shows it (§2.7): mint from the snapshot the
    /// screen read, so the token lists what the screen displayed.
    pub fn token(
        &self,
        key: &RecordKey,
        displayed: Option<ChangeId>,
    ) -> Result<ShownToken, TokenError> {
        self.inner.fold.token(&self.inner.signing, key, displayed)
    }

    /// A review token: every head of `key` (R19).
    pub fn review_token(&self, key: &RecordKey) -> Result<ShownToken, TokenError> {
        self.inner.fold.review_token(&self.inner.signing, key)
    }
}

/// Whether the project can be changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Writable,
    ReadOnly(ReadOnlyReason),
}

/// What a handle says of its project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFacts {
    pub id: ProjectId,
    pub kind: ProjectKind,
    pub device: DeviceId,
    pub folder: PathBuf,
    pub mode: Mode,
}

/// What happened in a project (R4, R16).
#[derive(Debug, Clone)]
pub enum StoreEvent {
    /// A batch was folded: a command's, or one the store wrote on its own.
    Committed {
        seq: u64,
        delta: Delta,
        changes: Vec<ChangeId>,
    },
    /// A notice for the person (§2.6), typed: `SecondWriter`, `SavedWithDiskError`, …
    Notice(StoreNotice),
    /// The project became read-only.
    Mode(Mode),
    /// Derived files were written (§5.10).
    Materialized { paths: Vec<String> },
    /// A long action reports where it is (PERFORMANCE.md rule 4): the first report comes
    /// 300 ms after the action began, and none if it ends sooner.
    Progress {
        work: Work,
        progress: crate::progress::Progress,
    },
    /// A checkpoint made a commit and moved the branch to it (§5.13 C5).
    Checkpointed { commit: GitOid },
}

/// What a [`StoreEvent::Progress`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Work {
    /// Derived files are written book by book: a book added, removed, restored or
    /// rescoped, a pin change, a resource upgrade, a sync's intake, or the open's first
    /// write of many books.
    Materialize,
}

/// A book's text read from one snapshot (`docs/TC4-DELTA-2026-10-16.md` §3.10 item 2): a
/// pending book read is a value the screen waits for, never an empty book.
#[derive(Debug, Clone)]
pub struct BookRead {
    pub snapshot: Snapshot,
    pub book: BookId,
    /// `None` when the book does not project.
    pub usfm: Option<String>,
}

/// State the handle and the actor share.
pub(crate) struct Shared {
    snapshot: RwLock<Snapshot>,
    seq: AtomicU64,
    pub signing: Arc<TokenKey>,
    facts: RwLock<ProjectFacts>,
    subscribers: Mutex<Vec<Sender<StoreEvent>>>,
    notices: Mutex<Vec<StoreNotice>>,
    next_notice: AtomicU64,
    pub counters: Arc<Counters>,
    pub hooks: Arc<Hooks>,
    /// Saves in flight, which the writer's flushes wait behind (PERFORMANCE.md).
    pub gate: Arc<crate::pace::SaveGate>,
    jobs: Mutex<u32>,
    jobs_done: Condvar,
    /// Messages and jobs the actor and the writer have taken up, `Sync` aside: `wait_idle`
    /// asks again until a round of syncs finds this unchanged.
    traffic: AtomicU64,
    /// Write groups the actor has opened that the writer has not closed: a checkpoint's seal
    /// writes packs that `metadata.json` lists only after the files that follow it (I7).
    open_groups: AtomicU32,
}

impl Shared {
    pub(crate) fn new(
        fold: FoldState,
        signing: Arc<TokenKey>,
        facts: ProjectFacts,
        counters: Arc<Counters>,
        hooks: Arc<Hooks>,
    ) -> Self {
        Self {
            snapshot: RwLock::new(Snapshot {
                inner: Arc::new(SnapshotInner {
                    fold,
                    seq: 1,
                    signing: signing.clone(),
                }),
            }),
            seq: AtomicU64::new(1),
            signing,
            facts: RwLock::new(facts),
            subscribers: Mutex::new(Vec::new()),
            notices: Mutex::new(Vec::new()),
            next_notice: AtomicU64::new(1),
            counters,
            hooks,
            gate: Arc::default(),
            jobs: Mutex::new(0),
            jobs_done: Condvar::new(),
            traffic: AtomicU64::new(0),
            open_groups: AtomicU32::new(0),
        }
    }

    /// A seal is about to write packs that `metadata.json` does not list yet.
    pub(crate) fn open_group(&self) {
        self.open_groups.fetch_add(1, Ordering::AcqRel);
    }

    /// The materialization that follows the seal has listed them.
    pub(crate) fn close_group(&self) {
        let _ = self
            .open_groups
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_sub(1));
    }

    pub(crate) fn group_open(&self) -> bool {
        self.open_groups.load(Ordering::Acquire) > 0
    }

    /// Notes that the actor or the writer took up a message that does work.
    pub(crate) fn note_traffic(&self) {
        self.traffic.fetch_add(1, Ordering::AcqRel);
    }

    pub(crate) fn publish(&self, fold: FoldState) -> u64 {
        let seq = self.seq.fetch_add(1, Ordering::AcqRel) + 1;
        let snapshot = Snapshot {
            inner: Arc::new(SnapshotInner {
                fold,
                seq,
                signing: self.signing.clone(),
            }),
        };
        *self.snapshot.write().unwrap_or_else(|e| e.into_inner()) = snapshot;
        seq
    }

    pub(crate) fn seq(&self) -> u64 {
        self.seq.load(Ordering::Acquire)
    }

    pub(crate) fn snapshot(&self) -> Snapshot {
        self.snapshot
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub(crate) fn facts(&self) -> ProjectFacts {
        self.facts.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub(crate) fn set_mode(&self, mode: Mode) {
        let changed = {
            let mut facts = self.facts.write().unwrap_or_else(|e| e.into_inner());
            let changed = facts.mode != mode;
            facts.mode = mode.clone();
            changed
        };
        if changed {
            self.emit(StoreEvent::Mode(mode));
        }
    }

    pub(crate) fn emit(&self, event: StoreEvent) {
        let mut subscribers = self.subscribers.lock().unwrap_or_else(|e| e.into_inner());
        subscribers.retain(|s| s.send(event.clone()).is_ok());
    }

    pub(crate) fn add_notice(
        &self,
        notice: tc_journal::fold::Notice,
        copy: Option<String>,
    ) -> StoreNotice {
        let notice = StoreNotice {
            id: self.next_notice.fetch_add(1, Ordering::Relaxed),
            notice,
            copy,
        };
        self.notices
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(notice.clone());
        notice
    }

    pub(crate) fn remove_notice(&self, id: u64) -> Option<StoreNotice> {
        let mut notices = self.notices.lock().unwrap_or_else(|e| e.into_inner());
        let at = notices.iter().position(|n| n.id == id)?;
        Some(notices.remove(at))
    }

    pub(crate) fn notices(&self) -> Vec<StoreNotice> {
        self.notices
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Takes away the notices that a hold of `rel` raised: the hold ended (§6.2). The byte
    /// copy stays until the person dismisses a notice of it.
    pub(crate) fn remove_notices_of(&self, rel: &str) {
        use tc_journal::fold::Notice;
        self.notices
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|n| match &n.notice {
                Notice::OutsideEditHeld { path, .. } | Notice::StorageDamage { path, .. } => {
                    path.as_str() != rel
                }
                _ => true,
            });
    }

    /// Takes away the notices `matching` picks.
    pub(crate) fn remove_notices_matching(
        &self,
        matching: impl Fn(&tc_journal::fold::Notice) -> bool,
    ) {
        self.notices
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|n| !matching(&n.notice));
    }

    /// Takes away the blocked-materialize notice that names `path` alone: the path was
    /// written (R16).
    pub(crate) fn remove_blocked_notice(&self, path: &tc_journal::RepoPath) {
        use tc_journal::fold::Notice;
        self.remove_notices_matching(|n| {
            matches!(n, Notice::MaterializeBlocked { paths, .. } if paths.len() == 1 && paths[0] == *path)
        });
    }

    /// Whether a blocked-materialize notice for exactly these paths and this reason is
    /// listed.
    pub(crate) fn has_blocked_notice(
        &self,
        paths: &[tc_journal::RepoPath],
        reason: &tc_journal::fold::Blocked,
    ) -> bool {
        use tc_journal::fold::Notice;
        self.notices
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .any(|n| {
                matches!(&n.notice, Notice::MaterializeBlocked { paths: p, reason: r } if p == paths && r == reason)
            })
    }

    /// Whether a notice for the hold of `rel` is listed.
    pub(crate) fn has_hold_notice(&self, rel: &str) -> bool {
        use tc_journal::fold::Notice;
        self.notices
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .any(|n| match &n.notice {
                Notice::OutsideEditHeld { path, .. } | Notice::StorageDamage { path, .. } => {
                    path.as_str() == rel
                }
                _ => false,
            })
    }

    /// A background job starts; [`ProjectHandle::wait_idle`] waits for it.
    pub(crate) fn job_started(&self) {
        *self.jobs.lock().unwrap_or_else(|e| e.into_inner()) += 1;
    }

    pub(crate) fn job_done(&self) {
        let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        *jobs = jobs.saturating_sub(1);
        self.jobs_done.notify_all();
    }

    fn wait_jobs(&self) {
        let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        while *jobs > 0 {
            jobs = self.jobs_done.wait(jobs).unwrap_or_else(|e| e.into_inner());
        }
    }
}

// ---------------------------------------------------------------------------------------
// Messages to the actor
// ---------------------------------------------------------------------------------------

/// Why derived files are written now (§5.10): leaving a book or a mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leave {
    Book(BookId),
    Mode,
}

/// Why a checkpoint runs (§5.13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckpointReason {
    LeavingTranslate,
    LeavingProject,
    BeforeSync,
    BeforeExport,
    AfterImport,
    Periodic,
    Other(String),
}

impl CheckpointReason {
    /// The words of a commit message (§5.13 C5).
    pub fn words(&self) -> String {
        match self {
            Self::LeavingTranslate => "leaving Translate".into(),
            Self::LeavingProject => "leaving the project".into(),
            Self::BeforeSync => "before sync".into(),
            Self::BeforeExport => "before export".into(),
            Self::AfterImport => "after import".into(),
            Self::Periodic => "every 30 minutes".into(),
            Self::Other(words) => words.clone(),
        }
    }
}

/// What a checkpoint did (§5.13).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckpointReport {
    /// The commit the branch now points at, when this checkpoint made one.
    pub commit: Option<GitOid>,
    /// The packs the seal wrote.
    pub sealed: Vec<String>,
    /// Why no commit was made, when none was.
    pub skipped: Option<CheckpointSkip>,
}

/// Why a checkpoint made no commit. Each is a state, not a failure: the work is saved and the
/// next checkpoint tries again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointSkip {
    /// Nothing changed since the last commit (C4: no empty commits).
    NoChanges,
    /// The project is read-only.
    ReadOnly,
    /// Another installation is saving into this folder (§5.0).
    SecondWriter,
    /// The session has no repository (libgit2 refuses the folder, or it cannot be made).
    NoHistory,
    /// The seal failed: the tree's files would show changes its packs lack.
    SealFailed,
    /// An expected pack is missing or unreadable: a commit without it would remove it from
    /// Door43 (§4.4).
    PackMissing,
    /// The tree is not a valid burrito: a bug, nothing is committed (§9.4).
    TreeInvalid,
    /// The branch moved while the tree was built; the next checkpoint commits again.
    HeadMoved,
}

pub(crate) enum Msg {
    Command {
        command: Box<Command>,
        reply: Resolver<Result<StoreReply, CommandError>>,
        sent: Instant,
    },
    Materialize {
        reply: Option<Resolver<Result<(), StoreError>>>,
    },
    Checkpoint {
        reason: CheckpointReason,
        reply: Resolver<Result<CheckpointReport, StoreError>>,
    },
    /// The writer examined a changed file and plans to record it (§11.1 X4): the actor
    /// commits the plan as one batch and answers with [`crate::writer::Job::Recorded`].
    RecordOutside {
        rel: String,
        seen: tc_journal::Md5,
        plan: Box<tc_journal::outside::OutsidePlan>,
        /// How many fields of `metadata.json` that the app derives the edit changed.
        derived: u32,
        copy: String,
    },
    /// The writer built a commit object (§5.13 C5): the actor moves the branch to it, the
    /// last step of a checkpoint and the only one the actor takes after the seal (R21).
    MoveHead {
        commit: GitOid,
        /// Where the branch was when the writer read it; `None` for a branch with no commit.
        expected: Option<GitOid>,
        tree: GitOid,
        report: CheckpointReport,
        reply: Resolver<Result<CheckpointReport, StoreError>>,
    },
    /// The writer's checkpoint ended without a commit: the actor answers (a close waits for
    /// exactly this message while it serves the writer's, so neither waits for the other).
    CheckpointEnded {
        result: Result<CheckpointReport, StoreError>,
        reply: Resolver<Result<CheckpointReport, StoreError>>,
    },
    /// The writer's first job failed: the project is read-only and holds no lock (§6.1).
    WritesFailed {
        reason: String,
    },
    /// The watcher saw a pending or pack file change: the actor looks for one it does not
    /// know (a second writer, §5.0).
    JournalChanged,
    Sync {
        reply: Resolver<()>,
    },
    Close {
        reply: Resolver<Result<(), StoreError>>,
    },
}

/// An open project (see the module documentation).
pub struct ProjectHandle {
    pub(crate) shared: Arc<Shared>,
    pub(crate) tx: Mutex<Sender<Msg>>,
    /// The writer's queue: the actor never stands between a person and a derived file's
    /// notice or a held file's restore.
    pub(crate) writer: Mutex<Sender<crate::writer::Job>>,
    pub(crate) drafts: Arc<DraftWriter>,
    pub(crate) actor: Mutex<Option<std::thread::JoinHandle<()>>>,
    /// Held while `.tc/index.bin` is written.
    pub(crate) index_lock: Arc<Mutex<()>>,
    /// The watches of the folder (§11.1), stopped before anything else closes.
    pub(crate) watcher: Mutex<Option<crate::watch::Watching>>,
}

/// The work an open leaves to a background thread (§6.1 O8, O14): writing the fold cache,
/// and reading the paths O9 took as converged without reading them.
pub(crate) struct Background {
    pub writer: Sender<crate::writer::Job>,
    pub vfs: Arc<dyn crate::vfs::Vfs>,
    pub counters: Arc<Counters>,
    pub folder: crate::layout::Folder,
    pub index: Option<Vec<tc_journal::index::Section>>,
    pub project: ProjectId,
    pub check: Vec<String>,
    pub stamp_ns: i64,
}

impl std::fmt::Debug for ProjectHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ProjectHandle({:?})", self.shared.facts())
    }
}

impl ProjectHandle {
    fn send(&self, msg: Msg) -> bool {
        self.tx
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .send(msg)
            .is_ok()
    }

    fn send_writer(&self, job: crate::writer::Job) -> bool {
        self.writer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .send(job)
            .is_ok()
    }

    pub fn facts(&self) -> ProjectFacts {
        self.shared.facts()
    }

    /// The newest published snapshot (R5).
    pub fn snapshot(&self) -> Snapshot {
        self.shared.snapshot()
    }

    /// An edit token for `key` as the newest snapshot shows it. A screen that drew from an
    /// older snapshot mints from that snapshot ([`Snapshot::token`]).
    pub fn mint(
        &self,
        key: &RecordKey,
        displayed: Option<ChangeId>,
    ) -> Result<ShownToken, TokenError> {
        self.snapshot().token(key, displayed)
    }

    pub fn mint_review(&self, key: &RecordKey) -> Result<ShownToken, TokenError> {
        self.snapshot().review_token(key)
    }

    fn parse_kind(&self, text: &str, kind: TokenKind) -> Result<ShownToken, TokenError> {
        let token = ShownToken::parse(&self.shared.signing, text)?;
        if token.kind() != kind {
            return Err(TokenError::Malformed(format!(
                "a {} token where a {} token was expected",
                if token.kind() == TokenKind::Edit {
                    "edit"
                } else {
                    "review"
                },
                if kind == TokenKind::Edit {
                    "edit"
                } else {
                    "review"
                },
            )));
        }
        Ok(token)
    }

    /// Reads an edit token the UI sent back, refusing a forged one or a review token.
    pub fn parse_edit(&self, text: &str) -> Result<ShownToken, TokenError> {
        self.parse_kind(text, TokenKind::Edit)
    }

    pub fn parse_review(&self, text: &str) -> Result<ShownToken, TokenError> {
        self.parse_kind(text, TokenKind::Review)
    }

    /// Hands a command to the actor. The reply comes after the batch is durable and folded
    /// (B6); the caller never blocks. A draft goes to the draft writer instead (R22).
    pub fn submit(&self, command: Command) -> Pending<Result<StoreReply, CommandError>> {
        if let Command::Draft(draft) = command {
            if let Mode::ReadOnly(reason) = self.facts().mode {
                return ready(Err(CommandError::ReadOnly(reason)));
            }
            self.drafts.put(draft);
            return ready(Ok(StoreReply {
                tokens: Vec::new(),
                new_conflicts: Vec::new(),
                stored: Stored::Unchanged,
                seq: self.shared.seq(),
                changes: Vec::new(),
                skipped: Vec::new(),
                kept: Vec::new(),
            }));
        }
        let (pending, resolver) = pending(|| Err(CommandError::Closed));
        let sent = self.send(Msg::Command {
            command: Box::new(command),
            reply: resolver,
            sent: Instant::now(),
        });
        let _ = sent;
        pending
    }

    pub fn subscribe(&self) -> Receiver<StoreEvent> {
        let (tx, rx) = mpsc::channel();
        self.shared
            .subscribers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(tx);
        rx
    }

    /// The notices listed now (§2.6): file and machine facts the fold does not list.
    pub fn notices(&self) -> Vec<StoreNotice> {
        self.shared.notices()
    }

    /// Dismisses a notice; its byte copy, if any, is removed (§3.6).
    pub fn dismiss_notice(&self, id: u64) -> Pending<Result<(), StoreError>> {
        let (pending, resolver) =
            pending(|| Err(StoreError::Other("the project is closed".into())));
        self.send_writer(crate::writer::Job::DismissNotice {
            id,
            reply: resolver,
        });
        pending
    }

    /// *Restore from the project* for a held file (§6.2): it is written from the project's
    /// records, and the hold ends.
    pub fn restore_held(&self, path: &str) -> Pending<Result<(), StoreError>> {
        let (pending, resolver) =
            pending(|| Err(StoreError::Other("the project is closed".into())));
        self.send_writer(crate::writer::Job::RestoreHeld {
            rel: path.to_owned(),
            reply: resolver,
        });
        pending
    }

    /// The window got focus (§11.1): the writer stats every path the app wrote, reads those
    /// that are not clean by stat, lists the folders for files that are new, and hands what
    /// differs from the project to be recorded. It projects no file. Resolves when it has
    /// looked; what it handed over is durable after [`ProjectHandle::wait_idle`].
    pub fn focus(&self) -> Pending<Result<crate::reply::FocusReport, StoreError>> {
        let (pending, resolver) =
            pending(|| Err(StoreError::Other("the project is closed".into())));
        self.send_writer(crate::writer::Job::Focus { reply: resolver });
        pending
    }

    /// *Take in the file as an edit* for a held file (§6.2, APP-DESIGN R7): its records are
    /// recorded with no base, each one that differs from the project's a conflict with its
    /// text, and the hold ends. Resolves when the batch is durable.
    pub fn take_in_held(&self, path: &str) -> Pending<Result<crate::reply::TakenIn, StoreError>> {
        let (pending, resolver) =
            pending(|| Err(StoreError::Other("the project is closed".into())));
        self.send_writer(crate::writer::Job::TakeInHeld {
            rel: path.to_owned(),
            reply: resolver,
        });
        pending
    }

    /// Starts the open's background work (see [`Background`]).
    pub(crate) fn start_background(&self, job: Background) {
        let shared = self.shared.clone();
        let index_lock = self.index_lock.clone();
        shared.job_started();
        let spawned = std::thread::Builder::new()
            .name("tc-store background".into())
            .spawn(move || {
                if let Some(sections) = job.index {
                    let bytes = tc_journal::index::Index::encode(job.project, sections.iter());
                    let _guard = index_lock.lock().unwrap_or_else(|e| e.into_inner());
                    let dir = job.folder.path(crate::layout::TC);
                    // A failed write only costs the next open the parse of those files.
                    let _ = crate::atomic::atomic_write(
                        &*job.vfs,
                        &dir,
                        "index.bin",
                        &bytes,
                        crate::atomic::SyncDir::Now,
                    );
                }
                if !job.check.is_empty() {
                    let results = crate::open::storage_check(
                        &*job.vfs,
                        &job.counters,
                        &job.folder,
                        &job.check,
                        job.stamp_ns,
                    );
                    let _ = job.writer.send(crate::writer::Job::StorageCheck(results));
                    let (pending, resolver) = pending(|| ());
                    if job
                        .writer
                        .send(crate::writer::Job::Sync { reply: resolver })
                        .is_ok()
                    {
                        pending.wait();
                    }
                }
                shared.job_done();
            });
        if spawned.is_err() {
            self.shared.job_done();
        }
    }

    /// Drafts from the last session whose text differs from what their records hold (O12).
    pub fn offered_drafts(&self) -> Vec<OfferedDraft> {
        self.drafts.offered()
    }

    /// Discards an offered draft.
    pub fn discard_draft(&self, id: u64) {
        self.drafts.discard(id);
    }

    /// Leaving a book or a mode: derived files are written now (§5.10).
    pub fn leave(&self, _what: Leave) -> Pending<Result<(), StoreError>> {
        let (pending, resolver) =
            pending(|| Err(StoreError::Other("the project is closed".into())));
        self.send(Msg::Materialize {
            reply: Some(resolver),
        });
        pending
    }

    /// Seal, materialize and commit (§5.13). Only the seal and the move of the branch run on
    /// the actor; the files, the tree and the commit object are the writer's work from a
    /// snapshot, so saves go on meanwhile (R21).
    pub fn checkpoint(
        &self,
        reason: CheckpointReason,
    ) -> Pending<Result<CheckpointReport, StoreError>> {
        let (pending, resolver) =
            pending(|| Err(StoreError::Other("the project is closed".into())));
        self.send(Msg::Checkpoint {
            reason,
            reply: resolver,
        });
        pending
    }

    pub fn counters(&self) -> StoreCounters {
        self.shared.counters.read()
    }

    /// A book's text from the newest snapshot, read off the caller's thread.
    pub fn read_book(&self, book: BookId) -> Pending<BookRead> {
        let snapshot = self.snapshot();
        let shared = self.shared.clone();
        let (pending, resolver) = pending(move || BookRead {
            snapshot: snapshot.clone(),
            book,
            usfm: None,
        });
        let snapshot = self.snapshot();
        rayon::spawn(move || {
            shared.hooks.wait(Point::BeforeBookRead);
            let usfm = snapshot.fold().book_usfm(book);
            resolver.resolve(BookRead {
                snapshot,
                book,
                usfm,
            });
        });
        pending
    }

    /// Waits until the actor's queue is empty and no background job of the store runs:
    /// tests use it before they count operations or stop the writer.
    pub fn wait_idle(&self) {
        loop {
            let before = self.shared.traffic.load(Ordering::Acquire);
            let (idle, resolver) = pending(|| ());
            if self.send(Msg::Sync { reply: resolver }) {
                idle.wait();
            }
            // The writer after the actor: what the actor sent it is in its queue by now.
            let (idle, resolver) = pending(|| ());
            if self.send_writer(crate::writer::Job::Sync { reply: resolver }) {
                idle.wait();
                // And the actor again: what the writer sent it (a branch to move, a recorded
                // edit) is in its queue by now.
                let (idle, resolver) = pending(|| ());
                if self.send(Msg::Sync { reply: resolver }) {
                    idle.wait();
                }
            }
            self.shared.wait_jobs();
            // A round that took up any work may have queued more for the other thread.
            if self.shared.traffic.load(Ordering::Acquire) == before {
                break;
            }
        }
        self.drafts.flush();
    }

    /// Ends the watches and the thread that gathers what they tell, so no job reaches the
    /// writer after the project's last write.
    fn stop_watching(&self) {
        let watching = self
            .watcher
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        drop(watching);
    }

    /// Writes what is due (a seal and a materialization), releases the lock and stops the
    /// actor.
    pub fn close(self) -> Result<(), StoreError> {
        self.stop_watching();
        // The open's background work ends first, while the actor can still answer it; the
        // actor stops the draft writer and releases the lock last, so nothing is written
        // into the folder after the lock is gone (§5.0).
        self.shared.wait_jobs();
        let (pending, resolver) = pending(|| Ok(()));
        let sent = self.send(Msg::Close { reply: resolver });
        let result = if sent { pending.wait() } else { Ok(()) };
        self.drafts.stop();
        if let Some(actor) = self.actor.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = actor.join();
        }
        result
    }
}

impl Drop for ProjectHandle {
    fn drop(&mut self) {
        // A handle dropped without close still stops its actor; what was saved is in
        // pending files, and the next open seals and materializes it.
        self.stop_watching();
        self.shared.wait_jobs();
        let (_pending, resolver) = pending(|| Ok(()));
        self.send(Msg::Close { reply: resolver });
        if let Some(actor) = self.actor.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = actor.join();
        }
        self.drafts.stop();
    }
}
