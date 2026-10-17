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
//! The ladder itself is the journal's (`tc_journal::pins::Ladder`, which [`ProjectPins`]
//! implements), so the fold, the Check screens and this crate resolve with one
//! implementation (STORAGE §7.7). A rung answers a book when its pin covers the book: the
//! pin's own recorded `books`, else here the books its installed commit holds
//! ([`LocalCoverage`], tC4 `coverageFor`). The fold reads no installed copy: there a pin
//! that records nothing covers every book (`Ladder::resolution`). A slot whose value is not
//! a usable pin, such as a pin with a tag and no commit, stops the ladder for the books it
//! records ([`Resolved::Unusable`]).

use tc_core::BookId;
use tc_door43::RepoRef;
use tc_journal::keys::ToolId;
use tc_journal::pins::{Answer, Collection, Coverage, Ladder, Unrecorded, UnusablePin};
use tc_journal::record::ResourceRef;

use crate::pin::{BookCoverage, ResourcePin};
use crate::sets::{ProjectPins, Rung, SetSlot};

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

/// The installed coverage of a resource as the journal's ladder asks for it.
fn installed_coverage(local: &dyn LocalCoverage, resource: &ResourceRef) -> Option<Coverage> {
    let repo = resource.repo_path.parse().ok()?;
    local
        .local_books(&ResourcePin::new(repo, resource.sha))
        .map(|books| books.coverage())
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

/// Whether `pin` covers `book` on this computer, by the journal's rule
/// (`tc_journal::pins::Pin::coverage_with`, tC4 `coverageFor`).
pub fn covers(pin: &ResourcePin, book: BookId, local: &dyn LocalCoverage) -> bool {
    pin.journal_pin()
        .coverage_with(&|r: &ResourceRef| installed_coverage(local, r))
        .covers(book)
}

/// The rung and pin that answer a (slot, collection).
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
}

/// Where the ladder stops for a (slot, collection) on this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// A usable pin answers.
    Pin(Resolution),
    /// A slot value that is not a usable pin answers, such as a pin with a tag and no
    /// commit. The next rung is not used in its place: that would check the book against
    /// another language's list without saying so. No install satisfies it, and no
    /// decision matches it, until the pins are repaired.
    Unusable { rung: Rung, pin: UnusablePin },
}

impl Resolved {
    pub fn rung(&self) -> Rung {
        match self {
            Self::Pin(resolution) => resolution.rung,
            Self::Unusable { rung, .. } => *rung,
        }
    }

    /// The usable resolution, if the ladder stopped at one.
    pub fn usable(self) -> Option<Resolution> {
        match self {
            Self::Pin(resolution) => Some(resolution),
            Self::Unusable { .. } => None,
        }
    }
}

/// Resolves one language-set slot for `collection` over the ladder (tC4 `resolveSetSlot`
/// and, for the stories, `resolveObsSetSlot`), reading a pin that records no books with the
/// books of its installed commit. An absent slot covers nothing; the rung is skipped (D64).
pub fn resolve_slot(
    pins: &ProjectPins,
    slot: SetSlot,
    collection: Collection,
    local: &dyn LocalCoverage,
) -> Option<Resolved> {
    let installed = |r: &ResourceRef| installed_coverage(local, r);
    match pins.answer_slot(slot, collection, Unrecorded::Installed(&installed))? {
        Answer::Pin { rung, .. } => {
            let pin = pins.set(rung)?.readable_pin(slot)?;
            Some(Resolved::Pin(Resolution { rung, pin }))
        }
        Answer::Unusable { rung, pin } => Some(Resolved::Unusable { rung, pin }),
    }
}

/// Resolves a checking tool's list for a collection (tC4 `resolveToolBook`).
pub fn resolve_tool(
    pins: &ProjectPins,
    tool: ToolId,
    collection: Collection,
    local: &dyn LocalCoverage,
) -> Option<Resolved> {
    resolve_slot(pins, SetSlot::of_tool(tool, collection), collection, local)
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
    /// The rung that answers holds a value that is not a usable pin (no commit): nothing
    /// can be installed for it until the pins are repaired.
    UnusablePin,
    /// The words-links pin is the pre-release one-repository form (D97 ruling 6): it is
    /// never read, and the project must be created again.
    OldTwl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preflight {
    pub tool: ToolId,
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
    tool: ToolId,
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
    if tool == ToolId::TranslationWords
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
    let slot = SetSlot::of_tool(tool, Collection::Book(book));
    match resolve_tool(pins, tool, Collection::Book(book), local) {
        Some(Resolved::Pin(resolution)) => {
            let missing = missing_pin(pins, tool, &resolution, installed);
            out.state = missing.as_ref().map_or(PreflightState::Ready, availability);
            out.needs = missing;
            let primary = pins.set(Rung::Primary).and_then(|set| set.pin(slot));
            out.unavailable_primary = primary
                .filter(|_| resolution.used_fallback())
                .filter(|pin| coverage_for(pin, local).1 == CoverageSource::Unknown)
                .filter(|pin| !installed(pin))
                .cloned();
            out.resolution = Some(resolution);
            return out;
        }
        Some(Resolved::Unusable { .. }) => {
            out.state = PreflightState::UnusablePin;
            return out;
        }
        None => {}
    }
    let pinned: Vec<&ResourcePin> = Rung::LADDER
        .into_iter()
        .filter_map(|rung| pins.set(rung)?.pin(slot))
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
    tool: ToolId,
    resolution: &Resolution,
    installed: &dyn Fn(&ResourcePin) -> bool,
) -> Option<ResourcePin> {
    if !installed(&resolution.pin) {
        return Some(resolution.pin.clone());
    }
    if tool != ToolId::TranslationWords {
        return None;
    }
    pins.set(resolution.rung)?
        .readable_pin(SetSlot::TranslationWords)
        .filter(|tw| !installed(tw))
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
    use tc_journal::pins::Answer;

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

    fn nothing_installed(_: &ResourcePin) -> Option<BookCoverage> {
        None
    }

    fn book_resolution(
        pins: &ProjectPins,
        tool: ToolId,
        book: BookId,
        local: &dyn LocalCoverage,
    ) -> Option<Resolution> {
        resolve_tool(pins, tool, Collection::Book(book), local)?.usable()
    }

    #[test]
    fn d30_1_resolution_is_per_tool_and_book() {
        let cov = coverage();
        let r = resources();
        let tit = book_resolution(&r, ToolId::TranslationNotes, BookId::Titus, &cov).unwrap();
        let heb = book_resolution(&r, ToolId::TranslationNotes, BookId::Hebrews, &cov).unwrap();
        assert_eq!(tit.rung, Rung::Primary);
        assert_eq!(heb.rung, Rung::Fallback);
        assert!(heb.used_fallback());
    }

    #[test]
    fn d30_2_neither_rung_covers_the_book() {
        let cov = coverage();
        assert!(
            book_resolution(
                &resources(),
                ToolId::TranslationNotes,
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
            preflight_tool_book(
                pins,
                ToolId::TranslationNotes,
                book,
                local,
                installed,
                online,
            )
            .state
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
            state(
                Some(&r),
                BookId::Revelation,
                &nothing_installed,
                &none,
                false
            ),
            PreflightState::Unavailable
        );
        assert_eq!(
            state(
                Some(&r),
                BookId::Revelation,
                &nothing_installed,
                &none,
                true
            ),
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
            ToolId::TranslationWords,
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
            ToolId::TranslationNotes,
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
            ToolId::TranslationNotes,
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
            ToolId::TranslationWords,
            BookId::Titus,
            &nothing_installed,
            &|_| true,
            true,
        );
        assert_eq!(pre.state, PreflightState::OldTwl);
        let notes = preflight_tool_book(
            Some(&r),
            ToolId::TranslationNotes,
            BookId::Titus,
            &coverage(),
            &|_| true,
            true,
        );
        assert_eq!(notes.state, PreflightState::Ready);
    }

    #[test]
    fn d17_a_stored_record_is_compared_by_repository_and_sha() {
        let r = resources();
        let now = r.answer(
            ToolId::TranslationNotes,
            Collection::Book(BookId::Titus),
            Unrecorded::Installed(&|res: &ResourceRef| installed_coverage(&coverage(), res)),
        );
        let rec = |repo: &str, sha_of: &str| ResourceRef {
            repo_path: format!("git.door43.org/{repo}").into(),
            sha: sha40(sha_of).parse().unwrap(),
        };
        let ok = |record: &ResourceRef| tc_journal::pins::resource_ok(record, now.as_ref());
        assert!(ok(&rec("Es-419_gl/es-419_tn", "es-419_gl/es-419_tn@v66")));
        assert!(ok(&rec("es-419_GL/ES-419_TN", "es-419_gl/es-419_tn@v66")));
        assert!(!ok(&rec("Es-419_gl/es-419_tn", "es-419_gl/es-419_tn@v67")));
        assert!(!ok(&rec("unfoldingWord/en_tn", "unfoldingword/en_tn@v86")));
        assert_eq!(now.unwrap().version(), Some("v66"));
    }

    #[test]
    fn the_fold_reads_the_pins_alone_and_an_unrecorded_pin_covers_every_book() {
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
        let fold = |pins: &ProjectPins, book| {
            pins.resolution(ToolId::TranslationNotes, Collection::Book(book))
        };
        // Nothing records coverage: in the fold the primary covers every book, whatever is
        // installed, so every device computes the same answer (STORAGE §7.7).
        let tit = fold(&r, BookId::Titus).unwrap();
        assert_eq!(tit.resource(), Some(&es_tn.resource()));
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
        assert_eq!(
            fold(&r, BookId::Titus).unwrap().resource(),
            Some(&es_tn.resource())
        );
        assert_eq!(
            fold(&r, BookId::Ruth).unwrap().resource(),
            Some(&en_tn.resource())
        );
    }

    #[test]
    fn a_pin_with_no_commit_is_kept_and_stops_the_ladder_for_its_books() {
        let mut r = resources();
        let mut json = r.group_json(crate::PinGroup::Primary).unwrap();
        let tag_only = serde_json::json!({
            "repoPath": "git.door43.org/Es-419_gl/es-419_tn",
            "version": "v66",
            "flavor": "parascriptural/x-bcvnotes",
            "books": ["TIT"]
        });
        json["translationNotes"] = tag_only.clone();
        r.set_group_json(crate::PinGroup::Primary, &json).unwrap();
        let primary = r.primary.as_ref().unwrap();
        assert!(primary.pin(SetSlot::TranslationNotes).is_none());
        assert_eq!(
            primary.unreadable.get(&SetSlot::TranslationNotes),
            Some(&tag_only)
        );
        // Writing the set back keeps the slot as it was read, in its place.
        assert_eq!(primary.to_json().unwrap(), json);
        // Titus: the primary answers with an unusable pin; the fallback is not used.
        let tit = resolve_tool(
            &r,
            ToolId::TranslationNotes,
            Collection::Book(BookId::Titus),
            &coverage(),
        )
        .unwrap();
        assert!(matches!(
            tit,
            Resolved::Unusable {
                rung: Rung::Primary,
                ..
            }
        ));
        assert!(matches!(
            r.resolution(ToolId::TranslationNotes, Collection::Book(BookId::Titus)),
            Some(Answer::Unusable { .. })
        ));
        let pre = preflight_tool_book(
            Some(&r),
            ToolId::TranslationNotes,
            BookId::Titus,
            &coverage(),
            &|_| true,
            true,
        );
        assert_eq!(pre.state, PreflightState::UnusablePin);
        // A book it does not record falls to English.
        let heb = book_resolution(&r, ToolId::TranslationNotes, BookId::Hebrews, &coverage());
        assert_eq!(heb.unwrap().rung, Rung::Fallback);
    }

    #[test]
    fn a_pin_not_in_tc4s_form_is_kept_as_written_and_still_read() {
        let mut r = resources();
        let mut json = r.group_json(crate::PinGroup::Fallback).unwrap();
        let mut lower = json["translationNotes"].clone();
        lower["books"] = serde_json::json!(["tit"]);
        json["translationNotes"] = lower.clone();
        r.set_group_json(crate::PinGroup::Fallback, &json).unwrap();
        let fallback = r.fallback.as_ref().unwrap();
        assert!(fallback.pin(SetSlot::TranslationNotes).is_none());
        assert_eq!(fallback.to_json().unwrap(), json, "nothing is rewritten");
        // The ladder reads it as tC4's coverageFor does: the code in upper case.
        let view = fallback.readable_pin(SetSlot::TranslationNotes).unwrap();
        assert!(view.books.as_ref().unwrap().covers(BookId::Titus));
        // With no Spanish notes, Titus resolves to it; Hebrews, which its record leaves
        // out, to nothing, though the installed copy holds it: a record wins.
        r.primary
            .as_mut()
            .unwrap()
            .pins
            .remove(&SetSlot::TranslationNotes);
        let tit = book_resolution(&r, ToolId::TranslationNotes, BookId::Titus, &coverage());
        assert_eq!(
            tit.unwrap(),
            Resolution {
                rung: Rung::Fallback,
                pin: view
            }
        );
        assert_eq!(
            book_resolution(&r, ToolId::TranslationNotes, BookId::Hebrews, &coverage()),
            None
        );
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
