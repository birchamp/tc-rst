//! The UI's files (APP-DESIGN §5.3), read from the folder `--ui` names.
//!
//! For the unit that writes `index.html` (A4a): every origin is new at each launch, because
//! the port is, so `immutable` helps only within one launch; everything under `/ui/` must
//! still carry a content hash in its name; and `index.html` cannot hold an inline script,
//! because its CSP allows none.

use std::path::{Component, Path as FsPath, PathBuf};
use std::sync::Arc;

use crate::backend::Backend;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::state::Shared;

/// The Content-Security-Policy of §5.2 for `index.html`.
pub const CSP: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' \
                       'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'; frame-src \
                       'self'; object-src 'none'; base-uri 'none'";

const IMMUTABLE: &str = "public, max-age=31536000, immutable";

pub(crate) async fn index(State(shared): State<Arc<Shared>>) -> Response {
    let mut response = serve(&shared.ui_dir.join("index.html"), "no-store").await;
    if response.status() == StatusCode::OK {
        response.headers_mut().insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CSP),
        );
    }
    response
}

pub(crate) async fn prepare(State(shared): State<Arc<Shared>>) -> Response {
    serve(&shared.ui_dir.join("prepare.html"), "no-store").await
}

pub(crate) async fn ui(State(shared): State<Arc<Shared>>, Path(path): Path<String>) -> Response {
    match under(&shared.ui_dir.join("ui"), &path) {
        Some(file) => serve(&file, IMMUTABLE).await,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Help pages never redirect: tower-http's `ServeDir` answers a directory with one, which
/// would send the help window to a URL the page-swap shim does not know (DELTA §3.15.8).
pub(crate) async fn help(State(shared): State<Arc<Shared>>, Path(path): Path<String>) -> Response {
    match under(&shared.ui_dir.join("help"), &path) {
        Some(file) => serve(&file, "no-cache").await,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// `path` below `root`, or `None` for a path that could leave it. An empty segment, `.`,
/// `..`, a backslash, a colon (a drive or a stream name on Windows) or a NUL refuses the
/// whole path.
fn under(root: &FsPath, path: &str) -> Option<PathBuf> {
    let mut file = root.to_path_buf();
    for segment in path.split('/') {
        let refused = segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.contains(['\\', ':', '\0']);
        if refused {
            return None;
        }
        file.push(segment);
    }
    // Belt and braces: whatever the segments said, only plain names were pushed.
    file.components()
        .all(|part| !matches!(part, Component::ParentDir))
        .then_some(file)
}

async fn serve(file: &FsPath, cache_control: &'static str) -> Response {
    // Only a regular file: a directory is a 404, never a redirect or a listing.
    match tokio::fs::metadata(file).await {
        Ok(meta) if meta.is_file() => {}
        _ => return StatusCode::NOT_FOUND.into_response(),
    }
    let Ok(bytes) = tokio::fs::read(file).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type(file)),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );
    response
}

fn content_type(file: &FsPath) -> &'static str {
    let extension = file
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        // `instantiateStreaming` refuses any other type.
        Some("wasm") => "application/wasm",
        Some("json") => "application/json",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// `GET /media/obs/{pack}/{path}`: a picture of an installed pack (§5.3). The backend decides
/// what it serves; the answer is cached by the page for the launch, since a pack's id names
/// its commit.
pub(crate) async fn obs_picture<B: Backend>(
    State(state): State<AppState<B>>,
    Path((pack, path)): Path<(String, String)>,
) -> Response {
    match state.backend.obs_picture(&pack, &path).await {
        Some(file) => {
            let mut response = file.bytes.into_response();
            let headers = response.headers_mut();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static(file.content_type),
            );
            headers.insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, max-age=3600"),
            );
            response
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
