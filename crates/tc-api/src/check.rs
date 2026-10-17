//! The parts of the Check screen's types that a write's `Ack` and a change scope carry
//! (APP-DESIGN §3.7, AD17). Unit B5a adds the session and item views.

use serde::{Deserialize, Serialize};
use tc_core::{BookId, Reference};

use crate::shown::ConflictMarker;

/// A checking tool, by tC4's tool id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Tool {
    TranslationNotes,
    TranslationWords,
}

/// One tool's check list for one book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct CheckListRef {
    pub tool: Tool,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: BookId,
}

/// The group a check is listed under in By category (tc-check `Category`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Category {
    Note(NoteCategory),
    Word(WordCategory),
}

/// A translationNotes category, as tC4 names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum NoteCategory {
    Discourse,
    Numbers,
    Figures,
    Culture,
    Grammar,
    Other,
}

/// The folder of a translationWords article.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum WordCategory {
    Kt,
    Names,
    Other,
}

/// The status dot of a rail row. An invalidated check shows red.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Dot {
    Valid,
    Invalid,
    Todo,
}

/// What the rail's filter chips read from a row. Each field is a separate fact a chip
/// tests; together they are not one state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct RowFlags {
    pub dot: Dot,
    /// tC4's `isDecided` (tc-check `CheckState::is_decided`): progress and "To do" read it.
    pub decided: bool,
    pub bookmarked: bool,
    pub commented: bool,
}

/// One row of the check rail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct RailRow {
    /// The check's place in the session's list.
    pub index: u32,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::reference()"))]
    pub reference: Reference,
    pub title: String,
    pub flags: RowFlags,
    /// The tW term, as an index into the session's terms.
    pub term: Option<u32>,
    pub category: Option<Category>,
    pub marker: Option<ConflictMarker>,
}

/// A rail row a write or a re-assessment changed. It replaces the row with the same index,
/// so the UI patches its rail instead of fetching it again (AD17).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct RailRowPatch {
    pub row: RailRow,
}
