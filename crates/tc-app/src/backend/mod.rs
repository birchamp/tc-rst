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
//! Backends: [`store::StoreBackend`], tc-store over the projects folder (unit C0), in every
//! build; with the feature `fixture-backend`, [`memory::MemoryBackend`] over STORAGE §13.2's
//! state seed ([`seed`]) of the parity scenarios ([`scenario`]), never in a release build
//! (AD35).

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tc_api::event::{CheckpointOutcome, SessionMode};
use tc_api::ids::ProjectRef;
use tc_api::token::TokenTextError;
use tc_api::write::ReadOnlyReason;
use tc_journal::command::{Choice, Command, Draft, DraftField, SectionValue};
use tc_journal::fold::{ConflictId, Delta, FoldState, ProjectKind};
use tc_journal::token::{TokenError, TokenKind};
use tc_journal::{BookId, ChangeId, DeviceId, RecordKey, ShownToken};

use crate::jobs::ProgressSink;

pub use tc_store::{Refusal as RuleRefusal, StoreCounters, StoreNotice};

#[cfg(feature = "fixture-backend")]
pub mod memory;
#[cfg(feature = "fixture-backend")]
pub mod scenario;
#[cfg(feature = "fixture-backend")]
pub mod scenario_store;
#[cfg(feature = "fixture-backend")]
pub mod seed;
pub mod store;

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

/// The token of an offered draft's record, read back from the UI (C1b): an edit token this
/// project's key signed, or one it can no longer check (a new installation secret, a folder
/// copied from another computer), which still names its record and heads. Only *Keep* takes
/// it, and the store treats an unverified one as no base (R20), so it never stands in for an
/// edit token.
#[derive(Clone, PartialEq, Eq)]
pub struct DraftToken(ShownToken);

impl fmt::Debug for DraftToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DraftToken(..)")
    }
}

impl DraftToken {
    /// Reads a draft's token: the MAC first, as an edit token; a token whose MAC fails is read
    /// unverified. Any other fault (not a token, a review token) is refused.
    pub fn parse(
        handle: &dyn ProjectHandle,
        wire: &tc_api::ShownToken,
    ) -> Result<(Self, bool), TokenError> {
        match handle.parse_edit(wire) {
            Ok(token) => Ok((Self(token.0), true)),
            Err(TokenError::BadMac) => {
                let token = of_kind(ShownToken::parse_unverified(wire.body())?, TokenKind::Edit)?;
                Ok((Self(token), false))
            }
            Err(error) => Err(error),
        }
    }

    pub fn key(&self) -> &RecordKey {
        self.0.key()
    }

    pub fn heads(&self) -> &[ChangeId] {
        self.0.heads()
    }
}

impl From<EditToken> for DraftToken {
    fn from(token: EditToken) -> Self {
        Self(token.0)
    }
}

// ---------------------------------------------------------------------------------------
// Commands and replies
// ---------------------------------------------------------------------------------------

/// Who asked for a command (AD37). The `Committed` event of the batch it stores names this,
/// so the `Changed` frame of a person's own save names their request and their field shows
/// no "changed while you were typing".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// The UI request this command answers.
    Request(tc_api::ids::RequestId),
    /// The server's own work (a scripted step, the alignment save timer).
    Server,
}

/// One command for the project (STORAGE §2.7), built only from the token kind it needs:
/// an edit from an [`EditToken`], a resolution from [`ReviewToken`]s. Each unit that sends a
/// command adds its constructor here. A command is the server's own ([`Origin::Server`])
/// until [`StoreCommand::requested_by`] names the request it answers.
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
pub struct StoreCommand {
    command: Command,
    /// Each record the command writes, with the heads its token named.
    #[cfg_attr(
        not(feature = "fixture-backend"),
        expect(
            dead_code,
            reason = "only the memory backend answers from the displayed heads; the store reads its own"
        )
    )]
    shown: Vec<(RecordKey, Vec<ChangeId>)>,
    origin: Origin,
}

impl StoreCommand {
    fn new(command: Command, shown: Vec<(RecordKey, Vec<ChangeId>)>) -> Self {
        Self {
            command,
            shown,
            origin: Origin::Server,
        }
    }

    /// `SaveVerse` (STORAGE §5.1): the words over the verse the token shows.
    pub fn save_verse(shown: EditToken, words: String) -> Self {
        let listed = vec![(shown.key().clone(), shown.heads().to_vec())];
        Self::new(
            Command::SaveVerse {
                shown: shown.0,
                words,
            },
            listed,
        )
    }

    /// `SaveSection` (STORAGE §5.2): one edit token per verse and chapter front the section
    /// editor displayed, with its new value, or `None` for one displayed and left as it was;
    /// and the book's layout token when the book has one. The store checks only what was
    /// displayed, so a change made meanwhile elsewhere becomes a conflict, never a refusal.
    pub fn save_section(
        book: BookId,
        shown: Vec<(EditToken, Option<SectionValue>)>,
        layout: Option<EditToken>,
    ) -> Self {
        let listed = shown
            .iter()
            .map(|(t, _)| t)
            .chain(layout.as_ref())
            .map(|t| (t.key().clone(), t.heads().to_vec()))
            .collect();
        Self::new(
            Command::SaveSection {
                book,
                shown: shown.into_iter().map(|(t, value)| (t.0, value)).collect(),
                layout: layout.map(|t| t.0),
            },
            listed,
        )
    }

    /// `Draft` (STORAGE §5.14): unsaved editor text with the tokens of what the editor
    /// displayed. The store keeps it beside the actor (R22), so it writes no change and never
    /// waits behind a command.
    pub fn draft(field: DraftField, shown: Vec<EditToken>, text: String) -> Self {
        let listed = shown
            .iter()
            .map(|t| (t.key().clone(), t.heads().to_vec()))
            .collect();
        Self::new(
            Command::Draft(Draft {
                field,
                shown: shown.into_iter().map(|t| t.0).collect(),
                text,
            }),
            listed,
        )
    }

    /// `KeepDraft` (STORAGE §5.14, C1b): a draft offered at open, saved as its field's save
    /// with its tokens, so a record changed since becomes a conflict. A token that no longer
    /// verifies is no base: the store writes the text with no parents (R20).
    pub fn keep_draft(field: DraftField, shown: Vec<DraftToken>, text: String) -> Self {
        let listed = shown
            .iter()
            .map(|t| (t.0.key().clone(), t.0.heads().to_vec()))
            .collect();
        Self::new(
            Command::KeepDraft {
                draft: Draft {
                    field,
                    shown: shown.into_iter().map(|t| t.0).collect(),
                    text,
                },
                section: Vec::new(),
            },
            listed,
        )
    }

    /// The UI request this command answers: its batch's event is announced as that
    /// request's own save (AD37).
    pub fn requested_by(mut self, request: tc_api::ids::RequestId) -> Self {
        self.origin = Origin::Request(request);
        self
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// `Resolve` (STORAGE §5.9): one review token per record of the conflict.
    pub fn resolve(conflict: ConflictId, shown: Vec<ReviewToken>, choice: Choice) -> Self {
        let listed = shown
            .iter()
            .map(|t| (t.key().clone(), t.heads().to_vec()))
            .collect();
        Self::new(
            Command::Resolve {
                conflict,
                shown: shown.into_iter().map(|t| t.0).collect(),
                choice,
            },
            listed,
        )
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

    /// The journal's command and who asked for it, for the store, which runs the one and
    /// announces the other.
    pub(in crate::backend) fn into_parts(self) -> (Command, Origin) {
        (self.command, self.origin)
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
    /// Conflicts a bulk choice left for a choice of their own (tc-store).
    pub skipped: Vec<ConflictId>,
}

/// Why a command wrote nothing (tc-store `CommandError`). The memory backend makes none of
/// `Refused`, `LookAgain` and `PlanStale`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    /// A rule of the command refused it; nothing was written (STORAGE §5.0 B1).
    #[error("{0}")]
    Refused(RuleRefusal),
    /// The disk refused it; a retry uses the same token.
    #[error("not written: {0}")]
    NotWritten(String),
    /// A resolution over heads that moved since the review was shown (STORAGE §5.9).
    #[error("a newer change arrived; look again")]
    LookAgain,
    /// An upgrade plan made over heads that moved (STORAGE §5.8).
    #[error("the project changed while the plan was made")]
    PlanStale,
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

/// Why the fold changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreCause {
    /// Teammates' changes received by a sync or from a file.
    Intake { devices: Vec<DeviceId> },
    /// A command of this app, with who asked for it as `submit` was given it.
    Command { origin: Origin },
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
    /// A notice for the person (STORAGE §2.6): a second writer, a save the disk only half
    /// confirmed, a file that could not be read. tc-store keeps the list too (C1b shows it).
    Notice(StoreNotice),
    /// Derived files were written (STORAGE §5.10); D1 shows what could not be.
    Materialized { paths: Vec<String> },
}

/// A draft from an earlier session whose text differs from what its records hold (STORAGE
/// §5.14, O12). tc-journal's tokens stay inside, as everywhere in the seam (AD3).
#[derive(Debug, Clone)]
pub struct OfferedDraft {
    /// The store's number for it, for C1b's keep and discard.
    pub id: u64,
    pub field: DraftField,
    pub text: String,
    /// Every token verifies under this installation's key. Otherwise the offer shows the
    /// text alone, and Keep writes it with no parents, as a conflict (R20, §6.6).
    pub verified: bool,
    pub at: tc_journal::WallTime,
    draft: Draft,
}

impl OfferedDraft {
    /// The records the draft's editor displayed, the first one being the field's own.
    pub fn keys(&self) -> impl Iterator<Item = &RecordKey> {
        self.draft.shown.iter().map(ShownToken::key)
    }

    /// The draft's tokens as the UI holds tokens, verified or not: what *Keep* sends back
    /// (C1b). A token tc-api cannot carry is an error, never a panic.
    pub fn wire_tokens(&self) -> Result<Vec<tc_api::ShownToken>, TokenTextError> {
        self.draft
            .shown
            .iter()
            .map(|t| tc_api::ShownToken::from_body(&t.to_text()))
            .collect()
    }

    pub(in crate::backend) fn new(
        id: u64,
        verified: bool,
        at: tc_journal::WallTime,
        draft: Draft,
    ) -> Self {
        Self {
            id,
            field: draft.field.clone(),
            text: draft.text.clone(),
            verified,
            at,
            draft,
        }
    }
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

/// What a Home card's summary was computed from (APP-DESIGN §4.5, C1a D8): the names of the
/// saves not sealed yet (`.tc/pending/`, one flat folder) and the size and modification time of
/// `.tc/materialized.json`, which a seal, a consolidation or an intake rewrites (it records the
/// md5 of every pack it lists). One listing and one stat at any history: the packs are not
/// listed, since they are nested by sealer and month and their number grows with the history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SummaryStamp([u8; 32]);

impl SummaryStamp {
    /// SHA-256 over a domain tag, each name followed by a zero byte, then `1` with the manifest's
    /// size and mtime (little-endian), or `0` when there is no manifest.
    pub fn of(pending: &[String], manifest: Option<(u64, i64)>) -> Self {
        let mut names: Vec<&String> = pending.iter().collect();
        names.sort();
        let mut context = ring::digest::Context::new(&ring::digest::SHA256);
        context.update(b"tc4-summary-stamp\0");
        for name in names {
            context.update(name.as_bytes());
            context.update(b"\0");
        }
        match manifest {
            Some((len, mtime)) => {
                context.update(&[1]);
                context.update(&len.to_le_bytes());
                context.update(&mtime.to_le_bytes());
            }
            None => context.update(&[0]),
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(context.finish().as_ref());
        Self(out)
    }

    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn parse_hex(text: &str) -> Option<Self> {
        if text.len() != 64 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).ok()?;
        }
        Some(Self(out))
    }
}

/// What the projects folder holds: the projects, and every folder that holds a project's
/// files but cannot be opened as one. Home shows the second kind with its reason, so no folder
/// that holds someone's work is left off it (AD12, B2): a declared difference from tC4, whose
/// listing hides such a folder (APP-DESIGN §4.5).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    /// By folder name.
    pub projects: Vec<ProjectEntry>,
    /// By folder name.
    pub unusable: Vec<UnusableFolder>,
}

/// A folder that holds a project's files (`ingredients/tc/project.json`, `metadata.json`,
/// `ingredients/` or `.tc/`) but cannot be opened as a project: a `project.json` the store's
/// open would refuse or cannot read, one that is missing beside the project's other files, a
/// name tc-api cannot carry, or a link to such a folder elsewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnusableFolder {
    /// The folder's name, with any byte that is not UTF-8 replaced.
    pub name: String,
    pub reason: String,
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
    /// The open ended with no outcome (the session registry's own arm, for an open whose
    /// result never came). No backend answers it since C0.
    Unavailable,
}

/// Why a copy was not made (tc-store `CreateError`). Nothing of it is left.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CopyFailure {
    #[error("a folder with that name exists")]
    NameExists,
    #[error("{0}")]
    Failed(String),
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
    /// The projects this store holds, and the folders that hold a project's files but cannot
    /// be opened as one. No lock, no fold.
    fn list(&self) -> Result<Listing, StoreError>;

    /// Opens `folder`. Blocking: called on the blocking pool. A second open of a folder whose
    /// handle is live is `LockHeld` (STORAGE §6.1 O1), which the session registry exists to
    /// never meet. `progress` takes the store's own reports (AD8).
    fn open(
        &self,
        folder: &ProjectRef,
        progress: ProgressSink,
    ) -> Result<Arc<dyn ProjectHandle>, OpenFailure>;

    /// Copies `folder`, every save and its `drafts.json` included, to a new folder `name` in
    /// the projects folder: the copy offer of a read-only project (STORAGE §5.0, R7). Blocking.
    fn copy(&self, folder: &ProjectRef, name: &ProjectRef) -> Result<(), CopyFailure>;

    /// The person said that the computer `folder`'s lock names is closed (STORAGE §5.0): the
    /// next open of `folder` takes the lock and opens writable, where it would otherwise open
    /// read-only with `OpenOnAnotherComputer`.
    fn trust_lock(&self, folder: &ProjectRef);

    /// `test/reset`: every project back to its seed. The registry closes every session first.
    #[cfg(feature = "test-hooks")]
    fn reset(&self) -> Result<(), StoreError>;

    /// tc-store's hold points, which `test/hold` arms beside the app's own (C1a); `None` for a
    /// store that has none.
    #[cfg(feature = "test-hooks")]
    fn store_hooks(&self) -> Option<Arc<tc_store::Hooks>> {
        None
    }
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

    /// The store's operation counters (R14): files opened and created, flushes, renames,
    /// commands and their queue waits. What a count test reads.
    fn counters(&self) -> StoreCounters;

    /// Drafts from an earlier session that differ from what their records hold (O12).
    fn offered_drafts(&self) -> Vec<OfferedDraft>;

    /// *Discard* of an offered draft: its entry leaves `drafts.json` (STORAGE §5.14).
    fn discard_draft(&self, id: u64);

    /// Blocks until the store's queue is empty and its background work is done: a test
    /// calls it before it counts.
    #[cfg(feature = "test-hooks")]
    fn wait_idle(&self);

    /// Closes the project and lets go of its lock.
    fn close(self: Arc<Self>) -> BoxFuture<Result<(), StoreError>>;
}
