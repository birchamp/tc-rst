//! Editing one verse's alignment: the operations behind the Align workspace.
//!
//! Each operation either changes the value or refuses with an [`EditError`] and leaves it
//! as it was. Together they keep these invariants, as tC4's `edit.ts` does:
//!
//! - A target word is in exactly one place: the bank or one card. A word reaches a card
//!   only from the bank ([`AlignmentValue::link`]) or from another card
//!   ([`AlignmentValue::move_word`]), and leaves a card only for the bank or another card.
//! - Cards stay in original-language verse order. Only neighbouring cards merge, the
//!   lower one keeping its place with the words of both in card order, so every phrase
//!   is a contiguous run of the original verse. A split puts the phrase's words back as
//!   single cards in place, and the placed target words stay on the first of them.
//! - Placed words keep the order they were placed in, as tC4 records them.
//!
//! The bank starts in tC4's bootstrap order (see [`TargetVerse::bank_order`]), and
//! [`AlignmentValue::clear`] puts it back in that order. A carry-over to edited text
//! leaves it in verse order instead, as tC4's reflow does ([`crate::reflow`]). A word
//! sent back to the bank goes before the first banked word that the bootstrap puts after
//! it: in a bank in bootstrap order, exactly where the bootstrap had it. tC4's own unlink
//! instead re-sorts the whole bank by occurrence and then by the locale's collation, so
//! the bank changes order under the translator's hand; this keeps every other word where
//! it was.
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

    /// Send a placed word back to the bank, before the first banked word that tC4's
    /// bootstrap puts after it for `verse`. A word the verse no longer has goes after the
    /// words of its occurrence. tC4's `unlinkWord`, with the bank order described in the
    /// module documentation.
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
        bank_insert(&mut self.word_bank, returned, verse);
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
    /// in the bank, and the whole bank in bootstrap order for `verse`, whatever order it
    /// was in. Banked words the verse no longer has stay, after the words of their
    /// occurrence.
    pub fn clear(&mut self, verse: &TargetVerse) {
        let groups = std::mem::take(&mut self.alignments);
        for group in groups {
            self.word_bank.extend(group.targets);
            self.alignments
                .extend(group.sources.into_iter().map(Group::single));
        }
        self.word_bank.sort_by_key(|w| bootstrap_key(w, verse));
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

/// Where tC4's bootstrap banks `word` for `verse`: by occurrence, then by place in the
/// verse; a word the verse does not have goes after the words of its occurrence.
fn bootstrap_key(word: &TargetWord, verse: &TargetVerse) -> (u32, usize) {
    (word.occurrence, verse.position(word).unwrap_or(usize::MAX))
}

/// Insert `word` before the first banked word that the bootstrap puts after it.
fn bank_insert(bank: &mut Vec<TargetWord>, word: TargetWord, verse: &TargetVerse) {
    let new_key = bootstrap_key(&word, verse);
    let at = bank
        .iter()
        .position(|w| bootstrap_key(w, verse) > new_key)
        .unwrap_or(bank.len());
    bank.insert(at, word);
}
