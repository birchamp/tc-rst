//! What the server needs from the application (APP-DESIGN §3.3, §4.1, §5.4).
//!
//! tc-server routes and guards; the application answers. The seam is bytes in and bytes
//! out, so the server never parses a request type and the application never sees HTTP.
//! `tc-app` implements the same shape (`App::call`, `events().subscribe`, `quit`,
//! `quitting`), and the program crate joins the two with a thin adapter; neither crate
//! depends on the other. `stub::Stub` is the backend of this crate's own tests.

use std::future::Future;

use axum::body::Bytes;
use tc_api::event::EventFrame;

pub trait Backend: Send + Sync + 'static {
    type Events: EventSource;

    /// One API request. `path` is `service/name`, always a row of tc-api's tables: the server
    /// answers 404 for any other path before it calls this. `body` is at most
    /// [`Limits::api_body`](crate::Limits::api_body) bytes. The server runs it on a task of
    /// its own: it runs to its end even when the client goes away.
    fn call(&self, path: &'static str, body: Bytes) -> impl Future<Output = Answer> + Send;

    /// The event stream after `after` (0: the reader holds no frame). The ring, replay,
    /// `Resync` and coalesced progress are the backend's (§4.1, §5.4).
    fn subscribe(&self, after: u64) -> Self::Events;

    /// The quit work of §5.1 before the process exits: aligner links, checkpoints, actors.
    /// It may run after `app/quit` already ran it, so it must be safe to run twice. The server
    /// calls it after the drain, but a call that outlasted the drain may still be running.
    fn quit(&self) -> impl Future<Output = ()> + Send;

    /// Resolves once the application asked to quit (`app/quit`).
    fn quitting(&self) -> impl Future<Output = ()> + Send;

    /// A picture of an installed Open Bible Stories pack, for `GET /media/obs/{pack}/{path}`
    /// (§5.3). `pack` is one URL segment naming the pack and `path` the file below its
    /// `ingredients/`, both as the page was given them; the application checks both and
    /// answers `None` for anything it does not serve. A backend with no pictures keeps this.
    fn obs_picture(
        &self,
        _pack: &str,
        _path: &str,
    ) -> impl Future<Output = Option<MediaFile>> + Send {
        async { None }
    }

    /// The print document of a finished print export, for `GET /print/{job}` (§5.5): the page
    /// the shell's `printToPDF` and a test's `page.pdf()` print. `job` is the URL segment as
    /// the page gave it; the application answers `None` for an unknown or unfinished job and
    /// for an export of another kind. A backend with no prints keeps this.
    fn print_page(&self, _job: &str) -> impl Future<Output = Option<MediaFile>> + Send {
        async { None }
    }
}

/// A file the page fetches by URL, whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaFile {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
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
    /// The application asked a test crash to end the process before replying, and the
    /// process is still here: the ender of a test only records. The client gets 503 and no
    /// body. A real ender never returns.
    #[cfg(feature = "test-hooks")]
    Ended,
}

pub enum Then {
    Continue,
    /// Send the reply, then end the process once its body was handed to the socket (a test
    /// crash after the reply).
    #[cfg(feature = "test-hooks")]
    EndProcess,
}
