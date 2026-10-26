//! The versions a project pins that this computer lacks: the count a sync result offers
//! Download for, and the pin that Download must fetch (tC4 #678, D88 point 6). Ported from
//! tC4 `test/syncButton.test.tsx:229-266` and `test/installed.test.ts:412-507`.

// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::{fixture, repo_path};
use tc_resources::resolve::{missing_pin_count, unsatisfied_pin_for};
use tc_resources::{
    GatewayLanguage, LanguageSet, ProjectPins, ResourcePin, ScripturePane, SetSlot,
};

fn pin(repo: &str, sha: char, version: &str) -> ResourcePin {
    ResourcePin::new(repo_path(repo), sha.to_string().repeat(40).parse().unwrap())
        .with_version(version.parse().unwrap())
        .with_flavor("scripture/textTranslation")
}

fn english() -> LanguageSet {
    LanguageSet::new(GatewayLanguage {
        language: "en".to_owned(),
        owner: "unfoldingWord".parse().unwrap(),
    })
}

/// What a computer holds, as the store answers it: the exact commit of the repository.
fn holding(installs: Vec<ResourcePin>) -> impl Fn(&ResourcePin) -> bool {
    move |pin| installs.iter().any(|held| held.same_identity(pin))
}

/// The conformance sample's pin file, which tC4's tests read too.
fn sample() -> ProjectPins {
    let json: serde_json::Value = serde_json::from_slice(&fixture(
        "tc4/sample-burrito/ingredients/checking/resources.json",
    ))
    .unwrap();
    ProjectPins::from_resources_json(&json).unwrap()
}

/// Every pin of the sample that Source texts can download: both sets, the
/// original-language texts and the panes. The lexicons are not among them.
fn sample_downloadable(pins: &ProjectPins) -> Vec<ResourcePin> {
    let mut out: Vec<ResourcePin> = Vec::new();
    for set in [&pins.primary, &pins.fallback].into_iter().flatten() {
        out.extend(set.pins.values().cloned());
    }
    out.extend(pins.original_language.nt.iter().cloned());
    out.extend(pins.original_language.ot.iter().cloned());
    out.extend(pins.extra_scripture.iter().map(|pane| pane.pin.clone()));
    out
}

#[test]
fn d88_6_the_count_is_each_pin_that_names_a_version_this_computer_lacks() {
    let ult = pin("unfoldingWord/en_ult", 'a', "v91");
    let uhb = pin("unfoldingWord/hbo_uhb", 'b', "v3.0.0");
    let mut pins = ProjectPins {
        primary: Some(english().with(SetSlot::SimplifiedText, ult.clone())),
        ..ProjectPins::default()
    };
    pins.original_language.ot = Some(uhb.clone());

    // Control: every version installed counts 0.
    assert_eq!(
        missing_pin_count(&pins, &holding(vec![ult.clone(), uhb.clone()])),
        0
    );
    // The original-language pin names a version this computer lacks.
    assert_eq!(missing_pin_count(&pins, &holding(vec![ult.clone()])), 1);
    // The same repository at another commit is not the pinned version (D58).
    let other_commit = pin("unfoldingWord/hbo_uhb", 'c', "v3.0.0");
    assert_eq!(
        missing_pin_count(&pins, &holding(vec![ult, other_commit])),
        1
    );
    // No pins count 0.
    assert_eq!(missing_pin_count(&ProjectPins::default(), &|_| false), 0);
}

#[test]
fn d88_6_a_sync_whose_only_missing_version_is_a_source_pane_counts_it() {
    let pins = sample();
    let installs = sample_downloadable(&pins);
    // The sample pins both lexicons, and nothing here installs them.
    assert!(pins.lexicon.nt.is_some() && pins.lexicon.ot.is_some());
    assert_eq!(missing_pin_count(&pins, &holding(installs.clone())), 0);

    let ult = &pins
        .extra_scripture
        .iter()
        .find(|pane| pane.id.as_str() == "ult")
        .unwrap()
        .pin;
    let without_ult: Vec<ResourcePin> = installs
        .iter()
        .filter(|held| !held.is_repo(ult.repo.repo()))
        .cloned()
        .collect();
    assert_eq!(missing_pin_count(&pins, &holding(without_ult)), 1);

    // The UST is pinned twice, as the fallback's simplified Bible and as a pane, and
    // counts once per place, as in tC4.
    let ust = &pins.extra_scripture[1].pin;
    let without_ust: Vec<ResourcePin> = installs
        .iter()
        .filter(|held| !held.is_repo(ust.repo.repo()))
        .cloned()
        .collect();
    assert_eq!(missing_pin_count(&pins, &holding(without_ust)), 2);
}

#[test]
fn an_extra_scripture_pin_whose_sha_is_not_local_is_the_pin_a_download_fetches() {
    let pins = sample();
    let ult = &pins
        .extra_scripture
        .iter()
        .find(|pane| pane.id.as_str() == "ult")
        .unwrap()
        .pin;
    assert_eq!(
        unsatisfied_pin_for(&pins, ult.repo.repo(), &|_| false),
        Some(ult)
    );
    // Control: the exact identity is installed, so there is nothing to fetch.
    assert_eq!(
        unsatisfied_pin_for(&pins, ult.repo.repo(), &holding(vec![ult.clone()])),
        None
    );
}

#[test]
fn round_20_a_download_must_satisfy_the_project_pin() {
    let tq = pin("unfoldingWord/en_tq", 'a', "v88");
    let tq_repo = tq.repo.repo().clone();
    let set = || {
        english().with(
            SetSlot::TranslationNotes,
            pin("unfoldingWord/en_tn", 'e', "v89"),
        )
    };
    let with_tq = ProjectPins {
        primary: Some(set().with(SetSlot::TranslationQuestions, tq.clone())),
        fallback: Some(set()),
        ..ProjectPins::default()
    };

    // Version skew: a same-repository install at the catalogue's latest commit does not
    // satisfy the pin.
    let latest = pin("unfoldingWord/en_tq", 'b', "v89");
    assert_eq!(
        unsatisfied_pin_for(&with_tq, &tq_repo, &holding(vec![latest])),
        Some(&tq)
    );
    // An exact-identity install satisfies it.
    assert_eq!(
        unsatisfied_pin_for(&with_tq, &tq_repo, &holding(vec![tq.clone()])),
        None
    );
    // A repository the project does not pin has no wanted identity.
    let without_tq = ProjectPins {
        primary: Some(set()),
        ..ProjectPins::default()
    };
    assert_eq!(unsatisfied_pin_for(&without_tq, &tq_repo, &|_| false), None);
    assert_eq!(
        unsatisfied_pin_for(&ProjectPins::default(), &tq_repo, &|_| false),
        None
    );

    // The original-language pins are searched, after the language sets.
    let ugnt_in_set = pin("unfoldingWord/el-x-koine_ugnt", '1', "v1");
    let ugnt = pin("unfoldingWord/el-x-koine_ugnt", '2', "v2");
    let ugnt_repo = ugnt.repo.repo().clone();
    let mut original_only = ProjectPins::default();
    original_only.original_language.nt = Some(ugnt.clone());
    assert_eq!(
        unsatisfied_pin_for(&original_only, &ugnt_repo, &|_| false),
        Some(&ugnt)
    );
    assert_eq!(
        unsatisfied_pin_for(&original_only, &ugnt_repo, &holding(vec![ugnt.clone()])),
        None
    );
    let mut both = ProjectPins {
        primary: Some(set().with(SetSlot::SimplifiedText, ugnt_in_set.clone())),
        ..ProjectPins::default()
    };
    both.original_language.nt = Some(ugnt);
    assert_eq!(
        unsatisfied_pin_for(&both, &ugnt_repo, &|_| false),
        Some(&ugnt_in_set)
    );
}

#[test]
fn the_lookup_takes_the_primary_set_then_the_fallback_set_then_the_panes() {
    // tC4 `unsatisfiedProjectPinFor` on a project that pins en_ust three times: "The
    // primary rung wins when both rungs pin the repo", and the panes come last.
    let primary = pin("unfoldingWord/en_ust", 'a', "v86");
    let fallback = pin("unfoldingWord/en_ust", 'b', "v87");
    let pane = pin("unfoldingWord/en_ust", 'c', "v88");
    let ust = primary.repo.repo().clone();
    let pins = ProjectPins {
        primary: Some(english().with(SetSlot::SimplifiedText, primary.clone())),
        fallback: Some(english().with(SetSlot::SimplifiedText, fallback.clone())),
        extra_scripture: vec![ScripturePane {
            id: "ust".parse().unwrap(),
            pin: pane.clone(),
        }],
        ..ProjectPins::default()
    };

    assert_eq!(unsatisfied_pin_for(&pins, &ust, &|_| false), Some(&primary));
    assert_eq!(
        unsatisfied_pin_for(&pins, &ust, &holding(vec![primary.clone()])),
        Some(&fallback)
    );
    assert_eq!(
        unsatisfied_pin_for(
            &pins,
            &ust,
            &holding(vec![primary.clone(), fallback.clone()])
        ),
        Some(&pane)
    );
    assert_eq!(
        unsatisfied_pin_for(&pins, &ust, &holding(vec![primary, fallback, pane])),
        None
    );
}
