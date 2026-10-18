//! The test crash points against the real binary (AD35, APP-DESIGN §6.6): `tc4 serve
//! --test-hooks` ends itself at a named point with exit code 86.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::{Read as _, Write as _};
use std::time::Duration;

use common::{START, Tc4, http};
use tempfile::TempDir;

const REQUEST: &str = "00000000000000000000000000000009";
const REPORT: &str = r#"{"request":"00000000000000000000000000000002","view":"Check","project":null,"message":"unreachable executed","at":1}"#;
const CRASH_EXIT: i32 = 86;

/// A `tc4 serve` with the given extra flags, its address read.
fn start(extra: &[&str]) -> (Tc4, TempDir, TempDir) {
    let ui_root = TempDir::new().unwrap();
    let ui = http::seed_ui(ui_root.path());
    let app_data = TempDir::new().unwrap();
    let mut tc4 = Tc4::spawn(app_data.path(), &ui, extra);
    tc4.address(START);
    (tc4, app_data, ui_root)
}

fn arm(tc4: &Tc4, at: &str, path: &str) {
    let body = format!(r#"{{"request":"{REQUEST}","at":"{at}","path":"{path}"}}"#);
    let reply = tc4.api("test/crash", &body);
    assert_eq!(reply.status, 200, "{}", reply.text());
    assert_eq!(reply.json()["type"], "ok", "{}", reply.text());
}

/// What the server sends back for a request, if anything: the bytes until it closes.
fn raw_answer(tc4: &Tc4, request: &common::Req) -> Vec<u8> {
    let mut stream = tc4.connect();
    stream.write_all(&request.to_bytes()).unwrap();
    let mut answer = Vec::new();
    // A reset instead of a clean close is how a killed process can look; both mean "no reply".
    let _ = stream.read_to_end(&mut answer);
    answer
}

#[test]
fn test_crash_before_reply_ends_tc4_without_a_reply() {
    let (mut tc4, _app_data, _ui) = start(&["--test-hooks"]);
    arm(&tc4, "beforeReply", "app/clientError");
    let answer = raw_answer(&tc4, &tc4.api_req("app/clientError", REPORT));
    assert!(
        answer.is_empty(),
        "got a reply: {}",
        String::from_utf8_lossy(&answer)
    );
    let status = tc4.wait_exit(Duration::from_secs(10)).expect("tc4 ended");
    assert_eq!(status.code(), Some(CRASH_EXIT));
    // Before the reply, after the report was logged.
    assert!(
        tc4.stderr().contains("the UI reported an error"),
        "stderr: {}",
        tc4.stderr()
    );
}

#[test]
fn test_crash_before_store_ends_tc4_before_the_report_is_logged() {
    let (mut tc4, _app_data, _ui) = start(&["--test-hooks"]);
    arm(&tc4, "beforeStore", "app/clientError");
    let answer = raw_answer(&tc4, &tc4.api_req("app/clientError", REPORT));
    assert!(answer.is_empty());
    let status = tc4.wait_exit(Duration::from_secs(10)).expect("tc4 ended");
    assert_eq!(status.code(), Some(CRASH_EXIT));
    assert!(
        !tc4.stderr().contains("the UI reported an error"),
        "the write happened before the crash: {}",
        tc4.stderr()
    );
}

#[test]
fn test_crash_after_reply_sends_the_reply_then_ends_tc4() {
    let (mut tc4, _app_data, _ui) = start(&["--test-hooks"]);
    arm(&tc4, "afterReply", "app/clientError");
    let reply = tc4.api("app/clientError", REPORT);
    assert_eq!(reply.status, 200);
    assert_eq!(
        reply.json(),
        serde_json::json!({"type": "ok", "value": null})
    );
    let status = tc4.wait_exit(Duration::from_secs(10)).expect("tc4 ended");
    assert_eq!(status.code(), Some(CRASH_EXIT));
}

#[test]
fn test_crash_waits_for_its_path() {
    let (mut tc4, _app_data, _ui) = start(&["--test-hooks"]);
    arm(&tc4, "beforeStore", "net/set");
    let reply = tc4.api("app/clientError", REPORT);
    assert_eq!(
        reply.json(),
        serde_json::json!({"type": "ok", "value": null})
    );
    std::thread::sleep(Duration::from_millis(300));
    assert!(tc4.try_exit().is_none(), "it ended for another path");
    assert_eq!(tc4.api("test/quiet", "{}").status, 200);
    assert_eq!(tc4.quit().code(), Some(0));
}

#[test]
fn test_hooks_are_404_without_the_flag() {
    let (mut tc4, _app_data, _ui) = start(&[]);
    for path in ["test/quiet", "test/crash", "test/echo"] {
        assert_eq!(tc4.api(path, "{}").status, 404, "{path}");
    }
    assert_eq!(tc4.api("app/hello", "{}").status, 200);
    assert_eq!(tc4.quit().code(), Some(0));
}

#[test]
fn test_echo_answers_a_reply_of_the_size_asked() {
    let (mut tc4, _app_data, _ui) = start(&["--test-hooks"]);
    let reply = tc4.api("test/echo", r#"{"size":20480}"#);
    assert_eq!(reply.status, 200);
    assert!(reply.body.len() >= 20480, "{}", reply.body.len());
    assert!(reply.body.len() < 21500, "{}", reply.body.len());
    let json = reply.json();
    assert_eq!(json["type"], "ok");
    assert!(!json["value"]["words"].as_array().unwrap().is_empty());
    let too_big = tc4.api("test/echo", r#"{"size":4194305}"#);
    assert_eq!(too_big.json()["value"]["type"], "badRequest");
    assert_eq!(tc4.quit().code(), Some(0));
}
