//! The computed state of a decision (STORAGE.md §7.7), on the sample project's own
//! decisions and text, and tC4's revalidation suites (`revalidate`, `obsCheckSession`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::{apply, decision_of, file_resource, obs_notes, pin, sample, t, verse_content};
use serde_json::Value;
use tc_check::derive::notes_for_stories;
use tc_check::rail::{Filter, counts};
use tc_check::status::{Edit, Triage, assess};
use tc_check::{
    CheckState, Collection, DecisionRecord, Dot, Judgement, Ladder, LanguageSets, ResourceRef,
    Selection, ToolId, UserStatus, selection,
};
use tc_core::BookId;
use tc_journal::decision::verse_text_md5_v1;
use tc_journal::json::JsonMap;
use tc_journal::record::PinGroupValue;

const VERSE: &str = "Pablo, siervo de Dios y apóstol de Jesucristo, según la fe de los escogidos";

fn resource() -> ResourceRef {
    pin("Es-419_gl/es-419_tn", "v66", &[]).resource
}

fn dios() -> Vec<Selection> {
    vec![Selection {
        text: "Dios".into(),
        occurrence: 1,
        occurrences: 1,
        extra: JsonMap::new(),
    }]
}

fn valid_on(text: &str, words: Vec<Selection>) -> DecisionRecord {
    apply(None, Edit::Triage(Triage::Valid(words)), text, &resource())
}

#[test]
fn a_fresh_decision_holds() {
    let d = valid_on(VERSE, dios());
    let a = assess(&d, Some(&resource()), Some(&t(VERSE)));
    assert_eq!(a.invalidation, None);
    assert!(!a.verse_edits);
    assert_eq!(a.status, Some(UserStatus::Valid));
    let state = CheckState::of(Some(&d), Some(&resource()), Some(&t(VERSE)));
    assert!(state.is_decided());
    assert_eq!(state.dot(), Dot::Valid);
}

#[test]
fn an_edit_that_keeps_the_selected_words_keeps_the_decision() {
    let d = valid_on(VERSE, dios());
    let edited = VERSE.replace("Pablo", "Paulo");
    let a = assess(&d, Some(&resource()), Some(&t(&edited)));
    assert!(a.verse_edits);
    assert_eq!(a.invalidation, None);
}

#[test]
fn an_edit_that_loses_the_selected_words_invalidates_and_keeps_the_decision() {
    let d = valid_on(VERSE, dios());
    let edited = VERSE.replace("Dios", "Señor");
    let state = CheckState::of(Some(&d), Some(&resource()), Some(&t(&edited)));
    let a = state.assessment.unwrap();
    let why = a.invalidation.unwrap();
    assert!(why.selections_lost && !why.resource_changed && !why.imported);
    assert_eq!(a.status, Some(UserStatus::Invalid));
    assert!(!a.user_invalid);
    assert!(!state.is_decided());
    assert_eq!(state.dot(), Dot::Invalid);
    // Nothing was rewritten: the old text makes it valid again.
    assert_eq!(
        assess(&d, Some(&resource()), Some(&t(VERSE))).invalidation,
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
        assess(&again, Some(&resource()), Some(&t(&edited))).invalidation,
        None
    );
    assert!(again.judgement.nothing_to_select && again.judgement.selections.is_none());
}

#[test]
fn an_invalidated_decision_shows_the_persons_to_do_and_marks_their_invalid() {
    let edited = VERSE.replace("Dios", "Señor");
    let mut d = valid_on(VERSE, dios());
    d.judgement.status = Some(UserStatus::Todo);
    let a = assess(&d, Some(&resource()), Some(&t(&edited)));
    assert!(a.invalidation.is_some());
    assert_eq!(a.status, Some(UserStatus::Todo));
    assert!(!a.user_invalid);
    d.judgement.status = Some(UserStatus::Invalid);
    let a = assess(&d, Some(&resource()), Some(&t(&edited)));
    assert_eq!(a.status, Some(UserStatus::Invalid));
    assert!(a.user_invalid);
}

#[test]
fn a_resource_change_invalidates_and_pinning_back_restores() {
    let d = valid_on(VERSE, dios());
    let upgraded = pin("Es-419_gl/es-419_tn", "v67", &[]).resource;
    let why = assess(&d, Some(&upgraded), Some(&t(VERSE)))
        .invalidation
        .unwrap();
    assert!(why.resource_changed && !why.selections_lost);
    assert!(
        assess(&d, None, Some(&t(VERSE)))
            .invalidation
            .unwrap()
            .resource_changed
    );
    // The repository path compares without letter case; the commit is exact.
    let mut recased = d.clone();
    recased.resource.repo_path = "git.door43.org/es-419_GL/ES-419_TN".into();
    assert_eq!(
        assess(&recased, Some(&resource()), Some(&t(VERSE))).invalidation,
        None
    );
}

#[test]
fn an_imported_invalidation_holds_until_the_person_decides_again() {
    let mut d = valid_on(VERSE, dios());
    d.imported_invalid = true;
    assert!(
        assess(&d, Some(&resource()), Some(&t(VERSE)))
            .invalidation
            .unwrap()
            .imported
    );
    let bookmarked = apply(Some(&d), Edit::Bookmark(true), VERSE, &resource());
    assert!(bookmarked.imported_invalid && bookmarked.bookmarked);
    // To do is not a judgement: the imported invalidation stays, and the words too.
    let todo = apply(Some(&d), Edit::Triage(Triage::Todo), VERSE, &resource());
    assert!(todo.imported_invalid);
    assert_eq!(
        todo.judgement.selections,
        Some(dios()),
        "To do keeps the words"
    );
    let state = CheckState::of(Some(&todo), Some(&resource()), Some(&t(VERSE)));
    assert!(!state.is_decided() && Filter::ToDo.matches(&state));
    let decided = apply(
        Some(&todo),
        Edit::Triage(Triage::Valid(dios())),
        VERSE,
        &resource(),
    );
    assert!(!decided.imported_invalid);
    assert!(CheckState::of(Some(&decided), Some(&resource()), Some(&t(VERSE))).is_decided());
}

#[test]
fn an_invalidated_check_put_back_to_do_stays_to_do() {
    // tC4 `recordDecision({status: 'todo'})` keeps `invalidated: true`.
    let d = valid_on(VERSE, dios());
    let edited = VERSE.replace("Dios", "Señor");
    let todo = apply(Some(&d), Edit::Triage(Triage::Todo), &edited, &resource());
    assert_eq!(todo.judgement.verse_text_md5, verse_text_md5_v1(VERSE));
    let state = CheckState::of(Some(&todo), Some(&resource()), Some(&t(&edited)));
    assert!(state.invalidation().is_some_and(|i| i.selections_lost));
    assert_eq!(state.status(), Some(UserStatus::Todo));
    assert!(!state.is_decided());
    assert_eq!(counts(std::slice::from_ref(&state))[1], (Filter::ToDo, 1));
    // The rail shows it red, and the Invalid chip, which reads the status, leaves it out.
    assert_eq!(state.dot(), Dot::Invalid);
    assert!(!Filter::Invalid.matches(&state));

    // The same after an upgrade: the decision keeps the resource it was made against.
    let upgraded = pin("Es-419_gl/es-419_tn", "v67", &[]).resource;
    let todo = apply(Some(&d), Edit::Triage(Triage::Todo), VERSE, &upgraded);
    assert_eq!(todo.resource, resource());
    let state = CheckState::of(Some(&todo), Some(&upgraded), Some(&t(VERSE)));
    assert!(state.invalidation().is_some_and(|i| i.resource_changed));
    assert!(!state.is_decided() && Filter::ToDo.matches(&state));
}

#[test]
fn deciding_again_judges_the_text_and_the_list_in_front_of_the_person() {
    let d = valid_on(VERSE, dios());
    let edited = VERSE.replace("Pablo", "Paulo y Dios");
    let upgraded = pin("Es-419_gl/es-419_tn", "v67", &[]).resource;
    let before = assess(&d, Some(&upgraded), Some(&t(&edited)));
    let why = before.invalidation.unwrap();
    assert!(why.selections_lost && why.resource_changed && before.verse_edits);

    let words = selection::from_indices(&t(&edited), &[2]).unwrap();
    assert_eq!((words[0].text.as_str(), words[0].occurrences), ("Dios", 2));
    let again = apply(
        Some(&d),
        Edit::Triage(Triage::Valid(words)),
        &edited,
        &upgraded,
    );
    assert_eq!(again.judgement.verse_text_md5, verse_text_md5_v1(&edited));
    assert_eq!(again.resource, upgraded);
    let after = assess(&again, Some(&upgraded), Some(&t(&edited)));
    assert_eq!(after.invalidation, None);
    assert!(
        !after.verse_edits,
        "the decision is about the edited text now"
    );
    assert_eq!(after.status, Some(UserStatus::Valid));

    let invalid = apply(Some(&d), Edit::Triage(Triage::Invalid), &edited, &upgraded);
    let a = assess(&invalid, Some(&upgraded), Some(&t(&edited)));
    assert_eq!(a.invalidation, None);
    assert_eq!(a.status, Some(UserStatus::Invalid));
    assert!(
        !a.user_invalid,
        "an Invalid that holds is not an invalidation"
    );
}

#[test]
fn an_empty_comment_is_no_comment() {
    let mut d = valid_on(VERSE, dios());
    d.comment = Some(String::new());
    let state = CheckState::of(Some(&d), Some(&resource()), Some(&t(VERSE)));
    assert!(!state.commented && !Filter::Commented.matches(&state));
    d.comment = Some("¿Dios o Señor?".into());
    assert!(CheckState::of(Some(&d), Some(&resource()), Some(&t(VERSE))).commented);
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
    assert!(t(VERSE).fits(&dios()));
    let untouched = DecisionRecord {
        judgement: Judgement {
            selections: None,
            ..d.judgement.clone()
        },
        ..d.clone()
    };
    assert_eq!(
        assess(&untouched, Some(&resource()), Some(&t(&replaced))).invalidation,
        None
    );
    let nothing = apply(
        None,
        Edit::Triage(Triage::Valid(vec![])),
        VERSE,
        &resource(),
    );
    assert_eq!(
        assess(&nothing, Some(&resource()), Some(&t(&replaced))).invalidation,
        None
    );
    assert!(CheckState::of(Some(&nothing), Some(&resource()), Some(&t(&replaced))).is_decided());
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
    assert_eq!(valid.judgement.verse_text_md5, verse_text_md5_v1(VERSE));
    let invalid = apply(
        Some(&valid),
        Edit::Triage(Triage::Invalid),
        VERSE,
        &resource(),
    );
    assert_eq!(invalid.judgement.selections, None);
    assert_eq!(invalid.judgement.status, Some(UserStatus::Invalid));
    let state = CheckState::of(Some(&invalid), Some(&resource()), Some(&t(VERSE)));
    assert!(state.is_decided(), "an Invalid is a decision");
    let commented = apply(
        Some(&invalid),
        Edit::Comment("¿Dios o Señor?".into()),
        VERSE,
        &resource(),
    );
    assert_eq!(commented.judgement, invalid.judgement);
    assert!(CheckState::of(Some(&commented), Some(&resource()), Some(&t(VERSE))).commented);
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
    let pins = LanguageSets {
        primary: PinGroupValue::from_json(&pins_json["languageSets"]["primary"]),
        fallback: PinGroupValue::from_json(&pins_json["languageSets"]["fallback"]),
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
        let resolution = pins
            .resolution(tool, Collection::Book(BookId::Titus))
            .unwrap();
        let resolved = resolution.resource().unwrap();
        assert_eq!(
            resolved,
            &file_resource(&file),
            "{tool}: the file records the primary pin"
        );
        for json in file["decisions"].as_array().unwrap() {
            let reference = &json["contextId"]["reference"];
            let chapter = u16::try_from(reference["chapter"].as_u64().unwrap()).unwrap();
            let verse = u16::try_from(reference["verse"].as_u64().unwrap()).unwrap();
            let text = verse_content(&usfm, chapter, verse);
            let d = decision_of(json, &file_resource(&file), &text);
            let state = CheckState::of(Some(&d), Some(resolved), Some(&t(&text)));
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
            let after = CheckState::of(Some(&d), Some(resolved), Some(&t(&edited)));
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
fn every_word_of_the_sample_titus_holds_through_an_edit_elsewhere_in_its_verse() {
    // tC4 recorded "de" in a verse that also says "desde" with a word count its own
    // validator reads as changed; a selection recorded here holds until its text changes.
    let usfm = sample("TIT.usfm");
    let book = tc_usfm::Book::parse(&usfm);
    let mut words = 0;
    for verse in book.verses() {
        let content = verse.body();
        let text = t(content);
        let edited = t(&format!("{}!", content.trim_end()));
        for i in 0..selection::target_words(text.shown()).len() {
            let picked = selection::from_indices(&text, &[i]).unwrap();
            let d = valid_on(content, picked.clone());
            let a = assess(&d, Some(&resource()), Some(&edited));
            assert!(a.verse_edits);
            assert_eq!(a.invalidation, None, "{picked:?} in {text:?}");
            words += 1;
        }
    }
    assert_eq!(words, 114, "the drafted words of the sample Titus");
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
    let words = selection::from_indices(&t(frame), &[8]).unwrap();
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
    let state = CheckState::of(Some(&d), Some(&obs), Some(&t(frame)));
    assert!(state.is_decided());
    let flagged = CheckState::of(Some(&d), Some(&obs), Some(&t(edited)));
    assert_eq!(flagged.status(), Some(UserStatus::Invalid));
    assert!(flagged.invalidation().is_some() && !flagged.is_decided());
    assert!(flagged.bookmarked && flagged.commented);
    assert_eq!(d.judgement.selections, Some(words));
}
