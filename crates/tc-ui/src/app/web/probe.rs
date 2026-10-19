//! The no-loss probe (feature `test-hooks` only, never in a release build): two test fields
//! saved through `test/fieldStore`, so `app/no-loss.mjs` can prove the save machine, the draft
//! buffer, `boot.js`'s recovery, the navigation gate and the close guard in Chromium before any
//! screen with a real save exists. The page mounts it outside the App when its address asks
//! `?probe=fields`, so the App's way Home after a restart never unmounts its fields. It also
//! draws the real save indicator (X2) over the probe's saves, and opens the real App language
//! dialog over its fields, so the browser tests can prove that a language switch keeps typed
//! text.
//!
//! Test triggers, read only here: `sessionStorage["tc4.test.panicOnInput"] = "1"` makes the next
//! input handler panic before it tells the machine, and `sessionStorage["tc4.test.panicAtBoot"]
//! = n` makes the next n page loads panic in `run_app`. A ticker reads the machine every 20 ms,
//! so the closures of an aborted instance trap again after a panic.

use std::rc::Rc;

use leptos::prelude::*;
use send_wrapper::SendWrapper;
use serde_json::json;
use tc_api::buffer::{BufferKey, DraftCall, FieldKey, FieldKind, FieldPlace, SentSave};
use tc_api::ids::{ProjectId, ProjectRef};
use tc_api::test::{FieldDraft, FieldLog, FieldStore};
use tc_api::token::ShownToken;
use tc_api::write::Ack;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{future_to_promise, spawn_local};

use super::saves::{FieldWriter, LocalBoxFuture, Saves};
use super::{SaveIndicatorView, js, panic, use_app, use_runtime};
use crate::app::client::{CallError, Transport};
use crate::app::save::indicator::Worst;
use crate::app::save::{FieldSave, FieldScope, Pair, Source};

/// The folder and project id of the test fields: they have no project, and a buffer key needs
/// one.
pub const PROBE_PROJECT: &str = "test-fields";
pub const PROBE_ID: &str = "00000000-0000-0000-0000-000000000000";

const PANIC_ON_INPUT: &str = "tc4.test.panicOnInput";
const PANIC_AT_BOOT: &str = "tc4.test.panicAtBoot";

/// Whether the page's address asks for the probe.
pub(super) fn requested() -> bool {
    js::window()
        .ok()
        .and_then(|window| window.location().search().ok())
        .is_some_and(|search| search.split(['?', '&']).any(|part| part == "probe=fields"))
}

/// A boot that panics, for the recovery tests: `run_app` calls this first.
#[expect(clippy::panic, reason = "a test-hooks panic the recovery tests need")]
pub(super) fn panic_at_boot() {
    let left = js::session_get(PANIC_AT_BOOT).and_then(|n| n.parse::<u32>().ok());
    if let Some(left) = left.filter(|n| *n > 0) {
        js::session_set(PANIC_AT_BOOT, &(left - 1).to_string());
        panic!("tc4 test: a panic at boot ({left} left)");
    }
}

pub(super) fn key(place: &str) -> Option<BufferKey> {
    Some(BufferKey {
        project: ProjectRef::parse(PROBE_PROJECT).ok()?,
        id: ProjectId::parse(PROBE_ID).ok()?,
        field: FieldKey {
            kind: FieldKind::Test,
            place: FieldPlace::parse(place).ok()?,
        },
    })
}

struct TestWriter;

impl FieldWriter for TestWriter {
    fn save(
        &self,
        api: &super::Api,
        key: &BufferKey,
        sent: &SentSave,
        base: &[ShownToken],
    ) -> LocalBoxFuture<Result<Ack, CallError>> {
        let api = api.clone();
        let request = base.first().map(|shown| FieldStore {
            request: sent.request,
            field: key.field.place.as_str().to_owned(),
            shown: shown.clone(),
            text: sent.text.clone(),
        });
        Box::pin(async move {
            match request {
                Some(request) => api.call(&request).await,
                None => Err(CallError::Transport(Transport::Unreadable(
                    "a save with no token".to_owned(),
                ))),
            }
        })
    }

    fn draft(
        &self,
        api: &super::Api,
        key: &BufferKey,
        text: &str,
        base: &[ShownToken],
    ) -> Option<LocalBoxFuture<Result<(), CallError>>> {
        let api = api.clone();
        let request = FieldDraft {
            request: js::random_request_id(),
            field: key.field.place.as_str().to_owned(),
            shown: base.first()?.clone(),
            text: text.to_owned(),
        };
        Some(Box::pin(async move { api.call(&request).await }))
    }

    fn draft_call(&self, key: &BufferKey, base: &[ShownToken]) -> Option<DraftCall> {
        let body = json!({ "field": key.field.place.as_str(), "shown": base.first()? });
        Some(DraftCall {
            path: "test/fieldDraft".to_owned(),
            body: serde_json::to_string(&body).ok()?,
        })
    }
}

/// The probe's writer, registered before the page restores its buffer.
pub(super) fn register(saves: &Saves) {
    saves.register(FieldKind::Test, Rc::new(TestWriter));
    expose(saves);
}

fn state_name(state: Option<&FieldSave>) -> &'static str {
    match state {
        None => "none",
        Some(FieldSave::Clean) => "clean",
        Some(FieldSave::Dirty { .. }) => "dirty",
        Some(FieldSave::Saving { .. }) => "saving",
        Some(FieldSave::Failed(_)) => "failed",
        Some(FieldSave::Held(_)) => "held",
        Some(FieldSave::SavedWithWarning(_)) => "saved-with-warning",
    }
}

/// `window.__tc4Probe`: what the browser tests read without the DOM, so no entry or token is
/// ever drawn on the page.
fn expose(saves: &Saves) {
    let Ok(window) = js::window() else {
        return;
    };
    let object = js_sys::Object::new();
    let for_buffered = saves.clone();
    let buffered = Closure::<dyn Fn(String, String) -> js_sys::Promise>::new(
        move |project: String, id: String| {
            let saves = for_buffered.clone();
            future_to_promise(async move {
                let (Ok(project), Ok(id)) = (ProjectRef::parse(&project), ProjectId::parse(&id))
                else {
                    return Ok(JsValue::from_str("[]"));
                };
                let entries = saves.buffered_for(&project, &id).await;
                Ok(JsValue::from_str(
                    &serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_owned()),
                ))
            })
        },
    );
    let for_state = saves.clone();
    let state = Closure::<dyn Fn(String) -> String>::new(move |place: String| {
        let state = key(&place).and_then(|key| for_state.state(&key));
        state_name(state.as_ref()).to_owned()
    });
    let _ = js_sys::Reflect::set(&object, &JsValue::from_str("buffered"), buffered.as_ref());
    let _ = js_sys::Reflect::set(&object, &JsValue::from_str("state"), state.as_ref());
    let _ = js_sys::Reflect::set(&window, &JsValue::from_str("__tc4Probe"), &object);
    // For the page's life.
    buffered.forget();
    state.forget();
}

/// The closures an aborted instance leaves behind: a timer that reads the machine.
fn start_ticker(saves: &Saves) {
    let saves = saves.clone();
    let probe_key = key("probe-a");
    let tick = Closure::<dyn FnMut()>::new(move || {
        if let Some(key) = &probe_key {
            let _ = saves.state(key);
        }
    });
    if let Ok(window) = js::window() {
        let _ = window.set_interval_with_callback_and_timeout_and_arguments_0(
            tick.as_ref().unchecked_ref(),
            20,
        );
    }
    tick.forget();
}

fn use_saves() -> Option<Saves> {
    use_runtime().map(|runtime| runtime.saves().clone())
}

/// The probe: two fields, the indicator, Retry, Leave and the restart notice.
#[component]
pub(super) fn Probe() -> impl IntoView {
    let Some(saves) = use_saves() else {
        return view! { <p>"no runtime"</p> }.into_any();
    };
    panic::set_context(Some("probe"), ProjectRef::parse(PROBE_PROJECT).ok());
    start_ticker(&saves);
    let indicator = saves.indicator();
    let state = move || indicator.get().state;
    let state_for_retry = state.clone();
    let left = ArcRwSignal::new(false);
    let left_for_view = left.clone();
    let restarted = use_runtime().map(|runtime| runtime.state().restarted.clone());
    let x2 = {
        let indicator = saves.indicator();
        let saves = SendWrapper::new(saves.clone());
        view! {
            <div data-testid="probe-x2" data-on="dark">
                <SaveIndicatorView
                    indicator=Signal::derive(move || indicator.get())
                    on_retry=Callback::new(move |_| saves.retry())
                />
            </div>
        }
    };
    let language = RwSignal::new(false);
    let ctx = use_app();
    let language_dialog = move || {
        let ctx = ctx.clone()?;
        language.get().then(|| {
            let applied = ctx.s.server.saved_locale.get_untracked();
            view! {
                <crate::views::account_menu::AppLanguageDialog
                    applied=applied
                    actions=ctx.actions.clone()
                    on_close=Callback::new(move |()| language.set(false))
                />
            }
        })
    };
    let saves_for_retry = SendWrapper::new(saves.clone());
    let saves_for_leave = SendWrapper::new(saves.clone());
    let retry = move |_| saves_for_retry.retry();
    let leave = move |_| {
        let saves = (*saves_for_leave).clone();
        let left = left.clone();
        left.try_set(false);
        spawn_local(async move {
            if saves.gate().await {
                left.try_set(true);
            }
        });
    };
    view! {
        <div data-testid="probe" style="padding:24px;display:flex;flex-direction:column;gap:12px">
            <ProbeField place="probe-a" />
            <ProbeField place="probe-b" />
            <p data-testid="probe-indicator" data-state=move || state().data_state()></p>
            <Show when=move || state_for_retry() == Worst::Error>
                <button data-testid="probe-retry" on:click=retry.clone()>"Retry"</button>
            </Show>
            <button data-testid="probe-leave" on:click=leave>"Leave"</button>
            <Show when=move || left_for_view.get()>
                <p data-testid="probe-left">"left"</p>
            </Show>
            <Show when=move || restarted.as_ref().is_some_and(|r| r.get())>
                <p data-testid="probe-restarted">"restarted"</p>
            </Show>
            {x2}
            <button data-testid="probe-language" on:click=move |_| language.set(true)>"Language"</button>
            {language_dialog}
        </div>
    }
    .into_any()
}

/// One test field, opened on `test/fieldLog`'s pair.
#[component]
fn ProbeField(place: &'static str) -> impl IntoView {
    let (Some(saves), Some(key), Some(runtime)) = (use_saves(), key(place), use_runtime()) else {
        return view! { <p>"no field"</p> }.into_any();
    };
    let shown: ArcRwSignal<Option<ArcRwSignal<String>>> = ArcRwSignal::new(None);
    {
        let shown = shown.clone();
        let saves = saves.clone();
        let key = key.clone();
        spawn_local(async move {
            let Ok(log) = runtime
                .api()
                .call(&FieldLog {
                    field: place.to_owned(),
                })
                .await
            else {
                return;
            };
            let pair = Pair {
                value: log.current.value,
                token: log.current.token,
                view: None,
            };
            shown.try_set(Some(saves.open(&key, FieldScope::App, pair)));
        });
    }
    let value = {
        let shown = shown.clone();
        move || shown.get().map(|text| text.get()).unwrap_or_default()
    };
    let ready = move || shown.get().is_some();
    let on_input = {
        let saves = SendWrapper::new(saves.clone());
        let key = SendWrapper::new(key.clone());
        move |event: leptos::ev::Event| {
            if js::session_get(PANIC_ON_INPUT).as_deref() == Some("1") {
                js::session_remove(PANIC_ON_INPUT);
                input_panic();
            }
            saves.typed(&key, &event_target_value(&event), Source::Input);
        }
    };
    let on_focus = {
        let saves = SendWrapper::new(saves.clone());
        let key = SendWrapper::new(key.clone());
        move |event: leptos::ev::FocusEvent| {
            if let Some(element) = event
                .target()
                .and_then(|target| target.dyn_into::<web_sys::HtmlElement>().ok())
            {
                saves.focus(&key, &element);
            }
        }
    };
    let on_blur = {
        let saves = SendWrapper::new(saves.clone());
        let key = SendWrapper::new(key.clone());
        move |_: leptos::ev::FocusEvent| saves.blur(&key)
    };
    let on_composition = {
        let saves = SendWrapper::new(saves);
        let key = SendWrapper::new(key);
        move |_: leptos::ev::CompositionEvent| saves.composition(&key)
    };
    view! {
        <textarea
            data-testid=format!("probe-field-{}", place.trim_start_matches("probe-"))
            data-ready=move || ready().to_string()
            prop:value=value
            on:input=on_input
            on:focus=on_focus
            on:blur=on_blur
            on:compositionstart=on_composition
        ></textarea>
    }
    .into_any()
}

#[expect(clippy::panic, reason = "a test-hooks panic the recovery tests need")]
fn input_panic() {
    panic!("tc4 test: a panic in an input handler");
}
