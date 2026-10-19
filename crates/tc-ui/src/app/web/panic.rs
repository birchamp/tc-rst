//! The panic hook (APP-DESIGN 6.8, AD30, A-15): it runs before the abort, writes the crash
//! marker and reports the panic, and schedules nothing, because after the abort the instance is
//! dead and nothing it scheduled may run. `boot.js` does the rest.
//!
//! The hook reads its context with `try_borrow`, never `borrow`: a panic while the context is
//! borrowed would panic again in the hook and abort before the marker exists.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

use tc_api::app::ClientError;
use tc_api::endpoint::{API_VERSION, API_VERSION_HEADER, TOKEN_HEADER};
use tc_api::ids::{ProjectRef, WallMs};
use wasm_bindgen::JsValue;
use web_sys::{Headers, Request, RequestInit};

use super::js;
use crate::app::boot::TOKEN_KEY;
use crate::app::client::api_url;
use crate::app::recovery::{
    CRASH_KEY, CrashMarker, VIEW_ERROR_TEXT_KEY, ViewErrorText, cut_message, follows_panic,
};

#[derive(Default)]
struct Context {
    page: String,
    view: Option<String>,
    project: Option<ProjectRef>,
}

thread_local! {
    static CONTEXT: RefCell<Context> = RefCell::new(Context::default());
}

/// Installs the hook for the page load `page` (the id `boot.js` made).
pub(super) fn install(page: &str) {
    let _ = CONTEXT.try_with(|context| {
        if let Ok(mut context) = context.try_borrow_mut() {
            context.page = page.to_owned();
        }
    });
    std::panic::set_hook(Box::new(hook));
}

/// The view and the project the page shows, for the crash marker and the report.
pub fn set_context(view: Option<&str>, project: Option<ProjectRef>) {
    let _ = CONTEXT.try_with(|context| {
        if let Ok(mut context) = context.try_borrow_mut() {
            context.view = view.map(str::to_owned);
            context.project = project;
        }
    });
}

/// Set by the first panic of the page. The instance is aborted after it, but a closure it left
/// registered can still run and panic again on the state the abort left behind ("RefCell
/// already borrowed"); the marker and the report keep the first panic, which is the cause.
static PANICKED: AtomicBool = AtomicBool::new(false);

fn hook(info: &std::panic::PanicHookInfo<'_>) {
    console_error_panic_hook::hook(info);
    if PANICKED.swap(true, Ordering::SeqCst) {
        return;
    }
    let message = info.to_string();
    let (page, view, project) = CONTEXT
        .try_with(|context| {
            context
                .try_borrow()
                .map(|c| (c.page.clone(), c.view.clone(), c.project.clone()))
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let at = WallMs(js::wall_ms());
    let marker = CrashMarker {
        page,
        view: view.clone(),
        project: project.clone(),
        message: message.clone(),
        at,
    };
    if let Ok(text) = serde_json::to_string(&marker) {
        js::session_set(CRASH_KEY, &text);
    }
    report(ClientError {
        request: js::random_request_id(),
        view,
        project,
        message: cut_message(&message).0,
        at,
    });
}

/// `app/clientError` with `keepalive`, so the request outlives the page; every failure is
/// ignored.
fn report(error: ClientError) {
    let Some(token) = js::session_get(TOKEN_KEY) else {
        return;
    };
    let Ok(body) = serde_json::to_string(&error) else {
        return;
    };
    let Ok(headers) = Headers::new() else {
        return;
    };
    let _ = headers.set(TOKEN_HEADER, &token);
    let _ = headers.set(API_VERSION_HEADER, &API_VERSION.to_string());
    let _ = headers.set("Content-Type", "application/json");
    let init = RequestInit::new();
    init.set_method("POST");
    init.set_headers(&headers);
    init.set_body(&JsValue::from_str(&body));
    // `keepalive` lets the request outlive the page; web-sys has no setter for it.
    let _ = js_sys::Reflect::set(&init, &JsValue::from_str("keepalive"), &JsValue::TRUE);
    let Ok(request) = Request::new_with_str_and_init(&api_url::<ClientError>(), &init) else {
        return;
    };
    if let Ok(window) = js::window() {
        let _ = window.fetch_with_request(&request);
    }
}

/// The crash marker a previous page load left, which this load then removes; `None` when the
/// last load did not end in a panic.
pub(super) fn take_previous(page: &str) -> Option<CrashMarker> {
    let marker = follows_panic(js::session_get(CRASH_KEY).as_deref(), page)?;
    js::session_remove(CRASH_KEY);
    Some(marker)
}

/// Keeps the static callout's words in the shown language for `boot.js` (6.8).
pub(super) fn keep_view_error_text(text: &ViewErrorText) {
    if let Ok(json) = serde_json::to_string(text) {
        js::session_set(VIEW_ERROR_TEXT_KEY, &json);
    }
}
