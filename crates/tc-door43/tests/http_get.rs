//! `Http::get`, the one GET that every network client outside this crate sends with.

// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(clippy::unwrap_used)]

use std::time::Duration;
use tc_door43::{Http, HttpError, HttpSpec, InternetSwitch, RedirectRule};
use tc_door43_fake::{FakeDoor43, Fault, FaultRule};
use url::Url;

const REPO: &str = "/api/v1/repos/unfoldingWord/en_tn";

fn http(fake: &FakeDoor43, switch: &InternetSwitch) -> Http {
    let origin: Url = fake.url().parse().unwrap();
    let spec = HttpSpec {
        user_agent: "test",
        connect_timeout: Duration::from_secs(5),
        read_timeout: Duration::from_secs(5),
        redirects: RedirectRule {
            origins: vec![origin.clone()],
            https_hosts: Vec::new(),
            max: 5,
        },
        origins: vec![origin],
        loopback: true,
    };
    Http::with_finder(spec, switch.clone(), None).unwrap()
}

fn url(fake: &FakeDoor43, path: &str) -> Url {
    format!("{}{path}", fake.url()).parse().unwrap()
}

fn on() -> InternetSwitch {
    let switch = InternetSwitch::new();
    switch.turn_on();
    switch
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_get_while_the_switch_is_off_sends_nothing() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let http = http(&fake, &InternetSwitch::new());
    let error = http
        .get(url(&fake, REPO), "application/json", &[])
        .await
        .unwrap_err();
    assert!(matches!(error, HttpError::Offline), "{error:?}");
    assert!(fake.requests().is_empty(), "{:?}", fake.requests());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_redirect_the_rule_does_not_allow_is_refused() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let elsewhere = FakeDoor43::start_seeded().await.unwrap();
    fake.inject(FaultRule::once(
        "GET",
        REPO,
        Fault::RedirectTo(format!("{}{REPO}", elsewhere.url())),
    ));
    let http = http(&fake, &on());
    let error = http
        .get(url(&fake, REPO), "application/json", &[])
        .await
        .unwrap_err();
    assert!(matches!(error, HttpError::Redirect), "{error:?}");
    assert!(
        elsewhere.requests().is_empty(),
        "the other server heard nothing"
    );

    // A redirect within the allowed origin is followed.
    fake.inject(FaultRule::once(
        "GET",
        "/api/v1/repos/x/y",
        Fault::RedirectTo(format!("{}{REPO}", fake.url())),
    ));
    let response = http
        .get(url(&fake, "/api/v1/repos/x/y"), "application/json", &[])
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_body_over_its_limit_is_refused() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let http = http(&fake, &on());
    let whole = http
        .get(url(&fake, REPO), "application/json", &[])
        .await
        .unwrap()
        .body(1 << 20)
        .await
        .unwrap();
    assert!(whole.len() > 10, "{}", whole.len());
    let error = http
        .get(url(&fake, REPO), "application/json", &[])
        .await
        .unwrap()
        .body(10)
        .await
        .unwrap_err();
    assert!(
        matches!(error, HttpError::TooLarge { limit: 10 }),
        "{error:?}"
    );
}
