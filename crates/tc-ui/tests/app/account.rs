//! The Door43 sign-in step (M12): tC4's `si` and its actions as transitions.

use tc_api::ApiError;
use tc_api::account::{AccountView, KeepState, KeychainState};
use tc_api::endpoint::NetKind;
use tc_api::ids::RequestId;
use tc_api::refusal::{Refusal, SignInRefusal};
use tc_ui::app::account::{SignInFlow, SignInForm, SignInNext, SignInProblem};
use tc_ui::app::client::{CallError, Transport};
use tc_ui::app::internet::{InternetFlow, NetStep, Pending};

fn rid(n: u8) -> RequestId {
    RequestId::from_bytes([n; 16])
}

fn signed_in() -> AccountView {
    AccountView {
        user: Some("facilitator-zq".to_owned()),
        kept: KeepState::NotKept,
        notice: None,
        server: "git.door43.org".to_owned(),
        keychain: KeychainState::Works,
    }
}

fn open(flow: u64) -> SignInFlow {
    let mut step = SignInFlow::Idle;
    step.open(flow, None);
    step.edit(|form| {
        form.login = "  facilitator-zq ".to_owned();
        form.password = " a pass-word ".to_owned();
    });
    step
}

fn form(step: &SignInFlow) -> &SignInForm {
    match step {
        SignInFlow::Open(step) => &step.form,
        SignInFlow::Idle => panic!("the step is closed"),
    }
}

fn error(step: &SignInFlow) -> Option<&SignInProblem> {
    match step {
        SignInFlow::Open(step) => step.error.as_ref(),
        SignInFlow::Idle => panic!("the step is closed"),
    }
}

#[test]
fn the_step_opens_empty_with_stay_off_and_no_request() {
    let mut step = SignInFlow::Idle;
    assert_eq!(step.flow(), None);
    step.open(3, None);
    assert_eq!(step.flow(), Some(3));
    assert_eq!(form(&step), &SignInForm::default());
    assert!(!form(&step).stay);
    assert!(!step.busy());
    assert_eq!(error(&step), None);
    // Opening it again starts afresh.
    step.edit(|form| form.login = "someone".into());
    step.open(4, None);
    assert_eq!(form(&step).login, "");
}

#[test]
fn send_trims_the_login_and_a_second_press_sends_nothing() {
    let mut step = open(1);
    let request = step.send(1, rid(1), KeychainState::Works).unwrap();
    assert_eq!(request.login, "facilitator-zq");
    assert_eq!(
        request.password, " a pass-word ",
        "the password goes as typed"
    );
    assert_eq!(request.request, rid(1));
    assert!(step.busy());
    assert_eq!(step.send(1, rid(2), KeychainState::Works), None);
    // Typing still works while it is on its way, as in tC4.
    step.edit(|form| form.login.push('x'));
    assert!(form(&step).login.ends_with('x'));
    // A press for another opening of the step sends nothing.
    let mut other = open(1);
    assert_eq!(other.send(2, rid(3), KeychainState::Works), None);
}

#[test]
fn stay_is_sent_only_when_the_keychain_works() {
    let mut step = open(1);
    step.edit(|form| form.stay = true);
    assert!(step.send(1, rid(1), KeychainState::Works).unwrap().stay);
    let mut step = open(1);
    step.edit(|form| form.stay = true);
    assert!(!step.send(1, rid(1), KeychainState::Missing).unwrap().stay);
}

#[test]
fn a_refusal_shows_its_code_and_empties_the_password() {
    let mut step = open(1);
    step.send(1, rid(1), KeychainState::Works);
    let refused = Err(CallError::Api(ApiError::Refused(Refusal::SignIn(
        SignInRefusal::AuthFailed,
    ))));
    assert_eq!(step.answered(1, rid(1), refused), SignInNext::Nothing);
    assert!(!step.busy());
    let problem = error(&step).unwrap();
    assert_eq!(problem.code(), "share.auth-failed");
    assert_eq!(form(&step).password, "");
    assert_eq!(form(&step).login, "  facilitator-zq ", "the login stays");

    // Another failure shows its own words and no code.
    step.edit(|form| form.password = "again".into());
    step.send(1, rid(2), KeychainState::Works);
    assert_eq!(error(&step), None, "a new press clears the callout");
    let lost = Err(CallError::Transport(Transport::Status(503)));
    step.answered(1, rid(2), lost);
    let problem = error(&step).unwrap();
    assert_eq!(problem.code(), "");
    assert!(matches!(problem, SignInProblem::Other(text) if text.contains("503")));
    assert_eq!(form(&step).password, "");
}

#[test]
fn a_reply_of_a_closed_or_reopened_step_is_dropped() {
    let mut step = open(1);
    step.send(1, rid(1), KeychainState::Works);
    assert_eq!(step.cancel(), Some(rid(1)));
    step.open(2, None);
    step.edit(|form| form.login = "next".into());
    let before = step.clone();
    assert_eq!(
        step.answered(1, rid(1), Ok(signed_in())),
        SignInNext::Nothing
    );
    assert_eq!(
        step, before,
        "the step opened after the Cancel is as it was"
    );
    // A reply to another request of the same step changes nothing either.
    step.send(2, rid(5), KeychainState::Works);
    assert_eq!(
        step.answered(2, rid(4), Ok(signed_in())),
        SignInNext::Nothing
    );
    assert!(step.busy());
}

#[test]
fn cancel_names_the_sign_in_on_its_way() {
    let mut step = open(1);
    assert_eq!(step.cancel(), None, "nothing was sent");
    assert_eq!(step, SignInFlow::Idle);
    let mut step = open(1);
    step.send(1, rid(7), KeychainState::Works);
    assert_eq!(step.cancel(), Some(rid(7)));
    assert_eq!(step.cancel(), None);
}

#[test]
fn a_sign_in_runs_the_step_that_waited() {
    let mut step = SignInFlow::Idle;
    step.open(1, Some(Pending::Profile));
    step.edit(|form| {
        form.login = "facilitator-zq".into();
        form.password = "a pass-word".into();
    });
    step.send(1, rid(1), KeychainState::Works);
    assert_eq!(
        step.answered(1, rid(1), Ok(signed_in())),
        SignInNext::SignedIn(Some(Pending::Profile))
    );
    assert_eq!(step, SignInFlow::Idle, "the step closed");
}

#[test]
fn needs_internet_asks_again() {
    let mut step = open(4);
    step.send(4, rid(1), KeychainState::Works);
    let off = Err(CallError::Api(ApiError::NeedsInternet(NetKind::SignIn)));
    assert_eq!(
        step.answered(4, rid(1), off),
        SignInNext::AskAgain(Pending::SignIn { flow: 4 })
    );
    assert!(!step.busy());
    assert_eq!(error(&step), None);
    assert_eq!(form(&step).password, " a pass-word ", "kept for the resend");
}

#[test]
fn the_form_never_prints_its_password() {
    let step = open(1);
    let printed = format!("{step:?} {:?}", form(&step));
    assert!(!printed.contains("pass-word"), "{printed}");
    assert!(printed.contains("facilitator-zq"));
}

#[test]
fn the_account_steps_ask_their_own_question() {
    assert_eq!(Pending::SignIn { flow: 1 }.kind(), NetKind::SignIn);
    assert_eq!(Pending::CheckSignIn.kind(), NetKind::CheckSignIn);
    assert_eq!(Pending::Profile.kind(), NetKind::Profile);
}

#[test]
fn a_sign_in_question_dropped_by_not_now_runs_nothing() {
    let mut flow = InternetFlow::Idle;
    assert_eq!(
        flow.ask(false, Pending::SignIn { flow: 2 }),
        NetStep::Nothing
    );
    assert_eq!(flow.asking(), Some(NetKind::SignIn));
    // A second press while the question is open is dropped, as tC4's pendingAsk drops it.
    assert_eq!(
        flow.ask(false, Pending::SignIn { flow: 2 }),
        NetStep::Dropped(Pending::SignIn { flow: 2 })
    );
    assert_eq!(
        flow.not_now(),
        NetStep::Dropped(Pending::SignIn { flow: 2 })
    );
    assert_eq!(flow, InternetFlow::Idle);
}
