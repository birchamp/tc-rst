//! Resolution, the tool preflight and the guided fix, from tC4's suites (`resolve`,
//! `gatewayTwl`, `coverage16`, `guidedFix`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::{LanguageSet, Pins, Set, Slot, pin, sha40};
use tc_check::fix::{
    Installed, Machine, PreflightState, SideloadRefusal, guided_fix, preflight, repin,
    verify_sideload,
};
use tc_check::fix::{Resolution, pin_label};
use tc_check::{Answer, Collection, Coverage, Ladder, Pin, Rung, SlotPin, ToolId, Unrecorded};
use tc_core::BookId;
use tc_journal::json::Json;

/// Where the ladder stops on a computer with nothing installed, when a usable pin does.
fn resolve(pins: &Pins, tool: ToolId, collection: Collection) -> Option<Resolution> {
    match pins.answer(tool, collection, Unrecorded::Installed(&|_| None))? {
        Answer::Pin { rung, pin } => Some(Resolution { rung, pin }),
        Answer::Unusable { .. } => None,
    }
}

fn all_pins(pins: &Pins) -> Vec<Pin> {
    [&pins.primary, &pins.fallback]
        .into_iter()
        .flat_map(|s| s.as_ref().unwrap().pins())
        .collect()
}

fn set(lang: &str, org: &str, v: &str, books: &[&str]) -> LanguageSet {
    let mut s = LanguageSet::default();
    for (slot, suffix) in [
        (Slot::TranslationNotes, "tn"),
        (Slot::TranslationWordsLinks, "twl"),
        (Slot::TranslationWords, "tw"),
        (Slot::TranslationAcademy, "ta"),
    ] {
        s.insert(slot, pin(&format!("{org}/{lang}_{suffix}"), v, books));
    }
    s
}

/// tC4 test/resolve.test.ts: Spanish primary with partial coverage, English fallback.
fn spanish_and_english() -> Pins {
    Pins {
        primary: Some(set(
            "es-419",
            "Es-419_gl",
            "v66",
            &["3JN", "JON", "RUT", "TIT"],
        )),
        fallback: Some(set(
            "en",
            "unfoldingWord",
            "v86",
            &["TIT", "JON", "HEB", "PSA"],
        )),
    }
}

fn installed(pins: &[&Pin]) -> Vec<Installed> {
    pins.iter()
        .map(|p| Installed {
            resource: p.resource.clone(),
            version: p.version.clone(),
            coverage: p.coverage.clone(),
        })
        .collect()
}

fn book(b: BookId) -> Collection {
    Collection::Book(b)
}

#[test]
fn the_resolution_unit_is_tool_and_book_on_two_rungs() {
    let pins = spanish_and_english();
    let tn = ToolId::TranslationNotes;
    assert_eq!(
        resolve(&pins, tn, book(BookId::Titus)).unwrap().rung,
        Rung::Primary
    );
    let heb = resolve(&pins, tn, book(BookId::Hebrews)).unwrap();
    assert_eq!(heb.rung, Rung::Fallback);
    assert!(heb.used_fallback());
    assert!(resolve(&pins, tn, book(BookId::Revelation)).is_none());
}

#[test]
fn a_pin_s_own_coverage_outranks_what_is_installed() {
    let mut pins = spanish_and_english();
    // A primary pin written before coverage was recorded.
    let es_tn = pin("Es-419_gl/es-419_tn", "v66", &[]);
    pins.primary
        .as_mut()
        .unwrap()
        .insert(Slot::TranslationNotes, es_tn.clone());
    let tn = ToolId::TranslationNotes;
    let titus = book(BookId::Titus);
    // The fold reads a pin that records no books as covering every book (STORAGE §7.7),
    // so its result is the same on every computer: Spanish answers.
    assert_eq!(
        pins.resolution(tn, titus).map(|a| a.rung()),
        Some(Rung::Primary)
    );
    // A screen on a computer with nothing installed cannot tell: Titus falls to English.
    assert_eq!(resolve(&pins, tn, titus).unwrap().rung, Rung::Fallback);
    // With the installed copy's books, as tC4's screens resolve, Spanish answers when the
    // copy holds the book, and English when it does not.
    let holds = |codes: &'static [&'static str]| {
        let es_tn = es_tn.resource.clone();
        move |r: &tc_check::ResourceRef| {
            r.same_resource(&es_tn)
                .then(|| Coverage::from_codes(codes.iter().copied()))
        }
    };
    let titus_installed = holds(&["TIT"]);
    let ruth_installed = holds(&["RUT"]);
    let on = |local: &dyn Fn(&tc_check::ResourceRef) -> Option<Coverage>| {
        pins.answer(tn, titus, Unrecorded::Installed(local))
            .map(|a| a.rung())
    };
    assert_eq!(on(&titus_installed), Some(Rung::Primary));
    assert_eq!(on(&ruth_installed), Some(Rung::Fallback));
}

#[test]
fn a_story_collection_resolves_to_its_first_pinned_rung() {
    let mut primary = LanguageSet::default();
    primary.insert(Slot::ObsNotes, pin("unfoldingWord/en_obs-tn", "v13", &[]));
    let pins = Pins {
        primary: Some(primary),
        fallback: None,
    };
    let r = resolve(&pins, ToolId::TranslationNotes, Collection::Stories).unwrap();
    assert_eq!(r.pin.version.as_deref(), Some("v13"));
    assert!(resolve(&pins, ToolId::TranslationWords, Collection::Stories).is_none());
    assert_eq!(
        pins.resolution(ToolId::TranslationNotes, Collection::Stories)
            .and_then(|a| a.version().map(str::to_owned)),
        Some("v13".to_owned())
    );
}

#[test]
fn preflight_states_are_distinct() {
    let pins = spanish_and_english();
    let all = all_pins(&pins);
    let everything = installed(&all.iter().collect::<Vec<_>>());
    let online = Machine {
        installed: &everything,
        online: true,
    };
    let tn = ToolId::TranslationNotes;
    assert_eq!(
        preflight(None, tn, book(BookId::Titus), &online).state,
        PreflightState::Unpinned
    );
    assert_eq!(
        preflight(Some(&pins), tn, book(BookId::Titus), &online).state,
        PreflightState::Ready
    );
    assert_eq!(
        preflight(Some(&pins), tn, book(BookId::Revelation), &online).state,
        PreflightState::NotCovered
    );

    // Nothing installed and no recorded coverage, offline: unavailable, not a false
    // "not covered".
    let mut unknown = pins.clone();
    for s in [&mut unknown.primary, &mut unknown.fallback] {
        s.as_mut().unwrap().forget_coverage();
    }
    let offline = Machine {
        installed: &[],
        online: false,
    };
    assert_eq!(
        preflight(Some(&unknown), tn, book(BookId::Revelation), &offline).state,
        PreflightState::Unavailable
    );
    let fetch = preflight(
        Some(&unknown),
        tn,
        book(BookId::Revelation),
        &Machine {
            installed: &[],
            online: true,
        },
    );
    assert_eq!(fetch.state, PreflightState::Fetch);
    assert_eq!(
        fetch.needs.unwrap().resource,
        unknown
            .pin(Rung::Primary, Slot::TranslationNotes)
            .unwrap()
            .resource
    );
}

#[test]
fn an_old_one_repository_word_links_pin_is_refused() {
    let mut pins = spanish_and_english();
    let tw = pins
        .primary
        .as_ref()
        .unwrap()
        .get(Slot::TranslationWords)
        .unwrap();
    pins.primary
        .as_mut()
        .unwrap()
        .insert(Slot::TranslationWordsLinks, tw);
    let machine = Machine {
        installed: &[],
        online: true,
    };
    assert_eq!(
        preflight(
            Some(&pins),
            ToolId::TranslationWords,
            book(BookId::Titus),
            &machine
        )
        .state,
        PreflightState::OldWordLinks
    );
    assert_eq!(
        preflight(
            Some(&pins),
            ToolId::TranslationNotes,
            book(BookId::Titus),
            &machine
        )
        .state,
        PreflightState::Fetch
    );
}

/// tC4 test/gatewayTwl.test.ts: an Arabic primary set that uses the English word links
/// with its own translation words, over the English fallback.
fn arabic() -> (Pins, Pin, Pin, Pin) {
    let twl = pin("unfoldingWord/en_twl", "v86", &["TIT"]);
    let ar_tw = pin("ar_gt/ar_tw", "v3", &["BIBLE"]);
    let en_tw = pin("unfoldingWord/en_tw", "v91", &["BIBLE"]);
    let mut primary = LanguageSet::default();
    primary.insert(Slot::TranslationNotes, pin("ar_gt/ar_tn", "v3.2", &["TIT"]));
    primary.insert(Slot::TranslationWordsLinks, twl.clone());
    primary.insert(Slot::TranslationWords, ar_tw.clone());
    let mut fallback = LanguageSet::default();
    fallback.insert(Slot::TranslationWordsLinks, twl.clone());
    fallback.insert(Slot::TranslationWords, en_tw.clone());
    (
        Pins {
            primary: Some(primary),
            fallback: Some(fallback),
        },
        twl,
        ar_tw,
        en_tw,
    )
}

#[test]
fn the_tw_tool_needs_the_resolving_set_s_articles_too() {
    let (pins, twl, ar_tw, en_tw) = arabic();
    let tw = ToolId::TranslationWords;
    let titus = book(BookId::Titus);
    let only = |p: &[&Pin], online| {
        let i = installed(p);
        preflight(
            Some(&pins),
            tw,
            titus,
            &Machine {
                installed: &i,
                online,
            },
        )
    };
    let r = resolve(&pins, tw, titus).unwrap();
    assert_eq!(
        (r.rung, r.pin.resource.clone()),
        (Rung::Primary, twl.resource.clone())
    );

    let missing_tw = only(&[&twl], true);
    assert_eq!(missing_tw.state, PreflightState::Fetch);
    assert_eq!(missing_tw.needs.as_ref().unwrap().resource, ar_tw.resource);
    let offline = only(&[&twl], false);
    assert_eq!(offline.state, PreflightState::Unavailable);
    assert_eq!(offline.needs.as_ref().unwrap().resource, ar_tw.resource);
    let missing_twl = only(&[&ar_tw], false);
    assert_eq!(missing_twl.needs.as_ref().unwrap().resource, twl.resource);
    let both = only(&[&twl, &ar_tw], true);
    assert_eq!((both.state, both.needs), (PreflightState::Ready, None));
    // tN does not read translation words.
    let ar_tn = pins.pin(Rung::Primary, Slot::TranslationNotes).unwrap();
    let i = installed(&[&ar_tn]);
    let tn = preflight(
        Some(&pins),
        ToolId::TranslationNotes,
        titus,
        &Machine {
            installed: &i,
            online: true,
        },
    );
    assert_eq!(tn.state, PreflightState::Ready);

    // A book only the fallback covers needs the fallback's articles.
    let mut pins = pins;
    let es_twl = pin("es-419_gl/es-419_twl", "v18", &["TIT"]);
    pins.primary
        .as_mut()
        .unwrap()
        .insert(Slot::TranslationWordsLinks, es_twl.clone());
    let wider = pin("unfoldingWord/en_twl", "v86", &["TIT", "MRK"]);
    pins.fallback
        .as_mut()
        .unwrap()
        .insert(Slot::TranslationWordsLinks, wider.clone());
    let i = installed(&[&es_twl, &ar_tw, &wider]);
    let mark = preflight(
        Some(&pins),
        tw,
        book(BookId::Mark),
        &Machine {
            installed: &i,
            online: true,
        },
    );
    assert_eq!(mark.resolution.as_ref().unwrap().rung, Rung::Fallback);
    assert_eq!(mark.needs.unwrap().resource, en_tw.resource);
}

#[test]
fn a_fallback_used_because_the_primary_is_not_installed_is_warned() {
    let mut pins = spanish_and_english();
    let es_tn = pin("Es-419_gl/es-419_tn", "v66", &[]);
    pins.primary
        .as_mut()
        .unwrap()
        .insert(Slot::TranslationNotes, es_tn.clone());
    let en_tn = pins.pin(Rung::Fallback, Slot::TranslationNotes).unwrap();
    let i = installed(&[&en_tn]);
    let pre = preflight(
        Some(&pins),
        ToolId::TranslationNotes,
        book(BookId::Titus),
        &Machine {
            installed: &i,
            online: false,
        },
    );
    assert_eq!(pre.state, PreflightState::Ready);
    assert_eq!(pre.unavailable_primary, Some(SlotPin::Usable(es_tn)));
    // A primary that records it does not cover the book is no warning.
    let pre = preflight(
        Some(&spanish_and_english()),
        ToolId::TranslationNotes,
        book(BookId::Hebrews),
        &Machine {
            installed: &i,
            online: false,
        },
    );
    assert_eq!(pre.unavailable_primary, None);
}

#[test]
fn the_guided_fix_names_the_pin_its_rung_and_the_installed_alternatives() {
    let (pins, twl, ar_tw, _) = arabic();
    let older_tw = Installed {
        resource: tc_check::ResourceRef {
            repo_path: ar_tw.resource.repo_path.clone(),
            sha: sha40("ar_gt/ar_tw@v2"),
        },
        version: Some("v2".into()),
        coverage: Coverage::Whole,
    };
    let mut have = installed(&[&twl]);
    have.push(older_tw.clone());
    let machine = Machine {
        installed: &have,
        online: false,
    };
    let pre = preflight(
        Some(&pins),
        ToolId::TranslationWords,
        book(BookId::Titus),
        &machine,
    );
    assert_eq!(pre.state, PreflightState::Unavailable);
    let fix = guided_fix(&pins, &pre, &machine).unwrap();
    assert_eq!(fix.pin.resource, ar_tw.resource);
    assert_eq!(fix.rung, Rung::Primary);
    assert_eq!(fix.candidates, std::slice::from_ref(&older_tw));
    assert_eq!(pin_label(&fix.pin), "ar_tw · v3");

    let offer = repin(&pins, fix.rung, &fix.pin, &older_tw).unwrap();
    assert_eq!(offer.slots, [Slot::TranslationWords]);
    assert_eq!(offer.to.resource.sha, older_tw.resource.sha);
    assert_eq!(offer.to.version.as_deref(), Some("v2"));
    assert!(repin(&pins, Rung::Fallback, &fix.pin, &older_tw).is_none());
}

#[test]
fn a_repin_moves_every_slot_of_the_rung_that_pins_the_missing_commit() {
    // tC4 test/guidedFix.test.ts: a set that pins one repository in two slots.
    let missing = pin("unfoldingWord/en_tw", "v90", &[]);
    let mut s = LanguageSet::default();
    s.insert(
        Slot::TranslationNotes,
        pin("unfoldingWord/en_tn", "v89", &[]),
    );
    s.insert(Slot::TranslationWordsLinks, missing.clone());
    s.insert(Slot::TranslationWords, missing.clone());
    let pins = Pins {
        primary: Some(s.clone()),
        fallback: Some(s),
    };
    let untagged = Installed {
        resource: tc_check::ResourceRef {
            repo_path: missing.resource.repo_path.clone(),
            sha: sha40("unfoldingWord/en_tw@v89"),
        },
        version: None,
        coverage: Coverage::Unrecorded,
    };
    let offer = repin(&pins, Rung::Primary, &missing, &untagged).unwrap();
    assert_eq!(
        offer.slots,
        [Slot::TranslationWordsLinks, Slot::TranslationWords]
    );
    assert_eq!(
        offer.to.version, None,
        "an installed copy without a tag has no label"
    );
    let other = pin("unfoldingWord/en_tw", "v88", &[]);
    assert!(repin(&pins, Rung::Primary, &other, &untagged).is_none());
}

#[test]
fn a_sideloaded_file_must_declare_the_pinned_commit() {
    let pinned = pin("unfoldingWord/en_tn", "v90", &[]);
    assert_eq!(verify_sideload(&pinned, Some(&pinned.resource.sha)), Ok(()));
    let other = sha40("unfoldingWord/en_tn@v89");
    match verify_sideload(&pinned, Some(&other)) {
        Err(SideloadRefusal::OtherCommit {
            found,
            pinned: p,
            label,
        }) => {
            assert_eq!(found, other);
            assert_eq!(p, pinned.resource.sha);
            assert_eq!(label, "en_tn · v90");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        verify_sideload(&pinned, None),
        Err(SideloadRefusal::NoRevision)
    );
}

#[test]
fn pins_read_from_tc4_json() {
    let json = serde_json::json!({
        "gatewayLanguage": {"languageId": "en", "owner": "unfoldingWord"},
        "translationNotes": {
            "repoPath": "git.door43.org/unfoldingWord/en_tn",
            "version": "v91",
            "sha": "e586762e330f482a60c52aedd1c7b3a2f155df8a",
            "flavor": "parascriptural/x-bcvnotes",
            "books": ["TIT", "JON"]
        },
        "translationWords": {
            "repoPath": "git.door43.org/unfoldingWord/en_tw",
            "sha": "ff5b3852c27c3a0d01b109e482eb26047dcd20e2",
            "books": ["BIBLE"]
        }
    });
    let set = LanguageSet::from_json(&json).unwrap();
    assert!(set.unusable().is_empty());
    assert_eq!(set.slot_count(), 2);
    let tn = set.get(Slot::TranslationNotes).unwrap();
    assert!(tn.coverage.covers(BookId::Jonah) && !tn.coverage.covers(BookId::Ruth));
    assert_eq!(
        set.get(Slot::TranslationWords).unwrap().coverage,
        Coverage::Whole
    );
    assert!(matches!(
        read(&serde_json::json!({"repoPath": "git.door43.org/x/y", "version": "v1"})),
        Some(SlotPin::Unusable(_))
    ));
}

#[test]
fn one_unreadable_pin_leaves_the_rest_of_the_set_readable() {
    let tn = serde_json::json!({
        "repoPath": "git.door43.org/unfoldingWord/en_tn",
        "version": "v91",
        "sha": "e586762e330f482a60c52aedd1c7b3a2f155df8a",
        "books": ["TIT", 3]
    });
    let json = serde_json::json!({
        "translationNotes": tn,
        "obs-images": {"repoPath": "git.door43.org/unfoldingWord/obs-images", "version": "v1"},
        "obs-tn": null
    });
    let set = LanguageSet::from_json(&json).unwrap();
    // tC4 `coverageFor` reads a books list with a non-code in it as no record.
    let pin = set.get(Slot::TranslationNotes).unwrap();
    assert_eq!(pin.coverage, Coverage::Unrecorded);
    assert_eq!(pin.version.as_deref(), Some("v91"));
    assert_eq!(set.unusable(), [Slot::ObsImages]);
    assert!(set.get(Slot::ObsImages).is_none());
    // A null slot is no pin (tC4 `if (pin)`).
    assert!(set.held(Slot::ObsNotes).is_none());
    assert!(LanguageSet::from_json(&serde_json::json!(["not", "a", "set"])).is_none());
    let books_not_a_list = serde_json::json!({
        "repoPath": "git.door43.org/unfoldingWord/en_tn",
        "sha": "e586762e330f482a60c52aedd1c7b3a2f155df8a",
        "books": "TIT"
    });
    match read(&books_not_a_list) {
        Some(SlotPin::Usable(pin)) => assert_eq!(pin.coverage, Coverage::Unrecorded),
        other => panic!("{other:?}"),
    }
}

/// A slot value as the ladder reads it.
fn read(value: &serde_json::Value) -> Option<SlotPin> {
    SlotPin::read(&Json::try_from(value).unwrap())
}

/// A pin with a tag and no commit, as tC4 JSON writes it, and what the ladder reads of it.
fn tag_only(repo: &str, books: Option<&[&str]>) -> (serde_json::Value, SlotPin) {
    let mut json = serde_json::json!({
        "repoPath": format!("git.door43.org/{repo}"),
        "version": "v66",
    });
    if let Some(books) = books {
        json["books"] = serde_json::json!(books);
    }
    let held = read(&json).unwrap();
    assert!(matches!(held, SlotPin::Unusable(_)), "{held:?}");
    (json, held)
}

#[test]
fn an_unusable_primary_pin_is_not_replaced_by_the_fallback() {
    let mut pins = spanish_and_english();
    let (json, es_tn) = tag_only("Es-419_gl/es-419_tn", Some(&["TIT", "JON"]));
    pins.primary
        .as_mut()
        .unwrap()
        .insert_json(Slot::TranslationNotes, &json);
    let tn = ToolId::TranslationNotes;
    let all = all_pins(&pins);
    let everything = installed(&all.iter().collect::<Vec<_>>());
    let machine = Machine {
        installed: &everything,
        online: true,
    };

    // tC4 resolves Titus to the primary pin, which no install satisfies; English is
    // not used in its place.
    let none = |_: &tc_check::ResourceRef| None;
    match pins.answer(tn, book(BookId::Titus), Unrecorded::Installed(&none)) {
        Some(Answer::Unusable { rung, pin }) => {
            assert_eq!(rung, Rung::Primary);
            assert_eq!(SlotPin::Unusable(pin), es_tn);
        }
        other => panic!("{other:?}"),
    }
    assert!(resolve(&pins, tn, book(BookId::Titus)).is_none());
    let pre = preflight(Some(&pins), tn, book(BookId::Titus), &machine);
    assert_eq!(pre.state, PreflightState::UnusablePin);
    assert_eq!((pre.resolution.as_ref(), pre.needs.as_ref()), (None, None));
    assert_eq!(
        pre.unusable.as_ref().map(|(rung, _)| *rung),
        Some(Rung::Primary)
    );
    assert!(guided_fix(&pins, &pre, &machine).is_none());
    // Nor does the guided fix offer English, whose coverage is unknown here: getting it
    // would not repair the Spanish pin.
    let mut unknown_english = pins.clone();
    let fallback = unknown_english.fallback.as_mut().unwrap();
    let mut en_tn = fallback.get(Slot::TranslationNotes).unwrap();
    en_tn.coverage = Coverage::Unrecorded;
    fallback.insert(Slot::TranslationNotes, en_tn);
    let bare = Machine {
        installed: &[],
        online: true,
    };
    let pre = preflight(Some(&unknown_english), tn, book(BookId::Titus), &bare);
    assert_eq!(pre.state, PreflightState::UnusablePin);
    assert_eq!(guided_fix(&unknown_english, &pre, &bare), None);

    // A book the unusable pin records it does not cover takes the fallback, as in tC4,
    // and the other slots of the set still resolve.
    let heb = resolve(&pins, tn, book(BookId::Hebrews)).unwrap();
    assert_eq!(heb.rung, Rung::Fallback);
    let pre = preflight(Some(&pins), tn, book(BookId::Hebrews), &machine);
    assert_eq!(
        (pre.state, pre.unavailable_primary),
        (PreflightState::Ready, None)
    );
    assert_eq!(
        resolve(&pins, ToolId::TranslationWords, book(BookId::Titus))
            .unwrap()
            .rung,
        Rung::Primary
    );
}

#[test]
fn an_unusable_primary_pin_that_records_no_books_warns_like_a_missing_one() {
    let mut pins = spanish_and_english();
    let (json, es_tn) = tag_only("Es-419_gl/es-419_tn", None);
    pins.primary
        .as_mut()
        .unwrap()
        .insert_json(Slot::TranslationNotes, &json);
    let tn = ToolId::TranslationNotes;
    let en_tn = pins.pin(Rung::Fallback, Slot::TranslationNotes).unwrap();
    let i = installed(&[&en_tn]);
    let machine = Machine {
        installed: &i,
        online: true,
    };
    // Its coverage is unknown and no install can tell it, so English answers, warned
    // (tC4 `warnedUnavailablePrimary`).
    let pre = preflight(Some(&pins), tn, book(BookId::Titus), &machine);
    assert_eq!(pre.state, PreflightState::Ready);
    assert_eq!(pre.resolution.unwrap().rung, Rung::Fallback);
    assert_eq!(pre.unavailable_primary, Some(es_tn.clone()));

    // With no fallback for the slot, the tool reports the unusable pin, not "unpinned".
    pins.fallback = None;
    let pre = preflight(Some(&pins), tn, book(BookId::Titus), &machine);
    assert_eq!(pre.state, PreflightState::UnusablePin);
    assert_eq!(pre.unusable.map(|(_, p)| SlotPin::Unusable(p)), Some(es_tn));
}

#[test]
fn an_unusable_story_pin_or_articles_pin_stops_the_tool() {
    // A story collection is answered by its first pinned rung, usable or not.
    let mut primary = LanguageSet::default();
    primary.insert_json(Slot::ObsNotes, &tag_only("unfoldingWord/en_obs-tn", None).0);
    let mut fallback = LanguageSet::default();
    fallback.insert(Slot::ObsNotes, pin("unfoldingWord/en_obs-tn", "v13", &[]));
    let pins = Pins {
        primary: Some(primary),
        fallback: Some(fallback),
    };
    let machine = Machine {
        installed: &[],
        online: true,
    };
    let none = |_: &tc_check::ResourceRef| None;
    assert!(matches!(
        pins.answer(
            ToolId::TranslationNotes,
            Collection::Stories,
            Unrecorded::Installed(&none)
        ),
        Some(Answer::Unusable {
            rung: Rung::Primary,
            ..
        })
    ));
    let pre = preflight(
        Some(&pins),
        ToolId::TranslationNotes,
        Collection::Stories,
        &machine,
    );
    assert_eq!(pre.state, PreflightState::UnusablePin);

    // The tW tool reads the resolving set's articles; an unusable articles pin stops it
    // even when the word links are installed.
    let (mut pins, twl, _, en_tw) = arabic();
    pins.primary.as_mut().unwrap().insert_json(
        Slot::TranslationWords,
        &tag_only("ar_gt/ar_tw", Some(&["BIBLE"])).0,
    );
    let i = installed(&[&twl, &en_tw]);
    let pre = preflight(
        Some(&pins),
        ToolId::TranslationWords,
        book(BookId::Titus),
        &Machine {
            installed: &i,
            online: true,
        },
    );
    assert_eq!(pre.state, PreflightState::UnusablePin);
    assert_eq!(pre.unusable.map(|(rung, _)| rung), Some(Rung::Primary));
}
