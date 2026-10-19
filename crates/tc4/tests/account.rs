//! The Door43 account of the real binary (unit A8): which server a build signs in to, and which
//! keychain it has.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{START, Tc4, http};
use tempfile::TempDir;

fn start(extra: &[&str]) -> (Tc4, TempDir, TempDir) {
    let ui_root = TempDir::new().unwrap();
    let ui = http::seed_ui(ui_root.path());
    let app_data = TempDir::new().unwrap();
    let mut tc4 = Tc4::spawn(app_data.path(), &ui, extra);
    tc4.address(START);
    (tc4, app_data, ui_root)
}

fn account(tc4: &Tc4) -> serde_json::Value {
    let reply = tc4.api("account/state", "{}");
    assert_eq!(reply.status, 200, "{}", reply.text());
    assert_eq!(reply.json()["type"], "ok", "{}", reply.text());
    reply.json()["value"].clone()
}

/// tC4 #120 and J11 1: a development build signs in to Door43's QA server, a release build to
/// production; `account/state` names it and sends nothing (it reads memory).
#[test]
fn a_development_build_signs_in_to_qa_door43() {
    let (mut tc4, _app_data, _ui) = start(&[]);
    let server = account(&tc4)["server"].as_str().unwrap().to_owned();
    if cfg!(debug_assertions) {
        assert_eq!(server, "qa.door43.org");
    } else {
        assert_eq!(server, "git.door43.org");
    }
    tc4.quit();
}

/// The product has no keychain until unit E1a, so its sign-in step has no "Stay signed in"
/// (tC4 #496); a test build with its hooks keeps sign-ins in a test keychain.
#[test]
fn a_test_build_with_hooks_has_a_working_test_keychain_and_a_plain_run_has_none() {
    let (mut tc4, _app_data, _ui) = start(&["--test-hooks"]);
    assert_eq!(account(&tc4)["keychain"], "works");
    let reply = tc4.api(
        "test/keychain",
        r#"{"request":"00000000000000000000000000000001","mode":null,"holds":null,"relaunch":false}"#,
    );
    assert_eq!(reply.json()["type"], "ok", "{}", reply.text());
    tc4.quit();

    let (mut tc4, _app_data, _ui) = start(&[]);
    assert_eq!(account(&tc4)["keychain"], "missing");
    tc4.quit();
}
