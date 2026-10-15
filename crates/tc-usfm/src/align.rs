//! Word alignment in USFM: `\zaln-s`/`\zaln-e` milestones around `\w` target words.
//!
//! The aligned form is the one unfoldingWord's en_ult and en_ust use, which is what tC4
//! exports: word-aligner's `merge` written out by usfm-js with forced new lines.
//!
//! ```text
//! \zaln-s |x-strong="G14010" x-lemma="δοῦλος" x-morph="Gr,N,,,,,NMS," x-occurrence="1" x-occurrences="1" x-content="δοῦλος"\*\w a|x-occurrence="1" x-occurrences="1"\w*
//! \w servant|x-occurrence="1" x-occurrences="1"\w*\zaln-e\*
//! ```
//!
//! # The model
//!
//! An [`AlignmentGroup`] is a chain of source words (nested `\zaln-s`, outermost first)
//! over target words. [`parse_aligned`] gives one group per *run*: a stretch of the
//! verse where the same milestones stay open over one or more `\w` words. So when one
//! source word is rendered in two places (en_ult TIT 1:1 renders ἐκλεκτῶν as "of" and,
//! in a second run right after it, as "the chosen people"), the result has two groups.
//! tC4's §5.1 record keeps only one alignment per source chain, which is why tC4's own
//! export rewrites 5 of the 46 en_ult verses of Titus; this model writes all 46 back.
//!
//! # What [`weave`] writes
//!
//! - Every target word becomes `\w word|x-occurrence="n" x-occurrences="m"\w*`, counted
//!   by [`target_words`]. Words in no group stay bare.
//! - A group's milestones open before its first word and close after its last word, with
//!   the text between its words inside them. The group is cut into several runs, each
//!   with its own copy of the milestones, where its words are not next to each other or
//!   where tC4 would cut it: markup between two words (a note, a paragraph marker, the
//!   edge of a character span), or text between them that tC4's merge splits into more
//!   than one piece. tC4 makes a piece of each punctuation token and of the whitespace
//!   or symbols before the first one, so `a b`, `a—b`, `a, b` and `2,000` stay in one
//!   run while `a {b`, `a.” b` and `a, “b` do not.
//! - Milestone attributes come in the order `x-strong`, `x-lemma`, `x-morph`,
//!   `x-occurrence`, `x-occurrences`, `x-content`, then [`SourceWord::extra`].
//! - Layout: a space (U+0020) right before a `\w` or a run's first `\zaln-s` is written
//!   as a line break, as usfm-js does. Every other byte of the plain verse is kept.
//!
//! # What [`parse_aligned`] reads back
//!
//! It undoes the above: a line break (LF not preceded by CR) right before a target
//! word or a `\zaln-s` becomes a space again, and every other byte outside the alignment
//! markup is kept. Alignment markup counts only in verse text, not inside notes or
//! headings. A `\w` is alignment markup when it sits inside a `\zaln-s` or carries
//! `x-occurrence`; any other `\w` (a glossary word, a Hebrew word with `lemma`) is
//! ordinary markup and is kept as written.
//!
//! # How exact the round trip is
//!
//! - `weave(&parse_aligned(x)?.plain, &parse_aligned(x)?.groups) == x` byte for byte for
//!   every verse of en_ult and en_ust Titus (the tests check all 92), and in general for
//!   aligned text in the layout above.
//! - For other aligned text the groups survive but bytes may differ: attributes in
//!   another order, a run that tC4 would have cut, other whitespace before a `\w`, a
//!   `\zaln-s` that closes before the `\zaln-s` nested in it, or an empty
//!   `\zaln-s…\*\zaln-e\*` pair (reported as a group without targets, which weave does
//!   not write, as tC4 does not).
//! - From the plain side, `parse_aligned(&weave(p, groups)?)?.plain == p` unless `p` has
//!   an LF directly before a word: that LF reads back as a space, so the verse text is
//!   unchanged but for that whitespace.
//!
//! [`weave`] writes the same bytes as tC4's export (word-aligner-lib 1.0.1) for the same
//! verse and record; `tests/tc4_weave.rs` compares the two on real and generated verses.
//! It differs on purpose in two places: tC4 moves the words of a character span such as
//! `\nd Lord\nd*` out of the span (leaving `\nd \nd*`), and it drops the CR of CRLF line
//! ends. Both would change the translator's text, so weave keeps the text as it is.

use crate::Error;
use crate::doc::{Document, Milestone, Node, Span};
use crate::markers;
use crate::text::{heading_after, offset_in};
use crate::words::{self, TokenKind};
use std::collections::HashMap;

/// A word of the target text: its text and its occurrence in the verse.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TargetWord {
    pub text: String,
    pub occurrence: u32,
    pub occurrences: u32,
}

/// An original-language word, as a `\zaln-s` milestone describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceWord {
    /// `x-content`: the surface form in the original-language verse.
    pub content: String,
    /// `x-strong`, such as `G14010` or `c:H1961`.
    pub strong: Option<String>,
    /// `x-lemma`.
    pub lemma: Option<String>,
    /// `x-morph`.
    pub morph: Option<String>,
    /// `x-occurrence`: which instance of `content` in the original-language verse.
    pub occurrence: u32,
    /// `x-occurrences`: how many times `content` occurs in the original-language verse.
    pub occurrences: u32,
    /// Any other attributes, in written order. [`weave`] writes them after the six above.
    pub extra: Vec<(String, String)>,
}

/// Source words aligned to target words. See the module documentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignmentGroup {
    pub sources: Vec<SourceWord>,
    pub targets: Vec<TargetWord>,
}

/// An aligned verse taken apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignedVerse {
    /// The verse with all alignment markup removed.
    pub plain: String,
    /// Every target word in order, aligned or not, as written.
    pub words: Vec<TargetWord>,
    /// The alignment groups in verse order, one per run.
    pub groups: Vec<AlignmentGroup>,
}

/// Take an aligned verse body apart into plain USFM, its target words and its groups.
///
/// Fails rather than drop anything when the alignment markup is malformed: an unbalanced
/// milestone, a `\zaln-s` without `x-content`, `x-occurrence` or `x-occurrences`, or a
/// target `\w` whose content is not plain text or whose attributes are not exactly
/// `x-occurrence` and `x-occurrences`.
pub fn parse_aligned(body: &str) -> Result<AlignedVerse, Error> {
    let doc = Document::parse(body);
    let mut strip = Strip::default();
    strip.walk(doc.nodes(), false)?;
    if !strip.open.is_empty() {
        return Err(Error::UnclosedMilestone);
    }
    Ok(AlignedVerse {
        plain: strip.plain,
        words: strip.words,
        groups: strip.groups,
    })
}

/// The verse body with all alignment markup removed. Shorthand for
/// `parse_aligned(body)?.plain`.
pub fn strip_alignment(body: &str) -> Result<String, Error> {
    Ok(parse_aligned(body)?.plain)
}

/// The target words of a plain verse body, numbered by occurrence: the words [`weave`]
/// wraps in `\w`. They are the words of [`crate::words`] in verse text, outside notes,
/// figures and headings (see [`crate::text`]).
pub fn target_words(plain: &str) -> Result<Vec<TargetWord>, Error> {
    Ok(flatten(plain)?.slots)
}

/// Write `groups` into a plain verse body. See the module documentation for the form.
///
/// Every target of every group must be a word of the verse ([`target_words`]) with the
/// same occurrence and occurrence count, and no word may belong to two groups that have
/// sources. A group without sources aligns nothing, so its words stay bare; a group
/// without targets writes nothing. Fails with [`Error::AlreadyAligned`] when the verse
/// already has `\w` or `\zaln` markup in its text.
pub fn weave(plain: &str, groups: &[AlignmentGroup]) -> Result<String, Error> {
    let flat = flatten(plain)?;
    let owner = assign(&flat.slots, groups)?;
    let mut out = Output::default();
    let mut pending: Vec<Piece<'_>> = Vec::new();
    // The open run: its group, how many `\zaln-s` it opened, and its last word.
    let mut open: Option<(usize, usize, usize)> = None;
    for piece in flat.pieces {
        let Piece::Word(slot) = piece else {
            pending.push(piece);
            continue;
        };
        let group = owner[slot];
        let continues = flat.joins[slot]
            && matches!(open, Some((g, _, last)) if Some(g) == group && last + 1 == slot);
        if !continues && let Some((_, depth, _)) = open.take() {
            out.raw(&"\\zaln-e\\*".repeat(depth));
        }
        out.flush(&mut pending);
        out.item_start();
        if let (false, Some(g)) = (continues, group) {
            for source in &groups[g].sources {
                out.raw(&milestone_start(source)?);
            }
            open = Some((g, groups[g].sources.len(), slot));
        }
        let word = &flat.slots[slot];
        out.raw(&format!(
            "\\w {}|x-occurrence=\"{}\" x-occurrences=\"{}\"\\w*",
            word.text, word.occurrence, word.occurrences
        ));
        if let Some(run) = open.as_mut() {
            run.2 = slot;
        }
    }
    if let Some((_, depth, _)) = open {
        out.raw(&"\\zaln-e\\*".repeat(depth));
    }
    out.flush(&mut pending);
    Ok(out.text)
}

/// Which group owns each word slot.
fn assign(slots: &[TargetWord], groups: &[AlignmentGroup]) -> Result<Vec<Option<usize>>, Error> {
    let index: HashMap<(&str, u32), usize> = slots
        .iter()
        .enumerate()
        .map(|(i, w)| ((w.text.as_str(), w.occurrence), i))
        .collect();
    let mut owner = vec![None; slots.len()];
    for (g, group) in groups.iter().enumerate() {
        for target in &group.targets {
            let missing = || Error::TargetWordNotFound(target.clone());
            let &slot = index
                .get(&(target.text.as_str(), target.occurrence))
                .ok_or_else(missing)?;
            if slots[slot].occurrences != target.occurrences {
                return Err(missing());
            }
            if !group.sources.is_empty() && owner[slot].replace(g).is_some() {
                return Err(Error::TargetWordAlignedTwice(target.clone()));
            }
        }
    }
    Ok(owner)
}

fn milestone_start(source: &SourceWord) -> Result<String, Error> {
    let occurrence = source.occurrence.to_string();
    let occurrences = source.occurrences.to_string();
    let known = [
        ("x-strong", source.strong.as_deref()),
        ("x-lemma", source.lemma.as_deref()),
        ("x-morph", source.morph.as_deref()),
        ("x-occurrence", Some(occurrence.as_str())),
        ("x-occurrences", Some(occurrences.as_str())),
        ("x-content", Some(source.content.as_str())),
    ];
    let attributes = known
        .into_iter()
        .filter_map(|(key, value)| Some((key, value?)))
        .chain(source.extra.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    let mut out = String::from("\\zaln-s |");
    for (n, (key, value)) in attributes.enumerate() {
        let key_ok = !key.is_empty()
            && key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !key_ok || value.contains(['"', '\\']) {
            return Err(Error::UnencodableAttribute {
                key: key.to_owned(),
                value: value.to_owned(),
            });
        }
        if n > 0 {
            out.push(' ');
        }
        out.push_str(key);
        out.push_str("=\"");
        out.push_str(value);
        out.push('"');
    }
    out.push_str("\\*");
    Ok(out)
}

/// The woven text, and whether its last piece was verse text (not markup).
#[derive(Default)]
struct Output {
    text: String,
    last_is_text: bool,
}

impl Output {
    fn raw(&mut self, s: &str) {
        self.text.push_str(s);
        self.last_is_text = false;
    }

    fn flush(&mut self, pending: &mut Vec<Piece<'_>>) {
        for piece in pending.drain(..) {
            match piece {
                Piece::Text(s) => {
                    self.text.push_str(s);
                    self.last_is_text = !s.is_empty() || self.last_is_text;
                }
                Piece::Raw(s) => self.raw(s),
                Piece::Word(_) => {}
            }
        }
    }

    /// The layout rule: a space right before a word or a run becomes a line break.
    fn item_start(&mut self) {
        if self.last_is_text && self.text.ends_with(' ') {
            self.text.pop();
            self.text.push('\n');
        }
    }
}

enum Piece<'a> {
    Text(&'a str),
    Raw(&'a str),
    Word(usize),
}

/// A plain verse body as text, markup and word slots, in order.
struct Flat<'a> {
    pieces: Vec<Piece<'a>>,
    slots: Vec<TargetWord>,
    /// Whether word `i` may continue a run that ends with word `i - 1`: only text that
    /// tC4 keeps as one piece stands between them.
    joins: Vec<bool>,
}

/// What stands between the last word and the current position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gap {
    /// Markup.
    Markup,
    /// Only text, which tC4's merge cuts into this many pieces.
    Text(u32),
}

fn flatten(plain: &str) -> Result<Flat<'_>, Error> {
    let doc = Document::parse(plain);
    let mut flattener = Flattener {
        plain,
        pieces: Vec::new(),
        words: Vec::new(),
        joins: Vec::new(),
        in_heading: false,
        gap: Gap::Markup,
    };
    flattener.nodes(doc.nodes())?;
    let Flattener {
        pieces,
        words,
        joins,
        ..
    } = flattener;
    let slots = words::number_occurrences(words)
        .into_iter()
        .map(|w| TargetWord {
            text: w.text.to_owned(),
            occurrence: w.occurrence,
            occurrences: w.occurrences,
        })
        .collect();
    Ok(Flat {
        pieces,
        slots,
        joins,
    })
}

struct Flattener<'a> {
    plain: &'a str,
    pieces: Vec<Piece<'a>>,
    words: Vec<(&'a str, usize)>,
    joins: Vec<bool>,
    in_heading: bool,
    gap: Gap,
}

impl<'a> Flattener<'a> {
    fn nodes(&mut self, nodes: &[Node<'a>]) -> Result<(), Error> {
        for node in nodes {
            match node {
                Node::Text(text) if !self.in_heading => self.text(text),
                Node::Text(text) => self.pieces.push(Piece::Text(text)),
                Node::Marker(marker) => {
                    self.in_heading = heading_after(marker, self.in_heading);
                    self.raw(marker.raw());
                }
                Node::Span(span) if is_word_span(span) && !self.in_heading => {
                    return Err(Error::AlreadyAligned);
                }
                Node::Span(span) if !self.in_heading && !markers::is_non_text_span(span.name()) => {
                    self.raw(span.open.raw());
                    self.nodes(&span.children)?;
                    self.span_end(span);
                }
                Node::Milestone(m) if is_zaln(m) && !self.in_heading => {
                    return Err(Error::AlreadyAligned);
                }
                other => self.leaves(other),
            }
        }
        Ok(())
    }

    /// Verse text: its words become slots, the rest stays text.
    fn text(&mut self, text: &'a str) {
        let mut gap_start = 0;
        for token in words::classify(text) {
            match token.kind {
                TokenKind::Word | TokenKind::Number => {
                    self.pieces.push(Piece::Text(&text[gap_start..token.start]));
                    self.pieces.push(Piece::Word(self.words.len()));
                    self.words
                        .push((token.text, offset_in(self.plain, token.text)));
                    self.joins.push(matches!(self.gap, Gap::Text(0 | 1)));
                    self.gap = Gap::Text(0);
                    gap_start = token.start + token.text.len();
                }
                // tC4 makes a piece of every punctuation token, and of the whitespace and
                // symbols right after a word; whitespace after punctuation joins its piece.
                TokenKind::Punctuation => {
                    if let Gap::Text(n) = &mut self.gap {
                        *n += 1;
                    }
                }
                TokenKind::Whitespace | TokenKind::Other => {
                    if self.gap == Gap::Text(0) {
                        self.gap = Gap::Text(1);
                    }
                }
            }
        }
        self.pieces.push(Piece::Text(&text[gap_start..]));
    }

    fn raw(&mut self, s: &'a str) {
        self.pieces.push(Piece::Raw(s));
        self.gap = Gap::Markup;
    }

    fn span_end(&mut self, span: &Span<'a>) {
        if let Some(attributes) = &span.attributes {
            self.raw(attributes.raw());
        }
        if let Some(close) = &span.close {
            self.raw(close.raw());
        }
    }

    /// Copy a node verbatim, leaf by leaf, without looking for words.
    fn leaves(&mut self, node: &Node<'a>) {
        match node {
            Node::Bom(s) | Node::Stray(s) => self.raw(s),
            Node::Text(s) => {
                self.pieces.push(Piece::Text(s));
                self.gap = Gap::Markup;
            }
            Node::Marker(m) => self.raw(m.raw()),
            Node::Span(span) => {
                self.raw(span.open.raw());
                for child in &span.children {
                    self.leaves(child);
                }
                self.span_end(span);
            }
            Node::Milestone(m) => {
                self.raw(m.marker.raw());
                if let Some(attributes) = &m.attributes {
                    self.raw(attributes.raw());
                }
                self.raw(m.end);
            }
        }
    }
}

fn is_word_span(span: &Span<'_>) -> bool {
    span.name() == "w" && !span.open.is_nested() && span.close.is_some()
}

fn is_zaln(m: &Milestone<'_>) -> bool {
    m.name() == "zaln-s" || m.name() == "zaln-e"
}

/// State of [`parse_aligned`]'s walk.
#[derive(Default)]
struct Strip {
    plain: String,
    last_is_text: bool,
    in_heading: bool,
    /// Open `\zaln-s` milestones: an id, the source word, and whether any word sits inside.
    open: Vec<(usize, SourceWord, bool)>,
    next_id: usize,
    words: Vec<TargetWord>,
    groups: Vec<AlignmentGroup>,
    /// The milestone ids above the last word, and the group that word went into.
    current: Option<(Vec<usize>, usize)>,
}

impl Strip {
    /// Alignment markup counts only where [`weave`] would put it: in verse text, not in
    /// notes or headings. Anywhere else it is copied as it is.
    fn walk(&mut self, nodes: &[Node<'_>], in_non_text: bool) -> Result<(), Error> {
        for node in nodes {
            let eligible = !in_non_text && !self.in_heading;
            match node {
                Node::Text(text) => {
                    self.plain.push_str(text);
                    self.last_is_text = true;
                }
                Node::Marker(marker) => {
                    self.in_heading = heading_after(marker, self.in_heading);
                    self.raw(marker.raw());
                }
                Node::Milestone(m) if is_zaln(m) && eligible => {
                    if m.name() == "zaln-s" {
                        self.item_start();
                        let source = source_word(m)?;
                        self.open.push((self.next_id, source, false));
                        self.next_id += 1;
                    } else {
                        let (_, source, has_words) =
                            self.open.pop().ok_or(Error::UnbalancedMilestone)?;
                        if !has_words {
                            self.groups.push(AlignmentGroup {
                                sources: vec![source],
                                targets: Vec::new(),
                            });
                        }
                        self.current = None;
                    }
                    self.last_is_text = false;
                }
                Node::Span(span) if eligible && self.is_target_word(span) => {
                    self.item_start();
                    let word = target_word(span)?;
                    self.plain.push_str(&word.text);
                    self.last_is_text = false;
                    self.add_word(word);
                }
                Node::Span(span) => {
                    self.raw(span.open.raw());
                    let non_text = in_non_text || markers::is_non_text_span(span.name());
                    self.walk(&span.children, non_text)?;
                    if let Some(attributes) = &span.attributes {
                        self.raw(attributes.raw());
                    }
                    if let Some(close) = &span.close {
                        self.raw(close.raw());
                    }
                }
                other => self.raw(&other.to_usfm()),
            }
        }
        Ok(())
    }

    /// A `\w` inside a `\zaln-s`, or one that carries `x-occurrence`, is a target word.
    /// Any other `\w` is a glossary word or an original-language word, not alignment.
    fn is_target_word(&self, span: &Span<'_>) -> bool {
        if !is_word_span(span) {
            return false;
        }
        !self.open.is_empty()
            || span.attributes.is_some_and(|a| {
                a.parse()
                    .is_ok_and(|list| list.iter().any(|a| a.key == Some("x-occurrence")))
            })
    }

    fn raw(&mut self, s: &str) {
        self.plain.push_str(s);
        self.last_is_text = false;
    }

    /// The inverse of the layout rule: a line break right before a word or a run is a
    /// space. A CRLF line end was never a space, so it stays.
    fn item_start(&mut self) {
        if self.last_is_text && self.plain.ends_with('\n') && !self.plain.ends_with("\r\n") {
            self.plain.pop();
            self.plain.push(' ');
        }
    }

    fn add_word(&mut self, word: TargetWord) {
        self.words.push(word.clone());
        if self.open.is_empty() {
            self.current = None;
            return;
        }
        let ids: Vec<usize> = self.open.iter().map(|(id, _, _)| *id).collect();
        for entry in &mut self.open {
            entry.2 = true;
        }
        if let Some((current_ids, group)) = &self.current
            && *current_ids == ids
        {
            self.groups[*group].targets.push(word);
            return;
        }
        self.groups.push(AlignmentGroup {
            sources: self.open.iter().map(|(_, s, _)| s.clone()).collect(),
            targets: vec![word],
        });
        self.current = Some((ids, self.groups.len() - 1));
    }
}

fn target_word(span: &Span<'_>) -> Result<TargetWord, Error> {
    let bad = || Error::InvalidAlignedWord(Node::Span(span.clone()).to_usfm());
    let [Node::Text(text)] = span.children.as_slice() else {
        return Err(bad());
    };
    let attributes = span.attributes.ok_or_else(bad)?.parse()?;
    let mut occurrence = None;
    let mut occurrences = None;
    for attribute in attributes {
        match attribute.key {
            Some("x-occurrence") => occurrence = Some(count(attribute.value).ok_or_else(bad)?),
            Some("x-occurrences") => occurrences = Some(count(attribute.value).ok_or_else(bad)?),
            _ => return Err(bad()),
        }
    }
    match (occurrence, occurrences) {
        (Some(occurrence), Some(occurrences)) if occurrence <= occurrences => Ok(TargetWord {
            text: (*text).to_owned(),
            occurrence,
            occurrences,
        }),
        _ => Err(bad()),
    }
}

fn source_word(m: &Milestone<'_>) -> Result<SourceWord, Error> {
    let bad = || Error::InvalidSourceWord(Node::Milestone(m.clone()).to_usfm());
    let attributes = m.attributes.ok_or_else(bad)?.parse()?;
    let mut word = SourceWord {
        content: String::new(),
        strong: None,
        lemma: None,
        morph: None,
        occurrence: 0,
        occurrences: 0,
        extra: Vec::new(),
    };
    let mut content = None;
    for attribute in attributes {
        let value = attribute.value.to_owned();
        match attribute.key.ok_or_else(bad)? {
            "x-content" => content = Some(value),
            "x-strong" => word.strong = Some(value),
            "x-lemma" => word.lemma = Some(value),
            "x-morph" => word.morph = Some(value),
            "x-occurrence" => word.occurrence = count(&value).ok_or_else(bad)?,
            "x-occurrences" => word.occurrences = count(&value).ok_or_else(bad)?,
            key => word.extra.push((key.to_owned(), value)),
        }
    }
    word.content = content.ok_or_else(bad)?;
    if word.occurrence == 0 || word.occurrence > word.occurrences {
        return Err(bad());
    }
    Ok(word)
}

/// A positive count written in plain ASCII digits.
fn count(value: &str) -> Option<u32> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    value.parse().ok().filter(|&n| n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(content: &str, occurrence: u32) -> SourceWord {
        SourceWord {
            content: content.to_owned(),
            strong: Some("G0001".to_owned()),
            lemma: None,
            morph: None,
            occurrence,
            occurrences: 1,
            extra: Vec::new(),
        }
    }

    fn target(text: &str, occurrence: u32, occurrences: u32) -> TargetWord {
        TargetWord {
            text: text.to_owned(),
            occurrence,
            occurrences,
        }
    }

    fn one(sources: &[&str], targets: &[(&str, u32, u32)]) -> AlignmentGroup {
        AlignmentGroup {
            sources: sources.iter().map(|s| source(s, 1)).collect(),
            targets: targets.iter().map(|&(t, n, m)| target(t, n, m)).collect(),
        }
    }

    /// The woven text with every milestone shortened to `<content>` and `</>`, and every
    /// word to `[word]`, so that a test can show the shape.
    fn shape(woven: &str) -> String {
        let mut out = String::new();
        let doc = Document::parse(woven);
        fn walk(nodes: &[Node<'_>], out: &mut String) {
            for node in nodes {
                match node {
                    Node::Milestone(m) if m.name() == "zaln-s" => {
                        let content = m.attributes.unwrap().get("x-content").unwrap().unwrap();
                        out.push_str(&format!("<{content}>"));
                    }
                    Node::Milestone(m) if m.name() == "zaln-e" => out.push_str("</>"),
                    Node::Span(s) if s.name() == "w" => {
                        out.push('[');
                        walk(&s.children, out);
                        out.push(']');
                    }
                    Node::Span(s) => {
                        out.push_str(s.open.raw());
                        walk(&s.children, out);
                        out.push_str(s.close.map_or("", |c| c.raw()));
                    }
                    other => out.push_str(&other.to_usfm()),
                }
            }
        }
        walk(doc.nodes(), &mut out);
        out
    }

    #[test]
    fn weave_follows_the_line_break_convention() {
        let groups = vec![
            one(&["α"], &[("a", 1, 1), ("servant", 1, 1)]),
            one(&["β", "γ"], &[("of", 1, 1)]),
        ];
        let woven = weave("a servant, of God.\n", &groups).unwrap();
        assert_eq!(
            woven,
            "\\zaln-s |x-strong=\"G0001\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"α\"\\*\\w a|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\n\
             \\w servant|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\\zaln-e\\*,\n\
             \\zaln-s |x-strong=\"G0001\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"β\"\\*\\zaln-s |x-strong=\"G0001\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"γ\"\\*\\w of|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\\zaln-e\\*\\zaln-e\\*\n\
             \\w God|x-occurrence=\"1\" x-occurrences=\"1\"\\w*.\n"
        );
        let parsed = parse_aligned(&woven).unwrap();
        assert_eq!(parsed.plain, "a servant, of God.\n");
        assert_eq!(parsed.groups, groups);
        assert_eq!(parsed.words.len(), 4);
    }

    #[test]
    fn runs_are_cut_where_tc4_cuts_them() {
        let all = |plain: &str| {
            let words: Vec<(String, u32, u32)> = target_words(plain)
                .unwrap()
                .into_iter()
                .map(|w| (w.text, w.occurrence, w.occurrences))
                .collect();
            let targets: Vec<(&str, u32, u32)> =
                words.iter().map(|(t, n, m)| (t.as_str(), *n, *m)).collect();
            shape(&weave(plain, &[one(&["α"], &targets)]).unwrap())
        };
        // One piece of text between the words: one run.
        assert_eq!(all("a b"), "<α>[a]\n[b]</>");
        assert_eq!(all("a, b"), "<α>[a],\n[b]</>");
        assert_eq!(all("a—b"), "<α>[a]—[b]</>");
        assert_eq!(all("2,000"), "<α>[2],[000]</>");
        assert_eq!(all("a $ b"), "<α>[a] $\n[b]</>");
        // Two pieces or markup between them: a new run with the same milestones.
        assert_eq!(all("a {b}"), "<α>[a]</> {<α>[b]</>}");
        assert_eq!(all("a.” b"), "<α>[a]</>.”\n<α>[b]</>");
        assert_eq!(
            all("a \\f + \\ft x\\f* b"),
            "<α>[a]</> \\f + \\ft x\\f*\n<α>[b]</>"
        );
        assert_eq!(all("a\n\\q2 b"), "<α>[a]</>\n\\q2 <α>[b]</>");
        assert_eq!(
            all("a \\nd b c\\nd* d"),
            "<α>[a]</> \\nd <α>[b]\n[c]</>\\nd*\n<α>[d]</>"
        );
    }

    #[test]
    fn adjacent_runs_of_one_source_word_stay_apart() {
        let text = "\\zaln-s |x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"α\"\\*\\w a|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\\zaln-e\\*\n\\zaln-s |x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"α\"\\*\\w b|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\\zaln-e\\*";
        let parsed = parse_aligned(text).unwrap();
        assert_eq!(parsed.groups.len(), 2);
        assert_eq!(weave(&parsed.plain, &parsed.groups).unwrap(), text);
    }

    #[test]
    fn crlf_and_glossary_words_survive_a_strip() {
        let woven = weave("a,\r\nb", &[one(&["α"], &[("a", 1, 1), ("b", 1, 1)])]).unwrap();
        assert_eq!(shape(&woven), "<α>[a],\r\n[b]</>");
        assert_eq!(strip_alignment(&woven).unwrap(), "a,\r\nb");
        let hebrew = "\\w דְּבַר|lemma=\"דָּבָר\" strong=\"H1697\"\\w*־\\w יְהוָ֔ה|lemma=\"יְהֹוָה\"\\w*";
        assert_eq!(strip_alignment(hebrew).unwrap(), hebrew);
        assert_eq!(weave(hebrew, &[]), Err(Error::AlreadyAligned));
    }

    #[test]
    fn weave_refuses_words_that_are_not_in_the_verse() {
        let group = |t: TargetWord| AlignmentGroup {
            sources: vec![source("α", 1)],
            targets: vec![t],
        };
        assert_eq!(
            weave("a b", &[group(target("c", 1, 1))]),
            Err(Error::TargetWordNotFound(target("c", 1, 1)))
        );
        assert_eq!(
            weave("a b a", &[group(target("a", 1, 1))]),
            Err(Error::TargetWordNotFound(target("a", 1, 1)))
        );
        assert_eq!(
            weave("a b", &[group(target("a", 1, 1)), group(target("a", 1, 1))]),
            Err(Error::TargetWordAlignedTwice(target("a", 1, 1)))
        );
        let sourceless = AlignmentGroup {
            sources: Vec::new(),
            targets: vec![target("z", 1, 1)],
        };
        assert_eq!(
            weave("a b", &[sourceless]),
            Err(Error::TargetWordNotFound(target("z", 1, 1)))
        );
        assert_eq!(
            weave("\\w a|x-occurrence=\"1\" x-occurrences=\"1\"\\w*", &[]),
            Err(Error::AlreadyAligned)
        );
    }

    #[test]
    fn malformed_alignment_is_refused() {
        assert_eq!(
            parse_aligned("\\zaln-e\\*"),
            Err(Error::UnbalancedMilestone)
        );
        assert_eq!(
            parse_aligned("\\zaln-s |x-content=\"α\" x-occurrence=\"1\" x-occurrences=\"1\"\\*"),
            Err(Error::UnclosedMilestone)
        );
        assert!(matches!(
            parse_aligned("\\zaln-s |x-occurrence=\"1\" x-occurrences=\"1\"\\*\\zaln-e\\*"),
            Err(Error::InvalidSourceWord(_))
        ));
        assert!(matches!(
            parse_aligned("\\w a|x-occurrence=\"2\" x-occurrences=\"1\"\\w*"),
            Err(Error::InvalidAlignedWord(_))
        ));
        assert!(matches!(
            parse_aligned("\\w a|lemma=\"a\" x-occurrence=\"1\" x-occurrences=\"1\"\\w*"),
            Err(Error::InvalidAlignedWord(_))
        ));
        let inside = "\\zaln-s |x-content=\"α\" x-occurrence=\"1\" x-occurrences=\"1\"\\*\\w a\\w*\\zaln-e\\*";
        assert!(matches!(
            parse_aligned(inside),
            Err(Error::InvalidAlignedWord(_))
        ));
    }
}
