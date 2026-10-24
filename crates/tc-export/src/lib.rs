//! The export kernel's pure parts. Today this is the print model: what the Community
//! Checking preview and the PDF show of a Bible book or of an Open Bible Stories project, as
//! typed values, by tC4's rules at `d310bf3`. It writes no HTML; the producers (aligned and
//! plain USFM, OBS Markdown, the print HTML for the PDF, the Burrito zip) come later and
//! read this model (APP-DESIGN §10, units A10 and D2).
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
pub mod page;
pub mod stories;
mod text;

pub use text::{PrintText, Run};

/// The direction a print is set in. tC4 sets it from the project's `scriptDirection`:
/// `rtl` is right to left and every other value left to right (`src/data/export/pdf.ts:125`,
/// `src/views/CommunityChecking.jsx:99,181`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}
