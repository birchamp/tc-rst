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
//! - a sidebar (`\esb` … `\esbe`, with its headings and paragraphs) is taken whole, when
//!   it starts after the previous verse or chapter marker;
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
//! key either: it keeps its bytes and cannot be looked up by key, but it can be replaced
//! by its index in [`Book::segments`] ([`Book::replace_bodies`], [`Book::replace_units`]).
//! So can each copy of a verse that a book holds twice.
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

    /// The readable text of the body, never hashed. See [`crate::text`].
    pub fn display_text(&self) -> String {
        text::display_text(self.body)
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
    /// one space is written before a non-empty body, unless the body starts with a
    /// backslash or a form feed, which end the number by themselves (`\v 3\p`). So
    /// writing back a unit's own body gives the book unchanged. Fails with
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
        self.rewrite(vec![(index, Replacement::Body(body))])
            .map_err(|e| keyed(e, chapter, verses))
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
        self.rewrite(vec![(index, Replacement::Unit(unit))])
            .map_err(|e| keyed(e, chapter, verses))
    }

    /// The book with the bodies of several verse units replaced and every other byte
    /// unchanged. Each edit names its unit by its index in [`Book::segments`], so a unit
    /// without a key (`\v 3a`) or a verse the book holds twice can be edited too. The new
    /// book is read once for all the edits.
    ///
    /// Each body is written as [`Book::replace_body`] writes it. Fails with
    /// [`Error::NotAVerseUnit`] for an index that is not a verse unit,
    /// [`Error::RepeatedEdit`] for an index named twice, and [`Error::SegmentChanged`] with
    /// the index of the first segment that would not come out as asked.
    pub fn replace_bodies(&self, edits: &[(usize, &str)]) -> Result<String, Error> {
        self.rewrite(
            edits
                .iter()
                .map(|&(index, body)| (index, Replacement::Body(body)))
                .collect(),
        )
    }

    /// The book with several whole verse units (leading markers, `\v` marker and body)
    /// replaced, each named by its index in [`Book::segments`], and every other byte
    /// unchanged. Each new text must be exactly one unit with the same chapter and verse
    /// label (or the same verse span). Fails as [`Book::replace_bodies`] does.
    pub fn replace_units(&self, edits: &[(usize, &str)]) -> Result<String, Error> {
        self.rewrite(
            edits
                .iter()
                .map(|&(index, unit)| (index, Replacement::Unit(unit)))
                .collect(),
        )
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

    /// Apply `edits`, then re-read the new book and confirm that every edited unit is
    /// still the same unit with the new text, and that every other segment is unchanged.
    fn rewrite(&self, mut edits: Vec<(usize, Replacement<'_>)>) -> Result<String, Error> {
        edits.sort_by_key(|&(index, _)| index);
        if let Some(pair) = edits.windows(2).find(|pair| pair[0].0 == pair[1].0) {
            return Err(Error::RepeatedEdit { segment: pair[0].0 });
        }
        let mut out = String::with_capacity(self.source.len());
        let mut at = 0;
        for &(index, replacement) in &edits {
            let segment = self.segments.get(index);
            let Some((segment, unit)) = segment.and_then(|s| Some((s, s.as_verse()?))) else {
                return Err(Error::NotAVerseUnit { segment: index });
            };
            match replacement {
                Replacement::Body(body) => {
                    let body_start = segment.range.end - unit.body.len();
                    out.push_str(&self.source[at..body_start]);
                    if needs_space(unit.marker, body) {
                        out.push(' ');
                    }
                    out.push_str(body);
                }
                Replacement::Unit(text) => {
                    out.push_str(&self.source[at..segment.range.start]);
                    out.push_str(text);
                }
            }
            at = segment.range.end;
        }
        out.push_str(&self.source[at..]);
        self.check_rewrite(&out, &edits)?;
        Ok(out)
    }

    fn check_rewrite(
        &self,
        new_source: &str,
        edits: &[(usize, Replacement<'_>)],
    ) -> Result<(), Error> {
        let new = Book::parse(new_source);
        let mut edits = edits.iter().peekable();
        for (i, old) in self.segments.iter().enumerate() {
            let edit = edits.next_if(|(index, _)| *index == i).map(|(_, r)| r);
            let same = new.segments.get(i).is_some_and(|new| match edit {
                None => old.text == new.text && old.kind == new.kind,
                Some(replacement) => {
                    same_place(old, new)
                        && match replacement {
                            Replacement::Body(body) => new.as_verse().is_some_and(|unit| {
                                old.as_verse().map(|u| u.leading) == Some(unit.leading)
                                    && unit.body == *body
                            }),
                            Replacement::Unit(text) => new.text == *text,
                        }
                }
            });
            if !same {
                return Err(Error::SegmentChanged { segment: i });
            }
        }
        if new.segments.len() != self.segments.len() {
            return Err(Error::SegmentChanged {
                segment: self.segments.len(),
            });
        }
        Ok(())
    }
}

/// What an edit puts in place of a unit.
#[derive(Debug, Clone, Copy)]
enum Replacement<'e> {
    /// The body after the `\v` marker.
    Body(&'e str),
    /// Leading markers, `\v` marker and body.
    Unit(&'e str),
}

/// A keyed edit reports a changed structure by the verse it edited.
fn keyed(error: Error, chapter: u16, verses: VerseSpan) -> Error {
    match error {
        Error::SegmentChanged { .. } => Error::StructureChanged { chapter, verses },
        other => other,
    }
}

/// Whether a space must go between a `\v` marker without a separator and a new body, as
/// the lexer reads them: the verse number runs to the first ASCII whitespace or
/// backslash, and a space, tab, CR or LF right after it becomes the marker's separator.
/// So a body that starts with a backslash or another whitespace (a form feed) needs no
/// space, and every other body needs one to keep its first character.
fn needs_space(marker: &str, body: &str) -> bool {
    let Some(first) = body.bytes().next() else {
        return false;
    };
    let ends_number = first == b'\\' || first.is_ascii_whitespace();
    let becomes_separator = matches!(first, b' ' | b'\t' | b'\n' | b'\r');
    !marker.ends_with(char::is_whitespace) && (!ends_number || becomes_separator)
}

/// Whether two segments are verse units with the same chapter and verse: the same span,
/// or for a unit without one the same label.
fn same_place(old: &Segment<'_>, new: &Segment<'_>) -> bool {
    match (old.as_verse(), new.as_verse()) {
        (Some(old), Some(new)) => {
            old.chapter == new.chapter
                && match old.verses {
                    Some(verses) => new.verses == Some(verses),
                    None => old.label == new.label,
                }
        }
        _ => false,
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
    /// `\esbe`: the sidebar it ends leads, whole.
    SidebarEnd,
    /// Text or inline markup.
    Content,
    /// Never part of a leading run.
    Stop,
}

fn lead(node: &Node<'_>) -> Lead {
    match node {
        Node::Text(text) if text.chars().all(char::is_whitespace) => Lead::Space,
        Node::Marker(m) if !m.is_closing() && !m.is_nested() && m.name() == "esbe" => {
            Lead::SidebarEnd
        }
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
            Lead::SidebarEnd => {
                let opener = (floor..i - 1)
                    .rev()
                    .find(|&k| sidebar_marker(&nodes[k]).is_some());
                match opener {
                    Some(k) if sidebar_marker(&nodes[k]) == Some("esb") => {
                        i = k;
                        start = i;
                    }
                    _ => break,
                }
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

/// `esb` or `esbe` for a marker that starts or ends a sidebar.
fn sidebar_marker<'a>(node: &Node<'a>) -> Option<&'a str> {
    match node {
        Node::Marker(m) if !m.is_closing() && !m.is_nested() => {
            Some(m.name()).filter(|name| matches!(*name, "esb" | "esbe"))
        }
        _ => None,
    }
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
        assert_eq!(book.verse(3, span("1")).unwrap().display_text(), "מִזְמ֥וֹר");
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
        assert_eq!(v1.display_text(), "In the beginning. A line");
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

    #[test]
    fn writing_back_a_body_after_a_marker_without_separator_changes_nothing() {
        for src in [
            "\\c 1\n\\v 1\\p text\n\\v 2 b\n",
            "\\c 1\n\\v 1\\w a\\w* b\n\\v 2 b\n",
            "\\c 1\n\\v 1\\f + \\ft n\\f* a\n",
            "\\c 1\n\\v 1\\\\ odd\n",
            "\\c 1\n\\v 1\u{c}form feed\n",
        ] {
            let book = Book::parse(src);
            let unit = book.verse(1, span("1")).unwrap();
            assert!(!unit.marker().ends_with(char::is_whitespace), "{src:?}");
            assert_eq!(
                book.replace_body(1, span("1"), unit.body()).as_deref(),
                Ok(src)
            );
        }
        // A body that would run into the number still gets its space.
        let book = Book::parse("\\c 1\n\\v 1\\p text\n");
        assert_eq!(
            book.replace_body(1, span("1"), "new").unwrap(),
            "\\c 1\n\\v 1 new"
        );
    }

    #[test]
    fn a_sidebar_leads_whole_into_the_verse_after_it() {
        let src = "\\c 1\n\\esb \\cat Theme\\cat*\n\\s1 Inside\n\\p Sidebar text.\n\\esbe\n\\s1 Title\n\\p\n\\v 1 One.\n\\v 2 Two.\n\\esb \\cat Note\\cat*\n\\ms Away\n\\p More sidebar.\n\\esbe\n\\p\n\\v 3 Three.\n";
        let book = Book::parse(src);
        let chapters: Vec<&str> = book
            .segments()
            .iter()
            .filter(|s| matches!(s.kind(), SegmentKind::Chapter(_)))
            .map(Segment::text)
            .collect();
        assert_eq!(chapters, vec!["\\c 1\n"]);
        assert_eq!(
            book.verse(1, span("1")).unwrap().leading(),
            "\\esb \\cat Theme\\cat*\n\\s1 Inside\n\\p Sidebar text.\n\\esbe\n\\s1 Title\n\\p\n"
        );
        assert_eq!(book.verse(1, span("2")).unwrap().body(), "Two.\n");
        assert_eq!(
            book.verse(1, span("3")).unwrap().leading(),
            "\\esb \\cat Note\\cat*\n\\ms Away\n\\p More sidebar.\n\\esbe\n\\p\n"
        );
        // A sidebar inside a body is not verse text.
        let unit = "\\v 2 Two \\esb \\p Aside.\n\\esbe\n\\p again.\n";
        assert_eq!(text::display_text(&unit[5..]), "Two again.");
    }

    #[test]
    fn units_without_a_unique_key_are_edited_by_index() {
        let src = "\\c 1\n\\v 1 a\n\\v 1 b\n\\v 3a c\n\\v 4 d\n";
        let book = Book::parse(src);
        let index = |text: &str| {
            book.segments()
                .iter()
                .position(|s| s.text() == text)
                .unwrap()
        };
        let (first, second, letter) = (index("\\v 1 a\n"), index("\\v 1 b\n"), index("\\v 3a c\n"));
        assert_eq!(
            book.replace_bodies(&[(second, "B\n"), (letter, "C\n")])
                .unwrap(),
            "\\c 1\n\\v 1 a\n\\v 1 B\n\\v 3a C\n\\v 4 d\n"
        );
        assert_eq!(
            book.replace_units(&[(first, "\\p\n\\v 1 A\n")]).unwrap(),
            "\\c 1\n\\p\n\\v 1 A\n\\v 1 b\n\\v 3a c\n\\v 4 d\n"
        );
        assert_eq!(
            book.replace_units(&[(letter, "\\v 3b c\n")]),
            Err(Error::SegmentChanged { segment: letter })
        );
        assert_eq!(
            book.replace_bodies(&[(second, "x\n\\v 9 y\n")]),
            Err(Error::SegmentChanged { segment: second })
        );
        assert_eq!(
            book.replace_bodies(&[(second, "x\n"), (second, "y\n")]),
            Err(Error::RepeatedEdit { segment: second })
        );
        assert_eq!(
            book.replace_bodies(&[(0, "x\n")]),
            Err(Error::NotAVerseUnit { segment: 0 })
        );
        assert_eq!(
            book.replace_bodies(&[(99, "x\n")]),
            Err(Error::NotAVerseUnit { segment: 99 })
        );
        assert_eq!(book.replace_bodies(&[]).as_deref(), Ok(src));
    }
}
