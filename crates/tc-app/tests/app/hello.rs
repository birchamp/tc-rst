//! `app/hello` answers from live state, in the bytes the server sends.

use std::sync::Arc;

use serde_json::{Value, json};
use tc_api::Reply;
use tc_api::account::{AccountView, KeepState};
use tc_api::app::{AppPrefsView, Hello, HelloView};
use tc_api::event::{Event, StartupProgress, StartupState};
use tc_api::ids::LocaleId;
use tc_api::net::Set;
use tc_api::prefs::SetLocale;
use tc_app::events::Next;
use tc_app::{Answer, App, Then};

use super::common::{Parts, TestApp, build, hello_value, open_session, rid, token};

/// The row's "done when": `hello` has real state, in the bytes the server sends.
#[tokio::test]
async fn hello_answers_real_state() {
    let dir = tempfile::tempdir().unwrap();
    let parts = Parts::new();
    parts.keychain.seed(token("kept-secret"));
    let (scope, info, open) = open_session(3);
    parts.sessions.add(scope, info, open.clone());
    let t = TestApp::start_with(dir.path(), &parts, |_, _| {}).await;
    let app = &t.app;

    let answer = app.call("app/hello", b"{}").await;
    let Answer::Reply { body, then } = answer else {
        panic!("{answer:?}")
    };
    assert_eq!(then, Then::Continue);

    // Typed, and as the JSON the UI reads.
    let typed: Reply<HelloView> = serde_json::from_slice(&body).unwrap();
    let Reply::Ok(hello) = typed else {
        panic!("{typed:?}")
    };
    assert_eq!(hello.api, 1);
    assert_eq!(hello.build, build());
    assert_eq!(hello.instance, app.instance());
    assert!(!hello.internet);
    assert_eq!(
        hello.account,
        AccountView {
            user: None,
            kept: KeepState::Kept,
            notice: None
        }
    );
    assert_eq!(hello.sessions, vec![open]);
    assert_eq!(
        hello.prefs,
        AppPrefsView {
            app_locale: LocaleId::parse("en").unwrap()
        }
    );
    assert_eq!(hello.startup, StartupState::Ready);

    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "ok");
    assert_eq!(json["value"]["api"], 1);
    assert_eq!(
        json["value"]["build"],
        json!({"version": "4.0.0-test", "commit": "0123abc"})
    );
    assert_eq!(json["value"]["instance"], app.instance().to_string());
    assert_eq!(json["value"]["internet"], false);
    assert_eq!(
        json["value"]["account"],
        json!({"user": null, "kept": "kept", "notice": null})
    );
    assert_eq!(json["value"]["sessions"][0]["role"], "screen");
    assert_eq!(json["value"]["prefs"], json!({"appLocale": "en"}));
    assert_eq!(json["value"]["startup"], json!({"type": "ready"}));
    assert!(
        !String::from_utf8_lossy(&body).contains("kept-secret"),
        "the token never reaches the UI"
    );

    // It follows the state it reports.
    app.request(&Set {
        request: rid(1),
        on: true,
    })
    .await
    .unwrap();
    assert_eq!(hello_value(app).await["internet"], true);

    app.request(&SetLocale {
        request: rid(2),
        locale: LocaleId::parse("ar").unwrap(),
    })
    .await
    .unwrap();
    assert_eq!(hello_value(app).await["prefs"]["appLocale"], "ar");

    app.set_startup(StartupState::Preparing(StartupProgress {
        done: 1,
        total: 10,
    }));
    assert_eq!(
        hello_value(app).await["startup"],
        json!({"type": "preparing", "value": {"done": 1, "total": 10}})
    );

    // Another launch is another instance, and the switch is never stored (§5.6).
    let second = TestApp::start_with(dir.path(), &Parts::new(), |_, _| {}).await;
    assert_ne!(second.app.instance(), app.instance());
    let value = hello_value(&second.app).await;
    assert_eq!(value["internet"], false);
    assert_eq!(value["instance"], second.app.instance().to_string());
}

#[tokio::test]
async fn hello_through_the_typed_call_matches_the_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_in(dir.path(), |_, _| {}).await;
    let typed = t
        .app
        .request(&Hello {})
        .await
        .unwrap()
        .into_result()
        .unwrap();
    assert_eq!(
        serde_json::to_value(&typed).unwrap(),
        hello_value(&t.app).await
    );
}

#[tokio::test]
async fn set_startup_is_announced_and_in_hello() {
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_in(dir.path(), |_, _| {}).await;
    let mut events = t.app.events().subscribe(t.app.events().head());
    t.app.set_startup(StartupState::Failed {
        reason: "disk full".to_owned(),
    });
    let Some(Next::Frame(frame)) = events.try_next() else {
        panic!("no frame")
    };
    assert_eq!(
        frame.event,
        Event::Startup(StartupState::Failed {
            reason: "disk full".to_owned()
        })
    );
    assert_eq!(hello_value(&t.app).await["startup"]["type"], "failed");
}

/// Compile-time: tc-server's `Backend` needs a `Send` call future and an `App` it can share.
#[tokio::test]
async fn app_and_its_call_future_are_send() {
    fn send<T: Send>(_: &T) {}
    fn shared<T: Send + Sync + Clone + 'static>() {}
    shared::<App>();
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_in(dir.path(), |_, _| {}).await;
    let call = t.app.call("app/hello", b"{}");
    send(&call);
    let _ = Arc::new(call.await);
}
