//! The states of the aligner's view (APP-DESIGN §3.7, AD18, A-30). Unit B6 adds the view.

use serde::{Deserialize, Serialize};

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

/// The word bank's selection, by word index in the verse.
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
    Selected(Vec<u32>),
    /// Selected and ready to drop onto a card with the next click.
    Armed(Vec<u32>),
}

/// Whether the aligner shows the verse text as it is now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Freshness {
    Current,
    /// The verse text changed since the aligner opened it.
    Stale,
}
