//! `GET /api/v1/events`: the event stream as server-sent events (APP-DESIGN §5.4, AD10).
//!
//! The ring of frames, replay after a reconnect, `Resync` and coalesced progress belong to the
//! backend's event bus (§4.1). This writes the frames it yields, in order, and nothing else:
//! it never skips a frame on a stream that stays open. A reader that falls behind gets
//! [`Next::Lagged`], and the server ends the connection; the UI reconnects with the last
//! sequence it read and the bus replays from there, or sends `Resync`.

use axum::body::{Body, Bytes};
use axum::extract::{Query, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use futures_util::stream;
use serde::Deserialize;
use tokio::sync::watch;
use tokio::time::{Interval, MissedTickBehavior};

use crate::backend::{Backend, EventSource, Next};
use crate::log::warn;
use crate::state::AppState;

#[derive(Deserialize)]
pub(crate) struct EventsQuery {
    after: Option<u64>,
}

pub(crate) async fn stream<B: Backend>(
    State(state): State<AppState<B>>,
    Query(query): Query<EventsQuery>,
) -> Response {
    if state.shared.is_closing() {
        return crate::lifecycle::closing_response();
    }
    let heartbeat = state.shared.limits.heartbeat;
    let mut beat = tokio::time::interval_at(tokio::time::Instant::now() + heartbeat, heartbeat);
    // A late tick is not made up for: one comment line after a long stall is enough.
    beat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let open = Open {
        events: state.backend.subscribe(query.after.unwrap_or(0)),
        beat,
        ended: state.shared.streams_end.subscribe(),
        started: false,
    };
    let body = Body::from_stream(stream::unfold(open, step));
    let mut response = body.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

struct Open<E> {
    events: E,
    beat: Interval,
    ended: watch::Receiver<bool>,
    started: bool,
}

type Chunk = Result<Bytes, std::convert::Infallible>;

/// One chunk of the stream, or its end.
async fn step<E: EventSource>(mut open: Open<E>) -> Option<(Chunk, Open<E>)> {
    if !open.started {
        open.started = true;
        // So the client's `fetch` resolves at once and the stream is seen as open.
        return Some((Ok(Bytes::from_static(b": tc4\n\n")), open));
    }
    // Every branch is cancel-safe, so the losers lose nothing.
    let chunk = tokio::select! {
        next = open.events.next() => match next {
            Next::Frame(frame) => {
                // serde_json writes no raw newline, so the frame is one `data:` line. A frame
                // that cannot be written ends the stream, so the UI reconnects and gets the
                // replay; skipping it would drop a frame on a stream that stays open.
                let Ok(data) = serde_json::to_string(&frame) else {
                    warn("a frame could not be written; the stream ends");
                    return None;
                };
                Bytes::from(format!("id: {}\ndata: {data}\n\n", frame.seq))
            }
            Next::Lagged | Next::Closed => return None,
        },
        _ = open.beat.tick() => Bytes::from_static(b":\n\n"),
        _ = open.ended.wait_for(|ended| *ended) => return None,
    };
    Some((Ok(chunk), open))
}
