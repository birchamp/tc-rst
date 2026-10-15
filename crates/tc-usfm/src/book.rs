//! A book cut into segments: header, chapter markers and verse units.
//!
//! [`Book::parse`] cuts the whole text into consecutive segments, so concatenating the
//! segments' text gives the input byte for byte. Each `\v` after the first `\c` starts a
//! verse unit keyed by its chapter and its verse span.
//!
//! # Which unit owns a leading marker
//!
//! A unit owns the markers that lead into it: the paragraph, poetry, list and break
//! markers (`\p`, `\q1`, `\li1`, `\b`, `\nb`…), headings with their text (`\s1 …`, `\d …`,
//! `\ms …`, `\r …`, `\sp …`, `\rem …`), `\ts\*` chunk milestones and `\ca …\ca*` that
//! stand between the end of the previous verse's text and its `\v` marker. Precisely,
//! walking back from the `\v` marker:
//!
//! - whitespace-only text is skipped (it stays with the previous unit unless a leading
//!   marker comes before it);
//! - a paragraph-type marker, a `\ts` milestone or a `\ca` span is taken;
//! - text or inline markup is taken only together with the heading marker it belongs to;
//!   if it belongs to a paragraph-type marker (`\q2 and the second line`) or to the
//!   previous verse marker, it is verse text and the walk stops;
//! - the walk never takes a title or introduction marker (`\mt1`, `\is`…), an
//!   identification marker, a chapter label (`\cl`, `\cp`, `\cd`) or a verse or chapter
//!   marker.
//!
//! The same walk gives a chapter segment its leading markers (`\ts\*` before `\c 2`). A
//! chapter segment also owns the `\ca`, `\cl`, `\cp` and `\cd` right after its `\c`, and
//! anything between the chapter marker and the first verse unit that does not lead into
//! that verse. Markers after the last verse of the book stay in that verse's body.
//!
//! So a verse unit is `leading markers + \v marker + body`, and the body runs to the
//! start of the next unit. Adding a paragraph break before verse 5 edits verse 5's unit.
//! A `\v` before the first `\c` is part of the header and has no key, and a unit whose
//! label is not a verse or span (`\v 3a`) or whose chapter label is not a number has no
//! key either: it keeps its bytes but cannot be looked up or replaced.
//!
//! tC4's journal cuts the other way (`journal/skeleton.mjs`): a verse slot runs to the
//! next `\v`, so the paragraph marker before verse 5 is the end of verse 4's slot. Here
//! the heading `\s The Lord's Prayer` before verse 9 belongs to verse 9, the verse it
//! introduces, and a Psalm's `\d` superscription belongs to verse 1, which is what a
//! translator edits together.

use crate::Error;
use crate::doc::{Document, Node};
use crate::markers::MarkerClass;
use crate::text::{self, offset_in};
use crate::words::WordToken;
use std::ops::Range;
use tc_core::VerseSpan;

/// A book cut into segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Book<'a> {
    source: &'a str,
    segments: Vec<Segment<'a>>,
}

/// One piece of a book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment<'a> {
    kind: SegmentKind<'a>,
    range: Range<usize>,
    text: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentKind<'a> {
    /// Everything before the first chapter segment: identification, titles, introduction.
    Header,
    Chapter(ChapterMark<'a>),
    Verse(VerseUnit<'a>),
}

/// A `\c` marker with what it owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChapterMark<'a> {
    label: &'a str,
    number: Option<u16>,
}

/// One verse unit: leading markers, the `\v` marker and the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerseUnit<'a> {
    chapter: Option<u16>,
    label: &'a str,
    verses: Option<VerseSpan>,
    leading: &'a str,
    marker: &'a str,
    body: &'a str,
}

impl<'a> Segment<'a> {
    pub fn kind(&self) -> &SegmentKind<'a> {
        &self.kind
    }

    /// Where the segment sits in the book, in bytes.
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    pub fn text(&self) -> &'a str {
        self.text
    }

    pub fn as_verse(&self) -> Option<&VerseUnit<'a>> {
        match &self.kind {
            SegmentKind::Verse(unit) => Some(unit),
            _ => None,
        }
    }
}

impl<'a> ChapterMark<'a> {
    /// The chapter number exactly as written after `\c`.
    pub fn label(&self) -> &'a str {
        self.label
    }

    /// The chapter number, or `None` when the label is not a number from 1 to 65535.
    pub fn number(&self) -> Option<u16> {
        self.number
    }
}

impl<'a> VerseUnit<'a> {
    /// The number of the chapter the unit is in, or `None` when that chapter's label is
    /// not a number.
    pub fn chapter(&self) -> Option<u16> {
        self.chapter
    }

    /// The verse number exactly as written after `\v`, such as `9-10` or `3a`.
    pub fn label(&self) -> &'a str {
        self.label
    }

    /// The verse span, or `None` when the label is not a verse or span such as `3a`.
    pub fn verses(&self) -> Option<VerseSpan> {
        self.verses
    }

    /// The markers that lead into the verse, before its `\v` marker.
    pub fn leading(&self) -> &'a str {
        self.leading
    }

    /// The `\v` marker with its number and the one space (or line break) after it.
    pub fn marker(&self) -> &'a str {
        self.marker
    }

    /// Everything after the `\v` marker up to the next unit, including trailing line
    /// breaks, inline markup, notes and paragraph markers inside the verse.
    pub fn body(&self) -> &'a str {
        self.body
    }

    /// The normalized plain text of the body. See [`crate::text`].
    pub fn plain_text(&self) -> String {
        text::plain_text(self.body)
    }

    /// The words of the body, numbered by occurrence. Offsets are relative to the body.
    pub fn words(&self) -> Vec<WordToken<'a>> {
        text::verse_words(self.body)
    }

    fn key(&self) -> Option<(u16, VerseSpan)> {
        Some((self.chapter?, self.verses?))
    }
}

impl<'a> Book<'a> {
    /// Cut a book into segments. This never fails: any text is a valid book, though a
    /// text without `\c` is all header.
    pub fn parse(source: &'a str) -> Self {
        let doc = Document::parse(source);
        let segments = segment(source, doc.nodes());
        Self { source, segments }
    }

    pub fn source(&self) -> &'a str {
        self.source
    }

    /// The segments in order. Their texts concatenate to the source.
    pub fn segments(&self) -> &[Segment<'a>] {
        &self.segments
    }

    /// The verse units in order.
    pub fn verses(&self) -> impl Iterator<Item = &VerseUnit<'a>> {
        self.segments.iter().filter_map(Segment::as_verse)
    }

    /// The unit of `chapter` whose `\v` is exactly `verses`: `9-10` does not match `9`.
    pub fn verse(&self, chapter: u16, verses: VerseSpan) -> Result<&VerseUnit<'a>, Error> {
        let index = self.unit_index(chapter, verses)?;
        self.segments[index]
            .as_verse()
            .ok_or(Error::VerseNotFound { chapter, verses })
    }

    /// The book with the body of one unit replaced and every other byte unchanged.
    ///
    /// When the `\v` marker has no space after its number (`\v 3` at the end of the file),
    /// one space is written before a non-empty body. Fails with
    /// [`Error::StructureChanged`] when the new body would add or remove a unit, or would
    /// move a boundary between units: a body that contains `\v` or `\c`, that ends with
    /// markers that would lead into the next verse (`…\n\p\n`), or that starts with LF
    /// after a marker ending in a lone CR (the two would read as one line end of the
    /// marker). The check re-reads the whole new book, so whatever this returns is known
    /// to differ from the old book in this unit's body alone.
    pub fn replace_body(
        &self,
        chapter: u16,
        verses: VerseSpan,
        body: &str,
    ) -> Result<String, Error> {
        let index = self.unit_index(chapter, verses)?;
        let segment = &self.segments[index];
        let unit = segment
            .as_verse()
            .ok_or(Error::VerseNotFound { chapter, verses })?;
        let body_start = segment.range.end - unit.body.len();
        let needs_space = !body.is_empty() && !unit.marker.ends_with(char::is_whitespace);
        let mut out = String::with_capacity(self.source.len() + body.len() + 1);
        out.push_str(&self.source[..body_start]);
        if needs_space {
            out.push(' ');
        }
        out.push_str(body);
        out.push_str(&self.source[segment.range.end..]);
        self.check_only_unit_changed(&out, index, chapter, verses, |new| {
            new.as_verse()
                .is_some_and(|new| new.leading == unit.leading && new.body == body)
        })?;
        Ok(out)
    }

    /// The book with one whole unit (leading markers, `\v` marker and body) replaced and
    /// every other byte unchanged. The new text must be exactly one unit with the same
    /// chapter and verse number, or this fails with [`Error::StructureChanged`].
    pub fn replace_unit(
        &self,
        chapter: u16,
        verses: VerseSpan,
        unit: &str,
    ) -> Result<String, Error> {
        let index = self.unit_index(chapter, verses)?;
        let range = self.segments[index].range.clone();
        let mut out = String::with_capacity(self.source.len() + unit.len());
        out.push_str(&self.source[..range.start]);
        out.push_str(unit);
        out.push_str(&self.source[range.end..]);
        self.check_only_unit_changed(&out, index, chapter, verses, |new| new.text == unit)?;
        Ok(out)
    }

    fn unit_index(&self, chapter: u16, verses: VerseSpan) -> Result<usize, Error> {
        let mut found = self
            .segments
            .iter()
            .enumerate()
            .filter(|(_, s)| s.as_verse().and_then(VerseUnit::key) == Some((chapter, verses)))
            .map(|(i, _)| i);
        let first = found
            .next()
            .ok_or(Error::VerseNotFound { chapter, verses })?;
        if found.next().is_some() {
            return Err(Error::AmbiguousVerse { chapter, verses });
        }
        Ok(first)
    }

    /// Re-parse `new_source` and confirm that only segment `index` changed, that it is
    /// still the same unit, and that `same_unit` accepts it.
    fn check_only_unit_changed(
        &self,
        new_source: &str,
        index: usize,
        chapter: u16,
        verses: VerseSpan,
        same_unit: impl Fn(&Segment<'_>) -> bool,
    ) -> Result<(), Error> {
        let changed = || Error::StructureChanged { chapter, verses };
        let new = Book::parse(new_source);
        if new.segments.len() != self.segments.len() {
            return Err(changed());
        }
        for (i, (old, new)) in self.segments.iter().zip(&new.segments).enumerate() {
            let same = if i == index {
                let key = |s: &Segment<'_>| s.as_verse().and_then(VerseUnit::key);
                key(new).is_some() && key(old) == key(new) && same_unit(new)
            } else {
                old.text == new.text && old.kind == new.kind
            };
            if !same {
                return Err(changed());
            }
        }
        Ok(())
    }
}

/// How a node behaves when walking back from a `\v` or `\c` marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lead {
    /// Whitespace-only text.
    Space,
    /// A paragraph-type marker, a `\ts` milestone or a `\ca` span: it leads.
    Marker,
    /// A heading marker: it leads, together with its text.
    Heading,
    /// Text or inline markup.
    Content,
    /// Never part of a leading run.
    Stop,
}

fn lead(node: &Node<'_>) -> Lead {
    match node {
        Node::Text(text) if text.chars().all(char::is_whitespace) => Lead::Space,
        Node::Marker(m) if !m.is_closing() && !m.is_nested() => match m.class() {
            MarkerClass::Paragraph => Lead::Marker,
            MarkerClass::Heading => Lead::Heading,
            MarkerClass::Chapter
            | MarkerClass::Verse
            | MarkerClass::Identification
            | MarkerClass::Introduction
            | MarkerClass::ChapterLevel => Lead::Stop,
            MarkerClass::Note | MarkerClass::NoteContent | MarkerClass::Character => Lead::Content,
        },
        Node::Milestone(m) if m.name() == "ts" || m.name().starts_with("ts-") => Lead::Marker,
        Node::Span(s) if s.name() == "ca" => Lead::Marker,
        Node::Bom(_) => Lead::Stop,
        _ => Lead::Content,
    }
}

/// The first node of the run that leads into the anchor at `anchor`, never before `floor`.
fn leading_start(nodes: &[Node<'_>], anchor: usize, floor: usize) -> usize {
    let mut start = anchor;
    let mut i = anchor;
    while i > floor {
        match lead(&nodes[i - 1]) {
            Lead::Space => i -= 1,
            Lead::Marker | Lead::Heading => {
                i -= 1;
                start = i;
            }
            Lead::Content => {
                let mut k = i - 1;
                while k > floor && matches!(lead(&nodes[k - 1]), Lead::Content | Lead::Space) {
                    k -= 1;
                }
                if k > floor && lead(&nodes[k - 1]) == Lead::Heading {
                    i = k - 1;
                    start = i;
                } else {
                    break;
                }
            }
            Lead::Stop => break,
        }
    }
    start
}

/// The end of what a chapter marker at `chapter` owns after itself: `\ca`, and `\cl`,
/// `\cp` or `\cd` with their text.
fn chapter_run_end(nodes: &[Node<'_>], chapter: usize) -> usize {
    let mut q = chapter + 1;
    while let Some(node) = nodes.get(q) {
        let owned = match node {
            Node::Span(s) => s.name() == "ca",
            Node::Marker(m) => !m.is_closing() && m.class() == MarkerClass::ChapterLevel,
            _ => lead(node) == Lead::Space,
        };
        if !owned {
            break;
        }
        q += 1;
        if matches!(node, Node::Marker(_)) {
            while nodes
                .get(q)
                .is_some_and(|n| matches!(lead(n), Lead::Content | Lead::Space))
            {
                q += 1;
            }
        }
    }
    q
}

#[derive(Debug, Clone, Copy)]
enum Anchor<'a> {
    Chapter { label: &'a str },
    Verse { label: &'a str, marker: &'a str },
}

fn segment<'a>(source: &'a str, nodes: &[Node<'a>]) -> Vec<Segment<'a>> {
    let starts: Vec<usize> = nodes.iter().map(|n| node_offset(source, n)).collect();
    let mut anchors: Vec<(usize, Anchor<'a>)> = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        let Node::Marker(m) = node else { continue };
        if let (true, Some(label)) = (m.is_chapter(), m.number()) {
            anchors.push((i, Anchor::Chapter { label }));
        } else if let (true, Some(label), false) = (m.is_verse(), m.number(), anchors.is_empty()) {
            anchors.push((
                i,
                Anchor::Verse {
                    label,
                    marker: m.raw(),
                },
            ));
        }
    }
    let mut leads = Vec::with_capacity(anchors.len());
    let mut floor = 0;
    for &(at, anchor) in &anchors {
        leads.push(leading_start(nodes, at, floor));
        floor = match anchor {
            Anchor::Chapter { .. } => chapter_run_end(nodes, at),
            Anchor::Verse { .. } => at + 1,
        };
    }
    let offset = |node: usize| starts.get(node).copied().unwrap_or(source.len());

    let mut segments = Vec::with_capacity(anchors.len() + 1);
    let header_end = leads.first().map_or(source.len(), |&l| offset(l));
    if header_end > 0 {
        segments.push(Segment {
            kind: SegmentKind::Header,
            range: 0..header_end,
            text: &source[..header_end],
        });
    }
    let mut chapter = None;
    for (k, &(at, anchor)) in anchors.iter().enumerate() {
        let start = offset(leads[k]);
        let end = leads.get(k + 1).map_or(source.len(), |&l| offset(l));
        let kind = match anchor {
            Anchor::Chapter { label } => {
                chapter = parse_chapter(label);
                SegmentKind::Chapter(ChapterMark {
                    label,
                    number: chapter,
                })
            }
            Anchor::Verse { label, marker } => {
                let marker_start = offset(at);
                let body_start = marker_start + marker.len();
                SegmentKind::Verse(VerseUnit {
                    chapter,
                    label,
                    verses: label.parse().ok(),
                    leading: &source[start..marker_start],
                    marker,
                    body: &source[body_start..end],
                })
            }
        };
        segments.push(Segment {
            kind,
            range: start..end,
            text: &source[start..end],
        });
    }
    segments
}

fn parse_chapter(label: &str) -> Option<u16> {
    if label.is_empty() || !label.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    label.parse().ok().filter(|&n| n > 0)
}

/// Where a top-level node starts in the source.
fn node_offset(source: &str, node: &Node<'_>) -> usize {
    let first = match node {
        Node::Bom(s) | Node::Text(s) | Node::Stray(s) => *s,
        Node::Marker(m) => m.raw(),
        Node::Span(span) => span.open.raw(),
        Node::Milestone(m) => m.marker.raw(),
    };
    offset_in(source, first)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(s: &str) -> VerseSpan {
        s.parse().unwrap()
    }

    fn leading_of(src: &str) -> Vec<(String, &str)> {
        Book::parse(src)
            .verses()
            .map(|u| (u.label().to_owned(), u.leading()))
            .collect()
    }

    #[test]
    fn psalm_layouts_give_each_verse_what_introduces_it() {
        // ULT style: a superscription and a book heading lead into verse 1.
        assert_eq!(
            leading_of(
                "\\c 1\n\\ms1 Book One\n\\q1\n\\v 1 Blessed\n\\q2 is he.\n\\q1\n\\v 2 But\n"
            ),
            vec![
                ("1".to_owned(), "\\ms1 Book One\n\\q1\n"),
                ("2".to_owned(), "\\q1\n")
            ]
        );
        assert_eq!(
            leading_of("\\c 3\n\\d A psalm of David.\n\\q1\n\\v 1 Yahweh,\n"),
            vec![("1".to_owned(), "\\d A psalm of David.\n\\q1\n")]
        );
        // UHB style: the superscription is verse 1's own text.
        let uhb = "\\c 3\n\\d\n\\v 1 \\w מִזְמ֥וֹר|lemma=\"מִזְמוֹר\"\\w*\n\\q1\n\\v 2 \\w יְ֭הוָה\\w*\n";
        let book = Book::parse(uhb);
        assert_eq!(book.verse(3, span("1")).unwrap().leading(), "\\d\n");
        assert_eq!(book.verse(3, span("1")).unwrap().plain_text(), "מִזְמ֥וֹר");
        // Markers in the middle of a line, and old chunk headings.
        assert_eq!(
            leading_of("\\c 1\n\\q1 \\v 3 Text.\n\\s5\n\\v 4 More \\ts\\*\n\\v 5 End.\n"),
            vec![
                ("3".to_owned(), "\\q1 "),
                ("4".to_owned(), "\\s5\n"),
                ("5".to_owned(), "\\ts\\*\n")
            ]
        );
        // Text after a paragraph marker is the previous verse's text.
        assert_eq!(
            leading_of("\\c 1\n\\p\n\\v 5 One\n\\p And then \\v 6 more.\n"),
            vec![("5".to_owned(), "\\p\n"), ("6".to_owned(), "")]
        );
    }

    #[test]
    fn leading_markers_belong_to_the_next_verse() {
        let src = "\\id GEN\n\\mt1 Genesis\n\n\\ts\\*\n\\c 1\n\\s Heading\n\\p\n\\v 1 In the beginning.\n\\q1 A line\n\\q2\n\\v 2 Next.";
        let book = Book::parse(src);
        let texts: Vec<&str> = book.segments().iter().map(Segment::text).collect();
        assert_eq!(
            texts,
            vec![
                "\\id GEN\n\\mt1 Genesis\n\n",
                "\\ts\\*\n\\c 1\n",
                "\\s Heading\n\\p\n\\v 1 In the beginning.\n\\q1 A line\n",
                "\\q2\n\\v 2 Next.",
            ]
        );
        let v1 = book.verse(1, span("1")).unwrap();
        assert_eq!(v1.leading(), "\\s Heading\n\\p\n");
        assert_eq!(v1.marker(), "\\v 1 ");
        assert_eq!(v1.body(), "In the beginning.\n\\q1 A line\n");
        assert_eq!(v1.plain_text(), "In the beginning. A line");
    }

    #[test]
    fn replacing_a_body_changes_nothing_else() {
        let src = "\\c 1\n\\p\n\\v 1 One.\n\\v 2 Two.\n\\p\n\\v 3";
        let book = Book::parse(src);
        assert_eq!(
            book.replace_body(1, span("2"), "Deux.\n").unwrap(),
            "\\c 1\n\\p\n\\v 1 One.\n\\v 2 Deux.\n\\p\n\\v 3"
        );
        assert_eq!(
            book.replace_body(1, span("3"), "Trois.").unwrap(),
            "\\c 1\n\\p\n\\v 1 One.\n\\v 2 Two.\n\\p\n\\v 3 Trois."
        );
        for bad in ["Two.\n\\v 4 Four.", "Two.\n\\p\n", "Two \\c 2 x"] {
            assert_eq!(
                book.replace_body(1, span("2"), bad),
                Err(Error::StructureChanged {
                    chapter: 1,
                    verses: span("2")
                }),
                "{bad:?}"
            );
        }
        assert_eq!(
            book.replace_body(1, span("4"), "x"),
            Err(Error::VerseNotFound {
                chapter: 1,
                verses: span("4")
            })
        );
    }

    #[test]
    fn replacing_a_unit_can_change_its_leading_markers() {
        let src = "\\c 1\n\\p\n\\v 1 One.\n\\v 2 Two.\n";
        let book = Book::parse(src);
        assert_eq!(
            book.replace_unit(1, span("2"), "\\p\n\\v 2 Two.\n")
                .unwrap(),
            "\\c 1\n\\p\n\\v 1 One.\n\\p\n\\v 2 Two.\n"
        );
        assert!(book.replace_unit(1, span("2"), "\\v 3 Two.\n").is_err());
    }

    #[test]
    fn a_line_feed_cannot_join_a_markers_lone_carriage_return() {
        let book = Book::parse("\\c 1\n\\v 1\r\\v 2 b\n");
        assert_eq!(book.verse(1, span("1")).unwrap().marker(), "\\v 1\r");
        assert_eq!(
            book.replace_body(1, span("1"), "\n"),
            Err(Error::StructureChanged {
                chapter: 1,
                verses: span("1")
            })
        );
        assert_eq!(
            book.replace_body(1, span("1"), "a\n").unwrap(),
            "\\c 1\n\\v 1\ra\n\\v 2 b\n"
        );
    }

    #[test]
    fn duplicate_verses_are_ambiguous() {
        let book = Book::parse("\\c 1\n\\v 1 a\n\\v 1 b\n");
        assert_eq!(
            book.verse(1, span("1")),
            Err(Error::AmbiguousVerse {
                chapter: 1,
                verses: span("1")
            })
        );
    }
}
