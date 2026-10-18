//! The client of the loopback spike: what the aligner's click costs from a wasm `fetch` to
//! `tc4 serve` and back to a parsed reply.
//!
//! One export, [`run`]. It holds the event stream open, as the UI always does (that takes one
//! of the browser's six connections to the origin), then makes `warmup + clicks` sequential
//! `test/echo` calls, each sent when the previous reply has arrived, as the UI's click queue
//! sends them (AD18). It times each from the request to the parsed reply.
#![cfg(target_arch = "wasm32")]

use js_sys::{Float64Array, Reflect};
use tc_api::Reply;
use tc_api::test::{Echo, EchoView};
use wasm_bindgen::JsCast as _;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, ReadableStreamDefaultReader, Request, RequestInit, Response};

fn fail(what: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&what.to_string())
}

fn window() -> Result<web_sys::Window, JsValue> {
    web_sys::window().ok_or_else(|| fail("no window"))
}

/// The launch's token: the shell opens the page at `url#t=<token>` (APP-DESIGN §5.2).
fn token() -> Result<String, JsValue> {
    let hash = window()?.location().hash()?;
    hash.strip_prefix("#t=")
        .map(str::to_owned)
        .ok_or_else(|| fail("the page was opened without #t=<token>"))
}

/// Sends a request and waits for the response head.
async fn fetch(url: &str, method: &str, headers: &Headers, body: Option<&str>) -> Result<Response, JsValue> {
    let init = RequestInit::new();
    init.set_method(method);
    init.set_headers(headers);
    if let Some(body) = body {
        init.set_body(&JsValue::from_str(body));
    }
    let request = Request::new_with_str_and_init(url, &init)?;
    let response = JsFuture::from(window()?.fetch_with_request(&request)).await?;
    response.dyn_into()
}

/// Opens `GET /api/v1/events` and returns once the response has begun: the server writes a
/// comment line at once, so the stream is open when this returns.
async fn open_events(token: Option<&str>) -> Result<ReadableStreamDefaultReader, JsValue> {
    let headers = Headers::new()?;
    headers.set("X-Tc-Api", "1")?;
    if let Some(token) = token {
        headers.set("X-Tc-Token", token)?;
    }
    let response = fetch("/api/v1/events", "GET", &headers, None).await?;
    if response.status() != 200 {
        return Err(fail(format_args!("events: status {}", response.status())));
    }
    let body = response.body().ok_or_else(|| fail("events: no body"))?;
    body.get_reader()
        .dyn_into()
        .map_err(|_| fail("events: the reader is not a default reader"))
}

/// Reads the stream to its end, as the UI's reader does.
async fn read_all(reader: ReadableStreamDefaultReader) {
    loop {
        let Ok(chunk) = JsFuture::from(reader.read()).await else {
            return;
        };
        let done = Reflect::get(&chunk, &JsValue::from_str("done"))
            .ok()
            .and_then(|done| done.as_bool());
        if done != Some(false) {
            return;
        }
    }
}

/// The milliseconds of each of `clicks` round trips of `test/echo` asking for a reply of
/// `size` bytes, after `warmup` more that are not kept. With `self_header` the page sets
/// `X-Tc-Token` itself, as the UI does (AD11); without it, something else must add the header
/// (the proxy pass).
#[wasm_bindgen]
pub async fn run(clicks: u32, warmup: u32, size: u32, self_header: bool) -> Result<Float64Array, JsValue> {
    let token = token()?;
    let token = self_header.then_some(token.as_str());
    let reader = open_events(token).await?;
    wasm_bindgen_futures::spawn_local(read_all(reader));

    let performance = window()?.performance().ok_or_else(|| fail("no performance"))?;
    let body = serde_json::to_string(&Echo { size }).map_err(fail)?;
    let mut samples = Vec::with_capacity(clicks as usize);
    for click in 0..warmup + clicks {
        let started = performance.now();
        let headers = Headers::new()?;
        headers.set("Content-Type", "application/json")?;
        headers.set("X-Tc-Api", "1")?;
        if let Some(token) = token {
            headers.set("X-Tc-Token", token)?;
        }
        let response = fetch("/api/v1/test/echo", "POST", &headers, Some(&body)).await?;
        if response.status() != 200 {
            return Err(fail(format_args!("echo: status {}", response.status())));
        }
        let text = JsFuture::from(response.text()?).await?.as_string().unwrap_or_default();
        match serde_json::from_str::<Reply<EchoView>>(&text).map_err(fail)? {
            Reply::Ok(view) if !view.words.is_empty() => {}
            _ => return Err(fail("echo: the reply holds no words")),
        }
        let elapsed = performance.now() - started;
        if click >= warmup {
            samples.push(elapsed);
        }
    }
    Ok(Float64Array::from(samples.as_slice()))
}
