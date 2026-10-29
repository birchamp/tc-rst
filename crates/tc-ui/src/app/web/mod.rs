//! The runtime in the browser (APP-DESIGN 6): the API client, the event stream, the query
//! cache, the server's state and the boot. It is glue over the pure modules of [`crate::app`]:
//! they decide, and this executes. wasm32 with the `app` feature only.
//!
//! Rules for everything here: a `RefCell` borrow is never held across an `.await` or while
//! calling out; the signals the cache and [`ServerState`] own are reference counted, so a late
//! completion never meets a disposed one; Leptos context values are wrapped in `SendWrapper`,
//! which is sound because a browser runs the UI on one thread; and nothing panics.

pub mod align;
mod api;
pub mod app_ctx;
pub(crate) mod bridge;
pub mod check;
mod create;
mod exchange;
pub(crate) mod export;
mod fix;
mod import;
mod jobs;
pub(crate) mod js;
mod leave;
mod outside;
mod panic;
#[cfg(feature = "test-hooks")]
mod probe;
mod query;
mod review;
mod saves;
mod shell;
mod sources;
mod state;
mod stream;
mod translate;
pub mod understand;
mod upgrades;
mod view_error;

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use js_sys::Array;
use leptos::prelude::*;
use send_wrapper::SendWrapper;
use tc_api::app::{Hello, HelloView};
use tc_api::event::EventFrame;
use tc_api::ids::{LocaleId, Scope, ServerInstance};
use tc_api::jobs::Get as JobsGet;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, future_to_promise, spawn_local};

pub use api::Api;
pub use app_ctx::{Actions, AppCtx, AppState, Dialog, Layers, use_app};
pub use panic::set_context;
pub use query::QueryCache;
pub use saves::{FieldWriter, GateHook, GateHookId, LocalBoxFuture, Saves, WriteCap};
pub use shell::SaveIndicatorView;
pub use state::ServerState;
pub use view_error::ViewError;

use crate::app::boot::{
    BuildAction, Fate, RELOADED_FOR_KEY, StartupAction, TOKEN_KEY, build_action, build_line,
    startup_action, stop_reason, token_from,
};
use crate::app::cache::CachedQuery;
use crate::app::client::CallError;
use crate::app::effects::{Effect, active_jobs, apply_reread, jobs_to_reread, merge_job, react};
use crate::app::events::{EventReader, Incoming};
use crate::app::recovery::{CrashMarker, ViewErrorText};
use crate::attrs::Attrs;
use crate::ds::{Button, ButtonVariant, Callout, CalloutTone, Size};
use crate::i18n::{self, Catalog, keys, rebuild_text, show_locale, t};

/// A listener added with [`Runtime::listen`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListenerId(u64);

type Listener = Rc<dyn Fn(&EventFrame)>;

/// How many times a re-read of `hello` is repeated because a frame that changes what it holds
/// came while it was on its way. Past that the frames' values stand.
const HELLO_ATTEMPTS: usize = 5;

struct Inner {
    api: Api,
    cache: QueryCache,
    state: ServerState,
    saves: Saves,
    busy: Rc<api::Busy>,
    listeners: RefCell<Vec<(ListenerId, Listener)>>,
    next_listener: Cell<u64>,
    /// How many frames that change what `hello` holds have been applied. A `hello` that was
    /// on its way while one came may be older than it, and is not applied.
    state_frames: Cell<u64>,
    hello_running: Cell<bool>,
    hello_again: Cell<bool>,
}

/// What every screen stands on: the API, the cache and the server's state. Cheap to clone.
#[derive(Clone)]
pub struct Runtime(Rc<Inner>);

/// The page's clock in milliseconds (`performance.now()`), for timers that start at an
/// action, such as the open overlay's (B3).
pub fn now_ms() -> f64 {
    js::now_ms()
}

/// The runtime the page provides, from the reactive context.
pub fn use_runtime() -> Option<Runtime> {
    use_context::<SendWrapper<Runtime>>().map(|runtime| (*runtime).clone())
}

impl Runtime {
    fn new(
        api: Api,
        cache: QueryCache,
        state: ServerState,
        saves: Saves,
        busy: Rc<api::Busy>,
    ) -> Self {
        Self(Rc::new(Inner {
            api,
            cache,
            state,
            saves,
            busy,
            listeners: RefCell::new(Vec::new()),
            next_listener: Cell::new(0),
            state_frames: Cell::new(0),
            hello_running: Cell::new(false),
            hello_again: Cell::new(false),
        }))
    }

    pub fn api(&self) -> &Api {
        &self.0.api
    }

    pub fn cache(&self) -> &QueryCache {
        &self.0.cache
    }

    pub fn state(&self) -> &ServerState {
        &self.0.state
    }

    /// The page's saves (6.5).
    pub fn saves(&self) -> &Saves {
        &self.0.saves
    }

    /// Whether `at` is an open that can still answer (APP-DESIGN 4.3).
    pub fn is_current(&self, at: &Scope) -> bool {
        self.0.cache.is_current(at)
    }

    /// A read an action makes itself, with the scope drop of a read: the reply is dropped
    /// when its open ended meanwhile or the server answers `StaleScope`. Not for a write,
    /// whose reply is never dropped (APP-DESIGN 6.2).
    pub async fn read_once<Q: CachedQuery>(&self, request: &Q) -> Result<Q::Ok, CallError> {
        use crate::app::cache::ReadOf;
        let result = self.0.api.call(request).await;
        let ReadOf::Project { at, .. } = request.read_of() else {
            return result;
        };
        if matches!(
            &result,
            Err(CallError::Api(tc_api::ApiError::StaleScope { .. }))
        ) {
            self.0.cache.closed(&at);
            return Err(CallError::Dropped);
        }
        if self.is_current(&at) {
            result
        } else {
            Err(CallError::Dropped)
        }
    }

    /// Calls `f` with every frame, after the runtime has applied it: the save machine needs
    /// `Changed` with `LocalSave` (AD37). The listener lives until [`unlisten`](Self::unlisten).
    pub fn listen(&self, f: impl Fn(&EventFrame) + 'static) -> ListenerId {
        let id = ListenerId(self.0.next_listener.get());
        self.0.next_listener.set(id.0 + 1);
        self.0.listeners.borrow_mut().push((id, Rc::new(f)));
        id
    }

    /// An aligner's save state, as its latest view or the answer to a flush says: what the
    /// indicator and the close guard count (B6, 5.4).
    pub fn align_save(&self, aligner: tc_api::ids::AlignerId, save: tc_api::align::AlignSave) {
        self.0.state.align_saves.update(|map| {
            map.insert(aligner, save);
        });
        self.0.saves.server_states_changed();
    }

    /// The aligner is closed or gone: nothing of it waits to be saved.
    pub fn align_forget(&self, aligner: tc_api::ids::AlignerId) {
        self.0.state.align_saves.update(|map| {
            map.remove(&aligner);
        });
        self.0.saves.server_states_changed();
    }

    pub fn unlisten(&self, id: ListenerId) {
        self.0
            .listeners
            .borrow_mut()
            .retain(|(held, _)| *held != id);
    }

    /// Applies what the stream delivered.
    fn dispatch(&self, incoming: Incoming) {
        for effect in react(&incoming) {
            self.apply(effect);
        }
        if let Incoming::Frame(frame) = &incoming {
            // A field's own save shows no "while you were typing" notice (AD37).
            if let tc_api::event::Event::Session {
                at,
                event:
                    tc_api::event::SessionEvent::Changed {
                        view,
                        cause: tc_api::event::ChangeCause::LocalSave { request },
                        ..
                    },
            } = &frame.event
            {
                self.0.saves.local_change(*request, at, *view);
            }
            // A copy of the list: a listener may add or remove listeners.
            let listeners: Vec<Listener> = self
                .0
                .listeners
                .borrow()
                .iter()
                .map(|(_, listener)| Rc::clone(listener))
                .collect();
            for listener in listeners {
                listener(frame);
            }
        }
    }

    fn apply(&self, effect: Effect) {
        let state = &self.0.state;
        let changes_hello = matches!(
            effect,
            Effect::Internet(_)
                | Effect::Account(_)
                | Effect::Startup(_)
                | Effect::SessionOpened(_)
                | Effect::SessionClosed(_)
                | Effect::SessionMode { .. }
                | Effect::Restarted(_)
        );
        if changes_hello {
            self.0.state_frames.set(self.0.state_frames.get() + 1);
        }
        match effect {
            Effect::ReadHello => self.read_hello(),
            Effect::ReadJobs => self.read_jobs(),
            Effect::ReadEverything => self.0.cache.resync(),
            Effect::Restarted(instance) => self.restarted(instance),
            Effect::Topic(topic) => self.0.cache.topic(&topic),
            Effect::Changed { at, scope } => self.0.cache.changed(&at, &scope),
            Effect::SessionOpened(at) => self.0.cache.opened(at),
            Effect::SessionClosed(at) => {
                self.0.cache.closed(&at);
                state.session_closed(&at);
                self.sessions_changed();
            }
            Effect::SessionMode { at, mode } => {
                state.session_mode(&at, mode);
                self.sessions_changed();
            }
            Effect::Internet(on) => state.internet.set(on),
            Effect::Account(account) => state.account.set(account),
            Effect::Startup(startup) => state.startup.set(startup),
            Effect::Job(view) => {
                let mut active = 0;
                state.jobs.update(|jobs| {
                    merge_job(jobs, view);
                    active = active_jobs(jobs);
                });
                self.0.busy.set_jobs(active);
            }
            Effect::Checkpoint { at, outcome } => {
                state.checkpoint_ended(&at, &outcome);
                state.checkpoints.update(|map| {
                    map.insert(at, outcome);
                });
                self.0.saves.server_states_changed();
            }
            Effect::MaterializeBlocked { at, paths } => state.blocked.update(|map| {
                map.insert(at, paths);
            }),
            Effect::OutsideEdits { at, outside } => state.session_outside(&at, outside),
            Effect::AlignSave {
                aligner,
                state: save,
            } => {
                state.align_saves.update(|map| {
                    map.insert(aligner, save);
                });
                self.0.saves.server_states_changed();
            }
            // The Align screen reads its view again: its listener sees the frame.
            Effect::Suggest { .. } => {}
        }
    }

    /// The server started again (3.10, AD4), whoever found out first: a frame of the new run,
    /// a `hello` that names it, or the `hello` a lost reply's resend asks first. Once per run:
    /// the others find it done.
    fn restarted(&self, instance: ServerInstance) {
        if self.0.state.instance.get_untracked() == instance {
            return;
        }
        js::session_set(crate::app::recovery::INSTANCE_KEY, &instance.to_string());
        self.0.cache.restarted();
        self.0.state.restarted(instance);
        // The new run's jobs are not the old run's.
        self.0.busy.set_jobs(0);
        // Nothing is sent again to a run that has no reply cache (AD4).
        self.0.saves.restarted(instance);
    }

    /// Fields reattach to the current opens, and hold while theirs is read-only or gone.
    fn sessions_changed(&self) {
        let sessions = self.0.state.sessions.get_untracked();
        self.0.saves.sessions(&sessions);
    }

    /// Reads `hello` again and replaces what it holds. A request while one runs makes it run
    /// once more afterwards, because the frame that asked came after that one was sent.
    fn read_hello(&self) {
        if self.0.hello_running.replace(true) {
            self.0.hello_again.set(true);
            return;
        }
        let runtime = self.clone();
        spawn_local(async move {
            loop {
                runtime.0.hello_again.set(false);
                runtime.read_hello_once().await;
                if !runtime.0.hello_again.get() {
                    break;
                }
            }
            runtime.0.hello_running.set(false);
        });
    }

    /// Reads each job held as active with `jobs/get`, after a gap in the stream. The final frame
    /// of a job may have been in the gap, and `hello` has no jobs.
    fn read_jobs(&self) {
        let held = self.0.state.jobs.with_untracked(jobs_to_reread);
        if held.is_empty() {
            return;
        }
        let runtime = self.clone();
        spawn_local(async move {
            for before in held {
                // A read that fails leaves the job as the frames left it; the next gap tries
                // again.
                let Ok(reply) = runtime.0.api.call(&JobsGet { job: before.id }).await else {
                    continue;
                };
                let mut active = 0;
                runtime.0.state.jobs.update(|jobs| {
                    apply_reread(jobs, &before, reply);
                    active = active_jobs(jobs);
                });
                runtime.0.busy.set_jobs(active);
            }
        });
    }

    async fn read_hello_once(&self) {
        for _ in 0..HELLO_ATTEMPTS {
            let before = self.0.state_frames.get();
            match self.0.api.call(&Hello {}).await {
                // Not older than a frame already applied: take it.
                Ok(hello) if self.0.state_frames.get() == before => {
                    // A `hello` of another run is a restart, which `reseed` alone would hide.
                    self.restarted(hello.instance);
                    self.0.state.reseed(&hello);
                    self.0.saves.sessions(&hello.sessions);
                    return;
                }
                // A frame came while it was on its way, so it may be older than that frame.
                Ok(_) => {}
                // The values stay as the frames left them; the next frame or `Resync` tries
                // again.
                Err(error) => {
                    web_sys::console::error_1(&JsValue::from_str(&format!(
                        "tc4: hello could not be read again: {error}"
                    )));
                    return;
                }
            }
        }
    }
}

/// Starts the app. `boot.js` calls it with the folder the page was served from
/// (`/ui/<build>/`), where the catalogs are, and the id it gave this page load, which the
/// panic hook writes into the crash marker (6.8).
#[wasm_bindgen]
pub fn run_app(base: String, page: String) {
    panic::install(&page);
    #[cfg(feature = "test-hooks")]
    probe::panic_at_boot();
    // The last load ended in a panic: its screen is not restored (AD22).
    let after = panic::take_previous(&page);
    js::mark("tc4:boot");
    spawn_local(async move {
        let busy = Rc::new(api::Busy::default());
        // The page is busy from here until its root is on screen or the boot error is.
        let work = busy.begin();
        if let Err(reason) = boot(&base, &busy, after).await {
            show_boot_error(reason);
        }
        drop(work);
    });
}

/// The nine catalogs that are not compiled in, fetched in parallel. The promise gives their
/// texts as an array, in the order of the ten languages without English.
///
/// It is awaited only after the stream has opened and `hello` has answered, and a fetch that
/// fails sooner would reject it with no handler yet, which the page reports as an unhandled
/// rejection. A handler that does nothing is attached at once; the await later sees the
/// rejection all the same.
fn fetch_catalogs(api: &Api, base: &str) -> js_sys::Promise {
    let all = Array::new();
    for id in LocaleId::all().filter(|id| *id != LocaleId::ENGLISH) {
        let (api, url) = (api.clone(), format!("{base}i18n/{}.json", id.as_str()));
        all.push(&future_to_promise(async move {
            api.get_text(&url)
                .await
                .map(|text| JsValue::from_str(&text))
                .map_err(|error| JsValue::from_str(&format!("{url}: {error}")))
        }));
    }
    let promise = js_sys::Promise::all(&all);
    // Once per page, so the closure is not freed: the promise may settle at any time.
    let quiet = Closure::<dyn FnMut(JsValue)>::new(|_| {});
    let _ = promise.catch(&quiet);
    quiet.forget();
    promise
}

fn install_catalogs(texts: &JsValue) -> Result<(), String> {
    let texts = Array::from(texts);
    for (id, text) in LocaleId::all()
        .filter(|id| *id != LocaleId::ENGLISH)
        .zip(texts.iter())
    {
        let text = text.as_string().unwrap_or_default();
        let catalog = Catalog::parse(&text, rebuild_text(id))
            .map_err(|error| format!("i18n/{}.json: {error}", id.as_str()))?;
        i18n::install_catalog(id, catalog);
    }
    Ok(())
}

/// The steps of APP-DESIGN 6.8 and 3.10, in order. An `Err` is the reason the page shows
/// instead of the app.
async fn boot(base: &str, busy: &Rc<api::Busy>, after: Option<CrashMarker>) -> Result<(), String> {
    if !base.starts_with("/ui/") || !base.ends_with('/') {
        return Err(format!(
            "the page was started from {base}, which is not a /ui/<build>/ folder"
        ));
    }
    let window = js::window().map_err(|e| js::describe(&e))?;
    let fragment = window.location().hash().unwrap_or_default();
    let kept = js::session_get(TOKEN_KEY);
    let token =
        token_from(&fragment, kept.as_deref()).ok_or("this window was opened without its key")?;
    js::session_set(TOKEN_KEY, token.as_str());
    // The first-launch window reads its key from its address, as the shell opens it.
    let prepare = format!("/prepare.html#t={}", token.as_str());
    let api = Api::new(token, Rc::clone(busy));

    let catalogs = fetch_catalogs(&api, base);

    // The stream first, and `hello` only when its head is in: the server subscribes this
    // reader at once, so no frame can fall between the subscription and `hello`'s snapshot
    // (APP-DESIGN 5.4).
    let connection = stream::Connection::open(&api, 0)
        .await
        .map_err(|error| format!("the event stream: {error}"))?;
    let hello: HelloView = api
        .call(&Hello {})
        .await
        .map_err(|error| format!("app/hello: {error}"))?;
    js::mark("tc4:hello");
    web_sys::console::info_1(&JsValue::from_str(&build_line(&hello.build)));

    let kept = js::session_get(RELOADED_FOR_KEY);
    let action = build_action(&hello.build.commit, hello.api, kept.as_deref(), |marker| {
        js::session_set(RELOADED_FOR_KEY, marker)
    });
    if action == BuildAction::Proceed {
        js::session_remove(RELOADED_FOR_KEY);
    } else {
        carry_out(action);
        return Ok(());
    }
    // The first launch is still preparing the resources, or it failed: the first-launch window
    // shows the count or the failure, and comes back at `Ready` (5.3, D14). Nothing has mounted.
    if startup_action(&hello.startup) == StartupAction::Prepare {
        FATE.with(|fate| fate.borrow_mut().stop());
        let _ = window.location().replace(&prepare);
        return Ok(());
    }

    let texts = JsFuture::from(catalogs)
        .await
        .map_err(|error| js::describe(&error))?;
    // The parse of nine catalogs is the time between these two marks.
    js::mark("tc4:catalogs-fetched");
    install_catalogs(&texts)?;
    js::mark("tc4:catalogs");
    // Before anything mounts: `<html lang dir>` is the saved language's from the first paint.
    show_locale(hello.prefs.app_locale);

    // The run whose drafts `boot.js` posts after three panics (6.8).
    js::session_set(
        crate::app::recovery::INSTANCE_KEY,
        &hello.instance.to_string(),
    );
    let state = ServerState::from_hello(&hello);
    let cache = QueryCache::new(api.clone());
    for session in &hello.sessions {
        cache.opened(session.at.clone());
    }
    // The writers first, then the buffer's entries of this run come back as fields (AD22).
    let saves = Saves::new(api.clone(), state.clone(), Rc::clone(busy));
    check::register_writers(&saves);
    #[cfg(feature = "test-hooks")]
    let probe = probe::requested();
    #[cfg(not(feature = "test-hooks"))]
    let probe = false;
    #[cfg(feature = "test-hooks")]
    if probe {
        probe::register(&saves);
    }
    translate::register(&saves);
    saves.restore(&hello);
    saves.guard_close();
    let runtime = Runtime::new(api, cache, state, saves.clone(), Rc::clone(busy));
    let weak = Rc::downgrade(&runtime.0);
    saves.connect(saves::RuntimeHooks {
        restarted: Box::new({
            let weak = weak.clone();
            move |instance| {
                if let Some(inner) = weak.upgrade() {
                    Runtime(inner).restarted(instance);
                }
            }
        }),
        read_sessions: Box::new(move || {
            if let Some(inner) = weak.upgrade() {
                Runtime(inner).read_hello();
            }
        }),
    });
    spawn_local(stream::run(
        runtime.clone(),
        connection,
        EventReader::new(hello.instance),
    ));

    // Where a reload in the same launch returns to (6.6, D16). After a panic the view comes
    // back too, and the boundary shows the callout in place of its screen (AD22).
    let kept = js::session_get(crate::app::shell::NAV_KEY);
    let (view, project) = crate::app::shell::restore(kept.as_deref(), &hello);
    let place = project
        .as_ref()
        .and_then(|_| crate::app::shell::restore_place(kept.as_deref()));
    let s = AppState::new(
        base,
        runtime.state().clone(),
        view,
        project,
        place,
        after.as_ref().map(|marker| marker.message.clone()),
    );
    let ctx = AppCtx {
        actions: Actions::new(runtime.clone(), s.clone()),
        s,
    };
    ctx.actions.resume_receive_job();

    let root = root_element().ok_or("the page has no #root")?;
    let provided = SendWrapper::new(runtime);
    let app = leptos::mount::mount_to(root, move || {
        provide_context(provided.clone());
        app_ctx::provide_app(ctx.clone());
        // The static callout's words follow the shown language (6.8).
        leptos::prelude::Effect::new(move |_| {
            let (lang, dir) = i18n::html_lang_dir(i18n::shown_locale());
            panic::keep_view_error_text(&ViewErrorText {
                lang: lang.to_owned(),
                dir: dir.to_owned(),
                title: t(keys::app::VIEW_ERROR, &[]),
                home: t(keys::app::BACK_TO_HOME, &[]),
            });
        });
        keep_held(provided.state());
        page(after.clone(), probe)
    });
    // Kept, not forgotten: the boot error replaces the app when a session meets a server of
    // another build, and dropping the handle is what takes the app off the page.
    APP.with(|view| *view.borrow_mut() = Some(Box::new(app)));
    js::mark("tc4:mounted");
    Ok(())
}

/// The page: the port of tC4's `App.jsx`, or, in a test build that asks for it, the no-loss
/// probe outside the App, so a restart's way Home never unmounts its fields (D15).
fn page(after: Option<CrashMarker>, probe: bool) -> AnyView {
    #[cfg(feature = "test-hooks")]
    if let Some(name) = probe::q9_requested() {
        return view! { <probe::Q9 name=name /> }.into_any();
    }
    #[cfg(feature = "test-hooks")]
    if probe {
        return view! { <ProbeRoot after=after /> }.into_any();
    }
    let _ = (after, probe);
    view! { <shell::App /> }.into_any()
}

/// The probe page as A4b left it: tC4's view-error callout in place of the probe after a
/// panic, until "Back to Home", which saves what can be saved first (AD22); then the probe.
#[cfg(feature = "test-hooks")]
#[component]
fn ProbeRoot(after: Option<CrashMarker>) -> impl IntoView {
    let failed = ArcRwSignal::new(after.map(|marker| marker.message));
    let saves = use_runtime().map(|runtime| SendWrapper::new(runtime.saves().clone()));
    let home = {
        let failed = failed.clone();
        Callback::new(move |()| {
            let failed = failed.clone();
            let Some(saves) = saves.as_ref().map(|saves| (**saves).clone()) else {
                failed.try_set(None);
                return;
            };
            spawn_local(async move {
                if saves.gate().await {
                    failed.try_set(None);
                }
            });
        })
    };
    move || match failed.get() {
        Some(message) => view! { <ViewError message=message on_home=home /> }.into_any(),
        None => view! { <probe::Probe /> }.into_any(),
    }
}

fn root_element() -> Option<web_sys::HtmlElement> {
    use wasm_bindgen::JsCast as _;
    document()
        .get_element_by_id("root")
        .and_then(|element| element.dyn_into().ok())
}

thread_local! {
    /// The mounted app, whose handle the boot error drops.
    static APP: RefCell<Option<Box<dyn Any>>> = const { RefCell::new(None) };
    static FATE: RefCell<Fate> = const { RefCell::new(Fate::new()) };
}

/// True once the page has reloaded itself or stopped for good: the event stream and every later
/// decision leave it as it is.
fn settled() -> bool {
    FATE.with(|fate| fate.borrow().is_settled())
}

/// Does what [`build_action`] or `version_error_action` decided (APP-DESIGN 3.10), at the boot
/// or in the middle of a session: reload the page, or replace it with the boot error naming
/// both builds. [`Fate`] keeps the first decision.
fn carry_out(action: BuildAction) {
    let action = FATE.with(|fate| fate.borrow_mut().take(action));
    match action {
        None | Some(BuildAction::Proceed) => {}
        Some(BuildAction::Reload) => {
            if let Ok(window) = js::window() {
                let _ = window.location().reload();
            }
        }
        Some(BuildAction::Stop { ui, server }) => mount_boot_error(stop_reason(&ui, &server)),
    }
}

/// What the page shows when it cannot start (APP-DESIGN 6.12): tC4's view-error callout, with
/// the reason, and Retry, which reloads. Only the first reason is shown, and none for a page
/// that is reloading.
fn show_boot_error(reason: String) {
    if FATE.with(|fate| fate.borrow_mut().stop()) {
        mount_boot_error(reason);
    }
}

/// The boot error in the saved language when its catalog is installed, else in English. It
/// replaces the app if the app is on the page.
/// `window.__tcHeld`: how much of what only events carry the page holds (`{jobs, checkpoints,
/// blocked, alignSaves}`), for the smoke: a restart forgets all of it (B2 D14, which took it off
/// the stand-in Home).
fn keep_held(state: &ServerState) {
    let state = state.clone();
    leptos::prelude::Effect::new(move |_| {
        let held = js_sys::Object::new();
        let counts = [
            ("jobs", state.jobs.with(std::collections::BTreeMap::len)),
            (
                "checkpoints",
                state.checkpoints.with(std::collections::HashMap::len),
            ),
            (
                "blocked",
                state.blocked.with(std::collections::HashMap::len),
            ),
            (
                "alignSaves",
                state.align_saves.with(std::collections::HashMap::len),
            ),
        ];
        for (name, count) in counts {
            let _ = js_sys::Reflect::set(&held, &name.into(), &JsValue::from_f64(count as f64));
        }
        if let Ok(window) = web_sys::window().ok_or(()) {
            let _ = js_sys::Reflect::set(&window, &"__tcHeld".into(), &held);
        }
    });
}

fn mount_boot_error(reason: String) {
    js::mark("tc4:boot-error");
    // The app's view comes off the page before the error goes on it.
    let app = APP.with(|view| view.borrow_mut().take());
    drop(app);
    let Some(root) = root_element() else {
        return;
    };
    leptos::mount::mount_to(root, move || view! { <BootError reason=reason.clone() /> }).forget();
}

#[component]
fn BootError(reason: String) -> impl IntoView {
    let retry = Callback::new(move |_: web_sys::MouseEvent| {
        if let Ok(window) = js::window() {
            let _ = window.location().reload();
        }
    });
    view! {
        <div style="flex:1;padding:24px">
            <Callout
                tone=CalloutTone::Warn
                attrs=Attrs::from([("role", "alert"), ("data-testid", "boot-error")])
                style=crate::css! { "overflow-wrap": "anywhere" }
            >
                <strong>{move || t(keys::app::VIEW_ERROR, &[])}</strong>
                " "
                {reason}
                " "
                <Button
                    size=Size::Sm
                    variant=ButtonVariant::Outline
                    attrs=Attrs::from([("data-testid", "boot-error-retry")])
                    on_click=retry
                >
                    {move || t(keys::app::RETRY, &[])}
                </Button>
            </Callout>
        </div>
    }
}
