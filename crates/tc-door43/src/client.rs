use crate::catalog::{CatalogEntry, CatalogQuery, RawCatalogEntry, RawSearchResults};
use crate::error::{AuthStatus, Door43Error, UnreachableCause};
use crate::git_tree::ObjectId;
use crate::internet::InternetSwitch;
use crate::names::{
    AccountName, CommitSha, RepoName, RepoRef, ServerUrl, TagName, TokenName, TokenScope,
    Visibility,
};
use crate::repos::{
    CreatedRepo, NewRepoOwner, Organization, RawAccessToken, RawCommit, RawCreatedToken,
    RawOrgPermissions, RawOrganization, RawRelease, RawRepository, RawTag, RawUser, Release,
    RepoInfo, Tag,
};
use crate::secret::{Credentials, Session, Token};
use crate::verify::{VerifiedArchive, VerifiedSbExport, verify_archive, verify_sb_export};
use base64::Engine;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderValue, RETRY_AFTER};
use reqwest::{Method, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::fmt;
use std::time::Duration;
use url::Url;

const USER_AGENT: &str = concat!("tc-door43/", env!("CARGO_PKG_VERSION"));
const JSON: &str = "application/json";
const ZIP: &str = "application/zip, application/octet-stream";
const MAX_JSON_BYTES: u64 = 64 << 20;
const MAX_ERROR_BODY: usize = 16 << 10;
const PAGE_LIMIT: u32 = 50;
/// 40 pages of 50 is far beyond any real repository's tags or releases (tC4 uses the same
/// bound); a listing that keeps going is refused rather than walked forever.
const MAX_PAGES: u32 = 40;
const MAX_REDIRECTS: usize = 5;

/// Timeouts and size caps. The defaults suit a slow field connection and the largest
/// resource exports (en_ult is about 8 MB zipped).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    pub connect_timeout: Duration,
    /// The longest wait for the next bytes of an answer.
    pub read_timeout: Duration,
    pub max_download_bytes: u64,
    pub max_unpacked_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(20),
            read_timeout: Duration::from_secs(60),
            max_download_bytes: 512 << 20,
            max_unpacked_bytes: 2 << 30,
        }
    }
}

/// The one HTTP client for Door43. Every call reads the internet switch before it sends
/// anything and refuses with [`Door43Error::Offline`] while the switch is off; a download
/// in progress stops at its next chunk when the switch goes off.
///
/// Secrets go only in the `Authorization` (and `X-GITEA-OTP`) header, marked sensitive.
/// No URL carries one, and the log lines name only the method, path and status.
#[derive(Clone)]
pub struct Door43Client {
    http: reqwest::Client,
    server: ServerUrl,
    internet: InternetSwitch,
    limits: Limits,
}

impl fmt::Debug for Door43Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The reqwest client is left out: its Debug lists proxies, whose URLs can carry
        // credentials.
        f.debug_struct("Door43Client")
            .field("server", &self.server.to_string())
            .field("internet_on", &self.internet.is_on())
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy)]
enum Auth<'a> {
    Anonymous,
    Token(&'a Token),
    Basic(&'a Credentials),
}

/// The header values for one request, and every form of the secrets in them, so an
/// error message that echoes a header can be scrubbed.
struct AuthHeaders {
    authorization: Option<HeaderValue>,
    one_time_code: Option<HeaderValue>,
    secrets: Vec<String>,
}

impl AuthHeaders {
    fn of(auth: Auth<'_>) -> Result<Self, &'static str> {
        let sensitive = |value: String| -> Result<HeaderValue, &'static str> {
            let mut header = HeaderValue::from_str(&value)
                .map_err(|_| "the secret cannot be sent in a header")?;
            header.set_sensitive(true);
            Ok(header)
        };
        match auth {
            Auth::Anonymous => Ok(Self {
                authorization: None,
                one_time_code: None,
                secrets: Vec::new(),
            }),
            Auth::Token(token) => {
                let secret = token.expose_secret().to_owned();
                Ok(Self {
                    authorization: Some(sensitive(format!("token {secret}"))?),
                    one_time_code: None,
                    secrets: vec![secret],
                })
            }
            Auth::Basic(credentials) => {
                let password = credentials.password().expose_secret().to_owned();
                // Encoded from UTF-8 bytes: a password outside Latin-1 must still work.
                let encoded = base64::engine::general_purpose::STANDARD
                    .encode(format!("{}:{password}", credentials.login()));
                let mut secrets = vec![password, encoded.clone()];
                let one_time_code = match credentials.one_time_code() {
                    Some(code) => {
                        secrets.push(code.expose_secret().to_owned());
                        Some(sensitive(code.expose_secret().to_owned())?)
                    }
                    None => None,
                };
                Ok(Self {
                    authorization: Some(sensitive(format!("Basic {encoded}"))?),
                    one_time_code,
                    secrets,
                })
            }
        }
    }

    fn scrub(&self, mut message: String) -> String {
        for secret in self.secrets.iter().filter(|s| !s.is_empty()) {
            message = message.replace(secret.as_str(), "<redacted>");
        }
        message
    }
}

impl Door43Client {
    pub fn new(server: ServerUrl, internet: InternetSwitch) -> Result<Self, Door43Error> {
        Self::with_limits(server, internet, Limits::default())
    }

    pub fn with_limits(
        server: ServerUrl,
        internet: InternetSwitch,
        limits: Limits,
    ) -> Result<Self, Door43Error> {
        let origin = server.clone();
        // A redirect to another host would carry the request off the configured server;
        // only same-origin redirects are followed.
        let redirects = reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                attempt.error("too many redirects")
            } else if origin.is_same_origin(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("a redirect to another server")
            }
        });
        let builder = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(limits.connect_timeout)
            .read_timeout(limits.read_timeout)
            .redirect(redirects)
            .referer(false);
        // A loopback server is the test fake; a system proxy must not see that traffic.
        let builder = if server.is_loopback() {
            builder.no_proxy()
        } else {
            builder.https_only(true)
        };
        let http = builder.build().map_err(|e| Door43Error::Setup {
            reason: e.to_string(),
        })?;
        Ok(Self {
            http,
            server,
            internet,
            limits,
        })
    }

    pub fn server(&self) -> &ServerUrl {
        &self.server
    }

    pub fn internet(&self) -> &InternetSwitch {
        &self.internet
    }

    /// Search the catalog (`GET /api/v1/catalog/search`).
    pub async fn catalog_search(
        &self,
        query: &CatalogQuery,
    ) -> Result<Vec<CatalogEntry>, Door43Error> {
        let mut url = self.api(&["catalog", "search"]);
        let pairs = query.pairs();
        if !pairs.is_empty() {
            url.query_pairs_mut().extend_pairs(pairs);
        }
        let route = route_of(&Method::GET, &url);
        let results: RawSearchResults = self.json(Method::GET, url, Auth::Anonymous, None).await?;
        if !results.ok {
            return Err(unexpected(&route, "the search answered ok: false"));
        }
        results
            .data
            .into_iter()
            .map(RawCatalogEntry::parse)
            .collect::<Result<_, _>>()
            .map_err(|reason| unexpected(&route, reason))
    }

    /// `GET /api/v1/repos/{owner}/{repo}`. A session is needed to see a private repository.
    pub async fn repo(
        &self,
        repo: &RepoRef,
        session: Option<&Session>,
    ) -> Result<RepoInfo, Door43Error> {
        let url = self.repo_api(repo, &[]);
        let route = route_of(&Method::GET, &url);
        let raw: RawRepository = self.json(Method::GET, url, auth_of(session), None).await?;
        raw.parse().map_err(|reason| unexpected(&route, reason))
    }

    /// Whether the repository exists (and the session, if any, can see it). Only a 404
    /// counts as absence; any other failure is an error, never a "no".
    pub async fn repo_exists(
        &self,
        repo: &RepoRef,
        session: Option<&Session>,
    ) -> Result<bool, Door43Error> {
        match self.repo(repo, session).await {
            Ok(_) => Ok(true),
            Err(Door43Error::NotFound { .. }) => Ok(false),
            Err(other) => Err(other),
        }
    }

    /// Every tag, walking the pages (`GET /repos/{owner}/{repo}/tags`).
    pub async fn tags(&self, repo: &RepoRef) -> Result<Vec<Tag>, Door43Error> {
        let url = self.repo_api(repo, &["tags"]);
        let route = route_of(&Method::GET, &url);
        let raw: Vec<RawTag> = self.pages(url, Auth::Anonymous).await?;
        raw.into_iter()
            .map(RawTag::parse)
            .collect::<Result<_, _>>()
            .map_err(|reason| unexpected(&route, reason))
    }

    /// The commit a tag points at, in one call (`GET /repos/{owner}/{repo}/tags/{tag}`).
    /// This is the independent answer a first install checks a download against.
    pub async fn tag_commit(
        &self,
        repo: &RepoRef,
        tag: &TagName,
    ) -> Result<CommitSha, Door43Error> {
        let mut segments = vec!["tags"];
        segments.extend(tag.segments());
        let url = self.repo_api(repo, &segments);
        let route = route_of(&Method::GET, &url);
        let raw: RawTag = self.json(Method::GET, url, Auth::Anonymous, None).await?;
        let found = raw.parse().map_err(|reason| unexpected(&route, reason))?;
        if found.name != *tag {
            return Err(unexpected(
                &route,
                format!("asked for tag {tag}, Door43 answered tag {}", found.name),
            ));
        }
        Ok(found.commit)
    }

    /// Every release, newest first as Door43 lists them.
    pub async fn releases(&self, repo: &RepoRef) -> Result<Vec<Release>, Door43Error> {
        let url = self.repo_api(repo, &["releases"]);
        let route = route_of(&Method::GET, &url);
        let raw: Vec<RawRelease> = self.pages(url, Auth::Anonymous).await?;
        raw.into_iter()
            .map(RawRelease::parse)
            .collect::<Result<_, _>>()
            .map_err(|reason| unexpected(&route, reason))
    }

    /// The newest published release. `NotFound` when the repository has none.
    pub async fn latest_release(&self, repo: &RepoRef) -> Result<Release, Door43Error> {
        let url = self.repo_api(repo, &["releases", "latest"]);
        let route = route_of(&Method::GET, &url);
        let raw: RawRelease = self.json(Method::GET, url, Auth::Anonymous, None).await?;
        raw.parse().map_err(|reason| unexpected(&route, reason))
    }

    /// Download the repository at exactly `commit` and return its files only after they
    /// are shown to be that commit's content. See [`crate::verify`] for the checks.
    pub async fn download_archive(
        &self,
        repo: &RepoRef,
        commit: &CommitSha,
    ) -> Result<VerifiedArchive, Door43Error> {
        let tree = self.commit_tree(repo, commit).await?;
        let file = format!("{commit}.zip");
        let url = self.repo_api(repo, &["archive", &file]);
        let route = route_of(&Method::GET, &url);
        let bytes = self.download(url).await?;
        verify_archive(&bytes, commit, &tree, self.limits.max_unpacked_bytes)
            .map_err(|source| Door43Error::Verification { route, source })
    }

    /// Download the DCS Scripture Burrito export of `tag` and return it only when its
    /// metadata declares `expected` as its revision. `expected` must come from the pin or
    /// from [`Door43Client::tag_commit`], never from the zip.
    pub async fn download_sb_export(
        &self,
        repo: &RepoRef,
        tag: &TagName,
        expected: &CommitSha,
    ) -> Result<VerifiedSbExport, Door43Error> {
        let mut file: Vec<String> = tag.segments().map(str::to_owned).collect();
        if let Some(last) = file.last_mut() {
            last.push_str(".zip");
        }
        let mut segments = vec![repo.owner.as_str(), repo.name.as_str(), "sb"];
        segments.extend(file.iter().map(String::as_str));
        let url = self.server.url_with_path(segments);
        let route = route_of(&Method::GET, &url);
        let bytes = self.download(url).await?;
        verify_sb_export(&bytes, expected, self.limits.max_unpacked_bytes)
            .map_err(|source| Door43Error::Verification { route, source })
    }

    /// Sign in and mint a token named `name` with `scopes`. Door43 lists tokens without
    /// their secrets and refuses a second token with the same name, so a token with this
    /// name is deleted first. The password travels in the `Authorization` header of these
    /// calls only, and the returned token is the only thing to keep (D85).
    pub async fn mint_token(
        &self,
        credentials: &Credentials,
        name: &TokenName,
        scopes: &[TokenScope],
    ) -> Result<Session, Door43Error> {
        let basic = Auth::Basic(credentials);
        let username = self.login(basic).await?;
        let tokens_url = self.api(&["users", username.as_str(), "tokens"]);
        let existing: Vec<RawAccessToken> = self.pages(tokens_url.clone(), basic).await?;
        for token in existing.iter().filter(|t| t.name == name.as_str()) {
            let id = token.id.to_string();
            let url = self.api(&["users", username.as_str(), "tokens", &id]);
            match self.send(Method::DELETE, url, basic, JSON, None).await {
                Ok(_) | Err(Door43Error::NotFound { .. }) => {}
                Err(other) => return Err(other),
            }
        }
        let body = serde_json::json!({
            "name": name.as_str(),
            "scopes": scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        });
        let route = route_of(&Method::POST, &tokens_url);
        let created: RawCreatedToken = self
            .json(
                Method::POST,
                tokens_url,
                basic,
                Some(body.to_string().into_bytes()),
            )
            .await?;
        if created.sha1.is_empty() {
            return Err(unexpected(&route, "the created token carries no secret"));
        }
        Ok(Session {
            username,
            token: Token::new(created.sha1),
        })
    }

    /// Resume a session from a token kept in the OS keychain: ask Door43 whose token it
    /// is. A revoked or expired token fails with `AuthFailed`.
    pub async fn session_for_token(&self, token: Token) -> Result<Session, Door43Error> {
        let username = self.login(Auth::Token(&token)).await?;
        Ok(Session { username, token })
    }

    /// The user's organizations, each with whether the user may create a repository in it.
    pub async fn list_orgs(&self, session: &Session) -> Result<Vec<Organization>, Door43Error> {
        let auth = Auth::Token(&session.token);
        let url = self.api(&["user", "orgs"]);
        let route = route_of(&Method::GET, &url);
        let raw: Vec<RawOrganization> = self.pages(url, auth).await?;
        let mut orgs = Vec::with_capacity(raw.len());
        for org in raw {
            let name: AccountName = org
                .username
                .parse()
                .map_err(|e| unexpected(&route, format!("{e}")))?;
            let url = self.api(&[
                "users",
                session.username.as_str(),
                "orgs",
                name.as_str(),
                "permissions",
            ]);
            let permissions: RawOrgPermissions = self.json(Method::GET, url, auth, None).await?;
            let full_name = if org.full_name.is_empty() {
                name.to_string()
            } else {
                org.full_name
            };
            orgs.push(Organization {
                name,
                full_name,
                can_create_repository: permissions.can_create_repository,
            });
        }
        Ok(orgs)
    }

    /// Create an empty repository with a `main` default branch. 409 becomes `NameExists`.
    pub async fn create_repo(
        &self,
        session: &Session,
        owner: &NewRepoOwner,
        name: &RepoName,
        visibility: Visibility,
    ) -> Result<CreatedRepo, Door43Error> {
        let url = match owner {
            NewRepoOwner::SignedInUser => self.api(&["user", "repos"]),
            NewRepoOwner::Organization(org) => self.api(&["orgs", org.as_str(), "repos"]),
        };
        let route = route_of(&Method::POST, &url);
        // `auto_init: false`: the first push brings the project's whole history, and an
        // initial commit made by the server would turn it into a non-fast-forward.
        let body = serde_json::json!({
            "name": name.as_str(),
            "auto_init": false,
            "default_branch": "main",
            "private": visibility == Visibility::Private,
        });
        let raw: RawRepository = self
            .json(
                Method::POST,
                url,
                Auth::Token(&session.token),
                Some(body.to_string().into_bytes()),
            )
            .await?;
        raw.parse_created(&self.server)
            .map_err(|reason| unexpected(&route, reason))
    }

    async fn login(&self, auth: Auth<'_>) -> Result<AccountName, Door43Error> {
        let url = self.api(&["user"]);
        let route = route_of(&Method::GET, &url);
        let user: RawUser = self.json(Method::GET, url, auth, None).await?;
        user.login
            .parse()
            .map_err(|e| unexpected(&route, format!("the account login: {e}")))
    }

    async fn commit_tree(
        &self,
        repo: &RepoRef,
        commit: &CommitSha,
    ) -> Result<ObjectId, Door43Error> {
        let mut url = self.repo_api(repo, &["git", "commits", commit.as_str()]);
        url.query_pairs_mut()
            .append_pair("stat", "false")
            .append_pair("verification", "false")
            .append_pair("files", "false");
        let route = route_of(&Method::GET, &url);
        let raw: RawCommit = self.json(Method::GET, url, Auth::Anonymous, None).await?;
        if !raw.sha.eq_ignore_ascii_case(commit.as_str()) {
            return Err(unexpected(
                &route,
                format!("asked for commit {commit}, Door43 answered {:?}", raw.sha),
            ));
        }
        raw.commit
            .tree
            .sha
            .parse()
            .map_err(|()| unexpected(&route, "the commit's tree id is not a sha"))
    }

    async fn download(&self, url: Url) -> Result<Vec<u8>, Door43Error> {
        let (route, response) = self
            .send(Method::GET, url, Auth::Anonymous, ZIP, None)
            .await?;
        self.read_body(&route, response, self.limits.max_download_bytes)
            .await
    }

    fn api(&self, segments: &[&str]) -> Url {
        self.server
            .url_with_path(["api", "v1"].into_iter().chain(segments.iter().copied()))
    }

    fn repo_api(&self, repo: &RepoRef, rest: &[&str]) -> Url {
        let head = ["repos", repo.owner.as_str(), repo.name.as_str()];
        self.server.url_with_path(
            ["api", "v1"]
                .into_iter()
                .chain(head)
                .chain(rest.iter().copied()),
        )
    }

    /// Every page of a Gitea listing. The server may clamp `limit`, so the first page's
    /// length is taken as the real page size and a shorter or empty page ends the walk
    /// (tC4 `resourceFetch.ts` `dcsPages`).
    async fn pages<T: DeserializeOwned>(
        &self,
        base: Url,
        auth: Auth<'_>,
    ) -> Result<Vec<T>, Door43Error> {
        let mut all = Vec::new();
        let mut page_size = None;
        for page in 1..=MAX_PAGES {
            let mut url = base.clone();
            url.query_pairs_mut()
                .append_pair("limit", &PAGE_LIMIT.to_string())
                .append_pair("page", &page.to_string());
            let items: Vec<T> = self.json(Method::GET, url, auth, None).await?;
            let count = items.len();
            if count == 0 {
                return Ok(all);
            }
            all.extend(items);
            if count < *page_size.get_or_insert(count) {
                return Ok(all);
            }
        }
        Err(unexpected(
            &route_of(&Method::GET, &base),
            format!("the listing runs past {MAX_PAGES} pages"),
        ))
    }

    async fn json<T: DeserializeOwned>(
        &self,
        method: Method,
        url: Url,
        auth: Auth<'_>,
        body: Option<Vec<u8>>,
    ) -> Result<T, Door43Error> {
        let (route, response) = self.send(method, url, auth, JSON, body).await?;
        let bytes = self.read_body(&route, response, MAX_JSON_BYTES).await?;
        serde_json::from_slice(&bytes)
            .map_err(|e| unexpected(&route, format!("the answer is not the expected JSON: {e}")))
    }

    /// The only place a request leaves this crate.
    async fn send(
        &self,
        method: Method,
        url: Url,
        auth: Auth<'_>,
        accept: &'static str,
        body: Option<Vec<u8>>,
    ) -> Result<(String, reqwest::Response), Door43Error> {
        let route = route_of(&method, &url);
        if self.internet.check().is_err() {
            tracing::debug!(%route, "door43 request refused: the internet is off");
            return Err(Door43Error::Offline { route });
        }
        let headers = AuthHeaders::of(auth).map_err(|reason| Door43Error::AuthFailed {
            route: route.clone(),
            status: AuthStatus::Unauthorized,
            message: reason.to_owned(),
        })?;
        let mut request = self.http.request(method, url).header(ACCEPT, accept);
        if let Some(value) = &headers.authorization {
            request = request.header(AUTHORIZATION, value.clone());
        }
        if let Some(value) = &headers.one_time_code {
            request = request.header("X-GITEA-OTP", value.clone());
        }
        if let Some(body) = body {
            request = request.header(CONTENT_TYPE, JSON).body(body);
        }
        tracing::debug!(%route, "door43 request");
        let response = request
            .send()
            .await
            .map_err(|e| transport_error(&route, e))?;
        let status = response.status();
        tracing::debug!(%route, status = status.as_u16(), "door43 answer");
        if status.is_success() {
            return Ok((route, response));
        }
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let message = headers.scrub(message_of(&read_error_body(response).await));
        Err(classify(route, status, message, retry_after))
    }

    async fn read_body(
        &self,
        route: &str,
        mut response: reqwest::Response,
        limit: u64,
    ) -> Result<Vec<u8>, Door43Error> {
        let too_large = || Door43Error::TooLarge {
            route: route.to_owned(),
            limit,
        };
        if response.content_length().is_some_and(|len| len > limit) {
            return Err(too_large());
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| transport_error(route, e))?
        {
            if self.internet.check().is_err() {
                return Err(Door43Error::Offline {
                    route: route.to_owned(),
                });
            }
            if (body.len() + chunk.len()) as u64 > limit {
                return Err(too_large());
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }
}

fn auth_of(session: Option<&Session>) -> Auth<'_> {
    session.map_or(Auth::Anonymous, |s| Auth::Token(&s.token))
}

/// The method and path. Routes are built from checked names, so they hold no secret; the
/// query is left out.
fn route_of(method: &Method, url: &Url) -> String {
    format!("{method} {}", url.path())
}

fn unexpected(route: &str, reason: impl Into<String>) -> Door43Error {
    Door43Error::UnexpectedResponse {
        route: route.to_owned(),
        reason: reason.into(),
    }
}

fn transport_error(route: &str, error: reqwest::Error) -> Door43Error {
    if error.is_redirect() {
        return unexpected(
            route,
            "Door43 redirected the request to another server, which this client does not follow",
        );
    }
    let cause = if error.is_timeout() {
        UnreachableCause::Timeout
    } else if error.is_connect() {
        UnreachableCause::Connect
    } else if error.is_body() || error.is_decode() {
        UnreachableCause::Interrupted
    } else {
        UnreachableCause::Other(error.without_url().to_string())
    };
    Door43Error::Unreachable {
        route: route.to_owned(),
        cause,
    }
}

fn classify(
    route: String,
    status: StatusCode,
    message: String,
    retry_after: Option<Duration>,
) -> Door43Error {
    match status.as_u16() {
        401 => Door43Error::AuthFailed {
            route,
            status: AuthStatus::Unauthorized,
            message,
        },
        403 => Door43Error::AuthFailed {
            route,
            status: AuthStatus::Forbidden,
            message,
        },
        404 => Door43Error::NotFound { route, message },
        409 => Door43Error::NameExists { route, message },
        429 => Door43Error::RateLimited { route, retry_after },
        code @ 500..=599 => Door43Error::Server {
            route,
            status: code,
            message,
        },
        code => Door43Error::Rejected {
            route,
            status: code,
            message,
        },
    }
}

async fn read_error_body(mut response: reqwest::Response) -> Vec<u8> {
    let mut body = Vec::new();
    while body.len() < MAX_ERROR_BODY {
        match response.chunk().await {
            Ok(Some(chunk)) => body.extend_from_slice(&chunk),
            _ => break,
        }
    }
    body.truncate(MAX_ERROR_BODY);
    body
}

/// Gitea's error body is `{"message": "...", "url": "..."}`; anything else is shown as
/// its first 200 characters.
fn message_of(body: &[u8]) -> String {
    #[derive(Deserialize)]
    struct ApiError {
        message: Option<String>,
    }
    if let Ok(ApiError {
        message: Some(message),
    }) = serde_json::from_slice(body)
    {
        return message;
    }
    let text: String = String::from_utf8_lossy(body).chars().take(200).collect();
    if text.trim().is_empty() {
        "(no message)".to_owned()
    } else {
        text
    }
}
