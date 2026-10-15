//! Records and their values (`docs/STORAGE.md` §2.3). Each change carries one [`Record`];
//! the enum pairs every key with its value type, so a verse key with an alignment value
//! cannot be built.

use crate::ids::{
    BatchId, ChangeId, ChapterNo, CommentId, DeviceId, FrameNo, GitOid, Md5, RepoPath, ShortId,
    StoryNo, VerseNo,
};
use crate::json::{Json, JsonMap};
use crate::keys::{CheckKey, MetaField, PinGroup, RecordKey, SettingKey, SlotName, UnknownKey};
use std::collections::BTreeMap;
use tc_core::BookId;

pub use tc_burrito::ScopeRange;
pub use tc_usfm::codec::{MarkerTerm, MarkerWs};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Record {
    BookPresence {
        book: BookId,
        value: BookPresence,
    },
    BookHeader {
        book: BookId,
        value: UsfmBytes,
    },
    ChapterFront {
        book: BookId,
        chapter: ChapterNo,
        value: ChapterFront,
    },
    Verse {
        book: BookId,
        chapter: ChapterNo,
        verse: VerseNo,
        value: VerseValue,
    },
    BookLayout {
        book: BookId,
        value: Option<Vec<LayoutItem>>,
    },
    Alignment {
        book: BookId,
        chapter: ChapterNo,
        verse: VerseNo,
        value: AlignmentValue,
    },
    Check {
        key: CheckKey,
        value: CheckValue,
    },
    Comment {
        id: CommentId,
        value: CommentValue,
    },
    Pins {
        group: PinGroup,
        value: PinGroupValue,
    },
    /// `None` = the key was removed.
    Setting {
        key: SettingKey,
        value: Option<Json>,
    },
    ProjectMeta {
        field: MetaField,
        value: Option<Json>,
    },
    Versification {
        value: VersificationValue,
    },
    DeviceProfile {
        device: DeviceId,
        value: DeviceProfileValue,
    },
    StoryBase {
        story: StoryNo,
        value: String,
    },
    Frame {
        story: StoryNo,
        frame: FrameNo,
        value: String,
    },
    StoryRef {
        story: StoryNo,
        value: Option<String>,
    },
    CarriedFile {
        path: RepoPath,
        value: CarriedValue,
    },
    BatchRelease {
        batch: BatchId,
        value: BatchReleaseValue,
    },
    Unknown(UnknownRecord),
}

impl Record {
    pub fn key(&self) -> RecordKey {
        match self {
            Self::BookPresence { book, .. } => RecordKey::BookPresence(*book),
            Self::BookHeader { book, .. } => RecordKey::BookHeader(*book),
            Self::ChapterFront { book, chapter, .. } => RecordKey::ChapterFront(*book, *chapter),
            Self::Verse {
                book,
                chapter,
                verse,
                ..
            } => RecordKey::Verse(*book, *chapter, *verse),
            Self::BookLayout { book, .. } => RecordKey::BookLayout(*book),
            Self::Alignment {
                book,
                chapter,
                verse,
                ..
            } => RecordKey::Alignment(*book, *chapter, *verse),
            Self::Check { key, .. } => RecordKey::Check(key.clone()),
            Self::Comment { id, .. } => RecordKey::Comment(*id),
            Self::Pins { group, .. } => RecordKey::Pins(*group),
            Self::Setting { key, .. } => RecordKey::Setting(key.clone()),
            Self::ProjectMeta { field, .. } => RecordKey::ProjectMeta(field.clone()),
            Self::Versification { .. } => RecordKey::Versification,
            Self::DeviceProfile { device, .. } => RecordKey::DeviceProfile(*device),
            Self::StoryBase { story, .. } => RecordKey::StoryBase(*story),
            Self::Frame { story, frame, .. } => RecordKey::Frame(*story, *frame),
            Self::StoryRef { story, .. } => RecordKey::StoryRef(*story),
            Self::CarriedFile { path, .. } => RecordKey::CarriedFile(path.clone()),
            Self::BatchRelease { batch, .. } => RecordKey::BatchRelease(*batch),
            Self::Unknown(record) => RecordKey::Unknown(UnknownKey {
                kind: record.kind.clone(),
                key_json: record.key_json.clone(),
            }),
        }
    }
}

/// A book's scope: the whole book, or a passage set (D26) as Scripture Burrito range
/// strings, at least one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Whole,
    Ranges(ScopeRanges),
}

/// One or more scope ranges, in the order given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeRanges(Vec<ScopeRange>);

impl ScopeRanges {
    pub fn new(ranges: Vec<ScopeRange>) -> Option<Self> {
        (!ranges.is_empty()).then_some(Self(ranges))
    }

    pub fn as_slice(&self) -> &[ScopeRange] {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BookPresence {
    /// `how` is part of the value, so "Restore the text" against a concurrent "Start empty"
    /// is a visible `BookPresence` conflict, never an equal-value merge (§7.6).
    Present { scope: Scope, how: PresentHow },
    /// `seen` = short ids of every head of every record of this book that the remover held,
    /// sorted, no repeats.
    Removed { seen: Vec<ShortId> },
}

/// How a book became present. `Added`: a new book (an add, an import, a book file added
/// outside the app).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentHow {
    Added,
    RestoredText,
    StartedEmpty,
}

/// USFM bytes that are not verse content, kept byte for byte (line endings included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsfmBytes(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChapterFront {
    Present(UsfmBytes),
    Absent,
}

/// A chapter and verse, as a renumber records where text came from or went to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChapterVerse {
    pub chapter: ChapterNo,
    pub verse: VerseNo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerseValue {
    /// A verse, or the first verse of a span when `last > verse`.
    Text {
        marker: MarkerWs,
        /// Everything after the marker up to `layout`: the words, inline markup, internal
        /// poetry and heading lines. Never contains a `\v` or `\c` marker (rule V3).
        words: String,
        /// The trailing run of lines that hold only paragraph-level markers and whitespace
        /// (§2.4). `words + layout` is the tC4 §8.4 verse content, byte for byte.
        layout: String,
        last: VerseNo,
        /// Set by a renumber: the verse this text had before it was moved here. The verse's
        /// anchor is `from`, else its own number (§7.7).
        from: Option<ChapterVerse>,
    },
    /// This verse is inside the span that starts at `by` (`by < verse`, same chapter).
    Covered { by: VerseNo },
    /// The verse is not in this translation. `moved_to` is set by a renumber.
    Absent { moved_to: Option<ChapterVerse> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LayoutItem {
    Chapter(ChapterNo),
    Verse(ChapterNo, VerseNo),
}

/// A resource pin: the repository path and the commit sha. `repo_path` is compared
/// case-insensitively.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRef {
    pub repo_path: Box<str>,
    pub sha: GitOid,
}

/// One source word of a tC4 §5.1 alignment group (`topWords`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceWord {
    pub word: String,
    pub strong: Option<String>,
    pub lemma: Option<String>,
    pub morph: Option<String>,
    pub occurrence: u32,
    pub occurrences: u32,
    /// Fields tC3 or tC4 wrote that this model does not name, kept as they were.
    pub extra: JsonMap,
}

/// One target word (`bottomWords` and the word bank).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetWord {
    pub word: String,
    pub occurrence: u32,
    pub occurrences: u32,
    pub extra: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignmentGroup {
    pub top_words: Vec<SourceWord>,
    pub bottom_words: Vec<TargetWord>,
    pub extra: JsonMap,
}

/// The tC4 §5.1 alignment record plus what it was made against. `invalid` is never stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignmentValue {
    /// The last verse of the span the aligner showed.
    pub span_end: VerseNo,
    pub alignments: Vec<AlignmentGroup>,
    pub word_bank: Vec<TargetWord>,
    /// md5(plain_text_v1(span content)) when aligned.
    pub target_verse_md5: Md5,
    /// tC4's label, such as "dcs::unfoldingWord/el-x-koine_ugnt@v0.34".
    pub source_version: Box<str>,
    /// The original-language pin; `None` only for imports.
    pub source: Option<ResourceRef>,
    /// D73.
    pub done: bool,
    /// tC3 or tC4 said invalid; cleared by "Mark valid".
    pub marked_invalid: bool,
    /// Dismissed from the Detached list (§7.7); kept.
    pub dismissed: bool,
    pub extra: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckValue {
    Decision(DecisionRecord),
    /// A carry-over re-attached this decision under `to` (D36).
    MovedTo {
        to: CheckKey,
    },
}

/// The tC4 §5.2 record. `invalidated`, `verseEdits`, `userInvalid` and a derived `status`
/// are not stored; the fold computes them (§7.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionRecord {
    /// The full tC3 contextId, verbatim.
    pub context: JsonMap,
    pub category: Option<Box<str>>,
    pub judgement: Judgement,
    /// The check comment; `None` projects `false`.
    pub comment: Option<String>,
    /// Projects `reminders`.
    pub bookmarked: bool,
    /// The tN / TWL pin the item list came from.
    pub resource: ResourceRef,
    /// tC3 or tC4 said invalidated; cleared by a new judgement.
    pub imported_invalid: bool,
    pub dismissed: bool,
    pub extra: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judgement {
    /// `None` projects `false`.
    pub selections: Option<Vec<Selection>>,
    pub nothing_to_select: bool,
    /// What the person chose.
    pub status: Option<UserStatus>,
    /// md5(plain_text_v1(verse)) when judged.
    pub verse_text_md5: Md5,
}

/// One selected word of a decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub text: String,
    pub occurrence: u32,
    pub occurrences: u32,
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserStatus {
    Valid,
    Invalid,
    Todo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentValue {
    pub target: CommentTarget,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommentTarget {
    /// A verse anchor (§7.7), not a label.
    Verse {
        book: BookId,
        chapter: ChapterNo,
        verse: VerseNo,
    },
    Check(CheckKey),
    Frame {
        story: StoryNo,
        frame: FrameNo,
    },
}

/// The value of one pin slot, as tC4 writes it in `resources.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotValue(pub Json);

/// Slots of one group, keyed by tC4 slot name. `ExtraScripture` is one slot holding the
/// ordered list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PinGroupValue(pub BTreeMap<SlotName, SlotValue>);

/// `vrs.json` verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersificationValue {
    pub name: Option<Box<str>>,
    pub bytes: String,
}

/// A Door43 user name, as Door43 gave it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Door43Username(pub Box<str>);

/// The tC4 actor or tC3 user a derived device stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyIdentity {
    Tc4Actor { actor: Box<str> },
    Tc3User { user: Box<str> },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeviceProfileValue {
    pub display_name: Option<Box<str>>,
    /// Only after the D98 consent notice.
    pub door43_username: Option<Door43Username>,
    pub legacy: Option<LegacyIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CarriedValue {
    /// The file's bytes are the git blob `oid` in the local object database (§11.4).
    Present { oid: GitOid, size: u64, md5: Md5 },
    /// Removed by an explicit "Remove file" action. Never written because a file went missing.
    Removed,
}

/// "Apply what was recovered" (§7.4): the changes of an incomplete batch this person saw
/// and chose to apply. Sorted, no repeats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchReleaseValue {
    pub changes: Vec<ChangeId>,
}

/// A record of a kind from a newer minor format, kept as canonical JSON: the key fields
/// (every member of the record object but `value`) and the `value` member, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownRecord {
    pub kind: Box<str>,
    pub key_json: Box<str>,
    pub value_json: Option<Box<str>>,
}
