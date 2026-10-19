//! The internet question (M15): tC4's `allowInternet` as transitions.

use tc_api::account::WebUrl;
use tc_api::endpoint::NetKind;
use tc_ui::app::internet::{InternetFlow, LinkKind, NetStep, Pending};

fn link() -> Pending {
    Pending::Link(
        LinkKind::Help,
        WebUrl::parse("https://github.com/unfoldingWord/translationCore4/releases").unwrap(),
    )
}

#[test]
fn an_open_internet_runs_at_once() {
    let mut flow = InternetFlow::Idle;
    assert_eq!(
        flow.ask(true, Pending::Feedback),
        NetStep::Run(Pending::Feedback)
    );
    assert_eq!(flow, InternetFlow::Idle);
}

#[test]
fn a_closed_internet_asks_with_the_steps_kind() {
    let mut flow = InternetFlow::Idle;
    assert_eq!(flow.ask(false, link()), NetStep::Nothing);
    assert_eq!(flow.asking(), Some(NetKind::HelpLink));
    let mut flow = InternetFlow::Idle;
    flow.ask(false, Pending::Feedback);
    assert_eq!(
        flow,
        InternetFlow::Asking {
            kind: NetKind::Feedback,
            then: Pending::Feedback
        }
    );
}

#[test]
fn not_now_runs_nothing() {
    let mut flow = InternetFlow::Idle;
    flow.ask(false, Pending::Feedback);
    assert_eq!(flow.not_now(), NetStep::Dropped(Pending::Feedback));
    assert_eq!(flow, InternetFlow::Idle);
    // Not now with no question does nothing.
    assert_eq!(flow.not_now(), NetStep::Nothing);
}

#[test]
fn turn_on_runs_the_step_once() {
    let mut flow = InternetFlow::Idle;
    flow.ask(false, link());
    assert_eq!(flow.turn_on(), NetStep::TurnOn);
    assert_eq!(flow.asking(), Some(NetKind::HelpLink));
    // A second press while net/set is on its way does nothing.
    assert_eq!(flow.turn_on(), NetStep::Nothing);
    assert_eq!(flow.not_now(), NetStep::Nothing);
    assert_eq!(flow.turned_on(true), NetStep::Run(link()));
    assert_eq!(flow, InternetFlow::Idle);
    assert_eq!(flow.turned_on(true), NetStep::Nothing);
}

#[test]
fn a_failed_turn_on_shows_failed_and_runs_nothing() {
    let mut flow = InternetFlow::Idle;
    flow.ask(false, Pending::Feedback);
    flow.turn_on();
    assert_eq!(flow.turned_on(false), NetStep::Dropped(Pending::Feedback));
    assert_eq!(flow, InternetFlow::Failed);
    assert!(flow.is_open());
    assert_eq!(flow.asking(), None);
    // A step while the failure shows is dropped, as tC4's open question drops it.
    assert_eq!(flow.ask(false, link()), NetStep::Dropped(link()));
    flow.close_failed();
    assert_eq!(flow, InternetFlow::Idle);
}

#[test]
fn a_second_step_while_a_question_is_open_is_dropped() {
    let mut flow = InternetFlow::Idle;
    flow.ask(false, Pending::Feedback);
    assert_eq!(flow.ask(false, link()), NetStep::Dropped(link()));
    assert_eq!(flow.asking(), Some(NetKind::Feedback));
    flow.turn_on();
    assert_eq!(flow.ask(false, link()), NetStep::Dropped(link()));
}

#[test]
fn needs_internet_asks_once_more_then_drops() {
    let mut flow = InternetFlow::Idle;
    assert_eq!(flow.needs_internet(link(), false), NetStep::Nothing);
    assert_eq!(flow.asking(), Some(NetKind::HelpLink));
    let mut flow = InternetFlow::Idle;
    assert_eq!(flow.needs_internet(link(), true), NetStep::Dropped(link()));
    assert_eq!(flow, InternetFlow::Idle);
}

#[test]
fn each_step_asks_its_own_question() {
    assert_eq!(Pending::Feedback.kind(), NetKind::Feedback);
    assert_eq!(link().kind(), NetKind::HelpLink);
    assert_eq!(NetKind::HelpLink.ask_key(), "net.ask.helpLink");
}
