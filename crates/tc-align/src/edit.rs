//! Editing one verse's alignment: the operations behind the Align workspace.
//!
//! Each operation either changes the value or refuses with an [`EditError`] and leaves it
//! as it was. Together they keep these invariants, as tC4's `edit.ts` does:
//!
//! - A target word is in exactly one place: the bank or one card. A word reaches a card
//!   only from the bank ([`AlignmentValue::link`], [`AlignmentValue::link_many`]) or from
//!   another card ([`AlignmentValue::move_word`]), and leaves a card only for the bank or
//!   another card.
//! - Cards stay in original-language verse order. Only neighbouring cards merge, the
//!   lower one keeping its place with the words of both in card order, so every phrase
//!   is a contiguous run of the original verse. A split puts the phrase's words back as
//!   single cards in place, and the placed target words stay on the first of them.
//!
//! The word bank and each card's target words are in the order the words stand in the
//! verse ([`AlignmentValue::bank_in_verse_order`], [`AlignmentValue::cards_in_verse_order`]):
//! a repeated word by its occurrence, and a word the verse no longer has last. A word sent
//! back to the bank returns to its place in the verse and the whole bank is put in verse
//! order, so in a bank already in verse order no other word moves
//! ([`AlignmentValue::unlink`]); [`AlignmentValue::clear`] also leaves the whole bank in
//! verse order. tC4 brought this in with 3d43f7c and 91131d9 (#734, the bank) and 26aded2
//! (#754, the cards), as tC3 kept them. The aligner ([`crate::Aligner`]) puts the bank and
//! every card in verse order after each edit and when a verse opens; the operations here
//! sort only where tC4's do, so a placed word joins its card at the end until the aligner
//! sorts it. [`AlignmentValue::bootstrap`] keeps tC4's record of an unaligned verse, which
//! banks the words grouped by occurrence ([`TargetVerse::bank_order`]); the aligner shows
//! that bank in verse order too, from the first view of the verse.
//!
//! None of these touch the hash, `done` or `marked_invalid`: [`AlignmentValue::settle`]
//! does, after each edit, as tC4's aligner does through `stampTargetVerse` and
//! `settleDone`.

use crate::value::{AlignmentValue, Basis, Group};
use crate::words::{SourceVerse, TargetVerse, same_word};
use tc_usfm::TargetWord;

/// Why an edit was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    #[error("there is no card {0}")]
    NoSuchCard(usize),
    #[error("the word {:?} (occurrence {}) is not in the word bank", .0.text, .0.occurrence)]
    NotInBank(TargetWord),
    #[error("the word {:?} (occurrence {}) is not on card {card}", .word.text, .word.occurrence)]
    NotOnCard { card: usize, word: TargetWord },
    #[error("a word cannot move onto the card it is on")]
    SameCard,
    #[error("cards {0} and {1} are not next to each other; only neighbouring cards merge")]
    NotAdjacent(usize, usize),
    #[error("card {0} holds one original word; only a phrase splits")]
    NotAPhrase(usize),
    #[error("there is no suggestion {0}")]
    NoSuchProposal(usize),
    #[error("none of the words is in the word bank")]
    NothingToPlace,
}

impl AlignmentValue {
    /// What an unaligned verse starts from: one card per original word in verse order,
    /// nothing placed, and every target word in the bank in tC4's bootstrap order. The
    /// value is made against `target`'s text. tC4's `bootstrapVerse`.
    pub fn bootstrap(source: &SourceVerse, target: &TargetVerse, basis: Basis) -> Self {
        Self {
            span_end: basis.span_end,
            alignments: source.words().iter().cloned().map(Group::single).collect(),
            word_bank: target.bank_order(),
            target_verse_md5: target.md5(),
            source_version: basis.source_version,
            source: basis.source,
            done: false,
            marked_invalid: false,
            dismissed: false,
            extra: serde_json::Map::new(),
            word_extra: Default::default(),
        }
    }

    fn card(&self, card: usize) -> Result<&Group, EditError> {
        self.alignments.get(card).ok_or(EditError::NoSuchCard(card))
    }

    /// Place a banked word on a card. The word leaves the bank, so it can never be on two
    /// cards. tC4's `linkWord`.
    pub fn link(&mut self, card: usize, word: &TargetWord) -> Result<(), EditError> {
        self.card(card)?;
        let at = self
            .word_bank
            .iter()
            .position(|w| same_word(w, word))
            .ok_or_else(|| EditError::NotInBank(word.clone()))?;
        let banked = self.word_bank.remove(at);
        self.alignments[card].targets.push(banked);
        Ok(())
    }

    /// Place several banked words on one card, in the order given, and return how many
    /// were placed. A word that is no longer in the bank is skipped, as tC4's reduce over
    /// `linkWord` skips it (`src/state.jsx:5249-5261` at d310bf3); the edit is refused,
    /// changing nothing, only when no word could be placed.
    pub fn link_many(&mut self, card: usize, words: &[TargetWord]) -> Result<usize, EditError> {
        self.card(card)?;
        let placed = words.iter().filter(|w| self.link(card, w).is_ok()).count();
        if placed == 0 {
            return Err(EditError::NothingToPlace);
        }
        Ok(placed)
    }

    /// Send a placed word back to the bank, at its place in `verse`, and put the whole
    /// bank in verse order, so a bank stored in another order comes back in verse order
    /// too. A word the verse no longer has goes last. tC4's `unlinkWord` (#734).
    pub fn unlink(
        &mut self,
        card: usize,
        word: &TargetWord,
        verse: &TargetVerse,
    ) -> Result<(), EditError> {
        let at = self
            .card(card)?
            .targets
            .iter()
            .position(|w| same_word(w, word))
            .ok_or_else(|| EditError::NotOnCard {
                card,
                word: word.clone(),
            })?;
        let returned = self.alignments[card].targets.remove(at);
        self.word_bank.push(returned);
        self.bank_in_verse_order(verse);
        Ok(())
    }

    /// Move a placed word from one card to another in one step; the bank is untouched.
    /// tC4's `moveWord`.
    pub fn move_word(
        &mut self,
        from: usize,
        to: usize,
        word: &TargetWord,
    ) -> Result<(), EditError> {
        self.card(to)?;
        let at = self
            .card(from)?
            .targets
            .iter()
            .position(|w| same_word(w, word))
            .ok_or_else(|| EditError::NotOnCard {
                card: from,
                word: word.clone(),
            })?;
        if from == to {
            return Err(EditError::SameCard);
        }
        let moved = self.alignments[from].targets.remove(at);
        self.alignments[to].targets.push(moved);
        Ok(())
    }

    /// Join two neighbouring cards into one phrase. The lower card keeps its place;
    /// source and target words join in card order, whichever card was dragged. tC4's
    /// `mergeAlignments`.
    pub fn merge(&mut self, a: usize, b: usize) -> Result<(), EditError> {
        self.card(a)?;
        self.card(b)?;
        if a.abs_diff(b) != 1 {
            return Err(EditError::NotAdjacent(a, b));
        }
        let (keep, drop) = (a.min(b), a.max(b));
        let second = self.alignments.remove(drop);
        let first = &mut self.alignments[keep];
        first.sources.extend(second.sources);
        first.targets.extend(second.targets);
        Ok(())
    }

    /// Split a phrase back into one card per source word, in place. Its placed words stay
    /// on the first of them. tC4's `splitAlignment`.
    pub fn split(&mut self, card: usize) -> Result<(), EditError> {
        if self.card(card)?.sources.len() < 2 {
            return Err(EditError::NotAPhrase(card));
        }
        let phrase = self.alignments.remove(card);
        let mut singles: Vec<Group> = phrase.sources.into_iter().map(Group::single).collect();
        singles[0].targets = phrase.targets;
        self.alignments.splice(card..card, singles);
        Ok(())
    }

    /// Start the verse over: every phrase split into single cards, every placed word back
    /// in the bank, and the whole bank in verse order for `verse`, whatever order it was
    /// in. Banked words the verse no longer has stay, last. tC4 has no such action; the
    /// order is the owner's ruling for every bank (Q15 of the 2026-10-16 delta).
    pub fn clear(&mut self, verse: &TargetVerse) {
        let groups = std::mem::take(&mut self.alignments);
        for group in groups {
            self.word_bank.extend(group.targets);
            self.alignments
                .extend(group.sources.into_iter().map(Group::single));
        }
        self.bank_in_verse_order(verse);
    }

    /// Put the word bank in the order its words stand in `verse`; a word the verse does not
    /// have goes last. tC4's `inVerseOrder` (#734).
    pub fn bank_in_verse_order(&mut self, verse: &TargetVerse) {
        verse.sort_in_verse_order(&mut self.word_bank);
    }

    /// Put each card's target words in the order they stand in `verse`; a word the verse
    /// does not have goes last in its card. Only the target words move. tC4's
    /// `cardsInVerseOrder` (#754).
    pub fn cards_in_verse_order(&mut self, verse: &TargetVerse) {
        for group in &mut self.alignments {
            verse.sort_in_verse_order(&mut group.targets);
        }
    }

    /// After an edit made against `verse`: the record now vouches for that text, tC3's
    /// and tC4's re-review flag is cleared, and `done` is recomputed: set when the verse
    /// is fully aligned, cleared otherwise, so an edit takes back an earlier Mark valid.
    /// tC4's `settleDone(stampTargetVerse(record, text))`.
    pub fn settle(&mut self, verse: &TargetVerse) {
        self.target_verse_md5 = verse.md5();
        self.marked_invalid = false;
        self.done = self.is_fully_aligned();
    }

    /// Mark valid: the translator declares the verse done, words left in the bank or not.
    /// The record vouches for `verse` and its re-review flag is cleared. tC4's `markDone`.
    pub fn mark_done(&mut self, verse: &TargetVerse) {
        self.target_verse_md5 = verse.md5();
        self.marked_invalid = false;
        self.done = true;
    }
}
