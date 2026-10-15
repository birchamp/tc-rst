//! Finding newer releases without installing anything.

// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use common::aquifer_fake::{FakeAsset, FakeRelease};
use common::{aquifer_pin, aquifer_rig, aquifer_zip, release, rig};
use tc_core::Testament;
use tc_door43::Door43Error;
use tc_resources::updates::{
    AquiferUpdate, apply_aquifer_upgrade, apply_upgrade, check_aquifer_updates, check_for_updates,
    offer_is_stale,
};
use tc_resources::{
    AquiferHelps, GatewayLanguage, LanguageSet, ProjectPins, ResourcePin, Rung, ScripturePane,
    SetSlot, install_all,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn newer_production_releases_are_offered_per_set_and_nothing_is_installed() {
    let rig = rig().await;
    let tn86 = release(&rig, "unfoldingWord/en_tn", "v86").await;
    let tn91 = release(&rig, "unfoldingWord/en_tn", "v91").await;
    let twl = release(&rig, "unfoldingWord/en_twl", "v86").await;
    let ult = release(&rig, "unfoldingWord/en_ult", "v89").await;
    let gone = ResourcePin::new(
        common::repo_path("unfoldingWord/en_ta"),
        "c".repeat(40).parse().unwrap(),
    );
    let english = LanguageSet::new(GatewayLanguage {
        language: "en".to_owned(),
        owner: "unfoldingWord".parse().unwrap(),
    })
    .with(SetSlot::TranslationNotes, tn86.clone())
    .with(SetSlot::TranslationWordsLinks, twl.clone())
    .with(SetSlot::TranslationAcademy, gone.clone());
    let pins = ProjectPins {
        primary: Some(english.clone()),
        fallback: Some(english),
        extra_scripture: vec![ScripturePane {
            id: "ult".parse().unwrap(),
            pin: ult.clone(),
        }],
        ..ProjectPins::default()
    };

    rig.fake.clear_requests();
    let report = check_for_updates(&rig.client, &pins, &[Testament::New])
        .await
        .unwrap();
    assert_eq!(report.sets.len(), 2);
    let primary = &report.sets[0];
    assert_eq!(primary.rung, Rung::Primary);
    assert_eq!(primary.upgrades.len(), 1);
    let upgrade = &primary.upgrades[0];
    // v92 is a pre-release and is never offered.
    assert_eq!(upgrade.to.version.as_ref().unwrap().as_str(), "v91");
    assert_eq!(upgrade.to.sha, tn91.sha);
    assert_eq!(upgrade.from, tn86);
    assert_eq!(upgrade.slots, [SetSlot::TranslationNotes]);
    assert_eq!(primary.current, std::slice::from_ref(&twl.repo));
    assert_eq!(
        primary.unanswered,
        std::slice::from_ref(&gone.repo),
        "a repository the catalog lacks is said so"
    );
    assert!(report.texts.is_empty(), "the pane is at its newest release");
    assert!(report.unanswered_texts.is_empty());
    let requests = rig.fake.requests();
    assert!(!requests.is_empty());
    assert!(
        requests.iter().all(|r| r.path == "/api/v1/catalog/search"),
        "only the catalog is asked: {requests:?}"
    );
    assert!(
        rig.store.installed().unwrap().is_empty(),
        "nothing is installed"
    );

    // Accepting: install every resource of the release first, then move the pins.
    install_all(&rig.store, &rig.client, std::slice::from_ref(&upgrade.to))
        .await
        .unwrap();
    let next = apply_upgrade(&pins, primary);
    assert_eq!(
        next.primary
            .as_ref()
            .unwrap()
            .pin(SetSlot::TranslationNotes)
            .unwrap()
            .sha,
        tn91.sha
    );
    assert_eq!(next.fallback, pins.fallback);
    assert!(offer_is_stale(primary, next.primary.as_ref()));
    assert!(!offer_is_stale(&report.sets[1], next.fallback.as_ref()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn checking_for_updates_offline_sends_nothing() {
    let rig = rig().await;
    let pins = tc_resources::suite::project_pins();
    rig.internet.turn_off();
    rig.fake.clear_requests();
    let err = check_for_updates(&rig.client, &pins, &[Testament::New])
        .await
        .unwrap_err();
    assert!(matches!(err, Door43Error::Offline { .. }), "{err}");
    assert!(rig.fake.requests().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aquifer_offers_a_newer_release_or_a_reuploaded_asset_by_digest() {
    let rig = aquifer_rig().await;
    let notes = aquifer_pin(
        "UWTranslationNotes",
        "por",
        &aquifer_zip("UWTranslationNotes", "por"),
    );
    let questions = aquifer_pin(
        "UWTranslationQuestions",
        "por",
        &aquifer_zip("UWTranslationQuestions", "por"),
    );
    let newer = aquifer_zip("UWTranslationNotes", "eng");
    rig.github.add_release(FakeRelease {
        owner: "BibleAquifer".to_owned(),
        repo: "UWTranslationNotes".to_owned(),
        tag: "v2026-10-19".to_owned(),
        commit: "e".repeat(40),
        published_at: "2026-10-19T00:00:00Z".to_owned(),
        assets: vec![FakeAsset {
            name: "Portuguese-json.zip".to_owned(),
            bytes: newer.clone(),
            listed_digest: true,
        }],
    });
    let helps = AquiferHelps {
        notes: Some(notes.clone()),
        questions: Some(questions.clone()),
        ..AquiferHelps::default()
    };
    let offers = check_aquifer_updates(&rig.client, &helps).await.unwrap();
    assert_eq!(offers.len(), 2);
    let AquiferUpdate::Offer { to, published } = &offers[0].update else {
        panic!("{:?}", offers[0].update);
    };
    assert_eq!(to.tag.as_str(), "v2026-10-19");
    assert_eq!(to.commit.as_str(), "e".repeat(40));
    assert_eq!(to.sha256, tc_resources::Sha256::of(&newer));
    assert_eq!(published.as_deref(), Some("2026-10-19T00:00:00Z"));
    assert_eq!(offers[1].update, AquiferUpdate::Current);
    assert!(
        rig.github
            .requests()
            .iter()
            .all(|r| !r.contains("/releases/download/")),
        "nothing is downloaded"
    );
    assert!(rig.store.installed().unwrap().is_empty());

    let next = apply_aquifer_upgrade(&helps, &offers[0]).unwrap();
    assert_eq!(next.notes.as_ref(), Some(to));
    assert_eq!(next.questions.as_ref(), Some(&questions));
    assert!(
        apply_aquifer_upgrade(&next, &offers[0]).is_none(),
        "a stale offer applies to nothing"
    );

    rig.internet.turn_off();
    assert!(check_aquifer_updates(&rig.client, &helps).await.is_err());
}
