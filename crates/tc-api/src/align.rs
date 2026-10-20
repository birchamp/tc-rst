//! The Align screen's types (APP-DESIGN §3.7, AD18, A-30; unit B6).
//!
//! The aligner lives on the server (AD18): `align/open` makes a session for one verse and every
//! edit returns the whole view, built by a pure function of `tc_align::Aligner`. The UI queues
//! its clicks and sends the next when the last reply has come, each naming the `seq` of the view
//! it was made on, so an edit is never refused for coming too soon and never applies to a view
//! the person did not see. The server saves the links itself (2 s after the last change, on
//! leaving the verse, on replace or close, and before the quit checkpoint); the view's `save`
//! and the `AlignSave` event say where that stands.

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::ids::{AlignerId, RequestId, Scope};
use crate::shown::Direction;
use crate::write::NotSaved;

/// The server's save of an open aligner's links. The save indicator and the close guard
/// count it, as tC4's count `alignSaveState` (A-4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AlignSave {
    Saved,
    /// Links changed and are not saved yet; the server saves them 2 s after the last change,
    /// on leaving the verse, and before quitting.
    Unsaved,
    Saving,
    Failed(NotSaved),
}

/// The word bank's selection, by position in [`AlignerView::bank`].
///
/// A selected word that has since become a suggestion shows on its card, not in the bank, and
/// is not counted: the cards look as they do with nothing selected (tC4 #788, Q16).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BankSelection {
    None,
    /// These bank chips are selected, so the cards take a click (or a drop) that places them.
    Armed(Vec<u32>),
}

/// Whether the open verse's stored alignment still vouches for it (`tc_align::Aligner::is_stale`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Freshness {
    Current,
    /// When the verse opened, its stored alignment needed review: the verse's text or words,
    /// its span or its original-language text changed since it was made, or it is marked for
    /// re-review (tC4 #787, #796). The view shows `align.stale` ("The draft changed since this
    /// alignment was made. Check that the words still line up.") until the first edit, Mark
    /// valid, or keeping the carried-over links.
    Stale,
}

/// Which testament a book is in: the original-language text, the suggestion model and the
/// mode switch's label (`align.modeOrig.nt`, `.ot`) follow it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Testament {
    Nt,
    Ot,
}

/// A verse or a span of verses of the open book, as the rail lists it: the chapter, the verse it
/// starts at (the key its alignment is stored under) and the last verse it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct AlignPlace {
    pub chapter: u16,
    pub verse: u16,
    /// At least `verse`; more for a span such as 9-10.
    pub last: u16,
}

impl AlignPlace {
    /// tC4's `ref`: `chapter:verse`, or `chapter:verse-last` for a span.
    pub fn label(&self) -> String {
        if self.last > self.verse {
            format!("{}:{}-{}", self.chapter, self.verse, self.last)
        } else {
            format!("{}:{}", self.chapter, self.verse)
        }
    }
}

/// A word of the translation or of the original, by its text and its place among the words with
/// that text in the verse (tC4's invariant I-2). `occurrences` above 1 shows the number as a
/// superscript and names the chip "word 2" (tC4 #735).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Word {
    pub word: String,
    pub occurrence: u32,
    pub occurrences: u32,
}

/// One card: the original-language word or words (a merged phrase has several), the translation
/// words placed on it in verse order, and the words a suggestion would place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub index: u32,
    pub top: Vec<Word>,
    pub placed: Vec<Word>,
    /// Proposals standing on this card. They are not links until the person accepts them.
    pub suggested: Vec<Word>,
    /// The gateway lens's header: the words of the gateway text (ULT) that render this card's
    /// original words. Present only while the lens is available and has a word for the card.
    pub gateway: Option<String>,
}

/// The gateway lens: the gateway text of the open verse, whose own alignment gives each card its
/// header. Available only for a project numbered as its sources are, and when the gateway pane
/// carries this verse with alignment data (tC4 #131).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct GatewayLens {
    pub text: String,
}

/// The state of a verse on the rail, and of the open verse (tc-align `VerseState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum RailState {
    /// The verse has no text yet.
    Undrafted,
    /// Not done: nothing aligned yet, or words left to place.
    Todo,
    /// Marked done and still vouching for the verse.
    Valid,
    /// The alignment no longer vouches for the verse; it is kept for review.
    Invalid,
}

/// A verse's rail row: its state and the words placed out of the words held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct RailStatus {
    pub state: RailState,
    pub placed: u32,
    pub total: u32,
}

/// A link the carry-over on opening could not keep (tC4 #213): the word is back in the bank.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct DroppedLinkView {
    pub card: u32,
    pub top: Vec<Word>,
    pub word: Word,
}

/// The suggestions switch and what the engine knows (tC4's `alignSuggest` row).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct SuggestView {
    /// The project's switch (`prefs/setAlignSuggestions`).
    pub on: bool,
    pub status: SuggestStatus,
    /// A Suggest has been asked for this verse and answered, so "nothing to suggest" can show.
    pub asked: bool,
    /// How many words a suggestion stands on.
    pub standing: u32,
    /// A Suggest is on its way.
    pub suggesting: bool,
}

/// Where the suggestion engine stands for the open book's testament (tC4 `suggestScheduler`).
/// The `Suggest` event carries it whole, and every aligner view carries it again, so a
/// `Resync` cannot leave "Reading" standing (§5.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SuggestStatus {
    Off,
    /// The engine is reading the project's aligned verses.
    Reading,
    /// No verse of this testament is aligned yet.
    None,
    Ready {
        verses: u32,
        boosted: bool,
    },
    Error {
        reason: String,
    },
}

/// Why a verse cannot be aligned: there is nothing to align, and tC4 shows a notice instead of
/// an aligner (`Align.jsx`, `AlignBlocked`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Blocked {
    /// The verse has no text, or no word to place.
    Undrafted,
    /// The project pins no original-language text.
    Unpinned,
    /// The pinned text is not on this computer.
    Missing,
    /// The original-language text is there but cannot be read.
    Unreadable,
    /// The project's versification could not be loaded.
    VersificationUnavailable,
    /// The project's versification is not recorded and could not be matched.
    VersificationUnknown,
    /// In the project's numbering, this verse does not line up with one source verse.
    NoCounterpart,
    /// The verse holds the alignment markup of the aligned file it was added from, which is its
    /// alignment (tC4 #794): aligning it would write over it.
    AlignedInText,
}

/// An open aligner (tC4's `alignSession`), whole: every edit returns it again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct AlignerView {
    pub aligner: AlignerId,
    /// One more for every change of the view; an edit names the `seq` it was made on.
    pub seq: u64,
    pub place: AlignPlace,
    pub testament: Testament,
    /// The direction of the translation (the bank) and of the original (the cards).
    pub target_dir: Direction,
    pub orig_dir: Direction,
    /// The original-language verse as the reference band shows it.
    pub original: String,
    /// The gateway lens, when it is available.
    pub gateway: Option<GatewayLens>,
    /// The bank as it shows: the words not placed and not standing as suggestions, in verse
    /// order.
    pub bank: Vec<Word>,
    pub selection: BankSelection,
    pub cards: Vec<Card>,
    /// Words placed out of all words.
    pub placed: u32,
    pub total: u32,
    /// Mark valid is lit: the verse is marked done and still vouches for itself.
    pub done: bool,
    /// The last Mark valid was refused because suggestions stand; the next edit clears it.
    pub mark_refused: bool,
    pub suggest: SuggestView,
    pub freshness: Freshness,
    pub dropped: Vec<DroppedLinkView>,
    /// The open verse's row on the rail, so the rail follows each edit without asking again.
    pub status: RailStatus,
    pub save: AlignSave,
}

/// `align/index`: every verse of the book as the rail lists it, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Index {
    pub at: Scope,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: BookId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct IndexView {
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub items: Vec<IndexItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct IndexItem {
    pub place: AlignPlace,
    /// The start of the verse's text, for the row. Empty for an undrafted verse.
    pub text: String,
    pub status: RailStatus,
}

/// `align/open`: the aligner for one verse of the book. A verse whose session is live returns
/// that session; any other verse replaces the open one, which is saved first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Open {
    pub at: Scope,
    pub request: RequestId,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: BookId,
    /// The verse to open; `None` opens where the book resumes: the first verse not done, else
    /// the first drafted one.
    pub verse: Option<OpenVerse>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenVerse {
    pub chapter: u16,
    pub verse: u16,
}

/// What `align/open` found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Opened {
    Ready(Box<AlignerView>),
    /// There is nothing to align here; the screen says why. Nothing is lost. `at` is the verse
    /// that was asked for or that the book resumes at, so the rail can show it as the open one;
    /// none when the book has no drafted verse.
    Blocked {
        why: Blocked,
        at: Option<OpenVerse>,
    },
}

/// What a click, a drop or a button does to the open verse (tC4's `applyAlignEdit` family).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum AlignEdit {
    /// A click, or Enter or Space, on a bank chip, with the keys held. The chip is named by
    /// its word, not its place: a click made before the reply to the one before it came must
    /// still mean the chip the person saw.
    ClickBank {
        word: Word,
        click: BankClick,
    },
    /// A click on a card while bank words are selected: the selection is placed on it.
    PlaceSelection {
        card: u32,
    },
    /// A drop of bank words on a card.
    PlaceWords {
        card: u32,
        words: Vec<Word>,
    },
    /// A click on a placed word: back to the bank.
    Unplace {
        card: u32,
        word: Word,
    },
    /// A drop of a placed word on another card.
    Move {
        from: u32,
        to: u32,
        word: Word,
    },
    /// A drop of a card on its neighbour: they become one phrase.
    Merge {
        from: u32,
        to: u32,
    },
    /// SPLIT on a phrase.
    Split {
        card: u32,
    },
    /// Mark valid: the verse is done, words left in the bank or not (D73).
    MarkValid,
    /// The check mark on a suggested chip.
    Accept {
        card: u32,
        word: Word,
    },
    AcceptAll,
    /// Reject all, and Reset.
    RejectAll,
}

/// What a click on a bank chip does, from the keys held with it (tc-align `Click`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum BankClick {
    /// The word alone, or nothing when it is the only word selected.
    Plain,
    /// Ctrl or Cmd: add the word, or take it out.
    Toggle,
    /// Shift: add every word from the first selected word to this one.
    Range,
}

/// `align/edit`: one edit of the open verse. `seq` is the view the edit was made on; a view
/// that is not the newest, or an aligner that was replaced, is `StaleSession`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Edit {
    pub at: Scope,
    pub request: RequestId,
    pub aligner: AlignerId,
    pub seq: u64,
    pub edit: AlignEdit,
}

/// `align/keepReflow`: the person has seen the links the carry-over dropped and keeps the rest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeepReflow {
    pub at: Scope,
    pub request: RequestId,
    pub aligner: AlignerId,
    pub seq: u64,
}

/// `align/flush`: save the open verse's links now. The navigation gate calls it, and a failure
/// keeps the person where they are (FR-32). With no aligner open, or nothing unsaved, it
/// answers `Saved`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Flush {
    pub at: Scope,
    pub request: RequestId,
}

/// `align/close`: the person left the Align tool. The links are saved first; if that fails the
/// aligner stays open and unsaved, and the answer says so.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Close {
    pub at: Scope,
    pub request: RequestId,
    pub aligner: AlignerId,
}

/// `align/suggest`: ask the engine for links for the words still in the bank. The proposals
/// stand on their cards, dashed, until the person accepts or rejects them; nothing is written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Suggest {
    pub at: Scope,
    pub request: RequestId,
    pub aligner: AlignerId,
}
