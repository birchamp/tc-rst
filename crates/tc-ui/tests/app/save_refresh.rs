//! The refresh rule for open editors (APP-DESIGN AD37, 6.3): a value and its token move
//! together, never into a focused field and never over input; a field's own save shows no
//! notice; texts compare in NFC; a refresh older than what the field holds is ignored.

use tc_api::app::{OpenSession, SessionRole};
use tc_api::buffer::FieldKind;
use tc_api::event::SessionMode;
use tc_api::ids::ProjectId;
use tc_api::shown::{PersonLabel, Who};
use tc_ui::app::save::classify::Answered;
use tc_ui::app::save::{Command, FieldSave, FieldScope, Notice};

use super::save_kit::{PROJECT_ID, Page, ack, key_of, pair, pair_at, scope, tok, view};

fn juan() -> Option<PersonLabel> {
    Some(PersonLabel {
        who: Who::Username("juan".into()),
        computer: None,
    })
}

fn verse() -> tc_api::buffer::BufferKey {
    key_of(FieldKind::Verse, "TIT 1:1")
}

/// A verse editor open on "Paul" at token 1, view 10.
fn opened() -> (Page, tc_api::buffer::BufferKey) {
    let mut page = Page::new();
    let k = verse();
    page.open_with(&k, FieldScope::App, pair("Paul", 1, Some(10)));
    (page, k)
}

fn refresh(
    page: &mut Page,
    k: &tc_api::buffer::BufferKey,
    value: &str,
    token: u64,
    view: u64,
) -> Vec<Command> {
    let commands = page.m.refresh(k, pair(value, token, Some(view)), juan());
    page.run(commands)
}

#[test]
fn refresh_under_open_clean_editor_swaps_value_and_token_together() {
    let (mut page, k) = opened();
    let commands = refresh(&mut page, &k, "Paul, a servant", 2, 11);
    assert_eq!(
        commands,
        vec![Command::Show {
            key: k.clone(),
            text: "Paul, a servant".into()
        }]
    );
    assert_eq!(page.m.text(&k), Some("Paul, a servant"));
    assert_eq!(page.m.base(&k), Some(&[tok(2)][..]));
    page.focus(&k);
    page.type_text(&k, "Paul, a servant of God");
    page.blur(&k);
    assert_eq!(
        page.sends()[0].1,
        vec![tok(2)],
        "the next save names the new token"
    );
    assert!(page.m.notice(&k).is_none());
}

#[test]
fn refresh_waits_for_blur_while_the_editor_has_focus() {
    let (mut page, k) = opened();
    page.focus(&k);
    let commands = refresh(&mut page, &k, "Paul, a servant", 2, 11);
    assert!(
        commands.is_empty(),
        "nothing is written into a focused field"
    );
    assert_eq!(page.m.text(&k), Some("Paul"));
    assert_eq!(page.m.base(&k), Some(&[tok(1)][..]));
    let commands = page.blur(&k);
    assert!(commands.contains(&Command::Show {
        key: k.clone(),
        text: "Paul, a servant".into()
    }));
    assert_eq!(page.m.base(&k), Some(&[tok(2)][..]));
}

#[test]
fn typing_before_blur_keeps_the_text_and_base_and_shows_the_notice() {
    let (mut page, k) = opened();
    page.focus(&k);
    refresh(&mut page, &k, "Paul, a servant", 2, 11);
    page.type_text(&k, "Paul, an apostle");
    assert_eq!(page.m.notice(&k), Some(&Notice { by: juan() }));
    let commands = page.blur(&k);
    assert!(!commands.iter().any(|c| matches!(c, Command::Show { .. })));
    assert_eq!(page.sends()[0].0.text, "Paul, an apostle");
    assert_eq!(
        page.sends()[0].1,
        vec![tok(1)],
        "the base it was typed over: a conflict, never a revert"
    );
}

#[test]
fn refresh_under_typed_editor_keeps_text_and_token() {
    let (mut page, k) = opened();
    page.focus(&k);
    page.type_text(&k, "Paul, an apostle");
    let commands = refresh(&mut page, &k, "Paul, a servant", 2, 11);
    assert!(commands.is_empty());
    assert_eq!(page.m.text(&k), Some("Paul, an apostle"));
    assert_eq!(
        page.m.notice(&k),
        Some(&Notice { by: juan() }),
        "it names the teammate"
    );
    page.blur(&k);
    assert_eq!(page.sends()[0].1, vec![tok(1)]);
}

#[test]
fn own_save_event_before_ack_shows_no_notice() {
    let (mut page, k) = opened();
    page.focus(&k);
    page.type_text(&k, "Paul, a servant");
    page.blur(&k);
    let flight = page.inflight.pop_front().unwrap();
    // The save's own Changed comes first, then the refetch it causes, then the Ack.
    page.m.local_change(flight.sent.request, view(11));
    let commands = refresh(&mut page, &k, "Paul, a servant", 2, 11);
    assert!(commands.is_empty());
    page.answer(&flight, Answered::Acked(ack("Paul, a servant", 2, 11)));
    assert!(page.m.notice(&k).is_none());
    assert_eq!(page.state(&k), Some(FieldSave::Clean));
    assert_eq!(page.m.base(&k), Some(&[tok(2)][..]));
}

#[test]
fn own_save_event_before_ack_shows_no_notice_without_its_changed_frame() {
    // Even if the Changed frame was lost in a gap and only the refetch came.
    let (mut page, k) = opened();
    page.focus(&k);
    page.type_text(&k, "Paul, a servant");
    page.type_text(&k, "Paul, a servant of");
    page.blur(&k);
    let flight = page.inflight.pop_front().unwrap();
    refresh(&mut page, &k, "Paul, a servant of", 2, 11);
    page.answer(&flight, Answered::Acked(ack("Paul, a servant of", 2, 11)));
    assert!(page.m.notice(&k).is_none());
}

#[test]
fn own_save_event_after_ack_shows_no_notice() {
    let (mut page, k) = opened();
    page.focus(&k);
    page.type_text(&k, "Paul, a servant");
    page.blur(&k);
    let flight = page.inflight.pop_front().unwrap();
    page.answer(&flight, Answered::Acked(ack("Paul, a servant", 2, 11)));
    page.m.local_change(flight.sent.request, view(11));
    page.focus(&k);
    page.type_text(&k, "Paul, a servant of");
    let commands = refresh(&mut page, &k, "Paul, a servant", 2, 11);
    assert!(commands.is_empty());
    assert!(page.m.notice(&k).is_none(), "the refetch of its own save");
}

#[test]
fn a_refresh_that_differs_only_in_nfc_shows_nothing() {
    let (mut page, k) = opened();
    page.focus(&k);
    page.type_text(&k, "Paul, an apostle");
    // The same version's text, read again in another normalization.
    page.m.refresh(&k, pair("Paul", 5, Some(12)), juan());
    assert!(page.m.notice(&k).is_none());
    let mut page2 = Page::new();
    let k2 = verse();
    page2.open_with(&k2, FieldScope::App, pair("e\u{301}", 1, Some(10)));
    page2.focus(&k2);
    page2.type_text(&k2, "x");
    page2.m.refresh(&k2, pair("\u{e9}", 3, Some(11)), juan());
    assert!(page2.m.notice(&k2).is_none(), "é and é are one text");
}

#[test]
fn a_refresh_older_than_the_ack_is_ignored() {
    let (mut page, k) = opened();
    page.focus(&k);
    page.type_text(&k, "Paul, a servant");
    page.blur(&k);
    page.deliver(Answered::Acked(ack("Paul, a servant", 3, 12)));
    let commands = refresh(&mut page, &k, "Paul, older", 2, 11);
    assert!(commands.is_empty());
    assert_eq!(page.m.text(&k), Some("Paul, a servant"));
    assert_eq!(page.m.base(&k), Some(&[tok(3)][..]));
}

#[test]
fn a_clean_field_after_its_ack_takes_a_refresh_again() {
    let (mut page, k) = opened();
    page.focus(&k);
    page.type_text(&k, "Paul, a servant");
    page.blur(&k);
    page.deliver(Answered::Acked(ack("Paul, a servant", 3, 12)));
    // A teammate's later change: nothing unsaved remains, so no "while you were typing".
    let commands = refresh(&mut page, &k, "Paul, a servant of God", 4, 13);
    assert_eq!(
        commands,
        vec![Command::Show {
            key: k.clone(),
            text: "Paul, a servant of God".into()
        }]
    );
    assert!(page.m.notice(&k).is_none());
    assert_eq!(page.m.base(&k), Some(&[tok(4)][..]));
}

#[test]
fn a_teammate_change_during_a_save_is_judged_after_the_ack() {
    let (mut page, k) = opened();
    page.focus(&k);
    page.type_text(&k, "Paul, a servant");
    page.blur(&k);
    let flight = page.inflight.pop_front().unwrap();
    refresh(&mut page, &k, "Juan's text", 5, 14);
    assert!(
        page.m.notice(&k).is_none(),
        "not yet: it may be the field's own save"
    );
    page.answer(&flight, Answered::Acked(ack("Paul, a servant", 3, 12)));
    assert_eq!(
        page.m.text(&k),
        Some("Juan's text"),
        "clean and not focused: the newer pair"
    );
    assert_eq!(page.m.base(&k), Some(&[tok(5)][..]));
}

#[test]
fn a_restored_field_learns_its_stored_text_from_a_read_of_its_base() {
    let mut page = Page::new();
    let k = verse();
    page.open_with(&k, FieldScope::App, pair("Paul", 1, Some(10)));
    page.focus(&k);
    page.type_text(&k, "Paul, a servant");
    page.focused.clear();
    let mut after = Page::on(page.store.clone());
    let (key_back, entry) = tc_ui::app::buffer::entries(&after.store).pop().unwrap();
    let commands = after.m.restore(key_back, entry, FieldScope::App, 0.0);
    after.run(commands);
    // The editor opens on a read of the version the entry is based on: the text differs, so
    // the field stays dirty and shows the person's text, with no notice.
    let shown = after.open_with(&k, FieldScope::App, pair("Paul", 1, Some(10)));
    assert_eq!(shown, "Paul, a servant");
    assert!(after.m.notice(&k).is_none());
    assert!(matches!(after.state(&k), Some(FieldSave::Dirty { .. })));
    // A read of another version since: a teammate's, with the notice.
    let mut third = Page::on(page.store.clone());
    let (key_back, entry) = tc_ui::app::buffer::entries(&third.store).pop().unwrap();
    let commands = third.m.restore(key_back, entry, FieldScope::App, 0.0);
    third.run(commands);
    let commands = third.m.refresh(&k, pair("Juan's", 9, Some(20)), juan());
    third.run(commands);
    assert_eq!(third.m.notice(&k), Some(&Notice { by: juan() }));
    assert_eq!(third.m.text(&k), Some("Paul, a servant"));
}

#[test]
fn a_composition_is_never_written_into() {
    let (mut page, k) = opened();
    page.focus(&k);
    page.m.composition(&k);
    let commands = refresh(&mut page, &k, "Paul, a servant", 2, 11);
    assert!(
        commands.is_empty(),
        "an IME composition is never broken by a refresh"
    );
    // The composition ends with Arabic text: the field keeps it and its base.
    page.type_text(&k, "Paul \u{643}\u{64e}\u{62a}\u{64e}\u{628}\u{64e}");
    assert_eq!(page.m.notice(&k), Some(&Notice { by: juan() }));
    page.blur(&k);
    assert!(page.shows().is_empty());
    assert_eq!(page.sends()[0].1, vec![tok(1)]);
}

// --- Across a reopen: views count per open (3.2) ----------------------------------------------

fn open_at(epoch: u64) -> OpenSession {
    OpenSession {
        at: scope(epoch),
        id: ProjectId::parse(PROJECT_ID).unwrap(),
        mode: SessionMode::Writable,
        role: SessionRole::Screen,
        outside: Default::default(),
    }
}

fn sessions(page: &mut Page, open: &[OpenSession]) {
    let commands = page.m.sessions(open, page.now);
    page.run(commands);
}

/// A verse editor of the project's open at epoch 1, on "Paul" at token 1, view 40.
fn opened_at_epoch_1() -> (Page, tc_api::buffer::BufferKey) {
    let mut page = Page::new();
    let k = verse();
    sessions(&mut page, &[open_at(1)]);
    page.open_with(
        &k,
        FieldScope::Project(Some(scope(1))),
        pair_at("Paul", 1, 1, 40),
    );
    (page, k)
}

/// The editor closes while its save is in flight, the Ack comes (view 41 of the first open),
/// the person goes Home and opens the project again, where a teammate's change is at view 2 of
/// the new open. The editor shows the teammate's text with its token, whether or not the page
/// heard of the reopen before the editor opened.
#[test]
fn an_editor_closed_mid_save_shows_a_teammates_text_after_a_reopen() {
    for heard_of_home in [true, false] {
        let (mut page, k) = opened_at_epoch_1();
        page.focus(&k);
        page.type_text(&k, "Paul, a servant");
        page.close(&k);
        let flight = page
            .inflight
            .pop_front()
            .expect("leaving the field saves it");
        page.answer(&flight, Answered::Acked(ack("Paul, a servant", 5, 41)));
        if heard_of_home {
            sessions(&mut page, &[]);
            sessions(&mut page, &[open_at(2)]);
        }
        let shown = page.open_with(
            &k,
            FieldScope::Project(Some(scope(2))),
            pair_at("Paul, a servant of God", 9, 2, 2),
        );
        assert_eq!(
            shown, "Paul, a servant of God",
            "heard of Home: {heard_of_home}"
        );
        assert_eq!(page.m.base(&k), Some(&[tok(9)][..]));
        assert_eq!(page.m.notice(&k), None);
    }
}

#[test]
fn a_refresh_of_the_new_open_reaches_a_clean_editor_reopened_under_it() {
    let (mut page, k) = opened_at_epoch_1();
    sessions(&mut page, &[open_at(2)]);
    let commands = page
        .m
        .refresh(&k, pair_at("Paul, an apostle", 9, 2, 2), juan());
    page.run(commands);
    assert_eq!(page.shows(), ["Paul, an apostle"]);
    assert_eq!(page.m.base(&k), Some(&[tok(9)][..]));
    // A read of the first open that comes late is older than anything of the new one.
    let commands = page.m.refresh(&k, pair_at("Paul", 1, 1, 99), None);
    page.run(commands);
    assert_eq!(page.m.text(&k), Some("Paul, an apostle"));
}

#[test]
fn a_refresh_of_the_new_open_under_typed_text_shows_the_notice() {
    let (mut page, k) = opened_at_epoch_1();
    page.focus(&k);
    page.type_text(&k, "Paul, a servant");
    sessions(&mut page, &[open_at(2)]);
    let commands = page
        .m
        .refresh(&k, pair_at("Paul, an apostle", 9, 2, 2), juan());
    page.run(commands);
    assert_eq!(page.m.notice(&k), Some(&Notice { by: juan() }));
    assert_eq!(page.m.text(&k), Some("Paul, a servant"));
    assert_eq!(
        page.m.base(&k),
        Some(&[tok(1)][..]),
        "the save becomes a conflict"
    );
}

#[test]
fn views_of_a_field_of_the_app_compare_by_their_count() {
    let (mut page, k) = opened();
    let commands = page.m.refresh(&k, pair("Paul, old", 2, Some(9)), juan());
    page.run(commands);
    assert_eq!(page.m.text(&k), Some("Paul"));
    page.m
        .local_change(tc_api::ids::RequestId::from_bytes([3; 16]), view(12));
    let commands = page.m.refresh(&k, pair("Paul, new", 3, Some(11)), juan());
    page.run(commands);
    assert_eq!(page.m.text(&k), Some("Paul, new"));
}
