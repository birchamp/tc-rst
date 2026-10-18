//! Starts the real `tc4` binary with piped stdio, and speaks HTTP to it with the client of
//! tc-server's tests. Every wait has a timeout, and a child a failed test leaves running is
//! killed.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../../../tc-server/tests/common/mod.rs"]
pub mod http;

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tc_api::endpoint::{API_VERSION_HEADER, TOKEN_HEADER};

pub use http::{Req, Response};

/// How long a start gets to print its address line: far more than it needs, since the
/// machine is shared.
pub const START: Duration = Duration::from_secs(20);

/// The first line `tc4 serve` prints.
#[derive(Debug, Clone)]
pub struct Address {
    pub url: String,
    pub token: String,
    pub port: u16,
}

pub struct Tc4 {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    stderr: Arc<Mutex<String>>,
    stderr_reader: Option<std::thread::JoinHandle<()>>,
    pub address: Option<Address>,
    started: Instant,
}

/// The binary, with the hooks the test build carries.
pub fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_tc4")
}

impl Tc4 {
    /// `tc4 serve --app-data <app_data> --ui <ui> <extra>` with piped stdio.
    pub fn spawn(app_data: &Path, ui: &Path, extra: &[&str]) -> Tc4 {
        let mut args = vec![
            "serve".to_owned(),
            "--app-data".to_owned(),
            app_data.display().to_string(),
            "--ui".to_owned(),
            ui.display().to_string(),
        ];
        args.extend(extra.iter().map(|arg| (*arg).to_owned()));
        Self::spawn_args(&args)
    }

    /// As `spawn`, for another build of the binary.
    pub fn spawn_binary(binary: &Path, app_data: &Path, ui: &Path, extra: &[&str]) -> Tc4 {
        let mut args = vec![
            "serve".to_owned(),
            "--app-data".to_owned(),
            app_data.display().to_string(),
            "--ui".to_owned(),
            ui.display().to_string(),
        ];
        args.extend(extra.iter().map(|arg| (*arg).to_owned()));
        Self::spawn_command(binary, &args)
    }

    pub fn spawn_args(args: &[String]) -> Tc4 {
        Self::spawn_command(Path::new(binary()), args)
    }

    fn spawn_command(binary: &Path, args: &[String]) -> Tc4 {
        let mut child = Command::new(binary)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let (sender, lines) = channel();
        let stdout = child.stdout.take().unwrap();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let stderr = Arc::new(Mutex::new(String::new()));
        let collected = Arc::clone(&stderr);
        let pipe = child.stderr.take().unwrap();
        let stderr_reader = std::thread::spawn(move || {
            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                let mut text = collected.lock().unwrap();
                text.push_str(&line);
                text.push('\n');
            }
        });
        let stdin = child.stdin.take();
        Tc4 {
            child,
            stdin,
            lines,
            stderr,
            stderr_reader: Some(stderr_reader),
            address: None,
            started: Instant::now(),
        }
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// The next line on stdout, or `None` if none comes in time (or stdout ended).
    pub fn next_line(&mut self, within: Duration) -> Option<String> {
        self.lines.recv_timeout(within).ok()
    }

    /// Reads the address line and remembers it.
    pub fn address(&mut self, within: Duration) -> Address {
        let line = self
            .next_line(within)
            .unwrap_or_else(|| panic!("no address line in {within:?}; stderr: {}", self.stderr()));
        let json: serde_json::Value = serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("the first line is not JSON ({e}): {line}"));
        let url = json["url"].as_str().expect("a url").to_owned();
        let token = json["token"].as_str().expect("a token").to_owned();
        let port = url
            .trim_end_matches('/')
            .rsplit(':')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let address = Address { url, token, port };
        self.address = Some(address.clone());
        address
    }

    /// Everything else on stdout until it ends (the process is gone or about to be).
    pub fn rest_of_stdout(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        while let Some(line) = self.next_line(Duration::from_secs(2)) {
            lines.push(line);
        }
        lines
    }

    pub fn stderr(&self) -> String {
        self.stderr.lock().unwrap().clone()
    }

    pub fn send_line(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().expect("stdin is open");
        stdin.write_all(line.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    }

    pub fn send_bytes(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().expect("stdin is open");
        stdin.write_all(bytes).unwrap();
        stdin.flush().unwrap();
    }

    /// The shell died: stdin reaches its end.
    pub fn close_stdin(&mut self) {
        self.stdin = None;
    }

    pub fn try_exit(&mut self) -> Option<ExitStatus> {
        let status = self.child.try_wait().unwrap();
        if status.is_some() {
            self.collect_stderr();
        }
        status
    }

    /// Waits for the reader thread to take the last of stderr: the process has ended, so its end
    /// of the pipe is closed. Without this, a test that reads `stderr()` right after the exit
    /// can see it empty.
    fn collect_stderr(&mut self) {
        if let Some(reader) = self.stderr_reader.take() {
            let _ = reader.join();
        }
    }

    /// Waits for the process to end.
    pub fn wait_exit(&mut self, within: Duration) -> Option<ExitStatus> {
        let until = Instant::now() + within;
        loop {
            if let Some(status) = self.try_exit() {
                return Some(status);
            }
            if Instant::now() >= until {
                return None;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn since_start(&self) -> Duration {
        self.started.elapsed()
    }

    /// `{"quit":true}`, then waits for `{"closed":true}` and the exit.
    pub fn quit(&mut self) -> ExitStatus {
        self.send_line(r#"{"quit":true}"#);
        assert_eq!(
            self.next_line(Duration::from_secs(10)).as_deref(),
            Some(r#"{"closed":true}"#),
            "stderr: {}",
            self.stderr()
        );
        self.wait_exit(Duration::from_secs(10))
            .expect("tc4 ended after closed")
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.collect_stderr();
    }

    fn address_ref(&self) -> &Address {
        self.address.as_ref().expect("the address was read")
    }

    /// A request with the headers every good request carries.
    pub fn req(&self, method: &str, path: &str) -> Req {
        let address = self.address_ref();
        Req::new(method, path)
            .with("Host", &format!("127.0.0.1:{}", address.port))
            .with(TOKEN_HEADER, &address.token)
    }

    pub fn api_req(&self, path: &str, json: &str) -> Req {
        self.req("POST", &format!("/api/v1/{path}"))
            .with(
                "Origin",
                &format!("http://127.0.0.1:{}", self.address_ref().port),
            )
            .with(API_VERSION_HEADER, "1")
            .with("Content-Type", "application/json")
            .body(json.as_bytes())
    }

    pub fn send(&self, request: &Req) -> Response {
        let mut stream = self.connect();
        stream.write_all(&request.to_bytes()).unwrap();
        http::read_response(&mut stream)
    }

    pub fn api(&self, path: &str, json: &str) -> Response {
        self.send(&self.api_req(path, json))
    }

    pub fn connect(&self) -> TcpStream {
        let stream = TcpStream::connect(("127.0.0.1", self.address_ref().port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream
    }
}

impl Drop for Tc4 {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Runs `tc4` with `args` and no stdin to the end. For command lines that must be refused at
/// once; a hang fails the test.
pub fn run_to_end(args: &[&str]) -> (Option<i32>, String, String) {
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    let mut tc4 = Tc4::spawn_args(&args);
    tc4.close_stdin();
    let status = tc4.wait_exit(Duration::from_secs(20));
    let stdout = tc4.rest_of_stdout().join("\n");
    let code = status.map(|status| status.code().unwrap_or(-1));
    (code, stdout, tc4.stderr())
}
