//! Which pinned resource answers a (tool, book), and what opening it requires (tC4
//! `resolve.ts`, D17 and the five D30 constraints):
//!
//! 1. The resolution unit is (tool, book): checks in one book never mix languages.
//! 2. The ladder is exactly `primary` then `fallback`.
//! 3. A project's pins bind every opener; nothing here reads a personal preference.
//! 4. Online with the pinned version absent: fetch it.
//! 5. Offline with it absent: that (tool, book) is unavailable, a first-class state that
//!    blocks nothing else.
//!
//! A rung answers a book when its pin covers the book. A pin's own recorded `books` win;
//! otherwise the books the exact installed commit holds ([`LocalCoverage`]); otherwise
//! coverage is unknown and the rung does not answer. The fold (STORAGE §7.7
//! `resource_ok`) must resolve the same way on every device, so it reads recorded
//! coverage only: it passes [`RecordedOnly`].

use tc_core::BookId;
use tc_door43::{GitOid, RepoRef};

use crate::pin::{BookCoverage, RepoPath, ResourcePin};
use crate::sets::{ProjectPins, Rung, SetSlot, Tool};

/// The books an installed commit holds, for a pin whose own record says nothing.
pub trait LocalCoverage {
    /// `None` when this exact commit (repository and sha) is not installed.
    fn local_books(&self, pin: &ResourcePin) -> Option<BookCoverage>;
}

impl<F: Fn(&ResourcePin) -> Option<BookCoverage>> LocalCoverage for F {
    fn local_books(&self, pin: &ResourcePin) -> Option<BookCoverage> {
        self(pin)
    }
}

/// No local input: recorded coverage only, as the fold resolves.
#[derive(Debug, Clone, Copy, Default)]
pub struct RecordedOnly;

impl LocalCoverage for RecordedOnly {
    fn local_books(&self, _: &ResourcePin) -> Option<BookCoverage> {
        None
    }
}

/// Where a pin's coverage came from. Only `Unknown` is ambiguous, and only it warrants
/// the warned fallback (D41).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageSource {
    /// The pin records its books, captured against this commit.
    Pin,
    /// Read from the installed copy of this commit.
    Local,
    /// Neither: the book may or may not be in the resource.
    Unknown,
}

pub fn coverage_for(
    pin: &ResourcePin,
    local: &dyn LocalCoverage,
) -> (Option<BookCoverage>, CoverageSource) {
    if let Some(books) = &pin.books {
        return (Some(books.clone()), CoverageSource::Pin);
    }
    match local.local_books(pin) {
        Some(books) => (Some(books), CoverageSource::Local),
        None => (None, CoverageSource::Unknown),
    }
}

pub fn covers(pin: &ResourcePin, book: BookId, local: &dyn LocalCoverage) -> bool {
    coverage_for(pin, local)
        .0
        .is_some_and(|coverage| coverage.covers(book))
}

/// The rung and pin that answer a (slot, book).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub rung: Rung,
    pub pin: ResourcePin,
}

impl Resolution {
    /// True when the primary did not answer and English did. A resolution change is a
    /// warned update (D17).
    pub fn used_fallback(&self) -> bool {
        self.rung == Rung::Fallback
    }

    /// The record a decision stores as its `resource` (tC4 `resolutionRecord`).
    pub fn record(&self) -> ResolutionRecord {
        ResolutionRecord {
            repo_path: self.pin.repo.clone(),
            version: self.pin.version.as_ref().map(|v| v.as_str().to_owned()),
            sha: self.pin.sha,
            language_set: self.rung,
        }
    }
}

/// tC4's §5.2 `resource` record: what a decision file stores so a later reader knows
/// which resource produced its checks. The version is a label for people.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionRecord {
    pub repo_path: RepoPath,
    pub version: Option<String>,
    pub sha: GitOid,
    pub language_set: Rung,
}

/// STORAGE §2.3 `ResourceRef`: the identity a decision or alignment was made against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRef {
    pub repo_path: RepoPath,
    pub sha: GitOid,
}

impl From<&ResourcePin> for ResourceRef {
    fn from(pin: &ResourcePin) -> Self {
        Self {
            repo_path: pin.repo.clone(),
            sha: pin.sha,
        }
    }
}

/// Resolves one language-set slot for a book over the ladder (tC4 `resolveSetSlot`). An
/// absent slot covers no book; the rung is skipped (D64).
pub fn resolve_slot(
    pins: &ProjectPins,
    slot: SetSlot,
    book: BookId,
    local: &dyn LocalCoverage,
) -> Option<Resolution> {
    Rung::LADDER.into_iter().find_map(|rung| {
        let pin = pins.set(rung)?.pin(slot)?;
        covers(pin, book, local).then(|| Resolution {
            rung,
            pin: pin.clone(),
        })
    })
}

/// Resolves a checking tool for a book (tC4 `resolveToolBook`).
pub fn resolve_tool_book(
    pins: &ProjectPins,
    tool: Tool,
    book: BookId,
    local: &dyn LocalCoverage,
) -> Option<Resolution> {
    resolve_slot(pins, tool.slot(), book, local)
}

/// Resolves an Open Bible Stories slot (tC4 `resolveObsSetSlot`): the collection is not
/// divided into books, so a pin answers on presence.
pub fn resolve_obs_slot(pins: &ProjectPins, slot: SetSlot) -> Option<Resolution> {
    Rung::LADDER.into_iter().find_map(|rung| {
        let pin = pins.set(rung)?.pin(slot)?;
        Some(Resolution {
            rung,
            pin: pin.clone(),
        })
    })
}

/// Whether a stored record still matches the current resolution (tC4
/// `recordMatchesResolution`). Repository without regard to case, sha exactly; the
/// label is not compared. A record with no sha (tC3-era) cannot be a [`ResourceRef`],
/// so the caller passes `None`, which never matches: the warned update, the safe way.
pub fn record_matches(stored: Option<&ResourceRef>, resolution: Option<&Resolution>) -> bool {
    match (stored, resolution) {
        (Some(stored), Some(r)) => stored.repo_path == r.pin.repo && stored.sha == r.pin.sha,
        _ => false,
    }
}

/// What a decision is about: a Bible book, or Open Bible Stories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CheckUnit {
    Book(BookId),
    Stories,
}

/// Resolves a tool for a book, or for the stories by presence.
pub fn resolve_unit(
    pins: &ProjectPins,
    tool: Tool,
    unit: CheckUnit,
    local: &dyn LocalCoverage,
) -> Option<Resolution> {
    match unit {
        CheckUnit::Book(book) => resolve_tool_book(pins, tool, book, local),
        CheckUnit::Stories => resolve_obs_slot(pins, tool.obs_slot()),
    }
}

/// STORAGE §7.7 `resource_ok`: whether a decision's `resource` is what its (tool, unit)
/// resolves to over the pins alone, with recorded coverage and no installed input, so
/// every device computes the same answer. A decision that fails it computes as
/// invalidated and is kept (D36).
pub fn resource_ok(
    pins: &ProjectPins,
    tool: Tool,
    unit: CheckUnit,
    resource: &ResourceRef,
) -> bool {
    record_matches(
        Some(resource),
        resolve_unit(pins, tool, unit, &RecordedOnly).as_ref(),
    )
}

/// STORAGE §7.7's original-language clause of an alignment's validity: an alignment made
/// against a pinned text (`AlignmentValue::source`) is valid only while the book's
/// testament still pins that commit. An import with no recorded source passes.
pub fn alignment_source_ok(pins: &ProjectPins, book: BookId, source: Option<&ResourceRef>) -> bool {
    match source {
        None => true,
        Some(source) => pins
            .original_language
            .for_book(book)
            .is_some_and(|pin| pin.sha == source.sha),
    }
}

/// What a check session can do with one (tool, book) (tC4 `PreflightState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreflightState {
    /// The resolved pin is installed: open the session.
    Ready,
    /// The pinned version is absent and the internet is on: fetch it.
    Fetch,
    /// The pinned version is absent and the internet is off. First-class, never an
    /// error, never a block on other work (D30.5).
    Unavailable,
    /// No language set pins this tool.
    Unpinned,
    /// Pins exist, but neither rung covers this book.
    NotCovered,
    /// The words-links pin is the pre-release one-repository form (D97 ruling 6): it is
    /// never read, and the project must be created again.
    OldTwl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preflight {
    pub tool: Tool,
    pub book: BookId,
    pub state: PreflightState,
    pub resolution: Option<Resolution>,
    /// The pin this machine lacks: what to fetch, or what the fix screen names. For tW
    /// this can be the resolving set's articles pin.
    pub needs: Option<ResourcePin>,
    /// Set when the session resolved to English only because the primary pin, whose
    /// coverage is unknown, is not installed (B20, narrowed by D41): the session is usable,
    /// but the screen must say it is checking against a substitute.
    pub unavailable_primary: Option<ResourcePin>,
}

/// Decides what opening this (tool, book) requires (tC4 `preflightToolBook`). `installed`
/// answers whether this exact pinned commit is on this machine.
pub fn preflight_tool_book(
    pins: Option<&ProjectPins>,
    tool: Tool,
    book: BookId,
    local: &dyn LocalCoverage,
    installed: &dyn Fn(&ResourcePin) -> bool,
    online: bool,
) -> Preflight {
    let mut out = Preflight {
        tool,
        book,
        state: PreflightState::Unpinned,
        resolution: None,
        needs: None,
        unavailable_primary: None,
    };
    let Some(pins) = pins.filter(|p| p.has_language_sets()) else {
        return out;
    };
    if tool == Tool::TranslationWords
        && let Some(old) = pins.old_twl_pin()
    {
        out.state = PreflightState::OldTwl;
        out.needs = Some(old.clone());
        return out;
    }
    let availability = |pin: &ResourcePin| {
        if installed(pin) {
            PreflightState::Ready
        } else if online {
            PreflightState::Fetch
        } else {
            PreflightState::Unavailable
        }
    };
    if let Some(resolution) = resolve_tool_book(pins, tool, book, local) {
        let missing = missing_pin(pins, tool, &resolution, installed);
        out.state = missing.as_ref().map_or(PreflightState::Ready, availability);
        out.needs = missing;
        let primary = pins.set(Rung::Primary).and_then(|set| set.pin(tool.slot()));
        out.unavailable_primary = primary
            .filter(|_| resolution.used_fallback())
            .filter(|pin| coverage_for(pin, local).1 == CoverageSource::Unknown)
            .filter(|pin| !installed(pin))
            .cloned();
        out.resolution = Some(resolution);
        return out;
    }
    let pinned: Vec<&ResourcePin> = Rung::LADDER
        .into_iter()
        .filter_map(|rung| pins.set(rung)?.pin(tool.slot()))
        .collect();
    if pinned.is_empty() {
        return out;
    }
    // A pin whose coverage is unknown and that is not installed may hold the book: when
    // online that is a fetch, not a verdict about the book.
    let unfetched = pinned
        .into_iter()
        .filter(|p| coverage_for(p, local).1 == CoverageSource::Unknown)
        .find(|p| !installed(p));
    match unfetched {
        Some(pin) if online => {
            out.state = PreflightState::Fetch;
            out.needs = Some(pin.clone());
        }
        Some(_) => out.state = PreflightState::Unavailable,
        None => out.state = PreflightState::NotCovered,
    }
    out
}

/// The pin a resolved (tool, book) still lacks. The tW tool reads two pins of the
/// resolving set: the links it resolved on and that set's articles (D97).
fn missing_pin(
    pins: &ProjectPins,
    tool: Tool,
    resolution: &Resolution,
    installed: &dyn Fn(&ResourcePin) -> bool,
) -> Option<ResourcePin> {
    if !installed(&resolution.pin) {
        return Some(resolution.pin.clone());
    }
    if tool != Tool::TranslationWords {
        return None;
    }
    pins.set(resolution.rung)?
        .pin(SetSlot::TranslationWords)
        .filter(|tw| !installed(tw))
        .cloned()
}

/// How many of a project's pins name a version this computer lacks (tC4
/// `missingPinCount`, `src/data/installed.ts:282`, D88 point 6): each pin of both language
/// sets, the original-language `nt` and `ot` pins and each source pane pin, once per
/// place it is pinned. `installed` answers whether this exact commit is on this machine.
///
/// Lexicon pins are left out, as tC4 leaves them out: the Download that this count offers
/// after a sync opens Source texts, which offers no lexicon.
pub fn missing_pin_count(pins: &ProjectPins, installed: &dyn Fn(&ResourcePin) -> bool) -> usize {
    downloadable_pins(pins)
        .filter(|pin| !installed(pin))
        .count()
}

/// The project's pin of `repo` that no install on this machine satisfies: the version a
/// download of that repository from Source texts must fetch, instead of the catalogue's
/// latest (tC4 `unsatisfiedProjectPinFor`, `src/data/installed.ts:328`, D58). The primary
/// set is searched first, then the fallback set, `nt`, `ot` and the source panes. `None`
/// when the project pins `repo` nowhere or every pin of it is installed.
pub fn unsatisfied_pin_for<'a>(
    pins: &'a ProjectPins,
    repo: &RepoRef,
    installed: &dyn Fn(&ResourcePin) -> bool,
) -> Option<&'a ResourcePin> {
    downloadable_pins(pins).find(|pin| pin.is_repo(repo) && !installed(pin))
}

/// The pins that Source texts can download, in tC4's search order.
fn downloadable_pins(pins: &ProjectPins) -> impl Iterator<Item = &ResourcePin> {
    let sets = Rung::LADDER
        .into_iter()
        .filter_map(|rung| pins.set(rung))
        .flat_map(|set| set.pins.values());
    let original = &pins.original_language;
    sets.chain(original.nt.iter())
        .chain(original.ot.iter())
        .chain(pins.extra_scripture.iter().map(|pane| &pane.pin))
}

/// Where in a project's pins a pin sits, for a coverage write-back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinPlace {
    Set(Rung, SetSlot),
    Pane(crate::sets::PaneId),
}

/// Records book coverage on pins that lack it while their resource is installed (tC4
/// `backfillCoverage`, D41, D61, D62). A recorded list is never shrunk or replaced, but a
/// sha-exact local read may widen it; a whole-collection record already covers every
/// book. Pure: returns the new pins and the places that changed, and the caller writes
/// them through the project store.
pub fn backfill_coverage(
    pins: &ProjectPins,
    local: &dyn LocalCoverage,
) -> (ProjectPins, Vec<PinPlace>) {
    let mut next = pins.clone();
    let mut changed = Vec::new();
    for rung in Rung::LADDER {
        let Some(set) = next.set_mut(rung).as_mut() else {
            continue;
        };
        for (slot, pin) in set.pins.iter_mut() {
            if slot.is_per_book() && widen(pin, local) {
                changed.push(PinPlace::Set(rung, *slot));
            }
        }
    }
    for pane in &mut next.extra_scripture {
        if widen(&mut pane.pin, local) {
            changed.push(PinPlace::Pane(pane.id.clone()));
        }
    }
    (next, changed)
}

fn widen(pin: &mut ResourcePin, local: &dyn LocalCoverage) -> bool {
    let Some(found) = local.local_books(pin) else {
        return false;
    };
    match (&pin.books, found) {
        (Some(BookCoverage::WholeCollection), _) => false,
        (None, found) => {
            pin.books = Some(found);
            true
        }
        (Some(BookCoverage::Books(_)), BookCoverage::WholeCollection) => false,
        (Some(BookCoverage::Books(held)), BookCoverage::Books(read)) => {
            let missing: Vec<BookId> = read.into_iter().filter(|b| !held.contains(b)).collect();
            if missing.is_empty() {
                return false;
            }
            let mut wider = held.clone();
            wider.extend(missing);
            pin.books = Some(BookCoverage::Books(wider));
            true
        }
    }
}

#[cfg(test)]
mod tests {
    //! tC4 `test/resolve.test.ts`, ported.

    use std::collections::HashMap;

    use super::*;
    use crate::sets::{GatewayLanguage, LanguageSet};

    /// tC4's deterministic fake sha per (repo, version): djb2-xor, 8 hex digits, five
    /// times.
    fn sha40(s: &str) -> String {
        let mut h: u32 = 5381;
        for c in s.encode_utf16() {
            h = h.wrapping_mul(33) ^ u32::from(c);
        }
        format!("{h:08x}").repeat(5)
    }

    fn pin(repo: &str, version: &str) -> ResourcePin {
        ResourcePin::new(
            format!("git.door43.org/{repo}").parse().unwrap(),
            sha40(&format!("{}@{version}", repo.to_lowercase()))
                .parse()
                .unwrap(),
        )
        .with_version(version.parse().unwrap())
        .with_flavor("parascriptural/x-bcvnotes")
    }

    fn set(lang: &str, owner: &str, v: &str) -> LanguageSet {
        LanguageSet::new(GatewayLanguage {
            language: lang.to_owned(),
            owner: owner.parse().unwrap(),
        })
        .with(
            SetSlot::TranslationNotes,
            pin(&format!("{owner}/{lang}_tn"), v),
        )
        .with(
            SetSlot::TranslationWordsLinks,
            pin(&format!("{owner}/{lang}_twl"), v),
        )
        .with(
            SetSlot::TranslationWords,
            pin(&format!("{owner}/{lang}_tw"), v),
        )
        .with(
            SetSlot::TranslationAcademy,
            pin(&format!("{owner}/{lang}_ta"), v),
        )
    }

    fn resources() -> ProjectPins {
        ProjectPins {
            primary: Some(set("es-419", "Es-419_gl", "v66")),
            fallback: Some(set("en", "unfoldingWord", "v86")),
            ..ProjectPins::default()
        }
    }

    fn books(codes: &[&str]) -> BookCoverage {
        BookCoverage::from_codes(codes).unwrap()
    }

    /// es-419 covers its four released books; English covers the canon. Keyed by exact
    /// identity, so the closure compares repository and sha.
    fn coverage() -> impl Fn(&ResourcePin) -> Option<BookCoverage> {
        let r = resources();
        let p = r.primary.unwrap();
        let f = r.fallback.unwrap();
        let mut map: HashMap<(String, String), BookCoverage> = HashMap::new();
        let mut add = |pin: &ResourcePin, b: &[&str]| {
            map.insert(
                (pin.repo.to_string().to_lowercase(), pin.sha.to_string()),
                books(b),
            );
        };
        for slot in [SetSlot::TranslationNotes, SetSlot::TranslationWordsLinks] {
            add(p.pin(slot).unwrap(), &["3JN", "JON", "RUT", "TIT"]);
            add(f.pin(slot).unwrap(), &["TIT", "JON", "HEB", "PSA"]);
        }
        move |pin: &ResourcePin| {
            map.get(&(pin.repo.to_string().to_lowercase(), pin.sha.to_string()))
                .cloned()
        }
    }

    #[test]
    fn d30_1_resolution_is_per_tool_and_book() {
        let cov = coverage();
        let r = resources();
        let tit = resolve_tool_book(&r, Tool::TranslationNotes, BookId::Titus, &cov).unwrap();
        let heb = resolve_tool_book(&r, Tool::TranslationNotes, BookId::Hebrews, &cov).unwrap();
        assert_eq!(tit.rung, Rung::Primary);
        assert_eq!(heb.rung, Rung::Fallback);
        assert!(heb.used_fallback());
    }

    #[test]
    fn d30_2_neither_rung_covers_the_book() {
        let cov = coverage();
        assert!(
            resolve_tool_book(
                &resources(),
                Tool::TranslationNotes,
                BookId::Revelation,
                &cov
            )
            .is_none()
        );
    }

    #[test]
    fn d30_4_and_5_preflight_states() {
        let cov = coverage();
        let r = resources();
        let all = |_: &ResourcePin| true;
        let none = |_: &ResourcePin| false;
        let state = |pins: Option<&ProjectPins>,
                     book,
                     local: &dyn LocalCoverage,
                     installed: &dyn Fn(&ResourcePin) -> bool,
                     online| {
            preflight_tool_book(pins, Tool::TranslationNotes, book, local, installed, online).state
        };
        assert_eq!(
            state(None, BookId::Titus, &cov, &all, true),
            PreflightState::Unpinned
        );
        assert_eq!(
            state(Some(&r), BookId::Revelation, &cov, &all, true),
            PreflightState::NotCovered
        );
        // Offline with unknown coverage (nothing fetched yet) is unavailable, not a false
        // not-covered verdict.
        assert_eq!(
            state(Some(&r), BookId::Revelation, &RecordedOnly, &none, false),
            PreflightState::Unavailable
        );
        assert_eq!(
            state(Some(&r), BookId::Revelation, &RecordedOnly, &none, true),
            PreflightState::Fetch
        );
        assert_eq!(
            state(Some(&r), BookId::Titus, &cov, &all, false),
            PreflightState::Ready
        );
        assert_eq!(
            state(Some(&r), BookId::Titus, &cov, &none, false),
            PreflightState::Unavailable
        );
        assert_eq!(
            state(Some(&r), BookId::Titus, &cov, &none, true),
            PreflightState::Fetch
        );
    }

    #[test]
    fn tw_needs_the_resolving_sets_articles_too() {
        let cov = coverage();
        let r = resources();
        let tw = r
            .primary
            .as_ref()
            .unwrap()
            .pin(SetSlot::TranslationWords)
            .unwrap()
            .clone();
        let tw_for_check = tw.clone();
        let installed = move |p: &ResourcePin| !p.same_identity(&tw_for_check);
        let pre = preflight_tool_book(
            Some(&r),
            Tool::TranslationWords,
            BookId::Titus,
            &cov,
            &installed,
            true,
        );
        assert_eq!(pre.state, PreflightState::Fetch);
        assert_eq!(pre.needs, Some(tw));
    }

    #[test]
    fn the_warned_fallback_is_only_for_an_unknown_uninstalled_primary() {
        let r = resources();
        let f = r
            .fallback
            .as_ref()
            .unwrap()
            .pin(SetSlot::TranslationNotes)
            .unwrap()
            .clone();
        let f_check = f.clone();
        // Only the English notes are installed and known.
        let local = move |p: &ResourcePin| p.same_identity(&f_check).then(|| books(&["HEB"]));
        let f_inst = f.clone();
        let installed = move |p: &ResourcePin| p.same_identity(&f_inst);
        let pre = preflight_tool_book(
            Some(&r),
            Tool::TranslationNotes,
            BookId::Hebrews,
            &local,
            &installed,
            false,
        );
        assert_eq!(pre.state, PreflightState::Ready);
        assert_eq!(
            pre.unavailable_primary.unwrap().repo.name().as_str(),
            "es-419_tn"
        );
        // When the primary records its coverage, falling back is plainly correct.
        let mut recorded = r.clone();
        let p = recorded.primary.as_mut().unwrap();
        let tn = p.pins.get_mut(&SetSlot::TranslationNotes).unwrap();
        tn.books = Some(books(&["TIT"]));
        let pre = preflight_tool_book(
            Some(&recorded),
            Tool::TranslationNotes,
            BookId::Hebrews,
            &local,
            &installed,
            false,
        );
        assert_eq!(pre.unavailable_primary, None);
    }

    #[test]
    fn the_old_one_repository_twl_form_is_its_own_state() {
        let mut r = resources();
        let tw = r
            .fallback
            .as_ref()
            .unwrap()
            .pin(SetSlot::TranslationWords)
            .unwrap()
            .clone();
        r.fallback
            .as_mut()
            .unwrap()
            .pins
            .insert(SetSlot::TranslationWordsLinks, tw);
        let pre = preflight_tool_book(
            Some(&r),
            Tool::TranslationWords,
            BookId::Titus,
            &RecordedOnly,
            &|_| true,
            true,
        );
        assert_eq!(pre.state, PreflightState::OldTwl);
        let notes = preflight_tool_book(
            Some(&r),
            Tool::TranslationNotes,
            BookId::Titus,
            &coverage(),
            &|_| true,
            true,
        );
        assert_eq!(notes.state, PreflightState::Ready);
    }

    #[test]
    fn d17_a_stored_record_is_compared_by_repository_and_sha() {
        let cov = coverage();
        let now = resolve_tool_book(&resources(), Tool::TranslationNotes, BookId::Titus, &cov);
        let rec = |repo: &str, sha_of: &str| ResourceRef {
            repo_path: format!("git.door43.org/{repo}").parse().unwrap(),
            sha: sha40(sha_of).parse().unwrap(),
        };
        assert!(record_matches(
            Some(&rec("Es-419_gl/es-419_tn", "es-419_gl/es-419_tn@v66")),
            now.as_ref()
        ));
        assert!(record_matches(
            Some(&rec("es-419_GL/ES-419_TN", "es-419_gl/es-419_tn@v66")),
            now.as_ref()
        ));
        assert!(!record_matches(
            Some(&rec("Es-419_gl/es-419_tn", "es-419_gl/es-419_tn@v67")),
            now.as_ref()
        ));
        assert!(!record_matches(
            Some(&rec("unfoldingWord/en_tn", "unfoldingword/en_tn@v86")),
            now.as_ref()
        ));
        assert!(!record_matches(None, now.as_ref()));
        let record = now.unwrap().record();
        assert_eq!(record.version.as_deref(), Some("v66"));
        assert_eq!(record.language_set, Rung::Primary);
    }

    #[test]
    fn the_folds_rules_read_recorded_coverage_only() {
        let mut r = resources();
        let es_tn = r
            .primary
            .as_ref()
            .unwrap()
            .pin(SetSlot::TranslationNotes)
            .unwrap()
            .clone();
        let en_tn = r
            .fallback
            .as_ref()
            .unwrap()
            .pin(SetSlot::TranslationNotes)
            .unwrap()
            .clone();
        // Nothing records coverage: no rung answers, whatever is installed.
        assert!(!resource_ok(
            &r,
            Tool::TranslationNotes,
            CheckUnit::Book(BookId::Titus),
            &ResourceRef::from(&es_tn)
        ));
        r.primary
            .as_mut()
            .unwrap()
            .pins
            .get_mut(&SetSlot::TranslationNotes)
            .unwrap()
            .books = Some(books(&["TIT"]));
        r.fallback
            .as_mut()
            .unwrap()
            .pins
            .get_mut(&SetSlot::TranslationNotes)
            .unwrap()
            .books = Some(BookCoverage::WholeCollection);
        let tit = CheckUnit::Book(BookId::Titus);
        assert!(resource_ok(
            &r,
            Tool::TranslationNotes,
            tit,
            &ResourceRef::from(&es_tn)
        ));
        assert!(!resource_ok(
            &r,
            Tool::TranslationNotes,
            tit,
            &ResourceRef::from(&en_tn)
        ));
        assert!(resource_ok(
            &r,
            Tool::TranslationNotes,
            CheckUnit::Book(BookId::Ruth),
            &ResourceRef::from(&en_tn)
        ));

        let ugnt = pin("unfoldingWord/el-x-koine_ugnt", "v0.34");
        r.original_language.nt = Some(ugnt.clone());
        assert!(alignment_source_ok(
            &r,
            BookId::Titus,
            Some(&ResourceRef::from(&ugnt))
        ));
        assert!(!alignment_source_ok(
            &r,
            BookId::Titus,
            Some(&ResourceRef::from(&pin(
                "unfoldingWord/el-x-koine_ugnt",
                "v0.35"
            )))
        ));
        assert!(!alignment_source_ok(
            &r,
            BookId::Jonah,
            Some(&ResourceRef::from(&ugnt))
        ));
        assert!(alignment_source_ok(&r, BookId::Jonah, None));
    }

    #[test]
    fn backfill_records_widens_and_never_shrinks() {
        let mut r = resources();
        let fallback = r.fallback.as_mut().unwrap();
        fallback
            .pins
            .get_mut(&SetSlot::TranslationNotes)
            .unwrap()
            .books = Some(books(&["TIT", "MAT"]));
        fallback
            .pins
            .get_mut(&SetSlot::TranslationWords)
            .unwrap()
            .books = Some(BookCoverage::WholeCollection);
        let local = |p: &ResourcePin| match p.repo.name().as_str() {
            "en_tn" => Some(books(&["TIT", "JON"])),
            "en_tw" => Some(books(&["TIT"])),
            "es-419_ta" => Some(BookCoverage::WholeCollection),
            "es-419_tn" => Some(books(&["TIT"])),
            _ => None,
        };
        let (next, changed) = backfill_coverage(&r, &local);
        let f = next.fallback.as_ref().unwrap();
        assert_eq!(
            f.pin(SetSlot::TranslationNotes).unwrap().books,
            Some(books(&["TIT", "MAT", "JON"]))
        );
        assert_eq!(
            f.pin(SetSlot::TranslationWords).unwrap().books,
            Some(BookCoverage::WholeCollection)
        );
        let p = next.primary.as_ref().unwrap();
        assert_eq!(
            p.pin(SetSlot::TranslationAcademy).unwrap().books,
            Some(BookCoverage::WholeCollection)
        );
        assert_eq!(changed.len(), 3);
        let (again, none) = backfill_coverage(&next, &local);
        assert!(none.is_empty());
        assert_eq!(again, next);
    }
}
