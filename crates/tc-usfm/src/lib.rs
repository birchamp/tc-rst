//! Lossless USFM for the tC4 rebuild.
//!
//! - [`Document`] parses any text into a typed tree that writes back to the exact input
//!   bytes: byte order mark, CRLF, odd whitespace, unknown markers, attributes, notes,
//!   figures and milestones included. Parsing never fails.
//! - [`Book`] cuts a book into a header, chapter segments and verse units keyed by chapter
//!   and [`tc_core::VerseSpan`], and replaces one unit while keeping every other byte. The
//!   rule for which unit owns a leading paragraph marker or heading is in [`book`].
//! - [`plain_text`] gives the readable text of a verse for display and hashing, with the
//!   normalization documented in [`text`].
//! - [`words`] splits text into words exactly as tC4 does, with occurrence numbers.
//! - [`align`] reads, strips and weaves `\zaln` word alignment the way en_ult and tC4's
//!   export write it.

pub mod align;
mod attributes;
pub mod book;
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
    #[error("malformed attribute list {0:?}")]
    InvalidAttributes(String),
    #[error("a \\zaln-e has no matching \\zaln-s")]
    UnbalancedMilestone,
    #[error("a \\zaln-s is not closed before the end of the verse")]
    UnclosedMilestone,
    #[error("\\zaln-s needs x-content, x-occurrence and x-occurrences: {0}")]
    InvalidSourceWord(String),
    #[error("an aligned \\w needs plain text and only x-occurrence and x-occurrences: {0}")]
    InvalidAlignedWord(String),
    #[error("the verse already has \\w or \\zaln markup in its text")]
    AlreadyAligned,
    #[error("the word {:?} (occurrence {} of {}) is not in the verse", .0.text, .0.occurrence, .0.occurrences)]
    TargetWordNotFound(TargetWord),
    #[error("the word {:?} (occurrence {}) is in two alignment groups", .0.text, .0.occurrence)]
    TargetWordAlignedTwice(TargetWord),
    #[error("the attribute {key}={value:?} cannot be written in USFM")]
    UnencodableAttribute { key: String, value: String },
}
