//! The routes: the UI's files, the API from tc-api's tables, and the stand-in backend.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;
use std::io::Write as _;
use std::sync::Arc;

use common::{Recorder, TestServer, stub};
use tc_api::app::HelloView;
use tc_api::error::Feature;
use tc_api::event::StartupState;
use tc_api::{ALL, ApiError, Reply};
use tc_server::{Answer, Backend, Then, product_paths, test_paths};

#[test]
fn static_routes_serve_the_ui_dir() {
    let server = TestServer::start(stub());
    let index = server.get("/");
    assert_eq!(index.body, b"<!doctype html><title>tC4</title>");
    assert_eq!(
        index.header("content-type"),
        Some("text/html; charset=utf-8")
    );
    let js = server.get("/ui/app.js");
    assert_eq!(js.body, b"console.log(1)");
    assert_eq!(
        js.header("content-type"),
        Some("text/javascript; charset=utf-8")
    );
    assert_eq!(
        js.header("cache-control"),
        Some("public, max-age=31536000, immutable")
    );
    let wasm = server.get("/ui/app.wasm");
    assert_eq!(wasm.body, b"\0asm\x01\0\0\0");
    assert_eq!(wasm.header("content-type"), Some("application/wasm"));
    assert_eq!(
        server.get("/ui/style.css").header("content-type"),
        Some("text/css; charset=utf-8")
    );
    let help = server.get("/help/index.html");
    assert_eq!(help.body, b"<p>Help</p>");
    assert_eq!(help.header("cache-control"), Some("no-cache"));
    let prepare = server.get("/prepare.html");
    assert_eq!(prepare.body, b"<p>Preparing</p>");
    assert_eq!(prepare.header("cache-control"), Some("no-store"));
    assert_eq!(
        prepare.header("content-type"),
        Some("text/html; charset=utf-8")
    );
}

#[test]
fn static_paths_never_leave_the_ui_dir() {
    let server = TestServer::start(stub());
    // A file with this name sits outside the UI folder, and one inside `ui/` must not mask it.
    for path in [
        "/ui/../index.html",
        "/ui/../../secret.txt",
        "/ui/%2e%2e/%2e%2e/secret.txt",
        "/ui/%2e%2e/index.html",
        "/ui/..%2fsecret.txt",
        "/ui/a\\b",
        "/ui/a%5Cb",
        "/ui/c:x",
        "/ui/c%3Ax",
        "/ui//app.js",
        "/ui/./app.js",
        "/ui/app.js%00",
        "/ui/%00",
        "/help/../index.html",
        "/help/%2e%2e/index.html",
    ] {
        let response = server.get(path);
        assert_eq!(response.status, 404, "GET {path}");
        assert!(
            !response.text().contains("not for the UI"),
            "GET {path} served the secret"
        );
    }
}

#[test]
fn a_missing_index_is_a_404_not_a_failure() {
    let server = TestServer::start(stub());
    std::fs::remove_file(server.ui_root.path().join("site/index.html")).unwrap();
    assert_eq!(server.get("/").status, 404);
    assert_eq!(server.get("/ui/app.js").status, 200);
}

#[test]
fn every_endpoint_is_routed_once() {
    let recorder = Arc::new(Recorder::default());
    let server = TestServer::start(Arc::clone(&recorder));
    let wanted: Vec<&str> = ALL.iter().map(|e| e.path).collect();
    for path in &wanted {
        let response = server.api(path, "{}");
        assert_eq!(response.status, 200, "{path}");
        assert_eq!(
            response.json(),
            serde_json::json!({"type": "ok", "value": null})
        );
    }
    assert_eq!(recorder.paths(), wanted, "each row once, in table order");
    let mut listed: Vec<&str> = product_paths().to_vec();
    listed.extend_from_slice(test_paths());
    assert_eq!(listed, wanted, "the two lists are the whole table");
    assert_eq!(
        wanted.iter().collect::<BTreeSet<_>>().len(),
        wanted.len(),
        "no path twice"
    );

    let before = recorder.paths().len();
    for path in [
        "app/nope",
        "nope/hello",
        "app",
        "app/hello/x",
        "app/Hello",
        "test/nope",
    ] {
        assert_eq!(server.api(path, "{}").status, 404, "{path}");
    }
    assert_eq!(server.api("", "{}").status, 404);
    assert_eq!(
        recorder.paths().len(),
        before,
        "a path outside the table reaches no backend"
    );
}

#[test]
fn a_get_on_an_api_path_is_not_routed() {
    let recorder = Arc::new(Recorder::default());
    let server = TestServer::start(Arc::clone(&recorder));
    let response = server.send(
        &server
            .authed("GET", "/api/v1/app/hello")
            .with("X-Tc-Api", "1"),
    );
    assert_eq!(response.status, 405);
    assert!(recorder.paths().is_empty());
}

#[test]
fn test_rows_are_404_without_test_hooks() {
    let recorder = Arc::new(Recorder::default());
    let server = TestServer::start_with(Arc::clone(&recorder), |_| {}, false);
    for path in test_paths() {
        assert_eq!(server.api(path, "{}").status, 404, "{path}");
    }
    assert!(recorder.paths().is_empty());
    for path in product_paths() {
        assert_eq!(server.api(path, "{}").status, 200, "{path}");
    }
    assert_eq!(recorder.paths().len(), product_paths().len());
}

#[test]
fn an_api_body_over_the_cap_is_413() {
    let recorder = Arc::new(Recorder::default());
    let server = TestServer::start(Arc::clone(&recorder));
    let cap = 8 * 1024 * 1024;
    // A declared length over the cap is refused before any body is read.
    let declared = server
        .api_req("app/hello", "")
        .with("Content-Length", &(cap + 1).to_string());
    let mut stream = server.connect();
    stream.write_all(&declared.head_bytes()).unwrap();
    let response = common::read_response(&mut stream);
    assert_eq!(response.status, 413);
    assert_eq!(response.header("connection"), Some("close"));
    // A chunked body that grows past it is refused too.
    let chunked = server
        .api_req("app/hello", "")
        .with("Transfer-Encoding", "chunked");
    let mut stream = server.connect();
    stream.write_all(&chunked.head_bytes()).unwrap();
    let block = vec![b' '; 1024 * 1024];
    for _ in 0..=8 {
        let ok = stream
            .write_all(format!("{:x}\r\n", block.len()).as_bytes())
            .and_then(|()| stream.write_all(&block))
            .and_then(|()| stream.write_all(b"\r\n"));
        if ok.is_err() {
            break;
        }
    }
    let _ = stream.write_all(b"0\r\n\r\n");
    let response = common::read_response(&mut stream);
    assert_eq!(response.status, 413);
    assert!(
        recorder.paths().is_empty(),
        "no part of it reached the backend"
    );
}

#[test]
fn a_body_at_the_cap_reaches_the_backend() {
    let recorder = Arc::new(Recorder::default());
    let server = TestServer::start(Arc::clone(&recorder));
    let cap = 8 * 1024 * 1024;
    let mut body = vec![b' '; cap];
    body[0] = b'{';
    body[cap - 1] = b'}';
    let response = server.send(&server.api_req("app/hello", "").body(&body));
    assert_eq!(response.status, 200);
    let seen = recorder.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].1, body, "the whole body, unchanged");
}

struct Panics;

impl Backend for Panics {
    type Events = tc_server::stub::StubEvents;

    #[allow(clippy::panic)]
    async fn call(&self, path: &'static str, _body: tc_server::Bytes) -> Answer {
        if path == "app/clientError" {
            panic!("a bug in the app");
        }
        Answer::Reply {
            body: br#"{"type":"ok","value":null}"#.to_vec(),
            then: Then::Continue,
        }
    }
    fn subscribe(&self, _: u64) -> Self::Events {
        tc_server::stub::StubEvents
    }
    async fn quit(&self) {}
    async fn quitting(&self) {
        std::future::pending::<()>().await;
    }
}

#[test]
fn a_panicking_backend_call_is_500_and_the_server_goes_on() {
    let server = TestServer::start(Arc::new(Panics));
    let response = server.api("app/clientError", "{}");
    assert_eq!(response.status, 500);
    assert!(
        !response.text().contains("a bug in the app"),
        "the panic text stays inside"
    );
    assert_eq!(server.api("app/hello", "{}").status, 200);
    assert_eq!(server.api("app/clientError", "{}").status, 500);
    assert_eq!(server.get("/").status, 200);
}

#[test]
fn hello_from_the_stub_names_its_instance_and_the_api() {
    let server = TestServer::start(stub());
    let hello: Reply<HelloView> =
        serde_json::from_slice(&server.api("app/hello", "{}").body).unwrap();
    let Reply::Ok(hello) = hello else {
        panic!("hello answered {hello:?}")
    };
    assert_eq!(hello.api, 1);
    assert!(!hello.internet);
    assert!(hello.sessions.is_empty());
    assert_eq!(hello.prefs.app_locale, tc_api::ids::LocaleId::ENGLISH);
    assert_eq!(hello.startup, StartupState::Ready);
    assert_eq!(hello.build.commit, tc_api::app::BUILD_COMMIT);
    assert_eq!(hello.build.version, tc_api::app::BUILD_VERSION);
    assert_eq!(hello.account.user, None);
    // The same server says the same instance; another server says another.
    let again: Reply<HelloView> =
        serde_json::from_slice(&server.api("app/hello", "{}").body).unwrap();
    assert_eq!(again, Reply::Ok(hello.clone()));
    let other = TestServer::start(stub());
    let theirs: Reply<HelloView> =
        serde_json::from_slice(&other.api("app/hello", "{}").body).unwrap();
    let Reply::Ok(theirs) = theirs else { panic!() };
    assert_ne!(theirs.instance, hello.instance);
}

#[test]
fn the_stub_parses_every_body_and_refuses_what_it_does_not_build() {
    let server = TestServer::start(stub());
    let bad = |reply: serde_json::Value| -> ApiError {
        serde_json::from_value::<Reply<serde_json::Value>>(reply)
            .unwrap()
            .into_result()
            .unwrap_err()
    };
    // A key the request does not know.
    let error = bad(server.api("app/hello", r#"{"x":1}"#).json());
    assert!(
        matches!(error, ApiError::BadRequest { ref field, .. } if field == "body"),
        "{error:?}"
    );
    // Not JSON at all.
    let error = bad(server.api("app/hello", "hello").json());
    assert!(matches!(error, ApiError::BadRequest { .. }));
    // A good body for a row it does not build.
    let error = bad(server
        .api(
            "net/set",
            r#"{"request":"00000000000000000000000000000001","on":true}"#,
        )
        .json());
    assert_eq!(error, ApiError::Unavailable(Feature::Writes));
    let error = bad(server.api("net/set", "{}").json());
    assert!(matches!(error, ApiError::BadRequest { .. }), "{error:?}");
    // Every row of the table answers, as an error or a value.
    for info in ALL {
        let response = server.api(info.path, "{}");
        assert_eq!(response.status, 200, "{}", info.path);
        let _: Reply<serde_json::Value> = serde_json::from_slice(&response.body).unwrap();
    }
}

#[test]
fn test_quiet_is_busy_while_another_request_is_being_answered() {
    let server = Arc::new(TestServer::start(stub()));
    assert_eq!(
        server.api_ok("test/quiet", "{}"),
        serde_json::json!({"busy": false})
    );
    // The largest echo keeps the stub inside a call for a while; the other worker answers.
    let slow = {
        let server = Arc::clone(&server);
        std::thread::spawn(move || server.api("test/echo", r#"{"size":4194304}"#))
    };
    let mut saw_busy = false;
    while !slow.is_finished() {
        saw_busy |= server.api_ok("test/quiet", "{}") == serde_json::json!({"busy": true});
    }
    assert_eq!(slow.join().unwrap().status, 200);
    assert!(saw_busy, "the echo was never seen in flight");
    assert_eq!(
        server.api_ok("test/quiet", "{}"),
        serde_json::json!({"busy": false})
    );
}

#[test]
fn test_echo_answers_a_list_of_words_and_refuses_a_size_over_the_cap() {
    let server = TestServer::start(stub());
    let echo = server.api_ok("test/echo", r#"{"size":20480}"#);
    let words = echo["words"].as_array().unwrap();
    assert!(words.len() > 100);
    let reply = server.api("test/echo", r#"{"size":20480}"#);
    assert!(
        reply.body.len() >= 20480 && reply.body.len() < 20480 + 128,
        "{}",
        reply.body.len()
    );
    let too_big = server.api("test/echo", r#"{"size":4194305}"#);
    let error: Reply<serde_json::Value> = serde_json::from_slice(&too_big.body).unwrap();
    assert!(matches!(error, Reply::Err(ApiError::BadRequest { ref field, .. }) if field == "size"));
}

#[test]
fn a_client_error_report_is_accepted() {
    let server = TestServer::start(stub());
    let body = r#"{"request":"00000000000000000000000000000002","view":"Check","project":null,"message":"unreachable executed","at":1}"#;
    let response = server.api("app/clientError", body);
    assert_eq!(
        response.json(),
        serde_json::json!({"type": "ok", "value": null})
    );
}
