//! The `export` service's reads (APP-DESIGN §3.9, unit B8): what the Community Checking
//! preview prints of the open book or of the open story project. The server answers them with
//! tc-export's and tc-obs' print rules (A10), the rules D2's PDF prints with, so the preview and
//! the PDF state the same verses, frames and lines the same way. The page setup is the
//! preview's own view state; the UI applies it ([`crate::model::print`]). D2 adds the export
//! jobs (`preflight`, `run`) to this service.
//!
//! Reading either writes nothing.

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::ids::{RefPart, Scope, StoryNo};

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
