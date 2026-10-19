//! A fake Door43 (DCS, a Gitea fork) server for tests. Nothing in it touches the network:
//! it listens on `127.0.0.1` on a free port and keeps its repositories in a temporary
//! folder that is removed when the [`FakeDoor43`] is dropped.
//!
//! The answers follow the DCS swagger (`swagger.v1.json`, `1.23.7+dcs`) in shape. Their
//! values come from the fake's own state and real bare git repositories, so tags,
//! commits, trees and archives are what git itself produces, as they are behind Gitea.
//! [`FakeDoor43::start_seeded`] fills it with real resource files from `fixtures/tc4`
//! (see [`seeded`]). There are no recorded live Door43 answers to replay yet; when there
//! are, tests should check this fake against them.
//!
//! Served routes:
//!
//! - API: `GET /api/v1/catalog/search`, `GET /api/v1/catalog/entry/{owner}/{repo}/{ref}`,
//!   `GET /api/v1/user`, `GET /api/v1/user/orgs`, `POST /api/v1/user/repos`,
//!   `POST /api/v1/orgs/{org}/repos`, `GET|POST /api/v1/users/{user}/tokens`,
//!   `DELETE /api/v1/users/{user}/tokens/{id-or-name}`,
//!   `GET /api/v1/users/{user}/orgs/{org}/permissions`, `GET /api/v1/repos/{owner}/{repo}`,
//!   `.../tags`, `.../tags/{tag}`, `.../releases`, `.../releases/latest`,
//!   `.../git/commits/{ref}`, `.../archive/{ref}.zip|.tar.gz`.
//! - Web: `GET /{owner}/{repo}/archive/{ref}.zip`, `GET /{owner}/{repo}/sb/{tag}.zip` (the
//!   burrito export, for repositories that already hold a `metadata.json`; the fake does
//!   not convert Resource Containers).
//! - Git smart HTTP for clone, fetch and push: `/{owner}/{repo}.git/info/refs`,
//!   `.../git-upload-pack`, `.../git-receive-pack`, served by `git http-backend` as CGI.
//!   Push needs Basic auth with the password or a token with `write:repository` (else 401, or
//!   403 for a token without the scope). As on Door43, an account that may read the
//!   repository but not write to it gets the advertisement and an HTTP 200 whose report
//!   declines every ref, from a pre-receive hook that prints what Gitea's prints
//!   (docs/evidence/door43-live-2026-10-17.md, step 6). A private repository is cloned,
//!   fetched or pushed to only by its owner or an organization member, with the password or
//!   a token with the scope; anyone else gets 404, as from Gitea.
//!
//! Auth works as Gitea's does: `Authorization: token <t>` or `Bearer <t>`, or Basic with a
//! password or a token as the password, plus `X-GITEA-OTP` for an account with a one-time
//! code. A `token` or `access_token` query parameter is refused with 400, so a client that
//! puts a token in a URL fails loudly. Every request is recorded (method, path, query and
//! the kind of credential, never its value) for tests to inspect.
//!
//! [`FakeProxy`] is a proxy that refuses every request and records its first line, for
//! tests of which proxy a client uses.
//!
//! The binary `tc-door43-fake` runs one as a process, seeded with the parity account of
//! [`seeded::parity_account`], for the browser tests of the app. It prints one line,
//! `{"url": …, "control": …}`, and serves until its stdin closes or reads `{"quit":true}`.
//! `control` is a second loopback server the tests call: `GET /requests`,
//! `POST /requests/clear`, `POST /faults`, `POST /release`, `POST /tokens`,
//! `GET /tokens/{login}` and `GET /secret/{login}/{name}` (see the binary's source).
//!
//! The fake needs `git` in `PATH`, with `git http-backend` (part of git's core tools).

// The `Repository` answer has about 80 fields, past the `json!` macro's default depth.
#![recursion_limit = "256"]

mod api;
mod git;
mod json;
pub mod proxy;
pub mod seeded;
mod smart_http;
mod zipping;

pub use git::SeedMode;
pub use proxy::{FakePacScript, FakeProxy};

use git::Git;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::oneshot;

#[derive(Debug, thiserror::Error)]
pub enum FakeError {
    #[error("git is not installed or not in PATH; the fake Door43 needs git and git-http-backend")]
    GitMissing,
    #[error("git {args} failed: {stderr}")]
    Git { args: String, stderr: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(String),
}

#[derive(Debug, Clone)]
pub struct FakeUser {
    pub login: String,
    pub email: String,
    pub password: String,
    /// When set, a password sign-in also needs this code in `X-GITEA-OTP`.
    pub one_time_code: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FakeOrg {
    pub name: String,
    pub full_name: String,
    pub members: Vec<OrgMember>,
}

#[derive(Debug, Clone)]
pub struct OrgMember {
    pub login: String,
    pub can_create_repository: bool,
}

/// A repository to create, with its history. Each commit is a whole snapshot: its tree
/// holds exactly its files.
#[derive(Debug, Clone)]
pub struct RepoSeed {
    pub owner: String,
    pub name: String,
    /// The DCS subject, such as `TSV Translation Notes`.
    pub subject: String,
    pub language: String,
    pub topics: Vec<String>,
    pub private: bool,
    pub commits: Vec<CommitSeed>,
}

#[derive(Debug, Clone)]
pub struct CommitSeed {
    pub message: String,
    pub files: Vec<SeedFile>,
    /// A lightweight tag on this commit.
    pub tag: Option<String>,
    /// A release for the tag. Ignored without a tag.
    pub release: Option<ReleaseKind>,
}

#[derive(Debug, Clone)]
pub struct SeedFile {
    pub path: String,
    pub mode: SeedMode,
    pub bytes: Vec<u8>,
}

impl SeedFile {
    pub fn regular(path: &str, bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            path: path.to_owned(),
            mode: SeedMode::Regular,
            bytes: bytes.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseKind {
    Published,
    PreRelease,
    Draft,
}

/// A misbehavior to inject for requests whose path starts with `path_prefix`.
#[derive(Debug, Clone)]
pub struct FaultRule {
    /// `None` matches every method.
    pub method: Option<String>,
    pub path_prefix: String,
    /// How many matching requests get the fault; `None` is every one.
    pub times: Option<usize>,
    pub fault: Fault,
}

impl FaultRule {
    pub fn once(method: &str, path_prefix: &str, fault: Fault) -> Self {
        Self {
            method: Some(method.to_owned()),
            path_prefix: path_prefix.to_owned(),
            times: Some(1),
            fault,
        }
    }

    pub fn always(method: &str, path_prefix: &str, fault: Fault) -> Self {
        Self {
            method: Some(method.to_owned()),
            path_prefix: path_prefix.to_owned(),
            times: None,
            fault,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    /// Answer with this status and a Gitea error body.
    Status { status: u16, message: String },
    /// 429 with `Retry-After`.
    RateLimited { retry_after_secs: u64 },
    /// 400 whose message repeats the request's `Authorization` header, as a careless
    /// server might.
    EchoAuthorization,
    /// 400 with a `text/html` body (not Gitea's JSON) of `filler` characters followed by
    /// the request's `Authorization` header, as a careless proxy might answer.
    EchoAuthorizationAsText { filler: usize },
    /// Answer with exactly this status, these headers and this body, for answers the fake
    /// does not otherwise give: an odd catalog entry, another tag than the one asked for,
    /// a `Retry-After` date.
    Raw {
        status: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    },
    /// Wait, then answer normally.
    Delay(Duration),
    /// 302 to this address.
    RedirectTo(String),
    /// The archive's zip comment says this instead of the commit id.
    ArchiveComment(String),
    /// The archive also holds this file (under the archive's top folder).
    ArchiveExtraFile { path: String, bytes: Vec<u8> },
    /// The burrito export's metadata declares this revision.
    SbRevision(String),
    /// The burrito export holds altered bytes for this path (relative to the burrito
    /// root), so it no longer matches its metadata.
    SbCorruptFile { path: String },
    /// A download (an archive or a burrito export) sends its first `after` bytes, then
    /// nothing more until the test calls [`FakeDoor43::release_held`], then the rest. A
    /// test can act between two chunks of a download and know which bytes arrived. Each
    /// held download waits for a release of its own: see [`FakeDoor43::release_held`].
    HoldBody { after: usize },
    /// A download sends its first `after` bytes and then the connection breaks, though its
    /// `Content-Length` promised more.
    BreakOff { after: usize },
    /// A download comes without `Content-Length`, as an answer built while it is sent
    /// does.
    NoContentLength,
    /// The request waits until the test calls [`FakeDoor43::release_held`], then is answered
    /// normally (with the next fault that applies, if any). A test can act while a client
    /// waits for Door43's answer. Each held request waits for a release of its own, as a held
    /// download does.
    Hold,
}

impl Fault {
    fn answers_instead(&self) -> bool {
        !matches!(
            self,
            Fault::ArchiveComment(_)
                | Fault::ArchiveExtraFile { .. }
                | Fault::SbRevision(_)
                | Fault::SbCorruptFile { .. }
                | Fault::HoldBody { .. }
                | Fault::BreakOff { .. }
                | Fault::NoContentLength
        )
    }
}

/// The kind of credential a request carried. The value is never recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSeen {
    None,
    Basic { login: String },
    Token,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub authorization: AuthSeen,
}

/// A token as the fake holds it, without its secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeToken {
    pub id: u64,
    pub name: String,
    pub scopes: Vec<String>,
}

pub(crate) struct UserRec {
    pub id: u64,
    pub user: FakeUser,
    pub tokens: Vec<TokenRec>,
}

#[derive(Clone)]
pub(crate) struct TokenRec {
    pub id: u64,
    pub name: String,
    pub sha1: String,
    pub scopes: Vec<String>,
}

pub(crate) struct OrgRec {
    pub id: u64,
    pub org: FakeOrg,
}

#[derive(Clone)]
pub(crate) struct RepoRec {
    pub id: u64,
    pub owner: String,
    pub name: String,
    pub description: String,
    pub subject: String,
    pub language: String,
    pub topics: Vec<String>,
    pub private: bool,
    pub default_branch: String,
    pub created_unix: i64,
    pub releases: Vec<ReleaseRec>,
}

#[derive(Clone)]
pub(crate) struct ReleaseRec {
    pub id: u64,
    pub tag: String,
    pub kind: ReleaseKind,
    pub created_unix: i64,
}

pub(crate) struct State {
    pub users: Vec<UserRec>,
    pub orgs: Vec<OrgRec>,
    pub repos: Vec<RepoRec>,
    pub faults: Vec<FaultRule>,
    pub requests: Vec<RecordedRequest>,
    pub max_page_size: usize,
    next_id: u64,
    clock: i64,
    pub metadata_cache: HashMap<String, Option<serde_json::Value>>,
}

/// 2026-01-01T00:00:00Z. Seeded commits are an hour apart from here, so their shas are
/// the same on every run.
const CLOCK_START: i64 = 1_767_225_600;

impl State {
    fn new() -> Self {
        Self {
            users: Vec::new(),
            orgs: Vec::new(),
            repos: Vec::new(),
            faults: Vec::new(),
            requests: Vec::new(),
            max_page_size: 50,
            next_id: 1,
            clock: CLOCK_START,
            metadata_cache: HashMap::new(),
        }
    }

    pub fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn tick(&mut self) -> i64 {
        let now = self.clock;
        self.clock += 3600;
        now
    }

    pub fn user(&self, login: &str) -> Option<&UserRec> {
        self.users
            .iter()
            .find(|u| u.user.login.eq_ignore_ascii_case(login))
    }

    pub fn user_mut(&mut self, login: &str) -> Option<&mut UserRec> {
        self.users
            .iter_mut()
            .find(|u| u.user.login.eq_ignore_ascii_case(login))
    }

    pub fn org(&self, name: &str) -> Option<&OrgRec> {
        self.orgs
            .iter()
            .find(|o| o.org.name.eq_ignore_ascii_case(name))
    }

    pub fn repo(&self, owner: &str, name: &str) -> Option<&RepoRec> {
        self.repos
            .iter()
            .find(|r| r.owner.eq_ignore_ascii_case(owner) && r.name.eq_ignore_ascii_case(name))
    }

    pub fn repo_mut(&mut self, owner: &str, name: &str) -> Option<&mut RepoRec> {
        self.repos
            .iter_mut()
            .find(|r| r.owner.eq_ignore_ascii_case(owner) && r.name.eq_ignore_ascii_case(name))
    }

    /// The account's name with its own casing.
    pub fn account_name(&self, name: &str) -> Option<String> {
        self.user(name)
            .map(|u| u.user.login.clone())
            .or_else(|| self.org(name).map(|o| o.org.name.clone()))
    }

    /// Whether `login` may push to or create in `owner`.
    pub fn can_write(&self, login: &str, owner: &str) -> bool {
        login.eq_ignore_ascii_case(owner)
            || self.org(owner).is_some_and(|o| {
                o.org
                    .members
                    .iter()
                    .any(|m| m.login.eq_ignore_ascii_case(login) && m.can_create_repository)
            })
    }

    /// Whether `login` may read `owner`'s private repositories.
    pub fn is_member(&self, login: &str, owner: &str) -> bool {
        login.eq_ignore_ascii_case(owner)
            || self.org(owner).is_some_and(|o| {
                o.org
                    .members
                    .iter()
                    .any(|m| m.login.eq_ignore_ascii_case(login))
            })
    }

    pub fn new_token(&mut self, login: &str, name: &str, scopes: Vec<String>) -> Option<TokenRec> {
        let id = self.next_id();
        let sha1 = token_secret(id, login, self.clock);
        let token = TokenRec {
            id,
            name: name.to_owned(),
            sha1,
            scopes,
        };
        self.user_mut(login)?.tokens.push(token.clone());
        Some(token)
    }
}

/// A 40-hex token. Unpredictability does not matter in a test server; uniqueness does.
fn token_secret(id: u64, login: &str, clock: i64) -> String {
    use sha1::{Digest, Sha1};
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let digest = Sha1::digest(format!("fake-token:{id}:{login}:{clock}:{nanos}").as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) struct Shared {
    state: Mutex<State>,
    pub git: Git,
    pub url: String,
    /// The releases of the requests [`Fault::Hold`] holds and the downloads
    /// [`Fault::HoldBody`] holds, oldest first: one per request, so a release can never pass
    /// to one that has not started yet.
    held: Mutex<VecDeque<oneshot::Sender<()>>>,
}

impl Shared {
    pub fn state(&self) -> MutexGuard<'_, State> {
        // A test that panicked while holding the lock must not hide the state from the
        // assertions that follow.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A new held download's release, which ends its wait when it fires or is dropped.
    pub fn hold(&self) -> oneshot::Receiver<()> {
        let (release, released) = oneshot::channel();
        self.held().push_back(release);
        released
    }

    fn held(&self) -> MutexGuard<'_, VecDeque<oneshot::Sender<()>>> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The faults that apply to this request, consuming one use of each.
    pub fn take_faults(&self, method: &str, path: &str, answers_instead: bool) -> Vec<Fault> {
        let mut state = self.state();
        let mut taken = Vec::new();
        state.faults.retain_mut(|rule| {
            let matches = rule
                .method
                .as_deref()
                .is_none_or(|m| m.eq_ignore_ascii_case(method))
                && path.starts_with(&rule.path_prefix)
                && rule.fault.answers_instead() == answers_instead;
            if !matches {
                return true;
            }
            taken.push(rule.fault.clone());
            match &mut rule.times {
                Some(times) => {
                    *times = times.saturating_sub(1);
                    *times > 0
                }
                None => true,
            }
        });
        taken
    }
}

/// A running fake Door43. Dropping it stops the server and deletes its repositories.
pub struct FakeDoor43 {
    shared: Arc<Shared>,
    shutdown: Option<oneshot::Sender<()>>,
    _dir: TempDir,
}

impl FakeDoor43 {
    /// An empty server: no users, organizations or repositories.
    pub async fn start() -> Result<Self, FakeError> {
        let dir = tempfile::Builder::new().prefix("door43-fake-").tempdir()?;
        let git = Git::new(dir.path()).await?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let shared = Arc::new(Shared {
            state: Mutex::new(State::new()),
            git,
            url,
            held: Mutex::new(VecDeque::new()),
        });
        let app = api::router(shared.clone());
        let (shutdown, stopped) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await;
        });
        Ok(Self {
            shared,
            shutdown: Some(shutdown),
            _dir: dir,
        })
    }

    /// A server holding the [`seeded`] users, organizations and resource repositories.
    pub async fn start_seeded() -> Result<Self, FakeError> {
        let fake = Self::start().await?;
        seeded::seed(&fake).await?;
        Ok(fake)
    }

    /// `http://127.0.0.1:<port>`, with no trailing slash.
    pub fn url(&self) -> &str {
        &self.shared.url
    }

    /// The smart HTTP clone address of a repository.
    pub fn clone_url(&self, owner: &str, name: &str) -> String {
        format!("{}/{owner}/{name}.git", self.shared.url)
    }

    pub fn add_user(&self, user: FakeUser) {
        let mut state = self.shared.state();
        let id = state.next_id();
        state.users.push(UserRec {
            id,
            user,
            tokens: Vec::new(),
        });
    }

    pub fn add_org(&self, org: FakeOrg) {
        let mut state = self.shared.state();
        let id = state.next_id();
        state.orgs.push(OrgRec { id, org });
    }

    /// Create a repository and its history. An owner that is neither a user nor an
    /// organization becomes an organization. Returns the commit shas in order.
    pub async fn add_repo(&self, seed: RepoSeed) -> Result<Vec<String>, FakeError> {
        let (dir, branch) = {
            let mut state = self.shared.state();
            if state.repo(&seed.owner, &seed.name).is_some() {
                return Err(FakeError::Invalid(format!(
                    "{}/{} exists already",
                    seed.owner, seed.name
                )));
            }
            if state.account_name(&seed.owner).is_none() {
                let id = state.next_id();
                state.orgs.push(OrgRec {
                    id,
                    org: FakeOrg {
                        name: seed.owner.clone(),
                        full_name: seed.owner.clone(),
                        members: Vec::new(),
                    },
                });
            }
            let owner = state
                .account_name(&seed.owner)
                .unwrap_or(seed.owner.clone());
            let id = state.next_id();
            let created_unix = state.tick();
            state.repos.push(RepoRec {
                id,
                owner,
                name: seed.name.clone(),
                description: String::new(),
                subject: seed.subject.clone(),
                language: seed.language.clone(),
                topics: seed.topics.clone(),
                private: seed.private,
                default_branch: "master".to_owned(),
                created_unix,
                releases: Vec::new(),
            });
            (
                self.shared.git.repo_dir(&seed.owner, &seed.name),
                "master".to_owned(),
            )
        };
        let git = &self.shared.git;
        git.init_bare(&dir, &branch).await?;
        let mut shas = Vec::new();
        for commit in &seed.commits {
            let when = self.shared.state().tick();
            let files: Vec<(String, SeedMode, Vec<u8>)> = commit
                .files
                .iter()
                .map(|f| (f.path.clone(), f.mode, f.bytes.clone()))
                .collect();
            let sha = git
                .commit_snapshot(&dir, &branch, &files, &commit.message, when)
                .await?;
            if let Some(tag) = &commit.tag {
                git.tag(&dir, tag, &sha).await?;
                if let Some(kind) = commit.release {
                    let mut state = self.shared.state();
                    let id = state.next_id();
                    if let Some(repo) = state.repo_mut(&seed.owner, &seed.name) {
                        repo.releases.push(ReleaseRec {
                            id,
                            tag: tag.clone(),
                            kind,
                            created_unix: when,
                        });
                    }
                }
            }
            shas.push(sha);
        }
        Ok(shas)
    }

    /// Give `login` a token directly, as if minted earlier. Returns the secret.
    pub fn issue_token(
        &self,
        login: &str,
        name: &str,
        scopes: &[&str],
    ) -> Result<String, FakeError> {
        let scopes = scopes.iter().map(|s| (*s).to_owned()).collect();
        self.shared
            .state()
            .new_token(login, name, scopes)
            .map(|t| t.sha1)
            .ok_or_else(|| FakeError::Invalid(format!("no user {login}")))
    }

    /// The secret of `login`'s token named `name`, for tests that search files and replies
    /// for it. `None` when there is no such token.
    pub fn token_secret(&self, login: &str, name: &str) -> Option<String> {
        self.shared
            .state()
            .user(login)?
            .tokens
            .iter()
            .find(|t| t.name == name)
            .map(|t| t.sha1.clone())
    }

    /// The tokens `login` holds, without their secrets.
    pub fn tokens_of(&self, login: &str) -> Vec<FakeToken> {
        self.shared
            .state()
            .user(login)
            .map(|u| {
                u.tokens
                    .iter()
                    .map(|t| FakeToken {
                        id: t.id,
                        name: t.name.clone(),
                        scopes: t.scopes.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn inject(&self, rule: FaultRule) {
        self.shared.state().faults.push(rule);
    }

    /// Lets the oldest request that [`Fault::Hold`] holds, or download that
    /// [`Fault::HoldBody`] holds, and that no release has freed yet, go on. A release that comes after the fake began to
    /// answer, but before the download reaches its hold, is kept for that download. A
    /// release with no such download does nothing; it is never kept for a later download.
    /// A download the client dropped counts until the server notices, so a release after
    /// an abort frees that download or nothing.
    pub fn release_held(&self) {
        let mut held = self.shared.held();
        while let Some(release) = held.pop_front() {
            if release.send(()).is_ok() {
                return;
            }
        }
    }

    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.shared.state().requests.clone()
    }

    pub fn clear_requests(&self) {
        self.shared.state().requests.clear();
    }

    /// The most items a listing returns per page, whatever `limit` asks (Gitea's
    /// `MAX_RESPONSE_ITEMS`; 50 by default).
    pub fn set_max_page_size(&self, size: usize) {
        self.shared.state().max_page_size = size.max(1);
    }

    /// The bare repository on disk.
    pub fn git_dir(&self, owner: &str, name: &str) -> PathBuf {
        self.shared.git.repo_dir(owner, name)
    }

    /// The commit `rev` (a tag, branch or sha) names in a repository.
    pub async fn resolve(&self, owner: &str, name: &str, rev: &str) -> Result<String, FakeError> {
        self.shared
            .git
            .resolve(&self.git_dir(owner, name), rev)
            .await
    }

    /// The tree id of the commit `rev` names.
    pub async fn tree_of(&self, owner: &str, name: &str, rev: &str) -> Result<String, FakeError> {
        self.shared
            .git
            .rev_parse(&self.git_dir(owner, name), &format!("{rev}^{{tree}}"))
            .await
    }
}

impl Drop for FakeDoor43 {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}
