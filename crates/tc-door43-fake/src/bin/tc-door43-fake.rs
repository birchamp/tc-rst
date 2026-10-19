//! The fake Door43 as a process, for the app's browser tests and parity captures (APP-DESIGN
//! §7.1, §9.2 R13): `tc4 serve --door43 <url>` signs in to it, and the tests drive it through a
//! second loopback server, its control port.
//!
//! It holds the parity account ([`seeded::parity_account`]: `facilitator-zq`, the user of tC4's
//! own e2e fake, with its organizations and their repositories) and nothing else. It prints one line on stdout,
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
    Status { status: u16, message: String },
    Delay { ms: u64 },
    RateLimited { retry_after_secs: u64 },
    Hold,
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
