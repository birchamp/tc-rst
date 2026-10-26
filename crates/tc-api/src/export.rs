//! The `export` service's reads (APP-DESIGN §3.9, unit B8): what the Community Checking
//! preview prints of the open book or of the open story project. The server answers them with
//! tc-export's and tc-obs' print rules (A10), the rules D2's PDF prints with, so the preview and
//! the PDF state the same verses, frames and lines the same way. The page setup is the
//! preview's own view state; the UI applies it ([`crate::model::print`]).
//!
//! D2's exports: [`Preflight`] says what is in conflict before an export, and [`RunExport`] starts
//! the export's job, which checkpoints first and ends with the file to download
//! ([`crate::jobs::JobOutcome::ExportReady`]).
//!
//! Reading writes nothing, and no export writes into the project folder.

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::ids::{DownloadId, RefPart, RequestId, Scope, StoryNo};
use crate::model::print::PageSetup;

/// `export/book`: the printed items of a book of the open project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Book {
    pub at: Scope,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: BookId,
}

/// A book as its print shows it (`printedItems`, `tC4:src/data/bookModel.js:52-64`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct BookPrint {
    /// In chapter order: each chapter with a verse that has text to print, and one line for
    /// each run of chapters with none between two such chapters. Chapters before the first or
    /// after the last are left out. Empty when no verse has text to print.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub items: Vec<PrintItem>,
    /// Some verse of the book prints as not yet drafted: the preview's "Incomplete draft"
    /// notice (tc-export's `has_undrafted`).
    pub undrafted: bool,
}

/// A printed chapter, or the line for a run of chapters with nothing to print.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum PrintItem {
    Chapter(PrintChapter),
    Gap(Run),
}

/// A printed chapter: every verse of it, in book order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct PrintChapter {
    pub chapter: u16,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub verses: Vec<PrintVerse>,
}

/// A verse of a printed chapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct PrintVerse {
    /// `4`, or `4-5` for a span.
    pub label: RefPart,
    /// The text the verse shows (tc-usfm's `shown_text_v1`), never empty; none when the verse
    /// prints as not yet drafted, as a verse drafted with a footnote alone does.
    pub text: Option<String>,
}

/// A run of numbered parts, `from` to `to`: one part when they are equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub from: u16,
    pub to: u16,
}

/// `export/stories`: the printed stories of the open story project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Stories {
    pub at: Scope,
}

/// A story project as its print shows it (`readPrintedStories`,
/// `tC4:src/data/storyModel.ts:65-83`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct StoriesPrint {
    /// In story order: each drafted story, and one line for each run of undrafted stories
    /// between two drafted ones. Empty when no story is drafted.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub items: Vec<PrintStoryItem>,
    /// Some frame of the project has no text.
    pub undrafted: bool,
}

/// A printed story, or the line for a run of undrafted stories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum PrintStoryItem {
    Story(PrintStory),
    Gap(StoryRun),
}

/// A run of undrafted stories, `from` to `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct StoryRun {
    pub from: StoryNo,
    pub to: StoryNo,
}

/// A drafted story.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct PrintStory {
    pub story: StoryNo,
    /// The story's own title; empty when it has none, and the print says "Story 4".
    pub title: String,
    /// Each drafted frame, and one line for each run of undrafted frames.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub frames: Vec<PrintFrame>,
    /// The reference line, when the story has one with text.
    pub reference: Option<String>,
}

/// A drafted frame, or the line for a run of undrafted frames (1 is the first frame).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum PrintFrame {
    Drafted(DraftedFrame),
    Gap(Run),
}

/// A drafted frame with its text and, when a pinned picture pack has it, its picture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct DraftedFrame {
    pub frame: u16,
    pub text: String,
    /// The picture's address under `/media/obs/`.
    pub picture: Option<String>,
}

/// What an export reads: a book of a Bible project, or the stories of a story project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Subject {
    Book {
        #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
        book: BookId,
    },
    Stories,
}

/// `export/preflight`: the open conflicts of what an export would read, so the menu can say
/// "3 verses have two versions" before it runs (STORAGE §8.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Preflight {
    pub at: Scope,
    pub subject: Subject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct PreflightView {
    /// The open conflicts of the subject's records, of every kind.
    pub conflicts: u32,
    /// The verses (for stories, the frames, titles and reference lines) with two versions:
    /// the export uses the version shown.
    pub verses: u32,
}

/// What an export makes (tC4 `src/data/export/producers.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Producer {
    AlignedUsfm,
    PlainUsfm,
    StoryMarkdown,
    /// The print document `GET /print/{job}` serves for the PDF.
    PrintHtml,
}

/// `export/run` (job `Export`): checkpoints, makes the file and registers it for download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunExport {
    pub request: RequestId,
    pub at: Scope,
    pub producer: Producer,
    pub subject: Subject,
    /// What only the UI holds for a print: required for [`Producer::PrintHtml`], and refused
    /// with any other producer.
    pub print: Option<PrintRequest>,
    /// The page's offset from UTC, so the file is dated by the translator's own day, as tC4
    /// dates it in its window.
    pub utc_offset: UtcOffset,
}

/// Minutes east of UTC (JavaScript's `-getTimezoneOffset()`), at most 14 hours either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "i16", into = "i16")]
pub struct UtcOffset(i16);

impl UtcOffset {
    pub const UTC: Self = Self(0);

    pub fn new(minutes: i16) -> Option<Self> {
        (-14 * 60..=14 * 60)
            .contains(&minutes)
            .then_some(Self(minutes))
    }

    pub fn minutes(self) -> i16 {
        self.0
    }
}

impl TryFrom<i16> for UtcOffset {
    type Error = String;

    fn try_from(minutes: i16) -> Result<Self, Self::Error> {
        Self::new(minutes).ok_or_else(|| format!("{minutes} minutes is no offset from UTC"))
    }
}

impl From<UtcOffset> for i16 {
    fn from(offset: UtcOffset) -> Self {
        offset.0
    }
}

#[cfg(feature = "proptest")]
impl proptest::arbitrary::Arbitrary for UtcOffset {
    type Parameters = ();
    type Strategy = proptest::strategy::BoxedStrategy<Self>;

    fn arbitrary_with((): ()) -> Self::Strategy {
        use proptest::prelude::*;
        (-14 * 60..=14 * 60i16).prop_map(Self).boxed()
    }
}

/// The print's choices and words as the person sees them: the preview's page setup, the
/// title in the interface language (`bookName`, or the project's name), and the interface
/// language's catalog messages for each line the print words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrintRequest {
    pub setup: PageSetup,
    pub title: String,
    pub words: PrintWords,
}

/// The catalog messages of a print's lines, each with its `{n}` or `{from}` and `{to}` still
/// in it: the server fills them with [`crate::model::fill`], as the UI's `t` does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrintWords {
    /// `cc.chapterHeading`: "Chapter {n}".
    pub chapter_heading: String,
    /// `cc.chapterNotDrafted`.
    pub chapter_not_drafted: String,
    /// `cc.chaptersNotDrafted`.
    pub chapters_not_drafted: String,
    /// `cc.notYetDrafted`.
    pub not_yet_drafted: String,
    /// `cc.frameNotDrafted`.
    pub frame_not_drafted: String,
    /// `cc.framesNotDrafted`.
    pub frames_not_drafted: String,
    /// `cc.storyNotDrafted`.
    pub story_not_drafted: String,
    /// `cc.storiesNotDrafted`.
    pub stories_not_drafted: String,
    /// `storyDraft.storyNumber`.
    pub story_number: String,
    /// `storyDraft.imageAlt`.
    pub image_alt: String,
    /// `cc.nothingDraftedObs`.
    pub nothing_drafted_obs: String,
}

impl PrintWords {
    /// The catalog keys, in field order: what the UI looks up to fill this.
    pub const KEYS: [&'static str; 11] = [
        "cc.chapterHeading",
        "cc.chapterNotDrafted",
        "cc.chaptersNotDrafted",
        "cc.notYetDrafted",
        "cc.frameNotDrafted",
        "cc.framesNotDrafted",
        "cc.storyNotDrafted",
        "cc.storiesNotDrafted",
        "storyDraft.storyNumber",
        "storyDraft.imageAlt",
        "cc.nothingDraftedObs",
    ];

    /// The message of catalog key `key`, or `None` for a key a print does not word.
    pub fn message(&self, key: &str) -> Option<&str> {
        Some(match key {
            "cc.chapterHeading" => &self.chapter_heading,
            "cc.chapterNotDrafted" => &self.chapter_not_drafted,
            "cc.chaptersNotDrafted" => &self.chapters_not_drafted,
            "cc.notYetDrafted" => &self.not_yet_drafted,
            "cc.frameNotDrafted" => &self.frame_not_drafted,
            "cc.framesNotDrafted" => &self.frames_not_drafted,
            "cc.storyNotDrafted" => &self.story_not_drafted,
            "cc.storiesNotDrafted" => &self.stories_not_drafted,
            "storyDraft.storyNumber" => &self.story_number,
            "storyDraft.imageAlt" => &self.image_alt,
            "cc.nothingDraftedObs" => &self.nothing_drafted_obs,
            _ => return None,
        })
    }
}

/// A finished export: its file and the verses whose alignment it left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct Exported {
    pub file: ExportedFile,
    pub facts: ExportFacts,
}

/// Where an export's file is. Each name is `<subject>-<YYYY-MM-DD>.<ext>`, as tC4's
/// `exportFilename` names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ExportedFile {
    /// `GET /download/{download}/{file_name}`.
    Download {
        download: DownloadId,
        file_name: String,
    },
    /// The print document at `GET /print/{job}`, this job's id, which the shell prints to
    /// the PDF `file_name`.
    Print { file_name: String },
}

/// The verses an aligned export kept as stored, without their alignment (tC4 e2bfd8c3, #796).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ExportFacts {
    /// At most 20 of them, in book order.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub skipped: Vec<SkippedVerse>,
    /// All of them.
    pub skipped_total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct SkippedVerse {
    pub chapter: u16,
    /// `4`, or `4-5` for a span.
    pub verse: RefPart,
    pub why: SkipReason,
}

/// Why an export kept a verse as stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum SkipReason {
    /// The alignment no longer matches the verse's words.
    NeedsReview,
    /// The verse holds the alignment markup of the file it was added from.
    InlineAlignment,
    /// The verse holds markup the weave cannot place words in.
    WeaveRefused,
    /// A plain export kept the verse as stored, alignment markup and all, because the markup
    /// would not come out.
    MarkupKept,
}
