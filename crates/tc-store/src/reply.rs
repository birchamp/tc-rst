//! What a command gets back (`docs/STORAGE.md` §5.0 B6, APP-DESIGN §4.2): the new tokens,
//! the conflicts it caused, how durable the save is, and the sequence number of the
//! snapshot that holds it; or why nothing was written.

use crate::platform::UnsafeFolder;
use tc_journal::fold::ConflictId;
use tc_journal::token::TokenError;
use tc_journal::{ChangeId, DeviceId, RecordKey, ShownToken};

/// How far a save got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stored {
    /// "Saved": the pending file and its rename are durable (§5.0 B3).
    Durable,
    /// "Saved, but the disk reported an error: <reason>. Your work may not survive a power
    /// cut. Check the disk." The file is in place, so the save counts as written.
    WrittenNotDurable(String),
    /// Nothing to write: the new value equals the displayed one, or the command writes no
    /// change (a draft).
    Unchanged,
}

/// The reply to a command that was not refused.
#[derive(Debug, Clone)]
pub struct StoreReply {
    /// A new edit token for each record the batch wrote, listing what the screen now shows.
    pub tokens: Vec<(RecordKey, ShownToken)>,
    /// Conflicts that did not exist before the batch.
    pub new_conflicts: Vec<ConflictId>,
    pub stored: Stored,
    /// The sequence number of the first published snapshot that holds the batch: a read of
    /// that snapshot or a later one sees the save (APP-DESIGN R16).
    pub seq: u64,
    /// The changes the batch wrote.
    pub changes: Vec<ChangeId>,
    /// Conflicts a bulk choice left for a choice of their own.
    pub skipped: Vec<ConflictId>,
}

/// Why a project is read-only (§5.0, §6.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOnlyReason {
    /// The lock names another computer: the folder may be open there (§5.0).
    OpenOnAnotherComputer { host: String },
    /// Another installation is writing into this folder (§5.0, `SecondWriter`).
    SecondWriter { device: DeviceId },
    /// FAT32, exFAT or a cloud-synced folder.
    UnsafeFolder(UnsafeFolder),
    /// The project needs a newer translationCore to be changed (§12).
    NewerMinor { min_reader_minor: String },
    /// The project folder stopped being writable while open.
    WriteFailed(String),
}

/// Why a command wrote nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    /// The command breaks a rule; the reason is in words.
    #[error("{0}")]
    Refused(Refusal),
    /// The disk refused the save: "Not saved: <reason>. Retry". The editor keeps its text;
    /// a retry builds the change again from the same token.
    #[error("Not saved: {0}. Retry")]
    NotWritten(String),
    /// "A newer change to this verse just arrived. Look again." The records moved since the
    /// screen was drawn, and the command must not guess.
    #[error("A newer change just arrived. Look again.")]
    LookAgain,
    /// An upgrade plan made from a snapshot whose records moved: plan again (§5.8).
    #[error("the project changed while the upgrade was planned; it is planned again")]
    PlanStale,
    #[error("the project is open read-only")]
    ReadOnly(ReadOnlyReason),
    #[error("the token was not made by this installation for this project: {0}")]
    BadToken(TokenError),
    /// The project is closing or closed.
    #[error("the project is closed")]
    Closed,
}

/// What rule a refused command broke (§5.0 B1).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("this command does not save {0}")]
    WrongRecord(&'static str),
    #[error(
        "a resolution screen's token cannot save an edit, and an editor's cannot resolve a conflict"
    )]
    WrongTokenKind,
    #[error("the verse is not text here; change its structure in the section editor")]
    NotText,
    #[error("{0}")]
    Rule(String),
    #[error("the section as shown is not coherent: {0}")]
    Incoherent(String),
    #[error("a project needs at least one book: add another book before removing this one")]
    LastBook,
    #[error("{0}")]
    NotValidInBurrito(String),
    #[error("a version shown here cannot be read on this computer yet")]
    Unreadable,
    #[error("{0}")]
    Resolve(String),
    #[error("{0}")]
    Text(String),
    #[error("{0}")]
    Book(String),
    #[error("nothing recovered of this batch waits to be applied")]
    NothingToRelease,
    #[error("only an alignment or a check decision is dismissed")]
    NotDismissable,
    #[error("this command is for the other kind of project")]
    WrongProject,
    #[error("a batch never holds two changes of one record")]
    TwoChangesOfOneRecord,
}
