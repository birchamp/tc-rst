//! The seam's errors as tc-api's (APP-DESIGN §3.5).

use tc_api::error::Feature;
use tc_api::refusal::{LockHolder, OpenRefusal};
use tc_api::{ApiError, NotSaved, Refusal};

use crate::backend::{CommandError, OpenFailure, Stored};

/// Why a write stored nothing. The field keeps its text in every case.
pub fn command_error(error: CommandError) -> ApiError {
    match error {
        CommandError::NotWritten(reason) => ApiError::NotSaved(NotSaved::Disk(reason)),
        CommandError::ReadOnly(reason) => ApiError::NotSaved(NotSaved::ReadOnly(reason)),
        CommandError::BadToken(_) => ApiError::NotSaved(NotSaved::BadToken),
        // The session closed under the write: the UI reattaches and resends with the field's
        // base token (A-19).
        CommandError::Closed => ApiError::NotSaved(NotSaved::StaleScope),
        CommandError::Unavailable => ApiError::Unavailable(Feature::Writes),
    }
}

/// Why a project did not open, in tC4's words.
pub fn open_failure(failure: OpenFailure) -> ApiError {
    match failure {
        OpenFailure::NotAProject => ApiError::Refused(Refusal::Open(OpenRefusal::NotAProject)),
        OpenFailure::Unreadable { reason } => {
            ApiError::Refused(Refusal::Open(OpenRefusal::Unreadable { reason }))
        }
        OpenFailure::LockHeld { holder } => {
            ApiError::Refused(Refusal::Open(OpenRefusal::LockHeld {
                holder: holder.map(|(host, pid)| LockHolder { host, pid }),
            }))
        }
        OpenFailure::NewerFormat { major } => {
            ApiError::Refused(Refusal::Open(OpenRefusal::NewerFormat { format: major }))
        }
        OpenFailure::Unavailable => ApiError::Unavailable(Feature::Writes),
    }
}

/// `Ack.stored` and `Ack.disk_warning`.
pub fn stored(stored: &Stored) -> (tc_api::write::Stored, Option<String>) {
    match stored {
        Stored::Durable => (tc_api::write::Stored::Written, None),
        Stored::WrittenNotDurable(reason) => (tc_api::write::Stored::Written, Some(reason.clone())),
        Stored::Unchanged => (tc_api::write::Stored::Unchanged, None),
    }
}
