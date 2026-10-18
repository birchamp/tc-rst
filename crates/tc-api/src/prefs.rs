//! The `prefs` service: this person's preferences on this computer, never in the project
//! (§4.9). A failed write keeps the value for the session and adds a report, except the app
//! language, whose dialog waits for the save (`PrefsRefusal::LocaleNotSaved`).

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::check::Tool;
use crate::ids::{LocaleId, RefPart, RequestId, Scope, StoryNo};

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

/// One observation of the place in a book or a story, one record each (tC4 `rememberPlace`,
/// `tC4:src/state.jsx:3821-3845`). A mode, chapter or tool left out keeps the stored one;
/// the verse or frame says whether it is kept, cleared or set (`recordPlace`): a mode switch
/// keeps the verse, a chapter click clears it, a frame click keeps the mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PlaceUpdate {
    Book {
        #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
        book: BookId,
        mode: Option<PlaceMode>,
        chapter: Option<u16>,
        /// The key of a Translate verse (`4`, or `4-5` for a bridge; `editVerse`,
        /// `tC4:src/state.jsx:7191`), or a check item's verse as its reference writes it
        /// (`intro`, `front`, `3,5`, `1a`; `referenceParts`, `tC4:src/state.jsx:3874`). tC4
        /// keeps either as given.
        verse: PlaceChange<RefPart>,
        /// The Check tool, when the mode is Check.
        tool: Option<Tool>,
    },
    /// A story. tC4 records its number as the chapter and its frame as the verse.
    Story {
        story: StoryNo,
        mode: Option<PlaceMode>,
        /// The frame; 0 is the title.
        frame: PlaceChange<u16>,
        tool: Option<Tool>,
    },
}

/// What an observation does to the stored verse or frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum PlaceChange<T> {
    /// The observation names none, so the stored one stays (a mode switch).
    Keep,
    /// No verse is known now (a chapter click; tC4 stores `null`).
    Clear,
    Set(T),
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
