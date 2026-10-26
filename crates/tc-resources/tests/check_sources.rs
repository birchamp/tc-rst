//! What a check reads: the Door43 check list and the Door43 original-language text,
//! whatever the helps tabs read from.

// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::{fixture, install_fixture, release, rig};
use tc_core::{BookId, Testament};
use tc_door43_fake::{CommitSeed, ReleaseKind, RepoSeed, SeedFile};
use tc_resources::{
    CheckBlocker, GatewayLanguage, LanguageSet, Packaging, ProjectPins, SetSlot, ToolId,
    check_sources, install_dcs_as, resolve::PreflightState,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_notes_check_needs_the_door43_original_language_text() {
    let rig = rig().await;
    // The Hebrew Bible as its repository holds it: `32-JON.usfm` at the tag.
    rig.fake
        .add_repo(RepoSeed {
            owner: "unfoldingWord".to_owned(),
            name: "hbo_uhb".to_owned(),
            subject: "Hebrew Old Testament".to_owned(),
            language: "hbo".to_owned(),
            topics: vec!["tc-ready".to_owned()],
            private: false,
            commits: vec![CommitSeed {
                message: "Release v3.0.0".to_owned(),
                files: vec![SeedFile::regular(
                    "32-JON.usfm",
                    fixture("tc4/test-fixtures/hbo_uhb/JON.usfm"),
                )],
                tag: Some("v3.0.0".to_owned()),
                release: Some(ReleaseKind::Published),
            }],
        })
        .await
        .unwrap();
    let uhb = release(&rig, "unfoldingWord/hbo_uhb", "v3.0.0").await;
    let tn = install_fixture(
        &rig.store,
        "en_tn@v86",
        "unfoldingWord/en_tn",
        "v86",
        &[
            ("TIT.tsv", "ingredients/TIT.tsv"),
            ("JON.tsv", "ingredients/JON.tsv"),
        ],
    );
    let english = LanguageSet::new(GatewayLanguage {
        language: "en".to_owned(),
        owner: "unfoldingWord".parse().unwrap(),
    })
    .with(SetSlot::TranslationNotes, tn.clone());
    let mut pins = ProjectPins {
        primary: Some(english.clone()),
        fallback: Some(english),
        ..ProjectPins::default()
    };
    pins.original_language.ot = Some(uhb.clone());

    let blocked = check_sources(
        &rig.store,
        &pins,
        ToolId::TranslationNotes,
        BookId::Jonah,
        false,
    )
    .unwrap_err();
    assert_eq!(
        blocked,
        vec![CheckBlocker::OriginalLanguageNotInstalled(uhb.clone())]
    );

    install_dcs_as(&rig.store, &rig.client, &uhb, Packaging::Archive)
        .await
        .unwrap();
    let sources = check_sources(
        &rig.store,
        &pins,
        ToolId::TranslationNotes,
        BookId::Jonah,
        false,
    )
    .unwrap();
    assert_eq!(sources.list.pin, tn);
    assert_eq!(sources.original_language, uhb);
    assert_eq!(sources.articles, None);

    // Titus is in the New Testament, for which no original-language text is pinned.
    let blocked = check_sources(
        &rig.store,
        &pins,
        ToolId::TranslationNotes,
        BookId::Titus,
        false,
    )
    .unwrap_err();
    assert_eq!(
        blocked,
        vec![CheckBlocker::OriginalLanguageNotPinned(Testament::New)]
    );

    // The words check has no links pinned, and the Hebrew Bible lacks Ruth.
    let blocked = check_sources(
        &rig.store,
        &pins,
        ToolId::TranslationWords,
        BookId::Ruth,
        true,
    )
    .unwrap_err();
    assert_eq!(
        blocked,
        vec![
            CheckBlocker::List {
                state: PreflightState::Unpinned,
                needs: None
            },
            CheckBlocker::OriginalLanguageLacksBook(uhb),
        ]
    );
}
