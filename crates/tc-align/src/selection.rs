//! Which bank words a click selects, for placing several on one card in one edit (tC4
//! #736, `shownSelection` and `nextSelection` in `src/views/Align.jsx:480-505` at
//! d310bf3, as tC3 had it).
//!
//! The rule works on the shown bank: the banked words without a standing suggestion,
//! which show on their card instead ([`crate::Aligner::shown_bank`]). A selected word
//! that has since become a suggestion stays selected underneath but is not shown as
//! selected, and only the shown selection is placed (the owner's Q16 ruling).
//!
//! Every selection is kept in bank order, so a card takes the words in the order the
//! translator sees them. The bank is in verse order, so for a right-to-left verse "bank
//! order" is reading order, whichever way the screen draws it.

use crate::words::same_word;
use std::collections::HashSet;
use tc_usfm::TargetWord;

/// What a click on a bank word does, from the keys held with it. Enter or Space on a
/// focused word is a click too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Click {
    /// The word alone, or nothing when it is the only word selected.
    Plain,
    /// Ctrl or Cmd: add the word, or take it out.
    Toggle,
    /// Shift: add every word from the first selected word to this one.
    Range,
}

impl Click {
    /// The click an event's modifier keys make. Ctrl or Cmd wins over Shift, as tC4 tests
    /// it first.
    pub fn from_modifiers(ctrl_or_meta: bool, shift: bool) -> Self {
        if ctrl_or_meta {
            Self::Toggle
        } else if shift {
            Self::Range
        } else {
            Self::Plain
        }
    }
}

/// The selected words that `shown_bank` shows, in bank order.
pub fn shown_selection(shown_bank: &[TargetWord], armed: &[TargetWord]) -> Vec<TargetWord> {
    let keys: HashSet<(&str, u32)> = armed
        .iter()
        .map(|w| (w.text.as_str(), w.occurrence))
        .collect();
    shown_bank
        .iter()
        .filter(|w| keys.contains(&(w.text.as_str(), w.occurrence)))
        .cloned()
        .collect()
}

/// The selection after `click` on the word at `index` of `shown_bank`, or `None` when
/// nothing is selected.
///
/// - [`Click::Toggle`] adds the word, or takes it out.
/// - [`Click::Range`] adds every word from the first selected word, in bank order, to the
///   clicked word, both included, in either direction. It never takes a word out. With
///   nothing selected it selects the clicked word alone.
/// - [`Click::Plain`] selects the clicked word alone, or clears the selection when that
///   word is the only one selected.
///
/// Only the shown part of `armed` counts, so a selected word that became a suggestion
/// neither anchors a range nor stays in the result. An `index` past the shown bank changes
/// nothing.
pub fn next_selection(
    shown_bank: &[TargetWord],
    armed: &[TargetWord],
    click: Click,
    index: usize,
) -> Option<Vec<TargetWord>> {
    let armed = shown_selection(shown_bank, armed);
    let Some(word) = shown_bank.get(index) else {
        return non_empty(armed);
    };
    let is_armed = armed.iter().any(|w| same_word(w, word));
    let next = match click {
        Click::Toggle if is_armed => {
            let rest: Vec<TargetWord> = armed.into_iter().filter(|w| !same_word(w, word)).collect();
            shown_selection(shown_bank, &rest)
        }
        Click::Toggle => {
            let mut more = armed;
            more.push(word.clone());
            shown_selection(shown_bank, &more)
        }
        Click::Range => match armed.first() {
            Some(first) => {
                // The first selected word is shown, so it has a place in the shown bank.
                let anchor = shown_bank
                    .iter()
                    .position(|w| same_word(w, first))
                    .unwrap_or(index);
                let (from, to) = (anchor.min(index), anchor.max(index));
                let mut more = armed;
                more.extend_from_slice(&shown_bank[from..=to]);
                shown_selection(shown_bank, &more)
            }
            None => vec![word.clone()],
        },
        Click::Plain if is_armed && armed.len() == 1 => Vec::new(),
        Click::Plain => vec![word.clone()],
    };
    non_empty(next)
}

/// What dragging the word at `index` of `shown_bank` carries: the shown selection when
/// the word is selected, else the word alone. Empty for an `index` past the shown bank.
pub fn dragged_words(
    shown_bank: &[TargetWord],
    armed: &[TargetWord],
    index: usize,
) -> Vec<TargetWord> {
    let Some(word) = shown_bank.get(index) else {
        return Vec::new();
    };
    let armed = shown_selection(shown_bank, armed);
    if armed.iter().any(|w| same_word(w, word)) {
        armed
    } else {
        vec![word.clone()]
    }
}

fn non_empty(words: Vec<TargetWord>) -> Option<Vec<TargetWord>> {
    (!words.is_empty()).then_some(words)
}
