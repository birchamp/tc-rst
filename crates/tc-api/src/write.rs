//! What a write gets back (APP-DESIGN §3.4, AD4, AD5).
//!
//! Every write request carries `at: Scope`, `request: RequestId` and the tokens of what was
//! displayed; each screen's unit defines its own requests. The reply is an [`Ack`] once the
//! change is durable (STORAGE's B3), or [`NotSaved`] inside `ApiError::NotSaved`, and the
//! text then stays in the editor.

use serde::{Deserialize, Serialize};

use crate::check::RailRowPatch;
use crate::ids::{ConflictRef, RecordRef, ViewSeq};
use crate::shown::PersonLabel;
use crate::token::ShownToken;

/// A write that is durable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct Ack {
    /// The session view that holds this write: a read with this view or a later one sees it.
    pub view: ViewSeq,
    pub stored: Stored,
    /// The tokens the next save of each record uses (STORAGE:1455).
    pub tokens: Vec<TokenUpdate>,
    /// Conflicts this write made: "Juan changed this verse while you were editing".
    pub new_conflicts: Vec<ConflictBrief>,
    /// The text as stored, in NFC. A field is clean only when this equals the NFC form of
    /// its current text; comparing with the text that was sent would mark newer typing as
    /// saved after a retry (AD4).
    pub saved: Vec<SavedValue>,
    /// Check rail rows this write changed (AD17).
    pub rows: Vec<RailRowPatch>,
    /// The change was renamed into place but the disk reported an error after it
    /// (STORAGE:1424-1428): saved, with a warning.
    pub disk_warning: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Stored {
    Written,
    /// The value equalled the displayed one, so no change was written (STORAGE:1447).
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct TokenUpdate {
    pub record: RecordRef,
    pub token: ShownToken,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ConflictBrief {
    pub conflict: ConflictRef,
    pub record: RecordRef,
    pub by: PersonLabel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct SavedValue {
    pub record: RecordRef,
    pub text: String,
}

/// Why a write stored nothing. The field keeps its text and shows
/// "Not saved: <reason>. Retry".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NotSaved {
    /// A rule of the command refused it (STORAGE B1, :1411-1413).
    Refused(SaveRefusal),
    /// The change could not be written (B3 `NotWritten`, STORAGE:1422-1424). A retry uses
    /// the same token.
    Disk(String),
    /// O1 another computer, O4, O5, a second writer, a write that failed at open.
    ReadOnly(ReadOnlyReason),
    /// A resolution over heads that moved since the review was shown (STORAGE:1615-1616).
    LookAgain,
    /// An upgrade or gateway plan over heads that moved (STORAGE:1603-1606).
    PlanStale,
    /// The project was closed or opened again under the field. The UI attaches to the
    /// current session and sends the text again with the field's base token, so a change
    /// made meanwhile becomes a conflict, never a revert (A-19).
    StaleScope,
    /// A forged token, or a token of the wrong kind: a bug, reported. A draft whose token
    /// no longer verifies never gets here; it is offered as text (§6.6).
    BadToken,
}

/// A save a command's rules refuse. Each screen's unit adds the rules its commands have.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum SaveRefusal {
    /// The field's text breaks its content rules; the editor keeps the text and shows why.
    InvalidText { reason: String },
    /// The displayed values of a passage do not fit together: an editor bug, reported
    /// (STORAGE §5.2).
    IncoherentView,
    /// The last present book of a project cannot be removed (STORAGE §5.7).
    LastBook,
}

/// Why a project is open read-only (STORAGE §5.0, §6.1, O1, O4, O5). Unit variants only, so
/// each is a plain string on the wire; the host a lock names and the reason a write failed
/// stay in the store's facts and notices, and the banners (`tc_ui::app::offer::banner`, C1b)
/// name neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum ReadOnlyReason {
    /// O4: the project needs a newer app to write it.
    NeedsNewerApp,
    /// O5: a FAT32 or exFAT drive, which cannot keep a rename safe through a power cut.
    UnsafeDrive,
    /// O5: a folder that a cloud service copies to other computers.
    CloudSynced,
    /// Another installation saves into this folder now.
    SecondWriter,
    /// O1: the project's lock names another computer; the project may be open there
    /// (STORAGE §5.0). The person can say that computer is closed
    /// (`session/otherComputerClosed`).
    OpenOnAnotherComputer,
    /// A write the open needed failed (a full disk, a folder that went read-only): the
    /// project shows everything it holds, and the next open finishes the work (STORAGE §6.1).
    FolderNotWritable,
}
