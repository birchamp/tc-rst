//! The fake as a process (`tc-door43-fake`): the parity account, the control port, and its
//! end when its stdin closes.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use base64::Engine;
use serde_json::{Value, json};
use tc_door43_fake::seeded::{
    PARITY_EMAIL, PARITY_LANGUAGE, PARITY_ORGS, PARITY_PASSWORD, PARITY_USER,
};

/// A running binary, with its own temporary folder so a test can see what it leaves.
struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
    url: String,
    control: String,
    tmp: tempfile::TempDir,
}

fn start() -> Running {
    let tmp = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_tc-door43-fake"))
        .env("TMPDIR", tmp.path())
        .env("TMP", tmp.path())
        .env("TEMP", tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdin = child.stdin.take();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let first: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("{line:?}: {e}"));
    assert!(first.get("error").is_none(), "{first}");
    Running {
        child,
        stdin,
        url: first["url"].as_str().unwrap().to_owned(),
        control: first["control"].as_str().unwrap().to_owned(),
        tmp,
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// tC4's token scopes; `write:user` holds the `read:user` that `GET /api/v1/user` needs.
const SCOPES: [&str; 3] = ["write:repository", "write:organization", "write:user"];

fn http() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

fn basic(login: &str, password: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{login}:{password}"));
    format!("Basic {encoded}")
}

async fn user_status(url: &str, authorization: &str) -> u16 {
    http()
        .get(format!("{url}/api/v1/user"))
        .header("Authorization", authorization)
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

async fn json_of(response: reqwest::Response) -> Value {
    serde_json::from_slice(&response.bytes().await.unwrap()).unwrap()
}

async fn control_get(run: &Running, path: &str) -> Value {
    json_of(
        http()
            .get(format!("{}{path}", run.control))
            .send()
            .await
            .unwrap(),
    )
    .await
}

async fn control_post(run: &Running, path: &str, body: Value) -> Value {
    let response = http()
        .post(format!("{}{path}", run.control))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200, "{path}");
    json_of(response).await
}

#[tokio::test]
async fn the_binary_serves_the_parity_account_and_its_control_port() {
    let run = start();
    let password = basic(PARITY_USER, PARITY_PASSWORD);
    let response = http()
        .get(format!("{}/api/v1/user", run.url))
        .header("Authorization", &password)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let user = json_of(response).await;
    assert_eq!(user["login"], PARITY_USER);
    assert_eq!(
        user_status(&run.url, &basic(PARITY_EMAIL, PARITY_PASSWORD)).await,
        200
    );
    assert_eq!(
        user_status(&run.url, &basic(PARITY_USER, "wrong")).await,
        401
    );

    let requests = control_get(&run, "/requests").await;
    let seen: Vec<(String, String, String)> = requests
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["method"].as_str().unwrap().to_owned(),
                r["path"].as_str().unwrap().to_owned(),
                r["auth"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let get = |auth: &str| ("GET".to_owned(), "/api/v1/user".to_owned(), auth.to_owned());
    assert_eq!(
        seen,
        vec![
            get(&format!("basic:{PARITY_USER}")),
            get(&format!("basic:{PARITY_EMAIL}")),
            get(&format!("basic:{PARITY_USER}")),
        ]
    );
    let text = requests.to_string();
    assert!(!text.contains(PARITY_PASSWORD), "{text}");

    let issued = control_post(
        &run,
        "/tokens",
        json!({"login": PARITY_USER, "name": "translationCore", "scopes": SCOPES}),
    )
    .await;
    let secret = issued["secret"].as_str().unwrap().to_owned();
    assert_eq!(user_status(&run.url, &format!("token {secret}")).await, 200);
    let back = control_get(&run, &format!("/secret/{PARITY_USER}/translationCore")).await;
    assert_eq!(back["secret"], secret.as_str());
    let none = control_get(&run, &format!("/secret/{PARITY_USER}/other-app")).await;
    assert_eq!(none["secret"], Value::Null);
    let tokens = control_get(&run, &format!("/tokens/{PARITY_USER}")).await;
    assert_eq!(tokens[0]["name"], "translationCore");
    assert_eq!(tokens[0]["scopes"], json!(SCOPES));
    assert!(!tokens.to_string().contains(&secret));

    control_post(&run, "/requests/clear", json!({})).await;
    assert_eq!(control_get(&run, "/requests").await, json!([]));
}

#[tokio::test]
async fn a_status_fault_answers_instead_and_runs_out() {
    let run = start();
    control_post(
        &run,
        "/faults",
        json!({"method": "GET", "path": "/api/v1/user", "times": 1,
               "fault": {"type": "status", "status": 503, "message": "maintenance"}}),
    )
    .await;
    let password = basic(PARITY_USER, PARITY_PASSWORD);
    assert_eq!(user_status(&run.url, &password).await, 503);
    assert_eq!(user_status(&run.url, &password).await, 200);
}

#[tokio::test]
async fn a_held_request_answers_after_its_release() {
    let run = start();
    control_post(
        &run,
        "/faults",
        json!({"method": "GET", "path": "/api/v1/user", "times": 1, "fault": {"type": "hold"}}),
    )
    .await;
    let url = run.url.clone();
    let held =
        tokio::spawn(async move { user_status(&url, &basic(PARITY_USER, PARITY_PASSWORD)).await });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!held.is_finished(), "it waits for its release");
    assert_eq!(
        control_get(&run, "/requests")
            .await
            .as_array()
            .unwrap()
            .len(),
        1,
        "it was recorded before it was held"
    );
    control_post(&run, "/release", json!({})).await;
    assert_eq!(held.await.unwrap(), 200);
}

#[test]
fn the_binary_ends_when_its_stdin_closes() {
    let mut run = start();
    let entries = |dir: &Path| std::fs::read_dir(dir).unwrap().count();
    assert_eq!(entries(run.tmp.path()), 1, "the repositories' folder");
    drop(run.stdin.take());
    let started = Instant::now();
    let status = loop {
        if let Some(status) = run.child.try_wait().unwrap() {
            break status;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "it did not end within 5 s"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(status.success(), "{status:?}");
    assert_eq!(entries(run.tmp.path()), 0, "its temporary folder is gone");
}

#[test]
fn a_quit_line_ends_it_too() {
    let mut run = start();
    let mut stdin = run.stdin.take().unwrap();
    writeln!(stdin, "{{\"quit\":true}}").unwrap();
    let started = Instant::now();
    while run.child.try_wait().unwrap().is_none() {
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(stdin);
}

/// The parity captures sign in to this fake in the rebuild and to tC4's own e2e fake in tC4:
/// the same person on both sides.
#[test]
fn the_parity_account_is_the_baselines_fake_user() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../parity/tc4-baseline/mock-server/fake-door43.mjs");
    let text = std::fs::read_to_string(path).unwrap();
    let user = format!(
        "{{ username: '{PARITY_USER}', password: '{PARITY_PASSWORD}', email: '{PARITY_EMAIL}' }}"
    );
    assert!(text.contains(&user), "FAKE_USER is not {user}");
    for (name, full_name, can_create, repositories) in PARITY_ORGS {
        let org = format!(
            "{{ username: '{name}', fullName: '{full_name}', canCreateRepository: {can_create}, repositories: {{ '{PARITY_LANGUAGE}': {repositories} }} }}"
        );
        assert!(text.contains(&org), "FAKE_ORGANIZATIONS lacks {org}");
    }
    assert_eq!(
        text.matches("canCreateRepository:").count(),
        PARITY_ORGS.len(),
        "the same number of organizations"
    );
}

/// E1a step 5: the control port takes `breakOff`, and a git reply it names stops part way,
/// as a dropped connection does (R13).
#[tokio::test]
async fn a_break_off_fault_cuts_a_git_reply() {
    let run = start();
    let path = "/Equipo-Ejemplo/es-419_project_1.git/info/refs";
    let url = format!("{}{path}?service=git-upload-pack", run.url);
    let whole = http()
        .get(&url)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert!(!whole.is_empty(), "the advertisement has bytes");
    control_post(
        &run,
        "/faults",
        json!({"method": "GET", "path": path, "times": 1,
               "fault": {"type": "breakOff", "after": 0}}),
    )
    .await;
    let cut = match http().get(&url).send().await {
        Ok(response) => response.bytes().await.map(|b| b.len()).unwrap_or(0),
        Err(_) => 0,
    };
    assert!(
        cut < whole.len(),
        "the reply stopped part way: {cut} of {}",
        whole.len()
    );
}

/// The releases of `owner`'s repositories, as the catalog search lists them: `name@tag` for
/// each published release.
async fn catalog(url: &str, owner: &str) -> Vec<String> {
    let found: Value = http()
        .get(format!("{url}/api/v1/catalog/search?owner={owner}"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .map(|body| serde_json::from_str(&body).unwrap())
        .unwrap();
    let mut out: Vec<String> = found["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            format!(
                "{}@{}",
                entry["name"].as_str().unwrap(),
                entry["release"]["tag_name"].as_str().unwrap_or("")
            )
        })
        .collect();
    out.sort();
    out
}

/// FIX-J: the fake the browser tests start publishes unfoldingWord's resource releases, the
/// same ones the Rust tests' seeded fake holds, so a download or an upgrade through the screens
/// has something to fetch and both kinds of test agree on what exists.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_seeded_fake_publishes_the_en_pins_versions() {
    let running = start();
    let seeded = tc_door43_fake::FakeDoor43::start_seeded().await.unwrap();
    let served = catalog(&running.url, "unfoldingWord").await;
    assert!(
        !served.is_empty(),
        "the binary publishes unfoldingWord's resources"
    );
    assert_eq!(served, catalog(seeded.url(), "unfoldingWord").await);
    assert!(
        served.iter().any(|r| r.starts_with("en_tn@")),
        "the notes the sample pins are there: {served:?}"
    );
}
