//! Where a project's helps come from, and what a check needs.
//!
//! The helps tabs (notes, words, academy, questions) read from Door43 or from Aquifer;
//! the owner requires both. Choosing Aquifer changes what the tabs show and nothing else:
//! a check list always comes from the project's Door43 language sets and is checked
//! against the Door43 original-language text (UGNT or UHB). Aquifer's notes record no
//! occurrence and its words name no original-language words (`aquifer.md`, "Can Aquifer
//! tN drive a check"), and Aquifer's own Greek and Hebrew are SBLGNT and WLC, whose words
//! are not the ones the notes quote. So [`check_sources`] never reads the choice, and the
//! types make the rule hold: the original-language pins are Door43 pins.
//!
//! # How the stored record maps to this type
//!
//! STORAGE has no pin group for Aquifer. The choice is the project setting `helpsSource`
//! (a `Setting` record, one top-level key of `settings.json`) whose value is this type's
//! JSON: `{"provider": "dcs"}` or `{"provider": "aquifer", "notes": {pin}, ...}`, each
//! Aquifer pin in [`AquiferPin`]'s shape. A project without the setting uses Door43.

use serde::{Deserialize, Serialize};
use tc_core::{BookId, Testament};
use tc_helps::aquifer::AquiferHelpsKind;

use crate::error::ParseError;
use crate::layout;
use crate::pin::{AquiferPin, ResourcePin};
use crate::resolve::{PreflightState, Resolution, preflight_tool_book};
use crate::sets::{ProjectPins, SetSlot};
use crate::store::ResourceStore;
use tc_journal::keys::ToolId;

/// The project setting's key.
pub const HELPS_SOURCE_SETTING: &str = "helpsSource";

/// Where the helps tabs read from.
// One value per open project, so the size of the Aquifer pins costs nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "camelCase")]
pub enum HelpsChoice {
    /// The project's Door43 language sets, over the two-rung ladder.
    #[default]
    Dcs,
    /// Aquifer's mirrors of the unfoldingWord helps, one pinned asset per kind.
    Aquifer(AquiferHelps),
}

/// The Aquifer assets a project reads its helps tabs from. A kind with no pin is a tab
/// that says it has nothing pinned.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AquiferHelps {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<AquiferPin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<AquiferPin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub academy: Option<AquiferPin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub questions: Option<AquiferPin>,
}

impl AquiferHelps {
    /// Each pin with the kind its slot holds.
    pub fn pins(&self) -> impl Iterator<Item = (AquiferHelpsKind, &AquiferPin)> {
        [
            (AquiferHelpsKind::Notes, &self.notes),
            (AquiferHelpsKind::Words, &self.words),
            (AquiferHelpsKind::Academy, &self.academy),
            (AquiferHelpsKind::Questions, &self.questions),
        ]
        .into_iter()
        .filter_map(|(kind, pin)| pin.as_ref().map(|p| (kind, p)))
    }

    /// Refuses a pin of another resource in a slot: the notes slot must pin
    /// `UWTranslationNotes`, and so on.
    pub fn check(&self) -> Result<(), ParseError> {
        for (kind, pin) in self.pins() {
            let found = AquiferHelpsKind::from_resource_code(pin.repo.name.as_str());
            if found != Some(kind) {
                return Err(ParseError::Pins(format!(
                    "{} cannot be the Aquifer {kind:?} pin",
                    pin.repo
                )));
            }
        }
        Ok(())
    }
}

impl HelpsChoice {
    /// Reads the `helpsSource` setting's value, checking each Aquifer pin's slot.
    pub fn from_setting(value: &serde_json::Value) -> Result<Self, ParseError> {
        let choice = HelpsChoice::deserialize(value)
            .map_err(|e| ParseError::Pins(format!("{HELPS_SOURCE_SETTING}: {e}")))?;
        if let HelpsChoice::Aquifer(helps) = &choice {
            helps.check()?;
        }
        Ok(choice)
    }

    pub fn to_setting(&self) -> serde_json::Value {
        // Plain strings and objects always serialize.
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// Why a check cannot run for a (tool, book).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckBlocker {
    /// The check list's resource cannot be opened: the preflight state says why, and
    /// names the pin to fetch when there is one.
    List {
        state: PreflightState,
        needs: Option<ResourcePin>,
    },
    /// No Door43 original-language text is pinned for the book's testament.
    OriginalLanguageNotPinned(Testament),
    /// The pinned original-language text is not installed on this computer.
    OriginalLanguageNotInstalled(ResourcePin),
    /// The installed original-language text does not hold the book.
    OriginalLanguageLacksBook(ResourcePin),
}

/// What a check of one (tool, book) reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckSources {
    pub tool: ToolId,
    pub book: BookId,
    /// The notes or words links the check list comes from (the D30 ladder).
    pub list: Resolution,
    /// For tW, the resolving set's articles (D97).
    pub articles: Option<ResourcePin>,
    /// The Door43 UGNT or UHB the quotes and alignments are read against.
    pub original_language: ResourcePin,
}

/// What a check of `tool` in `book` reads, or every reason it cannot run. The helps
/// choice is not an input: see the module documentation.
pub fn check_sources(
    store: &ResourceStore,
    pins: &ProjectPins,
    tool: ToolId,
    book: BookId,
    online: bool,
) -> Result<CheckSources, Vec<CheckBlocker>> {
    let installed = |pin: &ResourcePin| store.has_dcs(pin);
    let preflight = preflight_tool_book(Some(pins), tool, book, store, &installed, online);
    let mut blockers = Vec::new();
    let list = match (preflight.state, preflight.resolution) {
        (PreflightState::Ready, Some(resolution)) => Some(resolution),
        (state, _) => {
            blockers.push(CheckBlocker::List {
                state,
                needs: preflight.needs,
            });
            None
        }
    };
    let original = match pins.original_language.for_book(book) {
        None => {
            blockers.push(CheckBlocker::OriginalLanguageNotPinned(book.testament()));
            None
        }
        Some(pin) => match store.dcs(pin) {
            Ok(Some(install)) if layout::book_file(install.files(), book, "usfm").is_some() => {
                Some(pin.clone())
            }
            Ok(Some(_)) => {
                blockers.push(CheckBlocker::OriginalLanguageLacksBook(pin.clone()));
                None
            }
            Ok(None) | Err(_) => {
                blockers.push(CheckBlocker::OriginalLanguageNotInstalled(pin.clone()));
                None
            }
        },
    };
    match (list, original) {
        (Some(list), Some(original_language)) if blockers.is_empty() => {
            let articles = (tool == ToolId::TranslationWords)
                .then(|| pins.set(list.rung)?.readable_pin(SetSlot::TranslationWords))
                .flatten();
            Ok(CheckSources {
                tool,
                book,
                list,
                articles,
                original_language,
            })
        }
        _ => Err(blockers),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(repo: &str) -> AquiferPin {
        serde_json::from_value(serde_json::json!({
            "repo": format!("BibleAquifer/{repo}"),
            "tag": "v2026-10-12",
            "commit": "dbfb46e027ee0807563918f4b95c80879cc32c60",
            "language": "por",
            "asset": "Portuguese-json.zip",
            "sha256": "646074262e54f0aaebbeb5e416f411a6c7df56204dcb560bc24d339c409888f3"
        }))
        .unwrap()
    }

    #[test]
    fn the_setting_round_trips_and_checks_each_slot() {
        assert_eq!(
            HelpsChoice::from_setting(&serde_json::json!({"provider": "dcs"})).unwrap(),
            HelpsChoice::Dcs
        );
        let choice = HelpsChoice::Aquifer(AquiferHelps {
            notes: Some(pin("UWTranslationNotes")),
            ..AquiferHelps::default()
        });
        let value = choice.to_setting();
        assert_eq!(value["provider"], "aquifer");
        assert_eq!(value["notes"]["repo"], "BibleAquifer/UWTranslationNotes");
        assert_eq!(HelpsChoice::from_setting(&value).unwrap(), choice);

        let misplaced = serde_json::json!({
            "provider": "aquifer",
            "notes": serde_json::to_value(pin("UWTranslationWords")).unwrap()
        });
        assert!(HelpsChoice::from_setting(&misplaced).is_err());
        assert!(HelpsChoice::from_setting(&serde_json::json!({"provider": "api"})).is_err());
    }
}
