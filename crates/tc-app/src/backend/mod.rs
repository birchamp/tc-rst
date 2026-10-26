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
use tc_journal::command::{
    Choice, Command, DecisionEdit, Draft, DraftField, SectionValue, UpgradePlan,
};
use tc_journal::fold::{ConflictId, Delta, FoldState, ProjectKind};
use tc_journal::token::{TokenError, TokenKind};
use tc_journal::{BookId, ChangeId, DeviceId, GitOid, RecordKey, ShownToken};

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

    /// *Restore this version* (#78, owner ruling H5): the value of the version `from` of the
    /// record `shown` names, kept over every head the token shows.
    pub fn restore(shown: EditToken, from: ChangeId) -> Self {
        let listed = vec![(shown.key().clone(), shown.heads().to_vec())];
        Self::new(
            Command::Restore {
                shown: shown.0,
                from,
            },
            listed,
        )
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

    /// `SaveLayout` (STORAGE §5.1): only the displayed verse's layout changes.
    pub fn save_layout(shown: EditToken, layout: String) -> Self {
        let listed = vec![(shown.key().clone(), shown.heads().to_vec())];
        Self::new(
            Command::SaveLayout {
                shown: shown.0,
                layout,
            },
            listed,
        )
    }

    /// A comment box blur adds once; later blurs continue the displayed comment.
    pub fn add_comment(target: tc_journal::record::CommentTarget, text: String) -> Self {
        Self::new(Command::AddComment { target, text }, Vec::new())
    }

    pub fn revise_comment(shown: EditToken, text: String) -> Self {
        let listed = vec![(shown.key().clone(), shown.heads().to_vec())];
        Self::new(
            Command::ReviseComment {
                shown: shown.0,
                text,
            },
            listed,
        )
    }

    /// OBS frames and their closing reference share the store's validated command.
    pub fn save_frame(shown: EditToken, text: String) -> Self {
        let listed = vec![(shown.key().clone(), shown.heads().to_vec())];
        Self::new(
            Command::SaveFrame {
                shown: shown.0,
                text,
            },
            listed,
        )
    }

    /// `AddBook` (STORAGE §5.7): a blank book seeded from the chapters and verses of
    /// `source`. It writes no record that existed, so it carries no token.
    pub fn add_book(book: BookId, source: String) -> Self {
        Self::new(
            Command::AddBook {
                book,
                scope: tc_journal::record::Scope::Whole,
                seed: tc_journal::command::SeedSource { usfm: source },
            },
            Vec::new(),
        )
    }

    /// `AddBookText` (STORAGE §5.7, tC4 `addUsfmBooks`): a book with its own text, in one
    /// batch whose cause is an import of `file_name`.
    pub fn add_book_text(book: BookId, usfm: String, file_name: String) -> Self {
        Self::new(
            Command::AddBookText {
                book,
                scope: tc_journal::record::Scope::Whole,
                usfm,
                file_name,
            },
            Vec::new(),
        )
    }

    /// `SetSetting` (STORAGE §5.4): one top-level key of `settings.json`, over the value the
    /// token shows.
    pub fn set_setting(shown: EditToken, value: Option<tc_journal::Json>) -> Self {
        let listed = vec![(shown.key().clone(), shown.heads().to_vec())];
        Self::new(
            Command::SetSetting {
                shown: shown.0,
                value,
            },
            listed,
        )
    }

    /// `SetMeta` (STORAGE §3.4): one project detail over the token that showed it.
    pub fn set_meta(shown: EditToken, value: Option<tc_journal::Json>) -> Self {
        let listed = vec![(shown.key().clone(), shown.heads().to_vec())];
        Self::new(
            Command::SetMeta {
                shown: shown.0,
                value,
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

    /// `SaveAlignment` (STORAGE §5.3): the aligner's links of a verse, over the alignment the
    /// first token shows, made against the verse the second shows. The store stamps it from
    /// that verse, so a verse that changed meanwhile leaves the alignment stale at once.
    pub fn save_alignment(
        shown: EditToken,
        verse_shown: EditToken,
        value: tc_journal::command::AlignmentEdit,
    ) -> Self {
        let listed = vec![
            (shown.key().clone(), shown.heads().to_vec()),
            (verse_shown.key().clone(), verse_shown.heads().to_vec()),
        ];
        Self::new(
            Command::SaveAlignment {
                shown: shown.0,
                verse_shown: verse_shown.0,
                value,
            },
            listed,
        )
    }

    /// `SaveDecision` (STORAGE §5.4): one act of the checker over the decision the detail showed,
    /// and the verse whose text it was judged against (the same token again when the check
    /// names no verse text). The store checks only what was displayed, so a change made
    /// meanwhile elsewhere becomes a conflict, never a refusal.
    pub fn save_decision(shown: EditToken, verse_shown: EditToken, value: DecisionEdit) -> Self {
        let listed = [&shown, &verse_shown]
            .into_iter()
            .map(|t| (t.key().clone(), t.heads().to_vec()))
            .collect();
        Self::new(
            Command::SaveDecision {
                shown: shown.0,
                verse_shown: verse_shown.0,
                value,
            },
            listed,
        )
    }

    /// `Draft` (STORAGE §5.14): unsaved editor text with the tokens of what the editor
    /// displayed, and `at`, the page's clock at the text's last keystroke (its buffer entry's
    /// `at`, APP-DESIGN §6.6). The store keeps it beside the actor (R22), so it writes no
    /// change and never waits behind a command.
    pub fn draft(
        field: DraftField,
        shown: Vec<EditToken>,
        text: String,
        at: tc_journal::WallTime,
    ) -> Self {
        let listed = shown
            .iter()
            .map(|t| (t.key().clone(), t.heads().to_vec()))
            .collect();
        Self::new(
            Command::Draft {
                draft: Draft {
                    field,
                    shown: shown.into_iter().map(|t| t.0).collect(),
                    text,
                },
                at,
            },
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

    /// `ApplyUpgrade` (STORAGE §5.8, unit C8): a plan made off the actor from one snapshot, each
    /// change naming the heads its record had there. The store writes the whole plan as one
    /// batch, or refuses with `PlanStale` when some record's heads moved.
    pub fn apply_upgrade(plan: UpgradePlan) -> Self {
        let listed = plan
            .changes
            .iter()
            .map(|change| (change.record.key(), change.parents.clone()))
            .collect();
        Self::new(Command::ApplyUpgrade { plan }, listed)
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

    /// `ResolveBulk` (STORAGE §5.9 step 3): one batch over every listed conflict, each with
    /// one review token per record; the store leaves out the conflicts the devices cannot
    /// decide alone.
    pub fn resolve_bulk(
        devices: std::collections::BTreeSet<DeviceId>,
        conflicts: Vec<(ConflictId, Vec<ReviewToken>)>,
    ) -> Self {
        let listed = conflicts
            .iter()
            .flat_map(|(_, tokens)| tokens.iter())
            .map(|t| (t.key().clone(), t.heads().to_vec()))
            .collect();
        Self::new(
            Command::ResolveBulk {
                devices,
                conflicts: conflicts
                    .into_iter()
                    .map(|(id, tokens)| (id, tokens.into_iter().map(|t| t.0).collect()))
                    .collect(),
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
    /// The server run whose page typed it, when the store recorded one.
    pub run: Option<tc_api::ids::ServerInstance>,
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
        run: Option<tc_api::ids::ServerInstance>,
        verified: bool,
        at: tc_journal::WallTime,
        draft: Draft,
    ) -> Self {
        Self {
            id,
            run,
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
/// A folder with no manifest (copied or received without `.tc/`, which Door43 and Git leave
/// out) is stamped by the files its summary reads as well, or every such copy would share one
/// stamp, whatever it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SummaryStamp([u8; 32]);

impl SummaryStamp {
    /// SHA-256 over a domain tag, the number of names, each name followed by a zero byte, then
    /// each file's path followed by a zero byte and `1` with its size and mtime (little-endian),
    /// or `0` when it is missing.
    pub fn of(pending: &[String], files: &[(String, Option<(u64, i64)>)]) -> Self {
        let mut names: Vec<&String> = pending.iter().collect();
        names.sort();
        let mut files: Vec<_> = files.iter().collect();
        files.sort();
        let mut context = ring::digest::Context::new(&ring::digest::SHA256);
        context.update(b"tc4-summary-stamp\0");
        context.update(&(names.len() as u64).to_le_bytes());
        for name in names {
            context.update(name.as_bytes());
            context.update(b"\0");
        }
        for (path, file) in files {
            context.update(path.as_bytes());
            context.update(b"\0");
            match file {
                Some((len, mtime)) => {
                    context.update(&[1]);
                    context.update(&len.to_le_bytes());
                    context.update(&mtime.to_le_bytes());
                }
                None => context.update(&[0]),
            }
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
    /// The other entries, by name: files, folders with none of a project's files, staging
    /// folders. Home lists none of them, but a project added under one of their names would
    /// land on it (B9b D5).
    pub others: Vec<String>,
}

/// A folder that holds a project's files (`ingredients/tc/project.json`, `metadata.json`,
/// `ingredients/` or `.tc/`) but cannot be opened as a project: a `project.json` the store's
/// open would refuse or cannot read, one that is missing beside the project's other files, a
/// name tc-api cannot carry, or a link to such a folder elsewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnusableFolder {
    /// The folder's name, with any byte that is not UTF-8 replaced.
    pub name: String,
    pub reason: UnusableReason,
}

/// Why a folder that holds a project's files cannot be opened. Home says it in words
/// (`home.unusable.*`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnusableReason {
    /// `ingredients/tc/project.json` is missing: this may be a translationCore 4.0 project to
    /// import, or a project whose `project.json` was deleted.
    NoProjectFile,
    /// `project.json` is there but could not be read.
    ProjectFileUnreadable(String),
    /// `project.json` is one the store's open refuses.
    ProjectFileDamaged(String),
    /// A link to a folder elsewhere: tc-store reads no link as project data, and its checks of
    /// a folder (the lock, the drive, the cloud client) would be about the link, not the
    /// folder the work is in.
    Link,
    /// A name tc-api cannot carry.
    NameNotAllowed,
    /// A name that is not valid Unicode.
    NameNotText,
}

impl UnusableReason {
    pub fn to_api(&self) -> tc_api::home::UnusableReason {
        use tc_api::home::UnusableReason as Api;
        match self {
            Self::NoProjectFile => Api::NoProjectFile,
            Self::ProjectFileUnreadable(reason) => Api::ProjectFileUnreadable {
                reason: reason.clone(),
            },
            Self::ProjectFileDamaged(reason) => Api::ProjectFileDamaged {
                reason: reason.clone(),
            },
            Self::Link => Api::Link,
            Self::NameNotAllowed => Api::NameNotAllowed,
            Self::NameNotText => Api::NameNotText,
        }
    }
}

// ---------------------------------------------------------------------------------------
// What Home reads of a project (B2)
// ---------------------------------------------------------------------------------------

/// Where a shared project lives on Door43: the `origin` of its remote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Door43Remote {
    pub host: String,
    pub owner: String,
    pub repo: String,
}

impl Door43Remote {
    /// `https://<host>/<owner>/<repo>`, with or without `.git`.
    pub fn parse_url(url: &str) -> Option<Self> {
        let rest = url.strip_prefix("https://")?;
        let rest = rest.strip_suffix(".git").unwrap_or(rest);
        match rest.split('/').collect::<Vec<_>>().as_slice() {
            [host, owner, repo] if !host.is_empty() && !owner.is_empty() && !repo.is_empty() => {
                Some(Self {
                    host: (*host).to_owned(),
                    owner: (*owner).to_owned(),
                    repo: (*repo).to_owned(),
                })
            }
            _ => None,
        }
    }
}

/// What a card's frame needs of a project, read without a lock and without a fold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPeek {
    /// `metadata.json` as a document (the store's file, or the memory backend's fields in its
    /// shape); `Err` says why it could not be read. The card reads its facts from it
    /// (`crate::home::card_facts`).
    pub metadata: Result<serde_json::Value, String>,
    /// The settings file's `textDirection`, the second source of a project's direction
    /// (`crate::direction`).
    pub text_direction: Option<String>,
    pub remote: Option<Door43Remote>,
    /// `None` when the saves could not be looked at (another program holds `.tc/pending` or
    /// `.tc/materialized.json`): no summary is then the card's, and its counts show "—".
    pub stamp: Option<SummaryStamp>,
}

/// The derived files a summary is made of, read without a lock and without a fold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryFiles {
    /// Each asked book's USFM text, or why it could not be read.
    pub books: Vec<(BookId, Result<String, String>)>,
    /// A story project's stories: the listing, then each story's text.
    pub stories: Option<Result<Vec<StoryText>, String>>,
    /// `ingredients/tc/conflicts.json`; `None` when there is none, which is no conflict.
    pub conflicts: Result<Option<Vec<u8>>, String>,
    /// A story project's `ingredients/checking/resources.json`, for the gateway's titles.
    pub resources: Option<Result<Vec<u8>, String>>,
}

/// A story's number and its text, or why it could not be read.
pub type StoryText = (u8, Result<String, String>);

impl Default for SummaryFiles {
    fn default() -> Self {
        Self {
            books: Vec::new(),
            stories: None,
            conflicts: Ok(None),
            resources: None,
        }
    }
}

/// What a summary's read checks and counts.
pub struct SummaryWatch {
    pub cancel: crate::jobs::CancelToken,
    pub counters: Arc<crate::counters::AppCounters>,
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
    /// The projects folder itself is on a drive that cannot keep a project safe, or a cloud
    /// client copies it: a copy there would open read-only for the same reason (C1b).
    #[error("the projects folder cannot keep a copy safe: {0:?}")]
    Unsafe(ReadOnlyReason),
    #[error("{0}")]
    Failed(String),
}

/// A project to create: the folder its name makes and the first records of its history (the
/// details, the versification, the settings and the pins), written as one batch.
#[derive(Debug, Clone)]
pub struct NewProjectSpec {
    pub folder: ProjectRef,
    pub kind: ProjectKind,
    pub records: Vec<tc_journal::Record>,
}

/// Why a project was not created (tc-store `CreateError`). Nothing of it is left.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CreateFailure {
    /// Something is at the folder's name already; it keeps every byte.
    #[error("a folder with that name exists")]
    NameExists,
    /// A detail the Scripture Burrito cannot hold, in words (#582).
    #[error("{0}")]
    NotHeld(String),
    #[error("{0}")]
    Failed(String),
    /// The backend creates nothing (the memory backend).
    #[error("this backend creates no projects")]
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    /// The folder is no project of the projects folder (any more): it went away, or lost its
    /// `project.json`, since it was listed.
    #[error("not a project")]
    NotAProject,
    /// The kept copy was imported or dismissed already.
    #[error("the copy is no kept copy any more")]
    NotKept,
    #[error("{0}")]
    Other(String),
}

/// Why a checkpoint runs (tc-store `CheckpointReason`; D1 adds the others).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckpointReason {
    /// tC4's "leaving the project" checkpoint, when the person goes Home.
    LeavingProject,
    /// tC4's "leaving Translate" checkpoint, at a mode switch (D1).
    LeavingTranslate,
    /// A mode switch from another mode: its words name it ("leaving Check").
    LeavingMode(String),
    /// tC4's "before export" checkpoint: an export reads the project as committed (D2).
    BeforeExport,
    Other(String),
}

/// The projects of this computer.
pub trait ProjectStore: Send + Sync + 'static {
    /// The projects this store holds, and the folders that hold a project's files but cannot
    /// be opened as one. No lock, no fold.
    fn list(&self) -> Result<Listing, StoreError>;

    /// What `folder`'s Home card shows besides its summary: `metadata.json`, the Door43 remote
    /// and the summary's stamp. No lock, no fold. Blocking. `NotAProject` when the folder went
    /// away since it was listed.
    fn peek(&self, folder: &ProjectRef) -> Result<ProjectPeek, StoreError>;

    /// What a summary of `folder` is made of: the derived files (tc-store), or the same files
    /// rendered from the fold (memory). No lock. Blocking: the `HomeSummary` job calls it,
    /// and it stops at `watch.cancel` between files.
    fn summary_files(
        &self,
        folder: &ProjectRef,
        books: &[BookId],
        watch: &SummaryWatch,
    ) -> Result<SummaryFiles, StoreError>;

    /// One derived file of `folder` as the project holds it, read without a lock and without a
    /// fold: the store's file, or the memory backend's rendering of it. `Ok(None)` when the
    /// project has no such file. Blocking. Project settings opened from Home read their values
    /// this way (AD14).
    fn derived_file(
        &self,
        folder: &ProjectRef,
        file: &tc_journal::fold::DerivedFile,
    ) -> Result<Option<Vec<u8>>, StoreError>;

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

    /// Creates a project in the projects folder: built in a staging folder and renamed into
    /// place without replacing anything (R2), so a crash leaves the whole project or none.
    /// Blocking. Only the store on disk creates projects.
    fn create(&self, new: NewProjectSpec) -> Result<(), CreateFailure> {
        let _ = new;
        Err(CreateFailure::Unavailable)
    }

    /// The person said that the computer `folder`'s lock names is closed (STORAGE §5.0): the
    /// next open of `folder` takes the lock and opens writable, where it would otherwise open
    /// read-only with `OpenOnAnotherComputer`.
    fn trust_lock(&self, folder: &ProjectRef);

    /// Names this server run (§6.6): each open's drafts record it, and a draft of this run is
    /// its page's own typing, never offered to it.
    fn name_run(&self, run: tc_api::ids::ServerInstance);

    /// `test/reset`: every project back to its seed. The registry closes every session first.
    #[cfg(feature = "test-hooks")]
    fn reset(&self) -> Result<(), StoreError>;

    /// Teammates' versions of the verse `key` of `folder`, each by a teammate of its own, all
    /// from the verse's current version, written into the project as a sync's intake would
    /// (B10's scripted sync): the changes' ids. A store with no teammates refuses.
    #[cfg(feature = "test-hooks")]
    fn teammates_edit_verse(
        &self,
        folder: &ProjectRef,
        key: &tc_journal::RecordKey,
        texts: &[&str],
    ) -> Result<Vec<tc_journal::ChangeId>, StoreError> {
        let _ = (folder, key, texts);
        Err(StoreError::Other("this store has no teammates".to_owned()))
    }

    /// Teammate 1's join of the verses `first` to `last` of `chapter` of `book` in `folder`,
    /// one batch over the verses' current versions, written as a sync's intake would: the
    /// changes' ids. A store with no teammates refuses.
    #[cfg(feature = "test-hooks")]
    fn teammates_join_verses(
        &self,
        folder: &ProjectRef,
        book: BookId,
        chapter: tc_journal::ChapterNo,
        first: tc_journal::VerseNo,
        last: tc_journal::VerseNo,
    ) -> Result<Vec<tc_journal::ChangeId>, StoreError> {
        let _ = (folder, book, chapter, first, last);
        Err(StoreError::Other("this store has no teammates".to_owned()))
    }

    /// Teammates 1 and 2's versions of one record of `kind` in `folder`, both from one version,
    /// written as a sync's intake would ([`memory::teammates::Teammates::change_kind`]): the
    /// changes' ids. A store with no teammates refuses.
    #[cfg(feature = "test-hooks")]
    fn teammates_change_kind(
        &self,
        folder: &ProjectRef,
        kind: tc_api::test::TeammatesKind,
    ) -> Result<Vec<tc_journal::ChangeId>, StoreError> {
        let _ = (folder, kind);
        Err(StoreError::Other("this store has no teammates".to_owned()))
    }

    /// A scripted sync ended: Door43 has `folder`'s project as `fold` holds it, so the next
    /// teammates edit from it (B11). A store with no teammates keeps nothing.
    #[cfg(feature = "test-hooks")]
    fn synced(&self, folder: &ProjectRef, fold: FoldState) {
        let _ = (folder, fold);
    }

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

    /// The bytes of a carried file's version, the git blob `oid` its record names (STORAGE
    /// §11.4), for an export. Reads only. `Ok(None)` when this computer lacks the blob.
    fn carried_bytes(&self, oid: GitOid) -> BoxFuture<Result<Option<Vec<u8>>, StoreError>>;

    /// The store's operation counters (R14): files opened and created, flushes, renames,
    /// commands and their queue waits. What a count test reads.
    fn counters(&self) -> StoreCounters;

    /// The notices listed now (STORAGE §2.6): what the files and this computer showed that
    /// the fold does not list. A backend with no files has none.
    fn notices(&self) -> Vec<StoreNotice> {
        Vec::new()
    }

    /// The books another program changed that wait for the question, and every kept copy
    /// (D1, delta §2.3, §2.11), as the store's manifest lists them. A backend with no files has
    /// none.
    fn outside_edits(&self) -> tc_api::session::OutsideEdits {
        tc_api::session::OutsideEdits::default()
    }

    /// The window got focus: the store looks for files another program changed and holds them
    /// (STORAGE §11.1). A backend with no files finds none.
    fn focus(&self) -> BoxFuture<Result<(), StoreError>> {
        Box::pin(async { Ok(()) })
    }

    /// *Import* of the kept copy `copy` (delta §2.11 Q4).
    fn import_kept(&self, _copy: &str) -> BoxFuture<Result<tc_store::ImportReport, StoreError>> {
        Box::pin(async { Err(StoreError::Other("this backend keeps no copies".to_owned())) })
    }

    /// *Not now* for the waiting file `file`: the project keeps its text (delta §2.11 Q5).
    fn not_now(&self, _file: &str) -> BoxFuture<Result<(), StoreError>> {
        Box::pin(async { Err(StoreError::Other("this backend keeps no copies".to_owned())) })
    }

    /// *Dismiss* of the kept copy `copy`; refused while its book still waits.
    fn dismiss_kept(&self, _copy: &str) -> BoxFuture<Result<(), StoreError>> {
        Box::pin(async { Err(StoreError::Other("this backend keeps no copies".to_owned())) })
    }

    /// Drafts from an earlier session that differ from what their records hold (O12).
    fn offered_drafts(&self) -> Vec<OfferedDraft>;

    /// *Discard* of an offered draft: its entry leaves `drafts.json` (STORAGE §5.14), written
    /// before this returns. Whether the file holds the change.
    fn discard_draft(&self, id: u64) -> bool;

    /// Cancel only the current draft matching this editor's exact text, base and time.
    /// A newer entry and offered drafts stay; true means the resulting list is durable.
    fn discard_current_draft(
        &self,
        field: DraftField,
        shown: Vec<EditToken>,
        text: String,
        at: tc_journal::WallTime,
    ) -> bool;

    /// Takes an unsaved text in as an offered draft of `drafts.json` (APP-DESIGN §6.6),
    /// written before this returns its number; the same text of the field already offered
    /// keeps its own. `None` when it could not be written (a read-only project, a disk that
    /// refuses): the text stays where it was.
    fn offer_draft(
        &self,
        field: DraftField,
        shown: Vec<DraftToken>,
        text: String,
        verified: bool,
        at: tc_journal::WallTime,
    ) -> Option<u64>;

    /// Blocks until the store's queue is empty and its background work is done: a test
    /// calls it before it counts.
    #[cfg(feature = "test-hooks")]
    fn wait_idle(&self);

    /// Closes the project and lets go of its lock.
    fn close(self: Arc<Self>) -> BoxFuture<Result<(), StoreError>>;
}
