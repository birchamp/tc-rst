//! The readable text of a verse, as tC4's source panes show it. The Check screen and the
//! aligner show [`crate::shown_text_v1`] instead. The words of a verse are
//! [`crate::target_words`].
//!
//! [`display_text`] reads a verse body (the USFM after `\v N `) and keeps only what a reader
//! sees as the verse, as tC4's `verseText` does (`src/views/verseText.js:9` at d310bf3, the
//! text tC4's source panes show):
//!
//! - Text outside markers, and the text inside character spans such as `\w`, `\add` or
//!   `\nd`, is kept. Attribute lists (`|x-occurrence="1"`) are not text.
//! - Notes (`\f`, `\fe`, `\ef`, `\x`, `\ex`), figures (`\fig`), alternate and published
//!   numbers (`\ca`, `\va`, `\vp`), `\cat` and `\rq` are left out with everything inside.
//! - Milestones such as `\zaln-s`, `\zaln-e` and `\ts\*` carry no text.
//! - A heading inside the body (`\s`, `\ms`, `\r`, `\rem`…) is left out up to the next
//!   paragraph marker. A sidebar (`\esb` to `\esbe`) is left out with everything inside.
//!   A paragraph marker, verse or chapter marker counts as a space.
//! - The text of `\d`, `\sp`, `\qa` and `\lit` is kept: usfm-js shows it as verse content,
//!   so a Psalm's superscription written inside verse 1 (`\v 1 \d To the chief musician.`)
//!   is part of verse 1, its words counted with the verse's, as tC4's aligner counts them.
//!   The marker counts as a space ([`markers::shows_as_verse_text`]).
//!
//! Then the text is normalized: every run of whitespace becomes one U+0020 SPACE, and
//! leading and trailing whitespace is removed. Whitespace here is Unicode `White_Space`
//! except the no-break spaces U+00A0, U+2007 and U+202F, which are kept because they are
//! typography, not layout. No Unicode normalization (NFC) is applied: the text keeps the
//! code points of the file.
//!
//! It is never hashed. tC4 hashes a verse for its alignment record (`targetVerseMd5`,
//! invariant I-3) with usfm-js: the top-level text of its journal slot, trimmed
//! (`verseTextMd5` in `journal/fold.mjs`), which is [`crate::plain_text_v1`]. On the tC4
//! fixtures the two texts are equal for all 239 verses of the plain drafts and of UHB Jonah
//! (`fixtures/usfm/verse-text-vectors.json`). They differ where usfm-js is uneven: it keeps
//! the line breaks inside a poetic verse and doubled spaces, it hashes a heading that
//! introduces the next verse into this one (its slot runs to the next `\v`), and it drops
//! the text of `\li` list items, of a `\qs` written over lines (the ULT's Selah) and of
//! `\w` words inside `\zaln` milestones.

use crate::doc::{Document, Marker, Node};
use crate::markers::{self, MarkerClass};

/// The readable text of a verse body, whitespace normalized. See the module documentation.
pub fn display_text(body: &str) -> String {
    let doc = Document::parse(body);
    let mut raw = String::with_capacity(body.len());
    for piece in text_pieces(doc.nodes()) {
        match piece {
            TextPiece::Text(text) => raw.push_str(text),
            TextPiece::Break => raw.push(' '),
        }
    }
    normalize_whitespace(&raw)
}

/// What the reader sees of a verse, in order: stretches of text, and breaks where a
/// paragraph-level marker stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextPiece<'a> {
    Text(&'a str),
    Break,
}

fn text_pieces<'a>(nodes: &[Node<'a>]) -> Vec<TextPiece<'a>> {
    let mut pieces = Vec::new();
    let mut flow = Flow::default();
    collect(nodes, &mut pieces, &mut flow);
    pieces
}

fn collect<'a>(nodes: &[Node<'a>], out: &mut Vec<TextPiece<'a>>, flow: &mut Flow) {
    for node in nodes {
        match node {
            Node::Text(text) if flow.is_verse_text() => out.push(TextPiece::Text(text)),
            Node::Marker(marker) => {
                if !marker.is_closing() && marker.class().is_paragraph_level() {
                    out.push(TextPiece::Break);
                }
                *flow = flow.after(marker);
            }
            Node::Span(span) if flow.is_verse_text() && !markers::is_non_text_span(span.name()) => {
                collect(&span.children, out, flow);
            }
            _ => {}
        }
    }
}

/// Where a walk through USFM stands: in verse text, in the text of a heading, or in a
/// sidebar. Only verse text is read as the verse, tokenized and aligned.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Flow {
    heading: bool,
    sidebar: bool,
}

impl Flow {
    /// The flow after `marker`. Headings, titles and chapter labels start a heading's text;
    /// paragraph, verse and chapter markers end it, and so do the headings whose text is
    /// verse text ([`markers::shows_as_verse_text`], such as a superscription's `\d`).
    /// `\esb` starts a sidebar and only `\esbe` ends it, because a sidebar holds headings
    /// and paragraphs of its own.
    pub(crate) fn after(self, marker: &Marker<'_>) -> Self {
        if marker.is_closing() || marker.is_nested() {
            return self;
        }
        match marker.name() {
            "esb" => {
                return Self {
                    heading: false,
                    sidebar: true,
                };
            }
            "esbe" => return Self::default(),
            _ => {}
        }
        let heading = match marker.class() {
            MarkerClass::Heading => !markers::shows_as_verse_text(marker.name()),
            MarkerClass::Identification | MarkerClass::Introduction | MarkerClass::ChapterLevel => {
                true
            }
            MarkerClass::Paragraph | MarkerClass::Verse | MarkerClass::Chapter => false,
            MarkerClass::Note | MarkerClass::NoteContent | MarkerClass::Character => self.heading,
        };
        Self { heading, ..self }
    }

    pub(crate) fn is_verse_text(self) -> bool {
        !self.heading && !self.sidebar
    }
}

/// The byte offset of `inner` in `outer`. `inner` must be a slice of `outer`.
pub(crate) fn offset_in(outer: &str, inner: &str) -> usize {
    let offset = (inner.as_ptr() as usize).wrapping_sub(outer.as_ptr() as usize);
    debug_assert!(offset + inner.len() <= outer.len(), "not a slice of outer");
    offset
}

/// Collapse each whitespace run to one space and trim. See the module documentation for
/// which characters count as whitespace.
pub fn normalize_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for c in text.chars() {
        if is_layout_space(c) {
            pending_space = !out.is_empty();
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(c);
        }
    }
    out
}

fn is_layout_space(c: char) -> bool {
    c.is_whitespace() && !matches!(c, '\u{A0}' | '\u{2007}' | '\u{202F}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_attributes_and_headings_are_not_verse_text() {
        let body = "The first \\w word|x-occurrence=\"1\"\\w*\n\\zaln-e\\* has a note \\f + \\fr 1:1 \\ft hidden\\f*  here.\n\\s A heading\n\\q2 and \\nd Lord\\nd*'s \\ca 2\\ca*line.\u{A0}!";
        assert_eq!(
            display_text(body),
            "The first word has a note here. and Lord's line.\u{A0}!"
        );
    }

    #[test]
    fn a_superscription_is_verse_text_and_a_section_heading_is_not() {
        let body = "\\d A psalm of David.\n\\q1 Yahweh, hear David.\n\\s Not this\n\\q2 and \\sp this\n\\q1 too.";
        assert_eq!(
            display_text(body),
            "A psalm of David. Yahweh, hear David. and this too."
        );
        let david: Vec<(u32, u32)> = crate::target_words(body)
            .iter()
            .filter(|w| w.text == "David")
            .map(|w| (w.occurrence, w.occurrences))
            .collect();
        assert_eq!(david, vec![(1, 2), (2, 2)]);
    }

    #[test]
    fn words_are_found_inside_word_markers_with_their_offsets() {
        let body = "\\zaln-s |x-content=\"x\"\\*\\w non\\w*-\\w lying\\w*\\zaln-e\\* non \\f + \\ft lying\\f*";
        let found: Vec<(&str, u32, u32)> = crate::align::word_tokens(body)
            .iter()
            .map(|w| (w.text, w.occurrence, w.occurrences))
            .collect();
        assert_eq!(found, vec![("non", 1, 2), ("lying", 1, 1), ("non", 2, 2)]);
        let first = crate::align::word_tokens(body)[0];
        assert_eq!(&body[first.start..first.start + 3], "non");
    }

    #[test]
    fn whitespace_collapses_and_trims() {
        assert_eq!(normalize_whitespace("\n a \t\r\n b\u{2003}c  "), "a b c");
        assert_eq!(display_text("___\n"), "___");
        assert_eq!(display_text(""), "");
    }
}
