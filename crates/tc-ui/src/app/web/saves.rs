//! The save machine in the browser (APP-DESIGN 6.5, 6.6): glue that carries out what
//! [`crate::app::save`] decides. It writes the draft buffer in `localStorage` and its lines to
//! the shell's mirror, sends saves through each kind's [`FieldWriter`], resends a lost reply
//! only after `app/hello` names the run the field sent to, keeps one timer at the machine's next
//! deadline, keeps `boot.js`'s capture listener on the focused editor with the entry it is to
//! write, and keeps the indicator, the close guard and `window.__tcIdle` current.
//!
//! The machine's `RefCell` is never borrowed across an `.await` or a call out: every method
//! takes the machine's commands first and runs them after the borrow has ended.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::rc::{Rc, Weak};

use js_sys::{Function, Promise, Reflect};
use leptos::prelude::*;
use tc_api::align::AlignSave;
use tc_api::app::{Hello, HelloView, OpenSession};
use tc_api::buffer::{BufferEntry, BufferKey, DraftCall, FieldKind, SentSave};
use tc_api::event::{CheckpointOutcome, SessionMode};
use tc_api::ids::{ProjectId, ProjectRef, RequestId, Scope, ServerInstance, ViewSeq, WallMs};
use tc_api::shown::PersonLabel;
use tc_api::token::ShownToken;
use tc_api::write::Ack;
use tc_api::{ApiError, error::Feature};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};

use super::api::{Api, Busy};
use super::js::{self, LocalStore};
use super::{ServerState, bridge};
use crate::app::buffer::{self, StoreError};
use crate::app::client::CallError;
use crate::app::save::classify::{Answered, classify};
use crate::app::save::indicator::{self, Indicator, Worst};
use crate::app::save::{
    Command, FieldSave, FieldScope, GateState, Notice, Pair, SaveMachine, Source,
};

/// A future that stays on the page's one thread.
pub type LocalBoxFuture<T> = Pin<Box<dyn Future<Output = T>>>;

/// How one kind of field is saved: each screen's unit registers the writer of its fields.
pub trait FieldWriter {
    /// The save request for `sent` over `base`. The future owns what it needs.
    fn save(
        &self,
        api: &Api,
        key: &BufferKey,
        sent: &SentSave,
        base: &[ShownToken],
    ) -> LocalBoxFuture<Result<Ack, CallError>>;
    /// The draft request, if the kind has a server draft.
    fn draft(
        &self,
        api: &Api,
        key: &BufferKey,
        text: &str,
        base: &[ShownToken],
    ) -> Option<LocalBoxFuture<Result<(), CallError>>>;
    /// How `boot.js` posts the field's draft after three panics (6.8), if it has one.
    fn draft_call(&self, key: &BufferKey, base: &[ShownToken]) -> Option<DraftCall>;
}

/// A hook the navigation gate runs once every field is saved (B6 registers `align/flush`).
pub type GateHook = Rc<dyn Fn() -> LocalBoxFuture<bool>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateHookId(u64);

/// Proof that the open is writable (6.4): editors take `Option<WriteCap>`, and their edit
/// controls are off without one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteCap(());

struct Inner {
    api: Api,
    state: ServerState,
    busy: Rc<Busy>,
    machine: RefCell<SaveMachine>,
    writers: RefCell<HashMap<FieldKind, Rc<dyn FieldWriter>>>,
    shown: RefCell<HashMap<BufferKey, ArcRwSignal<String>>>,
    /// The focused editor of each field and the template `boot.js` holds for it.
    tracked: RefCell<HashMap<BufferKey, (web_sys::HtmlElement, String)>>,
    /// Fields whose item holds another run's entry: never written over (6.6).
    foreign: RefCell<HashSet<BufferKey>>,
    timer: Cell<Option<i32>>,
    tick: RefCell<Option<Closure<dyn FnMut()>>>,
    indicator: ArcRwSignal<Indicator>,
    can_close: Cell<Option<bool>>,
    hooks: RefCell<Vec<(GateHookId, GateHook)>>,
    next_hook: Cell<u64>,
    /// Resolvers of the promises [`Saves::changed`] hands out.
    waiters: RefCell<Vec<Function>>,
    buffer_failure_reported: Cell<bool>,
    /// `boot.js` and its capture listener are on the page. Without them the machine writes the
    /// entry of every keystroke itself.
    boot: bool,
}

/// The page's saves. Cheap to clone.
#[derive(Clone)]
pub struct Saves(Rc<Inner>);

impl Saves {
    pub(super) fn new(api: Api, state: ServerState, busy: Rc<Busy>) -> Self {
        let instance = state.instance.get_untracked();
        let inner = Rc::new_cyclic(|weak: &Weak<Inner>| {
            let weak = weak.clone();
            let tick = Closure::<dyn FnMut()>::new(move || {
                if let Some(inner) = weak.upgrade() {
                    let saves = Saves(inner);
                    saves.0.timer.set(None);
                    let commands = saves.machine(|m| m.tick(js::now_ms()));
                    saves.run(commands);
                }
            });
            Inner {
                api,
                state,
                busy,
                machine: RefCell::new(SaveMachine::new(instance, Box::new(js::random_request_id))),
                writers: RefCell::new(HashMap::new()),
                shown: RefCell::new(HashMap::new()),
                tracked: RefCell::new(HashMap::new()),
                foreign: RefCell::new(HashSet::new()),
                timer: Cell::new(None),
                tick: RefCell::new(Some(tick)),
                indicator: ArcRwSignal::new(Indicator {
                    state: Worst::Saved,
                    commit_error: false,
                }),
                can_close: Cell::new(None),
                hooks: RefCell::new(Vec::new()),
                next_hook: Cell::new(0),
                waiters: RefCell::new(Vec::new()),
                buffer_failure_reported: Cell::new(false),
                boot: js::window().is_ok_and(|window| {
                    Reflect::get(&window, &JsValue::from_str("tc4Boot"))
                        .is_ok_and(|boot| boot.is_object())
                }),
            }
        });
        Saves(inner)
    }

    /// Runs `f` on the machine and returns its commands; none if the machine is borrowed,
    /// which the rule of this module never lets happen.
    fn machine<R: Default>(&self, f: impl FnOnce(&mut SaveMachine) -> R) -> R {
        match self.0.machine.try_borrow_mut() {
            Ok(mut machine) => f(&mut machine),
            Err(_) => {
                web_sys::console::error_1(&JsValue::from_str(
                    "tc4: the save machine was borrowed twice",
                ));
                R::default()
            }
        }
    }

    pub fn register(&self, kind: FieldKind, writer: Rc<dyn FieldWriter>) {
        self.0.writers.borrow_mut().insert(kind, writer);
    }

    fn writer(&self, kind: FieldKind) -> Option<Rc<dyn FieldWriter>> {
        self.0.writers.borrow().get(&kind).cloned()
    }

    /// An editor mounts on a field: the signal is the text it shows, which the machine changes
    /// only while the editor is not focused (AD37).
    pub fn open(&self, key: &BufferKey, scope: FieldScope, pair: Pair) -> ArcRwSignal<String> {
        let (text, commands) = self.machine(|m| m.open(key.clone(), scope, pair, js::now_ms()));
        let shown = ArcRwSignal::new(text);
        self.0.shown.borrow_mut().insert(key.clone(), shown.clone());
        self.run(commands);
        shown
    }

    pub fn typed(&self, key: &BufferKey, text: &str, source: Source) {
        let source = if self.0.boot { source } else { Source::Program };
        let commands = self.machine(|m| m.typed(key, text, source, js::now_ms()));
        self.run(commands);
    }

    pub fn composition(&self, key: &BufferKey) {
        self.machine(|m| m.composition(key));
    }

    /// The editor took focus: `boot.js` now writes each keystroke into the field's entry
    /// before the WebAssembly sees it (6.6, A-15).
    pub fn focus(&self, key: &BufferKey, element: &web_sys::HtmlElement) {
        let commands = self.machine(|m| m.focus(key, js::now_ms()));
        if !self.0.foreign.borrow().contains(key) {
            self.0
                .tracked
                .borrow_mut()
                .insert(key.clone(), (element.clone(), String::new()));
        }
        self.run(commands);
    }

    pub fn blur(&self, key: &BufferKey) {
        self.untrack(key);
        let commands = self.machine(|m| m.blur(key, js::now_ms()));
        self.run(commands);
    }

    /// The Save button, or Done.
    pub fn save(&self, key: &BufferKey) {
        let commands = self.machine(|m| m.save(key, js::now_ms()));
        self.run(commands);
    }

    /// Cancel of a section card.
    pub fn discard(&self, key: &BufferKey) {
        self.untrack(key);
        let commands = self.machine(|m| m.discard(key));
        self.0.shown.borrow_mut().remove(key);
        self.run(commands);
    }

    /// The editor unmounts.
    pub fn close(&self, key: &BufferKey) {
        self.untrack(key);
        let commands = self.machine(|m| m.close(key, js::now_ms()));
        self.0.shown.borrow_mut().remove(key);
        self.run(commands);
    }

    /// A refresh of a field's value under its editor (AD37, 6.3).
    pub fn refresh(&self, key: &BufferKey, pair: Pair, by: Option<PersonLabel>) {
        let commands = self.machine(|m| m.refresh(key, pair, by));
        self.run(commands);
    }

    /// The Retry button.
    pub fn retry(&self) {
        let commands = self.machine(|m| m.retry(js::now_ms()));
        self.run(commands);
    }

    pub fn state(&self, key: &BufferKey) -> Option<FieldSave> {
        self.machine(|m| m.state(key).cloned())
    }

    pub fn notice(&self, key: &BufferKey) -> Option<Notice> {
        self.machine(|m| m.notice(key).cloned())
    }

    pub fn indicator(&self) -> ArcRwSignal<Indicator> {
        self.0.indicator.clone()
    }

    /// Everything saved, aligners included; a failed checkpoint does not count (6.5).
    pub fn all_clean(&self) -> bool {
        let worst = self.machine(|m| Some(m.worst())).unwrap_or(Worst::Error);
        indicator::all_clean(worst, &self.align_saves())
    }

    /// `Some` only while the open `at` is writable (6.4).
    pub fn write_cap(&self, at: &Scope) -> Option<WriteCap> {
        self.0.state.sessions.with_untracked(|sessions| {
            sessions
                .iter()
                .any(|s| s.at == *at && s.mode == SessionMode::Writable)
                .then_some(WriteCap(()))
        })
    }

    pub fn add_gate_hook(&self, hook: GateHook) -> GateHookId {
        let id = GateHookId(self.0.next_hook.get());
        self.0.next_hook.set(id.0 + 1);
        self.0.hooks.borrow_mut().push((id, hook));
        id
    }

    pub fn remove_gate_hook(&self, id: GateHookId) {
        self.0.hooks.borrow_mut().retain(|(held, _)| *held != id);
    }

    /// The navigation gate (6.5): every field that may save is saved, each standing failure
    /// retried once, the hooks run, and again while new work appeared. False while a failure
    /// stands: the person stays where they are (FR-32).
    pub async fn gate(&self) -> bool {
        self.machine(|m| m.begin_gate());
        let passed = loop {
            let commands = self.machine(|m| m.flush_for_gate(js::now_ms()));
            let sent = !commands.is_empty();
            self.run(commands);
            match self.machine(|m| Some(m.gate())) {
                Some(GateState::Blocked) | None => break false,
                Some(GateState::Again) if sent => {}
                Some(GateState::Again | GateState::Waiting) => self.changed().await,
                Some(GateState::Pass) => {
                    let hooks: Vec<GateHook> = self
                        .0
                        .hooks
                        .borrow()
                        .iter()
                        .map(|(_, hook)| Rc::clone(hook))
                        .collect();
                    let mut flushed = true;
                    for hook in hooks {
                        if !hook().await {
                            flushed = false;
                            break;
                        }
                    }
                    if !flushed {
                        break false;
                    }
                    if self.machine(|m| Some(m.gate())) == Some(GateState::Pass) {
                        break true;
                    }
                }
            }
        };
        self.machine(|m| m.end_gate());
        self.after();
        passed
    }

    /// The entries of a folder with its project id, from this page's storage and the shell's
    /// mirror, newest per field: what `session/open { buffered }` carries (6.6, B1b, C1b).
    pub async fn buffered_for(
        &self,
        project: &ProjectRef,
        id: &ProjectId,
    ) -> Vec<(BufferKey, BufferEntry)> {
        let local = buffer::entries_of(&LocalStore::open(), project, id);
        let mirror = bridge::take_buffered().await;
        buffer::merge(local, &mirror)
            .into_iter()
            .filter(|(key, _)| key.project == *project && key.id == *id)
            .collect()
    }

    /// The copy of a read-only project takes its folder's entries (A-10).
    pub fn rekey(
        &self,
        from: (&ProjectRef, &ProjectId),
        to: (&ProjectRef, &ProjectId),
    ) -> Result<usize, StoreError> {
        buffer::rekey(&mut LocalStore::open(), from, to)
    }

    /// After a reload in the same run (AD22, 6.6): each entry of this run whose project is open
    /// (or that needs no open) becomes a field again, and a save that was in flight is sent
    /// again with its own id and body. Every other entry waits for the offer at its project's
    /// next open. The writers must be registered first.
    pub(super) fn restore(&self, hello: &HelloView) {
        let now = js::now_ms();
        for (key, entry) in buffer::entries(&LocalStore::open()) {
            if entry.instance != hello.instance || self.writer(key.field.kind).is_none() {
                continue;
            }
            let scope = if key.field.kind == FieldKind::Test {
                FieldScope::App
            } else {
                match hello
                    .sessions
                    .iter()
                    .find(|s| s.at.project == key.project && s.id == key.id)
                {
                    Some(session) => FieldScope::Project(Some(session.at.clone())),
                    None => continue,
                }
            };
            let commands = self.machine(|m| m.restore(key, entry, scope, now));
            self.run(commands);
        }
        self.sessions(&hello.sessions);
    }

    pub(super) fn sessions(&self, open: &[OpenSession]) {
        let commands = self.machine(|m| m.sessions(open, js::now_ms()));
        self.run(commands);
    }

    /// The server started again (AD4): every field is dropped and nothing is sent.
    pub(super) fn restarted(&self, instance: ServerInstance) {
        let keys: Vec<BufferKey> = self.0.tracked.borrow().keys().cloned().collect();
        for key in keys {
            self.untrack(&key);
        }
        self.0.foreign.borrow_mut().clear();
        self.machine(|m| m.restarted(instance));
        self.after();
    }

    pub(super) fn local_change(&self, request: RequestId, view: ViewSeq) {
        self.machine(|m| m.local_change(request, view));
    }

    /// The close guard (6.5): with the shell, `setCanClose` on every change and the gate on
    /// a close request; in a browser, `boot.js`'s `beforeunload` asks while anything is unsaved
    /// (`tc4Boot.setUnsaved`), never under the shell, where it would refuse the close silently
    /// (tC4 #356). The listener is `boot.js`'s, not the WebAssembly's, so the reload after a panic
    /// is never held by a question: the buffer brings the text back.
    pub(super) fn guard_close(&self) {
        if bridge::has_bridge() {
            let saves = self.clone();
            bridge::on_close_requested(move || {
                let saves = saves.clone();
                wasm_bindgen_futures::future_to_promise(async move {
                    Ok(JsValue::from_bool(saves.gate().await))
                })
            });
        }
        self.after();
    }

    /// The aligners' and the checkpoints' states changed: the indicator follows.
    pub(super) fn server_states_changed(&self) {
        self.after();
    }

    fn align_saves(&self) -> Vec<AlignSave> {
        self.0
            .state
            .align_saves
            .with_untracked(|saves| saves.values().cloned().collect())
    }

    fn checkpoint_failed(&self) -> bool {
        self.0.state.checkpoints.with_untracked(|outcomes| {
            outcomes
                .values()
                .any(|outcome| matches!(outcome, CheckpointOutcome::Failed { .. }))
        })
    }

    /// A promise that resolves at the next change of any field.
    async fn changed(&self) {
        let waiters = Rc::clone(&self.0);
        let promise = Promise::new(&mut |resolve, _| {
            waiters.waiters.borrow_mut().push(resolve);
        });
        let _ = JsFuture::from(promise).await;
    }

    fn untrack(&self, key: &BufferKey) {
        if let Some((element, _)) = self.0.tracked.borrow_mut().remove(key) {
            boot_call("untrack", &[element.into()]);
        }
    }

    /// Carries out the machine's commands, then brings everything that follows the fields up
    /// to date.
    fn run(&self, commands: Vec<Command>) {
        let instance = self.machine(|m| Some(m.instance()));
        for command in commands {
            match command {
                Command::Write { key, entry, mirror } => self.write(&key, entry, mirror),
                Command::Remove {
                    key,
                    if_text,
                    mirror,
                } => {
                    if let Some(instance) = instance {
                        let _ = buffer::remove(
                            &mut LocalStore::open(),
                            &key,
                            if_text.as_deref(),
                            instance,
                        );
                    }
                    if mirror && let Ok(line) = buffer::removed_line(&key) {
                        bridge::buffer_changed(&line);
                    }
                }
                Command::Send { key, sent, base } => self.send(key, sent, base),
                Command::Resend { key, sent, base } => self.resend(key, sent, base),
                Command::Draft { key, text, base } => {
                    if let Some(future) = self
                        .writer(key.field.kind)
                        .and_then(|writer| writer.draft(&self.0.api, &key, &text, &base))
                    {
                        // Best effort: a draft's answer changes nothing.
                        spawn_local(async move {
                            let _ = future.await;
                        });
                    }
                }
                // No draft endpoint discards yet; C2 adds it with `editor/draft`.
                Command::DiscardDraft { .. } => {}
                Command::Show { key, text } => {
                    if let Some(shown) = self.0.shown.borrow().get(&key) {
                        shown.try_set(text);
                    }
                }
            }
        }
        self.after();
    }

    fn write(&self, key: &BufferKey, mut entry: BufferEntry, mirror: bool) {
        if self.0.foreign.borrow().contains(key) {
            return;
        }
        entry.at = WallMs(js::wall_ms());
        entry.draft = self
            .writer(key.field.kind)
            .and_then(|writer| writer.draft_call(key, &entry.base));
        let mut store = LocalStore::open();
        let text = match buffer::write(&mut store, key, &entry) {
            Ok(text) => Some(text),
            Err(StoreError::Foreign) => {
                // Another run's text waits for the offer; this field keeps no buffer here.
                self.0.foreign.borrow_mut().insert(key.clone());
                self.untrack(key);
                None
            }
            Err(error) => {
                // No stale entry may stay behind with an older `sent` (rule 2 of the plan).
                let _ = buffer::remove(&mut store, key, None, entry.instance);
                self.report_buffer_failure(&error);
                serde_json::to_string(&entry).ok()
            }
        };
        if mirror
            && let Some(text) = text
            && let Ok(line) = buffer::changed_line(key, &text)
        {
            bridge::buffer_changed(&line);
        }
    }

    fn report_buffer_failure(&self, error: &StoreError) {
        if self.0.buffer_failure_reported.replace(true) {
            return;
        }
        let report = tc_api::app::ClientError {
            request: js::random_request_id(),
            view: None,
            project: None,
            message: format!("the draft buffer could not be written: {error}"),
            at: WallMs(js::wall_ms()),
        };
        let api = self.0.api.clone();
        spawn_local(async move {
            let _ = api.call(&report).await;
        });
    }

    fn send(&self, key: BufferKey, sent: SentSave, base: Vec<ShownToken>) {
        let saves = self.clone();
        let future = self
            .writer(key.field.kind)
            .map(|writer| writer.save(&self.0.api, &key, &sent, &base));
        spawn_local(async move {
            let result = match future {
                Some(future) => future.await,
                None => Err(CallError::Api(ApiError::Unavailable(Feature::Writes))),
            };
            let answered = classify(result);
            let commands = saves.machine(|m| m.reply(&key, sent.request, answered, js::now_ms()));
            saves.run(commands);
        });
    }

    /// A lost reply goes again only to the run the field sent to (6.5): a run that started
    /// again has no reply cache and would store the text twice.
    fn resend(&self, key: BufferKey, sent: SentSave, base: Vec<ShownToken>) {
        let saves = self.clone();
        spawn_local(async move {
            match saves.0.api.call(&Hello {}).await {
                Ok(hello) if Some(hello.instance) == saves.machine(|m| Some(m.instance())) => {
                    saves.send(key, sent, base);
                }
                Ok(hello) => saves.restarted(hello.instance),
                Err(_) => {
                    let commands = saves
                        .machine(|m| m.reply(&key, sent.request, Answered::Lost, js::now_ms()));
                    saves.run(commands);
                }
            }
        });
    }

    /// Everything that follows the fields: the busy count, the timer, the indicator, the close
    /// guard, `boot.js`'s templates, and whoever waits for a change.
    fn after(&self) {
        let (busy, deadline, worst) = self
            .machine(|m| Some((m.busy(), m.next_deadline(), m.worst())))
            .unwrap_or((0, None, Worst::Error));
        self.0.busy.set_saves(busy);
        self.arm(deadline);
        let aligns = self.align_saves();
        let shown = indicator::indicator(worst, &aligns, self.checkpoint_failed());
        self.0.indicator.try_set(shown);
        let clean = indicator::all_clean(worst, &aligns);
        if self.0.can_close.replace(Some(clean)) != Some(clean) {
            if bridge::has_bridge() {
                bridge::set_can_close(clean);
            } else {
                boot_call("setUnsaved", &[JsValue::from_bool(!clean)]);
            }
        }
        self.retrack();
        let waiters: Vec<Function> = std::mem::take(&mut *self.0.waiters.borrow_mut());
        for resolve in waiters {
            let _ = resolve.call0(&JsValue::NULL);
        }
    }

    fn arm(&self, deadline: Option<f64>) {
        let Ok(window) = js::window() else {
            return;
        };
        if let Some(handle) = self.0.timer.take() {
            window.clear_timeout_with_handle(handle);
        }
        let Some(deadline) = deadline else {
            return;
        };
        // Whole milliseconds, never negative, and never past what a timer takes.
        let wait = (deadline - js::now_ms()).ceil().clamp(0.0, 2_000_000_000.0) as i32;
        if let Some(tick) = self.0.tick.borrow().as_ref()
            && let Ok(handle) = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                tick.as_ref().unchecked_ref(),
                wait,
            )
        {
            self.0.timer.set(Some(handle));
        }
    }

    /// Hands `boot.js` the entry it writes for each focused editor whose field has none: the
    /// field's tokens change with every `Ack`.
    fn retrack(&self) {
        let keys: Vec<BufferKey> = self.0.tracked.borrow().keys().cloned().collect();
        for key in keys {
            let Some(mut template) = self.machine(|m| m.template(&key)) else {
                continue;
            };
            template.draft = self
                .writer(key.field.kind)
                .and_then(|writer| writer.draft_call(&key, &template.base));
            let (Ok(json), Ok(item)) = (serde_json::to_string(&template), buffer::item_key(&key))
            else {
                continue;
            };
            let mut tracked = self.0.tracked.borrow_mut();
            let Some((element, held)) = tracked.get_mut(&key) else {
                continue;
            };
            if *held != json {
                boot_call(
                    "track",
                    &[
                        element.clone().into(),
                        JsValue::from_str(&item),
                        JsValue::from_str(&json),
                    ],
                );
                *held = json;
            }
        }
    }
}

/// Calls `window.tc4Boot[name](...args)`; `boot.js` defines it before the WebAssembly loads.
fn boot_call(name: &str, args: &[JsValue]) {
    let Ok(window) = js::window() else {
        return;
    };
    let Ok(boot) = Reflect::get(&window, &JsValue::from_str("tc4Boot")) else {
        return;
    };
    let Ok(function) = Reflect::get(&boot, &JsValue::from_str(name)) else {
        return;
    };
    let Ok(function) = function.dyn_into::<Function>() else {
        return;
    };
    let array = js_sys::Array::new();
    for arg in args {
        array.push(arg);
    }
    let _ = function.apply(&boot, &array);
}
