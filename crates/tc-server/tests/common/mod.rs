//! What the integration tests share: a server on its own thread, a blocking HTTP client over
//! a raw `TcpStream` (a client library might route loopback through the container's proxy),
//! and a reader for server-sent events.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test, and not every test binary uses each shared helper"
)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use tc_api::endpoint::{API_VERSION_HEADER, TOKEN_HEADER};
use tc_api::upload::{UPLOAD_FLOW_HEADER, UPLOAD_NAME_HEADER, UPLOADS_PATH, UploadFlow};
use tc_server::stub::{Stub, StubEvents};
use tc_server::{Answer, Backend, Exit, Server, ServerConfig, ServerHandle, Stdio, Then};
use tempfile::TempDir;
use tokio::io::{AsyncWrite, AsyncWriteExt as _, DuplexStream};

/// Stdout of the server, collected with the time of the first write.
#[derive(Clone, Default)]
pub struct Collected(Arc<Mutex<(Vec<u8>, Option<Instant>)>>);

impl Collected {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap().0).into_owned()
    }

    pub fn first_write(&self) -> Option<Instant> {
        self.0.lock().unwrap().1
    }
}

impl AsyncWrite for Collected {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let mut inner = self.0.lock().unwrap();
        inner.1.get_or_insert_with(Instant::now);
        inner.0.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

pub struct TestServer {
    pub port: u16,
    pub token: String,
    pub app_data: TempDir,
    /// The registries of uploads and downloads.
    pub handle: ServerHandle,
    /// Holds `site/` (the UI folder) and `secret.txt` beside it.
    pub ui_root: TempDir,
    stdin: Option<DuplexStream>,
    pub stdout: Collected,
    thread: Option<std::thread::JoinHandle<Exit>>,
    io: tokio::runtime::Runtime,
    started: Instant,
}

/// Writes the files a UI folder holds, and a secret beside the folder.
pub fn seed_ui(root: &std::path::Path) -> std::path::PathBuf {
    let site = root.join("site");
    for (file, bytes) in [
        ("index.html", &b"<!doctype html><title>tC4</title>"[..]),
        ("ui/app.js", b"console.log(1)"),
        ("ui/app.wasm", b"\0asm\x01\0\0\0"),
        ("ui/style.css", b"body{}"),
        ("help/index.html", b"<p>Help</p>"),
        ("help/sub/page.html", b"<p>Sub</p>"),
        (
            "help/st-where-projects-live.html",
            b"<code>~/pankosmia/tc4-projects</code>",
        ),
        ("prepare.html", b"<p>Preparing</p>"),
    ] {
        let path = site.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    std::fs::write(root.join("secret.txt"), b"not for the UI").unwrap();
    site
}

impl TestServer {
    pub fn start<B: Backend>(backend: Arc<B>) -> TestServer {
        Self::start_with(backend, |_| {}, true)
    }

    /// `configure` changes the config before the server binds; `hooks` mounts the `test`
    /// rows.
    pub fn start_with<B: Backend>(
        backend: Arc<B>,
        configure: impl FnOnce(&mut ServerConfig),
        hooks: bool,
    ) -> TestServer {
        Self::start_on(backend, configure, hooks, 2).0
    }

    /// A server whose app data is `dir` (it must exist; the test deletes it).
    pub fn start_in<B: Backend>(backend: Arc<B>, dir: &std::path::Path) -> TestServer {
        let app_data = TempDir::new_in(dir).unwrap();
        Self::start_inner(backend, |_| {}, true, app_data, 2).0
    }

    /// As `start_with`, on a runtime of `workers` threads, whose handle comes back too. One
    /// worker shows a blocking call that several would hide by stealing work.
    pub fn start_on<B: Backend>(
        backend: Arc<B>,
        configure: impl FnOnce(&mut ServerConfig),
        hooks: bool,
        workers: usize,
    ) -> (TestServer, tokio::runtime::Handle) {
        Self::start_inner(backend, configure, hooks, TempDir::new().unwrap(), workers)
    }

    fn start_inner<B: Backend>(
        backend: Arc<B>,
        configure: impl FnOnce(&mut ServerConfig),
        hooks: bool,
        app_data: TempDir,
        workers: usize,
    ) -> (TestServer, tokio::runtime::Handle) {
        let ui_root = TempDir::new().unwrap();
        let site = seed_ui(ui_root.path());
        let mut config = ServerConfig::new(app_data.path().to_path_buf(), site);
        configure(&mut config);
        let server = Server::bind(config).unwrap();
        let server = if hooks {
            server.with_test_hooks()
        } else {
            server
        };
        let port: u16 = server
            .address()
            .url
            .trim_end_matches('/')
            .rsplit(':')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let token = server.address().token.clone();
        let handle = server.handle();
        let (writer, reader) = tokio::io::duplex(4096);
        let stdout = Collected::default();
        let out = stdout.clone();
        let (runtime_handle, handle_back) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(workers)
                .enable_all()
                .build()
                .unwrap();
            runtime_handle.send(runtime.handle().clone()).unwrap();
            runtime
                .block_on(server.run(
                    backend,
                    Stdio {
                        input: tokio::io::BufReader::new(reader),
                        output: out,
                    },
                ))
                .unwrap()
        });
        let server = TestServer {
            port,
            token,
            app_data,
            handle,
            ui_root,
            stdin: Some(writer),
            stdout,
            thread: Some(thread),
            io: tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap(),
            started: Instant::now(),
        };
        (server, handle_back.recv().unwrap())
    }

    pub fn quit_message(&mut self) {
        self.write_stdin(b"{\"quit\":true}\n");
    }

    pub fn write_stdin(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().expect("stdin is open");
        self.io.block_on(stdin.write_all(bytes)).unwrap();
    }

    /// The shell died: stdin reaches its end.
    pub fn close_stdin(&mut self) {
        self.stdin = None;
    }

    /// Waits for the run to end and says how.
    pub fn join(&mut self) -> Exit {
        self.thread.take().expect("joined once").join().unwrap()
    }

    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }

    pub fn since_start(&self) -> Duration {
        self.started.elapsed()
    }

    /// A request with the headers every good request carries.
    pub fn authed(&self, method: &str, path: &str) -> Req {
        Req::new(method, path)
            .with("Host", &format!("127.0.0.1:{}", self.port))
            .with(TOKEN_HEADER, &self.token)
    }

    /// An API call as the UI makes it.
    pub fn api_req(&self, path: &str, json: &str) -> Req {
        self.authed("POST", &format!("/api/v1/{path}"))
            .with("Origin", &format!("http://127.0.0.1:{}", self.port))
            .with(API_VERSION_HEADER, "1")
            .with("Content-Type", "application/json")
            .body(json.as_bytes())
    }

    /// An upload as the UI makes it: the file is the body, its name and flow are headers.
    pub fn upload_req(&self, flow: UploadFlow, name: &str, body: &[u8]) -> Req {
        self.authed("POST", UPLOADS_PATH)
            .with("Origin", &format!("http://127.0.0.1:{}", self.port))
            .with(API_VERSION_HEADER, "1")
            .with(UPLOAD_FLOW_HEADER, flow.header_value())
            .with(
                UPLOAD_NAME_HEADER,
                &utf8_percent_encode(name, NON_ALPHANUMERIC).to_string(),
            )
            .with("Content-Type", "application/octet-stream")
            .body(body)
    }

    /// The folder uploads are stored in.
    pub fn uploads_dir(&self) -> std::path::PathBuf {
        self.app_data.path().join("uploads")
    }

    pub fn get(&self, path: &str) -> Response {
        self.send(&self.authed("GET", path))
    }

    pub fn api(&self, path: &str, json: &str) -> Response {
        self.send(&self.api_req(path, json))
    }

    /// An API call whose reply is `Reply<T>`'s JSON; the reply must be `ok`.
    pub fn api_ok(&self, path: &str, json: &str) -> serde_json::Value {
        let response = self.api(path, json);
        assert_eq!(response.status, 200, "{path}: {}", response.text());
        let reply: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(reply["type"], "ok", "{path}: {reply}");
        reply["value"].clone()
    }

    pub fn connect(&self) -> TcpStream {
        let stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream.set_nodelay(true).unwrap();
        stream
    }

    pub fn send(&self, request: &Req) -> Response {
        let mut stream = self.connect();
        // A server that refuses a request may answer and close before the body is all sent;
        // its answer is what the test reads.
        if let Err(e) = stream.write_all(&request.to_bytes()) {
            assert!(
                matches!(
                    e.kind(),
                    std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::ConnectionReset
                ),
                "writing a request: {e}"
            );
        }
        read_response(&mut stream)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        // A test that failed leaves the server running; closing stdin ends it.
        self.stdin = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Clone)]
pub struct Req {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    /// `Content-Length` is written from the body unless set here.
    chunked: bool,
}

impl Req {
    pub fn new(method: &str, path: &str) -> Req {
        Req {
            method: method.to_owned(),
            path: path.to_owned(),
            headers: vec![("Connection".to_owned(), "close".to_owned())],
            body: Vec::new(),
            chunked: false,
        }
    }

    /// Sets a header, replacing any with the name.
    pub fn with(mut self, name: &str, value: &str) -> Req {
        self = self.without(name);
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    /// Adds a header line, keeping any with the name.
    pub fn add(mut self, name: &str, value: &str) -> Req {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    pub fn without(mut self, name: &str) -> Req {
        self.headers.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
        self
    }

    pub fn body(mut self, bytes: &[u8]) -> Req {
        self.body = bytes.to_vec();
        self
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = format!("{} {} HTTP/1.1\r\n", self.method, self.path).into_bytes();
        for (name, value) in &self.headers {
            out.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
        }
        let has_length = self
            .headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case("content-length"));
        if !self.body.is_empty() && !has_length {
            out.extend_from_slice(format!("Content-Length: {}\r\n", self.body.len()).as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&self.body);
        out
    }

    /// The head only, for a test that sends the body itself.
    pub fn head_bytes(&self) -> Vec<u8> {
        let mut head = self.to_bytes();
        let end = head.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        head.truncate(end);
        head
    }
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Headers,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|e| panic!("{e}: {}", self.text()))
    }
}

type Headers = Vec<(String, String)>;

/// The length of the head, the status and the headers of a response, once the head is whole.
fn split_head(buf: &[u8]) -> Option<(usize, u16, Headers)> {
    let end = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&buf[..end]).ok()?;
    let mut lines = head.split("\r\n");
    let status = lines.next()?.split(' ').nth(1)?.parse().ok()?;
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(n, v)| (n.trim().to_owned(), v.trim().to_owned()))
        .collect();
    Some((end + 4, status, headers))
}

/// Reads one whole response (the request said `Connection: close`, or carries a length).
pub fn read_response(stream: &mut TcpStream) -> Response {
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 16 * 1024];
    let (head_len, status, headers) = loop {
        if let Some(head) = split_head(&buf) {
            break head;
        }
        match stream.read(&mut chunk) {
            Ok(0) => panic!("the connection closed before a response head: {buf:?}"),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) => panic!("reading a response head: {e}; got {} bytes", buf.len()),
        }
    };
    let find = |name: &str| {
        headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    };
    let mut body = buf[head_len..].to_vec();
    let chunked = find("transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked"));
    let length: Option<usize> = find("content-length").and_then(|v| v.parse().ok());
    loop {
        let done = if chunked {
            decode_chunked(&body).is_some()
        } else {
            length.is_some_and(|n| body.len() >= n)
        };
        if done {
            break;
        }
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => body.extend_from_slice(&chunk[..n]),
            // A reset after the response was sent is how a refused upload ends.
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => break,
            Err(e) => panic!("reading a response body: {e}"),
        }
    }
    let body = if chunked {
        decode_chunked(&body).unwrap_or_default()
    } else {
        body
    };
    Response {
        status,
        headers,
        body,
    }
}

/// The decoded body when `raw` holds a complete chunked body.
fn decode_chunked(raw: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut at = 0;
    loop {
        let line_end = raw[at..].windows(2).position(|w| w == b"\r\n")? + at;
        let size = usize::from_str_radix(
            std::str::from_utf8(&raw[at..line_end])
                .ok()?
                .split(';')
                .next()?
                .trim(),
            16,
        )
        .ok()?;
        at = line_end + 2;
        if size == 0 {
            return raw.len().checked_sub(at).map(|_| out);
        }
        if raw.len() < at + size + 2 {
            return None;
        }
        out.extend_from_slice(&raw[at..at + size]);
        at += size + 2;
    }
}

/// One item of an event stream.
#[derive(Debug, PartialEq, Eq)]
pub enum Sse {
    /// A line that starts with `:`, without it.
    Comment(String),
    Frame {
        id: u64,
        data: String,
    },
}

pub enum Got {
    Item(Sse),
    /// The stream ended (the chunked body terminated).
    End,
    TimedOut,
}

/// A `text/event-stream` response read as it arrives.
pub struct SseReader {
    stream: TcpStream,
    raw: Vec<u8>,
    decoded: Vec<u8>,
    ended: bool,
    pub status: u16,
    pub headers: Vec<(String, String)>,
}

impl SseReader {
    pub fn open(server: &TestServer, query: &str) -> SseReader {
        Self::open_with(
            server,
            server.authed("GET", &format!("/api/v1/events{query}")),
        )
    }

    pub fn open_with(server: &TestServer, request: Req) -> SseReader {
        let request = request
            .without("connection")
            .with(API_VERSION_HEADER, "1")
            .with("Accept", "text/event-stream");
        let mut stream = server.connect();
        stream.write_all(&request.to_bytes()).unwrap();
        let mut raw = Vec::new();
        let mut chunk = [0_u8; 4096];
        let (head_len, status, headers) = loop {
            if let Some(head) = split_head(&raw) {
                break head;
            }
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0, "closed before the head");
            raw.extend_from_slice(&chunk[..n]);
        };
        raw.drain(..head_len);
        SseReader {
            stream,
            raw,
            decoded: Vec::new(),
            ended: false,
            status,
            headers,
        }
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Moves complete chunks from `raw` to `decoded`.
    fn decode(&mut self) {
        loop {
            let Some(line_end) = self.raw.windows(2).position(|w| w == b"\r\n") else {
                return;
            };
            let Ok(size) = usize::from_str_radix(
                std::str::from_utf8(&self.raw[..line_end])
                    .unwrap_or("x")
                    .trim(),
                16,
            ) else {
                panic!("a bad chunk size line: {:?}", &self.raw[..line_end]);
            };
            if size == 0 {
                self.ended = true;
                return;
            }
            let need = line_end + 2 + size + 2;
            if self.raw.len() < need {
                return;
            }
            self.decoded
                .extend_from_slice(&self.raw[line_end + 2..line_end + 2 + size]);
            self.raw.drain(..need);
        }
    }

    fn take_event(&mut self) -> Option<Sse> {
        let end = self.decoded.windows(2).position(|w| w == b"\n\n")?;
        let block: Vec<u8> = self.decoded.drain(..end + 2).collect();
        let text = String::from_utf8(block).unwrap();
        let text = text.trim_end_matches('\n');
        if let Some(comment) = text.strip_prefix(':') {
            return Some(Sse::Comment(comment.trim().to_owned()));
        }
        let mut id = None;
        let mut data = None;
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("id: ") {
                id = Some(v.parse().unwrap());
            } else if let Some(v) = line.strip_prefix("data: ") {
                assert!(data.is_none(), "two data lines in one frame");
                data = Some(v.to_owned());
            } else {
                panic!("an unexpected line in a frame: {line:?}");
            }
        }
        Some(Sse::Frame {
            id: id.expect("a frame has an id"),
            data: data.expect("a frame has data"),
        })
    }

    pub fn read(&mut self, within: Duration) -> Got {
        let deadline = Instant::now() + within;
        loop {
            self.decode();
            if let Some(item) = self.take_event() {
                return Got::Item(item);
            }
            if self.ended {
                return Got::End;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Got::TimedOut;
            }
            self.stream.set_read_timeout(Some(left)).unwrap();
            let mut chunk = [0_u8; 16 * 1024];
            match self.stream.read(&mut chunk) {
                Ok(0) => {
                    self.ended = true;
                }
                Ok(n) => self.raw.extend_from_slice(&chunk[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return Got::TimedOut;
                }
                Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => {
                    self.ended = true;
                }
                Err(e) => panic!("reading the stream: {e}"),
            }
        }
    }

    /// The next item, or a panic if none comes.
    pub fn next(&mut self, within: Duration) -> Sse {
        match self.read(within) {
            Got::Item(item) => item,
            Got::End => panic!("the stream ended"),
            Got::TimedOut => panic!("no item within {within:?}"),
        }
    }

    /// The next item that is a frame, skipping comments.
    pub fn next_frame(&mut self, within: Duration) -> (u64, tc_api::event::EventFrame) {
        let deadline = Instant::now() + within;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.next(left.max(Duration::from_millis(1))) {
                Sse::Comment(_) => {}
                Sse::Frame { id, data } => return (id, serde_json::from_str(&data).unwrap()),
            }
        }
    }
}

pub fn stub() -> Arc<Stub> {
    Arc::new(Stub::new().unwrap())
}

/// Records every call and answers `ok`.
#[derive(Default)]
pub struct Recorder {
    pub seen: Mutex<Vec<(String, Vec<u8>)>>,
}

impl Recorder {
    pub fn paths(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .map(|(p, _)| p.clone())
            .collect()
    }
}

impl Backend for Recorder {
    type Events = StubEvents;

    async fn call(&self, path: &'static str, body: tc_server::Bytes) -> Answer {
        self.seen
            .lock()
            .unwrap()
            .push((path.to_owned(), body.to_vec()));
        Answer::Reply {
            body: br#"{"type":"ok","value":null}"#.to_vec(),
            then: Then::Continue,
        }
    }

    fn subscribe(&self, _after: u64) -> StubEvents {
        StubEvents
    }

    async fn quit(&self) {}

    async fn quitting(&self) {
        std::future::pending::<()>().await;
    }
}
