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
//! projects `"invalid"` with, and the words: a value that holds a word the verse does not
//! have cannot be woven, whatever its hash says (the hash leaves out some words, such as a
//! `\qs` Selah). It is the one rule of the Align rail, the open verse's notice and the reflow
//! (tC4 #796). It compares the run with the span moved with its text, so a renumbered verse
//! stays valid.
//!
//! A value keeps the rules every edit keeps ([`Alignment::well_formed`]): each target word
//! is held once, in the bank or on one card, and every occurrence lies between 1 and its
//! count.

use std::collections::HashSet;
use tc_journal::alignment::AlignedRun;
use tc_journal::ids::VerseNo;
use tc_journal::record::{AlignmentValue, ResourceRef, SourceWord, TargetWord};

use crate::edit::Alignment;
use crate::words::TargetVerse;

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

/// Whether `value`, placed at `run` with `verse` as its text, no longer vouches for it: the
/// span, the text or the pin changed, it is marked for re-review, or it holds a word, placed
/// or banked, that is not a word of the verse by text, occurrence and count, or holds one
/// twice. A value that holds fewer words than the verse still vouches for its links (tC4
/// reads a record with no word bank); the aligner banks the rest. See the module
/// documentation.
pub fn needs_review(value: &AlignmentValue, run: &AlignedRun, verse: &TargetVerse) -> bool {
    let key = |w: &TargetWord| (w.word.clone(), w.occurrence, w.occurrences);
    let mut words: HashSet<_> = verse.words().iter().map(key).collect();
    !value.is_valid(run) || !value.all_target_words().all(|w| words.remove(&key(w)))
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
/// verse it is placed on, `None` for a verse with no text. [`needs_review`] outranks `done`;
/// `done` alone makes a verse valid, so a fully placed record that was never marked done
/// is still to do.
pub fn verse_status(
    value: Option<&AlignmentValue>,
    current: Option<(&AlignedRun, &TargetVerse)>,
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
        Some(v) if needs_review(v, run, verse) => VerseState::Invalid,
        Some(v) if v.done => VerseState::Valid,
        _ => VerseState::Todo,
    };
    VerseStatus {
        state,
        placed,
        total,
    }
}
