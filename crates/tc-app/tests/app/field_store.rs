//! `test/fieldStore`: a write through the real reply cache that records every text it
//! stores, durably, with the token and conflict rules of the store (STORAGE:1446-1453).
//!
//! These tests use real time and a multi-thread runtime: the store writes its file on the
//! blocking pool, which a paused clock would not wait for.

use std::time::Duration;

use proptest::prelude::*;
use tc_api::test::{Crash, CrashPoint, FieldDraft, Hold, HoldPoint, Release, TokenBase};
use tc_api::write::Stored;
use tc_api::{ApiError, NotSaved, Reply, ShownToken};
use tc_app::{Answer, App, Then};
use unicode_normalization::UnicodeNormalization;

use super::common::{
    Parts, TestApp, answer, body_of, field_log, field_token, fields_file, is_busy, post, reply_in,
    rid, store, store_req, until_held_count, until_quiet,
};

async fn started() -> (tempfile::TempDir, TestApp) {
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_in(dir.path(), |_, _| {}).await;
    (dir, t)
}

/// Another app on the same folders, as the harness restarts `tc4`.
async fn restarted(dir: &tempfile::TempDir) -> TestApp {
    TestApp::start_in(dir.path(), |_, _| {}).await
}

fn texts(log: &tc_api::test::FieldLogView) -> Vec<(String, TokenBase)> {
    log.stores
        .iter()
        .map(|stored| (stored.text.clone(), stored.base))
        .collect()
}

macro_rules! multi {
    ($(#[$meta:meta])* async fn $name:ident() $body:block) => {
        $(#[$meta])*
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn $name() $body
    };
}

multi! {
    async fn a_field_starts_empty_with_a_token() {
        let (_dir, t) = started().await;
        let log = field_log(&t.app, "title").await;
        assert_eq!(log.current.value, "");
        assert!(log.stores.is_empty());
        assert!(log.current.marker.is_none());
        // The token of a field nobody stored into is one the store accepts.
        let ack = store(&t.app, 1, "title", &log.current.token, "Titus").await.unwrap();
        assert_eq!(ack.stored, Stored::Written);
        assert_eq!(field_log(&t.app, "title").await.current.value, "Titus");
    }
}

multi! {
    async fn field_store_records_each_text_once() {
        let (_dir, t) = started().await;
        let t0 = field_token(&t.app, "title").await;
        let first = store(&t.app, 1, "title", &t0, "One").await.unwrap();
        let t1 = first.tokens[0].token.clone();
        let second = store(&t.app, 2, "title", &t1, "Two").await.unwrap();
        let log = field_log(&t.app, "title").await;
        assert_eq!(
            texts(&log),
            vec![
                ("One".to_owned(), TokenBase::Current),
                ("Two".to_owned(), TokenBase::Current)
            ]
        );
        assert_eq!(log.stores[0].request, rid(1));
        assert_eq!(log.stores[1].request, rid(2));
        assert_eq!(log.current.value, "Two");
        assert_eq!(log.current.token, second.tokens[0].token, "the token of the last Ack");
        assert_eq!(second.saved[0].text, "Two");
        assert!(second.new_conflicts.is_empty());
        assert_eq!(second.view.get(), 2);
    }
}

multi! {
    async fn an_unchanged_text_stores_nothing() {
        let (dir, t) = started().await;
        let t0 = field_token(&t.app, "title").await;
        let first = store(&t.app, 1, "title", &t0, "One").await.unwrap();
        let t1 = first.tokens[0].token.clone();
        let before = fields_file(&t.app_data);

        let again = store(&t.app, 2, "title", &t1, "One").await.unwrap();
        assert_eq!(again.stored, Stored::Unchanged);
        assert_eq!(again.tokens[0].token, t1);
        assert_eq!(again.saved[0].text, "One");
        assert_eq!(field_log(&t.app, "title").await.stores.len(), 1);
        assert_eq!(fields_file(&t.app_data), before, "the file was not touched");
        assert_eq!(field_log(&restarted(&dir).await.app, "title").await.stores.len(), 1);
    }
}

multi! {
    /// A store of the text of an older version, over a newer one, leaves the field clean on
    /// the older token. Answering with the newest token would make the UI's next save replace
    /// the newer version without a conflict.
    async fn an_unchanged_store_over_an_older_version_keeps_that_versions_token() {
        let (_dir, t) = started().await;
        let t0 = field_token(&t.app, "title").await;
        let t1 = store(&t.app, 1, "title", &t0, "T1").await.unwrap().tokens[0].token.clone();
        let t2 = store(&t.app, 2, "title", &t1, "T2").await.unwrap().tokens[0].token.clone();
        assert_ne!(t1, t2);

        let unchanged = store(&t.app, 3, "title", &t1, "T1").await.unwrap();
        assert_eq!(unchanged.stored, Stored::Unchanged);
        assert_eq!(unchanged.saved[0].text, "T1");
        assert_eq!(unchanged.tokens[0].token, t1, "the token it came with, not the newest");
        assert_eq!(unchanged.view.get(), 2, "the view is the current one");
        assert_eq!(field_log(&t.app, "title").await.stores.len(), 2);

        // The next save from that token meets the newer version as a conflict.
        let next = store(&t.app, 4, "title", &unchanged.tokens[0].token, "T3").await.unwrap();
        assert_eq!(next.stored, Stored::Written);
        assert_eq!(next.new_conflicts.len(), 1);
        let log = field_log(&t.app, "title").await;
        assert_eq!(log.stores.last().unwrap().base, TokenBase::Stale);
    }
}

multi! {
    async fn text_is_stored_in_nfc() {
        let (_dir, t) = started().await;
        let t0 = field_token(&t.app, "title").await;
        let decomposed = "e\u{301}";
        let ack = store(&t.app, 1, "title", &t0, decomposed).await.unwrap();
        assert_eq!(ack.saved[0].text, "\u{e9}");
        let log = field_log(&t.app, "title").await;
        assert_eq!(log.stores[0].text, "\u{e9}");
        assert_eq!(log.current.value, "\u{e9}");
        let same = store(&t.app, 2, "title", &ack.tokens[0].token, "\u{e9}").await.unwrap();
        assert_eq!(same.stored, Stored::Unchanged, "compared in NFC");
    }
}

multi! {
    async fn a_stale_token_is_stored_beside_and_marked() {
        let (_dir, t) = started().await;
        let t0 = field_token(&t.app, "title").await;
        store(&t.app, 1, "title", &t0, "T1").await.unwrap();
        let second = store(&t.app, 2, "title", &t0, "T2").await.unwrap();
        assert_eq!(second.stored, Stored::Written);
        assert_eq!(second.new_conflicts.len(), 1);
        assert_eq!(second.new_conflicts[0].by.who, tc_api::shown::Who::Mine);
        assert_eq!(
            texts(&field_log(&t.app, "title").await),
            vec![
                ("T1".to_owned(), TokenBase::Current),
                ("T2".to_owned(), TokenBase::Stale)
            ]
        );
    }
}

multi! {
    async fn a_token_of_another_field_or_a_forged_one_is_bad_token() {
        let (_dir, t) = started().await;
        let other_dir = tempfile::tempdir().unwrap();
        let other = TestApp::start_in(other_dir.path(), |_, _| {}).await;
        let own = field_token(&t.app, "title").await;
        let of_another_field = field_token(&t.app, "other").await;
        let of_another_key = field_token(&other.app, "title").await;

        // One character of the body changed: the tag no longer fits.
        let text = serde_json::to_string(&own).unwrap();
        let mut chars: Vec<char> = text.trim_matches('"').chars().collect();
        let last = chars.len() - 1;
        chars[last] = if chars[last] == 'A' { 'B' } else { 'A' };
        let flipped: String = chars.into_iter().collect();
        let flipped = ShownToken::from_body(flipped.strip_prefix("st1.").unwrap()).unwrap();

        for (n, (what, token)) in [
            ("another field's", of_another_field),
            ("another key's", of_another_key),
            ("a changed one", flipped),
        ]
        .into_iter()
        .enumerate()
        {
            let id = 10 + u8::try_from(n).unwrap();
            let refused = store(&t.app, id, "title", &token, "x").await;
            assert_eq!(refused, Err(ApiError::NotSaved(NotSaved::BadToken)), "{what}");
        }
        assert!(field_log(&t.app, "title").await.stores.is_empty());
    }
}

async fn hold_saves(app: &App) {
    post(
        app,
        &Hold {
            request: rid(200),
            at: HoldPoint::Save,
        },
    )
    .await
    .unwrap();
}

async fn release_saves(app: &App, id: u8) {
    post(
        app,
        &Release {
            request: rid(id),
            at: HoldPoint::Save,
        },
    )
    .await
    .unwrap();
}

multi! {
    async fn a_retry_after_a_lost_reply_stores_once() {
        let (_dir, t) = started().await;
        let t0 = field_token(&t.app, "title").await;
        hold_saves(&t.app).await;
        let request = store_req(1, "title", &t0, "Saved");
        let body = body_of(&request);

        // The caller goes away (a reload) while the store is held.
        let lost = {
            let app = t.app.clone();
            let body = body.clone();
            tokio::spawn(async move { app.call("test/fieldStore", &body).await })
        };
        until_held_count(&t.app, 1).await;
        lost.abort();
        let _ = lost.await;
        release_saves(&t.app, 201).await;
        until_quiet(&t.app).await;

        let retry = t.app.call("test/fieldStore", &body).await;
        let third = t.app.call("test/fieldStore", &body).await;
        assert_eq!(retry, third);
        let Reply::Ok(ack): Reply<tc_api::write::Ack> = reply_in(&retry) else {
            panic!("{retry:?}")
        };
        assert_eq!(ack.stored, Stored::Written);
        assert_eq!(field_log(&t.app, "title").await.stores.len(), 1);
    }
}

multi! {
    async fn a_retry_while_the_store_is_held_waits_and_stores_once() {
        let (_dir, t) = started().await;
        let t0 = field_token(&t.app, "title").await;
        hold_saves(&t.app).await;
        let body = body_of(&store_req(1, "title", &t0, "Saved"));

        let spawn = |app: App, body: Vec<u8>| {
            tokio::spawn(async move { app.call("test/fieldStore", &body).await })
        };
        let first = spawn(t.app.clone(), body.clone());
        until_held_count(&t.app, 1).await;
        let second = spawn(t.app.clone(), body.clone());
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!second.is_finished(), "the retry waits for the first attempt");

        assert!(is_busy(&t.app).await);
        // Reads do not wait behind a held store.
        let log = tokio::time::timeout(Duration::from_secs(60), field_log(&t.app, "title"))
            .await
            .expect("hung: fieldLog answered while the store was held");
        assert!(log.stores.is_empty(), "nothing is stored yet");

        release_saves(&t.app, 201).await;
        let (first, second) = (first.await.unwrap(), second.await.unwrap());
        let (Answer::Reply { body: a, .. }, Answer::Reply { body: b, .. }) = (&first, &second)
        else {
            panic!("{first:?} {second:?}")
        };
        assert_eq!(a, b, "identical bytes");
        assert_eq!(field_log(&t.app, "title").await.stores.len(), 1);
        until_quiet(&t.app).await;
        assert!(!is_busy(&t.app).await);
    }
}

multi! {
    async fn a_retry_with_another_text_is_request_reused() {
        let (_dir, t) = started().await;
        let t0 = field_token(&t.app, "title").await;
        store(&t.app, 1, "title", &t0, "First").await.unwrap();
        let reused = store(&t.app, 1, "title", &t0, "Second").await;
        assert_eq!(reused, Err(ApiError::RequestReused));
        assert_eq!(field_log(&t.app, "title").await.stores.len(), 1);
        assert_eq!(field_log(&t.app, "title").await.current.value, "First");
    }
}

multi! {
    async fn stores_of_one_app_go_one_at_a_time() {
        let (_dir, t) = started().await;
        let ta = field_token(&t.app, "a").await;
        let tb = field_token(&t.app, "b").await;
        hold_saves(&t.app).await;
        let spawn = |app: App, request| {
            tokio::spawn(async move { app.call("test/fieldStore", &body_of(&request)).await })
        };
        let a = spawn(t.app.clone(), store_req(1, "a", &ta, "A"));
        until_held_count(&t.app, 1).await;
        let b = spawn(t.app.clone(), store_req(2, "b", &tb, "B"));
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!b.is_finished(), "B waits behind A");
        assert_eq!(t.app.holding(), 1, "only A is at the hold");
        assert!(field_log(&t.app, "b").await.stores.is_empty());

        release_saves(&t.app, 201).await;
        let (a, b) = (a.await.unwrap(), b.await.unwrap());
        assert!(matches!(a, Answer::Reply { .. }) && matches!(b, Answer::Reply { .. }));
        assert_eq!(field_log(&t.app, "a").await.stores.len(), 1);
        assert_eq!(field_log(&t.app, "b").await.stores.len(), 1);
    }
}

async fn arm(app: &App, id: u8, at: CrashPoint, path: Option<&str>) {
    post(
        app,
        &Crash {
            request: rid(id),
            at,
            path: path.map(str::to_owned),
        },
    )
    .await
    .unwrap();
}

multi! {
    async fn crash_before_store_stores_nothing() {
        let (dir, t) = started().await;
        let t0 = field_token(&t.app, "title").await;
        let before = fields_file(&t.app_data);
        arm(&t.app, 100, CrashPoint::BeforeStore, Some("test/fieldStore")).await;

        let ended = answer(&t.app, &store_req(1, "title", &t0, "Lost")).await;
        assert_eq!(ended, Answer::Ended);
        assert_eq!(t.ender.ended(), vec![CrashPoint::BeforeStore]);
        // The app is gone: nothing else answers.
        assert_eq!(t.app.call("test/quiet", b"{}").await, Answer::Ended);
        assert_eq!(t.app.call("app/hello", b"{}").await, Answer::Ended);

        let after = restarted(&dir).await;
        assert!(field_log(&after.app, "title").await.stores.is_empty());
        assert_eq!(fields_file(&t.app_data), before);
    }
}

multi! {
    async fn crash_before_reply_keeps_the_store_across_a_restart() {
        let (dir, t) = started().await;
        let t0 = field_token(&t.app, "title").await;
        arm(&t.app, 100, CrashPoint::BeforeReply, Some("test/fieldStore")).await;
        let ended = answer(&t.app, &store_req(1, "title", &t0, "Kept")).await;
        assert_eq!(ended, Answer::Ended, "the reply is lost");
        assert_eq!(t.ender.ended(), vec![CrashPoint::BeforeReply]);

        let after = restarted(&dir).await;
        let log = field_log(&after.app, "title").await;
        assert_eq!(texts(&log), vec![("Kept".to_owned(), TokenBase::Current)]);
        // The token the new app gives is one the next store takes as the current version.
        let next = store(&after.app, 2, "title", &log.current.token, "Next").await.unwrap();
        assert_eq!(next.new_conflicts.len(), 0);
        assert_eq!(
            field_log(&after.app, "title").await.stores[1].base,
            TokenBase::Current
        );
        // A resend of the lost request with the token it was made over is a second store of
        // the same text, marked stale. It is what the reload test of the save machine must
        // never see: after a reload the UI compares its buffered text with the value it reads.
        let resend = store(&after.app, 1, "title", &t0, "Kept").await.unwrap();
        assert_eq!(resend.stored, Stored::Written);
        assert_eq!(resend.new_conflicts.len(), 1);
        assert_eq!(
            field_log(&after.app, "title").await.stores.last().unwrap().base,
            TokenBase::Stale
        );
    }
}

multi! {
    async fn crash_after_reply_answers_then_ends() {
        let (_dir, t) = started().await;
        let t0 = field_token(&t.app, "title").await;
        arm(&t.app, 100, CrashPoint::AfterReply, Some("test/fieldStore")).await;
        let answered = answer(&t.app, &store_req(1, "title", &t0, "Sent")).await;
        let Answer::Reply { then, .. } = &answered else {
            panic!("{answered:?}")
        };
        assert_eq!(*then, Then::EndProcess);
        let Reply::Ok(ack): Reply<tc_api::write::Ack> = reply_in(&answered) else {
            panic!("{answered:?}")
        };
        assert_eq!(ack.stored, Stored::Written);
        assert_eq!(field_log(&t.app, "title").await.stores.len(), 1);
        // The server ends the process; the app does not end itself.
        assert!(t.ender.ended().is_empty());
        assert!(matches!(t.app.call("app/hello", b"{}").await, Answer::Reply { .. }));
    }
}

multi! {
    async fn a_crash_armed_for_another_path_waits_for_that_path() {
        let (_dir, t) = started().await;
        let t0 = field_token(&t.app, "title").await;
        arm(&t.app, 100, CrashPoint::BeforeStore, Some("net/set")).await;

        store(&t.app, 1, "title", &t0, "Runs").await.unwrap();
        let report = tc_api::app::ClientError {
            request: rid(2),
            view: None,
            project: None,
            message: "x".to_owned(),
            at: tc_api::ids::WallMs(1),
        };
        assert!(matches!(answer(&t.app, &report).await, Answer::Reply { .. }));
        assert!(t.ender.ended().is_empty());

        let set = tc_api::net::Set { request: rid(3), on: true };
        assert_eq!(answer(&t.app, &set).await, Answer::Ended);
        assert_eq!(t.ender.ended(), vec![CrashPoint::BeforeStore]);
        assert!(!t.app.internet().is_on(), "the write did not run");
    }
}

/// A write that is not a hook, with nothing to store: the cheapest way to make a new write.
fn report(id: u8) -> tc_api::app::ClientError {
    tc_api::app::ClientError {
        request: rid(id),
        view: None,
        project: None,
        message: "x".to_owned(),
        at: tc_api::ids::WallMs(1),
    }
}

fn then_of(answer: Answer) -> Then {
    match answer {
        Answer::Reply { then, .. } => then,
        other => panic!("{other:?}"),
    }
}

multi! {
    async fn an_after_reply_crash_fires_once() {
        let (_dir, t) = started().await;
        arm(&t.app, 100, CrashPoint::AfterReply, None).await;
        assert_eq!(then_of(answer(&t.app, &report(1)).await), Then::EndProcess);
        assert_eq!(then_of(answer(&t.app, &report(2)).await), Then::Continue);
        // A retry of the first is a copy of its reply: it ends nothing.
        assert_eq!(then_of(answer(&t.app, &report(1)).await), Then::Continue);
    }
}

multi! {
    /// The retry of a write whose first attempt came before the arm is a copy of a reply that
    /// was sent long ago: it must neither end the process nor use up the arm that the next new
    /// write is waiting for (APP-DESIGN §5.8, plan D5). A4b's reload tests resend a request
    /// after arming a crash.
    async fn a_retry_never_takes_an_after_reply_crash() {
        let (_dir, t) = started().await;
        assert_eq!(then_of(answer(&t.app, &report(1)).await), Then::Continue);

        arm(&t.app, 100, CrashPoint::AfterReply, None).await;
        assert_eq!(
            then_of(answer(&t.app, &report(1)).await),
            Then::Continue,
            "the retry of a finished write is a copy"
        );
        assert_eq!(
            then_of(answer(&t.app, &report(2)).await),
            Then::EndProcess,
            "the arm waited for the next new write"
        );
    }
}

multi! {
    /// A store that waits behind a hold when the app ends is never written: no process that
    /// is gone writes a file.
    async fn a_store_held_when_tc4_ends_is_never_stored() {
        let (dir, t) = started().await;
        let t0 = field_token(&t.app, "title").await;
        let before = fields_file(&t.app_data);
        hold_saves(&t.app).await;
        let held = {
            let app = t.app.clone();
            let body = body_of(&store_req(1, "title", &t0, "Never"));
            tokio::spawn(async move { app.call("test/fieldStore", &body).await })
        };
        until_held_count(&t.app, 1).await;

        arm(&t.app, 100, CrashPoint::BeforeStore, Some("net/set")).await;
        let set = tc_api::net::Set { request: rid(3), on: true };
        assert_eq!(answer(&t.app, &set).await, Answer::Ended);

        // The end released the hold, and the dead check refused the write.
        assert_eq!(held.await.unwrap(), Answer::Ended);
        let after = restarted(&dir).await;
        assert!(field_log(&after.app, "title").await.stores.is_empty());
        assert_eq!(fields_file(&t.app_data), before, "the file is byte-identical");
    }
}

multi! {
    async fn a_disk_failure_is_not_saved_and_changes_nothing() {
        let (_dir, t) = started().await;
        let t0 = field_token(&t.app, "title").await;
        let folder = t.app_data.join("test-hooks");
        let file = folder.join("fields.json");
        let original = std::fs::read(&file).unwrap();
        // A non-empty folder in the file's place: the rename over it fails, for any user
        // (a permission would prove nothing as root).
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir(&file).unwrap();
        std::fs::write(file.join("keep"), b"x").unwrap();

        let refused = store(&t.app, 1, "title", &t0, "Text").await;
        assert!(
            matches!(refused, Err(ApiError::NotSaved(NotSaved::Disk(_)))),
            "{refused:?}"
        );
        assert!(field_log(&t.app, "title").await.stores.is_empty(), "memory is unchanged");
        let leftovers: Vec<_> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "no temporary file is left: {leftovers:?}");

        std::fs::remove_dir_all(&file).unwrap();
        std::fs::write(&file, &original).unwrap();
        let ack = store(&t.app, 2, "title", &t0, "Text").await.unwrap();
        assert_eq!(ack.view.get(), 1, "the same text stores as version 1");
    }
}

// --- No loss, no repeat -------------------------------------------------------------------

const TEXTS: [&str; 5] = ["", "a", "e\u{301}", "\u{e9}", "b"];
const FIELDS: [&str; 3] = ["one", "two", "three"];

#[derive(Debug, Clone)]
struct Op {
    field: usize,
    text: usize,
    /// How many versions back the token is from the current one (cut at the first).
    back: usize,
    /// Repeat an earlier call, byte for byte, instead of a new store.
    retry: Option<usize>,
}

fn ops() -> impl Strategy<Value = Vec<Op>> {
    proptest::collection::vec(
        (
            0..FIELDS.len(),
            0..TEXTS.len(),
            0usize..3,
            proptest::option::weighted(0.25, 0usize..16),
        )
            .prop_map(|(field, text, back, retry)| Op {
                field,
                text,
                back,
                retry,
            }),
        1..14,
    )
}

#[derive(Clone)]
struct Model {
    /// Per field: the stored texts with their base, and the token of every version.
    stores: Vec<Vec<(String, TokenBase)>>,
    tokens: Vec<Vec<ShownToken>>,
}

fn nfc(text: &str) -> String {
    text.nfc().collect()
}

async fn check_against(app: &App, model: &Model, what: &str) {
    for (index, name) in FIELDS.iter().enumerate() {
        let log = field_log(app, name).await;
        let stored: Vec<(String, TokenBase)> = log
            .stores
            .iter()
            .map(|s| (s.text.clone(), s.base))
            .collect();
        assert_eq!(stored, model.stores[index], "{what}: {name}");
        let value = model.stores[index]
            .last()
            .map(|(text, _)| text.clone())
            .unwrap_or_default();
        assert_eq!(log.current.value, value, "{what}: {name}");
        assert_eq!(
            Some(&log.current.token),
            model.tokens[index].last(),
            "{what}: {name}: the token of the current version"
        );
    }
}

async fn run_case(ops: Vec<Op>) {
    let dir = tempfile::tempdir().unwrap();
    let parts = Parts::new();
    let t = TestApp::start_with(dir.path(), &parts, |_, _| {}).await;
    let mut model = Model {
        stores: vec![Vec::new(); FIELDS.len()],
        tokens: Vec::new(),
    };
    for name in FIELDS {
        model.tokens.push(vec![field_token(&t.app, name).await]);
    }
    // Every call so far: its request, and the bytes it was answered with.
    let mut calls: Vec<(tc_api::test::FieldStore, Vec<u8>)> = Vec::new();
    let mut next_id = 0u64;

    for (n, op) in ops.iter().enumerate() {
        if let Some(earlier) = op.retry.filter(|i| *i < calls.len()) {
            let (request, first) = calls[earlier].clone();
            let again = t.app.call("test/fieldStore", &body_of(&request)).await;
            let Answer::Reply { body, .. } = again else {
                panic!("step {n}: {again:?}")
            };
            assert_eq!(body, first, "step {n}: a retry gets the first reply");
        } else {
            next_id += 1;
            let mut id = [0u8; 16];
            id[..8].copy_from_slice(&next_id.to_be_bytes());
            let version = model.stores[op.field].len();
            let named = version.saturating_sub(op.back);
            let token = model.tokens[op.field][named].clone();
            let mut request = store_req(0, FIELDS[op.field], &token, TEXTS[op.text]);
            request.request = tc_api::ids::RequestId::from_bytes(id);

            let sent = t.app.call("test/fieldStore", &body_of(&request)).await;
            let Reply::Ok(ack): Reply<tc_api::write::Ack> = reply_in(&sent) else {
                panic!("step {n}: {sent:?}")
            };
            let Answer::Reply { body, .. } = sent else {
                panic!("a reply was read from it")
            };
            calls.push((request, body));

            let text = nfc(TEXTS[op.text]);
            let named_value = if named == 0 {
                String::new()
            } else {
                model.stores[op.field][named - 1].0.clone()
            };
            if text == named_value {
                assert_eq!(ack.stored, Stored::Unchanged, "step {n}");
                assert_eq!(
                    ack.tokens[0].token, token,
                    "step {n}: the token it came with"
                );
            } else {
                assert_eq!(ack.stored, Stored::Written, "step {n}");
                let stale = named < version;
                assert_eq!(ack.new_conflicts.len(), usize::from(stale), "step {n}");
                model.stores[op.field].push((
                    text.clone(),
                    if stale {
                        TokenBase::Stale
                    } else {
                        TokenBase::Current
                    },
                ));
                model.tokens[op.field].push(ack.tokens[0].token.clone());
                assert_eq!(ack.saved[0].text, text, "step {n}");
            }
        }
        check_against(&t.app, &model, &format!("after step {n}")).await;
    }

    let again = TestApp::start_in(dir.path(), |_, _| {}).await;
    check_against(&again.app, &model, "after a restart").await;
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// The log never loses a text and never repeats one: a retry adds nothing, a text equal
    /// to the one the token names adds nothing, every other store adds exactly one line, and
    /// a store over an older version is marked `stale`, never silently `current`.
    #[test]
    fn field_store_never_loses_or_repeats_a_text(ops in ops()) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(run_case(ops));
    }
}

// --- Drafts -------------------------------------------------------------------------------

fn draft_req(id: u8, field: &str, token: &ShownToken, text: &str) -> FieldDraft {
    FieldDraft {
        request: rid(id),
        field: field.to_owned(),
        shown: token.clone(),
        text: text.to_owned(),
    }
}

multi! {
    async fn a_draft_is_kept_and_changes_no_value() {
        let (_dir, t) = started().await;
        let t0 = field_token(&t.app, "title").await;
        let t1 = store(&t.app, 1, "title", &t0, "Saved").await.unwrap().tokens[0].token.clone();

        post(&t.app, &draft_req(2, "title", &t1, "Saved and e\u{301}")).await.unwrap();
        let log = field_log(&t.app, "title").await;
        assert_eq!(log.current.value, "Saved");
        assert_eq!(log.current.token, t1, "the value and its token are unchanged");
        assert_eq!(log.stores.len(), 1);
        let draft = log.draft.unwrap();
        assert_eq!(draft.text, "Saved and \u{e9}", "kept in NFC");
        assert_eq!(draft.request, rid(2));
        assert_eq!(draft.base, TokenBase::Current);
        assert_eq!(log.drafts, 1);

        post(&t.app, &draft_req(3, "title", &t0, "Older base")).await.unwrap();
        let log = field_log(&t.app, "title").await;
        assert_eq!(log.draft.as_ref().unwrap().text, "Older base", "the newest replaces it");
        assert_eq!(log.draft.unwrap().base, TokenBase::Stale);
        assert_eq!(log.drafts, 2);
        assert_eq!(log.stores.len(), 1);
    }
}

multi! {
    async fn a_draft_with_a_forged_or_foreign_token_is_bad_token() {
        let (_dir, t) = started().await;
        let other_dir = tempfile::tempdir().unwrap();
        let other = TestApp::start_in(other_dir.path(), |_, _| {}).await;
        let of_another_field = field_token(&t.app, "other").await;
        let of_another_key = field_token(&other.app, "title").await;
        for (n, token) in [of_another_field, of_another_key].into_iter().enumerate() {
            let id = 10 + u8::try_from(n).unwrap();
            let refused = post(&t.app, &draft_req(id, "title", &token, "x")).await;
            assert_eq!(refused, Err(ApiError::NotSaved(NotSaved::BadToken)));
        }
        let log = field_log(&t.app, "title").await;
        assert!(log.draft.is_none());
        assert_eq!(log.drafts, 0);
    }
}

multi! {
    async fn a_draft_survives_a_restart() {
        let (dir, t) = started().await;
        let t0 = field_token(&t.app, "title").await;
        post(&t.app, &draft_req(1, "title", &t0, "Typed")).await.unwrap();
        let after = restarted(&dir).await;
        let log = field_log(&after.app, "title").await;
        assert_eq!(log.draft.unwrap().text, "Typed");
        assert_eq!(log.drafts, 1);
        assert_eq!(log.current.value, "");
    }
}

multi! {
    /// A draft waits at its hold before it takes the lane, so a held draft never delays a
    /// save (R22 asks the same of the real store); and the two writes of the one file never
    /// overwrite each other.
    async fn a_held_draft_does_not_hold_a_store() {
        let (_dir, t) = started().await;
        let t0 = field_token(&t.app, "title").await;
        post(&t.app, &Hold { request: rid(200), at: HoldPoint::DraftWrite }).await.unwrap();
        let draft = {
            let app = t.app.clone();
            let body = body_of(&draft_req(1, "title", &t0, "Draft"));
            tokio::spawn(async move { app.call("test/fieldDraft", &body).await })
        };
        until_held_count(&t.app, 1).await;

        let ack = tokio::time::timeout(
            Duration::from_secs(60),
            store(&t.app, 2, "title", &t0, "Saved"),
        )
        .await
        .expect("hung: the store waited behind the held draft")
        .unwrap();
        assert_eq!(ack.stored, Stored::Written);

        post(&t.app, &Release { request: rid(201), at: HoldPoint::DraftWrite }).await.unwrap();
        assert!(matches!(draft.await.unwrap(), Answer::Reply { .. }));
        let log = field_log(&t.app, "title").await;
        assert_eq!(texts(&log), vec![("Saved".to_owned(), TokenBase::Current)]);
        let kept = log.draft.unwrap();
        assert_eq!(kept.text, "Draft");
        assert_eq!(kept.base, TokenBase::Stale, "its token named the version before the store");
    }
}
