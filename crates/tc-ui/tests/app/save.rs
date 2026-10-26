//! The save machine (APP-DESIGN 6.5): ports of tC4's scheduler tests, one field where tC4 has
//! one book, and the rebuild's own rules for retries, reloads, restarts and sessions.

use tc_api::align::AlignSave;
use tc_api::app::{OpenSession, SessionRole};
use tc_api::buffer::{FieldKind, SentSave};
use tc_api::event::SessionMode;
use tc_api::ids::{ProjectId, ServerInstance};
use tc_api::write::ReadOnlyReason;
use tc_api::{ApiError, NotSaved};
use tc_ui::app::client::{CallError, Transport};
use tc_ui::app::save::classify::{Answered, SaveError, classify};
use tc_ui::app::save::indicator::{Worst, all_clean, indicator};
use tc_ui::app::save::{
    Command, FieldSave, FieldScope, GateState, Hold, RETRY_MAX_MS, ResendCheck, SaveMachine,
};

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

/// A verse save writes the verse's reflowed alignment in the same batch (R25, Q-6): its `Ack`
/// carries both tokens, and the field takes its own value's, so the next save names the verse.
#[test]
fn an_ack_with_the_reflowed_alignments_token_saves_the_field_on_its_own_token() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "con la esperanza");
    page.blur(&k);
    let mut ack = super::save_kit::ack("con la esperanza", 7, 2);
    ack.tokens.insert(
        0,
        tc_api::write::TokenUpdate {
            record: tc_api::ids::RecordRef::parse(r#"{"kind":"alignment"}"#).unwrap(),
            token: super::save_kit::tok(8),
        },
    );
    page.deliver(Answered::Acked(ack));
    assert_eq!(worst(&page), Worst::Saved);
    page.focus(&k);
    page.type_text(&k, "con la esperanza de la vida");
    page.blur(&k);
    let sends = page.sends();
    assert_eq!(
        sends.last().map(|(_, base, _)| base.clone()),
        Some(vec![super::save_kit::tok(7)])
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

/// 6.5: a resend goes only to the run it was sent to, decided after its `app/hello`. The event
/// stream can find the restart while that `hello` is on its way; the `hello` then names the run
/// the page already knows, and the request must still not go to it, which has no reply cache.
#[test]
fn a_resend_goes_only_to_the_run_it_was_sent_to() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "kept");
    page.blur(&k);
    let first = page.deliver(Answered::Lost);
    let commands = page.advance(250.0);
    let sent_to = commands
        .iter()
        .find_map(|c| match c {
            Command::Resend { instance, .. } => Some(*instance),
            _ => None,
        })
        .expect("a resend");
    assert_eq!(sent_to, instance(), "the run the request went to");
    let request = first.sent.request;
    let new_run = ServerInstance::from_bytes([2; 16]);
    assert_eq!(
        page.m.resend_check(&k, request, sent_to, sent_to),
        ResendCheck::Send
    );
    assert_eq!(
        page.m.resend_check(&k, request, sent_to, new_run),
        ResendCheck::Restarted(new_run)
    );
    // The stream finds the restart first; the field stays, held, with its editor on screen.
    page.m.restarted(new_run);
    assert_eq!(
        page.m.resend_check(&k, request, sent_to, new_run),
        ResendCheck::Restarted(new_run),
        "the hello names the new run, which the page knows: still nothing is sent"
    );
    assert_eq!(
        page.m.resend_check(&k, request, sent_to, sent_to),
        ResendCheck::Nothing,
        "the page has moved on from the run the request went to"
    );
    // A field that no longer waits for the request sends nothing either.
    let mut page = Page::new();
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "kept");
    page.blur(&k);
    let first = page.deliver(Answered::Lost);
    page.answer(&first, disk("full"));
    assert_eq!(
        page.m
            .resend_check(&k, first.sent.request, instance(), instance()),
        ResendCheck::Nothing
    );
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

/// A reissue after `RequestReused` while the project has no open holds the field. Its entry
/// no longer names the answered request, so a reload never sends that id again.
#[test]
fn request_reused_with_no_open_drops_the_answered_request_from_the_entry() {
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
    page.deliver(Answered::Reissue);
    assert_eq!(page.state(&k), Some(FieldSave::Held(Hold::NoOpen)));
    let entry = page.entry(&k).unwrap();
    assert_eq!((entry.text.as_str(), entry.sent), ("new", None));
    let line = page.mirror.last().unwrap();
    assert!(line.contains("\"sent\":null"), "the mirror's too: {line}");
    let commands = page.m.sessions(&[writable(2)], page.now);
    page.run(commands);
    let again = page.sends().last().unwrap().clone();
    assert_eq!(page.sends().len(), 2);
    assert_eq!(
        (again.0.text.as_str(), again.0.scope),
        ("new", Some(scope(2)))
    );
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

/// C4: a mark under a typed check comment moves the token its draft names, so the unsaved
/// comment is drafted again at once, though its text is the one already drafted; a saved
/// comment is not drafted again.
#[test]
fn a_mark_under_an_unsaved_check_comment_drafts_it_again() {
    let mut page = Page::new();
    let k = key_of(FieldKind::CheckComment, "tn TIT 1:1 abcd");
    page.open_with(
        &k,
        FieldScope::Project(Some(scope(1))),
        pair("", 1, Some(1)),
    );
    page.focus(&k);
    page.type_text(&k, "ask Pablo");
    page.advance(5000.0);
    assert_eq!(page.drafts(), ["ask Pablo"]);
    let commands = page.m.redraft(&k, page.now);
    page.run(commands);
    assert_eq!(page.drafts(), ["ask Pablo", "ask Pablo"], "drafted again");
    page.save(&k);
    page.deliver_ok();
    let commands = page.m.redraft(&k, page.now);
    page.run(commands);
    assert_eq!(
        page.drafts().len(),
        2,
        "a saved comment is not drafted again"
    );
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

/// C2: Ack changes the draft's base even when a throttled draft of that text already ran.
/// Its refreshed draft goes in the Ack's command batch, before the next timer can save it.
#[test]
fn an_ack_drafts_newer_text_immediately_on_its_new_tokens() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "old", 1);
    page.focus(&k);
    page.type_text(&k, "newer");
    page.advance(1000.0);
    assert_eq!(page.drafts(), ["newer"]);
    page.type_text(&k, "sent");
    page.blur(&k);
    page.focus(&k);
    page.type_text(&k, "newer");
    let commands = page.m.reply(
        &k,
        page.inflight.front().unwrap().sent.request,
        Answered::Acked(ack("sent", 9, 2)),
        page.now,
    );
    let write = commands
        .iter()
        .position(|command| {
            matches!(command,
                Command::Write { entry, .. } if entry.text == "newer" && entry.base == [tok(9)]
            )
        })
        .expect("buffer with the Ack's token precedes the draft");
    let draft = commands
        .iter()
        .position(|command| {
            matches!(command,
                Command::Draft { text, base, .. } if text == "newer" && *base == [tok(9)]
            )
        })
        .expect("the same Ack batch drafts the remaining text");
    assert!(write < draft);
    assert_eq!(page.m.text(&k), Some("newer"));
    assert!(matches!(page.m.state(&k), Some(FieldSave::Dirty { .. })));
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
fn a_restart_keeps_only_the_fields_on_screen_sends_nothing_and_ignores_late_replies() {
    let mut page = Page::new();
    let (a, b) = (key("a"), key("b"));
    page.open(&a, "", 1);
    page.open(&b, "", 2);
    // `a` closes while its save is in flight; `b` saves with its editor on screen.
    page.focus(&a);
    page.type_text(&a, "in flight, closed");
    page.close(&a);
    page.focus(&b);
    page.type_text(&b, "in flight, on screen");
    page.blur(&b);
    assert_eq!(page.sends().len(), 2);
    let before = page.store.items.clone();
    let mirrored = page.mirror.len();
    let new_run = tc_api::ids::ServerInstance::from_bytes([2; 16]);
    page.m.restarted(new_run);
    assert!(page.m.state(&a).is_none(), "a field with no editor goes");
    assert_eq!(page.state(&b), Some(FieldSave::Held(Hold::Restarted)));
    assert_eq!(
        worst(&page),
        Worst::Dirty,
        "the text on screen is not saved"
    );
    assert!(
        page.gate(|_, _| unreachable!("nothing is sent"), |_| {}),
        "a held field does not block the gate"
    );
    // Late replies of the old run change nothing.
    while !page.inflight.is_empty() {
        let flight = page.inflight.pop_front().unwrap();
        let answered = page.stored(&flight);
        assert!(page.answer(&flight, answered).is_empty());
    }
    page.advance(10_000.0);
    assert_eq!(page.sends().len(), 2, "nothing is sent again");
    assert_eq!(page.store.items, before, "every entry stays for the offer");
    assert_eq!(page.mirror.len(), mirrored);
    // A second report of the same run, from another part of the page, changes nothing: a field
    // of the new run, closed while its save is in flight, stays.
    let c = key("c");
    page.open(&c, "", 3);
    page.focus(&c);
    page.type_text(&c, "typed in the new run");
    page.close(&c);
    let in_flight = page.state(&c);
    assert!(matches!(in_flight, Some(FieldSave::Saving { .. })));
    page.m.restarted(new_run);
    assert_eq!(page.state(&b), Some(FieldSave::Held(Hold::Restarted)));
    assert_eq!(page.state(&c), in_flight);
    let flight = page.inflight.pop_front().unwrap();
    let answered = page.stored(&flight);
    page.answer(&flight, answered);
    assert_eq!(page.state(&c), Some(FieldSave::Clean));

    // Typing into `b` goes on into the old run's entry, beside the save it never answered, so
    // the offer stores it once over the right token; this run never restores or sends it.
    page.focus(&b);
    page.type_text(&b, "in flight, on screen, and more");
    page.advance(10_000.0);
    let entry = page.entry(&b).unwrap();
    assert_eq!(entry.text, "in flight, on screen, and more");
    assert_eq!(entry.instance, instance(), "the old run's entry");
    assert_eq!(
        entry.sent.map(|sent| sent.text),
        Some("in flight, on screen".to_owned())
    );
    assert_eq!(page.sends().len(), 3, "only the new run's own field saved");
    assert!(
        page.drafts().is_empty(),
        "no draft goes to the new run either"
    );

    // An entry of the old run with a save in flight is never restored into the new one, and
    // its save is never sent.
    let entry_a = page.entry(&a).unwrap();
    assert!(entry_a.sent.is_some());
    assert!(
        page.m
            .restore(a.clone(), entry_a, FieldScope::App, page.now)
            .is_empty()
    );
    assert!(page.m.state(&a).is_none(), "no field of a dead run");
    page.advance(10_000.0);
    assert_eq!(page.sends().len(), 3);

    // A5 goes Home: the editor closes and the field goes; its entry waits for the offer.
    page.focused.clear();
    page.close(&b);
    assert!(page.m.state(&b).is_none());
    assert_eq!(
        page.entry(&b).unwrap().text,
        "in flight, on screen, and more"
    );
    assert_eq!(worst(&page), Worst::Saved);
}

/// RT-19 and 5.4: the write's `StaleScope` can come before the session event of the reopen.
/// The field never goes back to the open that refused it; it waits, asks for the sessions
/// once, and sends once the page knows the new open.
#[test]
fn stale_scope_before_the_session_event_waits_for_the_new_open() {
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
    let refused = page.deliver(Answered::Reattach);
    assert_eq!(
        page.sends().len(),
        1,
        "never sent again to the open that refused it"
    );
    assert_eq!(page.session_reads, 1, "the sessions are asked for");
    assert!(matches!(page.state(&k), Some(FieldSave::Dirty { .. })));
    let entry = page.entry(&k).unwrap();
    assert_eq!(
        (entry.text.as_str(), entry.sent),
        ("new", None),
        "a reload never repeats the refused request"
    );
    assert_eq!(page.m.gate(), GateState::Waiting, "the gate waits for it");
    assert_eq!(page.m.busy(), 1);
    // Time, typing, the Save button and a hello that still lists the old open send nothing.
    page.advance(10_000.0);
    page.type_text(&k, "newer");
    page.blur(&k);
    page.save(&k);
    page.advance(10_000.0);
    let commands = page.m.sessions(&[writable(1)], page.now);
    page.run(commands);
    assert_eq!(page.sends().len(), 1);
    assert_eq!(page.session_reads, 1, "asked once, not in a loop");
    // The reopen: one save, to the new open, with the base token and a new id.
    let commands = page.m.sessions(&[writable(2)], page.now);
    page.run(commands);
    let sends = page.sends();
    assert_eq!(sends.len(), 2);
    let (sent, base, resend) = sends.last().unwrap().clone();
    assert_eq!(
        (sent.text.as_str(), sent.scope, base, resend),
        ("newer", Some(scope(2)), vec![tok(7)], false)
    );
    assert_ne!(sent.request, refused.sent.request);
    page.deliver_ok();
    assert_eq!(page.state(&k), Some(FieldSave::Clean));
}

#[test]
fn stale_scope_before_the_session_event_holds_once_the_project_is_closed() {
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
    page.deliver(Answered::Reattach);
    let commands = page.m.sessions(&[], page.now);
    page.run(commands);
    assert_eq!(page.state(&k), Some(FieldSave::Held(Hold::NoOpen)));
    assert_eq!(page.m.gate(), GateState::Pass);
    assert_eq!(page.sends().len(), 1);
}

/// Two tabs of one browser share a field's item (the owner allows a second tab outside the
/// shell). Tab A's save of A1 is in flight; tab B types B1 into the same field and reloads. B's
/// page saves B1 over the version B was editing, so the store keeps both as a conflict; it never
/// sends A's request again or takes A's answer as its base, which would store B1 over A1 unseen.
#[test]
fn a_second_tab_that_reloads_never_takes_over_the_first_tabs_save() {
    let k = key("t");
    let mut tab_a = Page::new();
    tab_a.open(&k, "", 1);
    tab_a.focus(&k);
    tab_a.type_text(&k, "A1");
    tab_a.blur(&k);
    let a_request = tab_a.sends()[0].0.request;
    let mut tab_b = Page::on(tab_a.store.clone());
    tab_b.open(&k, "", 1);
    tab_b.focus(&k);
    tab_b.type_text(&k, "B1");
    // Tab B reloads.
    let mut reloaded = Page::on(tab_b.store.clone());
    for (key, entry) in tc_ui::app::buffer::entries(&reloaded.store) {
        let commands = reloaded.m.restore(key, entry, FieldScope::App, 0.0);
        reloaded.run(commands);
    }
    reloaded.advance(2000.0);
    let sends = reloaded.sends();
    assert_eq!(sends.len(), 1);
    let (sent, base, resend) = sends[0].clone();
    assert_ne!(sent.request, a_request, "tab A's request is not tab B's");
    assert_eq!(
        (sent.text.as_str(), base, resend),
        ("B1", vec![tok(1)], false)
    );
}

/// 6.6: a field whose item holds another run's entry, or one this build cannot read, is never
/// written there, and its first keystroke reaches the shell's mirror at once, long before a save.
#[test]
fn typing_into_a_field_whose_item_is_not_this_runs_reaches_the_mirror_at_once() {
    use tc_ui::app::buffer::{Store, item_key, write};
    let k = key("t");
    let old_run = tc_api::buffer::BufferEntry {
        api: 1,
        instance: tc_api::ids::ServerInstance::from_bytes([9; 16]),
        base: vec![tok(1)],
        text: "dead".into(),
        sent: None,
        at: tc_api::ids::WallMs(1),
        draft: None,
    };
    for unreadable in [false, true] {
        let mut page = Page::new();
        if unreadable {
            page.store
                .set(&item_key(&k).unwrap(), r#"{"api":2,"text":"dead"}"#)
                .unwrap();
        } else {
            write(&mut page.store, &k, &old_run).unwrap();
        }
        let before = page.store.items.clone();
        page.open(&k, "", 1);
        page.focus(&k);
        page.type_text(&k, " alive");
        let lines: Vec<tc_api::buffer::BufferLine> = page
            .mirror
            .iter()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(
            lines.iter().any(|line| matches!(line,
                tc_api::buffer::BufferLine::Changed { entry, .. }
                    if entry.text == " alive" && entry.instance == instance())),
            "the keystroke is in the mirror (unreadable: {unreadable}): {lines:?}"
        );
        assert!(page.sends().is_empty());
        assert_eq!(page.store.items, before, "the item waits for the offer");
    }
}

/// tC4 drains its schedulers on `pagehide`; so does the page, best effort.
/// tC4's `pagehide` drains each scheduler (`state.jsx:3699-3704`), and `drain` retries a
/// standing failure once (`saveScheduler.ts:139-146`): leaving the page sends what may save and
/// retries each failure with a new id, never a section card's buffer.
#[test]
fn hiding_the_page_sends_what_may_save_and_retries_each_failure_once() {
    let mut page = Page::new();
    let (typed, failed) = (key("typed"), key("failed"));
    let section = key_of(FieldKind::Section, "TIT 1:1-4");
    page.open(&typed, "", 1);
    page.open(&failed, "", 2);
    page.open_with(&section, FieldScope::App, pair("", 3, None));
    page.focus(&failed);
    page.type_text(&failed, "refused");
    page.blur(&failed);
    let refused = page.deliver(disk("full"));
    page.focus(&section);
    page.type_text(&section, "a card");
    page.focused.clear();
    page.focus(&typed);
    page.type_text(&typed, "half a sentence");
    let commands = page.m.hide(page.now);
    page.run(commands);
    let sends = page.sends();
    assert_eq!(page.sent_texts(), ["refused", "refused", "half a sentence"]);
    assert_ne!(
        sends[1].0.request, refused.sent.request,
        "a retry has a new id: the reply cache keeps the refusal"
    );
    let entry = page.entry(&typed).unwrap();
    assert_eq!(
        entry.sent.map(|sent| sent.text),
        Some("half a sentence".to_owned()),
        "the next load of this run sends the same request again"
    );
    assert_eq!(
        page.entry(&failed).unwrap().sent.map(|sent| sent.request),
        Some(sends[1].0.request)
    );
    assert!(
        page.entry(&section).unwrap().sent.is_none(),
        "a card's buffer is saved only by its Save"
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

/// The page after a reload in the same run: each entry of the run becomes a field again.
fn reloaded(page: &Page) -> Page {
    let mut after = Page::on(page.store.clone());
    after.now = page.now;
    for (k, entry) in tc_ui::app::buffer::entries(&after.store) {
        let commands = after.m.restore(k, entry, FieldScope::App, after.now);
        after.run(commands);
    }
    after
}

#[test]
fn a_buffer_write_that_fails_still_saves_and_keeps_the_older_entry() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "text");
    page.store.fail_writes = true;
    page.blur(&k);
    assert_eq!(page.sends().len(), 1, "the save goes anyway");
    let kept = page.entry(&k).expect("the keystroke's entry stays");
    assert_eq!((kept.text.as_str(), kept.sent), ("text", None));
    page.store.fail_writes = false;
    page.deliver_ok();
    assert_eq!(page.state(&k), Some(FieldSave::Clean));
    assert!(page.entry(&k).is_none(), "removed once its text is stored");
}

/// 6.6: a full storage refuses the write that drops a refused save from the entry. The item
/// keeps the text with that save, and a reload sends it again under its id, which the reply
/// cache answers with the refusal it already gave; removing the item lost the text.
#[test]
fn a_full_storage_keeps_the_text_of_a_refused_save_through_a_reload() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "v0", 1);
    page.focus(&k);
    page.type_text(&k, "v0 x");
    page.blur(&k);
    let first = saving(&page, &k);
    page.store.fail_writes = true;
    page.deliver(disk("full"));
    assert!(matches!(page.state(&k), Some(FieldSave::Failed(_))));
    let mut after = reloaded(&page);
    assert_eq!(
        after.m.text(&k),
        Some("v0 x"),
        "the reload brings the text back"
    );
    assert_eq!(
        after.sends()[0].0,
        first,
        "the same request, which the cache answers"
    );
    after.deliver(disk("full"));
    assert!(matches!(after.state(&k), Some(FieldSave::Failed(_))));
}

/// 6.6: a full storage refuses the write that names the save starting now. The item keeps the
/// keystroke's entry, and a reload before that save lands brings the text back and saves it.
#[test]
fn a_full_storage_keeps_the_text_when_the_write_naming_a_save_is_refused() {
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "v0", 1);
    page.focus(&k);
    page.type_text(&k, "v0 x");
    page.store.fail_writes = true;
    page.blur(&k);
    assert_eq!(page.sends().len(), 1);
    // The page goes before the save reaches the store.
    let mut after = reloaded(&page);
    assert_eq!(
        after.m.text(&k),
        Some("v0 x"),
        "the reload brings the text back"
    );
    after.advance(2000.0);
    let sends = after.sends();
    assert_eq!(sends.len(), 1);
    assert_eq!(
        (sends[0].0.text.as_str(), sends[0].1.clone()),
        ("v0 x", vec![tok(1)])
    );
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
            indicator(fields, &aligns, false, &[]).state,
            expected,
            "{fields:?} {aligns:?}"
        );
    }
    assert_eq!(Worst::Saving.data_state(), "saving");
}

#[test]
fn checkpoint_error_shows_only_when_no_save_failed() {
    assert!(indicator(Worst::Saved, &[], true, &[]).commit_error);
    assert!(indicator(Worst::Dirty, &[], true, &[]).commit_error);
    assert!(!indicator(Worst::Error, &[], true, &[]).commit_error);
    assert!(
        !indicator(
            Worst::Saved,
            &[AlignSave::Failed(NotSaved::LookAgain)],
            true,
            &[]
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
        indicator(Worst::Saved, &[], true, &[]).state == Worst::Saved
            && all_clean(Worst::Saved, &[])
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

/// B4b: a section card displays every verse of its section, so its save and its draft are
/// based on the tokens of all of them, in the order the card shows them (STORAGE §5.2, R6).
#[test]
fn a_section_card_is_based_on_every_verse_it_displays() {
    let mut m = SaveMachine::new(instance(), ids());
    let k = key_of(FieldKind::Section, "TIT 2:9-10");
    let (_, commands) = m.open_over(
        k.clone(),
        FieldScope::Project(Some(scope(1))),
        pair("", 1, Some(1)),
        vec![tok(2), tok(3)],
        0.0,
    );
    assert!(commands.is_empty());
    assert_eq!(m.base(&k), Some(&[tok(1), tok(2), tok(3)][..]));
    // Typing and a pause send a draft over all three tokens and no save.
    m.focus(&k, 0.0);
    let typed = m.typed(&k, "9 a\n10 b", tc_ui::app::save::Source::Program, 10.0);
    let later = m.tick(10_000.0);
    assert!(
        typed
            .iter()
            .chain(&later)
            .all(|c| !matches!(c, Command::Send { .. })),
        "{typed:?} {later:?}"
    );
    assert!(later.iter().any(|c| matches!(
        c,
        Command::Draft { base, .. } if *base == [tok(1), tok(2), tok(3)]
    )));
    // Save sends the text over all three.
    let sent = m.save(&k, 10_001.0);
    assert!(
        sent.iter().any(|c| matches!(
            c,
            Command::Send { base, .. } if *base == [tok(1), tok(2), tok(3)]
        )),
        "{sent:?}"
    );
}

/// A section save's `Ack`: the card's text under the first verse, and a token for each verse it
/// displayed, in order (`tc_app::wire::write::section_ack`).
fn section_ack(text: &str, tokens: &[u64], view: u64) -> tc_api::write::Ack {
    let mut ack = super::save_kit::ack(text, tokens[0], view);
    for (n, token) in tokens.iter().enumerate().skip(1) {
        ack.tokens.push(tc_api::write::TokenUpdate {
            record: tc_api::ids::RecordRef::parse(&format!(r#"{{"kind":"verse{n}"}}"#)).unwrap(),
            token: tok(*token),
        });
    }
    ack
}

fn send_of(commands: &[Command]) -> (tc_api::ids::RequestId, Vec<tc_api::token::ShownToken>) {
    commands
        .iter()
        .find_map(|c| match c {
            Command::Send { sent, base, .. } => Some((sent.request, base.clone())),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no send in {commands:?}"))
}

/// An `Ack` keeps a section card based on every verse it displays: a base cut to the first
/// verse made the next save write the whole text into that verse.
#[test]
fn a_section_ack_keeps_the_card_based_on_every_verse() {
    let mut m = SaveMachine::new(instance(), ids());
    let k = key_of(FieldKind::Section, "TIT 2:9-10");
    let at = FieldScope::Project(Some(scope(1)));
    m.open_over(
        k.clone(),
        at,
        pair("9 a\n10 b", 1, Some(1)),
        vec![tok(2)],
        0.0,
    );
    m.focus(&k, 0.0);
    m.typed(&k, "9 a2\n10 b", tc_ui::app::save::Source::Input, 1.0);
    let (request, base) = send_of(&m.save(&k, 2.0));
    assert_eq!(base, [tok(1), tok(2)]);
    // The text changed while the save ran, so the field stays dirty after the Ack.
    m.typed(&k, "9 a2\n\t10 b", tc_ui::app::save::Source::Program, 3.0);
    m.reply(
        &k,
        request,
        Answered::Acked(section_ack("9 a2\n10 b", &[11, 12], 2)),
        4.0,
    );
    assert_eq!(m.base(&k), Some(&[tok(11), tok(12)][..]));
    let (_, base) = send_of(&m.save(&k, 5.0));
    assert_eq!(base, [tok(11), tok(12)]);
}

/// A section save restored after a reload is answered with no card; the card opened again
/// takes a newer read whole, every verse's token with it, and keeps the `Ack`'s tokens over an
/// older read.
#[test]
fn a_section_card_opened_over_its_answered_save_names_every_verse() {
    for (read_seq, expected) in [(9, [tok(21), tok(22)]), (1, [tok(11), tok(12)])] {
        let mut m = SaveMachine::new(instance(), ids());
        let k = key_of(FieldKind::Section, "TIT 2:9-10");
        let at = FieldScope::Project(Some(scope(1)));
        let entry = tc_api::buffer::BufferEntry {
            api: 1,
            instance: instance(),
            base: vec![tok(1), tok(2)],
            text: "9 a\n10 b".into(),
            sent: Some(SentSave {
                request: tc_api::ids::RequestId::from_bytes([7; 16]),
                text: "9 a\n10 b".into(),
                scope: Some(scope(1)),
            }),
            at: tc_api::ids::WallMs(1),
            draft: None,
        };
        let (request, base) = send_of(&m.restore(k.clone(), entry, at.clone(), 0.0));
        assert_eq!(base, [tok(1), tok(2)]);
        m.reply(
            &k,
            request,
            Answered::Acked(section_ack("9 a\n10 b", &[11, 12], 5)),
            1.0,
        );
        let (shown, _) = m.open_over(
            k.clone(),
            at,
            super::save_kit::pair_at("9 a\n10 b", 21, 1, read_seq),
            vec![tok(22)],
            2.0,
        );
        assert_eq!(shown, "9 a\n10 b");
        m.focus(&k, 2.0);
        m.typed(&k, "9 a amén\n10 b", tc_ui::app::save::Source::Input, 3.0);
        let (_, base) = send_of(&m.save(&k, 4.0));
        assert_eq!(base, expected, "read at {read_seq}");
    }
}

#[test]
fn a_new_comment_drafts_on_pauses_and_revises_its_first_acknowledged_identity() {
    let mut page = Page::new();
    let k = key_of(FieldKind::Comment, "TIT 1:4");
    assert_eq!(page.m.open_comment(k.clone(), FieldScope::App), "");
    page.focus(&k);
    for text in ["one", "two", "three"] {
        page.type_text(&k, text);
        page.advance(2500.0);
    }
    assert!(page.sends().is_empty(), "pauses write drafts only");
    assert_eq!(page.drafts(), ["one", "two", "three"]);
    page.blur(&k);
    assert_eq!(page.sent_texts(), ["three"]);
    assert!(page.sends()[0].1.is_empty(), "first save adds a comment");
    page.focus(&k);
    page.type_text(&k, "newer while the reply waits");
    page.deliver_ok();
    page.advance(5000.0);
    assert_eq!(
        page.sent_texts(),
        ["three"],
        "Ack must not blur focused typing"
    );
    assert_eq!(page.entry(&k).unwrap().text, "newer while the reply waits");
    assert_eq!(page.m.base(&k), Some(&[tok(101)][..]));
    page.blur(&k);
    assert_eq!(
        page.sends()[1].1,
        [tok(101)],
        "the second save revises the minted comment"
    );
    page.deliver_ok();
    assert_eq!(page.state(&k), Some(FieldSave::Clean));
    assert!(page.entry(&k).is_none());
}

#[test]
fn a_tokenless_comment_restores_its_original_save_and_preserves_newer_typing() {
    let mut m = SaveMachine::new(instance(), ids());
    let k = key_of(FieldKind::Comment, "03-07");
    m.open_comment(k.clone(), FieldScope::App);
    m.typed(&k, "sent", tc_ui::app::save::Source::Program, 0.0);
    let (request, base) = send_of(&m.blur(&k, 1.0));
    assert!(base.is_empty());
    m.typed(&k, "newer", tc_ui::app::save::Source::Program, 2.0);
    let entry = tc_api::buffer::BufferEntry {
        text: m.text(&k).unwrap().to_owned(),
        ..m.template(&k).unwrap()
    };
    let mut reloaded = SaveMachine::new(instance(), ids());
    let (retried, base) = send_of(&reloaded.restore(k.clone(), entry, FieldScope::App, 3.0));
    assert_eq!(retried, request, "reload resends the original append id");
    assert!(base.is_empty());
    reloaded.reply(&k, retried, Answered::Acked(ack("sent", 8, 2)), 4.0);
    assert_eq!(reloaded.open_comment(k.clone(), FieldScope::App), "newer");
    let (revision, base) = send_of(&reloaded.blur(&k, 5.0));
    assert_ne!(revision, request);
    assert_eq!(
        base,
        [tok(8)],
        "the unsent text becomes a revision after the first append Ack"
    );
}

#[test]
fn clearing_a_comment_uses_the_save_in_flight_then_its_acknowledged_text() {
    let mut m = SaveMachine::new(instance(), ids());
    let k = key_of(FieldKind::Comment, "TIT 1:4");
    m.open(k.clone(), FieldScope::App, pair("old", 1, Some(1)), 0.0);
    m.typed(&k, "new saved text", tc_ui::app::save::Source::Program, 1.0);
    let (request, _) = send_of(&m.blur(&k, 2.0));
    m.focus(&k, 3.0);
    let (restored, commands) = m.revert_comment(&k, 4.0);
    assert_eq!(
        restored, "new saved text",
        "a render snapshot must not revert the pending revision"
    );
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, Command::Send { .. }))
    );
    m.reply(
        &k,
        request,
        Answered::Acked(ack("new saved text", 2, 2)),
        5.0,
    );
    let (restored, _) = m.revert_comment(&k, 6.0);
    assert_eq!(restored, "new saved text");
    assert_eq!(m.state(&k), Some(&FieldSave::Clean));
}

#[test]
fn a_comment_remount_reuses_its_acknowledged_identity_while_the_read_is_stale() {
    let mut m = SaveMachine::new(instance(), ids());
    let k = key_of(FieldKind::Comment, "TIT 1:4");
    let at = FieldScope::Project(Some(scope(1)));
    m.sessions(&[writable(1)], 0.0);
    m.open_comment(k.clone(), at.clone());
    m.typed(&k, "newly added", tc_ui::app::save::Source::Program, 1.0);
    let (request, _) = send_of(&m.blur(&k, 2.0));
    m.reply(&k, request, Answered::Acked(ack("newly added", 9, 5)), 3.0);
    m.close(&k, 4.0);
    assert_eq!(
        m.keys().count(),
        0,
        "a visited clean comment is outside timer/gate/indicator scans"
    );
    assert_eq!(m.next_deadline(), None);
    assert_eq!(m.busy(), 0);
    // The held editor/book read still says there is no comment at the target.
    assert_eq!(m.open_comment(k.clone(), at.clone()), "newly added");
    assert_eq!(m.base(&k), Some(&[tok(9)][..]));
    m.typed(&k, "a revision", tc_ui::app::save::Source::Program, 5.0);
    let (request, base) = send_of(&m.blur(&k, 6.0));
    assert_eq!(
        base,
        [tok(9)],
        "remount cannot append another permanent comment"
    );
    m.reply(&k, request, Answered::Acked(ack("a revision", 10, 6)), 7.0);
    m.close(&k, 8.0);
    let (shown, _) = m.open(
        k.clone(),
        at.clone(),
        super::save_kit::pair_at("stale", 1, 1, 1),
        9.0,
    );
    assert_eq!(
        shown, "a revision",
        "an older nonempty read cannot replace the proved pair either"
    );
    m.close(&k, 10.0);
    let (shown, _) = m.open(
        k.clone(),
        at,
        super::save_kit::pair_at("newer external revision", 11, 1, 7),
        11.0,
    );
    assert_eq!(shown, "newer external revision");
    m.close(&k, 12.0);
    m.sessions(&[], 13.0);
    assert_eq!(
        m.open_comment(k, FieldScope::Project(Some(scope(2)))),
        "",
        "a closed project's pairs cannot leak to another open"
    );
}

#[test]
fn a_new_server_run_forgets_closed_comment_pairs() {
    let mut m = SaveMachine::new(instance(), ids());
    let k = key_of(FieldKind::Comment, "03-07");
    m.open(k.clone(), FieldScope::App, pair("saved", 1, Some(1)), 0.0);
    m.close(&k, 1.0);
    m.restarted(ServerInstance::from_bytes([7; 16]));
    assert_eq!(m.open_comment(k, FieldScope::App), "");
}

#[test]
fn an_invalid_text_refusal_keeps_the_field_and_its_actionable_reason_until_retry() {
    let mut page = Page::new();
    let k = key_of(FieldKind::StoryRef, "03");
    page.open(&k, "a reference", 1);
    page.type_text(&k, "");
    page.blur(&k);
    let reason = "A reference needs text <and> must fit on one line.";
    page.deliver(Answered::Failed(SaveError::NotSaved(NotSaved::Refused(
        tc_api::write::SaveRefusal::InvalidText {
            reason: reason.to_owned(),
        },
    ))));
    assert_eq!(page.m.invalid_text_reason().as_deref(), Some(reason));
    assert_eq!(page.entry(&k).unwrap().text, "");
    page.type_text(&k, "corrected reference");
    page.retry();
    assert_eq!(page.m.invalid_text_reason(), None);
    assert_eq!(page.sends().last().unwrap().1, [tok(1)]);
    page.deliver_ok();
    assert_eq!(page.state(&k), Some(FieldSave::Clean));
}

/// INV-A-REFUSED-SAVE-NEVER-TRAPS-THE-TRANSLATOR (tC4 #801): a save refused for a reason no
/// retry in this open can fix (a newer change arrived under a merged verse: "look again")
/// keeps its text and its reason, and after the gate's one retry the gate lets the person go
/// Home. A disk failure, which a retry may fix, still holds the gate.
#[test]
fn a_save_refused_for_good_lets_the_person_go_home_and_keeps_the_text() {
    let look_again = || Answered::Failed(SaveError::NotSaved(NotSaved::LookAgain));
    let mut page = Page::new();
    let k = key("a");
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "kept text");
    page.blur(&k);
    page.deliver(look_again());
    let passed = page.gate(|_, _| look_again(), |_| {});
    assert!(passed, "the gate lets the person go");
    assert_eq!(page.sends().len(), 2, "one retry by the gate, no more");
    assert_eq!(
        page.state(&k),
        Some(FieldSave::Failed(SaveError::NotSaved(NotSaved::LookAgain))),
        "the field still says why"
    );
    assert_eq!(
        page.entry(&k).map(|e| e.text),
        Some("kept text".to_owned()),
        "the text stays"
    );
    // A disk failure is the other kind: a retry may fix it, so it holds the gate.
    let mut page = Page::new();
    page.open(&k, "", 1);
    page.focus(&k);
    page.type_text(&k, "later");
    page.blur(&k);
    page.deliver(Answered::Failed(SaveError::NotSaved(NotSaved::Disk(
        "full".into(),
    ))));
    assert!(!page.gate(
        |_, _| Answered::Failed(SaveError::NotSaved(NotSaved::Disk("full".into()))),
        |_| {}
    ));
}

/// E1a, INV-A-REFUSED-SAVE-NEVER-TRAPS-THE-TRANSLATOR (tC4 #801, #807): a sync replaced the open
/// a save was made in, so the save is refused as stale. The text stays, and once no open of
/// the project is left (the person left it), Home is not held waiting for one.
#[test]
fn stale_scope_save_keeps_text_and_home_leaves_in_one_click() {
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
    page.type_text(&k, "typed before the sync replaced the open");
    page.blur(&k);
    page.deliver(Answered::Reattach);
    assert_eq!(
        page.m.gate(),
        GateState::Waiting,
        "it waits for the new open"
    );
    // The project is no longer open anywhere: nothing will ever answer the wait.
    let commands = page.m.sessions(&[], page.now);
    page.run(commands);
    assert_ne!(
        page.m.gate(),
        GateState::Waiting,
        "INV-A-REFUSED-SAVE-NEVER-TRAPS-THE-TRANSLATOR: Home is not held for an open that is gone"
    );
    assert_ne!(
        page.m.gate(),
        GateState::Blocked,
        "Home leaves in one click"
    );
    assert_eq!(
        page.entry(&k).map(|e| e.text).as_deref(),
        Some("typed before the sync replaced the open"),
        "the text stays for the next open"
    );
}
