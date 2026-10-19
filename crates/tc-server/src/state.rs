//! What the handlers and layers share.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

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
    pub phase: PhaseCell,
    pub in_flight: AtomicUsize,
    /// `true` once the event streams must end.
    pub streams_end: watch::Sender<bool>,
}

/// Where a run is in its life (APP-DESIGN §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Serving,
    /// The quit began: new requests get 503, and the requests already accepted finish.
    Draining,
    /// The drain is over and the backend's quit work runs: no backend call starts.
    Closing,
}

/// The run's [`Phase`], which the quit sequence moves forward and every request reads.
pub(crate) struct PhaseCell(AtomicU8);

impl PhaseCell {
    pub(crate) fn new() -> Self {
        Self(AtomicU8::new(Phase::Serving as u8))
    }

    pub(crate) fn get(&self) -> Phase {
        match self.0.load(Ordering::SeqCst) {
            0 => Phase::Serving,
            1 => Phase::Draining,
            // Only `enter` stores, and only a `Phase`.
            _ => Phase::Closing,
        }
    }

    pub(crate) fn enter(&self, phase: Phase) {
        self.0.store(phase as u8, Ordering::SeqCst);
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
