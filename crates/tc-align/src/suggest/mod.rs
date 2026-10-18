//! Alignment suggestions learned from the project's own alignments (tC4 D72, D93).
//!
//! The rules, from tC4's owner rulings:
//!
//! - Suggestions are off until the translator turns them on (the switch lives in the app).
//! - The engine learns only from this project's confirmed alignments, one model per
//!   testament: Hebrew memory cannot inform a Greek verse.
//! - A suggestion is a [`Proposal`]: nothing here changes an [`AlignmentValue`]. The
//!   translator accepts a proposal into an ordinary placement or rejects it
//!   ([`crate::Aligner`]); an unaccepted proposal is never written, and Mark valid is
//!   refused while one stands.
//! - Every confirmed save joins the model's alignment memory at once
//!   ([`Model::append_verse`]); the booster retrains in the background only when the
//!   memory first reaches a step of [`RETRAIN_BUDGET`].
//!
//! The engine is tC4's: wordMAP 0.6.2 with uw-wordmapbooster 1.0.5's
//! `MorphJLBoostWordMap` (source n-grams up to 3 words, target up to 5, 1000 boosting
//! steps of depth-5 trees, learning rate 0.7, one whole-verse suggestion, no confidence
//! cut). Below [`MIN_BOOST_VERSES`] verses, or when the booster cannot be fitted, plain
//! wordMAP answers from the memory. The booster is fitted on a share of the verses
//! bounded by [`max_complexity`]; every verse still joins the memory.
//!
//! This module is the bridge between values and the engine (tC4 `suggest.ts`): the engine
//! sees verse positions, the editor sees words. [`training_verse`] and [`session_input`]
//! derive the engine's view from a value and its verse text with the one tokenizer, and
//! [`proposals`] maps the engine's answer back to bank words on cards.

mod boost;
mod engine;
mod wordmap;

pub use boost::Booster;
pub use engine::{MIN_BOOST_VERSES, Model, bound_corpus, max_complexity};

use crate::edit::Alignment;
use crate::words::TargetVerse;
use std::collections::HashMap;
use tc_journal::record::AlignmentValue;
use tc_journal::record::TargetWord;

/// A word as the engine sees it: its text, its position in the verse, its occurrence,
/// and for an original-language word its Strong's number, lemma and morphology.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSeed {
    pub text: String,
    pub position: usize,
    pub occurrence: u32,
    pub occurrences: u32,
    pub strong: Option<String>,
    pub lemma: Option<String>,
    pub morph: Option<String>,
}

/// A link in verse positions: source word positions joined to target word positions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PositionLink {
    pub source: Vec<usize>,
    pub target: Vec<usize>,
}

/// One verse of training data: its original words in order, its target words in order,
/// and its confirmed links. Every position of every link is a word of the verse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainingVerse {
    reference: String,
    source: Vec<TokenSeed>,
    target: Vec<TokenSeed>,
    links: Vec<PositionLink>,
}

impl TrainingVerse {
    /// A label such as `TIT 1:1`. Verses with the same label train as one, the last
    /// given winning, as in tC4.
    pub fn reference(&self) -> &str {
        &self.reference
    }

    pub fn source(&self) -> &[TokenSeed] {
        &self.source
    }

    pub fn target(&self) -> &[TokenSeed] {
        &self.target
    }

    pub fn links(&self) -> &[PositionLink] {
        &self.links
    }

    /// The same verse under another label.
    pub fn relabeled(&self, reference: impl Into<String>) -> Self {
        Self {
            reference: reference.into(),
            ..self.clone()
        }
    }

    /// The verse as a question with nothing placed: what the engine is asked when the
    /// translator presses Suggest on an empty verse.
    pub fn as_question(&self) -> SessionInput {
        SessionInput {
            source: self.source.clone(),
            target: self.target.clone(),
            manual: Vec::new(),
        }
    }

    /// One verse of an aligned text such as en_ult, read as a project's own confirmed
    /// alignment, as tC4's suggestion benches read en_ult (`verseFromObjects` in
    /// `test/helpers/ult-corpus.ts`): one source word per `\zaln-s` in the order they
    /// open, one link per aligned `\w` from the chain of milestones open above it to that
    /// one word, target positions from the words of the verse's text. `None` when the
    /// verse aligns nothing.
    pub fn from_aligned_usfm(
        reference: impl Into<String>,
        body: &str,
    ) -> Result<Option<Self>, tc_usfm::Error> {
        let doc = tc_usfm::Document::parse(body);
        let mut walk = AlignedWalk::default();
        walk.nodes(doc.nodes())?;
        let target = target_seeds(&TargetVerse::new(&walk.plain));
        let position: HashMap<(&str, u32), usize> = target
            .iter()
            .map(|t| ((t.text.as_str(), t.occurrence), t.position))
            .collect();
        let links: Vec<PositionLink> = walk
            .links
            .into_iter()
            .filter_map(|(source, word, occurrence)| {
                let p = position.get(&(word.as_str(), occurrence))?;
                Some(PositionLink {
                    source,
                    target: vec![*p],
                })
            })
            .collect();
        if walk.source.is_empty() || links.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            reference: reference.into(),
            source: walk.source,
            target,
            links,
        }))
    }
}

/// The walk of [`TrainingVerse::from_aligned_usfm`] over usfm-js's verse objects.
#[derive(Default)]
struct AlignedWalk {
    plain: String,
    source: Vec<TokenSeed>,
    open: Vec<usize>,
    /// Each aligned word: the milestones above it, its text and its written occurrence.
    links: Vec<(Vec<usize>, String, u32)>,
}

impl AlignedWalk {
    fn nodes(&mut self, nodes: &[tc_usfm::Node<'_>]) -> Result<(), tc_usfm::Error> {
        use tc_usfm::Node;
        let number = |v: Option<&str>| v.and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
        for node in nodes {
            match node {
                Node::Text(text) => self.plain.push_str(text),
                Node::Milestone(m) if m.name() == "zaln-s" => {
                    let get = |key: &str| -> Result<Option<String>, tc_usfm::Error> {
                        Ok(match &m.attributes {
                            Some(a) => a.get(key)?.map(str::to_owned),
                            None => None,
                        })
                    };
                    let position = self.source.len();
                    self.source.push(TokenSeed {
                        text: get("x-content")?.unwrap_or_default(),
                        position,
                        occurrence: number(get("x-occurrence")?.as_deref()),
                        occurrences: number(get("x-occurrences")?.as_deref()),
                        strong: get("x-strong")?,
                        lemma: get("x-lemma")?,
                        morph: get("x-morph")?,
                    });
                    self.open.push(position);
                }
                Node::Milestone(m) if m.name() == "zaln-e" => {
                    self.open.pop();
                }
                Node::Span(span) if span.name() == "w" => {
                    let mut text = String::new();
                    for child in &span.children {
                        if let Node::Text(t) = child {
                            text.push_str(t);
                        }
                    }
                    self.plain.push_str(&text);
                    if !self.open.is_empty() {
                        let occurrence = match &span.attributes {
                            Some(a) => number(a.get("x-occurrence")?),
                            None => 1,
                        };
                        self.links.push((self.open.clone(), text, occurrence));
                    }
                }
                // Notes carry no text in usfm-js's verse objects; other character spans
                // are walked for their words.
                Node::Span(span) if !is_note(span.name()) => self.nodes(&span.children)?,
                _ => {}
            }
        }
        Ok(())
    }
}

fn is_note(name: &str) -> bool {
    use tc_usfm::markers::{MarkerClass, classify};
    classify(name) == MarkerClass::Note || matches!(name, "fig" | "ca" | "va" | "vp" | "cat" | "rq")
}

/// The open verse as the engine sees it: both sides in positions, and the links the
/// translator has placed, which a boosted model takes as context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInput {
    source: Vec<TokenSeed>,
    target: Vec<TokenSeed>,
    manual: Vec<PositionLink>,
}

impl SessionInput {
    /// A question for the engine. `None` when a placed link names a position that is not
    /// a word of the verse.
    pub fn new(
        source: Vec<TokenSeed>,
        target: Vec<TokenSeed>,
        manual: Vec<PositionLink>,
    ) -> Option<Self> {
        let fits = manual.iter().all(|l| {
            l.source.iter().all(|&p| p < source.len()) && l.target.iter().all(|&p| p < target.len())
        });
        fits.then_some(Self {
            source,
            target,
            manual,
        })
    }

    pub fn source(&self) -> &[TokenSeed] {
        &self.source
    }

    pub fn target(&self) -> &[TokenSeed] {
        &self.target
    }

    pub fn manual(&self) -> &[PositionLink] {
        &self.manual
    }
}

/// A link the engine predicts, in positions of the [`SessionInput`] it was asked.
#[derive(Debug, Clone, PartialEq)]
pub struct RawLink {
    pub source: Vec<usize>,
    pub target: Vec<usize>,
    pub confidence: f64,
}

/// An original-language word by text and occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OriginalWord {
    pub word: String,
    pub occurrence: u32,
}

/// A proposed placement: a bank word on a card. It is never written; accepting it is an
/// ordinary placement of the word on the card.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    /// The card, by index in the value it was made for. [`rebind`] finds it again after
    /// an edit by [`Proposal::source`].
    pub card: usize,
    /// The original word the engine predicted, which identifies the card whatever its
    /// index.
    pub source: OriginalWord,
    /// The bank word.
    pub word: TargetWord,
    pub confidence: f64,
}

type WordKey = (String, u32);

fn key(word: &str, occurrence: u32) -> WordKey {
    (word.to_owned(), occurrence)
}

/// The value's source words in card order, and the card of each.
fn source_seeds(value: &AlignmentValue) -> (Vec<TokenSeed>, Vec<usize>) {
    let mut seeds = Vec::new();
    let mut card_of = Vec::new();
    for (card, group) in value.alignments.iter().enumerate() {
        for word in &group.top_words {
            seeds.push(TokenSeed {
                text: word.word.clone(),
                position: seeds.len(),
                occurrence: word.occurrence,
                occurrences: word.occurrences,
                strong: word.strong.clone().filter(|s| !s.is_empty()),
                lemma: word.lemma.clone().filter(|s| !s.is_empty()),
                morph: word.morph.clone().filter(|s| !s.is_empty()),
            });
            card_of.push(card);
        }
    }
    (seeds, card_of)
}

/// The target words of a verse in verse order, from its text.
pub fn target_seeds(verse: &TargetVerse) -> Vec<TokenSeed> {
    verse
        .words()
        .iter()
        .enumerate()
        .map(|(position, w)| TokenSeed {
            text: w.word.clone(),
            position,
            occurrence: w.occurrence,
            occurrences: w.occurrences,
            strong: None,
            lemma: None,
            morph: None,
        })
        .collect()
}

/// The value's placed links in positions. A card whose words the text or the cards no
/// longer hold is left out rather than given an invented position.
fn placed_links(
    value: &AlignmentValue,
    source: &[TokenSeed],
    target: &[TokenSeed],
) -> Vec<PositionLink> {
    let source_at: HashMap<WordKey, usize> = source
        .iter()
        .map(|s| (key(&s.text, s.occurrence), s.position))
        .collect();
    let target_at: HashMap<WordKey, usize> = target
        .iter()
        .map(|t| (key(&t.text, t.occurrence), t.position))
        .collect();
    let mut out = Vec::new();
    for group in value
        .alignments
        .iter()
        .filter(|g| !g.bottom_words.is_empty())
    {
        let s: Vec<usize> = group
            .top_words
            .iter()
            .filter_map(|w| source_at.get(&key(&w.word, w.occurrence)).copied())
            .collect();
        let t: Vec<usize> = group
            .bottom_words
            .iter()
            .filter_map(|w| target_at.get(&key(&w.word, w.occurrence)).copied())
            .collect();
        if s.len() == group.top_words.len() && t.len() == group.bottom_words.len() {
            out.push(PositionLink {
                source: s,
                target: t,
            });
        }
    }
    out
}

/// One saved verse as training data, or `None` when it has no usable link (nothing
/// placed, no text, or every placed word gone from the text). tC4's `trainingVerseOf`.
pub fn training_verse(
    reference: impl Into<String>,
    value: &AlignmentValue,
    verse: &TargetVerse,
) -> Option<TrainingVerse> {
    if verse.text().is_empty() || value.placed() == 0 {
        return None;
    }
    let (source, _) = source_seeds(value);
    let target = target_seeds(verse);
    let links = placed_links(value, &source, &target);
    (!links.is_empty()).then(|| TrainingVerse {
        reference: reference.into(),
        source,
        target,
        links,
    })
}

/// The open verse for the engine: its words and the links already placed.
pub fn session_input(value: &AlignmentValue, verse: &TargetVerse) -> SessionInput {
    let (source, _) = source_seeds(value);
    let target = target_seeds(verse);
    let manual = placed_links(value, &source, &target);
    SessionInput {
        source,
        target,
        manual,
    }
}

/// The engine's answer for [`session_input`]`(value, verse)` as proposals. Only words
/// still in the bank are proposed, each once, on the card that holds the predicted source
/// words. A prediction whose source words sit on two cards is left out: placing it would
/// merge cards, which is the translator's act. tC4's `linksFor`.
pub fn proposals(value: &AlignmentValue, verse: &TargetVerse, raw: &[RawLink]) -> Vec<Proposal> {
    let (seeds, card_of) = source_seeds(value);
    let target = target_seeds(verse);
    let bank: HashMap<WordKey, &TargetWord> = value
        .word_bank
        .iter()
        .map(|w| (key(&w.word, w.occurrence), w))
        .collect();
    let mut taken = std::collections::HashSet::new();
    let mut out = Vec::new();
    for link in raw {
        let mut cards: Vec<usize> = link
            .source
            .iter()
            .filter_map(|p| card_of.get(*p).copied())
            .collect();
        cards.dedup();
        cards.sort_unstable();
        cards.dedup();
        let ([card], Some(predicted)) = (
            cards.as_slice(),
            link.source.first().and_then(|p| seeds.get(*p)),
        ) else {
            continue;
        };
        for t in link.target.iter().filter_map(|p| target.get(*p)) {
            let k = key(&t.text, t.occurrence);
            let Some(word) = bank.get(&k) else {
                continue;
            };
            if !taken.insert(k) {
                continue;
            }
            out.push(Proposal {
                card: *card,
                source: OriginalWord {
                    word: predicted.text.clone(),
                    occurrence: predicted.occurrence,
                },
                word: (*word).clone(),
                confidence: link.confidence,
            });
        }
    }
    out
}

/// The proposals that still stand after an edit: the word must still be in the bank, and
/// the card is found again as the one holding the predicted original word, since a merge
/// or split moves card indexes. tC4's `rebindSuggestions`.
pub fn rebind(value: &AlignmentValue, proposals: &[Proposal]) -> Vec<Proposal> {
    proposals
        .iter()
        .filter(|p| {
            value
                .word_bank
                .iter()
                .any(|w| w.word == p.word.word && w.occurrence == p.word.occurrence)
        })
        .filter_map(|p| {
            let card = value.alignments.iter().position(|g| {
                g.top_words
                    .iter()
                    .any(|w| w.word == p.source.word && w.occurrence == p.source.occurrence)
            })?;
            Some(Proposal { card, ..p.clone() })
        })
        .collect()
}

/// The memory sizes at which the booster retrains, after the one when the Align tool
/// opens. Above the last, it does not retrain in the session.
pub const RETRAIN_BUDGET: [usize; 7] = [10, 25, 50, 100, 250, 500, 1000];

/// Whether growing the memory from `prev` to `next` verses first reaches a step of
/// [`RETRAIN_BUDGET`]: the moment a background retrain starts.
pub fn crosses_retrain_budget(prev: usize, next: usize) -> bool {
    RETRAIN_BUDGET.iter().any(|&t| prev < t && next >= t)
}
