//! The flows that run one at a time (AD31, A-31): sync, gateway change, share, sign-in and
//! import each run one at a time across the app, as tC4's `syncing`, `gatewayBusy` and
//! `signInFlight` flags make them. A USB export or receive runs one at a time per project, and
//! never beside a sync of the same project, because STORAGE does not say that a receive and a
//! sync of one project may run together.
//!
//! A flow is entered with a [`FlowRequest`] and held by the [`FlowPass`] it returns. The pass
//! frees the flow when it drops, so a job that holds it frees the flow when the job ends, also
//! when the job panics.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tc_api::error::{ApiError, Flow};
use tc_api::ids::ProjectRef;

/// A flow and what it needs to be one at a time. The project is in the variant, so a
/// per-project flow cannot be entered without one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlowRequest {
    Sync(ProjectRef),
    GatewayChange,
    Share,
    SignIn,
    Import,
    ExchangeExport(ProjectRef),
    ExchangeReceive(ProjectRef),
}

impl FlowRequest {
    pub fn flow(&self) -> Flow {
        match self {
            Self::Sync(_) => Flow::Sync,
            Self::GatewayChange => Flow::GatewayChange,
            Self::Share => Flow::Share,
            Self::SignIn => Flow::SignIn,
            Self::Import => Flow::Import,
            Self::ExchangeExport(_) => Flow::ExchangeExport,
            Self::ExchangeReceive(_) => Flow::ExchangeReceive,
        }
    }

    /// Whether the two cannot run together. Symmetric.
    ///
    /// - Sync, gateway change, share, sign-in and import: one of each at a time across the
    ///   app. Two syncs of different projects exclude each other, as in tC4.
    /// - USB export and receive: one exchange per project, whichever kind. Exchanges of
    ///   different projects run together.
    /// - A sync excludes an exchange of the same project, and runs beside one of another.
    /// - Nothing else excludes anything.
    pub fn excludes(&self, other: &FlowRequest) -> bool {
        use FlowRequest::{
            ExchangeExport, ExchangeReceive, GatewayChange, Import, Share, SignIn, Sync,
        };
        match (self, other) {
            (Sync(_), Sync(_))
            | (GatewayChange, GatewayChange)
            | (Share, Share)
            | (SignIn, SignIn)
            | (Import, Import) => true,
            (
                ExchangeExport(a) | ExchangeReceive(a),
                ExchangeExport(b) | ExchangeReceive(b) | Sync(b),
            )
            | (Sync(a), ExchangeExport(b) | ExchangeReceive(b)) => a == b,
            _ => false,
        }
    }
}

type Held = Arc<Mutex<Vec<FlowRequest>>>;

fn lock(held: &Held) -> MutexGuard<'_, Vec<FlowRequest>> {
    // A panic never leaves the list half changed: it only pushes or removes one entry.
    held.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The flows running now. Clones share one gate.
#[derive(Clone, Default)]
pub struct FlowGate {
    held: Held,
}

/// Holds a flow until it drops.
#[must_use = "the flow is free again as soon as the pass is dropped"]
pub struct FlowPass {
    held: Held,
    request: FlowRequest,
}

impl FlowGate {
    /// Takes the flow, or refuses with `ApiError::Busy` naming the flow that is running and
    /// excludes it.
    pub fn enter(&self, request: FlowRequest) -> Result<FlowPass, ApiError> {
        let mut held = lock(&self.held);
        if let Some(running) = held.iter().find(|running| running.excludes(&request)) {
            return Err(ApiError::Busy(running.flow()));
        }
        held.push(request.clone());
        Ok(FlowPass {
            held: Arc::clone(&self.held),
            request,
        })
    }

    pub fn running(&self) -> Vec<Flow> {
        lock(&self.held).iter().map(FlowRequest::flow).collect()
    }
}

impl FlowPass {
    pub fn request(&self) -> &FlowRequest {
        &self.request
    }
}

impl Drop for FlowPass {
    fn drop(&mut self) {
        let mut held = lock(&self.held);
        if let Some(index) = held.iter().position(|running| *running == self.request) {
            held.remove(index);
        }
    }
}
