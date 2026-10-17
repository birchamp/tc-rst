//! The stand-in's HTTP side: the security layer of §5.2, the routes the probes use, and a log
//! of which requests arrived with the token.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::fs::File;
use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{Path as UrlPath, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::content;

/// How many requests the probe log keeps.
const LOG_CAPACITY: usize = 20_000;

/// The Content-Security-Policy of §5.2 for `index.html`.
const CSP: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' \
                   'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'; frame-src \
                   'self'; object-src 'none'; base-uri 'none'";

/// A fresh per-launch token: 32 random bytes, base64url without padding (43 characters).
pub fn new_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| format!("no randomness: {e}"))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

/// Whether a request carried the launch's token.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TokenSeen {
    Ok,
    Missing,
    Wrong,
}

/// One request as the probe log keeps it. The token itself is never kept.
#[derive(Clone, Debug, Serialize)]
pub struct Hit {
    at: u64,
    method: String,
    path: String,
    probe: Option<String>,
    token: TokenSeen,
    status: u16,
}

pub struct Shared {
    token: String,
    host: String,
    origin: String,
    port: u16,
    instance: String,
    started: u64,
    reply_bytes: usize,
    clicks: AtomicU64,
    hits: Mutex<VecDeque<Hit>>,
    log: Mutex<File>,
    typed: Mutex<File>,
}

impl Shared {
    pub fn new(
        app_data: &Path,
        port: u16,
        token: String,
        reply_bytes: usize,
    ) -> Result<Self, String> {
        let open = |name: &str| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(app_data.join(name))
                .map_err(|e| format!("cannot open {name}: {e}"))
        };
        let mut instance = [0u8; 6];
        getrandom::fill(&mut instance).map_err(|e| format!("no randomness: {e}"))?;
        Ok(Self {
            token,
            host: format!("127.0.0.1:{port}"),
            origin: format!("http://127.0.0.1:{port}"),
            port,
            instance: instance.iter().map(|b| format!("{b:02x}")).collect(),
            started: now_ms(),
            reply_bytes,
            clicks: AtomicU64::new(0),
            hits: Mutex::new(VecDeque::with_capacity(LOG_CAPACITY)),
            log: Mutex::new(open("standin.log")?),
            typed: Mutex::new(open("typed.log")?),
        })
    }

    /// The one line `tc4 serve` prints first (§5.1).
    pub fn address_line(&self) -> String {
        serde_json::json!({ "url": format!("{}/", self.origin), "token": self.token }).to_string()
    }

    /// Appends one event to `standin.log` in app data and syncs it, so a later reader sees it
    /// even if this process is ended right after.
    pub fn log_event(&self, event: &str, detail: serde_json::Value) {
        let mut line = serde_json::json!({
            "t": now_ms(),
            "ev": event,
            "pid": std::process::id(),
            "instance": self.instance,
        });
        if let (Some(line), serde_json::Value::Object(detail)) = (line.as_object_mut(), detail) {
            line.extend(detail);
        }
        if let Ok(mut file) = self.log.lock() {
            let _ = writeln!(file, "{line}");
            let _ = file.sync_data();
        }
    }

    fn record(&self, hit: Hit) {
        if let Ok(mut hits) = self.hits.lock() {
            if hits.len() == LOG_CAPACITY {
                hits.pop_front();
            }
            hits.push_back(hit);
        }
    }

    fn token_seen(&self, headers: &HeaderMap) -> TokenSeen {
        match headers.get("x-tc-token") {
            None => TokenSeen::Missing,
            Some(value) if constant_time_eq(value.as_bytes(), self.token.as_bytes()) => {
                TokenSeen::Ok
            }
            Some(_) => TokenSeen::Wrong,
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn probe_of(query: Option<&str>) -> Option<String> {
    query?
        .split('&')
        .find_map(|pair| pair.strip_prefix("probe="))
        .map(str::to_owned)
}

pub fn router(shared: Arc<Shared>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/prepare.html", get(prepare))
        .route("/ui/{*path}", get(ui))
        .route("/help/{*path}", get(help))
        .route("/media/{*path}", get(media))
        .route("/download/{id}/{name}", get(download))
        .route("/print/{job}", get(print))
        .route("/api/v1/align/click", post(click))
        .route("/api/v1/events", get(events))
        .route("/api/v1/spike/hello", post(hello))
        .route("/api/v1/spike/log", post(probe_log))
        .route("/api/v1/spike/typed", post(typed))
        .layer(middleware::from_fn_with_state(Arc::clone(&shared), guard))
        .with_state(shared)
}

/// The security layer of §5.2, in its order: `Host`, the token, `Origin` on anything but
/// GET, `X-Tc-Api` on `/api/`. No CORS header is ever sent.
async fn guard(State(shared): State<Arc<Shared>>, request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let probe = probe_of(request.uri().query());
    let headers = request.headers();
    let token = shared.token_seen(headers);
    let host_ok =
        headers.get(header::HOST).map(HeaderValue::as_bytes) == Some(shared.host.as_bytes());
    let origin_ok = method == Method::GET
        || method == Method::HEAD
        || headers.get(header::ORIGIN).map(HeaderValue::as_bytes) == Some(shared.origin.as_bytes());
    let api_ok = !path.starts_with("/api/")
        || headers.get("x-tc-api").map(HeaderValue::as_bytes) == Some(b"1".as_slice());
    let response = if !host_ok {
        (StatusCode::FORBIDDEN, "foreign host").into_response()
    } else if token != TokenSeen::Ok {
        (StatusCode::UNAUTHORIZED, "no token").into_response()
    } else if !origin_ok {
        (StatusCode::FORBIDDEN, "foreign origin").into_response()
    } else if !api_ok {
        (StatusCode::CONFLICT, "api version").into_response()
    } else {
        next.run(request).await
    };
    shared.record(Hit {
        at: now_ms(),
        method: method.to_string(),
        path,
        probe,
        token,
        status: response.status().as_u16(),
    });
    response
}

fn with_type(content_type: &'static str, body: impl Into<Body>) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body.into(),
    )
        .into_response()
}

async fn index() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
            (header::CONTENT_SECURITY_POLICY, CSP),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        content::INDEX_HTML,
    )
        .into_response()
}

async fn prepare() -> Response {
    with_type("text/html; charset=utf-8", content::PREPARE_HTML)
}

async fn ui(UrlPath(path): UrlPath<String>) -> Response {
    match path.as_str() {
        "probe.js" => with_type("text/javascript; charset=utf-8", content::PROBE_JS),
        "probe.css" => with_type("text/css; charset=utf-8", content::PROBE_CSS),
        "fonts/probe.woff2" => with_type("font/woff2", content::PROBE_FONT),
        "fonts/awami.ttf" => with_type("font/ttf", content::AWAMI_FONT),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn help(UrlPath(path): UrlPath<String>) -> Response {
    match path.as_str() {
        "page.html" => with_type("text/html; charset=utf-8", content::HELP_HTML),
        "child.html" => with_type("text/html; charset=utf-8", content::CHILD_HTML),
        "help.css" => with_type("text/css; charset=utf-8", content::HELP_CSS),
        p if p.starts_with("img/") && p.ends_with(".png") => {
            with_type("image/png", content::picture())
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn media(UrlPath(path): UrlPath<String>) -> Response {
    if path.ends_with(".png") {
        with_type("image/png", content::picture())
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

async fn download(UrlPath((id, name)): UrlPath<(String, String)>) -> Response {
    let safe: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8".to_owned()),
            (header::CACHE_CONTROL, "no-store".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{safe}\""),
            ),
        ],
        content::download_body(&id),
    )
        .into_response()
}

async fn print(UrlPath(_job): UrlPath<String>) -> Response {
    with_type("text/html; charset=utf-8", content::PRINT_HTML)
}

#[derive(Deserialize, Default)]
struct ClickBody {
    #[serde(default)]
    word: u64,
}

async fn click(State(shared): State<Arc<Shared>>, body: Bytes) -> Response {
    let word = serde_json::from_slice::<ClickBody>(&body)
        .unwrap_or_default()
        .word;
    let seq = shared.clicks.fetch_add(1, Ordering::Relaxed);
    with_type(
        "application/json",
        content::click_reply(seq, word, shared.reply_bytes),
    )
}

#[derive(Deserialize)]
struct EventsQuery {
    every: Option<u64>,
}

/// A server-sent-events stream: a frame at once, then one every `every` ms (100 by default).
async fn events(Query(query): Query<EventsQuery>) -> Response {
    let every = Duration::from_millis(query.every.unwrap_or(100).clamp(10, 60_000));
    let start = tokio::time::Instant::now();
    let frames = futures_util::stream::unfold(0u64, move |seq| async move {
        let due = start + every * u32::try_from(seq).unwrap_or(u32::MAX);
        tokio::time::sleep_until(due).await;
        let frame = format!(
            "id: {seq}\ndata: {{\"seq\":{seq},\"kind\":\"progress\",\"at\":{}}}\n\n",
            now_ms()
        );
        Some((Ok::<_, Infallible>(Bytes::from(frame)), seq + 1))
    });
    (
        [
            (header::CONTENT_TYPE, "text/event-stream"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from_stream(frames),
    )
        .into_response()
}

async fn hello(State(shared): State<Arc<Shared>>) -> Response {
    let body = serde_json::json!({
        "instance": shared.instance,
        "pid": std::process::id(),
        "port": shared.port,
        "started": shared.started,
    });
    with_type("application/json", body.to_string())
}

#[derive(Deserialize)]
struct LogBody {
    probe: String,
}

/// The requests a probe made, as the server saw them.
async fn probe_log(State(shared): State<Arc<Shared>>, body: Bytes) -> Response {
    let Ok(LogBody { probe }) = serde_json::from_slice::<LogBody>(&body) else {
        return (StatusCode::BAD_REQUEST, "a probe id is needed").into_response();
    };
    let hits: Vec<Hit> = shared.hits.lock().map_or_else(
        |_| Vec::new(),
        |hits| {
            hits.iter()
                .filter(|hit| hit.probe.as_deref() == Some(probe.as_str()))
                .cloned()
                .collect()
        },
    );
    with_type(
        "application/json",
        serde_json::json!({ "hits": hits }).to_string(),
    )
}

#[derive(Deserialize, Serialize)]
struct TypedBody {
    field: String,
    n: u64,
    at: u64,
}

/// The typing probe's witness: every keystroke the page reports, one line each, so the count
/// survives the shell's death even when this process ends with it.
async fn typed(State(shared): State<Arc<Shared>>, body: Bytes) -> Response {
    let Ok(typed) = serde_json::from_slice::<TypedBody>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if let Ok(mut file) = shared.typed.lock() {
        let line = serde_json::json!({ "t": now_ms(), "field": typed.field, "n": typed.n, "at": typed.at });
        let _ = writeln!(file, "{line}");
    }
    StatusCode::NO_CONTENT.into_response()
}
