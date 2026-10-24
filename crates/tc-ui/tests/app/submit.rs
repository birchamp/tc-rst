//! INV-ONE-REQUEST-PER-ATTEMPT (UI-3): one attempt of a dialog or button sends one request id;
//! a resend after a lost reply reuses it, so the server's reply cache answers it, and a refused
//! attempt makes the next press a new attempt.

use tc_api::ids::RequestId;
use tc_ui::app::submit::Submit;

fn id(n: u8) -> RequestId {
    RequestId::from_bytes([n; 16])
}

#[test]
fn a_lost_reply_resends_the_same_request() {
    let mut submit = Submit::new(id(1));
    assert_eq!(submit.press(), Some(id(1)));
    submit.lost("the app's server did not answer".into());
    assert!(!submit.busy(), "the button is on again after a lost reply");
    assert_eq!(submit.problem(), Some("the app's server did not answer"));
    assert_eq!(submit.press(), Some(id(1)), "the same request goes again");
}

#[test]
fn a_refused_attempt_takes_a_new_request() {
    let mut submit = Submit::new(id(1));
    assert_eq!(submit.press(), Some(id(1)));
    submit.refused("The name is taken.".into(), id(2));
    assert_eq!(submit.problem(), Some("The name is taken."));
    assert_eq!(
        submit.press(),
        Some(id(2)),
        "a new attempt has a new request"
    );
    assert_eq!(submit.problem(), None, "a new press clears the old problem");
}

#[test]
fn a_second_press_in_flight_sends_nothing() {
    let mut submit = Submit::new(id(1));
    assert_eq!(submit.press(), Some(id(1)));
    assert!(submit.busy());
    assert_eq!(submit.press(), None);
    submit.answered();
    assert_eq!(
        submit.press(),
        None,
        "an answered attempt sends nothing more"
    );
}
