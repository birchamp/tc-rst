//! The export kernel's pure parts: the print model of a Bible book or of an Open Bible
//! Stories project, by tC4's rules at `d310bf3`, and the producers [`produce`] runs (aligned
//! and plain USFM, OBS Markdown, the print HTML for the PDF). Data in, bytes out: the app
//! reads the project, checkpoints, and writes what a producer returns into the downloads
//! folder (APP-DESIGN §10, units A10 and D2).
//!
//! - [`usfm`]: aligned and plain USFM of one book. A verse's alignment is woven only when
//!   `tc_align::needs_review` says the record still vouches for it; any other verse with a
//!   record keeps its stored bytes and is named in [`ExportFacts`] (tC4 e2bfd8c3, #796).
//! - [`markdown`]: an OBS project's `content/` files, for the app to zip.
//! - [`html`]: the print document the window prints to PDF.
//!
//! - The page setup and the print parts are tc-api's (`tc_api::model::print`), so the preview
//!   and the PDF print from one model; [`page`] adds what only the PDF reads (margins, the
//!   stylesheet's variables, the paper's CSS name).
//! - [`book`] ports `printedItems` (`src/data/bookModel.js`): which chapters a book prints and
//!   the line that stands for each run of undrafted chapters, written in tc-api's model by
//!   [`book::print_items`].
//! - [`stories`] ports `src/views/print/PrintStories.jsx` over tc-obs' print model
//!   ([`tc_obs::print`]): each printed story's title, frames, pictures and reference line.
//! - [`PrintText`] is each line the interface language words, as tC4's catalog key and its
//!   values, so the caller words it in the language it shows.
//!
//! The book model a print reads is the editor's: each verse's label, whether it is drafted,
//! and its shown text ([`book::BookVerse`]). tC4 builds the editor and the PDF from one
//! `bookModel` so that the preview and the PDF state the same verses the same way; the
//! rebuild keeps that by taking the rows the editor's view is built from, never a second
//! reading of the USFM.

pub mod book;
pub mod html;
pub mod markdown;
pub mod page;
pub mod stories;
mod text;
pub mod usfm;

pub use html::{PrintBookInput, PrintStoriesInput};
pub use text::{PrintText, Run};
pub use usfm::{BookInput, VerseAlignment};

use tc_core::Reference;

/// An export's producer with what it reads (tC4 `src/data/export/producers.ts`; the Burrito
/// zip is the app's, since it reads the project folder). USFM reads a book, Story Markdown an
/// OBS project's files, the print HTML a book's or a story project's print.
pub enum ExportInput<'a> {
    AlignedUsfm(&'a BookInput),
    PlainUsfm(&'a BookInput),
    /// The project's files as `(path from the project folder, bytes)`.
    StoryMarkdown(&'a [(String, Vec<u8>)]),
    PrintBook(PrintBookInput<'a>),
    PrintStories(PrintStoriesInput<'a>),
}

/// What a producer made: one file, or the files of a folder as `(path, bytes)` for the app to
/// zip (Story Markdown), since this crate does no I/O and holds no archive writer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Produced {
    File(Vec<u8>),
    Files(Vec<(String, Vec<u8>)>),
}

/// Why a verse with an alignment record was exported as stored, without its record woven.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkipReason {
    /// The record no longer vouches for the verse (`tc_align::needs_review`).
    NeedsReview,
    /// The verse holds the `\zaln` markup of an aligned file it was added from.
    InlineAlignment,
    /// The weave refused the verse, such as one with glossary `\w` words.
    WeaveRefused,
    /// A plain export could not take the verse's alignment markup out, so the verse keeps
    /// its stored bytes, markup and all.
    MarkupKept,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SkippedVerse {
    pub reference: Reference,
    pub why: SkipReason,
}

/// What the export says beside its file: the verses whose alignment it left out.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExportFacts {
    /// The first [`ExportFacts::NAMED`] of them, in book order.
    pub skipped: Vec<SkippedVerse>,
    /// All of them.
    pub skipped_total: u32,
}

impl ExportFacts {
    /// tC4 names at most 20 verses and counts the rest (50cfec3e), so a book left unaligned
    /// after an edit still gives a message a person can read.
    pub const NAMED: usize = 20;

    pub(crate) fn skip(&mut self, verse: SkippedVerse) {
        if self.skipped.len() < Self::NAMED {
            self.skipped.push(verse);
        }
        self.skipped_total = self.skipped_total.saturating_add(1);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// tC4's `export.nothing-drafted`: a book with no drafted verse prints nothing.
    #[error("the book has no drafted verse, so the print has nothing to show")]
    NothingDrafted,
}

/// Run the producer `input` names.
pub fn produce(input: &ExportInput<'_>) -> Result<(Produced, ExportFacts), ExportError> {
    match input {
        ExportInput::AlignedUsfm(book) => {
            let (usfm, facts) = usfm::aligned(book);
            Ok((Produced::File(usfm.into_bytes()), facts))
        }
        ExportInput::PlainUsfm(book) => {
            let (usfm, facts) = usfm::plain(book);
            Ok((Produced::File(usfm.into_bytes()), facts))
        }
        ExportInput::StoryMarkdown(files) => Ok((
            Produced::Files(markdown::content_files(files)),
            ExportFacts::default(),
        )),
        ExportInput::PrintBook(print) => Ok((
            Produced::File(html::book_document(print)?.into_bytes()),
            ExportFacts::default(),
        )),
        ExportInput::PrintStories(print) => Ok((
            Produced::File(html::stories_document(print).into_bytes()),
            ExportFacts::default(),
        )),
    }
}

/// The direction a print is set in. tC4 sets it from the project's `scriptDirection`:
/// `rtl` is right to left and every other value left to right (`src/data/export/pdf.ts:125`,
/// `src/views/CommunityChecking.jsx:99,181`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}
