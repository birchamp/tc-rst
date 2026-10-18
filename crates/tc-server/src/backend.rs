//! What the server needs from the application (APP-DESIGN §3.3, §4.1, §5.4).
//!
//! tc-server routes and guards; the application answers. The seam is bytes in and bytes
//! out, so the server never parses a request type and the application never sees HTTP.
//! `tc-app` implements the same shape (`App::call`, `events().subscribe`, `quit`,
//! `quitting`), and the program crate joins the two with a thin adapter; neither crate
//! depends on the other. [`Stub`](crate::stub::Stub) is the backend of this crate's own tests.

use std::future::Future;

use axum::body::Bytes;
use tc_api::event::EventFrame;

pub trait Backend: Send + Sync + 'static {
    type Events: EventSource;

    /// One API request. `path` is `service/name`, always a row of tc-api's tables: the server
    /// answers 404 for any other path before it calls this. `body` is at most
    /// [`Limits::api_body`](crate::Limits::api_body) bytes.
    fn call(&self, path: &'static str, body: Bytes) -> impl Future<Output = Answer> + Send;

    /// The event stream after `after` (0: the reader holds no frame). The ring, replay,
    /// `Resync` and coalesced progress are the backend's (§4.1, §5.4).
    fn subscribe(&self, after: u64) -> Self::Events;

    /// The quit work of §5.1 before the process exits: aligner links, checkpoints, actors.
    /// It may run after `app/quit` already ran it, so it must be safe to run twice.
    fn quit(&self) -> impl Future<Output = ()> + Send;

    /// Resolves once the application asked to quit (`app/quit`).
    fn quitting(&self) -> impl Future<Output = ()> + Send;
}

/// One reader's place in the event stream.
pub trait EventSource: Send + 'static {
    /// The next frame. Dropping the future before it resolves loses nothing.
    fn next(&mut self) -> impl Future<Output = Next> + Send;
}

// A frame is yielded and written at once, never stored, so boxing it would only add an
// allocation per event.
#[allow(clippy::large_enum_variant)]
pub enum Next {
    Frame(EventFrame),
    /// The reader fell too far behind. The server ends the connection; the UI reconnects
    /// with the last sequence it read and gets the replay, or `Resync`.
    Lagged,
    /// No more frames will come.
    Closed,
}

pub enum Answer {
    /// HTTP 200, `application/json`: the bytes of a `tc_api::Reply`.
    Reply { body: Vec<u8>, then: Then },
    /// 404.
    NotFound,
}

pub enum Then {
    Continue,
}
