//! The pins of the ladder and tC4's D30 resolution (`docs/STORAGE.md` §7.7): which pinned
//! resource a (tool, book) or a (tool, the stories) checks against.
//!
//! This is the one model of a pin slot and of the ladder. The fold resolves with it from the
//! projected `Primary` and `Fallback` groups and nothing else ([`Unrecorded::CoversEveryBook`]);
//! a screen that opens a tool on this computer resolves with it too, reading a pin that
//! records no books with the books of its installed copy ([`Unrecorded::Installed`]), as
//! tC4's `coverageFor` does (`src/data/resolve.ts` at d310bf3, lines 84-113). The ladder is
//! the provided methods of [`Ladder`]: the journal's [`LanguageSets`] and the resource
//! store's typed pins implement [`Ladder::held`] and share everything else.
//!
//! The rules, all tC4's (`resolve.ts`):
//!
//! - A rung's slot answers a book when its pin covers the book; the first rung that answers
//!   is the resolution, and the next rung is tried only when the slot is empty (`null` or
//!   absent) or does not cover the book (`resolveSetSlot`, lines 180-199).
//! - Coverage is the pin's own `books` record when it is a non-empty list of strings: each
//!   code is read in upper case, and `BIBLE` anywhere in it covers every book. An empty list,
//!   a list holding anything but strings, or no `books` at all records nothing
//!   (`coverageFor`). A code that names no book covers nothing.
//! - A slot whose value is not a usable pin (no `repoPath`, or no 40-hex `sha`) still
//!   answers the books it records, so the ladder stops there and no decision matches it
//!   (`recordMatchesResolution` needs equal shas). Having no commit, it has no installed copy
//!   to read coverage from, so if it records no books it covers none and the next rung is
//!   tried.
//! - The stories of Open Bible Stories are not divided into books: the first rung whose
//!   slot holds anything answers, usable or not (`resolveObsSetSlot`, lines 166-178).
//! - A decision's resource matches a resolution when the repository paths are equal
//!   ignoring ASCII case and the shas are equal (`samePath`, `recordMatchesResolution`).

use crate::ids::GitOid;
use crate::json::{Json, JsonMap};
use crate::keys::{SlotName, ToolId};
use crate::record::{PinGroupValue, ResourceRef, SlotValue};
use std::collections::BTreeSet;
use std::fmt;
use tc_core::BookId;

/// The two rungs of the ladder, in the order they are tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rung {
    Primary,
    Fallback,
}

impl Rung {
    pub const LADDER: [Rung; 2] = [Rung::Primary, Rung::Fallback];

    /// tC4's `languageSet` name: `primary`, `fallback`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Fallback => "fallback",
        }
    }
}

impl From<Rung> for crate::keys::PinGroup {
    fn from(rung: Rung) -> Self {
        match rung {
            Rung::Primary => Self::Primary,
            Rung::Fallback => Self::Fallback,
        }
    }
}

impl fmt::Display for Rung {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// What a check list and a decision file cover, and so a resolution unit with a tool: one
/// book, or the fifty stories of Open Bible Stories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Collection {
    Book(BookId),
    Stories,
}

impl fmt::Display for Collection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Book(book) => f.write_str(book.code()),
            Self::Stories => f.write_str("OBS"),
        }
    }
}

/// A slot of a language set, by tC4's slot name.
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
    /// Every slot, in tC4's order for `resources.json`.
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

    /// The help slots a release upgrade or a re-pin moves, in tC4's display order
    /// (`SET_SLOTS` of `upgrade.ts`).
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
            Self::TranslationNotes => "translationNotes",
            Self::TranslationWordsLinks => "translationWordsLinks",
            Self::TranslationWords => "translationWords",
            Self::TranslationAcademy => "translationAcademy",
            Self::TranslationQuestions => "translationQuestions",
            Self::SimplifiedText => "simplifiedText",
            Self::Obs => "obs",
            Self::ObsNotes => "obs-tn",
            Self::ObsWordsLinks => "obs-twl",
            Self::ObsQuestions => "obs-tq",
            Self::ObsImages => "obs-images",
        }
    }

    /// The slot named `name`, if it is one.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|slot| slot.name() == name)
    }

    /// The Bible-book slots, whose coverage a resolution reads (tC4 `coverageBackfill`
    /// `SET_SLOTS`). The stories' slots cover the whole collection.
    pub fn is_per_book(self) -> bool {
        matches!(
            self,
            Self::TranslationNotes
                | Self::TranslationWordsLinks
                | Self::TranslationWords
                | Self::TranslationAcademy
                | Self::TranslationQuestions
                | Self::SimplifiedText
        )
    }

    /// The slot a tool's check list (or, for questions, its help list) of `collection`
    /// comes from (tC4 `TOOL_SLOT`, `OBS_HELP_SLOT`): `translationWordsLinks` for
    /// translationWords, and the `obs-` slots for the stories.
    pub fn of_tool(tool: ToolId, collection: Collection) -> Self {
        match (collection, tool) {
            (Collection::Book(_), ToolId::TranslationWords) => Self::TranslationWordsLinks,
            (Collection::Book(_), ToolId::TranslationNotes) => Self::TranslationNotes,
            (Collection::Book(_), ToolId::TranslationQuestions) => Self::TranslationQuestions,
            (Collection::Stories, ToolId::TranslationWords) => Self::ObsWordsLinks,
            (Collection::Stories, ToolId::TranslationNotes) => Self::ObsNotes,
            (Collection::Stories, ToolId::TranslationQuestions) => Self::ObsQuestions,
        }
    }
}

impl fmt::Display for SetSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The books a pin records (tC4 `books`, D41).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Coverage {
    /// No usable record: no `books`, an empty list, or a list holding something other than
    /// strings.
    Unrecorded,
    /// `BIBLE` is in the list: a resource not divided into books covers every book.
    Whole,
    Books(BTreeSet<BookId>),
}

/// tC4's whole-collection marker.
pub const WHOLE_COLLECTION: &str = "BIBLE";

impl Coverage {
    /// Reads a list of codes as tC4's `coverageFor` reads a pin's `books`.
    pub fn from_codes<'a>(codes: impl IntoIterator<Item = &'a str>) -> Self {
        let codes: Vec<String> = codes.into_iter().map(str::to_ascii_uppercase).collect();
        if codes.is_empty() {
            return Self::Unrecorded;
        }
        if codes.iter().any(|c| c == WHOLE_COLLECTION) {
            return Self::Whole;
        }
        Self::Books(codes.iter().filter_map(|c| c.parse().ok()).collect())
    }

    /// The `books` record of a slot value.
    pub fn of_slot(slot: &Json) -> Self {
        let codes: Option<Vec<&str>> =
            slot.as_obj()
                .and_then(|m| m.get("books"))
                .and_then(|b| match b {
                    Json::Arr(items) => items.iter().map(Json::as_str).collect(),
                    _ => None,
                });
        codes.map_or(Self::Unrecorded, Self::from_codes)
    }

    pub fn is_recorded(&self) -> bool {
        !matches!(self, Self::Unrecorded)
    }

    /// Whether the record covers `book`. Recording nothing covers nothing: what an
    /// unrecorded pin covers is the caller's rule ([`Unrecorded`]).
    pub fn covers(&self, book: BookId) -> bool {
        match self {
            Self::Unrecorded => false,
            Self::Whole => true,
            Self::Books(books) => books.contains(&book),
        }
    }
}

/// A usable pin: a repository at a commit, with its label and the books it records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub resource: ResourceRef,
    /// The release tag, a label for people (D58).
    pub version: Option<String>,
    pub coverage: Coverage,
}

impl Pin {
    /// The slot value tC4 writes for this pin: `repoPath`, `version` when it has one, `sha`
    /// and `books` when it records them.
    pub fn to_json(&self) -> Json {
        let mut map = JsonMap::new();
        map.insert("repoPath".into(), Json::from(&*self.resource.repo_path));
        if let Some(version) = &self.version {
            map.insert("version".into(), Json::from(version.as_str()));
        }
        map.insert("sha".into(), Json::Str(self.resource.sha.to_hex()));
        let codes: Option<Vec<Json>> = match &self.coverage {
            Coverage::Unrecorded => None,
            Coverage::Whole => Some(vec![Json::from(WHOLE_COLLECTION)]),
            Coverage::Books(books) => Some(books.iter().map(|b| Json::from(b.code())).collect()),
        };
        if let Some(codes) = codes {
            map.insert("books".into(), Json::Arr(codes));
        }
        Json::Obj(map)
    }

    /// The books this pin covers on this computer (tC4 `coverageFor`): its own record, else
    /// the books its installed copy holds, else none.
    pub fn coverage_with(&self, installed: &dyn Fn(&ResourceRef) -> Option<Coverage>) -> Coverage {
        if self.coverage.is_recorded() {
            return self.coverage.clone();
        }
        installed(&self.resource)
            .filter(Coverage::is_recorded)
            .unwrap_or(Coverage::Unrecorded)
    }
}

/// Why a slot's value is not a usable pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UnusableReason {
    #[error("the slot is not a pin object")]
    NotAnObject,
    #[error("the pin names no repository")]
    NoRepoPath,
    #[error("the pin names no commit: only a 40-digit lowercase sha identifies a pin (D58, D59)")]
    NoSha,
}

/// A slot value that is not a usable pin, such as a pin with a tag and no commit. No install
/// or file can satisfy it, and no decision matches it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnusablePin {
    /// The books it records.
    pub coverage: Coverage,
    /// Its repository path, when it names one.
    pub repo_path: Option<String>,
    pub version: Option<String>,
    pub reason: UnusableReason,
}

/// What a language set holds in one slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotPin {
    Usable(Pin),
    Unusable(UnusablePin),
}

impl SlotPin {
    /// Reads a slot value; `None` for `null`, which is no pin (tC4 reads it as absent).
    pub fn read(slot: &Json) -> Option<Self> {
        if matches!(slot, Json::Null) {
            return None;
        }
        let coverage = Coverage::of_slot(slot);
        let Some(map) = slot.as_obj() else {
            return Some(Self::Unusable(UnusablePin {
                coverage,
                repo_path: None,
                version: None,
                reason: UnusableReason::NotAnObject,
            }));
        };
        let text = |key: &str| map.get(key).and_then(Json::as_str);
        let repo_path = text("repoPath").filter(|p| !p.is_empty());
        let version = text("version").map(str::to_owned);
        let sha = text("sha").and_then(GitOid::parse);
        Some(match (repo_path, sha) {
            (Some(repo_path), Some(sha)) => Self::Usable(Pin {
                resource: ResourceRef {
                    repo_path: repo_path.into(),
                    sha,
                },
                version,
                coverage,
            }),
            (repo_path, _) => Self::Unusable(UnusablePin {
                coverage,
                repo_path: repo_path.map(str::to_owned),
                version,
                reason: if repo_path.is_none() {
                    UnusableReason::NoRepoPath
                } else {
                    UnusableReason::NoSha
                },
            }),
        })
    }

    /// The books this slot records.
    pub fn coverage(&self) -> &Coverage {
        match self {
            Self::Usable(pin) => &pin.coverage,
            Self::Unusable(pin) => &pin.coverage,
        }
    }

    pub fn usable(&self) -> Option<&Pin> {
        match self {
            Self::Usable(pin) => Some(pin),
            Self::Unusable(_) => None,
        }
    }
}

/// What a usable pin that records no books covers.
#[derive(Clone, Copy)]
pub enum Unrecorded<'a> {
    /// Every book. The fold's rule (§7.7): its result is a function of the pins alone, never
    /// of what one computer has installed (I12), and the coverage backfill (§5.6) records
    /// the books once the resource is installed.
    CoversEveryBook,
    /// The books its installed copy holds, else none: tC4's `coverageFor`, for opening a tool
    /// on this computer.
    Installed(&'a dyn Fn(&ResourceRef) -> Option<Coverage>),
}

impl fmt::Debug for Unrecorded<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CoversEveryBook => f.write_str("CoversEveryBook"),
            Self::Installed(_) => f.write_str("Installed"),
        }
    }
}

/// Where the ladder stops for a (tool, collection).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// A usable pin.
    Pin { rung: Rung, pin: Pin },
    /// A value that is not a usable pin. The next rung is not used in its place: that would
    /// check the book against another language's list without saying so.
    Unusable { rung: Rung, pin: UnusablePin },
}

impl Answer {
    pub fn rung(&self) -> Rung {
        match self {
            Self::Pin { rung, .. } | Self::Unusable { rung, .. } => *rung,
        }
    }

    /// The resource a decision must record to be valid: `None` for an unusable pin, which no
    /// decision matches.
    pub fn resource(&self) -> Option<&ResourceRef> {
        match self {
            Self::Pin { pin, .. } => Some(&pin.resource),
            Self::Unusable { .. } => None,
        }
    }

    /// The label the pin carries.
    pub fn version(&self) -> Option<&str> {
        match self {
            Self::Pin { pin, .. } => pin.version.as_deref(),
            Self::Unusable { pin, .. } => pin.version.as_deref(),
        }
    }
}

/// A project's two language sets as the ladder reads them. Implementors say what a rung
/// holds in a slot; the ladder itself is the provided methods, so every reader of pins
/// resolves the same way.
pub trait Ladder {
    /// What `rung` holds in `slot`; `None` when the slot is absent or `null`.
    fn held(&self, rung: Rung, slot: SetSlot) -> Option<SlotPin>;

    /// The usable pin `rung` holds in `slot`.
    fn pin(&self, rung: Rung, slot: SetSlot) -> Option<Pin> {
        match self.held(rung, slot)? {
            SlotPin::Usable(pin) => Some(pin),
            SlotPin::Unusable(_) => None,
        }
    }

    /// The first rung that pins exactly `resource` in `slot`: a pin both rungs carry is
    /// pinned by the primary (tC4 `rungPinning`, D97).
    fn rung_pinning(&self, slot: SetSlot, resource: &ResourceRef) -> Option<Rung> {
        Rung::LADDER.into_iter().find(|rung| {
            self.pin(*rung, slot)
                .is_some_and(|p| p.resource.same_resource(resource))
        })
    }

    /// Where the ladder stops for `slot` and `collection` (tC4 `resolveSetSlot`,
    /// `resolveObsSetSlot`). See the module documentation for the rules.
    fn answer_slot(
        &self,
        slot: SetSlot,
        collection: Collection,
        unrecorded: Unrecorded<'_>,
    ) -> Option<Answer> {
        Rung::LADDER.into_iter().find_map(|rung| {
            let held = self.held(rung, slot)?;
            let covers = match collection {
                Collection::Stories => true,
                Collection::Book(book) => match &held {
                    SlotPin::Usable(pin) if !pin.coverage.is_recorded() => match unrecorded {
                        Unrecorded::CoversEveryBook => true,
                        Unrecorded::Installed(local) => pin.coverage_with(local).covers(book),
                    },
                    held => held.coverage().covers(book),
                },
            };
            covers.then_some(match held {
                SlotPin::Usable(pin) => Answer::Pin { rung, pin },
                SlotPin::Unusable(pin) => Answer::Unusable { rung, pin },
            })
        })
    }

    /// Where the ladder stops for a tool's check list of `collection`.
    fn answer(
        &self,
        tool: ToolId,
        collection: Collection,
        unrecorded: Unrecorded<'_>,
    ) -> Option<Answer> {
        self.answer_slot(SetSlot::of_tool(tool, collection), collection, unrecorded)
    }

    /// The fold's resolution of a (tool, collection) (§7.7): the ladder over the pins alone.
    fn resolution(&self, tool: ToolId, collection: Collection) -> Option<Answer> {
        self.answer(tool, collection, Unrecorded::CoversEveryBook)
    }
}

/// The project's language sets: the projected `Primary` and `Fallback` pin groups.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LanguageSets {
    pub primary: Option<PinGroupValue>,
    pub fallback: Option<PinGroupValue>,
}

impl LanguageSets {
    pub fn set(&self, rung: Rung) -> Option<&PinGroupValue> {
        match rung {
            Rung::Primary => self.primary.as_ref(),
            Rung::Fallback => self.fallback.as_ref(),
        }
    }

    /// Whether the project pins any language set (tC4: `languageSets` present).
    pub fn is_empty(&self) -> bool {
        self.primary.is_none() && self.fallback.is_none()
    }
}

impl Ladder for LanguageSets {
    fn held(&self, rung: Rung, slot: SetSlot) -> Option<SlotPin> {
        self.set(rung)?.held(slot)
    }
}

impl PinGroupValue {
    /// What this language set holds in `slot`; `None` when the slot is absent or `null`.
    pub fn held(&self, slot: SetSlot) -> Option<SlotPin> {
        SlotPin::read(self.slot(slot.name())?)
    }

    /// Pins `pin` in `slot`, written as tC4 writes a pin ([`Pin::to_json`]).
    pub fn set_pin(&mut self, slot: SetSlot, pin: &Pin) {
        if let Ok(name) = SlotName::parse(slot.name()) {
            self.0.insert(name, SlotValue(pin.to_json()));
        }
    }
}

/// tC4's `recordMatchesResolution`: a decision made against `record` holds against the
/// resolution `answer` when it is a usable pin of the same resource.
pub fn resource_ok(record: &ResourceRef, answer: Option<&Answer>) -> bool {
    answer
        .and_then(Answer::resource)
        .is_some_and(|resolved| resolved.same_resource(record))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(v: serde_json::Value) -> Json {
        Json::try_from(&v).unwrap()
    }

    fn group(slots: serde_json::Value) -> PinGroupValue {
        let serde_json::Value::Object(map) = slots else {
            panic!("an object")
        };
        PinGroupValue(
            map.iter()
                .map(|(k, v)| (SlotName::parse(k).unwrap(), SlotValue(json(v.clone()))))
                .collect(),
        )
    }

    const SHA_A: &str = "e586762e330f482a60c52aedd1c7b3a2f155df8a";
    const SHA_B: &str = "7cbb3419f79780ce84b92ad7d42bd476cc5d8a64";

    #[test]
    fn coverage_reads_books_as_tc4_coverage_for_does() {
        let of = |v: serde_json::Value| Coverage::of_slot(&json(serde_json::json!({ "books": v })));
        assert_eq!(of(serde_json::json!([])), Coverage::Unrecorded);
        assert_eq!(of(serde_json::json!(["TIT", 3])), Coverage::Unrecorded);
        assert_eq!(of(serde_json::json!("TIT")), Coverage::Unrecorded);
        assert_eq!(of(serde_json::json!(["bible"])), Coverage::Whole);
        assert_eq!(of(serde_json::json!(["TIT", "BIBLE"])), Coverage::Whole);
        let books = of(serde_json::json!(["tit", "JON", "TRANSLATE"]));
        assert!(books.covers(BookId::Titus) && books.covers(BookId::Jonah));
        assert!(!books.covers(BookId::Ruth));
        assert_eq!(
            Coverage::of_slot(&json(serde_json::json!({}))),
            Coverage::Unrecorded
        );
    }

    #[test]
    fn the_ladder_follows_coverage_and_stops_at_an_unusable_pin() {
        let sets = LanguageSets {
            primary: Some(group(serde_json::json!({
                "translationNotes": {"repoPath": "git.door43.org/es-419_gl/es-419_tn", "sha": SHA_A, "books": ["TIT"]},
                "translationWordsLinks": {"repoPath": "git.door43.org/es-419_gl/es-419_twl", "version": "v2", "books": ["TIT"]},
                "translationQuestions": {"repoPath": "git.door43.org/es-419_gl/es-419_tq", "version": "v2"},
                "obs-tn": {"repoPath": "git.door43.org/es-419_gl/es-419_obs-tn", "version": "v2"},
            }))),
            fallback: Some(group(serde_json::json!({
                "translationNotes": {"repoPath": "git.door43.org/unfoldingWord/en_tn", "sha": SHA_B},
                "translationWordsLinks": {"repoPath": "git.door43.org/unfoldingWord/en_twl", "sha": SHA_B},
                "translationQuestions": {"repoPath": "git.door43.org/unfoldingWord/en_tq", "sha": SHA_B},
                "obs-tn": {"repoPath": "git.door43.org/unfoldingWord/en_obs-tn", "sha": SHA_B},
            }))),
        };
        let fold = |tool, collection| sets.resolution(tool, collection);
        // Recorded coverage answers on the primary; another book falls to the fallback, whose
        // pin records no books and so covers every book in the fold.
        let tit = fold(ToolId::TranslationNotes, Collection::Book(BookId::Titus)).unwrap();
        assert_eq!(tit.rung(), Rung::Primary);
        let ruth = fold(ToolId::TranslationNotes, Collection::Book(BookId::Ruth)).unwrap();
        assert_eq!(ruth.rung(), Rung::Fallback);
        // An unusable pin that records the book stops the ladder; one that records nothing
        // covers nothing and the next rung answers.
        let twl = fold(ToolId::TranslationWords, Collection::Book(BookId::Titus)).unwrap();
        assert!(matches!(
            twl,
            Answer::Unusable {
                rung: Rung::Primary,
                ..
            }
        ));
        assert_eq!(twl.resource(), None);
        let tq = fold(
            ToolId::TranslationQuestions,
            Collection::Book(BookId::Titus),
        )
        .unwrap();
        assert_eq!(tq.rung(), Rung::Fallback);
        // The stories: the first rung that holds the slot answers, usable or not.
        let obs = fold(ToolId::TranslationNotes, Collection::Stories).unwrap();
        assert!(matches!(
            obs,
            Answer::Unusable {
                rung: Rung::Primary,
                ..
            }
        ));
        // On a computer, a pin that records no books covers what its install holds.
        let none = |_: &ResourceRef| None;
        let local = sets.answer(
            ToolId::TranslationNotes,
            Collection::Book(BookId::Ruth),
            Unrecorded::Installed(&none),
        );
        assert_eq!(local, None);
        let ruth_installed = |_: &ResourceRef| Some(Coverage::from_codes(["RUT"]));
        let local = sets.answer(
            ToolId::TranslationNotes,
            Collection::Book(BookId::Ruth),
            Unrecorded::Installed(&ruth_installed),
        );
        assert_eq!(local.map(|a| a.rung()), Some(Rung::Fallback));
    }

    #[test]
    fn a_decision_matches_the_same_resource_ignoring_path_case() {
        let pin = |path: &str, sha: &str| ResourceRef {
            repo_path: path.into(),
            sha: GitOid::parse(sha).unwrap(),
        };
        let answer = Answer::Pin {
            rung: Rung::Primary,
            pin: Pin {
                resource: pin("git.door43.org/Es-419_gl/es-419_tn", SHA_A),
                version: None,
                coverage: Coverage::Unrecorded,
            },
        };
        assert!(resource_ok(
            &pin("git.door43.org/es-419_gl/es-419_tn", SHA_A),
            Some(&answer)
        ));
        assert!(!resource_ok(
            &pin("git.door43.org/es-419_gl/es-419_tn", SHA_B),
            Some(&answer)
        ));
        assert!(!resource_ok(&pin("x", SHA_A), None));
    }
}
