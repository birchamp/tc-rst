//! The stand-in behaves as §5.1 and §5.2 say `tc4 serve` does, and serves what the probes need.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    port: u16,
    token: String,
    app_data: PathBuf,
}

impl Server {
    fn start(name: &str) -> Self {
        let app_data =
            std::env::temp_dir().join(format!("tc4-standin-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&app_data);
        let mut child = Command::new(env!("CARGO_BIN_EXE_tc4-standin"))
            .args(["serve", "--app-data"])
            .arg(&app_data)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("the stand-in starts");
        let stdin = child.stdin.take();
        let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
        let mut line = String::new();
        stdout.read_line(&mut line).expect("an address line");
        let address: serde_json::Value = serde_json::from_str(&line).expect("JSON");
        let url = address["url"].as_str().expect("url").to_owned();
        let token = address["token"].as_str().expect("token").to_owned();
        let port = url
            .strip_prefix("http://127.0.0.1:")
            .and_then(|rest| rest.strip_suffix('/'))
            .expect("a loopback url with a trailing slash")
            .parse()
            .expect("a port");
        Self {
            child,
            stdin,
            stdout,
            port,
            token,
            app_data,
        }
    }

    fn host(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    fn origin(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Sends one request with exactly these headers and reads the whole response.
    fn raw(&self, method: &str, path: &str, headers: &[(&str, String)], body: &str) -> Response {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut request = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\n");
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
        stream.write_all(request.as_bytes()).unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        Response::parse(&bytes)
    }

    fn get(&self, path: &str) -> Response {
        self.raw(
            "GET",
            path,
            &[("Host", self.host()), ("X-Tc-Token", self.token.clone())],
            "",
        )
    }

    fn post_api(&self, path: &str, body: &str) -> Response {
        self.raw(
            "POST",
            path,
            &[
                ("Host", self.host()),
                ("X-Tc-Token", self.token.clone()),
                ("Origin", self.origin()),
                ("X-Tc-Api", "1".to_owned()),
                ("Content-Type", "application/json".to_owned()),
            ],
            body,
        )
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.app_data.join("standin.log")).unwrap_or_default()
    }

    fn wait_exit(&mut self, within: Duration) -> Option<Duration> {
        let start = Instant::now();
        while start.elapsed() < within {
            if self.child.try_wait().unwrap().is_some() {
                return Some(start.elapsed());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.app_data);
    }
}

struct Response {
    status: u16,
    head: String,
    body: Vec<u8>,
}

impl Response {
    fn parse(bytes: &[u8]) -> Self {
        let split = bytes
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("a response head");
        let head = String::from_utf8_lossy(&bytes[..split]).into_owned();
        let status = head.split(' ').nth(1).unwrap().parse().unwrap();
        let mut body = bytes[split + 4..].to_vec();
        if head
            .to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
        {
            body = dechunk(&body);
        }
        Self { status, head, body }
    }

    fn header(&self, name: &str) -> Option<String> {
        let name = format!("{}:", name.to_ascii_lowercase());
        self.head.lines().find_map(|line| {
            line.to_ascii_lowercase()
                .starts_with(&name)
                .then(|| line[name.len()..].trim().to_owned())
        })
    }
}

fn dechunk(mut body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let line_end = body.windows(2).position(|w| w == b"\r\n").unwrap();
        let size =
            usize::from_str_radix(std::str::from_utf8(&body[..line_end]).unwrap().trim(), 16)
                .unwrap();
        body = &body[line_end + 2..];
        if size == 0 {
            return out;
        }
        out.extend_from_slice(&body[..size]);
        body = &body[size + 2..];
    }
}

#[test]
fn prints_one_loopback_address_line_with_a_43_character_token() {
    let server = Server::start("address");
    assert_eq!(server.token.len(), 43);
    assert!(
        server
            .token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    );
    let other = Server::start("address-other");
    assert_ne!(server.token, other.token, "every launch has its own token");
}

#[test]
fn every_route_refuses_a_request_without_the_token() {
    let server = Server::start("token");
    for path in [
        "/",
        "/ui/probe.js",
        "/help/page.html",
        "/media/obs/01.png",
        "/download/1/a.txt",
        "/print/1",
        "/prepare.html",
    ] {
        let missing = server.raw("GET", path, &[("Host", server.host())], "");
        assert_eq!(missing.status, 401, "{path} without a token");
        let wrong = server.raw(
            "GET",
            path,
            &[("Host", server.host()), ("X-Tc-Token", "x".repeat(43))],
            "",
        );
        assert_eq!(wrong.status, 401, "{path} with another token");
        assert_eq!(server.get(path).status, 200, "{path} with the token");
    }
}

#[test]
fn a_foreign_host_or_origin_is_refused_and_no_cors_header_is_sent() {
    let server = Server::start("host");
    let foreign = server.raw(
        "GET",
        "/",
        &[
            ("Host", "evil.example:80".to_owned()),
            ("X-Tc-Token", server.token.clone()),
        ],
        "",
    );
    assert_eq!(foreign.status, 403);
    let cross = server.raw(
        "POST",
        "/api/v1/spike/hello",
        &[
            ("Host", server.host()),
            ("X-Tc-Token", server.token.clone()),
            ("Origin", "http://evil.example".to_owned()),
            ("X-Tc-Api", "1".to_owned()),
        ],
        "{}",
    );
    assert_eq!(cross.status, 403);
    let no_api_header = server.raw(
        "POST",
        "/api/v1/spike/hello",
        &[
            ("Host", server.host()),
            ("X-Tc-Token", server.token.clone()),
            ("Origin", server.origin()),
        ],
        "{}",
    );
    assert_eq!(no_api_header.status, 409);
    for response in [
        foreign,
        cross,
        no_api_header,
        server.get("/"),
        server.post_api("/api/v1/spike/hello", "{}"),
    ] {
        assert!(
            !response
                .head
                .to_ascii_lowercase()
                .contains("access-control-"),
            "no CORS header: {}",
            response.head
        );
    }
}

#[test]
fn index_carries_the_content_security_policy() {
    let server = Server::start("csp");
    let index = server.get("/");
    let csp = index.header("content-security-policy").expect("a CSP");
    assert!(
        csp.contains("default-src 'self'") && csp.contains("connect-src 'self'"),
        "{csp}"
    );
    assert_eq!(index.header("cache-control").as_deref(), Some("no-store"));
}

#[test]
fn a_click_answers_at_least_20_kb_of_json() {
    let server = Server::start("click");
    let reply = server.post_api("/api/v1/align/click", r#"{"word":3}"#);
    assert_eq!(reply.status, 200);
    assert!(
        reply.body.len() >= 20 * 1024 && reply.body.len() < 22 * 1024,
        "{} bytes",
        reply.body.len()
    );
    let json: serde_json::Value = serde_json::from_slice(&reply.body).unwrap();
    assert_eq!(json["armed"], "b3");
    assert!(json["view"]["bank"].as_array().unwrap().len() > 20);
}

#[test]
fn pictures_are_obs_sized_pngs() {
    let server = Server::start("picture");
    let picture = server.get("/media/obs/07.png?r=1");
    assert_eq!(picture.header("content-type").as_deref(), Some("image/png"));
    assert_eq!(&picture.body[..8], b"\x89PNG\r\n\x1a\n");
    let width = u32::from_be_bytes(picture.body[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(picture.body[20..24].try_into().unwrap());
    assert_eq!((width, height), (640, 360));
    assert!(
        (50_000..100_000).contains(&picture.body.len()),
        "{} bytes",
        picture.body.len()
    );
}

#[test]
fn a_download_is_an_attachment_and_print_is_html() {
    let server = Server::start("download");
    let download = server.get("/download/7/probe.txt");
    assert_eq!(
        download.header("content-disposition").as_deref(),
        Some("attachment; filename=\"probe.txt\"")
    );
    assert!(!download.body.is_empty());
    let print = server.get("/print/1");
    assert!(String::from_utf8_lossy(&print.body).contains("id=\"print-ok\""));
}

#[test]
fn the_event_stream_sends_frames_one_by_one() {
    let server = Server::start("events");
    let mut stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let request = format!(
        "GET /api/v1/events?every=100 HTTP/1.1\r\nHost: {}\r\nX-Tc-Token: {}\r\nX-Tc-Api: 1\r\n\r\n",
        server.host(),
        server.token
    );
    stream.write_all(request.as_bytes()).unwrap();
    let start = Instant::now();
    let mut seen = Vec::new();
    let mut text = String::new();
    let mut buffer = [0u8; 4096];
    while seen.len() < 3 {
        let n = stream.read(&mut buffer).unwrap();
        assert!(n > 0, "the stream ended");
        text.push_str(&String::from_utf8_lossy(&buffer[..n]));
        while let Some(at) = text.find("data: ") {
            text = text[at + 6..].to_owned();
            seen.push(start.elapsed());
        }
    }
    assert!(
        seen[0] < Duration::from_millis(500),
        "first frame at {:?}",
        seen[0]
    );
    assert!(
        seen[2] >= Duration::from_millis(150),
        "frames are paced, not sent at once: {seen:?}"
    );
    assert!(
        seen[2] < Duration::from_millis(1500),
        "third frame at {:?}",
        seen[2]
    );
}

#[test]
fn the_probe_log_says_whether_each_request_carried_the_token() {
    let server = Server::start("log");
    server.raw(
        "GET",
        "/media/obs/01.png?probe=p1",
        &[("Host", server.host())],
        "",
    );
    server.get("/media/obs/01.png?probe=p1");
    let log = server.post_api("/api/v1/spike/log", r#"{"probe":"p1"}"#);
    let hits: serde_json::Value = serde_json::from_slice(&log.body).unwrap();
    let seen: Vec<(String, u64)> = hits["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| {
            (
                h["token"].as_str().unwrap().to_owned(),
                h["status"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        vec![("missing".to_owned(), 401), ("ok".to_owned(), 200)]
    );
}

#[test]
fn a_closed_stdin_quits_within_5_s() {
    let mut server = Server::start("stdin");
    drop(server.stdin.take());
    let took = server
        .wait_exit(Duration::from_secs(5))
        .expect("exits within 5 s");
    assert!(took < Duration::from_secs(5));
    assert!(
        server.log().contains("\"why\":\"stdin-closed\""),
        "{}",
        server.log()
    );
}

#[test]
fn a_quit_message_is_answered_then_the_server_exits() {
    let mut server = Server::start("quit");
    let stdin = server.stdin.as_mut().unwrap();
    stdin.write_all(b"{\"quit\":true}\n").unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    server.stdout.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "{\"closed\":true}");
    assert!(server.wait_exit(Duration::from_secs(5)).is_some());
    assert!(server.log().contains("\"why\":\"quit-message\""));
}

#[test]
fn the_token_is_never_written_to_app_data() {
    let mut server = Server::start("secret");
    server.get("/");
    server.post_api("/api/v1/spike/typed", r#"{"field":"f","n":3,"at":1}"#);
    drop(server.stdin.take());
    server.wait_exit(Duration::from_secs(5));
    for entry in std::fs::read_dir(&server.app_data).unwrap() {
        let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        assert!(
            !text.contains(&server.token),
            "the token leaked into app data"
        );
    }
    let typed = std::fs::read_to_string(server.app_data.join("typed.log")).unwrap();
    assert!(typed.contains("\"n\":3"), "{typed}");
}
