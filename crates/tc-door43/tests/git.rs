//! Sync's git calls (`tc_door43::git`) against the fake's smart HTTP.

// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(clippy::unwrap_used)]

mod common;

use common::{client, credentials, repo, token_name};
use std::time::{Duration, Instant};
use tc_door43::{
    Door43Client, Door43Error, GitRemote, NewRepoOwner, Session, Token, TokenScope, Visibility, git,
};
use tc_door43_fake::{FakeDoor43, Fault, FaultRule, seeded};

fn commit(repo: &git2::Repository, text: &str) -> git2::Oid {
    let mut tree = repo.treebuilder(None).unwrap();
    tree.insert("README.md", repo.blob(text.as_bytes()).unwrap(), 0o100_644)
        .unwrap();
    let tree = repo.find_tree(tree.write().unwrap()).unwrap();
    let signature = git2::Signature::now("Test", "test@example.org").unwrap();
    repo.commit(
        Some("refs/heads/main"),
        &signature,
        &signature,
        text,
        &tree,
        &[],
    )
    .unwrap()
}

fn remote(client: &Door43Client, path: &str, session: Option<Session>) -> GitRemote {
    GitRemote {
        server: client.server().clone(),
        repo: repo(path),
        session,
    }
}

/// Runs `call` with a fresh bare repository on a blocking thread, as sync does.
async fn in_repo<T: Send + 'static>(
    call: impl FnOnce(&git2::Repository) -> T + Send + 'static,
) -> T {
    tokio::task::spawn_blocking(move || {
        let dir = tempfile::tempdir().unwrap();
        call(&git2::Repository::init_bare(dir.path()).unwrap())
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fetch_and_a_push_round_trip_through_the_fake() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    let session = client
        .mint_token(&credentials(), &token_name(), &TokenScope::FOR_SHARING)
        .await
        .unwrap();
    client
        .create_repo(
            &session,
            &NewRepoOwner::SignedInUser,
            &"synced".parse().unwrap(),
            Visibility::Public,
        )
        .await
        .unwrap();
    let path = format!("{}/synced", seeded::USER);
    let target = remote(&client, &path, Some(session));
    let http = client.http().clone();
    let (pushed, fetched, advertised) = in_repo(move |local| {
        let pushed = commit(local, "In the beginning");
        git::push(&http, local, &target, &["refs/heads/main:refs/heads/main"]).unwrap();
        let other = tempfile::tempdir().unwrap();
        let other = git2::Repository::init_bare(other.path()).unwrap();
        git::fetch(
            &http,
            &other,
            &target,
            &["refs/heads/main:refs/remotes/door43/main"],
        )
        .unwrap();
        let fetched = other.refname_to_id("refs/remotes/door43/main").unwrap();
        let advertised = git::ls_remote(&http, &other, &target).unwrap();
        (pushed, fetched, advertised)
    })
    .await;
    assert_eq!(fetched, pushed);
    assert_eq!(
        fake.resolve(seeded::USER, "synced", "main").await.unwrap(),
        pushed.to_string()
    );
    assert!(
        advertised
            .iter()
            .any(|(name, oid)| name == "refs/heads/main" && oid.to_string() == pushed.to_string()),
        "{advertised:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_push_reads_as_door43s_refusal() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    // The seeded user is a member of this organization, without write access.
    let token = fake
        .issue_token(seeded::USER, "push", &["write:repository"])
        .unwrap();
    let session = Session {
        username: seeded::USER.parse().unwrap(),
        token: Token::new(token).unwrap(),
    };
    let target = remote(
        &client,
        &format!("{}/es-419_tn", seeded::READ_ONLY_ORG),
        Some(session),
    );
    let http = client.http().clone();
    let error = in_repo(move |local| {
        commit(local, "a probe");
        git::push(
            &http,
            local,
            &target,
            &["refs/heads/main:refs/heads/tc-probe"],
        )
        .unwrap_err()
    })
    .await;
    assert!(
        matches!(
            &error,
            Door43Error::NoWriteAccess { route, server_message }
                if route == "POST /es-419_gl/es-419_tn.git/git-receive-pack"
                    && server_message == "User permission denied for writing."
        ),
        "{error:?}"
    );
    assert!(
        fake.resolve(seeded::READ_ONLY_ORG, "es-419_tn", "refs/heads/tc-probe")
            .await
            .is_err()
    );
}

/// INV-NO-REQUEST-WHILE-SWITCH-OFF: a switch turned off during a fetch stops it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fetch_stops_when_the_switch_goes_off() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    fake.inject(FaultRule::once(
        "POST",
        "/unfoldingWord/en_tn.git/git-upload-pack",
        Fault::Hold,
    ));
    let target = remote(&client, "unfoldingWord/en_tn", None);
    let http = client.http().clone();
    let fetch = in_repo(move |local| {
        let result = git::fetch(&http, local, &target, &["refs/tags/*:refs/tags/*"]);
        (result, local.references().unwrap().count())
    });
    let fetch = tokio::spawn(fetch);
    let started = Instant::now();
    while !fake
        .requests()
        .iter()
        .any(|r| r.method == "POST" && r.path.ends_with("git-upload-pack"))
    {
        assert!(started.elapsed() < Duration::from_secs(30), "no fetch came");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    client.internet().turn_off();
    fake.release_held();
    let (result, refs) = fetch.await.unwrap();
    assert!(
        matches!(&result, Err(Door43Error::Offline { route })
            if route == "POST /unfoldingWord/en_tn.git/git-upload-pack"),
        "{result:?}"
    );
    assert_eq!(refs, 0, "the stopped fetch wrote no ref");
}

/// A signed-in client for a new public repository `synced` of the seeded user.
async fn new_repo(fake: &FakeDoor43) -> (Door43Client, Session) {
    let client = client(fake);
    let session = client
        .mint_token(&credentials(), &token_name(), &TokenScope::FOR_SHARING)
        .await
        .unwrap();
    client
        .create_repo(
            &session,
            &NewRepoOwner::SignedInUser,
            &"synced".parse().unwrap(),
            Visibility::Public,
        )
        .await
        .unwrap();
    (client, session)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_push_that_does_not_build_on_the_branch_reads_as_ref_moved() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let (client, session) = new_repo(&fake).await;
    let target = remote(&client, &format!("{}/synced", seeded::USER), Some(session));
    let http = client.http().clone();
    let error = in_repo(move |first| {
        commit(first, "a teammate's work");
        git::push(&http, first, &target, &["refs/heads/main:refs/heads/main"]).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let second = git2::Repository::init_bare(dir.path()).unwrap();
        commit(&second, "this device's work");
        git::push(
            &http,
            &second,
            &target,
            &["refs/heads/main:refs/heads/main"],
        )
        .unwrap_err()
    })
    .await;
    assert!(matches!(&error, Door43Error::RefMoved { .. }), "{error:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_token_is_auth_failed_and_offered_once() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let (client, _) = new_repo(&fake).await;
    let wrong = Session {
        username: seeded::USER.parse().unwrap(),
        token: Token::new("not-the-token".to_owned()).unwrap(),
    };
    let target = remote(&client, &format!("{}/synced", seeded::USER), Some(wrong));
    let http = client.http().clone();
    let error = in_repo(move |local| {
        commit(local, "work");
        git::push(&http, local, &target, &["refs/heads/main:refs/heads/main"]).unwrap_err()
    })
    .await;
    assert!(
        matches!(
            &error,
            Door43Error::AuthFailed {
                status: tc_door43::AuthStatus::Unauthorized,
                ..
            }
        ),
        "{error:?}"
    );
    let pack_requests = fake
        .requests()
        .iter()
        .filter(|r| r.path.contains("synced.git"))
        .count();
    assert!(pack_requests <= 2, "{pack_requests} requests");
}
