//! Keeping an alignment across an edit of its verse text (tC4 #213 `reflowAlignment`).
//!
//! [`reflow`] keeps every link whose target word the new text still has, by text and
//! occurrence, with its occurrence count updated; drops only the links to words that are
//! gone; rebuilds the bank from the new text's other words in verse order; and never
//! touches a card's source words or makes a link. It reports each dropped link in
//! [`Reflowed::dropped`].
//!
//! Nothing here writes. tC4 writes the reflow of every edited verse at draft save, so a
//! dropped link is gone without a word. Here a text save writes nothing on alignments
//! (STORAGE.md §5.1, §5.2): the stored value stays as it was and computes invalid, since
//! its hash no longer matches (§7.7). The aligner opens the verse with the reflowed value
//! ([`crate::Aligner::open`]). When the reflow is [lossless](Reflowed::is_lossless) it is
//! saved on leaving the verse, as tC4 would have saved it. When it dropped links, the
//! aligner shows them and saves only after the translator edits the verse or keeps the
//! reflow; until then the stored value, with every link, stays as it is, and after that
//! it stays in History as the parent of the new change.
use crate::value::{AlignmentValue, Group};
use crate::words::TargetVerse;
use tc_usfm::{SourceWord, TargetWord};

/// A link the new text cannot keep: the word is gone, or its occurrence moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedLink {
    /// The card the word was on, by its index in the value.
    pub card: usize,
    /// The card's source words.
    pub sources: Vec<SourceWord>,
    /// The target word as the old value recorded it.
    pub word: TargetWord,
}

/// The result of a reflow that changed something.
#[derive(Debug, Clone, PartialEq)]
pub struct Reflowed {
    /// The value for the new text: kept links, the rest of the words in the bank, made
    /// against the new text, `done` recomputed.
    pub value: AlignmentValue,
    /// The links that were dropped, in card order.
    pub dropped: Vec<DroppedLink>,
}

impl Reflowed {
    /// Every link was kept: the reflow only renumbered words and banked new ones.
    pub fn is_lossless(&self) -> bool {
        self.dropped.is_empty()
    }
}

/// Why a reflow left the value as it is. In every case the caller keeps the stored value
/// unchanged and its computed validity stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoReflow {
    /// The value was made against this very text.
    Current,
    /// tC3 or tC4 marked it for re-review (a span created or broken): a person decides.
    MarkedInvalid,
    /// Nothing was placed, so there is nothing to keep.
    NothingPlaced,
    /// The new text has no words.
    NoWords,
}

/// Carry `value` over to the edited `verse`, or say why tC4 would leave it as it is.
/// See the module documentation.
pub fn reflow(value: &AlignmentValue, verse: &TargetVerse) -> Result<Reflowed, NoReflow> {
    if value.marked_invalid {
        return Err(NoReflow::MarkedInvalid);
    }
    if value.placed() == 0 {
        return Err(NoReflow::NothingPlaced);
    }
    if !value.is_stale_against(verse) {
        return Err(NoReflow::Current);
    }
    if verse.words().is_empty() {
        return Err(NoReflow::NoWords);
    }
    Ok(carry(value, verse))
}

/// The reflow itself, without tC4's reasons to leave a value alone. The aligner uses it
/// to show any stale value against the verse it shows, so that every word it offers is a
/// word of that verse.
pub(crate) fn carry(value: &AlignmentValue, verse: &TargetVerse) -> Reflowed {
    let mut kept = vec![false; verse.words().len()];
    let mut dropped = Vec::new();
    let mut alignments = Vec::with_capacity(value.alignments.len());
    for (card, group) in value.alignments.iter().enumerate() {
        let mut targets = Vec::new();
        for word in &group.targets {
            match verse.position(word) {
                // tC4 keeps a word placed twice on two cards twice; the second copy
                // would put one word on two cards, so it is reported dropped instead.
                Some(at) if !kept[at] => {
                    kept[at] = true;
                    targets.push(verse.words()[at].clone());
                }
                _ => dropped.push(DroppedLink {
                    card,
                    sources: group.sources.clone(),
                    word: word.clone(),
                }),
            }
        }
        alignments.push(Group {
            sources: group.sources.clone(),
            targets,
        });
    }
    let word_bank = verse
        .words()
        .iter()
        .zip(&kept)
        .filter(|(_, kept)| !**kept)
        .map(|(w, _)| w.clone())
        .collect();
    let mut next = AlignmentValue {
        alignments,
        word_bank,
        ..value.clone()
    };
    next.settle(verse);
    Reflowed {
        value: next,
        dropped,
    }
}
