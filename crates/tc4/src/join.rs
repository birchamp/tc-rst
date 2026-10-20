//! The join of the server and the application (APP-DESIGN §10, order paragraph).
//!
//! `tc-server` routes bytes to a [`Backend`] and `tc-app` answers bytes, and neither crate
//! depends on the other. This program is the one place that knows both, so the application
//! is a backend here by a newtype and every answer is mapped one to one.

use std::sync::Arc;

use tc_server::{Answer, Backend, Bytes, EventSource, Next, Then};

/// The application, as the server's backend.
pub(crate) struct AppBackend(pub(crate) tc_app::App);

/// One reader of the application's event bus.
pub(crate) struct AppEvents(tc_app::events::Subscription);

impl Backend for AppBackend {
    type Events = AppEvents;

    async fn call(&self, path: &'static str, body: Bytes) -> Answer {
        answer(self.0.call(path, &body).await)
    }

    fn subscribe(&self, after: u64) -> AppEvents {
        AppEvents(self.0.events().subscribe(after))
    }

    async fn quit(&self) {
        self.0.quit().await;
    }

    async fn quitting(&self) {
        self.0.quitting().await;
    }
}

impl EventSource for AppEvents {
    async fn next(&mut self) -> Next {
        match self.0.next().await {
            tc_app::events::Next::Frame(frame) => Next::Frame(frame),
            tc_app::events::Next::Lagged => Next::Lagged,
            tc_app::events::Next::Closed => Next::Closed,
        }
    }
}

fn answer(answer: tc_app::Answer) -> Answer {
    match answer {
        tc_app::Answer::Reply { body, then } => Answer::Reply {
            body,
            then: match then {
                tc_app::Then::Continue => Then::Continue,
                #[cfg(feature = "test-hooks")]
                tc_app::Then::EndProcess => Then::EndProcess,
            },
        },
        tc_app::Answer::NotFound => Answer::NotFound,
        #[cfg(feature = "test-hooks")]
        tc_app::Answer::Ended => Answer::Ended,
    }
}

// Both crates end a test crash with the same code, which the process tests look for.
#[cfg(feature = "test-hooks")]
const _: () = assert!(tc_app::hooks::CRASH_EXIT_CODE == tc_server::CRASH_EXIT_CODE);

/// The server's uploaded files, as the application reads them.
pub struct ServerUploads(pub Arc<tc_server::Uploads>);

impl tc_app::UploadedFiles for ServerUploads {
    fn take(&self, id: &tc_api::ids::UploadId) -> Option<tc_app::UploadedFile> {
        self.0.take(id).map(|upload| tc_app::UploadedFile {
            name: upload.name,
            path: upload.path,
        })
    }
}
