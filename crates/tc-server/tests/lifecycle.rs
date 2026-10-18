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
    /// When `call` last returned its answer.
    call_finished: Mutex<Option<Instant>>,
    /// When `quit` was last entered.
    quit_started: Mutex<Option<Instant>>,
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
            call_finished: Mutex::new(None),
            quit_started: Mutex::new(None),
        }
    }

    fn call_finished(&self) -> Option<Instant> {
        *self.call_finished.lock().unwrap()
    }

    fn quit_started(&self) -> Option<Instant> {
        *self.quit_started.lock().unwrap()
    }
}

impl Backend for Observed {
    type Events = StubEvents;

    async fn call(&self, path: &'static str, body: Bytes) -> Answer {
        tokio::time::sleep(self.call_takes).await;
        let answer = self.inner.call(path, body).await;
        *self.call_finished.lock().unwrap() = Some(Instant::now());
        answer
    }
    fn subscribe(&self, after: u64) -> StubEvents {
        self.inner.subscribe(after)
    }
    async fn quit(&self) {
        self.quit_calls.fetch_add(1, Ordering::SeqCst);
        *self.quit_started.lock().unwrap() = Some(Instant::now());
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

#[test]
fn quit_work_begins_after_the_requests_being_answered() {
    let backend = Arc::new(Observed {
        call_takes: Duration::from_millis(300),
        ..Observed::new()
    });
    let mut server = TestServer::start(Arc::clone(&backend));
    let mut stream = server.connect();
    stream
        .write_all(&server.api_req("app/hello", "{}").to_bytes())
        .unwrap();
    std::thread::sleep(Duration::from_millis(100));
    server.quit_message();
    let response = common::read_response(&mut stream);
    assert_eq!(response.status, 200, "{}", response.text());
    assert_eq!(server.join(), Exit::QuitMessage);
    let finished = backend.call_finished().expect("the call finished");
    let started = backend.quit_started().expect("the quit ran");
    assert!(
        finished <= started,
        "the quit work began {:?} before the request being answered finished",
        finished.duration_since(started)
    );
}

#[test]
fn a_call_whose_client_went_away_runs_to_its_end() {
    let backend = Arc::new(Observed {
        call_takes: Duration::from_millis(300),
        ..Observed::new()
    });
    let server = TestServer::start(Arc::clone(&backend));
    let mut stream = server.connect();
    stream
        .write_all(&server.api_req("app/hello", "{}").to_bytes())
        .unwrap();
    std::thread::sleep(Duration::from_millis(50));
    stream.shutdown(std::net::Shutdown::Both).unwrap();
    drop(stream);
    let until = Instant::now() + Duration::from_secs(1);
    while backend.call_finished().is_none() {
        assert!(
            Instant::now() < until,
            "a call was cut short when its client went away"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn quit_waits_for_a_call_whose_client_went_away() {
    let backend = Arc::new(Observed {
        call_takes: Duration::from_millis(300),
        ..Observed::new()
    });
    let mut server = TestServer::start(Arc::clone(&backend));
    let mut stream = server.connect();
    stream
        .write_all(&server.api_req("app/hello", "{}").to_bytes())
        .unwrap();
    std::thread::sleep(Duration::from_millis(50));
    stream.shutdown(std::net::Shutdown::Both).unwrap();
    drop(stream);
    std::thread::sleep(Duration::from_millis(50));
    server.quit_message();
    assert_eq!(server.join(), Exit::QuitMessage);
    let finished = backend
        .call_finished()
        .expect("the quit waited for the call");
    let started = backend.quit_started().expect("the quit ran");
    assert!(
        finished <= started,
        "the quit work began before the call finished"
    );
}

#[test]
fn the_whole_close_fits_in_close_within() {
    let backend = Arc::new(Observed {
        call_takes: Duration::from_secs(30),
        quit_hangs: true,
        ..Observed::new()
    });
    let mut server = TestServer::start_with(
        Arc::clone(&backend),
        |config| {
            config.limits.drain_within = Duration::from_millis(400);
            config.limits.close_within = Duration::from_millis(600);
        },
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
    assert!(
        began.elapsed() < Duration::from_secs(1),
        "the close took {:?}",
        began.elapsed()
    );
    assert_eq!(backend.quit_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn a_line_that_is_not_utf8_is_ignored() {
    let backend = Arc::new(Observed::new());
    let mut server = TestServer::start(backend);
    server.write_stdin(b"\xff\xfe\n");
    std::thread::sleep(Duration::from_millis(200));
    assert!(!server.finished(), "a stray byte ended the run");
    assert_eq!(server.get("/").status, 200);
    server.quit_message();
    assert_eq!(server.join(), Exit::QuitMessage);
}

#[test]
fn a_last_quit_line_without_a_newline_still_quits() {
    let backend = Arc::new(Observed::new());
    let mut server = TestServer::start(backend);
    server.write_stdin(b"{\"quit\":true}");
    server.close_stdin();
    assert_eq!(server.join(), Exit::QuitMessage);
    assert_eq!(server.stdout.text(), "{\"closed\":true}\n");
}

#[test]
fn a_request_whose_body_arrives_after_the_quit_began_is_not_run() {
    let gate = Arc::new(Notify::new());
    let backend = Arc::new(Observed {
        quit_waits_for: Some(Arc::clone(&gate)),
        ..Observed::new()
    });
    let mut server = TestServer::start_with(
        Arc::clone(&backend),
        |config| config.limits.drain_within = Duration::from_millis(100),
        true,
    );
    // The head of an API call whose body is still on its way.
    let mut stream = server.connect();
    stream
        .write_all(&server.api_req("app/hello", "{}").head_bytes())
        .unwrap();
    std::thread::sleep(Duration::from_millis(50));
    server.quit_message();
    let until = Instant::now() + Duration::from_secs(5);
    while backend.quit_calls.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < until, "the quit never began");
        std::thread::sleep(Duration::from_millis(5));
    }
    // The drain gave up on it and the backend is closing; now the body arrives.
    stream.write_all(b"{}").unwrap();
    let response = common::read_response(&mut stream);
    assert_eq!(response.status, 503, "{}", response.text());
    assert!(
        backend.call_finished().is_none(),
        "the call ran against a closing backend"
    );
    gate.notify_one();
    assert_eq!(server.join(), Exit::QuitMessage);
}
