//! What the panic hook leaves for `boot.js` (APP-DESIGN 6.8), and a dialog's one request
//! (4.8, RT-27).

use std::path::Path;

use tc_api::ids::{ProjectRef, RequestId, WallMs};
use tc_ui::app::confirm::{Confirm, ConfirmState};
use tc_ui::app::recovery::{
    CRASH_KEY, CrashMarker, MAX_CLIENT_MESSAGE, NAV_KEY, PANICS_KEY, VIEW_ERROR_TEXT_KEY,
    cut_message, follows_panic,
};

#[test]
fn the_message_is_cut_at_8_kib_on_a_char_boundary() {
    let short = "a panic";
    assert_eq!(cut_message(short), (short.to_owned(), false));
    // Hebrew letters are two bytes: one straddles the limit.
    let long = format!("a{}", "\u{5d0}".repeat(MAX_CLIENT_MESSAGE));
    let (cut, was_cut) = cut_message(&long);
    assert!(was_cut);
    assert!(cut.len() <= MAX_CLIENT_MESSAGE);
    assert_eq!(
        cut.len(),
        MAX_CLIENT_MESSAGE - 1,
        "the straddling letter is left out whole"
    );
    assert!(long.starts_with(&cut));
}

fn marker(page: &str) -> String {
    serde_json::to_string(&CrashMarker {
        page: page.to_owned(),
        view: Some("draft".into()),
        project: Some(ProjectRef::parse("en_ult").unwrap()),
        message: "index out of bounds".into(),
        at: WallMs(5),
    })
    .unwrap()
}

#[test]
fn a_marker_of_this_page_is_not_a_previous_panic() {
    assert!(follows_panic(Some(&marker("aa")), "aa").is_none());
    let previous = follows_panic(Some(&marker("aa")), "bb").unwrap();
    assert_eq!(previous.message, "index out of bounds");
    assert_eq!(previous.view.as_deref(), Some("draft"));
}

#[test]
fn a_marker_that_does_not_parse_is_ignored() {
    assert!(follows_panic(None, "bb").is_none());
    assert!(follows_panic(Some("{"), "bb").is_none());
    assert!(follows_panic(Some(r#"{"page":"aa"}"#), "bb").is_none());
}

#[test]
fn the_keys_are_the_ones_recovery_js_reads() {
    let js = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("app/recovery.js"))
        .unwrap();
    for (name, key) in [
        ("CRASH_KEY", CRASH_KEY),
        ("PANICS_KEY", PANICS_KEY),
        ("TEXT_KEY", VIEW_ERROR_TEXT_KEY),
        ("NAV_KEY", NAV_KEY),
        ("INSTANCE_KEY", tc_ui::app::recovery::INSTANCE_KEY),
        ("TOKEN_KEY", tc_ui::app::boot::TOKEN_KEY),
        ("BUFFER_PREFIX", tc_api::buffer::BUFFER_ITEM_PREFIX),
    ] {
        let line = format!("{name} = '{key}'");
        assert!(js.contains(&line), "recovery.js lacks {line}");
    }
    let line = format!("API_VERSION = {};", tc_api::API_VERSION);
    assert!(js.contains(&line), "recovery.js lacks {line}");
}

fn id(n: u8) -> RequestId {
    RequestId::from_bytes([n; 16])
}

#[test]
fn a_second_press_while_in_flight_sends_nothing() {
    let mut confirm = Confirm::new(id(1));
    assert!(!confirm.disabled());
    assert_eq!(confirm.press(), Some(id(1)));
    assert!(confirm.disabled());
    assert_eq!(confirm.press(), None);
    confirm.answered();
    assert_eq!(confirm.state(), ConfirmState::Answered);
    assert_eq!(
        confirm.press(),
        None,
        "an answered dialog sends nothing more"
    );
}

#[test]
fn a_lost_reply_resends_the_same_id() {
    let mut confirm = Confirm::new(id(1));
    confirm.press();
    assert_eq!(confirm.lost(), id(1));
    assert!(confirm.disabled());
}

#[test]
fn a_new_dialog_has_a_new_id() {
    let mut first = Confirm::new(id(1));
    let mut second = Confirm::new(id(2));
    assert_ne!(first.press(), second.press());
}
