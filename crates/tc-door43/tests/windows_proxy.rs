//! The Windows finder against a PAC script and a proxy on this computer. WinHTTP's auto-proxy
//! service fetches the script and runs it; the client then opens its tunnel through the proxy
//! the script names. Nothing reaches a network: the script is served on 127.0.0.1, it calls no
//! DNS function, the proxy refuses every request, and the servers are `.test` names that are
//! never resolved. The user's own Internet Options are not read, and every script has a URL of
//! its own, because WinHTTP keeps a script's answers for its URL.
#![cfg(windows)]
// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(clippy::unwrap_used)]

mod common;

use common::repo;
use std::sync::Arc;
use tc_door43::{
    Door43Client, Door43Error, InternetSwitch, Limits, ProxyAnswer, ProxyFinder, UnreachableCause,
    WinHttpFinder,
};
use tc_door43_fake::{FakePacScript, FakeProxy};
use url::Url;

fn origin() -> Url {
    "https://door43.test/".parse().unwrap()
}

/// A finder for the script `body` serves, which it reads from a server on this computer.
async fn finder(body: &str) -> (WinHttpFinder, FakePacScript) {
    let script = FakePacScript::start(body).await.unwrap();
    (WinHttpFinder::with_pac_script(script.url().parse().unwrap()), script)
}

/// What the finder answers, asked as the clients ask it: off the async runtime's threads.
async fn answer(finder: WinHttpFinder) -> Option<ProxyAnswer> {
    tokio::task::spawn_blocking(move || finder.proxy_for(&origin()))
        .await
        .unwrap()
}

fn pac(result: &str) -> String {
    format!("function FindProxyForURL(url, host) {{ return \"{result}\"; }}")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_request_goes_through_the_proxy_the_pac_script_names() {
    let proxy = FakeProxy::start().await.unwrap();
    let address = proxy.url().trim_start_matches("http://").trim_end_matches('/').to_owned();
    let (finder, script) = finder(&pac(&format!("PROXY {address}"))).await;
    let switch = InternetSwitch::new();
    switch.turn_on();
    let client = Door43Client::with_proxy_finder(
        origin(),
        switch,
        Limits::default(),
        Some(Arc::new(finder)),
    )
    .unwrap();
    let error = client
        .repo(&repo("unfoldingWord/en_tn"), None)
        .await
        .unwrap_err();
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
    assert!(script.fetched() >= 1, "WinHTTP fetched the script");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn direct_from_the_script_is_an_answer() {
    let (finder, _script) = finder(&pac("DIRECT")).await;
    assert_eq!(answer(finder).await, Some(ProxyAnswer::Direct));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_first_proxy_of_a_list_is_taken() {
    let (finder, _script) = finder(&pac("PROXY 127.0.0.1:9; PROXY 127.0.0.1:10; DIRECT")).await;
    assert_eq!(
        answer(finder).await,
        Some(ProxyAnswer::Through("http://127.0.0.1:9/".parse().unwrap()))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_socks_proxy_is_an_answer() {
    let (finder, _script) = finder(&pac("SOCKS 127.0.0.1:9")).await;
    assert_eq!(
        answer(finder).await,
        Some(ProxyAnswer::Through("socks4://127.0.0.1:9".parse().unwrap()))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_script_that_cannot_be_fetched_names_nothing() {
    // A port on this computer that nothing listens on: the system's own settings then decide.
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url: Url = format!("http://{}/proxy.pac", closed.local_addr().unwrap())
        .parse()
        .unwrap();
    drop(closed);
    assert_eq!(answer(WinHttpFinder::with_pac_script(url)).await, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_script_that_throws_names_nothing() {
    let (finder, _script) = finder("function FindProxyForURL(url, host) { throw 'no'; }").await;
    assert_eq!(answer(finder).await, None);
}
