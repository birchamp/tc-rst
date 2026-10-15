//! Word alignment for the Align workspace: linking the words of a translation to the
//! original-language words they render.
//!
//! - [`words`]: the two sides. [`SourceVerse`] holds the original-language words with
//!   their Strong's number, lemma, morphology and occurrence; [`TargetVerse`] holds the
//!   translation's plain verse text, its md5 and its words. A word is its text plus its
//!   occurrence in the verse, numbered as tC4 numbers it.
//! - [`value`]: [`AlignmentValue`], the stored alignment of one verse or span with the
//!   fields of STORAGE.md §2.3, what it was made against, and its computed validity
//!   (STORAGE.md §7.7) and Align-rail status.
//! - [`edit`]: bootstrap, place and unplace a word, move it, merge and split cards, clear,
//!   and the `done` rule (D73), with tC4's invariants.
//! - [`usfm`]: cards to and from aligned USFM through `tc-usfm`, for the aligned USFM
//!   export in en_ult's layout.
//! - [`reflow`]: carrying an alignment over an edit of its verse text, reporting every
//!   link it drops.
//! - [`record`]: tC4's §5.1 record JSON, for imports and the derived sidecar.
//! - [`frame`]: which original-language verse stands behind a project verse.
//! - [`suggest`]: links proposed from the project's own alignments with tC4's engine
//!   (wordMAP 0.6.2 and uw-wordmapbooster 1.0.5's `MorphJLBoostWordMap`).
//! - [`aligner`]: one open verse in the Align workspace: edits, Mark valid, and
//!   suggestions that stay proposals until the translator accepts them (D72, D93).
//!
//! Everything here is pure: no file or network access. A value is written by `tc-store`
//! as one `Alignment` change (STORAGE.md §5.3).

pub mod aligner;
pub mod edit;
pub mod frame;
pub mod record;
pub mod reflow;
pub mod suggest;
pub mod usfm;
pub mod value;
pub mod words;

pub use aligner::{Aligner, MarkValidRefused};
pub use edit::EditError;
pub use reflow::{DroppedLink, NoReflow, Reflowed, reflow};
pub use tc_usfm::{SourceWord, TargetWord};
pub use value::{
    AlignmentValue, Basis, CommitSha, Current, Group, ResourceRef, Stale, VerseState, VerseStatus,
    verse_status,
};
pub use words::{SourceVerse, TargetVerse, TextMd5};

/// Errors reading external data into alignment values.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("{0:?} is not an md5: 32 lowercase hexadecimal digits")]
    InvalidMd5(String),
    #[error("{0:?} is not a commit sha: 40 lowercase hexadecimal digits")]
    InvalidSha(String),
    #[error("malformed USFM: {0}")]
    Usfm(tc_usfm::Error),
    #[error("malformed alignment record: {0}")]
    Record(String),
}

/// Lowercase hexadecimal digits into `N` bytes.
pub(crate) fn parse_hex<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != 2 * N {
        return None;
    }
    let digit = |b: u8| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    };
    let mut bytes = [0u8; N];
    for (byte, pair) in bytes.iter_mut().zip(s.as_bytes().chunks(2)) {
        *byte = digit(pair[0])? << 4 | digit(pair[1])?;
    }
    Some(bytes)
}

/// `bytes` as lowercase hexadecimal digits.
pub(crate) fn write_hex(f: &mut std::fmt::Formatter<'_>, bytes: &[u8]) -> std::fmt::Result {
    bytes.iter().try_for_each(|b| write!(f, "{b:02x}"))
}
