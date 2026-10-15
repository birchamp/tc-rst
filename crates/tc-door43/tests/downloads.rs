//! Downloads are verified before they are returned.

mod common;

use common::{client, client_with, fixture, repo, sha, tag};
use tc_door43::{Door43Error, FileMode, Limits, VerificationError};
use tc_door43_fake::{CommitSeed, FakeDoor43, Fault, FaultRule, RepoSeed, SeedFile, SeedMode};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_archive_at_a_commit_is_that_commits_content() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let head = fake.resolve("uW", "en_ugl", "master").await.unwrap();
    let archive = client(&fake)
        .download_archive(&repo("uW/en_ugl"), &sha(&head))
        .await
        .unwrap();
    assert_eq!(archive.commit, sha(&head));
    assert_eq!(
        archive.tree.to_string(),
        fake.tree_of("uW", "en_ugl", &head).await.unwrap()
    );
    let paths: Vec<&str> = archive.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        ["README.md", "ingredients/content/1.json", "metadata.json"]
    );
    let lexicon = fixture("tc4/test-fixtures/resources/en_ugl@d9d29e2d5892/1.json");
    assert_eq!(archive.files[1].bytes, lexicon);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_tree_check_agrees_with_git_on_modes_links_and_nesting() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let file = |path: &str, mode: SeedMode, bytes: &[u8]| SeedFile {
        path: path.to_owned(),
        mode,
        bytes: bytes.to_vec(),
    };
    let shas = fake
        .add_repo(RepoSeed {
            owner: "uW".into(),
            name: "odd-shapes".into(),
            subject: String::new(),
            language: "en".into(),
            topics: Vec::new(),
            private: false,
            commits: vec![CommitSeed {
                message: "shapes".into(),
                files: vec![
                    file("a.b", SeedMode::Regular, b"sorts before the folder a"),
                    file("a/x", SeedMode::Regular, b"x"),
                    file("a/deep/er/file.txt", SeedMode::Regular, b""),
                    file("run.sh", SeedMode::Executable, b"#!/bin/sh\necho hi\n"),
                    file("link", SeedMode::Symlink, b"a/x"),
                    file("ñandú/tëxt.md", SeedMode::Regular, "ünïcode\n".as_bytes()),
                ],
                tag: None,
                release: None,
            }],
        })
        .await
        .unwrap();
    let archive = client(&fake)
        .download_archive(&repo("uW/odd-shapes"), &sha(&shas[0]))
        .await
        .unwrap();
    assert_eq!(
        archive.tree.to_string(),
        fake.tree_of("uW", "odd-shapes", &shas[0]).await.unwrap()
    );
    let mode_of = |path: &str| archive.files.iter().find(|f| f.path == path).unwrap().mode;
    assert_eq!(mode_of("run.sh"), FileMode::Executable);
    assert_eq!(mode_of("link"), FileMode::Symlink);
    assert_eq!(mode_of("a/x"), FileMode::Regular);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_archive_naming_another_commit_is_refused() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let head = fake.resolve("uW", "en_ugl", "master").await.unwrap();
    let other = fake.resolve("unfoldingWord", "en_tn", "v86").await.unwrap();
    fake.inject(FaultRule::once(
        "GET",
        "/api/v1/repos/uW/en_ugl/archive/",
        Fault::ArchiveComment(other.clone()),
    ));
    let result = client(&fake)
        .download_archive(&repo("uW/en_ugl"), &sha(&head))
        .await;
    assert!(
        matches!(
            &result,
            Err(Door43Error::Verification {
                source: VerificationError::ArchiveCommentMismatch { found, .. },
                ..
            }) if *found == other
        ),
        "{result:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_archive_with_content_the_commit_lacks_is_refused() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let head = fake.resolve("uW", "en_ugl", "master").await.unwrap();
    fake.inject(FaultRule::once(
        "GET",
        "/api/v1/repos/uW/en_ugl/archive/",
        Fault::ArchiveExtraFile {
            path: "ingredients/content/2.json".into(),
            bytes: b"{}".to_vec(),
        },
    ));
    let result = client(&fake)
        .download_archive(&repo("uW/en_ugl"), &sha(&head))
        .await;
    assert!(
        matches!(
            &result,
            Err(Door43Error::Verification {
                source: VerificationError::TreeMismatch { .. },
                ..
            })
        ),
        "{result:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_archive_of_an_unknown_commit_is_not_found() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let result = client(&fake)
        .download_archive(
            &repo("uW/en_ugl"),
            &sha("0123456789012345678901234567890123456789"),
        )
        .await;
    assert!(
        matches!(result, Err(Door43Error::NotFound { .. })),
        "{result:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_burrito_export_checked_against_the_tags_api_is_returned_with_fixture_bytes() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    let tn = repo("unfoldingWord/en_tn");
    // A first install: the expected commit comes from the tags API, not from the zip.
    let expected = client.tag_commit(&tn, &tag("v86")).await.unwrap();
    let export = client
        .download_sb_export(&tn, &tag("v86"), &expected)
        .await
        .unwrap();
    assert_eq!(export.revision, expected);
    let bytes_of = |path: &str| {
        export
            .files
            .iter()
            .find(|f| f.path == path)
            .map(|f| f.bytes.clone())
    };
    assert_eq!(
        bytes_of("ingredients/TIT.tsv").unwrap(),
        fixture("tc4/test-fixtures/resources/en_tn@v86/TIT.tsv")
    );
    assert_eq!(
        bytes_of("ingredients/JON.tsv").unwrap(),
        fixture("tc4/test-fixtures/resources/en_tn@v86/JON.tsv")
    );
    assert!(bytes_of(".DS_Store").is_none(), "export debris is dropped");
    assert!(export.files.iter().all(|f| !f.path.starts_with(".git/")));
    let metadata: serde_json::Value = serde_json::from_slice(&fixture(
        "tc4/test-fixtures/resources/en_tn@v86/metadata.json",
    ))
    .unwrap();
    let listed = metadata["ingredients"].as_object().unwrap().len();
    assert_eq!(export.missing_ingredients.len(), listed - 2);
    assert!(
        export
            .missing_ingredients
            .contains(&"ingredients/GEN.tsv".to_owned())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_burrito_export_of_another_commit_is_refused() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    let tn = repo("unfoldingWord/en_tn");
    let v91 = client.tag_commit(&tn, &tag("v91")).await.unwrap();
    let result = client.download_sb_export(&tn, &tag("v86"), &v91).await;
    assert!(
        matches!(
            &result,
            Err(Door43Error::Verification {
                source: VerificationError::RevisionMismatch { expected, .. },
                ..
            }) if *expected == v91
        ),
        "{result:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_export_that_declares_a_moved_tag_is_refused() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    let tn = repo("unfoldingWord/en_tn");
    let v86 = client.tag_commit(&tn, &tag("v86")).await.unwrap();
    fake.inject(FaultRule::once(
        "GET",
        "/unfoldingWord/en_tn/sb/",
        Fault::SbRevision("ffffffffffffffffffffffffffffffffffffffff".into()),
    ));
    let result = client.download_sb_export(&tn, &tag("v86"), &v86).await;
    assert!(
        matches!(
            &result,
            Err(Door43Error::Verification {
                source: VerificationError::RevisionMismatch { .. },
                ..
            })
        ),
        "{result:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_export_whose_bytes_differ_from_its_metadata_is_refused() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    let tn = repo("unfoldingWord/en_tn");
    let v86 = client.tag_commit(&tn, &tag("v86")).await.unwrap();
    fake.inject(FaultRule::once(
        "GET",
        "/unfoldingWord/en_tn/sb/",
        Fault::SbCorruptFile {
            path: "ingredients/TIT.tsv".into(),
        },
    ));
    let result = client.download_sb_export(&tn, &tag("v86"), &v86).await;
    assert!(
        matches!(
            &result,
            Err(Door43Error::Verification {
                source: VerificationError::IngredientMismatch { path, .. },
                ..
            }) if path == "ingredients/TIT.tsv"
        ),
        "{result:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_download_past_the_size_limit_is_refused() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let small = client_with(
        &fake,
        Limits {
            max_download_bytes: 4096,
            ..Limits::default()
        },
    );
    let tn = repo("unfoldingWord/en_tn");
    let v86 = small.tag_commit(&tn, &tag("v86")).await.unwrap();
    let result = small.download_sb_export(&tn, &tag("v86"), &v86).await;
    assert!(
        matches!(result, Err(Door43Error::TooLarge { limit: 4096, .. })),
        "{result:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_commit_without_a_tag_has_no_export() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let head = fake.resolve("uW", "en_ugl", "master").await.unwrap();
    let result = client(&fake)
        .download_sb_export(&repo("uW/en_ugl"), &tag("v1"), &sha(&head))
        .await;
    assert!(
        matches!(result, Err(Door43Error::NotFound { .. })),
        "{result:?}"
    );
}
