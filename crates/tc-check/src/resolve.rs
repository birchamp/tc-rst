//! Which pinned resource a (tool, book) checks against (tC4 `resolve.ts`, D17 and D30).
//!
//! A project pins two language sets, primary and fallback. A tool's check list for a book
//! comes from the first set whose pin for the tool covers the book. Identity is the
//! repository path (compared ignoring letter case) and the commit; the version tag is a
//! label.
//!
//! The pins mirror the journal's `Pins` records (STORAGE.md §2.3): [`Pins::primary`] and
//! [`Pins::fallback`] are the `Primary` and `Fallback` pin groups, and each slot of a
//! [`LanguageSet`] is the slot of the same tC4 name. [`ResourceRef`] is the journal's
//! `ResourceRef`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde_json::Value;
use tc_core::BookId;
use tc_helps::CommitSha;

use crate::Error;
use crate::key::{Collection, ToolId};

/// A repository path such as `git.door43.org/unfoldingWord/en_tn`, kept as written and
/// compared ignoring ASCII letter case, as DCS addresses are.
#[derive(Debug, Clone, Eq)]
pub struct RepoPath(Box<str>);

impl RepoPath {
    /// Accepts non-empty text without whitespace.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let ok = !text.is_empty() && !text.chars().any(char::is_whitespace);
        ok.then(|| Self(text.into()))
            .ok_or_else(|| Error::RepoPath(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The repository name, the last segment: `en_tn`.
    pub fn repo(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }
}

impl PartialEq for RepoPath {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}

impl std::hash::Hash for RepoPath {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_ascii_lowercase().hash(state);
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One resource at one commit: what a decision records it was made against.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceRef {
    pub repo_path: RepoPath,
    pub sha: CommitSha,
}

impl fmt::Display for ResourceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.repo_path, self.sha)
    }
}

/// Which books a pinned resource holds, as the pin records it (tC4 `books`, D41).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Coverage {
    /// The pin records no books: a pin written before coverage was recorded.
    Unrecorded,
    /// `["BIBLE"]`: a resource not divided into books, such as translation words.
    Whole,
    Books(BTreeSet<BookId>),
}

impl Coverage {
    /// Reads a pin's `books` list as tC4's `coverageFor` does: an empty list records
    /// nothing, `BIBLE` is the whole collection, and codes that name no book (`TRANSLATE`)
    /// cover nothing.
    pub fn from_codes<'a>(codes: impl IntoIterator<Item = &'a str>) -> Self {
        let codes: Vec<String> = codes.into_iter().map(str::to_ascii_uppercase).collect();
        if codes.is_empty() {
            return Self::Unrecorded;
        }
        if codes.iter().any(|c| c == "BIBLE") {
            return Self::Whole;
        }
        Self::Books(codes.iter().filter_map(|c| c.parse().ok()).collect())
    }

    pub fn covers(&self, book: BookId) -> bool {
        match self {
            Self::Unrecorded => false,
            Self::Whole => true,
            Self::Books(books) => books.contains(&book),
        }
    }

    pub fn is_recorded(&self) -> bool {
        !matches!(self, Self::Unrecorded)
    }
}

/// A resource pinned in a language set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub resource: ResourceRef,
    /// The release tag, a label for people.
    pub version: Option<String>,
    pub coverage: Coverage,
}

impl Pin {
    /// `en_tn · v86`, or the first 12 digits of the commit when there is no tag (tC4's
    /// guided-fix label).
    pub fn label(&self) -> String {
        let version = self
            .version
            .clone()
            .unwrap_or_else(|| self.resource.sha.as_str().chars().take(12).collect());
        format!("{} · {version}", self.resource.repo_path.repo())
    }
}

/// The two rungs of tC4's ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rung {
    Primary,
    Fallback,
}

impl Rung {
    pub const LADDER: [Rung; 2] = [Rung::Primary, Rung::Fallback];

    pub fn name(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Fallback => "fallback",
        }
    }
}

/// The slots of a language set, by their tC4 names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    TranslationNotes,
    TranslationWordsLinks,
    TranslationWords,
    TranslationAcademy,
    TranslationQuestions,
    SimplifiedText,
    Obs,
    ObsNotes,
    ObsWordsLinks,
    ObsQuestions,
    ObsImages,
}

impl Slot {
    /// The help slots a release upgrade or a re-pin can move, in tC4's display order
    /// (`SET_SLOTS` of `upgrade.ts`).
    pub const HELPS: [Slot; 9] = [
        Slot::TranslationNotes,
        Slot::TranslationWordsLinks,
        Slot::TranslationWords,
        Slot::TranslationAcademy,
        Slot::TranslationQuestions,
        Slot::Obs,
        Slot::ObsNotes,
        Slot::ObsWordsLinks,
        Slot::ObsImages,
    ];

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }

    const ALL: [Slot; 11] = [
        Slot::TranslationNotes,
        Slot::TranslationWordsLinks,
        Slot::TranslationWords,
        Slot::TranslationAcademy,
        Slot::TranslationQuestions,
        Slot::SimplifiedText,
        Slot::Obs,
        Slot::ObsNotes,
        Slot::ObsWordsLinks,
        Slot::ObsQuestions,
        Slot::ObsImages,
    ];

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

    /// The slot a tool's check list (or, for questions, its help list) comes from.
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

/// One language set: its pins by slot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LanguageSet {
    pub pins: BTreeMap<Slot, Pin>,
}

impl LanguageSet {
    pub fn get(&self, slot: Slot) -> Option<&Pin> {
        self.pins.get(&slot)
    }

    /// Reads a tC4 language set (`languageSets.primary` of `resources.json`, or the slots
    /// of a journal `Pins` group): every slot this crate knows, as a pin. Other keys,
    /// such as `gatewayLanguage`, are not pins and are left out.
    pub fn from_json(value: &Value) -> Result<Self, Error> {
        let object = value
            .as_object()
            .ok_or_else(|| Error::Pin(value.to_string()))?;
        let mut pins = BTreeMap::new();
        for (name, pin) in object {
            if let Some(slot) = Slot::parse(name) {
                pins.insert(slot, Pin::from_json(pin)?);
            }
        }
        Ok(Self { pins })
    }
}

impl Pin {
    /// Reads a tC4 pin: `{"repoPath", "version"?, "sha", "books"?, ...}`. A pin without
    /// a commit is refused: the commit is its identity (D58).
    pub fn from_json(value: &Value) -> Result<Self, Error> {
        let bad = || Error::Pin(value.to_string());
        let text = |key: &str| value.get(key).and_then(Value::as_str);
        let repo_path = RepoPath::parse(text("repoPath").ok_or_else(bad)?)?;
        let sha = CommitSha::parse(text("sha").ok_or_else(bad)?).map_err(|_| bad())?;
        let books: Vec<&str> = match value.get("books") {
            None => Vec::new(),
            Some(books) => books
                .as_array()
                .ok_or_else(bad)?
                .iter()
                .map(|b| b.as_str().ok_or_else(bad))
                .collect::<Result<_, _>>()?,
        };
        Ok(Self {
            resource: ResourceRef { repo_path, sha },
            version: text("version").map(str::to_owned),
            coverage: Coverage::from_codes(books),
        })
    }
}

/// A project's language sets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pins {
    pub primary: Option<LanguageSet>,
    pub fallback: Option<LanguageSet>,
}

impl Pins {
    pub fn set(&self, rung: Rung) -> Option<&LanguageSet> {
        match rung {
            Rung::Primary => self.primary.as_ref(),
            Rung::Fallback => self.fallback.as_ref(),
        }
    }

    pub fn pin(&self, rung: Rung, slot: Slot) -> Option<&Pin> {
        self.set(rung)?.get(slot)
    }

    /// The first rung that pins exactly `resource` in `slot`. A pin both rungs carry
    /// is pinned by the primary (tC4 `rungPinning`, D97).
    pub fn rung_pinning(&self, slot: Slot, resource: &ResourceRef) -> Option<Rung> {
        Rung::LADDER.into_iter().find(|rung| {
            self.pin(*rung, slot)
                .is_some_and(|p| p.resource == *resource)
        })
    }
}

/// The pin a (tool, collection) checks against, and the rung it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub rung: Rung,
    pub pin: Pin,
}

impl Resolution {
    /// True when the primary set did not answer.
    pub fn used_fallback(&self) -> bool {
        self.rung == Rung::Fallback
    }
}

/// The resolution the journal's fold uses (STORAGE.md §7.7): the ladder over the pins
/// with each pin's recorded coverage only, so it never depends on what is installed. A
/// story collection is not divided into books, so its first pinned rung answers.
pub fn resolve(pins: &Pins, tool: ToolId, collection: Collection) -> Option<Resolution> {
    resolve_with(pins, tool, collection, |_| None)
}

/// [`resolve`] with tC4's local fallback for coverage: a pin that records no books is
/// read with the books its installed copy holds, when `local` knows them.
pub fn resolve_with(
    pins: &Pins,
    tool: ToolId,
    collection: Collection,
    local: impl Fn(&ResourceRef) -> Option<Coverage>,
) -> Option<Resolution> {
    let slot = Slot::of_tool(tool, collection);
    Rung::LADDER.into_iter().find_map(|rung| {
        let pin = pins.pin(rung, slot)?;
        let covers = match collection {
            Collection::Stories => true,
            Collection::Book(book) => coverage_of(pin, &local).covers(book),
        };
        covers.then(|| Resolution {
            rung,
            pin: pin.clone(),
        })
    })
}

/// A pin's coverage: its own record, else what is installed (tC4 `coverageFor`).
pub(crate) fn coverage_of(
    pin: &Pin,
    local: &impl Fn(&ResourceRef) -> Option<Coverage>,
) -> Coverage {
    if pin.coverage.is_recorded() {
        return pin.coverage.clone();
    }
    local(&pin.resource)
        .filter(Coverage::is_recorded)
        .unwrap_or(Coverage::Unrecorded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_reads_the_pin_record() {
        assert_eq!(Coverage::from_codes([]), Coverage::Unrecorded);
        assert_eq!(Coverage::from_codes(["bible"]), Coverage::Whole);
        let c = Coverage::from_codes(["tit", "JON", "TRANSLATE"]);
        assert!(c.covers(BookId::Titus) && c.covers(BookId::Jonah));
        assert!(!c.covers(BookId::Ruth));
    }

    #[test]
    fn repo_paths_compare_ignoring_case() {
        let a = RepoPath::parse("git.door43.org/Es-419_gl/es-419_tn").unwrap();
        let b = RepoPath::parse("git.door43.org/es-419_gl/es-419_tn").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.repo(), "es-419_tn");
        assert!(RepoPath::parse("").is_err());
    }
}
