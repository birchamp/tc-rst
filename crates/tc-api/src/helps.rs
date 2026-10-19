//! The `helps` service (APP-DESIGN §3.7, §3.9, unit B3): the translation helps of one chapter
//! of the open book, and an article. The server reads a book's helps once per open book
//! (`helps_load_one_book`) and answers a chapter from them.

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::ids::{RefPart, Scope};
use crate::model::ArticleBlock;
use crate::shown::Direction;

/// `helps/passage`: the helps of one chapter of the open book.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Passage {
    pub at: Scope,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: BookId,
    pub chapter: u16,
}

/// The helps panel's tabs for one chapter: each a slot that is ready or says why not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct PassageView {
    pub notes: Slot<NoteCard>,
    pub words: Slot<WordCard>,
    pub questions: Slot<QuestionCard>,
    pub simplified: Slot<SimplifiedVerse>,
    /// The repository of the pinned simplified text (`en_ust`): its tab is named after it
    /// (tC4's `simplifiedLabel`), whatever its state.
    pub simplified_pin: Option<String>,
}

/// One tab's content, or why it has none (tC4's `SlotState`, D30: absence is stated).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Slot<T> {
    Ready(ReadySlot<T>),
    /// The language's package does not include the resource.
    None,
    /// The pinned resource is not on this computer yet.
    Fetch,
    /// The installed resource has nothing for this book.
    Missing,
    /// A pre-release words-links pin, which is never read.
    OldTwl,
    Error {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ReadySlot<T> {
    /// The chapter's items, in the resource's order.
    pub items: Vec<T>,
    /// The direction of the gateway language that answered (#650); none when the app knows
    /// no gateway by the set's language, and the cards then take the page's.
    pub dir: Option<Direction>,
    /// English answered because the primary language's resource is not on this computer.
    pub fallback: bool,
}

/// Where a card's quote is in one source pane: its title in that pane's words, and the
/// tokens of each verse that render it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct PaneFocus {
    /// The pane's id, as `editor/sources` names it.
    pub pane: String,
    /// The pane's rendering of the quote; none when nothing renders it.
    pub title: Option<String>,
    pub lit: Vec<LitVerse>,
}

/// The tokens of one pane verse that render a quote, by index into its `tokens`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct LitVerse {
    pub verse: RefPart,
    pub tokens: Vec<u32>,
}

/// A translation note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct NoteCard {
    /// The row's id, unique in the book.
    pub id: String,
    /// The reference as written: chapter and verse parts (`1`, `intro`, `1-3`).
    pub chapter: RefPart,
    pub verse: RefPart,
    /// The original-language quote; empty for an introduction.
    pub quote: String,
    pub occurrence: i32,
    pub note: Vec<ArticleBlock>,
    /// The Translation Academy article the note names (its slug).
    pub academy: Option<String>,
    pub panes: Vec<PaneFocus>,
}

/// A link to a translation words article.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct WordCard {
    pub id: String,
    pub chapter: RefPart,
    pub verse: RefPart,
    /// The original words.
    pub quote: String,
    pub occurrence: i32,
    /// The article's folder, `kt`, `names` or `other`.
    pub category: Option<String>,
    /// The article's name: the card's body, and the link.
    pub slug: String,
    pub panes: Vec<PaneFocus>,
}

/// A comprehension question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct QuestionCard {
    pub id: String,
    pub chapter: RefPart,
    pub verse: RefPart,
    pub question: String,
    pub response: String,
}

/// One verse of the simplified text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct SimplifiedVerse {
    pub key: RefPart,
    pub text: String,
}

/// `helps/article`: a Translation Academy or Translation Words article of the open book's
/// helps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Article {
    pub at: Scope,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: BookId,
    pub link: ArticleLink,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ArticleLink {
    /// A note's article: its slug.
    Academy { slug: String },
    /// A word's article.
    Words {
        category: Option<String>,
        slug: String,
    },
}

/// An article, or the statement that the pinned resources do not carry it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ArticleView {
    Found {
        title: String,
        blocks: Vec<ArticleBlock>,
        dir: Direction,
    },
    Missing,
}
