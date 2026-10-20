//! Commands (`docs/STORAGE.md` §2.7): what a screen asks the project actor in `tc-store` to
//! do. Each one is one batch, all or nothing. Every command that changes a record carries
//! the [`ShownToken`] of what was displayed, so its parents are what the person saw (I4).

use crate::change::Cause;
use crate::fold::ConflictId;
use crate::ids::{BatchId, ChangeId, DeviceId, RepoPath, WallTime};
use crate::json::{Json, JsonMap};
use crate::keys::CheckKey;
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
    /// A new version of the comment the comment box showed (§5.5, APP-DESIGN R24): the same
    /// comment record, its earlier text kept in History. The target stays.
    ReviseComment {
        shown: ShownToken,
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
    /// A book added with its own text, in one batch (tC4 `addUsfmBooks`, #767): the presence,
    /// the book's records and the alignments of its aligned verses, all or none. The cause is
    /// an import of `file_name`.
    AddBookText {
        book: BookId,
        scope: Scope,
        usfm: String,
        file_name: String,
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
    /// §5.9, §8.3: one review token per record involved, each listing every head as the
    /// resolution screen displayed it.
    Resolve {
        conflict: ConflictId,
        shown: Vec<ShownToken>,
        choice: Choice,
    },
    /// A bulk choice (§5.9): for each conflict listed, the one candidate made on a computer
    /// of `devices`, all in one batch. Each conflict carries its review tokens; a conflict
    /// two or more candidates of the set, or none, come from is left out and stays listed.
    ResolveBulk {
        devices: std::collections::BTreeSet<DeviceId>,
        conflicts: Vec<(ConflictId, Vec<ShownToken>)>,
    },
    /// Re-key a decision a carry-over could not place (§8.3, APP-DESIGN R7): the old key
    /// gets `MovedTo { to }` with the heads `shown` lists, and `to` gets the decision the
    /// screen showed, with the heads `to_shown` lists (none for a check never decided), so a
    /// decision already at `to` is a visible conflict, never replaced.
    MoveCheck {
        shown: ShownToken,
        to: CheckKey,
        to_shown: ShownToken,
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
    /// §5.14: kept in `.tc/drafts.json`, beside the actor; writes no change. `at` is the
    /// page's clock at the text's last keystroke, the clock of the page's buffer entries, so
    /// the open compares the two (APP-DESIGN §6.6).
    Draft {
        draft: Draft,
        at: WallTime,
    },
    /// *Keep* a draft offered at open (§5.14): the draft saved with its tokens, as the save of
    /// its field would be. A token this installation can no longer verify (a new
    /// installation secret, APP-DESIGN R20) is no base: the draft's value is written with no
    /// parents, so it stands as a conflict with what the record holds unless the values are
    /// equal. A section draft's values are given in `section`, read from its buffer by the
    /// section editor, with the tokens of `draft.shown`.
    KeepDraft {
        draft: Draft,
        section: Vec<(ShownToken, Option<SectionValue>)>,
    },
}

/// Unsaved editor text (§5.14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub field: DraftField,
    /// The tokens of what the editor displayed: the field's record, every record the section
    /// editor displayed (APP-DESIGN R6), the comment a revision continues, or none for a new
    /// comment.
    pub shown: Vec<ShownToken>,
    /// The editor's text: the field's text, the section buffer, or, for an alignment, the
    /// unsaved links as [`AlignmentEdit::to_json`] writes them.
    pub text: String,
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

impl AlignmentEdit {
    /// The canonical JSON of the edit, as a draft keeps it: `{"alignments", "done",
    /// "wordBank"}` with groups and words in the wire form of §2.8.
    pub fn to_json(&self) -> String {
        crate::canon::alignment_edit_json(self)
    }

    /// Reads what [`AlignmentEdit::to_json`] wrote; `None` for anything else.
    pub fn from_json(text: &str) -> Option<Self> {
        crate::canon::alignment_edit_from_json(text)
    }
}

/// What the checker changed in one item (§5.4). [`DecisionEdit::apply`] applies the changes
/// to the displayed value, or to `basis` for a first decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionEdit {
    /// For a first decision: the derived check item's context and category.
    pub basis: Option<DecisionBasis>,
    /// The tN or TWL pin of the check list the person judged in: the (tool, book)'s current
    /// resolution as the screen resolved it (§7.7). A judgement records it.
    pub resource: ResourceRef,
    pub changes: Vec<DecisionChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionBasis {
    pub context: JsonMap,
    pub category: Option<Box<str>>,
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

/// A person's choice in a conflict (§8.3). The batch writes one change per record the
/// choice decides, with every current head of that record as parents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// Keep one candidate: its value on its record, and for a grouped conflict its side's
    /// value on every record of the group. For a book's presence race, *Restore the text*
    /// also restores every verse *Start empty* blanked (§7.6).
    Candidate { head: ChangeId },
    /// Values the person edited, combined or chose unit by unit, and confirmed: one per
    /// record of the conflict.
    Values(Vec<Record>),
    /// *Keep Maria's passage*: the first head names the side, each picks its record's
    /// version (`FoldState::keep_passage`).
    KeepPassage { heads: Vec<ChangeId> },
    /// *Keep 4 and 5 separate*, with the versions picked where a verse holds several.
    KeepSeparate { heads: Vec<ChangeId> },
    /// *Keep the joined verses*, with the span picked where there are several.
    KeepJoined { heads: Vec<ChangeId> },
    /// *Apply Pedro's change to the moved check*: writes the moved check's new key too.
    ApplyToMovedCheck,
    /// *Rename* one file of a path-case conflict: writes the new path's record too.
    RenameCarried { from: RepoPath, to: RepoPath },
}

/// Which editor field a draft belongs to (§5.14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftField {
    Words,
    Layout,
    /// The section editor's whole buffer of `book`, with the tokens of every displayed
    /// record (APP-DESIGN R6).
    Section {
        book: BookId,
    },
    Frame,
    StoryRef,
    CheckComment,
    /// The comment box (APP-DESIGN R15): a new comment on `target`, or a revision of the
    /// comment its one token names.
    UserComment {
        target: CommentTarget,
    },
    /// The aligner's unsaved links (APP-DESIGN R23).
    Alignment,
}
