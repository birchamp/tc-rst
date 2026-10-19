//! The internet question (M15): tC4's `allowInternet` as transitions.

use tc_api::account::WebUrl;
use tc_api::endpoint::NetKind;
use tc_ui::app::internet::{InternetFlow, LinkKind, NetStep, Pending, SwitchRequests, SwitchStep};

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
    // The question closes at once, as tC4's `answerAsk` closes it, and nothing else shows.
    assert_eq!(flow.asking(), None);
    assert!(flow.is_open());
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

/// A server that answers each request with the state it asked for, in the order sent, as
/// `net/set` does; `presses` toggles the switch that many times before any reply comes.
fn presses_before_replies(confirmed: bool, presses: usize) -> (Vec<bool>, bool) {
    let mut switch = SwitchRequests::default();
    let mut queue = Vec::new();
    for _ in 0..presses {
        let on = !switch.base(confirmed);
        queue.extend(switch.ask(on));
    }
    let mut sent = Vec::new();
    let mut server = confirmed;
    while let Some(on) = queue.pop() {
        sent.push(on);
        server = on;
        match switch.answered(Some(server)) {
            SwitchStep::Send(next) => queue.push(next),
            SwitchStep::Settled(state) => assert_eq!(state, Some(server)),
        }
    }
    assert_eq!(
        switch,
        SwitchRequests::default(),
        "nothing is left on its way"
    );
    (sent, server)
}

#[test]
fn a_double_click_on_the_switch_turns_it_twice() {
    // tC4's `setInternet` is synchronous: two presses are on, then off.
    let (sent, server) = presses_before_replies(false, 2);
    assert_eq!(sent, [true, false]);
    assert!(!server);
    let (sent, server) = presses_before_replies(true, 2);
    assert_eq!(sent, [false, true]);
    assert!(server);
}

#[test]
fn presses_before_a_reply_end_at_the_last_choice() {
    for presses in 1..=7 {
        for confirmed in [false, true] {
            let (sent, server) = presses_before_replies(confirmed, presses);
            assert_eq!(
                server,
                confirmed ^ (presses % 2 == 1),
                "{presses} presses from {confirmed}"
            );
            // One request at a time, at most two in all: the first and the last choice.
            assert!(!sent.is_empty() && sent.len() <= 2, "{sent:?}");
        }
    }
}

#[test]
fn a_press_after_the_reply_turns_away_from_the_servers_state() {
    let mut switch = SwitchRequests::default();
    assert_eq!(switch.ask(true), Some(true));
    assert!(switch.base(false), "on its way: the state asked for");
    assert_eq!(switch.answered(Some(true)), SwitchStep::Settled(Some(true)));
    assert!(switch.base(true));
    // The `Internet` event turned it off meanwhile (another page, a restart): a press turns it on.
    assert!(!switch.base(false));
}

#[test]
fn a_failed_request_sends_the_last_choice_or_settles_failed() {
    let mut switch = SwitchRequests::default();
    switch.ask(true);
    assert_eq!(switch.answered(None), SwitchStep::Settled(None));
    assert!(!switch.base(false), "a failure leaves the server's state");
    switch.ask(true);
    assert_eq!(switch.ask(false), None);
    assert_eq!(switch.answered(None), SwitchStep::Send(false));
    assert_eq!(
        switch.answered(Some(false)),
        SwitchStep::Settled(Some(false))
    );
}

#[test]
fn turn_on_waits_for_the_switch_request_on_its_way() {
    // The menu sent "off"; the question's "Turn on internet" asks for on after it.
    let mut switch = SwitchRequests::default();
    let mut flow = InternetFlow::Idle;
    assert_eq!(switch.ask(false), Some(false));
    flow.ask(false, Pending::Feedback);
    assert_eq!(flow.turn_on(), NetStep::TurnOn);
    assert_eq!(
        switch.ask(true),
        None,
        "it waits for the request on its way"
    );
    assert_eq!(switch.answered(Some(false)), SwitchStep::Send(true));
    let SwitchStep::Settled(state) = switch.answered(Some(true)) else {
        panic!("settled");
    };
    assert_eq!(
        flow.turned_on(state == Some(true)),
        NetStep::Run(Pending::Feedback)
    );
}
