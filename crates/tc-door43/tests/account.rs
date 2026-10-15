//! Signing in, organizations, and creating repositories.

mod common;

use common::{client, credentials, git, repo, run, token_name};
use tc_door43::{
    AuthStatus, Credentials, Door43Error, NewRepoOwner, OneTimeCode, Password, TokenScope,
    Visibility,
};
use tc_door43_fake::{FakeDoor43, seeded};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn minting_replaces_the_token_of_the_same_name() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    let first = client
        .mint_token(&credentials(), &token_name(), &TokenScope::FOR_SHARING)
        .await
        .unwrap();
    assert_eq!(first.username.as_str(), seeded::USER);
    let tokens = fake.tokens_of(seeded::USER);
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].name, "translationCore");
    assert_eq!(
        tokens[0].scopes,
        ["write:repository", "write:organization", "write:user"]
    );

    let second = client
        .mint_token(&credentials(), &token_name(), &TokenScope::FOR_SHARING)
        .await
        .unwrap();
    assert_ne!(first.token, second.token);
    assert_eq!(fake.tokens_of(seeded::USER).len(), 1);
    assert!(matches!(
        client.session_for_token(first.token).await,
        Err(Door43Error::AuthFailed {
            status: AuthStatus::Unauthorized,
            ..
        })
    ));
    let resumed = client
        .session_for_token(second.token.clone())
        .await
        .unwrap();
    assert_eq!(resumed, second);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_email_signs_in_as_the_account_login() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let by_email = Credentials::new(
        seeded::USER_EMAIL,
        Password::new(seeded::PASSWORD.to_owned()),
    )
    .unwrap();
    let session = client(&fake)
        .mint_token(&by_email, &token_name(), &TokenScope::FOR_SHARING)
        .await
        .unwrap();
    assert_eq!(session.username.as_str(), seeded::USER);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wrong_password_mints_nothing() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let wrong = Credentials::new(seeded::USER, Password::new("wrong".to_owned())).unwrap();
    let result = client(&fake)
        .mint_token(&wrong, &token_name(), &TokenScope::FOR_SHARING)
        .await;
    assert!(
        matches!(
            result,
            Err(Door43Error::AuthFailed {
                status: AuthStatus::Unauthorized,
                ..
            })
        ),
        "{result:?}"
    );
    assert!(fake.tokens_of(seeded::USER).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_two_factor_account_needs_its_code() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    let plain = Credentials::new(
        seeded::OTP_USER,
        Password::new(seeded::OTP_PASSWORD.to_owned()),
    )
    .unwrap();
    assert!(matches!(
        client
            .mint_token(&plain, &token_name(), &TokenScope::FOR_SHARING)
            .await,
        Err(Door43Error::AuthFailed { .. })
    ));
    let with_code = plain.with_one_time_code(OneTimeCode::new(seeded::OTP_CODE.to_owned()));
    let session = client
        .mint_token(&with_code, &token_name(), &TokenScope::FOR_SHARING)
        .await
        .unwrap();
    assert_eq!(session.username.as_str(), seeded::OTP_USER);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn organizations_come_with_whether_a_repository_may_be_created() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    let session = client
        .mint_token(&credentials(), &token_name(), &TokenScope::FOR_SHARING)
        .await
        .unwrap();
    let orgs: Vec<(String, String, bool)> = client
        .list_orgs(&session)
        .await
        .unwrap()
        .into_iter()
        .map(|o| (o.name.to_string(), o.full_name, o.can_create_repository))
        .collect();
    assert_eq!(
        orgs,
        [
            (
                seeded::TEAM_ORG.to_owned(),
                "Kaleo Translation Team".to_owned(),
                true
            ),
            (
                seeded::READ_ONLY_ORG.to_owned(),
                "Español Latinoamericano".to_owned(),
                false
            ),
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repositories_are_created_for_the_user_or_a_permitted_organization() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    let session = client
        .mint_token(&credentials(), &token_name(), &TokenScope::FOR_SHARING)
        .await
        .unwrap();
    let name = "en_tit_book".parse().unwrap();

    let mine = client
        .create_repo(
            &session,
            &NewRepoOwner::SignedInUser,
            &name,
            Visibility::Public,
        )
        .await
        .unwrap();
    assert_eq!(mine.repo, repo(&format!("{}/en_tit_book", seeded::USER)));
    assert_eq!(
        mine.clone_url.as_str(),
        format!("{}/{}/en_tit_book.git", fake.url(), seeded::USER)
    );
    assert!(client.server().is_same_origin(&mine.html_url));
    assert!(
        client
            .repo_exists(&mine.repo, Some(&session))
            .await
            .unwrap()
    );
    let info = client.repo(&mine.repo, Some(&session)).await.unwrap();
    assert!(
        info.empty,
        "auto_init is off, so the first push fast-forwards"
    );
    assert_eq!(info.default_branch, "main");

    assert!(matches!(
        client
            .create_repo(
                &session,
                &NewRepoOwner::SignedInUser,
                &name,
                Visibility::Public
            )
            .await,
        Err(Door43Error::NameExists { .. })
    ));

    let team = NewRepoOwner::Organization(seeded::TEAM_ORG.parse().unwrap());
    let shared = client
        .create_repo(&session, &team, &name, Visibility::Private)
        .await
        .unwrap();
    assert_eq!(shared.repo.owner.as_str(), seeded::TEAM_ORG);
    let info = client.repo(&shared.repo, Some(&session)).await.unwrap();
    assert_eq!(info.visibility, Visibility::Private);
    assert!(!client.repo_exists(&shared.repo, None).await.unwrap());

    let read_only = NewRepoOwner::Organization(seeded::READ_ONLY_ORG.parse().unwrap());
    assert!(matches!(
        client
            .create_repo(&session, &read_only, &name, Visibility::Public)
            .await,
        Err(Door43Error::AuthFailed {
            status: AuthStatus::Forbidden,
            ..
        })
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_token_without_the_create_scope_is_refused_by_name() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    let session = client
        .mint_token(
            &credentials(),
            &token_name(),
            &[TokenScope::WriteRepository, TokenScope::ReadUser],
        )
        .await
        .unwrap();
    let result = client
        .create_repo(
            &session,
            &NewRepoOwner::SignedInUser,
            &"project".parse().unwrap(),
            Visibility::Public,
        )
        .await;
    assert!(
        matches!(
            &result,
            Err(Door43Error::AuthFailed { status: AuthStatus::Forbidden, message, .. })
                if message.contains("required scope")
        ),
        "{result:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_created_repository_takes_a_push_with_the_minted_token() {
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
            &"shared-project".parse().unwrap(),
            Visibility::Public,
        )
        .await
        .unwrap();

    let work = tempfile::tempdir().unwrap();
    let project = work.path().join("project");
    std::fs::create_dir(&project).unwrap();
    run(git(&project, work.path()).args(["init", "--quiet", "--initial-branch", "main"])).await;
    std::fs::write(project.join("metadata.json"), "{}\n").unwrap();
    run(git(&project, work.path()).args(["add", "metadata.json"])).await;
    run(git(&project, work.path()).args(["commit", "--quiet", "-m", "first"])).await;
    let local = run(git(&project, work.path()).args(["rev-parse", "HEAD"])).await;

    let helper = "!f() { echo username=$DOOR43_USER; echo password=$DOOR43_TOKEN; }; f";
    run(git(&project, work.path())
        .env("DOOR43_USER", session.username.as_str())
        .env("DOOR43_TOKEN", session.token.expose_secret())
        .args([
            "-c",
            "credential.helper=",
            "-c",
            &format!("credential.helper={helper}"),
        ])
        .args(["push", "--quiet", created.clone_url.as_str(), "main"]))
    .await;
    assert_eq!(
        fake.resolve(seeded::USER, "shared-project", "main")
            .await
            .unwrap(),
        local
    );
    let info = client.repo(&created.repo, Some(&session)).await.unwrap();
    assert!(!info.empty);
}
