//! Each kind of failure maps to its own error.

// `repo_error` sits outside `#[test]` functions, where clippy.toml already allows unwrap.
#![allow(clippy::unwrap_used)]

mod common;

use common::{client, client_with, repo};
use std::time::Duration;
use tc_door43::{Door43Client, Door43Error, InternetSwitch, Limits, UnreachableCause};
use tc_door43_fake::{FakeDoor43, Fault, FaultRule};

fn fail_once(fake: &FakeDoor43, fault: Fault) {
    fake.inject(FaultRule::once(
        "GET",
        "/api/v1/repos/unfoldingWord/en_tn",
        fault,
    ));
}

async fn repo_error(fake: &FakeDoor43) -> Door43Error {
    client(fake)
        .repo(&repo("unfoldingWord/en_tn"), None)
        .await
        .unwrap_err()
}

/// A connection that cannot be made is `Connect`, not `Timeout`, on every platform. The
/// address is loopback port 0, which no program can listen on: the system refuses at once
/// (Linux) or calls the address invalid (Windows), and sends nothing. A port that was bound
/// and closed is refused too, but Windows answers that after about two seconds and
/// sometimes not before the connect timeout, so a test on it fails at random there
/// (`docs/evidence/windows-build-2026-10-17.md`); the client reports what the system says.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_connection_that_cannot_be_made_is_unreachable() {
    let switch = InternetSwitch::new();
    switch.turn_on();
    let limits = Limits {
        connect_timeout: Duration::from_secs(5),
        ..Limits::default()
    };
    let client =
        Door43Client::with_limits("http://127.0.0.1:0".parse().unwrap(), switch, limits).unwrap();
    let error = client
        .repo(&repo("unfoldingWord/en_tn"), None)
        .await
        .unwrap_err();
    assert!(
        matches!(
            &error,
            Door43Error::Unreachable {
                cause: UnreachableCause::Connect,
                ..
            }
        ),
        "{error:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_that_stops_answering_times_out() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    fail_once(&fake, Fault::Delay(Duration::from_secs(5)));
    let impatient = client_with(
        &fake,
        Limits {
            read_timeout: Duration::from_millis(300),
            ..Limits::default()
        },
    );
    let error = impatient
        .repo(&repo("unfoldingWord/en_tn"), None)
        .await
        .unwrap_err();
    assert!(
        matches!(
            &error,
            Door43Error::Unreachable {
                cause: UnreachableCause::Timeout,
                ..
            }
        ),
        "{error:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn statuses_map_to_their_kinds() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let status = |status: u16| Fault::Status {
        status,
        message: format!("status {status}"),
    };

    fail_once(&fake, status(500));
    assert!(matches!(
        repo_error(&fake).await,
        Door43Error::Server { status: 500, message, .. } if message == "status 500"
    ));
    fail_once(&fake, status(502));
    assert!(matches!(
        repo_error(&fake).await,
        Door43Error::Server { status: 502, .. }
    ));
    fail_once(&fake, status(422));
    assert!(matches!(
        repo_error(&fake).await,
        Door43Error::Rejected { status: 422, .. }
    ));
    fail_once(&fake, status(409));
    assert!(matches!(
        repo_error(&fake).await,
        Door43Error::NameExists { .. }
    ));
    fail_once(
        &fake,
        Fault::RateLimited {
            retry_after_secs: 7,
        },
    );
    assert!(matches!(
        repo_error(&fake).await,
        Door43Error::RateLimited { retry_after: Some(d), .. } if d == Duration::from_secs(7)
    ));
    let missing = client(&fake)
        .repo(&repo("unfoldingWord/nothing"), None)
        .await
        .unwrap_err();
    assert!(matches!(
        missing,
        Door43Error::NotFound { route, .. } if route == "GET /api/v1/repos/unfoldingWord/nothing"
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_success_answer_of_the_wrong_shape_is_unexpected() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    fail_once(
        &fake,
        Fault::Status {
            status: 200,
            message: "not a repository".into(),
        },
    );
    assert!(matches!(
        repo_error(&fake).await,
        Door43Error::UnexpectedResponse { .. }
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_redirect_to_another_server_is_not_followed() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let elsewhere = FakeDoor43::start_seeded().await.unwrap();
    fail_once(
        &fake,
        Fault::RedirectTo(format!(
            "{}/api/v1/repos/unfoldingWord/en_tn",
            elsewhere.url()
        )),
    );
    let error = repo_error(&fake).await;
    assert!(
        matches!(error, Door43Error::UnexpectedResponse { .. }),
        "{error:?}"
    );
    assert_eq!(elsewhere.requests(), []);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_redirect_within_the_server_is_followed() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    fake.inject(FaultRule::once(
        "GET",
        "/api/v1/repos/unfoldingWord/en_ult",
        Fault::RedirectTo(format!("{}/api/v1/repos/unfoldingWord/en_tn", fake.url())),
    ));
    let info = client(&fake)
        .repo(&repo("unfoldingWord/en_ult"), None)
        .await
        .unwrap();
    assert_eq!(info.repo, repo("unfoldingWord/en_tn"));
}

fn rate_limited_until(fake: &FakeDoor43, date: &str) {
    fail_once(
        fake,
        Fault::Raw {
            status: 429,
            headers: vec![("Retry-After".into(), date.into())],
            body: br#"{"message":"rate limit exceeded"}"#.to_vec(),
        },
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retry_after_date_becomes_a_wait_from_now() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    rate_limited_until(&fake, "Wed, 21 Oct 2099 07:28:00 GMT");
    let error = repo_error(&fake).await;
    assert!(
        matches!(
            &error,
            Door43Error::RateLimited { retry_after: Some(wait), .. }
                if *wait > Duration::from_secs(365 * 24 * 3600)
        ),
        "{error:?}"
    );
    rate_limited_until(&fake, "Sun, 06 Nov 1994 08:49:37 GMT");
    let error = repo_error(&fake).await;
    assert!(
        matches!(
            &error,
            Door43Error::RateLimited { retry_after: Some(wait), .. } if wait.is_zero()
        ),
        "{error:?}"
    );
}
