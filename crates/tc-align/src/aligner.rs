//! One verse open in the Align workspace (tC4's align session in `state.jsx`).
//!
//! The aligner holds the verse's alignment, the verse text it is shown against, and the
//! standing suggestions. Every edit goes through one path, as in tC4's `applyAlignEdit`:
//! the edit, then [`AlignmentValue::settle`] against the shown text, then the suggestions
//! are bound again to the edited cards ([`suggest::rebind`]). Suggestions are proposals
//! (D72): the value changes only when the translator accepts one, which is the same
//! placement a click makes, and Mark valid is refused while any stands.
//!
//! Saving is the caller's: `tc-store` writes [`Aligner::value`] as one `Alignment` change
//! on leaving the verse or two seconds after the last change (STORAGE.md §5.3).

use crate::edit::EditError;
use crate::reflow::{self, DroppedLink};
use crate::suggest::{self, Proposal, RawLink, SessionInput};
use crate::value::{AlignmentValue, Basis};
use crate::words::{SourceVerse, TargetVerse};
use tc_usfm::TargetWord;

/// Mark valid was refused because suggestions stand: they must be accepted or rejected
/// first, so that "done" never vouches for a link the translator did not make.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("confirm or reject the suggested links first")]
pub struct MarkValidRefused;

/// One open verse. See the module documentation.
#[derive(Debug, Clone)]
pub struct Aligner {
    value: AlignmentValue,
    verse: TargetVerse,
    proposals: Vec<Proposal>,
    dropped: Vec<DroppedLink>,
    earlier: Option<AlignmentValue>,
    changed: bool,
}

impl Aligner {
    /// Open a verse.
    ///
    /// - With no stored value, or one with no cards (the removal form, or the empty
    ///   value a span change leaves in tC4), the verse starts from
    ///   [`AlignmentValue::bootstrap`].
    /// - A stored value made for another span than `basis.span_end` (the verse was
    ///   joined into a span or split from one since) does not fit the original words
    ///   shown: the verse starts from the bootstrap, and the stored value is kept aside
    ///   in [`Aligner::earlier`] for the app to offer, as STORAGE.md §7.7 does.
    /// - A stored value made against this text opens as it is.
    /// - A stored value made against other text opens carried over to `verse` (see
    ///   [`crate::reflow`]), so every word the aligner offers is a word of the verse.
    ///   If the carry-over dropped a link, the links are in [`Aligner::dropped`], and the
    ///   aligner counts as changed only after the translator edits the verse or calls
    ///   [`Aligner::keep_reflow`]; leaving without either keeps the stored value as it
    ///   is. A carry-over that dropped nothing counts as changed at once, as tC4 saves
    ///   it, unless tC4 would have left the value alone (it was marked invalid, or held
    ///   no placed word).
    pub fn open(
        stored: Option<&AlignmentValue>,
        verse: TargetVerse,
        source: &SourceVerse,
        basis: Basis,
    ) -> Self {
        let mut aligner = Self {
            value: AlignmentValue::bootstrap(source, &verse, basis.clone()),
            verse,
            proposals: Vec::new(),
            dropped: Vec::new(),
            earlier: None,
            changed: false,
        };
        let Some(stored) = stored.filter(|s| !s.alignments.is_empty()) else {
            return aligner;
        };
        if stored.span_end != basis.span_end {
            aligner.earlier = Some(stored.clone());
            return aligner;
        }
        if !stored.is_stale_against(&aligner.verse) {
            aligner.value = stored.clone();
            return aligner;
        }
        let tc4_would_reflow = reflow::reflow(stored, &aligner.verse).is_ok();
        let carried = reflow::carry(stored, &aligner.verse);
        aligner.changed = tc4_would_reflow && carried.is_lossless();
        aligner.value = carried.value;
        aligner.dropped = carried.dropped;
        aligner
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

    /// The value was made against other text than the verse shown: the aligner shows
    /// "The draft changed since this alignment was made".
    pub fn is_stale(&self) -> bool {
        self.value.is_stale_against(&self.verse)
    }

    /// Links the carry-over on opening could not keep, to show the translator.
    pub fn dropped(&self) -> &[DroppedLink] {
        &self.dropped
    }

    /// The translator has seen [`Aligner::dropped`] and keeps the carried-over
    /// alignment: it is saved like an edit.
    pub fn keep_reflow(&mut self) {
        self.changed = true;
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

    fn edit(
        &mut self,
        f: impl FnOnce(&mut AlignmentValue, &TargetVerse) -> Result<(), EditError>,
    ) -> Result<(), EditError> {
        f(&mut self.value, &self.verse)?;
        self.value.settle(&self.verse);
        self.proposals = suggest::rebind(&self.value, &self.proposals);
        self.changed = true;
        Ok(())
    }

    /// Place a banked word on a card.
    pub fn place(&mut self, card: usize, word: &TargetWord) -> Result<(), EditError> {
        self.edit(|v, _| v.link(card, word))
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

    /// Start the verse over.
    pub fn clear(&mut self) {
        // Clearing cannot fail; the edit path only adds the settle and the rebind.
        let _ = self.edit(|v, verse| {
            v.clear(verse);
            Ok(())
        });
    }

    /// Mark valid: the verse is done, words left in the bank or not (D73).
    pub fn mark_valid(&mut self) -> Result<(), MarkValidRefused> {
        if !self.proposals.is_empty() {
            return Err(MarkValidRefused);
        }
        self.value.mark_done(&self.verse);
        self.changed = true;
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

    /// Accept every proposal. Returns how many were placed.
    pub fn accept_all(&mut self) -> usize {
        let mut placed = 0;
        while let Some(proposal) = self.proposals.first().cloned() {
            if self.place(proposal.card, &proposal.word).is_ok() {
                placed += 1;
            } else {
                self.proposals.remove(0);
            }
        }
        placed
    }

    /// Reject every proposal. The value is not changed.
    pub fn reject_all(&mut self) {
        self.proposals.clear();
    }
}
