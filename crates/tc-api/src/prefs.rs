//! The `prefs` service: this person's preferences on this computer, never in the project
//! (§4.9). A failed write keeps the value for the session and adds a report, except the app
//! language, whose dialog waits for the save (`PrefsRefusal::LocaleNotSaved`).

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::check::Tool;
use crate::ids::{IdError, LocaleId, RequestId, Scope};

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
        verse: PlaceChange<PlaceVerse>,
        /// The Check tool, when the mode is Check.
        tool: Option<Tool>,
    },
    /// A story. tC4 records its number as the chapter and its frame as the verse.
    Story {
        story: u8,
        mode: Option<PlaceMode>,
        /// The frame; 0 is the title.
        frame: PlaceChange<u16>,
        tool: Option<Tool>,
    },
}

/// What an observation does to the stored verse or frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum PlaceChange<T> {
    /// The observation names none, so the stored one stays (a mode switch).
    Keep,
    /// No verse is known now (a chapter click; tC4 stores `null`).
    Clear,
    Set(T),
}

/// A verse as a place records it: the key of a Translate verse (`4`, or `4-5` for a
/// bridge; `editVerse`, `tC4:src/state.jsx:7191`), or a check item's verse as its
/// reference writes it (`intro`, `front`, `3,5`, `1a`; `referenceParts`,
/// `tC4:src/state.jsx:3874`). tC4 keeps either as given, so this keeps any text that a
/// reference part can be: not empty and without `:` (tc-check's `RefPart`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlaceVerse(String);

impl PlaceVerse {
    /// Far longer than any verse part a resource writes; it bounds what a request carries.
    pub const MAX_BYTES: usize = 256;

    pub fn parse(text: &str) -> Result<Self, IdError> {
        const WHAT: &str = "verse";
        if text.is_empty() {
            return Err(IdError {
                what: WHAT,
                reason: "empty",
            });
        }
        if text.len() > Self::MAX_BYTES {
            return Err(IdError {
                what: WHAT,
                reason: "too long",
            });
        }
        if text.contains(':') {
            return Err(IdError {
                what: WHAT,
                reason: "holds a chapter separator",
            });
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for PlaceVerse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PlaceVerse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "proptest")]
impl proptest::arbitrary::Arbitrary for PlaceVerse {
    type Parameters = ();
    type Strategy = proptest::strategy::BoxedStrategy<Self>;
    fn arbitrary_with((): ()) -> Self::Strategy {
        use proptest::prelude::*;
        let forms = prop_oneof![
            (1u16..=176, 0u16..3).prop_map(|(v, extra)| if extra == 0 {
                v.to_string()
            } else {
                format!("{v}-{}", v + extra)
            }),
            proptest::sample::select(&["intro", "front", "3,5", "1a"][..]).prop_map(str::to_owned),
            // The pattern is a literal that compiles.
            proptest::string::string_regex("[^:]{1,20}")
                .unwrap_or_else(|_| unreachable!("a valid regex")),
        ];
        forms.prop_map(PlaceVerse).boxed()
    }
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
