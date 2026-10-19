//! D95: every request goes through the session's internet switch, which starts off.

mod common;

use common::{credentials, left_in, repo, sha, tag, token_name};
use std::time::{Duration, Instant};
use tc_door43::{
    CatalogQuery, Door43Client, Door43Error, DownloadTo, InternetSwitch, NewRepoOwner, Session,
    Token, TokenScope, Visibility,
};
use tc_door43_fake::{FakeDoor43, Fault, FaultRule};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_call_is_refused_before_anything_is_sent_while_the_switch_is_off() {
    let dl = tempfile::tempdir().unwrap();
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let v86 = sha(&fake.resolve("unfoldingWord", "en_tn", "v86").await.unwrap());
    let client = Door43Client::new(fake.url().parse().unwrap(), InternetSwitch::new()).unwrap();
    let tn = repo("unfoldingWord/en_tn");
    let session = Session {
        username: "translator".parse().unwrap(),
        token: Token::new("not-sent".to_owned()).unwrap(),
    };

    let outcomes: Vec<(&str, Result<(), Door43Error>)> = vec![
        (
            "catalog_search",
            client
                .catalog_search(&CatalogQuery::default())
                .await
                .map(drop),
        ),
        ("repo", client.repo(&tn, None).await.map(drop)),
        ("repo_exists", client.repo_exists(&tn, None).await.map(drop)),
        ("tags", client.tags(&tn).await.map(drop)),
        (
            "tag_commit",
            client.tag_commit(&tn, &tag("v86")).await.map(drop),
        ),
        ("releases", client.releases(&tn).await.map(drop)),
        ("latest_release", client.latest_release(&tn).await.map(drop)),
        (
            "download_archive",
            client
                .download_archive(&tn, &v86, DownloadTo::folder(dl.path()))
                .await
                .map(drop),
        ),
        (
            "download_sb_export",
            client
                .download_sb_export(&tn, &tag("v86"), &v86, DownloadTo::folder(dl.path()))
                .await
                .map(drop),
        ),
        (
            "mint_token",
            client
                .mint_token(&credentials(), &token_name(), &TokenScope::FOR_SHARING)
                .await
                .map(drop),
        ),
        (
            "session_for_token",
            client
                .session_for_token(Token::new("x".to_owned()).unwrap())
                .await
                .map(drop),
        ),
        ("list_orgs", client.list_orgs(&session).await.map(drop)),
        (
            "count_repositories",
            client
                .count_repositories(Some(&session), &"Equipo-Ejemplo".parse().unwrap(), "es-419")
                .await
                .map(drop),
        ),
        (
            "create_repo",
            client
                .create_repo(
                    &session,
                    &NewRepoOwner::SignedInUser,
                    &"project".parse().unwrap(),
                    Visibility::Public,
                )
                .await
                .map(drop),
        ),
    ];
    for (call, outcome) in outcomes {
        assert!(
            matches!(outcome, Err(Door43Error::Offline { .. })),
            "{call}: {outcome:?}"
        );
    }
    assert_eq!(fake.requests(), []);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn turning_the_switch_on_and_off_takes_effect_on_the_next_request() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let switch = InternetSwitch::new();
    let client = Door43Client::new(fake.url().parse().unwrap(), switch.clone()).unwrap();
    let tn = repo("unfoldingWord/en_tn");

    switch.turn_on();
    assert!(client.repo_exists(&tn, None).await.unwrap());
    switch.turn_off();
    assert!(matches!(
        client.repo_exists(&tn, None).await,
        Err(Door43Error::Offline { .. })
    ));
    assert_eq!(fake.requests().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_download_waiting_for_its_answer_stops_when_the_switch_goes_off() {
    let dl = tempfile::tempdir().unwrap();
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let v86 = sha(&fake.resolve("unfoldingWord", "en_tn", "v86").await.unwrap());
    fake.inject(FaultRule::once(
        "GET",
        "/unfoldingWord/en_tn/sb/",
        Fault::Delay(Duration::from_secs(5)),
    ));
    let switch = InternetSwitch::new();
    switch.turn_on();
    let client = Door43Client::new(fake.url().parse().unwrap(), switch.clone()).unwrap();
    let folder = dl.path().to_owned();
    let download = tokio::spawn(async move {
        client
            .download_sb_export(
                &repo("unfoldingWord/en_tn"),
                &tag("v86"),
                &v86,
                DownloadTo::folder(&folder),
            )
            .await
    });
    // Wait until the request has reached the server, then turn the internet off while
    // the answer is still coming.
    while !fake.requests().iter().any(|r| r.path.contains("/sb/")) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    switch.turn_off();
    let off = Instant::now();
    let result = download.await.unwrap();
    assert!(
        matches!(&result, Err(Door43Error::Offline { route }) if route.contains("/sb/v86.zip")),
        "{result:?}"
    );
    assert!(
        off.elapsed() < Duration::from_secs(2),
        "the download stopped {:?} after the switch went off, not within the check interval",
        off.elapsed()
    );
    assert_eq!(left_in(dl.path()), Vec::<String>::new());
}
