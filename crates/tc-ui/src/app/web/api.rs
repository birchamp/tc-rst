//! The API client (APP-DESIGN 6.2, AD9): one `fetch` for each call, with the launch's token.

use std::cell::Cell;
use std::rc::Rc;

use tc_api::Endpoint;
use tc_api::endpoint::{API_VERSION, API_VERSION_HEADER, EVENTS_PATH, TOKEN_HEADER};
use wasm_bindgen::JsCast as _;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortSignal, Headers, ReadableStreamDefaultReader, Request, RequestInit, Response};

use super::js;
use crate::app::boot::{BuildAction, RELOADED_FOR_KEY, Token, version_error_action};
use crate::app::client::{CallError, Transport, api_url, interpret, refused_stream};
use crate::app::effects::BusyCount;

/// The work in flight, for `window.__tcIdle`. It changes the flag at every change.
#[derive(Default)]
pub(super) struct Busy {
    count: Cell<BusyCount>,
}

impl Busy {
    /// One call, catalog fetch or step of the boot, until the guard is dropped, however the
    /// work ends or is cancelled.
    pub(super) fn begin(self: &Rc<Self>) -> Work {
        self.change(|count| count.fetches += 1);
        Work(Rc::clone(self))
    }

    /// The jobs that are active now.
    pub(super) fn set_jobs(&self, jobs: usize) {
        self.change(|count| count.jobs = jobs);
    }

    fn change(&self, edit: impl FnOnce(&mut BusyCount)) {
        let mut count = self.count.get();
        edit(&mut count);
        self.count.set(count);
        js::set_idle(count.is_idle());
    }
}

pub(super) struct Work(Rc<Busy>);

impl Drop for Work {
    fn drop(&mut self) {
        self.0
            .change(|count| count.fetches = count.fetches.saturating_sub(1));
    }
}

/// Calls to the server. Cheap to clone.
#[derive(Clone)]
pub struct Api {
    token: Rc<Token>,
    busy: Rc<Busy>,
}

fn unreachable(error: JsValue) -> CallError {
    CallError::Transport(Transport::Unreachable(js::describe(&error)))
}

fn unreadable(what: &str) -> CallError {
    CallError::Transport(Transport::Unreadable(what.to_owned()))
}

impl Api {
    pub(super) fn new(token: Token, busy: Rc<Busy>) -> Self {
        Self {
            token: Rc::new(token),
            busy,
        }
    }

    /// One request (APP-DESIGN 6.2). The reply is the endpoint's value, or why there is none:
    /// an `ApiError` the server answered with, or a failure below the API. A write's reply is
    /// never dropped here; only reads are, by the cache (APP-DESIGN 6.2).
    pub async fn call<E: Endpoint>(&self, request: &E) -> Result<E::Ok, CallError> {
        let _work = self.busy.begin();
        let body = serde_json::to_string(request)
            .map_err(|error| unreadable(&format!("the request does not serialize: {error}")))?;
        let response = self
            .fetch("POST", &api_url::<E>(), Some(&body), None)
            .await?;
        let (status, text) = read_text(response).await?;
        reload_on_version_error(interpret::<E::Ok>(status, &text))
    }

    /// A file of the UI, such as a catalog.
    pub(super) async fn get_text(&self, url: &str) -> Result<String, CallError> {
        let _work = self.busy.begin();
        let response = self.fetch("GET", url, None, None).await?;
        match read_text(response).await? {
            (200, text) => Ok(text),
            (status, _) => Err(CallError::Transport(Transport::Status(status))),
        }
    }

    /// `GET /api/v1/events?after=n`, ready when the response head is in: the server writes a
    /// comment line at once, so the subscription exists by then (APP-DESIGN 5.4). `signal`
    /// ends the connection.
    pub(super) async fn open_events(
        &self,
        after: u64,
        signal: &AbortSignal,
    ) -> Result<ReadableStreamDefaultReader, CallError> {
        reload_on_version_error(self.open_events_once(after, signal).await)
    }

    async fn open_events_once(
        &self,
        after: u64,
        signal: &AbortSignal,
    ) -> Result<ReadableStreamDefaultReader, CallError> {
        let url = format!("{EVENTS_PATH}?after={after}");
        let response = self.fetch("GET", &url, None, Some(signal)).await?;
        let status = response.status();
        if status != 200 {
            let (_, text) = read_text(response).await?;
            return Err(refused_stream(status, &text));
        }
        let body = response
            .body()
            .ok_or_else(|| unreadable("the event stream has no body"))?;
        body.get_reader()
            .dyn_into::<ReadableStreamDefaultReader>()
            .map_err(|_| unreadable("the event stream has no default reader"))
    }

    /// The token goes in a header of every request, the API's and the files' (APP-DESIGN
    /// 5.2). The browser adds `Origin` to a POST; no `mode` is set.
    async fn fetch(
        &self,
        method: &str,
        url: &str,
        body: Option<&str>,
        signal: Option<&AbortSignal>,
    ) -> Result<Response, CallError> {
        let headers = Headers::new().map_err(unreachable)?;
        headers
            .set(TOKEN_HEADER, self.token.as_str())
            .map_err(unreachable)?;
        headers
            .set(API_VERSION_HEADER, &API_VERSION.to_string())
            .map_err(unreachable)?;
        if body.is_some() {
            headers
                .set("Content-Type", "application/json")
                .map_err(unreachable)?;
        }
        let init = RequestInit::new();
        init.set_method(method);
        init.set_headers(&headers);
        if let Some(body) = body {
            init.set_body(&JsValue::from_str(body));
        }
        init.set_signal(signal);
        let request = Request::new_with_str_and_init(url, &init).map_err(unreachable)?;
        let promise = js::window()
            .map_err(unreachable)?
            .fetch_with_request(&request);
        let response = JsFuture::from(promise).await.map_err(unreachable)?;
        response
            .dyn_into::<Response>()
            .map_err(|_| unreadable("fetch did not give a response"))
    }
}

async fn read_text(response: Response) -> Result<(u16, String), CallError> {
    let status = response.status();
    let promise = response.text().map_err(unreachable)?;
    let text = JsFuture::from(promise).await.map_err(unreachable)?;
    Ok((status, text.as_string().unwrap_or_default()))
}

/// Every call's result and the event stream's open pass through here. A server that refuses
/// the API version is another build than this page (APP-DESIGN 3.10): the page reloads once,
/// whichever request it was that found out. [`version_error_action`] decides, and what it needs
/// of the page is the marker, which the reload must be able to keep.
fn reload_on_version_error<T>(result: Result<T, CallError>) -> Result<T, CallError> {
    if let Err(error) = &result {
        let kept = js::session_get(RELOADED_FOR_KEY);
        let action = version_error_action(error, kept.as_deref(), |marker| {
            js::session_set(RELOADED_FOR_KEY, marker)
        });
        if action == Some(BuildAction::Reload)
            && let Ok(window) = js::window()
        {
            let _ = window.location().reload();
        }
    }
    result
}
