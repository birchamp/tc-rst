//! One verse open in the Align workspace (tC4's align session in `state.jsx`).
//!
//! The aligner holds the verse's alignment, the verse text it is shown against, the
//! standing suggestions and the selected bank words. Every edit goes through one path, as
//! in tC4's `applyAlignEdit` (`src/state.jsx:5225-5247` at d310bf3): the edit, then the
//! bank and every card in verse order ([`Alignment::bank_in_verse_order`],
//! [`Alignment::cards_in_verse_order`]), then [`Alignment::settle`] against the
//! verse's words, then the suggestions are bound again to the edited cards
//! ([`suggest::rebind`]). An edit that places words also clears the selection. Suggestions
//! are proposals (D72): the value changes only when the translator accepts one, which is
//! the same placement a click makes, and Mark valid is refused while any stands.
//!
//! The bank and the cards are in verse order from the first view of a verse, whatever
//! order the stored value has (the owner's Q15 ruling; tC4 sorts a stored value on open,
//! `src/state.jsx:1193-1198`, but not its bootstrap). Order alone never makes the value
//! count as changed: a value stored in another order, such as an import, stays as it was
//! until the translator edits the verse or marks it valid (Q19).
//!
//! Saving is the caller's: `tc-store` writes [`Aligner::value`] as one `Alignment` change
//! on leaving the verse or two seconds after the last change (STORAGE.md §5.3).

use crate::edit::{self, Alignment, EditError, bootstrap};
use crate::reflow::{self, DroppedLink};
use crate::selection::{self, Click};
use crate::suggest::{self, Proposal, RawLink, SessionInput};
use crate::value::{Basis, Pin, counts_by_verse, holds_the_words_of, needs_review};
use crate::words::{SourceVerse, TargetVerse};
use std::collections::HashSet;
use tc_journal::record::{AlignmentGroup, AlignmentValue, TargetWord};

/// Mark valid was refused because suggestions stand: they must be accepted or rejected
/// first, so that "done" never vouches for a link the translator did not make.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("confirm or reject the suggested links first")]
pub struct MarkValidRefused;

/// Why a verse cannot open in the Align workspace: there is nothing to align, and tC4
/// shows its "missing" notice instead of an aligner (`buildAlignmentSession`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Unavailable {
    /// The verse holds the alignment markup of the aligned file it was added from, which is
    /// its alignment (tC4 #794). This reason outranks the others.
    #[error("the verse already holds an alignment in its text")]
    AlignedInText,
    /// The original-language verse is not available (not installed, or the source has
    /// no words for this verse).
    #[error("the original-language verse is not available")]
    NoSource,
    /// The verse has no words. tC4 refuses an empty verse; a verse of punctuation alone
    /// is refused here too, since there is no word to place and Mark valid would vouch
    /// for nothing.
    #[error("the verse has no words to align")]
    NoWords,
}

/// One open verse. See the module documentation.
#[derive(Debug, Clone)]
pub struct Aligner {
    value: AlignmentValue,
    verse: TargetVerse,
    proposals: Vec<Proposal>,
    dropped: Vec<DroppedLink>,
    earlier: Option<AlignmentValue>,
    changed: bool,
    stale: bool,
    /// The selected bank words, as clicked. Only the shown part counts
    /// ([`Aligner::selection`]).
    armed: Vec<TargetWord>,
}

impl Aligner {
    /// Open a verse, or say why it cannot open.
    ///
    /// - A verse whose text holds alignment markup does not open, nor does one with no
    ///   original-language words or no target words.
    /// - With no stored value, or one with no cards (the removal form, or the empty
    ///   value a span change leaves in tC4), the verse starts from [`bootstrap`].
    /// - A stored value made for another span than the run `basis` names, moved with its
    ///   text ([`Basis::fits`], the journal's rule: the verse was joined into a span or
    ///   split from one since; a renumbered verse still fits) does not fit the original
    ///   words shown: the verse starts from the bootstrap, and the stored value is kept
    ///   aside in [`Aligner::earlier`] for the app to offer, as STORAGE.md §7.7 does.
    /// - A stored value made against this text opens as it is, if it is
    ///   [well formed](Alignment::well_formed) and holds the words of the verse.
    /// - A stored value made against other text, one that holds a word twice, or one whose
    ///   words are not the verse's (the hash is of `plain_text_v1`, which leaves out
    ///   some words, such as a `\qs` Selah, so an edit of such a word keeps the
    ///   hash), opens
    ///   carried over to `verse` (see [`crate::reflow`]), so every word the aligner
    ///   offers is a word of the verse, in one place: a bank copy of a placed word goes,
    ///   and a word on a second card is dropped and reported. `made_against` is the verse
    ///   text the value was made against (its hash is the value's), when the caller has it:
    ///   each link then follows its own word ([`crate::reflow_edit`]); without it, a link
    ///   to a word the changed verse holds more than once is not clear and is dropped.
    ///   If the carry-over dropped a link, the links are in [`Aligner::dropped`], and the
    ///   aligner counts as changed only after the translator edits the verse or calls
    ///   [`Aligner::keep_reflow`]; leaving without either keeps the stored value as it
    ///   is. A carry-over that dropped nothing counts as changed at once, as tC4 saves
    ///   it, unless tC4 would have left the value alone (it was marked invalid, or held
    ///   no placed word).
    ///
    /// The cards are then fitted to `source`, the pinned original-language words (tC4
    /// #790): every pinned word on a card, in source order, compared in NFC. A word stays on
    /// two cards only while both hold links (aligned USFM writes a word aligned to two
    /// separate runs of the translation so). A card holding a word the pin lacks is taken
    /// apart: its pinned words get cards and its links return to the bank, listed in
    /// [`Aligner::dropped`] like the carry-over's. A value whose source
    /// words carry `x-ref` (per-verse occurrences in a span, #409) is left as it is. The fit
    /// is never saved on its own; it rides on the first save.
    ///
    /// The open verse shows "The draft changed" ([`Aligner::is_stale`]) when the stored value
    /// [needs review](needs_review) against the verse and `source`: its text, words or pin
    /// changed, a card does not stand on `source`, or it is marked for re-review. So an open
    /// that takes a link off always shows it.
    ///
    /// Whichever it starts from, the bank and every card are shown in verse order. That
    /// alone does not count as a change. Nothing is selected.
    pub fn open(
        stored: Option<&AlignmentValue>,
        made_against: Option<&TargetVerse>,
        verse: TargetVerse,
        source: &SourceVerse,
        basis: Basis,
    ) -> Result<Self, Unavailable> {
        if verse.holds_alignment_markup() {
            return Err(Unavailable::AlignedInText);
        }
        if source.is_empty() {
            return Err(Unavailable::NoSource);
        }
        if verse.words().is_empty() {
            return Err(Unavailable::NoWords);
        }
        let mut aligner = Self {
            value: bootstrap(source, &verse, basis.clone()),
            verse,
            proposals: Vec::new(),
            dropped: Vec::new(),
            earlier: None,
            changed: false,
            stale: false,
            armed: Vec::new(),
        };
        aligner.start_from(stored, made_against, &basis, source);
        aligner.value.bank_in_verse_order(&aligner.verse);
        aligner.value.cards_in_verse_order(&aligner.verse);
        Ok(aligner)
    }

    /// Replace the bootstrap with what `stored` gives, as [`Aligner::open`] describes.
    fn start_from(
        &mut self,
        stored: Option<&AlignmentValue>,
        made_against: Option<&TargetVerse>,
        basis: &Basis,
        source: &SourceVerse,
    ) {
        let Some(stored) = stored.filter(|s| !s.alignments.is_empty()) else {
            return;
        };
        if !basis.fits(stored) {
            self.earlier = Some(stored.clone());
            return;
        }
        let run = basis.aligned_run(&self.verse);
        self.stale = needs_review(stored, &run, &self.verse, Some(source));
        let mut saves = false;
        if !stored.is_stale_against(&self.verse)
            && stored.well_formed().is_ok()
            && holds_the_words_of(stored, &self.verse)
        {
            self.value = stored.clone();
        } else {
            // AD19: a carry-over over a changed text is saved as the store's reflow would
            // have saved it; a value fixed for any other reason is not rewritten unasked.
            saves =
                stored.is_stale_against(&self.verse) && reflow::reflow(stored, &self.verse).is_ok();
            let carried = reflow::carry(stored, made_against, &self.verse);
            self.value = carried.value;
            self.dropped = carried.dropped;
        }
        self.dropped.extend(fit_cards(&mut self.value, source));
        self.changed = saves && self.dropped.is_empty();
    }

    pub fn value(&self) -> &AlignmentValue {
        &self.value
    }

    pub fn into_value(self) -> AlignmentValue {
        self.value
    }

    pub fn verse(&self) -> &TargetVerse {
        &self.verse
    }

    /// The aligner shows "The draft changed since this alignment was made. Check that the
    /// words still line up." (tC4 #787): the stored value needed review when the verse
    /// opened (see [`Aligner::open`]), and the translator has not edited the verse, marked it
    /// valid or kept the carried-over links since.
    pub fn is_stale(&self) -> bool {
        self.stale
    }

    /// Links the carry-over on opening could not keep, to show the translator.
    pub fn dropped(&self) -> &[DroppedLink] {
        &self.dropped
    }

    /// The translator has seen [`Aligner::dropped`] and keeps the carried-over
    /// alignment: it is saved like an edit.
    pub fn keep_reflow(&mut self) {
        self.changed = true;
        self.stale = false;
    }

    /// The caller has stored the value as it stands: it is no longer to be saved, until the
    /// translator changes it again.
    pub fn mark_saved(&mut self) {
        self.changed = false;
    }

    /// A stored alignment made for another span, kept aside when the verse opened.
    pub fn earlier(&self) -> Option<&AlignmentValue> {
        self.earlier.as_ref()
    }

    /// The value is to be saved: the translator edited it or marked it valid, or the
    /// carry-over on opening lost nothing (see [`Aligner::open`]).
    pub fn is_changed(&self) -> bool {
        self.changed
    }

    pub fn proposals(&self) -> &[Proposal] {
        &self.proposals
    }

    /// The one path of every edit. See the module documentation.
    fn edit<T>(
        &mut self,
        f: impl FnOnce(&mut AlignmentValue, &TargetVerse) -> Result<T, EditError>,
    ) -> Result<T, EditError> {
        let out = f(&mut self.value, &self.verse)?;
        self.value.bank_in_verse_order(&self.verse);
        self.value.cards_in_verse_order(&self.verse);
        self.value.settle(&self.verse);
        self.proposals = suggest::rebind(&self.value, &self.proposals);
        self.changed = true;
        Ok(out)
    }

    /// An edit that places words: on success it also clears the selection, as tC4's
    /// `applyAlignEdit(…, true)` disarms.
    fn place_edit<T>(
        &mut self,
        f: impl FnOnce(&mut AlignmentValue, &TargetVerse) -> Result<T, EditError>,
    ) -> Result<T, EditError> {
        let out = self.edit(f)?;
        self.armed.clear();
        Ok(out)
    }

    /// Place a banked word on a card.
    pub fn place(&mut self, card: usize, word: &TargetWord) -> Result<(), EditError> {
        self.place_edit(|v, _| v.link(card, word))
    }

    /// Place several banked words on a card in one edit, as a drop of dragged bank words
    /// does, and return how many were placed. A word no longer in the bank is skipped;
    /// the edit is refused, changing nothing and keeping the selection, only when no
    /// word could be placed. tC4's `placeAlignWordsAt` (#736).
    pub fn place_many(&mut self, card: usize, words: &[TargetWord]) -> Result<usize, EditError> {
        self.place_edit(|v, _| v.link_many(card, words))
    }

    /// A click on a card while bank words are selected: the shown selection
    /// ([`Aligner::selection`]) is placed on the card in one edit. Selected words that
    /// became suggestions stay where they are (Q16). tC4's `placeAlignWord`.
    pub fn place_selection(&mut self, card: usize) -> Result<usize, EditError> {
        let words = self.selection();
        self.place_many(card, &words)
    }

    /// Send a placed word back to the bank.
    pub fn unplace(&mut self, card: usize, word: &TargetWord) -> Result<(), EditError> {
        self.edit(|v, verse| v.unlink(card, word, verse))
    }

    /// Move a placed word to another card.
    pub fn move_word(
        &mut self,
        from: usize,
        to: usize,
        word: &TargetWord,
    ) -> Result<(), EditError> {
        self.edit(|v, _| v.move_word(from, to, word))
    }

    /// Merge two neighbouring cards into a phrase.
    pub fn merge(&mut self, a: usize, b: usize) -> Result<(), EditError> {
        self.edit(|v, _| v.merge(a, b))
    }

    /// Split a phrase back into single cards.
    pub fn split(&mut self, card: usize) -> Result<(), EditError> {
        self.edit(|v, _| v.split(card))
    }

    /// Start the verse over. Standing suggestions go: they were drawn from links that are gone
    /// (tC4 #758).
    pub fn clear(&mut self) {
        // Clearing cannot fail; the edit path only adds the sorts and the settle.
        let _ = self.edit(|v, verse| {
            v.clear(verse);
            Ok(())
        });
        self.proposals.clear();
    }

    /// Mark valid: the verse is done, words left in the bank or not (D73).
    pub fn mark_valid(&mut self) -> Result<(), MarkValidRefused> {
        if !self.proposals.is_empty() {
            return Err(MarkValidRefused);
        }
        self.value.mark_done(&self.verse);
        self.changed = true;
        self.stale = false;
        Ok(())
    }

    /// What to ask the engine for this verse.
    pub fn session_input(&self) -> SessionInput {
        suggest::session_input(&self.value, &self.verse)
    }

    /// Show the engine's answer to [`Aligner::session_input`] as proposals, replacing
    /// any that stood. The value is not changed.
    pub fn show_suggestions(&mut self, raw: &[RawLink]) {
        self.proposals = suggest::proposals(&self.value, &self.verse, raw);
    }

    /// Accept one proposal: its word is placed on its card, as a click would place it.
    pub fn accept(&mut self, index: usize) -> Result<(), EditError> {
        let Some(proposal) = self.proposals.get(index).cloned() else {
            return Err(EditError::NoSuchProposal(index));
        };
        self.place(proposal.card, &proposal.word)
    }

    /// Accept every proposal in one edit, each placed as a click would place it, and
    /// return how many were placed. tC4's `acceptAllAlignSuggestions`.
    ///
    /// Each standing proposal is for a different bank word and an existing card, and a
    /// placement moves no card, so each one places; one that did not would be skipped
    /// without holding back the others. With nothing to place, nothing changes.
    pub fn accept_all(&mut self) -> usize {
        let standing = self.proposals.clone();
        self.place_edit(|v, _| {
            let placed = standing
                .iter()
                .filter(|p| v.link(p.card, &p.word).is_ok())
                .count();
            if placed == 0 {
                return Err(EditError::NothingToPlace);
            }
            Ok(placed)
        })
        .unwrap_or(0)
    }

    /// Reject every proposal. The value is not changed.
    pub fn reject_all(&mut self) {
        self.proposals.clear();
    }

    /// The bank as the Align workspace shows it, in bank order: the banked words without
    /// a standing suggestion, which show on their card instead.
    pub fn shown_bank(&self) -> Vec<TargetWord> {
        let proposed: HashSet<(&str, u32)> = self
            .proposals
            .iter()
            .map(|p| (p.word.word.as_str(), p.word.occurrence))
            .collect();
        self.value
            .word_bank
            .iter()
            .filter(|w| !proposed.contains(&(w.word.as_str(), w.occurrence)))
            .cloned()
            .collect()
    }

    /// The selected bank words the bank shows, in bank order: what a card click places
    /// and what the bank draws as selected.
    pub fn selection(&self) -> Vec<TargetWord> {
        selection::shown_selection(&self.shown_bank(), &self.armed)
    }

    /// Whether the cards take a click: some shown bank word is selected. While only words
    /// that became suggestions are selected, the cards look as they do with nothing
    /// selected (Q16).
    pub fn is_armed(&self) -> bool {
        !self.selection().is_empty()
    }

    /// A click on the word at `index` of [`Aligner::shown_bank`]: the selection becomes
    /// what [`selection::next_selection`] says. The value is not changed.
    pub fn click_bank_word(&mut self, index: usize, click: Click) {
        self.armed = selection::next_selection(&self.shown_bank(), &self.armed, click, index)
            .unwrap_or_default();
    }

    /// What dragging the word at `index` of [`Aligner::shown_bank`] carries, to give to
    /// [`Aligner::place_many`] at the drop: the shown selection when the word is selected,
    /// else the word alone.
    pub fn dragged_words(&self, index: usize) -> Vec<TargetWord> {
        selection::dragged_words(&self.shown_bank(), &self.armed, index)
    }
}

/// Fit `value`'s cards to the pinned words `source`, as [`Aligner::open`] describes, and
/// return the links it took off.
fn fit_cards(value: &mut AlignmentValue, source: &SourceVerse) -> Vec<DroppedLink> {
    if counts_by_verse(value) {
        return Vec::new();
    }
    let pin = Pin::new(source);
    // Each card with the places of its source words in the pin. A card with no source word
    // and no link only keeps its fields, and goes last.
    let mut cards: Vec<(Vec<usize>, AlignmentGroup)> = Vec::new();
    let mut dropped = Vec::new();
    for (card, group) in std::mem::take(&mut value.alignments)
        .into_iter()
        .enumerate()
    {
        match pin.card(&group) {
            // A source word may stand on two cards: aligned USFM writes a word aligned to two
            // separate runs of the translation as two milestones.
            Some(at) => cards.push((at, group)),
            None => {
                dropped.extend(group.bottom_words.iter().map(|word| DroppedLink {
                    card,
                    top_words: group.top_words.clone(),
                    word: word.clone(),
                }));
                value.word_bank.extend(group.bottom_words);
                // The card's own fields stay, on a card with no word, as `clear` keeps them.
                if !group.extra.is_empty() {
                    let fields = AlignmentGroup {
                        top_words: Vec::new(),
                        bottom_words: Vec::new(),
                        extra: group.extra,
                    };
                    cards.push((Vec::new(), fields));
                }
            }
        }
    }
    let mut holders = vec![0usize; source.words().len()];
    for &p in cards.iter().flat_map(|(at, _)| at) {
        holders[p] += 1;
    }
    // A card with nothing on it whose words other cards hold too is a second card for the
    // same words, such as the one an import leaves when the aligned file writes a Hebrew word
    // in another order of its marks than the pin: it goes.
    cards.retain(|(at, g)| {
        let spare = g.bottom_words.is_empty()
            && g.extra.is_empty()
            && !at.is_empty()
            && at.iter().all(|&p| holders[p] > 1);
        if spare {
            for &p in at {
                holders[p] -= 1;
            }
        }
        !spare
    });
    cards.extend(
        source
            .words()
            .iter()
            .enumerate()
            .filter(|(p, _)| holders[*p] == 0)
            .map(|(p, w)| (vec![p], edit::card(w.clone()))),
    );
    cards.sort_by_key(|(at, _)| at.first().copied().unwrap_or(usize::MAX));
    value.alignments = cards.into_iter().map(|(_, g)| g).collect();
    dropped
}
