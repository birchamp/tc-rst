//! The UI's save machine against the real `test/fieldStore` (APP-DESIGN 6.5, A-2, A-3): a page
//! played in Rust executes the machine's commands as the browser glue does, against this app's
//! reply cache, holds and crash points, with reloads and crashes at every point.
//!
//! A reload drops the page and its calls in flight without stopping their work on the server,
//! as a browser reload does, and builds a new page on the same browser storage. A crash ends
//! the app at a crash point; a new app starts on the same folders, and the page comes back at a
//! new origin (a new port), with empty storage and the shell's mirror kept. The page then plays
//! the offer of the next open (6.6), and goes on.
//!
//! These tests use real time and a multi-thread runtime: the store writes its file on the
//! blocking pool. The page's own clock is virtual.

use std::collections::{BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use proptest::prelude::*;
use tc_api::buffer::{
    BufferEntry, BufferKey, BufferLine, FieldKey, FieldKind, FieldPlace, SentSave,
};
use tc_api::ids::{ProjectId, ProjectRef, RequestId, WallMs};
use tc_api::test::{
    Crash, CrashPoint, FieldDraft, FieldLogView, FieldStore, Hold, HoldPoint, Release, TokenBase,
};
use tc_api::write::Ack;
use tc_api::{ApiError, Reply, ShownToken};
use tc_app::{Answer, App, Then};
use tc_ui::app::buffer::{self, MemStore};
use tc_ui::app::client::{CallError, Transport};
use tc_ui::app::save::classify::{Answered, classify};
use tc_ui::app::save::{Command, FieldSave, FieldScope, Pair, ResendCheck, SaveMachine, Source};
use tokio::task::JoinHandle;

use super::common::{TestApp, field_log};

const FIELD: &str = "probe-a";
const TEXTS: [&str; 7] = [
    "",
    "a",
    "e\u{301}",
    "\u{e9}",
    "Paul, a servant",
    "\u{5d1}\u{5bc}\u{5b0}\u{5e8}\u{5b5}\u{5d0}\u{5e9}\u{5c1}\u{5b4}\u{596}\u{5d9}\u{5ea}",
    "\u{643}\u{64e}\u{62a}\u{64e}\u{628}\u{64e}",
];

fn key() -> BufferKey {
    BufferKey {
        project: ProjectRef::parse("test-fields").unwrap(),
        id: ProjectId::parse("00000000-0000-0000-0000-000000000000").unwrap(),
        field: FieldKey {
            kind: FieldKind::Test,
            place: FieldPlace::parse(FIELD).unwrap(),
        },
    }
}

/// Random ids for every page of the test binary: no page reuses another's.
fn fresh_id() -> RequestId {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut bytes = [0xa5u8; 16];
    bytes[8..].copy_from_slice(&NEXT.fetch_add(1, Ordering::Relaxed).to_be_bytes());
    RequestId::from_bytes(bytes)
}

fn machine(app: &App) -> SaveMachine {
    SaveMachine::new(app.instance(), Box::new(fresh_id))
}

struct Flight {
    sent: SentSave,
    handle: JoinHandle<Answer>,
}

/// What `then: EndProcess` left: the reply was sent and the process ends after it.
enum Delivered {
    Answered,
    /// The app is gone (a crash point fired, or `afterReply` ended it): start it again.
    Ended,
    /// The store is held: nothing came yet.
    Held,
}

struct Page {
    m: SaveMachine,
    store: MemStore,
    inflight: VecDeque<Flight>,
    editor: bool,
    focused: bool,
}

impl Page {
    fn new(app: &App, store: MemStore) -> Self {
        Self {
            m: machine(app),
            store,
            inflight: VecDeque::new(),
            editor: false,
            focused: false,
        }
    }
}

struct World {
    dir: tempfile::TempDir,
    t: TestApp,
    page: Page,
    /// The shell's mirror: every line, in order.
    mirror: Vec<String>,
    now: f64,
    last_typed: String,
    /// Each clean `Ack`'s text and the field's value at that moment.
    clean_checks: Vec<(String, String)>,
}

fn store_body(sent: &SentSave, base: &[ShownToken]) -> Vec<u8> {
    serde_json::to_vec(&FieldStore {
        request: sent.request,
        field: FIELD.to_owned(),
        shown: base.first().cloned().expect("a save names a token"),
        text: sent.text.clone(),
    })
    .unwrap()
}

impl World {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let t = TestApp::start_in(dir.path(), |_, _| {}).await;
        let page = Page::new(&t.app, MemStore::new());
        World {
            dir,
            t,
            page,
            mirror: Vec::new(),
            now: 0.0,
            last_typed: String::new(),
            clean_checks: Vec::new(),
        }
    }

    /// Carries the machine's commands out as `web::saves` does.
    async fn run(&mut self, commands: Vec<Command>) {
        for command in commands {
            match command {
                Command::Write { key, entry, mirror } => {
                    let entry = BufferEntry {
                        at: WallMs(self.now as i64),
                        ..entry
                    };
                    let written = buffer::write_entry(&mut self.page.store, &key, &entry);
                    if mirror && let Some(line) = written.line {
                        self.mirror.push(line);
                    }
                }
                Command::Remove {
                    key,
                    instance,
                    if_text,
                    mirror,
                } => {
                    let _ =
                        buffer::remove(&mut self.page.store, &key, if_text.as_deref(), instance);
                    if mirror {
                        self.mirror
                            .push(buffer::removed_line(&key, instance).unwrap());
                    }
                }
                Command::Send { sent, base, .. } => self.send(sent, &base),
                Command::Resend {
                    key,
                    sent,
                    base,
                    instance,
                } => {
                    // Only to the run the field sent to (D8): a run that started again has no
                    // reply cache.
                    match self.t.app.call("app/hello", b"{}").await {
                        Answer::Reply { body, .. } => {
                            let hello: Reply<tc_api::app::HelloView> =
                                serde_json::from_slice(&body).unwrap();
                            let Reply::Ok(hello) = hello else {
                                panic!("hello: {hello:?}")
                            };
                            match self.page.m.resend_check(
                                &key,
                                sent.request,
                                instance,
                                hello.instance,
                            ) {
                                ResendCheck::Send => self.send(sent, &base),
                                ResendCheck::Restarted(run) => {
                                    // The restart path; A5 then goes Home, which closes the
                                    // editor.
                                    self.page.m.restarted(run);
                                    let commands = self.page.m.close(&key, self.now);
                                    self.page.editor = false;
                                    self.page.focused = false;
                                    Box::pin(self.run(commands)).await;
                                }
                                ResendCheck::Nothing => {}
                            }
                        }
                        _ => {
                            let commands =
                                self.page
                                    .m
                                    .reply(&key, sent.request, Answered::Lost, self.now);
                            Box::pin(self.run(commands)).await;
                        }
                    }
                }
                Command::Draft { text, base, .. } => {
                    // Best effort, never awaited: a held store must not hold the page.
                    let body = serde_json::to_vec(&FieldDraft {
                        request: fresh_id(),
                        field: FIELD.to_owned(),
                        shown: base[0].clone(),
                        text,
                    })
                    .unwrap();
                    let app = self.t.app.clone();
                    drop(tokio::spawn(async move {
                        app.call("test/fieldDraft", &body).await
                    }));
                }
                Command::DiscardDraft { .. } | Command::Show { .. } | Command::ReadSessions => {}
            }
        }
    }

    fn send(&mut self, sent: SentSave, base: &[ShownToken]) {
        let app = self.t.app.clone();
        let body = store_body(&sent, base);
        let handle = tokio::spawn(async move { app.call("test/fieldStore", &body).await });
        self.page.inflight.push_back(Flight { sent, handle });
    }

    /// A request to the app, which may have ended in the background meanwhile: then the shell
    /// starts `tc4` again and loads a new page, and the request goes to the new app.
    async fn ask<E: tc_api::Endpoint>(&mut self, req: &E) -> E::Ok
    where
        E::Ok: std::fmt::Debug,
    {
        loop {
            match self.t.app.request(req).await {
                Ok(Reply::Ok(value)) => return value,
                Err(tc_app::CallFailure::Ended) => Box::pin(self.restart()).await,
                other => panic!("{}: {other:?}", E::PATH),
            }
        }
    }

    async fn log(&mut self) -> FieldLogView {
        self.ask(&tc_api::test::FieldLog {
            field: FIELD.to_owned(),
        })
        .await
    }

    async fn open_editor(&mut self) {
        if self.page.editor {
            return;
        }
        let log = self.log().await;
        let pair = Pair {
            value: log.current.value,
            token: log.current.token,
            view: None,
        };
        let (_, commands) = self.page.m.open(key(), FieldScope::App, pair, self.now);
        self.page.editor = true;
        self.run(commands).await;
    }

    async fn type_text(&mut self, text: &str) {
        self.open_editor().await;
        if !self.page.focused {
            self.page.focused = true;
            let commands = self.page.m.focus(&key(), self.now);
            self.run(commands).await;
        }
        // boot.js's capture listener, before the WebAssembly sees the keystroke, which then
        // leaves the write to it only if it says its item took the text.
        let template = self.page.m.template(&key()).unwrap();
        let input = buffer::input_to_entry(
            &mut self.page.store,
            &key(),
            text,
            WallMs(self.now as i64),
            &template,
        );
        if let Some(line) = input.line {
            self.mirror.push(line);
        }
        let source = if input.stored {
            Source::Input
        } else {
            Source::Program
        };
        let commands = self.page.m.typed(&key(), text, source, self.now);
        self.last_typed = text.to_owned();
        self.run(commands).await;
    }

    async fn blur(&mut self) {
        if self.page.focused {
            self.page.focused = false;
            let commands = self.page.m.blur(&key(), self.now);
            self.run(commands).await;
        }
    }

    async fn advance(&mut self, ms: f64) {
        let until = self.now + ms;
        let mut rounds = 0;
        while let Some(deadline) = self.page.m.next_deadline().filter(|d| *d <= until) {
            rounds += 1;
            assert!(
                rounds < 1000,
                "a deadline that never passes: {deadline} at {} in {:?}",
                self.now,
                self.page.m.state(&key())
            );
            self.now = deadline.max(self.now);
            let commands = self.page.m.tick(self.now);
            self.run(commands).await;
        }
        self.now = until;
        let commands = self.page.m.tick(self.now);
        self.run(commands).await;
    }

    /// The oldest call's answer, given to the machine. `keep_last` says whether an answer
    /// after which the process ended reached the page before it did.
    async fn deliver(&mut self, keep_last: bool) -> Delivered {
        let Some(flight) = self.page.inflight.pop_front() else {
            return Delivered::Answered;
        };
        for _ in 0..200 {
            if flight.handle.is_finished() || self.t.app.holding() > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        if !flight.handle.is_finished() && self.t.app.holding() > 0 {
            self.page.inflight.push_front(flight);
            return Delivered::Held;
        }
        let request = flight.sent.request;
        let answer = flight.handle.await.unwrap();
        let (reply, ended) = match answer {
            Answer::Reply { body, then } => {
                let reply: Reply<Ack> = serde_json::from_slice(&body).unwrap();
                (
                    Some(reply.into_result().map_err(CallError::Api)),
                    then == Then::EndProcess,
                )
            }
            Answer::Ended => (None, true),
            Answer::NotFound => panic!("test/fieldStore is not routed"),
        };
        if let Some(reply) = reply
            && (!ended || keep_last)
        {
            let commands = self
                .page
                .m
                .reply(&key(), request, classify(reply), self.now);
            self.run(commands).await;
            if matches!(self.page.m.state(&key()), Some(FieldSave::Clean)) {
                let text = buffer::nfc(self.page.m.text(&key()).unwrap());
                // The value at this moment; an app that ended meanwhile says nothing about it.
                if let Ok(Reply::Ok(log)) = self
                    .t
                    .app
                    .request(&tc_api::test::FieldLog {
                        field: FIELD.to_owned(),
                    })
                    .await
                {
                    self.clean_checks.push((text, log.current.value));
                }
            }
        }
        if ended {
            Delivered::Ended
        } else {
            Delivered::Answered
        }
    }

    /// The oldest call's reply is lost; its work on the server goes on.
    async fn cut(&mut self) {
        if let Some(flight) = self.page.inflight.pop_front() {
            drop(flight.handle);
            let commands = self
                .page
                .m
                .reply(&key(), flight.sent.request, Answered::Lost, self.now);
            self.run(commands).await;
        }
    }

    /// The page reloads: its calls in flight are dropped without stopping their work, and a new
    /// page restores the entries of this run from the same storage (AD22).
    async fn reload(&mut self) {
        let store = std::mem::take(&mut self.page.store);
        let old = std::mem::replace(&mut self.page, Page::new(&self.t.app, store));
        drop(old.inflight);
        for (k, entry) in buffer::entries(&self.page.store) {
            if entry.instance == self.page.m.instance() {
                let commands = self.page.m.restore(k, entry, FieldScope::App, self.now);
                self.run(commands).await;
            }
        }
    }

    async fn hold(&mut self) {
        self.ask(&Hold {
            request: fresh_id(),
            at: HoldPoint::Save,
        })
        .await;
    }

    async fn release(&mut self) {
        self.ask(&Release {
            request: fresh_id(),
            at: HoldPoint::Save,
        })
        .await;
    }

    async fn arm(&mut self, at: CrashPoint) {
        self.ask(&Crash {
            request: fresh_id(),
            at,
            path: Some("test/fieldStore".to_owned()),
        })
        .await;
    }

    /// A crash point may have ended the app in the background, in a store whose reply the page
    /// has not read yet. The shell then starts `tc4` again and loads a new page (AD39).
    async fn keep_alive(&mut self) {
        if self.t.app.call("test/quiet", b"{}").await == Answer::Ended {
            self.restart().await;
        }
    }

    /// `tc4` ended: a new one starts on the same folders, the page comes back at a new origin
    /// (empty storage, the mirror kept), and the next open offers what is unsaved (6.6).
    async fn restart(&mut self) {
        if self.t.app.call("test/quiet", b"{}").await != Answer::Ended {
            // `afterReply`: the process ended right after its reply. Nothing else of it was
            // running (one save of the field at a time); let what was started finish.
            let _ = self
                .t
                .app
                .request(&Release {
                    request: fresh_id(),
                    at: HoldPoint::Save,
                })
                .await;
            // A crash point may end it while this waits: an ended app runs nothing either.
            for _ in 0..5000 {
                match self.t.app.request(&tc_api::test::Quiet {}).await {
                    Ok(Reply::Ok(quiet)) if quiet.busy => {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                    _ => break,
                }
            }
        }
        self.t = TestApp::start_in(self.dir.path(), |_, _| {}).await;
        self.page = Page::new(&self.t.app, MemStore::new());
        self.offer().await;
    }

    /// The offer of the next open, as the design defines it (6.6, STORAGE §5.14): the newest
    /// mirrored entry of the field (the shell's lines, reduced as `takeBuffered` does); dropped
    /// when the stored value already has its text; else Keep stores it, over the stored value's
    /// token when the stored value is the text of the save the entry was waiting for (that save
    /// landed before the crash, and the newer text continues from it), and over the entry's own
    /// token otherwise, so a change made since becomes a conflict.
    async fn offer(&mut self) {
        let newest: Option<BufferEntry> =
            buffer::merge(Vec::new(), &buffer::current_lines(&self.mirror))
                .into_iter()
                .filter(|(k, _)| *k == key())
                .map(|(_, entry)| entry)
                .max_by_key(|entry| entry.at);
        let Some(entry) = newest else {
            return;
        };
        let log = self.log().await;
        let stored = log.current.value;
        if buffer::same_text(&entry.text, &stored) {
            self.mirror
                .push(buffer::removed_line(&key(), entry.instance).unwrap());
            return;
        }
        let landed = entry
            .sent
            .as_ref()
            .is_some_and(|sent| buffer::same_text(&sent.text, &stored));
        let base = if landed {
            log.current.token
        } else {
            entry.base[0].clone()
        };
        let keep = FieldStore {
            request: fresh_id(),
            field: FIELD.to_owned(),
            shown: base,
            text: entry.text.clone(),
        };
        self.ask(&keep).await;
        self.mirror
            .push(buffer::removed_line(&key(), entry.instance).unwrap());
    }

    /// Everything answered and saved, with nothing held.
    async fn settle(&mut self) {
        for _ in 0..40 {
            self.keep_alive().await;
            while self.t.app.holding() > 0 {
                self.release().await;
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            let mut held = 0;
            while !self.page.inflight.is_empty() {
                match self.deliver(true).await {
                    Delivered::Ended => self.restart().await,
                    Delivered::Held => {
                        held += 1;
                        assert!(held < 100, "a store stays held");
                        self.release().await;
                    }
                    Delivered::Answered => {}
                }
            }
            self.blur().await;
            let commands = self.page.m.retry(self.now);
            self.run(commands).await;
            self.advance(3000.0).await;
            let saved = matches!(self.page.m.state(&key()), Some(FieldSave::Clean) | None);
            if saved && self.page.inflight.is_empty() {
                break;
            }
        }
        super::common::until_quiet(&self.t.app).await;
    }

    /// The row's assertions: the last text stored, once; no request id twice; no store over an
    /// older version; no earlier text marked clean in its place; every mirror line readable.
    async fn check(&mut self) {
        let state = self.page.m.state(&key()).cloned();
        assert!(
            matches!(state, Some(FieldSave::Clean) | None),
            "the field ends saved: {state:?}"
        );
        let log = self.log().await;
        let last = buffer::nfc(&self.last_typed);
        assert_eq!(log.current.value, last, "the last text is stored");
        let ids: BTreeSet<RequestId> = log.stores.iter().map(|s| s.request).collect();
        assert_eq!(ids.len(), log.stores.len(), "a request id stored twice");
        assert!(
            log.stores.iter().all(|s| s.base == TokenBase::Current),
            "a store over an older version (a conflict with the person's own save): {:?}",
            log.stores
                .iter()
                .map(|s| (&s.text, s.base))
                .collect::<Vec<_>>()
        );
        if let Some(stored) = log.stores.last() {
            assert_eq!(stored.text, last);
        }
        for pair in log.stores.windows(2) {
            assert_ne!(pair[0].text, pair[1].text, "a text stored twice in a row");
        }
        for (text, value) in &self.clean_checks {
            assert_eq!(
                text, value,
                "a field was marked clean on text that was not the stored value"
            );
        }
        for line in &self.mirror {
            serde_json::from_str::<BufferLine>(line).unwrap_or_else(|e| panic!("{line}: {e}"));
        }
    }
}

macro_rules! multi {
    ($(#[$meta:meta])* async fn $name:ident() $body:block) => {
        $(#[$meta])*
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn $name() $body
    };
}

multi! {
    async fn a_reload_while_the_save_is_held_stores_once() {
        let mut w = World::new().await;
        w.hold().await;
        w.type_text("T1").await;
        w.blur().await;
        super::common::until_held_count(&w.t.app, 1).await;
        w.reload().await;
        w.release().await;
        w.settle().await;
        w.check().await;
        assert_eq!(w.log().await.stores.len(), 1);
    }
}

multi! {
    /// RT-2, case 1: T1 is sent, the person types T2, the page reloads. The retry is T1's own
    /// request, and T2 is saved after it, over T1's token.
    async fn a_reload_after_newer_typing_stores_both_in_order() {
        let mut w = World::new().await;
        w.hold().await;
        w.type_text("T1").await;
        w.blur().await;
        w.type_text("T1 T2").await;
        super::common::until_held_count(&w.t.app, 1).await;
        w.reload().await;
        w.release().await;
        w.settle().await;
        w.check().await;
        let log = w.log().await;
        let texts: Vec<_> = log.stores.iter().map(|s| (s.text.as_str(), s.base)).collect();
        assert_eq!(texts, [("T1", TokenBase::Current), ("T1 T2", TokenBase::Current)]);
    }
}

multi! {
    /// RT-2, case 2: the retry of a save that still waits on the store waits for it, and the
    /// text is stored once.
    async fn a_retry_of_a_save_still_running_waits_and_stores_once() {
        let mut w = World::new().await;
        w.hold().await;
        w.type_text("Saved").await;
        w.blur().await;
        super::common::until_held_count(&w.t.app, 1).await;
        w.cut().await;
        w.advance(250.0).await;
        assert_eq!(w.page.inflight.len(), 1, "the lost reply is sent again");
        w.release().await;
        w.settle().await;
        w.check().await;
        assert_eq!(w.log().await.stores.len(), 1);
    }
}

multi! {
    async fn crash_before_store_then_restart_offers_the_text_and_keep_stores_it_once() {
        let mut w = World::new().await;
        w.type_text("Typed before the crash").await;
        w.arm(CrashPoint::BeforeStore).await;
        w.blur().await;
        assert!(matches!(w.deliver(true).await, Delivered::Ended));
        assert!(w.log_of_new_app_is_empty().await);
        w.restart().await;
        w.settle().await;
        w.check().await;
        assert_eq!(w.log().await.stores.len(), 1);
    }
}

multi! {
    async fn crash_before_reply_then_restart_offers_nothing_and_resends_nothing() {
        let mut w = World::new().await;
        w.type_text("Stored, reply lost").await;
        w.arm(CrashPoint::BeforeReply).await;
        w.blur().await;
        assert!(matches!(w.deliver(true).await, Delivered::Ended));
        w.restart().await;
        w.settle().await;
        w.advance(10_000.0).await;
        w.check().await;
        assert_eq!(w.log().await.stores.len(), 1);
    }
}

multi! {
    async fn crash_after_reply_without_the_reply_resends_nothing() {
        let mut w = World::new().await;
        w.type_text("Answered, never read").await;
        w.arm(CrashPoint::AfterReply).await;
        w.blur().await;
        assert!(matches!(w.deliver(false).await, Delivered::Ended));
        w.restart().await;
        w.settle().await;
        w.check().await;
        assert_eq!(w.log().await.stores.len(), 1);
    }
}

multi! {
    /// The offer's rule for an entry whose save landed before the crash: the newer text it
    /// holds continues from the stored text, so it is stored over the current token, never as a
    /// conflict with the person's own save.
    async fn typing_after_a_save_that_landed_then_a_crash_keeps_both_without_a_conflict() {
        let mut w = World::new().await;
        w.type_text("T1").await;
        w.arm(CrashPoint::BeforeReply).await;
        w.blur().await;
        w.type_text("T1 T2").await;
        assert!(matches!(w.deliver(true).await, Delivered::Ended));
        w.restart().await;
        w.settle().await;
        w.check().await;
        let texts: Vec<_> = w.log().await.stores.into_iter().map(|s| s.text).collect();
        assert_eq!(texts, ["T1", "T1 T2"]);
    }
}

multi! {
    /// A restarted `tc4` that got the old port back: the page's storage holds an entry of the
    /// old run with a save in flight. The new page restores nothing of it and sends nothing.
    async fn an_entry_of_another_run_is_never_resent() {
        let mut w = World::new().await;
        w.type_text("Stored once").await;
        w.arm(CrashPoint::BeforeReply).await;
        w.blur().await;
        assert!(matches!(w.deliver(true).await, Delivered::Ended));
        let kept = std::mem::take(&mut w.page.store);
        assert!(buffer::read(&kept, &key()).unwrap().sent.is_some());
        w.t = TestApp::start_in(w.dir.path(), |_, _| {}).await;
        w.page = Page::new(&w.t.app, kept.clone());
        w.reload().await;
        w.advance(10_000.0).await;
        assert!(w.page.inflight.is_empty(), "nothing restored, nothing sent");
        assert_eq!(w.log().await.stores.len(), 1);
        assert_eq!(w.page.store.items, kept.items, "the entry stays for the offer");
    }
}

multi! {
    async fn drafts_reach_the_server_and_change_no_value() {
        let mut w = World::new().await;
        w.type_text("A draft").await;
        w.advance(1000.0).await;
        for _ in 0..1000 {
            if w.log().await.drafts == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let log = w.log().await;
        assert_eq!(log.drafts, 1);
        assert_eq!(log.draft.unwrap().text, "A draft");
        assert_eq!(log.current.value, "");
        assert!(log.stores.is_empty());
    }
}

impl World {
    async fn log_of_new_app_is_empty(&self) -> bool {
        let after = TestApp::start_in(self.dir.path(), |_, _| {}).await;
        field_log(&after.app, FIELD).await.stores.is_empty()
    }
}

// --- Reloads and crashes at every point ---------------------------------------------------

#[derive(Debug, Clone)]
enum Step {
    Type(usize),
    Blur,
    Advance(u16),
    Deliver(bool),
    Cut,
    Reload,
    Hold,
    Release,
    Crash(CrashPoint),
}

fn reload_steps() -> impl Strategy<Value = Vec<Step>> {
    let step = prop_oneof![
        5 => (0..TEXTS.len()).prop_map(Step::Type),
        2 => Just(Step::Blur),
        3 => (0u16..3000).prop_map(Step::Advance),
        4 => any::<bool>().prop_map(Step::Deliver),
        1 => Just(Step::Cut),
        2 => Just(Step::Reload),
        1 => Just(Step::Hold),
        1 => Just(Step::Release),
    ];
    proptest::collection::vec(step, 1..24)
}

fn crash_steps() -> impl Strategy<Value = Vec<Step>> {
    let point = prop_oneof![
        Just(CrashPoint::BeforeStore),
        Just(CrashPoint::BeforeReply),
        Just(CrashPoint::AfterReply),
    ];
    let step = prop_oneof![
        5 => (0..TEXTS.len()).prop_map(Step::Type),
        2 => Just(Step::Blur),
        3 => (0u16..3000).prop_map(Step::Advance),
        4 => any::<bool>().prop_map(Step::Deliver),
        1 => Just(Step::Cut),
        2 => Just(Step::Reload),
        1 => Just(Step::Hold),
        1 => Just(Step::Release),
        2 => point.prop_map(Step::Crash),
    ];
    proptest::collection::vec(step, 1..24)
}

async fn play(steps: Vec<Step>) {
    let mut w = World::new().await;
    for step in steps {
        w.keep_alive().await;
        match step {
            Step::Type(i) => w.type_text(TEXTS[i]).await,
            Step::Blur => w.blur().await,
            Step::Advance(ms) => w.advance(f64::from(ms)).await,
            Step::Deliver(keep) => {
                if let Delivered::Ended = w.deliver(keep).await {
                    w.restart().await;
                }
            }
            Step::Cut => w.cut().await,
            Step::Reload => w.reload().await,
            Step::Hold => w.hold().await,
            Step::Release => w.release().await,
            Step::Crash(at) => w.arm(at).await,
        }
    }
    w.settle().await;
    w.check().await;
}

fn block_on(steps: Vec<Step>) {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(play(steps));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// The row's property (6.5, A-2): reloads at every point of typing, saves, replies, lost
    /// replies and held stores. The last text is stored exactly once, no request id twice,
    /// nothing over an older version, and no earlier text is marked clean in its place.
    #[test]
    fn reload_anywhere_stores_the_last_text_once(steps in reload_steps()) {
        block_on(steps);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// The same with `test/crash` at every point of a store (before it, before its reply,
    /// after its reply): a new `tc4`, a page at a new origin, and the offer of the next open.
    #[test]
    fn crash_anywhere_stores_the_last_text_once(steps in crash_steps()) {
        block_on(steps);
    }
}

#[test]
fn a_status_503_is_a_lost_reply() {
    // A process that ended answers 503 through tc-server: the same request goes again.
    assert_eq!(
        classify(Err(CallError::Transport(Transport::Status(503)))),
        Answered::Lost
    );
    assert!(matches!(
        classify(Err(CallError::Api(ApiError::RequestReused))),
        Answered::Reissue
    ));
}
