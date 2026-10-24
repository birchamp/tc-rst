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

/// The target words of a verse's content, in order, numbered by occurrence as the aligner
/// reads them.
pub fn verse_words(content: &str) -> Vec<TargetWord> {
    tc_usfm::target_words(content)
        .into_iter()
        .map(|w| TargetWord {
            word: w.text,
            occurrence: w.occurrence,
            occurrences: w.occurrences,
            extra: JsonMap::new(),
        })
        .collect()
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

    /// Every target word the record holds: the bank first, then each card's words.
    pub fn all_target_words(&self) -> impl Iterator<Item = &TargetWord> {
        self.word_bank
            .iter()
            .chain(self.alignments.iter().flat_map(|g| g.bottom_words.iter()))
    }

    /// Whether the alignment no longer vouches for `verse`, the words of the run's text in
    /// order ([`verse_words`]): it is stale, or it holds a word, placed or banked, that is
    /// not a word of the verse by text, occurrence and count, or holds one twice. The one
    /// rule of the alignment files and the Align rail (INV-ONE-REVIEW-RULE); the rail adds
    /// only the pin's check (`tc_align::needs_review`).
    pub fn needs_review(&self, at: &AlignedRun, verse: &[TargetWord]) -> bool {
        let key = |w: &TargetWord| (w.word.clone(), w.occurrence, w.occurrences);
        let mut words: std::collections::HashSet<_> = verse.iter().map(key).collect();
        !self.is_valid(at) || !self.all_target_words().all(|w| words.remove(&key(w)))
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
/// `None` unless the markup reads apart and the value's groups woven back into the plain
/// content give `content` byte for byte; such a verse is imported as written.
pub fn split_aligned(content: &str, span_end: VerseNo) -> Option<(String, AlignmentValue)> {
    let verse = tc_usfm::parse_aligned(content).ok()?;
    if verse.groups.is_empty() {
        return None;
    }
    let placed: BTreeSet<(&str, u32)> = verse
        .groups
        .iter()
        .flat_map(|g| g.targets.iter().map(|t| (t.text.as_str(), t.occurrence)))
        .collect();
    let value = AlignmentValue {
        span_end,
        alignments: verse
            .groups
            .iter()
            .map(|g| AlignmentGroup {
                top_words: g.sources.iter().map(SourceWord::from_usfm).collect(),
                bottom_words: g.targets.iter().map(TargetWord::from_usfm).collect(),
                extra: JsonMap::new(),
            })
            .collect(),
        word_bank: verse
            .words
            .iter()
            .filter(|w| !placed.contains(&(w.text.as_str(), w.occurrence)))
            .map(TargetWord::from_usfm)
            .collect(),
        target_verse_md5: crate::decision::verse_text_md5_v1(&verse.plain),
        source_version: "".into(),
        source: None,
        done: false,
        marked_invalid: false,
        dismissed: false,
        extra: JsonMap::new(),
    };
    let woven = tc_usfm::weave(&verse.plain, &value.usfm_groups()).ok()?;
    if woven != content {
        return None;
    }
    let done = value.is_fully_aligned();
    Some((verse.plain, AlignmentValue { done, ..value }))
}
