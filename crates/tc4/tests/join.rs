//! `tc4 serve` runs tc-app's `App` behind tc-server (APP-DESIGN §10, order paragraph): what the
//! shell and the UI see over HTTP is the application's own state, not a stand-in's.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::{Read as _, Write as _};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use common::{START, Tc4, http};
use serde_json::{Value, json};
use tempfile::TempDir;

/// An app-data folder and a UI folder for a test.
struct Dirs {
    app_data: TempDir,
    _ui_root: TempDir,
    ui: std::path::PathBuf,
}

fn dirs() -> Dirs {
    let ui_root = TempDir::new().unwrap();
    let ui = http::seed_ui(ui_root.path());
    Dirs {
        app_data: TempDir::new().unwrap(),
        _ui_root: ui_root,
        ui,
    }
}

fn start(dirs: &Dirs) -> Tc4 {
    let mut tc4 = Tc4::spawn(dirs.app_data.path(), &dirs.ui, &[]);
    tc4.address(START);
    tc4
}

/// The `value` of an `ok` reply to an API call.
fn ok(tc4: &Tc4, path: &str, body: &Value) -> Value {
    let reply = tc4.api(path, &body.to_string());
    assert_eq!(reply.status, 200, "{path}: {}", reply.text());
    let json = reply.json();
    assert_eq!(json["type"], "ok", "{path}: {}", reply.text());
    json["value"].clone()
}

fn hello(tc4: &Tc4) -> Value {
    ok(tc4, "app/hello", &json!({}))
}

fn net(tc4: &Tc4, id: u8, on: bool) {
    let request = format!("{id:02x}").repeat(16);
    ok(tc4, "net/set", &json!({"request": request, "on": on}));
}

#[test]
fn tc4_serve_answers_hello_with_real_state() {
    let dirs = dirs();
    let mut first = start(&dirs);
    let before = hello(&first);
    assert_eq!(before["api"], 1);
    assert_eq!(before["build"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(before["internet"], false);
    assert_eq!(before["prefs"]["appLocale"], "en");
    assert_eq!(before["sessions"], json!([]));
    assert_eq!(before["startup"]["type"], "ready");

    // The state in the reply is the app's: it follows what the UI did.
    net(&first, 1, true);
    ok(
        &first,
        "prefs/setLocale",
        &json!({"request": "03".repeat(16), "locale": "pt-BR"}),
    );
    let after = hello(&first);
    assert_eq!(after["internet"], true);
    assert_eq!(after["prefs"]["appLocale"], "pt-BR");
    assert_eq!(after["instance"], before["instance"]);
    assert_eq!(first.quit().code(), Some(0));
    drop(first);

    // Every launch is a new instance, and a launch starts with the switch off.
    let mut second = start(&dirs);
    let again = hello(&second);
    assert_ne!(again["instance"], before["instance"]);
    assert_eq!(again["internet"], false);
    assert_eq!(second.quit().code(), Some(0));
}

#[test]
fn the_app_decides_what_a_request_means_not_the_server() {
    let dirs = dirs();
    let mut tc4 = start(&dirs);
    // A body that is not a request is the app's `BadRequest`, a reply, not a server error.
    let reply = tc4.api("net/set", r#"{"on":true,"extra":1}"#);
    assert_eq!(reply.status, 200, "{}", reply.text());
    assert_eq!(reply.json()["type"], "err", "{}", reply.text());
    assert_eq!(reply.json()["value"]["type"], "badRequest");
    // A path that is not an endpoint never reaches the app.
    assert_eq!(tc4.api("net/nothing", "{}").status, 404);
    assert_eq!(tc4.quit().code(), Some(0));
}

/// Server-sent events read raw from a socket: `id:` and `data:` lines, whatever the chunking.
struct Events {
    stream: TcpStream,
    text: String,
}

impl Events {
    fn open(tc4: &Tc4, query: &str) -> Events {
        let request = tc4
            .req("GET", &format!("/api/v1/events{query}"))
            .without("connection")
            .with("X-Tc-Api", "1")
            .with("Accept", "text/event-stream");
        let mut stream = tc4.connect();
        stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        stream.write_all(&request.to_bytes()).unwrap();
        let mut events = Events {
            stream,
            text: String::new(),
        };
        events.read_until(|text| text.contains("\r\n\r\n"));
        assert!(events.text.starts_with("HTTP/1.1 200"), "{}", events.text);
        let head_end = events.text.find("\r\n\r\n").unwrap() + 4;
        events.text.drain(..head_end);
        events
    }

    fn read_until(&mut self, done: impl Fn(&str) -> bool) {
        let until = Instant::now() + Duration::from_secs(10);
        let mut chunk = [0_u8; 4096];
        while !done(&self.text) {
            assert!(Instant::now() < until, "timed out; got {:?}", self.text);
            match self.stream.read(&mut chunk) {
                Ok(0) => panic!("the stream ended; got {:?}", self.text),
                Ok(n) => self.text.push_str(&String::from_utf8_lossy(&chunk[..n])),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => panic!("reading the stream: {e}"),
            }
        }
    }

    fn frames(&self) -> Vec<(u64, Value)> {
        let mut frames = Vec::new();
        let mut id = None;
        for line in self.text.lines() {
            if let Some(n) = line.strip_prefix("id: ") {
                id = Some(n.parse::<u64>().unwrap());
            } else if let Some(data) = line.strip_prefix("data: ") {
                frames.push((
                    id.take().expect("an id before its data"),
                    serde_json::from_str(data).unwrap(),
                ));
            }
        }
        frames
    }

    /// The first `count` frames, waiting for them.
    fn next_frames(&mut self, count: usize) -> Vec<(u64, Value)> {
        self.read_until(|text| {
            text.matches("\ndata: ").count() + usize::from(text.starts_with("data: ")) >= count
        });
        let mut frames = self.frames();
        frames.truncate(count);
        frames
    }
}

fn kinds(frames: &[(u64, Value)]) -> Vec<String> {
    frames
        .iter()
        .map(|(_, frame)| {
            let event = &frame["event"];
            match event["type"].as_str().unwrap() {
                "internet" => format!("internet {}", event["value"]["on"]),
                other => other.to_owned(),
            }
        })
        .collect()
}

#[test]
fn events_replay_and_resync_through_the_app_bus() {
    let dirs = dirs();
    let mut tc4 = start(&dirs);
    let instance = hello(&tc4)["instance"].clone();
    net(&tc4, 1, true);
    net(&tc4, 2, false);
    net(&tc4, 3, true);

    // A reader that holds nothing gets every frame the ring holds, in order, from this run.
    let mut all = Events::open(&tc4, "?after=0");
    let frames = all.next_frames(3);
    assert_eq!(
        frames.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(
        kinds(&frames),
        ["internet true", "internet false", "internet true"]
    );
    for (id, frame) in &frames {
        assert_eq!(frame["seq"], *id);
        assert_eq!(frame["instance"], instance);
    }

    // A reader that holds some gets the rest, and then what happens next.
    let mut later = Events::open(&tc4, "?after=2");
    assert_eq!(later.next_frames(1)[0].0, 3);
    net(&tc4, 4, false);
    let live = later.next_frames(2);
    assert_eq!(live[1].0, 4);
    assert_eq!(kinds(&live[1..]), ["internet false"]);
    // The first reader, still connected, saw the same live frame.
    assert_eq!(all.next_frames(4)[3].0, 4);

    // A number this run never reached is another run's: a Resync first, then the live frames.
    let mut stale = Events::open(&tc4, "?after=1000000");
    let resync = stale.next_frames(1);
    assert_eq!(kinds(&resync), ["resync"]);
    net(&tc4, 5, true);
    let next = stale.next_frames(2);
    assert_eq!(next[1].0, 5);
    assert_eq!(tc4.quit().code(), Some(0));
}

#[test]
fn a_projects_folder_that_is_not_there_is_one_error_line_and_exit_4() {
    let dirs = dirs();
    let missing = dirs.app_data.path().join("no-such-projects-folder");
    let mut tc4 = Tc4::spawn(
        &dirs.app_data.path().join("data"),
        &dirs.ui,
        &["--projects", &missing.display().to_string()],
    );
    let status = tc4.wait_exit(Duration::from_secs(20)).expect("it exits");
    assert_eq!(status.code(), Some(4), "stderr: {}", tc4.stderr());
    // The error is the only line: the address line comes after the app has started.
    let lines = tc4.rest_of_stdout();
    assert_eq!(lines.len(), 1, "{lines:?}");
    let error: Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(error["error"]["type"], "start", "{error}");
    let reason = error["error"]["value"]["reason"].as_str().unwrap();
    assert!(reason.contains("no-such-projects-folder"), "{reason}");
    assert!(
        !missing.exists(),
        "a folder was made in place of the named one"
    );
}

#[test]
fn without_projects_the_app_makes_its_default_folder_under_the_home_folder() {
    let dirs = dirs();
    let home = TempDir::new().unwrap();
    let mut tc4 = Tc4::spawn_at_home(dirs.app_data.path(), &dirs.ui, &[], home.path());
    tc4.address(START);
    let default = home.path().join("translationCore4").join("projects");
    assert!(default.is_dir(), "{}", tc4.stderr());
    assert_eq!(tc4.quit().code(), Some(0));
    // The line comes after the address line, so it is read once the process has ended.
    assert!(
        tc4.stderr().contains(&default.display().to_string()),
        "the folder in use is on stderr: {}",
        tc4.stderr()
    );
}
