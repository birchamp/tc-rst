//! The API route: `POST /api/v1/{service}/{name}` for exactly the rows of tc-api's tables
//! (APP-DESIGN §3.3, §5.3).

use std::collections::HashSet;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::Ordering;

use axum::body::{Body, Bytes};
use axum::extract::{Path, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::FutureExt as _;
use futures_util::StreamExt as _;
use tc_api::{ApiError, Reply};

use crate::backend::{Answer, Backend, Then};
use crate::state::AppState;

macro_rules! paths {
    ($( $kind:ident $([$net:ident])? $path:literal : $req:path => $ok:ty; )*) => {
        &[$($path),*]
    };
}

/// The product endpoints, in table order.
pub fn product_paths() -> &'static [&'static str] {
    tc_api::endpoint_table!(paths)
}

/// The `test` endpoints, in table order.
#[cfg(feature = "test-hooks")]
pub fn test_paths() -> &'static [&'static str] {
    tc_api::test_hook_table!(paths)
}

/// Which paths the server hands to the backend.
pub(crate) struct RouteTable {
    product: HashSet<&'static str>,
    #[cfg(feature = "test-hooks")]
    test: HashSet<&'static str>,
}

impl RouteTable {
    pub(crate) fn new() -> Self {
        Self {
            product: product_paths().iter().copied().collect(),
            #[cfg(feature = "test-hooks")]
            test: test_paths().iter().copied().collect(),
        }
    }

    /// The table's own copy of `path`, which the backend takes as `&'static str`.
    pub(crate) fn find(&self, path: &str, test_hooks: bool) -> Option<&'static str> {
        if let Some(found) = self.product.get(path) {
            return Some(*found);
        }
        #[cfg(feature = "test-hooks")]
        if test_hooks {
            return self.test.get(path).copied();
        }
        #[cfg(not(feature = "test-hooks"))]
        let _ = test_hooks;
        None
    }
}

pub(crate) fn json_response(status: StatusCode, body: Vec<u8>) -> Response {
    let mut response = (status, Body::from(body)).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// A reply that carries an error and no value, in the wire form of any endpoint's reply.
pub(crate) fn error_reply(status: StatusCode, error: ApiError) -> Response {
    let body = serde_json::to_vec(&Reply::<()>::Err(error)).unwrap_or_default();
    json_response(status, body)
}

/// What reading a request body can end with.
pub(crate) enum BodyError {
    /// More than the cap, by the declared length or by the bytes that came.
    TooLarge,
    /// The client went away or sent a broken body.
    Broken,
}

/// Reads a whole body of at most `limit` bytes. A body over the cap is refused whole, never
/// cut and passed on.
pub(crate) async fn read_body(
    body: Body,
    declared: Option<u64>,
    limit: usize,
) -> Result<Bytes, BodyError> {
    if declared.is_some_and(|length| length > limit as u64) {
        return Err(BodyError::TooLarge);
    }
    let mut collected = Vec::new();
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| BodyError::Broken)?;
        if collected.len() + chunk.len() > limit {
            return Err(BodyError::TooLarge);
        }
        collected.extend_from_slice(&chunk);
    }
    Ok(Bytes::from(collected))
}

pub(crate) fn declared_length(request: &Request) -> Option<u64> {
    request
        .headers()
        .get(header::CONTENT_LENGTH)?
        .to_str()
        .ok()?
        .parse()
        .ok()
}

pub(crate) fn too_large() -> Response {
    let mut response = (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONNECTION, HeaderValue::from_static("close"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// `POST /api/v1/{service}/{name}`.
pub(crate) async fn api<B: Backend>(
    State(state): State<AppState<B>>,
    Path((service, name)): Path<(String, String)>,
    request: Request,
) -> Response {
    let joined = format!("{service}/{name}");
    let Some(path) = state
        .shared
        .routes
        .find(&joined, state.shared.test_hooks.load(Ordering::Relaxed))
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let declared = declared_length(&request);
    let body = match read_body(request.into_body(), declared, state.shared.limits.api_body).await {
        Ok(body) => body,
        Err(BodyError::TooLarge) => return too_large(),
        Err(BodyError::Broken) => return StatusCode::BAD_REQUEST.into_response(),
    };
    // A panic in the application must not end the server; the person sees an internal error.
    let answer = AssertUnwindSafe(state.backend.call(path, body))
        .catch_unwind()
        .await;
    match answer {
        Ok(answer) => answer_response(answer),
        Err(_) => {
            eprintln!("tc4: a request to {path} panicked");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

fn answer_response(answer: Answer) -> Response {
    match answer {
        Answer::Reply { body, then } => match then {
            Then::Continue => json_response(StatusCode::OK, body),
        },
        Answer::NotFound => StatusCode::NOT_FOUND.into_response(),
    }
}
