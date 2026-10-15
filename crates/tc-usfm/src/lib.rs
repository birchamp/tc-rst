//! Lossless USFM for the tC4 rebuild.
//!
//! - [`Document`] parses any text into a typed tree that writes back to the exact input
//!   bytes: byte order mark, CRLF, odd whitespace, unknown markers, attributes, notes,
//!   figures and milestones included. Parsing never fails.
//! - [`Book`] cuts a book into a header, chapter segments and verse units keyed by chapter
//!   and [`tc_core::VerseSpan`], and replaces units (by key, or by index for units
//!   without a unique key) while keeping every other byte. The rule for which unit owns a
//!   leading paragraph marker, heading or sidebar is in [`book`].
//! - [`plain_text`] gives the readable text of a verse for display and hashing, with the
//!   normalization documented in [`text`].
//! - [`words`] splits text into words exactly as tC4 does, with occurrence numbers.
//! - [`align`] reads, strips and weaves `\zaln` word alignment the way en_ult and tC4's
//!   export write it.
//! - [`codec`] cuts a book into the journal's records (header, chapter fronts, verse words
//!   and layout) and writes it back byte for byte.
//! - [`derivations`] holds the versioned derivations that decide computed validity:
//!   [`plain_text_v1`], [`tokenize_v1`] and [`selections_v1`], tC4's behaviour byte for byte.

pub mod align;
mod attributes;
pub mod book;
pub mod codec;
pub mod derivations;
mod doc;
mod lex;
pub mod markers;
pub mod text;
pub mod words;

pub use align::{
    AlignedVerse, AlignmentGroup, SourceWord, TargetWord, parse_aligned, strip_alignment,
    target_words, weave,
};
pub use attributes::{Attribute, Attributes};
pub use book::{Book, ChapterMark, Segment, SegmentKind, VerseUnit};
pub use derivations::{SelectionCount, plain_text_v1, selections_v1, tokenize_v1};
pub use doc::{Document, Marker, Milestone, Node, Span};
pub use text::plain_text;

use tc_core::VerseSpan;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("the book has no verse {chapter}:{verses}")]
    VerseNotFound { chapter: u16, verses: VerseSpan },
    #[error("the book has more than one verse {chapter}:{verses}")]
    AmbiguousVerse { chapter: u16, verses: VerseSpan },
    #[error(
        "the new text for {chapter}:{verses} would add, remove or move a verse, chapter or leading marker"
    )]
    StructureChanged { chapter: u16, verses: VerseSpan },
    #[error("segment {segment} is not a verse unit of the book")]
    NotAVerseUnit { segment: usize },
    #[error("segment {segment} is edited twice")]
    RepeatedEdit { segment: usize },
    #[error(
        "the edit would add, remove or move a verse, chapter or leading marker at segment {segment}"
    )]
    SegmentChanged { segment: usize },
    #[error("malformed attribute list {0:?}")]
    InvalidAttributes(String),
    #[error("a \\zaln-e has no matching \\zaln-s")]
    UnbalancedMilestone,
    #[error("a \\zaln-s is not closed before the end of the verse")]
    UnclosedMilestone,
    #[error(
        "a \\zaln-s needs x-content, and x-occurrence from 1 to x-occurrences, with no attribute twice: {0}"
    )]
    InvalidSourceWord(String),
    #[error(
        "an aligned word must be a \\w (not \\+w) with plain text and exactly x-occurrence and x-occurrences: {0}"
    )]
    InvalidAlignedWord(String),
    #[error(
        "broken word markup {0:?}: a \\w without its closing marker, a \\w* that closes nothing, or a \\zaln without its \\*"
    )]
    BrokenWordMarkup(String),
    #[error("alignment markup in a heading or sidebar, where it cannot be woven back: {0}")]
    AlignmentOutsideVerseText(String),
    #[error("the aligned word {:?} is not one word of the verse text", .0.text)]
    NotOneWord(TargetWord),
    #[error(
        "the verse has \\w or \\zaln markup outside notes; only a verse without word markup can be woven"
    )]
    WordMarkup,
    #[error("the word {:?} (occurrence {} of {}) is not in the verse", .0.text, .0.occurrence, .0.occurrences)]
    TargetWordNotFound(TargetWord),
    #[error("the word {:?} (occurrence {}) is in two alignment groups", .0.text, .0.occurrence)]
    TargetWordAlignedTwice(TargetWord),
    #[error("the source word {content:?} cannot be occurrence {occurrence} of {occurrences}")]
    InvalidOccurrence {
        content: String,
        occurrence: u32,
        occurrences: u32,
    },
    #[error(
        "the attribute {key}={value:?} cannot be written so that it reads back: a malformed, repeated or reserved key, or a quote or backslash in the value"
    )]
    UnencodableAttribute { key: String, value: String },
}
