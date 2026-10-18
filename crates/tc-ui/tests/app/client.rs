//! The errors of a call, the page's token and build checks, and `Load` (APP-DESIGN 6.2, 3.10,
//! 5.2).

use tc_api::app::{BUILD_COMMIT, Hello};
use tc_api::endpoint::EVENTS_PATH;
use tc_api::error::Flow;
use tc_api::net::NetView;
use tc_api::upload::UPLOADS_PATH;
use tc_api::{ALL, API_VERSION, ApiError, Reply};
use tc_ui::app::boot::{
    BuildCheck, RELOADED_FOR_KEY, TOKEN_KEY, Token, check_build, token_from, unkept_marker,
};
use tc_ui::app::client::{API_ROOT, CallError, Transport, api_url, interpret};
use tc_ui::app::load::Load;

const TOKEN: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOP-";
const OTHER_TOKEN: &str = "_bcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNO-9";

fn json<T: serde::Serialize>(reply: &Reply<T>) -> String {
    serde_json::to_string(reply).unwrap()
}

#[test]
fn an_ok_reply_is_its_value() {
    let body = json(&Reply::Ok(NetView { on: true }));
    assert_eq!(interpret::<NetView>(200, &body), Ok(NetView { on: true }));
    // A command that answers nothing.
    let unit = json(&Reply::Ok(()));
    assert_eq!(interpret::<()>(200, &unit), Ok(()));
}

#[test]
fn an_err_reply_is_the_api_error() {
    for error in [
        ApiError::Busy(Flow::Sync),
        ApiError::StaleScope { current: None },
        ApiError::RequestReused,
    ] {
        let body = json(&Reply::<NetView>::Err(error.clone()));
        assert_eq!(interpret::<NetView>(200, &body), Err(CallError::Api(error)));
    }
}

#[test]
fn a_409_carries_the_version_error() {
    let body = json(&Reply::<()>::Err(ApiError::Version { server: 7 }));
    let error = interpret::<NetView>(409, &body).unwrap_err();
    assert_eq!(error, CallError::Api(ApiError::Version { server: 7 }));
    assert_eq!(error.version_mismatch(), Some(7));
    // A 409 that is not that reply is a failure below the API.
    assert_eq!(
        interpret::<NetView>(409, "conflict"),
        Err(CallError::Transport(Transport::Status(409)))
    );
    let ok_body = json(&Reply::Ok(()));
    assert_eq!(
        interpret::<NetView>(409, &ok_body),
        Err(CallError::Transport(Transport::Status(409)))
    );
    // Other refusals are not a version mismatch.
    assert_eq!(CallError::Dropped.version_mismatch(), None);
    assert_eq!(
        CallError::Api(ApiError::StaleSession).version_mismatch(),
        None
    );
}

#[test]
fn other_statuses_are_transport_failures() {
    // The statuses and plain-text bodies tc-server answers with below the API.
    let valid = json(&Reply::Ok(NetView { on: true }));
    for (status, body) in [
        (401, "unauthorized"),
        (403, "forbidden"),
        (404, ""),
        (413, "request body too large"),
        (500, "internal error"),
        (503, ""),
        // A valid reply under a status the API never uses is still not a reply.
        (500, valid.as_str()),
        (204, ""),
    ] {
        assert_eq!(
            interpret::<NetView>(status, body),
            Err(CallError::Transport(Transport::Status(status))),
            "{status} {body:?}"
        );
    }
}

#[test]
fn an_unreadable_body_is_a_transport_failure() {
    for body in [
        "<html>a proxy's page</html>",
        "",
        r#"{"type":"ok","value":{"nope":1}}"#,
        r#"{"type":"weird"}"#,
        r#"{"type":"err","value":{"type":"notAnError"}}"#,
    ] {
        assert!(
            matches!(
                interpret::<NetView>(200, body),
                Err(CallError::Transport(Transport::Unreadable(_)))
            ),
            "{body:?}"
        );
    }
}

#[test]
fn api_paths_are_under_the_api_root() {
    assert_eq!(API_ROOT, "/api/v1/");
    assert_eq!(api_url::<Hello>(), "/api/v1/app/hello");
    assert!(EVENTS_PATH.starts_with(API_ROOT));
    assert!(UPLOADS_PATH.starts_with(API_ROOT));
    for endpoint in ALL {
        let path = endpoint.path;
        assert!(
            !path.starts_with('/') && path.matches('/').count() == 1,
            "{path} is service/name"
        );
    }
}

#[test]
fn the_test_tokens_are_43_characters_of_the_alphabet() {
    for token in [TOKEN, OTHER_TOKEN] {
        assert_eq!(token.len(), 43);
        assert!(Token::parse(token).is_some());
    }
}

#[test]
fn the_fragment_token_wins() {
    for fragment in [
        format!("#t={TOKEN}"),
        format!("t={TOKEN}"),
        format!("#x=1&t={TOKEN}&y=2"),
        format!("#t={TOKEN}&t={OTHER_TOKEN}"),
    ] {
        assert_eq!(
            token_from(&fragment, Some(OTHER_TOKEN)).map(|t| t.as_str().to_owned()),
            Some(TOKEN.to_owned()),
            "{fragment}"
        );
    }
}

#[test]
fn a_malformed_fragment_falls_back_to_the_kept_token() {
    let too_long = format!("{TOKEN}A");
    let plus = TOKEN.replacen('a', "+", 1);
    for fragment in [
        String::new(),
        "#".to_owned(),
        "#t=".to_owned(),
        "#t=short".to_owned(),
        format!("#t={too_long}"),
        format!("#t={plus}"),
        format!("#T={TOKEN}"),
        format!("#xt={TOKEN}"),
    ] {
        assert_eq!(
            token_from(&fragment, Some(OTHER_TOKEN)).map(|t| t.as_str().to_owned()),
            Some(OTHER_TOKEN.to_owned()),
            "{fragment:?}"
        );
    }
}

#[test]
fn no_token_anywhere_is_none() {
    assert_eq!(token_from("", None), None);
    assert_eq!(token_from("#t=bad", None), None);
    assert_eq!(token_from("", Some("bad")), None);
    assert_eq!(token_from("#t=bad", Some("")), None);
}

#[test]
fn the_token_never_shows_in_debug() {
    let token = Token::parse(TOKEN).unwrap();
    assert_eq!(format!("{token:?}"), "Token(..)");
    assert_eq!(format!("{:?}", Some(&token)), "Some(Token(..))");
    assert!(!format!("{:#?}", token_from(&format!("#t={TOKEN}"), None)).contains("abcdef"));
    assert_eq!(token.as_str(), TOKEN);
}

#[test]
fn the_storage_keys_are_not_catalog_namespaces() {
    // tests/i18n_keys.rs scans for strings that start with a catalog namespace.
    assert_eq!(TOKEN_KEY, "tc4.token");
    assert_eq!(RELOADED_FOR_KEY, "tc4.reloadedFor");
}

#[test]
fn the_same_build_proceeds() {
    assert_eq!(
        check_build(BUILD_COMMIT, API_VERSION, None),
        BuildCheck::Same
    );
    // A marker left by an earlier reload does not matter once the builds agree.
    assert_eq!(
        check_build(BUILD_COMMIT, API_VERSION, Some("0123abc/1")),
        BuildCheck::Same
    );
}

#[test]
fn another_build_reloads_once_then_stops() {
    let first = check_build("0123abc", API_VERSION, None);
    let BuildCheck::ReloadOnce { marker } = first else {
        panic!("{first:?}");
    };
    assert_eq!(marker, format!("0123abc/{API_VERSION}"));
    // The reload left the marker, and the builds still differ.
    let second = check_build("0123abc", API_VERSION, Some(&marker));
    let BuildCheck::Stale { ui, server } = second else {
        panic!("{second:?}");
    };
    assert!(ui.contains(BUILD_COMMIT), "{ui}");
    assert!(server.contains("0123abc"), "{server}");
    // A different mismatch is worth one reload of its own.
    assert!(matches!(
        check_build("fffffff", API_VERSION, Some(&marker)),
        BuildCheck::ReloadOnce { .. }
    ));
}

#[test]
fn another_api_version_reloads_once_then_stops() {
    let newer = API_VERSION + 1;
    let first = check_build(BUILD_COMMIT, newer, None);
    let BuildCheck::ReloadOnce { marker } = first else {
        panic!("{first:?}");
    };
    assert_eq!(
        check_build(BUILD_COMMIT, newer, Some(&marker)),
        BuildCheck::Stale {
            ui: format!("{BUILD_COMMIT} (API {API_VERSION})"),
            server: format!("{BUILD_COMMIT} (API {newer})"),
        }
    );
}

#[test]
fn a_page_that_cannot_keep_the_marker_does_not_reload_for_ever() {
    // sessionStorage is blocked: the marker of the reload cannot be written, so the page after
    // the reload would see none and reload again. The mismatch is a stale page at once.
    let BuildCheck::ReloadOnce { .. } = check_build("0123abc", API_VERSION, None) else {
        panic!("the first look reloads");
    };
    assert_eq!(
        unkept_marker("0123abc", API_VERSION),
        check_build(
            "0123abc",
            API_VERSION,
            Some(&format!("0123abc/{API_VERSION}"))
        )
    );
    assert!(matches!(
        unkept_marker("0123abc", API_VERSION),
        BuildCheck::Stale { .. }
    ));
    assert!(matches!(
        unkept_marker(BUILD_COMMIT, API_VERSION + 1),
        BuildCheck::Stale { .. }
    ));
}

#[test]
fn refreshing_keeps_the_value_and_a_failure_does_not() {
    let failed: Load<u8> = Load::Failed(CallError::Dropped);
    assert_eq!(Load::Ready(3).refreshing(), Load::Refreshing(3));
    assert_eq!(Load::Refreshing(3).refreshing(), Load::Refreshing(3));
    assert_eq!(Load::<u8>::Loading.refreshing(), Load::Loading);
    assert_eq!(failed.clone().refreshing(), Load::Loading);

    assert_eq!(Load::Ready(3).value(), Some(&3));
    assert_eq!(Load::Refreshing(3).value(), Some(&3));
    assert_eq!(Load::<u8>::Loading.value(), None);
    assert_eq!(failed.value(), None);

    assert!(Load::<u8>::Loading.is_pending());
    assert!(Load::Refreshing(3).is_pending());
    assert!(!Load::Ready(3).is_pending());
    assert!(!failed.is_pending());

    // A failure is its own state, never an absent value.
    assert_eq!(failed.error(), Some(&CallError::Dropped));
    assert_eq!(Load::Ready(3).error(), None);

    // A refresh that ends with nothing newer leaves the value, no longer pending.
    assert_eq!(Load::Refreshing(3).settled(), Load::Ready(3));
    assert_eq!(Load::Ready(3).settled(), Load::Ready(3));
    assert_eq!(Load::<u8>::Loading.settled(), Load::Loading);
    assert_eq!(failed.clone().settled(), failed);

    assert_eq!(Load::Ready(3).map(|n| n + 1), Load::Ready(4));
    assert_eq!(Load::Refreshing(3).map(|n| n + 1), Load::Refreshing(4));
    assert_eq!(
        failed.clone().map(|n| n + 1),
        Load::Failed(CallError::Dropped)
    );
    assert_eq!(
        Load::Ready(String::from("a")).as_ref(),
        Load::Ready(&String::from("a"))
    );
}
