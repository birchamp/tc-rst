//! A project's pins, in tC4's §5.3 shape: two language sets (the gateway language and the
//! English fallback), the original-language texts, the lexicons and the extra scripture
//! panes.
//!
//! # How the stored records map to these types
//!
//! STORAGE §2.2 keeps `resources.json` as five `Record::Pins { group, value }` records, one
//! per [`PinGroup`]. A `PinGroupValue` is a map from tC4 slot name to slot value; this crate
//! reads and writes that map as a JSON object with [`ProjectPins::group_json`] and
//! [`ProjectPins::set_group_json`]:
//!
//! | `PinGroup` | Here | Slots (tC4 names) |
//! |---|---|---|
//! | `Primary` | [`ProjectPins::primary`] | `gatewayLanguage` and the [`SetSlot`]s |
//! | `Fallback` | [`ProjectPins::fallback`] | the same |
//! | `OriginalLanguage` | [`ProjectPins::original_language`] | `nt`, `ot` |
//! | `Lexicon` | [`ProjectPins::lexicon`] | `nt`, `ot` |
//! | `ExtraScripture` | [`ProjectPins::extra_scripture`] | one slot, `extraScripture`, holding the ordered list of `{id, ...pin}` |
//!
//! A slot this version does not know (from a newer app) is kept in
//! [`LanguageSet::extra`] and written back unchanged.
//!
//! A gateway change rewrites `Primary` and `ExtraScripture` in one batch; a help-set
//! upgrade rewrites one rung; a text upgrade rewrites the groups that pin the text
//! (STORAGE §5.8).

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tc_core::{BookId, Testament};
use tc_door43::{AccountName, RepoRef};

use crate::error::ParseError;
use crate::pin::ResourcePin;

/// The two rungs of the ladder (D30): exactly these, in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rung {
    Primary,
    Fallback,
}

impl Rung {
    pub const LADDER: [Rung; 2] = [Rung::Primary, Rung::Fallback];
}

impl fmt::Display for Rung {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Rung::Primary => "primary",
            Rung::Fallback => "fallback",
        })
    }
}

/// STORAGE §2.2's `PinGroup`: the five records `resources.json` is projected from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PinGroup {
    Primary,
    Fallback,
    OriginalLanguage,
    Lexicon,
    ExtraScripture,
}

impl PinGroup {
    pub const ALL: [PinGroup; 5] = [
        PinGroup::Primary,
        PinGroup::Fallback,
        PinGroup::OriginalLanguage,
        PinGroup::Lexicon,
        PinGroup::ExtraScripture,
    ];
}

impl From<Rung> for PinGroup {
    fn from(rung: Rung) -> Self {
        match rung {
            Rung::Primary => PinGroup::Primary,
            Rung::Fallback => PinGroup::Fallback,
        }
    }
}

/// A resource slot of a language set, with tC4's slot name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SetSlot {
    TranslationNotes,
    TranslationWordsLinks,
    TranslationWords,
    TranslationAcademy,
    TranslationQuestions,
    /// The language's simplified Bible (`en_ust`, `<lang>_gst`): a gateway Bible, not a
    /// help, so a help-set upgrade never offers it (D72).
    SimplifiedText,
    Obs,
    ObsNotes,
    ObsWordsLinks,
    ObsQuestions,
    ObsImages,
}

impl SetSlot {
    /// Every slot, in tC4's fixed order for `resources.json`.
    pub const ALL: [SetSlot; 11] = [
        SetSlot::TranslationNotes,
        SetSlot::TranslationWordsLinks,
        SetSlot::TranslationWords,
        SetSlot::TranslationAcademy,
        SetSlot::TranslationQuestions,
        SetSlot::SimplifiedText,
        SetSlot::Obs,
        SetSlot::ObsNotes,
        SetSlot::ObsWordsLinks,
        SetSlot::ObsQuestions,
        SetSlot::ObsImages,
    ];

    /// The help slots an upgrade offers, in display order (tC4 `SET_SLOTS`).
    pub const HELPS: [SetSlot; 9] = [
        SetSlot::TranslationNotes,
        SetSlot::TranslationWordsLinks,
        SetSlot::TranslationWords,
        SetSlot::TranslationAcademy,
        SetSlot::TranslationQuestions,
        SetSlot::Obs,
        SetSlot::ObsNotes,
        SetSlot::ObsWordsLinks,
        SetSlot::ObsImages,
    ];

    /// tC4's slot name in `resources.json`.
    pub fn name(self) -> &'static str {
        match self {
            SetSlot::TranslationNotes => "translationNotes",
            SetSlot::TranslationWordsLinks => "translationWordsLinks",
            SetSlot::TranslationWords => "translationWords",
            SetSlot::TranslationAcademy => "translationAcademy",
            SetSlot::TranslationQuestions => "translationQuestions",
            SetSlot::SimplifiedText => "simplifiedText",
            SetSlot::Obs => "obs",
            SetSlot::ObsNotes => "obs-tn",
            SetSlot::ObsWordsLinks => "obs-twl",
            SetSlot::ObsQuestions => "obs-tq",
            SetSlot::ObsImages => "obs-images",
        }
    }

    /// The Bible-book slots whose coverage a resolution reads (tC4 `coverageBackfill`
    /// `SET_SLOTS`). Open Bible Stories slots cover the whole collection.
    pub fn is_per_book(self) -> bool {
        matches!(
            self,
            SetSlot::TranslationNotes
                | SetSlot::TranslationWordsLinks
                | SetSlot::TranslationWords
                | SetSlot::TranslationAcademy
                | SetSlot::TranslationQuestions
                | SetSlot::SimplifiedText
        )
    }
}

impl fmt::Display for SetSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for SetSlot {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SetSlot::ALL
            .into_iter()
            .find(|slot| slot.name() == s)
            .ok_or_else(|| ParseError::Slot(s.to_owned()))
    }
}

/// A gateway language as a set names it: tC4's `gatewayLanguage: {languageId, owner}`.
/// The owner matters: `fr` is published by both Xenizo and MVHS.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GatewayLanguage {
    /// BCP 47, as Door43 reports it, such as `es-419`.
    pub language: String,
    pub owner: AccountName,
}

/// One language set: the gateway language and the resources pinned for it. A set read
/// from a project may be partial; [`LanguageSet::is_complete`] says whether it has what a
/// Bible or OBS project needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageSet {
    pub gateway: GatewayLanguage,
    pub pins: BTreeMap<SetSlot, ResourcePin>,
    /// Slots this version does not know, kept as they were read and written back.
    pub extra: Map<String, Value>,
}

/// What a project is about, which decides the slots a complete set needs (D75).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectKind {
    Bible,
    Obs,
}

impl LanguageSet {
    pub fn new(gateway: GatewayLanguage) -> Self {
        Self {
            gateway,
            pins: BTreeMap::new(),
            extra: Map::new(),
        }
    }

    pub fn pin(&self, slot: SetSlot) -> Option<&ResourcePin> {
        self.pins.get(&slot)
    }

    pub fn with(mut self, slot: SetSlot, pin: ResourcePin) -> Self {
        self.pins.insert(slot, pin);
        self
    }

    /// The slots a complete set of this kind pins (tC4 `burritoStore.ts` completeness).
    pub fn required(kind: ProjectKind) -> &'static [SetSlot] {
        match kind {
            ProjectKind::Bible => &[
                SetSlot::TranslationNotes,
                SetSlot::TranslationWordsLinks,
                SetSlot::TranslationWords,
                SetSlot::TranslationAcademy,
            ],
            ProjectKind::Obs => &[
                SetSlot::Obs,
                SetSlot::ObsNotes,
                SetSlot::ObsWordsLinks,
                SetSlot::TranslationWords,
                SetSlot::TranslationAcademy,
            ],
        }
    }

    pub fn is_complete(&self, kind: ProjectKind) -> bool {
        Self::required(kind)
            .iter()
            .all(|slot| self.pins.contains_key(slot))
    }

    /// One entry per distinct repository among the help slots, with every slot that
    /// names it (tC4 `reposOfSet`). The simplified Bible is not a help (D72).
    pub fn help_repos(&self) -> Vec<(ResourcePin, Vec<SetSlot>)> {
        let mut out: Vec<(ResourcePin, Vec<SetSlot>)> = Vec::new();
        for slot in SetSlot::HELPS {
            let Some(pin) = self.pins.get(&slot) else {
                continue;
            };
            match out.iter_mut().find(|(held, _)| held.repo == pin.repo) {
                Some((_, slots)) => slots.push(slot),
                None => out.push((pin.clone(), vec![slot])),
            }
        }
        out
    }

    /// The tC4 §5.3 object: `gatewayLanguage`, then each pinned slot in tC4's order, then
    /// the slots this version does not know.
    pub fn to_json(&self) -> Value {
        let mut out = Map::new();
        out.insert(
            GATEWAY_LANGUAGE.to_owned(),
            serde_json::json!({
                "languageId": self.gateway.language,
                "owner": self.gateway.owner.as_str(),
            }),
        );
        for slot in SetSlot::ALL {
            if let Some(pin) = self.pins.get(&slot) {
                out.insert(slot.name().to_owned(), pin_json(pin));
            }
        }
        for (key, value) in &self.extra {
            out.insert(key.clone(), value.clone());
        }
        Value::Object(out)
    }

    /// Reads a tC4 §5.3 set object. A malformed pin in a known slot is refused, so a bad
    /// record is reported where it is read; an unknown slot is kept.
    pub fn from_json(value: &Value) -> Result<Self, ParseError> {
        let object = value
            .as_object()
            .ok_or_else(|| ParseError::Pins("a language set is not an object".to_owned()))?;
        let gateway = object
            .get(GATEWAY_LANGUAGE)
            .ok_or_else(|| ParseError::Pins("a language set has no gatewayLanguage".to_owned()))?;
        let text = |key: &str| {
            gateway
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| ParseError::Pins(format!("gatewayLanguage has no {key}")))
        };
        let language = text("languageId")?.to_owned();
        let owner = text("owner")?
            .parse()
            .map_err(|_| ParseError::Pins(format!("gatewayLanguage owner {:?}", text("owner"))))?;
        let mut set = LanguageSet::new(GatewayLanguage { language, owner });
        for (key, value) in object {
            if key == GATEWAY_LANGUAGE {
                continue;
            }
            match key.parse::<SetSlot>() {
                Ok(slot) => {
                    set.pins.insert(slot, read_pin(key, value)?);
                }
                Err(_) => {
                    set.extra.insert(key.clone(), value.clone());
                }
            }
        }
        Ok(set)
    }
}

const GATEWAY_LANGUAGE: &str = "gatewayLanguage";
const EXTRA_SCRIPTURE: &str = "extraScripture";

fn pin_json(pin: &ResourcePin) -> Value {
    // A pin is plain strings and lists, which always serialize.
    serde_json::to_value(pin).unwrap_or(Value::Null)
}

fn read_pin(slot: &str, value: &Value) -> Result<ResourcePin, ParseError> {
    ResourcePin::deserialize(value).map_err(|e| ParseError::Pins(format!("slot {slot}: {e}")))
}

/// A pin per testament: the original-language texts and the lexicons.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Testaments {
    pub nt: Option<ResourcePin>,
    pub ot: Option<ResourcePin>,
}

impl Testaments {
    pub fn for_book(&self, book: BookId) -> Option<&ResourcePin> {
        self.get(book.testament())
    }

    pub fn get(&self, testament: Testament) -> Option<&ResourcePin> {
        match testament {
            Testament::New => self.nt.as_ref(),
            Testament::Old => self.ot.as_ref(),
        }
    }

    pub fn set(&mut self, testament: Testament, pin: Option<ResourcePin>) {
        match testament {
            Testament::New => self.nt = pin,
            Testament::Old => self.ot = pin,
        }
    }

    fn to_json(&self) -> Value {
        let mut out = Map::new();
        if let Some(pin) = &self.nt {
            out.insert("nt".to_owned(), pin_json(pin));
        }
        if let Some(pin) = &self.ot {
            out.insert("ot".to_owned(), pin_json(pin));
        }
        Value::Object(out)
    }

    fn from_json(value: &Value) -> Result<Self, ParseError> {
        let object = value
            .as_object()
            .ok_or_else(|| ParseError::Pins("a testament group is not an object".to_owned()))?;
        let mut out = Testaments::default();
        for (key, value) in object {
            match key.as_str() {
                "nt" => out.nt = Some(read_pin(key, value)?),
                "ot" => out.ot = Some(read_pin(key, value)?),
                other => return Err(ParseError::Pins(format!("unknown testament {other:?}"))),
            }
        }
        Ok(out)
    }
}

/// The id of a source pane: `ult`, `ust`, `glt`, `gst`, or another a project names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PaneId(String);

impl PaneId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PaneId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for PaneId {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let ok = !s.is_empty()
            && s.len() <= 64
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'));
        if ok {
            Ok(Self(s.to_owned()))
        } else {
            Err(ParseError::Slot(s.to_owned()))
        }
    }
}

impl TryFrom<String> for PaneId {
    type Error = ParseError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<PaneId> for String {
    fn from(id: PaneId) -> Self {
        id.0
    }
}

/// One source pane of `extraScripture`: a gateway Bible shown beside the translation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScripturePane {
    pub id: PaneId,
    #[serde(flatten)]
    pub pin: ResourcePin,
}

/// A project's pins: the five `PinGroup` values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectPins {
    pub primary: Option<LanguageSet>,
    pub fallback: Option<LanguageSet>,
    pub original_language: Testaments,
    pub lexicon: Testaments,
    pub extra_scripture: Vec<ScripturePane>,
}

impl ProjectPins {
    pub fn set(&self, rung: Rung) -> Option<&LanguageSet> {
        match rung {
            Rung::Primary => self.primary.as_ref(),
            Rung::Fallback => self.fallback.as_ref(),
        }
    }

    pub fn set_mut(&mut self, rung: Rung) -> &mut Option<LanguageSet> {
        match rung {
            Rung::Primary => &mut self.primary,
            Rung::Fallback => &mut self.fallback,
        }
    }

    /// Whether the project pins any language set (tC4: `languageSets` present).
    pub fn has_language_sets(&self) -> bool {
        self.primary.is_some() || self.fallback.is_some()
    }

    /// Every pin the project holds, each once per place it is pinned.
    pub fn all_pins(&self) -> Vec<&ResourcePin> {
        let mut out = Vec::new();
        for rung in Rung::LADDER {
            if let Some(set) = self.set(rung) {
                out.extend(set.pins.values());
            }
        }
        for group in [&self.original_language, &self.lexicon] {
            out.extend(group.nt.iter());
            out.extend(group.ot.iter());
        }
        out.extend(self.extra_scripture.iter().map(|p| &p.pin));
        out
    }

    /// D97 ruling 6: a `translationWordsLinks` pin must name a repository whose name ends
    /// in `_twl`. A pin in the one-repository form (`<lang>_tw` in the links slot) was
    /// written by a pre-release tC4; no links are read from it. Returns the first such
    /// pin over the ladder.
    pub fn old_twl_pin(&self) -> Option<&ResourcePin> {
        Rung::LADDER.into_iter().find_map(|rung| {
            self.set(rung)?
                .pin(SetSlot::TranslationWordsLinks)
                .filter(|pin| !is_twl_repo(pin.repo.repo()))
        })
    }

    /// The value of one `Pins` record, as the JSON object of its slots.
    pub fn group_json(&self, group: PinGroup) -> Value {
        match group {
            PinGroup::Primary | PinGroup::Fallback => {
                let rung = if group == PinGroup::Primary {
                    Rung::Primary
                } else {
                    Rung::Fallback
                };
                self.set(rung)
                    .map_or_else(|| Value::Object(Map::new()), LanguageSet::to_json)
            }
            PinGroup::OriginalLanguage => self.original_language.to_json(),
            PinGroup::Lexicon => self.lexicon.to_json(),
            PinGroup::ExtraScripture => {
                let panes: Vec<Value> = self
                    .extra_scripture
                    .iter()
                    .map(|p| serde_json::to_value(p).unwrap_or(Value::Null))
                    .collect();
                serde_json::json!({ EXTRA_SCRIPTURE: panes })
            }
        }
    }

    /// Replaces one group from the JSON object of its slots. An empty object is an
    /// unpinned group. Nothing changes when the value is refused.
    pub fn set_group_json(&mut self, group: PinGroup, value: &Value) -> Result<(), ParseError> {
        let empty = value.as_object().is_some_and(Map::is_empty);
        match group {
            PinGroup::Primary | PinGroup::Fallback => {
                let set = if empty {
                    None
                } else {
                    Some(LanguageSet::from_json(value)?)
                };
                if group == PinGroup::Primary {
                    self.primary = set;
                } else {
                    self.fallback = set;
                }
            }
            PinGroup::OriginalLanguage => self.original_language = Testaments::from_json(value)?,
            PinGroup::Lexicon => self.lexicon = Testaments::from_json(value)?,
            PinGroup::ExtraScripture => {
                let panes = match value.get(EXTRA_SCRIPTURE) {
                    None if empty => Vec::new(),
                    None => {
                        return Err(ParseError::Pins(
                            "the extraScripture group has no extraScripture slot".to_owned(),
                        ));
                    }
                    Some(list) => Vec::<ScripturePane>::deserialize(list)
                        .map_err(|e| ParseError::Pins(format!("extraScripture: {e}")))?,
                };
                self.extra_scripture = panes;
            }
        }
        Ok(())
    }

    /// Reads tC4's `checking/resources.json` (§5.3, `schemaVersion` 2).
    pub fn from_resources_json(value: &Value) -> Result<Self, ParseError> {
        if value.get("schemaVersion").and_then(Value::as_u64) != Some(2) {
            return Err(ParseError::Pins(
                "resources.json is not schemaVersion 2".to_owned(),
            ));
        }
        let mut pins = ProjectPins::default();
        if let Some(sets) = value.get("languageSets") {
            for (rung, group) in [
                ("primary", PinGroup::Primary),
                ("fallback", PinGroup::Fallback),
            ] {
                if let Some(set) = sets.get(rung) {
                    pins.set_group_json(group, set)?;
                }
            }
        }
        if let Some(resources) = value.get("resources") {
            if let Some(ol) = resources.get("originalLanguage") {
                pins.set_group_json(PinGroup::OriginalLanguage, ol)?;
            }
            if let Some(lexicon) = resources.get("lexicon") {
                pins.set_group_json(PinGroup::Lexicon, lexicon)?;
            }
        }
        if let Some(list) = value.get(EXTRA_SCRIPTURE) {
            pins.set_group_json(
                PinGroup::ExtraScripture,
                &serde_json::json!({ EXTRA_SCRIPTURE: list }),
            )?;
        }
        Ok(pins)
    }

    /// Writes tC4's `checking/resources.json` shape (§5.3, `schemaVersion` 2).
    pub fn to_resources_json(&self) -> Value {
        let mut out = Map::new();
        out.insert("schemaVersion".to_owned(), Value::from(2));
        if self.has_language_sets() {
            let mut sets = Map::new();
            for rung in Rung::LADDER {
                if let Some(set) = self.set(rung) {
                    sets.insert(rung.to_string(), set.to_json());
                }
            }
            out.insert("languageSets".to_owned(), Value::Object(sets));
        }
        let mut resources = Map::new();
        for (name, group) in [
            ("originalLanguage", &self.original_language),
            ("lexicon", &self.lexicon),
        ] {
            if group.nt.is_some() || group.ot.is_some() {
                resources.insert(name.to_owned(), group.to_json());
            }
        }
        out.insert("resources".to_owned(), Value::Object(resources));
        if !self.extra_scripture.is_empty() {
            out.insert(
                EXTRA_SCRIPTURE.to_owned(),
                self.group_json(PinGroup::ExtraScripture)[EXTRA_SCRIPTURE].clone(),
            );
        }
        Value::Object(out)
    }
}

pub(crate) fn is_twl_repo(repo: &RepoRef) -> bool {
    repo.name.as_str().to_ascii_lowercase().ends_with("_twl")
}

/// The tools that derive a check list, and the slot each derives from. Articles never
/// drive resolution (tC4 `TOOL_SLOT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tool {
    TranslationNotes,
    TranslationWords,
}

impl Tool {
    pub fn slot(self) -> SetSlot {
        match self {
            Tool::TranslationNotes => SetSlot::TranslationNotes,
            Tool::TranslationWords => SetSlot::TranslationWordsLinks,
        }
    }

    /// The slot the tool reads for Open Bible Stories (tC4 `OBS_TOOL_SLOT`).
    pub fn obs_slot(self) -> SetSlot {
        match self {
            Tool::TranslationNotes => SetSlot::ObsNotes,
            Tool::TranslationWords => SetSlot::ObsWordsLinks,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn real_resources_json() -> Value {
        serde_json::json!({
            "schemaVersion": 2,
            "languageSets": {
                "primary": {
                    "gatewayLanguage": {"languageId": "es-419", "owner": "es-419_gl"},
                    "translationNotes": {"repoPath": "git.door43.org/es-419_gl/es-419_tn", "version": "v66", "sha": "22f3d0c61e2ab4701cb869547de9c3c43da07208", "flavor": "parascriptural/x-bcvnotes", "books": ["3JN", "JON", "RUT", "TIT"]},
                    "x-futureSlot": {"anything": true}
                },
                "fallback": {
                    "gatewayLanguage": {"languageId": "en", "owner": "unfoldingWord"},
                    "translationNotes": {"repoPath": "git.door43.org/unfoldingWord/en_tn", "version": "v91", "sha": "e586762e330f482a60c52aedd1c7b3a2f155df8a", "flavor": "parascriptural/x-bcvnotes"}
                }
            },
            "resources": {
                "originalLanguage": {"nt": {"repoPath": "git.door43.org/unfoldingWord/el-x-koine_ugnt", "version": "v0.34", "sha": "fc95b2b8aad08bb65ab54628ab685413a1139e97", "flavor": "scripture/textTranslation"}},
                "lexicon": {"ot": {"repoPath": "git.door43.org/uW/en_uhl", "sha": "72df5ac25acf9d51e826b20e3ad883a5a657ef4e", "flavor": "peripheral/x-lexicon"}}
            },
            "extraScripture": [{"id": "ult", "repoPath": "git.door43.org/unfoldingWord/en_ult", "version": "v91", "sha": "35d215957f3203fd2e2fac5702ce14902d417f9d", "flavor": "scripture/textTranslation"}]
        })
    }

    #[test]
    fn resources_json_round_trips_and_keeps_unknown_slots() {
        let json = real_resources_json();
        let pins = ProjectPins::from_resources_json(&json).unwrap();
        let primary = pins.primary.as_ref().unwrap();
        assert_eq!(primary.gateway.language, "es-419");
        assert!(primary.extra.contains_key("x-futureSlot"));
        assert_eq!(pins.extra_scripture[0].id.as_str(), "ult");
        assert!(pins.lexicon.ot.is_some() && pins.lexicon.nt.is_none());
        assert_eq!(pins.to_resources_json(), json);
    }

    #[test]
    fn each_pin_group_round_trips_through_its_record_value() {
        let pins = ProjectPins::from_resources_json(&real_resources_json()).unwrap();
        for group in PinGroup::ALL {
            let value = pins.group_json(group);
            let mut back = ProjectPins::default();
            back.set_group_json(group, &value).unwrap();
            assert_eq!(back.group_json(group), value, "{group:?}");
        }
        let mut cleared = pins.clone();
        cleared
            .set_group_json(PinGroup::Primary, &serde_json::json!({}))
            .unwrap();
        assert!(cleared.primary.is_none());
    }

    #[test]
    fn a_malformed_pin_is_refused_and_changes_nothing() {
        let mut pins = ProjectPins::from_resources_json(&real_resources_json()).unwrap();
        let before = pins.clone();
        let bad = serde_json::json!({
            "gatewayLanguage": {"languageId": "en", "owner": "unfoldingWord"},
            "translationNotes": {"repoPath": "git.door43.org/unfoldingWord/en_tn", "sha": "not-a-sha"}
        });
        assert!(pins.set_group_json(PinGroup::Primary, &bad).is_err());
        assert_eq!(pins, before);
    }

    #[test]
    fn the_one_repository_twl_form_is_found() {
        let mut pins = ProjectPins::from_resources_json(&real_resources_json()).unwrap();
        assert!(pins.old_twl_pin().is_none());
        let tw: ResourcePin = serde_json::from_value(serde_json::json!({
            "repoPath": "git.door43.org/unfoldingWord/en_tw",
            "sha": "ff5b3852c27c3a0d01b109e482eb26047dcd20e2"
        }))
        .unwrap();
        pins.fallback
            .as_mut()
            .unwrap()
            .pins
            .insert(SetSlot::TranslationWordsLinks, tw.clone());
        assert_eq!(pins.old_twl_pin(), Some(&tw));
    }
}
