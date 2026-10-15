//! Commands (`docs/STORAGE.md` §2.7): what a screen asks the project actor in `tc-store` to
//! do. Each one is one batch, all or nothing. Every command that changes a record carries
//! the [`ShownToken`] of what was displayed, so its parents are what the person saw (I4).

use crate::change::Cause;
use crate::fold::ConflictId;
use crate::ids::{BatchId, ChangeId};
use crate::json::{Json, JsonMap};
use crate::record::{
    AlignmentGroup, ChapterFront, CommentTarget, PinGroupValue, Record, ResourceRef, Scope,
    Selection, TargetWord, UserStatus, VerseValue,
};
use crate::token::ShownToken;
use tc_core::BookId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Keeps marker, layout, last and from (§5.1).
    SaveVerse {
        shown: ShownToken,
        words: String,
    },
    /// A paragraph break after a verse (§5.1).
    SaveLayout {
        shown: ShownToken,
        layout: String,
    },
    /// One token per verse and chapter front the editor displayed; `None` = displayed and
    /// unchanged (§5.2). `layout` is the book's `BookLayout` token, when it has one.
    SaveSection {
        book: BookId,
        shown: Vec<(ShownToken, Option<SectionValue>)>,
        layout: Option<ShownToken>,
    },
    SaveAlignment {
        shown: ShownToken,
        verse_shown: ShownToken,
        value: AlignmentEdit,
    },
    SaveDecision {
        shown: ShownToken,
        verse_shown: ShownToken,
        value: DecisionEdit,
    },
    AddComment {
        target: CommentTarget,
        text: String,
    },
    SetSetting {
        shown: ShownToken,
        value: Option<Json>,
    },
    SetMeta {
        shown: ShownToken,
        value: Option<Json>,
    },
    /// Outside an upgrade (§5.6).
    SetPins {
        shown: ShownToken,
        value: PinGroupValue,
    },
    AddBook {
        book: BookId,
        scope: Scope,
        seed: SeedSource,
    },
    RemoveBook {
        book: BookId,
        presence_shown: ShownToken,
    },
    RestoreBook {
        book: BookId,
        presence_shown: ShownToken,
        mode: RestoreMode,
    },
    /// OBS (§5.15); a story's reference line is saved the same way with its own token.
    SaveFrame {
        shown: ShownToken,
        text: String,
    },
    /// §5.8.
    ApplyUpgrade {
        plan: UpgradePlan,
    },
    /// §5.9, §8.3: one token per record involved, each listing every head as the
    /// resolution screen displayed it.
    Resolve {
        conflict: ConflictId,
        shown: Vec<ShownToken>,
        choice: Choice,
    },
    Restore {
        shown: ShownToken,
        from: ChangeId,
    },
    RemoveCarriedFile {
        shown: ShownToken,
    },
    /// A passage set (D26).
    SetBookScope {
        presence_shown: ShownToken,
        scope: Scope,
    },
    /// "Apply what was recovered" (§7.4).
    ReleaseBatch {
        batch: BatchId,
    },
    /// A Detached alignment or decision (§7.7).
    Dismiss {
        shown: ShownToken,
    },
    /// §5.14; writes no change.
    Draft {
        shown: ShownToken,
        field: DraftField,
        text: String,
    },
}

/// A new value for the record a section token names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectionValue {
    Verse(VerseValue),
    ChapterFront(ChapterFront),
}

/// What the aligner made. The actor adds what it was made against from the displayed verse
/// and the pins (§5.3): span end, text hash, source, source version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignmentEdit {
    pub alignments: Vec<AlignmentGroup>,
    pub word_bank: Vec<TargetWord>,
    /// D73, as the aligner computed it.
    pub done: bool,
}

/// What the checker changed in one item (§5.4). The actor applies the changes to the
/// displayed value, or to `basis` for a first decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionEdit {
    /// For a first decision: the derived check item's context, category and the tN or TWL
    /// pin the item list came from.
    pub basis: Option<DecisionBasis>,
    pub changes: Vec<DecisionChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionBasis {
    pub context: JsonMap,
    pub category: Option<Box<str>>,
    pub resource: ResourceRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionChange {
    Status(Option<UserStatus>),
    Selections(Option<Vec<Selection>>),
    NothingToSelect(bool),
    Comment(Option<String>),
    Bookmarked(bool),
}

/// The literal source a new book's stubs copy their chapters, verses, spans and paragraph
/// markers from (tC4 `seedBookFromSource`, §5.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedSource {
    pub usfm: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreMode {
    KeepText,
    StartEmpty,
}

/// A resource upgrade or gateway change planned off the actor from a snapshot (§5.8). Each
/// planned change names the heads its record had in the snapshot; the actor confirms that
/// every record still has them before it writes the plan as one batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradePlan {
    /// The new pin groups, the carried-over and moved decisions.
    pub changes: Vec<PlannedChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedChange {
    pub record: Record,
    /// The record's heads in the snapshot.
    pub parents: Vec<ChangeId>,
    pub cause: Cause,
}

/// A person's choice in a conflict (§8.3). Either way the batch writes one change per
/// record of the conflict, with every current head of that record as parents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// Keep one candidate: its value on its record, and for a grouped conflict its side's
    /// value on every record of the group.
    Candidate { head: ChangeId },
    /// Values the person edited, combined or chose unit by unit, and confirmed: one per
    /// record of the conflict.
    Values(Vec<Record>),
}

/// Which editor field a draft belongs to (§5.14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftField {
    Words,
    Layout,
    /// The section editor's whole buffer, with the tokens of every displayed record.
    Section,
    Frame,
    StoryRef,
    CheckComment,
}
