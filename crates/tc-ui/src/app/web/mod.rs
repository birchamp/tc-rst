//! The runtime in the browser (APP-DESIGN 6): the API client, the event stream, the query
//! cache, the server's state and the boot. It is glue over the pure modules of [`crate::app`]:
//! they decide, and this executes. wasm32 with the `app` feature only.
//!
//! Rules for everything here: a `RefCell` borrow is never held across an `.await` or while
//! calling out; the signals the cache and [`ServerState`] own are reference counted, so a late
//! completion never meets a disposed one; Leptos context values are wrapped in `SendWrapper`,
//! which is sound because a browser runs the UI on one thread; and nothing panics.

mod api;
mod js;
mod query;
mod root;
mod state;
mod stream;

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use js_sys::Array;
use leptos::prelude::*;
use send_wrapper::SendWrapper;
use tc_api::app::{Hello, HelloView};
use tc_api::event::EventFrame;
use tc_api::ids::{LocaleId, Scope};
use tc_api::jobs::Get as JobsGet;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, future_to_promise, spawn_local};

pub use api::Api;
pub use query::QueryCache;
pub use state::ServerState;

use crate::app::boot::{
    BuildCheck, RELOADED_FOR_KEY, TOKEN_KEY, check_build, token_from, unkept_marker,
};
use crate::app::cache::CachedQuery;
use crate::app::client::CallError;
use crate::app::effects::{Effect, active_jobs, apply_reread, jobs_to_reread, merge_job, react};
use crate::app::events::{EventReader, Incoming};
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

/// The runtime the page provides, from the reactive context.
pub fn use_runtime() -> Option<Runtime> {
    use_context::<SendWrapper<Runtime>>().map(|runtime| (*runtime).clone())
}

impl Runtime {
    fn new(api: Api, cache: QueryCache, state: ServerState, busy: Rc<api::Busy>) -> Self {
        Self(Rc::new(Inner {
            api,
            cache,
            state,
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
            Effect::Restarted(instance) => {
                self.0.cache.restarted();
                state.restarted.set(true);
                state.instance.set(instance);
                // The new run numbers its jobs from the start: an old id may name another job.
                state.jobs.set(BTreeMap::new());
                self.0.busy.set_jobs(0);
            }
            Effect::Topic(topic) => self.0.cache.topic(&topic),
            Effect::Changed { at, scope } => self.0.cache.changed(&at, &scope),
            Effect::SessionOpened(at) => self.0.cache.opened(at),
            Effect::SessionClosed(at) => {
                self.0.cache.closed(&at);
                state.session_closed(&at);
            }
            Effect::SessionMode { at, mode } => state.session_mode(&at, mode),
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
            Effect::Checkpoint { at, outcome } => state.checkpoints.update(|map| {
                map.insert(at, outcome);
            }),
            Effect::MaterializeBlocked { at, paths } => state.blocked.update(|map| {
                map.insert(at, paths);
            }),
            Effect::AlignSave {
                aligner,
                state: save,
            } => state.align_saves.update(|map| {
                map.insert(aligner, save);
            }),
        }
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
                    self.0.state.reseed(&hello);
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
/// (`/ui/<build>/`), where the catalogs are.
#[wasm_bindgen]
pub fn run_app(base: String) {
    console_error_panic_hook::set_once();
    js::mark("tc4:boot");
    spawn_local(async move {
        let busy = Rc::new(api::Busy::default());
        // The page is busy from here until its root is on screen or the boot error is.
        let work = busy.begin();
        if let Err(reason) = boot(&base, &busy).await {
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
async fn boot(base: &str, busy: &Rc<api::Busy>) -> Result<(), String> {
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

    let mut check = check_build(
        &hello.build.commit,
        hello.api,
        js::session_get(RELOADED_FOR_KEY).as_deref(),
    );
    if let BuildCheck::ReloadOnce { marker } = &check
        && !js::session_set(RELOADED_FOR_KEY, marker)
    {
        // Without the marker the page after the reload would reload again, for ever.
        check = unkept_marker(&hello.build.commit, hello.api);
    }
    match check {
        BuildCheck::Same => js::session_remove(RELOADED_FOR_KEY),
        BuildCheck::ReloadOnce { .. } => {
            let _ = window.location().reload();
            return Ok(());
        }
        BuildCheck::Stale { ui, server } => {
            return Err(format!(
                "this page is from build {ui}, but the app's server is build {server}"
            ));
        }
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

    let state = ServerState::from_hello(&hello);
    let cache = QueryCache::new(api.clone());
    for session in &hello.sessions {
        cache.opened(session.at.clone());
    }
    let runtime = Runtime::new(api, cache, state, Rc::clone(busy));
    spawn_local(stream::run(
        runtime.clone(),
        connection,
        EventReader::new(hello.instance),
    ));

    let root = root_element().ok_or("the page has no #root")?;
    let provided = SendWrapper::new(runtime);
    leptos::mount::mount_to(root, move || {
        provide_context(provided.clone());
        view! { <root::StandIn /> }
    })
    .forget();
    js::mark("tc4:mounted");
    Ok(())
}

fn root_element() -> Option<web_sys::HtmlElement> {
    use wasm_bindgen::JsCast as _;
    document()
        .get_element_by_id("root")
        .and_then(|element| element.dyn_into().ok())
}

/// What the page shows when it cannot start (APP-DESIGN 6.12): tC4's view-error callout, with
/// the reason, and Retry, which reloads. In the saved language when its catalog is installed,
/// else in English.
fn show_boot_error(reason: String) {
    js::mark("tc4:boot-error");
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
