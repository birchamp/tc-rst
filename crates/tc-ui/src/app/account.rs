//! The Door43 sign-in step (M12, APP-DESIGN 5.6, 6.7, unit A8): tC4's `si` and its actions
//! (`openSignIn`, `patchSi`, `submitSignIn`, `signInNow`, `tC4:src/state.jsx:5818-5895`) as a
//! value with transitions.
//!
//! The step opens empty with "Stay signed in" off. Sign in asks the internet question while
//! the switch is off (tC4 asks when the password is sent, not when the step opens), then sends
//! `account/signIn` once: a second press while it is on its way sends nothing. A reply lands
//! only in the step it was sent from, so a step closed with Cancel, or one opened after it, is
//! never signed in by an earlier press. A refusal shows in the step with its code and empties
//! the password, as tC4's does. Cancel closes the step at once and names the sign-in on its
//! way, which the server then cancels (`account/cancelSignIn`). The password is never written
//! anywhere: not to the draft buffer, not to page storage, not to a log.

use std::fmt;

use tc_api::ApiError;
use tc_api::account::{AccountView, KeychainState, SignIn};
use tc_api::ids::RequestId;
use tc_api::refusal::{Message, Refusal, SignInRefusal};

use crate::app::client::CallError;
use crate::app::internet::Pending;
use crate::app::shell::js_trim;

/// M12's fields (tC4's `si.login`, `si.password`, `si.stay`).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct SignInForm {
    pub login: String,
    pub password: String,
    pub stay: bool,
}

/// An assertion that fails prints its values: never the password.
impl fmt::Debug for SignInForm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignInForm")
            .field("login", &self.login)
            .field("password", &"..")
            .field("stay", &self.stay)
            .finish()
    }
}

/// What the step's callout says: a refusal in its words, or another failure's own text (tC4
/// shows the report's message when a failure has no code).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignInProblem {
    Refused(SignInRefusal),
    Other(String),
}

impl SignInProblem {
    /// The callout's `data-code`: tC4's code (`share.auth-failed`, …), or nothing.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Refused(refusal) => refusal.code().unwrap_or_default(),
            Self::Other(_) => "",
        }
    }
}

/// The open step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignInStep {
    /// Which opening of the step this is (tC4's `si.opened`).
    pub flow: u64,
    pub form: SignInForm,
    /// The step that waited for the sign-in, run once it is done (6.7).
    pub then: Option<Pending>,
    pub error: Option<SignInProblem>,
    /// The sign-in on its way (tC4's `si.busy`), by its request id.
    pub sending: Option<RequestId>,
}

/// tC4's `s.modal === 'signIn' && s.si`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SignInFlow {
    #[default]
    Idle,
    Open(SignInStep),
}

/// What the caller does after a reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignInNext {
    Nothing,
    /// Signed in: the step closed; run the step that waited for it.
    SignedIn(Option<Pending>),
    /// `NeedsInternet`: the switch went off after the question. Ask again (AD25), once.
    AskAgain(Pending),
}

impl SignInFlow {
    /// Opens the step, empty, "Stay signed in" off (tC4's `openSignIn`). `then` runs once
    /// signed in.
    pub fn open(&mut self, flow: u64, then: Option<Pending>) {
        *self = Self::Open(SignInStep {
            flow,
            form: SignInForm::default(),
            then,
            error: None,
            sending: None,
        });
    }

    /// A change of a field, any time the step is open, a sign-in on its way included (tC4's
    /// fields stay enabled).
    pub fn edit(&mut self, change: impl FnOnce(&mut SignInForm)) {
        if let Self::Open(step) = self {
            change(&mut step.form);
        }
    }

    /// The opening the step is in, when it is open.
    pub fn flow(&self) -> Option<u64> {
        match self {
            Self::Open(step) => Some(step.flow),
            Self::Idle => None,
        }
    }

    /// A sign-in is on its way: Sign in reads "Signing in…" and is off.
    pub fn busy(&self) -> bool {
        matches!(self, Self::Open(step) if step.sending.is_some())
    }

    /// Sign in, once the internet is on: the request to send, or `None` when the step it was
    /// pressed in is gone or a sign-in is on its way already. The login is trimmed as tC4
    /// trims it (`si.login.trim()`); the password goes as typed; "Stay signed in" goes only
    /// when the keychain can keep a sign-in. The callout clears.
    pub fn send(
        &mut self,
        flow: u64,
        request: RequestId,
        keychain: KeychainState,
    ) -> Option<SignIn> {
        let Self::Open(step) = self else {
            return None;
        };
        if step.flow != flow || step.sending.is_some() {
            return None;
        }
        step.sending = Some(request);
        step.error = None;
        Some(SignIn {
            request,
            login: js_trim(&step.form.login).to_owned(),
            password: step.form.password.clone(),
            stay: step.form.stay && keychain == KeychainState::Works,
        })
    }

    /// The reply to `request`, sent from the step `flow`. A reply of another step or another
    /// request changes nothing.
    pub fn answered(
        &mut self,
        flow: u64,
        request: RequestId,
        reply: Result<AccountView, CallError>,
    ) -> SignInNext {
        let Self::Open(step) = self else {
            return SignInNext::Nothing;
        };
        if step.flow != flow || step.sending != Some(request) {
            return SignInNext::Nothing;
        }
        step.sending = None;
        let problem = match reply {
            Ok(view) if view.user.is_some() => {
                let then = step.then.take();
                *self = Self::Idle;
                return SignInNext::SignedIn(then);
            }
            // The server cancelled it (a test reset): nobody signed in, nothing to say.
            Ok(_) => return SignInNext::Nothing,
            Err(CallError::Api(ApiError::NeedsInternet(_))) => {
                return SignInNext::AskAgain(Pending::SignIn { flow });
            }
            Err(CallError::Api(ApiError::Refused(Refusal::SignIn(refusal)))) => {
                SignInProblem::Refused(refusal)
            }
            Err(other) => SignInProblem::Other(other.to_string()),
        };
        step.form.password.clear();
        step.error = Some(problem);
        SignInNext::Nothing
    }

    /// Cancel, Escape, the ✕ or the scrim: the step closes, and the sign-in on its way, if
    /// any, is named for `account/cancelSignIn`.
    pub fn cancel(&mut self) -> Option<RequestId> {
        match std::mem::take(self) {
            Self::Open(step) => step.sending,
            Self::Idle => None,
        }
    }
}
