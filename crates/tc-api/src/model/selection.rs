//! The words a checker taps in their draft (tC4 `Check.jsx`, `TargetWords`).
//!
//! A selection is a set of word indices, kept as one local value in the UI until a mark saves
//! it (#757). The server turns the indices into the stored selections with
//! `tc_check::selection::from_indices`, so an index here and an index there are the same word.

use std::collections::BTreeSet;

/// The words selected after a drag from word `from` to word `to` (tC4's `dragRange`, #737): the
/// words selected when the press began, plus every word from `from` to `to`, whole, whichever
/// way the pointer went.
pub fn drag_range(base: &BTreeSet<u32>, from: u32, to: u32) -> BTreeSet<u32> {
    let mut next = base.clone();
    next.extend(from.min(to)..=from.max(to));
    next
}

/// A click, Space or Enter on word `at`: selects it, or clears it when it is selected, and
/// leaves the others alone, so a selection with gaps stays possible.
pub fn toggle(selection: &BTreeSet<u32>, at: u32) -> BTreeSet<u32> {
    let mut next = selection.clone();
    if !next.remove(&at) {
        next.insert(at);
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[u32]) -> BTreeSet<u32> {
        items.iter().copied().collect()
    }

    /// tC4 #737: pressing on a word and dragging selects whole words, the words selected
    /// before stay, and a drag back toward the start selects the same range.
    #[test]
    fn a_drag_selects_whole_words_and_keeps_the_earlier_selection() {
        assert_eq!(drag_range(&set(&[0]), 2, 4), set(&[0, 2, 3, 4]));
        assert_eq!(drag_range(&set(&[0]), 4, 2), set(&[0, 2, 3, 4]));
        assert_eq!(drag_range(&set(&[]), 3, 3), set(&[3]));
        assert_eq!(drag_range(&set(&[1, 2]), 2, 3), set(&[1, 2, 3]));
    }

    #[test]
    fn toggling_a_word_leaves_the_others() {
        assert_eq!(toggle(&set(&[1]), 3), set(&[1, 3]));
        assert_eq!(toggle(&set(&[1, 3]), 3), set(&[1]));
    }
}
