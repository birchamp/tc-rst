//! Discovery: catalog search, repository info, tags and releases.

mod common;

use common::{client, repo, sha, tag};
use tc_door43::{CatalogQuery, CatalogRef, Door43Error, ReleaseState, Stage, Visibility};
use tc_door43_fake::{
    CommitSeed, FakeDoor43, Fault, FaultRule, ReleaseKind, RepoSeed, SeedFile, seeded,
};

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
        .unwrap()
        .items;
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
        .unwrap()
        .items;
    assert_eq!(refs(preprod), [CatalogRef::Tag(tag("v92"))]);

    let history = client
        .catalog_search(&notes(Some(Stage::Prod), true))
        .await
        .unwrap()
        .items;
    let mut history = refs(history);
    history.sort_by_key(|r| format!("{r:?}"));
    assert_eq!(
        history,
        [CatalogRef::Tag(tag("v86")), CatalogRef::Tag(tag("v91"))]
    );

    let latest = client
        .catalog_search(&notes(Some(Stage::Latest), true))
        .await
        .unwrap()
        .items;
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
        .unwrap()
        .items;
    assert_eq!(spanish.len(), 1);
    assert_eq!(spanish[0].repo, repo("es-419_gl/es-419_tn"));
    assert_eq!(spanish[0].language, "es-419");

    let none = client
        .catalog_search(&CatalogQuery {
            topics: vec!["no-such-topic".into()],
            ..CatalogQuery::default()
        })
        .await
        .unwrap()
        .items;
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
    assert_eq!(tags.skipped, []);
    let tags = tags.items;
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
    assert_eq!(releases.skipped, []);
    let releases = releases.items;
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

fn answer_once(fake: &FakeDoor43, path: &str, body: serde_json::Value) {
    fake.inject(FaultRule::once(
        "GET",
        path,
        Fault::Raw {
            status: 200,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: body.to_string().into_bytes(),
        },
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_repository_gitea_allows_but_the_app_would_not_create_is_found() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let metadata = common::fixture("tc4/test-fixtures/resources/en_tn@v86/metadata.json");
    fake.add_repo(RepoSeed {
        owner: "someone".into(),
        name: "notes_".into(),
        subject: "TSV Translation Notes".into(),
        language: "en".into(),
        topics: vec![],
        private: false,
        commits: vec![CommitSeed {
            message: "x".into(),
            files: vec![SeedFile::regular("metadata.json", metadata)],
            tag: Some("v1".into()),
            release: Some(ReleaseKind::Published),
        }],
    })
    .await
    .unwrap();
    let found = client(&fake)
        .catalog_search(&CatalogQuery {
            subjects: vec!["TSV Translation Notes".into()],
            ..CatalogQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(found.skipped, []);
    assert!(
        found.items.iter().any(|e| e.repo == repo("someone/notes_")),
        "{:?}",
        found.items
    );
    let info = client(&fake)
        .repo(&repo("someone/notes_"), None)
        .await
        .unwrap();
    assert_eq!(info.repo.name.as_str(), "notes_");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tags_git_allows_are_listed_and_resolved() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let commit = |text: &str, tag: &str| CommitSeed {
        message: text.into(),
        files: vec![SeedFile::regular("a.txt", text.as_bytes().to_vec())],
        tag: Some(tag.into()),
        release: Some(ReleaseKind::Published),
    };
    let shas = fake
        .add_repo(RepoSeed {
            owner: "uW".into(),
            name: "tagged".into(),
            subject: String::new(),
            language: "en".into(),
            topics: vec![],
            private: false,
            commits: vec![
                commit("a", "v1"),
                commit("b", "v2@rc1"),
                commit("c", "versión#3"),
            ],
        })
        .await
        .unwrap();
    let client = client(&fake);
    let tagged = repo("uW/tagged");
    let tags = client.tags(&tagged).await.unwrap();
    assert_eq!(tags.skipped, []);
    let mut names: Vec<&str> = tags.items.iter().map(|t| t.name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["v1", "v2@rc1", "versión#3"]);
    assert_eq!(
        client.tag_commit(&tagged, &tag("versión#3")).await.unwrap(),
        sha(&shas[2])
    );
    assert_eq!(
        client.tag_commit(&tagged, &tag("v2@rc1")).await.unwrap(),
        sha(&shas[1])
    );
    let releases = client.releases(&tagged).await.unwrap();
    assert_eq!(releases.skipped, []);
    assert_eq!(releases.items.len(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_unreadable_entry_is_set_aside_and_the_rest_are_kept() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    let good_sha = fake.resolve("unfoldingWord", "en_tn", "v86").await.unwrap();
    answer_once(
        &fake,
        "/api/v1/catalog/search",
        serde_json::json!({ "ok": true, "data": [
            { "owner": "unfoldingWord", "name": "en_tn", "ref_type": "tag",
              "branch_or_tag_name": "v86", "commit_sha": good_sha },
            { "owner": "someone", "name": "x", "ref_type": "tag",
              "branch_or_tag_name": "v1", "commit_sha": "" },
            { "owner": "someone", "name": "a/b", "ref_type": "tag",
              "branch_or_tag_name": "v1", "commit_sha": good_sha },
            { "name": "no-owner" },
        ]}),
    );
    let found = client
        .catalog_search(&CatalogQuery::default())
        .await
        .unwrap();
    assert_eq!(found.items.len(), 1);
    assert_eq!(found.items[0].commit, sha(&good_sha));
    let skipped: Vec<&str> = found.skipped.iter().map(|s| s.entry.as_str()).collect();
    assert_eq!(skipped, ["someone/x v1", "someone/a/b v1", "?/no-owner ?"]);
    assert!(
        found.skipped[0].reason.contains("commit sha"),
        "{:?}",
        found.skipped
    );

    answer_once(
        &fake,
        "/api/v1/repos/unfoldingWord/en_tn/tags",
        serde_json::json!([
            { "name": "v86", "commit": { "sha": good_sha } },
            { "name": "bad tag", "commit": { "sha": good_sha } },
            { "name": "v87", "commit": { "sha": "not-a-sha" } },
        ]),
    );
    let tags = client.tags(&repo("unfoldingWord/en_tn")).await.unwrap();
    assert_eq!(tags.items.len(), 1);
    assert_eq!(tags.skipped.len(), 2);
    assert_eq!(tags.skipped[0].entry, "bad tag");

    answer_once(
        &fake,
        "/api/v1/repos/unfoldingWord/en_tn/releases",
        serde_json::json!([
            { "id": 1, "tag_name": "v86", "name": "v86" },
            { "id": 2, "tag_name": "", "name": "broken" },
            { "tag_name": "v3" },
        ]),
    );
    let releases = client.releases(&repo("unfoldingWord/en_tn")).await.unwrap();
    assert_eq!(releases.items.len(), 1);
    let skipped: Vec<&str> = releases.skipped.iter().map(|s| s.entry.as_str()).collect();
    assert_eq!(skipped, ["release 2 ()", "release ? (v3)"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_answer_about_another_tag_or_commit_is_refused() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    let tn = repo("unfoldingWord/en_tn");
    let v91 = fake.resolve("unfoldingWord", "en_tn", "v91").await.unwrap();
    answer_once(
        &fake,
        "/api/v1/repos/unfoldingWord/en_tn/tags/v86",
        serde_json::json!({ "name": "v91", "commit": { "sha": v91 } }),
    );
    let result = client.tag_commit(&tn, &tag("v86")).await;
    assert!(
        matches!(&result, Err(Door43Error::UnexpectedResponse { reason, .. }) if reason.contains("v91")),
        "{result:?}"
    );

    let head = fake.resolve("uW", "en_ugl", "master").await.unwrap();
    let tree = fake.tree_of("uW", "en_ugl", &head).await.unwrap();
    answer_once(
        &fake,
        "/api/v1/repos/uW/en_ugl/git/commits/",
        serde_json::json!({ "sha": v91, "commit": { "tree": { "sha": tree } } }),
    );
    let result = client
        .download_archive(&repo("uW/en_ugl"), &sha(&head))
        .await;
    assert!(
        matches!(&result, Err(Door43Error::UnexpectedResponse { reason, .. }) if reason.contains(&v91)),
        "{result:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repository_addresses_off_the_server_are_refused() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    for (html, clone) in [
        (
            "https://evil.example/me/proj".to_owned(),
            format!("{}/me/proj.git", fake.url()),
        ),
        (
            format!("{}/me/proj", fake.url()),
            "https://user:pw@evil.example/me/proj.git".to_owned(),
        ),
        (
            format!("{}/me/proj", fake.url()),
            fake.url().replace("http://", "http://user:pw@") + "/me/proj.git",
        ),
    ] {
        answer_once(
            &fake,
            "/api/v1/repos/me/proj",
            serde_json::json!({
                "owner": { "login": "me" }, "name": "proj", "full_name": "me/proj",
                "default_branch": "main", "html_url": html, "clone_url": clone,
            }),
        );
        let result = client.repo(&repo("me/proj"), None).await;
        assert!(
            matches!(&result, Err(Door43Error::UnexpectedResponse { reason, .. }) if reason.contains("address")),
            "{result:?}"
        );
    }
}
