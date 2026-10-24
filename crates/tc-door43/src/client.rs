use crate::catalog::{CatalogEntry, CatalogQuery, RawCatalogEntry, RawSearchResults};
use crate::download::{
    DownloadTo, DownloadWatch, DownloadedZip, STOP_CHECK_INTERVAL, Writing, disk,
};
use crate::error::{AuthStatus, Door43Error, UnreachableCause, VerificationError};
use crate::http::{Http, HttpError, HttpSpec, ProxyFinder, RedirectRule, system_proxy_finder};
use crate::internet::InternetSwitch;
use crate::listing::Listing;
use crate::names::{
    AccountName, GitOid, NewRepoName, RepoRef, ServerUrl, TagName, TokenName, TokenScope,
    Visibility,
};
use crate::repos::{
    CreatedRepo, NewRepoOwner, Organization, RawAccessToken, RawCommit, RawCreatedToken,
    RawOrgPermissions, RawOrganization, RawRelease, RawRepository, RawTag, RawUser, Release,
    RepoInfo, Tag,
};
use crate::secret::{Credentials, Session, Token};
use crate::verify::{
    VerifiedArchive, VerifiedSbExport, verify_archive_from, verify_sb_export_from,
};
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderValue, RETRY_AFTER};
use reqwest::{Method, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::fmt;
use std::fs::File;
use std::io::BufReader;
use std::time::{Duration, SystemTime};
use url::Url;

const USER_AGENT: &str = concat!("tc-door43/", env!("CARGO_PKG_VERSION"));
const JSON: &str = "application/json";
const ZIP: &str = "application/zip, application/octet-stream";
const MAX_JSON_BYTES: u64 = 64 << 20;
const MAX_ERROR_BODY: usize = 16 << 10;
/// The most of an error body an error message shows.
const MAX_MESSAGE_CHARS: usize = 200;
const REDACTED: &str = "<redacted>";
const PAGE_LIMIT: u32 = 50;
/// 40 pages of 50 is far beyond any real repository's tags or releases (tC4 uses the same
/// bound); a listing that keeps going is refused rather than walked forever.
const MAX_PAGES: u32 = 40;
const MAX_REDIRECTS: usize = 5;

/// Timeouts and size caps. A download is streamed to a file and checked from there, and
/// its unpacked files are held in memory, so `max_unpacked_bytes` bounds the memory one
/// download takes and `max_download_bytes` the disk. The defaults suit a slow field
/// connection on a low-memory laptop and leave room over the largest real resource (en_ult
/// v89 is 7.7 MB zipped); raise them for something bigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    pub connect_timeout: Duration,
    /// The longest wait for the next bytes of an answer.
    pub read_timeout: Duration,
    /// 64 MiB by default.
    pub max_download_bytes: u64,
    /// The decompressed total of one archive; 256 MiB by default.
    pub max_unpacked_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(20),
            read_timeout: Duration::from_secs(60),
            max_download_bytes: 64 << 20,
            max_unpacked_bytes: 256 << 20,
        }
    }
}

/// The one HTTP client for Door43. Every call reads the internet switch before it sends
/// anything and refuses with [`Door43Error::Offline`] while the switch is off; a download
/// in progress stops at its next chunk when the switch goes off, or within
/// [`STOP_CHECK_INTERVAL`] while it waits for one ([`crate::download`]).
///
/// Secrets go only in the `Authorization` (and `X-GITEA-OTP`) header, marked sensitive.
/// No URL carries one, and the log lines name only the method, path and status.
#[derive(Clone)]
pub struct Door43Client {
    http: Http,
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
    /// The header values were built when the token, code or credentials were made, which
    /// refused anything a header cannot carry; so this cannot fail.
    fn of(auth: Auth<'_>) -> Self {
        match auth {
            Auth::Anonymous => Self {
                authorization: None,
                one_time_code: None,
                secrets: Vec::new(),
            },
            Auth::Token(token) => Self {
                authorization: Some(token.header().clone()),
                one_time_code: None,
                secrets: forms_of(&[token.expose_secret()]),
            },
            Auth::Basic(credentials) => {
                let code = credentials.one_time_code();
                let mut raw = vec![credentials.password().expose_secret(), credentials.basic()];
                raw.extend(code.map(|c| c.expose_secret()));
                Self {
                    authorization: Some(credentials.basic_header().clone()),
                    one_time_code: code.map(|c| c.header().clone()),
                    secrets: forms_of(&raw),
                }
            }
        }
    }
}

/// Each secret as sent, and as a server might write it back: percent-encoded, or escaped
/// inside a JSON string.
fn forms_of(secrets: &[&str]) -> Vec<String> {
    let mut forms: Vec<String> = Vec::new();
    for secret in secrets.iter().filter(|s| !s.is_empty()) {
        let percent: String = url::form_urlencoded::byte_serialize(secret.as_bytes()).collect();
        let quoted = serde_json::to_string(secret).unwrap_or_default();
        let json = quoted
            .strip_prefix('"')
            .and_then(|q| q.strip_suffix('"'))
            .unwrap_or_default();
        for form in [*secret, percent.as_str(), json] {
            if !form.is_empty() && !forms.iter().any(|f| f == form) {
                forms.push(form.to_owned());
            }
        }
    }
    // Longest first, so a secret that contains another is replaced whole.
    forms.sort_by_key(|f| std::cmp::Reverse(f.len()));
    forms
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
        Self::with_proxy_finder(server, internet, limits, system_proxy_finder())
    }

    /// A client whose proxy `finder` names, asked only while the internet is on
    /// ([`crate::http`]). `None` leaves the system's own proxy settings.
    pub fn with_proxy_finder(
        server: ServerUrl,
        internet: InternetSwitch,
        limits: Limits,
        finder: Option<std::sync::Arc<dyn ProxyFinder>>,
    ) -> Result<Self, Door43Error> {
        // A redirect to another host would carry the request off the configured server;
        // only same-origin redirects are followed.
        let redirects = RedirectRule {
            origins: vec![server.as_url().clone()],
            https_hosts: Vec::new(),
            max: MAX_REDIRECTS,
        };
        let spec = HttpSpec {
            user_agent: USER_AGENT,
            connect_timeout: limits.connect_timeout,
            read_timeout: limits.read_timeout,
            redirects,
            origins: vec![server.as_url().clone()],
            // A loopback server is the test fake; a system proxy must not see that traffic.
            loopback: server.is_loopback(),
        };
        let http =
            Http::with_finder(spec, internet.clone(), finder).map_err(|e| Door43Error::Setup {
                reason: e.to_string(),
            })?;
        Ok(Self {
            http,
            server,
            internet,
            limits,
        })
    }

    /// The gate this client sends through, for sync's git calls ([`crate::git`]).
    pub fn http(&self) -> &Http {
        &self.http
    }

    pub fn server(&self) -> &ServerUrl {
        &self.server
    }

    pub fn internet(&self) -> &InternetSwitch {
        &self.internet
    }

    /// Search the catalog (`GET /api/v1/catalog/search`). An entry this client cannot read
    /// is set aside in [`Listing::skipped`], so it never hides the others.
    pub async fn catalog_search(
        &self,
        query: &CatalogQuery,
    ) -> Result<Listing<CatalogEntry>, Door43Error> {
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
        Ok(Listing::from_values(
            &route,
            results.data,
            RawCatalogEntry::label,
            RawCatalogEntry::parse,
        ))
    }

    /// `GET /api/v1/repos/{owner}/{repo}`. A session is needed to see a private repository.
    /// An answer whose page or clone address is not on this client's server is refused.
    pub async fn repo(
        &self,
        repo: &RepoRef,
        session: Option<&Session>,
    ) -> Result<RepoInfo, Door43Error> {
        let url = self.repo_api(repo, &[]);
        let route = route_of(&Method::GET, &url);
        let raw: RawRepository = self.json(Method::GET, url, auth_of(session), None).await?;
        raw.parse(&self.server)
            .map_err(|reason| unexpected(&route, reason))
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

    /// Every tag, walking the pages (`GET /repos/{owner}/{repo}/tags`). A tag this client
    /// cannot read is set aside in [`Listing::skipped`].
    pub async fn tags(&self, repo: &RepoRef) -> Result<Listing<Tag>, Door43Error> {
        let url = self.repo_api(repo, &["tags"]);
        let route = route_of(&Method::GET, &url);
        let raw: Vec<Value> = self.pages(url, Auth::Anonymous).await?;
        Ok(Listing::from_values(
            &route,
            raw,
            RawTag::label,
            RawTag::parse,
        ))
    }

    /// The commit a tag points at, in one call (`GET /repos/{owner}/{repo}/tags/{tag}`).
    /// This is the independent answer a first install checks a download against.
    pub async fn tag_commit(&self, repo: &RepoRef, tag: &TagName) -> Result<GitOid, Door43Error> {
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

    /// Every release, newest first as Door43 lists them. A release this client cannot read
    /// is set aside in [`Listing::skipped`].
    pub async fn releases(&self, repo: &RepoRef) -> Result<Listing<Release>, Door43Error> {
        let url = self.repo_api(repo, &["releases"]);
        let route = route_of(&Method::GET, &url);
        let raw: Vec<Value> = self.pages(url, Auth::Anonymous).await?;
        Ok(Listing::from_values(
            &route,
            raw,
            RawRelease::label,
            RawRelease::parse,
        ))
    }

    /// The newest published release. `NotFound` when the repository has none.
    pub async fn latest_release(&self, repo: &RepoRef) -> Result<Release, Door43Error> {
        let url = self.repo_api(repo, &["releases", "latest"]);
        let route = route_of(&Method::GET, &url);
        let raw: RawRelease = self.json(Method::GET, url, Auth::Anonymous, None).await?;
        raw.parse().map_err(|reason| unexpected(&route, reason))
    }

    /// Download the repository at exactly `commit` and return its files only after they
    /// are shown to be that commit's content. See [`crate::verify`] for the checks, and
    /// [`crate::download`] for how the bytes travel and how the download is stopped.
    pub async fn download_archive(
        &self,
        repo: &RepoRef,
        commit: &GitOid,
        to: DownloadTo<'_>,
    ) -> Result<VerifiedArchive, Door43Error> {
        Ok(self.download_archive_zip(repo, commit, to).await?.1)
    }

    /// [`Door43Client::download_archive`], also returning the zip as it was downloaded,
    /// for a caller that records which download its files came from (a repackaged
    /// lexicon names its archive's SHA-256). The zip stays on disk until it is dropped.
    pub async fn download_archive_zip(
        &self,
        repo: &RepoRef,
        commit: &GitOid,
        to: DownloadTo<'_>,
    ) -> Result<(DownloadedZip, VerifiedArchive), Door43Error> {
        let file = format!("{commit}.zip");
        let url = self.repo_api(repo, &["archive", &file]);
        let route = route_of(&Method::GET, &url);
        // Before the commit lookup, so a cancelled download sends nothing.
        self.check_stop(&route, to.watch)?;
        // The lookup is part of the download: a stall here must not hold a Cancel either.
        let looked_up = self.commit_tree(repo, commit);
        let tree = self.unless_stopped(&route, to.watch, looked_up).await??;
        let (route, zip) = self.download(url, to).await?;
        let (commit, max) = (*commit, self.limits.max_unpacked_bytes);
        let archive = check_in_background(&route, &zip, move |reader| {
            verify_archive_from(reader, &commit, &tree, max)
        })
        .await?;
        self.check_stop(&route, to.watch)?;
        Ok((zip, archive))
    }

    /// Download the DCS Scripture Burrito export of `tag` and return it only when its
    /// metadata declares `expected` as its revision. `expected` must come from the pin or
    /// from [`Door43Client::tag_commit`], never from the zip. The zip is removed before
    /// this returns.
    pub async fn download_sb_export(
        &self,
        repo: &RepoRef,
        tag: &TagName,
        expected: &GitOid,
        to: DownloadTo<'_>,
    ) -> Result<VerifiedSbExport, Door43Error> {
        let mut file: Vec<String> = tag.segments().map(str::to_owned).collect();
        if let Some(last) = file.last_mut() {
            last.push_str(".zip");
        }
        let mut segments = vec![repo.owner.as_str(), repo.name.as_str(), "sb"];
        segments.extend(file.iter().map(String::as_str));
        let url = self.server.url_with_path(segments);
        let (route, zip) = self.download(url, to).await?;
        let (expected, max) = (*expected, self.limits.max_unpacked_bytes);
        let export = check_in_background(&route, &zip, move |reader| {
            verify_sb_export_from(reader, &expected, max)
        })
        .await?;
        self.check_stop(&route, to.watch)?;
        Ok(export)
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
        // The error leaves the value out: it is the secret.
        let token = Token::new(created.sha1)
            .map_err(|_| unexpected(&route, "the created token is empty or not sendable"))?;
        Ok(Session { username, token })
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

    /// How many repositories `owner` has in the language `language` (APP-DESIGN R12, tC4
    /// `countRepositories`): Door43's `X-Total-Count` of a search for them, with `limit=1` so
    /// no listing is read. An answer without the header, or with one that is not a count, is
    /// unexpected.
    pub async fn count_repositories(
        &self,
        session: Option<&Session>,
        owner: &AccountName,
        language: &str,
    ) -> Result<u64, Door43Error> {
        let mut url = self.api(&["repos", "search"]);
        url.query_pairs_mut()
            .append_pair("owner", owner.as_str())
            .append_pair("lang", language)
            .append_pair("limit", "1");
        let (route, response) = self
            .send(Method::GET, url, auth_of(session), JSON, None)
            .await?;
        let count = response
            .headers()
            .get("x-total-count")
            .and_then(|v| v.to_str().ok())
            .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|v| v.parse::<u64>().ok());
        self.read_body(&route, response, MAX_JSON_BYTES).await?;
        count.ok_or_else(|| unexpected(&route, "the answer carries no X-Total-Count"))
    }

    /// Create an empty repository with a `main` default branch. 409 becomes `NameExists`.
    pub async fn create_repo(
        &self,
        session: &Session,
        owner: &NewRepoOwner,
        name: &NewRepoName,
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

    async fn commit_tree(&self, repo: &RepoRef, commit: &GitOid) -> Result<GitOid, Door43Error> {
        let mut url = self.repo_api(repo, &["git", "commits", &commit.to_hex()]);
        url.query_pairs_mut()
            .append_pair("stat", "false")
            .append_pair("verification", "false")
            .append_pair("files", "false");
        let route = route_of(&Method::GET, &url);
        let raw: RawCommit = self.json(Method::GET, url, Auth::Anonymous, None).await?;
        if !raw.sha.eq_ignore_ascii_case(&commit.to_hex()) {
            return Err(unexpected(
                &route,
                format!("asked for commit {commit}, Door43 answered {:?}", raw.sha),
            ));
        }
        GitOid::parse_any_case(&raw.commit.tree.sha)
            .ok_or_else(|| unexpected(&route, "the commit's tree id is not a sha"))
    }

    /// GET `url` into a new file in `to`'s folder. See [`crate::download`].
    async fn download(
        &self,
        url: Url,
        to: DownloadTo<'_>,
    ) -> Result<(String, DownloadedZip), Door43Error> {
        let route = route_of(&Method::GET, &url);
        self.check_stop(&route, to.watch)?;
        let sent = self.send(Method::GET, url, Auth::Anonymous, ZIP, None);
        let (route, mut response) = self.unless_stopped(&route, to.watch, sent).await??;
        let limit = self.limits.max_download_bytes;
        let too_large = || Door43Error::TooLarge {
            route: route.clone(),
            limit,
        };
        let announced = response.content_length();
        if announced.is_some_and(|len| len > limit) {
            return Err(too_large());
        }
        let mut writing = Writing::create(to.folder, &route)?;
        let mut received: u64 = 0;
        loop {
            let next = self.unless_stopped(&route, to.watch, response.chunk());
            let Some(chunk) = next.await?.map_err(|e| transport_error(&route, e))? else {
                break;
            };
            self.check_stop(&route, to.watch)?;
            received += chunk.len() as u64;
            if received > limit {
                return Err(too_large());
            }
            writing.write(&route, &chunk)?;
            to.watch.received(received, announced);
        }
        let zip = writing.finish(&route)?;
        Ok((route, zip))
    }

    /// `Err` when the internet switch is off ([`Door43Error::Offline`]) or the watch has
    /// cancelled ([`Door43Error::Cancelled`]).
    fn check_stop(&self, route: &str, watch: &dyn DownloadWatch) -> Result<(), Door43Error> {
        if self.internet.check().is_err() {
            tracing::debug!(%route, "door43 download stopped: the internet is off");
            return Err(Door43Error::Offline {
                route: route.to_owned(),
            });
        }
        if watch.cancelled() {
            tracing::debug!(%route, "door43 download cancelled");
            return Err(Door43Error::Cancelled {
                route: route.to_owned(),
            });
        }
        Ok(())
    }

    /// Waits for `future`, and every [`STOP_CHECK_INTERVAL`] meanwhile asks
    /// [`Door43Client::check_stop`]; a stop drops the future, which ends its request.
    async fn unless_stopped<F: Future>(
        &self,
        route: &str,
        watch: &dyn DownloadWatch,
        future: F,
    ) -> Result<F::Output, Door43Error> {
        let mut future = std::pin::pin!(future);
        loop {
            match tokio::time::timeout(STOP_CHECK_INTERVAL, future.as_mut()).await {
                Ok(output) => return Ok(output),
                Err(_) => self.check_stop(route, watch)?,
            }
        }
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
        let Ok(http) = self.http.client().await else {
            tracing::debug!(%route, "door43 request refused: the internet is off");
            return Err(Door43Error::Offline { route });
        };
        let headers = AuthHeaders::of(auth);
        let mut request = http.request(method, url).header(ACCEPT, accept);
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
            .and_then(|v| retry_after(v, SystemTime::now()));
        let (body, cut) = read_error_body(response).await;
        let message = message_of(&body, cut, &headers.secrets);
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

/// Runs a check of the downloaded zip on the blocking pool: it reads the file and unpacks
/// and hashes every entry, which would hold up the async tasks for as long as it takes.
async fn check_in_background<T: Send + 'static>(
    route: &str,
    zip: &DownloadedZip,
    check: impl FnOnce(BufReader<File>) -> Result<T, VerificationError> + Send + 'static,
) -> Result<T, Door43Error> {
    let (owned, path) = (route.to_owned(), zip.path().to_owned());
    let task = tokio::task::spawn_blocking(move || {
        let file = File::open(&path).map_err(|e| disk(&owned, &path, e))?;
        check(BufReader::new(file)).map_err(|source| Door43Error::Verification {
            route: owned,
            source,
        })
    });
    match task.await {
        Ok(checked) => checked,
        // A panic in the check is raised here, as if it had run on this task.
        Err(e) if e.is_panic() => std::panic::resume_unwind(e.into_panic()),
        // The runtime is shutting down.
        Err(_) => Err(Door43Error::Cancelled {
            route: route.to_owned(),
        }),
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
    let cause = match HttpError::from(error) {
        HttpError::Redirect => {
            return unexpected(
                route,
                "Door43 redirected the request to another server, which this client does not follow",
            );
        }
        HttpError::Timeout => UnreachableCause::Timeout,
        HttpError::Connect => UnreachableCause::Connect,
        HttpError::Interrupted => UnreachableCause::Interrupted,
        other => UnreachableCause::Other(other.to_string()),
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

/// `Retry-After` in seconds, or as an HTTP date measured from `now` (zero once passed).
fn retry_after(value: &str, now: SystemTime) -> Option<Duration> {
    let value = value.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let at = httpdate::parse_http_date(value).ok()?;
    Some(at.duration_since(now).unwrap_or(Duration::ZERO))
}

/// The first `MAX_ERROR_BODY` bytes, and whether there was more.
async fn read_error_body(mut response: reqwest::Response) -> (Vec<u8>, bool) {
    let mut body = Vec::new();
    while body.len() <= MAX_ERROR_BODY {
        match response.chunk().await {
            Ok(Some(chunk)) => body.extend_from_slice(&chunk),
            _ => break,
        }
    }
    let cut = body.len() > MAX_ERROR_BODY;
    body.truncate(MAX_ERROR_BODY);
    (body, cut)
}

/// The message of an error body, with every form of `secrets` taken out. Gitea's error
/// body is `{"message": "...", "url": "..."}`; anything else is shown as its first
/// `MAX_MESSAGE_CHARS` characters. `cut` says the body was cut short before it got here.
fn message_of(body: &[u8], cut: bool, secrets: &[String]) -> String {
    #[derive(Deserialize)]
    struct ApiError {
        message: Option<String>,
    }
    let text = match serde_json::from_slice(body) {
        Ok(ApiError {
            message: Some(message),
        }) => redact(&message, false, secrets),
        _ => redact(&String::from_utf8_lossy(body), cut, secrets),
    };
    if text.trim().is_empty() {
        "(no message)".to_owned()
    } else {
        text
    }
}

/// `text` without `secrets`, shortened to `MAX_MESSAGE_CHARS`. Each secret is replaced
/// before the text is shortened, so the cut never leaves part of one. A text that was cut,
/// here or before (`cut`), also loses a tail that begins a secret, since the rest of that
/// secret may have been past the cut.
fn redact(text: &str, cut: bool, secrets: &[String]) -> String {
    let mut whole = text.to_owned();
    for secret in secrets.iter().filter(|s| !s.is_empty()) {
        whole = replace_ignoring_ascii_case(&whole, secret, REDACTED);
    }
    let mut shown: String = whole.chars().take(MAX_MESSAGE_CHARS).collect();
    if cut || shown.len() < whole.len() {
        let tail = secrets
            .iter()
            .map(|secret| secret_prefix_at_end(&shown, secret))
            .max()
            .unwrap_or(0);
        shown.truncate(shown.len() - tail);
        shown.push('…');
    }
    shown
}

/// Every occurrence of `needle` in `haystack`, matched without regard to ASCII case (a
/// server may print a hex token in capitals), replaced by `with`.
fn replace_ignoring_ascii_case(haystack: &str, needle: &str, with: &str) -> String {
    let lower_haystack = haystack.to_ascii_lowercase();
    let lower_needle = needle.to_ascii_lowercase();
    let mut out = String::with_capacity(haystack.len());
    let mut from = 0;
    // ASCII lowercasing keeps every byte offset, so offsets found in the lowercase copy are
    // offsets in `haystack`, on the same character boundaries.
    while let Some(found) = lower_haystack[from..].find(&lower_needle) {
        let at = from + found;
        out.push_str(&haystack[from..at]);
        out.push_str(with);
        from = at + needle.len();
    }
    out.push_str(&haystack[from..]);
    out
}

/// The length in bytes of the longest end of `text` that starts `secret`.
fn secret_prefix_at_end(text: &str, secret: &str) -> usize {
    let lower_text = text.to_ascii_lowercase();
    let lower_secret = secret.to_ascii_lowercase();
    (1..=lower_text.len().min(lower_secret.len()))
        .rev()
        .find(|&len| {
            lower_secret.is_char_boundary(len)
                && lower_text.is_char_boundary(lower_text.len() - len)
                && lower_text.ends_with(&lower_secret[..len])
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "S3CRETTOKENVALUE0123456789abcdefghijklmn";

    fn secrets() -> Vec<String> {
        forms_of(&[TOKEN, "pässwörd \"x\"", "dHJhbnNsYXRvcjpjb3JyZWN0"])
    }

    #[test]
    fn a_secret_across_the_cut_leaves_no_part_behind() {
        for offset in 150..=205 {
            let body = format!("{}token {TOKEN} trailing words", "x".repeat(offset));
            let shown = message_of(body.as_bytes(), false, &secrets());
            for len in 4..=TOKEN.len() {
                assert!(!shown.contains(&TOKEN[..len]), "offset {offset}: {shown}");
            }
            assert!(shown.chars().count() <= MAX_MESSAGE_CHARS + 1, "{shown}");
        }
    }

    #[test]
    fn a_body_cut_before_it_arrived_loses_a_trailing_secret_start() {
        let body = format!("{}{}", "y".repeat(20), &TOKEN[..12]);
        let shown = message_of(body.as_bytes(), true, &secrets());
        assert_eq!(shown, format!("{}…", "y".repeat(20)));
    }

    #[test]
    fn json_messages_and_their_escaped_forms_are_scrubbed() {
        let body = serde_json::json!({ "message": format!("bad pässwörd \"x\" and {TOKEN}") });
        let shown = message_of(body.to_string().as_bytes(), false, &secrets());
        assert_eq!(shown, "bad <redacted> and <redacted>");
        let html = format!(
            "<p>value=p%C3%A4ssw%C3%B6rd+%22x%22 {}</p>",
            TOKEN.to_lowercase()
        );
        let shown = message_of(html.as_bytes(), false, &secrets());
        assert_eq!(shown, "<p>value=<redacted> <redacted></p>");
    }

    #[test]
    fn short_messages_without_secrets_are_kept_whole() {
        assert_eq!(
            message_of(b"plain failure", false, &secrets()),
            "plain failure"
        );
        assert_eq!(message_of(b"  ", false, &secrets()), "(no message)");
        let long = "z".repeat(300);
        assert_eq!(
            message_of(long.as_bytes(), false, &[]),
            format!("{}…", "z".repeat(MAX_MESSAGE_CHARS))
        );
    }

    #[test]
    fn retry_after_takes_seconds_or_an_http_date() {
        let now = httpdate::parse_http_date("Wed, 21 Oct 2026 07:28:00 GMT").unwrap();
        assert_eq!(retry_after("7", now), Some(Duration::from_secs(7)));
        assert_eq!(
            retry_after("Wed, 21 Oct 2026 07:30:00 GMT", now),
            Some(Duration::from_secs(120))
        );
        assert_eq!(
            retry_after("Wed, 21 Oct 2026 07:00:00 GMT", now),
            Some(Duration::ZERO)
        );
        assert_eq!(retry_after("soon", now), None);
    }

    #[test]
    fn the_default_limits_bound_one_download_in_memory() {
        let limits = Limits::default();
        assert!(limits.max_download_bytes + limits.max_unpacked_bytes <= 320 << 20);
        // Room over the largest real resource export (en_ult v89, 7,709,900 bytes).
        assert!(limits.max_download_bytes >= 8 * 7_709_900);
    }
}
