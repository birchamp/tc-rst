//! The page's side of the shell bridge, `window.tc4Shell` (APP-DESIGN 5.7, AD21). Without the
//! shell (a browser, Playwright without a stub) there is no bridge, and every call here does
//! nothing: the buffer then lives in `localStorage` alone.
//!
//! - `bufferChanged(line)`: one `BufferLine` as JSON; the shell appends it to
//!   `drafts-buffer.log`. Fire and forget.
//! - `takeBuffered(): Promise<string[]>`: the shell's current lines, the last per field, with
//!   removed fields left out, on every call.
//! - `setCanClose(bool)`: whether everything is saved.
//! - `onCloseRequested(fn)`: `fn` returns `Promise<boolean>`; the page runs the navigation gate
//!   and answers whether everything is saved. The shell closes on `true` and asks its "Unsaved
//!   changes, close anyway?" question on `false`, or if no answer came within 5 s.

use js_sys::{Array, Function, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use super::js;

fn shell() -> Option<JsValue> {
    let window = js::window().ok()?;
    let shell = Reflect::get(&window, &JsValue::from_str("tc4Shell")).ok()?;
    shell.is_object().then_some(shell)
}

fn method(name: &str) -> Option<(JsValue, Function)> {
    let shell = shell()?;
    let function = Reflect::get(&shell, &JsValue::from_str(name))
        .ok()?
        .dyn_into::<Function>()
        .ok()?;
    Some((shell, function))
}

pub(super) fn has_bridge() -> bool {
    shell().is_some()
}

pub(super) fn buffer_changed(line: &str) {
    if let Some((shell, function)) = method("bufferChanged") {
        let _ = function.call1(&shell, &JsValue::from_str(line));
    }
}

pub(super) async fn take_buffered() -> Vec<String> {
    let Some((shell, function)) = method("takeBuffered") else {
        return Vec::new();
    };
    let Ok(answer) = function.call0(&shell) else {
        return Vec::new();
    };
    let lines = match answer.dyn_into::<Promise>() {
        Ok(promise) => match JsFuture::from(promise).await {
            Ok(lines) => lines,
            Err(_) => return Vec::new(),
        },
        Err(value) => value,
    };
    if !Array::is_array(&lines) {
        return Vec::new();
    }
    Array::from(&lines)
        .iter()
        .filter_map(|line| line.as_string())
        .collect()
}

pub(super) fn set_can_close(clean: bool) {
    if let Some((shell, function)) = method("setCanClose") {
        let _ = function.call1(&shell, &JsValue::from_bool(clean));
    }
}

/// Registers `answer` with the shell. The closure lives as long as the page.
pub(super) fn on_close_requested(answer: impl Fn() -> Promise + 'static) {
    let Some((shell, function)) = method("onCloseRequested") else {
        return;
    };
    let closure = Closure::<dyn Fn() -> Promise>::new(answer);
    let _ = function.call1(&shell, closure.as_ref());
    closure.forget();
}
