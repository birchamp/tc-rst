//! How a call to the app's server ended (UI-3): one shape for every screen, so a failure below
//! the API is never shown as a refusal the server did not make.

use tc_api::ApiError;

use crate::app::client::{CallError, Transport};

/// How a call ended. `web::app_ctx::Actions::run_net` makes one and handles the internet question and
/// the sign-in step itself; a call that can need neither converts its result with `From`.
#[derive(Debug)]
pub enum NetEnd<T> {
    Ok(T),
    /// The internet question or the sign-in step took over; it resumes the step after.
    Asked,
    /// The server refused, for a reason the screen shows.
    Failed(ApiError),
    /// No usable answer from the app's server.
    Transport(Transport),
    /// The project was closed or opened again before the reply came.
    Dropped,
}

impl<T> From<Result<T, CallError>> for NetEnd<T> {
    fn from(result: Result<T, CallError>) -> Self {
        match result {
            Ok(ok) => Self::Ok(ok),
            Err(CallError::Api(error)) => Self::Failed(error),
            Err(CallError::Transport(transport)) => Self::Transport(transport),
            Err(CallError::Dropped) => Self::Dropped,
        }
    }
}
