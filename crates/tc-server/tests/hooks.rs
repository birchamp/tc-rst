//! Ending the process at a named point (AD35, APP-DESIGN §6.6), with a recorder in place of
//! the real exit. `crates/tc4/tests/hooks.rs` drives the same hooks against the binary.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::Write as _;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::TestServer;
use tc_api::test::CrashPoint;
use tc_server::stub::{Stub, StubEvents};
use tc_server::{Answer, Backend, Bytes, Ended, RecordingEnder, Then};

const REQUEST: &str = "00000000000000000000000000000009";
const REPORT: &str = r#"{"request":"00000000000000000000000000000002","view":"Check","project":null,"message":"unreachable executed","at":1}"#;

/// A server whose stand-in backend and whose own crash after a reply both use `ender`.
fn start(ender: &Arc<RecordingEnder>) -> TestServer {
    let backend = Arc::new(Stub::new().unwrap().with_ender(ender.clone()));
    let ender = Arc::clone(ender);
    TestServer::start_with(backend, move |config| config.ender = ender, true)
}

fn arm(server: &TestServer, at: &str, path: Option<&str>) -> serde_json::Value {
    let path = path.map_or("null".to_owned(), |path| format!("\"{path}\""));
    let body = format!(r#"{{"request":"{REQUEST}","at":"{at}","path":{path}}}"#);
    let response = server.api("test/crash", &body);
    assert_eq!(response.status, 200, "{}", response.text());
    response.json()
}

fn ok_null() -> serde_json::Value {
    serde_json::json!({"type": "ok", "value": null})
}

/// Gives the recorder a moment: the end after a reply is asked for from the connection's
/// task, a hair after the client has the body.
fn settle() {
    std::thread::sleep(Duration::from_millis(150));
}

/// Answers every call with a body of `size` bytes and a request to end the process after it.
struct EndsAfter {
    size: usize,
}

impl Backend for EndsAfter {
    type Events = StubEvents;

    async fn call(&self, _path: &'static str, _body: Bytes) -> Answer {
        Answer::Reply {
            body: vec![b'x'; self.size],
            then: Then::EndProcess,
        }
    }
    fn subscribe(&self, _after: u64) -> StubEvents {
        StubEvents
    }
    async fn quit(&self) {}
    async fn quitting(&self) {
        std::future::pending::<()>().await;
    }
}

#[test]
fn after_reply_crash_ends_only_after_the_body() {
    let ender = Arc::new(RecordingEnder::default());
    let configured = Arc::clone(&ender);
    let server = TestServer::start_with(
        Arc::new(EndsAfter { size: 20_000 }),
        move |config| config.ender = configured,
        true,
    );
    let response = server.api("app/hello", "{}");
    let read_at = Instant::now();
    assert_eq!(response.status, 200);
    assert_eq!(response.body.len(), 20_000, "the whole body arrived");
    settle();
    let calls = ender.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0].0, Ended::AfterReply);
    assert!(
        calls[0].1 + Duration::from_millis(5) >= read_at,
        "the process was asked to end {:?} before the client had the body",
        read_at.duration_since(calls[0].1)
    );
}

#[test]
fn after_reply_crash_waits_for_a_slow_reader() {
    // A body far larger than the socket buffers cannot be handed over until the client reads:
    // the end must wait for it, not for the reply to be built.
    let ender = Arc::new(RecordingEnder::default());
    let configured = Arc::clone(&ender);
    let server = TestServer::start_with(
        Arc::new(EndsAfter {
            size: 48 * 1024 * 1024,
        }),
        move |config| config.ender = configured,
        true,
    );
    let mut stream = server.connect();
    stream
        .write_all(&server.api_req("app/hello", "{}").to_bytes())
        .unwrap();
    std::thread::sleep(Duration::from_millis(400));
    assert!(
        ender.calls().is_empty(),
        "the process was asked to end before the client read the body"
    );
    let response = common::read_response(&mut stream);
    assert_eq!(response.body.len(), 48 * 1024 * 1024);
    settle();
    assert_eq!(ender.calls().len(), 1);
}

#[test]
fn crash_before_store_ends_before_the_report_is_logged() {
    let ender = Arc::new(RecordingEnder::default());
    let server = start(&ender);
    assert_eq!(
        arm(&server, "beforeStore", Some("app/clientError")),
        ok_null()
    );
    let response = server.api("app/clientError", REPORT);
    assert_eq!(response.status, 503);
    assert_eq!(response.header("connection"), Some("close"));
    let calls = ender.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0].0, Ended::Now(CrashPoint::BeforeStore));
}

#[test]
fn crash_before_reply_ends_after_the_report_is_logged() {
    let ender = Arc::new(RecordingEnder::default());
    let server = start(&ender);
    assert_eq!(
        arm(&server, "beforeReply", Some("app/clientError")),
        ok_null()
    );
    let response = server.api("app/clientError", REPORT);
    assert_eq!(response.status, 503);
    assert_eq!(response.header("connection"), Some("close"));
    assert_eq!(response.body.len(), 0, "no reply body");
    let calls = ender.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0].0, Ended::Now(CrashPoint::BeforeReply));
}

#[test]
fn crash_after_reply_answers_then_ends() {
    let ender = Arc::new(RecordingEnder::default());
    let server = start(&ender);
    assert_eq!(
        arm(&server, "afterReply", Some("app/clientError")),
        ok_null()
    );
    let response = server.api("app/clientError", REPORT);
    assert_eq!(response.status, 200);
    assert_eq!(response.json(), ok_null(), "the reply is whole");
    settle();
    let calls = ender.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0].0, Ended::AfterReply);
}

#[test]
fn an_armed_crash_fires_once_and_waits_for_its_path() {
    let ender = Arc::new(RecordingEnder::default());
    let server = start(&ender);
    arm(&server, "beforeStore", Some("net/set"));
    assert_eq!(
        server.api_ok("app/clientError", REPORT),
        serde_json::Value::Null
    );
    assert!(ender.calls().is_empty(), "armed for another path");
    // Arming again replaces the armed crash; no path means any write.
    arm(&server, "beforeStore", None);
    assert_eq!(server.api("app/clientError", REPORT).status, 503);
    assert_eq!(ender.calls().len(), 1);
    assert_eq!(
        server.api_ok("app/clientError", REPORT),
        serde_json::Value::Null
    );
    assert_eq!(ender.calls().len(), 1, "it fired once");
}

#[test]
fn a_draft_write_crash_is_accepted_and_never_reached() {
    let ender = Arc::new(RecordingEnder::default());
    let server = start(&ender);
    assert_eq!(arm(&server, "duringDraftWrite", None), ok_null());
    assert_eq!(
        server.api_ok("app/clientError", REPORT),
        serde_json::Value::Null
    );
    assert!(ender.calls().is_empty());
}

#[test]
fn a_crash_for_a_query_or_an_unknown_path_is_refused() {
    let ender = Arc::new(RecordingEnder::default());
    let server = start(&ender);
    for path in ["app/hello", "nope/x", ""] {
        let reply = arm(&server, "beforeStore", Some(path));
        assert_eq!(reply["type"], "err", "{path}: {reply}");
        assert_eq!(reply["value"]["type"], "badRequest", "{path}: {reply}");
        assert_eq!(reply["value"]["value"]["field"], "path", "{path}: {reply}");
    }
    // Nothing was armed by the refused ones.
    assert_eq!(
        server.api_ok("app/clientError", REPORT),
        serde_json::Value::Null
    );
    assert!(ender.calls().is_empty());
}

#[test]
fn the_arming_hooks_never_fire_a_crash() {
    let ender = Arc::new(RecordingEnder::default());
    let server = start(&ender);
    arm(&server, "beforeStore", None);
    arm(&server, "beforeReply", None);
    assert_eq!(server.api_ok("test/quiet", "{}")["busy"], false);
    settle();
    assert!(ender.calls().is_empty());
    // And it is still armed for the write.
    assert_eq!(server.api("app/clientError", REPORT).status, 503);
    assert_eq!(ender.calls().len(), 1);
    assert_eq!(ender.calls()[0].0, Ended::Now(CrashPoint::BeforeReply));
}
