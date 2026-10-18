//! What a call to the API can end in (APP-DESIGN 6.2, AD9), and how a reply is read.
//!
//! HTTP statuses are used only below the API: 401 and 403 for the token, Host and Origin, 409
//! for the API version, 413 for a body over its cap, 503 while the server closes. A proxy or
//! an expired token can therefore never be read as a domain refusal. A network failure is
//! not a reply at all. The three kinds are kept apart in [`CallError`], and a screen matches
//! all of them.

use serde::de::{DeserializeOwned, IgnoredAny};
use tc_api::{ApiError, Endpoint, Reply};

/// The path every API request is under.
pub const API_ROOT: &str = "/api/v1/";

/// `/api/v1/<service>/<name>`: where an endpoint's request is posted.
pub fn api_url<E: Endpoint>() -> String {
    format!("{API_ROOT}{}", E::PATH)
}

/// Why a call did not give its value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CallError {
    /// The server answered with an `ApiError`: a refusal, a stale session, a bug it reports.
    #[error(transparent)]
    Api(ApiError),
    /// A read whose open was closed or replaced before its reply came. The reply was dropped,
    /// and a screen for that open is gone or about to be. A write's reply is never dropped
    /// (APP-DESIGN 6.5): its `Ack` and its `NotSaved::StaleScope` are what the save machine
    /// needs.
    #[error("the project was closed or opened again")]
    Dropped,
    /// No usable answer: the connection failed or the server said something that is not the
    /// API.
    #[error(transparent)]
    Transport(Transport),
}

/// A failure below the API.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Transport {
    /// No reply: the server is gone or restarting, or the page is leaving.
    #[error("the app's server did not answer: {0}")]
    Unreachable(String),
    /// A status the API itself never answers with (401, 403, 404, 413, 500, 503).
    #[error("the app's server answered {0}")]
    Status(u16),
    /// A 200 whose body is not the reply this build expects.
    #[error("the app's server sent a reply that could not be read: {0}")]
    Unreadable(String),
}

impl CallError {
    /// The API version the server speaks when this call was refused for the version: the
    /// page is from another build and reloads once (APP-DESIGN 3.10).
    pub fn version_mismatch(&self) -> Option<u32> {
        match self {
            Self::Api(ApiError::Version { server }) => Some(*server),
            _ => None,
        }
    }
}

/// An answer as the server sent it: 200 carries `Reply<T>`, and 409 carries
/// `Reply::Err(ApiError::Version)`. Every other status is a failure below the API.
pub fn interpret<T: DeserializeOwned>(status: u16, body: &str) -> Result<T, CallError> {
    match status {
        200 => match serde_json::from_str::<Reply<T>>(body) {
            Ok(reply) => reply.into_result().map_err(CallError::Api),
            Err(error) => Err(CallError::Transport(Transport::Unreadable(
                error.to_string(),
            ))),
        },
        409 => match serde_json::from_str::<Reply<IgnoredAny>>(body) {
            Ok(Reply::Err(error)) => Err(CallError::Api(error)),
            _ => Err(CallError::Transport(Transport::Status(status))),
        },
        other => Err(CallError::Transport(Transport::Status(other))),
    }
}

/// The answer of the event stream's open when its status is not 200. The server checks the
/// token, the Host and Origin, and the API version of every `/api/` path, the stream's too, so
/// a page of another API version is refused here with the same 409 as in a call.
pub fn refused_stream(status: u16, body: &str) -> CallError {
    interpret::<IgnoredAny>(status, body)
        .err()
        .unwrap_or(CallError::Transport(Transport::Status(status)))
}
