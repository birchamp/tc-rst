//! The fake Door43 as a process, for the app's browser tests and parity captures (APP-DESIGN
//! §7.1, §9.2 R13): `tc4 serve --door43 <url>` signs in to it, and the tests drive it through a
//! second loopback server, its control port.
//!
//! It holds the parity account ([`seeded::parity_account`]: `facilitator-zq`, the user of tC4's
//! own e2e fake, with its organizations and their repositories) and unfoldingWord's resource
//! releases ([`seeded::unfolding_word_resources`], the ones the Rust tests' seeded fake holds). It prints one line on stdout,
//! `{"url": "http://127.0.0.1:<port>", "control": "http://127.0.0.1:<port>"}`, and serves until
//! its stdin closes or reads the line `{"quit":true}`; then its repositories' temporary folder
//! goes and it exits with 0. A start that fails prints `{"error": "<reason>"}` and exits with 1.
//!
//! The control port:
//!
//! | Route | Body | Answer |
//! |---|---|---|
//! | `GET /requests` | — | every request so far: `[{"method", "path", "query", "auth"}]`, `auth` being `none`, `basic:<login>`, `token` or `other` (never a secret) |
//! | `POST /requests/clear` | — | `{}` |
//! | `POST /faults` | `{"method": "GET" or null, "path": <prefix>, "times": n or null, "fault": {"type": "status", "status", "message"} or {"type": "delay", "ms"} or {"type": "rateLimited", "retryAfterSecs"} or {"type": "hold"}}` | `{}` |
//! | `POST /release` | — | `{}`: the oldest held request goes on |
//! | `POST /tokens` | `{"login", "name", "scopes": [...]}` | `{"secret"}`: a token as if minted earlier |
//! | `GET /tokens/{login}` | — | `[{"id", "name", "scopes"}]` |
//! | `GET /secret/{login}/{name}` | — | `{"secret": <text> or null}` |

use std::io::{BufRead, Write};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use serde_json::{Value, json};
use tc_door43_fake::{AuthSeen, FakeDoor43, Fault, FaultRule, seeded};
use tokio::sync::oneshot;

type Fake = State<Arc<FakeDoor43>>;

fn main() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => return failed(&error.to_string()),
    };
    runtime.block_on(run())
}

fn say(line: &Value) -> std::io::Result<()> {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{line}")?;
    out.flush()
}

fn failed(reason: &str) -> ExitCode {
    let _ = say(&json!({ "error": reason }));
    ExitCode::from(1)
}

async fn run() -> ExitCode {
    let fake = match FakeDoor43::start().await {
        Ok(fake) => Arc::new(fake),
        Err(error) => return failed(&error.to_string()),
    };
    if let Err(error) = seeded::parity_account(&fake).await {
        return failed(&error.to_string());
    }
    if let Err(error) = seeded::unfolding_word_resources(&fake).await {
        return failed(&error.to_string());
    }
    if let Err(error) = seeded::tc3_original_languages(&fake).await {
        return failed(&error.to_string());
    }
    let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(error) => return failed(&error.to_string()),
    };
    let control = match listener.local_addr() {
        Ok(addr) => format!("http://{addr}"),
        Err(error) => return failed(&error.to_string()),
    };
    let app = control_router(fake.clone());
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    // tokio's stdin needs a feature this crate does not take; a thread reads it instead.
    let (quit, quitting) = oneshot::channel::<()>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            match line {
                Ok(line) if is_quit(&line) => break,
                Ok(_) => {}
                Err(_) => break,
            }
        }
        let _ = quit.send(());
    });

    if say(&json!({ "url": fake.url(), "control": control })).is_err() {
        return ExitCode::from(1);
    }
    let _ = quitting.await;
    // The fake's server stops and its temporary folder goes with the last reference; the
    // control server's tasks end with the runtime.
    drop(fake);
    ExitCode::SUCCESS
}

fn is_quit(line: &str) -> bool {
    serde_json::from_str::<Value>(line).is_ok_and(|v| v["quit"] == Value::Bool(true))
}

fn control_router(fake: Arc<FakeDoor43>) -> Router {
    Router::new()
        .route("/requests", get(requests))
        .route("/requests/clear", post(clear_requests))
        .route("/faults", post(add_fault))
        .route("/release", post(release))
        .route("/tokens", post(issue_token))
        .route("/repos", post(add_repo))
        .route("/users", post(add_user))
        .route("/branch", post(branch))
        .route("/tokens/{login}", get(tokens_of))
        .route("/secret/{login}/{name}", get(secret))
        .with_state(fake)
}

async fn requests(State(fake): Fake) -> Response {
    let all: Vec<Value> = fake
        .requests()
        .into_iter()
        .map(|r| {
            let auth = match r.authorization {
                AuthSeen::None => "none".to_owned(),
                AuthSeen::Basic { login } => format!("basic:{login}"),
                AuthSeen::Token => "token".to_owned(),
                AuthSeen::Other => "other".to_owned(),
            };
            json!({ "method": r.method, "path": r.path, "query": r.query, "auth": auth })
        })
        .collect();
    axum::Json(Value::Array(all)).into_response()
}

async fn clear_requests(State(fake): Fake) -> Response {
    fake.clear_requests();
    axum::Json(json!({})).into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FaultBody {
    method: Option<String>,
    path: String,
    times: Option<usize>,
    fault: FaultKind,
}

#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum FaultKind {
    Status {
        status: u16,
        message: String,
    },
    Delay {
        ms: u64,
    },
    RateLimited {
        retry_after_secs: u64,
    },
    Hold,
    /// A download or a git reply sends `after` bytes, then the connection breaks (R13).
    BreakOff {
        after: usize,
    },
}

fn bad(reason: String) -> Response {
    (
        StatusCode::BAD_REQUEST,
        axum::Json(json!({ "error": reason })),
    )
        .into_response()
}

async fn add_fault(State(fake): Fake, body: axum::body::Bytes) -> Response {
    let body: FaultBody = match serde_json::from_slice(&body) {
        Ok(body) => body,
        Err(error) => return bad(error.to_string()),
    };
    let fault = match body.fault {
        FaultKind::Status { status, message } => Fault::Status { status, message },
        FaultKind::Delay { ms } => Fault::Delay(Duration::from_millis(ms)),
        FaultKind::RateLimited { retry_after_secs } => Fault::RateLimited { retry_after_secs },
        FaultKind::Hold => Fault::Hold,
        FaultKind::BreakOff { after } => Fault::BreakOff { after },
    };
    fake.inject(FaultRule {
        method: body.method,
        path_prefix: body.path,
        times: body.times,
        fault,
    });
    axum::Json(json!({})).into_response()
}

async fn release(State(fake): Fake) -> Response {
    fake.release_held();
    axum::Json(json!({})).into_response()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TokenBody {
    login: String,
    name: String,
    scopes: Vec<String>,
}

async fn issue_token(State(fake): Fake, body: axum::body::Bytes) -> Response {
    let body: TokenBody = match serde_json::from_slice(&body) {
        Ok(body) => body,
        Err(error) => return bad(error.to_string()),
    };
    let scopes: Vec<&str> = body.scopes.iter().map(String::as_str).collect();
    match fake.issue_token(&body.login, &body.name, &scopes) {
        Ok(secret) => axum::Json(json!({ "secret": secret })).into_response(),
        Err(error) => bad(error.to_string()),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BranchBody {
    owner: String,
    name: String,
    branch: String,
    /// Moves the branch here when given; the answer is the branch's commit either way.
    sha: Option<String>,
}

/// A branch's commit, read or moved (E1a: Door43 loses the newest pushes).
async fn branch(State(fake): Fake, body: axum::body::Bytes) -> Response {
    let body: BranchBody = match serde_json::from_slice(&body) {
        Ok(body) => body,
        Err(error) => return bad(error.to_string()),
    };
    if let Some(sha) = &body.sha
        && let Err(error) = fake
            .set_branch(&body.owner, &body.name, &body.branch, sha)
            .await
    {
        return bad(error.to_string());
    }
    match fake.resolve(&body.owner, &body.name, &body.branch).await {
        Ok(sha) => axum::Json(json!({ "sha": sha })).into_response(),
        Err(error) => bad(error.to_string()),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UserBody {
    login: String,
    password: String,
    org: String,
}

/// A teammate: another user, a member of `org` who may push there (E1a step 6).
async fn add_user(State(fake): Fake, body: axum::body::Bytes) -> Response {
    let body: UserBody = match serde_json::from_slice(&body) {
        Ok(body) => body,
        Err(error) => return bad(error.to_string()),
    };
    fake.add_user(tc_door43_fake::FakeUser {
        email: format!("{}@example.org", body.login),
        login: body.login.clone(),
        password: body.password,
        one_time_code: None,
    });
    if fake.add_member(&body.org, &body.login) {
        axum::Json(json!({})).into_response()
    } else {
        bad(format!("no organization {}", body.org))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RepoBody {
    owner: String,
    name: String,
    /// Remove a repository of that name first, so it starts empty again.
    #[serde(default)]
    replace: bool,
}

/// An empty repository of `owner`, which a first sync fills (E1a: journeys that sync for real).
async fn add_repo(State(fake): Fake, body: axum::body::Bytes) -> Response {
    let body: RepoBody = match serde_json::from_slice(&body) {
        Ok(body) => body,
        Err(error) => return bad(error.to_string()),
    };
    if body.replace
        && let Err(error) = fake.remove_repo(&body.owner, &body.name)
    {
        return bad(error.to_string());
    }
    let seed = tc_door43_fake::RepoSeed {
        owner: body.owner,
        name: body.name,
        subject: "Bible".to_owned(),
        language: "es-419".to_owned(),
        topics: Vec::new(),
        private: false,
        commits: Vec::new(),
    };
    match fake.add_repo(seed).await {
        Ok(_) => axum::Json(json!({})).into_response(),
        Err(error) => bad(error.to_string()),
    }
}

async fn tokens_of(State(fake): Fake, Path(login): Path<String>) -> Response {
    let tokens: Vec<Value> = fake
        .tokens_of(&login)
        .into_iter()
        .map(|t| json!({ "id": t.id, "name": t.name, "scopes": t.scopes }))
        .collect();
    axum::Json(Value::Array(tokens)).into_response()
}

async fn secret(State(fake): Fake, Path((login, name)): Path<(String, String)>) -> Response {
    axum::Json(json!({ "secret": fake.token_secret(&login, &name) })).into_response()
}
