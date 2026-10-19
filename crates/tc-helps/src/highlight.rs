//! Which words of an aligned verse render a help's quote.
//!
//! A note or word link names original-language words (its quote, with ` & ` between
//! discontinuous spans) and which occurrence of the whole quote in the verse it means. An
//! aligned verse ties each target word to the original words it renders. This module
//! finds the target words that render the quote, as tC4 does
//! (`src/data/sourceHighlight.ts`), so the gateway text can be lit and the gateway
//! rendering of the quote shown.
//!
//! The same input works for a gateway Bible's `\zaln` alignment and for a project's own
//! alignment records (top words as the sources, bottom words as the target words).
//!
//! Open Bible Stories helps quote the gateway story text itself; [`highlight_plain`]
//! finds such a quote in plain text.

use std::collections::{BTreeMap, BTreeSet};

use unicode_general_category::{GeneralCategory, get_general_category};
use unicode_normalization::UnicodeNormalization;

use crate::Error;
use crate::ids::{Occurrence, Quote};

/// One original-language word instance: its surface form and its occurrence in the verse
/// (the `x-content` and `x-occurrence` of a `\zaln-s` milestone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceWord {
    pub content: String,
    pub occurrence: u32,
}

impl SourceWord {
    pub fn new(content: impl Into<String>, occurrence: u32) -> Self {
        Self {
            content: content.into(),
            occurrence,
        }
    }
}

/// Names one alignment in an [`AlignedVerse`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AlignmentId(usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// A target word, aligned to an alignment or to nothing.
    Word(Option<AlignmentId>),
    /// Spaces, punctuation and anything else between words.
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerseToken {
    pub text: String,
    pub kind: TokenKind,
}

/// A verse as display tokens, each word tied to the original words it renders.
///
/// An alignment is one group of original words (a `\zaln-s` milestone, with every
/// milestone it is nested in). Words that render that group point at it. Nested
/// milestones are separate alignments whose sources include the outer milestone's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AlignedVerse {
    alignments: Vec<Vec<SourceWord>>,
    /// [`normalize_word`] of each source word, parallel to `alignments`: a verse normalizes
    /// each word once, not once per quote word and group it is compared with.
    keys: Vec<Vec<String>>,
    tokens: Vec<VerseToken>,
}

impl AlignedVerse {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a group of original words and returns its id.
    pub fn add_alignment(&mut self, sources: Vec<SourceWord>) -> AlignmentId {
        self.keys
            .push(sources.iter().map(|s| normalize_word(&s.content)).collect());
        self.alignments.push(sources);
        AlignmentId(self.alignments.len() - 1)
    }

    /// Adds a target word, aligned to `alignment` or unaligned.
    pub fn push_word(
        &mut self,
        text: impl Into<String>,
        alignment: Option<AlignmentId>,
    ) -> Result<(), Error> {
        if let Some(AlignmentId(i)) = alignment
            && i >= self.alignments.len()
        {
            return Err(Error::UnknownAlignment(i));
        }
        self.tokens.push(VerseToken {
            text: text.into(),
            kind: TokenKind::Word(alignment),
        });
        Ok(())
    }

    /// Adds text between words.
    pub fn push_text(&mut self, text: impl Into<String>) {
        self.tokens.push(VerseToken {
            text: text.into(),
            kind: TokenKind::Text,
        });
    }

    pub fn tokens(&self) -> &[VerseToken] {
        &self.tokens
    }

    /// The original words of an alignment; `None` for an id from another verse.
    pub fn sources(&self, alignment: AlignmentId) -> Option<&[SourceWord]> {
        self.alignments.get(alignment.0).map(Vec::as_slice)
    }

    /// The verse text, token by token.
    pub fn text(&self) -> String {
        self.tokens.iter().map(|t| t.text.as_str()).collect()
    }
}

/// How sure a highlight is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchQuality {
    /// Nothing renders the quote.
    None,
    /// The whole quote, at the occurrence asked for.
    Exact,
    /// A one-word quote whose occurrence was not found as a whole-quote match, matched
    /// through the alignment's own occurrence numbers instead.
    WordOccurrence,
    /// Every occurrence of the quote: asked for with occurrence `-1`, or the occurrence is
    /// unrecorded and the quote occurs more than once.
    AllOccurrences,
    /// The quote did not match as a whole, so every word that renders any quote word is
    /// lit. Visible, but not a confirmed rendering.
    Approximate,
}

/// The tokens that render a quote, by index into [`AlignedVerse::tokens`], ascending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Highlight {
    pub tokens: Vec<usize>,
    pub quality: MatchQuality,
}

impl Highlight {
    fn none() -> Self {
        Self {
            tokens: Vec::new(),
            quality: MatchQuality::None,
        }
    }

    /// The texts of the lit tokens, in verse order.
    pub fn texts<'a>(&self, verse: &'a AlignedVerse) -> Vec<&'a str> {
        self.tokens
            .iter()
            .filter_map(|&i| verse.tokens.get(i))
            .map(|t| t.text.as_str())
            .collect()
    }
}

fn letter_or_number(c: char) -> bool {
    use GeneralCategory::*;
    matches!(
        get_general_category(c),
        UppercaseLetter
            | LowercaseLetter
            | TitlecaseLetter
            | ModifierLetter
            | OtherLetter
            | DecimalNumber
            | LetterNumber
            | OtherNumber
    )
}

fn mark(c: char) -> bool {
    use GeneralCategory::*;
    matches!(
        get_general_category(c),
        NonspacingMark | SpacingMark | EnclosingMark
    )
}

/// NFC, then strip characters that are not letters or numbers from both ends, so the
/// quote's `Θεοῦ,` matches the alignment's `Θεοῦ`. Marks and joiners inside a word stay.
pub fn normalize_word(word: &str) -> String {
    let nfc: String = word.nfc().collect();
    nfc.trim_matches(|c: char| !letter_or_number(c)).to_owned()
}

/// The Hebrew maqaf. UHB writes it between two `\w` words (`דְּבַר־יְהוָ֔ה`), so each side is
/// its own alignment source, while quotes keep the compound as it is printed.
const MAQAF: char = '\u{05BE}';

/// The quote's words, spans flattened: `&` and `…` dropped, maqaf compounds split, each
/// word normalized.
fn quote_words(quote: &str) -> Vec<String> {
    quote
        .split(|c: char| c.is_whitespace() || c == MAQAF)
        .filter(|w| *w != "&" && *w != "…")
        .map(normalize_word)
        .filter(|w| !w.is_empty())
        .collect()
}

/// One run of consecutive words that render the same alignment.
struct Group<'a> {
    sources: &'a [SourceWord],
    /// The sources' normalized words, in the same order.
    keys: &'a [String],
    tokens: Vec<usize>,
}

fn groups(verse: &AlignedVerse) -> Vec<Group<'_>> {
    let mut out: Vec<Group> = Vec::new();
    let mut previous: Option<AlignmentId> = None;
    for (i, token) in verse.tokens.iter().enumerate() {
        let TokenKind::Word(Some(id)) = token.kind else {
            // Text and unaligned words between two words of one group do not split it.
            continue;
        };
        let (Some(sources), Some(keys)) = (
            verse.sources(id).filter(|s| !s.is_empty()),
            verse.keys.get(id.0),
        ) else {
            continue;
        };
        match out.last_mut() {
            Some(group) if previous == Some(id) => group.tokens.push(i),
            _ => out.push(Group {
                sources,
                keys,
                tokens: vec![i],
            }),
        }
        previous = Some(id);
    }
    out
}

/// One original word instance of the verse: its surface form as the alignment writes it
/// and its occurrence (tC4's `${content}#${occurrence}`). Two forms of one word that differ
/// in an accent are two instances.
type Instance<'a> = (&'a str, u32);

fn has(group: &Group<'_>, word: &str, occurrence: Option<u32>) -> bool {
    group
        .keys
        .iter()
        .zip(group.sources)
        .any(|(key, s)| key == word && occurrence.is_none_or(|o| s.occurrence == o))
}

/// The instances of `word` among `group`'s original words.
fn instances_of<'a>(group: &Group<'a>, word: &str) -> impl Iterator<Item = Instance<'a>> {
    group
        .keys
        .iter()
        .zip(group.sources)
        .filter(move |(key, _)| *key == word)
        .map(|(_, s)| (s.content.as_str(), s.occurrence))
}

/// True when `group` renders one of `instances`.
fn renders(group: &Group<'_>, instances: &BTreeSet<Instance<'_>>) -> bool {
    group
        .sources
        .iter()
        .any(|s| instances.contains(&(s.content.as_str(), s.occurrence)))
}

struct Candidate<'a> {
    instances: BTreeSet<Instance<'a>>,
    span: usize,
}

/// Finds the target tokens that render `quote` at `occurrence`.
///
/// Every alignment group that holds the first quote word anchors a candidate, which then
/// takes each next quote word from the first group at or after the previous one.
/// Candidates that share an original word instance are the same occurrence of the quote;
/// within one, the tightest candidate wins (en_tn TIT 2:2 `τῇ ἀγάπῃ` must light "in
/// love", not "in faith"). The winner lights every group that renders any of its
/// instances, because one original word may be rendered by several groups (TIT 1:1
/// ἐκλεκτῶν is "of" and "the chosen people").
///
/// This is tC4's matcher. It needs the gateway to keep the original word order, which it
/// often does not. When no candidate completes, every group that renders a quote word is
/// lit; that is [`MatchQuality::Exact`] when the verse holds each quote word exactly as
/// often as the quote does (so nothing else can be meant), and
/// [`MatchQuality::Approximate`] otherwise. tC4 lights the same words in both cases.
pub fn highlight_quote(verse: &AlignedVerse, quote: &Quote, occurrence: Occurrence) -> Highlight {
    let words = quote_words(quote.as_str());
    let Some(first) = words.first() else {
        return Highlight::none();
    };
    let groups = groups(verse);

    let mut candidates = Vec::new();
    for start in 0..groups.len() {
        if !has(&groups[start], first, None) {
            continue;
        }
        let mut picked = vec![start];
        let mut at = start;
        let complete = words[1..].iter().all(|w| {
            match (at..groups.len()).find(|&j| has(&groups[j], w, None)) {
                Some(j) => {
                    picked.push(j);
                    at = j;
                    true
                }
                None => false,
            }
        });
        if !complete {
            continue;
        }
        let instances = picked
            .iter()
            .zip(&words)
            .flat_map(|(&g, w)| instances_of(&groups[g], w))
            .collect();
        candidates.push(Candidate {
            instances,
            span: at - start,
        });
    }

    let mut classes: Vec<Vec<Candidate<'_>>> = Vec::new();
    for c in candidates {
        let shared = classes
            .iter()
            .position(|class| class.iter().any(|m| !m.instances.is_disjoint(&c.instances)));
        match shared {
            Some(k) => classes[k].push(c),
            None => classes.push(vec![c]),
        }
    }
    fn tightest<'a>(class: &[Candidate<'a>]) -> Option<BTreeSet<Instance<'a>>> {
        class
            .iter()
            .min_by_key(|c| c.span)
            .map(|c| c.instances.clone())
    }

    let chosen: Option<(BTreeSet<Instance<'_>>, MatchQuality)> = match occurrence {
        Occurrence::Nth(n) => {
            let k = usize::try_from(n.get()).unwrap_or(usize::MAX);
            classes
                .get(k - 1)
                .and_then(|c| tightest(c))
                .map(|i| (i, MatchQuality::Exact))
        }
        // An item not tied to words has no occurrence to honor; tC treats it as the first.
        Occurrence::NotTied => classes
            .first()
            .and_then(|c| tightest(c))
            .map(|i| (i, MatchQuality::Exact)),
        Occurrence::All | Occurrence::Unrecorded => {
            let quality = match (occurrence, classes.len()) {
                (Occurrence::Unrecorded, 1) => MatchQuality::Exact,
                _ => MatchQuality::AllOccurrences,
            };
            let all: BTreeSet<Instance<'_>> = classes
                .iter()
                .filter_map(|c| tightest(c))
                .flatten()
                .collect();
            (!all.is_empty()).then_some((all, quality))
        }
    };

    let mut lit = BTreeSet::new();
    let quality = match chosen {
        Some((instances, quality)) => {
            for g in &groups {
                if renders(g, &instances) {
                    lit.extend(g.tokens.iter().copied());
                }
            }
            quality
        }
        None => {
            // No such occurrence of the whole quote. A one-word quote still resolves
            // through the alignment's own occurrence numbers; a longer one lights every
            // group holding any quote word, so the reader sees something, marked as such.
            let exact = match occurrence {
                Occurrence::Nth(n) if words.len() == 1 => Some(n.get()),
                _ => None,
            };
            for g in &groups {
                if words.iter().any(|w| has(g, w, exact)) {
                    lit.extend(g.tokens.iter().copied());
                }
            }
            if exact.is_some() {
                MatchQuality::WordOccurrence
            } else if first_occurrence(occurrence) && unambiguous(&groups, &words) {
                MatchQuality::Exact
            } else {
                MatchQuality::Approximate
            }
        }
    };
    if lit.is_empty() {
        return Highlight::none();
    }
    Highlight {
        tokens: lit.into_iter().collect(),
        quality,
    }
}

fn first_occurrence(occurrence: Occurrence) -> bool {
    match occurrence {
        Occurrence::Nth(n) => n.get() == 1,
        Occurrence::NotTied | Occurrence::All | Occurrence::Unrecorded => true,
    }
}

/// True when the verse holds each quote word exactly as many times as the quote does.
/// Then the quote can occur only once, and every instance of its words belongs to it, so
/// lighting every group that renders a quote word is certain even though the gateway
/// renders the words in another order (`μιᾶς γυναικὸς ἀνήρ`, "a husband of one wife").
fn unambiguous(groups: &[Group<'_>], words: &[String]) -> bool {
    let mut wanted: BTreeMap<&str, usize> = BTreeMap::new();
    for w in words {
        *wanted.entry(w.as_str()).or_default() += 1;
    }
    wanted.into_iter().all(|(word, count)| {
        let instances: BTreeSet<Instance<'_>> =
            groups.iter().flat_map(|g| instances_of(g, word)).collect();
        instances.len() == count
    })
}

/// The quote in the gateway language: the lit words in verse order, joined by spaces,
/// with `…` where unlit words fall between them. `None` when nothing renders the quote,
/// so the caller shows the original-language quote instead.
pub fn gateway_quote(
    verse: &AlignedVerse,
    quote: &Quote,
    occurrence: Occurrence,
) -> Option<String> {
    let lit: BTreeSet<usize> = highlight_quote(verse, quote, occurrence)
        .tokens
        .into_iter()
        .collect();
    if lit.is_empty() {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    let mut gap = false;
    for (i, token) in verse.tokens.iter().enumerate() {
        if !matches!(token.kind, TokenKind::Word(_)) {
            continue;
        }
        if lit.contains(&i) {
            if gap && !parts.is_empty() {
                parts.push("…");
            }
            parts.push(&token.text);
            gap = false;
        } else {
            gap = true;
        }
    }
    Some(parts.join(" "))
}

/// A token of plain text: a run of word characters, or a run of anything else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlainToken {
    pub text: String,
    pub word: bool,
}

fn word_char(c: char) -> bool {
    letter_or_number(c) || mark(c) || matches!(c, '\'' | '’' | '-')
}

/// Splits text into word runs (letters, numbers, marks, apostrophes and hyphens) and the
/// runs between them. Joining the tokens gives the text back.
pub fn tokenize_plain(text: &str) -> Vec<PlainToken> {
    let mut runs: Vec<(String, bool)> = Vec::new();
    for c in text.chars() {
        let in_word = word_char(c);
        match runs.last_mut() {
            Some((run, kind)) if *kind == in_word => run.push(c),
            _ => runs.push((c.to_string(), in_word)),
        }
    }
    runs.into_iter()
        .map(|(text, _)| PlainToken {
            // A run of only apostrophes or hyphens is not a word.
            word: text.chars().any(letter_or_number),
            text,
        })
        .collect()
}

/// Finds an Open Bible Stories quote in the story frame's tokens: each `&` span as a
/// contiguous run of words, each span after the one before, first match wins. A quote
/// that does not match in full lights nothing. Returns token indices.
pub fn highlight_plain(tokens: &[PlainToken], quote: &Quote) -> Vec<usize> {
    let words: Vec<(usize, String)> = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| t.word)
        .map(|(i, t)| (i, normalize_word(&t.text)))
        .filter(|(_, w)| !w.is_empty())
        .collect();
    let spans: Vec<Vec<String>> = quote
        .as_str()
        .split('&')
        .map(quote_words)
        .filter(|s| !s.is_empty())
        .collect();
    let mut lit = Vec::new();
    let mut from = 0;
    for span in &spans {
        let found = (from..words.len().saturating_sub(span.len() - 1))
            .find(|&k| span.iter().enumerate().all(|(j, w)| words[k + j].1 == *w));
        let Some(k) = found else {
            return Vec::new();
        };
        lit.extend(words[k..k + span.len()].iter().map(|(i, _)| *i));
        from = k + span.len();
    }
    lit
}
