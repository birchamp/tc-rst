//! Loading what is installed: each helps part from one pinned resource, the texts, and a
//! stated reason for everything that is not there.

// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::{aquifer_pin, aquifer_zip, fixture, install_fixture, repo_path};
use md5::{Digest, Md5};
use tc_core::{BookId, Reference};
use tc_helps::metadata::HelpsSource;
use tc_helps::{CheckBlocker as ItemBlocker, CheckReadiness, HelpsPart, ObsRef, Occurrence};
use tc_resources::resolve::{missing_pin_count, unsatisfied_pin_for};
use tc_resources::{
    AquiferHelps, GatewayLanguage, HelpsChoice, LanguageSet, Missing, PaneId, PartStatus,
    ProjectPins, ResourcePin, ResourceStore, Rung, ScripturePane, SetSlot, TextRole, TextStatus,
    install_aquifer_zip, load_book, load_obs, suite,
};

fn store() -> (tempfile::TempDir, ResourceStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = ResourceStore::open(dir.path()).unwrap();
    (dir, store)
}

fn set(language: &str, owner: &str) -> LanguageSet {
    LanguageSet::new(GatewayLanguage {
        language: language.to_owned(),
        owner: owner.parse().unwrap(),
    })
}

struct English {
    tn: ResourcePin,
    tn86: ResourcePin,
    twl: ResourcePin,
    tw: ResourcePin,
    ta: ResourcePin,
    ult: ResourcePin,
    es_tn: ResourcePin,
}

fn install_fixtures(store: &ResourceStore) -> English {
    English {
        tn: install_fixture(
            store,
            "en_tn@v91",
            "unfoldingWord/en_tn",
            "v91",
            &[("TIT.tsv", "ingredients/TIT.tsv")],
        ),
        tn86: install_fixture(
            store,
            "en_tn@v86",
            "unfoldingWord/en_tn",
            "v86",
            &[
                ("TIT.tsv", "ingredients/TIT.tsv"),
                ("JON.tsv", "ingredients/JON.tsv"),
            ],
        ),
        twl: install_fixture(
            store,
            "en_twl@v86",
            "unfoldingWord/en_twl",
            "v86",
            &[("TIT.tsv", "ingredients/TIT.tsv")],
        ),
        tw: install_fixture(
            store,
            "en_tw@v87",
            "unfoldingWord/en_tw",
            "v87",
            &[
                ("TIT.tsv", "ingredients/TIT.tsv"),
                ("payload-kt-god.md", "ingredients/payload/kt/god.md"),
            ],
        ),
        ta: install_fixture(
            store,
            "en_ta@v86",
            "unfoldingWord/en_ta",
            "v86",
            &[
                ("translate-toc.yaml", "ingredients/translate/toc.yaml"),
                (
                    "translate-figs-abstractnouns-01.md",
                    "ingredients/translate/figs-abstractnouns/01.md",
                ),
                (
                    "translate-figs-abstractnouns-title.md",
                    "ingredients/translate/figs-abstractnouns/title.md",
                ),
            ],
        ),
        ult: install_fixture(
            store,
            "en_ult@v89",
            "unfoldingWord/en_ult",
            "v89",
            &[("TIT.usfm", "ingredients/TIT.usfm")],
        ),
        es_tn: install_fixture(
            store,
            "es-419_tn@v66",
            "es-419_gl/es-419_tn",
            "v66",
            &[("TIT.tsv", "ingredients/TIT.tsv")],
        ),
    }
}

/// es-419 primary with notes only; the English fallback with the installed fixtures and
/// the bundled questions and simplified Bible, which are not installed; the bundled
/// original-language texts, not installed; the fixture ULT and the bundled UST as panes.
fn project(en: &English) -> ProjectPins {
    let bundled = suite::project_pins();
    let fallback = bundled.fallback.clone().unwrap();
    ProjectPins {
        primary: Some(set("es-419", "es-419_gl").with(SetSlot::TranslationNotes, en.es_tn.clone())),
        fallback: Some(
            set("en", "unfoldingWord")
                .with(SetSlot::TranslationNotes, en.tn.clone())
                .with(SetSlot::TranslationWordsLinks, en.twl.clone())
                .with(SetSlot::TranslationWords, en.tw.clone())
                .with(SetSlot::TranslationAcademy, en.ta.clone())
                .with(
                    SetSlot::TranslationQuestions,
                    fallback.pin(SetSlot::TranslationQuestions).unwrap().clone(),
                )
                .with(
                    SetSlot::SimplifiedText,
                    fallback.pin(SetSlot::SimplifiedText).unwrap().clone(),
                ),
        ),
        original_language: bundled.original_language.clone(),
        lexicon: bundled.lexicon.clone(),
        extra_scripture: vec![
            ScripturePane {
                id: "ult".parse().unwrap(),
                pin: en.ult.clone(),
            },
            bundled.extra_scripture[1].clone(),
        ],
        unreadable_panes: Vec::new(),
        unknown: Default::default(),
    }
}

fn source_repo(status: &PartStatus) -> String {
    match status {
        PartStatus::Loaded {
            source: HelpsSource::Dcs(pin),
            ..
        } => pin.repo.clone(),
        PartStatus::Loaded {
            source: HelpsSource::Aquifer(pin),
            ..
        } => pin.resource_code.clone(),
        PartStatus::Missing(m) => panic!("missing: {m:?}"),
    }
}

fn rung(status: &PartStatus) -> Option<Rung> {
    match status {
        PartStatus::Loaded { rung, .. } => *rung,
        PartStatus::Missing(m) => panic!("missing: {m:?}"),
    }
}

fn reference(text: &str) -> Reference {
    text.parse().unwrap()
}

#[test]
fn a_book_loads_each_part_from_its_rung_and_states_what_is_missing() {
    let (_dir, store) = store();
    let en = install_fixtures(&store);
    let pins = project(&en);
    let loaded = load_book(&store, &pins, &HelpsChoice::Dcs, BookId::Titus);

    let notes = &loaded.parts[&HelpsPart::Notes];
    assert_eq!(source_repo(notes), "es-419_tn");
    assert_eq!(rung(notes), Some(Rung::Primary));
    assert!(loaded.helps.notes_for(reference("TIT 1:1")).count() > 0);
    assert_eq!(
        loaded.helps.source(HelpsPart::Notes).unwrap().to_string(),
        format!("es-419_gl/es-419_tn v66 ({}…)", &en.es_tn.sha.to_hex()[..7])
    );

    let links = &loaded.parts[&HelpsPart::WordLinks];
    assert_eq!(
        (source_repo(links).as_str(), rung(links)),
        ("en_twl", Some(Rung::Fallback))
    );
    assert!(loaded.helps.word_links_for(reference("TIT 1:1")).count() > 0);

    // The articles of the rung that answered the links (D97).
    let words = &loaded.parts[&HelpsPart::WordArticles];
    assert_eq!(
        (source_repo(words).as_str(), rung(words)),
        ("en_tw", Some(Rung::Fallback))
    );
    let god = tc_helps::articles::word_article_id("payload/kt/god.md").unwrap();
    assert_eq!(loaded.helps.word_article(&god).unwrap().title, "God");

    // The notes' rung pins no academy, so the first rung that does answers.
    let academy = &loaded.parts[&HelpsPart::AcademyArticles];
    assert_eq!(source_repo(academy), "en_ta");
    let id = tc_helps::AcademyArticleId::Dcs {
        manual: tc_helps::Manual::Translate,
        slug: tc_helps::Slug::parse("figs-abstractnouns").unwrap(),
    };
    assert_eq!(
        loaded.helps.academy_article(&id).unwrap().title,
        "Abstract Nouns"
    );

    // The bundled questions are pinned, their coverage unknown, and not installed.
    let tq = pins
        .fallback
        .as_ref()
        .unwrap()
        .pin(SetSlot::TranslationQuestions)
        .unwrap();
    assert_eq!(
        loaded.parts[&HelpsPart::Questions],
        PartStatus::Missing(Missing::NotInstalled(tq.clone()))
    );
    assert_eq!(loaded.parts.len(), 5, "every Bible part is stated");

    let ugnt = pins.original_language.nt.clone().unwrap();
    assert_eq!(
        loaded.text(&TextRole::OriginalLanguage),
        Some(&TextStatus::Missing(Missing::NotInstalled(ugnt)))
    );
    match loaded.text(&TextRole::Pane("ult".parse::<PaneId>().unwrap())) {
        Some(TextStatus::Loaded { usfm, pin, .. }) => {
            assert_eq!(
                usfm.as_bytes(),
                fixture("tc4/test-fixtures/resources/en_ult@v89/TIT.usfm")
            );
            assert_eq!(pin, &en.ult);
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        loaded.text(&TextRole::Pane("ust".parse().unwrap())),
        Some(TextStatus::Missing(Missing::NotInstalled(_)))
    ));
    assert!(matches!(
        loaded.text(&TextRole::Simplified),
        Some(TextStatus::Missing(Missing::NotInstalled(_)))
    ));
    assert_eq!(loaded.texts.len(), 4);
}

#[test]
fn the_store_answers_which_pinned_versions_this_computer_lacks() {
    let (_dir, store) = store();
    let en = install_fixtures(&store);
    let pins = project(&en);
    let installed = |pin: &ResourcePin| store.has_dcs(pin);
    // Not installed: the fallback's questions and simplified Bible, both original-language
    // texts and the UST pane. The two lexicons are not installed either, and not counted.
    assert!(pins.lexicon.nt.as_ref().is_some_and(|p| !store.has_dcs(p)));
    assert_eq!(missing_pin_count(&pins, &installed), 5);
    let ust = &pins.extra_scripture[1].pin;
    let fallback_ust = pins
        .fallback
        .as_ref()
        .unwrap()
        .pin(SetSlot::SimplifiedText)
        .unwrap();
    assert_eq!(
        unsatisfied_pin_for(&pins, ust.repo.repo(), &installed),
        Some(fallback_ust)
    );
    assert_eq!(
        unsatisfied_pin_for(&pins, en.ult.repo.repo(), &installed),
        None
    );
}

#[test]
fn a_book_no_installed_resource_holds_is_stated_as_not_covered() {
    let (_dir, store) = store();
    let en = install_fixtures(&store);
    let pins = project(&en);
    let loaded = load_book(&store, &pins, &HelpsChoice::Dcs, BookId::Ruth);
    match &loaded.parts[&HelpsPart::Notes] {
        PartStatus::Missing(Missing::NotCovered { pinned }) => {
            assert_eq!(pinned, &vec![en.es_tn.clone(), en.tn.clone()]);
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        loaded.parts[&HelpsPart::WordLinks],
        PartStatus::Missing(Missing::NotCovered { .. })
    ));
    // Articles are not divided into books: they still load for the tabs' links.
    assert!(loaded.parts[&HelpsPart::WordArticles].is_loaded());
    assert!(matches!(
        loaded.text(&TextRole::Pane("ult".parse().unwrap())),
        Some(TextStatus::Missing(Missing::BookNotInResource { .. }))
    ));
    assert_eq!(loaded.helps.notes().len(), 0);
}

#[test]
fn english_standing_in_for_an_uninstalled_primary_is_stated() {
    let (_dir, store) = store();
    let en = install_fixtures(&store);
    let mut pins = project(&en);
    let unknown_primary = ResourcePin::new(
        repo_path("es-419_gl/es-419_tn"),
        "2".repeat(40).parse().unwrap(),
    )
    .with_version("v67".parse().unwrap());
    pins.primary =
        Some(set("es-419", "es-419_gl").with(SetSlot::TranslationNotes, unknown_primary.clone()));
    pins.fallback
        .as_mut()
        .unwrap()
        .pins
        .insert(SetSlot::TranslationNotes, en.tn86.clone());
    let loaded = load_book(&store, &pins, &HelpsChoice::Dcs, BookId::Jonah);
    match &loaded.parts[&HelpsPart::Notes] {
        PartStatus::Loaded {
            rung,
            substitute_for,
            ..
        } => {
            assert_eq!(*rung, Some(Rung::Fallback));
            assert_eq!(substitute_for.as_ref(), Some(&unknown_primary));
        }
        other => panic!("{other:?}"),
    }
    assert!(loaded.helps.notes_for(reference("JON 1:1")).count() > 0);
}

#[test]
fn a_pinned_resource_that_is_not_installed_is_stated() {
    let (_dir, store) = store();
    let pins = suite::project_pins();
    let loaded = load_book(&store, &pins, &HelpsChoice::Dcs, BookId::Titus);
    let tn = pins
        .primary
        .as_ref()
        .unwrap()
        .pin(SetSlot::TranslationNotes)
        .unwrap();
    assert_eq!(
        loaded.parts[&HelpsPart::Notes],
        PartStatus::Missing(Missing::NotInstalled(tn.clone()))
    );
    for (part, status) in &loaded.parts {
        assert!(!status.is_loaded(), "{part}");
    }
    for text in &loaded.texts {
        assert!(
            matches!(text.status, TextStatus::Missing(Missing::NotInstalled(_))),
            "{:?}",
            text.role
        );
    }
    let unpinned = load_book(
        &store,
        &ProjectPins::default(),
        &HelpsChoice::Dcs,
        BookId::Titus,
    );
    for status in unpinned.parts.values() {
        assert_eq!(status, &PartStatus::Missing(Missing::NotPinned));
    }
}

#[test]
fn the_old_one_repository_links_are_never_read() {
    let (_dir, store) = store();
    let en = install_fixtures(&store);
    let mut pins = project(&en);
    pins.fallback
        .as_mut()
        .unwrap()
        .pins
        .insert(SetSlot::TranslationWordsLinks, en.tw.clone());
    let loaded = load_book(&store, &pins, &HelpsChoice::Dcs, BookId::Titus);
    assert_eq!(
        loaded.parts[&HelpsPart::WordLinks],
        PartStatus::Missing(Missing::OldTwl(en.tw.clone()))
    );
    assert_eq!(loaded.helps.word_links().len(), 0);
}

fn install_aquifer(store: &ResourceStore, repo: &str) -> tc_resources::AquiferPin {
    let bytes = aquifer_zip(repo, "por");
    let pin = aquifer_pin(repo, "por", &bytes);
    install_aquifer_zip(store, &pin, &bytes, 1 << 30).unwrap();
    pin
}

#[test]
fn aquifer_helps_fill_the_tabs_but_their_notes_cannot_drive_a_check() {
    let (_dir, store) = store();
    let choice = HelpsChoice::Aquifer(AquiferHelps {
        notes: Some(install_aquifer(&store, "UWTranslationNotes")),
        words: Some(install_aquifer(&store, "UWTranslationWords")),
        academy: None,
        questions: Some(install_aquifer(&store, "UWTranslationQuestions")),
    });
    let loaded = load_book(&store, &ProjectPins::default(), &choice, BookId::Titus);
    assert_eq!(
        source_repo(&loaded.parts[&HelpsPart::Notes]),
        "UWTranslationNotes"
    );
    assert_eq!(
        source_repo(&loaded.parts[&HelpsPart::WordLinks]),
        "UWTranslationWords"
    );
    assert_eq!(
        source_repo(&loaded.parts[&HelpsPart::WordArticles]),
        "UWTranslationWords"
    );
    assert_eq!(
        source_repo(&loaded.parts[&HelpsPart::Questions]),
        "UWTranslationQuestions"
    );
    assert_eq!(
        loaded.parts[&HelpsPart::AcademyArticles],
        PartStatus::Missing(Missing::NotPinned)
    );

    let notes: Vec<_> = loaded.helps.notes_for(reference("TIT 1:1")).collect();
    assert!(!notes.is_empty());
    let check = notes.iter().find(|n| !n.quote.is_empty()).unwrap();
    assert_eq!(check.occurrence, Occurrence::Unrecorded);
    assert!(matches!(
        check.check_readiness(),
        CheckReadiness::Blocked(ref b) if b.contains(&ItemBlocker::OccurrenceUnrecorded)
    ));
    let words = loaded.helps.words_for(reference("TIT 1:1"));
    assert_eq!(words.len(), 5);
    assert!(loaded.helps.questions_for(reference("TIT 1:1")).count() > 0);

    // A pinned asset that is not installed is stated.
    let mut other = choice.clone();
    if let HelpsChoice::Aquifer(helps) = &mut other {
        let mut notes = helps.notes.clone().unwrap();
        notes.sha256 = tc_resources::Sha256::of(b"another asset");
        helps.notes = Some(notes.clone());
        let loaded = load_book(&store, &ProjectPins::default(), &other, BookId::Titus);
        assert_eq!(
            loaded.parts[&HelpsPart::Notes],
            PartStatus::Missing(Missing::AquiferNotInstalled(notes))
        );
    }
    // A book the asset does not hold.
    let ruth = load_book(&store, &ProjectPins::default(), &choice, BookId::Ruth);
    assert!(matches!(
        ruth.parts[&HelpsPart::Notes],
        PartStatus::Missing(Missing::BookNotInResource { .. })
    ));
}

/// A French notes asset in the release's shape: its Genesis file holds, as the real one
/// does, a 2 Timothy 2:16 note and a James 2:1 note filed under `00000000`, beside three
/// Genesis 1:1 notes (fixtures/aquifer/README.md). The 2 Timothy file holds one note of
/// its own, made from a Genesis note placed at 2 Timothy 1:1. There is no French
/// `metadata.json` in the fixtures, so the English one is used with the language changed.
fn install_misfiled_notes(store: &ResourceStore) -> tc_resources::AquiferPin {
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&fixture("aquifer/UWTranslationNotes/eng/metadata.json")).unwrap();
    metadata["resource_metadata"]["language"] = "fra".into();
    let genesis = fixture("aquifer/UWTranslationNotes/fra/json/01-subset.content.json");
    let articles: Vec<serde_json::Value> = serde_json::from_slice(&genesis).unwrap();
    let mut own = articles[4].clone();
    own["content_id"] = 900_001.into();
    own["index_reference"] = "55001001".into();
    own["associations"]["passage"] = serde_json::json!([{
        "start_ref": "55001001",
        "start_ref_usfm": "2TI 1:1",
        "end_ref": "55001001",
        "end_ref_usfm": "2TI 1:1",
    }]);
    let second_timothy = serde_json::to_vec(&[own]).unwrap();
    let bytes = common::zip(&[
        ("metadata.json", &serde_json::to_vec(&metadata).unwrap()),
        ("json/01.content.json", &genesis),
        ("json/55.content.json", &second_timothy),
    ]);
    let pin = tc_resources::AquiferPin {
        repo: tc_resources::AquiferRepo::bible_aquifer("UWTranslationNotes").unwrap(),
        tag: common::AQUIFER_TAG.parse().unwrap(),
        commit: common::aquifer_commit("UWTranslationNotes")
            .parse()
            .unwrap(),
        language: "fra".parse().unwrap(),
        asset: "French-json.zip".parse().unwrap(),
        sha256: tc_resources::Sha256::of(&bytes),
    };
    install_aquifer_zip(store, &pin, &bytes, 1 << 30).unwrap();
    pin
}

#[test]
fn an_aquifer_note_loads_with_its_passage_whatever_file_holds_it() {
    let (_dir, store) = store();
    let pin = install_misfiled_notes(&store);
    let installed = store.aquifer(&pin).unwrap().unwrap();
    assert_eq!(
        installed.books(),
        tc_resources::BookCoverage::books([BookId::Genesis, BookId::SecondTimothy, BookId::James])
            .as_ref()
    );
    let choice = HelpsChoice::Aquifer(AquiferHelps {
        notes: Some(pin),
        words: None,
        academy: None,
        questions: None,
    });
    let load = |book| load_book(&store, &ProjectPins::default(), &choice, book);

    // 2 Timothy has its own file and a note filed in Genesis: both load, nothing refused.
    let timothy = load(BookId::SecondTimothy);
    assert!(matches!(
        &timothy.parts[&HelpsPart::Notes],
        PartStatus::Loaded { refused, .. } if refused.is_empty()
    ));
    assert_eq!(timothy.helps.notes().len(), 2);
    assert_eq!(timothy.helps.notes_for(reference("2TI 2:16")).count(), 1);
    assert_eq!(timothy.helps.notes_for(reference("2TI 1:1")).count(), 1);

    // James has no file of its own; its note is in the Genesis file.
    let james = load(BookId::James);
    assert!(james.parts[&HelpsPart::Notes].is_loaded());
    assert_eq!(james.helps.notes().len(), 1);
    assert_eq!(james.helps.notes_for(reference("JAS 2:1")).count(), 1);

    // Genesis holds its own notes only.
    let genesis = load(BookId::Genesis);
    assert_eq!(genesis.helps.notes().len(), 3);
    assert_eq!(genesis.helps.notes_for(reference("GEN 1:1")).count(), 3);
    assert_eq!(genesis.helps.notes_for(reference("2TI 2:16")).count(), 0);

    let titus = load(BookId::Titus);
    assert!(matches!(
        titus.parts[&HelpsPart::Notes],
        PartStatus::Missing(Missing::BookNotInResource { .. })
    ));
}

#[test]
fn open_bible_stories_helps_load_from_door43() {
    let (_dir, store) = store();
    let en = install_fixtures(&store);
    let obs_tn = install_fixture(
        &store,
        "en_obs-tn@v13",
        "unfoldingWord/en_obs-tn",
        "v13",
        &[("OBS.tsv", "ingredients/OBS.tsv")],
    );
    let obs_twl = install_fixture(
        &store,
        "en_obs-twl@v3",
        "unfoldingWord/en_obs-twl",
        "v3",
        &[("OBS.tsv", "ingredients/OBS.tsv")],
    );
    let obs_tq = install_fixture(
        &store,
        "en_obs-tq@v10",
        "unfoldingWord/en_obs-tq",
        "v10",
        &[("OBS.tsv", "ingredients/OBS.tsv")],
    );
    // The bundled suite pins exactly these OBS helps commits.
    let bundled = suite::english_helps();
    assert_eq!(bundled.pin(SetSlot::ObsNotes).unwrap().sha, obs_tn.sha);
    assert_eq!(
        bundled.pin(SetSlot::ObsWordsLinks).unwrap().sha,
        obs_twl.sha
    );
    assert_eq!(bundled.pin(SetSlot::ObsQuestions).unwrap().sha, obs_tq.sha);
    let english = set("en", "unfoldingWord")
        .with(SetSlot::ObsNotes, obs_tn)
        .with(SetSlot::ObsWordsLinks, obs_twl)
        .with(SetSlot::ObsQuestions, obs_tq)
        .with(SetSlot::TranslationWords, en.tw.clone())
        .with(SetSlot::TranslationAcademy, en.ta.clone());
    let pins = ProjectPins {
        primary: Some(english.clone()),
        fallback: Some(english),
        ..ProjectPins::default()
    };
    let loaded = load_obs(&store, &pins);
    for part in [
        HelpsPart::ObsNotes,
        HelpsPart::ObsWordLinks,
        HelpsPart::ObsQuestions,
    ] {
        assert_eq!(rung(&loaded.parts[&part]), Some(Rung::Primary), "{part}");
    }
    let frame = ObsRef::new(1, 1).unwrap();
    assert!(loaded.helps.obs_notes_for(frame).count() > 0);
    assert!(loaded.helps.obs_word_links_for(frame).count() > 0);
    assert_eq!(loaded.helps.notes().len(), 2325);
    assert!(loaded.parts[&HelpsPart::WordArticles].is_loaded());
    assert_eq!(
        loaded.stories_status,
        PartStatus::Missing(Missing::NotPinned)
    );
}

/// en_obs-tq v10 with one row's Occurrence made `x`, and a Titus questions table with the
/// same bad row, both under en_obs-tq's metadata (the fixtures hold no Bible tQ).
fn install_questions_with_a_bad_row(store: &ResourceStore) -> (ResourcePin, usize) {
    let dir = "en_obs-tq@v10";
    let base = format!("tc4/test-fixtures/resources/{dir}");
    let obs = String::from_utf8(fixture(&format!("{base}/OBS.tsv"))).unwrap();
    let rows = obs.lines().filter(|l| !l.trim().is_empty()).count() - 1;
    let obs = obs.replacen("1:1\tes4e\t\t\t\t", "1:1\tes4e\t\t\tx\t", 1);
    let tit = "Reference\tID\tTags\tQuote\tOccurrence\tQuestion\tResponse\n\
        1:1\taaaa\t\t\tx\tWho wrote the letter?\tPaul.\n\
        1:2\tbbbb\t\t\t\tWhat did God promise?\tEternal life.\n\
        1:3\tcccc\t\t\t\tHow did God reveal his word?\tThrough preaching.\n";
    // The edited table is declared as it now is, as Door43's export would declare it.
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&fixture(&format!("{base}/metadata.json"))).unwrap();
    for (path, text) in [
        ("ingredients/OBS.tsv", &obs[..]),
        ("ingredients/TIT.tsv", tit),
    ] {
        let md5: String = Md5::digest(text.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        metadata["ingredients"][path] = serde_json::json!({
            "checksum": { "md5": md5 },
            "mimeType": "text/tab-separated-values",
            "size": text.len(),
        });
    }
    let metadata = serde_json::to_vec(&metadata).unwrap();
    let export = common::zip(&[
        ("en_obs-tq/metadata.json", &metadata),
        ("en_obs-tq/ingredients/OBS.tsv", obs.as_bytes()),
        ("en_obs-tq/ingredients/TIT.tsv", tit.as_bytes()),
    ]);
    let pin = common::fixture_pin(dir, "unfoldingWord/en_obs-tq", "v10");
    tc_resources::install_dcs_zip(store, &pin, &export, 1 << 30).unwrap();
    (pin, rows)
}

#[test]
fn questions_with_one_bad_row_still_load() {
    let (_dir, store) = store();
    let (tq, rows) = install_questions_with_a_bad_row(&store);
    let english = set("en", "unfoldingWord")
        .with(SetSlot::ObsQuestions, tq.clone())
        .with(SetSlot::TranslationQuestions, tq);
    let pins = ProjectPins {
        primary: Some(english.clone()),
        fallback: Some(english),
        ..ProjectPins::default()
    };

    let obs = load_obs(&store, &pins);
    assert!(obs.parts[&HelpsPart::ObsQuestions].is_loaded());
    assert_eq!(
        obs.helps.questions().len(),
        rows,
        "every question, the bad row's too"
    );
    let first = obs
        .helps
        .obs_questions_for(ObsRef::new(1, 1).unwrap())
        .next()
        .unwrap();
    assert_eq!(first.item.occurrence, Occurrence::Unrecorded);

    let book = load_book(&store, &pins, &HelpsChoice::Dcs, BookId::Titus);
    assert!(
        book.parts[&HelpsPart::Questions].is_loaded(),
        "{:?}",
        book.parts[&HelpsPart::Questions]
    );
    assert_eq!(book.helps.questions().len(), 3);
}

/// tC4 reads past a bad row of the OBS questions: one row the strict reader refuses (a cell
/// past the seventh, an Occurrence that is not a count) hides no other question.
#[test]
fn one_bad_obs_question_row_hides_no_other_question() {
    let (_dir, store) = store();
    let en = install_fixtures(&store);
    let dir = "en_obs-tq@v10";
    let mut tsv = String::from_utf8(fixture(&format!(
        "tc4/test-fixtures/resources/{dir}/OBS.tsv"
    )))
    .unwrap();
    let good = tsv.lines().count() - 1;
    tsv.push_str("1:2\tbad1\t\t\tx\tA bad row?\tStill read.\textra\n");
    // The export's metadata records each file's size and md5; the edited table gets its own.
    let mut metadata: serde_json::Value = serde_json::from_slice(&fixture(&format!(
        "tc4/test-fixtures/resources/{dir}/metadata.json"
    )))
    .unwrap();
    let digest = {
        use md5::Digest;
        md5::Md5::digest(tsv.as_bytes())
    };
    let record = &mut metadata["ingredients"]["ingredients/OBS.tsv"];
    record["size"] = tsv.len().into();
    record["checksum"]["md5"] = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
        .into();
    let metadata = serde_json::to_vec(&metadata).unwrap();
    let zip = common::zip(&[
        ("en_obs-tq/metadata.json", &metadata),
        ("en_obs-tq/ingredients/OBS.tsv", tsv.as_bytes()),
    ]);
    let obs_tq = common::fixture_pin(dir, "unfoldingWord/en_obs-tq", "v10");
    tc_resources::install_dcs_zip(&store, &obs_tq, &zip, 1 << 30).unwrap();
    let english = set("en", "unfoldingWord")
        .with(SetSlot::ObsQuestions, obs_tq)
        .with(SetSlot::TranslationWords, en.tw.clone())
        .with(SetSlot::TranslationAcademy, en.ta.clone());
    let pins = ProjectPins {
        primary: Some(english.clone()),
        fallback: Some(english),
        ..ProjectPins::default()
    };
    let loaded = load_obs(&store, &pins);
    assert!(
        loaded.parts[&HelpsPart::ObsQuestions].is_loaded(),
        "{:?}",
        loaded.parts[&HelpsPart::ObsQuestions]
    );
    let read: usize = (1..=50)
        .flat_map(|story| (0..=20).filter_map(move |frame| ObsRef::new(story, frame)))
        .map(|frame| loaded.helps.obs_questions_for(frame).count())
        .sum();
    assert!(read >= good, "{read} questions read of {good} good rows");
}
