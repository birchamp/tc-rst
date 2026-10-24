//! The `session` service (APP-DESIGN §3.9): opening a project for the screens and closing it at
//! Home (B3), and the draft offer and copy (§6.6, §6.4, C1b).
//!
//! At an open after a restart or a crash, unsaved text can be in three places: the project's
//! `.tc/drafts.json`, the page's draft buffer and the shell's mirror. The UI sends the folder's
//! buffered entries with `session/drafts`; the server judges every text against the record
//! (`model::draft`), takes each unsaved one into `drafts.json` as an offered entry before it
//! answers, and answers the questions and the buffer slots the page then drops. *Keep* and
//! *Discard* name an offered entry by its number, so one place answers each question, and a
//! reload simply asks again.
//!
//! A read-only project offers nothing and takes nothing in. It is copied with `session/copy`;
//! the copy takes the folder's `drafts.json`, and the UI moves the folder's buffer items and
//! mirror lines to the copy, whose first open offers them.

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::app::OpenSession;
use crate::buffer::{BufferKey, FieldPlace};
use crate::check::Tool;
use crate::home::ProjectKind;
use crate::ids::{
    ProjectId, ProjectRef, RequestId, Scope, ServerInstance, StoryNo, ViewSeq, WallMs,
};
use crate::prefs::{DraftUnit, PlaceMode};
use crate::shown::Direction;
use crate::token::ShownToken;
use crate::write::{ConflictBrief, Stored};

/// `session/open`: opens a project for the screens. A job: its `Opened` outcome carries the
/// session and what the screens need first. Opening any project stops the Home summary that
/// runs (§4.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Open {
    pub request: RequestId,
    pub project: ProjectRef,
    /// The tile the person chose, whose remembered place the outcome carries; none for the
    /// project's first book.
    pub unit: Option<OpenUnit>,
}

/// A book or a story of the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum OpenUnit {
    Book(#[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))] BookId),
    Story(StoryNo),
}

/// What an open gives the screens: the session, the project's chip, its books and the place
/// the person last worked in the unit asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct OpenedProject {
    pub session: OpenSession,
    /// The name in the project's default language, as Home's card names it.
    pub name: String,
    /// The project language's tag.
    pub tag: String,
    /// The direction of the project's script.
    pub dir: Direction,
    pub kind: ProjectKind,
    /// A Bible's books in canonical order; empty for a story project.
    #[cfg_attr(
        feature = "proptest",
        proptest(strategy = "proptest::collection::vec(crate::arb::book(), 0..4)")
    )]
    pub books: Vec<BookId>,
    /// Section or Verse, as this computer last showed the project (Section until set).
    pub draft_unit: DraftUnit,
    /// The place kept for the unit asked for, if any: a Home tile returns there.
    pub place: Option<KeptPlace>,
}

/// A place a Home tile returns to: the mode, the chapter (or story) and the Check tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct KeptPlace {
    pub mode: PlaceMode,
    pub chapter: u16,
    /// The tool open when the person left Check (tC4's `resumeCheckTool`, B5a).
    pub tool: Option<Tool>,
}

/// `session/close`: the person went Home. The screen's session closes after its "leaving the
/// project" checkpoint, behind the reply (§4.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Close {
    pub request: RequestId,
    pub at: Scope,
}

/// A mode of a project's screens, as a checkpoint's message names it (tC4 `MODE_NAME`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum ScreenMode {
    Understand,
    Translate,
    Check,
    CommunityChecking,
}

/// `session/leave` (D1, tC4 `go`): the person left a mode for another. The server checkpoints
/// when its policy says a mode switch is due (at most every two minutes today); its outcome is
/// the session's `Checkpoint` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LeaveMode {
    pub request: RequestId,
    pub at: Scope,
    pub from: ScreenMode,
}

/// `session/retryCheckpoint` (D1, tC4 `retryCheckpoint`): the save indicator's Retry after a
/// checkpoint failed. The project's checkpoint runs again; its outcome is the reply and the
/// session's `Checkpoint` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetryCheckpoint {
    pub request: RequestId,
    pub at: Scope,
}

/// `session/drafts`: what to offer of the open project's unsaved text. A command: the texts it
/// offers are written into `drafts.json` before it answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Drafts {
    pub at: Scope,
    pub request: RequestId,
    /// The folder's buffered entries, page and mirror merged (`tc_ui::app::buffer::merge`),
    /// but those the page's save machine holds, each a `changed`
    /// [`BufferLine`](crate::buffer::BufferLine) as JSON text: an entry keeps keys this build
    /// does not know, which a request's own fields would refuse. A line that does not parse, a
    /// removal, and an entry of another folder or project id are skipped.
    pub buffered: Vec<String>,
    /// `buffered` includes the shell's mirror (`tc4Shell.takeBuffered`), which holds every
    /// keystroke and every removal of each run: a run with no entry of a field ended with the
    /// field clean, so its stored drafts of the field are older than its save.
    pub mirror: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct OfferedDrafts {
    /// "Unsaved text from your last session was found for Titus 1:4." One question at a time,
    /// in this order. Empty while the project is read-only: nothing can be kept there, and
    /// the entries wait for the copy or a writable open.
    #[cfg_attr(
        feature = "proptest",
        proptest(
            strategy = "proptest::collection::vec(proptest::arbitrary::any::<DraftOffer>(), 0..3)"
        )
    )]
    pub offers: Vec<DraftOffer>,
    /// Buffer slots whose text is held elsewhere now: by the record, or by an offered entry of
    /// `drafts.json` written before this answer. The UI drops them, each only while its item
    /// and its mirror line still hold that text.
    #[cfg_attr(
        feature = "proptest",
        proptest(
            strategy = "proptest::collection::vec(proptest::arbitrary::any::<BufferSlot>(), 0..2)"
        )
    )]
    pub settle: Vec<BufferSlot>,
}

/// One unsaved text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct DraftOffer {
    pub kind: DraftKind,
    /// Where the field is, for the question's words: a buffered entry's own place, or the
    /// server's for a stored draft (`TIT 1:4`).
    pub place: FieldPlace,
    pub text: String,
    pub keep: Keep,
    /// The tokens *Keep* saves over: the version the person was editing, or the stored
    /// value's when the save the entry waited for had landed (§6.6, A4b).
    pub shown: Vec<ShownToken>,
    /// What *Keep* and *Discard* settle.
    pub settles: Settles,
}

/// A field of unsaved text, as the store keeps its drafts (STORAGE §5.14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum DraftKind {
    /// A verse's words.
    Words,
    /// A chapter's or book's front matter and headings.
    Layout,
    /// The section editor's buffer.
    Section,
    /// An OBS frame or a story's title (frame 0).
    Frame,
    StoryRef,
    CheckComment,
    Comment,
    /// The aligner's unsaved links (R23).
    Alignment,
}

/// What *Keep* does, and so which words the question uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Keep {
    /// "Keep it / Discard": saved over its tokens, so a record changed since becomes a
    /// conflict, never a revert.
    Save,
    /// "…but this computer cannot tell which version you were editing. Keep it as a separate
    /// version / Discard": its tokens fail their MAC (a new installation secret, a folder
    /// copied from another computer), so it is written with no parents and stands beside the
    /// current value as a conflict (R20).
    AsVersion,
    /// "Copy text / Discard": this build has no save for the field yet (a section before
    /// B4b, a new comment before C3). The offer stays until Discard.
    CopyText,
}

/// Where one offer's text is: an offered entry of `drafts.json`, and, for "Copy text", which
/// no entry can hold, the buffer slots.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct Settles {
    /// Entries of `drafts.json`, by the store's number for this open.
    pub stored: Vec<StoredDraftId>,
    /// Buffer items the UI removes once the offer is kept or discarded.
    #[cfg_attr(
        feature = "proptest",
        proptest(
            strategy = "proptest::collection::vec(proptest::arbitrary::any::<BufferSlot>(), 0..2)"
        )
    )]
    pub buffered: Vec<BufferSlot>,
}

/// One buffer item, as a removal names it (§6.6): the field, the run, the text the item must
/// still hold to be removed, so a newer text typed meanwhile stays, and the entry's time: an
/// item of the run no newer than it holds an older text of the same typing (a storage that
/// refused the newer one), and goes too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct BufferSlot {
    pub key: BufferKey,
    pub instance: ServerInstance,
    pub text: String,
    pub at: WallMs,
}

/// The store's number of an offered draft of `drafts.json`, valid for the open that offered it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(transparent)]
pub struct StoredDraftId(pub u64);

/// `session/keepDraft`: *Keep* of an offer, as its field's save with the offer's tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeepDraft {
    pub at: Scope,
    pub request: RequestId,
    pub kind: DraftKind,
    pub shown: Vec<ShownToken>,
    pub text: String,
    /// The offer's entries of `drafts.json`, which go once the text is kept.
    pub discard: Vec<StoredDraftId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct KeptDraft {
    /// The snapshot that holds the kept text (read-your-writes).
    pub view: ViewSeq,
    pub stored: Stored,
    /// "Juan changed this verse while you were editing": the kept text stands beside it.
    pub new_conflicts: Vec<ConflictBrief>,
}

/// `session/discardDraft`: *Discard* of an offer's entries of `drafts.json`, written before the
/// answer. The UI drops the offer's buffer slots itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscardDraft {
    pub at: Scope,
    pub request: RequestId,
    pub stored: Vec<StoredDraftId>,
}

/// `session/copy`: the copy offer of a read-only project (§6.4, STORAGE §5.0). The copy is a
/// new folder in the projects folder, with every save and the folder's `drafts.json`. It is
/// refused (`CreateRefusal::CopyUnsafe`) when the projects folder itself is on a drive that
/// cannot keep a project safe or is copied by a cloud client: the copy would open read-only
/// for the same reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CopyProject {
    pub at: Scope,
    pub request: RequestId,
}

/// The copy. The UI moves the folder's buffer items and mirror lines to `project` with `id`
/// (`tc_ui::app::offer::copy`), then opens it; its first open offers them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ProjectCopy {
    pub project: ProjectRef,
    pub id: ProjectId,
}

/// `session/otherComputerClosed`: the person says that the computer the project's lock names
/// is closed (STORAGE §5.0, `OpenOnAnotherComputer`). The next open of the folder takes the
/// lock and opens writable; the UI goes Home and opens it again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OtherComputerClosed {
    pub at: Scope,
    pub request: RequestId,
}
