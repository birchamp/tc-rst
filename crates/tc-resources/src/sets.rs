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
//! What this version does not know (a newer app's) is kept and written back unchanged: a
//! slot in [`LanguageSet::extra`], a `gatewayLanguage` field in
//! [`LanguageSet::gateway_extra`], a slot beside `nt` and `ot` in [`Testaments::extra`], a
//! pin's or pane's field in [`ResourcePin::extra`], and a key of `resources.json` beside
//! the five groups in [`ProjectPins::unknown`]. Writing
//! refuses a pin that tC4's §5.3 grammar refuses: one without a flavor
//! ([`UnwritablePin`]).
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

use crate::error::{ParseError, UnwritablePin};
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
    /// Fields of the `gatewayLanguage` object beside `languageId` and `owner`, kept as
    /// they were read and written back. They stay out of [`GatewayLanguage`], which
    /// identifies a gateway and is compared and hashed as one.
    pub gateway_extra: Map<String, Value>,
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
            gateway_extra: Map::new(),
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
    pub fn to_json(&self) -> Result<Value, UnwritablePin> {
        let mut gateway = Map::new();
        gateway.insert(
            LANGUAGE_ID.to_owned(),
            Value::from(self.gateway.language.as_str()),
        );
        gateway.insert(OWNER.to_owned(), Value::from(self.gateway.owner.as_str()));
        for (key, value) in &self.gateway_extra {
            gateway.entry(key.clone()).or_insert_with(|| value.clone());
        }
        let mut out = Map::new();
        out.insert(GATEWAY_LANGUAGE.to_owned(), Value::Object(gateway));
        for slot in SetSlot::ALL {
            if let Some(pin) = self.pins.get(&slot) {
                out.insert(slot.name().to_owned(), pin_json(slot.name(), pin)?);
            }
        }
        for (key, value) in &self.extra {
            out.entry(key.clone()).or_insert_with(|| value.clone());
        }
        Ok(Value::Object(out))
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
        let gateway = gateway
            .as_object()
            .ok_or_else(|| ParseError::Pins("gatewayLanguage is not an object".to_owned()))?;
        let text = |key: &str| {
            gateway
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| ParseError::Pins(format!("gatewayLanguage has no {key}")))
        };
        let language = text(LANGUAGE_ID)?.to_owned();
        let owner = text(OWNER)?
            .parse()
            .map_err(|_| ParseError::Pins(format!("gatewayLanguage owner {:?}", text(OWNER))))?;
        let mut set = LanguageSet::new(GatewayLanguage { language, owner });
        set.gateway_extra = gateway
            .iter()
            .filter(|(key, _)| !matches!(key.as_str(), LANGUAGE_ID | OWNER))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
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
const LANGUAGE_ID: &str = "languageId";
const OWNER: &str = "owner";
const EXTRA_SCRIPTURE: &str = "extraScripture";
const SCHEMA_VERSION: &str = "schemaVersion";
const LANGUAGE_SETS: &str = "languageSets";
const RESOURCES: &str = "resources";
const ORIGINAL_LANGUAGE: &str = "originalLanguage";
const LEXICON: &str = "lexicon";

fn pin_json(slot: &str, pin: &ResourcePin) -> Result<Value, UnwritablePin> {
    writable(slot, pin)?;
    // A pin is plain strings, lists and JSON values, which always serialize.
    Ok(serde_json::to_value(pin).unwrap_or(Value::Null))
}

/// tC4's §5.3 grammar (`pinEntryError`) requires a flavor; the other fields it checks
/// are valid by their types.
fn writable(slot: &str, pin: &ResourcePin) -> Result<(), UnwritablePin> {
    match pin.flavor.as_deref() {
        Some(flavor) if !flavor.is_empty() => Ok(()),
        _ => Err(UnwritablePin {
            slot: slot.to_owned(),
            pin: pin.to_string(),
        }),
    }
}

fn read_pin(slot: &str, value: &Value) -> Result<ResourcePin, ParseError> {
    ResourcePin::deserialize(value).map_err(|e| ParseError::Pins(format!("slot {slot}: {e}")))
}

/// A pin per testament: the original-language texts and the lexicons.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Testaments {
    pub nt: Option<ResourcePin>,
    pub ot: Option<ResourcePin>,
    /// Slots beside `nt` and `ot` this version does not know, kept as they were read and
    /// written back.
    pub extra: Map<String, Value>,
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

    /// Whether the group has anything to write: a pin or a slot a newer app added.
    fn is_empty(&self) -> bool {
        self.nt.is_none() && self.ot.is_none() && self.extra.is_empty()
    }

    fn to_json(&self) -> Result<Value, UnwritablePin> {
        let mut out = Map::new();
        if let Some(pin) = &self.nt {
            out.insert("nt".to_owned(), pin_json("nt", pin)?);
        }
        if let Some(pin) = &self.ot {
            out.insert("ot".to_owned(), pin_json("ot", pin)?);
        }
        for (key, value) in &self.extra {
            out.entry(key.clone()).or_insert_with(|| value.clone());
        }
        Ok(Value::Object(out))
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
                _ => {
                    out.extra.insert(key.clone(), value.clone());
                }
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

/// One source pane of `extraScripture`: a gateway Bible shown beside the translation. A
/// field this version does not know lands in the pin's [`ResourcePin::extra`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScripturePane {
    pub id: PaneId,
    #[serde(flatten)]
    pub pin: ResourcePin,
}

/// Keys of tC4's `resources.json` this version does not know, kept as read and written
/// back by [`ProjectPins::to_resources_json`]. No `Pins` record holds them (STORAGE
/// §2.2): they live as long as the value read from the file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnknownKeys {
    /// Beside `schemaVersion`, `languageSets`, `resources` and `extraScripture`.
    pub top: Map<String, Value>,
    /// Under `languageSets`, beside `primary` and `fallback`.
    pub language_sets: Map<String, Value>,
    /// Under `resources`, beside `originalLanguage` and `lexicon`.
    pub resources: Map<String, Value>,
}

/// A project's pins: the five `PinGroup` values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectPins {
    pub primary: Option<LanguageSet>,
    pub fallback: Option<LanguageSet>,
    pub original_language: Testaments,
    pub lexicon: Testaments,
    pub extra_scripture: Vec<ScripturePane>,
    pub unknown: UnknownKeys,
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

    /// The value of one `Pins` record, as the JSON object of its slots. Refused when a pin
    /// of the group has no flavor.
    pub fn group_json(&self, group: PinGroup) -> Result<Value, UnwritablePin> {
        Ok(match group {
            PinGroup::Primary | PinGroup::Fallback => {
                let rung = if group == PinGroup::Primary {
                    Rung::Primary
                } else {
                    Rung::Fallback
                };
                match self.set(rung) {
                    Some(set) => set.to_json()?,
                    None => Value::Object(Map::new()),
                }
            }
            PinGroup::OriginalLanguage => self.original_language.to_json()?,
            PinGroup::Lexicon => self.lexicon.to_json()?,
            PinGroup::ExtraScripture => {
                let mut panes = Vec::new();
                for pane in &self.extra_scripture {
                    writable(&format!("{EXTRA_SCRIPTURE}.{}", pane.id), &pane.pin)?;
                    // A pane is plain strings, lists and JSON values, which always serialize.
                    panes.push(serde_json::to_value(pane).unwrap_or(Value::Null));
                }
                serde_json::json!({ EXTRA_SCRIPTURE: panes })
            }
        })
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
        let unknown = |object: Option<&Value>, known: &[&str]| -> Map<String, Value> {
            object
                .and_then(Value::as_object)
                .map(|o| {
                    o.iter()
                        .filter(|(k, _)| !known.contains(&k.as_str()))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect()
                })
                .unwrap_or_default()
        };
        let sets = value.get(LANGUAGE_SETS);
        if let Some(sets) = sets {
            for (rung, group) in [
                ("primary", PinGroup::Primary),
                ("fallback", PinGroup::Fallback),
            ] {
                if let Some(set) = sets.get(rung) {
                    pins.set_group_json(group, set)?;
                }
            }
        }
        let resources = value.get(RESOURCES);
        if let Some(resources) = resources {
            if let Some(ol) = resources.get(ORIGINAL_LANGUAGE) {
                pins.set_group_json(PinGroup::OriginalLanguage, ol)?;
            }
            if let Some(lexicon) = resources.get(LEXICON) {
                pins.set_group_json(PinGroup::Lexicon, lexicon)?;
            }
        }
        if let Some(list) = value.get(EXTRA_SCRIPTURE) {
            pins.set_group_json(
                PinGroup::ExtraScripture,
                &serde_json::json!({ EXTRA_SCRIPTURE: list }),
            )?;
        }
        pins.unknown = UnknownKeys {
            top: unknown(
                Some(value),
                &[SCHEMA_VERSION, LANGUAGE_SETS, RESOURCES, EXTRA_SCRIPTURE],
            ),
            language_sets: unknown(sets, &["primary", "fallback"]),
            resources: unknown(resources, &[ORIGINAL_LANGUAGE, LEXICON]),
        };
        Ok(pins)
    }

    /// Writes tC4's `checking/resources.json` shape (§5.3, `schemaVersion` 2), with the
    /// keys [`ProjectPins::unknown`] kept. Refused when a pin has no flavor.
    pub fn to_resources_json(&self) -> Result<Value, UnwritablePin> {
        let mut out = self.unknown.top.clone();
        out.insert(SCHEMA_VERSION.to_owned(), Value::from(2));
        if self.has_language_sets() || !self.unknown.language_sets.is_empty() {
            let mut sets = self.unknown.language_sets.clone();
            for rung in Rung::LADDER {
                if let Some(set) = self.set(rung) {
                    sets.insert(rung.to_string(), set.to_json()?);
                }
            }
            out.insert(LANGUAGE_SETS.to_owned(), Value::Object(sets));
        }
        let mut resources = self.unknown.resources.clone();
        for (name, group) in [
            (ORIGINAL_LANGUAGE, &self.original_language),
            (LEXICON, &self.lexicon),
        ] {
            if !group.is_empty() {
                resources.insert(name.to_owned(), group.to_json()?);
            }
        }
        out.insert(RESOURCES.to_owned(), Value::Object(resources));
        if !self.extra_scripture.is_empty() {
            out.insert(
                EXTRA_SCRIPTURE.to_owned(),
                self.group_json(PinGroup::ExtraScripture)?[EXTRA_SCRIPTURE].clone(),
            );
        }
        Ok(Value::Object(out))
    }
}

pub(crate) fn is_twl_repo(repo: &RepoRef) -> bool {
    repo.name.as_str().to_ascii_lowercase().ends_with("_twl")
}

/// A translation words articles repository: `en_tw`, `<lang>_tw`.
pub(crate) fn is_tw_repo(repo: &RepoRef) -> bool {
    repo.name.as_str().to_ascii_lowercase().ends_with("_tw")
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
        assert_eq!(pins.to_resources_json().unwrap(), json);
    }

    #[test]
    fn a_newer_apps_fields_are_kept_at_every_level() {
        let mut json = real_resources_json();
        json["x-top"] = serde_json::json!({"a": 1});
        json["languageSets"]["x-rung"] = serde_json::json!([1]);
        json["resources"]["x-resource"] = serde_json::json!("kept");
        json["languageSets"]["primary"]["translationNotes"]["x-future"] =
            serde_json::json!({"b": [2]});
        json["extraScripture"][0]["x-pane"] = serde_json::json!(1);
        json["languageSets"]["primary"]["gatewayLanguage"]["x-gl"] = serde_json::json!("kept");
        json["resources"]["originalLanguage"]["x-testament"] = serde_json::json!({"c": 3});
        // A group holding only a newer app's slot is still written.
        json["resources"]["lexicon"] = serde_json::json!({"x-lexicon": true});
        let pins = ProjectPins::from_resources_json(&json).unwrap();
        assert_eq!(pins.to_resources_json().unwrap(), json);
        // Each field also survives a rewrite of its group's record.
        for group in PinGroup::ALL {
            let value = pins.group_json(group).unwrap();
            let mut back = ProjectPins::default();
            back.set_group_json(group, &value).unwrap();
            assert_eq!(back.group_json(group).unwrap(), value, "{group:?}");
        }
        let primary = pins.group_json(PinGroup::Primary).unwrap();
        assert_eq!(primary["translationNotes"]["x-future"]["b"][0], 2);
        assert_eq!(primary["gatewayLanguage"]["x-gl"], "kept");
        assert_eq!(primary["gatewayLanguage"]["languageId"], "es-419");
        let panes = pins.group_json(PinGroup::ExtraScripture).unwrap();
        assert_eq!(panes["extraScripture"][0]["x-pane"], 1);
        let ol = pins.group_json(PinGroup::OriginalLanguage).unwrap();
        assert_eq!(ol["x-testament"]["c"], 3);
        assert!(ol["nt"]["repoPath"].is_string());
        let lexicon = pins.group_json(PinGroup::Lexicon).unwrap();
        assert_eq!(lexicon, serde_json::json!({"x-lexicon": true}));
    }

    #[test]
    fn a_newer_apps_field_never_overrides_what_this_version_knows() {
        let mut pins = ProjectPins::from_resources_json(&real_resources_json()).unwrap();
        let primary = pins.primary.as_mut().unwrap();
        primary
            .gateway_extra
            .insert("owner".to_owned(), serde_json::json!("someone-else"));
        primary
            .extra
            .insert("translationNotes".to_owned(), serde_json::json!(0));
        pins.original_language
            .extra
            .insert("nt".to_owned(), serde_json::json!(0));
        let written = pins.to_resources_json().unwrap();
        assert_eq!(written, real_resources_json());
    }

    #[test]
    fn an_empty_books_list_is_no_record_and_the_pins_still_read() {
        let mut json = real_resources_json();
        json["languageSets"]["primary"]["translationNotes"]["books"] = serde_json::json!([]);
        let pins = ProjectPins::from_resources_json(&json).unwrap();
        let tn = pins
            .primary
            .as_ref()
            .unwrap()
            .pin(SetSlot::TranslationNotes)
            .unwrap();
        assert_eq!(tn.books, None);
    }

    /// tC4 `journal/grammar.mjs` `pinEntryError`, for a pin slot (not `gatewayLanguage`):
    /// `None` when tC4 accepts the entry.
    fn tc4_pin_entry_error(slot: &str, entry: &Value) -> Option<String> {
        let entry = entry.as_object()?;
        let is_extra = slot.starts_with("extraScripture.");
        let mut fields = vec![
            ("repoPath", true),
            ("sha", true),
            ("version", false),
            ("flavor", true),
        ];
        if is_extra {
            fields.push(("id", true));
        }
        for (key, required) in fields {
            match entry.get(key) {
                None if required => return Some(format!("without {key}")),
                None => {}
                Some(Value::String(s)) if !s.is_empty() => {}
                Some(_) => return Some(format!("{key} is not a non-empty string")),
            }
        }
        let sha = entry["sha"].as_str().unwrap_or_default();
        if sha.len() != 40 || !sha.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Some("sha is not 40 lowercase hex".to_owned());
        }
        if let Some(books) = entry.get("books") {
            let books = books.as_array()?;
            let whole = books.len() == 1 && books[0] == "BIBLE";
            let book_code = |b: &Value| {
                b.as_str().is_some_and(|s| {
                    s.len() == 3
                        && s.bytes()
                            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                })
            };
            if !whole && !books.iter().all(book_code) {
                return Some("books".to_owned());
            }
        }
        if is_extra && entry["id"].as_str() != slot.strip_prefix("extraScripture.") {
            return Some("id".to_owned());
        }
        None
    }

    fn written_pins(pins: &ProjectPins) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        for group in PinGroup::ALL {
            let value = pins.group_json(group).unwrap();
            match group {
                PinGroup::ExtraScripture => {
                    for pane in value["extraScripture"].as_array().unwrap() {
                        let id = pane["id"].as_str().unwrap();
                        out.push((format!("extraScripture.{id}"), pane.clone()));
                    }
                }
                _ => {
                    for (slot, entry) in value.as_object().unwrap() {
                        if slot != GATEWAY_LANGUAGE && !slot.starts_with("x-") {
                            out.push((slot.clone(), entry.clone()));
                        }
                    }
                }
            }
        }
        out
    }

    #[test]
    fn every_written_pin_is_one_tc4_accepts() {
        let mut real = ProjectPins::from_resources_json(&real_resources_json()).unwrap();
        real.fallback
            .as_mut()
            .unwrap()
            .pins
            .get_mut(&SetSlot::TranslationNotes)
            .unwrap()
            .books = Some(crate::pin::BookCoverage::WholeCollection);
        for pins in [real, crate::suite::project_pins()] {
            let written = written_pins(&pins);
            assert!(written.len() > 3);
            for (slot, entry) in written {
                assert_eq!(tc4_pin_entry_error(&slot, &entry), None, "{slot}: {entry}");
            }
        }
    }

    #[test]
    fn a_pin_without_a_flavor_is_never_written() {
        let mut pins = crate::suite::project_pins();
        let mut tn = pins
            .primary
            .as_ref()
            .unwrap()
            .pin(SetSlot::TranslationNotes)
            .unwrap()
            .clone();
        tn.flavor = None;
        assert!(
            tc4_pin_entry_error("translationNotes", &serde_json::to_value(&tn).unwrap()).is_some()
        );
        pins.primary
            .as_mut()
            .unwrap()
            .pins
            .insert(SetSlot::TranslationNotes, tn);
        let refused = pins.group_json(PinGroup::Primary).unwrap_err();
        assert_eq!(refused.slot, "translationNotes");
        assert!(pins.to_resources_json().is_err());
        assert!(pins.group_json(PinGroup::Fallback).is_ok());
        pins.extra_scripture[0].pin.flavor = None;
        assert!(pins.group_json(PinGroup::ExtraScripture).is_err());
    }

    #[test]
    fn each_pin_group_round_trips_through_its_record_value() {
        let pins = ProjectPins::from_resources_json(&real_resources_json()).unwrap();
        for group in PinGroup::ALL {
            let value = pins.group_json(group).unwrap();
            let mut back = ProjectPins::default();
            back.set_group_json(group, &value).unwrap();
            assert_eq!(back.group_json(group).unwrap(), value, "{group:?}");
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
