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
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::{Rc, Weak};

use js_sys::{Function, Promise, Reflect};
use leptos::prelude::*;
use tc_api::align::AlignSave;
use tc_api::app::{Hello, HelloView, OpenSession};
use tc_api::buffer::{BufferEntry, BufferKey, DraftCall, FieldKind, SentSave};
use tc_api::editor::{DiscardDraft, Draft};
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
use crate::app::client::{CallError, Transport};
use crate::app::save::classify::{Answered, classify};
use crate::app::save::indicator::{self, Indicator, Worst};
use crate::app::save::{
    Command, FieldSave, FieldScope, GateState, Notice, Pair, ResendCheck, SaveMachine, Source, View,
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
        _api: &Api,
        _key: &BufferKey,
        _text: &str,
        _base: &[ShownToken],
    ) -> Option<LocalBoxFuture<Result<(), CallError>>> {
        None
    }
    /// A production draft keeps the entry's run and last-keystroke timestamp.
    fn draft_entry(
        &self,
        api: &Api,
        key: &BufferKey,
        _scope: Option<Scope>,
        entry: &BufferEntry,
    ) -> Option<LocalBoxFuture<Result<(), CallError>>> {
        self.draft(api, key, &entry.text, &entry.base)
    }
    /// A typed production draft, retained whole if its reply is lost.
    fn draft_request(
        &self,
        _key: &BufferKey,
        _scope: Option<Scope>,
        _entry: &BufferEntry,
    ) -> Option<Draft> {
        None
    }
    /// How `boot.js` posts the field's draft after three panics (6.8), if it has one.
    fn draft_call(&self, _key: &BufferKey, _base: &[ShownToken]) -> Option<DraftCall> {
        None
    }
    fn draft_call_entry(
        &self,
        key: &BufferKey,
        _scope: Option<Scope>,
        entry: &BufferEntry,
    ) -> Option<DraftCall> {
        self.draft_call(key, &entry.base)
    }
}

/// A hook the navigation gate runs once every field is saved (B6 registers `align/flush`).
pub type GateHook = Rc<dyn Fn() -> LocalBoxFuture<bool>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateHookId(u64);

/// What the saves ask of the runtime (`web::Runtime`), which owns the server's state.
pub(super) struct RuntimeHooks {
    /// A resend's `app/hello` named another server run: the runtime's one restart path, which
    /// keeps the run for `boot.js`, empties the cache, shows the restart and tells the saves.
    pub restarted: Box<dyn Fn(ServerInstance)>,
    /// Read the open sessions again (`Command::ReadSessions`).
    pub read_sessions: Box<dyn Fn()>,
}

/// Proof that the open is writable (6.4): editors take `Option<WriteCap>`, and their edit
/// controls are off without one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteCap(());

#[derive(Default)]
struct DraftLane {
    running: bool,
    active: Option<Draft>,
    completed: Option<Draft>,
    next: Option<Draft>,
    failed: Option<(Draft, CallError)>,
}

struct CancelDraft {
    request: DiscardDraft,
    drafted: bool,
    ambiguous: bool,
}

struct Inner {
    api: Api,
    state: ServerState,
    busy: Rc<Busy>,
    machine: RefCell<SaveMachine>,
    writers: RefCell<HashMap<FieldKind, Rc<dyn FieldWriter>>>,
    shown: RefCell<HashMap<BufferKey, ArcRwSignal<String>>>,
    /// The timestamp of this page's latest text, also when localStorage refused its entry.
    changed: RefCell<HashMap<BufferKey, WallMs>>,
    drafts: RefCell<HashMap<BufferKey, DraftLane>>,
    cancels: RefCell<HashMap<BufferKey, CancelDraft>>,
    /// The focused editor of each field and the template `boot.js` holds for it.
    tracked: RefCell<HashMap<BufferKey, (web_sys::HtmlElement, String)>>,
    /// What the runtime does when the saves find out something only it can act on.
    runtime: RefCell<Option<RuntimeHooks>>,
    timer: Cell<Option<i32>>,
    tick: RefCell<Option<Closure<dyn FnMut()>>>,
    indicator: ArcRwSignal<Indicator>,
    /// Counts the machine's runs, so a view can follow what it reads with `notice` or `state`.
    changes: ArcRwSignal<u64>,
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
                changed: RefCell::new(HashMap::new()),
                drafts: RefCell::new(HashMap::new()),
                cancels: RefCell::new(HashMap::new()),
                tracked: RefCell::new(HashMap::new()),
                runtime: RefCell::new(None),
                timer: Cell::new(None),
                tick: RefCell::new(Some(tick)),
                indicator: ArcRwSignal::new(Indicator {
                    state: Worst::Saved,
                    commit_error: false,
                    retry: indicator::Retry::Save,
                }),
                changes: ArcRwSignal::new(0),
                can_close: Cell::new(None),
                hooks: RefCell::new(Vec::new()),
                next_hook: Cell::new(0),
                waiters: RefCell::new(Vec::new()),
                buffer_failure_reported: Cell::new(false),
                boot: boot_object().is_some(),
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

    /// The runtime's side, once it exists: the saves are made before it, for the restore.
    pub(super) fn connect(&self, hooks: RuntimeHooks) {
        *self.0.runtime.borrow_mut() = Some(hooks);
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
        self.open_over(key, scope, pair, Vec::new())
    }

    /// A new comment has no token until its first save answers.
    pub fn open_comment(
        &self,
        key: &BufferKey,
        scope: FieldScope,
        pair: Option<Pair>,
    ) -> ArcRwSignal<String> {
        if let Some(pair) = pair {
            return self.open(key, scope, pair);
        }
        let text = self.machine(|m| m.open_comment(key.clone(), scope));
        let shown = ArcRwSignal::new(text);
        self.0.shown.borrow_mut().insert(key.clone(), shown.clone());
        self.after();
        shown
    }

    /// Clearing cannot retract a saved comment. The machine chooses the latest version,
    /// including a save in flight, rather than the value an earlier render showed.
    pub fn revert_comment(&self, key: &BufferKey) -> String {
        let (text, commands) = self.machine(|m| m.revert_comment(key, js::now_ms()));
        self.run(commands);
        if matches!(
            self.state(key),
            Some(FieldSave::Clean | FieldSave::SavedWithWarning(_))
        ) {
            self.clean_acked_drafts(key);
        }
        text
    }

    /// As [`open`](Self::open) for a field that displays several records, a section card: the
    /// tokens of the others follow `pair.token` in its base.
    pub fn open_over(
        &self,
        key: &BufferKey,
        scope: FieldScope,
        pair: Pair,
        rest: Vec<ShownToken>,
    ) -> ArcRwSignal<String> {
        let (text, commands) =
            self.machine(|m| m.open_over(key.clone(), scope, pair, rest, js::now_ms()));
        let shown = ArcRwSignal::new(text);
        self.0.shown.borrow_mut().insert(key.clone(), shown.clone());
        self.run(commands);
        shown
    }

    /// The text of a field changed. `Source::Input` stands only when `boot.js` says it wrote
    /// this very text into the field's item for this keystroke; otherwise (no `boot.js`, an
    /// editor it does not track, an item of another run or one it cannot read, a full storage)
    /// the machine's own write carries the keystroke, to the shell's mirror at least.
    pub fn typed(&self, key: &BufferKey, text: &str, source: Source) {
        let source = if source == Source::Input && self.boot_wrote(key, text) {
            Source::Input
        } else {
            Source::Program
        };
        let changed = buffer::read(&LocalStore::open(), key)
            .filter(|entry| {
                source == Source::Input
                    && entry.instance == self.0.state.instance.get_untracked()
                    && entry.text == text
            })
            .map_or_else(|| WallMs(js::wall_ms()), |entry| entry.at);
        self.0.changed.borrow_mut().insert(key.clone(), changed);
        let commands = self.machine(|m| m.typed(key, text, source, js::now_ms()));
        self.run(commands);
    }

    /// Takes `boot.js`'s word for the keystroke it just handled (`tc4Boot.wrote`): the item it
    /// wrote and the text it wrote there. Taken once, so it never stands for a later change.
    fn boot_wrote(&self, key: &BufferKey, text: &str) -> bool {
        if !self.0.boot {
            return false;
        }
        let Some(boot) = boot_object() else {
            return false;
        };
        let wrote = Reflect::get(&boot, &JsValue::from_str("wrote")).unwrap_or(JsValue::NULL);
        let _ = Reflect::set(&boot, &JsValue::from_str("wrote"), &JsValue::NULL);
        if !wrote.is_object() {
            return false;
        }
        let field = |name: &str| {
            Reflect::get(&wrote, &JsValue::from_str(name))
                .ok()
                .and_then(|value| value.as_string())
        };
        let Ok(item) = buffer::item_key(key) else {
            return false;
        };
        field("item").as_deref() == Some(item.as_str()) && field("text").as_deref() == Some(text)
    }

    pub fn composition(&self, key: &BufferKey) {
        self.machine(|m| m.composition(key));
    }

    /// The editor took focus: `boot.js` now writes each keystroke into the field's entry
    /// before the WebAssembly sees it (6.6, A-15), or, where the item holds another run's entry
    /// or one this build cannot read, sends it to the shell's mirror only.
    pub fn focus(&self, key: &BufferKey, element: &web_sys::HtmlElement) {
        let commands = self.machine(|m| m.focus(key, js::now_ms()));
        self.0
            .tracked
            .borrow_mut()
            .insert(key.clone(), (element.clone(), String::new()));
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

    pub fn invalid_text_reason(&self) -> Option<String> {
        self.machine(|m| m.invalid_text_reason())
    }

    /// Changes after every run of the machine: a view that reads a field's `notice` tracks it.
    pub fn changes(&self) -> ArcRwSignal<u64> {
        self.0.changes.clone()
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

    /// The copy of a read-only project takes its folder's entries, the page's and the
    /// shell's mirror's (A-10, 6.6, `offer::copy`). Returns the slots whose item the storage
    /// would not take under the copy: they stay with the original, and the mirror has them.
    pub async fn rekey(
        &self,
        from: (&ProjectRef, &ProjectId),
        to: (&ProjectRef, &ProjectId),
    ) -> Vec<tc_api::session::BufferSlot> {
        let mirror = bridge::take_buffered().await;
        let copied = crate::app::offer::copy(&mut LocalStore::open(), &mirror, from, to);
        for line in &copied.lines {
            bridge::buffer_changed(line);
        }
        copied.stayed
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
            self.0.changed.borrow_mut().insert(key.clone(), entry.at);
            let commands = self.machine(|m| m.restore(key, entry, scope, now));
            self.run(commands);
        }
        self.sessions(&hello.sessions);
    }

    pub(super) fn sessions(&self, open: &[OpenSession]) {
        let commands = self.machine(|m| m.sessions(open, js::now_ms()));
        self.run(commands);
    }

    /// The server started again (AD4): nothing is sent, and only the fields whose editor is
    /// still on screen stay, held, so their typing is still buffered. Telling it twice changes
    /// nothing.
    pub(super) fn restarted(&self, instance: ServerInstance) {
        self.machine(|m| m.restarted(instance));
        self.after();
    }

    /// A `Changed` that a save of this page caused, in the open `at`.
    pub(super) fn local_change(&self, request: RequestId, at: &Scope, view: ViewSeq) {
        self.machine(|m| m.local_change(request, View::of(Some(at), view)));
    }

    /// The close guard (6.5): with the shell, `setCanClose` on every change and, on a close
    /// request, the gate, answering whether everything is saved; in a browser, `boot.js`'s
    /// `beforeunload` asks while anything is unsaved (`tc4Boot.setUnsaved`), never under the
    /// shell, where it would refuse the close silently (tC4 #356). The listener is `boot.js`'s,
    /// not the WebAssembly's, so the reload after a panic is never held by a question: the
    /// buffer brings the text back. When the page hides, every field that may save is sent with
    /// `keepalive`, as tC4 drains on `pagehide`.
    pub(super) fn guard_close(&self) {
        if bridge::has_bridge() {
            let saves = self.clone();
            bridge::on_close_requested(move || {
                let saves = saves.clone();
                wasm_bindgen_futures::future_to_promise(async move {
                    // The gate passes over a held field and a section buffer, which it cannot
                    // save; the shell still asks before their text goes with the window.
                    let saved = saves.gate().await && saves.all_clean();
                    Ok(JsValue::from_bool(saved))
                })
            });
        }
        self.on_page("pagehide", |saves| {
            saves.0.api.set_keepalive(true);
            let commands = saves.machine(|m| m.hide(js::now_ms()));
            saves.run(commands);
        });
        // A page the browser keeps and shows again sends as usual.
        self.on_page("pageshow", |saves| saves.0.api.set_keepalive(false));
        self.after();
    }

    /// A listener on `window` for the page's life.
    fn on_page(&self, event: &str, f: fn(&Saves)) {
        let Ok(window) = js::window() else {
            return;
        };
        let weak = Rc::downgrade(&self.0);
        let listener = Closure::<dyn FnMut()>::new(move || {
            if let Some(inner) = weak.upgrade()
                && !super::panic::has_panicked()
            {
                f(&Saves(inner));
            }
        });
        let _ = window.add_event_listener_with_callback(event, listener.as_ref().unchecked_ref());
        listener.forget();
    }

    /// The aligners' and the checkpoints' states changed: the indicator follows.
    pub(super) fn server_states_changed(&self) {
        self.after();
    }

    fn align_saves(&self) -> Vec<AlignSave> {
        let mut states: Vec<AlignSave> = self
            .0
            .state
            .align_saves
            .with_untracked(|saves| saves.values().cloned().collect());
        if let Some(reason) = super::align::edit_failure() {
            states.push(AlignSave::Failed(tc_api::write::NotSaved::Disk(reason)));
        } else if super::align::edits_pending() {
            states.push(AlignSave::Unsaved);
        }
        states
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
    /// to date. Nothing after a panic: the aborted instance's closures can still run (a timer,
    /// a late reply, `pagehide`), and their writes would put the machine's older text over the
    /// keystrokes `boot.js` kept.
    fn run(&self, commands: Vec<Command>) {
        if super::panic::has_panicked() {
            return;
        }
        for command in commands {
            match command {
                Command::Write { key, entry, mirror } => self.write(&key, entry, mirror),
                Command::Remove {
                    key,
                    instance,
                    if_text,
                    mirror,
                } => {
                    let _ =
                        buffer::remove(&mut LocalStore::open(), &key, if_text.as_deref(), instance);
                    if mirror && let Ok(line) = buffer::removed_line(&key, instance) {
                        bridge::buffer_changed(&line);
                    }
                }
                Command::Send { key, sent, base } => self.send(key, sent, base),
                Command::Resend {
                    key,
                    sent,
                    base,
                    instance,
                } => self.resend(key, sent, base, instance),
                Command::Draft { key, text, base } => {
                    let entry = self.machine(|m| m.template(&key)).map(|mut entry| {
                        entry.text = text;
                        entry.base = base;
                        entry.at = self.changed_at(&key);
                        entry
                    });
                    let writer = self.writer(key.field.kind);
                    let request = entry.as_ref().and_then(|entry| {
                        writer
                            .as_ref()
                            .and_then(|writer| writer.draft_request(&key, self.scope(&key), entry))
                    });
                    if let Some(request) = request {
                        self.queue_draft(key, request);
                    } else if let Some(future) = entry.and_then(|entry| {
                        writer.and_then(|writer| {
                            writer.draft_entry(&self.0.api, &key, self.scope(&key), &entry)
                        })
                    }) {
                        // Best effort: a draft's answer changes nothing.
                        spawn_local(async move {
                            let _ = future.await;
                        });
                    }
                }
                // The card already awaited durable server discard before dropping its field.
                Command::DiscardDraft { .. } => {}
                Command::Show { key, text } => {
                    if let Some(shown) = self.0.shown.borrow().get(&key) {
                        shown.try_set(text);
                    }
                }
                Command::ReadSessions => {
                    if let Some(hooks) = self.0.runtime.borrow().as_ref() {
                        (hooks.read_sessions)();
                    }
                }
            }
        }
        self.after();
    }

    /// A `Write` command: [`buffer::write_entry`] decides what the storage gets, and the
    /// mirror gets the entry in any case. Another run's item is left for the offer, and this
    /// run's text of the field then lives in the mirror (6.6).
    fn write(&self, key: &BufferKey, mut entry: BufferEntry, mirror: bool) {
        entry.at = self.changed_at(key);
        entry.draft = self
            .writer(key.field.kind)
            .and_then(|writer| writer.draft_call_entry(key, self.scope(key), &entry));
        let written = buffer::write_entry(&mut LocalStore::open(), key, &entry);
        if let Some(error) = &written.failure {
            self.report_buffer_failure(error);
        }
        if mirror && let Some(line) = written.line {
            bridge::buffer_changed(&line);
        }
    }

    fn queue_draft(&self, key: BufferKey, request: Draft) {
        self.0
            .drafts
            .borrow_mut()
            .entry(key.clone())
            .or_default()
            .next = Some(request);
        self.start_drafts(key);
    }

    fn start_drafts(&self, key: BufferKey) {
        {
            let mut drafts = self.0.drafts.borrow_mut();
            let lane = drafts.entry(key.clone()).or_default();
            if lane.running || lane.failed.is_some() || lane.next.is_none() {
                return;
            }
            lane.running = true;
        }
        let saves = self.clone();
        spawn_local(async move {
            loop {
                let request = saves.0.drafts.borrow_mut().get_mut(&key).and_then(|lane| {
                    let request = lane.next.take();
                    lane.active = request.clone();
                    request
                });
                let Some(request) = request else { break };
                let result = saves.0.api.call(&request).await;
                if let Some(lane) = saves.0.drafts.borrow_mut().get_mut(&key) {
                    lane.active = None;
                    match result {
                        Ok(()) => lane.completed = Some(request),
                        Err(error) => {
                            lane.failed = Some((request, error));
                            break;
                        }
                    }
                }
            }
            if let Some(lane) = saves.0.drafts.borrow_mut().get_mut(&key) {
                lane.running = false;
            }
            saves.after();
        });
    }

    async fn drain_drafts(&self, key: &BufferKey) -> Result<(), String> {
        let failed = self.0.drafts.borrow_mut().get_mut(key).and_then(|lane| {
            let failed = lane.failed.take();
            if failed.is_some() {
                lane.running = true;
            }
            failed
        });
        if let Some((mut request, error)) = failed {
            if ambiguous(&error) {
                let hello = self.0.api.call(&Hello {}).await.map_err(|error| {
                    let mut drafts = self.0.drafts.borrow_mut();
                    let lane = drafts.entry(key.clone()).or_default();
                    lane.running = false;
                    lane.failed = Some((request.clone(), error.clone()));
                    drop(drafts);
                    self.after();
                    error.to_string()
                })?;
                if hello.instance != request.instance {
                    let mut drafts = self.0.drafts.borrow_mut();
                    let lane = drafts.entry(key.clone()).or_default();
                    lane.running = false;
                    lane.failed = Some((request, error));
                    drop(drafts);
                    self.after();
                    return Err(
                        "The app restarted; the draft is kept for the next open.".to_owned()
                    );
                }
            } else {
                request.request = js::random_request_id();
            }
            // Resolve this exact earlier call before a newer put or Cancel can overtake it.
            if let Some(lane) = self.0.drafts.borrow_mut().get_mut(key) {
                lane.active = Some(request.clone());
            }
            let result = self.0.api.call(&request).await;
            if let Err(error) = result {
                let mut drafts = self.0.drafts.borrow_mut();
                let lane = drafts.entry(key.clone()).or_default();
                lane.running = false;
                lane.active = None;
                lane.failed = Some((request, error.clone()));
                drop(drafts);
                self.after();
                return Err(error.to_string());
            }
            if let Some(lane) = self.0.drafts.borrow_mut().get_mut(key) {
                lane.running = false;
                lane.active = None;
                lane.completed = Some(request);
            }
            self.after();
        }
        self.start_drafts(key.clone());
        loop {
            let state = self.0.drafts.borrow().get(key).map(|lane| {
                (
                    lane.running,
                    lane.failed.as_ref().map(|(_, error)| error.to_string()),
                )
            });
            match state {
                Some((_, Some(error))) => return Err(error),
                Some((true, None)) => self.changed().await,
                _ => return Ok(()),
            }
        }
    }

    /// Save never waits for a draft. Once a clean Ack arrives, remember only the calls
    /// already outstanding, then remove their exact entries after those calls finish.
    /// A later keystroke's text/time/tokens cannot match any of these compare-and-removes.
    fn clean_acked_drafts(&self, key: &BufferKey) {
        let captured: Vec<Draft> = self
            .0
            .drafts
            .borrow()
            .get(key)
            .map(|lane| {
                lane.active
                    .iter()
                    .chain(lane.next.iter())
                    .chain(lane.completed.iter())
                    .chain(lane.failed.iter().map(|(request, _)| request))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if captured.is_empty() {
            return;
        }
        let (saves, key) = (self.clone(), key.clone());
        spawn_local(async move {
            if saves.drain_drafts(&key).await.is_err() {
                return;
            }
            for draft in captured {
                let request = DiscardDraft {
                    at: draft.at,
                    request: js::random_request_id(),
                    instance: draft.instance,
                    field: draft.field,
                    shown: draft.shown,
                    text: draft.text,
                    changed: draft.changed,
                };
                let result = saves.0.api.call(&request).await;
                if result.as_ref().is_err_and(ambiguous)
                    && let Ok(hello) = saves.0.api.call(&Hello {}).await
                    && hello.instance == request.instance
                {
                    let _ = saves.0.api.call(&request).await;
                }
            }
        });
    }

    /// Cancel waits for the current draft lane and its durable compare-and-remove. Any
    /// failure leaves the field and its buffer in place; the card's Retry calls this again.
    pub async fn discard_confirmed(&self, key: &BufferKey) -> Result<bool, String> {
        let Some(mut entry) = self.machine(|m| m.template(key)) else {
            return Ok(true);
        };
        entry.text = self
            .machine(|m| m.text(key).map(str::to_owned))
            .unwrap_or_default();
        entry.at = self.changed_at(key);
        let writer = self
            .writer(key.field.kind)
            .ok_or("The draft cannot be discarded.")?;
        let draft = writer
            .draft_request(key, self.scope(key), &entry)
            .ok_or("The project is no longer open; the draft is kept.")?;
        {
            let mut cancels = self.0.cancels.borrow_mut();
            let same = cancels.get(key).is_some_and(|cancel| {
                cancel.request.instance == entry.instance
                    && cancel.request.shown == entry.base
                    && cancel.request.text == entry.text
                    && cancel.request.changed == entry.at
            });
            if !same {
                cancels.insert(
                    key.clone(),
                    CancelDraft {
                        request: DiscardDraft {
                            at: draft.at.clone(),
                            request: js::random_request_id(),
                            instance: draft.instance,
                            field: draft.field,
                            shown: draft.shown.clone(),
                            text: draft.text.clone(),
                            changed: draft.changed,
                        },
                        drafted: false,
                        ambiguous: false,
                    },
                );
            }
        }
        self.drain_drafts(key).await?;
        if !self.matches_entry(key, &entry) {
            return Ok(false);
        }
        let drafted = self
            .0
            .cancels
            .borrow()
            .get(key)
            .is_some_and(|cancel| cancel.drafted);
        if !drafted {
            self.queue_draft(key.clone(), draft);
            self.drain_drafts(key).await?;
            if let Some(cancel) = self.0.cancels.borrow_mut().get_mut(key) {
                cancel.drafted = true;
            }
        }
        let (mut request, lost) = self
            .0
            .cancels
            .borrow()
            .get(key)
            .map(|cancel| (cancel.request.clone(), cancel.ambiguous))
            .ok_or("The draft changed while Cancel waited.")?;
        if lost {
            let hello = self
                .0
                .api
                .call(&Hello {})
                .await
                .map_err(|error| error.to_string())?;
            if hello.instance != request.instance {
                return Err("The app restarted; the draft is kept for the next open.".to_owned());
            }
        }
        match self.0.api.call(&request).await {
            Ok(()) => {
                self.0.cancels.borrow_mut().remove(key);
                if self.matches_entry(key, &entry) {
                    self.discard(key);
                    self.0.changed.borrow_mut().remove(key);
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Err(error) => {
                if let Some(cancel) = self.0.cancels.borrow_mut().get_mut(key) {
                    cancel.ambiguous = ambiguous(&error);
                    if !cancel.ambiguous {
                        request.request = js::random_request_id();
                        cancel.request = request;
                    }
                }
                Err(error.to_string())
            }
        }
    }

    fn matches_entry(&self, key: &BufferKey, entry: &BufferEntry) -> bool {
        self.changed_at(key) == entry.at
            && self.machine(|m| {
                m.text(key) == Some(entry.text.as_str())
                    && m.base(key) == Some(entry.base.as_slice())
                    && m.instance() == entry.instance
            })
    }

    fn changed_at(&self, key: &BufferKey) -> WallMs {
        *self
            .0
            .changed
            .borrow_mut()
            .entry(key.clone())
            .or_insert_with(|| WallMs(js::wall_ms()))
    }

    fn scope(&self, key: &BufferKey) -> Option<Scope> {
        self.0.state.sessions.with_untracked(|sessions| {
            sessions
                .iter()
                .find(|session| session.at.project == key.project && session.id == key.id)
                .map(|session| session.at.clone())
        })
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
            let acked = matches!(&answered, Answered::Acked(_));
            let commands = saves.machine(|m| m.reply(&key, sent.request, answered, js::now_ms()));
            if acked
                && matches!(
                    saves.state(&key),
                    Some(FieldSave::Clean | FieldSave::SavedWithWarning(_))
                )
            {
                saves.clean_acked_drafts(&key);
            }
            saves.run(commands);
        });
    }

    /// A lost reply goes again only to the run the field sent to (6.5): a run that started
    /// again has no reply cache and would store the text twice. The machine decides after the
    /// `hello`, since a restart found meanwhile elsewhere on the page changes the answer.
    fn resend(
        &self,
        key: BufferKey,
        sent: SentSave,
        base: Vec<ShownToken>,
        sent_to: ServerInstance,
    ) {
        let saves = self.clone();
        spawn_local(async move {
            match saves.0.api.call(&Hello {}).await {
                Ok(hello) => match saves
                    .machine(|m| m.resend_check(&key, sent.request, sent_to, hello.instance))
                {
                    ResendCheck::Send => saves.send(key, sent, base),
                    // The runtime's restart path tells the saves, and the screens too (AD4).
                    ResendCheck::Restarted(run) => match saves.0.runtime.borrow().as_ref() {
                        Some(hooks) => (hooks.restarted)(run),
                        None => saves.restarted(run),
                    },
                    ResendCheck::Nothing => {}
                },
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
        if super::panic::has_panicked() {
            return;
        }
        let (busy, deadline, worst, failed) = self
            .machine(|m| Some((m.busy(), m.next_deadline(), m.worst(), m.failed_kinds())))
            .unwrap_or((0, None, Worst::Error, Vec::new()));
        self.0.busy.set_saves(busy);
        self.arm(deadline);
        let aligns = self.align_saves();
        let shown = indicator::indicator(worst, &aligns, self.checkpoint_failed(), &failed);
        self.0.indicator.try_set(shown);
        self.0.changes.try_update(|n| *n = n.wrapping_add(1));
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

    /// Hands `boot.js` the entry it writes for each focused editor: the field's entry as the
    /// machine holds it, whose tokens and save in flight change with every save and reply. An
    /// editor whose field the machine no longer holds is not tracked.
    fn retrack(&self) {
        let keys: Vec<BufferKey> = self.0.tracked.borrow().keys().cloned().collect();
        for key in keys {
            let Some(mut template) = self.machine(|m| m.template(&key)) else {
                self.untrack(&key);
                continue;
            };
            template.at = self.changed_at(&key);
            template.draft = self
                .writer(key.field.kind)
                .and_then(|writer| writer.draft_call_entry(&key, self.scope(&key), &template));
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

/// `window.tc4Boot`, which `boot.js` defines before the WebAssembly loads.
fn boot_object() -> Option<JsValue> {
    let window = js::window().ok()?;
    let boot = Reflect::get(&window, &JsValue::from_str("tc4Boot")).ok()?;
    boot.is_object().then_some(boot)
}

/// Calls `window.tc4Boot[name](...args)`.
fn boot_call(name: &str, args: &[JsValue]) {
    let Some(boot) = boot_object() else {
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

fn ambiguous(error: &CallError) -> bool {
    matches!(
        error,
        CallError::Dropped
            | CallError::Transport(Transport::Unreachable(_) | Transport::Status(503))
    )
}
