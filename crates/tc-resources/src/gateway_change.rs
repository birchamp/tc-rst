//! Changing a project's gateway language (tC4 `gatewayChange.ts`, D23a, D30.2, STORAGE
//! §5.8).
//!
//! Partial coverage needs no change: the ladder resolves per (tool, book), so a project
//! pinned Spanish then English uses Spanish where it covers a book and English elsewhere.
//! Only an explicit whole-project change moves which resource a checked book uses. What it
//! would cost (the decisions it would put back to work, and the books it would leave with no
//! rung) is counted before it is committed by `tc-check`'s `change` module, over the
//! journal's ladder; this module makes the pin edits. The change moves `primary` (and the
//! source panes, when given); the English fallback never changes here.

use tc_core::BookId;

use crate::gateways::{self, Gateway, GatewayBibles};
use crate::pin::ResourcePin;
use crate::resolve::{LocalCoverage, covers};
use crate::sets::{LanguageSet, PaneId, ProjectKind, ProjectPins, ScripturePane, SetSlot};
use crate::suite;

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

/// The optional pins a download of `gateway`'s package adds to the project's sets (tC4
/// `mergeOptionalPins`, `adoptDownloadedPins`): the questions, the simplified Bible, the Open
/// Bible Stories members and the image pack that this computer holds for the gateway, in each
/// of the project's sets that names that gateway and does not pin the slot. A pin already in
/// a set never moves, and the three story members enter a set together or not at all (a
/// partial download must not make a partial story set). They are found apart from the
/// required suite, because a person may download only the questions for a project whose set
/// is pinned. `None` when nothing changes.
pub fn merge_optional_pins(
    pins: &ProjectPins,
    gateway: &Gateway,
    installed: &[ResourcePin],
) -> Option<ProjectPins> {
    let pick = |slot: SetSlot| {
        gateway
            .repo_for(slot)
            .and_then(|repo| gateways::preferred(installed, &repo))
            .cloned()
    };
    let simplified = gateway
        .simplified_candidates()
        .iter()
        .find_map(|repo| gateways::preferred(installed, repo))
        .cloned();
    let owner = gateways::account(gateway.owner);
    let mut packs = installed.iter().filter(|p| {
        p.repo.owner() == &owner
            && p.flavor
                .as_deref()
                .is_some_and(|f| f.ends_with("/x-obsimages"))
    });
    let images = match (packs.next(), packs.next()) {
        (Some(pack), None) => Some(pack.clone()),
        _ => None,
    };
    let (questions, obs, obs_notes, obs_links, obs_questions) = (
        pick(SetSlot::TranslationQuestions),
        pick(SetSlot::Obs),
        pick(SetSlot::ObsNotes),
        pick(SetSlot::ObsWordsLinks),
        pick(SetSlot::ObsQuestions),
    );
    if [&questions, &simplified, &obs, &obs_questions, &images]
        .iter()
        .all(|pin| pin.is_none())
    {
        return None;
    }
    let mut next = pins.clone();
    let mut changed = false;
    for rung in crate::sets::Rung::LADDER {
        let Some(set) = next.set_mut(rung).as_mut() else {
            continue;
        };
        let names_it = set.gateway.named().is_some_and(|named| {
            named.language == gateway.language
                && named.owner.as_str().eq_ignore_ascii_case(gateway.owner)
        });
        if !names_it {
            continue;
        }
        let mut attach = |set: &mut LanguageSet, slot: SetSlot, pin: &Option<ResourcePin>| {
            if let Some(pin) = pin
                && !set.pins.contains_key(&slot)
            {
                set.pins.insert(slot, pin.clone());
                changed = true;
            }
        };
        attach(set, SetSlot::TranslationQuestions, &questions);
        attach(set, SetSlot::SimplifiedText, &simplified);
        let story = [SetSlot::Obs, SetSlot::ObsNotes, SetSlot::ObsWordsLinks];
        if story.iter().all(|slot| !set.pins.contains_key(slot))
            && let (Some(obs), Some(notes), Some(links)) = (&obs, &obs_notes, &obs_links)
        {
            for (slot, pin) in story.iter().zip([obs, notes, links]) {
                attach(set, *slot, &Some(pin.clone()));
            }
        }
        attach(set, SetSlot::ObsQuestions, &obs_questions);
        attach(set, SetSlot::ObsImages, &images);
    }
    changed.then_some(next)
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
        #[expect(
            clippy::unreachable,
            reason = "the four gateway pane ids are valid pane ids"
        )]
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
            }
            .into(),
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
                &|_: &ResourcePin| None,
                &[],
                ProjectKind::Bible
            )),
            ["en::unfoldingWord"]
        );
        assert_eq!(
            keys(gateways_covering_project(
                &installed,
                &|_: &ResourcePin| None,
                &[],
                ProjectKind::Bible
            )),
            ["en::unfoldingWord", "es-419::es-419_gl"]
        );
    }

    // ---- merge_optional_pins (tC4 `adoptDownloadedPins`) ----------------------------------

    fn flavored(repo: &str, version: &str) -> ResourcePin {
        pin(repo, version).with_flavor("parascriptural/x-bcvquestions")
    }

    fn english_sets() -> ProjectPins {
        let en = set("en", "unfoldingWord", "v91");
        ProjectPins {
            primary: Some(en.clone()),
            fallback: Some(en),
            ..ProjectPins::default()
        }
    }

    fn english() -> &'static Gateway {
        gateways::find("en", "unfoldingWord").unwrap()
    }

    #[test]
    fn a_downloaded_questions_resource_joins_each_set_that_names_the_gateway() {
        let installed = [flavored("unfoldingWord/en_tq", "v91")];
        let merged = merge_optional_pins(&english_sets(), english(), &installed).unwrap();
        for rung in [crate::sets::Rung::Primary, crate::sets::Rung::Fallback] {
            let set = merged.set(rung).unwrap();
            assert_eq!(
                set.pin(SetSlot::TranslationQuestions)
                    .unwrap()
                    .repo
                    .repo()
                    .name
                    .as_str(),
                "en_tq"
            );
            // Every pin the set held is as it was.
            let was = english_sets();
            let was = was.set(rung).unwrap();
            for (slot, pin) in &was.pins {
                assert_eq!(set.pin(*slot), Some(pin), "{slot:?}");
            }
        }
    }

    #[test]
    fn a_pin_a_set_already_holds_never_moves() {
        let mut pins = english_sets();
        if let Some(primary) = pins.primary.as_mut() {
            primary.pins.insert(
                SetSlot::TranslationQuestions,
                flavored("unfoldingWord/en_tq", "v90"),
            );
        }
        let installed = [flavored("unfoldingWord/en_tq", "v91")];
        let merged = merge_optional_pins(&pins, english(), &installed).unwrap();
        let held = |p: &ProjectPins| {
            p.primary
                .as_ref()
                .unwrap()
                .pin(SetSlot::TranslationQuestions)
                .unwrap()
                .sha
        };
        assert_eq!(held(&merged), held(&pins), "the primary keeps its v90");
        assert!(
            merged
                .fallback
                .as_ref()
                .unwrap()
                .pin(SetSlot::TranslationQuestions)
                .is_some(),
            "the fallback, which lacked it, gets the installed one"
        );
    }

    #[test]
    fn nothing_is_adopted_by_a_set_of_another_gateway_or_when_nothing_changes() {
        let installed = [flavored("unfoldingWord/en_tq", "v91")];
        let spanish = ProjectPins {
            primary: Some(es()),
            ..ProjectPins::default()
        };
        assert_eq!(merge_optional_pins(&spanish, english(), &installed), None);
        assert_eq!(merge_optional_pins(&english_sets(), english(), &[]), None);
        let held = merge_optional_pins(&english_sets(), english(), &installed).unwrap();
        assert_eq!(
            merge_optional_pins(&held, english(), &installed),
            None,
            "a second download of the same changes nothing"
        );
    }

    #[test]
    fn the_story_members_enter_a_set_together_or_not_at_all() {
        let story = |repo: &str| {
            pin(&format!("unfoldingWord/{repo}"), "v1").with_flavor("peripheral/x-obs")
        };
        let partial = [story("en_obs"), story("en_obs-tn")];
        assert_eq!(
            merge_optional_pins(&english_sets(), english(), &partial),
            None,
            "a partial download makes no partial story set"
        );
        let whole = [story("en_obs"), story("en_obs-tn"), story("en_obs-twl")];
        let merged = merge_optional_pins(&english_sets(), english(), &whole).unwrap();
        let set = merged.primary.unwrap();
        for slot in [SetSlot::Obs, SetSlot::ObsNotes, SetSlot::ObsWordsLinks] {
            assert!(set.pin(slot).is_some(), "{slot:?}");
        }
    }
}
