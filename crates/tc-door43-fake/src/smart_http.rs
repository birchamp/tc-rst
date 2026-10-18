//! Git smart HTTP through `git http-backend`, run as a CGI program per request.

use crate::Shared;
use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use std::path::Path;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;

pub(crate) struct CgiRequest<'a> {
    pub method: &'a str,
    /// `/<owner>/<repo>.git/<service path>`, lowercased like the folders on disk.
    pub path_info: String,
    pub query: &'a str,
    pub headers: &'a HeaderMap,
    /// Set after a successful sign-in; `git http-backend` refuses a push without it.
    pub remote_user: Option<&'a str>,
    pub body: Vec<u8>,
}

pub(crate) async fn run(shared: &Shared, request: CgiRequest<'_>) -> Response {
    match run_backend(shared, request).await {
        Ok(response) => response,
        Err(message) => (StatusCode::INTERNAL_SERVER_ERROR, message).into_response(),
    }
}

async fn run_backend(shared: &Shared, request: CgiRequest<'_>) -> Result<Response, String> {
    let header = |name: &str| {
        request
            .headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    };
    let mut command = shared.git.command(None);
    command
        .arg("http-backend")
        .env("GIT_PROJECT_ROOT", &shared.git.root)
        .env("GIT_HTTP_EXPORT_ALL", "1")
        .env("PATH_INFO", &request.path_info)
        .env("REQUEST_METHOD", request.method)
        .env("QUERY_STRING", request.query)
        .env("CONTENT_TYPE", header("content-type"))
        .env("CONTENT_LENGTH", request.body.len().to_string())
        .env("REMOTE_ADDR", "127.0.0.1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let protocol = header("git-protocol");
    if !protocol.is_empty() {
        command.env("GIT_PROTOCOL", protocol);
    }
    let encoding = header("content-encoding");
    if !encoding.is_empty() {
        command.env("HTTP_CONTENT_ENCODING", encoding);
    }
    if let Some(user) = request.remote_user {
        command.env("REMOTE_USER", user);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("git http-backend: {e}"))?;
    let stdin = child.stdin.take();
    let body = request.body;
    let feeder = tokio::spawn(async move {
        if let Some(mut stdin) = stdin {
            let _ = stdin.write_all(&body).await;
        }
    });
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("git http-backend: {e}"))?;
    let _ = feeder.await;
    if !output.status.success() && output.stdout.is_empty() {
        return Err(format!(
            "git http-backend failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    parse_cgi(&output.stdout)
}

/// Split CGI output into headers and body. A `Status:` header sets the status.
fn parse_cgi(output: &[u8]) -> Result<Response, String> {
    let (head, body) = split_head(output).ok_or("git http-backend wrote no header block")?;
    let mut status = StatusCode::OK;
    let mut headers = HeaderMap::new();
    for line in String::from_utf8_lossy(head).lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("status") {
            let code = value.split_whitespace().next().unwrap_or("500");
            status = code
                .parse::<u16>()
                .ok()
                .and_then(|c| StatusCode::from_u16(c).ok())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        } else if let (Ok(name), Ok(value)) = (
            axum::http::HeaderName::from_bytes(name.trim().as_bytes()),
            HeaderValue::from_str(value),
        ) {
            headers.append(name, value);
        }
    }
    let mut response = Response::new(Body::from(body.to_vec()));
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    Ok(response)
}

fn split_head(output: &[u8]) -> Option<(&[u8], &[u8])> {
    let crlf = output
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| (i, 4));
    let lf = output.windows(2).position(|w| w == b"\n\n").map(|i| (i, 2));
    let (at, len) = match (crlf, lf) {
        (Some(a), Some(b)) => {
            if a.0 <= b.0 {
                a
            } else {
                b
            }
        }
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => return None,
    };
    Some((&output[..at], &output[at + len..]))
}

/// The service a smart HTTP request asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Service {
    UploadPack,
    ReceivePack,
}

impl Service {
    pub fn of(path_tail: &str, query: &str) -> Option<Self> {
        let named = |name: &str| match name {
            "git-upload-pack" => Some(Service::UploadPack),
            "git-receive-pack" => Some(Service::ReceivePack),
            _ => None,
        };
        if path_tail == "info/refs" {
            query
                .split('&')
                .find_map(|pair| pair.strip_prefix("service="))
                .and_then(named)
        } else {
            named(path_tail)
        }
    }
}

/// Door43's answer to `POST git-receive-pack` from an account without write access, as
/// measured on 2026-10-18 (docs/evidence/door43-live-2026-10-17.md, step 6): HTTP 200, the
/// text of Gitea's pre-receive hook on side band 2, then a status report that unpacks the
/// pack and refuses every ref. Nothing is written.
pub(crate) fn declined(request: &[u8]) -> Response {
    let (refs, capabilities) = commands(request);
    let wants = |name: &str| capabilities.split(' ').any(|c| c == name);
    let mut report = Vec::new();
    if wants("report-status") || wants("report-status-v2") {
        report.extend(pkt(b"unpack ok\n"));
        for name in &refs {
            report.extend(pkt(
                format!("ng {name} pre-receive hook declined\n").as_bytes()
            ));
        }
        report.extend(b"0000");
    }
    let body = if wants("side-band-64k") || wants("side-band") {
        let mut body = Vec::new();
        for line in [
            "error:\n",
            "error: User permission denied for writing.\n",
            "error:\n",
        ] {
            body.extend(pkt(&[b"\x02", line.as_bytes()].concat()));
        }
        for chunk in report.chunks(990) {
            body.extend(pkt(&[b"\x01", chunk].concat()));
        }
        body.extend(b"0000");
        body
    } else {
        report
    };
    let mut response = Response::new(Body::from(body));
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-git-receive-pack-result"),
    );
    response
}

/// The refs a receive-pack request updates, and the capabilities its first command asks for.
fn commands(request: &[u8]) -> (Vec<String>, String) {
    let mut refs = Vec::new();
    let mut capabilities = String::new();
    let mut rest = request;
    while let Some(len) = rest
        .get(..4)
        .and_then(|h| std::str::from_utf8(h).ok())
        .and_then(|h| usize::from_str_radix(h, 16).ok())
    {
        if len < 4 || len > rest.len() {
            break;
        }
        let line = String::from_utf8_lossy(&rest[4..len]);
        rest = &rest[len..];
        let (command, caps) = line.split_once('\0').unwrap_or((&line, ""));
        if refs.is_empty() && !caps.is_empty() {
            capabilities = caps.trim_end().to_owned();
        }
        if let [_, _, name] = command.split_whitespace().collect::<Vec<_>>()[..] {
            refs.push(name.to_owned());
        }
    }
    (refs, capabilities)
}

fn pkt(payload: &[u8]) -> Vec<u8> {
    [format!("{:04x}", payload.len() + 4).as_bytes(), payload].concat()
}

pub(crate) fn exists(dir: &Path) -> bool {
    dir.join("HEAD").is_file()
}
