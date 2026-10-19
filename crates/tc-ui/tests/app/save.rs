//! The save machine (APP-DESIGN 6.5): ports of tC4's scheduler tests, one field where tC4 has
//! one book, and the rebuild's own rules for retries, reloads, restarts and sessions.

use tc_api::align::AlignSave;
use tc_api::app::{OpenSession, SessionRole};
use tc_api::buffer::{FieldKind, SentSave};
use tc_api::event::SessionMode;
use tc_api::ids::ProjectId;
use tc_api::write::ReadOnlyReason;
use tc_api::{ApiError, NotSaved};
use tc_ui::app::client::{CallError, Transport};
use tc_ui::app::save::classify::{Answered, SaveError, classify};
use tc_ui::app::save::indicator::{Worst, all_clean, indicator};
use tc_ui::app::save::{Command, FieldSave, FieldScope, Hold, RETRY_MAX_MS, SaveMachine};

use super::save_kit::{
    InFlight, PROJECT_ID, Page, ack, ids, instance, key, key_of, pair, scope, tok,
};

fn disk(reason: &str) -> Answered {
    Answered::Failed(SaveError::NotSaved(NotSaved::Disk(reason.to_owned())))
}

fn worst(page: &Page) -> Worst {
    page.m.worst()
}

// --- Ports of tC4's scheduler tests (`test/saveScheduler.test.ts`) ------------------------

/// "typing resets the debounce; one write carries the latest text"
#[test]
fn typing_resets_the_debounce_one_save_carries_the_latest_text() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "draft one");
    page.advance(1500.0);
    page.type_text(&k, "draft two");
    page.advance(1500.0);
    assert!(
        page.sends().is_empty(),
        "the second edit armed the timer again"
    );
    page.advance(500.0);
    assert_eq!(page.sent_texts(), ["draft two"]);
}

/// "a slow write keeps 'saving' until the promise resolves — never optimistic"
#[test]
fn the_state_stays_saving_until_the_reply_never_optimistic() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "slow");
    page.blur(&k);
    assert_eq!(worst(&page), Worst::Saving);
    page.advance(60_000.0);
    assert_eq!(worst(&page), Worst::Saving, "time passing fakes no reply");
    page.deliver_ok();
    assert_eq!(worst(&page), Worst::Saved);
    assert_eq!(page.sends().len(), 1);
}

/// "notifies the full transition sequence saved → dirty → saving → saved"
#[test]
fn states_go_saved_dirty_saving_saved_in_order() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    let mut states = vec![worst(&page)];
    page.focus(&k);
    page.type_text(&k, "sequenced");
    states.push(worst(&page));
    page.blur(&k);
    states.push(worst(&page));
    page.deliver_ok();
    states.push(worst(&page));
    assert_eq!(
        states,
        [Worst::Saved, Worst::Dirty, Worst::Saving, Worst::Saved]
    );
}

/// "keeps the dirty buffer, surfaces 'error', and retains the failed payload"
#[test]
fn a_failure_keeps_the_text_and_the_error() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "edit that fails");
    page.blur(&k);
    page.deliver(disk("disk on fire"));
    assert_eq!(worst(&page), Worst::Error);
    assert_eq!(
        page.state(&k),
        Some(FieldSave::Failed(SaveError::NotSaved(NotSaved::Disk(
            "disk on fire".into()
        ))))
    );
    assert_eq!(page.m.text(&k), Some("edit that fails"));
    let entry = page.entry(&k).expect("the buffer keeps the text");
    assert_eq!(entry.text, "edit that fails");
    assert_eq!(
        entry.sent, None,
        "a reload does not repeat the refused request"
    );
}

/// "does not hammer a failing store: autosave stays parked until retry()"
#[test]
fn a_failing_store_is_not_hammered_until_retry() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "first");
    page.blur(&k);
    page.deliver(disk("nope"));
    page.focus(&k);
    page.type_text(&k, "first, second");
    page.blur(&k);
    page.advance(10_000.0);
    assert_eq!(page.sends().len(), 1);
    assert_eq!(worst(&page), Worst::Error);
    assert_eq!(page.entry(&k).unwrap().text, "first, second");
}

/// "retry() re-attempts with everything buffered since the failure"
#[test]
fn retry_sends_everything_typed_since_the_failure_with_a_new_id() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "first");
    page.blur(&k);
    let failed = page.deliver(disk("nope"));
    page.focus(&k);
    page.type_text(&k, "first second");
    page.retry();
    let sends = page.sends();
    assert_eq!(sends.len(), 2);
    assert_eq!(sends[1].0.text, "first second");
    assert_ne!(
        sends[1].0.request, failed.sent.request,
        "a refusal is never resent under its id"
    );
    page.deliver_ok();
    assert_eq!(worst(&page), Worst::Saved);
}

/// "keeps the book dirty and the next flush writes the newer text"
#[test]
fn edits_during_a_save_keep_the_field_dirty_and_the_next_save_carries_them() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "v1 first");
    page.blur(&k);
    page.focus(&k);
    page.type_text(&k, "v1 first, v2 during flight");
    page.deliver_ok();
    assert!(matches!(page.state(&k), Some(FieldSave::Dirty { .. })));
    page.blur(&k);
    assert_eq!(
        page.sent_texts(),
        ["v1 first", "v1 first, v2 during flight"]
    );
    let second = page.sends()[1].clone();
    assert_eq!(
        second.1,
        vec![tok(101)],
        "the next save names the first save's token"
    );
    page.deliver_ok();
    assert_eq!(worst(&page), Worst::Saved);
}

/// "writes are per-book: two dirty books flush as two whole-file writes"
#[test]
fn two_fields_save_independently() {
    let mut page = Page::new();
    let (a, b) = (key("a"), key("b"));
    page.open(&a, "", 1);
    page.open(&b, "", 2);
    page.focus(&a);
    page.type_text(&a, "A");
    page.blur(&a);
    page.focus(&b);
    page.type_text(&b, "B");
    page.blur(&b);
    assert_eq!(page.inflight.len(), 2, "one save per field, both in flight");
    page.deliver(disk("full"));
    page.deliver_ok();
    assert!(matches!(page.state(&a), Some(FieldSave::Failed(_))));
    assert_eq!(page.state(&b), Some(FieldSave::Clean));
    assert_eq!(worst(&page), Worst::Error);
}

/// tC4 `alignSaveScheduler.test.ts` #356: rapid Retry clicks start one write.
#[test]
fn ten_retry_presses_while_one_runs_send_one_request() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "links");
    page.blur(&k);
    page.deliver(disk("nope"));
    for _ in 0..10 {
        page.retry();
    }
    assert_eq!(page.sends().len(), 2, "the first and one retry");
    assert_eq!(worst(&page), Worst::Saving);
    page.deliver_ok();
    assert_eq!(worst(&page), Worst::Saved);
}

/// tC4 `noteScheduler.test.ts`, round 21: the retry replays the newest text, once.
#[test]
fn retry_replays_the_newest_text_exactly_once() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "note one");
    page.blur(&k);
    page.deliver(disk("nope"));
    page.focus(&k);
    page.type_text(&k, "note two");
    page.type_text(&k, "note three");
    page.blur(&k);
    page.retry();
    page.deliver_ok();
    page.advance(10_000.0);
    assert_eq!(page.sent_texts(), ["note one", "note three"]);
    assert_eq!(worst(&page), Worst::Saved);
}

/// tC4's `drainSchedulers`, round 23: work staged while the gate waited is saved before it
/// passes.
#[test]
fn the_gate_saves_work_typed_while_it_waits() {
    let mut page = Page::new();
    let (a, b) = (key("a"), key("b"));
    page.open(&a, "", 1);
    page.open(&b, "", 2);
    page.focus(&a);
    page.type_text(&a, "A");
    let passed = page.gate(
        |page, flight| page.stored(&flight),
        |page| {
            page.focus(&b);
            page.type_text(&b, "B typed while the gate waited");
            page.focused.remove(&b);
        },
    );
    assert!(passed);
    assert_eq!(page.sent_texts(), ["A", "B typed while the gate waited"]);
    assert_eq!(page.state(&b), Some(FieldSave::Clean));
}

/// tC4's `drain()`: a standing failure is retried once, and the gate refuses while it stands.
#[test]
fn the_gate_resolves_false_while_a_failure_stands_after_one_retry() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "text");
    page.blur(&k);
    page.deliver(disk("nope"));
    let passed = page.gate(|_, _| disk("still"), |_| {});
    assert!(!passed);
    assert_eq!(page.sends().len(), 2, "one retry by the gate");
    // A failure that first comes during the gate is not retried by it either.
    page.retry();
    page.deliver_ok();
    page.focus(&k);
    page.type_text(&k, "more");
    let passed = page.gate(|_, _| disk("again"), |_| {});
    assert!(!passed);
    assert_eq!(page.sends().len(), 4);
    // Once the store takes it, the gate passes.
    let passed = page.gate(|page, flight| page.stored(&flight), |_| {});
    assert!(passed);
    assert_eq!(page.sent_texts().last().map(String::as_str), Some("more"));
}

// --- The rebuild's own rules -----------------------------------------------------------

#[test]
fn clean_only_when_the_ack_text_equals_the_nfc_of_the_current_text() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "e\u{301}");
    page.blur(&k);
    page.deliver_ok();
    assert_eq!(
        page.state(&k),
        Some(FieldSave::Clean),
        "é saved as é in NFC is clean"
    );
    assert_eq!(
        page.shows(),
        ["\u{e9}"],
        "the stored form is shown once the field is left"
    );

    page.focus(&k);
    page.type_text(&k, "x");
    page.blur(&k);
    page.focus(&k);
    page.type_text(&k, "xy");
    page.deliver_ok();
    assert!(
        matches!(page.state(&k), Some(FieldSave::Dirty { .. })),
        "an Ack of x does not clean xy"
    );
}

/// RT-2, case 1: an Ack for older text leaves newer typing dirty, and it saves next.
#[test]
fn an_ack_for_older_text_leaves_newer_typing_dirty_and_saves_it_next() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "T1");
    page.advance(2000.0);
    page.type_text(&k, "T1 T2");
    page.deliver_ok();
    assert!(matches!(page.state(&k), Some(FieldSave::Dirty { .. })));
    let entry = page.entry(&k).unwrap();
    assert_eq!(entry.text, "T1 T2");
    assert_eq!(entry.sent, None);
    assert_eq!(entry.base, vec![tok(101)]);
    page.advance(2000.0);
    assert_eq!(page.sent_texts(), ["T1", "T1 T2"]);
}

#[test]
fn a_lost_reply_is_retried_with_the_same_request_and_body() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "kept");
    page.blur(&k);
    let first = page.deliver(Answered::Lost);
    page.focus(&k);
    page.type_text(&k, "kept and more");
    page.advance(250.0);
    let sends = page.sends();
    assert_eq!(sends.len(), 2);
    assert!(sends[1].2, "a resend");
    assert_eq!(
        sends[1].0, first.sent,
        "the same id and the same body, not the current text"
    );
    assert_eq!(sends[1].1, first.base);
    page.deliver_ok();
    page.blur(&k);
    let sends = page.sends();
    assert_eq!(sends.len(), 3);
    assert_eq!(sends[2].0.text, "kept and more");
    assert_ne!(sends[2].0.request, first.sent.request);
}

#[test]
fn a_lost_reply_backs_off_to_five_seconds() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "kept");
    page.blur(&k);
    let mut gaps = Vec::new();
    for _ in 0..8 {
        page.deliver(Answered::Lost);
        let lost_at = page.now;
        let before = page.sends().len();
        while page.sends().len() == before {
            page.advance(50.0);
        }
        gaps.push(page.now - lost_at);
    }
    assert_eq!(
        gaps,
        [250.0, 500.0, 1000.0, 2000.0, 4000.0, 5000.0, 5000.0, 5000.0]
    );
    assert!(gaps.iter().all(|gap| *gap <= RETRY_MAX_MS));
    assert!(
        page.sends()
            .iter()
            .skip(1)
            .all(|(sent, _, resend)| *resend && *sent == page.sends()[0].0)
    );
}

#[test]
fn a_reload_retries_the_buffered_sent_body_not_the_current_text() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "T1");
    page.blur(&k);
    let first = page.inflight.pop_front().unwrap();
    page.focus(&k);
    page.type_text(&k, "T1 T2");
    // The page reloads with the save in flight.
    let mut after = Page::on(page.store.clone());
    let (restored_key, entry) = tc_ui::app::buffer::entries(&after.store).pop().unwrap();
    assert_eq!(restored_key, k);
    let commands = after.m.restore(k.clone(), entry, FieldScope::App, 0.0);
    after.run(commands);
    let sends = after.sends();
    assert_eq!(sends.len(), 1);
    assert_eq!(sends[0].0, first.sent, "the buffered request, T1 under R1");
    assert_eq!(sends[0].1, first.base);
    after.deliver_ok();
    assert!(matches!(after.state(&k), Some(FieldSave::Dirty { .. })));
    after.advance(2000.0);
    let sends = after.sends();
    assert_eq!(sends[1].0.text, "T1 T2");
    assert_ne!(sends[1].0.request, first.sent.request);
    after.deliver_ok();
    assert_eq!(after.state(&k), Some(FieldSave::Clean));
    assert!(after.entry(&k).is_none());
}

fn saving(page: &Page, k: &tc_api::buffer::BufferKey) -> SentSave {
    match page.state(k) {
        Some(FieldSave::Saving { sent }) => sent,
        other => panic!("{other:?}"),
    }
}

#[test]
fn refusals_are_never_resent_under_their_id() {
    // Retry, the gate's retry, a reattach and a reissue each mint a new id: the reply cache
    // keeps the refusal under the old one.
    let session = OpenSession {
        at: scope(2),
        id: ProjectId::parse(PROJECT_ID).unwrap(),
        mode: SessionMode::Writable,
        role: SessionRole::Screen,
    };
    let mut page = Page::new();
    let k = key_of(FieldKind::Verse, "TIT 1:1");
    page.open_with(
        &k,
        FieldScope::Project(Some(scope(1))),
        pair("", 1, Some(1)),
    );
    page.focus(&k);
    page.type_text(&k, "a");
    page.blur(&k);
    page.deliver(disk("x"));
    page.retry();
    page.deliver(Answered::Reissue);
    let commands = page.m.sessions(std::slice::from_ref(&session), page.now);
    page.run(commands);
    page.deliver(Answered::Reattach);
    page.deliver(disk("y"));
    page.gate(|_, _| disk("z"), |_| {});
    let ids: Vec<_> = page
        .sends()
        .iter()
        .map(|(sent, _, _)| sent.request)
        .collect();
    let unique: std::collections::BTreeSet<_> = ids.iter().collect();
    assert_eq!(ids.len(), 5);
    assert_eq!(
        unique.len(),
        ids.len(),
        "every resend after an answer has its own id"
    );
}

#[test]
fn request_reused_saves_anew_with_a_new_request() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "text");
    page.blur(&k);
    let first = page.deliver(Answered::Reissue);
    let second = saving(&page, &k);
    assert_ne!(second.request, first.sent.request);
    assert_eq!(second.text, "text");
    // A reused id that keeps coming back is a bug: the field shows it after a few.
    for _ in 0..3 {
        page.deliver(Answered::Reissue);
    }
    assert_eq!(
        page.state(&k),
        Some(FieldSave::Failed(SaveError::Api(ApiError::RequestReused)))
    );
}

fn writable(epoch: u64) -> OpenSession {
    OpenSession {
        at: scope(epoch),
        id: ProjectId::parse(PROJECT_ID).unwrap(),
        mode: SessionMode::Writable,
        role: SessionRole::Screen,
    }
}

#[test]
fn stale_scope_reattaches_with_the_base_token_and_a_new_id() {
    let mut page = Page::new();
    let k = key_of(FieldKind::Verse, "TIT 1:1");
    page.open_with(
        &k,
        FieldScope::Project(Some(scope(1))),
        pair("old", 7, Some(1)),
    );
    let commands = page.m.sessions(&[writable(1)], 0.0);
    page.run(commands);
    page.focus(&k);
    page.type_text(&k, "new");
    page.blur(&k);
    // The project was reopened (epoch 2) under the field.
    let commands = page.m.sessions(&[writable(2)], page.now);
    page.run(commands);
    let first = page.deliver(Answered::Reattach);
    let again = page.sends().last().unwrap().clone();
    assert_eq!(again.0.text, "new");
    assert_eq!(again.0.scope, Some(scope(2)), "the current open");
    assert_eq!(
        again.1,
        vec![tok(7)],
        "the base token, never one read later"
    );
    assert_ne!(again.0.request, first.sent.request);
}

#[test]
fn stale_scope_with_no_open_holds_until_the_project_opens_again() {
    let mut page = Page::new();
    let k = key_of(FieldKind::Verse, "TIT 1:1");
    page.open_with(
        &k,
        FieldScope::Project(Some(scope(1))),
        pair("old", 7, Some(1)),
    );
    page.focus(&k);
    page.type_text(&k, "new");
    page.blur(&k);
    let commands = page.m.sessions(&[], page.now);
    page.run(commands);
    page.deliver(Answered::Reattach);
    assert_eq!(page.state(&k), Some(FieldSave::Held(Hold::NoOpen)));
    assert_eq!(worst(&page), Worst::Dirty);
    assert_eq!(page.entry(&k).unwrap().text, "new");
    assert_eq!(page.entry(&k).unwrap().sent, None);
    page.advance(10_000.0);
    assert_eq!(page.sends().len(), 1, "nothing to send it to");
    let commands = page.m.sessions(&[writable(3)], page.now);
    page.run(commands);
    let again = page.sends().last().unwrap().clone();
    assert_eq!(page.sends().len(), 2);
    assert_eq!(again.0.scope, Some(scope(3)));
    assert_eq!(again.1, vec![tok(7)]);
}

#[test]
fn a_session_of_another_project_id_never_takes_a_field() {
    let mut page = Page::new();
    let k = key_of(FieldKind::Verse, "TIT 1:1");
    page.open_with(
        &k,
        FieldScope::Project(Some(scope(1))),
        pair("old", 7, Some(1)),
    );
    page.focus(&k);
    page.type_text(&k, "new");
    page.blur(&k);
    page.deliver(Answered::Reattach);
    // The folder was deleted and made again: same name, another project.
    let other = OpenSession {
        id: ProjectId::parse("00000000-1111-2222-3333-444444444444").unwrap(),
        ..writable(4)
    };
    let commands = page.m.sessions(&[other], page.now);
    page.run(commands);
    assert_eq!(page.state(&k), Some(FieldSave::Held(Hold::NoOpen)));
    assert_eq!(page.sends().len(), 1);
}

#[test]
fn save_only_fields_save_only_on_save() {
    let mut page = Page::new();
    let k = key_of(FieldKind::Section, "TIT 1:1-4");
    page.open_with(
        &k,
        FieldScope::Project(Some(scope(1))),
        pair("old", 1, Some(1)),
    );
    let commands = page.m.sessions(&[writable(1)], 0.0);
    page.run(commands);
    page.focus(&k);
    page.type_text(&k, "a new section");
    page.advance(2500.0);
    assert_eq!(page.drafts(), ["a new section"], "a draft after 2 s idle");
    page.blur(&k);
    page.focus(&k);
    page.type_text(&k, "a new section, more");
    page.blur(&k);
    assert_eq!(
        page.drafts(),
        ["a new section", "a new section, more"],
        "a draft on blur"
    );
    page.advance(10_000.0);
    assert!(page.gate(|page, flight| page.stored(&flight), |_| {}));
    // A reload restores it as a draft, and that does not save it either.
    let mut after = Page::on(page.store.clone());
    let (key_back, entry) = tc_ui::app::buffer::entries(&after.store).pop().unwrap();
    let commands = after
        .m
        .restore(key_back, entry, FieldScope::Project(Some(scope(1))), 0.0);
    after.run(commands);
    after.advance(10_000.0);
    assert!(
        page.sends().is_empty() && after.sends().is_empty(),
        "no save without Save"
    );
    page.save(&k);
    assert_eq!(page.sent_texts(), ["a new section, more"]);
}

#[test]
fn comment_fields_save_on_blur_never_on_a_pause() {
    let mut page = Page::new();
    let k = key_of(FieldKind::Comment, "TIT 1:1");
    page.open_with(
        &k,
        FieldScope::Project(Some(scope(1))),
        pair("", 1, Some(1)),
    );
    page.focus(&k);
    page.type_text(&k, "a");
    page.advance(3000.0);
    page.type_text(&k, "a b");
    page.advance(3000.0);
    page.type_text(&k, "a b c");
    page.advance(3000.0);
    assert!(page.sends().is_empty(), "three pauses send no comment");
    page.blur(&k);
    assert_eq!(page.sent_texts(), ["a b c"]);
}

#[test]
fn check_comment_saves_on_done_and_on_the_gate() {
    let mut page = Page::new();
    let k = key_of(FieldKind::CheckComment, "tn TIT 1:1 abcd");
    page.open_with(
        &k,
        FieldScope::Project(Some(scope(1))),
        pair("", 1, Some(1)),
    );
    page.focus(&k);
    page.type_text(&k, "first");
    page.blur(&k);
    page.advance(5000.0);
    assert!(
        page.sends().is_empty(),
        "blur alone does not save a check comment"
    );
    page.save(&k);
    assert_eq!(page.sent_texts(), ["first"], "Done saves it");
    page.deliver_ok();
    page.focus(&k);
    page.type_text(&k, "first, then left");
    assert!(page.gate(|page, flight| page.stored(&flight), |_| {}));
    assert_eq!(
        page.sent_texts(),
        ["first", "first, then left"],
        "leaving the item saves it"
    );
    // Closing the editor (moving to another item) saves it too (A-10).
    page.type_text(&k, "first, then left, closed");
    page.close(&k);
    assert_eq!(page.sent_texts().len(), 3);
}

#[test]
fn the_gate_never_saves_a_section_buffer_and_is_not_blocked_by_it() {
    let mut page = Page::new();
    let section = key_of(FieldKind::Section, "TIT 1:1-4");
    let verse = key("v");
    page.open_with(&section, FieldScope::App, pair("old", 1, None));
    page.open(&verse, "", 2);
    page.focus(&section);
    page.type_text(&section, "unconfirmed structure");
    page.focused.remove(&section);
    page.focus(&verse);
    page.type_text(&verse, "a verse");
    assert!(page.gate(|page, flight| page.stored(&flight), |_| {}));
    assert_eq!(page.sent_texts(), ["a verse"]);
    assert!(matches!(
        page.state(&section),
        Some(FieldSave::Dirty { .. })
    ));
    assert_eq!(
        page.m.busy(),
        0,
        "a section buffer does not keep the page busy"
    );
}

#[test]
fn a_held_field_does_not_block_the_gate() {
    let mut page = Page::new();
    let k = key_of(FieldKind::Verse, "TIT 1:1");
    page.open_with(
        &k,
        FieldScope::Project(Some(scope(1))),
        pair("old", 7, Some(1)),
    );
    page.focus(&k);
    page.type_text(&k, "new");
    let commands = page.m.sessions(&[], page.now);
    page.run(commands);
    assert_eq!(page.state(&k), Some(FieldSave::Held(Hold::NoOpen)));
    assert!(page.gate(|page, flight| page.stored(&flight), |_| {}));
    assert!(page.sends().is_empty());
}

#[test]
fn discard_removes_the_entry_and_the_draft() {
    let mut page = Page::new();
    let k = key_of(FieldKind::Section, "TIT 1:1-4");
    page.open_with(&k, FieldScope::App, pair("old", 1, None));
    page.focus(&k);
    page.type_text(&k, "a buffer");
    assert!(page.entry(&k).is_some());
    let commands = page.m.discard(&k);
    let commands = page.run(commands);
    assert!(commands.contains(&Command::DiscardDraft { key: k.clone() }));
    assert!(page.entry(&k).is_none());
    assert!(page.m.state(&k).is_none());
    assert_eq!(worst(&page), Worst::Saved);
}

#[test]
fn drafts_go_at_most_once_a_second_never_while_saving_and_never_twice_the_same_text() {
    let mut page = Page::new();
    let k = key_of(FieldKind::Comment, "TIT 1:1");
    page.open_with(&k, FieldScope::App, pair("", 1, None));
    page.focus(&k);
    page.type_text(&k, "a");
    page.advance(999.0);
    assert!(page.drafts().is_empty());
    page.advance(1.0);
    assert_eq!(page.drafts(), ["a"]);
    page.advance(5000.0);
    assert_eq!(page.drafts(), ["a"], "the same text is not sent twice");
    page.type_text(&k, "ab");
    page.blur(&k);
    // Saving: the draft waits, and a draft that overtook the save would leave older text.
    page.focus(&k);
    page.type_text(&k, "abc");
    page.advance(3000.0);
    assert_eq!(page.drafts(), ["a"], "no draft while the save is in flight");
    page.deliver_ok();
    assert_eq!(
        page.drafts(),
        ["a", "abc"],
        "the waiting draft goes after the reply"
    );
}

/// RT-20: typing causes no save and at most one draft a second.
#[test]
fn ten_seconds_of_typing_send_no_save_and_at_most_ten_drafts() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    let mut text = String::new();
    for n in 0..100 {
        text.push(char::from(b'a' + (n % 26) as u8));
        page.type_text(&k, &text);
        page.advance(100.0);
    }
    assert!(page.sends().is_empty(), "no save while typing goes on");
    assert!(page.drafts().len() <= 10, "{} drafts", page.drafts().len());
    assert!(page.drafts().len() >= 9, "{} drafts", page.drafts().len());
}

#[test]
fn read_only_holds_fields_and_keeps_their_entries() {
    let mut page = Page::new();
    let k = key_of(FieldKind::Verse, "TIT 1:1");
    page.open_with(
        &k,
        FieldScope::Project(Some(scope(1))),
        pair("old", 7, Some(1)),
    );
    page.focus(&k);
    page.type_text(&k, "new");
    let read_only = OpenSession {
        mode: SessionMode::ReadOnly(ReadOnlyReason::SecondWriter),
        ..writable(1)
    };
    let commands = page.m.sessions(&[read_only], page.now);
    page.run(commands);
    page.blur(&k);
    page.advance(10_000.0);
    assert_eq!(
        page.state(&k),
        Some(FieldSave::Held(Hold::ReadOnly(
            ReadOnlyReason::SecondWriter
        )))
    );
    assert!(page.sends().is_empty());
    assert_eq!(page.entry(&k).unwrap().text, "new");
    let commands = page.m.sessions(&[writable(1)], page.now);
    page.run(commands);
    assert!(matches!(
        page.state(&k),
        Some(FieldSave::Dirty { .. }) | Some(FieldSave::Saving { .. })
    ));
    page.advance(2000.0);
    assert_eq!(page.sent_texts(), ["new"]);
}

#[test]
fn a_restart_drops_every_field_keeps_every_entry_and_ignores_late_replies() {
    let mut page = Page::new();
    let (a, b) = (key("a"), key("b"));
    page.open(&a, "", 1);
    page.open(&b, "", 2);
    page.focus(&a);
    page.type_text(&a, "in flight");
    page.blur(&a);
    page.focus(&b);
    page.type_text(&b, "dirty");
    page.focused.clear();
    let before = page.store.items.clone();
    page.m
        .restarted(tc_api::ids::ServerInstance::from_bytes([2; 16]));
    let late = page.deliver_ok();
    page.advance(10_000.0);
    assert_eq!(page.sends().len(), 1, "nothing is sent again");
    assert_eq!(page.store.items, before, "every entry stays for the offer");
    assert!(page.m.state(&a).is_none() && page.m.state(&b).is_none());
    assert_eq!(late.sent.text, "in flight");
    // An entry of the old run is never restored into the new one.
    let (key_back, entry) = tc_ui::app::buffer::entries(&page.store).pop().unwrap();
    assert!(
        page.m
            .restore(key_back, entry, FieldScope::App, page.now)
            .is_empty()
    );
}

#[test]
fn the_entry_is_written_before_the_send() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "text");
    let commands = page.blur(&k);
    let write = commands
        .iter()
        .position(|c| matches!(c, Command::Write { entry, .. } if entry.sent.is_some()))
        .expect("a write with the save");
    let send = commands
        .iter()
        .position(|c| matches!(c, Command::Send { .. }))
        .expect("a send");
    assert!(write < send);
    let entry = page.entry(&k).unwrap();
    assert_eq!(entry.sent, Some(saving(&page, &k)));
}

#[test]
fn a_buffer_write_that_fails_still_saves_and_leaves_no_stale_sent() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "text");
    page.store.fail_writes = true;
    page.blur(&k);
    assert_eq!(page.sends().len(), 1, "the save goes anyway");
    assert!(
        page.entry(&k).is_none(),
        "no entry with an older text and no `sent`"
    );
    page.store.fail_writes = false;
    page.deliver_ok();
    assert_eq!(page.state(&k), Some(FieldSave::Clean));
}

/// Trap 5 of the plan: an Ack cleans a focused field and removes its entry; the next
/// keystroke must make one again.
#[test]
fn a_focused_field_cleaned_by_its_ack_writes_a_new_entry_on_the_next_keystroke() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "one");
    page.advance(2000.0);
    page.deliver_ok();
    assert!(page.entry(&k).is_none());
    page.type_text(&k, "one two");
    let entry = page.entry(&k).expect("boot.js made it from the template");
    assert_eq!(entry.text, "one two");
    assert_eq!(
        entry.base,
        vec![tok(101)],
        "the template names the Ack's token"
    );
}

#[test]
fn a_malformed_ack_is_a_failure_not_a_loop() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "text");
    page.blur(&k);
    let mut bad = ack("text", 9, 9);
    bad.saved.clear();
    page.deliver(Answered::Acked(bad));
    assert!(matches!(
        page.state(&k),
        Some(FieldSave::Failed(SaveError::BadAck(_)))
    ));
    page.advance(10_000.0);
    assert_eq!(page.sends().len(), 1);
}

#[test]
fn classify_maps_every_call_error() {
    let cases: Vec<(Result<tc_api::write::Ack, CallError>, Answered)> = vec![
        (Ok(ack("a", 1, 1)), Answered::Acked(ack("a", 1, 1))),
        (
            Err(CallError::Api(ApiError::NotSaved(NotSaved::StaleScope))),
            Answered::Reattach,
        ),
        (
            Err(CallError::Api(ApiError::StaleScope { current: None })),
            Answered::Reattach,
        ),
        (
            Err(CallError::Api(ApiError::RequestReused)),
            Answered::Reissue,
        ),
        (
            Err(CallError::Api(ApiError::Version { server: 2 })),
            Answered::Lost,
        ),
        (Err(CallError::Dropped), Answered::Lost),
        (
            Err(CallError::Transport(Transport::Unreachable("gone".into()))),
            Answered::Lost,
        ),
        (
            Err(CallError::Transport(Transport::Status(503))),
            Answered::Lost,
        ),
        (
            Err(CallError::Transport(Transport::Status(401))),
            Answered::Failed(SaveError::Transport(Transport::Status(401))),
        ),
        (
            Err(CallError::Transport(Transport::Unreadable("x".into()))),
            Answered::Failed(SaveError::Transport(Transport::Unreadable("x".into()))),
        ),
        (
            Err(CallError::Api(ApiError::NotSaved(NotSaved::BadToken))),
            Answered::Failed(SaveError::NotSaved(NotSaved::BadToken)),
        ),
        (
            Err(CallError::Api(ApiError::Unavailable(
                tc_api::error::Feature::Writes,
            ))),
            Answered::Failed(SaveError::Api(ApiError::Unavailable(
                tc_api::error::Feature::Writes,
            ))),
        ),
    ];
    for (reply, expected) in cases {
        assert_eq!(classify(reply.clone()), expected, "{reply:?}");
    }
}

proptest::proptest! {
    /// Only an `Ok` is an `Acked`: no error a server can answer is read as a save.
    #[test]
    fn classify_never_reads_an_error_as_a_save(error in proptest::prelude::any::<ApiError>()) {
        proptest::prop_assert!(!matches!(classify(Err(CallError::Api(error))), Answered::Acked(_)));
    }
}

#[test]
fn indicator_shows_the_worst_state() {
    let failed = AlignSave::Failed(NotSaved::Disk("x".into()));
    let rows = [
        (Worst::Saved, vec![], Worst::Saved),
        (Worst::Saved, vec![AlignSave::Unsaved], Worst::Dirty),
        (Worst::Dirty, vec![AlignSave::Saving], Worst::Saving),
        (Worst::Saving, vec![AlignSave::Saved], Worst::Saving),
        (
            Worst::Saved,
            vec![AlignSave::Saving, failed.clone()],
            Worst::Error,
        ),
        (Worst::Error, vec![AlignSave::Saved], Worst::Error),
    ];
    for (fields, aligns, expected) in rows {
        assert_eq!(
            indicator(fields, &aligns, false).state,
            expected,
            "{fields:?} {aligns:?}"
        );
    }
    assert_eq!(Worst::Saving.data_state(), "saving");
}

#[test]
fn checkpoint_error_shows_only_when_no_save_failed() {
    assert!(indicator(Worst::Saved, &[], true).commit_error);
    assert!(indicator(Worst::Dirty, &[], true).commit_error);
    assert!(!indicator(Worst::Error, &[], true).commit_error);
    assert!(
        !indicator(
            Worst::Saved,
            &[AlignSave::Failed(NotSaved::LookAgain)],
            true
        )
        .commit_error
    );
}

#[test]
fn all_clean_counts_aligners_not_checkpoints() {
    assert!(all_clean(Worst::Saved, &[AlignSave::Saved]));
    assert!(!all_clean(Worst::Saved, &[AlignSave::Unsaved]));
    assert!(!all_clean(Worst::Dirty, &[]));
    assert!(
        indicator(Worst::Saved, &[], true).state == Worst::Saved && all_clean(Worst::Saved, &[])
    );
}

#[test]
fn busy_counts_saves_in_flight_and_armed_timers_only() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    assert_eq!(page.m.busy(), 0);
    page.focus(&k);
    page.type_text(&k, "x");
    assert_eq!(page.m.busy(), 1, "an armed save timer");
    page.blur(&k);
    assert_eq!(page.m.busy(), 1, "a save in flight");
    page.deliver_ok();
    assert_eq!(page.m.busy(), 0);
}

#[test]
fn a_machine_reports_its_instance_and_no_deadline_when_idle() {
    let page = SaveMachine::new(instance(), ids());
    assert_eq!(page.instance(), instance());
    assert_eq!(page.next_deadline(), None);
    assert_eq!(page.worst(), Worst::Saved);
}

#[test]
fn a_late_reply_for_an_older_request_is_ignored() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "text");
    page.blur(&k);
    let first = page.inflight.front().cloned().unwrap();
    page.deliver(disk("x"));
    page.retry();
    // The first request's answer comes again, late.
    let late = InFlight { ..first };
    let commands = page.m.reply(
        &late.key,
        late.sent.request,
        Answered::Acked(ack("text", 5, 5)),
        page.now,
    );
    assert!(commands.is_empty());
    assert!(matches!(page.state(&k), Some(FieldSave::Saving { .. })));
}
