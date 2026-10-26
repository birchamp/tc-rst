//! Installing exact Door43 commits from the fake Door43 server.

// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::{commit_pin, nothing_installed, release, repo_path, rig, zip};
use tc_core::BookId;
use tc_door43::{Door43Error, VerificationError};
use tc_door43_fake::{CommitSeed, Fault, FaultRule, RepoSeed, SeedFile, SeedMode};
use tc_resources::{
    BookCoverage, InstallError, LEXICON_FILE, LexiconError, Origin, Packaging, Refusal,
    ResourceStore, Sha256, install_all, install_dcs, install_dcs_as, open_lexicon,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tagged_pin_installs_from_the_burrito_export_and_is_never_fetched_again() {
    let rig = rig().await;
    let pin = release(&rig, "unfoldingWord/en_tn", "v86").await;
    let installed = install_dcs(&rig.store, &rig.client, &pin).await.unwrap();
    installed.verify().unwrap();

    let Origin::Dcs {
        pin: held,
        packaging,
        missing_ingredients,
        left_out,
    } = installed.origin()
    else {
        panic!("a Door43 install");
    };
    assert_eq!(*packaging, Packaging::BurritoExport);
    assert_eq!(held.sha, pin.sha);
    assert_eq!(held.flavor.as_deref(), Some("parascriptural/x-bcvnotes"));
    // The fixture export lists 48 books and a license; the fake's commit holds two books.
    assert_eq!(missing_ingredients.len(), 47);
    assert!(left_out.is_empty());
    assert_eq!(
        installed.books(),
        BookCoverage::books([BookId::Jonah, BookId::Titus]).as_ref()
    );
    let tsv = installed.read_text("ingredients/TIT.tsv").unwrap();
    assert_eq!(
        tsv.as_bytes(),
        common::fixture("tc4/test-fixtures/resources/en_tn@v86/TIT.tsv")
    );
    let metadata = installed.read_text("metadata.json").unwrap();
    assert!(
        metadata.contains(&pin.sha.to_hex()),
        "the export declares the pinned revision"
    );

    rig.fake.clear_requests();
    let again = install_dcs(&rig.store, &rig.client, &pin).await.unwrap();
    assert_eq!(again.dir(), installed.dir());
    assert!(
        rig.fake.requests().is_empty(),
        "an installed commit is not fetched again"
    );
    assert!(rig.store.has_dcs(&pin));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lexicon_pin_by_commit_installs_as_one_file_repackaged_from_the_checked_archive() {
    let rig = rig().await;
    let pin = commit_pin(&rig, "uW", "en_ugl", "HEAD").await;
    let installed = install_dcs(&rig.store, &rig.client, &pin).await.unwrap();
    installed.verify().unwrap();
    let Origin::Dcs {
        pin: held,
        packaging,
        ..
    } = installed.origin()
    else {
        panic!("a Door43 install");
    };
    assert_eq!(*packaging, Packaging::Lexicon);
    assert_eq!(held.flavor.as_deref(), Some("peripheral/x-lexicon"));
    assert_eq!(installed.books(), Some(&BookCoverage::WholeCollection));
    assert_eq!(installed.files().collect::<Vec<_>>(), [LEXICON_FILE]);

    let lexicon = open_lexicon(&rig.store, &pin).unwrap();
    let fixture: serde_json::Value = serde_json::from_slice(&common::fixture(
        "tc4/test-fixtures/resources/en_ugl@d9d29e2d5892/1.json",
    ))
    .unwrap();
    let one = lexicon.entry(1).unwrap();
    assert_eq!(one.brief.as_deref(), fixture["brief"].as_str());
    assert_eq!(one.long.as_deref(), fixture["long"].as_str());
    assert_eq!(lexicon.len(), 1);
    assert_eq!(
        lexicon.files().get("README.md").map(String::as_bytes),
        Some(
            common::fixture("tc4/test-fixtures/resources/en_ugl@d9d29e2d5892/README.md").as_slice()
        ),
        "the archive's other files travel with the entries"
    );
    let folder = tempfile::tempdir().unwrap();
    let (zip, _) = rig
        .client
        .download_archive_zip(
            pin.repo.repo(),
            &pin.sha,
            tc_door43::DownloadTo::folder(folder.path()),
        )
        .await
        .unwrap();
    assert_eq!(
        lexicon.source().archive_sha256,
        Sha256::of(&std::fs::read(zip.path()).unwrap())
    );
    assert_eq!(lexicon.source().commit, pin.sha);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_archive_that_is_not_a_lexicon_is_refused_as_one() {
    let rig = rig().await;
    let ult = release(&rig, "unfoldingWord/en_ult", "v89").await;
    let err = install_dcs_as(&rig.store, &rig.client, &ult, Packaging::Lexicon)
        .await
        .unwrap_err();
    assert!(
        matches!(
            &err,
            InstallError::Lexicon {
                problem: LexiconError::NotALexicon { .. },
                ..
            }
        ),
        "{err}"
    );
    nothing_installed(&rig.store);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_archive_of_a_tagged_pin_gives_the_commits_own_bytes() {
    let rig = rig().await;
    let pin = release(&rig, "unfoldingWord/en_ult", "v89").await;
    let installed = install_dcs_as(&rig.store, &rig.client, &pin, Packaging::Archive)
        .await
        .unwrap();
    assert_eq!(
        installed.read("ingredients/TIT.usfm").unwrap(),
        common::fixture("tc4/test-fixtures/resources/en_ult@v89/TIT.usfm")
    );
    assert_eq!(
        installed.books(),
        BookCoverage::books([BookId::Titus]).as_ref()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wrong_sha_is_refused_and_nothing_is_installed() {
    let rig = rig().await;
    let v86 = release(&rig, "unfoldingWord/en_tn", "v86").await;
    let v91 = release(&rig, "unfoldingWord/en_tn", "v91").await;
    // The v86 label with the v91 commit: the export of v86 declares another revision.
    let mut mislabeled = v91.clone();
    mislabeled.version = v86.version.clone();
    let err = install_dcs(&rig.store, &rig.client, &mislabeled)
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            InstallError::Door43(Door43Error::Verification {
                source: VerificationError::RevisionMismatch { .. },
                ..
            })
        ),
        "{err}"
    );
    nothing_installed(&rig.store);

    // An archive whose comment names another commit.
    let lexicon = commit_pin(&rig, "uW", "en_ugl", "HEAD").await;
    rig.fake.inject(FaultRule::once(
        "GET",
        "/api/v1/repos/uW/en_ugl/archive",
        Fault::ArchiveComment("0".repeat(40)),
    ));
    let err = install_dcs(&rig.store, &rig.client, &lexicon)
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            InstallError::Door43(Door43Error::Verification {
                source: VerificationError::ArchiveCommentMismatch { .. },
                ..
            })
        ),
        "{err}"
    );
    nothing_installed(&rig.store);

    // An export whose file does not match its checksum.
    rig.fake.inject(FaultRule::once(
        "GET",
        "/unfoldingWord/en_tn/sb/",
        Fault::SbCorruptFile {
            path: "ingredients/TIT.tsv".to_owned(),
        },
    ));
    let err = install_dcs(&rig.store, &rig.client, &v86)
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            InstallError::Door43(Door43Error::Verification {
                source: VerificationError::IngredientMismatch { .. },
                ..
            })
        ),
        "{err}"
    );
    nothing_installed(&rig.store);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_export_of_another_repository_is_refused() {
    let rig = rig().await;
    let pin = release(&rig, "unfoldingWord/en_tn", "v86").await;
    let metadata = serde_json::json!({
        "format": "scripture burrito",
        "identification": {"primary": {"dcs": {"unfoldingWord/en_ult": {"revision": pin.sha.to_hex()}}}},
        "languages": [{"tag": "en"}],
        "type": {"flavorType": {"name": "scripture", "flavor": {"name": "textTranslation"}}},
        "ingredients": {}
    });
    let bytes = zip(&[
        ("en_tn/metadata.json", metadata.to_string().as_bytes()),
        ("en_tn/ingredients/TIT.tsv", b"Reference\tID\n"),
    ]);
    rig.fake.inject(FaultRule::once(
        "GET",
        "/unfoldingWord/en_tn/sb/",
        Fault::Raw {
            status: 200,
            headers: vec![("content-type".to_owned(), "application/zip".to_owned())],
            body: bytes,
        },
    ));
    let err = install_dcs(&rig.store, &rig.client, &pin)
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            InstallError::Refused {
                refusal: Refusal::WrongRepository { .. },
                ..
            }
        ),
        "{err}"
    );
    nothing_installed(&rig.store);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn offline_is_a_typed_error_and_nothing_is_sent() {
    let rig = rig().await;
    let pin = release(&rig, "unfoldingWord/en_tn", "v86").await;
    rig.internet.turn_off();
    rig.fake.clear_requests();
    let err = install_dcs(&rig.store, &rig.client, &pin)
        .await
        .unwrap_err();
    assert!(
        matches!(err, InstallError::Door43(Door43Error::Offline { .. })),
        "{err}"
    );
    assert!(
        rig.fake.requests().is_empty(),
        "nothing may be sent while offline"
    );
    nothing_installed(&rig.store);

    // Once installed, a pin opens offline.
    rig.internet.turn_on();
    install_dcs(&rig.store, &rig.client, &pin).await.unwrap();
    rig.internet.turn_off();
    rig.fake.clear_requests();
    install_dcs(&rig.store, &rig.client, &pin).await.unwrap();
    assert!(rig.fake.requests().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_interrupted_download_installs_nothing() {
    let rig = rig().await;
    let pin = release(&rig, "unfoldingWord/en_tn", "v86").await;
    let url = format!("{}/unfoldingWord/en_tn/sb/v86.zip", rig.fake.url());
    let whole = reqwest::get(&url)
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap()
        .to_vec();

    // The connection closes part way: the server promised more bytes than it sent.
    rig.fake.inject(FaultRule::once(
        "GET",
        "/unfoldingWord/en_tn/sb/",
        Fault::Raw {
            status: 200,
            headers: vec![("content-length".to_owned(), whole.len().to_string())],
            body: whole[..whole.len() / 2].to_vec(),
        },
    ));
    let err = install_dcs(&rig.store, &rig.client, &pin)
        .await
        .unwrap_err();
    assert!(matches!(err, InstallError::Door43(_)), "{err}");
    nothing_installed(&rig.store);

    // A server that ends the body early without saying how long it was.
    rig.fake.inject(FaultRule::once(
        "GET",
        "/unfoldingWord/en_tn/sb/",
        Fault::Raw {
            status: 200,
            headers: Vec::new(),
            body: whole[..whole.len() - 100].to_vec(),
        },
    ));
    let err = install_dcs(&rig.store, &rig.client, &pin)
        .await
        .unwrap_err();
    assert!(
        matches!(err, InstallError::Door43(Door43Error::Verification { .. })),
        "{err}"
    );
    nothing_installed(&rig.store);

    // The internet is turned off while the answer is on its way.
    rig.fake.inject(FaultRule::once(
        "GET",
        "/unfoldingWord/en_tn/sb/",
        Fault::Delay(std::time::Duration::from_millis(400)),
    ));
    let internet = rig.internet.clone();
    let switch_off = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        internet.turn_off();
    });
    let err = install_dcs(&rig.store, &rig.client, &pin)
        .await
        .unwrap_err();
    switch_off.await.unwrap();
    assert!(
        matches!(err, InstallError::Door43(Door43Error::Offline { .. })),
        "{err}"
    );
    nothing_installed(&rig.store);

    rig.internet.turn_on();
    install_dcs(&rig.store, &rig.client, &pin)
        .await
        .unwrap()
        .verify()
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn installing_a_release_stops_at_the_first_failure_and_names_it() {
    let rig = rig().await;
    let tn = release(&rig, "unfoldingWord/en_tn", "v91").await;
    let mut missing = release(&rig, "unfoldingWord/en_twl", "v86").await;
    missing.sha = "1".repeat(40).parse().unwrap();
    let ult = release(&rig, "unfoldingWord/en_ult", "v89").await;
    let (failed, err) = *install_all(
        &rig.store,
        &rig.client,
        &[tn.clone(), missing.clone(), ult.clone()],
    )
    .await
    .unwrap_err();
    assert_eq!(failed, missing);
    assert!(matches!(err, InstallError::Door43(_)), "{err}");
    assert!(rig.store.has_dcs(&tn));
    assert!(
        !rig.store.has_dcs(&ult),
        "the resource after the failure is not attempted"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bundled_store_seeds_a_first_run_that_is_offline() {
    let rig = rig().await;
    let pin = release(&rig, "es-419_gl/es-419_tn", "v66").await;
    let lexicon = commit_pin(&rig, "uW", "en_ugl", "HEAD").await;
    let bundled_dir = rig.dir.path().join("bundled");
    let bundled = ResourceStore::open(&bundled_dir).unwrap();
    install_dcs(&bundled, &rig.client, &pin).await.unwrap();
    install_dcs(&bundled, &rig.client, &lexicon).await.unwrap();

    rig.internet.turn_off();
    rig.fake.clear_requests();
    let machine = ResourceStore::open(rig.dir.path().join("machine")).unwrap();
    let report = machine
        .seed_from(&ResourceStore::open_read_only(&bundled_dir).unwrap())
        .unwrap();
    assert_eq!(report.added.len(), 2);
    let installed = install_dcs(&machine, &rig.client, &pin).await.unwrap();
    installed.verify().unwrap();
    assert!(rig.fake.requests().is_empty());
    // The registry owner differs in case from the pin's; the identity still matches.
    assert!(machine.has_dcs(&tc_resources::ResourcePin::new(
        repo_path("Es-419_gl/es-419_tn"),
        pin.sha
    )));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_link_in_a_commit_is_left_out_and_never_written() {
    let rig = rig().await;
    rig.fake
        .add_repo(RepoSeed {
            owner: "uW".to_owned(),
            name: "en_links".to_owned(),
            subject: "Bible".to_owned(),
            language: "en".to_owned(),
            topics: Vec::new(),
            private: false,
            commits: vec![CommitSeed {
                message: "A repository holding a link".to_owned(),
                files: vec![
                    SeedFile::regular("README.md", b"# Links\n".to_vec()),
                    SeedFile {
                        path: "LICENSE.md".to_owned(),
                        mode: SeedMode::Symlink,
                        bytes: b"../../../../etc/passwd".to_vec(),
                    },
                ],
                tag: None,
                release: None,
            }],
        })
        .await
        .unwrap();
    let pin = commit_pin(&rig, "uW", "en_links", "HEAD").await;
    let installed = install_dcs(&rig.store, &rig.client, &pin).await.unwrap();
    let Origin::Dcs { left_out, .. } = installed.origin() else {
        panic!("a Door43 install");
    };
    assert_eq!(left_out, &["LICENSE.md".to_owned()]);
    assert!(!installed.has_file("LICENSE.md"));
    assert!(installed.has_file("README.md"));
    installed.verify().unwrap();
    assert!(
        !installed.dir().join("content").join("LICENSE.md").exists(),
        "nothing is written for a link"
    );
}
