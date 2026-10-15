//! Changing a project's gateway language (tC4 `gatewayChange.ts`, D23a, D30.2, STORAGE
//! §5.8).
//!
//! Partial coverage needs no change: the ladder resolves per (tool, book), so a project
//! pinned Spanish then English uses Spanish where it covers a book and English elsewhere.
//! Only an explicit whole-project change moves which resource a checked book uses, so its
//! consequences are counted here, before it is committed, and the person can decline.
//! The change moves `primary` (and the source panes, when given); the English fallback
//! never changes here. A book with stored decisions that neither rung would answer after
//! the change blocks it.
//!
//! # How the stored records map to the input
//!
//! [`StoredDecisions`] is one (tool, unit) of `Check` records (STORAGE §2.2): `tool` from
//! the `CheckKey`'s tool, `unit` from its target (a book, or the stories), and one
//! [`ResourceRef`] per decision from its `DecisionRecord::resource`. `MovedTo` records
//! are not decisions and are left out.

use tc_core::BookId;

use crate::gateways::{self, Gateway, GatewayBibles};
use crate::pin::ResourcePin;
use crate::resolve::{CheckUnit, LocalCoverage, ResourceRef, covers, record_matches, resolve_unit};
use crate::sets::{LanguageSet, PaneId, ProjectKind, ProjectPins, ScripturePane, SetSlot, Tool};
use crate::suite;

/// The stored decisions of one (tool, unit), each by the resource it was made against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredDecisions {
    pub tool: Tool,
    pub unit: CheckUnit,
    pub resources: Vec<ResourceRef>,
}

/// One (tool, unit) whose decisions were made against a resource it would no longer
/// resolve to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffectedUnit {
    pub tool: Tool,
    pub unit: CheckUnit,
    /// How many of its decisions would no longer match.
    pub decisions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeConsequences {
    /// Units with decisions that keep resolving to what they were checked against.
    pub unaffected: usize,
    pub affected: Vec<AffectedUnit>,
    /// Decisions made against a resource the change moves away from. Some carry over
    /// (D36); the rest come back as work. None is deleted.
    pub decisions_at_risk: usize,
    /// Affected units that neither rung answers after the change. The change is refused
    /// while any exists: their decisions would match no rung (tC4 review round 7).
    pub uncovered: Vec<(Tool, CheckUnit)>,
}

impl ChangeConsequences {
    /// Nothing at all would be disturbed: commit without ceremony.
    pub fn harmless(&self) -> bool {
        self.affected.is_empty()
    }

    pub fn blocked(&self) -> bool {
        !self.uncovered.is_empty()
    }
}

/// What the change to `next` would cost (tC4 `consequencesOfGatewayChange` and
/// `uncoveredByChange`). A unit is affected when a decision was made against a resource
/// that is not what the unit resolves to after the change; membership in the new pins is
/// not enough, because a book the new primary covers moves to it even when the old
/// resource stays pinned as the fallback.
pub fn consequences_of_gateway_change(
    stored: &[StoredDecisions],
    next: &ProjectPins,
    local: &dyn LocalCoverage,
) -> ChangeConsequences {
    let mut out = ChangeConsequences {
        unaffected: 0,
        affected: Vec::new(),
        decisions_at_risk: 0,
        uncovered: Vec::new(),
    };
    for entry in stored.iter().filter(|e| !e.resources.is_empty()) {
        let after = resolve_unit(next, entry.tool, entry.unit, local);
        let moved = entry
            .resources
            .iter()
            .filter(|r| !record_matches(Some(r), after.as_ref()))
            .count();
        if moved == 0 {
            out.unaffected += 1;
            continue;
        }
        out.decisions_at_risk += moved;
        out.affected.push(AffectedUnit {
            tool: entry.tool,
            unit: entry.unit,
            decisions: moved,
        });
        if after.is_none() {
            out.uncovered.push((entry.tool, entry.unit));
        }
    }
    out
}

/// Pins with both language sets (D17): a missing `fallback` is the bundled English suite,
/// and a missing `primary` starts equal to it (tC4 `completeLanguageSets`, #485).
pub fn complete_language_sets(pins: &ProjectPins) -> ProjectPins {
    let mut next = pins.clone();
    let english = suite::english_helps();
    if next.fallback.is_none() {
        next.fallback = Some(english.clone());
    }
    if next.primary.is_none() {
        next.primary = Some(english.clone());
    }
    next
}

/// The pins after the change: `primary` moves, and the panes when given. The fallback
/// never changes here (D30.2).
pub fn apply_gateway_change(
    pins: &ProjectPins,
    primary: LanguageSet,
    panes: Option<Vec<ScripturePane>>,
) -> ProjectPins {
    let mut next = complete_language_sets(pins);
    next.primary = Some(primary);
    if let Some(panes) = panes {
        next.extra_scripture = panes;
    }
    next
}

const LITERAL_PANES: [&str; 2] = ["ult", "glt"];
const SIMPLIFIED_PANES: [&str; 2] = ["ust", "gst"];

fn pane_id(id: &str) -> PaneId {
    match id.parse() {
        Ok(id) => id,
        // The four gateway pane ids are valid pane ids.
        Err(_) => unreachable!("pane id {id} must parse"),
    }
}

/// The pane a view reads as the gateway text: the literal pane, else the first.
pub fn literal_pane_id(ids: &[PaneId]) -> Option<&PaneId> {
    ids.iter()
        .find(|id| LITERAL_PANES.contains(&id.as_str()))
        .or_else(|| ids.first())
}

/// A Bible project's source panes after a change (tC4 `sourcePanesForGateway`, #412,
/// #704): the literal pane takes the package's literal Bible and the simplified pane its
/// simplified Bible, and a pane the package cannot fill takes the English one. A pane is
/// named by its repository's suffix (`es-419_glt` is `glt`); a Bible with another name
/// takes `glt` or `gst`, so a later change still finds its pane. A pane that already pins
/// the chosen repository keeps its pin; panes that are not gateway panes stay; no pane is
/// added that the project does not have.
pub fn source_panes_for_gateway(
    current: &[ScripturePane],
    bibles: &GatewayBibles,
) -> Vec<ScripturePane> {
    let (english_literal, english_simplified) = suite::english_bibles();
    let pane = |ids: [&str; 2], pin: &ResourcePin| -> Option<ScripturePane> {
        let was = current.iter().find(|p| ids.contains(&p.id.as_str()))?;
        if was.pin.repo == pin.repo {
            return Some(was.clone());
        }
        let name = pin.repo.name().as_str().to_ascii_lowercase();
        let suffix = name.rsplit('_').next().unwrap_or_default();
        let id = if ids.contains(&suffix) {
            suffix
        } else {
            ids[1]
        };
        Some(ScripturePane {
            id: pane_id(id),
            pin: pin.clone(),
        })
    };
    let literal = pane(
        LITERAL_PANES,
        bibles.literal.as_ref().unwrap_or(english_literal),
    );
    let simplified = pane(
        SIMPLIFIED_PANES,
        bibles.simplified.as_ref().unwrap_or(english_simplified),
    );
    let others = current.iter().filter(|p| {
        !LITERAL_PANES.contains(&p.id.as_str()) && !SIMPLIFIED_PANES.contains(&p.id.as_str())
    });
    literal
        .into_iter()
        .chain(simplified)
        .chain(others.cloned())
        .collect()
}

/// The gateway packages a project can change to (tC4 `gatewaysCoveringProject`): every
/// complete package installed here that covers one of the project's books by its notes
/// or its words links. With no books (or for stories), every complete package.
pub fn gateways_covering_project(
    installed: &[ResourcePin],
    local: &dyn LocalCoverage,
    books: &[BookId],
    kind: ProjectKind,
) -> Vec<&'static Gateway> {
    gateways::GATEWAYS
        .iter()
        .filter(|g| {
            let Some(set) = gateways::language_set_from_installed(installed, g, kind) else {
                return false;
            };
            if kind == ProjectKind::Obs || books.is_empty() {
                return true;
            }
            [SetSlot::TranslationNotes, SetSlot::TranslationWordsLinks]
                .iter()
                .filter_map(|slot| set.pin(*slot))
                .any(|pin| books.iter().any(|b| covers(pin, *b, local)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! tC4 `test/gatewayChange.test.ts` and `test/gatewaySourcePanes.test.ts`, ported.

    use std::collections::HashMap;

    use super::*;
    use crate::pin::BookCoverage;
    use crate::resolve::RecordedOnly;
    use crate::sets::GatewayLanguage;

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
            sha40(&format!("{repo}@{version}")).parse().unwrap(),
        )
        .with_version(version.parse().unwrap())
    }

    fn set(lang: &str, org: &str, v: &str) -> LanguageSet {
        LanguageSet::new(GatewayLanguage {
            language: lang.to_owned(),
            owner: org.parse().unwrap(),
        })
        .with(
            SetSlot::TranslationNotes,
            pin(&format!("{org}/{lang}_tn"), v),
        )
        .with(
            SetSlot::TranslationWordsLinks,
            pin(&format!("{org}/{lang}_twl"), v),
        )
        .with(
            SetSlot::TranslationWords,
            pin(&format!("{org}/{lang}_tw"), v),
        )
        .with(
            SetSlot::TranslationAcademy,
            pin(&format!("{org}/{lang}_ta"), v),
        )
    }

    fn es() -> LanguageSet {
        set("es-419", "Es-419_gl", "v66")
    }
    fn en() -> LanguageSet {
        set("en", "unfoldingWord", "v89")
    }
    fn fr() -> LanguageSet {
        set("fr", "Xenizo", "v2.0")
    }

    fn pins(primary: LanguageSet, fallback: LanguageSet) -> ProjectPins {
        ProjectPins {
            primary: Some(primary),
            fallback: Some(fallback),
            ..ProjectPins::default()
        }
    }

    fn coverage(
        entries: &[(&ResourcePin, &[&str])],
    ) -> impl Fn(&ResourcePin) -> Option<BookCoverage> + use<> {
        let map: HashMap<String, BookCoverage> = entries
            .iter()
            .map(|(p, books)| {
                (
                    format!("{}@{}", p.repo.to_string().to_lowercase(), p.sha),
                    BookCoverage::from_codes(books).unwrap(),
                )
            })
            .collect();
        move |p: &ResourcePin| {
            map.get(&format!("{}@{}", p.repo.to_string().to_lowercase(), p.sha))
                .cloned()
        }
    }

    fn stored(book: BookId, repo: &str, version: &str, decisions: usize) -> StoredDecisions {
        let r = ResourceRef::from(&pin(repo, version));
        StoredDecisions {
            tool: Tool::TranslationNotes,
            unit: CheckUnit::Book(book),
            resources: vec![r; decisions],
        }
    }

    fn cov() -> impl Fn(&ResourcePin) -> Option<BookCoverage> {
        let (fr, en) = (fr(), en());
        coverage(&[
            (fr.pin(SetSlot::TranslationNotes).unwrap(), &["TIT", "JON"]),
            (
                fr.pin(SetSlot::TranslationWordsLinks).unwrap(),
                &["TIT", "JON"],
            ),
            (
                en.pin(SetSlot::TranslationNotes).unwrap(),
                &["TIT", "JON", "HEB", "PSA", "RUT"],
            ),
            (en.pin(SetSlot::TranslationWordsLinks).unwrap(), &["BIBLE"]),
        ])
    }

    #[test]
    fn a_change_that_disturbs_nothing_is_harmless() {
        let c = consequences_of_gateway_change(
            &[stored(BookId::Titus, "unfoldingWord/en_tn", "v89", 12)],
            &pins(en(), en()),
            &cov(),
        );
        assert!(c.harmless() && !c.blocked());
        assert_eq!(c.decisions_at_risk, 0);
    }

    #[test]
    fn decisions_that_would_need_review_are_counted_per_book() {
        let c = consequences_of_gateway_change(
            &[
                stored(BookId::Titus, "Es-419_gl/es-419_tn", "v66", 30),
                stored(BookId::Jonah, "Es-419_gl/es-419_tn", "v66", 17),
            ],
            &pins(fr(), en()),
            &cov(),
        );
        assert!(!c.harmless());
        let books: Vec<CheckUnit> = c.affected.iter().map(|a| a.unit).collect();
        assert_eq!(
            books,
            [
                CheckUnit::Book(BookId::Titus),
                CheckUnit::Book(BookId::Jonah)
            ]
        );
        assert_eq!(c.decisions_at_risk, 47);
    }

    #[test]
    fn a_book_the_new_primary_does_not_cover_keeps_its_fallback() {
        let c = consequences_of_gateway_change(
            &[stored(BookId::Hebrews, "unfoldingWord/en_tn", "v89", 25)],
            &pins(fr(), en()),
            &cov(),
        );
        assert!(c.harmless());
        assert_eq!(c.unaffected, 1);
    }

    #[test]
    fn no_decisions_cost_nothing() {
        let c = consequences_of_gateway_change(
            &[stored(BookId::Titus, "Es-419_gl/es-419_tn", "v66", 0)],
            &pins(fr(), en()),
            &cov(),
        );
        assert!(c.harmless());
        assert_eq!(c.unaffected, 0);
    }

    #[test]
    fn an_affected_book_neither_rung_covers_blocks_the_change() {
        let fr = fr();
        let local = coverage(&[(fr.pin(SetSlot::TranslationNotes).unwrap(), &["TIT"])]);
        let c = consequences_of_gateway_change(
            &[
                stored(BookId::Titus, "Es-419_gl/es-419_tn", "v66", 3),
                stored(BookId::Ruth, "Es-419_gl/es-419_tn", "v66", 2),
            ],
            &pins(fr.clone(), fr),
            &local,
        );
        assert!(c.blocked());
        assert_eq!(
            c.uncovered,
            [(Tool::TranslationNotes, CheckUnit::Book(BookId::Ruth))]
        );
    }

    #[test]
    fn stories_are_resolved_by_presence_and_an_absent_primary_member_falls_back() {
        let obs = |base: LanguageSet, token: &str| {
            let owner = base.gateway.owner.to_string();
            let lang = base.gateway.language.clone();
            base.with(
                SetSlot::ObsNotes,
                pin(&format!("{owner}/{lang}_obs-tn"), token),
            )
        };
        let old = obs(es(), "old");
        let next = obs(fr(), "next");
        let fallback = obs(en(), "fallback");
        let prior = ResourceRef::from(old.pin(SetSlot::ObsNotes).unwrap());
        let decisions = [StoredDecisions {
            tool: Tool::TranslationNotes,
            unit: CheckUnit::Stories,
            resources: vec![prior; 3],
        }];
        let c = consequences_of_gateway_change(
            &decisions,
            &pins(next.clone(), fallback.clone()),
            &RecordedOnly,
        );
        assert_eq!(c.decisions_at_risk, 3);
        assert!(!c.blocked());
        let mut no_notes = next.clone();
        no_notes.pins.remove(&SetSlot::ObsNotes);
        let mut fallback_no_notes = fallback.clone();
        fallback_no_notes.pins.remove(&SetSlot::ObsNotes);
        assert!(
            !consequences_of_gateway_change(
                &decisions,
                &pins(no_notes.clone(), fallback),
                &RecordedOnly
            )
            .blocked()
        );
        assert!(
            consequences_of_gateway_change(
                &decisions,
                &pins(no_notes, fallback_no_notes),
                &RecordedOnly
            )
            .blocked()
        );
    }

    #[test]
    fn missing_sets_are_filled_from_the_bundled_suite() {
        let full = complete_language_sets(&ProjectPins::default());
        assert_eq!(full.primary.as_ref(), Some(suite::english_helps()));
        assert_eq!(full.fallback.as_ref(), Some(suite::english_helps()));
        let only_primary = ProjectPins {
            primary: Some(es()),
            ..ProjectPins::default()
        };
        let full = complete_language_sets(&only_primary);
        assert_eq!(full.primary, Some(es()));
        assert_eq!(full.fallback.as_ref(), Some(suite::english_helps()));
        let next = apply_gateway_change(&ProjectPins::default(), es(), None);
        assert_eq!(next.primary, Some(es()));
        assert_eq!(next.fallback.as_ref(), Some(suite::english_helps()));
    }

    fn real(repo: &str, version: Option<&str>, sha: &str) -> ResourcePin {
        let p = ResourcePin::new(
            format!("git.door43.org/{repo}").parse().unwrap(),
            sha.parse().unwrap(),
        )
        .with_flavor("scripture/textTranslation");
        match version {
            Some(v) => p.with_version(v.parse().unwrap()),
            None => p,
        }
    }

    fn es_glt() -> ResourcePin {
        real(
            "es-419_gl/es-419_glt",
            Some("v42"),
            "3f9bf7e8806f2e310601d723eb201334fe3be1ab",
        )
    }
    fn es_gst() -> ResourcePin {
        real(
            "es-419_gl/es-419_gst",
            Some("v40"),
            "608e2294aa56938592ef592b0fc391d6d6178b2f",
        )
    }
    fn ar_avd() -> ResourcePin {
        real(
            "BSOJ/ar_avd",
            None,
            "3ba2e7f1d2fb3c5b9c9682dcc20d09b6f3770c06",
        )
    }

    fn ids(panes: &[ScripturePane]) -> Vec<(&str, &str)> {
        panes
            .iter()
            .map(|p| (p.id.as_str(), p.pin.repo.name().as_str()))
            .collect()
    }

    #[test]
    fn panes_move_to_the_packages_bibles_named_by_suffix() {
        let current = suite::project_pins().extra_scripture;
        let both = GatewayBibles {
            literal: Some(es_glt()),
            simplified: Some(es_gst()),
        };
        let spanish = source_panes_for_gateway(&current, &both);
        assert_eq!(
            ids(&spanish),
            [("glt", "es-419_glt"), ("gst", "es-419_gst")]
        );
        // A package with no Bible: both panes show the English ULT and UST (Q5).
        assert_eq!(
            source_panes_for_gateway(&spanish, &GatewayBibles::default()),
            current
        );
        // Only a literal Bible: the simplified pane shows the English UST.
        let literal_only = GatewayBibles {
            literal: Some(es_glt()),
            simplified: None,
        };
        assert_eq!(
            ids(&source_panes_for_gateway(&current, &literal_only)),
            [("glt", "es-419_glt"), ("ust", "en_ust")]
        );
    }

    #[test]
    fn a_literal_bible_with_another_name_fills_the_glt_pane() {
        let current = suite::project_pins().extra_scripture;
        let arabic = source_panes_for_gateway(
            &current,
            &GatewayBibles {
                literal: Some(ar_avd()),
                simplified: None,
            },
        );
        assert_eq!(ids(&arabic), [("glt", "ar_avd"), ("ust", "en_ust")]);
        assert_eq!(
            source_panes_for_gateway(&arabic, &GatewayBibles::default()),
            current
        );
        let order: Vec<PaneId> = ["ust", "glt"].iter().map(|s| s.parse().unwrap()).collect();
        assert_eq!(literal_pane_id(&order).unwrap().as_str(), "glt");
        let kjv: Vec<PaneId> = vec!["kjv".parse().unwrap()];
        assert_eq!(literal_pane_id(&kjv).unwrap().as_str(), "kjv");
        assert!(literal_pane_id(&[]).is_none());
    }

    #[test]
    fn a_pane_already_on_the_chosen_repository_keeps_its_pin_and_others_stay() {
        let mut newer = suite::project_pins().extra_scripture;
        newer[0].pin = real("unfoldingWord/en_ult", Some("v90"), &"a".repeat(40));
        let (ult, _) = suite::english_bibles();
        let kept = source_panes_for_gateway(
            &newer,
            &GatewayBibles {
                literal: Some(ult.clone()),
                simplified: None,
            },
        );
        assert_eq!(kept, newer);
        let own = ScripturePane {
            id: "kjv".parse().unwrap(),
            pin: real("someone/en_kjv", None, &"b".repeat(40)),
        };
        let current = vec![
            suite::project_pins().extra_scripture[0].clone(),
            own.clone(),
        ];
        let both = GatewayBibles {
            literal: Some(es_glt()),
            simplified: Some(es_gst()),
        };
        assert_eq!(
            ids(&source_panes_for_gateway(&current, &both)),
            [("glt", "es-419_glt"), ("kjv", "en_kjv")]
        );
        assert!(source_panes_for_gateway(&[], &both).is_empty());
    }

    #[test]
    fn the_change_writes_the_panes_when_given_and_keeps_the_fallback() {
        let start = suite::project_pins();
        let primary = LanguageSet {
            gateway: GatewayLanguage {
                language: "es-419".to_owned(),
                owner: "es-419_gl".parse().unwrap(),
            },
            ..suite::english_helps().clone()
        };
        let both = GatewayBibles {
            literal: Some(es_glt()),
            simplified: Some(es_gst()),
        };
        let panes = source_panes_for_gateway(&start.extra_scripture, &both);
        let next = apply_gateway_change(&start, primary.clone(), Some(panes.clone()));
        assert_eq!(next.extra_scripture, panes);
        assert_eq!(next.fallback, start.fallback);
        assert_eq!(
            apply_gateway_change(&start, primary, None).extra_scripture,
            start.extra_scripture
        );
    }

    #[test]
    fn packages_are_offered_by_the_books_they_cover() {
        let esp = |name: &str, v: &str, sha: &str, flavor: &str| {
            ResourcePin::new(
                format!("git.door43.org/es-419_gl/{name}").parse().unwrap(),
                sha.parse().unwrap(),
            )
            .with_version(v.parse().unwrap())
            .with_flavor(flavor)
        };
        let es_tn = esp(
            "es-419_tn",
            "v66",
            "22f3d0c61e2ab4701cb869547de9c3c43da07208",
            "parascriptural/x-bcvnotes",
        );
        let es_twl = esp(
            "es-419_twl",
            "v18",
            "c2d9547ec95e40c2ba085c597b127c94733feb8a",
            "parascriptural/x-bcvarticles",
        );
        let es_tw = esp(
            "es-419_tw",
            "v37",
            "7586f4ff1f0483ea40a4a68e5e1f33158e08c208",
            "parascriptural/x-bcvarticles",
        );
        let es_ta = esp(
            "es-419_ta",
            "v4",
            "26606b578c37cc2c0ee09bb7b9a291860ff59444",
            "peripheral/x-peripheralArticles",
        );
        let en = suite::english_helps();
        let en_slots = [
            SetSlot::TranslationNotes,
            SetSlot::TranslationWordsLinks,
            SetSlot::TranslationWords,
            SetSlot::TranslationAcademy,
        ];
        let mut installed: Vec<ResourcePin> = en_slots
            .iter()
            .map(|s| en.pin(*s).unwrap().clone())
            .collect();
        installed.extend([es_tn.clone(), es_twl.clone(), es_tw.clone(), es_ta.clone()]);
        let keys =
            |list: Vec<&Gateway>| list.iter().map(|g| g.key().to_string()).collect::<Vec<_>>();
        let en_tn = en.pin(SetSlot::TranslationNotes).unwrap();
        let en_twl = en.pin(SetSlot::TranslationWordsLinks).unwrap();
        let local = coverage(&[
            (en_tn, &["TIT", "PHM"]),
            (en_twl, &["TIT", "PHM"]),
            (&es_tn, &["TIT"]),
            (&es_twl, &["TIT"]),
        ]);
        assert_eq!(
            keys(gateways_covering_project(
                &installed,
                &local,
                &[BookId::Titus, BookId::Jonah],
                ProjectKind::Bible
            )),
            ["en::unfoldingWord", "es-419::es-419_gl"]
        );
        assert_eq!(
            keys(gateways_covering_project(
                &installed,
                &local,
                &[BookId::Philemon],
                ProjectKind::Bible
            )),
            ["en::unfoldingWord"]
        );
        let whole = coverage(&[(&es_tn, &["TIT"]), (&es_twl, &["BIBLE"])]);
        assert!(
            keys(gateways_covering_project(
                &installed,
                &whole,
                &[BookId::Genesis],
                ProjectKind::Bible
            ))
            .contains(&"es-419::es-419_gl".to_owned())
        );
        let links_only = coverage(&[(&es_tn, &["TIT"]), (&es_twl, &["TIT", "PHM"])]);
        assert!(
            keys(gateways_covering_project(
                &installed,
                &links_only,
                &[BookId::Philemon],
                ProjectKind::Bible
            ))
            .contains(&"es-419::es-419_gl".to_owned())
        );
        let no_ta: Vec<ResourcePin> = installed.iter().filter(|p| **p != es_ta).cloned().collect();
        assert_eq!(
            keys(gateways_covering_project(
                &no_ta,
                &RecordedOnly,
                &[],
                ProjectKind::Bible
            )),
            ["en::unfoldingWord"]
        );
        assert_eq!(
            keys(gateways_covering_project(
                &installed,
                &RecordedOnly,
                &[],
                ProjectKind::Bible
            )),
            ["en::unfoldingWord", "es-419::es-419_gl"]
        );
    }
}
