//! The computed state of a decision (STORAGE.md §7.7), on the sample project's own
//! decisions and text, and tC4's revalidation suites (`revalidate`, `obsCheckSession`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{decision_of, file_resource, obs_notes, pin, sample, verse_text};
use serde_json::Value;
use tc_check::derive::notes_for_stories;
use tc_check::resolve::{LanguageSet, resolve};
use tc_check::status::{Edit, Triage, apply, assess};
use tc_check::{
    CheckState, Collection, Decision, Dot, Judgement, Pins, ResourceRef, Selection, TextMd5,
    ToolId, UserStatus, selection,
};
use tc_core::BookId;

const VERSE: &str = "Pablo, siervo de Dios y apóstol de Jesucristo, según la fe de los escogidos";

fn resource() -> ResourceRef {
    pin("Es-419_gl/es-419_tn", "v66", &[]).resource
}

fn dios() -> Vec<Selection> {
    vec![Selection {
        text: "Dios".into(),
        occurrence: 1,
        occurrences: 1,
    }]
}

fn valid_on(text: &str, words: Vec<Selection>) -> Decision {
    apply(None, Edit::Triage(Triage::Valid(words)), text, &resource())
}

#[test]
fn a_fresh_decision_holds() {
    let d = valid_on(VERSE, dios());
    let a = assess(&d, Some(&resource()), Some(VERSE));
    assert_eq!(a.invalidation, None);
    assert!(!a.verse_edits);
    assert_eq!(a.status, Some(UserStatus::Valid));
    let state = CheckState::of(Some(&d), Some(&resource()), Some(VERSE));
    assert!(state.is_decided());
    assert_eq!(state.dot(), Dot::Valid);
}

#[test]
fn an_edit_that_keeps_the_selected_words_keeps_the_decision() {
    let d = valid_on(VERSE, dios());
    let edited = VERSE.replace("Pablo", "Paulo");
    let a = assess(&d, Some(&resource()), Some(&edited));
    assert!(a.verse_edits);
    assert_eq!(a.invalidation, None);
}

#[test]
fn an_edit_that_loses_the_selected_words_invalidates_and_keeps_the_decision() {
    let d = valid_on(VERSE, dios());
    let edited = VERSE.replace("Dios", "Señor");
    let state = CheckState::of(Some(&d), Some(&resource()), Some(&edited));
    let a = state.assessment.unwrap();
    let why = a.invalidation.unwrap();
    assert!(why.selections_lost && !why.resource_changed && !why.imported);
    assert_eq!(a.status, Some(UserStatus::Invalid));
    assert!(!a.user_invalid);
    assert!(!state.is_decided());
    assert_eq!(state.dot(), Dot::Invalid);
    // Nothing was rewritten: the old text makes it valid again.
    assert_eq!(
        assess(&d, Some(&resource()), Some(VERSE)).invalidation,
        None
    );
    // The person decides it again on the new text, and it holds.
    let again = apply(
        Some(&d),
        Edit::Triage(Triage::Valid(vec![])),
        &edited,
        &resource(),
    );
    assert_eq!(
        assess(&again, Some(&resource()), Some(&edited)).invalidation,
        None
    );
    assert!(again.judgement.nothing_to_select && again.judgement.selections.is_none());
}

#[test]
fn an_invalidated_decision_shows_the_persons_to_do_and_marks_their_invalid() {
    let edited = VERSE.replace("Dios", "Señor");
    let mut d = valid_on(VERSE, dios());
    d.judgement.status = Some(UserStatus::Todo);
    let a = assess(&d, Some(&resource()), Some(&edited));
    assert!(a.invalidation.is_some());
    assert_eq!(a.status, Some(UserStatus::Todo));
    assert!(!a.user_invalid);
    d.judgement.status = Some(UserStatus::Invalid);
    let a = assess(&d, Some(&resource()), Some(&edited));
    assert_eq!(a.status, Some(UserStatus::Invalid));
    assert!(a.user_invalid);
}

#[test]
fn a_resource_change_invalidates_and_pinning_back_restores() {
    let d = valid_on(VERSE, dios());
    let upgraded = pin("Es-419_gl/es-419_tn", "v67", &[]).resource;
    let why = assess(&d, Some(&upgraded), Some(VERSE))
        .invalidation
        .unwrap();
    assert!(why.resource_changed && !why.selections_lost);
    assert!(
        assess(&d, None, Some(VERSE))
            .invalidation
            .unwrap()
            .resource_changed
    );
    // The repository path compares without letter case; the commit is exact.
    let mut recased = d.clone();
    recased.resource.repo_path =
        tc_check::RepoPath::parse("git.door43.org/es-419_GL/ES-419_TN").unwrap();
    assert_eq!(
        assess(&recased, Some(&resource()), Some(VERSE)).invalidation,
        None
    );
}

#[test]
fn an_imported_invalidation_holds_until_the_person_decides_again() {
    let mut d = valid_on(VERSE, dios());
    d.imported_invalid = true;
    assert!(
        assess(&d, Some(&resource()), Some(VERSE))
            .invalidation
            .unwrap()
            .imported
    );
    let bookmarked = apply(Some(&d), Edit::Bookmark(true), VERSE, &resource());
    assert!(bookmarked.imported_invalid && bookmarked.bookmarked);
    let decided = apply(Some(&d), Edit::Triage(Triage::Todo), VERSE, &resource());
    assert!(!decided.imported_invalid);
    assert_eq!(
        decided.judgement.selections,
        Some(dios()),
        "To do keeps the words"
    );
}

#[test]
fn no_verse_text_is_no_evidence() {
    let d = valid_on(VERSE, dios());
    assert_eq!(assess(&d, Some(&resource()), None).invalidation, None);
}

#[test]
fn tc4_revalidation_cases() {
    // tC4 test/revalidate.test.ts, on the computed state.
    let d = valid_on(VERSE, dios());
    let replaced = VERSE.replace("Dios", "X");
    assert!(!selection::validate(VERSE, &dios()).changed);
    let untouched = Decision {
        judgement: Judgement {
            selections: None,
            ..d.judgement.clone()
        },
        ..d.clone()
    };
    assert_eq!(
        assess(&untouched, Some(&resource()), Some(&replaced)).invalidation,
        None
    );
    let nothing = apply(
        None,
        Edit::Triage(Triage::Valid(vec![])),
        VERSE,
        &resource(),
    );
    assert_eq!(
        assess(&nothing, Some(&resource()), Some(&replaced)).invalidation,
        None
    );
    assert!(CheckState::of(Some(&nothing), Some(&resource()), Some(&replaced)).is_decided());
}

#[test]
fn triage_buttons_record_what_tc4_records() {
    let words = dios();
    let valid = apply(
        None,
        Edit::Triage(Triage::Valid(words.clone())),
        VERSE,
        &resource(),
    );
    assert_eq!(valid.judgement.selections, Some(words));
    assert!(!valid.judgement.nothing_to_select);
    assert_eq!(valid.judgement.verse_text_md5, TextMd5::of(VERSE));
    let invalid = apply(
        Some(&valid),
        Edit::Triage(Triage::Invalid),
        VERSE,
        &resource(),
    );
    assert_eq!(invalid.judgement.selections, None);
    assert_eq!(invalid.judgement.status, Some(UserStatus::Invalid));
    let state = CheckState::of(Some(&invalid), Some(&resource()), Some(VERSE));
    assert!(state.is_decided(), "an Invalid is a decision");
    let commented = apply(
        Some(&invalid),
        Edit::Comment("¿Dios o Señor?".into()),
        VERSE,
        &resource(),
    );
    assert_eq!(commented.judgement, invalid.judgement);
    assert!(CheckState::of(Some(&commented), Some(&resource()), Some(VERSE)).commented);
    let cleared = apply(
        Some(&commented),
        Edit::Comment(String::new()),
        VERSE,
        &resource(),
    );
    assert_eq!(cleared.comment, None);
}

// ---- the sample project ------------------------------------------------------------

#[test]
fn the_sample_project_s_decisions_compute_as_tc4_shows_them() {
    let pins_json: Value = serde_json::from_str(&sample("checking/resources.json")).unwrap();
    let pins = Pins {
        primary: Some(LanguageSet::from_json(&pins_json["languageSets"]["primary"]).unwrap()),
        fallback: Some(LanguageSet::from_json(&pins_json["languageSets"]["fallback"]).unwrap()),
    };
    let usfm = sample("TIT.usfm");
    let mut seen = Vec::new();
    for (tool, file) in [
        (
            ToolId::TranslationNotes,
            "checking/translationNotes/TIT.json",
        ),
        (
            ToolId::TranslationWords,
            "checking/translationWords/TIT.json",
        ),
    ] {
        let file: Value = serde_json::from_str(&sample(file)).unwrap();
        let resolution = resolve(&pins, tool, Collection::Book(BookId::Titus)).unwrap();
        assert_eq!(
            resolution.pin.resource,
            file_resource(&file),
            "{tool}: the file records the primary pin"
        );
        for json in file["decisions"].as_array().unwrap() {
            let reference = &json["contextId"]["reference"];
            let chapter = u16::try_from(reference["chapter"].as_u64().unwrap()).unwrap();
            let verse = u16::try_from(reference["verse"].as_u64().unwrap()).unwrap();
            let text = verse_text(&usfm, chapter, verse);
            let d = decision_of(json, &file_resource(&file), &text);
            let state = CheckState::of(Some(&d), Some(&resolution.pin.resource), Some(&text));
            seen.push((
                json["contextId"]["checkId"].as_str().unwrap().to_owned(),
                state.is_decided(),
                state.dot(),
                state.invalidation().map(|i| i.imported),
                state.bookmarked,
                state.commented,
            ));
            // An edit that removes one "Dios" from 1:1 invalidates the decision on Θεοῦ,
            // whose selection counted two, and leaves the others.
            let edited = text.replacen("Dios", "Señor", 1);
            let after = CheckState::of(Some(&d), Some(&resolution.pin.resource), Some(&edited));
            let lost = after.invalidation().is_some_and(|i| i.selections_lost);
            assert_eq!(lost, json["contextId"]["checkId"] == "t1g7", "{json}");
        }
    }
    assert_eq!(
        seen,
        [
            ("gr8c".into(), false, Dot::Invalid, Some(true), false, false),
            ("swi9".into(), true, Dot::Todo, None, false, true),
            ("a9p2".into(), true, Dot::Todo, None, true, false),
            ("t1g7".into(), true, Dot::Valid, None, false, false),
        ]
    );
}

#[test]
fn a_frame_edit_invalidates_a_story_decision_and_keeps_it() {
    // tC4 test/obsCheckSession.test.ts: "the beginning" on story 1, frame 1.
    let list = notes_for_stories(&obs_notes("en_obs-tn@v13/OBS.tsv"), Some(1));
    let item = list
        .items
        .iter()
        .find(|i| i.key.check_id.as_str() == "lm48")
        .unwrap();
    assert_eq!(item.key.target.locator(), "1:1");
    let frame = "Así fue como Dios hizo todo en el principio. Creó el universo y todo lo que hay en él en seis días.";
    let edited = "Así fue como el Señor hizo todo al comienzo.";
    let obs = pin("unfoldingWord/en_obs-tn", "v13", &[]).resource;
    let words = selection::from_indices(frame, &[8]).unwrap();
    assert_eq!(words[0].text, "principio");
    let mut d = apply(
        None,
        Edit::Triage(Triage::Valid(words.clone())),
        frame,
        &obs,
    );
    d = apply(Some(&d), Edit::Bookmark(true), frame, &obs);
    d = apply(
        Some(&d),
        Edit::Comment("Preguntar al equipo.".into()),
        frame,
        &obs,
    );
    let state = CheckState::of(Some(&d), Some(&obs), Some(frame));
    assert!(state.is_decided());
    let flagged = CheckState::of(Some(&d), Some(&obs), Some(edited));
    assert_eq!(flagged.status(), Some(UserStatus::Invalid));
    assert!(flagged.invalidation().is_some() && !flagged.is_decided());
    assert!(flagged.bookmarked && flagged.commented);
    assert_eq!(d.judgement.selections, Some(words));
}
