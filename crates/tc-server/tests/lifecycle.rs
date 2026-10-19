//! How a run ends (APP-DESIGN §5.1), driven in process; `crates/tc4/tests/lifecycle.rs`
//! drives the same sequence against the real binary.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::{Read as _, Write as _};
use std::net::TcpStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use common::{Got, SseReader, TestServer};
use tc_server::stub::{Stub, StubEvents};
use tc_server::{Answer, Backend, Bytes, Exit};
use tokio::sync::Notify;

/// How long a test waits for a moment before it calls the run broken. Only a broken run
/// reaches it: no test relies on how long anything takes.
const BROKEN_AFTER: Duration = Duration::from_secs(60);

/// A moment the backend records and a test thread can wait for.
#[derive(Default)]
struct Moment {
    at: Mutex<Option<Instant>>,
    came: Condvar,
}

impl Moment {
    fn record(&self) {
        *self.at.lock().unwrap() = Some(Instant::now());
        self.came.notify_all();
    }

    fn get(&self) -> Option<Instant> {
        *self.at.lock().unwrap()
    }

    fn wait(&self, what: &str) -> Instant {
        let at = self.at.lock().unwrap();
        let (at, _) = self
            .came
            .wait_timeout_while(at, BROKEN_AFTER, |at| at.is_none())
            .unwrap();
        at.unwrap_or_else(|| panic!("{what} never came"))
    }
}

/// A stub that records when its calls and its quit work begin and end, and can be told to
/// hold either until a test releases it.
struct Observed {
    inner: Stub,
    quit_calls: AtomicUsize,
    quit_started: Moment,
    quit_finished: Moment,
    /// `quit()` waits for this before it finishes; `None`: it finishes at once.
    quit_waits_for: Option<Arc<Notify>>,
    /// `quit()` never finishes.
    quit_hangs: bool,
    call_started: Moment,
    call_finished: Moment,
    /// `call` waits for this before it answers; `None`: it answers at once.
    call_waits_for: Option<Arc<Notify>>,
}

impl Observed {
    fn new() -> Self {
        Self {
            inner: Stub::new().unwrap(),
            quit_calls: AtomicUsize::new(0),
            quit_started: Moment::default(),
            quit_finished: Moment::default(),
            quit_waits_for: None,
            quit_hangs: false,
            call_started: Moment::default(),
            call_finished: Moment::default(),
            call_waits_for: None,
        }
    }

    /// A backend whose calls answer only when the returned gate is notified.
    fn holding_calls() -> (Arc<Self>, Arc<Notify>) {
        let release = Arc::new(Notify::new());
        let backend = Self {
            call_waits_for: Some(Arc::clone(&release)),
            ..Self::new()
        };
        (Arc::new(backend), release)
    }
}

impl Backend for Observed {
    type Events = StubEvents;

    async fn call(&self, path: &'static str, body: Bytes) -> Answer {
        self.call_started.record();
        if let Some(gate) = &self.call_waits_for {
            gate.notified().await;
        }
        let answer = self.inner.call(path, body).await;
        self.call_finished.record();
        answer
    }
    fn subscribe(&self, after: u64) -> StubEvents {
        self.inner.subscribe(after)
    }
    async fn quit(&self) {
        self.quit_calls.fetch_add(1, Ordering::SeqCst);
        self.quit_started.record();
        if self.quit_hangs {
            std::future::pending::<()>().await;
        }
        if let Some(gate) = &self.quit_waits_for {
            gate.notified().await;
        }
        self.inner.quit().await;
        self.quit_finished.record();
    }
    async fn quitting(&self) {
        self.inner.quitting().await;
    }
}

/// A server whose drain outlasts any test, so the tests' own signals order every step.
fn patient_server(backend: &Arc<Observed>) -> TestServer {
    TestServer::start_with(
        Arc::clone(backend),
        |config| {
            config.limits.drain_within = BROKEN_AFTER;
            config.limits.close_within = BROKEN_AFTER;
        },
        true,
    )
}

/// Waits until the server refuses a new request: the quit sequence has begun.
fn wait_until_closing(server: &TestServer) {
    let until = Instant::now() + BROKEN_AFTER;
    while server.get("/").status != 503 {
        assert!(Instant::now() < until, "the quit never began");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Starts an API call and waits until the backend runs it.
fn start_a_call(server: &TestServer, backend: &Observed) -> TcpStream {
    let mut stream = server.connect();
    stream
        .write_all(&server.api_req("app/hello", "{}").to_bytes())
        .unwrap();
    backend.call_started.wait("the call");
    stream
}

/// Starts an API call, waits until the backend runs it, then hangs up and waits until the
/// server has dropped the connection, as a reload or a closed window does.
fn hang_up_during_a_call(server: &TestServer, backend: &Observed) {
    let mut stream = start_a_call(server, backend);
    stream.set_read_timeout(Some(BROKEN_AFTER)).unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut byte = [0_u8; 1];
    match stream.read(&mut byte) {
        Ok(0) => {}
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
        other => panic!("the server kept a connection whose client went away: {other:?}"),
    }
}

/// Sends the head of an API call that expects `100 Continue`, and waits for it. hyper sends
/// it only when the handler asks for the body, past the gate, so the request is then one the
/// server accepted.
fn start_a_call_without_its_body(server: &TestServer) -> TcpStream {
    let mut stream = server.connect();
    let request = server
        .api_req("app/hello", "{}")
        .with("Expect", "100-continue");
    stream.write_all(&request.head_bytes()).unwrap();
    let expected = b"HTTP/1.1 100 Continue\r\n\r\n";
    let mut got = vec![0_u8; expected.len()];
    stream.read_exact(&mut got).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&got),
        String::from_utf8_lossy(expected)
    );
    stream
}

/// A call that is running when the quit begins, answered once the quit has begun.
fn answer_a_call_across_the_quit() -> (Arc<Observed>, common::Response) {
    let (backend, release) = Observed::holding_calls();
    let mut server = patient_server(&backend);
    let mut stream = start_a_call(&server, &backend);
    server.quit_message();
    wait_until_closing(&server);
    release.notify_one();
    let response = common::read_response(&mut stream);
    assert_eq!(server.join(), Exit::QuitMessage);
    (backend, response)
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
    let quit_done = backend.quit_finished.get().expect("quit ran to its end");
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
    let (_, response) = answer_a_call_across_the_quit();
    assert_eq!(response.status, 200, "{}", response.text());
}

#[test]
fn a_request_whose_body_arrives_during_the_drain_still_gets_its_reply() {
    let backend = Arc::new(Observed::new());
    let mut server = patient_server(&backend);
    let mut stream = start_a_call_without_its_body(&server);
    server.quit_message();
    wait_until_closing(&server);
    stream.write_all(b"{}").unwrap();
    let response = common::read_response(&mut stream);
    assert_eq!(response.status, 200, "{}", response.text());
    assert_eq!(server.join(), Exit::QuitMessage);
    let finished = backend.call_finished.get().expect("the call ran");
    let started = backend.quit_started.get().expect("the quit ran");
    assert!(
        finished <= started,
        "the call ran against a closing backend"
    );
}
#[test]
fn a_request_that_outlasts_the_drain_does_not_keep_the_run_alive() {
    let backend = Arc::new(Observed {
        // Never notified: the call outlasts the run.
        call_waits_for: Some(Arc::new(Notify::new())),
        ..Observed::new()
    });
    let mut server = TestServer::start_with(
        Arc::clone(&backend),
        |config| config.limits.drain_within = Duration::from_millis(200),
        true,
    );
    let _stream = start_a_call(&server, &backend);
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
    let (backend, response) = answer_a_call_across_the_quit();
    assert_eq!(response.status, 200, "{}", response.text());
    let finished = backend.call_finished.get().expect("the call finished");
    let started = backend.quit_started.get().expect("the quit ran");
    assert!(
        finished <= started,
        "the quit work began {:?} before the request being answered finished",
        finished.duration_since(started)
    );
}
#[test]
fn a_call_whose_client_went_away_runs_to_its_end() {
    let (backend, release) = Observed::holding_calls();
    let server = TestServer::start(Arc::clone(&backend));
    hang_up_during_a_call(&server, &backend);
    release.notify_one();
    backend
        .call_finished
        .wait("the end of a call whose client went away");
}
#[test]
fn quit_waits_for_a_call_whose_client_went_away() {
    let (backend, release) = Observed::holding_calls();
    let mut server = patient_server(&backend);
    hang_up_during_a_call(&server, &backend);
    server.quit_message();
    wait_until_closing(&server);
    release.notify_one();
    assert_eq!(server.join(), Exit::QuitMessage);
    let finished = backend
        .call_finished
        .get()
        .expect("the quit waited for the call");
    let started = backend.quit_started.get().expect("the quit ran");
    assert!(
        finished <= started,
        "the quit work began before the call finished"
    );
}
#[test]
fn the_whole_close_fits_in_close_within() {
    let backend = Arc::new(Observed {
        quit_hangs: true,
        // Never notified: the call outlasts the run.
        call_waits_for: Some(Arc::new(Notify::new())),
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
    let _stream = start_a_call(&server, &backend);
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
fn a_request_whose_body_arrives_after_the_drain_gave_up_is_not_run() {
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
    let mut stream = start_a_call_without_its_body(&server);
    server.quit_message();
    // The drain gave up on the request and the backend is closing; now the body arrives.
    backend.quit_started.wait("the quit work");
    stream.write_all(b"{}").unwrap();
    let response = common::read_response(&mut stream);
    assert_eq!(response.status, 503, "{}", response.text());
    assert!(
        backend.call_started.get().is_none(),
        "the call ran against a closing backend"
    );
    gate.notify_one();
    assert_eq!(server.join(), Exit::QuitMessage);
}
