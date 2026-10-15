//! The typed, lossless document: a tree of nodes that borrow the source text.
//!
//! [`Document::parse`] never fails. Whatever the input, writing the nodes back out in
//! order gives the input byte for byte: every byte belongs to exactly one leaf.

use crate::attributes::Attributes;
use crate::lex::{Token, TokenKind, lex};
use crate::markers::{self, MarkerClass};
use std::fmt;

/// Character spans nest at most this deep. Deeper openers stay standalone markers, so a
/// hostile file cannot exhaust the stack.
const MAX_SPAN_DEPTH: usize = 64;

/// A parsed USFM text: a whole book or any fragment of one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document<'a> {
    source: &'a str,
    nodes: Vec<Node<'a>>,
}

/// One piece of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node<'a> {
    /// The byte order mark at the start of the input.
    Bom(&'a str),
    /// Text between markers, exactly as written.
    Text(&'a str),
    /// A marker that is not part of a span or milestone: paragraph markers, `\c`, `\v`, note
    /// content markers such as `\ft`, unknown markers, and closing markers that close nothing.
    Marker(Marker<'a>),
    /// A character span such as `\w …\w*`, `\f …\f*` or `\add …\add*`.
    Span(Span<'a>),
    /// A milestone such as `\zaln-s |…\*`, `\zaln-e\*` or `\ts\*`.
    Milestone(Milestone<'a>),
    /// A `\*` or an attribute list that belongs to no span or milestone.
    Stray(&'a str),
}

/// A marker token: the backslash, the name, and the whitespace (and for `\c` and `\v` the
/// number) that belong to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marker<'a> {
    raw: &'a str,
    /// Where the name ends in `raw`.
    name_end: usize,
}

/// A character span. `close` is `None` when the closing marker is missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span<'a> {
    pub open: Marker<'a>,
    pub children: Vec<Node<'a>>,
    pub attributes: Option<Attributes<'a>>,
    pub close: Option<Marker<'a>>,
}

/// A milestone: its start marker, its attributes, and the `\*` that ends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Milestone<'a> {
    pub marker: Marker<'a>,
    pub attributes: Option<Attributes<'a>>,
    pub end: &'a str,
}

impl<'a> Document<'a> {
    pub fn parse(source: &'a str) -> Self {
        let items = group_milestones(source, lex(source));
        let closers = match_spans(&items);
        let nodes = build(&items, &closers, 0, items.len());
        Self { source, nodes }
    }

    pub fn source(&self) -> &'a str {
        self.source
    }

    pub fn nodes(&self) -> &[Node<'a>] {
        &self.nodes
    }

    /// The USFM text of the document, rebuilt from its nodes.
    pub fn to_usfm(&self) -> String {
        let mut out = String::with_capacity(self.source.len());
        for node in &self.nodes {
            node.write_to(&mut out);
        }
        out
    }
}

impl fmt::Display for Document<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_usfm())
    }
}

impl<'a> Node<'a> {
    /// Append the exact source bytes of this node.
    pub fn write_to(&self, out: &mut String) {
        match self {
            Node::Bom(s) | Node::Text(s) | Node::Stray(s) => out.push_str(s),
            Node::Marker(m) => out.push_str(m.raw),
            Node::Span(span) => {
                out.push_str(span.open.raw);
                for child in &span.children {
                    child.write_to(out);
                }
                if let Some(attributes) = &span.attributes {
                    out.push_str(attributes.raw());
                }
                if let Some(close) = &span.close {
                    out.push_str(close.raw);
                }
            }
            Node::Milestone(m) => {
                out.push_str(m.marker.raw);
                if let Some(attributes) = &m.attributes {
                    out.push_str(attributes.raw());
                }
                out.push_str(m.end);
            }
        }
    }

    /// The exact source bytes of this node.
    pub fn to_usfm(&self) -> String {
        let mut out = String::new();
        self.write_to(&mut out);
        out
    }
}

impl<'a> Marker<'a> {
    /// `raw` is a marker token from the lexer: a backslash, an optional `+`, a name, and
    /// then `*`, a separator, or for `\c` and `\v` a number.
    fn from_raw(raw: &'a str) -> Self {
        let start = if raw.as_bytes().get(1) == Some(&b'+') {
            2
        } else {
            1
        };
        let name_end = raw.as_bytes()[start..]
            .iter()
            .position(|b| !(b.is_ascii_alphanumeric() || *b == b'-'))
            .map_or(raw.len(), |len| start + len);
        Self { raw, name_end }
    }

    /// The text after the backslash and the `+`, and the length of the name in it.
    fn body(&self) -> (&'a str, usize) {
        let start = if self.is_nested() { 2 } else { 1 };
        (&self.raw[start..], self.name_end - start)
    }

    /// The exact bytes of the marker, including its separator.
    pub fn raw(&self) -> &'a str {
        self.raw
    }

    /// The marker name without `\`, `+` or `*`, such as `q1` or `zaln-s`.
    pub fn name(&self) -> &'a str {
        let (body, name_len) = self.body();
        &body[..name_len]
    }

    /// True for `\+name`, the form used for character markers nested in other spans.
    pub fn is_nested(&self) -> bool {
        self.raw.as_bytes().get(1) == Some(&b'+')
    }

    /// True for `\name*`.
    pub fn is_closing(&self) -> bool {
        self.raw.as_bytes().get(self.name_end) == Some(&b'*')
    }

    /// The number written after `\c` or `\v`, such as `9-10`, exactly as written.
    pub fn number(&self) -> Option<&'a str> {
        let (body, name_len) = self.body();
        if self.is_nested() || !matches!(&body[..name_len], "c" | "v") {
            return None;
        }
        let label = body[name_len..].trim_start_matches([' ', '\t']);
        let end = label
            .find(|c: char| c.is_ascii_whitespace())
            .unwrap_or(label.len());
        Some(&label[..end]).filter(|label| !label.is_empty())
    }

    pub fn class(&self) -> MarkerClass {
        markers::classify(self.name())
    }

    /// A `\c` with a number.
    pub fn is_chapter(&self) -> bool {
        self.name() == "c" && self.number().is_some()
    }

    /// A `\v` with a number.
    pub fn is_verse(&self) -> bool {
        self.name() == "v" && self.number().is_some()
    }

    /// Paragraph-level markers, verses and chapters end every open character span.
    fn is_boundary(&self) -> bool {
        !self.is_closing() && !self.is_nested() && self.class().is_paragraph_level()
    }
}

impl<'a> Span<'a> {
    pub fn name(&self) -> &'a str {
        self.open.name()
    }
}

impl<'a> Milestone<'a> {
    pub fn name(&self) -> &'a str {
        self.marker.name()
    }
}

/// Tokens after a first pass that joins `\marker |attributes\*` into one milestone item.
enum Item<'a> {
    Bom(&'a str),
    Text(&'a str),
    Marker(Marker<'a>),
    Milestone(Milestone<'a>),
    Attributes(Attributes<'a>),
    MilestoneEnd(&'a str),
}

fn group_milestones(src: &str, tokens: Vec<Token>) -> Vec<Item<'_>> {
    let text = |t: &Token| &src[t.range.clone()];
    let mut items = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];
        match token.kind {
            TokenKind::Bom => items.push(Item::Bom(text(token))),
            TokenKind::Text => items.push(Item::Text(text(token))),
            TokenKind::Attributes => items.push(Item::Attributes(Attributes::new(text(token)))),
            TokenKind::MilestoneEnd => items.push(Item::MilestoneEnd(text(token))),
            TokenKind::Marker => {
                let marker = Marker::from_raw(text(token));
                let next = |k: usize| tokens.get(i + k).map(|t| t.kind);
                let milestone = if marker.is_closing() || marker.is_nested() {
                    None
                } else if next(1) == Some(TokenKind::MilestoneEnd) {
                    Some((None, 1))
                } else if next(1) == Some(TokenKind::Attributes)
                    && next(2) == Some(TokenKind::MilestoneEnd)
                {
                    Some((Some(Attributes::new(text(&tokens[i + 1]))), 2))
                } else {
                    None
                };
                match milestone {
                    Some((attributes, end)) => {
                        items.push(Item::Milestone(Milestone {
                            marker,
                            attributes,
                            end: text(&tokens[i + end]),
                        }));
                        i += end;
                    }
                    None => items.push(Item::Marker(marker)),
                }
            }
        }
        i += 1;
    }
    items
}

/// For each opening marker, the index of the closing marker that matches it. A closing
/// marker matches the nearest open marker of the same name; boundaries close everything.
fn match_spans(items: &[Item<'_>]) -> Vec<Option<usize>> {
    let mut closers = vec![None; items.len()];
    // Open markers: index, name and the nested flag.
    let mut open: Vec<(usize, &str, bool)> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let Item::Marker(marker) = item else { continue };
        if marker.is_boundary() {
            open.clear();
        } else if marker.is_closing() {
            let key = (marker.name(), marker.is_nested());
            if let Some(depth) = open.iter().rposition(|&(_, n, f)| (n, f) == key) {
                closers[open[depth].0] = Some(i);
                open.truncate(depth);
            }
        } else if open.len() < MAX_SPAN_DEPTH {
            open.push((i, marker.name(), marker.is_nested()));
        }
    }
    closers
}

fn build<'a>(
    items: &[Item<'a>],
    closers: &[Option<usize>],
    start: usize,
    end: usize,
) -> Vec<Node<'a>> {
    // Most spans in aligned text are `\w` words with one child; sizing their vectors from
    // the item count avoids the default first allocation of four nodes for each.
    let mut nodes = Vec::with_capacity((end - start).min(64));
    let mut i = start;
    while i < end {
        match &items[i] {
            Item::Bom(s) => nodes.push(Node::Bom(s)),
            Item::Text(s) => nodes.push(Node::Text(s)),
            Item::Attributes(a) => nodes.push(Node::Stray(a.raw())),
            Item::MilestoneEnd(s) => nodes.push(Node::Stray(s)),
            Item::Milestone(m) => nodes.push(Node::Milestone(m.clone())),
            Item::Marker(marker) => {
                let matched = closers[i]
                    .filter(|&c| c < end)
                    .and_then(|c| match &items[c] {
                        Item::Marker(close) => Some((c, *close)),
                        _ => None,
                    });
                if let Some((close, close_marker)) = matched {
                    let (children_end, attributes) = match &items[close - 1] {
                        Item::Attributes(a) if close - 1 > i => (close - 1, Some(*a)),
                        _ => (close, None),
                    };
                    nodes.push(Node::Span(Span {
                        open: *marker,
                        children: build(items, closers, i + 1, children_end),
                        attributes,
                        close: Some(close_marker),
                    }));
                    i = close + 1;
                    continue;
                }
                if opens_implicit_span(marker) {
                    // Notes do not nest, so an unclosed note also ends where the next one
                    // starts. That keeps this recursion one level deep.
                    let extent = (i + 1..end)
                        .find(|&k| {
                            matches!(&items[k], Item::Marker(m) if m.is_boundary() || opens_implicit_span(m))
                        })
                        .unwrap_or(end);
                    nodes.push(Node::Span(Span {
                        open: *marker,
                        children: build(items, closers, i + 1, extent),
                        attributes: None,
                        close: None,
                    }));
                    i = extent;
                    continue;
                }
                nodes.push(Node::Marker(*marker));
            }
        }
        i += 1;
    }
    nodes
}

fn opens_implicit_span(marker: &Marker<'_>) -> bool {
    !marker.is_closing() && !marker.is_nested() && markers::closes_implicitly(marker.name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_milestones_and_attributes_take_shape() {
        let src = "\\v 1 \\zaln-s |x-content=\"Παῦλος\"\\*\\w Paul|x-occurrence=\"1\"\\w*\\zaln-e\\*, \\f + \\ft note\\f* \\nd Lord";
        let doc = Document::parse(src);
        assert_eq!(doc.to_usfm(), src);
        let nodes = doc.nodes();
        assert!(matches!(&nodes[0], Node::Marker(m) if m.is_verse() && m.number() == Some("1")));
        assert!(
            matches!(&nodes[1], Node::Milestone(m) if m.name() == "zaln-s" && m.attributes.is_some())
        );
        let Node::Span(word) = &nodes[2] else {
            panic!("expected a word span, got {:?}", nodes[2]);
        };
        assert_eq!(word.name(), "w");
        assert_eq!(word.children, vec![Node::Text("Paul")]);
        assert_eq!(
            word.attributes.map(|a| a.raw()),
            Some("|x-occurrence=\"1\"")
        );
        assert!(matches!(&nodes[3], Node::Milestone(m) if m.name() == "zaln-e"));
        assert!(matches!(&nodes[5], Node::Span(s) if s.name() == "f" && s.close.is_some()));
        assert!(matches!(&nodes[7], Node::Marker(m) if m.name() == "nd"));
    }

    #[test]
    fn a_verse_number_ending_in_a_star_is_not_a_closing_marker() {
        let doc = Document::parse("\\v 1* text");
        let Node::Marker(verse) = &doc.nodes()[0] else {
            panic!("expected a verse marker");
        };
        assert!(!verse.is_closing());
        assert_eq!(verse.number(), Some("1*"));
        assert_eq!(doc.to_usfm(), "\\v 1* text");
    }

    #[test]
    fn unclosed_notes_end_at_the_next_paragraph() {
        let src = "a \\f + \\ft open note\n\\p\n\\v 2 b";
        let doc = Document::parse(src);
        assert_eq!(doc.to_usfm(), src);
        let Node::Span(note) = &doc.nodes()[1] else {
            panic!("expected an unclosed note");
        };
        assert!(note.close.is_none());
        assert_eq!(note.children.len(), 3, "caller text, \\ft and its text");
        assert!(matches!(&doc.nodes()[2], Node::Marker(m) if m.name() == "p"));
    }

    #[test]
    fn deep_nesting_is_capped() {
        let src = "\\+w ".repeat(10_000) + &"\\+w*".repeat(10_000);
        assert_eq!(Document::parse(&src).to_usfm(), src);
        let notes = "\\f + ".repeat(10_000) + &"\\add ".repeat(10_000);
        assert_eq!(Document::parse(&notes).to_usfm(), notes);
    }
}
