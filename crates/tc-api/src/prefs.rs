//! The `prefs` service: this person's preferences on this computer, never in the project
//! (§4.9). A failed write keeps the value for the session and adds a report, except the app
//! language, whose dialog waits for the save (`PrefsRefusal::LocaleNotSaved`).

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::check::Tool;
use crate::ids::{LocaleId, RequestId, Scope};

/// `prefs/setLocale`: saves the app language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetLocale {
    pub request: RequestId,
    pub locale: LocaleId,
}

/// `prefs/setDraftUnit`: how Translate shows the open project, by section or by verse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetDraftUnit {
    pub at: Scope,
    pub request: RequestId,
    pub unit: DraftUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum DraftUnit {
    Section,
    Verse,
}

/// `prefs/setAlignSuggestions`: the aligner's suggestions switch for the open project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetAlignSuggestions {
    pub at: Scope,
    pub request: RequestId,
    pub on: bool,
}

/// `prefs/recordPlace`: where the person works in the open project, so a Home tile
/// returns there (`tC4:src/data/place.ts`). Sent when the person moves, not per keystroke;
/// the server writes it 500 ms after the last one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordPlace {
    pub at: Scope,
    pub request: RequestId,
    pub place: PlaceUpdate,
}

/// One observation of the place. A field left out keeps the stored value: a mode switch
/// keeps the verse, a frame click keeps the mode (`recordPlace`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlaceUpdate {
    pub unit: PlaceUnit,
    pub mode: Option<PlaceMode>,
    /// The chapter, or for a story the story number.
    pub chapter: Option<u16>,
    /// The verse, or for a story the frame (0 is the title).
    pub verse: Option<u16>,
    /// The Check tool, when the mode is Check.
    pub tool: Option<Tool>,
}

/// A book or a story: one place record each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum PlaceUnit {
    Book(#[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))] BookId),
    Story(u8),
}

/// The mode a place records. Community Checking records as Check (tC4 D63).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum PlaceMode {
    Read,
    Draft,
    Check,
}
