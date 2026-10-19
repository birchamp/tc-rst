//! A confirming button of a dialog (APP-DESIGN 4.8, RT-27): one request id for the dialog,
//! and the button off while its request is in flight, so a double click neither gets a false
//! refusal ("Look again" after the first click succeeded) nor writes twice.

use tc_api::ids::RequestId;

/// Where a confirming button stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmState {
    Idle,
    InFlight,
    Answered,
}

/// The one request of a dialog.
#[derive(Debug, Clone)]
pub struct Confirm {
    request: RequestId,
    state: ConfirmState,
}

impl Confirm {
    /// When the dialog opens, with a fresh id.
    pub fn new(request: RequestId) -> Self {
        Self {
            request,
            state: ConfirmState::Idle,
        }
    }

    /// A press: the id to send, or `None` while one is in flight or answered.
    pub fn press(&mut self) -> Option<RequestId> {
        if self.state != ConfirmState::Idle {
            return None;
        }
        self.state = ConfirmState::InFlight;
        Some(self.request)
    }

    /// The reply was lost: the same request goes again, which the reply cache answers.
    pub fn lost(&mut self) -> RequestId {
        self.state = ConfirmState::InFlight;
        self.request
    }

    pub fn answered(&mut self) {
        self.state = ConfirmState::Answered;
    }

    pub fn state(&self) -> ConfirmState {
        self.state
    }

    /// Whether the button is off.
    pub fn disabled(&self) -> bool {
        self.state != ConfirmState::Idle
    }
}
