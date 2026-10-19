//! The fixture backend (APP-DESIGN §4.2, AD6, AD35; feature `fixture-backend`, never in a
//! release build): projects held as folds in memory, opened through the same seam as the
//! store.
//!
//! It runs no command: a command answers `Unavailable` unless a test queued an outcome for it
//! ([`Scripted`]), so the save behaviour of the UI is testable before tc-store runs saves
//! without a second copy of STORAGE's rules. Tokens are signed with a fixed installation
//! secret, so every run mints the same text. Teammates' changes are made by tc-journal's own
//! encoder and folded as an intake would fold them (AD36).
//!
//! Like tc-store, it refuses a second open of a folder whose handle is live (STORAGE §6.1
//! O1), so the session registry's sharing is tested against a backend that would refuse it.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tc_api::event::{CheckpointOutcome, SessionMode};
use tc_api::ids::ProjectRef;
use tc_api::write::ReadOnlyReason;
use tc_journal::command::Command;
use tc_journal::fold::{Delta, FoldInput, FoldState, ProjectKind};
use tc_journal::token::TokenError;
use tc_journal::{Change, ChangeId, DeviceId, GitOid, RecordKey, ShownToken, TokenKey};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use super::seed::StateSeed;
use super::{
    BoxFuture, CheckpointReason, CommandError, EditToken, OpenFailure, ProjectEntry, ProjectFacts,
    ProjectHandle, ProjectStore, Published, ReviewToken, Snapshot, StoreCause, StoreCommand,
    StoreError, StoreEvent, StoreReply, Stored,
};
use crate::jobs::ProgressSink;

/// The installation secret of every memory backend: tokens are the same on every run and in
/// every backend, so a test can name one.
const MEMORY_INSTALLATION_SECRET: [u8; 32] = *b"tc4-memory-backend-test-secret!!";

/// The memory backend's own device in every project: "this computer", whose person is mine.
pub const MEMORY_DEVICE: [u8; 15] = [0xB1; 15];

/// Where a shared project lives on Door43: the `origin` of its remote list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Door43Remote {
    pub owner: String,
    pub repo: String,
}

/// A project the memory backend holds.
#[derive(Debug, Clone)]
pub struct MemoryProject {
    pub id: tc_journal::ProjectId,
    pub kind: ProjectKind,
    pub fold: FoldState,
    /// The bytes of carried files, by git blob id.
    pub carried: BTreeMap<GitOid, Vec<u8>>,
    pub remote: Option<Door43Remote>,
}

impl MemoryProject {
    /// A checked seed of a tC4 project (AD7).
    pub fn from_seed(seed: StateSeed, remote: Option<Door43Remote>) -> Self {
        Self {
            id: seed.project,
            kind: seed.kind,
            fold: seed.fold,
            carried: seed.carried,
            remote,
        }
    }

    /// A project with no change.
    pub fn empty(id: tc_journal::ProjectId, kind: ProjectKind) -> Self {
        Self {
            id,
            kind,
            fold: FoldState::new(kind),
            carried: BTreeMap::new(),
            remote: None,
        }
    }

    /// A project of exactly `changes`, folded by tc-journal's fold.
    pub fn from_changes(
        id: tc_journal::ProjectId,
        kind: ProjectKind,
        changes: Vec<Change>,
    ) -> Self {
        Self {
            id,
            kind,
            fold: FoldState::fold(kind, changes.into_iter().map(FoldInput::from)),
            carried: BTreeMap::new(),
            remote: None,
        }
    }
}

/// What a command submitted to an open handle does instead of running (AD6).
#[derive(Debug, Clone)]
pub enum Scripted {
    /// A fresh edit token for each displayed value; the fold does not change; durable.
    Written,
    /// As `Written`, with the disk's warning.
    WrittenNotDurable(String),
    /// The disk refused it.
    NotWritten(String),
    /// The next outcome, after this long. The reply waits; the caller never does.
    After(Duration, Box<Scripted>),
    /// The project becomes read-only (a `Mode` event): this command and every later one is
    /// refused.
    ReadOnly(ReadOnlyReason),
    /// These changes reach the fold (a new snapshot, a `Committed` event), then the next
    /// outcome.
    TeammateHeads(Vec<Change>, Box<Scripted>),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MemoryError {
    #[error("the memory backend holds no project {0}")]
    UnknownFolder(ProjectRef),
    #[error("the memory backend already holds a project {0}")]
    FolderTaken(ProjectRef),
    #[error("{0} shows no text verse to edit")]
    NotATextVerse(String),
    #[error("a teammate's change could not be made: {0}")]
    Change(String),
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Every change under these locks is a store of whole values.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A fold with the key its tokens are signed with. The fold is shared: a snapshot of each open
/// and of each step is an `Arc` clone, never a copy of the fold.
struct SignedFold {
    fold: Arc<FoldState>,
    seq: u64,
    signing: Arc<TokenKey>,
}

impl Published for SignedFold {
    fn fold(&self) -> &FoldState {
        &self.fold
    }

    fn seq(&self) -> u64 {
        self.seq
    }

    fn token(
        &self,
        key: &RecordKey,
        displayed: Option<ChangeId>,
    ) -> Result<ShownToken, TokenError> {
        self.fold.token(&self.signing, key, displayed)
    }

    fn review_token(&self, key: &RecordKey) -> Result<ShownToken, TokenError> {
        self.fold.review_token(&self.signing, key)
    }
}

/// One folder's project and what happens to it.
struct ProjectState {
    seeded: MemoryProject,
    signing: Arc<TokenKey>,
    live: Mutex<Live>,
}

struct Live {
    /// The project now: what the next open gets, and an open handle's newest snapshot.
    current: Arc<FoldState>,
    /// The open handle, by the number of its open; `None` while closed.
    open: Option<u64>,
    opens: u64,
    published: Snapshot,
    seq: u64,
    mode: SessionMode,
    scripts: VecDeque<Scripted>,
    subscribers: Vec<UnboundedSender<StoreEvent>>,
    submitted: Vec<Command>,
}

impl ProjectState {
    fn new(project: MemoryProject) -> Self {
        let mut project = project;
        // A fold cloned from a cached corpus shares its counters with every clone; each
        // project counts its own work, or parallel tests count into each other's.
        project.fold.own_counters();
        let signing = Arc::new(TokenKey::derive(&MEMORY_INSTALLATION_SECRET, project.id));
        let current = Arc::new(project.fold.clone());
        let published = Snapshot::new(SignedFold {
            fold: current.clone(),
            seq: 0,
            signing: signing.clone(),
        });
        Self {
            seeded: project,
            signing,
            live: Mutex::new(Live {
                current,
                open: None,
                opens: 0,
                published,
                seq: 0,
                mode: SessionMode::Writable,
                scripts: VecDeque::new(),
                subscribers: Vec::new(),
                submitted: Vec::new(),
            }),
        }
    }

    fn publish(&self, live: &mut Live) {
        live.published = Snapshot::new(SignedFold {
            fold: live.current.clone(),
            seq: live.seq,
            signing: self.signing.clone(),
        });
    }

    fn send(live: &mut Live, event: &StoreEvent) {
        live.subscribers.retain(|tx| tx.send(event.clone()).is_ok());
    }

    /// Folds `changes` as an intake would: an open handle publishes a new snapshot and tells
    /// its subscribers.
    fn intake(&self, changes: Vec<Change>) -> Delta {
        let devices: Vec<DeviceId> = changes
            .iter()
            .map(|c| c.body().device)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut live = lock(&self.live);
        // Snapshots taken before keep the fold they had: the step works on a copy of the
        // shared state, which shares every part it does not touch.
        let delta = Arc::make_mut(&mut live.current).add(changes.into_iter().map(FoldInput::from));
        if live.open.is_some() {
            live.seq += 1;
            self.publish(&mut live);
            let event = StoreEvent::Committed {
                seq: live.seq,
                delta: delta.clone(),
                cause: StoreCause::Intake { devices },
            };
            Self::send(&mut live, &event);
        }
        delta
    }

    fn close(&self, open: u64) {
        let mut live = lock(&self.live);
        if live.open == Some(open) {
            live.open = None;
            // The session's event pump ends with the channel.
            live.subscribers.clear();
        }
    }
}

/// Projects in memory, by folder. Clones share the projects.
#[derive(Clone, Default)]
pub struct MemoryBackend {
    projects: Arc<Mutex<BTreeMap<ProjectRef, Arc<ProjectState>>>>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self::default()
    }

    fn state(&self, folder: &ProjectRef) -> Result<Arc<ProjectState>, MemoryError> {
        lock(&self.projects)
            .get(folder)
            .cloned()
            .ok_or_else(|| MemoryError::UnknownFolder(folder.clone()))
    }

    /// Adds a project under `folder`. Its fold counts its own work from here.
    pub fn add(&self, folder: ProjectRef, project: MemoryProject) -> Result<(), MemoryError> {
        let mut projects = lock(&self.projects);
        if projects.contains_key(&folder) {
            return Err(MemoryError::FolderTaken(folder));
        }
        projects.insert(folder, Arc::new(ProjectState::new(project)));
        Ok(())
    }

    /// Queues an outcome for the next command submitted to `folder` (first in, first out).
    pub fn script(&self, folder: &ProjectRef, outcome: Scripted) -> Result<(), MemoryError> {
        lock(&self.state(folder)?.live).scripts.push_back(outcome);
        Ok(())
    }

    /// Adds teammates' changes to `folder`'s fold, as an intake would. An open handle
    /// publishes a new snapshot and sends `Committed` with the cause `Intake`.
    pub fn teammate_edits(
        &self,
        folder: &ProjectRef,
        changes: Vec<Change>,
    ) -> Result<Delta, MemoryError> {
        Ok(self.state(folder)?.intake(changes))
    }

    /// The fold the next open of `folder` gets.
    pub fn fold(&self, folder: &ProjectRef) -> Option<FoldState> {
        let state = self.state(folder).ok()?;
        let live = lock(&state.live);
        Some(FoldState::clone(&live.current))
    }

    /// The project as it was added, with its Door43 remote.
    pub fn project(&self, folder: &ProjectRef) -> Option<MemoryProject> {
        self.state(folder).ok().map(|state| state.seeded.clone())
    }

    /// Every command submitted to `folder`, oldest first, as the store would have received
    /// it: what a test of the UI's saves checks.
    pub fn submitted(&self, folder: &ProjectRef) -> Vec<Command> {
        self.state(folder)
            .map(|state| lock(&state.live).submitted.clone())
            .unwrap_or_default()
    }
}

impl ProjectStore for MemoryBackend {
    fn list(&self) -> Result<Vec<ProjectEntry>, StoreError> {
        Ok(lock(&self.projects)
            .iter()
            .map(|(folder, state)| ProjectEntry {
                folder: folder.clone(),
                id: state.seeded.id,
                kind: state.seeded.kind,
            })
            .collect())
    }

    fn open(
        &self,
        folder: &ProjectRef,
        _progress: ProgressSink,
    ) -> Result<Arc<dyn ProjectHandle>, OpenFailure> {
        let state = self.state(folder).map_err(|_| OpenFailure::NotAProject)?;
        let open = {
            let mut live = lock(&state.live);
            if live.open.is_some() {
                // tc-store's O1: this process holds the folder's lock already.
                return Err(OpenFailure::LockHeld {
                    holder: Some(("this computer".to_owned(), std::process::id())),
                });
            }
            live.opens += 1;
            let open = live.opens;
            live.open = Some(open);
            live.seq = 1;
            state.publish(&mut live);
            open
        };
        Ok(Arc::new(MemoryHandle { state, open }))
    }

    #[cfg(feature = "test-hooks")]
    fn reset(&self) -> Result<(), StoreError> {
        let projects = lock(&self.projects);
        if let Some((folder, _)) = projects
            .iter()
            .find(|(_, state)| lock(&state.live).open.is_some())
        {
            return Err(StoreError::Other(format!(
                "{folder} is open; close every session before a reset"
            )));
        }
        for state in projects.values() {
            let mut live = lock(&state.live);
            let mut fold = state.seeded.fold.clone();
            fold.own_counters();
            live.current = Arc::new(fold);
            live.scripts.clear();
            live.submitted.clear();
            live.mode = SessionMode::Writable;
        }
        Ok(())
    }
}

/// An open memory project.
struct MemoryHandle {
    state: Arc<ProjectState>,
    /// Which open of the folder this is: a handle of an earlier open closes nothing.
    open: u64,
}

impl Drop for MemoryHandle {
    fn drop(&mut self) {
        self.state.close(self.open);
    }
}

impl MemoryHandle {
    /// `outcome`, carried out for a command that showed `shown` and was submitted at
    /// `submitted`: a delay counts from the submit, not from when the reply is first awaited.
    async fn run(
        state: Arc<ProjectState>,
        shown: Vec<(RecordKey, Vec<ChangeId>)>,
        outcome: Scripted,
        submitted: tokio::time::Instant,
    ) -> Result<StoreReply, CommandError> {
        let mut outcome = outcome;
        let mut due = submitted;
        loop {
            match outcome {
                Scripted::Written => return Self::written(&state, &shown, Stored::Durable),
                Scripted::WrittenNotDurable(reason) => {
                    return Self::written(&state, &shown, Stored::WrittenNotDurable(reason));
                }
                Scripted::NotWritten(reason) => return Err(CommandError::NotWritten(reason)),
                Scripted::After(wait, next) => {
                    due += wait;
                    tokio::time::sleep_until(due).await;
                    outcome = *next;
                }
                Scripted::ReadOnly(reason) => {
                    let mode = SessionMode::ReadOnly(reason);
                    let mut live = lock(&state.live);
                    live.mode = mode;
                    ProjectState::send(&mut live, &StoreEvent::Mode(mode));
                    return Err(CommandError::ReadOnly(reason));
                }
                Scripted::TeammateHeads(changes, next) => {
                    state.intake(changes);
                    outcome = *next;
                }
            }
        }
    }

    /// A fresh edit token for each displayed value, from the newest snapshot: what the
    /// screen shows after a save that changed nothing.
    fn written(
        state: &ProjectState,
        shown: &[(RecordKey, Vec<ChangeId>)],
        stored: Stored,
    ) -> Result<StoreReply, CommandError> {
        let snapshot = lock(&state.live).published.clone();
        let tokens = shown
            .iter()
            .map(|(key, heads)| {
                let displayed = match heads.as_slice() {
                    [one] => Some(*one),
                    _ => None,
                };
                snapshot
                    .mint(key, displayed)
                    .map(|token| (key.clone(), token))
                    .map_err(CommandError::BadToken)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(StoreReply {
            tokens,
            new_conflicts: Vec::new(),
            stored,
            seq: snapshot.seq(),
            changes: Vec::new(),
        })
    }
}

impl ProjectHandle for MemoryHandle {
    fn facts(&self) -> ProjectFacts {
        ProjectFacts {
            id: self.state.seeded.id,
            kind: self.state.seeded.kind,
            device: DeviceId::local(MEMORY_DEVICE),
            mode: lock(&self.state.live).mode,
        }
    }

    fn snapshot(&self) -> Snapshot {
        lock(&self.state.live).published.clone()
    }

    fn parse_edit(&self, token: &tc_api::ShownToken) -> Result<EditToken, TokenError> {
        EditToken::parse(&self.state.signing, token)
    }

    fn parse_review(&self, token: &tc_api::ReviewToken) -> Result<ReviewToken, TokenError> {
        ReviewToken::parse(&self.state.signing, token)
    }

    fn submit(&self, command: StoreCommand) -> BoxFuture<Result<StoreReply, CommandError>> {
        let state = self.state.clone();
        let next = {
            let mut live = lock(&state.live);
            if live.open != Some(self.open) {
                Err(CommandError::Closed)
            } else {
                live.submitted.push(command.journal().clone());
                match live.mode {
                    // A read-only project refuses before the script is looked at.
                    SessionMode::ReadOnly(reason) => Err(CommandError::ReadOnly(reason)),
                    SessionMode::Writable => {
                        live.scripts.pop_front().ok_or(CommandError::Unavailable)
                    }
                }
            }
        };
        let shown = command.shown().to_vec();
        let submitted = tokio::time::Instant::now();
        Box::pin(async move {
            match next {
                Ok(outcome) => Self::run(state, shown, outcome, submitted).await,
                Err(error) => Err(error),
            }
        })
    }

    fn subscribe(&self) -> UnboundedReceiver<StoreEvent> {
        let (tx, rx) = unbounded_channel();
        let mut live = lock(&self.state.live);
        if live.open == Some(self.open) {
            live.subscribers.push(tx);
        }
        rx
    }

    fn checkpoint(&self, _reason: CheckpointReason) -> BoxFuture<CheckpointOutcome> {
        // A memory project has no git history to commit to.
        Box::pin(async { CheckpointOutcome::Unchanged })
    }

    fn close(self: Arc<Self>) -> BoxFuture<Result<(), StoreError>> {
        self.state.close(self.open);
        Box::pin(async { Ok(()) })
    }
}

/// Teammates' changes, made by tc-journal's encoder (AD36).
pub mod teammates {
    use tc_journal::record::VerseValue;
    use tc_journal::{BatchId, BatchRef, Cause, ChangeBody, DeviceSeq, Record, WallTime};

    use super::*;

    /// 2026-10-01T00:00:00Z: later than any seeded change.
    const TEAMMATE_TIME: i64 = 1_790_812_800_000;

    /// Teammate `n`'s device: a Local device whose bytes sort by `n`, so the screens number
    /// it "Teammate n" (no profile names it).
    pub fn device(n: u8) -> DeviceId {
        let mut random = [0u8; 15];
        random[0] = 0x7E;
        random[1] = n;
        DeviceId::local(random)
    }

    /// One change per text, each by its own teammate (Teammate 1, 2, … in that order), all
    /// with the record's current heads as parents: what "two teammates change Titus 1:1
    /// from the same version" writes. Each is a batch of one.
    pub fn edit_verse(
        fold: &FoldState,
        key: &RecordKey,
        texts: &[&str],
    ) -> Result<Vec<Change>, MemoryError> {
        let not_text = || MemoryError::NotATextVerse(format!("{key:?}"));
        let record = fold.projected(key).ok_or_else(not_text)?;
        let parents = fold.heads(key);
        texts
            .iter()
            .zip(1u8..)
            .map(|(text, n)| {
                let mut record = record.clone();
                let Record::Verse {
                    value: VerseValue::Text { words, .. },
                    ..
                } = &mut record
                else {
                    return Err(not_text());
                };
                *words = (*text).to_owned();
                let device = device(n);
                let digest = ring::digest::digest(
                    &ring::digest::SHA256,
                    &[
                        b"tc4-teammate-batch\0".as_slice(),
                        device.as_bytes().as_slice(),
                    ]
                    .concat(),
                );
                let mut batch = [0u8; 16];
                batch.copy_from_slice(&digest.as_ref()[..16]);
                let batch = BatchRef::new(BatchId::from_bytes(batch), 1)
                    .ok_or_else(|| MemoryError::Change("an empty batch".to_owned()))?;
                Change::new(ChangeBody {
                    v: 0,
                    record,
                    parents: parents.clone(),
                    device,
                    seq: DeviceSeq(1),
                    at: WallTime(TEAMMATE_TIME + i64::from(n) * 60_000),
                    batch,
                    cause: Cause::Edit,
                })
                .map_err(|e| MemoryError::Change(e.to_string()))
            })
            .collect()
    }
}
