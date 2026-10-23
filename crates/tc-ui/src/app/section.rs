//! The decisions of the section card (unit B4b) that are not drawing: what the Place mode says
//! while a verse number is in hand (#604), the title of a word a number may or may not land
//! on, and whether Save is on. Pure, so they are tested on the host. The rules of the typed
//! text itself are `tc_api::model::section`, which the server runs too.

use crate::i18n::{keys, t};

/// tC4's `wordTitle`: the hover title and name of a word while a verse number is in hand, saying
/// why it can or cannot land there. Nothing while none is.
pub fn word_title(held: Option<&str>, pin_at: Option<&str>, ok: bool, word: &str) -> String {
    let Some(held) = held else {
        return String::new();
    };
    match (ok, pin_at) {
        (true, Some(m)) => t(
            keys::draft::JOIN_AT,
            &[("n", &held), ("m", &m), ("w", &word)],
        ),
        (true, None) => t(keys::draft::BEGIN_AT, &[("n", &held), ("w", &word)]),
        (false, Some(m)) => t(keys::draft::OCCUPIED, &[("n", &m)]),
        (false, None) => t(keys::draft::CANNOT_PASS, &[("n", &held)]),
    }
}

/// Whether Save section is on: the card has words, or a verse holds text now that an empty
/// card would remove (a section with no draft has nothing to save, #600).
pub fn can_save(empty: bool, any_drafted: bool) -> bool {
    !empty || any_drafted
}

/// What the bank's instruction row says (#604). Its two sentences keep their own rows in every
/// state, so a number picked up moves no word under the pointer (#598); this says which one is
/// shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hints {
    /// "Click a verse number, then click the word that begins the verse": no number in hand and
    /// one waiting in the bank.
    pub pick: bool,
    /// "Click the word where verse N begins": a number in hand. The number is the held one;
    /// while none is, the last verse's, which only fills the hidden row.
    pub drop: Option<String>,
    /// "Every verse is placed. Click a verse number to move where its verse begins.": the bank
    /// is empty and there are words to place them in.
    pub bank_empty: bool,
}

pub fn hints(held: Option<&str>, bank: usize, words: usize) -> Hints {
    Hints {
        pick: held.is_none() && bank > 0,
        drop: held.map(str::to_owned),
        bank_empty: bank == 0 && words > 0,
    }
}

/// A section remains open until its server draft was durably discarded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CancelState {
    #[default]
    Idle,
    Pending,
    Error(String),
}
