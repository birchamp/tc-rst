//! A push through libgit2 to the fake, which refuses an account without write access as
//! Door43 does: HTTP 200 and a ref declined by the pre-receive hook.

// The helper sits outside `#[test]` functions, where clippy.toml already allows this.
#![allow(clippy::unwrap_used)]

mod common;

use common::{client, credentials, repo, token_name};
use std::sync::{Arc, Mutex};
use tc_door43::{Door43Error, NewRepoOwner, RepoRef, TokenScope, Visibility, refused_push};
use tc_door43_fake::{FakeDoor43, seeded};

/// What libgit2 reported of one push.
struct Pushed {
    commit: String,
    result: Result<(), git2::Error>,
    /// Each ref's status from `push_update_reference`; `None` when it was accepted.
    statuses: Vec<(String, Option<String>)>,
    /// All that `sideband_progress` received.
    remote_text: String,
}

/// Push one new commit to `branch` of `url`, signed in as `login` with `token`. The commit
/// holds `files` files; files after the first differ in one line, so libgit2 sends most of
/// them as deltas.
async fn push(url: String, login: &str, token: String, branch: &str, files: usize) -> Pushed {
    let login = login.to_owned();
    let branch = branch.to_owned();
    tokio::task::spawn_blocking(move || {
        let dir = tempfile::tempdir().unwrap();
        let local = git2::Repository::init_bare(dir.path()).unwrap();
        let mut tree = local.treebuilder(None).unwrap();
        let blob = local.blob(b"hello\n").unwrap();
        tree.insert("README.md", blob, 0o100_644).unwrap();
        let verses: String = (1..=80)
            .map(|verse| format!("\\v {verse} In the beginning was the Word.\n"))
            .collect();
        for file in 1..files {
            let blob = local.blob(format!("\\id {file:03}\n{verses}").as_bytes());
            tree.insert(format!("{file:03}.usfm"), blob.unwrap(), 0o100_644)
                .unwrap();
        }
        let tree = local.find_tree(tree.write().unwrap()).unwrap();
        let signature = git2::Signature::now("Test", "test@example.org").unwrap();
        let reference = format!("refs/heads/{branch}");
        let commit = local
            .commit(
                Some(&reference),
                &signature,
                &signature,
                "first",
                &tree,
                &[],
            )
            .unwrap();
        push_reference(&local, &url, &login, &token, &reference, commit, || {})
    })
    .await
    .unwrap()
}

/// Push `reference` of `local`, which holds `commit`, to `url`, signed in as `login` with
/// `token`. `before_pack` runs after libgit2 has read the advertisement and checked the push
/// against it, before it sends the pack.
fn push_reference(
    local: &git2::Repository,
    url: &str,
    login: &str,
    token: &str,
    reference: &str,
    commit: git2::Oid,
    mut before_pack: impl FnMut(),
) -> Pushed {
    let statuses = Arc::new(Mutex::new(Vec::new()));
    let remote_text = Arc::new(Mutex::new(String::new()));
    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(|_, _, _| git2::Cred::userpass_plaintext(login, token));
    let seen = Arc::clone(&statuses);
    callbacks.push_update_reference(move |name, status| {
        let entry = (name.to_owned(), status.map(str::to_owned));
        seen.lock().unwrap().push(entry);
        Ok(())
    });
    let heard = Arc::clone(&remote_text);
    callbacks.sideband_progress(move |text| {
        heard
            .lock()
            .unwrap()
            .push_str(&String::from_utf8_lossy(text));
        true
    });
    callbacks.push_negotiation(move |_| {
        before_pack();
        Ok(())
    });
    let mut options = git2::PushOptions::new();
    options.remote_callbacks(callbacks);
    let mut remote = local.remote_anonymous(url).unwrap();
    let result = remote.push(&[format!("{reference}:{reference}")], Some(&mut options));
    drop(remote);
    Pushed {
        commit: commit.to_string(),
        result,
        statuses: statuses.lock().unwrap().clone(),
        remote_text: remote_text.lock().unwrap().clone(),
    }
}

/// Where a teammate's push lands while this push is between the advertisement and the pack.
#[derive(Debug, Clone, Copy)]
enum Race {
    /// On top of the branch this push builds on.
    Moved,
    /// On a branch that did not exist when this push read the advertisement.
    Created,
}

/// Push one commit to `main` of `url`, signed in as `login` with `token`, while a teammate's
/// commit lands on `main` in `server`, the fake's repository, after libgit2 has checked this
/// push against the advertisement. Returns what libgit2 reported and the teammate's commit.
async fn push_losing_a_race(
    url: String,
    login: &str,
    token: String,
    server: std::path::PathBuf,
    race: Race,
) -> (Pushed, String) {
    let login = login.to_owned();
    tokio::task::spawn_blocking(move || {
        let dir = tempfile::tempdir().unwrap();
        let local = git2::Repository::init_bare(dir.path()).unwrap();
        let reference = "refs/heads/main";
        let mut parents = Vec::new();
        if let Race::Moved = race {
            let first = readme_commit(&local, "first\n", &[]);
            local.reference(reference, first, true, "first").unwrap();
            let landed = push_reference(&local, &url, &login, &token, reference, first, || {});
            assert!(landed.result.is_ok(), "{:?}", landed.result);
            assert_eq!(landed.statuses, [(reference.to_owned(), None)]);
            parents.push(first);
        }
        let mine = readme_commit(&local, "mine\n", &parents);
        local.reference(reference, mine, true, "mine").unwrap();
        let mut theirs = None;
        let pushed = push_reference(&local, &url, &login, &token, reference, mine, || {
            let server = git2::Repository::open_bare(&server).unwrap();
            let commit = readme_commit(&server, "theirs\n", &parents);
            server
                .reference(reference, commit, true, "a teammate's push")
                .unwrap();
            theirs = Some(commit);
        });
        (pushed, theirs.unwrap().to_string())
    })
    .await
    .unwrap()
}

/// A commit in `repo` whose tree holds only `README.md` with `text`.
fn readme_commit(repo: &git2::Repository, text: &str, parents: &[git2::Oid]) -> git2::Oid {
    let mut tree = repo.treebuilder(None).unwrap();
    let blob = repo.blob(text.as_bytes()).unwrap();
    tree.insert("README.md", blob, 0o100_644).unwrap();
    let tree = repo.find_tree(tree.write().unwrap()).unwrap();
    let parents: Vec<git2::Commit> = parents
        .iter()
        .map(|id| repo.find_commit(*id).unwrap())
        .collect();
    let parents: Vec<&git2::Commit> = parents.iter().collect();
    let signature = git2::Signature::now("Test", "test@example.org").unwrap();
    repo.commit(None, &signature, &signature, text, &tree, &parents)
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_push_without_write_access_is_no_write_access_and_makes_no_branch() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    // The seeded user is a member of this organization, without write access.
    let target: RepoRef = repo(&format!("{}/es-419_tn", seeded::READ_ONLY_ORG));
    let token = fake
        .issue_token(seeded::USER, "push", &["write:repository"])
        .unwrap();
    let url = fake.clone_url(seeded::READ_ONLY_ORG, "es-419_tn");

    let pushed = push(url, seeded::USER, token, "tc-probe", 1).await;

    // As Door43: libgit2 sees no HTTP error, only the declined ref.
    assert!(pushed.result.is_ok(), "{:?}", pushed.result);
    assert_eq!(
        pushed.statuses,
        [(
            "refs/heads/tc-probe".to_owned(),
            Some("pre-receive hook declined".to_owned())
        )]
    );
    let (name, status) = &pushed.statuses[0];
    let error = refused_push(&target, status.as_deref().unwrap(), &pushed.remote_text);
    assert!(
        matches!(
            &error,
            Door43Error::NoWriteAccess { route, server_message }
                if route == "POST /es-419_gl/es-419_tn.git/git-receive-pack"
                    && server_message == "User permission denied for writing."
        ),
        "{name}: {error:?}"
    );
    assert!(
        fake.resolve(seeded::READ_ONLY_ORG, "es-419_tn", "refs/heads/tc-probe")
            .await
            .is_err()
    );
    let receive_pack: Vec<_> = fake
        .requests()
        .into_iter()
        .filter(|r| r.path.ends_with("git-receive-pack") || r.path.ends_with("info/refs"))
        .map(|r| r.method)
        .collect();
    assert_eq!(receive_pack.last().map(String::as_str), Some("POST"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_same_push_lands_where_the_account_may_write() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    let session = client
        .mint_token(&credentials(), &token_name(), &TokenScope::FOR_SHARING)
        .await
        .unwrap();
    let created = client
        .create_repo(
            &session,
            &NewRepoOwner::SignedInUser,
            &"pushed-project".parse().unwrap(),
            Visibility::Public,
        )
        .await
        .unwrap();

    let pushed = push(
        created.clone_url.to_string(),
        session.username.as_str(),
        session.token.expose_secret().to_owned(),
        "main",
        1,
    )
    .await;

    assert!(pushed.result.is_ok(), "{:?}", pushed.result);
    assert_eq!(pushed.statuses, [("refs/heads/main".to_owned(), None)]);
    assert_eq!(
        fake.resolve(seeded::USER, "pushed-project", "main")
            .await
            .unwrap(),
        pushed.commit
    );
}

/// A first share sends hundreds of objects. Door43's `git receive-pack` resolves their deltas
/// before its hook runs, and writes git's progress on the side band that carries the hook's
/// text. Only the hook's text is Door43's answer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_large_push_without_write_access_reports_only_door43s_words() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let target: RepoRef = repo(&format!("{}/es-419_tn", seeded::READ_ONLY_ORG));
    let token = fake
        .issue_token(seeded::USER, "push", &["write:repository"])
        .unwrap();
    let url = fake.clone_url(seeded::READ_ONLY_ORG, "es-419_tn");

    let pushed = push(url, seeded::USER, token, "tc-probe", 150).await;

    assert!(pushed.result.is_ok(), "{:?}", pushed.result);
    assert!(
        pushed.remote_text.starts_with("Resolving deltas:"),
        "the push should have carried git's progress: {:?}",
        pushed.remote_text
    );
    let [(_, Some(status))] = &pushed.statuses[..] else {
        panic!("one declined ref expected: {:?}", pushed.statuses);
    };
    assert_eq!(status, "pre-receive hook declined");
    let error = refused_push(&target, status, &pushed.remote_text);
    assert!(
        matches!(
            &error,
            Door43Error::NoWriteAccess { server_message, .. }
                if server_message == "User permission denied for writing."
        ),
        "{error:?}"
    );
    assert!(
        fake.resolve(seeded::READ_ONLY_ORG, "es-419_tn", "refs/heads/tc-probe")
            .await
            .is_err()
    );
}

/// Door43 answers a receive-pack request with HTTP 403 before any hook runs when the token
/// lacks `write:repository`. libgit2 reports that as an HTTP error, not as
/// `ErrorCode::Auth`, and the push never counts as a success (STORAGE.md §9.5).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_403_on_push_is_an_http_error_and_makes_no_branch() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    let session = client
        .mint_token(&credentials(), &token_name(), &TokenScope::FOR_SHARING)
        .await
        .unwrap();
    let created = client
        .create_repo(
            &session,
            &NewRepoOwner::SignedInUser,
            &"pushed-project".parse().unwrap(),
            Visibility::Public,
        )
        .await
        .unwrap();
    let reader = fake
        .issue_token(seeded::USER, "reader", &["read:repository"])
        .unwrap();

    let pushed = push(
        created.clone_url.to_string(),
        seeded::USER,
        reader,
        "main",
        1,
    )
    .await;

    // libgit2 passes neither the status code nor the body, only this message.
    let error = pushed.result.unwrap_err();
    assert_eq!(error.class(), git2::ErrorClass::Http, "{error:?}");
    assert_eq!(error.code(), git2::ErrorCode::GenericError, "{error:?}");
    assert_eq!(error.message(), "unexpected http status code: 403");
    assert!(pushed.statuses.is_empty(), "{:?}", pushed.statuses);
    assert!(
        fake.resolve(seeded::USER, "pushed-project", "main")
            .await
            .is_err()
    );
}

/// A teammate's push lands after libgit2 has checked this push against the advertisement, so
/// git's receive-pack refuses the ref, with `failed to update ref` before git 2.51 and with
/// `incorrect old value provided` or `reference already exists` from 2.51 on, as on Door43.
/// Each is `RefMoved`, so the sync fetches again and retries, and the teammate's commit stays.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_push_that_loses_a_race_is_ref_moved_and_keeps_the_teammates_commit() {
    for race in [Race::Moved, Race::Created] {
        let fake = FakeDoor43::start_seeded().await.unwrap();
        let client = client(&fake);
        let session = client
            .mint_token(&credentials(), &token_name(), &TokenScope::FOR_SHARING)
            .await
            .unwrap();
        let created = client
            .create_repo(
                &session,
                &NewRepoOwner::SignedInUser,
                &"raced-project".parse().unwrap(),
                Visibility::Public,
            )
            .await
            .unwrap();
        let target: RepoRef = repo(&format!("{}/raced-project", seeded::USER));

        let (pushed, theirs) = push_losing_a_race(
            created.clone_url.to_string(),
            session.username.as_str(),
            session.token.expose_secret().to_owned(),
            fake.git_dir(seeded::USER, "raced-project"),
            race,
        )
        .await;

        assert!(pushed.result.is_ok(), "{race:?}: {:?}", pushed.result);
        let [(name, Some(status))] = &pushed.statuses[..] else {
            panic!("{race:?}: one refused ref expected: {:?}", pushed.statuses);
        };
        assert_eq!(name, "refs/heads/main");
        let error = refused_push(&target, status, &pushed.remote_text);
        assert!(
            matches!(&error, Door43Error::RefMoved { .. }),
            "{race:?}: {error:?}"
        );
        assert_eq!(
            fake.resolve(seeded::USER, "raced-project", "main")
                .await
                .unwrap(),
            theirs,
            "{race:?}"
        );
        assert_ne!(pushed.commit, theirs);
    }
}
