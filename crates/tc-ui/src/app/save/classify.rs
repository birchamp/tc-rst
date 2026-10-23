//! What a write's answer means for the field that sent it (APP-DESIGN 6.5, AD4, A-2, A-19).
//!
//! One exhaustive match with no wildcard on `ApiError`, `NotSaved` or `Transport`: an error
//! tc-api adds does not compile here until someone decides whether it is a lost reply, a
//! failure or a reason to send again.

use tc_api::write::Ack;
use tc_api::{ApiError, NotSaved};

use crate::app::client::{CallError, Transport};

/// Why a save stored nothing, as the field shows it: "Not saved … Retry".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    NotSaved(NotSaved),
    /// An answer that is not `NotSaved`: a bug report, a backend that cannot save yet
    /// (`Unavailable` before S1), a refused request.
    Api(ApiError),
    /// A status or a body below the API: a refused token, a proxy's page.
    Transport(Transport),
    /// An `Ack` this build cannot read for this field.
    BadAck(&'static str),
}

impl SaveError {
    /// A field validation refusal is actionable beside the existing Not saved/Retry label.
    pub fn invalid_text_reason(&self) -> Option<&str> {
        match self {
            Self::NotSaved(NotSaved::Refused(tc_api::write::SaveRefusal::InvalidText {
                reason,
            })) => Some(reason),
            _ => None,
        }
    }
}

/// What the save machine does with an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answered {
    Acked(Ack),
    /// No answer reached the page: the request may or may not have run. It is sent again with
    /// the same id and body, which the reply cache answers (A-2).
    Lost,
    /// The project was closed or opened again under the field: send the text again to the
    /// current open with the field's base token and a new id (A-19).
    Reattach,
    /// The id came back with another body: a bug. Send the current text with a new id.
    Reissue,
    Failed(SaveError),
}

/// Reads a write's answer.
pub fn classify(reply: Result<Ack, CallError>) -> Answered {
    match reply {
        Ok(ack) => Answered::Acked(ack),
        Err(CallError::Api(error)) => match error {
            ApiError::NotSaved(NotSaved::StaleScope) | ApiError::StaleScope { .. } => {
                Answered::Reattach
            }
            ApiError::RequestReused => Answered::Reissue,
            // The page reloads for the other build (3.10); a page that stays sends the same
            // request again.
            ApiError::Version { .. } => Answered::Lost,
            ApiError::NotSaved(
                reason @ (NotSaved::Refused(_)
                | NotSaved::Disk(_)
                | NotSaved::ReadOnly(_)
                | NotSaved::LookAgain
                | NotSaved::PlanStale
                | NotSaved::BadToken),
            ) => Answered::Failed(SaveError::NotSaved(reason)),
            error @ (ApiError::BadRequest { .. }
            | ApiError::StaleSession
            | ApiError::ToolNotReady
            | ApiError::Refused(_)
            | ApiError::NeedsInternet(_)
            | ApiError::NeedsSignIn(_)
            | ApiError::Busy(_)
            | ApiError::Unavailable(_)
            | ApiError::Internal { .. }) => Answered::Failed(SaveError::Api(error)),
        },
        // A write's reply is never dropped (6.2); were one dropped, the request may have run.
        Err(CallError::Dropped) => Answered::Lost,
        Err(CallError::Transport(transport)) => match transport {
            Transport::Unreachable(_) | Transport::Status(503) => Answered::Lost,
            Transport::Status(_) | Transport::Unreadable(_) => {
                Answered::Failed(SaveError::Transport(transport))
            }
        },
    }
}
