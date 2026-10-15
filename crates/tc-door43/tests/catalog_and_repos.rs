//! Discovery: catalog search, repository info, tags and releases.

mod common;

use common::{client, repo, sha, tag};
use tc_door43::{CatalogQuery, CatalogRef, Door43Error, ReleaseState, Stage, Visibility};
use tc_door43_fake::{FakeDoor43, seeded};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_search_finds_the_newest_production_release_by_language_and_subject() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let entries = client(&fake)
        .catalog_search(&CatalogQuery {
            languages: vec!["en".into()],
            subjects: vec!["TSV Translation Notes".into()],
            ..CatalogQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(entries.len(), 1, "{entries:#?}");
    let entry = &entries[0];
    assert_eq!(entry.repo, repo("unfoldingWord/en_tn"));
    assert_eq!(entry.git_ref, CatalogRef::Tag(tag("v91")));
    assert_eq!(
        entry.commit,
        sha(&fake.resolve("unfoldingWord", "en_tn", "v91").await.unwrap())
    );
    assert_eq!(entry.subject, "TSV Translation Notes");
    assert_eq!(entry.language, "en");
    assert_eq!(entry.stage, "prod");
    assert_eq!(entry.flavor, "x-bcvnotes");
    assert!(entry.books.contains(&"tit".to_owned()));
    assert!(entry.topics.contains(&"tc-ready".to_owned()));
    assert!(
        entry
            .ingredients
            .iter()
            .any(|i| i.path == "./ingredients/TIT.tsv")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_search_applies_stage_history_owner_and_topic() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    let notes = |stage, include_history| CatalogQuery {
        repos: vec!["en_tn".parse().unwrap()],
        stage,
        include_history,
        ..CatalogQuery::default()
    };
    let refs = |entries: Vec<tc_door43::CatalogEntry>| -> Vec<CatalogRef> {
        entries.into_iter().map(|e| e.git_ref).collect()
    };

    let preprod = client
        .catalog_search(&notes(Some(Stage::Preprod), false))
        .await
        .unwrap();
    assert_eq!(refs(preprod), [CatalogRef::Tag(tag("v92"))]);

    let history = client
        .catalog_search(&notes(Some(Stage::Prod), true))
        .await
        .unwrap();
    let mut history = refs(history);
    history.sort_by_key(|r| format!("{r:?}"));
    assert_eq!(
        history,
        [CatalogRef::Tag(tag("v86")), CatalogRef::Tag(tag("v91"))]
    );

    let latest = client
        .catalog_search(&notes(Some(Stage::Latest), true))
        .await
        .unwrap();
    let latest = refs(latest);
    assert_eq!(latest.len(), 4, "{latest:?}");
    assert!(latest.contains(&CatalogRef::Branch("master".into())));
    assert!(latest.contains(&CatalogRef::Tag(tag("v92"))));

    let spanish = client
        .catalog_search(&CatalogQuery {
            owners: vec![seeded::READ_ONLY_ORG.parse().unwrap()],
            topics: vec!["tc-ready".into()],
            ..CatalogQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(spanish.len(), 1);
    assert_eq!(spanish[0].repo, repo("es-419_gl/es-419_tn"));
    assert_eq!(spanish[0].language, "es-419");

    let none = client
        .catalog_search(&CatalogQuery {
            topics: vec!["no-such-topic".into()],
            ..CatalogQuery::default()
        })
        .await
        .unwrap();
    assert!(none.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repo_info_carries_each_stage_and_door43_casing() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let info = client(&fake)
        .repo(&repo("UNFOLDINGWORD/EN_TN"), None)
        .await
        .unwrap();
    assert_eq!(info.repo, repo("unfoldingWord/en_tn"));
    assert_eq!(info.visibility, Visibility::Public);
    assert_eq!(info.default_branch, "master");
    assert!(!info.empty);
    let resolve = |rev: &'static str| fake.resolve("unfoldingWord", "en_tn", rev);
    let prod = info.catalog.prod.unwrap();
    assert_eq!(
        (prod.ref_name.as_str(), prod.commit),
        ("v91", sha(&resolve("v91").await.unwrap()))
    );
    let preprod = info.catalog.preprod.unwrap();
    assert_eq!(
        (preprod.ref_name.as_str(), preprod.commit),
        ("v92", sha(&resolve("v92").await.unwrap()))
    );
    let latest = info.catalog.latest.unwrap();
    assert_eq!(latest.commit, sha(&resolve("master").await.unwrap()));
    assert_eq!(info.subject, "TSV Translation Notes");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repo_exists_is_true_false_or_an_error_never_a_guess() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    assert!(
        client
            .repo_exists(&repo("unfoldingWord/en_tn"), None)
            .await
            .unwrap()
    );
    assert!(
        !client
            .repo_exists(&repo("unfoldingWord/nothing"), None)
            .await
            .unwrap()
    );
    fake.inject(tc_door43_fake::FaultRule::once(
        "GET",
        "/api/v1/repos/unfoldingWord/en_tn",
        tc_door43_fake::Fault::Status {
            status: 503,
            message: "maintenance".into(),
        },
    ));
    assert!(matches!(
        client.repo_exists(&repo("unfoldingWord/en_tn"), None).await,
        Err(Door43Error::Server { status: 503, .. })
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tags_walk_every_page_even_when_the_server_clamps_the_limit() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    fake.set_max_page_size(2);
    let tags = client(&fake)
        .tags(&repo("unfoldingWord/en_tn"))
        .await
        .unwrap();
    let mut names: Vec<String> = tags.iter().map(|t| t.name.to_string()).collect();
    names.sort();
    assert_eq!(names, ["v86", "v91", "v92"]);
    for t in &tags {
        let expected = fake
            .resolve("unfoldingWord", "en_tn", t.name.as_str())
            .await
            .unwrap();
        assert_eq!(t.commit, sha(&expected));
    }
    let pages = fake
        .requests()
        .into_iter()
        .filter(|r| r.path.ends_with("/tags"))
        .count();
    assert_eq!(pages, 2, "a full page of 2, then a short page of 1");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tag_resolves_to_its_commit_in_one_call() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    let tn = repo("unfoldingWord/en_tn");
    let commit = client.tag_commit(&tn, &tag("v86")).await.unwrap();
    assert_eq!(
        commit,
        sha(&fake.resolve("unfoldingWord", "en_tn", "v86").await.unwrap())
    );
    assert!(matches!(
        client.tag_commit(&tn, &tag("v1000")).await,
        Err(Door43Error::NotFound { .. })
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn releases_and_the_latest_published_one() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    let tn = repo("unfoldingWord/en_tn");
    let releases = client.releases(&tn).await.unwrap();
    let states: Vec<(String, ReleaseState)> = releases
        .iter()
        .map(|r| (r.tag.to_string(), r.state))
        .collect();
    assert_eq!(
        states,
        [
            ("v92".to_owned(), ReleaseState::PreRelease),
            ("v91".to_owned(), ReleaseState::Published),
            ("v86".to_owned(), ReleaseState::Published),
        ]
    );
    assert_eq!(client.latest_release(&tn).await.unwrap().tag, tag("v91"));
    assert!(matches!(
        client.latest_release(&repo("uW/en_ugl")).await,
        Err(Door43Error::NotFound { .. })
    ));
}
