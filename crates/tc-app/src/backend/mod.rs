//! The storage side of tc-app (APP-DESIGN §4.2): the one seam every read and write of a
//! project goes through, [`ProjectStore`] and [`ProjectHandle`].
//!
//! The names follow tc-store's `ProjectHandle`, `Snapshot`, `StoreReply` and `StoreEvent`, so
//! unit C0's adapter maps them one to one. What the seam adds is the split of tc-journal's one
//! token type into [`EditToken`] and [`ReviewToken`] (AD3, A-7): neither can be made outside
//! this module, a [`StoreCommand`] is built only from the kind it needs, and the journal token
//! inside never leaves the backend. A token is minted from the [`Snapshot`] a screen read, never
//! from a newer one, so it lists only what the person saw (STORAGE §2.7, I4).
//!
//! Backends: [`NoProjects`] in a build with none (release builds before C0); with the feature
//! `fixture-backend`, [`memory::MemoryBackend`] over STORAGE §13.2's state seed ([`seed`]) of
//! the parity scenarios ([`scenario`]), never in a release build (AD35).

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tc_api::event::{CheckpointOutcome, SessionMode};
use tc_api::ids::ProjectRef;
use tc_api::token::TokenTextError;
use tc_api::write::ReadOnlyReason;
use tc_journal::command::{Choice, Command};
use tc_journal::fold::{ConflictId, Delta, FoldState, ProjectKind};
use tc_journal::token::{TokenError, TokenKind};
use tc_journal::{ChangeId, DeviceId, RecordKey, ShownToken};

use crate::jobs::ProgressSink;

#[cfg(feature = "fixture-backend")]
pub mod memory;
#[cfg(feature = "fixture-backend")]
pub mod seed;

/// A future a backend hands back; the caller awaits it without blocking a thread.
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

// ---------------------------------------------------------------------------------------
// Snapshots and tokens
// ---------------------------------------------------------------------------------------

/// What a backend publishes as one snapshot: a fold, its sequence number, and the minting
/// of tokens with the project's key. The memory backend holds the key itself; tc-store's
/// own snapshot keeps its key inside tc-store, so C0 publishes that snapshot behind this.
pub(in crate::backend) trait Published: Send + Sync + 'static {
    fn fold(&self) -> &FoldState;
    fn seq(&self) -> u64;
    fn token(&self, key: &RecordKey, displayed: Option<ChangeId>)
    -> Result<ShownToken, TokenError>;
    fn review_token(&self, key: &RecordKey) -> Result<ShownToken, TokenError>;
}

/// The fold at one moment, with the sequence number of its publication. Clone is an `Arc`
/// clone, and a snapshot never changes: a newer state is a newer snapshot.
#[derive(Clone)]
pub struct Snapshot {
    inner: Arc<dyn Published>,
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Snapshot(seq {})", self.inner.seq())
    }
}

impl Snapshot {
    #[cfg(feature = "fixture-backend")]
    pub(in crate::backend) fn new(published: impl Published) -> Self {
        Self {
            inner: Arc::new(published),
        }
    }

    pub fn fold(&self) -> &FoldState {
        self.inner.fold()
    }

    pub fn seq(&self) -> u64 {
        self.inner.seq()
    }

    /// An edit token for `key` as this snapshot shows it (STORAGE §2.7): for a record in
    /// conflict, the displayed candidate only (the provisional one unless `displayed` names
    /// another head). Minting from the snapshot the screen read, not the newest one, keeps a
    /// teammate's change that arrived meanwhile out of the token, so the save makes a
    /// conflict with it instead of replacing it unseen (I4).
    pub fn mint(
        &self,
        key: &RecordKey,
        displayed: Option<ChangeId>,
    ) -> Result<EditToken, TokenError> {
        EditToken::from_journal(self.inner.token(key, displayed)?)
    }

    /// A review token for `key`: every head, as a resolution screen shows them.
    pub fn mint_review(&self, key: &RecordKey) -> Result<ReviewToken, TokenError> {
        ReviewToken::from_journal(self.inner.review_token(key)?)
    }
}

fn kind_name(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Edit => "an edit",
        TokenKind::Review => "a review",
    }
}

/// `token` if its kind, which its MAC covers (R19), is `kind`.
fn of_kind(token: ShownToken, kind: TokenKind) -> Result<ShownToken, TokenError> {
    if token.kind() == kind {
        Ok(token)
    } else {
        Err(TokenError::Malformed(format!(
            "{} token where {} token was expected",
            kind_name(token.kind()),
            kind_name(kind)
        )))
    }
}

/// The version of one record an editor displayed: tc-journal's token of the edit kind. Only
/// the backend makes one, so a review token cannot stand in for it by a conversion slip.
#[derive(Clone, PartialEq, Eq)]
pub struct EditToken(ShownToken);

/// Every head of a record, as a resolution screen displays them: tc-journal's token of the
/// review kind. Only a resolution takes it.
#[derive(Clone, PartialEq, Eq)]
pub struct ReviewToken(ShownToken);

impl fmt::Debug for EditToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EditToken(..)")
    }
}

impl fmt::Debug for ReviewToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ReviewToken(..)")
    }
}

impl EditToken {
    pub(in crate::backend) fn from_journal(token: ShownToken) -> Result<Self, TokenError> {
        of_kind(token, TokenKind::Edit).map(Self)
    }

    /// Reads the wire form of an edit token: the MAC first, then the kind, so a review token
    /// relabelled `st1.` through JSON is refused here.
    #[cfg(feature = "fixture-backend")]
    pub(in crate::backend) fn parse(
        signing: &tc_journal::TokenKey,
        wire: &tc_api::ShownToken,
    ) -> Result<Self, TokenError> {
        Self::from_journal(ShownToken::parse(signing, wire.body())?)
    }

    pub fn key(&self) -> &RecordKey {
        self.0.key()
    }

    /// Sorted, no repeats; empty for a record with no version yet.
    pub fn heads(&self) -> &[ChangeId] {
        self.0.heads()
    }

    /// The wire form `st1.<body>`: the only way an edit token's text leaves the backend. A
    /// body tc-api refuses (one over its size limit) is an error, never a panic.
    pub fn to_wire(&self) -> Result<tc_api::ShownToken, TokenTextError> {
        tc_api::ShownToken::from_body(&self.0.to_text())
    }
}

impl ReviewToken {
    pub(in crate::backend) fn from_journal(token: ShownToken) -> Result<Self, TokenError> {
        of_kind(token, TokenKind::Review).map(Self)
    }

    /// Reads the wire form of a review token, the MAC first, then the kind.
    #[cfg(feature = "fixture-backend")]
    pub(in crate::backend) fn parse(
        signing: &tc_journal::TokenKey,
        wire: &tc_api::ReviewToken,
    ) -> Result<Self, TokenError> {
        Self::from_journal(ShownToken::parse(signing, wire.body())?)
    }

    pub fn key(&self) -> &RecordKey {
        self.0.key()
    }

    pub fn heads(&self) -> &[ChangeId] {
        self.0.heads()
    }

    /// The wire form `rt1.<body>`.
    pub fn to_wire(&self) -> Result<tc_api::ReviewToken, TokenTextError> {
        tc_api::ReviewToken::from_body(&self.0.to_text())
    }
}

// ---------------------------------------------------------------------------------------
// Commands and replies
// ---------------------------------------------------------------------------------------

/// One command for the project (STORAGE §2.7), built only from the token kind it needs:
/// an edit from an [`EditToken`], a resolution from [`ReviewToken`]s. Each unit that sends a
/// command adds its constructor here.
///
/// A review token cannot make an edit:
///
/// ```compile_fail,E0308
/// use tc_app::backend::{ReviewToken, StoreCommand};
/// fn save(review: ReviewToken) -> StoreCommand {
///     StoreCommand::save_verse(review, "In the beginning".to_owned())
/// }
/// ```
///
/// and edit tokens cannot make a resolution:
///
/// ```compile_fail,E0308
/// use tc_app::backend::{EditToken, StoreCommand};
/// use tc_journal::command::Choice;
/// use tc_journal::fold::ConflictId;
/// fn resolve(conflict: ConflictId, edits: Vec<EditToken>) -> StoreCommand {
///     StoreCommand::resolve(conflict, edits, Choice::ApplyToMovedCheck)
/// }
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(
    not(feature = "fixture-backend"),
    expect(
        dead_code,
        reason = "no backend of a build without the fixture backend runs a command until C0's"
    )
)]
pub struct StoreCommand {
    command: Command,
    /// Each record the command writes, with the heads its token named.
    shown: Vec<(RecordKey, Vec<ChangeId>)>,
}

impl StoreCommand {
    /// `SaveVerse` (STORAGE §5.1): the words over the verse the token shows.
    pub fn save_verse(shown: EditToken, words: String) -> Self {
        let listed = vec![(shown.key().clone(), shown.heads().to_vec())];
        Self {
            command: Command::SaveVerse {
                shown: shown.0,
                words,
            },
            shown: listed,
        }
    }

    /// `Resolve` (STORAGE §5.9): one review token per record of the conflict.
    pub fn resolve(conflict: ConflictId, shown: Vec<ReviewToken>, choice: Choice) -> Self {
        let listed = shown
            .iter()
            .map(|t| (t.key().clone(), t.heads().to_vec()))
            .collect();
        Self {
            command: Command::Resolve {
                conflict,
                shown: shown.into_iter().map(|t| t.0).collect(),
                choice,
            },
            shown: listed,
        }
    }

    /// The records the command writes, each with the heads its token named.
    #[cfg(feature = "fixture-backend")]
    pub(in crate::backend) fn shown(&self) -> &[(RecordKey, Vec<ChangeId>)] {
        &self.shown
    }

    /// The journal's command, for the backend that runs it.
    #[cfg(feature = "fixture-backend")]
    pub(in crate::backend) fn journal(&self) -> &Command {
        &self.command
    }
}

/// How far a write got (tc-store `Stored`, STORAGE §5.0 B3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stored {
    /// "Saved": durable.
    Durable,
    /// In place, but the disk reported an error after it: saved, with a warning.
    WrittenNotDurable(String),
    /// Nothing to write: the value equals the displayed one.
    Unchanged,
}

/// The reply to a command that was not refused (B6).
#[derive(Debug, Clone)]
pub struct StoreReply {
    /// A new edit token for each record the batch wrote: what the screen shows now.
    pub tokens: Vec<(RecordKey, EditToken)>,
    /// Conflicts that did not exist before the batch.
    pub new_conflicts: Vec<ConflictId>,
    pub stored: Stored,
    /// The first published snapshot that holds the batch (R16): read-your-writes.
    pub seq: u64,
    /// The changes the batch wrote.
    pub changes: Vec<ChangeId>,
}

/// Why a command wrote nothing. C0 and C2 add the refusals tc-store makes (`Refused`,
/// `LookAgain`, `PlanStale`); the memory backend produces none of them.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    /// The disk refused it; a retry uses the same token.
    #[error("not written: {0}")]
    NotWritten(String),
    #[error("the project is open read-only")]
    ReadOnly(ReadOnlyReason),
    #[error("the token was not made here for this project: {0}")]
    BadToken(TokenError),
    /// The project closed under the command.
    #[error("the project is closed")]
    Closed,
    /// The backend runs no command (AD6: the memory backend without a scripted outcome).
    #[error("this backend runs no command")]
    Unavailable,
}

/// Why the fold changed without a command of this app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreCause {
    /// Teammates' changes received by a sync or from a file.
    Intake { devices: Vec<DeviceId> },
    /// A change the store made on its own.
    System,
}

/// What happened in an open project.
#[derive(Debug, Clone)]
pub enum StoreEvent {
    /// A batch was folded; `seq` is the snapshot that holds it.
    Committed {
        seq: u64,
        delta: Delta,
        cause: StoreCause,
    },
    /// The project became writable or read-only.
    Mode(SessionMode),
}

/// What a handle says of its project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFacts {
    pub id: tc_journal::ProjectId,
    pub kind: ProjectKind,
    /// This computer's device in the project: its person is "mine".
    pub device: DeviceId,
    pub mode: SessionMode,
}

/// A project the store holds, as a list names it: no lock, no fold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectEntry {
    pub folder: ProjectRef,
    pub id: tc_journal::ProjectId,
    pub kind: ProjectKind,
}

/// Why a project did not open (STORAGE §6.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenFailure {
    NotAProject,
    Unreadable {
        reason: String,
    },
    /// The project's lock is held, by the host and process named when known (O1). Inside one
    /// `tc4` the session registry is the only opener, so it never meets its own lock.
    LockHeld {
        holder: Option<(String, u32)>,
    },
    /// The project needs a newer app to open it.
    NewerFormat {
        major: u16,
    },
    /// This build has no backend that opens projects (before C0).
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("{0}")]
    Other(String),
}

/// Why a checkpoint runs (tc-store `CheckpointReason`; D1 adds the others).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckpointReason {
    /// tC4's "leaving the project" checkpoint, when the person goes Home.
    LeavingProject,
    Other(String),
}

/// The projects of this computer.
pub trait ProjectStore: Send + Sync + 'static {
    /// The projects this store holds. No lock, no fold.
    fn list(&self) -> Result<Vec<ProjectEntry>, StoreError>;

    /// Opens `folder`. Blocking: called on the blocking pool. A second open of a folder whose
    /// handle is live is `LockHeld` (STORAGE §6.1 O1), which the session registry exists to
    /// never meet. `progress` takes the store's own reports (AD8).
    fn open(
        &self,
        folder: &ProjectRef,
        progress: ProgressSink,
    ) -> Result<Arc<dyn ProjectHandle>, OpenFailure>;

    /// `test/reset`: every project back to its seed. The registry closes every session first.
    #[cfg(feature = "test-hooks")]
    fn reset(&self) -> Result<(), StoreError>;
}

/// One open project.
pub trait ProjectHandle: Send + Sync {
    fn facts(&self) -> ProjectFacts;

    /// The newest published snapshot, an `Arc` clone. Reads never wait for a command (R5).
    fn snapshot(&self) -> Snapshot;

    /// Reads an edit token the UI sent back: refuses one this project's key did not sign,
    /// then one of the review kind.
    fn parse_edit(&self, token: &tc_api::ShownToken) -> Result<EditToken, TokenError>;

    /// Reads a review token the UI sent back, the MAC first, then the kind.
    fn parse_review(&self, token: &tc_api::ReviewToken) -> Result<ReviewToken, TokenError>;

    /// Hands a command over; the reply comes once it is durable and folded. Never blocks the
    /// caller.
    fn submit(&self, command: StoreCommand) -> BoxFuture<Result<StoreReply, CommandError>>;

    /// The project's events from now on. The channel ends when the handle closes.
    fn subscribe(&self) -> tokio::sync::mpsc::UnboundedReceiver<StoreEvent>;

    fn checkpoint(&self, reason: CheckpointReason) -> BoxFuture<CheckpointOutcome>;

    /// Closes the project and lets go of its lock.
    fn close(self: Arc<Self>) -> BoxFuture<Result<(), StoreError>>;
}

/// The store of a build with no backend (release builds before C0): it lists nothing, and
/// opening anything is `Unavailable`.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoProjects;

impl ProjectStore for NoProjects {
    fn list(&self) -> Result<Vec<ProjectEntry>, StoreError> {
        Ok(Vec::new())
    }

    fn open(
        &self,
        _folder: &ProjectRef,
        _progress: ProgressSink,
    ) -> Result<Arc<dyn ProjectHandle>, OpenFailure> {
        Err(OpenFailure::Unavailable)
    }

    #[cfg(feature = "test-hooks")]
    fn reset(&self) -> Result<(), StoreError> {
        Ok(())
    }
}
