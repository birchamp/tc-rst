//! Editing one verse's alignment: the operations behind the Align workspace.
//!
//! Each operation either changes the value or refuses with an [`EditError`] and leaves it
//! as it was. Together they keep these invariants, as tC4's `edit.ts` does:
//!
//! - A target word is in exactly one place: the bank or one card. A word reaches a card
//!   only from the bank ([`Alignment::link`], [`Alignment::link_many`]) or from
//!   another card ([`Alignment::move_word`]), and leaves a card only for the bank or
//!   another card.
//! - Cards stay in original-language verse order. Only neighbouring cards merge, the
//!   lower one keeping its place with the words of both in card order, so every phrase
//!   is a contiguous run of the original verse. A split puts the phrase's words back as
//!   single cards in place, and the placed target words stay on the first of them.
//!
//! The word bank and each card's target words are in the order the words stand in the
//! verse ([`Alignment::bank_in_verse_order`], [`Alignment::cards_in_verse_order`]):
//! a repeated word by its occurrence, and a word the verse no longer has last. A word sent
//! back to the bank returns to its place in the verse and the whole bank is put in verse
//! order, so in a bank already in verse order no other word moves
//! ([`Alignment::unlink`]); [`Alignment::clear`] also leaves the whole bank in
//! verse order. tC4 brought this in with 3d43f7c and 91131d9 (#734, the bank) and 26aded2
//! (#754, the cards), as tC3 kept them. The aligner ([`crate::Aligner`]) puts the bank and
//! every card in verse order after each edit and when a verse opens; the operations here
//! sort only where tC4's do, so a placed word joins its card at the end until the aligner
//! sorts it. [`bootstrap`] keeps tC4's record of an unaligned verse, which
//! banks the words grouped by occurrence ([`TargetVerse::bank_order`]); the aligner shows
//! that bank in verse order too, from the first view of the verse.
//!
//! The value is the journal's `AlignmentValue`; the operations are the methods of the
//! [`Alignment`] trait on it.
//!
//! None of these touch the hash, `done` or `marked_invalid`: [`Alignment::settle`]
//! does, after each edit, as tC4's aligner does through `stampTargetVerse` and
//! `settleDone`.

use std::collections::HashSet;

use tc_journal::json::JsonMap;
use tc_journal::record::{AlignmentGroup, AlignmentValue, SourceWord, TargetWord};

use crate::value::{Basis, Malformed};
use crate::words::{SourceVerse, TargetVerse, WordId, same_word};

/// Why an edit was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    #[error("there is no card {0}")]
    NoSuchCard(usize),
    #[error("the word {:?} (occurrence {}) is not in the word bank", .0.word, .0.occurrence)]
    NotInBank(TargetWord),
    #[error("the word {:?} (occurrence {}) is not on card {card}", .word.word, .word.occurrence)]
    NotOnCard { card: usize, word: TargetWord },
    #[error("a word cannot move onto the card it is on")]
    SameCard,
    #[error("cards {0} and {1} are not next to each other; only neighbouring cards merge")]
    NotAdjacent(usize, usize),
    #[error("card {0} holds one original word; only a phrase splits")]
    NotAPhrase(usize),
    #[error(
        "card {0} holds fields this version does not know; joining or splitting it would lose them"
    )]
    CardFields(usize),
    #[error("there is no suggestion {0}")]
    NoSuchProposal(usize),
    #[error("none of the words is in the word bank")]
    NothingToPlace,
}

/// A card for one source word, with nothing placed.
pub fn card(source: SourceWord) -> AlignmentGroup {
    AlignmentGroup {
        top_words: vec![source],
        bottom_words: Vec::new(),
        extra: JsonMap::new(),
    }
}

/// What an unaligned verse starts from: one card per original word in verse order, nothing
/// placed, and every target word in the bank in tC4's bootstrap order. The value is made
/// against `target`'s text. tC4's `bootstrapVerse`.
pub fn bootstrap(source: &SourceVerse, target: &TargetVerse, basis: Basis) -> AlignmentValue {
    AlignmentValue {
        span_end: basis.span_end(),
        alignments: source.words().iter().cloned().map(card).collect(),
        word_bank: target.bank_order(),
        target_verse_md5: target.md5(),
        source_version: basis.source_version.into(),
        source: basis.source,
        done: false,
        marked_invalid: false,
        dismissed: false,
        extra: JsonMap::new(),
    }
}

/// The reads and edits of one verse's alignment, on the journal's value. See the module
/// documentation.
pub trait Alignment {
    /// Target words placed on any card.
    fn placed(&self) -> usize;
    /// Every target word the record holds, placed or banked.
    fn total(&self) -> usize;
    /// Every target word the record holds: the bank first, then each card's words.
    fn all_target_words(&self) -> impl Iterator<Item = &TargetWord>;
    /// The first rule the value breaks, if any. See [`crate::value`].
    fn well_formed(&self) -> Result<(), Malformed>;
    /// The I-3 check alone: was this made against a different text than `verse`?
    fn is_stale_against(&self, verse: &TargetVerse) -> bool;

    /// Place a banked word on a card. The word leaves the bank, so it can never be on two
    /// cards. tC4's `linkWord`.
    fn link(&mut self, card: usize, word: &TargetWord) -> Result<(), EditError>;
    /// Place several banked words on one card, in the order given, and return how many
    /// were placed. A word that is no longer in the bank is skipped, as tC4's reduce over
    /// `linkWord` skips it (`src/state.jsx:5249-5261` at d310bf3); the edit is refused,
    /// changing nothing, only when no word could be placed.
    fn link_many(&mut self, card: usize, words: &[TargetWord]) -> Result<usize, EditError>;
    /// Send a placed word back to the bank, at its place in `verse`, and put the whole
    /// bank in verse order, so a bank stored in another order comes back in verse order
    /// too. A word the verse no longer has goes last. tC4's `unlinkWord` (#734).
    fn unlink(
        &mut self,
        card: usize,
        word: &TargetWord,
        verse: &TargetVerse,
    ) -> Result<(), EditError>;
    /// Move a placed word from one card to another in one step; the bank is untouched.
    /// tC4's `moveWord`.
    fn move_word(&mut self, from: usize, to: usize, word: &TargetWord) -> Result<(), EditError>;
    /// Join two neighbouring cards into one phrase. The lower card keeps its place;
    /// source and target words join in card order, whichever card was dragged. tC4's
    /// `mergeAlignments`.
    fn merge(&mut self, a: usize, b: usize) -> Result<(), EditError>;
    /// Split a phrase back into one card per source word, in place. Its placed words stay
    /// on the first of them. tC4's `splitAlignment`.
    fn split(&mut self, card: usize) -> Result<(), EditError>;
    /// Start the verse over: every phrase split into single cards, every placed word back
    /// in the bank, and the whole bank in verse order for `verse`, whatever order it was
    /// in. Banked words the verse no longer has stay, last. tC4 has no such action; the
    /// order is the owner's ruling for every bank (Q15 of the 2026-10-16 delta). A card
    /// with fields this version does not know keeps them on the first of its cards.
    fn clear(&mut self, verse: &TargetVerse);
    /// Put the word bank in the order its words stand in `verse`; a word the verse does not
    /// have goes last. tC4's `inVerseOrder` (#734).
    fn bank_in_verse_order(&mut self, verse: &TargetVerse);
    /// Put each card's target words in the order they stand in `verse`; a word the verse
    /// does not have goes last in its card. Only the target words move. tC4's
    /// `cardsInVerseOrder` (#754).
    fn cards_in_verse_order(&mut self, verse: &TargetVerse);
    /// After an edit made against `verse`: the record now vouches for that text, the
    /// re-review flag stays, and `done` is recomputed: set when the verse is fully aligned
    /// and not flagged, cleared otherwise, so an edit takes back an earlier Mark valid.
    /// tC4's `settleDone(stampTargetVerse(record, text))`.
    fn settle(&mut self, verse: &TargetVerse);
    /// Mark valid: the translator declares the verse done, words left in the bank or not.
    /// The record vouches for `verse` and its re-review flag is cleared. tC4's `markDone`.
    fn mark_done(&mut self, verse: &TargetVerse);
}

fn card_at(value: &AlignmentValue, card: usize) -> Result<&AlignmentGroup, EditError> {
    value
        .alignments
        .get(card)
        .ok_or(EditError::NoSuchCard(card))
}

impl Alignment for AlignmentValue {
    fn placed(&self) -> usize {
        self.alignments.iter().map(|g| g.bottom_words.len()).sum()
    }

    fn total(&self) -> usize {
        self.placed() + self.word_bank.len()
    }

    fn all_target_words(&self) -> impl Iterator<Item = &TargetWord> {
        self.word_bank
            .iter()
            .chain(self.alignments.iter().flat_map(|g| g.bottom_words.iter()))
    }

    fn well_formed(&self) -> Result<(), Malformed> {
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
            .flat_map(|g| &g.top_words)
            .find(|w| !(1..=w.occurrences).contains(&w.occurrence))
        {
            return Err(Malformed::SourceOccurrence(Box::new(word.clone())));
        }
        Ok(())
    }

    fn is_stale_against(&self, verse: &TargetVerse) -> bool {
        self.target_verse_md5 != verse.md5()
    }

    fn link(&mut self, card: usize, word: &TargetWord) -> Result<(), EditError> {
        card_at(self, card)?;
        let at = self
            .word_bank
            .iter()
            .position(|w| same_word(w, word))
            .ok_or_else(|| EditError::NotInBank(word.clone()))?;
        let banked = self.word_bank.remove(at);
        self.alignments[card].bottom_words.push(banked);
        Ok(())
    }

    fn link_many(&mut self, card: usize, words: &[TargetWord]) -> Result<usize, EditError> {
        card_at(self, card)?;
        let placed = words.iter().filter(|w| self.link(card, w).is_ok()).count();
        if placed == 0 {
            return Err(EditError::NothingToPlace);
        }
        Ok(placed)
    }

    fn unlink(
        &mut self,
        card: usize,
        word: &TargetWord,
        verse: &TargetVerse,
    ) -> Result<(), EditError> {
        let at = card_at(self, card)?
            .bottom_words
            .iter()
            .position(|w| same_word(w, word))
            .ok_or_else(|| EditError::NotOnCard {
                card,
                word: word.clone(),
            })?;
        let returned = self.alignments[card].bottom_words.remove(at);
        self.word_bank.push(returned);
        self.bank_in_verse_order(verse);
        Ok(())
    }

    fn move_word(&mut self, from: usize, to: usize, word: &TargetWord) -> Result<(), EditError> {
        card_at(self, to)?;
        let at = card_at(self, from)?
            .bottom_words
            .iter()
            .position(|w| same_word(w, word))
            .ok_or_else(|| EditError::NotOnCard {
                card: from,
                word: word.clone(),
            })?;
        if from == to {
            return Err(EditError::SameCard);
        }
        let moved = self.alignments[from].bottom_words.remove(at);
        self.alignments[to].bottom_words.push(moved);
        Ok(())
    }

    fn merge(&mut self, a: usize, b: usize) -> Result<(), EditError> {
        card_at(self, a)?;
        card_at(self, b)?;
        if a.abs_diff(b) != 1 {
            return Err(EditError::NotAdjacent(a, b));
        }
        let (keep, drop) = (a.min(b), a.max(b));
        if !self.alignments[drop].extra.is_empty() {
            return Err(EditError::CardFields(drop));
        }
        let second = self.alignments.remove(drop);
        let first = &mut self.alignments[keep];
        first.top_words.extend(second.top_words);
        first.bottom_words.extend(second.bottom_words);
        Ok(())
    }

    fn split(&mut self, card: usize) -> Result<(), EditError> {
        let phrase = card_at(self, card)?;
        if phrase.top_words.len() < 2 {
            return Err(EditError::NotAPhrase(card));
        }
        let phrase = self.alignments.remove(card);
        let mut singles: Vec<AlignmentGroup> =
            phrase.top_words.into_iter().map(self::card).collect();
        singles[0].bottom_words = phrase.bottom_words;
        singles[0].extra = phrase.extra;
        self.alignments.splice(card..card, singles);
        Ok(())
    }

    fn clear(&mut self, verse: &TargetVerse) {
        let groups = std::mem::take(&mut self.alignments);
        for group in groups {
            self.word_bank.extend(group.bottom_words);
            let mut singles: Vec<AlignmentGroup> =
                group.top_words.into_iter().map(self::card).collect();
            match singles.first_mut() {
                Some(first) => first.extra = group.extra,
                // A card with no original word aligns nothing, but its fields are kept.
                None if !group.extra.is_empty() => singles.push(AlignmentGroup {
                    top_words: Vec::new(),
                    bottom_words: Vec::new(),
                    extra: group.extra,
                }),
                None => {}
            }
            self.alignments.extend(singles);
        }
        self.bank_in_verse_order(verse);
    }

    fn bank_in_verse_order(&mut self, verse: &TargetVerse) {
        self.sort_bank_by(&verse.order());
    }

    fn cards_in_verse_order(&mut self, verse: &TargetVerse) {
        self.sort_cards_by(&verse.order());
    }

    fn settle(&mut self, verse: &TargetVerse) {
        self.target_verse_md5 = verse.md5();
        // Only Mark valid clears a "needs review" mark (owner ruling Q37, tC4 #814).
        self.done = self.is_fully_aligned() && !self.marked_invalid;
    }

    fn mark_done(&mut self, verse: &TargetVerse) {
        self.target_verse_md5 = verse.md5();
        self.marked_invalid = false;
        self.done = true;
    }
}
