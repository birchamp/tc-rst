//! The few calls into the page that the runtime makes. Every one returns a `Result` or ignores
//! its failure: a refused storage or a missing `performance` object must not stop the app.

use std::cell::Cell;

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

pub(super) fn window() -> Result<web_sys::Window, JsValue> {
    web_sys::window().ok_or_else(|| JsValue::from_str("this page has no window"))
}

/// What a failed JavaScript call says: a string, or the `message` of an error.
pub(super) fn describe(error: &JsValue) -> String {
    error
        .as_string()
        .or_else(|| {
            js_sys::Reflect::get(error, &JsValue::from_str("message"))
                .ok()
                .and_then(|message| message.as_string())
        })
        .unwrap_or_else(|| "an unknown error".to_owned())
}

fn session_storage() -> Option<web_sys::Storage> {
    window().ok()?.session_storage().ok().flatten()
}

/// A `sessionStorage` entry. Storage can be blocked, and then there is none.
pub(super) fn session_get(key: &str) -> Option<String> {
    session_storage()?.get_item(key).ok().flatten()
}

pub(super) fn session_set(key: &str, value: &str) {
    if let Some(storage) = session_storage() {
        let _ = storage.set_item(key, value);
    }
}

pub(super) fn session_remove(key: &str) {
    if let Some(storage) = session_storage() {
        let _ = storage.remove_item(key);
    }
}

/// `performance.mark(name)`, which the measurements and the smoke read.
pub(super) fn mark(name: &str) {
    if let Some(performance) = window().ok().and_then(|window| window.performance()) {
        let _ = performance.mark(name);
    }
}

/// `window.__tcIdle`: true when no call, save or job is in flight (APP-DESIGN 6.2). The tests
/// wait for it instead of for a clock.
pub(super) fn set_idle(idle: bool) {
    if let Ok(window) = window() {
        let _ = js_sys::Reflect::set(
            &window,
            &JsValue::from_str("__tcIdle"),
            &JsValue::from_bool(idle),
        );
    }
}

pub(super) async fn sleep(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Ok(window) = window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        }
    });
    let _ = JsFuture::from(promise).await;
}

/// Aborts a connection that has been silent for too long. One timer that [`arm`](Self::arm)
/// sets again at every chunk, not a timer for each read, so a busy stream does not leave
/// thousands waiting.
pub(super) struct Watchdog {
    handle: Cell<Option<i32>>,
    fire: Closure<dyn FnMut()>,
    ms: i32,
}

impl Watchdog {
    pub(super) fn new(controller: web_sys::AbortController, ms: i32) -> Self {
        Self {
            handle: Cell::new(None),
            fire: Closure::new(move || controller.abort()),
            ms,
        }
    }

    pub(super) fn arm(&self) {
        self.disarm();
        if let Ok(window) = window()
            && let Ok(handle) = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                self.fire.as_ref().unchecked_ref(),
                self.ms,
            )
        {
            self.handle.set(Some(handle));
        }
    }

    pub(super) fn disarm(&self) {
        if let Some(handle) = self.handle.take()
            && let Ok(window) = window()
        {
            window.clear_timeout_with_handle(handle);
        }
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        // Before the closure goes: a timer that fired into it would call freed code.
        self.disarm();
    }
}
