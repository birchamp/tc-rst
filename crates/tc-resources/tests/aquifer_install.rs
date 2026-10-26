//! Installing Aquifer release assets from a fake GitHub.

// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::aquifer_fake::{ApiFault, AssetFault, FakeAsset, FakeRelease};
use common::{aquifer_pin, aquifer_rig, aquifer_zip, nothing_installed, zip};
use tc_core::BookId;
use tc_resources::{
    AquiferError, AquiferRepo, BookCoverage, InstallError, Origin, Refusal, install_aquifer,
    install_aquifer_zip, pin_aquifer_release,
};

const MAX: u64 = 512 << 20;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_release_asset_is_pinned_by_tag_commit_and_digest_then_installed() {
    let rig = aquifer_rig().await;
    let repo = AquiferRepo::bible_aquifer("UWTranslationNotes").unwrap();
    let release = rig.client.latest_release(&repo).await.unwrap();
    assert_eq!(release.tag.as_str(), "v2026-10-12");
    assert_eq!(release.json_assets().count(), 2);
    let asset = "Portuguese-json.zip".parse().unwrap();
    let pin = pin_aquifer_release(&rig.client, &repo, &release, &asset, "por".parse().unwrap())
        .await
        .unwrap();
    assert_eq!(
        pin,
        aquifer_pin(
            "UWTranslationNotes",
            "por",
            &aquifer_zip("UWTranslationNotes", "por")
        )
    );

    let installed = install_aquifer(&rig.store, &rig.client, &pin)
        .await
        .unwrap();
    installed.verify().unwrap();
    assert_eq!(
        installed.books(),
        BookCoverage::books([BookId::Titus]).as_ref()
    );
    let Origin::Aquifer {
        pin: held,
        refused_articles,
        book_files,
    } = installed.origin()
    else {
        panic!("an Aquifer install");
    };
    assert_eq!(held, &pin);
    assert_eq!(*refused_articles, 0);
    assert_eq!(
        book_files.get(&BookId::Titus).map(Vec::as_slice),
        Some(&["json/56.content.json".to_owned()][..])
    );
    assert_eq!(
        installed.read("json/56.content.json").unwrap(),
        common::fixture("aquifer/UWTranslationNotes/por/json/56.content.json")
    );
    // The download went through GitHub's redirect to the asset host.
    assert!(
        rig.github
            .requests()
            .iter()
            .any(|r| r.starts_with("/_assets/"))
    );

    rig.github.clear_requests();
    install_aquifer(&rig.store, &rig.client, &pin)
        .await
        .unwrap();
    assert!(
        rig.github.requests().is_empty(),
        "an installed asset is not fetched again"
    );
    assert_eq!(
        rig.store.aquifer(&pin).unwrap().unwrap().dir(),
        installed.dir()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn alphabetical_and_monograph_resources_cover_every_book() {
    let rig = aquifer_rig().await;
    for repo in ["UWTranslationWords", "UWTranslationManual"] {
        let pin = aquifer_pin(repo, "eng", &aquifer_zip(repo, "eng"));
        let installed = install_aquifer(&rig.store, &rig.client, &pin)
            .await
            .unwrap();
        assert_eq!(
            installed.books(),
            Some(&BookCoverage::WholeCollection),
            "{repo}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wrong_digest_is_refused_and_nothing_is_installed() {
    let rig = aquifer_rig().await;
    let mut pin = aquifer_pin(
        "UWTranslationNotes",
        "por",
        &aquifer_zip("UWTranslationNotes", "por"),
    );
    pin.sha256 = tc_resources::Sha256::of(b"another asset");
    let err = install_aquifer(&rig.store, &rig.client, &pin)
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            InstallError::Refused {
                refusal: Refusal::Sha256Mismatch { .. },
                ..
            }
        ),
        "{err}"
    );
    nothing_installed(&rig.store);

    // The maintainer re-uploaded the asset under the same tag: the pin's digest refuses it.
    let pin = aquifer_pin(
        "UWTranslationNotes",
        "por",
        &aquifer_zip("UWTranslationNotes", "por"),
    );
    rig.github
        .fault_next_download(AssetFault::Replace(aquifer_zip(
            "UWTranslationNotes",
            "eng",
        )));
    let err = install_aquifer(&rig.store, &rig.client, &pin)
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            InstallError::Refused {
                refusal: Refusal::Sha256Mismatch { .. },
                ..
            }
        ),
        "{err}"
    );
    nothing_installed(&rig.store);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn offline_is_a_typed_error_and_nothing_is_sent() {
    let rig = aquifer_rig().await;
    let pin = aquifer_pin(
        "UWTranslationNotes",
        "por",
        &aquifer_zip("UWTranslationNotes", "por"),
    );
    rig.internet.turn_off();
    let err = install_aquifer(&rig.store, &rig.client, &pin)
        .await
        .unwrap_err();
    assert!(
        matches!(err, InstallError::Aquifer(AquiferError::Offline { .. })),
        "{err}"
    );
    let repo = AquiferRepo::bible_aquifer("UWTranslationNotes").unwrap();
    assert!(matches!(
        rig.client.latest_release(&repo).await,
        Err(AquiferError::Offline { .. })
    ));
    assert!(
        rig.github.requests().is_empty(),
        "nothing may be sent while offline"
    );
    nothing_installed(&rig.store);

    // A zip at hand installs with the internet off.
    let bytes = aquifer_zip("UWTranslationNotes", "por");
    install_aquifer_zip(&rig.store, &pin, &bytes, MAX).unwrap();
    install_aquifer(&rig.store, &rig.client, &pin)
        .await
        .unwrap();
    assert!(rig.github.requests().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_interrupted_download_installs_nothing() {
    let rig = aquifer_rig().await;
    let pin = aquifer_pin(
        "UWTranslationNotes",
        "por",
        &aquifer_zip("UWTranslationNotes", "por"),
    );
    rig.github.fault_next_download(AssetFault::Truncate);
    let err = install_aquifer(&rig.store, &rig.client, &pin)
        .await
        .unwrap_err();
    assert!(
        matches!(err, InstallError::Aquifer(AquiferError::Unreachable { .. })),
        "{err}"
    );
    nothing_installed(&rig.store);
    install_aquifer(&rig.store, &rig.client, &pin)
        .await
        .unwrap()
        .verify()
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_redirect_off_the_server_is_not_followed() {
    let rig = aquifer_rig().await;
    let pin = aquifer_pin(
        "UWTranslationNotes",
        "por",
        &aquifer_zip("UWTranslationNotes", "por"),
    );
    let other = aquifer_rig().await;
    let elsewhere = format!(
        "{}_assets/BibleAquifer/UWTranslationNotes/v2026-10-12/Portuguese-json.zip",
        other.github.url()
    );
    rig.github
        .fault_next_download(AssetFault::RedirectTo(elsewhere));
    let err = install_aquifer(&rig.store, &rig.client, &pin)
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            InstallError::Aquifer(AquiferError::UnexpectedResponse { .. })
        ),
        "{err}"
    );
    assert!(
        other.github.requests().is_empty(),
        "the other server is never asked"
    );
    nothing_installed(&rig.store);
}

#[test]
fn an_asset_of_another_resource_or_language_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = tc_resources::ResourceStore::open(dir.path()).unwrap();
    let por = aquifer_zip("UWTranslationNotes", "por");

    let mut wrong_language = aquifer_pin("UWTranslationNotes", "por", &por);
    wrong_language.language = "eng".parse().unwrap();
    let err = install_aquifer_zip(&store, &wrong_language, &por, MAX).unwrap_err();
    assert!(
        matches!(
            err,
            InstallError::Refused {
                refusal: Refusal::WrongLanguage { .. },
                ..
            }
        ),
        "{err}"
    );

    let mut wrong_repo = aquifer_pin("UWTranslationNotes", "por", &por);
    wrong_repo.repo = AquiferRepo::bible_aquifer("UWTranslationWords").unwrap();
    let err = install_aquifer_zip(&store, &wrong_repo, &por, MAX).unwrap_err();
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

    let no_metadata = zip(&[("json/56.content.json", b"[]")]);
    let pin = aquifer_pin("UWTranslationNotes", "por", &no_metadata);
    let err = install_aquifer_zip(&store, &pin, &no_metadata, MAX).unwrap_err();
    assert!(
        matches!(
            err,
            InstallError::Refused {
                refusal: Refusal::NotAquifer(_),
                ..
            }
        ),
        "{err}"
    );

    let metadata = common::fixture("aquifer/UWTranslationNotes/por/metadata.json");
    let broken = zip(&[
        ("metadata.json", &metadata),
        ("json/56.content.json", b"{not json"),
    ]);
    let pin = aquifer_pin("UWTranslationNotes", "por", &broken);
    let err = install_aquifer_zip(&store, &pin, &broken, MAX).unwrap_err();
    assert!(
        matches!(
            err,
            InstallError::Refused {
                refusal: Refusal::NotAquifer(_),
                ..
            }
        ),
        "{err}"
    );

    let slip = zip(&[("metadata.json", &metadata), ("../evil.json", b"[]")]);
    let pin = aquifer_pin("UWTranslationNotes", "por", &slip);
    let err = install_aquifer_zip(&store, &pin, &slip, MAX).unwrap_err();
    assert!(
        matches!(
            err,
            InstallError::Refused {
                refusal: Refusal::EscapesRoot(_),
                ..
            }
        ),
        "{err}"
    );
    nothing_installed(&store);
}

#[test]
fn an_asset_with_names_a_file_system_would_rewrite_or_merge_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = tc_resources::ResourceStore::open(dir.path()).unwrap();
    let install = |extra: &[(&str, &[u8])]| {
        let bytes = common::aquifer_zip_with("UWTranslationNotes", "por", extra);
        let pin = aquifer_pin("UWTranslationNotes", "por", &bytes);
        install_aquifer_zip(&store, &pin, &bytes, MAX)
    };
    // Windows would write these under another name, or open another file through them.
    for name in [
        "CON.json",
        "nul.json",
        "notes/a.",
        "PROGRA~1",
        "ABCDEF~1.JSN",
    ] {
        let err = install(&[(name, b"x")]).unwrap_err();
        assert!(
            matches!(
                err,
                InstallError::Refused {
                    refusal: Refusal::NotPortable { .. },
                    ..
                }
            ),
            "{name}: {err}"
        );
    }
    // Windows and macOS would store each pair as one file or folder.
    for pair in [
        ["Notes.json", "notes.json"],
        ["café.md", "cafe\u{301}.md"],
        ["Notes/a.json", "notes/b.json"],
        ["A", "a/x"],
    ] {
        let err = install(&[(pair[0], b"x"), (pair[1], b"y")]).unwrap_err();
        assert!(
            matches!(
                err,
                InstallError::Refused {
                    refusal: Refusal::DuplicateEntry(_),
                    ..
                }
            ),
            "{pair:?}: {err}"
        );
    }
    nothing_installed(&store);
    // `ı` is not `i` on any of them, and `notes~1.json` is no 8.3 short name.
    let installed = install(&[
        ("ı", b"x"),
        ("i", b"y"),
        ("notes~1.json", b"z"),
        ("a~1.tar.gz", b"z"),
    ])
    .unwrap();
    installed.verify().unwrap();
    assert_eq!(installed.read("ı").unwrap(), b"x");
    assert_eq!(installed.read("i").unwrap(), b"y");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_release_without_a_digest_cannot_be_pinned() {
    let rig = aquifer_rig().await;
    rig.github.add_release(FakeRelease {
        owner: "BibleAquifer".to_owned(),
        repo: "UWTranslationNotes".to_owned(),
        tag: "v2026-10-19".to_owned(),
        commit: "a".repeat(40),
        published_at: "2026-10-19T00:00:00Z".to_owned(),
        assets: vec![FakeAsset {
            name: "Portuguese-json.zip".to_owned(),
            bytes: aquifer_zip("UWTranslationNotes", "por"),
            listed_digest: false,
        }],
    });
    let repo = AquiferRepo::bible_aquifer("UWTranslationNotes").unwrap();
    let release = rig.client.latest_release(&repo).await.unwrap();
    assert_eq!(release.tag.as_str(), "v2026-10-19");
    let err = pin_aquifer_release(
        &rig.client,
        &repo,
        &release,
        &"Portuguese-json.zip".parse().unwrap(),
        "por".parse().unwrap(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, InstallError::Refused { .. }), "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn turning_the_internet_off_during_a_download_installs_nothing() {
    let rig = aquifer_rig().await;
    let pin = aquifer_pin(
        "UWTranslationNotes",
        "por",
        &aquifer_zip("UWTranslationNotes", "por"),
    );
    // The request leaves while the switch is on; the answer arrives after it is off.
    rig.github
        .fault_next_download(AssetFault::Delay(std::time::Duration::from_millis(400)));
    let internet = rig.internet.clone();
    let switch_off = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        internet.turn_off();
    });
    let err = install_aquifer(&rig.store, &rig.client, &pin)
        .await
        .unwrap_err();
    switch_off.await.unwrap();
    assert!(
        matches!(err, InstallError::Aquifer(AquiferError::Offline { .. })),
        "{err}"
    );
    nothing_installed(&rig.store);
}

#[test]
fn an_asset_holding_a_link_is_refused() {
    use std::io::Write;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    writer.start_file("metadata.json", options).unwrap();
    writer
        .write_all(&common::fixture(
            "aquifer/UWTranslationNotes/por/metadata.json",
        ))
        .unwrap();
    writer
        .add_symlink("json/56.content.json", "../../../../etc/passwd", options)
        .unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    let pin = aquifer_pin("UWTranslationNotes", "por", &bytes);
    let dir = tempfile::tempdir().unwrap();
    let store = tc_resources::ResourceStore::open(dir.path()).unwrap();
    let err = install_aquifer_zip(&store, &pin, &bytes, MAX).unwrap_err();
    assert!(
        matches!(
            err,
            InstallError::Refused {
                refusal: Refusal::Symlink(ref path),
                ..
            } if path == "json/56.content.json"
        ),
        "{err}"
    );
    nothing_installed(&store);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn githubs_rate_limit_is_told_apart_from_a_refusal() {
    let rig = aquifer_rig().await;
    let repo = AquiferRepo::bible_aquifer("UWTranslationNotes").unwrap();
    rig.github.fail_next_api(ApiFault {
        status: 403,
        headers: vec![("x-ratelimit-remaining".to_owned(), "0".to_owned())],
    });
    let err = rig.client.latest_release(&repo).await.unwrap_err();
    assert!(
        matches!(err, AquiferError::RateLimited { status: 403, .. }),
        "{err}"
    );
    rig.github.fail_next_api(ApiFault {
        status: 403,
        headers: vec![("x-ratelimit-remaining".to_owned(), "12".to_owned())],
    });
    let err = rig.client.latest_release(&repo).await.unwrap_err();
    assert!(
        matches!(err, AquiferError::Http { status: 403, .. }),
        "{err}"
    );
    rig.github.fail_next_api(ApiFault {
        status: 429,
        headers: Vec::new(),
    });
    let err = rig.client.latest_release(&repo).await.unwrap_err();
    assert!(
        matches!(err, AquiferError::RateLimited { status: 429, .. }),
        "{err}"
    );
    assert!(rig.client.latest_release(&repo).await.is_ok());
}
