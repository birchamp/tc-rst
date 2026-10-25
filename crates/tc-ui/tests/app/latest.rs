//! INV-A-STALE-REPLY-NEVER-SHOWS: a reply lands only while its attempt is the newest.

use tc_ui::app::latest::Latest;

#[test]
fn a_newer_attempt_makes_an_older_reply_stale() {
    let latest = Latest::new();
    let first = latest.begin();
    assert!(latest.is_current(first));
    assert_eq!(
        latest.current(),
        first,
        "work that belongs to the newest attempt"
    );
    let second = latest.begin();
    assert!(!latest.is_current(first), "the older reply is stale");
    assert!(latest.is_current(second));
    // Beginning without keeping the attempt (leaving a screen) makes every reply stale.
    latest.begin();
    assert!(!latest.is_current(second));
}
