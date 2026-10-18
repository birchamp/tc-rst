//! What ends a run, and how it ends (APP-DESIGN §5.1): the shell's `{"quit":true}`, a closed
//! stdin (the shell died, AD39), the application asking to quit, or a signal. All four run
//! one sequence, so a quit never skips the backend's work.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tokio::io::{AsyncBufRead, AsyncBufReadExt as _, AsyncWrite, AsyncWriteExt as _};

use crate::log::warn;
use crate::state::Shared;

/// What ended the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// `{"quit":true}` on stdin: the shell is alive and waits for `{"closed":true}`.
    QuitMessage,
    /// Stdin ended or failed: nobody is there to answer.
    StdinClosed,
    /// The application asked to quit (`app/quit`).
    AppQuit,
    /// Ctrl+C, or SIGTERM on Unix.
    Signal,
}

/// A line on stdin. Other keys are ignored, so the shell can grow the message.
#[derive(Deserialize)]
struct Control {
    quit: bool,
}

/// Reads stdin until it says to quit or ends.
pub(crate) async fn watch_stdin<R>(mut input: R) -> Exit
where
    R: AsyncBufRead + Unpin,
{
    let mut line = String::new();
    loop {
        line.clear();
        match input.read_line(&mut line).await {
            Ok(0) | Err(_) => return Exit::StdinClosed,
            Ok(_) => match serde_json::from_str::<Control>(line.trim()) {
                Ok(Control { quit: true }) => return Exit::QuitMessage,
                Ok(Control { quit: false }) => {}
                // The line may be anything; it is not echoed.
                Err(_) => warn("ignoring a line on stdin that is not a control message"),
            },
        }
    }
}

/// Resolves when the process is asked to end. With `enabled` false it never resolves, so a
/// test run keeps Ctrl+C.
pub(crate) async fn watch_signals(enabled: bool) -> Exit {
    if !enabled {
        return std::future::pending().await;
    }
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let Ok(mut terminate) = signal(SignalKind::terminate()) else {
            return std::future::pending().await;
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        if tokio::signal::ctrl_c().await.is_err() {
            return std::future::pending().await;
        }
    }
    Exit::Signal
}

/// The layer under the security guard: once the quit sequence began, a request gets 503, so a
/// save that arrives now stays dirty in the UI and is never half-run after the backend
/// closed. The event stream and uploads are not counted as in flight: they can run for
/// minutes, and the sequence ends them instead of waiting.
pub(crate) async fn gate(
    State(shared): State<Arc<Shared>>,
    request: Request,
    next: Next,
) -> Response {
    if shared.is_closing() {
        return closing_response();
    }
    let path = request.uri().path();
    if path == tc_api::endpoint::EVENTS_PATH || path == tc_api::upload::UPLOADS_PATH {
        return next.run(request).await;
    }
    let _in_flight = InFlight::enter(&shared);
    next.run(request).await
}

pub(crate) fn closing_response() -> Response {
    let mut response = StatusCode::SERVICE_UNAVAILABLE.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONNECTION, HeaderValue::from_static("close"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Counts a request until its response is built, however the request ends.
struct InFlight<'a>(&'a Shared);

impl<'a> InFlight<'a> {
    fn enter(shared: &'a Shared) -> Self {
        shared.in_flight.fetch_add(1, Ordering::AcqRel);
        Self(shared)
    }
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.in_flight.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Waits up to `within` for the requests being answered to finish. Returns whether there
/// were any.
pub(crate) async fn drain(shared: &Shared, within: Duration) -> bool {
    let began = tokio::time::Instant::now();
    let mut any = false;
    while shared.in_flight.load(Ordering::Acquire) > 0 && began.elapsed() < within {
        any = true;
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    any
}

/// Tells the shell the app closed. A write error is ignored: the shell may be gone, and Rust
/// ignores SIGPIPE, so a write to a closed pipe fails instead of ending the process.
pub(crate) async fn say_closed<W>(output: &mut W)
where
    W: AsyncWrite + Unpin,
{
    let _ = output.write_all(b"{\"closed\":true}\n").await;
    let _ = output.flush().await;
}
