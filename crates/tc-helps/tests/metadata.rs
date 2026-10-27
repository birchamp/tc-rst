//! Identifying downloaded resources and checking them against their pins.

// Helpers outside #[test] functions unwrap too; clippy's test exemption covers only
// #[test] bodies.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::{fixture, resource};
use tc_core::BookId;
use tc_helps::metadata::{
    DcsPin, MetadataFormat, ResourceKind, Revision, ScriptDirection, parse_burrito_metadata,
    parse_rc_manifest,
};
use tc_helps::{Error, GitOid, JsonFile, PinMismatch, YamlFile};

fn sha(s: &str) -> GitOid {
    GitOid::parse(s).unwrap()
}

/// The pins tC4's fixture README records for each export.
const PINS: [(&str, &str, &str, &str, ResourceKind); 9] = [
    (
        "en_tn@v86",
        "unfoldingWord",
        "en_tn",
        "c354b8ae66a23c485bf6f38fd35bd8f7ef81e4e5",
        ResourceKind::TranslationNotes,
    ),
    (
        "en_twl@v86",
        "unfoldingWord",
        "en_twl",
        "570e76d0024c847689e48a20e2ac1a1d2c6eb6e3",
        ResourceKind::WordsLinks,
    ),
    (
        "en_twl@v91",
        "unfoldingWord",
        "en_twl",
        "7cbb3419f79780ce84b92ad7d42bd476cc5d8a64",
        ResourceKind::WordsLinks,
    ),
    (
        "es-419_tn@v66",
        "Idiomas-Puentes",
        "es-419_tn",
        "22f3d0c61e2ab4701cb869547de9c3c43da07208",
        ResourceKind::TranslationNotes,
    ),
    (
        "en_ta@v86",
        "unfoldingWord",
        "en_ta",
        "c7caddfb474efd713f36b35a3ffc927866c7b180",
        ResourceKind::TranslationAcademy,
    ),
    (
        "en_obs-tn@v13",
        "unfoldingWord",
        "en_obs-tn",
        "e86138ea13f619f09f7a6dcaa60592716d407fe4",
        ResourceKind::ObsNotes,
    ),
    (
        "en_obs-twl@v3",
        "unfoldingWord",
        "en_obs-twl",
        "44ebc9fafe8101665f985007d566f5036a2be85b",
        ResourceKind::ObsWordsLinks,
    ),
    (
        "en_obs-tq@v10",
        "unfoldingWord",
        "en_obs-tq",
        "01b92fe8793d62cff3a2221f5174c768cbad3dc1",
        ResourceKind::ObsQuestions,
    ),
    (
        "en_tw@v91",
        "unfoldingWord",
        "en_tw",
        "ff5b3852c27c3a0d01b109e482eb26047dcd20e2",
        ResourceKind::TranslationWords,
    ),
];

#[test]
fn every_export_names_its_owner_repository_kind_and_revision() {
    for (dir, owner, repo, commit, kind) in PINS {
        let meta = parse_burrito_metadata(&resource(&format!("{dir}/metadata.json")))
            .unwrap_or_else(|e| panic!("{dir}: {e}"));
        let id = &meta.identity;
        assert_eq!(id.format, MetadataFormat::ScriptureBurrito);
        assert_eq!(id.owner.as_deref(), Some(owner), "{dir}");
        assert_eq!(id.repo.as_deref(), Some(repo), "{dir}");
        assert_eq!(id.revision, Some(Revision::Commit(sha(commit))), "{dir}");
        assert_eq!(id.kind, kind, "{dir}");
        assert_eq!(id.direction, Some(ScriptDirection::Ltr));
        let pin = DcsPin {
            owner: owner.to_owned(),
            repo: repo.to_owned(),
            tag: Some(dir.split('@').nth(1).unwrap().to_owned()),
            commit: sha(commit),
        };
        pin.verify(id).unwrap();
    }
}

#[test]
fn coverage_language_and_copyright_come_from_the_export() {
    let es = parse_burrito_metadata(&resource("es-419_tn@v66/metadata.json")).unwrap();
    assert_eq!(es.identity.language, "es-419");
    assert_eq!(
        es.identity.books,
        vec![
            BookId::ThirdJohn,
            BookId::Jonah,
            BookId::Ruth,
            BookId::Titus
        ]
    );
    assert_eq!(
        es.identity.flavor.as_deref(),
        Some("parascriptural/x-bcvnotes")
    );
    assert_eq!(
        es.copyright,
        vec!["© Idiomas Puentes 2024, CC BY-SA 4.0".to_owned()]
    );
    let ult = parse_burrito_metadata(&resource("en_ult@v89/metadata.json")).unwrap();
    assert_eq!(ult.identity.kind, ResourceKind::Bible);
    assert!(ult.identity.books.contains(&BookId::Titus));
    let lexicon = parse_burrito_metadata(&resource("en_ugl@d9d29e2d5892/metadata.json")).unwrap();
    assert_eq!(lexicon.identity.kind, ResourceKind::Lexicon);
    assert_eq!(lexicon.identity.revision, Some(Revision::Label("1".into())));
    assert_eq!(lexicon.identity.owner, None);
}

#[test]
fn a_pin_refuses_another_commit_or_repository() {
    let meta = parse_burrito_metadata(&resource("es-419_tn@v66/metadata.json")).unwrap();
    // The export names the old organization; the pin names today's. The repository and
    // commit still match, so the pin holds.
    let pin = DcsPin {
        owner: "es-419_gl".into(),
        repo: "ES-419_TN".into(),
        tag: Some("v66".into()),
        commit: sha("22f3d0c61e2ab4701cb869547de9c3c43da07208"),
    };
    pin.verify(&meta.identity).unwrap();
    let wrong_commit = DcsPin {
        commit: sha("c354b8ae66a23c485bf6f38fd35bd8f7ef81e4e5"),
        ..pin.clone()
    };
    assert!(matches!(
        wrong_commit.verify(&meta.identity),
        Err(Error::PinMismatch(PinMismatch::Revision { .. }))
    ));
    let wrong_repo = DcsPin {
        repo: "es-419_tq".into(),
        ..pin.clone()
    };
    assert!(matches!(
        wrong_repo.verify(&meta.identity),
        Err(Error::PinMismatch(PinMismatch::Repository { .. }))
    ));
    let lexicon = parse_burrito_metadata(&resource("en_ugl@d9d29e2d5892/metadata.json")).unwrap();
    assert!(pin.verify(&lexicon.identity).is_err());
}

#[test]
fn an_rc_manifest_names_language_subject_version_and_books() {
    let manifest = parse_rc_manifest(&fixture("helps/rc/en_tn/manifest.yaml")).unwrap();
    let id = &manifest.identity;
    assert_eq!(id.format, MetadataFormat::ResourceContainer);
    assert_eq!(id.kind, ResourceKind::TranslationNotes);
    assert_eq!(id.language, "en");
    assert_eq!(id.subject.as_deref(), Some("TSV Translation Notes"));
    assert_eq!(id.version.as_deref(), Some("86"));
    assert_eq!(id.revision, None);
    assert_eq!(id.books, vec![BookId::Jonah, BookId::Titus]);
    assert_eq!(
        manifest.project_paths,
        vec![
            ("jon".to_owned(), "./tn_JON.tsv".to_owned()),
            ("tit".to_owned(), "./tn_TIT.tsv".to_owned())
        ]
    );
    assert!(manifest.relations.contains(&"hbo/uhb?v=2.1.30".to_owned()));
    assert_eq!(manifest.rights.as_deref(), Some("CC BY-SA 4.0"));
}

#[test]
fn a_byte_order_mark_is_not_part_of_the_metadata() {
    let json = resource("en_tn@v86/metadata.json");
    assert_eq!(
        parse_burrito_metadata(&format!("\u{feff}{json}")).unwrap(),
        parse_burrito_metadata(&json).unwrap()
    );
    let yaml = fixture("helps/rc/en_tn/manifest.yaml");
    assert_eq!(
        parse_rc_manifest(&format!("\u{feff}{yaml}")).unwrap(),
        parse_rc_manifest(&yaml).unwrap()
    );
}

#[test]
fn metadata_that_is_not_a_resource_is_refused() {
    for bad in ["{", "{}", r#"{"format":"scripture burrito"}"#, r#"[1,2]"#] {
        assert!(
            matches!(
                parse_burrito_metadata(bad),
                Err(Error::Json {
                    file: JsonFile::BurritoMetadata,
                    ..
                })
            ),
            "{bad}"
        );
    }
    for bad in [
        "projects: []\n",
        "dublin_core:\n  identifier: tn\n",
        ": :\n  - [",
    ] {
        assert!(
            matches!(
                parse_rc_manifest(bad),
                Err(Error::Yaml {
                    file: YamlFile::Manifest,
                    ..
                })
            ),
            "{bad}"
        );
    }
}
