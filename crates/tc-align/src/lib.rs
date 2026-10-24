//! Word alignment for the Align workspace: linking the words of a translation to the
//! original-language words they render.
//!
//! - [`words`]: the two sides. [`SourceVerse`] holds the original-language words with
//!   their Strong's number, lemma, morphology and occurrence, read from a book as tC4
//!   cuts a verse; [`TargetVerse`] holds the translation's verse as the aligner shows it,
//!   its words, and the md5 an alignment records of it (STORAGE.md §7.7). A word is its text plus its occurrence in the verse, numbered as tC4
//!   numbers it.
//! - [`value`]: the stored alignment of one verse or span is the journal's
//!   [`AlignmentValue`] (STORAGE.md §2.3); this module holds what it is made against
//!   ([`Basis`]), the rules every value keeps (each target word in one place), and the
//!   Align-rail status, which reads the journal's validity (STORAGE.md §7.7).
//! - [`edit`]: bootstrap, place and unplace words, move them, merge and split cards, clear,
//!   the bank and each card in verse order, and the `done` rule (D73), with tC4's
//!   invariants: the [`Alignment`] trait on the journal's value.
//! - [`selection`]: which bank words a click, Ctrl or Cmd click, Shift click or drag
//!   selects, for placing several on one card in one edit.
//! - [`usfm`]: cards to aligned USFM through `tc-usfm`, for the aligned USFM export in
//!   en_ult's layout. Reading aligned USFM is `tc_journal::alignment::split_aligned`.
//! - [`reflow`]: carrying an alignment over an edit of its verse text, each link following
//!   its own word, reporting every link it drops.
//! - tC3's and tC4's §5.1 record JSON is read by `tc_journal::fold::decode_alignment_record`
//!   (`Writer::Tc4`), the reader of the journal's own sidecar.
//! - [`frame`]: which original-language verse stands behind a project verse.
//! - [`suggest`]: links proposed from the project's own alignments with tC4's engine
//!   (wordMAP 0.6.2 and uw-wordmapbooster 1.0.5's `MorphJLBoostWordMap`).
//! - [`aligner`]: one open verse in the Align workspace: edits, Mark valid, and
//!   suggestions that stay proposals until the translator accepts them (D72, D93), and
//!   the selected bank words. A verse whose text holds alignment markup, or with no
//!   original-language words or no target words, does not open.
//!
//! Everything here is pure: no file or network access. A value is written by `tc-store`
//! as one `Alignment` change (STORAGE.md §5.3). The value, its validity and its verse
//! order are `tc-journal`'s; this crate holds no second copy of them (STORAGE.md §17).

pub mod aligner;
pub mod edit;
pub mod frame;
pub mod reflow;
pub mod selection;
pub mod suggest;
pub mod usfm;
pub mod value;
pub mod words;

pub use aligner::{Aligner, MarkValidRefused, Unavailable};
pub use edit::{Alignment, EditError, bootstrap};
pub use reflow::{DroppedLink, NoReflow, Reflowed, reflow, reflow_edit};
pub use selection::Click;
pub use tc_core::{GitOid, Md5};
pub use tc_journal::alignment::{AlignedRun, Stale};
pub use tc_journal::record::{AlignmentGroup, AlignmentValue, ResourceRef, SourceWord, TargetWord};
pub use value::{
    Basis, Malformed, VerseState, VerseStatus, holds_the_words_of, needs_review, verse_status,
};
pub use words::{Occurrence, SourceBook, SourceVerse, TargetVerse, WordId};

/// Errors reading external data into alignment values.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Digest(#[from] tc_core::DigestError),
    #[error("malformed USFM: {0}")]
    Usfm(tc_usfm::Error),
    #[error("malformed alignment record: {0}")]
    Record(String),
    #[error("malformed alignment record: {0}")]
    Malformed(#[from] value::Malformed),
}
