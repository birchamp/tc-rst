//! What the handlers and layers share.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use axum::extract::FromRef;
use tokio::sync::watch;

use crate::Limits;
use crate::downloads::Downloads;
use crate::routes::RouteTable;
use crate::token::LaunchToken;
use crate::uploads::Uploads;

pub(crate) struct Shared {
    pub token: LaunchToken,
    /// `127.0.0.1:PORT`, the only `Host` a request may carry.
    pub host: String,
    /// `http://127.0.0.1:PORT`, the only `Origin` a request that changes anything may carry.
    pub origin: String,
    pub limits: Limits,
    pub ui_dir: PathBuf,
    pub routes: RouteTable,
    pub uploads: Arc<Uploads>,
    pub downloads: Arc<Downloads>,
    #[cfg(feature = "test-hooks")]
    pub ender: Arc<dyn crate::hooks::Ender>,
    pub test_hooks: AtomicBool,
    /// Set when the quit sequence begins: new requests get 503.
    pub closing: AtomicBool,
    pub in_flight: AtomicUsize,
    /// `true` once the event streams must end.
    pub streams_end: watch::Sender<bool>,
}

impl Shared {
    pub(crate) fn is_closing(&self) -> bool {
        self.closing.load(Ordering::SeqCst)
    }
}

pub(crate) struct AppState<B> {
    pub shared: Arc<Shared>,
    pub backend: Arc<B>,
}

// Not derived: `B` itself is never cloned.
impl<B> Clone for AppState<B> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
            backend: Arc::clone(&self.backend),
        }
    }
}

impl<B> FromRef<AppState<B>> for Arc<Shared> {
    fn from_ref(state: &AppState<B>) -> Self {
        Arc::clone(&state.shared)
    }
}
