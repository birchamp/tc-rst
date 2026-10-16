//! The stored alignment of one verse or span, and what makes it valid.
//!
//! [`AlignmentValue`] has the fields of STORAGE.md §2.3 `AlignmentValue`, one for one, so
//! `tc-journal` maps it field by field:
//!
//! | here | STORAGE.md `AlignmentValue` | tC4 §5.1 record |
//! |---|---|---|
//! | `span_end` | `span_end` | the verse key's last verse |
//! | `alignments` | `alignments` | `alignments` (`sources` = `topWords`, `targets` = `bottomWords`) |
//! | `word_bank` | `word_bank` | `wordBank` |
//! | `target_verse_md5` | `target_verse_md5` | `targetVerseMd5` |
//! | `source_version` | `source_version` | `sourceVersion` |
//! | `source` | `source` | (none: the original-language pin, repoPath + sha) |
//! | `done` | `done` | `done` (absent means false) |
//! | `marked_invalid` | `marked_invalid` | `invalid` as tC3/tC4 stored it |
//! | `dismissed` | `dismissed` | (none) |
//! | `extra` | `extra` | any other field |
//! | `word_extra` | each `TargetWord`'s `extra`, by the word's text and occurrence | a target word's other fields |
//!
//! Occurrences are integers (I-2). `invalid` is never stored: [`AlignmentValue::staleness`]
//! computes it as STORAGE.md §7.7 does. `tc-journal`'s per-group `extra` is always empty
//! for a value from here: [`crate::record`] refuses a field on an alignment, since a
//! merge or split could not say which card it belongs to.
//!
//! A value keeps the rules every edit keeps ([`AlignmentValue::well_formed`]): each target
//! word is held once, in the bank or on one card, and every occurrence lies between 1
//! and its count. That is what lets `word_extra` be keyed by the word.

use crate::Error;
use crate::words::{TargetVerse, WordId};
use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::num::NonZeroU16;
use std::str::FromStr;
use tc_usfm::{SourceWord, TargetWord};

/// One entry of a verse's `alignments`: original-language words joined into one card,
/// and the target words placed on it. A group with no targets is a card nothing is
/// placed on yet. Groups stand in original-language verse order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// `topWords`: one source word, or several for a phrase.
    pub sources: Vec<SourceWord>,
    /// `bottomWords`: the placed target words. The aligner shows them in verse order; a
    /// value saved by an older tC4 or imported keeps the order it came in until the verse
    /// is edited or marked valid (Q19).
    pub targets: Vec<TargetWord>,
}

impl Group {
    /// A card for one source word, with nothing placed.
    pub fn single(source: SourceWord) -> Self {
        Self {
            sources: vec![source],
            targets: Vec::new(),
        }
    }
}

/// A git commit id: 40 lowercase hexadecimal digits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommitSha([u8; 20]);

impl FromStr for CommitSha {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        crate::parse_hex(s)
            .map(Self)
            .ok_or_else(|| Error::InvalidSha(s.to_owned()))
    }
}

impl fmt::Display for CommitSha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        crate::write_hex(f, &self.0)
    }
}

impl fmt::Debug for CommitSha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CommitSha({self})")
    }
}

/// A pinned resource: its repository path and commit. STORAGE.md's `ResourceRef`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceRef {
    /// Such as `git.door43.org/unfoldingWord/el-x-koine_ugnt`. Compared case-insensitively.
    pub repo_path: String,
    pub sha: CommitSha,
}

/// What an alignment is made against besides the target text: the last verse of the span
/// the aligner showed, and the original-language text it aligns to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Basis {
    pub span_end: NonZeroU16,
    /// tC4's label, such as `dcs::unfoldingWord/el-x-koine_ugnt@v0.34`.
    pub source_version: String,
    /// The original-language pin; `None` only for imports that could not resolve it.
    pub source: Option<ResourceRef>,
}

/// The stored alignment of one verse or span. See the module documentation.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignmentValue {
    pub span_end: NonZeroU16,
    pub alignments: Vec<Group>,
    pub word_bank: Vec<TargetWord>,
    /// The md5 of the verse's plain text when the alignment was made (I-3).
    pub target_verse_md5: crate::TextMd5,
    pub source_version: String,
    pub source: Option<ResourceRef>,
    /// The translator's "this verse is done" (D73): set when every word is placed, or by
    /// Mark valid; any later edit recomputes it.
    pub done: bool,
    /// tC3 or tC4 marked the record for re-review; cleared by Mark valid or a new edit.
    pub marked_invalid: bool,
    /// Dismissed from the Detached list (STORAGE.md §7.7); kept.
    pub dismissed: bool,
    /// Fields this version does not know, kept as they were.
    pub extra: serde_json::Map<String, serde_json::Value>,
    /// Fields a target word carried besides `word`, `occurrence` and `occurrences`, by the
    /// word they belong to: such as the `"type": "bottomWord"` that tC3 wrote on banked
    /// words and tC4 keeps on import. They go wherever the word goes, bank or card, as
    /// tC4 moves the whole word object.
    pub word_extra: BTreeMap<WordId, serde_json::Map<String, serde_json::Value>>,
}

/// How a value breaks the rules every edit keeps. See the module documentation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Malformed {
    #[error("the target word {:?} (occurrence {}) is held in more than one place", .0.text, .0.occurrence)]
    TargetWordTwice(TargetWord),
    #[error("the target word {:?} has occurrence {} of {}", .0.text, .0.occurrence, .0.occurrences)]
    TargetOccurrence(TargetWord),
    #[error("the original word {:?} has occurrence {} of {}", .0.content, .0.occurrence, .0.occurrences)]
    SourceOccurrence(Box<SourceWord>),
    #[error("fields are kept for the target word {:?} (occurrence {}), which the value does not hold", .0.text, .0.occurrence)]
    ExtraOfNoWord(WordId),
}

/// What the projection holds now for the verse an alignment is placed at (STORAGE.md §7.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Current<'a> {
    /// The last verse of the projected run the record is placed at.
    pub run_end: NonZeroU16,
    /// The md5 of the run's plain text now.
    pub text_md5: crate::TextMd5,
    /// The sha of the current original-language pin for the book's testament, if pinned.
    pub source_sha: Option<&'a CommitSha>,
}

/// Why an alignment no longer vouches for the verse. A record with any of these projects
/// `"invalid": true` and is kept as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stale {
    /// The verse was joined into or split from a span since the alignment was made.
    SpanChanged,
    /// The verse text changed (the I-3 hash differs).
    TextChanged,
    /// The original-language text was upgraded or unpinned.
    SourceChanged,
    /// tC3 or tC4 marked the record invalid.
    MarkedInvalid,
}

impl AlignmentValue {
    /// Every reason this alignment does not vouch for `current`, in a fixed order. Empty
    /// means valid.
    pub fn staleness(&self, current: &Current<'_>) -> Vec<Stale> {
        let mut reasons = Vec::new();
        if self.span_end != current.run_end {
            reasons.push(Stale::SpanChanged);
        }
        if self.target_verse_md5 != current.text_md5 {
            reasons.push(Stale::TextChanged);
        }
        if let Some(source) = &self.source
            && current.source_sha != Some(&source.sha)
        {
            reasons.push(Stale::SourceChanged);
        }
        if self.marked_invalid {
            reasons.push(Stale::MarkedInvalid);
        }
        reasons
    }

    pub fn is_valid(&self, current: &Current<'_>) -> bool {
        self.staleness(current).is_empty()
    }

    /// The I-3 check alone: was this made against a different text than `verse`?
    pub fn is_stale_against(&self, verse: &TargetVerse) -> bool {
        self.target_verse_md5 != verse.md5()
    }

    /// Target words placed on any card.
    pub fn placed(&self) -> usize {
        self.alignments.iter().map(|g| g.targets.len()).sum()
    }

    /// Every target word the record holds, placed or banked.
    pub fn total(&self) -> usize {
        self.placed() + self.word_bank.len()
    }

    /// Every target word the record holds: the bank first, then each card's words.
    pub fn all_target_words(&self) -> impl Iterator<Item = &TargetWord> {
        self.word_bank
            .iter()
            .chain(self.alignments.iter().flat_map(|g| g.targets.iter()))
    }

    /// The first rule the value breaks, if any. See the module documentation.
    pub fn well_formed(&self) -> Result<(), Malformed> {
        let mut held = HashSet::new();
        for word in self.all_target_words() {
            if !(1..=word.occurrences).contains(&word.occurrence) {
                return Err(Malformed::TargetOccurrence(word.clone()));
            }
            if !held.insert(WordId::of(word)) {
                return Err(Malformed::TargetWordTwice(word.clone()));
            }
        }
        if let Some(word) = self
            .alignments
            .iter()
            .flat_map(|g| &g.sources)
            .find(|w| !(1..=w.occurrences).contains(&w.occurrence))
        {
            return Err(Malformed::SourceOccurrence(Box::new(word.clone())));
        }
        if let Some(id) = self.word_extra.keys().find(|id| !held.contains(*id)) {
            return Err(Malformed::ExtraOfNoWord(id.clone()));
        }
        Ok(())
    }

    /// Forget the extra fields of words the value no longer holds.
    pub(crate) fn retain_word_extra(&mut self) {
        let held: HashSet<WordId> = self.all_target_words().map(WordId::of).collect();
        self.word_extra.retain(|id, _| held.contains(id));
    }

    /// tC4's "fully aligned" (tC3's `areAlgnmentsComplete`): the bank is empty, some card
    /// has a word, and no card with source words is left without one. An empty record is
    /// not aligned.
    pub fn is_fully_aligned(&self) -> bool {
        self.word_bank.is_empty()
            && self.alignments.iter().any(|g| !g.targets.is_empty())
            && self
                .alignments
                .iter()
                .all(|g| g.sources.is_empty() || !g.targets.is_empty())
    }
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

/// tC4's rail status (`alignVerseStatus`). `current` is `None` for a verse with no text.
/// Staleness outranks `done`; `done` alone makes a verse valid, so a fully placed record
/// that was never marked done is still to do.
pub fn verse_status(value: Option<&AlignmentValue>, current: Option<&Current<'_>>) -> VerseStatus {
    let Some(current) = current else {
        return VerseStatus {
            state: VerseState::Undrafted,
            placed: 0,
            total: 0,
        };
    };
    let (placed, total) = value.map_or((0, 0), |v| (v.placed(), v.total()));
    let state = match value {
        Some(v) if !v.is_valid(current) => VerseState::Invalid,
        Some(v) if v.done => VerseState::Valid,
        _ => VerseState::Todo,
    };
    VerseStatus {
        state,
        placed,
        total,
    }
}
