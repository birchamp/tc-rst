//! How a run ends (APP-DESIGN §5.1), driven in process; `crates/tc4/tests/lifecycle.rs`
//! drives the same sequence against the real binary.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::Write as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::{Got, SseReader, TestServer};
use tc_server::stub::{Stub, StubEvents};
use tc_server::{Answer, Backend, Bytes, Exit};
use tokio::sync::Notify;

/// A stub that counts and times its quit work, and can be told to wait in it.
struct Observed {
    inner: Stub,
    quit_calls: AtomicUsize,
    quit_finished: Mutex<Option<Instant>>,
    /// `quit()` waits for this before it finishes; `None`: it finishes at once.
    quit_waits_for: Option<Arc<Notify>>,
    /// `quit()` never finishes.
    quit_hangs: bool,
    /// `call` waits this long before it answers.
    call_takes: Duration,
}

impl Observed {
    fn new() -> Self {
        Self {
            inner: Stub::new().unwrap(),
            quit_calls: AtomicUsize::new(0),
            quit_finished: Mutex::new(None),
            quit_waits_for: None,
            quit_hangs: false,
            call_takes: Duration::ZERO,
        }
    }
}

impl Backend for Observed {
    type Events = StubEvents;

    async fn call(&self, path: &'static str, body: Bytes) -> Answer {
        tokio::time::sleep(self.call_takes).await;
        self.inner.call(path, body).await
    }
    fn subscribe(&self, after: u64) -> StubEvents {
        self.inner.subscribe(after)
    }
    async fn quit(&self) {
        self.quit_calls.fetch_add(1, Ordering::SeqCst);
        if self.quit_hangs {
            std::future::pending::<()>().await;
        }
        if let Some(gate) = &self.quit_waits_for {
            gate.notified().await;
        }
        self.inner.quit().await;
        *self.quit_finished.lock().unwrap() = Some(Instant::now());
    }
    async fn quitting(&self) {
        self.inner.quitting().await;
    }
}

#[test]
fn a_closed_stdin_ends_the_run_within_5_s() {
    let backend = Arc::new(Observed::new());
    let mut server = TestServer::start(Arc::clone(&backend));
    assert_eq!(server.get("/").status, 200);
    let closed_at = Instant::now();
    server.close_stdin();
    assert_eq!(server.join(), Exit::StdinClosed);
    assert!(closed_at.elapsed() < Duration::from_secs(5));
    assert_eq!(
        backend.quit_calls.load(Ordering::SeqCst),
        1,
        "a closed stdin is a quit"
    );
    assert_eq!(server.stdout.text(), "", "nobody is there to hear closed");
}

#[test]
fn the_quit_message_quits_the_backend_then_answers_closed() {
    let backend = Arc::new(Observed::new());
    let mut server = TestServer::start(Arc::clone(&backend));
    server.quit_message();
    assert_eq!(server.join(), Exit::QuitMessage);
    assert_eq!(backend.quit_calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.stdout.text(), "{\"closed\":true}\n");
    let quit_done = backend
        .quit_finished
        .lock()
        .unwrap()
        .expect("quit ran to its end");
    assert!(
        server.stdout.first_write().unwrap() >= quit_done,
        "closed was written before the backend finished"
    );
}

#[test]
fn other_stdin_lines_are_ignored() {
    let backend = Arc::new(Observed::new());
    let mut server = TestServer::start(Arc::clone(&backend));
    for line in [
        &b"hello\n"[..],
        b"{\"quit\":false}\n",
        b"\n",
        b"{\"quit\":\"yes\"}\n",
        b"{\"quitt\":true}\n",
        b"[1,2]\n",
    ] {
        server.write_stdin(line);
    }
    std::thread::sleep(Duration::from_millis(300));
    assert!(!server.finished(), "an ordinary line ended the run");
    assert_eq!(server.get("/").status, 200);
    assert_eq!(backend.quit_calls.load(Ordering::SeqCst), 0);
    // The message is read from its line, and extra keys do not hide it.
    server.write_stdin(b"{\"quit\":true,\"why\":\"window closed\"}\n");
    assert_eq!(server.join(), Exit::QuitMessage);
}

#[test]
fn app_quit_ends_the_run_like_the_quit_message() {
    let backend = Arc::new(Observed::new());
    let mut server = TestServer::start(Arc::clone(&backend));
    let reply = server.api_ok(
        "app/quit",
        r#"{"request":"00000000000000000000000000000003"}"#,
    );
    assert_eq!(reply, serde_json::Value::Null);
    assert_eq!(server.join(), Exit::AppQuit);
    assert_eq!(backend.quit_calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.stdout.text(), "{\"closed\":true}\n");
}

#[test]
fn requests_while_closing_get_503() {
    let gate = Arc::new(Notify::new());
    let backend = Arc::new(Observed {
        quit_waits_for: Some(Arc::clone(&gate)),
        ..Observed::new()
    });
    let mut server = TestServer::start(Arc::clone(&backend));
    server.quit_message();
    // The backend is closing; the server answers 503 to everything and says it will close.
    let until = Instant::now() + Duration::from_secs(5);
    while backend.quit_calls.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < until, "the quit never began");
        std::thread::sleep(Duration::from_millis(5));
    }
    let post = server.api("app/hello", "{}");
    assert_eq!(post.status, 503);
    assert_eq!(post.header("connection"), Some("close"));
    let page = server.get("/");
    assert_eq!(page.status, 503);
    assert_eq!(page.header("connection"), Some("close"));
    let stream = server.send(&server.authed("GET", "/api/v1/events").with("X-Tc-Api", "1"));
    assert_eq!(stream.status, 503);
    // The security checks still come first.
    let bare = common::Req::new("GET", "/").with("Host", &format!("127.0.0.1:{}", server.port));
    assert_eq!(server.send(&bare).status, 401);
    assert!(!server.finished());
    gate.notify_one();
    assert_eq!(server.join(), Exit::QuitMessage);
    assert_eq!(backend.quit_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn a_backend_quit_that_hangs_is_cut_at_the_limit() {
    let backend = Arc::new(Observed {
        quit_hangs: true,
        ..Observed::new()
    });
    let mut server = TestServer::start_with(
        backend,
        |config| config.limits.close_within = Duration::from_millis(200),
        true,
    );
    let began = Instant::now();
    server.quit_message();
    assert_eq!(server.join(), Exit::QuitMessage);
    assert!(
        began.elapsed() < Duration::from_secs(1),
        "{:?}",
        began.elapsed()
    );
    assert_eq!(server.stdout.text(), "{\"closed\":true}\n");
}

#[test]
fn a_request_being_answered_when_quit_begins_still_gets_its_reply() {
    let backend = Arc::new(Observed {
        call_takes: Duration::from_millis(300),
        ..Observed::new()
    });
    let mut server = TestServer::start(Arc::clone(&backend));
    let request = server.api_req("app/hello", "{}");
    let mut stream = server.connect();
    stream.write_all(&request.to_bytes()).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    server.quit_message();
    let response = common::read_response(&mut stream);
    assert_eq!(response.status, 200, "{}", response.text());
    assert_eq!(server.join(), Exit::QuitMessage);
}

#[test]
fn a_request_that_outlasts_the_drain_does_not_keep_the_run_alive() {
    let backend = Arc::new(Observed {
        call_takes: Duration::from_secs(30),
        ..Observed::new()
    });
    let mut server = TestServer::start_with(
        backend,
        |config| config.limits.drain_within = Duration::from_millis(200),
        true,
    );
    let mut stream = server.connect();
    stream
        .write_all(&server.api_req("app/hello", "{}").to_bytes())
        .unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let began = Instant::now();
    server.quit_message();
    assert_eq!(server.join(), Exit::QuitMessage);
    assert!(began.elapsed() < Duration::from_secs(2));
}

#[test]
fn an_open_stream_does_not_hold_the_run_up() {
    let backend = Arc::new(Observed::new());
    let mut server = TestServer::start(backend);
    let mut stream = SseReader::open(&server, "");
    assert!(matches!(stream.read(Duration::from_secs(2)), Got::Item(_)));
    let began = Instant::now();
    server.quit_message();
    assert_eq!(server.join(), Exit::QuitMessage);
    assert!(began.elapsed() < Duration::from_secs(2));
}
