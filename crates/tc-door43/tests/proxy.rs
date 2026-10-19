//! Which proxy a client uses, and when it asks for one. Nothing here reaches a network: the
//! proxy is a local one that refuses every request, and the servers it is asked for are
//! `.test` names that no request ever resolves.

// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(clippy::unwrap_used)]

mod common;

use common::repo;
use std::sync::{Arc, Mutex, PoisonError};
use tc_door43::{Door43Client, Door43Error, InternetSwitch, Limits, ProxyFinder, UnreachableCause};
use tc_door43_fake::{FakeDoor43, FakeProxy};
use url::Url;

/// Names one proxy for every origin and remembers which origins it was asked about.
struct Always {
    proxy: Url,
    asked: Mutex<Vec<String>>,
}

impl Always {
    fn named(proxy: &FakeProxy) -> Arc<Self> {
        Arc::new(Self {
            proxy: proxy.url().parse().unwrap(),
            asked: Mutex::default(),
        })
    }

    fn asked(&self) -> Vec<String> {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl ProxyFinder for Always {
    fn proxy_for(&self, origin: &Url) -> Option<Url> {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(origin.as_str().to_owned());
        Some(self.proxy.clone())
    }
}

fn client(server: &str, switch: &InternetSwitch, finder: &Arc<Always>) -> Door43Client {
    Door43Client::with_proxy_finder(
        server.parse().unwrap(),
        switch.clone(),
        Limits::default(),
        Some(finder.clone()),
    )
    .unwrap()
}

async fn ask(client: &Door43Client) -> Door43Error {
    client
        .repo(&repo("unfoldingWord/en_tn"), None)
        .await
        .unwrap_err()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_request_goes_through_the_proxy_the_finder_names() {
    let proxy = FakeProxy::start().await.unwrap();
    let finder = Always::named(&proxy);
    let switch = InternetSwitch::new();
    switch.turn_on();
    let error = ask(&client("https://door43.test", &switch, &finder)).await;
    // The proxy refused the tunnel, so the client could not connect.
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
    assert_eq!(proxy.requests(), ["CONNECT door43.test:443 HTTP/1.1"]);
    assert_eq!(finder.asked(), ["https://door43.test/"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_proxy_is_asked_for_only_while_the_internet_is_on_and_again_after_it_was_off() {
    let proxy = FakeProxy::start().await.unwrap();
    let finder = Always::named(&proxy);
    let switch = InternetSwitch::new();
    let client = client("https://door43.test", &switch, &finder);
    assert!(finder.asked().is_empty(), "building a client asks nothing");
    assert!(matches!(ask(&client).await, Door43Error::Offline { .. }));
    assert!(finder.asked().is_empty(), "a refused request asks nothing");
    assert!(proxy.requests().is_empty());

    switch.turn_on();
    ask(&client).await;
    ask(&client).await;
    assert_eq!(
        finder.asked().len(),
        1,
        "the answer is kept while it stays on"
    );
    assert_eq!(proxy.requests().len(), 2);

    switch.turn_off();
    assert!(matches!(ask(&client).await, Door43Error::Offline { .. }));
    assert_eq!(finder.asked().len(), 1);
    switch.turn_on();
    ask(&client).await;
    assert_eq!(
        finder.asked().len(),
        2,
        "another network may need another proxy"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_on_this_computer_never_sees_a_proxy() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let proxy = FakeProxy::start().await.unwrap();
    let finder = Always::named(&proxy);
    let switch = InternetSwitch::new();
    switch.turn_on();
    let client = client(fake.url(), &switch, &finder);
    client
        .repo(&repo("unfoldingWord/en_tn"), None)
        .await
        .unwrap();
    assert!(finder.asked().is_empty());
    assert!(proxy.requests().is_empty());
}

/// Names no proxy for any origin, as a finder does when the system has none for it.
struct Nothing;

impl ProxyFinder for Nothing {
    fn proxy_for(&self, _: &Url) -> Option<Url> {
        None
    }
}

/// Set in the copy of this test binary that the next test starts.
const CHILD: &str = "TC_PROXY_TEST_CHILD";

/// The client reads `HTTPS_PROXY` when it is built, and a test cannot set a variable in its
/// own process without `unsafe`, so [`the_system_proxy_serves_an_origin_the_finder_names_none_for`]
/// runs this one in a copy of the binary that has the variable. Run alone it does nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_request_of_the_system_proxy_test() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    let switch = InternetSwitch::new();
    switch.turn_on();
    let client = Door43Client::with_proxy_finder(
        "https://door43.test".parse().unwrap(),
        switch,
        Limits::default(),
        Some(Arc::new(Nothing)),
    )
    .unwrap();
    let error = ask(&client).await;
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
async fn the_system_proxy_serves_an_origin_the_finder_names_none_for() {
    let proxy = FakeProxy::start().await.unwrap();
    let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "the_request_of_the_system_proxy_test",
            "--nocapture",
        ])
        .env(CHILD, "1");
    // Removed before `HTTPS_PROXY` is set: Windows treats variable names without regard to
    // case, so `https_proxy` would remove it again.
    for other in [
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "NO_PROXY",
        "no_proxy",
        "REQUEST_METHOD",
    ] {
        command.env_remove(other);
    }
    let output = command
        .env("HTTPS_PROXY", proxy.url())
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        proxy.requests(),
        ["CONNECT door43.test:443 HTTP/1.1"],
        "the request did not go through the proxy that HTTPS_PROXY names"
    );
}
