//! The share dialog's first step, the repository link and Open from Door43 (unit B9b), against
//! the fake Door43 on loopback with the parity account: ports of tC4's
//! `test/share/recommend.test.ts` and of the server half of `test/openFromDoor43Dialog.test.tsx`.

use tc_api::account::{OrgChoice, Orgs, RepoLink, ShareTargets, SignIn, WebUrl};
use tc_api::door43::{OpenDoor43, RepoName, RepoSource, ShareOnDoor43, ShareOwner, SharedRepo};
use tc_api::endpoint::NetKind;
use tc_api::error::{Feature, SignInReason};
use tc_api::ids::{ProjectRef, RequestId};
use tc_api::jobs::{JobOutcome, JobState};
use tc_api::net::Set;
use tc_api::refusal::{OpenDoor43Refusal, ShareRefusal};
use tc_api::{ApiError, Refusal};
use tc_app::App;
use tc_app::share::recommend;
use tc_door43_fake::seeded::{PARITY_PASSWORD, PARITY_USER};
use tc_door43_fake::{FakeDoor43, Fault, FaultRule};

use super::common::{Parts, TestApp, post, rid, until};

async fn fake() -> FakeDoor43 {
    let fake = FakeDoor43::start().await.unwrap();
    tc_door43_fake::seeded::parity_account(&fake).await.unwrap();
    fake
}

async fn app_on(dir: &std::path::Path, door43: &str) -> TestApp {
    let url = door43.to_owned();
    TestApp::start_with(dir, &Parts::new(), |config, _| {
        config.door43 = url.parse().unwrap();
    })
    .await
}

async fn internet(app: &App, on: bool, id: u8) {
    let mut bytes = [0xB9; 16];
    bytes[0] = id;
    post(
        app,
        &Set {
            request: RequestId::from_bytes(bytes),
            on,
        },
    )
    .await
    .unwrap();
}

async fn sign_in(app: &App) {
    post(
        app,
        &SignIn {
            request: rid(1),
            login: PARITY_USER.to_owned(),
            password: PARITY_PASSWORD.to_owned(),
            stay: false,
        },
    )
    .await
    .unwrap();
}

fn orgs(language: &str) -> Orgs {
    Orgs {
        language: language.to_owned(),
    }
}

fn api_requests(fake: &FakeDoor43) -> Vec<String> {
    fake.requests()
        .into_iter()
        .filter(|r| r.path.starts_with("/api/"))
        .map(|r| match r.query {
            Some(query) => format!("{} {}?{query}", r.method, r.path),
            None => format!("{} {}", r.method, r.path),
        })
        .collect()
}

#[test]
fn recommend_marks_the_one_organization_with_the_most_repositories_in_the_language() {
    assert_eq!(recommend(&[(true, 2), (true, 7), (true, 3)]), Some(1));
}

#[test]
fn recommend_marks_none_on_a_tie_for_the_highest_count() {
    assert_eq!(recommend(&[(true, 7), (true, 7), (true, 3)]), None);
}

#[test]
fn recommend_marks_none_when_no_organization_has_a_repository_in_the_language() {
    assert_eq!(recommend(&[(true, 0), (true, 0)]), None);
    assert_eq!(recommend(&[]), None);
}

#[test]
fn recommend_never_marks_an_organization_the_person_cannot_create_in() {
    assert_eq!(recommend(&[(false, 9), (true, 2)]), Some(1));
    assert_eq!(recommend(&[(false, 9)]), None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_share_targets_are_the_organizations_with_the_recommended_one() {
    let fake = fake().await;
    let dir = tempfile::tempdir().unwrap();
    let t = app_on(dir.path(), fake.url()).await;
    internet(&t.app, true, 1).await;
    sign_in(&t.app).await;
    fake.clear_requests();
    let targets = post(&t.app, &orgs("es-419")).await.unwrap();
    assert_eq!(
        targets,
        ShareTargets {
            user: PARITY_USER.to_owned(),
            orgs: vec![
                OrgChoice {
                    name: "Equipo-Ejemplo".to_owned(),
                    full_name: "Equipo Ejemplo".to_owned(),
                    can_create: true,
                    recommended: true,
                },
                OrgChoice {
                    name: "es-419_gl".to_owned(),
                    full_name: "Latin American Spanish Gateway".to_owned(),
                    can_create: false,
                    recommended: false,
                },
            ],
        }
    );
    let searches: Vec<String> = api_requests(&fake)
        .into_iter()
        .filter(|r| r.contains("/repos/search"))
        .collect();
    assert_eq!(
        searches,
        [
            "GET /api/v1/repos/search?owner=Equipo-Ejemplo&lang=es-419&limit=1",
            "GET /api/v1/repos/search?owner=es-419_gl&lang=es-419&limit=1",
        ]
    );
    // Every call took the account server and the token, in a header (the fake refuses one in
    // a URL).
    assert!(
        fake.requests()
            .iter()
            .all(|r| r.authorization == tc_door43_fake::AuthSeen::Token),
        "{:?}",
        fake.requests()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_language_with_no_repositories_recommends_none_and_an_unusable_tag_asks_no_count() {
    let fake = fake().await;
    let dir = tempfile::tempdir().unwrap();
    let t = app_on(dir.path(), fake.url()).await;
    internet(&t.app, true, 1).await;
    sign_in(&t.app).await;
    let targets = post(&t.app, &orgs("fr")).await.unwrap();
    assert!(targets.orgs.iter().all(|o| !o.recommended));
    fake.clear_requests();
    let targets = post(&t.app, &orgs("es 419&lang=en")).await.unwrap();
    assert!(targets.orgs.iter().all(|o| !o.recommended));
    assert!(
        api_requests(&fake)
            .iter()
            .all(|r| !r.contains("/repos/search")),
        "{:?}",
        api_requests(&fake)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn share_targets_need_the_internet_and_a_sign_in_before_door43_is_asked() {
    let fake = fake().await;
    let dir = tempfile::tempdir().unwrap();
    let t = app_on(dir.path(), fake.url()).await;
    assert_eq!(
        post(&t.app, &orgs("es-419")).await,
        Err(ApiError::NeedsInternet(NetKind::Share))
    );
    internet(&t.app, true, 1).await;
    assert_eq!(
        post(&t.app, &orgs("es-419")).await,
        Err(ApiError::NeedsSignIn(SignInReason::NotSignedIn))
    );
    assert_eq!(api_requests(&fake), Vec::<String>::new());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_token_door43_refuses_is_forgotten_and_the_sign_in_step_comes_next() {
    let fake = fake().await;
    let dir = tempfile::tempdir().unwrap();
    let t = app_on(dir.path(), fake.url()).await;
    internet(&t.app, true, 1).await;
    sign_in(&t.app).await;
    fake.inject(FaultRule::once(
        "GET",
        "/api/v1/user/orgs",
        Fault::Status {
            status: 401,
            message: "invalid username, password or token".to_owned(),
        },
    ));
    assert_eq!(
        post(&t.app, &orgs("es-419")).await,
        Err(ApiError::NeedsSignIn(SignInReason::TokenRejected))
    );
    assert_eq!(
        post(&t.app, &orgs("es-419")).await,
        Err(ApiError::NeedsSignIn(SignInReason::NotSignedIn)),
        "the refused token is gone"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn door43_that_does_not_answer_is_door43_cannot_be_reached() {
    let fake = fake().await;
    let dir = tempfile::tempdir().unwrap();
    let t = app_on(dir.path(), fake.url()).await;
    internet(&t.app, true, 1).await;
    sign_in(&t.app).await;
    fake.inject(FaultRule::once(
        "GET",
        "/api/v1/repos/search",
        Fault::Status {
            status: 503,
            message: "down".to_owned(),
        },
    ));
    assert_eq!(
        post(&t.app, &orgs("es-419")).await,
        Err(ApiError::Refused(Refusal::Share(ShareRefusal::Offline)))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_repository_link_is_given_only_for_the_apps_door43_server() {
    let fake = fake().await;
    let dir = tempfile::tempdir().unwrap();
    let t = app_on(dir.path(), fake.url()).await;
    let ours = format!(
        "{}/{PARITY_USER}/sample_burrito",
        fake.url().trim_end_matches('/')
    );
    let link = RepoLink {
        url: WebUrl::parse(&ours).unwrap(),
    };
    assert_eq!(
        post(&t.app, &link).await,
        Err(ApiError::NeedsInternet(NetKind::RepoLink))
    );
    internet(&t.app, true, 1).await;
    assert_eq!(post(&t.app, &link).await.unwrap().url.as_str(), ours);
    for other in [
        "https://example.org/facilitator-zq/sample_burrito".to_owned(),
        format!("{}0/x/y", fake.url().trim_end_matches('/')),
        fake.url().to_owned(),
    ] {
        let refused = post(
            &t.app,
            &RepoLink {
                url: WebUrl::parse(&other).unwrap(),
            },
        )
        .await;
        assert!(
            matches!(refused, Err(ApiError::BadRequest { .. })),
            "{other}: {refused:?}"
        );
    }
    assert_eq!(
        api_requests(&fake),
        Vec::<String>::new(),
        "a link asks Door43 nothing"
    );
}

fn open(id: u8) -> OpenDoor43 {
    OpenDoor43 {
        request: rid(id),
        repo: RepoSource::new(PARITY_USER, "es-419_tit_equipo").unwrap(),
    }
}

async fn ended(app: &App, job: tc_api::ids::JobId) -> JobState {
    let mut state = None;
    until("the open ends", || {
        state = app.jobs().view(job).map(|v| v.state);
        matches!(
            state,
            Some(JobState::Done(_) | JobState::Failed(_) | JobState::Cancelled)
        )
    })
    .await;
    state.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_open_asks_the_internet_question_and_until_unit_x_is_not_available() {
    let dir = tempfile::tempdir().unwrap();
    let t = app_on(dir.path(), "http://127.0.0.1:9").await;
    assert_eq!(
        post(&t.app, &open(2)).await,
        Err(ApiError::NeedsInternet(NetKind::Open))
    );
    internet(&t.app, true, 1).await;
    let job = post(&t.app, &open(3)).await.unwrap();
    assert_eq!(
        ended(&t.app, job).await,
        JobState::Failed(ApiError::Unavailable(Feature::Sync))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scripted_open_ends_with_its_project_or_its_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let t = app_on(dir.path(), "http://127.0.0.1:9").await;
    internet(&t.app, true, 1).await;
    let titus = ProjectRef::parse("titus").unwrap();
    t.app.script_open_door43(Ok(titus.clone()));
    t.app
        .script_open_door43(Err(ApiError::Refused(Refusal::OpenDoor43(
            OpenDoor43Refusal::NoJournal,
        ))));
    let first = post(&t.app, &open(2)).await.unwrap();
    assert_eq!(
        ended(&t.app, first).await,
        JobState::Done(JobOutcome::Created { project: titus })
    );
    let second = post(&t.app, &open(3)).await.unwrap();
    assert_eq!(
        ended(&t.app, second).await,
        JobState::Failed(ApiError::Refused(Refusal::OpenDoor43(
            OpenDoor43Refusal::NoJournal
        )))
    );
}

fn share(id: u8) -> ShareOnDoor43 {
    ShareOnDoor43 {
        request: rid(id),
        project: ProjectRef::parse("KR_ulb").unwrap(),
        owner: ShareOwner::Mine,
        name: RepoName::parse("KR_ulb").unwrap(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scripted_share_ends_with_its_repository_or_its_refusal() {
    let fake = fake().await;
    let dir = tempfile::tempdir().unwrap();
    let t = app_on(dir.path(), fake.url()).await;
    assert_eq!(
        post(&t.app, &share(2)).await,
        Err(ApiError::NeedsInternet(NetKind::Share)),
        "the share asks the internet question"
    );
    internet(&t.app, true, 1).await;
    sign_in(&t.app).await;
    let repo = SharedRepo {
        repo: RepoSource::new(PARITY_USER, "KR_ulb").unwrap(),
        url: WebUrl::parse(&format!("{}/{PARITY_USER}/KR_ulb", fake.url())).unwrap(),
    };
    t.app.script_share(Ok(repo.clone()));
    t.app.script_share(Err(ApiError::Refused(Refusal::Share(
        ShareRefusal::NameExists,
    ))));
    let first = post(&t.app, &share(3)).await.unwrap();
    assert_eq!(
        ended(&t.app, first).await,
        JobState::Done(JobOutcome::Shared(repo))
    );
    let second = post(&t.app, &share(4)).await.unwrap();
    assert_eq!(
        ended(&t.app, second).await,
        JobState::Failed(ApiError::Refused(Refusal::Share(ShareRefusal::NameExists)))
    );
}
