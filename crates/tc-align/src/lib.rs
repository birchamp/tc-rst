//! Word alignment for the Align workspace: linking the words of a translation to the
//! original-language words they render.
//!
//! - [`words`]: the two sides. [`SourceVerse`] holds the original-language words with
//!   their Strong's number, lemma, morphology and occurrence, read from a book as tC4
//!   cuts a verse; [`TargetVerse`] holds the translation's plain verse text, its md5 and
//!   its words. A word is its text plus its occurrence in the verse, numbered as tC4
//!   numbers it.
//! - [`value`]: [`AlignmentValue`], the stored alignment of one verse or span with the
//!   fields of STORAGE.md §2.3, what it was made against, the rules every value keeps
//!   (each target word in one place), and its computed validity (STORAGE.md §7.7) and
//!   Align-rail status.
//! - [`edit`]: bootstrap, place and unplace words, move them, merge and split cards, clear,
//!   the bank and each card in verse order, and the `done` rule (D73), with tC4's
//!   invariants.
//! - [`selection`]: which bank words a click, Ctrl or Cmd click, Shift click or drag
//!   selects, for placing several on one card in one edit.
//! - [`usfm`]: cards to and from aligned USFM through `tc-usfm`, for the aligned USFM
//!   export in en_ult's layout.
//! - [`reflow`]: carrying an alignment over an edit of its verse text, reporting every
//!   link it drops.
//! - [`record`]: tC4's §5.1 record JSON, for imports and the derived sidecar.
//! - [`frame`]: which original-language verse stands behind a project verse.
//! - [`suggest`]: links proposed from the project's own alignments with tC4's engine
//!   (wordMAP 0.6.2 and uw-wordmapbooster 1.0.5's `MorphJLBoostWordMap`).
//! - [`aligner`]: one open verse in the Align workspace: edits, Mark valid, and
//!   suggestions that stay proposals until the translator accepts them (D72, D93), and
//!   the selected bank words. A verse with no original-language words or no target words
//!   does not open.
//!
//! Everything here is pure: no file or network access. A value is written by `tc-store`
//! as one `Alignment` change (STORAGE.md §5.3).

pub mod aligner;
pub mod edit;
pub mod frame;
pub mod record;
pub mod reflow;
pub mod selection;
pub mod suggest;
pub mod usfm;
pub mod value;
pub mod words;

pub use aligner::{Aligner, MarkValidRefused, Unavailable};
pub use edit::EditError;
pub use reflow::{DroppedLink, NoReflow, Reflowed, reflow};
pub use selection::Click;
pub use tc_core::{GitOid, Md5};
pub use tc_usfm::{SourceWord, TargetWord};
pub use value::{
    AlignmentValue, Basis, Current, Group, Malformed, ResourceRef, Stale, VerseState, VerseStatus,
    verse_status,
};
pub use words::{Occurrence, SourceVerse, TargetVerse, WordId};

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
