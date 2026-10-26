//! The page's side of the shell bridge, `window.tc4Shell` (APP-DESIGN 5.7, AD21). Without the
//! shell (a browser, Playwright without a stub) there is no bridge, and every call here does
//! nothing: the buffer then lives in `localStorage` alone.
//!
//! - `bufferChanged(line)`: one `BufferLine` as JSON; the shell appends it to
//!   `drafts-buffer.log`. Fire and forget.
//! - `takeBuffered(): Promise<string[]>`: the shell's current lines, the last per field and
//!   server run, with the entries a removal of the same run ended left out, on every call
//!   (`buffer::current_lines`).
//! - `setCanClose(bool)`: whether everything is saved.
//! - `onCloseRequested(fn)`: `fn` returns `Promise<boolean>`; the page runs the navigation gate
//!   and answers whether everything is saved. The shell closes on `true` and asks its "Unsaved
//!   changes, close anyway?" question on `false`, or if no answer came within 5 s.
//! - `openExternal(url)`: an `http` or `https` page of another site, in the system browser
//!   (5.6 point 8). Without the shell, `window.open` with `noopener`.
//! - `feedbackSend({category, message, name, email, version, attachment}): Promise<{ok:
//!   boolean, reason?: 'offline' | 'timeout' | 'refused' | 'not-configured'}>`: the shell sends
//!   the Feedback report (Q-2) once the page asked the internet question; `category` is the
//!   help desk's English name. A reason the page does not know, or a call that throws, is
//!   `refused`, as in tC4 (`tC4:src/state.jsx:4155-4161`).
//! - `onDownloadDone(fn)`: the shell calls `fn({state, filename})` when a download's save dialog
//!   closes (`completed`, `interrupted`, `cancelled`) and answers a remover.
//! - `printPdf(url): Promise<{ok: true, file} | {ok: false, reason: 'cancelled' | 'failed' |
//!   'refused'}>`: the shell prints the app's own `/print/<job>` page at `url` to a PDF, names it
//!   from the page's `<title>` and saves it; it never rejects.

use js_sys::{Array, Function, Object, Promise, Reflect};
use tc_api::refusal::FeedbackRefusal;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use super::js;
use crate::app::feedback::ShellReport;
use crate::app::print::{DownloadDone, PdfAnswer};

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

/// Opens `url`, an `http` or `https` address, in the system browser, or a new tab without the
/// shell.
pub(crate) fn open_external(url: &str) {
    if let Some((shell, function)) = method("openExternal") {
        let _ = function.call1(&shell, &JsValue::from_str(url));
        return;
    }
    if let Ok(window) = js::window() {
        let _ = window.open_with_url_and_target_and_features(url, "_blank", "noopener,noreferrer");
    }
}

/// Whether the shell offers a folder picker: without it there is no USB export (a browser gets
/// the Burrito zip download instead, APP-DESIGN §5.7).
pub(crate) fn has_choose_folder() -> bool {
    method("chooseFolder").is_some()
}

pub(crate) fn has_choose_file() -> bool {
    method("chooseFile").is_some()
}

/// The shell's folder or file picker: the path chosen, `None` when cancelled or absent.
async fn choose(name: &str) -> Option<String> {
    let (shell, function) = method(name)?;
    let answer = function.call0(&shell).ok()?;
    let answer = match answer.dyn_into::<Promise>() {
        Ok(promise) => JsFuture::from(promise).await.ok()?,
        Err(value) => value,
    };
    answer.as_string().filter(|path| !path.is_empty())
}

pub(crate) async fn choose_folder() -> Option<String> {
    choose("chooseFolder").await
}

pub(crate) async fn choose_file() -> Option<String> {
    choose("chooseFile").await
}

/// Whether the shell can print a PDF: without it there is no PDF export (`tC4:src/data/export/pdf.ts:120`).
pub(crate) fn has_print_pdf() -> bool {
    method("printPdf").is_some()
}

/// Asks the shell to print `url`, a `/print/<job>` page, to a PDF and save it. Without the
/// shell, a failure: the PDF item exists only with it. The contract says the promise never
/// rejects; a rejection, a throw or an answer it does not describe is a failure.
pub(crate) async fn print_pdf(url: &str) -> PdfAnswer {
    let Some((shell, function)) = method("printPdf") else {
        return PdfAnswer::Failed;
    };
    let Ok(answer) = function.call1(&shell, &JsValue::from_str(url)) else {
        return PdfAnswer::Failed;
    };
    let answer = match answer.dyn_into::<Promise>() {
        Ok(promise) => match JsFuture::from(promise).await {
            Ok(answer) => answer,
            Err(_) => return PdfAnswer::Failed,
        },
        Err(value) => value,
    };
    let field = |name: &str| Reflect::get(&answer, &JsValue::from_str(name)).ok();
    if field("ok").and_then(|ok| ok.as_bool()) == Some(true) {
        return match field("file").and_then(|file| file.as_string()) {
            Some(file) => PdfAnswer::Saved(file),
            None => PdfAnswer::Failed,
        };
    }
    match field("reason")
        .and_then(|reason| reason.as_string())
        .as_deref()
    {
        Some("cancelled") => PdfAnswer::Cancelled,
        _ => PdfAnswer::Failed,
    }
}

/// Whether the shell reports the end of each download (`onDownloadDone`).
pub(crate) fn has_download_done() -> bool {
    method("onDownloadDone").is_some()
}

/// Hands the shell's download reports to `listener` while the calling view lives, as tC4's
/// `ExportMenu` effect does: the remover the shell answers runs when the view goes. Without
/// the shell, nothing: no report ever comes.
pub(crate) fn on_download_done(listener: impl Fn(DownloadDone) + 'static) {
    let Some((shell, function)) = method("onDownloadDone") else {
        return;
    };
    let closure = Closure::<dyn Fn(JsValue)>::new(move |report: JsValue| {
        let field = |name: &str| {
            Reflect::get(&report, &JsValue::from_str(name))
                .ok()
                .and_then(|value| value.as_string())
        };
        listener(DownloadDone::of(
            field("state").as_deref(),
            field("filename").unwrap_or_default(),
        ));
    });
    let remover = function
        .call1(&shell, closure.as_ref())
        .ok()
        .and_then(|remover| remover.dyn_into::<Function>().ok());
    match remover {
        Some(remover) => {
            let kept = send_wrapper::SendWrapper::new((closure, remover));
            leptos::prelude::on_cleanup(move || {
                let (_closure, remover) = kept.take();
                let _ = remover.call0(&JsValue::NULL);
            });
        }
        // A shell that answers no remover keeps calling the listener, so it must outlive the view.
        None => closure.forget(),
    }
}

/// Whether the shell can send a Feedback report.
pub(crate) fn has_feedback_send() -> bool {
    method("feedbackSend").is_some()
}

/// The shell's answer to a Feedback report; `None` when there is no shell to send it.
pub(crate) async fn feedback_send(report: &ShellReport) -> Option<Result<(), FeedbackRefusal>> {
    let (shell, function) = method("feedbackSend")?;
    let body = Object::new();
    for (name, value) in [
        ("category", report.category),
        ("message", report.message.as_str()),
        ("name", report.name.as_str()),
        ("email", report.email.as_str()),
        ("version", report.version.as_str()),
        ("attachment", report.attachment.as_str()),
    ] {
        let _ = Reflect::set(&body, &JsValue::from_str(name), &JsValue::from_str(value));
    }
    let refused = Some(Err(FeedbackRefusal::Refused));
    let Ok(answer) = function.call1(&shell, &body) else {
        return refused;
    };
    let answer = match answer.dyn_into::<Promise>() {
        Ok(promise) => match JsFuture::from(promise).await {
            Ok(answer) => answer,
            Err(_) => return refused,
        },
        Err(value) => value,
    };
    let ok = Reflect::get(&answer, &JsValue::from_str("ok"))
        .ok()
        .and_then(|ok| ok.as_bool())
        .unwrap_or(false);
    if ok {
        return Some(Ok(()));
    }
    let reason = Reflect::get(&answer, &JsValue::from_str("reason"))
        .ok()
        .and_then(|reason| reason.as_string());
    Some(Err(reason
        .as_deref()
        .and_then(FeedbackRefusal::from_shell_reason)
        .unwrap_or(FeedbackRefusal::Refused)))
}
