//! The `editor` service's reads (APP-DESIGN §3.7, §3.9, units B3 and B4a): the open book with
//! its rail and comments, the source panes of a chapter, and the translation's text of the
//! book's chapters. The saves come with C2.

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::ids::{RecordRef, RefPart, Scope, ViewSeq, WallMs};
use crate::shown::{Direction, Known, Shown};

/// `editor/book`: the book Understand and Translate show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Book {
    pub at: Scope,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: BookId,
}

/// The open book: its chapters, the book rail, and the user comments on its verses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct BookView {
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: BookId,
    /// The chapters the book holds, in order.
    pub chapters: Vec<u16>,
    /// Every book of the project in canonical order: the open book's share drafted, counted
    /// now, and each other book's from Home's summary when one is known (tC4's
    /// `progressByProject`, #572).
    pub rail: Vec<RailBook>,
    /// The user comments on the book's verses, oldest first.
    pub comments: Vec<CommentView>,
}

/// One row of the book rail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct RailBook {
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: BookId,
    pub drafted: Known<u8>,
}

/// One user comment on a verse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct CommentView {
    pub chapter: u16,
    pub verse: u16,
    pub text: String,
    /// When it was written: the box shows the newest of a verse's comments (A-18).
    pub at: WallMs,
}

/// `editor/sources`: the source panes of one chapter of the open book.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sources {
    pub at: Scope,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: BookId,
    pub chapter: u16,
}

/// The project's source panes (its `extraScripture` pins, in order), each with the chapter or
/// the reason it has none. No pane: the project pins no source text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct SourcesView {
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub panes: Vec<SourcePane>,
    /// The original-language text of the book's testament (Translate's Greek or Hebrew chip);
    /// none when the project pins none. Its `id` is `orig`.
    pub original: Option<SourcePane>,
    /// How the project numbers the chapter's verses against the sources (tC4's `sourceRefs`).
    pub frame: SourceFrame,
}

/// The project's verse numbering against the sources', which are all in `eng` (tC4's
/// `RESOURCE_FRAME`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SourceFrame {
    /// The same numbering: the panes are the source chapter of the project chapter's number.
    Same,
    /// Another numbering (#118): the chapter's project verses in order, each with where the
    /// sources have it, and each pane's verses are the source verses under the project verse's
    /// key. One unit per verse, and no sections.
    Mapped {
        #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
        rows: Vec<FrameRow>,
    },
    /// The project's numbering could not be established: no passage is shown, never one under
    /// numbers the project does not have (`understand.frameUnavailable`).
    Unknown,
}

/// One project verse of a chapter in another numbering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct FrameRow {
    /// The project's verse key, `4` or `4-5`: the panes' verses and the comments use it.
    pub verse: RefPart,
    pub source: RowSource,
}

/// Where the sources have a project verse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RowSource {
    /// This book's chapter and verse in the sources' numbering: the unit's label.
    Verse { chapter: u16, verse: RefPart },
    /// Another book (#121), `NEH 1:1`: stated, never shown as this book's text.
    OtherBook { to: String },
    /// No place in the sources' numbering.
    Unmapped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct SourcePane {
    /// The pane's id, such as `ult`: the UI names it from its catalog (`source.ult`).
    pub id: String,
    /// The pinned version, `v91` (Translate's caption "· v91 · pinned"); none when the pin
    /// names no version.
    pub version: Option<String>,
    pub dir: Direction,
    pub text: PaneText,
}

/// A pane's chapter, or why there is none. The two absences and an error are each stated
/// (tC4's `sourceState.ts`, D30).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PaneText {
    Ready(PaneChapter),
    /// The pinned source is not on this computer.
    NotInstalled,
    /// The installed source does not hold the book.
    Missing,
    /// It could not be read.
    Error {
        reason: String,
    },
}

/// One chapter of a source text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct PaneChapter {
    /// The first verse of each translator's section (`\ts\*`), ascending: tC4's
    /// `sectionStarts`. Empty when the chapter has no verse.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub sections: Vec<u16>,
    /// The chapter's verses in order, a bridge (`4-5`) as one.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub verses: Vec<PaneVerse>,
}

/// One verse of a source pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct PaneVerse {
    /// `4`, or `4-5` for a bridge.
    pub key: RefPart,
    /// The text tC4's pane shows: its `verseText`, whitespace collapsed.
    pub text: String,
    /// The same text as tokens, words and what lies between them, as written: a card's
    /// highlight names tokens by their index here.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub tokens: Vec<String>,
    /// The poetry level of the paragraph marker that opens the verse's own text (`\v 5 \q2
    /// …`), 0 for a plain paragraph; none when its text opens with no marker.
    pub lead: Option<u8>,
    /// The poetry level of the last paragraph marker between the previous verse's text and
    /// this verse (`\q1` before `\v 5`); none when there is none.
    pub before: Option<u8>,
}

/// `editor/chapters`: the translation's text of every chapter of the open book. Translate reads
/// it once per book, so a chapter switch reads nothing of the translation (AD16).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Chapters {
    pub at: Scope,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: BookId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ChaptersView {
    /// The session view the read was made at: an editor's value and token are a pair of this
    /// view (AD37).
    pub view: ViewSeq,
    /// The book's chapters in order.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub chapters: Vec<ChapterText>,
    /// The project's verse conflicts that are open, all books: the toolbar's "N conflicts ·
    /// Review" (tC4's `verseConflicts`, #679).
    pub conflicts: u32,
}

/// One chapter of the translation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ChapterText {
    pub chapter: u16,
    /// The first verse of each of the translation's own `\ts\*` sections, ascending, by the
    /// panes' rule; empty when the translation marks none. The grid uses them when the source
    /// marks none (tC4's `chapterSections`).
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub sections: Vec<u16>,
    /// The chapter's verses in order, a span (`4-5`) as one; a verse inside a span or not in
    /// the translation has no row.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub verses: Vec<VerseRow>,
}

/// One verse of the translation as Translate shows it (tC4's `bookModel` entry).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct VerseRow {
    /// `4`, or `4-5` for a span.
    pub key: RefPart,
    /// The verse's record: the open editor matches an `Ack` and a `Changed` frame's authors to
    /// it ("Juan changed this verse while you were typing").
    pub record: RecordRef,
    /// Its words hold text: neither empty nor the stub `___` (#572).
    pub drafted: bool,
    /// The text the grid shows, whitespace collapsed; empty when not drafted.
    pub text: String,
    /// As [`PaneVerse::lead`]: the poetry level of a paragraph marker that opens the verse's
    /// own text.
    pub lead: Option<u8>,
    /// As [`PaneVerse::before`]: the poetry level of the last paragraph marker after the
    /// previous verse's text.
    pub before: Option<u8>,
    /// What the verse box edits: the verse's whole `words`, every poem line with its markers
    /// (#786, Q-20), with the token of the version shown and its conflict marker.
    pub words: Shown<String>,
}
