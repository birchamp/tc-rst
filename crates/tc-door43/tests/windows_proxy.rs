//! The Windows finder against a PAC script and a proxy on this computer. WinHTTP's auto-proxy
//! service fetches the script and runs it; the client then opens its tunnel through the proxy
//! the script names. Nothing reaches a network: the script is served on 127.0.0.1, it calls no
//! DNS function, the proxy refuses every request, and the servers are `.test` names that are
//! never resolved, or an address. The user's own Internet Options are not read, and every
//! script has a URL of its own, because WinHTTP keeps a script's answers for its URL, for the
//! whole computer. The finder defers to the `*_PROXY` variables, so these tests need them unset.
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
    (
        WinHttpFinder::with_pac_script(script.url().parse().unwrap()),
        script,
    )
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

/// The proxy's `host:port`, as a PAC script names it.
fn host_port(proxy: &FakeProxy) -> String {
    proxy.url().trim_start_matches("http://").to_owned()
}

/// A request to `server` through a client that asks `finder`, and the error it ends in: every
/// proxy here refuses it.
async fn ask(server: &str, finder: WinHttpFinder) -> Door43Error {
    let switch = InternetSwitch::new();
    switch.turn_on();
    let client = Door43Client::with_proxy_finder(
        server.parse().unwrap(),
        switch,
        Limits::default(),
        Some(Arc::new(finder)),
    )
    .unwrap();
    client
        .repo(&repo("unfoldingWord/en_tn"), None)
        .await
        .unwrap_err()
}

fn is_refused(error: &Door43Error) -> bool {
    matches!(
        error,
        Door43Error::Unreachable {
            cause: UnreachableCause::Connect,
            ..
        }
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_request_goes_through_the_proxy_the_pac_script_names() {
    let proxy = FakeProxy::start().await.unwrap();
    let (finder, script) = finder(&pac(&format!("PROXY {}", host_port(&proxy)))).await;
    let error = ask("https://door43.test", finder).await;
    // The proxy refused the tunnel, so the client could not connect.
    assert!(is_refused(&error), "{error:?}");
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
async fn an_https_proxy_is_an_answer() {
    let (finder, _script) = finder(&pac("HTTPS 127.0.0.1:9")).await;
    assert_eq!(
        answer(finder).await,
        Some(ProxyAnswer::Through(
            "https://127.0.0.1:9/".parse().unwrap()
        ))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_socks_proxy_from_the_script_gets_a_socks4_handshake() {
    let proxy = FakeProxy::start().await.unwrap();
    let (finder, _script) = finder(&pac(&format!("SOCKS {}", host_port(&proxy)))).await;
    // An address, not a name: a SOCKS4 client looks a name up itself, and nothing here may
    // ask DNS. 192.0.2.1 is a documentation address that nothing ever reaches.
    let error = ask("https://192.0.2.1", finder).await;
    assert!(
        matches!(error, Door43Error::Unreachable { .. }),
        "{error:?}"
    );
    assert_eq!(proxy.requests(), ["SOCKS4 CONNECT 192.0.2.1:443"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_script_that_is_missing_names_nothing() {
    let script = FakePacScript::missing().await.unwrap();
    let finder = WinHttpFinder::with_pac_script(script.url().parse().unwrap());
    assert_eq!(answer(finder).await, None);
    assert!(script.fetched() >= 1, "WinHTTP asked for the script");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_script_that_cannot_be_fetched_names_nothing() {
    // A port on this computer that nothing listens on: the system's own settings then decide.
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = closed.local_addr().unwrap();
    drop(closed);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let url: Url = format!("http://{address}/closed-{}-{stamp}.pac", std::process::id())
        .parse()
        .unwrap();
    assert_eq!(answer(WinHttpFinder::with_pac_script(url)).await, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_script_that_throws_names_nothing() {
    let (finder, script) = finder("function FindProxyForURL(url, host) { throw 'no'; }").await;
    assert_eq!(answer(finder).await, None);
    assert!(script.fetched() >= 1, "WinHTTP fetched the script");
}

/// Set in the copy of this test binary that the next test starts, to the script's URL.
const CHILD: &str = "TC_WINDOWS_PROXY_TEST_CHILD";

/// The finder reads the `*_PROXY` variables, and a test cannot set a variable in its own
/// process without `unsafe`, so [`the_proxy_variables_come_before_the_pac_script`] runs this
/// one in a copy of the binary that has them. Run alone it does nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_request_of_the_proxy_variables_test() {
    let Some(script) = std::env::var_os(CHILD) else {
        return;
    };
    let finder = WinHttpFinder::with_pac_script(script.to_str().unwrap().parse().unwrap());
    let error = ask("https://door43.test", finder).await;
    assert!(is_refused(&error), "{error:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_proxy_variables_come_before_the_pac_script() {
    let named = FakeProxy::start().await.unwrap();
    let scripted = FakeProxy::start().await.unwrap();
    let script = FakePacScript::start(&pac(&format!("PROXY {}", host_port(&scripted))))
        .await
        .unwrap();
    let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "the_request_of_the_proxy_variables_test",
            "--nocapture",
        ])
        .env(CHILD, script.url());
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
        .env("HTTPS_PROXY", named.url())
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(named.requests(), ["CONNECT door43.test:443 HTTP/1.1"]);
    assert_eq!(script.fetched(), 0, "the PAC script was not asked");
    assert!(scripted.requests().is_empty());
}
