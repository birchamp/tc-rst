//! The internet switch through the endpoints: refusals before the reply cache, and the
//! `Internet` frame that goes out before the reply.

use std::sync::Arc;

use tc_api::account::{HelpLink, LinkView, SignIn, WebUrl};
use tc_api::endpoint::NetKind;
use tc_api::error::Feature;
use tc_api::event::Event;
use tc_api::ids::RequestId;
use tc_api::net::{NetView, Set, State};
use tc_api::{ApiError, Reply};
use tc_app::events::Next;
use tc_app::{Answer, App};

use super::common::{TestApp, rid};

fn set(app: &App, id: u8, on: bool) -> impl std::future::Future<Output = Answer> + '_ {
    let body = serde_json::to_vec(&Set {
        request: rid(id),
        on,
    })
    .unwrap();
    async move { app.call("net/set", &body).await }
}

fn internet_frames(events: &mut tc_app::events::Subscription) -> Vec<bool> {
    let mut seen = Vec::new();
    while let Some(next) = events.try_next() {
        match next {
            Next::Frame(frame) => {
                if let Event::Internet { on } = frame.event {
                    seen.push(on);
                }
            }
            other => panic!("{other:?}"),
        }
    }
    seen
}

fn sign_in(id: u8) -> SignIn {
    SignIn {
        request: rid(id),
        login: "jdoe".to_owned(),
        password: "secret".to_owned(),
        stay: false,
    }
}

#[tokio::test]
async fn a_net_endpoint_is_refused_while_offline_and_never_cached() {
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_in(dir.path(), |_, _| {}).await;
    let url = WebUrl::parse("https://git.door43.org/help").unwrap();

    let help = t.app.request(&HelpLink { url: url.clone() }).await.unwrap();
    assert_eq!(
        help.into_result(),
        Err(ApiError::NeedsInternet(NetKind::HelpLink))
    );
    let refused = t.app.request(&sign_in(7)).await.unwrap();
    assert_eq!(
        refused.into_result(),
        Err(ApiError::NeedsInternet(NetKind::SignIn))
    );

    // The UI asks, turns the switch on and sends the same request again: that must run.
    t.app
        .request(&Set {
            request: rid(1),
            on: true,
        })
        .await
        .unwrap();
    let again = t.app.request(&sign_in(7)).await.unwrap();
    assert_eq!(
        again.into_result(),
        Err(ApiError::Unavailable(Feature::Account)),
        "the refusal was not kept under the request id"
    );
    let help = t.app.request(&HelpLink { url: url.clone() }).await.unwrap();
    assert_eq!(help.into_result(), Ok(LinkView { url }));

    // The switch going off again is refused again, whatever was answered before.
    t.app
        .request(&Set {
            request: rid(2),
            on: false,
        })
        .await
        .unwrap();
    let off = t.app.request(&sign_in(7)).await.unwrap();
    assert_eq!(
        off.into_result(),
        Err(ApiError::NeedsInternet(NetKind::SignIn))
    );
}

#[tokio::test]
async fn net_state_follows_the_switch() {
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_in(dir.path(), |_, _| {}).await;
    let state = |app: App| async move { app.request(&State {}).await.unwrap().into_result() };
    assert_eq!(state(t.app.clone()).await, Ok(NetView { on: false }));
    t.app
        .request(&Set {
            request: rid(1),
            on: true,
        })
        .await
        .unwrap();
    assert_eq!(state(t.app.clone()).await, Ok(NetView { on: true }));
}

#[tokio::test]
async fn net_set_announces_the_switch_before_answering() {
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_in(dir.path(), |_, _| {}).await;
    let clone_taken_before = t.app.internet();
    let mut events = t.app.events().subscribe(t.app.events().head());

    let Answer::Reply { body, .. } = set(&t.app, 1, true).await else {
        panic!()
    };
    // The frame is already there when the call returns.
    assert_eq!(internet_frames(&mut events), vec![true]);
    let reply: Reply<NetView> = serde_json::from_slice(&body).unwrap();
    assert_eq!(reply, Reply::Ok(NetView { on: true }));
    assert!(clone_taken_before.is_on(), "clones share one switch");

    // Turning it on when it is on says nothing new.
    set(&t.app, 2, true).await;
    assert_eq!(internet_frames(&mut events), Vec::<bool>::new());

    set(&t.app, 3, false).await;
    assert_eq!(internet_frames(&mut events), vec![false]);
    assert!(!clone_taken_before.is_on());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_last_internet_frame_matches_the_switch() {
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_in(dir.path(), |_, _| {}).await;
    let mut events = t.app.events().subscribe(t.app.events().head());
    let app = Arc::new(t.app.clone());

    let mut frames = Vec::new();
    for round in 0..20u8 {
        let mut calls = Vec::new();
        for n in 0..64u8 {
            let app = Arc::clone(&app);
            // A fixed mix of values: the test is about ordering, not about chance.
            let on = (u32::from(n) * 7 + u32::from(n) / 3 + u32::from(round)) % 2 == 0;
            calls.push(tokio::spawn(async move {
                let mut id = [0u8; 16];
                id[0] = round;
                id[1] = n;
                let set = Set {
                    request: RequestId::from_bytes(id),
                    on,
                };
                let body = serde_json::to_vec(&set).unwrap();
                app.call("net/set", &body).await;
            }));
        }
        for call in calls {
            call.await.unwrap();
        }
        frames.extend(internet_frames(&mut events));
        assert_eq!(
            frames.last().copied(),
            Some(app.internet().is_on()),
            "round {round}"
        );
    }
    assert!(
        frames.windows(2).all(|pair| pair[0] != pair[1]),
        "no two neighbours carry the same value: {frames:?}"
    );
}
