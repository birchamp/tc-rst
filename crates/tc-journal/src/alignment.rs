//! Alignments: what keeps a stored alignment valid (`docs/STORAGE.md` §7.7) and the order a
//! saved value holds its words in (§5.3). The Align screens and the fold both use these
//! functions, so the rail and the alignment files cannot disagree.

use crate::ids::{GitOid, Md5, VerseNo};
use crate::json::{Json, JsonMap};
use crate::record::{AlignmentGroup, AlignmentValue, SourceWord, TargetWord};
use std::collections::BTreeSet;

/// Where the fold places an alignment and what the projection holds there (§7.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlignedRun {
    /// The verse of the record's key: the anchor of the text it was made for.
    pub anchor: u16,
    /// The first and last verse of the projected run the record is placed at.
    pub run_start: u16,
    pub run_end: u16,
    /// [`crate::decision::verse_text_md5_v1`] of the run's content now.
    pub text_md5: Md5,
    /// The sha of the current original-language pin of the book's testament, if pinned.
    pub source_sha: Option<GitOid>,
}

/// Why an alignment no longer vouches for its verse. A record with any of these projects
/// `"invalid": true` and is kept as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stale {
    /// The run is not the span the alignment was made for, moved with its text: verses were
    /// joined to it or split from it.
    SpanChanged,
    /// The text changed (the I-3 hash differs).
    TextChanged,
    /// The original-language text was upgraded or unpinned.
    SourceChanged,
    /// tC3 or tC4 marked the record invalid, and nobody marked it valid since.
    MarkedInvalid,
}

impl AlignmentValue {
    /// The last verse of the run this alignment was made for, moved with its text to a run
    /// that starts at `run_start`: `run_start + (span_end − anchor)`. `None` when the span
    /// ends before its anchor, which no aligner writes. Comparing `span_end` itself with the
    /// run's end would call every alignment of a renumbered verse stale although its text
    /// and span are unchanged.
    pub fn moved_span_end(&self, anchor: u16, run_start: u16) -> Option<u16> {
        let length = self.span_end.get().checked_sub(anchor)?;
        Some(run_start.saturating_add(length))
    }

    /// Every reason this alignment does not vouch for the run it is placed at, in a fixed
    /// order (§7.7). Empty means valid.
    pub fn staleness(&self, at: &AlignedRun) -> Vec<Stale> {
        let mut reasons = Vec::new();
        if self.moved_span_end(at.anchor, at.run_start) != Some(at.run_end) {
            reasons.push(Stale::SpanChanged);
        }
        if self.target_verse_md5 != at.text_md5 {
            reasons.push(Stale::TextChanged);
        }
        if self
            .source
            .as_ref()
            .is_some_and(|s| Some(s.sha) != at.source_sha)
        {
            reasons.push(Stale::SourceChanged);
        }
        if self.marked_invalid {
            reasons.push(Stale::MarkedInvalid);
        }
        reasons
    }

    pub fn is_valid(&self, at: &AlignedRun) -> bool {
        self.staleness(at).is_empty()
    }

    /// Puts the word bank and every card's target words in the order they stand in the
    /// verse (§5.3, the owner's Q15 and Q19): a repeated word by its occurrence, and a word
    /// the verse does not hold after the rest, in the order it had. `verse` is the verse's
    /// target words in order, each with its occurrence (tC4 `inVerseOrder` and
    /// `cardsInVerseOrder`, 3d43f7c, 91131d9 and 26aded2). Every value the rebuild saves is
    /// in this order, so two saves never differ by order alone; an imported value keeps its
    /// order until someone edits it.
    pub fn put_in_verse_order<'a>(&mut self, verse: impl IntoIterator<Item = (&'a str, u32)>) {
        let order = VerseOrder::new(verse);
        self.sort_bank_by(&order);
        self.sort_cards_by(&order);
    }

    /// The word bank alone in verse order (tC4 `inVerseOrder`, #734).
    pub fn sort_bank_by(&mut self, order: &VerseOrder<'_>) {
        order.sort(&mut self.word_bank);
    }

    /// Each card's target words alone in verse order (tC4 `cardsInVerseOrder`, #754).
    pub fn sort_cards_by(&mut self, order: &VerseOrder<'_>) {
        for group in &mut self.alignments {
            order.sort(&mut group.bottom_words);
        }
    }

    /// Whether the bank and every card already stand in verse order.
    pub fn in_verse_order<'a>(&self, verse: impl IntoIterator<Item = (&'a str, u32)>) -> bool {
        let mut sorted = self.clone();
        sorted.put_in_verse_order(verse);
        sorted == *self
    }
}

/// Where each word of a verse stands, by its text and occurrence: the order §5.3 keeps the
/// bank and the cards in.
#[derive(Debug, Clone, Default)]
pub struct VerseOrder<'a> {
    rank: std::collections::HashMap<(&'a str, u32), usize>,
}

impl<'a> VerseOrder<'a> {
    /// The order of `verse`, its target words in order, each with its occurrence.
    pub fn new(verse: impl IntoIterator<Item = (&'a str, u32)>) -> Self {
        Self {
            rank: verse
                .into_iter()
                .enumerate()
                .map(|(i, word)| (word, i))
                .collect(),
        }
    }

    /// Where `word` stands in the verse, by its text and occurrence.
    pub fn position(&self, word: &str, occurrence: u32) -> Option<usize> {
        self.rank.get(&(word, occurrence)).copied()
    }

    /// Puts `words` in verse order: a word the verse does not hold goes last, keeping the
    /// order it had. The sort is stable.
    pub fn sort(&self, words: &mut [TargetWord]) {
        words.sort_by_cached_key(|w| self.position(&w.word, w.occurrence).unwrap_or(usize::MAX));
    }
}

impl SourceWord {
    /// A `\zaln-s` milestone's source word: `x-content` is `word`, every attribute beyond
    /// the six named ones is kept in `extra` under its own name.
    pub fn from_usfm(word: &tc_usfm::SourceWord) -> Self {
        Self {
            word: word.content.clone(),
            strong: word.strong.clone(),
            lemma: word.lemma.clone(),
            morph: word.morph.clone(),
            occurrence: word.occurrence,
            occurrences: word.occurrences,
            extra: word
                .extra
                .iter()
                .map(|(name, value)| (name.clone(), Json::Str(value.clone())))
                .collect(),
        }
    }

    /// The word as a `\zaln-s` writes it. An `extra` value that is not a string is written
    /// as its canonical JSON.
    pub fn to_usfm(&self) -> tc_usfm::SourceWord {
        tc_usfm::SourceWord {
            content: self.word.clone(),
            strong: self.strong.clone(),
            lemma: self.lemma.clone(),
            morph: self.morph.clone(),
            occurrence: self.occurrence,
            occurrences: self.occurrences,
            extra: self
                .extra
                .iter()
                .map(|(name, value)| {
                    let text = match value {
                        Json::Str(text) => text.clone(),
                        other => other.to_canonical_string(),
                    };
                    (name.clone(), text)
                })
                .collect(),
        }
    }
}

impl TargetWord {
    pub fn from_usfm(word: &tc_usfm::TargetWord) -> Self {
        Self {
            word: word.text.clone(),
            occurrence: word.occurrence,
            occurrences: word.occurrences,
            extra: JsonMap::new(),
        }
    }

    pub fn to_usfm(&self) -> tc_usfm::TargetWord {
        tc_usfm::TargetWord {
            text: self.word.clone(),
            occurrence: self.occurrence,
            occurrences: self.occurrences,
        }
    }
}

impl AlignmentValue {
    /// The groups as aligned USFM writes them (`tc_usfm::weave`): what an export of the
    /// verse, and the aligned-USFM split's check, weave. A card with no source word or no
    /// placed word aligns nothing and writes nothing.
    pub fn usfm_groups(&self) -> Vec<tc_usfm::AlignmentGroup> {
        self.alignments
            .iter()
            .filter(|g| !g.top_words.is_empty() && !g.bottom_words.is_empty())
            .map(|g| tc_usfm::AlignmentGroup {
                sources: g.top_words.iter().map(SourceWord::to_usfm).collect(),
                targets: g.bottom_words.iter().map(TargetWord::to_usfm).collect(),
            })
            .collect()
    }

    /// Whether every target word is placed and no card with source words is left without one
    /// (tC3's `areAlgnmentsComplete`, D73's "fully aligned"). An empty record is not aligned.
    pub fn is_fully_aligned(&self) -> bool {
        self.word_bank.is_empty()
            && self.alignments.iter().any(|g| !g.bottom_words.is_empty())
            && self
                .alignments
                .iter()
                .all(|g| g.top_words.is_empty() || !g.bottom_words.is_empty())
    }
}

/// An aligned verse taken apart for an import (§13.5): its content with the alignment markup
/// removed, and the `Alignment` value of the run it starts, whose span ends at `span_end`.
/// `None` unless the markup reads apart, its runs woven back into the plain content give
/// `content` byte for byte, and the cards make exactly the runs' links; such a verse is
/// imported as written.
///
/// The cards are tC4's (word-aligner's `unmerge`): `tc-usfm` gives one group per run of
/// words under the same milestones, and runs under the same outermost source word join one
/// card. Where en_ult writes one source word over two runs (TIT 1:1 "of" and "the chosen
/// people" under ἐκλεκτῶν), the card holds both, and an export writes them as one run: the
/// same links, in tC4's bytes. Where joining would link a word to a source word its run did
/// not name (runs `A B: x` and `A C: y` would link x to C), the verse is kept as written.
pub fn split_aligned(content: &str, span_end: VerseNo) -> Option<(String, AlignmentValue)> {
    let verse = tc_usfm::parse_aligned(content).ok()?;
    if verse.groups.is_empty() {
        return None;
    }
    // Each run as the value's words write it, so a verse whose words would not write back
    // the same (an attribute order, say) is kept as written.
    let runs: Vec<tc_usfm::AlignmentGroup> = verse
        .groups
        .iter()
        .filter(|g| !g.sources.is_empty() && !g.targets.is_empty())
        .map(|g| tc_usfm::AlignmentGroup {
            sources: g
                .sources
                .iter()
                .map(|s| SourceWord::from_usfm(s).to_usfm())
                .collect(),
            targets: g
                .targets
                .iter()
                .map(|t| TargetWord::from_usfm(t).to_usfm())
                .collect(),
        })
        .collect();
    if tc_usfm::weave(&verse.plain, &runs).ok()? != content {
        return None;
    }
    let alignments = tc4_cards(&verse.groups);
    let card_links: Vec<(tc_usfm::SourceWord, tc_usfm::TargetWord)> = alignments
        .iter()
        .flat_map(|card| {
            card.top_words
                .iter()
                .flat_map(|s| card.bottom_words.iter().map(|t| (s.to_usfm(), t.to_usfm())))
        })
        .collect();
    let run_links: Vec<(tc_usfm::SourceWord, tc_usfm::TargetWord)> = runs
        .iter()
        .flat_map(|run| {
            run.sources
                .iter()
                .flat_map(|s| run.targets.iter().map(|t| (s.clone(), t.clone())))
        })
        .collect();
    // A verse's links are few, so comparing them pairwise costs what the verse holds.
    let same = |a: &[_], b: &[_]| a.iter().all(|link| b.contains(link));
    if !same(&card_links, &run_links) || !same(&run_links, &card_links) {
        return None;
    }
    // Every word no card holds is banked, so no word of the verse is lost.
    let placed: BTreeSet<(&str, u32)> = alignments
        .iter()
        .flat_map(|g| {
            g.bottom_words
                .iter()
                .map(|t| (t.word.as_str(), t.occurrence))
        })
        .collect();
    let word_bank = verse
        .words
        .iter()
        .filter(|w| !placed.contains(&(w.text.as_str(), w.occurrence)))
        .map(TargetWord::from_usfm)
        .collect();
    let value = AlignmentValue {
        span_end,
        alignments,
        word_bank,
        target_verse_md5: crate::decision::verse_text_md5_v1(&verse.plain),
        source_version: "".into(),
        source: None,
        done: false,
        marked_invalid: false,
        dismissed: false,
        extra: JsonMap::new(),
    };
    let done = value.is_fully_aligned();
    Some((verse.plain, AlignmentValue { done, ..value }))
}

/// tC4's cards for the runs of an aligned verse, the rule [`split_aligned`] reads with: one
/// card per outermost source word, in the order its first run stands. A run with no target
/// word adds no source word, and a card left without one aligns nothing and is not kept, as
/// in word-aligner.
pub fn tc4_cards(runs: &[tc_usfm::AlignmentGroup]) -> Vec<AlignmentGroup> {
    let mut cards: Vec<((&str, u32), AlignmentGroup)> = Vec::new();
    for run in runs {
        let Some(outer) = run.sources.first() else {
            continue;
        };
        let key = (outer.content.as_str(), outer.occurrence);
        let at = match cards.iter().position(|(k, _)| *k == key) {
            Some(at) => at,
            None => {
                cards.push((
                    key,
                    AlignmentGroup {
                        top_words: Vec::new(),
                        bottom_words: Vec::new(),
                        extra: JsonMap::new(),
                    },
                ));
                cards.len() - 1
            }
        };
        let card = &mut cards[at].1;
        if !run.targets.is_empty() {
            for word in &run.sources {
                let held = card
                    .top_words
                    .iter()
                    .any(|s| s.word == word.content && s.occurrence == word.occurrence);
                if !held {
                    card.top_words.push(SourceWord::from_usfm(word));
                }
            }
        }
        card.bottom_words
            .extend(run.targets.iter().map(TargetWord::from_usfm));
    }
    cards
        .into_iter()
        .map(|(_, card)| card)
        .filter(|card| !card.top_words.is_empty())
        .collect()
}
