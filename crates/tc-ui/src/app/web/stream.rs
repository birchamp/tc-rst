//! The event stream's reader (APP-DESIGN 5.4, plan D4): one `fetch` that is read for ever.
//!
//! A connection that ends, fails, or is silent for 45 s (three heartbeats) is opened again
//! with the last frame read, after 250 ms that double up to 5 s. The rules for which frames
//! count are the [`EventReader`]'s; this only moves bytes.

use js_sys::{Reflect, Uint8Array};
use wasm_bindgen::{JsCast as _, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortController, ReadableStreamDefaultReader};

use super::api::Api;
use super::{Runtime, js};
use crate::app::client::{CallError, Transport};
use crate::app::events::EventReader;

/// No byte for this long ends the connection: the server sends a heartbeat every 15 s.
const SILENCE_MS: i32 = 45_000;
const BACKOFF_START_MS: i32 = 250;
const BACKOFF_MAX_MS: i32 = 5_000;

/// One open connection to the stream.
pub(super) struct Connection {
    reader: ReadableStreamDefaultReader,
    controller: AbortController,
    silence: js::Watchdog,
}

impl Connection {
    /// Opens the stream after `after`. It is open when this returns: the response head is in,
    /// and the server has subscribed this reader.
    pub(super) async fn open(api: &Api, after: u64) -> Result<Connection, CallError> {
        let controller = AbortController::new()
            .map_err(|e| CallError::Transport(Transport::Unreachable(js::describe(&e))))?;
        // The silence counts from the request, so a server that accepts and says nothing is
        // given up on too.
        let silence = js::Watchdog::new(controller.clone(), SILENCE_MS);
        silence.arm();
        let reader = api.open_events(after, &controller.signal()).await?;
        silence.arm();
        Ok(Connection {
            reader,
            controller,
            silence,
        })
    }

    /// Reads until the connection ends. True if it gave at least one chunk.
    async fn pump(&self, runtime: &Runtime, events: &mut EventReader) -> bool {
        let mut received = false;
        loop {
            // The page reloads or shows the boot error: what the stream brings is not its.
            if super::settled() {
                break;
            }
            let Ok(chunk) = JsFuture::from(self.reader.read()).await else {
                break;
            };
            let done = Reflect::get(&chunk, &JsValue::from_str("done"))
                .ok()
                .and_then(|done| done.as_bool());
            if done != Some(false) {
                break;
            }
            let Ok(value) = Reflect::get(&chunk, &JsValue::from_str("value"))
                .and_then(|value| value.dyn_into::<Uint8Array>())
            else {
                break;
            };
            self.silence.arm();
            received = true;
            match events.push(&value.to_vec()) {
                Ok(incoming) => {
                    for item in incoming {
                        runtime.dispatch(item);
                    }
                }
                // A line that long is not the server's. The frames before it were not
                // returned and `after` has not passed them, so the next connection asks again.
                Err(_) => break,
            }
        }
        received
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        // Let the browser release the socket.
        self.controller.abort();
    }
}

/// Reads the stream for ever, starting with the connection boot opened.
pub(super) async fn run(runtime: Runtime, first: Connection, mut events: EventReader) {
    let mut connection = first;
    let mut backoff = BACKOFF_START_MS;
    loop {
        if connection.pump(&runtime, &mut events).await {
            backoff = BACKOFF_START_MS;
        }
        drop(connection);
        connection = loop {
            // A request found out that the server is another build (APP-DESIGN 3.10), and the
            // page reloads or has stopped: nothing is left to read for.
            if super::settled() {
                return;
            }
            js::sleep(backoff).await;
            if super::settled() {
                return;
            }
            backoff = (backoff * 2).min(BACKOFF_MAX_MS);
            events.reconnected();
            match Connection::open(&runtime.0.api, events.after()).await {
                Ok(connection) => break connection,
                // The server is gone or closing; the next try is later.
                Err(_) => continue,
            }
        };
    }
}
