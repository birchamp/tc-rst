//! The conflict review (unit B11; STORAGE §8.5; tC4 `ConflictReview.jsx`, #679): one read of
//! a tab's entries with the counts of every tab, the History of one record, and the
//! resolution that keeps one version.
//!
//! tC4's review has one tab, the verses whose text two people changed, and holds them whole
//! (`s.verseConflicts`), so the Text tab is one read of every entry with its versions. The
//! other tabs list what the rest of the project holds two ways, the records left without a
//! place and the notices; their choices are unit C7b's.

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::check::{CheckPlace, Tool};
use crate::ids::{ChangeRef, ConflictRef, RecordRef, RequestId, Scope, StoryNo, ViewSeq, WallMs};
use crate::shown::{PersonLabel, Who};
use crate::token::{ReviewToken, ShownToken};
use crate::write::Ack;

/// The review's tabs, by kind (STORAGE §8.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum ReviewTab {
    /// The verses, chapter openings, book headings and story frames: tC4's review.
    Text,
    Alignments,
    /// Check decisions and comments.
    Checks,
    /// Pins, settings, project details, the books and the files the app carries.
    Settings,
    /// Records with no place on any screen (STORAGE §7.7).
    Detached,
    Notices,
}

impl ReviewTab {
    pub const ALL: [ReviewTab; 6] = [
        Self::Text,
        Self::Alignments,
        Self::Checks,
        Self::Settings,
        Self::Detached,
        Self::Notices,
    ];
}

/// `conflicts/list`: the tab's entries and how many every tab holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct List {
    pub at: Scope,
    pub tab: ReviewTab,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ReviewView {
    pub view: ViewSeq,
    /// Every tab, in [`ReviewTab::ALL`]'s order, with its count.
    pub tabs: Vec<TabCount>,
    pub entries: ReviewEntries,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct TabCount {
    pub tab: ReviewTab,
    pub count: u32,
}

/// What the asked tab lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ReviewEntries {
    Conflicts(Vec<ConflictEntry>),
    Detached(Vec<DetachedEntry>),
    Notices(Vec<NoticeEntry>),
}

/// One conflict: where it is, what kind, and every version, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ConflictEntry {
    pub conflict: ConflictRef,
    pub kind: ConflictKind,
    /// The record whose History the review shows.
    pub record: RecordRef,
    pub place: ReviewPlace,
    /// Conflicts made by one set of batches share a group, numbered from 0 in list order
    /// (STORAGE §8.1): one upgrade against another is one group.
    pub group: u32,
    pub versions: Vec<Version>,
    /// One per record of the conflict: what a resolution sends back.
    pub review: Vec<ReviewToken>,
    /// A version names an earlier change this computer does not have (STORAGE §7.4).
    pub incomplete_history: bool,
    /// Versions this computer could not read; they are kept, and no version can be kept
    /// over them until they can be read.
    pub unreadable: u32,
}

/// tc-journal's conflict kinds (STORAGE §8.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum ConflictKind {
    VerseText,
    VerseStructure,
    Passage,
    StructureIssue,
    BookHeader,
    ChapterFront,
    BookPresence,
    BookRemovedWhileEdited,
    LastBookRemoved,
    BookLayout,
    Alignment,
    Check,
    CheckMoved,
    Comment,
    Pins,
    Setting,
    ProjectMeta,
    Versification,
    StoryBase,
    Frame,
    StoryRef,
    CarriedFile,
    PathCase,
    DeviceProfile,
    /// A kind from a newer translationCore.
    Unknown,
}

/// Where a conflict or a detached record is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ReviewPlace {
    Verse {
        #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
        book: BookId,
        chapter: u16,
        verse: u16,
    },
    ChapterFront {
        #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
        book: BookId,
        chapter: u16,
    },
    Book {
        #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
        book: BookId,
    },
    /// A check of a tool at its place; `None` for a tool no list derives.
    Check {
        tool: Option<Tool>,
        place: CheckPlace,
    },
    Story {
        story: StoryNo,
        /// `None` for the story's base text or its reference line.
        frame: Option<u16>,
    },
    /// A record of the whole project: a pin group, a setting, the project's details, a file.
    Project,
}

/// One version of a record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub change: ChangeRef,
    pub by: MadeBy,
    pub at: WallMs,
    /// The version is dated more than a day after this computer's clock: "the clock of the
    /// computer it was made on may be wrong" (STORAGE §8.5, `ClockAhead`).
    pub clock_ahead: bool,
    /// The version the screens show until one is kept.
    pub shown: bool,
    pub value: VersionValue,
}

/// Who made a version: a person, by the one label of the review and the sync result
/// (#680), or, for a change no person typed in this app, its source (STORAGE §8.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MadeBy {
    Person(PersonLabel),
    /// An edit on Door43's website.
    Door43,
    /// A file of work a teammate saved for others.
    ExchangeFile,
    /// An import of a tC4 or tC3 project or a USFM file.
    Import,
}

/// What a version holds, as far as the review shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum VersionValue {
    /// Text as the screens show it: a verse, a chapter's opening, a frame.
    Text(String),
    /// The verse is joined into verse `by`.
    Joined { by: u16 },
    /// The record holds nothing: an absent verse, a removed book.
    Absent,
    /// A value the review does not draw (an alignment, a decision, a setting).
    NotShown,
}

/// A record left with no place on any screen (STORAGE §7.7): a decision whose check left
/// the resource, an alignment whose verse is gone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct DetachedEntry {
    pub record: RecordRef,
    pub what: DetachedKind,
    pub place: ReviewPlace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum DetachedKind {
    Decision,
    Alignment,
    Comment,
    Other,
}

/// One thing the files of the project or this computer showed when the project opened, or
/// that the journal could not take as it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct NoticeEntry {
    pub notice: NoticeView,
}

/// The notices, in the words' terms (STORAGE §2.6, §8.5: one sentence each, never a promise
/// that a person or device repairs it). A file is named by its path inside the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NoticeView {
    /// A file of saved changes was damaged; `recovered` of its changes were read.
    DamagedChanges { recovered: u32 },
    /// Changes made by a newer translationCore are kept, not read.
    NewerApp,
    /// Changes from a computer whose clock is ahead of this one's by `days`.
    ClockAhead { by: Option<PersonLabel>, days: u32 },
    /// Another program changed `file`; its edit was recorded as `changes` changes.
    OutsideEditRecorded { file: String, changes: u32 },
    /// Another program changed `file`; the change is kept aside and nothing is written over
    /// it until it is recorded or the file is restored.
    OutsideEditHeld { file: String },
    /// Another program changed `file` in a way that could not be read; a copy is kept.
    OutsideEditUnreadable { file: String },
    /// The project's details were edited outside the app; the bytes found are kept.
    DetailsEditedOutside,
    /// Another program holds `file` open, so the app cannot update it. The work is saved.
    WriteBlocked { file: String },
    /// `file` was missing or damaged and was written again from what the project holds.
    FileRepaired { file: String },
    /// `file` is missing and could not be restored; nothing made from it is removed.
    FileMissing { file: String },
    /// `file` could not be read; it is tried again later.
    FileUnreadable { file: String },
    /// The disk changed `file` though no program wrote it; a copy is kept and the project's
    /// own text is used.
    StorageDamage { file: String },
    /// `file` moved to `to`, a name a project can hold.
    FileMoved { file: String, to: String },
    /// The project folder was copied or restored from an older copy; work from here on is
    /// saved as this computer's new work.
    FolderCopied,
    /// Another installation saves into this folder.
    SecondWriter,
    /// A save reached the disk, which then reported an error.
    SavedWithDiskError,
    /// An import did not finish.
    ImportUnfinished,
    /// A batch of a teammate's changes arrived incomplete: `have` of `of`.
    IncompleteChanges { have: u32, of: u32 },
    /// Changes the journal could not place are kept apart.
    KeptApart { changes: u32 },
}

/// `conflicts/history`: every version of one record, newest first (#78), with the token a
/// *Restore this version* ([`Restore`]) sends back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct History {
    pub at: Scope,
    pub record: RecordRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct HistoryView {
    /// The record as the read showed it: what a [`Restore`] writes over.
    pub shown: ShownToken,
    pub versions: Vec<HistoryVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct HistoryVersion {
    pub version: Version,
    pub cause: VersionCause,
}

/// How a version came to be (STORAGE §8.3, History).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum VersionCause {
    Edited,
    /// Kept in a conflict.
    Kept,
    Restored,
    /// Carried over to a new release of a resource.
    CarriedOver,
    EditedOnDoor43,
    EditedInAnotherProgram,
    FromAFile,
    Imported,
    Created,
    StartedOver,
    /// translationCore's own change (a pin it filled in).
    ByTheApp,
    /// Made by a newer translationCore.
    Newer,
}

/// A choice on a conflict of the Text tab (STORAGE §8.3), as the review offers it. Every
/// choice writes one change per record of the conflict, so a passage's choice carries one
/// review token per record of the passage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum KeepChoice {
    /// *Keep this version*: the candidate, and on a grouped conflict its side.
    Candidate { change: ChangeRef },
    /// *Keep Maria's passage*: the first pick names the side; the others pick one version
    /// where a record of the passage holds several, so no clock picks what is kept.
    Passage { picks: Vec<ChangeRef> },
    /// *Keep 4 and 5 separate*, with a version picked where a verse holds several.
    Separate { picks: Vec<ChangeRef> },
    /// *Keep the joined verses*, with the span picked where there are several.
    Joined { picks: Vec<ChangeRef> },
}

/// `conflicts/resolve`: one choice on a conflict (C7a; B11 on scripted outcomes). The tokens
/// are the entry's `review`, one per record of the conflict; a resolution over versions that
/// changed since answers `NotSaved::LookAgain`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Resolve {
    pub at: Scope,
    pub request: RequestId,
    pub conflict: ConflictRef,
    pub review: Vec<ReviewToken>,
    pub choice: KeepChoice,
}

/// `conflicts/restore`: History's *Restore this version* (STORAGE §5.9): the value of `from`
/// written over the heads `shown` lists, which [`HistoryView`] gave with the record. On a
/// record in conflict the restored text becomes the version shown and the other versions stay
/// listed for a choice; nothing is written over a version the screen did not show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Restore {
    pub at: Scope,
    pub request: RequestId,
    pub record: RecordRef,
    pub shown: ShownToken,
    pub from: ChangeRef,
}

/// `conflicts/resolveBulk`: *Keep all of Maria's* (STORAGE §5.9 step 3): one batch that keeps
/// `person`'s version in each listed conflict where their computers hold exactly one of its
/// versions. A conflict whose versions changed since, that holds two versions from the
/// person's computers, or none, is left for a choice of its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveBulk {
    pub at: Scope,
    pub request: RequestId,
    pub person: Who,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub conflicts: Vec<BulkConflict>,
}

/// One conflict of a bulk keep, with the entry's tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BulkConflict {
    pub conflict: ConflictRef,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub review: Vec<ReviewToken>,
}

/// What a bulk keep did: the write, and the conflicts it left for a choice of their own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct BulkKept {
    pub ack: Ack,
    pub left: Vec<ConflictRef>,
}
