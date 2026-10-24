//! The stored alignment of one verse or span, and what makes it valid.
//!
//! The value is the journal's `AlignmentValue` (`docs/STORAGE.md` §2.3), with its cards
//! (`AlignmentGroup`: `top_words` the original-language words, `bottom_words` the target
//! words placed on it) and its word bank. A word carries any field tC3 or tC4 wrote on it
//! besides the ones the value names in its own `extra`, so the field goes wherever the word
//! goes, bank or card, as tC4 moves the whole word object.
//!
//! Whether a stored value still vouches for its verse is [`needs_review`]: the journal's rule
//! ([`AlignmentValue::staleness`] against an [`AlignedRun`], §7.7), the one the fold
//! projects `"invalid"` with, the words: a value that holds a word the verse does not
//! have cannot be woven, whatever its hash says (the hash leaves out some words, such as a
//! `\qs` Selah), and the pinned original-language words: a card for a word the pin lacks
//! cannot stand on it, whatever the record's pin says (an imported record names none, tC4
//! #790). It is the one rule of the Align rail, the open verse's notice and the reflow
//! (tC4 #796). It compares the run with the span moved with its text, so a renumbered verse
//! stays valid.
//!
//! A value keeps the rules every edit keeps ([`Alignment::well_formed`]): each target word
//! is held once, in the bank or on one card, and every occurrence lies between 1 and its
//! count.

use std::collections::HashMap;
use tc_journal::alignment::AlignedRun;
use tc_journal::ids::VerseNo;
use tc_journal::record::{AlignmentGroup, AlignmentValue, ResourceRef, SourceWord, TargetWord};
use unicode_normalization::UnicodeNormalization;

use crate::edit::Alignment;
use crate::words::{SourceVerse, TargetVerse};

/// What an alignment is made against besides the target text: the run of verses the
/// aligner shows and the original-language text it aligns to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Basis {
    /// The verse a value of this run is stored under: the anchor of the run's text (its
    /// `from` after a renumber, else its own first verse; §7.7).
    pub anchor: VerseNo,
    /// The first and last verse of the run the aligner shows, in the current numbering.
    pub run_start: VerseNo,
    pub run_end: VerseNo,
    /// tC4's label, such as `dcs::unfoldingWord/el-x-koine_ugnt@v0.34`.
    pub source_version: String,
    /// The original-language pin; `None` only for imports that could not resolve it.
    pub source: Option<ResourceRef>,
}

impl Basis {
    /// A run that was not renumbered: verses `first` to `last`, stored under `first`.
    pub fn run(
        first: VerseNo,
        last: VerseNo,
        source_version: String,
        source: Option<ResourceRef>,
    ) -> Self {
        Self {
            anchor: first,
            run_start: first,
            run_end: last,
            source_version,
            source,
        }
    }

    /// The last verse a value of this run records (`span_end`): the run's end moved to the
    /// anchor's numbering, so that [`AlignmentValue::moved_span_end`] gives the run's end.
    pub fn span_end(&self) -> VerseNo {
        let length = self.run_end.get().saturating_sub(self.run_start.get());
        VerseNo::new(self.anchor.get().saturating_add(length)).unwrap_or(self.anchor)
    }

    /// Whether `value` was made for this run's span, moved with its text (§7.7).
    pub fn fits(&self, value: &AlignmentValue) -> bool {
        value.moved_span_end(self.anchor.get(), self.run_start.get()) == Some(self.run_end.get())
    }

    /// Where this run stands now with `verse` as its text, for [`needs_review`].
    pub fn aligned_run(&self, verse: &TargetVerse) -> AlignedRun {
        AlignedRun {
            anchor: self.anchor.get(),
            run_start: self.run_start.get(),
            run_end: self.run_end.get(),
            text_md5: verse.md5(),
            source_sha: self.source.as_ref().map(|s| s.sha),
        }
    }
}

/// Whether `value` holds exactly the words of `verse`, by text, occurrence and count, placed
/// or banked.
pub fn holds_the_words_of(value: &AlignmentValue, verse: &TargetVerse) -> bool {
    let key = |w: &TargetWord| (w.word.clone(), w.occurrence, w.occurrences);
    let mut held: Vec<_> = value.all_target_words().map(key).collect();
    let mut words: Vec<_> = verse.words().iter().map(key).collect();
    held.sort_unstable();
    words.sort_unstable();
    held == words
}

/// Whether `value`, placed at `run` with `verse` as its text and `pin` as its pinned
/// original-language words, no longer vouches for it: the span, the text or the pin
/// changed, it is marked for re-review, it holds a word, placed or banked, that is not a
/// word of the verse by text, occurrence and count, or holds one twice, or a card does not
/// stand on `pin` ([`Pin::holds`]). A value that holds fewer words than the verse still
/// vouches for its links (tC4 reads a record with no word bank); the aligner banks the rest.
/// `pin` is `None` only where the pinned verse is not available. See the module
/// documentation.
pub fn needs_review(
    value: &AlignmentValue,
    run: &AlignedRun,
    verse: &TargetVerse,
    pin: Option<&SourceVerse>,
) -> bool {
    value.needs_review(run, verse.words()) || pin.is_some_and(|pin| !Pin::new(pin).holds(value))
}

/// The pinned original-language words, to find a card's source words in (tC4 #790). A source
/// word is found by its text and occurrence as written, else in NFC with the pin's words
/// numbered in NFC: the pin numbers two forms of one word apart, and an aligned file or a
/// tC4 4.0 record may write either.
pub(crate) struct Pin<'a> {
    as_written: HashMap<(&'a str, u32), usize>,
    in_nfc: HashMap<(String, u32), usize>,
}

impl<'a> Pin<'a> {
    pub(crate) fn new(source: &'a SourceVerse) -> Self {
        let as_written = source
            .words()
            .iter()
            .enumerate()
            .map(|(at, w)| ((w.word.as_str(), w.occurrence), at))
            .collect();
        let mut seen: HashMap<String, u32> = HashMap::new();
        let in_nfc = source
            .words()
            .iter()
            .enumerate()
            .map(|(at, w)| {
                let text = nfc(&w.word);
                let n = seen.entry(text.clone()).or_default();
                *n += 1;
                ((text, *n), at)
            })
            .collect();
        Self { as_written, in_nfc }
    }

    /// The places in the pin of `card`'s source words, or `None` when the card cannot stand
    /// on the pin: the pin lacks one of its source words, or it holds links and no source
    /// word.
    pub(crate) fn card(&self, card: &AlignmentGroup) -> Option<Vec<usize>> {
        let at = card
            .top_words
            .iter()
            .map(|w| {
                self.as_written
                    .get(&(w.word.as_str(), w.occurrence))
                    .or_else(|| self.in_nfc.get(&(nfc(&w.word), w.occurrence)))
                    .copied()
            })
            .collect::<Option<Vec<_>>>()?;
        (!at.is_empty() || card.bottom_words.is_empty()).then_some(at)
    }

    /// Whether every card of `value` stands on the pin. A value whose source words carry
    /// `x-ref` (per-verse occurrences in a span, #409) is not compared.
    pub(crate) fn holds(&self, value: &AlignmentValue) -> bool {
        counts_by_verse(value) || value.alignments.iter().all(|g| self.card(g).is_some())
    }
}

/// Whether `value`'s source words carry `x-ref`: their occurrences are counted in their own
/// verse of a span (#409), not in the pinned span.
pub(crate) fn counts_by_verse(value: &AlignmentValue) -> bool {
    value
        .alignments
        .iter()
        .flat_map(|g| &g.top_words)
        .any(|w| w.extra.contains_key("x-ref"))
}

fn nfc(text: &str) -> String {
    text.nfc().collect()
}

/// How a value breaks the rules every edit keeps. See the module documentation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Malformed {
    #[error("the target word {:?} (occurrence {}) is held in more than one place", .0.word, .0.occurrence)]
    TargetWordTwice(TargetWord),
    #[error("the target word {:?} has occurrence {} of {}", .0.word, .0.occurrence, .0.occurrences)]
    TargetOccurrence(TargetWord),
    #[error("the original word {:?} has occurrence {} of {}", .0.word, .0.occurrence, .0.occurrences)]
    SourceOccurrence(Box<SourceWord>),
}

/// A verse's state on the Align rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerseState {
    /// The verse has no text yet.
    Undrafted,
    /// Not done: nothing aligned yet, or words left to place.
    Todo,
    /// Done (D73) and still vouching for the verse.
    Valid,
    /// The alignment no longer vouches for the verse; it is kept for review.
    Invalid,
}

/// One row of the Align rail: the state and the words placed out of the words held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerseStatus {
    pub state: VerseState,
    pub placed: usize,
    pub total: usize,
}

/// tC4's rail status (`alignVerseStatus`). `current` is where the value is placed and the
/// verse it is placed on, `None` for a verse with no text, and `pin` the pinned
/// original-language words. [`needs_review`] outranks `done`; `done` alone makes a verse
/// valid, so a fully placed record that was never marked done is still to do.
pub fn verse_status(
    value: Option<&AlignmentValue>,
    current: Option<(&AlignedRun, &TargetVerse)>,
    pin: Option<&SourceVerse>,
) -> VerseStatus {
    let Some((run, verse)) = current else {
        return VerseStatus {
            state: VerseState::Undrafted,
            placed: 0,
            total: 0,
        };
    };
    let (placed, total) = value.map_or((0, 0), |v| (v.placed(), v.total()));
    let state = match value {
        Some(v) if needs_review(v, run, verse, pin) => VerseState::Invalid,
        Some(v) if v.done => VerseState::Valid,
        _ => VerseState::Todo,
    };
    VerseStatus {
        state,
        placed,
        total,
    }
}
