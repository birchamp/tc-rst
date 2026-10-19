//! The `session` service's draft offer and copy (APP-DESIGN §6.6, §6.4, C1b).
//!
//! At an open after a restart or a crash, unsaved text can be in three places: the project's
//! `.tc/drafts.json`, the page's draft buffer and the shell's mirror. The UI sends the folder's
//! buffered entries with `session/drafts`; the server merges them with the stored drafts and
//! answers what to offer. An offer carries what *Keep* sends back, so the server keeps no state
//! between the two requests and a reload simply asks again.
//!
//! A read-only project is copied with `session/copy`; the copy takes the folder's
//! `drafts.json`, and the UI moves its buffered entries to the copy, whose first open offers
//! them.

use serde::{Deserialize, Serialize};

use crate::buffer::{BufferKey, FieldPlace};
use crate::ids::{ProjectId, ProjectRef, RequestId, Scope, ServerInstance, ViewSeq};
use crate::token::ShownToken;
use crate::write::{ConflictBrief, Stored};

/// `session/drafts`: what to offer of the open project's unsaved text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Drafts {
    pub at: Scope,
    /// The folder's buffered entries, page and mirror merged (`tc_ui::app::buffer::merge`),
    /// each a `changed` [`BufferLine`](crate::buffer::BufferLine) as JSON text: an entry keeps
    /// keys this build does not know, which a request's own fields would refuse. A line that
    /// does not parse, a removal, and an entry of another folder or project id are skipped.
    pub buffered: Vec<String>,
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
    /// Buffered entries whose text the project already holds: the UI removes them, each only
    /// while its item still holds that text.
    #[cfg_attr(
        feature = "proptest",
        proptest(
            strategy = "proptest::collection::vec(proptest::arbitrary::any::<BufferSlot>(), 0..2)"
        )
    )]
    pub saved: Vec<BufferSlot>,
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

/// The stored drafts and buffered entries one offer stands for.
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

/// One buffer item, as a removal names it (§6.6): the field, the run, and the text the item
/// must still hold to be removed, so a newer text typed meanwhile stays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct BufferSlot {
    pub key: BufferKey,
    pub instance: ServerInstance,
    pub text: String,
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
    /// The offer's other stored drafts, discarded once the text is kept.
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

/// `session/discardDraft`: *Discard* of an offer's stored drafts. The UI removes the
/// buffered entries itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscardDraft {
    pub at: Scope,
    pub request: RequestId,
    pub stored: Vec<StoredDraftId>,
}

/// `session/copy`: the copy offer of a read-only project (§6.4, STORAGE §5.0). The copy is a
/// new folder in the projects folder, with every save and the folder's `drafts.json`; it
/// opens writable unless its own place makes it read-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CopyProject {
    pub at: Scope,
    pub request: RequestId,
}

/// The copy. The UI moves the folder's buffered entries to `project` with `id`
/// (`tc_ui::app::buffer::rekey`), then opens it; its first open offers them.
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
