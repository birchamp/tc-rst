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

/// True when the entry was written. A caller whose rule needs the entry to be there next time
/// (the marker of a reload) must check.
pub(super) fn session_set(key: &str, value: &str) -> bool {
    session_storage().is_some_and(|storage| storage.set_item(key, value).is_ok())
}

pub(super) fn session_remove(key: &str) {
    if let Some(storage) = session_storage() {
        let _ = storage.remove_item(key);
    }
}

/// `navigator.clipboard.writeText(text)`: true once the clipboard holds it.
pub(super) async fn copy_text(text: &str) -> bool {
    let write = || -> Result<js_sys::Promise, JsValue> {
        let navigator = js_sys::Reflect::get(&window()?.into(), &JsValue::from_str("navigator"))?;
        let clipboard = js_sys::Reflect::get(&navigator, &JsValue::from_str("clipboard"))?;
        let write_text: js_sys::Function =
            js_sys::Reflect::get(&clipboard, &JsValue::from_str("writeText"))?.dyn_into()?;
        Ok(write_text
            .call1(&clipboard, &JsValue::from_str(text))?
            .into())
    };
    match write() {
        Ok(promise) => JsFuture::from(promise).await.is_ok(),
        Err(_) => false,
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

/// `performance.now()`, the save machine's clock; 0 without a `performance` object.
pub(super) fn now_ms() -> f64 {
    window()
        .ok()
        .and_then(|window| window.performance())
        .map_or(0.0, |performance| performance.now())
}

/// `Date.now()`, the wall clock a buffer entry's `at` is in.
pub(super) fn wall_ms() -> i64 {
    // Milliseconds since 1970 fit an i64 for millions of years; the cast drops the fraction.
    js_sys::Date::now() as i64
}

/// 16 random bytes from the platform, with `Math.random` only if `crypto` refuses.
pub(super) fn random_bytes() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    let filled = window()
        .ok()
        .and_then(|window| window.crypto().ok())
        .is_some_and(|crypto| crypto.get_random_values_with_u8_array(&mut bytes).is_ok());
    if !filled {
        for byte in &mut bytes {
            // A value in [0, 256), cut to a byte.
            *byte = (js_sys::Math::random() * 256.0) as u8;
        }
    }
    bytes
}

pub(super) fn random_request_id() -> tc_api::ids::RequestId {
    tc_api::ids::RequestId::from_bytes(random_bytes())
}

/// The page's `localStorage`, as the draft buffer's store. Every call can fail: storage may be
/// blocked or full.
pub(super) struct LocalStore(Option<web_sys::Storage>);

impl LocalStore {
    pub(super) fn open() -> Self {
        Self(
            window()
                .ok()
                .and_then(|window| window.local_storage().ok().flatten()),
        )
    }
}

impl crate::app::buffer::Store for LocalStore {
    fn keys(&self) -> Vec<String> {
        let Some(storage) = &self.0 else {
            return Vec::new();
        };
        let length = storage.length().unwrap_or(0);
        (0..length)
            .filter_map(|index| storage.key(index).ok().flatten())
            .collect()
    }

    fn get(&self, item: &str) -> Option<String> {
        self.0.as_ref()?.get_item(item).ok().flatten()
    }

    fn set(&mut self, item: &str, value: &str) -> Result<(), crate::app::buffer::StoreError> {
        let storage = self.0.as_ref().ok_or_else(|| {
            crate::app::buffer::StoreError::Refused("this page has no localStorage".to_owned())
        })?;
        storage
            .set_item(item, value)
            .map_err(|error| crate::app::buffer::StoreError::Refused(describe(&error)))
    }

    fn remove(&mut self, item: &str) -> Result<(), crate::app::buffer::StoreError> {
        let storage = self.0.as_ref().ok_or_else(|| {
            crate::app::buffer::StoreError::Refused("this page has no localStorage".to_owned())
        })?;
        storage
            .remove_item(item)
            .map_err(|error| crate::app::buffer::StoreError::Refused(describe(&error)))
    }
}
