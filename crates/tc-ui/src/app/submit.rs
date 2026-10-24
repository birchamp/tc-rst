//! INV-ONE-REQUEST-PER-ATTEMPT (UI-3): a dialog's or a button's request, once per attempt.
//! Built on [`Confirm`]: a press sends the attempt's id, a second press in flight sends
//! nothing, a lost reply leaves the next press to send the same id (the server's reply cache
//! answers it, A-27), and a refusal shows its problem and makes the next press a new attempt.

use tc_api::ids::RequestId;

use crate::app::confirm::Confirm;

/// One attempt's request and what the last attempt left to say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submit {
    confirm: Confirm,
    problem: Option<String>,
}

impl Submit {
    /// A new attempt with `request`.
    pub fn new(request: RequestId) -> Self {
        Self {
            confirm: Confirm::new(request),
            problem: None,
        }
    }

    /// A press: the id to send, or `None` while the attempt is in flight or answered. The
    /// last problem goes with the press.
    pub fn press(&mut self) -> Option<RequestId> {
        let request = self.confirm.press()?;
        self.problem = None;
        Some(request)
    }

    /// No usable reply: `problem` says why, and the next press sends the same request again.
    pub fn lost(&mut self, problem: String) {
        self.confirm = Confirm::new(self.confirm.lost());
        self.problem = Some(problem);
    }

    /// The press was not sent after all (Not now at the internet question): the button is on
    /// again, with the same request.
    pub fn not_sent(&mut self) {
        self.confirm = Confirm::new(self.confirm.lost());
    }

    /// A new attempt with `next`, with nothing to say: the internet question or the sign-in
    /// step took over, or the run ended without an answer to show.
    pub fn next(&mut self, next: RequestId) {
        self.confirm = Confirm::new(next);
        self.problem = None;
    }

    /// The request of the attempt, in flight or next.
    pub fn request(&self) -> RequestId {
        self.confirm.request()
    }

    /// The server answered and the attempt is over.
    pub fn answered(&mut self) {
        self.confirm.answered();
    }

    /// The server refused, for `problem`: the next press is a new attempt with `next`.
    pub fn refused(&mut self, problem: String, next: RequestId) {
        self.confirm = Confirm::new(next);
        self.problem = Some(problem);
    }

    /// The attempt's request is in flight: the button is off.
    pub fn busy(&self) -> bool {
        self.confirm.state() == crate::app::confirm::ConfirmState::InFlight
    }

    /// What the last attempt left to say.
    pub fn problem(&self) -> Option<&str> {
        self.problem.as_deref()
    }
}
