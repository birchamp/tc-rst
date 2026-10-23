//! The checks every request passes before a handler sees it (APP-DESIGN §5.2), in order:
//! `Host`, the token, `Origin` on anything but GET, the API version on `/api/`. No response
//! carries a CORS header.
//!
//! Nothing here logs a header value or a URI with its query: either can hold the token.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tc_api::endpoint::{API_VERSION, API_VERSION_HEADER, TOKEN_HEADER};
use tc_api::error::ApiError;

use crate::routes::error_reply;
use crate::state::Shared;

/// The cookie a developer's browser holds in a debug build (§5.2, last row of the table).
#[cfg(debug_assertions)]
const DEV_COOKIE: &str = "tc4_token";

/// The query parameter that trades the token for that cookie, in a debug build.
#[cfg(debug_assertions)]
const DEV_TOKEN_PARAM: &str = "dev-token";

enum Verdict {
    Pass,
    Respond(Response),
}

/// The outermost layer.
pub(crate) async fn guard(
    State(shared): State<Arc<Shared>>,
    request: Request,
    next: Next,
) -> Response {
    let mut response = match check(&shared, &request) {
        Verdict::Pass => next.run(request).await,
        Verdict::Respond(response) => response,
    };
    let headers = response.headers_mut();
    // No handler adds a CORS header; this keeps it so if one ever does.
    let cors: Vec<_> = headers
        .keys()
        .filter(|name| name.as_str().starts_with("access-control-"))
        .cloned()
        .collect();
    for name in cors {
        headers.remove(name);
    }
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}

fn check(shared: &Shared, request: &Request) -> Verdict {
    let headers = request.headers();

    // 1. A request for another name reaches this server only through DNS rebinding.
    let mut hosts = headers.get_all(header::HOST).iter();
    match (hosts.next(), hosts.next()) {
        (Some(host), None) if host.as_bytes() == shared.host.as_bytes() => {}
        _ => return deny(StatusCode::FORBIDDEN, "forbidden"),
    }

    // 2. Before the token is required, a debug build trades it for a cookie once.
    #[cfg(debug_assertions)]
    if request.method() == Method::GET
        && request.uri().path() == "/"
        && let Some(token) = request
            .uri()
            .query()
            .and_then(|query| query_value(query, DEV_TOKEN_PARAM))
        && shared.token.matches(token.as_bytes())
    {
        return Verdict::Respond(dev_redirect(shared));
    }

    // 3. Every route, static files and unknown paths included.
    if !token_accepted(shared, headers) {
        return deny(StatusCode::UNAUTHORIZED, "unauthorized");
    }

    // 4. A page of another origin can send a simple POST without a preflight; it cannot
    //    choose the Origin header. HEAD is not GET here: it needs the header too.
    if request.method() != Method::GET && !origin_accepted(shared, headers) {
        return deny(StatusCode::FORBIDDEN, "forbidden");
    }

    // 5. A stale UI after an update reloads on this.
    if request.uri().path().starts_with("/api/") && !api_version_accepted(headers) {
        return Verdict::Respond(error_reply(
            StatusCode::CONFLICT,
            ApiError::Version {
                server: API_VERSION,
            },
        ));
    }
    Verdict::Pass
}

fn deny(status: StatusCode, text: &'static str) -> Verdict {
    let mut response = (status, text).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Verdict::Respond(response)
}

/// Every `X-Tc-Token` line, split on commas, must be the token, and there must be one.
/// Playwright's `extraHTTPHeaders` and the UI's own header can both set it on one request;
/// accepting that adds nothing an attacker can use, because each value must be the token.
fn token_accepted(shared: &Shared, headers: &HeaderMap) -> bool {
    let mut seen = false;
    for line in headers.get_all(TOKEN_HEADER) {
        for value in line.as_bytes().split(|byte| *byte == b',') {
            seen = true;
            if !shared.token.matches(value.trim_ascii()) {
                return false;
            }
        }
    }
    seen || cookie_accepted(shared, headers)
}

#[cfg(debug_assertions)]
fn cookie_accepted(shared: &Shared, headers: &HeaderMap) -> bool {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|line| line.to_str().ok())
        .flat_map(|line| line.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .any(|(name, value)| name == DEV_COOKIE && shared.token.matches(value.as_bytes()))
}

#[cfg(not(debug_assertions))]
fn cookie_accepted(_: &Shared, _: &HeaderMap) -> bool {
    false
}

fn origin_accepted(shared: &Shared, headers: &HeaderMap) -> bool {
    let mut origins = headers.get_all(header::ORIGIN).iter();
    matches!(
        (origins.next(), origins.next()),
        (Some(origin), None) if origin.as_bytes() == shared.origin.as_bytes()
    )
}

fn api_version_accepted(headers: &HeaderMap) -> bool {
    let mut versions = headers.get_all(API_VERSION_HEADER).iter();
    let want = API_VERSION.to_string();
    matches!(
        (versions.next(), versions.next()),
        (Some(version), None) if version.as_bytes() == want.as_bytes()
    )
}

#[cfg(debug_assertions)]
fn query_value<'a>(query: &'a str, name: &str) -> Option<&'a str> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(key, value)| (key == name).then_some(value))
}

#[cfg(debug_assertions)]
fn dev_redirect(shared: &Shared) -> Response {
    let token = shared.token.as_str();
    let mut response = StatusCode::SEE_OTHER.into_response();
    let headers = response.headers_mut();
    // The token is base64url, so both values are valid header text.
    if let Ok(location) = HeaderValue::from_str(&format!("/#t={token}")) {
        headers.insert(header::LOCATION, location);
    }
    if let Ok(cookie) = HeaderValue::from_str(&format!(
        "{DEV_COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/"
    )) {
        headers.insert(header::SET_COOKIE, cookie);
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
