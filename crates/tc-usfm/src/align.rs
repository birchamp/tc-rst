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
//! markup is kept. A `\w` is alignment markup when it sits inside a `\zaln-s` or carries
//! `x-occurrence`; any other `\w` (a glossary word, a Hebrew word with `lemma`) is
//! ordinary markup and is kept as written.
//!
//! Where alignment markup may stand:
//!
//! - In verse text it is read and stripped. That includes the text of the headings
//!   usfm-js shows as verse text, `\d`, `\sp`, `\qa` and `\lit`
//!   ([`crate::markers::shows_as_verse_text`]): en_ult and en_ust write a Psalm's
//!   superscription inside verse 1 as `\v 1 \d \zaln-s …`, tC4 counts and aligns its words
//!   with the verse's, and its export writes them back the same way.
//!   `tests/superscriptions.rs` checks this heading rule against tC4's export.
//! - A note, figure or other non-text span (`\f`, `\x`, `\fig`, `\ca`…) is a text of its
//!   own: it is kept whole, as tC4 keeps it, and [`weave`] copies it whole. So a
//!   `\+w covenant|x-occurrence="1" x-occurrences="1"\+w*` inside a footnote stays there.
//! - In any other heading (`\s`, `\ms`, `\r`, `\rem`…) or in a sidebar inside the body it
//!   is refused with [`Error::AlignmentOutsideVerseText`]: usfm-js does not show that text
//!   as verse text, tC4's export cannot merge an alignment into it, and weave could not
//!   write it back.
//! - Broken word markup outside notes (a `\w` without `\w*`, a `\w*` that closes nothing,
//!   a `\zaln-s` or `\zaln-e` without its `\*`) is refused with
//!   [`Error::BrokenWordMarkup`], and an aligned word written as a nested `\+w` with
//!   [`Error::InvalidAlignedWord`], rather than leave markup or attribute text in the plain
//!   verse.
//!
//! The occurrence numbers written on the `\w` words are checked against the plain verse.
//! An aligned word is found by where it stands, so a written number that disagrees with
//! the count (text edited outside the app, or a bare word next to aligned ones) is
//! corrected and reported in [`AlignedVerse::renumbered`]. A `\w` whose text is not
//! exactly one word of the plain verse (`\w a-b\w*`, or two `\w` with nothing between
//! them) cannot be placed and fails with [`Error::NotOneWord`].
//!
//! # How exact the round trip is
//!
//! - `weave(&parse_aligned(x)?.plain, &parse_aligned(x)?.groups) == x` byte for byte for
//!   every verse of en_ult and en_ust Titus (the tests check all 92), for every Psalm
//!   superscription verse of the ULT and UST v91, and in general for aligned text in the
//!   layout above.
//! - For other aligned text the groups survive but bytes may differ: attributes in
//!   another order, a run that tC4 would have cut, a `\zaln-e` after the punctuation that
//!   follows its last word, other whitespace before a `\w`, a `\zaln-s` that closes
//!   before the `\zaln-s` nested in it, renumbered occurrences, words that were bare and
//!   are now wrapped in `\w`, or an empty `\zaln-s…\*\zaln-e\*` pair (reported as a group
//!   without targets, which weave does not write, as tC4 does not).
//! - A verse that also holds non-alignment `\w` markup (a glossary or Strong's word)
//!   outside notes reads apart, but cannot be woven: see [`weave`].
//! - From the plain side, `parse_aligned(&weave(p, groups)?)?.plain == p` unless `p` has
//!   an LF directly before a word: that LF reads back as a space, so the verse text is
//!   unchanged but for that whitespace. A space after a lone CR is kept as a space, so
//!   that it cannot read back as a CRLF line end.
//!
//! [`weave`] writes the same bytes as tC4's export (word-aligner-lib 1.0.1) for the same
//! verse and record; `tests/tc4_weave.rs` compares the two on real and generated verses.
//! It differs on purpose in two places: tC4 moves the words of a character span such as
//! `\nd Lord\nd*` out of the span (leaving `\nd \nd*`), and it drops the CR of CRLF line
//! ends. Both would change the translator's text, so weave keeps the text as it is.

use crate::Error;
use crate::doc::{Document, Milestone, Node, Span};
use crate::markers;
use crate::text::{self, Flow, offset_in};
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
    /// Every word of the verse text of `plain` in order, aligned or not, numbered by
    /// occurrence: the same list as `target_words(&plain)` whenever that succeeds.
    pub words: Vec<TargetWord>,
    /// The alignment groups in verse order, one per run. Their targets carry the numbers
    /// of `words`.
    pub groups: Vec<AlignmentGroup>,
    /// Aligned words whose written `x-occurrence` or `x-occurrences` disagree with the
    /// count in `plain`, as (written, counted), in verse order. Empty for text that tC4 or
    /// [`weave`] wrote and nobody edited since.
    pub renumbered: Vec<(TargetWord, TargetWord)>,
}

/// Take an aligned verse body apart into plain USFM, its words and its groups.
///
/// Fails rather than drop or misread anything when the alignment markup is malformed:
/// an unbalanced milestone, a `\zaln-s` without `x-content`, `x-occurrence` or
/// `x-occurrences` or with an attribute twice, a target `\w` whose content is not plain
/// text or whose attributes are not exactly `x-occurrence` and `x-occurrences`, broken
/// `\w` or `\zaln` markup, alignment in a heading or sidebar, or an aligned word that is
/// not one word of the plain verse. See the module documentation.
pub fn parse_aligned(body: &str) -> Result<AlignedVerse, Error> {
    let doc = Document::parse(body);
    let mut strip = Strip::default();
    strip.walk(doc.nodes())?;
    if !strip.open.is_empty() {
        return Err(Error::UnclosedMilestone);
    }
    strip.finish()
}

/// The verse body with all alignment markup removed. Shorthand for
/// `parse_aligned(body)?.plain`.
pub fn strip_alignment(body: &str) -> Result<String, Error> {
    Ok(parse_aligned(body)?.plain)
}

/// The target words of a plain verse body, numbered by occurrence: the words [`weave`]
/// wraps in `\w`. They are the words of [`crate::words`] in verse text, outside notes,
/// figures, headings and sidebars (see [`crate::text`]). Fails like [`weave`] when the
/// verse has word markup.
pub fn target_words(plain: &str) -> Result<Vec<TargetWord>, Error> {
    Ok(flatten(plain)?.slots)
}

/// Write `groups` into a plain verse body. See the module documentation for the form.
///
/// Every target of every group must be a word of the verse ([`target_words`]) with the
/// same occurrence and occurrence count, and no word may belong to two groups that have
/// sources. A group without sources aligns nothing, so its words stay bare; a group
/// without targets writes nothing. Each source word that is written needs an occurrence
/// from 1 to its count and attributes that read back the same.
///
/// A verse with any `\w` or `\zaln` markup outside notes fails with
/// [`Error::WordMarkup`]: already aligned text, and also glossary or Strong's words such
/// as `\w grace|strong="G5485"\w*`. weave aligns only verses without word markup, because
/// it writes every word as `\w` and its own words could not be told apart from those.
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

/// The six attributes [`SourceWord`] names, in the order weave writes them.
const KNOWN_KEYS: [&str; 6] = [
    "x-strong",
    "x-lemma",
    "x-morph",
    "x-occurrence",
    "x-occurrences",
    "x-content",
];

/// The `\zaln-s` for `source`. It refuses what [`parse_aligned`] would refuse or read
/// differently, so that every milestone weave writes reads back as the same source word.
fn milestone_start(source: &SourceWord) -> Result<String, Error> {
    if source.occurrence == 0 || source.occurrence > source.occurrences {
        return Err(Error::InvalidOccurrence {
            content: source.content.clone(),
            occurrence: source.occurrence,
            occurrences: source.occurrences,
        });
    }
    let unencodable = |key: &str, value: &str| Error::UnencodableAttribute {
        key: key.to_owned(),
        value: value.to_owned(),
    };
    for (n, (key, value)) in source.extra.iter().enumerate() {
        // An extra attribute named like one of the six would be read back into that field.
        if KNOWN_KEYS.contains(&key.as_str()) || source.extra[..n].iter().any(|(k, _)| k == key) {
            return Err(unencodable(key, value));
        }
    }
    let occurrence = source.occurrence.to_string();
    let occurrences = source.occurrences.to_string();
    let known = [
        source.strong.as_deref(),
        source.lemma.as_deref(),
        source.morph.as_deref(),
        Some(occurrence.as_str()),
        Some(occurrences.as_str()),
        Some(source.content.as_str()),
    ];
    let attributes = KNOWN_KEYS
        .into_iter()
        .zip(known)
        .filter_map(|(key, value)| Some((key, value?)))
        .chain(source.extra.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    let mut out = String::from("\\zaln-s |");
    for (n, (key, value)) in attributes.enumerate() {
        let key_ok = !key.is_empty()
            && key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !key_ok || value.contains(['"', '\\']) {
            return Err(unencodable(key, value));
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

    /// The layout rule: a space right before a word or a run becomes a line break. After
    /// a lone CR it stays a space, since CR and LF would read back as one CRLF line end.
    fn item_start(&mut self) {
        if self.last_is_text && self.text.ends_with(' ') && !self.text.ends_with("\r ") {
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
        flow: Flow::default(),
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
    flow: Flow,
    gap: Gap,
}

impl<'a> Flattener<'a> {
    fn nodes(&mut self, nodes: &[Node<'a>]) -> Result<(), Error> {
        for node in nodes {
            match node {
                // Notes are copied whole, whatever markup they hold.
                Node::Span(span) if markers::is_non_text_span(span.name()) => self.leaves(node),
                // Any word markup outside notes, closed or not, in verse text or not: weave
                // never reads markup or attribute text as words.
                Node::Marker(m) if is_word_markup(m.name()) => return Err(Error::WordMarkup),
                Node::Span(span) if span.name() == "w" => return Err(Error::WordMarkup),
                Node::Milestone(m) if is_zaln(m) => return Err(Error::WordMarkup),
                Node::Marker(marker) => {
                    self.flow = self.flow.after(marker);
                    self.raw(marker.raw());
                }
                Node::Text(text) if self.flow.is_verse_text() => self.text(text),
                // A span in a heading is walked too, so that word markup in it is refused;
                // its text is copied, not tokenized.
                Node::Span(span) => {
                    self.raw(span.open.raw());
                    self.nodes(&span.children)?;
                    self.span_end(span);
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

/// `\w`, `\+w` and the `\zaln` milestones, in whatever form.
fn is_word_markup(name: &str) -> bool {
    matches!(name, "w" | "zaln-s" | "zaln-e")
}

fn is_zaln(m: &Milestone<'_>) -> bool {
    m.name() == "zaln-s" || m.name() == "zaln-e"
}

/// State of [`parse_aligned`]'s walk.
#[derive(Default)]
struct Strip {
    plain: String,
    last_is_text: bool,
    flow: Flow,
    /// Open `\zaln-s` milestones: an id, the source word, and whether any word sits inside.
    open: Vec<(usize, SourceWord, bool)>,
    next_id: usize,
    /// Every aligned word as written, with where its text starts in `plain`.
    written: Vec<(TargetWord, usize)>,
    /// Groups as their sources and the indices of their words in `written`.
    groups: Vec<(Vec<SourceWord>, Vec<usize>)>,
    /// The milestone ids above the last word, and the group that word went into.
    current: Option<(Vec<usize>, usize)>,
}

impl Strip {
    fn walk(&mut self, nodes: &[Node<'_>]) -> Result<(), Error> {
        for node in nodes {
            match node {
                Node::Text(text) => {
                    self.plain.push_str(text);
                    self.last_is_text = true;
                }
                // A note is a text of its own and is kept whole. See the module documentation.
                Node::Span(span) if markers::is_non_text_span(span.name()) => {
                    self.raw(&node.to_usfm());
                }
                Node::Marker(marker) if is_word_markup(marker.name()) => {
                    return Err(Error::BrokenWordMarkup(marker.raw().to_owned()));
                }
                Node::Marker(marker) => {
                    self.flow = self.flow.after(marker);
                    self.raw(marker.raw());
                }
                Node::Milestone(m) if is_zaln(m) => {
                    self.require_verse_text(node)?;
                    if m.name() == "zaln-s" {
                        self.item_start();
                        let source = source_word(m)?;
                        self.open.push((self.next_id, source, false));
                        self.next_id += 1;
                    } else {
                        let (_, source, has_words) =
                            self.open.pop().ok_or(Error::UnbalancedMilestone)?;
                        if !has_words {
                            self.groups.push((vec![source], Vec::new()));
                        }
                        self.current = None;
                    }
                    self.last_is_text = false;
                }
                Node::Span(span) if span.name() == "w" && self.is_aligned_word(span) => {
                    self.require_verse_text(node)?;
                    let word = target_word(span)?;
                    self.item_start();
                    let start = self.plain.len();
                    self.plain.push_str(&word.text);
                    self.last_is_text = false;
                    self.add_word(word, start);
                }
                Node::Span(span) => {
                    self.raw(span.open.raw());
                    self.walk(&span.children)?;
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
    fn is_aligned_word(&self, span: &Span<'_>) -> bool {
        !self.open.is_empty()
            || span
                .attributes
                .is_some_and(|a| a.raw().contains("x-occurrence"))
    }

    fn require_verse_text(&self, node: &Node<'_>) -> Result<(), Error> {
        if self.flow.is_verse_text() {
            Ok(())
        } else {
            Err(Error::AlignmentOutsideVerseText(node.to_usfm()))
        }
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

    fn add_word(&mut self, word: TargetWord, start: usize) {
        let index = self.written.len();
        self.written.push((word, start));
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
            self.groups[*group].1.push(index);
            return;
        }
        let sources = self.open.iter().map(|(_, s, _)| s.clone()).collect();
        self.groups.push((sources, vec![index]));
        self.current = Some((ids, self.groups.len() - 1));
    }

    /// Number every word of the plain verse, and give each aligned word the number of the
    /// word that stands where it stood.
    fn finish(self) -> Result<AlignedVerse, Error> {
        let counted = text::verse_words(&self.plain);
        let by_start: HashMap<usize, usize> = counted
            .iter()
            .enumerate()
            .map(|(i, w)| (w.start, i))
            .collect();
        let words: Vec<TargetWord> = counted
            .iter()
            .map(|w| TargetWord {
                text: w.text.to_owned(),
                occurrence: w.occurrence,
                occurrences: w.occurrences,
            })
            .collect();
        let mut placed = Vec::with_capacity(self.written.len());
        let mut renumbered = Vec::new();
        for (written, start) in self.written {
            let Some(&slot) = by_start
                .get(&start)
                .filter(|&&slot| words[slot].text == written.text)
            else {
                return Err(Error::NotOneWord(written));
            };
            if words[slot] != written {
                renumbered.push((written, words[slot].clone()));
            }
            placed.push(slot);
        }
        let groups = self
            .groups
            .into_iter()
            .map(|(sources, targets)| AlignmentGroup {
                sources,
                targets: targets.iter().map(|&t| words[placed[t]].clone()).collect(),
            })
            .collect();
        Ok(AlignedVerse {
            plain: self.plain,
            words,
            groups,
            renumbered,
        })
    }
}

fn target_word(span: &Span<'_>) -> Result<TargetWord, Error> {
    let bad = || Error::InvalidAlignedWord(Node::Span(span.clone()).to_usfm());
    // weave writes `\w`, so a `\+w` would not come back as written.
    if span.open.is_nested() {
        return Err(bad());
    }
    let [Node::Text(text)] = span.children.as_slice() else {
        return Err(bad());
    };
    let attributes = span.attributes.ok_or_else(bad)?.parse()?;
    let mut occurrence = None;
    let mut occurrences = None;
    for attribute in attributes {
        let field = match attribute.key {
            Some("x-occurrence") => &mut occurrence,
            Some("x-occurrences") => &mut occurrences,
            _ => return Err(bad()),
        };
        if field
            .replace(count(attribute.value).ok_or_else(bad)?)
            .is_some()
        {
            return Err(bad());
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
    let mut keys: Vec<&str> = Vec::with_capacity(attributes.len());
    for attribute in attributes {
        let key = attribute.key.ok_or_else(bad)?;
        // A repeated attribute would leave one of its values out of the record.
        if keys.contains(&key) {
            return Err(bad());
        }
        keys.push(key);
        let value = attribute.value.to_owned();
        match key {
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
        assert_eq!(weave(hebrew, &[]), Err(Error::WordMarkup));
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
            Err(Error::WordMarkup)
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

    const ZALN: &str =
        "\\zaln-s |x-strong=\"G0001\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"α\"\\*";
    const A: &str = "\\w a|x-occurrence=\"1\" x-occurrences=\"1\"\\w*";

    #[test]
    fn broken_or_nested_target_words_are_refused_not_copied() {
        // A target word without its `\w*`: the α→a link must not be lost silently.
        let unclosed = format!("{ZALN}\\w a|x-occurrence=\"1\" x-occurrences=\"1\"\\zaln-e\\* b");
        assert_eq!(
            parse_aligned(&unclosed),
            Err(Error::BrokenWordMarkup("\\w ".to_owned()))
        );
        // A target word written as a nested `\+w` inside `\add`.
        let nested = format!(
            "{ZALN}\\add \\+w a|x-occurrence=\"1\" x-occurrences=\"1\"\\+w*\\add*\\zaln-e\\*"
        );
        assert!(matches!(
            parse_aligned(&nested),
            Err(Error::InvalidAlignedWord(w)) if w.starts_with("\\+w a|")
        ));
        // A `\w*` that closes nothing, and milestones without their `\*`.
        for broken in [
            format!("{ZALN}a\\w*\\zaln-e\\*"),
            format!("{ZALN}{A}\\zaln-e"),
            "\\zaln-s |x-content=\"α\" x-occurrence=\"1\" x-occurrences=\"1\"\\w a\\w*".to_owned(),
        ] {
            assert!(
                matches!(parse_aligned(&broken), Err(Error::BrokenWordMarkup(_))),
                "{broken:?}"
            );
        }
        // A nested `\+w` that is not alignment is a glossary word and stays as written.
        let glossary = "\\add \\+w Actually|strong=\"H0376\"\\+w*, one\\add*";
        let parsed = parse_aligned(glossary).unwrap();
        assert_eq!(parsed.plain, glossary);
        assert!(parsed.groups.is_empty());
    }

    #[test]
    fn weave_never_reads_markup_as_words() {
        for plain in [
            "a \\w b",
            "a \\w b\\w* c",
            "a b\\w* c",
            "\\add \\+w a\\+w*\\add*",
            "\\s1 \\fqa \\w \\fqa*",
            "\\s1 Head \\bd x \\w y|lemma=\"y\"\\w*\\bd*\n\\p a",
            "a \\zaln-e b",
            "a \\zaln-s |x-content=\"α\"\\*b\\zaln-e\\*",
            "\\s Head \\w x|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\n\\p a",
        ] {
            assert_eq!(weave(plain, &[]), Err(Error::WordMarkup), "{plain:?}");
            assert_eq!(target_words(plain), Err(Error::WordMarkup), "{plain:?}");
        }
    }

    #[test]
    fn written_occurrences_are_checked_against_the_plain_verse() {
        // An aligned `a` next to a bare `a` was written as 1 of 1; it is 1 of 2.
        let verse = format!("{ZALN}{A}\\zaln-e\\* and a b");
        let parsed = parse_aligned(&verse).unwrap();
        assert_eq!(parsed.plain, "a and a b");
        assert_eq!(
            parsed.renumbered,
            vec![(target("a", 1, 1), target("a", 1, 2))]
        );
        assert_eq!(parsed.groups[0].targets, vec![target("a", 1, 2)]);
        assert_eq!(parsed.words, target_words(&parsed.plain).unwrap());
        let woven = weave(&parsed.plain, &parsed.groups).unwrap();
        let again = parse_aligned(&woven).unwrap();
        assert_eq!(again.plain, parsed.plain);
        assert_eq!(again.groups, parsed.groups);
        assert!(again.renumbered.is_empty());
        // Two words written as the only `a` each.
        let twice = parse_aligned(&format!("{A} {A}")).unwrap();
        assert_eq!(
            twice.renumbered,
            vec![
                (target("a", 1, 1), target("a", 1, 2)),
                (target("a", 1, 1), target("a", 2, 2))
            ]
        );
        // A `\w` that is not exactly one word of the plain verse cannot be placed.
        for verse in [
            "\\w a-b|x-occurrence=\"1\" x-occurrences=\"1\"\\w*".to_owned(),
            format!("{A}\\w b|x-occurrence=\"1\" x-occurrences=\"1\"\\w*"),
            format!("{A}bc"),
        ] {
            assert!(
                matches!(parse_aligned(&verse), Err(Error::NotOneWord(_))),
                "{verse:?}"
            );
        }
    }

    #[test]
    fn a_space_after_a_lone_carriage_return_stays_a_space() {
        for plain in ["a,\r b", "a\r b", "a \r b", "a\r\n b"] {
            let words: Vec<(&str, u32, u32)> = vec![("a", 1, 1), ("b", 1, 1)];
            for groups in [vec![], vec![one(&["α"], &words)]] {
                let woven = weave(plain, &groups).unwrap();
                assert!(!woven.contains("\r\n\\w"), "{woven:?}");
                assert_eq!(parse_aligned(&woven).unwrap().plain, plain, "{woven:?}");
            }
        }
    }

    #[test]
    fn milestones_that_would_not_read_back_are_refused() {
        let with = |occurrence, occurrences, extra: Vec<(&str, &str)>| AlignmentGroup {
            sources: vec![SourceWord {
                occurrence,
                occurrences,
                extra: extra
                    .into_iter()
                    .map(|(k, v)| (k.to_owned(), v.to_owned()))
                    .collect(),
                ..source("α", 1)
            }],
            targets: vec![target("a", 1, 1)],
        };
        for (occurrence, occurrences) in [(0, 0), (0, 1), (2, 1)] {
            assert_eq!(
                weave("a", &[with(occurrence, occurrences, vec![])]),
                Err(Error::InvalidOccurrence {
                    content: "α".to_owned(),
                    occurrence,
                    occurrences
                })
            );
        }
        for extra in [
            vec![("x-content", "β")],
            vec![("x-strong", "G2")],
            vec![("x-tw", "a"), ("x-tw", "b")],
            vec![("x tw", "a")],
            vec![("x-tw", "a\"b")],
        ] {
            assert!(
                matches!(
                    weave("a", &[with(1, 1, extra.clone())]),
                    Err(Error::UnencodableAttribute { .. })
                ),
                "{extra:?}"
            );
        }
        // An extra attribute named like a field that is empty would be read into it.
        let lemma_as_extra = AlignmentGroup {
            sources: vec![SourceWord {
                lemma: None,
                extra: vec![("x-lemma".to_owned(), "λόγος".to_owned())],
                ..source("α", 1)
            }],
            targets: vec![target("a", 1, 1)],
        };
        assert_eq!(
            weave("a", &[lemma_as_extra]),
            Err(Error::UnencodableAttribute {
                key: "x-lemma".to_owned(),
                value: "λόγος".to_owned()
            })
        );
        let fine = with(1, 2, vec![("x-tw", "rc://*/tw/dict/bible/kt/god")]);
        let woven = weave("a", std::slice::from_ref(&fine)).unwrap();
        assert_eq!(parse_aligned(&woven).unwrap().groups, vec![fine]);
    }

    #[test]
    fn repeated_attributes_are_refused_when_read() {
        let repeated = "\\zaln-s |x-strong=\"G1\" x-strong=\"G2\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"α\" x-content=\"β\"\\*\\w a|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\\zaln-e\\*";
        assert!(matches!(
            parse_aligned(repeated),
            Err(Error::InvalidSourceWord(_))
        ));
        let extra_twice = "\\zaln-s |x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"α\" x-tw=\"a\" x-tw=\"b\"\\*\\w a|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\\zaln-e\\*";
        assert!(matches!(
            parse_aligned(extra_twice),
            Err(Error::InvalidSourceWord(_))
        ));
        let word_twice = "\\w a|x-occurrence=\"1\" x-occurrence=\"1\" x-occurrences=\"1\"\\w*";
        assert!(matches!(
            parse_aligned(word_twice),
            Err(Error::InvalidAlignedWord(_))
        ));
    }

    #[test]
    fn alignment_in_a_heading_or_sidebar_is_refused() {
        let heading = format!("{ZALN}\\w A|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\\zaln-e\\*");
        for body in [
            format!("\\s {heading}\n\\q1 line"),
            format!("text\n\\s {heading}\n\\p more"),
            format!("text\n\\ms1 {heading}\n\\p more"),
            format!("text\n\\r {heading}\n\\p more"),
            // usfm-js knows `\d` by that name only.
            format!("\\d1 {heading}\n\\q1 line"),
            "text\n\\s Title \\w A|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\n\\p more".to_owned(),
            format!("text\n\\esb \\p {heading}\n\\esbe\n\\p more"),
            // A milestone opened in a heading would move onto the verse text after it.
            format!("\\s Title {ZALN}\n\\p {A}\\zaln-e\\*"),
        ] {
            assert!(
                matches!(
                    parse_aligned(&body),
                    Err(Error::AlignmentOutsideVerseText(_))
                ),
                "{body:?}"
            );
        }
        // After the heading's paragraph marker, verse text is aligned again.
        let after = format!("\\s Title\n\\q1 {heading}");
        let parsed = parse_aligned(&after).unwrap();
        assert_eq!(parsed.plain, "\\s Title\n\\q1 A");
        assert_eq!(parsed.groups.len(), 1);
    }

    /// A superscription written inside verse 1, as en_ult writes PSA 11:1, in short: it is
    /// verse text, its words are numbered with the verse's, and the verse weaves back into
    /// the same bytes.
    #[test]
    fn a_superscription_inside_a_verse_is_verse_text() {
        let zaln = |content: &str| {
            format!(
                "\\zaln-s |x-strong=\"G0001\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"{content}\"\\*"
            )
        };
        let w = |text: &str, n: u32, m: u32| {
            format!("\\w {text}|x-occurrence=\"{n}\" x-occurrences=\"{m}\"\\w*")
        };
        let verse = format!(
            "\\d {}{}\n{}\\zaln-e\\*.\n\\q1 {}{}\n{}\\zaln-e\\*.\n",
            zaln("לְ\u{2060}דָוִד"),
            w("Of", 1, 1),
            w("David", 1, 2),
            zaln("דָּוִד"),
            w("David", 2, 2),
            w("sang", 1, 1),
        );
        let parsed = parse_aligned(&verse).unwrap();
        assert_eq!(parsed.plain, "\\d Of David.\n\\q1 David sang.\n");
        assert!(parsed.renumbered.is_empty());
        assert_eq!(
            parsed.groups,
            vec![
                one(&["לְ\u{2060}דָוִד"], &[("Of", 1, 1), ("David", 1, 2)]),
                one(&["דָּוִד"], &[("David", 2, 2), ("sang", 1, 1)]),
            ]
        );
        assert_eq!(weave(&parsed.plain, &parsed.groups).unwrap(), verse);
        assert_eq!(target_words(&parsed.plain).unwrap(), parsed.words);
    }

    /// usfm-js shows the text of `\d`, `\sp`, `\qa` and `\lit` as verse text wherever it
    /// stands in a verse, and tC4's export aligns it there.
    #[test]
    fn headings_shown_as_verse_text_are_aligned_wherever_they_stand() {
        for marker in ["d", "sp", "qa", "lit"] {
            let plain = format!("a\n\\{marker} b\n\\q1 a b\n");
            let words: Vec<(&str, u32, u32)> =
                vec![("a", 1, 2), ("b", 1, 2), ("a", 2, 2), ("b", 2, 2)];
            let groups: Vec<AlignmentGroup> = words.iter().map(|&w| one(&["α"], &[w])).collect();
            let woven = weave(&plain, &groups).unwrap();
            assert_eq!(
                shape(&woven),
                format!("<α>[a]</>\n\\{marker} <α>[b]</>\n\\q1 <α>[a]</>\n<α>[b]</>\n")
            );
            let parsed = parse_aligned(&woven).unwrap();
            assert_eq!(parsed.plain, plain);
            assert_eq!(parsed.groups, groups);
        }
    }

    #[test]
    fn notes_are_kept_whole_whatever_they_hold() {
        let body = format!(
            "{ZALN}{A}\\zaln-e\\* \\f + \\ft {ZALN}\\w b|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\\zaln-e\\* and \\+w c|x-occurrence=\"1\" x-occurrences=\"1\"\\+w*\\f* d"
        );
        let parsed = parse_aligned(&body).unwrap();
        let note = &body[body.find("\\f +").unwrap()..body.find("\\f*").unwrap() + 3];
        assert_eq!(parsed.plain, format!("a {note} d"));
        assert_eq!(parsed.groups, vec![one(&["α"], &[("a", 1, 1)])]);
        assert_eq!(parsed.words, vec![target("a", 1, 1), target("d", 1, 1)]);
        let woven = weave(&parsed.plain, &parsed.groups).unwrap();
        assert!(woven.contains(note));
        assert_eq!(parse_aligned(&woven).unwrap().plain, parsed.plain);
    }

    #[test]
    fn verses_with_glossary_words_read_apart_but_do_not_weave() {
        let body = format!("{ZALN}{A}\\zaln-e\\* \\w grace|grace\\w*");
        let parsed = parse_aligned(&body).unwrap();
        assert_eq!(parsed.plain, "a \\w grace|grace\\w*");
        assert_eq!(parsed.words, vec![target("a", 1, 1), target("grace", 1, 1)]);
        assert_eq!(weave(&parsed.plain, &parsed.groups), Err(Error::WordMarkup));
    }
}
