//! The test crash points against the real binary (AD35, APP-DESIGN §6.6): `tc4 serve
//! --test-hooks` ends itself at a named point with exit code 86.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::{Read as _, Write as _};
use std::time::Duration;

use common::{START, Tc4, http};
use tempfile::TempDir;

const ARM: &str = "00000000000000000000000000000009";
const STORE: &str = "00000000000000000000000000000002";
const CRASH_EXIT: i32 = 86;
const FIELD: &str = "verse-1";
const TEXT: &str = "Paul, a servant of God";

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
    let body = format!(r#"{{"request":"{ARM}","at":"{at}","path":"{path}"}}"#);
    let reply = tc4.api("test/crash", &body);
    assert_eq!(reply.status, 200, "{}", reply.text());
    assert_eq!(reply.json()["type"], "ok", "{}", reply.text());
}

/// The request that stores `TEXT` in the field: the app's one phase-0 write that keeps what it
/// is given on disk, so a crash leaves a mark that shows whether the write ran.
fn store_request(tc4: &Tc4) -> common::Req {
    let log = tc4.api("test/fieldLog", &format!(r#"{{"field":"{FIELD}"}}"#));
    assert_eq!(log.json()["type"], "ok", "{}", log.text());
    let token = log.json()["value"]["current"]["token"].clone();
    let body = serde_json::json!({
        "request": STORE, "field": FIELD, "shown": token, "text": TEXT,
    });
    tc4.api_req("test/fieldStore", &body.to_string())
}

/// The texts stored into the field, read from the file the app keeps for tests. A crash ended
/// the process, so only what reached the disk shows.
fn stored(app_data: &TempDir) -> Vec<String> {
    let file = app_data.path().join("test-hooks").join("fields.json");
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    json["fields"][FIELD]
        .as_array()
        .map(|stores| {
            stores
                .iter()
                .map(|store| store["text"].as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default()
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
fn test_crash_before_reply_ends_tc4_without_a_reply_after_the_text_was_stored() {
    let (mut tc4, app_data, _ui) = start(&["--test-hooks"]);
    arm(&tc4, "beforeReply", "test/fieldStore");
    let answer = raw_answer(&tc4, &store_request(&tc4));
    assert!(
        answer.is_empty(),
        "got a reply: {}",
        String::from_utf8_lossy(&answer)
    );
    let status = tc4.wait_exit(Duration::from_secs(10)).expect("tc4 ended");
    assert_eq!(status.code(), Some(CRASH_EXIT));
    assert_eq!(stored(&app_data), [TEXT], "the write ran before the crash");
}

#[test]
fn test_crash_before_store_ends_tc4_before_the_text_is_stored() {
    let (mut tc4, app_data, _ui) = start(&["--test-hooks"]);
    arm(&tc4, "beforeStore", "test/fieldStore");
    let answer = raw_answer(&tc4, &store_request(&tc4));
    assert!(answer.is_empty());
    let status = tc4.wait_exit(Duration::from_secs(10)).expect("tc4 ended");
    assert_eq!(status.code(), Some(CRASH_EXIT));
    assert_eq!(
        stored(&app_data),
        Vec::<String>::new(),
        "the write happened before the crash"
    );
}

#[test]
fn test_crash_after_reply_sends_the_reply_then_ends_tc4() {
    let (mut tc4, app_data, _ui) = start(&["--test-hooks"]);
    arm(&tc4, "afterReply", "test/fieldStore");
    let reply = tc4.send(&store_request(&tc4));
    assert_eq!(reply.status, 200);
    assert_eq!(reply.json()["type"], "ok", "{}", reply.text());
    let status = tc4.wait_exit(Duration::from_secs(10)).expect("tc4 ended");
    assert_eq!(status.code(), Some(CRASH_EXIT));
    assert_eq!(stored(&app_data), [TEXT]);
}

#[test]
fn test_crash_waits_for_its_path() {
    let (mut tc4, _app_data, _ui) = start(&["--test-hooks"]);
    arm(&tc4, "beforeStore", "net/set");
    let reply = tc4.send(&store_request(&tc4));
    assert_eq!(reply.json()["type"], "ok", "{}", reply.text());
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
