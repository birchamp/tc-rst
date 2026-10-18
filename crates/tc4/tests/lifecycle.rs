//! How `tc4 serve` starts, quits and shares its app data, run against the real binary
//! (APP-DESIGN §5.1, §5.2). tc-server's own `tests/lifecycle.rs` drives the quit sequence in
//! process.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::Duration;

use common::{START, Tc4, http, run_to_end};
use tempfile::TempDir;

/// An app-data folder and a UI folder for a test.
struct Dirs {
    app_data: TempDir,
    ui_root: TempDir,
    ui: std::path::PathBuf,
}

fn dirs() -> Dirs {
    let ui_root = TempDir::new().unwrap();
    let ui = http::seed_ui(ui_root.path());
    Dirs {
        app_data: TempDir::new().unwrap(),
        ui_root,
        ui,
    }
}

fn start(dirs: &Dirs) -> Tc4 {
    let mut tc4 = Tc4::spawn(dirs.app_data.path(), &dirs.ui, &[]);
    tc4.address(START);
    tc4
}

fn code(status: std::process::ExitStatus) -> Option<i32> {
    status.code()
}

#[test]
fn the_address_line_comes_first_and_names_loopback() {
    let dirs = dirs();
    let mut tc4 = Tc4::spawn(dirs.app_data.path(), &dirs.ui, &[]);
    let address = tc4.address(START);
    assert_eq!(address.url, format!("http://127.0.0.1:{}/", address.port));
    assert_eq!(address.token.len(), 43, "{}", address.token);
    assert!(
        address
            .token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "not base64url: {}",
        address.token
    );
    let hello = tc4.api("app/hello", "{}");
    assert_eq!(hello.status, 200, "{}", hello.text());
    assert_eq!(hello.json()["type"], "ok");
    // The address line was the first and the only line so far.
    assert_eq!(code(tc4.quit()), Some(0));
    assert!(tc4.rest_of_stdout().is_empty());
}

#[test]
fn closed_stdin_quits_within_5_s() {
    let dirs = dirs();
    let mut tc4 = start(&dirs);
    assert_eq!(tc4.api("app/hello", "{}").status, 200);
    tc4.close_stdin();
    let status = tc4
        .wait_exit(Duration::from_secs(5))
        .unwrap_or_else(|| panic!("tc4 did not quit; stderr: {}", tc4.stderr()));
    assert_eq!(code(status), Some(0));
    assert_eq!(
        tc4.rest_of_stdout(),
        Vec::<String>::new(),
        "nobody is there to hear closed, and the address line was read"
    );
}

#[test]
fn quit_message_answers_closed_then_exits() {
    let dirs = dirs();
    let mut tc4 = start(&dirs);
    tc4.send_line(r#"{"quit":true}"#);
    assert_eq!(
        tc4.next_line(Duration::from_secs(5)).as_deref(),
        Some(r#"{"closed":true}"#)
    );
    let status = tc4.wait_exit(Duration::from_secs(5)).expect("it exits");
    assert_eq!(code(status), Some(0));
}

#[test]
fn app_quit_from_the_ui_quits_like_the_quit_message() {
    let dirs = dirs();
    let mut tc4 = start(&dirs);
    let reply = tc4.api(
        "app/quit",
        r#"{"request":"00000000000000000000000000000003"}"#,
    );
    assert_eq!(
        reply.json(),
        serde_json::json!({"type": "ok", "value": null})
    );
    assert_eq!(
        tc4.next_line(Duration::from_secs(5)).as_deref(),
        Some(r#"{"closed":true}"#)
    );
    assert_eq!(
        code(tc4.wait_exit(Duration::from_secs(5)).unwrap()),
        Some(0)
    );
}

#[cfg(unix)]
#[test]
fn sigterm_quits_like_the_quit_message() {
    let dirs = dirs();
    let mut tc4 = start(&dirs);
    let killed = std::process::Command::new("kill")
        .args(["-TERM", &tc4.pid().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    assert_eq!(
        tc4.next_line(Duration::from_secs(5)).as_deref(),
        Some(r#"{"closed":true}"#),
        "stderr: {}",
        tc4.stderr()
    );
    assert_eq!(
        code(tc4.wait_exit(Duration::from_secs(5)).unwrap()),
        Some(0)
    );
}

#[test]
fn one_stray_byte_on_stdin_does_not_quit() {
    let dirs = dirs();
    let mut tc4 = start(&dirs);
    tc4.send_bytes(b"\xff\xfe\n");
    tc4.send_line("not json");
    std::thread::sleep(Duration::from_millis(300));
    assert!(tc4.try_exit().is_none(), "a stray line ended tc4");
    assert_eq!(tc4.api("app/hello", "{}").status, 200);
    assert_eq!(code(tc4.quit()), Some(0));
}

#[test]
fn new_tc4_waits_for_the_old_one_then_names_the_holder() {
    let dirs = dirs();
    let mut a = start(&dirs);

    // B starts on the same app data while A runs: it waits, and prints and binds nothing.
    let mut b = Tc4::spawn(dirs.app_data.path(), &dirs.ui, &[]);
    assert_eq!(
        b.next_line(Duration::from_secs(1)),
        None,
        "B printed while A held the lock"
    );
    assert!(b.try_exit().is_none(), "B gave up at once");

    // A closes; B takes the lock and serves.
    assert_eq!(code(a.quit()), Some(0));
    let address = b.address(START);
    assert_eq!(b.api("app/hello", "{}").status, 200, "{}", address.url);

    // C finds B holding the lock for the whole wait, then names B and exits with 3.
    let mut c = Tc4::spawn(dirs.app_data.path(), &dirs.ui, &[]);
    let status = c
        .wait_exit(Duration::from_secs(40))
        .unwrap_or_else(|| panic!("C never gave up; stderr: {}", c.stderr()));
    let waited = c.since_start();
    assert_eq!(code(status), Some(3), "stderr: {}", c.stderr());
    assert!(
        waited >= Duration::from_secs(9) && waited < Duration::from_secs(25),
        "C waited {waited:?}, not about 10 s"
    );
    let lines = c.rest_of_stdout();
    assert_eq!(
        lines,
        [format!(
            r#"{{"error":{{"type":"appLockHeld","value":{{"pid":{}}}}}}}"#,
            b.pid()
        )]
    );
    assert!(
        c.stderr().contains(&format!("process {}", b.pid())),
        "stderr: {}",
        c.stderr()
    );
    assert_eq!(code(b.quit()), Some(0));
}

#[test]
fn a_tc4_that_crashed_leaves_no_wait() {
    let dirs = dirs();
    let mut a = start(&dirs);
    a.kill();
    let mut b = Tc4::spawn(dirs.app_data.path(), &dirs.ui, &[]);
    b.address(Duration::from_secs(5));
    let record = std::fs::read_to_string(dirs.app_data.path().join("app-lock.json")).unwrap();
    let record: serde_json::Value = serde_json::from_str(&record).unwrap();
    assert_eq!(record["pid"], b.pid(), "the record names the holder now");
    assert_eq!(code(b.quit()), Some(0));
}

#[test]
fn the_app_data_folder_is_created() {
    let dirs = dirs();
    let nested = dirs.app_data.path().join("a").join("b");
    let mut tc4 = Tc4::spawn(&nested, &dirs.ui, &[]);
    tc4.address(START);
    for name in [
        "app.lock",
        "app-lock.json",
        "uploads",
        "downloads",
        "resources",
    ] {
        assert!(nested.join(name).exists(), "{name}");
    }
    assert_eq!(code(tc4.quit()), Some(0));
}

#[test]
fn token_never_logged() {
    let dirs = dirs();
    let mut tc4 = start(&dirs);
    let token = tc4.address.clone().unwrap().token;
    assert_eq!(tc4.api("app/hello", "{}").status, 200);
    let wrong = tc4.req("GET", "/").with("X-Tc-Token", "wrong");
    assert_eq!(tc4.send(&wrong).status, 401);
    assert_eq!(tc4.send(&tc4.req("GET", "/nothing-here")).status, 404);
    // A debug build trades the token in the query for a cookie; the query is never logged.
    let dev = common::Req::new("GET", &format!("/?dev-token={token}")).with(
        "Host",
        &format!("127.0.0.1:{}", tc4.address.as_ref().unwrap().port),
    );
    let _ = tc4.send(&dev);
    assert_eq!(code(tc4.quit()), Some(0));
    let stderr = tc4.stderr();
    assert!(
        !stderr.contains(&token),
        "the token is in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("tc4: serving http://127.0.0.1:"),
        "stderr: {stderr}"
    );
    for line in tc4.rest_of_stdout() {
        assert!(!line.contains(&token), "{line}");
    }
}

#[test]
fn bad_arguments_exit_2() {
    let cases: [&[&str]; 8] = [
        &[],
        &["start"],
        &["serve", "--app-data", "/a", "--ui", "/u", "--nope"],
        &["serve", "--app-data"],
        &[
            "serve",
            "--app-data",
            "/a",
            "--app-data",
            "/b",
            "--ui",
            "/u",
        ],
        &[
            "serve",
            "--app-data",
            "/a",
            "--ui",
            "/u",
            "--door43",
            "ftp://x",
        ],
        &["serve", "--ui", "/u"],
        &["serve", "--app-data", "/a"],
    ];
    for args in cases {
        let (code, stdout, stderr) = run_to_end(args);
        assert_eq!(code, Some(2), "{args:?}: {stderr}");
        assert!(stderr.contains("usage:"), "{args:?}: {stderr}");
        assert_eq!(stdout, "", "{args:?}");
    }
}

#[test]
fn bad_arguments_take_no_lock_and_make_no_folder() {
    let dirs = dirs();
    let nested = dirs.app_data.path().join("never");
    let app_data = nested.display().to_string();
    let (code, _, _) = run_to_end(&["serve", "--app-data", &app_data, "--nope"]);
    assert_eq!(code, Some(2));
    assert!(!nested.exists());
}

#[test]
fn a_ui_folder_that_is_missing_is_a_start_error() {
    let dirs = dirs();
    let missing = dirs.ui_root.path().join("nonexistent");
    let mut bad = Tc4::spawn(dirs.app_data.path(), &missing, &[]);
    let status = bad.wait_exit(Duration::from_secs(20)).expect("it exits");
    assert_eq!(code(status), Some(4), "stderr: {}", bad.stderr());
    let lines = bad.rest_of_stdout();
    assert_eq!(lines.len(), 1, "{lines:?}");
    let error: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(error["error"]["type"], "start", "{error}");
    assert!(
        error["error"]["value"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("nonexistent")),
        "{error}"
    );
    // The lock went with the process: the next start does not wait for it.
    let mut good = Tc4::spawn(dirs.app_data.path(), &dirs.ui, &[]);
    good.address(Duration::from_secs(5));
    assert_eq!(code(good.quit()), Some(0));
}
