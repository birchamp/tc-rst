//! The seam's errors as tc-api's (APP-DESIGN §3.5).

use tc_api::error::Feature;
use tc_api::refusal::{LockHolder, OpenRefusal};
use tc_api::write::SaveRefusal;
use tc_api::{ApiError, NotSaved, Refusal};

use crate::backend::{CommandError, OpenFailure, RuleRefusal, Stored};

/// A refusal of the store that no screen has words for yet. The handler reports it and
/// answers `ApiError::Internal`, so "Ask for help" carries the store's sentence. Each unit
/// that sends a command gives the refusals it meets their words (C2 for a save's).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnwordedRefusal(pub String);

/// Why a write stored nothing. The field keeps its text in every case.
pub fn command_error(error: CommandError) -> Result<ApiError, UnwordedRefusal> {
    Ok(match error {
        CommandError::NotWritten(reason) => ApiError::NotSaved(NotSaved::Disk(reason)),
        CommandError::ReadOnly(reason) => ApiError::NotSaved(NotSaved::ReadOnly(reason)),
        CommandError::BadToken(_) => ApiError::NotSaved(NotSaved::BadToken),
        // The session closed under the write: the UI reattaches and resends with the field's
        // base token (A-19).
        CommandError::Closed => ApiError::NotSaved(NotSaved::StaleScope),
        CommandError::Unavailable => ApiError::Unavailable(Feature::Writes),
        CommandError::LookAgain => ApiError::NotSaved(NotSaved::LookAgain),
        CommandError::PlanStale => ApiError::NotSaved(NotSaved::PlanStale),
        CommandError::Refused(RuleRefusal::Incoherent(_)) => {
            ApiError::NotSaved(NotSaved::Refused(SaveRefusal::IncoherentView))
        }
        CommandError::Refused(RuleRefusal::Text(reason)) => {
            ApiError::NotSaved(NotSaved::Refused(SaveRefusal::InvalidText { reason }))
        }
        CommandError::Refused(RuleRefusal::LastBook) => {
            ApiError::NotSaved(NotSaved::Refused(SaveRefusal::LastBook))
        }
        // The review says why a choice could not be made (C7a).
        CommandError::Refused(refusal @ (RuleRefusal::Resolve(_) | RuleRefusal::Unreadable)) => {
            ApiError::NotSaved(NotSaved::Refused(SaveRefusal::Choice {
                reason: refusal.to_string(),
            }))
        }
        // A review token in a save, or an edit token in a resolution, got past the seam's
        // types and tc-app's parse: a bug, reported as a bad token is.
        CommandError::Refused(RuleRefusal::WrongTokenKind) => {
            ApiError::NotSaved(NotSaved::BadToken)
        }
        CommandError::Refused(refusal) => return Err(UnwordedRefusal(refusal.to_string())),
    })
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
