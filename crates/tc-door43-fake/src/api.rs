//! Routes, auth and handlers.

use crate::git::TagInfo;
use crate::json::{self, EntryFacts, RepoFacts, StagePoint};
use crate::smart_http::{self, CgiRequest, Service};
use crate::zipping::{self, SbExport};
use crate::{AuthSeen, Fault, RecordedRequest, ReleaseKind, RepoRec, Shared};
use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use base64::Engine;
use serde_json::{Value, json};
use std::sync::Arc;

type AppState = State<Arc<Shared>>;

pub(crate) fn router(shared: Arc<Shared>) -> Router {
    Router::new()
        .route("/api/v1/catalog/search", get(catalog_search))
        .route(
            "/api/v1/catalog/entry/{owner}/{repo}/{git_ref}",
            get(catalog_entry),
        )
        .route("/api/v1/user", get(current_user))
        .route("/api/v1/user/orgs", get(user_orgs))
        .route("/api/v1/user/repos", post(create_user_repo))
        .route("/api/v1/orgs/{org}/repos", post(create_org_repo))
        .route(
            "/api/v1/users/{username}/tokens",
            get(list_tokens).post(create_token),
        )
        .route(
            "/api/v1/users/{username}/tokens/{token}",
            delete(delete_token),
        )
        .route(
            "/api/v1/users/{username}/orgs/{org}/permissions",
            get(org_permissions),
        )
        .route("/api/v1/repos/{owner}/{repo}", get(repo_info))
        .route("/api/v1/repos/{owner}/{repo}/tags", get(list_tags))
        .route("/api/v1/repos/{owner}/{repo}/tags/{*tag}", get(one_tag))
        .route("/api/v1/repos/{owner}/{repo}/releases", get(list_releases))
        .route(
            "/api/v1/repos/{owner}/{repo}/releases/latest",
            get(latest_release),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/git/commits/{sha}",
            get(one_commit),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/archive/{archive}",
            get(archive),
        )
        .route("/{owner}/{repo}/archive/{archive}", get(archive))
        .route("/{owner}/{repo}/sb/{*file}", get(sb_export))
        .route("/{owner}/{repo}/info/refs", get(smart_http_route))
        .route("/{owner}/{repo}/git-upload-pack", post(smart_http_route))
        .route("/{owner}/{repo}/git-receive-pack", post(smart_http_route))
        .layer(DefaultBodyLimit::disable())
        .layer(middleware::from_fn_with_state(shared.clone(), observe))
        .with_state(shared)
}

/// A Gitea error answer: `{"message": ..., "url": ...}`.
pub(crate) struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "The target couldn't be found.")
    }

    fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "invalid username, password or token",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({ "message": self.message, "url": "/api/swagger" });
        (self.status, axum::Json(body)).into_response()
    }
}

impl From<crate::FakeError> for ApiError {
    fn from(error: crate::FakeError) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
    }
}

type ApiResult = Result<Response, ApiError>;

fn json_response(status: StatusCode, body: Value) -> Response {
    (status, axum::Json(body)).into_response()
}

fn listing(items: Vec<Value>, total: usize) -> Response {
    let mut response = json_response(StatusCode::OK, Value::Array(items));
    if let Ok(value) = HeaderValue::from_str(&total.to_string()) {
        response.headers_mut().insert("x-total-count", value);
    }
    response
}

/// Query parameters, repeated or comma-separated.
struct Params(Vec<(String, String)>);

impl Params {
    fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    fn all(&self, name: &str) -> Vec<String> {
        self.0
            .iter()
            .filter(|(k, _)| k == name)
            .flat_map(|(_, v)| v.split(','))
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
            .collect()
    }

    fn flag(&self, name: &str) -> bool {
        self.get(name)
            .is_some_and(|v| v.eq_ignore_ascii_case("true"))
    }

    fn number(&self, name: &str) -> Option<usize> {
        self.get(name).and_then(|v| v.parse().ok())
    }
}

/// A page of `items`. `limit` is clamped to the server's page size, as Gitea clamps it.
fn page<T: Clone>(items: &[T], params: &Params, max: usize) -> Vec<T> {
    let limit = params.number("limit").unwrap_or(max).clamp(1, max);
    let page = params.number("page").unwrap_or(1).max(1);
    items
        .iter()
        .skip((page - 1).saturating_mul(limit))
        .take(limit)
        .cloned()
        .collect()
}

// ---------------------------------------------------------------- observe and faults

async fn observe(State(shared): AppState, request: Request, next: Next) -> Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_owned();
    let query = request.uri().query().map(str::to_owned);
    let authorization = auth_seen(request.headers());
    shared.state().requests.push(RecordedRequest {
        method: method.clone(),
        path: path.clone(),
        query,
        authorization,
    });
    for fault in shared.take_faults(&method, &path, true) {
        match fault {
            Fault::Delay(duration) => tokio::time::sleep(duration).await,
            Fault::Status { status, message } => {
                let status =
                    StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                return ApiError::new(status, message).into_response();
            }
            Fault::RateLimited { retry_after_secs } => {
                let mut response =
                    ApiError::new(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded")
                        .into_response();
                if let Ok(value) = HeaderValue::from_str(&retry_after_secs.to_string()) {
                    response.headers_mut().insert(header::RETRY_AFTER, value);
                }
                return response;
            }
            Fault::EchoAuthorization => {
                return ApiError::new(
                    StatusCode::BAD_REQUEST,
                    format!(
                        "cannot use the header Authorization: {}",
                        authorization_text(&request)
                    ),
                )
                .into_response();
            }
            Fault::EchoAuthorizationAsText { filler } => {
                let body = format!("{}{}", "x".repeat(filler), authorization_text(&request));
                let mut response = (StatusCode::BAD_REQUEST, body).into_response();
                response.headers_mut().insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/html; charset=utf-8"),
                );
                return response;
            }
            Fault::Raw {
                status,
                headers,
                body,
            } => {
                let mut response = Response::new(Body::from(body));
                *response.status_mut() =
                    StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                for (name, value) in headers {
                    if let (Ok(name), Ok(value)) = (
                        header::HeaderName::from_bytes(name.as_bytes()),
                        HeaderValue::from_str(&value),
                    ) {
                        response.headers_mut().insert(name, value);
                    }
                }
                return response;
            }
            Fault::RedirectTo(location) => {
                let mut response = StatusCode::FOUND.into_response();
                if let Ok(value) = HeaderValue::from_str(&location) {
                    response.headers_mut().insert(header::LOCATION, value);
                }
                return response;
            }
            _ => {}
        }
    }
    next.run(request).await
}

fn authorization_text(request: &Request) -> String {
    request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned()
}

fn auth_seen(headers: &HeaderMap) -> AuthSeen {
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
        return AuthSeen::None;
    };
    match value.split_once(' ') {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("basic") => {
            decode_basic(rest).map_or(AuthSeen::Other, |(login, _)| AuthSeen::Basic { login })
        }
        Some((scheme, _))
            if scheme.eq_ignore_ascii_case("token") || scheme.eq_ignore_ascii_case("bearer") =>
        {
            AuthSeen::Token
        }
        _ => AuthSeen::Other,
    }
}

fn decode_basic(encoded: &str) -> Option<(String, String)> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let (login, secret) = text.split_once(':')?;
    Some((login.to_owned(), secret.to_owned()))
}

// ---------------------------------------------------------------- auth

/// Who is calling. `scopes` is `None` for a password sign-in, which may do anything.
pub(crate) enum Caller {
    Anonymous,
    User {
        login: String,
        scopes: Option<Vec<String>>,
    },
}

fn authenticate(shared: &Shared, headers: &HeaderMap, params: &Params) -> Result<Caller, ApiError> {
    if params.get("token").is_some() || params.get("access_token").is_some() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "a token in the URL is refused by the fake Door43; send it in the Authorization header",
        ));
    }
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
        return Ok(Caller::Anonymous);
    };
    let state = shared.state();
    let by_token = |secret: &str| {
        state.users.iter().find_map(|u| {
            u.tokens
                .iter()
                .find(|t| t.sha1 == secret)
                .map(|t| Caller::User {
                    login: u.user.login.clone(),
                    scopes: Some(t.scopes.clone()),
                })
        })
    };
    match value.split_once(' ') {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("basic") => {
            let (login, secret) = decode_basic(rest).ok_or_else(ApiError::unauthorized)?;
            let user = state
                .users
                .iter()
                .find(|u| {
                    u.user.login.eq_ignore_ascii_case(&login)
                        || u.user.email.eq_ignore_ascii_case(&login)
                })
                .ok_or_else(ApiError::unauthorized)?;
            if user.user.password == secret {
                if let Some(code) = &user.user.one_time_code {
                    let sent = headers.get("x-gitea-otp").and_then(|v| v.to_str().ok());
                    if sent != Some(code.as_str()) {
                        return Err(ApiError::new(
                            StatusCode::UNAUTHORIZED,
                            "Only signed in user is allowed to call APIs.",
                        ));
                    }
                }
                return Ok(Caller::User {
                    login: user.user.login.clone(),
                    scopes: None,
                });
            }
            match by_token(&secret) {
                Some(caller @ Caller::User { .. }) => Ok(caller),
                _ => Err(ApiError::unauthorized()),
            }
        }
        Some((scheme, rest))
            if scheme.eq_ignore_ascii_case("token") || scheme.eq_ignore_ascii_case("bearer") =>
        {
            by_token(rest.trim()).ok_or_else(ApiError::unauthorized)
        }
        _ => Err(ApiError::unauthorized()),
    }
}

/// The caller's login, when its credential carries `scope` (a write scope holds the read
/// of its group).
fn require(caller: &Caller, scope: &str) -> Result<String, ApiError> {
    match caller {
        Caller::Anonymous => Err(ApiError::new(StatusCode::UNAUTHORIZED, "token is required")),
        Caller::User {
            login,
            scopes: None,
        } => Ok(login.clone()),
        Caller::User {
            login,
            scopes: Some(scopes),
        } => {
            let write = scope.replacen("read:", "write:", 1);
            if scopes
                .iter()
                .any(|s| s == scope || *s == write || s == "all")
            {
                Ok(login.clone())
            } else {
                Err(ApiError::new(
                    StatusCode::FORBIDDEN,
                    format!("token does not have at least one of required scope(s): [{scope}]"),
                ))
            }
        }
    }
}

/// Token routes take Basic auth with the account's password, as Gitea's do.
fn require_password(caller: &Caller, username: &str) -> Result<String, ApiError> {
    match caller {
        Caller::User {
            login,
            scopes: None,
        } if login.eq_ignore_ascii_case(username) => Ok(login.clone()),
        Caller::User { scopes: None, .. } => Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "the user may only manage their own tokens",
        )),
        _ => Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "auth required: basic authentication with a password",
        )),
    }
}

fn caller_login(caller: &Caller) -> Option<&str> {
    match caller {
        Caller::User { login, .. } => Some(login),
        Caller::Anonymous => None,
    }
}

// ---------------------------------------------------------------- accounts and tokens

async fn current_user(
    State(shared): AppState,
    headers: HeaderMap,
    Query(params): Query<Vec<(String, String)>>,
) -> ApiResult {
    let params = Params(params);
    let caller = authenticate(&shared, &headers, &params)?;
    let login = require(&caller, "read:user")?;
    let state = shared.state();
    let user = state.user(&login).ok_or_else(ApiError::not_found)?;
    Ok(json_response(
        StatusCode::OK,
        json::user(&shared.url, user.id, &user.user.login, &user.user.email),
    ))
}

async fn list_tokens(
    State(shared): AppState,
    Path(username): Path<String>,
    headers: HeaderMap,
    Query(params): Query<Vec<(String, String)>>,
) -> ApiResult {
    let params = Params(params);
    let caller = authenticate(&shared, &headers, &params)?;
    let login = require_password(&caller, &username)?;
    let state = shared.state();
    let user = state.user(&login).ok_or_else(ApiError::not_found)?;
    let all: Vec<Value> = user
        .tokens
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "name": t.name,
                "scopes": t.scopes,
                "sha1": "",
                "token_last_eight": &t.sha1[t.sha1.len().saturating_sub(8)..],
            })
        })
        .collect();
    let total = all.len();
    Ok(listing(page(&all, &params, state.max_page_size), total))
}

const KNOWN_SCOPES: &[&str] = &[
    "all",
    "read:activitypub",
    "write:activitypub",
    "read:admin",
    "write:admin",
    "read:issue",
    "write:issue",
    "read:misc",
    "write:misc",
    "read:notification",
    "write:notification",
    "read:organization",
    "write:organization",
    "read:package",
    "write:package",
    "read:repository",
    "write:repository",
    "read:user",
    "write:user",
];

async fn create_token(
    State(shared): AppState,
    Path(username): Path<String>,
    headers: HeaderMap,
    Query(params): Query<Vec<(String, String)>>,
    body: Bytes,
) -> ApiResult {
    let params = Params(params);
    let caller = authenticate(&shared, &headers, &params)?;
    let login = require_password(&caller, &username)?;
    let body: Value = serde_json::from_slice(&body)
        .map_err(|e| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;
    let name = body.get("name").and_then(Value::as_str).unwrap_or_default();
    if name.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "[Name]: Required",
        ));
    }
    let scopes: Vec<String> = body
        .get("scopes")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if let Some(bad) = scopes.iter().find(|s| !KNOWN_SCOPES.contains(&s.as_str())) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("invalid access token scope provided: {bad}"),
        ));
    }
    let mut state = shared.state();
    let taken = state
        .user(&login)
        .is_some_and(|u| u.tokens.iter().any(|t| t.name == name));
    if taken {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "access token name has been used already",
        ));
    }
    let token = state
        .new_token(&login, name, scopes)
        .ok_or_else(ApiError::not_found)?;
    Ok(json_response(
        StatusCode::CREATED,
        json!({
            "id": token.id,
            "name": token.name,
            "scopes": token.scopes,
            "sha1": token.sha1,
            "token_last_eight": &token.sha1[token.sha1.len().saturating_sub(8)..],
        }),
    ))
}

async fn delete_token(
    State(shared): AppState,
    Path((username, token)): Path<(String, String)>,
    headers: HeaderMap,
    Query(params): Query<Vec<(String, String)>>,
) -> ApiResult {
    let params = Params(params);
    let caller = authenticate(&shared, &headers, &params)?;
    let login = require_password(&caller, &username)?;
    let mut state = shared.state();
    let user = state.user_mut(&login).ok_or_else(ApiError::not_found)?;
    let matching: Vec<u64> = user
        .tokens
        .iter()
        .filter(|t| t.id.to_string() == token || t.name == token)
        .map(|t| t.id)
        .collect();
    match matching.as_slice() {
        [] => Err(ApiError::not_found()),
        [id] => {
            user.tokens.retain(|t| t.id != *id);
            Ok(StatusCode::NO_CONTENT.into_response())
        }
        _ => Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "multiple matches for token name, please specify by ID",
        )),
    }
}

async fn user_orgs(
    State(shared): AppState,
    headers: HeaderMap,
    Query(params): Query<Vec<(String, String)>>,
) -> ApiResult {
    let params = Params(params);
    let caller = authenticate(&shared, &headers, &params)?;
    let login = require(&caller, "read:organization")?;
    let state = shared.state();
    let all: Vec<Value> = state
        .orgs
        .iter()
        .filter(|o| {
            o.org
                .members
                .iter()
                .any(|m| m.login.eq_ignore_ascii_case(&login))
        })
        .map(|o| json::organization(&shared.url, o.id, &o.org.name, &o.org.full_name))
        .collect();
    let total = all.len();
    Ok(listing(page(&all, &params, state.max_page_size), total))
}

async fn org_permissions(
    State(shared): AppState,
    Path((username, org)): Path<(String, String)>,
    headers: HeaderMap,
    Query(params): Query<Vec<(String, String)>>,
) -> ApiResult {
    let params = Params(params);
    let caller = authenticate(&shared, &headers, &params)?;
    let login = require(&caller, "read:organization")?;
    if !login.eq_ignore_ascii_case(&username) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "only the user may read their own permissions",
        ));
    }
    let state = shared.state();
    let org = state.org(&org).ok_or_else(ApiError::not_found)?;
    let member = org
        .org
        .members
        .iter()
        .find(|m| m.login.eq_ignore_ascii_case(&login));
    Ok(json_response(
        StatusCode::OK,
        json!({
            "can_create_repository": member.is_some_and(|m| m.can_create_repository),
            "can_read": true,
            "can_write": member.is_some_and(|m| m.can_create_repository),
            "is_admin": false,
            "is_owner": false,
        }),
    ))
}

// ---------------------------------------------------------------- creating repositories

async fn create_user_repo(
    State(shared): AppState,
    headers: HeaderMap,
    Query(params): Query<Vec<(String, String)>>,
    body: Bytes,
) -> ApiResult {
    let params = Params(params);
    let caller = authenticate(&shared, &headers, &params)?;
    let login = require(&caller, "write:user")?;
    create_repo(&shared, &login, &login, &body).await
}

async fn create_org_repo(
    State(shared): AppState,
    Path(org): Path<String>,
    headers: HeaderMap,
    Query(params): Query<Vec<(String, String)>>,
    body: Bytes,
) -> ApiResult {
    let params = Params(params);
    let caller = authenticate(&shared, &headers, &params)?;
    let login = require(&caller, "write:organization")?;
    {
        let state = shared.state();
        if state.org(&org).is_none() {
            return Err(ApiError::not_found());
        }
        if !state.can_write(&login, &org) {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "Given user is not allowed to create repository in organization.",
            ));
        }
    }
    create_repo(&shared, &login, &org, &body).await
}

fn valid_repo_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    let lower = name.to_ascii_lowercase();
    !name.is_empty()
        && name.len() <= 100
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        && !lower.ends_with(".git")
        && !lower.ends_with(".wiki")
}

async fn create_repo(shared: &Shared, login: &str, owner: &str, body: &[u8]) -> ApiResult {
    let body: Value = serde_json::from_slice(body)
        .map_err(|e| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;
    let name = body.get("name").and_then(Value::as_str).unwrap_or_default();
    if !valid_repo_name(name) {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "[Name]: invalid repository name",
        ));
    }
    let private = body
        .get("private")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let auto_init = body
        .get("auto_init")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let branch = body
        .get("default_branch")
        .and_then(Value::as_str)
        .filter(|b| !b.is_empty())
        .unwrap_or("main")
        .to_owned();
    let description = body
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let (dir, record) = {
        let mut state = shared.state();
        if state.repo(owner, name).is_some() {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "The repository with the same name already exists.",
            ));
        }
        let owner = state.account_name(owner).ok_or_else(ApiError::not_found)?;
        let id = state.next_id();
        let created_unix = state.tick();
        let record = RepoRec {
            id,
            owner: owner.clone(),
            name: name.to_owned(),
            description,
            subject: String::new(),
            language: String::new(),
            topics: Vec::new(),
            private,
            default_branch: branch.clone(),
            created_unix,
            releases: Vec::new(),
        };
        state.repos.push(record.clone());
        (shared.git.repo_dir(&owner, name), record)
    };
    shared.git.init_bare(&dir, &branch).await?;
    if auto_init {
        let when = shared.state().tick();
        let readme = format!("# {name}\n\n{}\n", record.description);
        shared
            .git
            .commit_snapshot(
                &dir,
                &branch,
                &[(
                    "README.md".to_owned(),
                    crate::SeedMode::Regular,
                    readme.into_bytes(),
                )],
                "Initial commit",
                when,
            )
            .await?;
    }
    let view = repo_view(shared, &record).await?;
    let can_push = shared.state().can_write(login, &record.owner);
    Ok(json_response(
        StatusCode::CREATED,
        repository_json(shared, &view, can_push),
    ))
}

// ---------------------------------------------------------------- repositories

/// A repository with what git knows about it.
struct RepoView {
    rec: RepoRec,
    tags: Vec<TagInfo>,
    /// Releases newest first, with their commits.
    releases: Vec<(crate::ReleaseRec, StagePoint)>,
    head: Option<StagePoint>,
}

async fn repo_view(shared: &Shared, rec: &RepoRec) -> Result<RepoView, ApiError> {
    let dir = shared.git.repo_dir(&rec.owner, &rec.name);
    let tags = shared.git.tags(&dir).await?;
    let mut releases = Vec::new();
    for release in &rec.releases {
        if let Some(tag) = tags.iter().find(|t| t.name == release.tag) {
            releases.push((
                release.clone(),
                StagePoint {
                    ref_name: release.tag.clone(),
                    commit: tag.commit.clone(),
                    unix: release.created_unix,
                    release_id: Some(release.id),
                },
            ));
        }
    }
    releases.sort_by_key(|r| std::cmp::Reverse(r.1.unix));
    let head = match shared
        .git
        .resolve(&dir, &format!("refs/heads/{}", rec.default_branch))
        .await
    {
        Ok(sha) => {
            let unix = shared.git.commit_time(&dir, &sha).await?;
            Some(StagePoint {
                ref_name: rec.default_branch.clone(),
                commit: sha,
                unix,
                release_id: None,
            })
        }
        Err(_) => None,
    };
    Ok(RepoView {
        rec: rec.clone(),
        tags,
        releases,
        head,
    })
}

impl RepoView {
    fn prod(&self) -> Option<&StagePoint> {
        self.releases
            .iter()
            .find(|(r, _)| r.kind == ReleaseKind::Published)
            .map(|(_, p)| p)
    }

    fn preprod(&self) -> Option<&StagePoint> {
        self.releases
            .iter()
            .find(|(r, _)| r.kind != ReleaseKind::Draft)
            .map(|(_, p)| p)
    }
}

fn owner_json(shared: &Shared, owner: &str) -> Value {
    let state = shared.state();
    if let Some(user) = state.user(owner) {
        json::user(&shared.url, user.id, &user.user.login, &user.user.email)
    } else if let Some(org) = state.org(owner) {
        json::user(&shared.url, org.id, &org.org.name, "")
    } else {
        Value::Null
    }
}

fn repository_json(shared: &Shared, view: &RepoView, can_push: bool) -> Value {
    let metadata = view
        .head
        .as_ref()
        .and_then(|h| shared.state().metadata_cache.get(&h.commit).cloned())
        .flatten();
    json::repository(
        &shared.url,
        &view.rec,
        &RepoFacts {
            owner: owner_json(shared, &view.rec.owner),
            empty: view.head.is_none(),
            can_push,
            prod: view.prod(),
            preprod: view.preprod(),
            latest: view.head.as_ref(),
            metadata: metadata.as_ref(),
        },
    )
}

/// The repository, when the caller may see it; private ones only to members.
fn visible_repo(
    shared: &Shared,
    caller: &Caller,
    owner: &str,
    name: &str,
) -> Result<RepoRec, ApiError> {
    let state = shared.state();
    let rec = state.repo(owner, name).ok_or_else(ApiError::not_found)?;
    let allowed =
        !rec.private || caller_login(caller).is_some_and(|l| state.is_member(l, &rec.owner));
    if allowed {
        Ok(rec.clone())
    } else {
        Err(ApiError::not_found())
    }
}

async fn metadata_at(shared: &Shared, rec: &RepoRec, commit: &str) -> Option<Value> {
    if let Some(cached) = shared.state().metadata_cache.get(commit) {
        return cached.clone();
    }
    let dir = shared.git.repo_dir(&rec.owner, &rec.name);
    let parsed = match shared.git.file_at(&dir, commit, "metadata.json").await {
        Ok(Some(bytes)) => serde_json::from_slice(&bytes).ok(),
        _ => None,
    };
    shared
        .state()
        .metadata_cache
        .insert(commit.to_owned(), parsed.clone());
    parsed
}

async fn repo_info(
    State(shared): AppState,
    Path((owner, name)): Path<(String, String)>,
    headers: HeaderMap,
    Query(params): Query<Vec<(String, String)>>,
) -> ApiResult {
    let params = Params(params);
    let caller = authenticate(&shared, &headers, &params)?;
    let rec = visible_repo(&shared, &caller, &owner, &name)?;
    let view = repo_view(&shared, &rec).await?;
    if let Some(head) = &view.head {
        metadata_at(&shared, &rec, &head.commit).await;
    }
    let can_push = caller_login(&caller).is_some_and(|l| shared.state().can_write(l, &rec.owner));
    Ok(json_response(
        StatusCode::OK,
        repository_json(&shared, &view, can_push),
    ))
}

async fn tag_values(shared: &Shared, view: &RepoView) -> Result<Vec<Value>, ApiError> {
    let dir = shared.git.repo_dir(&view.rec.owner, &view.rec.name);
    let mut out = Vec::new();
    for tag in &view.tags {
        let unix = shared.git.commit_time(&dir, &tag.commit).await?;
        out.push(json::tag(&shared.url, &view.rec, tag, unix));
    }
    Ok(out)
}

async fn list_tags(
    State(shared): AppState,
    Path((owner, name)): Path<(String, String)>,
    headers: HeaderMap,
    Query(params): Query<Vec<(String, String)>>,
) -> ApiResult {
    let params = Params(params);
    let caller = authenticate(&shared, &headers, &params)?;
    let rec = visible_repo(&shared, &caller, &owner, &name)?;
    let view = repo_view(&shared, &rec).await?;
    let all = tag_values(&shared, &view).await?;
    let total = all.len();
    let max = shared.state().max_page_size;
    Ok(listing(page(&all, &params, max), total))
}

async fn one_tag(
    State(shared): AppState,
    Path((owner, name, tag)): Path<(String, String, String)>,
    headers: HeaderMap,
    Query(params): Query<Vec<(String, String)>>,
) -> ApiResult {
    let params = Params(params);
    let caller = authenticate(&shared, &headers, &params)?;
    let rec = visible_repo(&shared, &caller, &owner, &name)?;
    let view = repo_view(&shared, &rec).await?;
    let found = view
        .tags
        .iter()
        .find(|t| t.name == tag)
        .ok_or_else(ApiError::not_found)?;
    let dir = shared.git.repo_dir(&rec.owner, &rec.name);
    let unix = shared.git.commit_time(&dir, &found.commit).await?;
    Ok(json_response(
        StatusCode::OK,
        json::tag(&shared.url, &rec, found, unix),
    ))
}

fn release_values(shared: &Shared, view: &RepoView, caller: &Caller) -> Vec<Value> {
    let author = owner_json(shared, &view.rec.owner);
    let sees_drafts =
        caller_login(caller).is_some_and(|l| shared.state().can_write(l, &view.rec.owner));
    view.releases
        .iter()
        .filter(|(r, _)| r.kind != ReleaseKind::Draft || sees_drafts)
        .map(|(r, _)| json::release(&shared.url, &view.rec, r, author.clone()))
        .collect()
}

async fn list_releases(
    State(shared): AppState,
    Path((owner, name)): Path<(String, String)>,
    headers: HeaderMap,
    Query(params): Query<Vec<(String, String)>>,
) -> ApiResult {
    let params = Params(params);
    let caller = authenticate(&shared, &headers, &params)?;
    let rec = visible_repo(&shared, &caller, &owner, &name)?;
    let view = repo_view(&shared, &rec).await?;
    let all = release_values(&shared, &view, &caller);
    let total = all.len();
    let max = shared.state().max_page_size;
    Ok(listing(page(&all, &params, max), total))
}

async fn latest_release(
    State(shared): AppState,
    Path((owner, name)): Path<(String, String)>,
    headers: HeaderMap,
    Query(params): Query<Vec<(String, String)>>,
) -> ApiResult {
    let params = Params(params);
    let caller = authenticate(&shared, &headers, &params)?;
    let rec = visible_repo(&shared, &caller, &owner, &name)?;
    let view = repo_view(&shared, &rec).await?;
    let (release, _) = view
        .releases
        .iter()
        .find(|(r, _)| r.kind == ReleaseKind::Published)
        .ok_or_else(ApiError::not_found)?;
    let author = owner_json(&shared, &rec.owner);
    Ok(json_response(
        StatusCode::OK,
        json::release(&shared.url, &rec, release, author),
    ))
}

async fn one_commit(
    State(shared): AppState,
    Path((owner, name, sha)): Path<(String, String, String)>,
    headers: HeaderMap,
    Query(params): Query<Vec<(String, String)>>,
) -> ApiResult {
    let params = Params(params);
    let caller = authenticate(&shared, &headers, &params)?;
    let rec = visible_repo(&shared, &caller, &owner, &name)?;
    let dir = shared.git.repo_dir(&rec.owner, &rec.name);
    let commit = shared
        .git
        .resolve(&dir, &sha)
        .await
        .map_err(|_| ApiError::not_found())?;
    let info = shared.git.commit_info(&dir, &commit).await?;
    Ok(json_response(
        StatusCode::OK,
        json::commit(&shared.url, &rec, &info),
    ))
}

// ---------------------------------------------------------------- downloads

async fn archive(
    State(shared): AppState,
    Path((owner, name, file)): Path<(String, String, String)>,
    method: Method,
    request_headers: HeaderMap,
    uri: axum::http::Uri,
    Query(params): Query<Vec<(String, String)>>,
) -> ApiResult {
    let params = Params(params);
    let caller = authenticate(&shared, &request_headers, &params)?;
    let rec = visible_repo(&shared, &caller, &owner, &name)?;
    let (git_ref, format, content_type) = if let Some(r) = file.strip_suffix(".zip") {
        (r, "zip", "application/zip")
    } else if let Some(r) = file.strip_suffix(".tar.gz") {
        (r, "tar.gz", "application/gzip")
    } else {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "unknown archive format",
        ));
    };
    let dir = shared.git.repo_dir(&rec.owner, &rec.name);
    let commit = shared
        .git
        .resolve(&dir, git_ref)
        .await
        .map_err(|_| ApiError::not_found())?;
    let mut bytes = shared.git.archive(&dir, &commit, format, &rec.name).await?;
    for fault in shared.take_faults(method.as_str(), uri.path(), false) {
        match fault {
            Fault::ArchiveComment(comment) => bytes = zipping::with_comment(bytes, &comment)?,
            Fault::ArchiveExtraFile { path, bytes: extra } => {
                bytes = zipping::with_extra_file(bytes, &format!("{}/{path}", rec.name), &extra)?;
            }
            _ => {}
        }
    }
    Ok(download(bytes, content_type, &file))
}

fn download(bytes: Vec<u8>, content_type: &str, file: &str) -> Response {
    let mut response = Response::new(Body::from(bytes));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    if let Ok(value) = HeaderValue::from_str(content_type) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{file}\"")) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    response
}

async fn sb_export(
    State(shared): AppState,
    Path((owner, name, file)): Path<(String, String, String)>,
    method: Method,
    request_headers: HeaderMap,
    uri: axum::http::Uri,
    Query(params): Query<Vec<(String, String)>>,
) -> ApiResult {
    let params = Params(params);
    let caller = authenticate(&shared, &request_headers, &params)?;
    let rec = visible_repo(&shared, &caller, &owner, &name)?;
    let tag = file.strip_suffix(".zip").ok_or_else(ApiError::not_found)?;
    let dir = shared.git.repo_dir(&rec.owner, &rec.name);
    let tags = shared.git.tags(&dir).await?;
    let commit = tags
        .iter()
        .find(|t| t.name == tag)
        .map(|t| t.commit.clone())
        .ok_or_else(ApiError::not_found)?;
    let archive = shared.git.archive(&dir, &commit, "zip", &rec.name).await?;
    let mut revision = commit.clone();
    let mut corrupt = Vec::new();
    for fault in shared.take_faults(method.as_str(), uri.path(), false) {
        match fault {
            Fault::SbRevision(r) => revision = r,
            Fault::SbCorruptFile { path } => corrupt.push(path),
            _ => {}
        }
    }
    let dcs_key = format!("{}/{}", rec.owner, rec.name);
    let timestamp = json::rfc3339(shared.state().tick());
    let export = SbExport {
        dcs_key: &dcs_key,
        revision: &revision,
        timestamp: &timestamp,
        corrupt: &corrupt,
    };
    let bytes = zipping::sb_export(&archive, &rec.name, &export)?.ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "the fake exports only repositories that hold a metadata.json",
        )
    })?;
    Ok(download(bytes, "application/zip", &file))
}

// ---------------------------------------------------------------- catalog

async fn catalog_points(
    shared: &Shared,
    view: &RepoView,
    stage: &str,
    include_history: bool,
) -> Vec<(StagePoint, String, bool, Value)> {
    let mut points: Vec<(StagePoint, String, bool, Value)> = Vec::new();
    let author = owner_json(shared, &view.rec.owner);
    for (release, point) in &view.releases {
        let (keep, stage_name) = match release.kind {
            ReleaseKind::Published => (true, "prod"),
            ReleaseKind::PreRelease => (stage != "prod", "preprod"),
            ReleaseKind::Draft => (false, ""),
        };
        if keep {
            points.push((
                StagePoint {
                    ref_name: point.ref_name.clone(),
                    commit: point.commit.clone(),
                    unix: point.unix,
                    release_id: point.release_id,
                },
                stage_name.to_owned(),
                true,
                json::release(&shared.url, &view.rec, release, author.clone()),
            ));
        }
    }
    if let (true, Some(head)) = (stage == "latest", &view.head) {
        points.push((
            StagePoint {
                ref_name: head.ref_name.clone(),
                commit: head.commit.clone(),
                unix: head.unix,
                release_id: None,
            },
            "latest".to_owned(),
            false,
            Value::Null,
        ));
    }
    points.sort_by_key(|p| std::cmp::Reverse(p.0.unix));
    if !include_history {
        points.truncate(1);
    }
    points
}

fn matches_any(values: &[String], actual: &str, partial: bool) -> bool {
    values.is_empty()
        || values.iter().any(|v| {
            if partial {
                actual.to_lowercase().contains(&v.to_lowercase())
            } else {
                actual.eq_ignore_ascii_case(v)
            }
        })
}

async fn catalog_search(
    State(shared): AppState,
    Query(params): Query<Vec<(String, String)>>,
) -> ApiResult {
    let params = Params(params);
    let partial = params.flag("partialMatch");
    let stage = params
        .all("stage")
        .first()
        .cloned()
        .unwrap_or_else(|| "prod".to_owned());
    let include_history = params.flag("includeHistory");
    let candidates: Vec<RepoRec> = shared
        .state()
        .repos
        .iter()
        .filter(|r| !r.private)
        .filter(|r| matches_any(&params.all("owner"), &r.owner, partial))
        .filter(|r| matches_any(&params.all("repo"), &r.name, partial))
        .filter(|r| matches_any(&params.all("subject"), &r.subject, partial))
        .filter(|r| {
            let topics = params.all("topic");
            topics.is_empty()
                || r.topics
                    .iter()
                    .any(|t| topics.iter().any(|w| w.eq_ignore_ascii_case(t)))
        })
        .cloned()
        .collect();
    let mut entries = Vec::new();
    for rec in candidates {
        let view = repo_view(&shared, &rec).await?;
        if let Some(head) = &view.head {
            metadata_at(&shared, &rec, &head.commit).await;
        }
        let repo_value = repository_json(&shared, &view, false);
        for (point, stage_name, is_tag, release) in
            catalog_points(&shared, &view, &stage, include_history).await
        {
            let metadata = metadata_at(&shared, &rec, &point.commit).await;
            let entry = json::catalog_entry(
                &shared.url,
                &rec,
                &EntryFacts {
                    point: &point,
                    stage: &stage_name,
                    is_tag,
                    release,
                    repo: repo_value.clone(),
                    metadata: metadata.as_ref(),
                },
            );
            let text = |field: &str| {
                entry
                    .get(field)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            let language_ok = matches_any(&params.all("lang"), &text("language"), partial);
            let tag_ok = matches_any(&params.all("tag"), &text("branch_or_tag_name"), false);
            let books = params.all("book");
            let book_ok = books.is_empty()
                || entry
                    .get("books")
                    .and_then(Value::as_array)
                    .is_some_and(|have| {
                        have.iter()
                            .filter_map(Value::as_str)
                            .any(|b| books.iter().any(|w| w.eq_ignore_ascii_case(b)))
                    });
            let q_ok = params.all("q").iter().all(|q| {
                let q = q.to_lowercase();
                [text("title"), text("name"), text("subject")]
                    .iter()
                    .any(|t| t.to_lowercase().contains(&q))
            });
            if language_ok && tag_ok && book_ok && q_ok {
                entries.push(entry);
            }
        }
    }
    let key = |e: &Value, f: &str| {
        e.get(f)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase()
    };
    entries.sort_by(|a, b| {
        (
            key(a, "language"),
            key(a, "subject"),
            key(a, "branch_or_tag_name"),
        )
            .cmp(&(
                key(b, "language"),
                key(b, "subject"),
                key(b, "branch_or_tag_name"),
            ))
    });
    let entries = match params.number("limit") {
        Some(limit) => {
            let page_number = params.number("page").unwrap_or(1).max(1);
            entries
                .into_iter()
                .skip((page_number - 1).saturating_mul(limit))
                .take(limit)
                .collect()
        }
        None => entries,
    };
    Ok(json_response(
        StatusCode::OK,
        json!({ "ok": true, "last_updated": json::rfc3339(crate::CLOCK_START), "data": entries }),
    ))
}

async fn catalog_entry(
    State(shared): AppState,
    Path((owner, name, git_ref)): Path<(String, String, String)>,
) -> ApiResult {
    let rec = {
        let state = shared.state();
        state
            .repo(&owner, &name)
            .filter(|r| !r.private)
            .cloned()
            .ok_or_else(ApiError::not_found)?
    };
    let view = repo_view(&shared, &rec).await?;
    let repo_value = repository_json(&shared, &view, false);
    let point = catalog_points(&shared, &view, "latest", true)
        .await
        .into_iter()
        .find(|(p, _, _, _)| p.ref_name == git_ref)
        .ok_or_else(ApiError::not_found)?;
    let (point, stage_name, is_tag, release) = point;
    let metadata = metadata_at(&shared, &rec, &point.commit).await;
    Ok(json_response(
        StatusCode::OK,
        json::catalog_entry(
            &shared.url,
            &rec,
            &EntryFacts {
                point: &point,
                stage: &stage_name,
                is_tag,
                release,
                repo: repo_value,
                metadata: metadata.as_ref(),
            },
        ),
    ))
}

// ---------------------------------------------------------------- git smart HTTP

fn challenge() -> Response {
    let mut response = (StatusCode::UNAUTHORIZED, "authentication required").into_response();
    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        HeaderValue::from_static("Basic realm=\"Gitea\""),
    );
    response
}

async fn smart_http_route(
    State(shared): AppState,
    Path((owner, repo)): Path<(String, String)>,
    method: Method,
    headers: HeaderMap,
    uri: axum::http::Uri,
    body: Bytes,
) -> Response {
    let name = repo.strip_suffix(".git").unwrap_or(&repo).to_owned();
    let tail = uri
        .path()
        .splitn(4, '/')
        .nth(3)
        .unwrap_or_default()
        .to_owned();
    let query = uri.query().unwrap_or_default().to_owned();
    let Some(service) = Service::of(&tail, &query) else {
        return ApiError::new(StatusCode::FORBIDDEN, "only smart HTTP is served").into_response();
    };
    let rec = match shared.state().repo(&owner, &name).cloned() {
        Some(rec) => rec,
        None => return ApiError::not_found().into_response(),
    };
    let params = Params(Vec::new());
    let caller = match authenticate(&shared, &headers, &params) {
        Ok(caller) => caller,
        Err(_) => return challenge(),
    };
    let remote_user = match (service, &caller) {
        (Service::ReceivePack, Caller::Anonymous) => return challenge(),
        (Service::ReceivePack, caller) => match require(caller, "write:repository") {
            Ok(login) if shared.state().can_write(&login, &rec.owner) => Some(login),
            Ok(login) if rec.private && !shared.state().is_member(&login, &rec.owner) => {
                return ApiError::not_found().into_response();
            }
            // As Door43 (docs/evidence/door43-live-2026-10-17.md, step 6): any signed-in
            // account gets the advertisement, and a push without write access is refused
            // inside git's report, with HTTP 200, never with a 403.
            Ok(_) if method == Method::POST => return smart_http::declined(&body),
            Ok(login) => Some(login),
            Err(error) => return error.into_response(),
        },
        (Service::UploadPack, Caller::Anonymous) if rec.private => return challenge(),
        // As Gitea: a private repository is read only by its owner and the members of its
        // organization, with a password or a token that can read repositories; to anyone
        // else it does not exist.
        (Service::UploadPack, caller) if rec.private => match require(caller, "read:repository") {
            Ok(login) if shared.state().is_member(&login, &rec.owner) => Some(login),
            Ok(_) => return ApiError::not_found().into_response(),
            Err(error) => return error.into_response(),
        },
        (Service::UploadPack, caller) => caller_login(caller).map(str::to_owned),
    };
    let dir = shared.git.repo_dir(&rec.owner, &rec.name);
    if !smart_http::exists(&dir) {
        return ApiError::not_found().into_response();
    }
    let path_info = format!(
        "/{}/{}.git/{tail}",
        rec.owner.to_lowercase(),
        rec.name.to_lowercase()
    );
    smart_http::run(
        &shared,
        CgiRequest {
            method: method.as_str(),
            path_info,
            query: &query,
            headers: &headers,
            remote_user: remote_user.as_deref(),
            body: body.to_vec(),
        },
    )
    .await
}
