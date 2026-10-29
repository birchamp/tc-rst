//! The Door43 account (APP-DESIGN §4.7, §5.6, Q-3, unit A8): sign-in, its cancel, the resume
//! of a kept token, sign-out, the profile link, and the session the Door43 steps of later
//! units take ([`App::door43_session`], [`App::door43_refused`]).
//!
//! What is kept, and where (tC4's D85, #366):
//!
//! - The password travels once, in the body of `account/signIn`, and leaves this server only
//!   in the `Authorization` header of the token calls ([`tc_door43::Credentials`]).
//! - The token Door43 mints is held in memory for this app session, and with "Stay signed in"
//!   in the keychain, which receives the token and nothing else. It never reaches a view, an
//!   event, a report, a log line or a URL.
//! - A report names the step, whether the token was kept and the HTTP status, never the login,
//!   the username, the email, the password or Door43's message (which can name the account).
//!
//! The keychain is read once, at start ([`AccountState::at_start`]), because an operating
//! system's keychain can prompt the person; a resume uses the token that read found. Keychain
//! calls run on the blocking pool and never under a lock, for the same reason. The OS keychain
//! comes with unit E1a (`keyring`, behind [`Keychain`]); until then the product has none
//! ([`crate::AppDeps::production`]), and test builds a [`MemKeychain`].
//!
//! Three rules hold whatever the timing, each with its test in `tests/app/account.rs`:
//!
//! - **One sign-in or resume talks to Door43 at a time** (the lane, tC4's `signInFlight` and
//!   `pendingResume`): the token calls list, delete and create the token named
//!   `translationCore`, so two at once could delete each other's token at Door43. A second
//!   sign-in waits for the first; it is never refused as busy.
//! - **Cancel signs nobody in.** `account/cancelSignIn` names its sign-in and works before it
//!   arrives (it then sends nothing), while it waits for the lane (it stops), while Door43
//!   answers the token calls (it stops and signs out, as tC4's cancel does, because those calls
//!   may have replaced the token an earlier sign-in kept), and after it ended (the session it
//!   made is signed out, the token it kept forgotten).
//! - **`kept` says what the keychain holds**: every keychain write and the state change after it
//!   happen one at a time (the keys lane), so a sign-out cannot forget a token between a
//!   sign-in's keep and its commit.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde_json::{Map, Value, json};
use tc_api::account::{
    AccountNotice, AccountView, CancelSignIn, KeepState, KeychainState, LinkView, ProfileLink,
    Resume, ResumeOutcome, ResumeView, SignIn, SignOut, WebUrl,
};
use tc_api::error::SignInReason;
use tc_api::event::Event;
use tc_api::ids::RequestId;
use tc_api::refusal::{Message, SignInRefusal};
use tc_api::{ApiError, Refusal};
use tc_door43::{
    Credentials, Door43Client, Door43Error, Password, ServerUrl, Session, Token, TokenName,
    TokenScope,
};
use tokio::sync::watch;

use crate::app::{App, lock};
use crate::dispatch::Handle;
use crate::reports::{ReportOp, ReportOutcome, facts};

// ---- the keychain ---------------------------------------------------------------------------

/// Where "Stay signed in" keeps a Door43 token.
pub trait Keychain: Send + Sync + 'static {
    /// Keeps the token. `NoKeychain` where this computer has none the app can use.
    fn keep(&self, token: &Token) -> Result<(), KeychainError>;

    /// The kept token. Bytes the keychain cannot open are removed and reported as
    /// `Unreadable`, as tC4's `readToken` does: they are no use to anyone. A computer with no
    /// keychain answers `NoKeychain`.
    fn read(&self) -> Result<Option<Token>, KeychainError>;

    fn forget(&self) -> Result<(), KeychainError>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeychainError {
    #[error("this computer has no keychain the app can use")]
    NoKeychain,
    #[error("the kept token could not be read: {0}")]
    Unreadable(String),
    #[error("the keychain refused: {0}")]
    Failed(String),
}

/// The computer's own credential store (ruling H11): Windows Credential Manager, the macOS
/// Keychain or the Secret Service, through `keyring`. One entry holds the token's secret.
pub struct OsKeychain {
    service: String,
    user: String,
    /// Install `keyring`'s platform store before the first call; `false` for a test's mock.
    platform: bool,
}

impl OsKeychain {
    /// The production keychain: `keyring`'s store for this platform, installed once. A computer
    /// whose store cannot start answers `NoKeychain` on every call.
    pub fn platform() -> Self {
        Self {
            platform: true,
            ..Self::over_default_store()
        }
    }

    /// Over whatever store `keyring_core` has as its default (a test's mock).
    pub fn over_default_store() -> Self {
        Self {
            service: "translationCore 4".to_owned(),
            user: "Door43".to_owned(),
            platform: false,
        }
    }

    fn entry(&self) -> Result<keyring_core::Entry, KeychainError> {
        // The platform store starts at its first use (the read at launch, in a real build);
        // `keyring` installs it once and remembers how that went.
        if self.platform && keyring::Entry::store_status().is_err() {
            return Err(KeychainError::NoKeychain);
        }
        keyring_core::Entry::new(&self.service, &self.user).map_err(keychain_error)
    }
}

fn keychain_error(error: keyring_core::Error) -> KeychainError {
    use keyring_core::Error as E;
    match error {
        E::NoDefaultStore | E::NoStorageAccess(_) => KeychainError::NoKeychain,
        E::BadEncoding(_) | E::BadDataFormat(..) => {
            KeychainError::Unreadable("the kept bytes are not a token".to_owned())
        }
        other => KeychainError::Failed(other.to_string()),
    }
}

impl Keychain for OsKeychain {
    fn keep(&self, token: &Token) -> Result<(), KeychainError> {
        self.entry()?
            .set_password(token.expose_secret())
            .map_err(keychain_error)
    }

    fn read(&self) -> Result<Option<Token>, KeychainError> {
        let entry = self.entry()?;
        let unreadable = |entry: &keyring_core::Entry, why: String| {
            // As tC4's `readToken`: bytes that cannot be opened are no use to anyone.
            let _ = entry.delete_credential();
            KeychainError::Unreadable(why)
        };
        match entry.get_password() {
            Ok(secret) => Token::new(secret)
                .map(Some)
                .map_err(|e| unreadable(&entry, e.to_string())),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(error) => match keychain_error(error) {
                KeychainError::Unreadable(why) => Err(unreadable(&entry, why)),
                other => Err(other),
            },
        }
    }

    fn forget(&self) -> Result<(), KeychainError> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(error) => Err(keychain_error(error)),
        }
    }
}

/// How a [`MemKeychain`] behaves, for tests of what the app does when the keychain does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemKeychainMode {
    Working,
    /// This computer has no keychain: nothing is kept, keeping is refused, a read answers
    /// that there is none, and forgetting has nothing to do.
    NoKeychain,
    /// Keeping fails; reading and forgetting work.
    FailingKeep,
    /// Forgetting fails; keeping and reading work.
    FailingForget,
    /// The kept bytes cannot be opened: the first read removes them and fails.
    Unreadable,
}

/// The calls a [`MemKeychain`] was asked, whatever its mode answered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeychainCalls {
    pub keeps: u32,
    pub reads: u32,
    pub forgets: u32,
}

/// A keychain in memory: the tests' keychain. The product uses [`OsKeychain`].
pub struct MemKeychain {
    token: Mutex<Option<Token>>,
    mode: Mutex<MemKeychainMode>,
    keeps: AtomicU32,
    reads: AtomicU32,
    forgets: AtomicU32,
}

impl Default for MemKeychain {
    fn default() -> Self {
        Self::new()
    }
}

fn guard<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Every change under these locks stores a whole value.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl MemKeychain {
    pub fn new() -> Self {
        Self::with_mode(MemKeychainMode::Working)
    }

    pub fn holding(token: Token) -> Self {
        let keychain = Self::new();
        keychain.seed(token);
        keychain
    }

    pub fn with_mode(mode: MemKeychainMode) -> Self {
        Self {
            token: Mutex::new(None),
            mode: Mutex::new(mode),
            keeps: AtomicU32::new(0),
            reads: AtomicU32::new(0),
            forgets: AtomicU32::new(0),
        }
    }

    /// Puts a token in as if an earlier session had kept it, in whatever mode this is.
    pub fn seed(&self, token: Token) {
        *guard(&self.token) = Some(token);
    }

    /// Forgets what it holds, without counting a call.
    pub fn clear(&self) {
        *guard(&self.token) = None;
    }

    pub fn mode(&self) -> MemKeychainMode {
        *guard(&self.mode)
    }

    pub fn set_mode(&self, mode: MemKeychainMode) {
        *guard(&self.mode) = mode;
    }

    /// Whether it holds a token, without counting a call.
    pub fn holds(&self) -> bool {
        guard(&self.token).is_some()
    }

    pub fn calls(&self) -> KeychainCalls {
        KeychainCalls {
            keeps: self.keeps.load(Ordering::SeqCst),
            reads: self.reads.load(Ordering::SeqCst),
            forgets: self.forgets.load(Ordering::SeqCst),
        }
    }

    /// Working, empty, and no calls counted: a computer the app has not run on.
    pub fn reset(&self) {
        self.set_mode(MemKeychainMode::Working);
        self.clear();
        for count in [&self.keeps, &self.reads, &self.forgets] {
            count.store(0, Ordering::SeqCst);
        }
    }
}

impl Keychain for MemKeychain {
    fn keep(&self, token: &Token) -> Result<(), KeychainError> {
        self.keeps.fetch_add(1, Ordering::SeqCst);
        match self.mode() {
            MemKeychainMode::NoKeychain => Err(KeychainError::NoKeychain),
            MemKeychainMode::FailingKeep => {
                Err(KeychainError::Failed("the keychain is locked".to_owned()))
            }
            MemKeychainMode::Working
            | MemKeychainMode::FailingForget
            | MemKeychainMode::Unreadable => {
                *guard(&self.token) = Some(token.clone());
                Ok(())
            }
        }
    }

    fn read(&self) -> Result<Option<Token>, KeychainError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let mut slot = guard(&self.token);
        match self.mode() {
            MemKeychainMode::NoKeychain => Err(KeychainError::NoKeychain),
            MemKeychainMode::Unreadable => {
                if slot.take().is_some() {
                    Err(KeychainError::Unreadable("damaged".to_owned()))
                } else {
                    Ok(None)
                }
            }
            MemKeychainMode::Working
            | MemKeychainMode::FailingKeep
            | MemKeychainMode::FailingForget => Ok(slot.clone()),
        }
    }

    fn forget(&self) -> Result<(), KeychainError> {
        self.forgets.fetch_add(1, Ordering::SeqCst);
        if self.mode() == MemKeychainMode::FailingForget {
            return Err(KeychainError::Failed("the keychain is locked".to_owned()));
        }
        *guard(&self.token) = None;
        Ok(())
    }
}

// ---- the account's state ----------------------------------------------------------------------

/// tC4's token name and scopes (`tC4:src/data/share/door43Api.ts:47-60`). Door43 refuses a
/// second token of one name, so a sign-in replaces the one an earlier sign-in made, on this
/// computer or another; tC4 does the same.
const TOKEN_NAME: &str = "translationCore";

/// The cancels a sign-in may meet before it arrives that are remembered.
const EARLY_CANCELS: usize = 16;

/// The signed-in session, and the sign-in that made it (`None`: a resume).
struct HeldSession {
    session: Session,
    by: Option<RequestId>,
}

/// What the keychain holds, as this run knows it.
enum Kept {
    Nothing,
    /// "Stay signed in" was asked and the keychain did not keep the token.
    Refused,
    /// A token, kept by a sign-in of this run (`by`) or found at start (`None`).
    Token {
        token: Token,
        by: Option<RequestId>,
    },
}

/// The account as this server run holds it.
pub(crate) struct AccountState {
    session: Option<HeldSession>,
    kept: Kept,
    keychain: KeychainState,
    notice: Option<AccountNotice>,
    /// The host `signIn.server` names.
    server: String,
    /// Raised by every change a resume in flight must not undo: a sign-in, a sign-out, a
    /// cancel that signs out, a test relaunch.
    revision: u64,
}

/// `url`'s host, with its port when it is not the scheme's default, as JavaScript's
/// `URL.host` writes it (tC4's `new URL(door43.server).host`).
fn host_of(server: &ServerUrl) -> String {
    let url = server.as_url();
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    }
}

impl AccountState {
    /// What the first read of the keychain means, and the reason to report when the read
    /// failed. A computer with no keychain, or an empty one, is not an error: the sign-in
    /// then lasts for this session only.
    pub(crate) fn at_start(
        read: Result<Option<Token>, KeychainError>,
        server: &ServerUrl,
    ) -> (Self, Option<String>) {
        let (kept, keychain, report) = match read {
            Ok(Some(token)) => (Kept::Token { token, by: None }, KeychainState::Works, None),
            Ok(None) => (Kept::Nothing, KeychainState::Works, None),
            Err(KeychainError::NoKeychain) => (Kept::Nothing, KeychainState::Missing, None),
            // The reason is the keychain's own text; a token is never in it (`Token` prints
            // redacted and the keychain returns bytes only when they open).
            Err(KeychainError::Unreadable(reason) | KeychainError::Failed(reason)) => (
                Kept::Nothing,
                KeychainState::Works,
                Some(format!("the kept sign-in could not be read: {reason}")),
            ),
        };
        let state = Self {
            session: None,
            kept,
            keychain,
            notice: None,
            server: host_of(server),
            revision: 0,
        };
        (state, report)
    }

    pub(crate) fn view(&self) -> AccountView {
        AccountView {
            user: self
                .session
                .as_ref()
                .map(|held| held.session.username.as_str().to_owned()),
            kept: match self.kept {
                Kept::Nothing => KeepState::NotKept,
                Kept::Refused => KeepState::NoKeychain,
                Kept::Token { .. } => KeepState::Kept,
            },
            notice: self.notice,
            server: self.server.clone(),
            keychain: self.keychain,
        }
    }

    fn kept_by(&self, sign_in: RequestId) -> bool {
        matches!(self.kept, Kept::Token { by: Some(by), .. } if by == sign_in)
    }

    fn kept_token(&self) -> Option<&Token> {
        match &self.kept {
            Kept::Token { token, .. } => Some(token),
            Kept::Nothing | Kept::Refused => None,
        }
    }
}

/// A sign-in's cancel: set once, seen by every clone.
#[derive(Clone)]
struct CancelToken(Arc<watch::Sender<bool>>);

impl CancelToken {
    fn new(cancelled: bool) -> Self {
        Self(Arc::new(watch::channel(cancelled).0))
    }

    fn cancel(&self) {
        self.0.send_replace(true);
    }

    fn is_cancelled(&self) -> bool {
        *self.0.borrow()
    }

    async fn cancelled(&self) {
        let mut seen = self.0.subscribe();
        // The sender lives in `self`, so the channel cannot close while this waits.
        let _ = seen.wait_for(|cancelled| *cancelled).await;
    }
}

#[derive(Default)]
struct SignIns {
    /// From a sign-in's arrival to its commit, so a cancel finds it wherever it is.
    running: HashMap<RequestId, CancelToken>,
    /// Cancels that came before their sign-in, newest last.
    early: VecDeque<RequestId>,
}

/// The account service's state and what it talks to.
pub(crate) struct Account {
    state: Mutex<AccountState>,
    keychain: Arc<dyn Keychain>,
    /// The client is built at start; one that cannot be built (no TLS backend) does not stop
    /// the app, whose work needs no Door43: each step that needs Door43 reports why.
    door43: Result<Door43Client, String>,
    /// One sign-in or resume talks to Door43 at a time.
    lane: tokio::sync::Mutex<()>,
    /// One keychain write and the state change after it at a time.
    keys: tokio::sync::Mutex<()>,
    sign_ins: Mutex<SignIns>,
}

impl Account {
    pub(crate) fn new(
        state: AccountState,
        keychain: Arc<dyn Keychain>,
        door43: Result<Door43Client, String>,
    ) -> Self {
        Self {
            state: Mutex::new(state),
            keychain,
            door43,
            lane: tokio::sync::Mutex::new(()),
            keys: tokio::sync::Mutex::new(()),
            sign_ins: Mutex::new(SignIns::default()),
        }
    }

    pub(crate) fn view(&self) -> AccountView {
        lock(&self.state).view()
    }

    /// A sign-in arrives: its cancel, already set when the cancel came first.
    fn arrive(&self, id: RequestId) -> CancelToken {
        let mut sign_ins = lock(&self.sign_ins);
        let early = match sign_ins.early.iter().position(|e| *e == id) {
            Some(at) => {
                sign_ins.early.remove(at);
                true
            }
            None => false,
        };
        let token = CancelToken::new(early);
        sign_ins.running.insert(id, token.clone());
        token
    }

    /// Cancels every sign-in in flight and forgets the early cancels: a launch has none.
    #[cfg(feature = "test-hooks")]
    fn cancel_all(&self) {
        let mut sign_ins = lock(&self.sign_ins);
        for token in sign_ins.running.values() {
            token.cancel();
        }
        sign_ins.early.clear();
    }
}

/// Takes a sign-in out of the running ones on every way out of its handler.
struct Running<'a> {
    account: &'a Account,
    id: RequestId,
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        lock(&self.account.sign_ins).running.remove(&self.id);
    }
}

/// A keychain call on the blocking pool: an operating system's keychain can wait on a prompt.
async fn on_keychain<T: Send + 'static>(
    keychain: &Arc<dyn Keychain>,
    call: impl FnOnce(&dyn Keychain) -> Result<T, KeychainError> + Send + 'static,
) -> Result<T, KeychainError> {
    let keychain = keychain.clone();
    tokio::task::spawn_blocking(move || call(keychain.as_ref()))
        .await
        .unwrap_or_else(|stopped| Err(KeychainError::Failed(stopped.to_string())))
}

/// The refusal a failed token call shows, in tC4's codes (`refusalForSignIn`). Only 401 and
/// 403 are about the password. Never `error.to_string()`: a `Door43Error` starts with its
/// route, and the token routes hold the account name.
fn sign_in_refusal(error: &Door43Error) -> SignInRefusal {
    match error {
        Door43Error::Offline { .. } | Door43Error::Unreachable { .. } => SignInRefusal::Offline,
        Door43Error::AuthFailed { .. } => SignInRefusal::AuthFailed,
        Door43Error::Server { .. } => SignInRefusal::ServerUnavailable,
        Door43Error::Rejected { message, .. }
        | Door43Error::NotFound { message, .. }
        | Door43Error::NameExists { message, .. } => SignInRefusal::Rejected {
            reason: message.clone(),
        },
        Door43Error::UnexpectedResponse { reason, .. } => SignInRefusal::Rejected {
            reason: reason.clone(),
        },
        Door43Error::RateLimited { .. } => SignInRefusal::Rejected {
            reason: "too many requests".to_owned(),
        },
        Door43Error::TooLarge { .. } => SignInRefusal::Rejected {
            reason: "the answer is too large".to_owned(),
        },
        // A token call is no push, so the push refusals cannot come from it.
        Door43Error::NoWriteAccess { .. }
        | Door43Error::RefRejected { .. }
        | Door43Error::RefMoved { .. }
        | Door43Error::Verification { .. }
        | Door43Error::Cancelled { .. }
        | Door43Error::Disk { .. }
        | Door43Error::Setup { .. } => SignInRefusal::Rejected {
            reason: "the sign-in could not be completed".to_owned(),
        },
    }
}

/// The HTTP status Door43 answered, for a report.
fn status_of(error: &Door43Error) -> Option<u16> {
    match error {
        Door43Error::AuthFailed { status, .. } => Some(match status {
            tc_door43::AuthStatus::Unauthorized => 401,
            tc_door43::AuthStatus::Forbidden => 403,
        }),
        Door43Error::Server { status, .. } | Door43Error::Rejected { status, .. } => Some(*status),
        Door43Error::RateLimited { .. } => Some(429),
        Door43Error::NotFound { .. } => Some(404),
        Door43Error::NameExists { .. } => Some(409),
        _ => None,
    }
}

/// What the keychain holds after a sign-in's keychain step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Keyed {
    /// Nothing was asked of it: this computer has none.
    Untouched,
    /// It answered that this computer has none.
    NoKeychain,
    /// This sign-in's token.
    Kept,
    /// Nothing: a token an earlier session kept was forgotten, or none was there.
    Emptied,
    /// What it held before: it would not forget.
    Unchanged,
}

impl App {
    fn account(&self) -> &Account {
        &self.inner.account
    }

    pub(crate) fn door43(&self) -> Result<&Door43Client, ApiError> {
        self.account().door43.as_ref().map_err(|reason| {
            self.internal(
                "the Door43 client could not start",
                facts(json!({ "reason": reason })),
            )
        })
    }

    fn report_account(&self, op: ReportOp, outcome: ReportOutcome, facts: Map<String, Value>) {
        let now = self.inner.clock.now();
        self.inner.reports.add(op, outcome, facts, now, now);
    }

    fn report_refused(&self, refusal: &SignInRefusal, status: Option<u16>) {
        self.report_account(
            ReportOp::Share,
            ReportOutcome::Refused {
                code: refusal.code().map(str::to_owned),
                key: refusal.key().to_owned(),
            },
            facts(json!({ "step": "sign-in", "status": status })),
        );
    }

    /// The account frame goes out while the state lock is held, so the last frame always
    /// equals the state, and before the caller replies (§5.4).
    fn publish_account(&self, state: &AccountState) -> AccountView {
        let view = state.view();
        self.inner.bus.publish(Event::Account(view.clone()));
        view
    }

    /// `account/signIn`, whose `NET` check dispatch made.
    async fn sign_in(&self, req: SignIn) -> Result<AccountView, ApiError> {
        let account = self.account();
        let id = req.request;
        let cancel = account.arrive(id);
        let _running = Running { account, id };

        // tC4's state layer trims the login (`si.login.trim()`); the password is sent as typed.
        let login = req.login.trim();
        let credentials = (!login.is_empty() && !req.password.is_empty())
            .then(|| Credentials::new(login, Password::new(req.password)).ok())
            .flatten();
        let Some(credentials) = credentials else {
            let refusal = SignInRefusal::AuthFailed;
            self.report_refused(&refusal, None);
            return Err(ApiError::Refused(Refusal::SignIn(refusal)));
        };

        let _lane = tokio::select! {
            biased;
            () = cancel.cancelled() => return Ok(account.view()),
            lane = account.lane.lock() => lane,
        };
        let client = self.door43()?;
        let name: TokenName = TOKEN_NAME
            .parse()
            .map_err(|_| self.internal("the token name does not parse", Map::new()))?;
        // From here a cancel signs out, as tC4's does: the token calls may already have
        // replaced the `translationCore` token an earlier sign-in kept, so keeping it would
        // show a saved sign-in Door43 refuses. The lane, held until this returns, keeps any
        // other sign-in from keeping a token in between.
        let minted = tokio::select! {
            biased;
            () = cancel.cancelled() => return Ok(self.sign_out().await),
            minted = client.mint_token(&credentials, &name, &TokenScope::FOR_SHARING) => minted,
        };
        let session = match minted {
            Ok(session) => session,
            Err(error) => {
                let refusal = sign_in_refusal(&error);
                self.report_refused(&refusal, status_of(&error));
                return Err(ApiError::Refused(Refusal::SignIn(refusal)));
            }
        };

        let keys = account.keys.lock().await;
        if cancel.is_cancelled() {
            drop(keys);
            return Ok(self.sign_out().await);
        }
        let (keyed, step_facts) = self.keep_or_forget(req.stay, &session.token).await;

        // The commit: a cancel that comes after this finds no running sign-in and signs the
        // session out, so it must not find this one running once it has committed.
        let mut sign_ins = lock(&account.sign_ins);
        if cancel.is_cancelled() {
            drop(sign_ins);
            return Ok(self.undo_keychain_step(id, keyed, &session.token).await);
        }
        sign_ins.running.remove(&id);
        let mut state = lock(&account.state);
        drop(sign_ins);
        state.kept = match keyed {
            Keyed::Kept => Kept::Token {
                token: session.token.clone(),
                by: Some(id),
            },
            Keyed::Unchanged => std::mem::replace(&mut state.kept, Kept::Nothing),
            Keyed::NoKeychain => {
                state.keychain = KeychainState::Missing;
                Kept::Refused
            }
            Keyed::Emptied | Keyed::Untouched if req.stay => Kept::Refused,
            Keyed::Emptied | Keyed::Untouched => Kept::Nothing,
        };
        state.session = Some(HeldSession {
            session,
            by: Some(id),
        });
        state.notice = None;
        state.revision += 1;
        let view = self.publish_account(&state);
        drop(state);
        self.report_account(ReportOp::Share, ReportOutcome::Ok, step_facts);
        Ok(view)
    }

    /// Keeps the token with "Stay signed in"; when it is not kept, forgets a token an earlier
    /// session kept, so it cannot sign the next session in (tC4 #366). Without a keychain
    /// nothing is asked of it. The facts are the sign-in report's.
    async fn keep_or_forget(&self, stay: bool, token: &Token) -> (Keyed, Map<String, Value>) {
        let account = self.account();
        let mut facts = facts(json!({ "step": "sign-in", "kept": false }));
        if lock(&account.state).keychain == KeychainState::Missing {
            return (Keyed::Untouched, facts);
        }
        if stay {
            let token = token.clone();
            match on_keychain(&account.keychain, move |k| k.keep(&token)).await {
                Ok(()) => {
                    facts.insert("kept".to_owned(), json!(true));
                    return (Keyed::Kept, facts);
                }
                Err(KeychainError::NoKeychain) => return (Keyed::NoKeychain, facts),
                Err(error) => {
                    facts.insert("keepError".to_owned(), json!(error.to_string()));
                }
            }
        }
        match on_keychain(&account.keychain, |k| k.forget()).await {
            Ok(()) => (Keyed::Emptied, facts),
            Err(error) => {
                facts.insert("forgetError".to_owned(), json!(error.to_string()));
                (Keyed::Unchanged, facts)
            }
        }
    }

    /// A sign-in cancelled after its keychain step signs out, as one cancelled during the token
    /// calls does: the token it kept is forgotten again, and `kept` says what the keychain now
    /// holds.
    async fn undo_keychain_step(&self, id: RequestId, keyed: Keyed, token: &Token) -> AccountView {
        let account = self.account();
        let forgot = match keyed {
            Keyed::Kept => Some(on_keychain(&account.keychain, |k| k.forget()).await),
            Keyed::Untouched | Keyed::NoKeychain | Keyed::Emptied | Keyed::Unchanged => None,
        };
        let mut state = lock(&account.state);
        state.session = None;
        state.revision += 1;
        match (keyed, forgot) {
            (Keyed::Kept, Some(Err(error))) => {
                // The keychain holds this sign-in's token and would not let it go: said, as a
                // sign-out that cannot forget is.
                state.kept = Kept::Token {
                    token: token.clone(),
                    by: Some(id),
                };
                state.notice = Some(AccountNotice::SignOutFailed);
                self.report_account(
                    ReportOp::Keychain,
                    ReportOutcome::Failed {
                        reason: error.to_string(),
                    },
                    facts(json!({ "step": "cancel" })),
                );
            }
            (Keyed::Kept | Keyed::Emptied, _) => state.kept = Kept::Nothing,
            (Keyed::NoKeychain, _) => state.keychain = KeychainState::Missing,
            (Keyed::Untouched | Keyed::Unchanged, _) => {}
        }
        self.publish_account(&state)
    }

    /// `account/cancelSignIn`.
    async fn cancel_sign_in(&self, req: CancelSignIn) {
        let account = self.account();
        let forget = {
            let mut sign_ins = lock(&account.sign_ins);
            if let Some(running) = sign_ins.running.get(&req.sign_in) {
                running.cancel();
                return;
            }
            let mut state = lock(&account.state);
            let made_it = state
                .session
                .as_ref()
                .is_some_and(|held| held.by == Some(req.sign_in));
            if !made_it {
                drop(state);
                if sign_ins.early.len() == EARLY_CANCELS {
                    sign_ins.early.pop_front();
                }
                sign_ins.early.push_back(req.sign_in);
                return;
            }
            state.session = None;
            state.revision += 1;
            self.publish_account(&state);
            state.kept_by(req.sign_in)
        };
        if forget {
            self.forget_kept_by(Some(req.sign_in)).await;
        }
    }

    /// Forgets the kept token when `by` kept it (`None`: whatever is kept), and says what
    /// the keychain holds after.
    async fn forget_kept_by(&self, by: Option<RequestId>) {
        let account = self.account();
        let _keys = account.keys.lock().await;
        let still = {
            let state = lock(&account.state);
            match by {
                Some(id) => state.kept_by(id),
                None => true,
            }
        };
        if !still {
            return;
        }
        let forgot = on_keychain(&account.keychain, |k| k.forget()).await;
        let mut state = lock(&account.state);
        match forgot {
            Ok(()) => {
                state.kept = Kept::Nothing;
            }
            Err(error) => {
                state.notice = Some(AccountNotice::SignOutFailed);
                self.report_account(
                    ReportOp::Keychain,
                    ReportOutcome::Failed {
                        reason: error.to_string(),
                    },
                    facts(json!({ "step": "sign-out" })),
                );
            }
        }
        self.publish_account(&state);
    }

    /// `account/signOut`: the token leaves memory and the keychain, with no request. A keychain
    /// that cannot forget is said, never hidden: the sign-in then stays saved (tC4 D88).
    async fn sign_out(&self) -> AccountView {
        let account = self.account();
        let _keys = account.keys.lock().await;
        let (revision, keychain) = {
            let mut state = lock(&account.state);
            state.session = None;
            state.revision += 1;
            (state.revision, state.keychain)
        };
        let forgot = match keychain {
            KeychainState::Works => on_keychain(&account.keychain, |k| k.forget()).await,
            KeychainState::Missing => Ok(()),
        };
        let mut state = lock(&account.state);
        match forgot {
            Ok(()) => {
                if state.revision == revision {
                    state.kept = Kept::Nothing;
                    state.notice = None;
                }
            }
            Err(error) => {
                state.notice = Some(AccountNotice::SignOutFailed);
                self.report_account(
                    ReportOp::Keychain,
                    ReportOutcome::Failed {
                        reason: error.to_string(),
                    },
                    facts(json!({ "step": "sign-out" })),
                );
            }
        }
        self.publish_account(&state)
    }

    /// Resumes the session from the token the start's keychain read found (tC4's
    /// `resumeKeptSession`). Never reads the keychain.
    async fn resume_kept(&self) -> Result<ResumeOutcome, ApiError> {
        let account = self.account();
        if lock(&account.state).session.is_some() {
            return Ok(ResumeOutcome::Resumed);
        }
        let _lane = account.lane.lock().await;
        let (token, revision) = {
            let state = lock(&account.state);
            if state.session.is_some() {
                return Ok(ResumeOutcome::Resumed);
            }
            match state.kept_token() {
                Some(token) => (token.clone(), state.revision),
                None => return Ok(ResumeOutcome::NoneKept),
            }
        };
        let client = self.door43()?;
        let answer = client.session_for_token(token.clone()).await;
        match answer {
            Ok(session) => {
                let mut state = lock(&account.state);
                if state.revision != revision {
                    return Ok(ResumeOutcome::NoneKept);
                }
                state.session = Some(HeldSession { session, by: None });
                state.notice = None;
                self.publish_account(&state);
                Ok(ResumeOutcome::Resumed)
            }
            Err(Door43Error::AuthFailed { .. }) => {
                let _keys = account.keys.lock().await;
                if lock(&account.state).revision != revision {
                    return Ok(ResumeOutcome::NoneKept);
                }
                let forgot = on_keychain(&account.keychain, |k| k.forget()).await;
                let mut state = lock(&account.state);
                if state.revision != revision {
                    return Ok(ResumeOutcome::NoneKept);
                }
                match forgot {
                    Ok(()) => state.kept = Kept::Nothing,
                    Err(error) => self.report_account(
                        ReportOp::Keychain,
                        ReportOutcome::Failed {
                            reason: error.to_string(),
                        },
                        facts(json!({ "step": "resume" })),
                    ),
                }
                state.notice = Some(AccountNotice::SavedRefused);
                state.revision += 1;
                self.publish_account(&state);
                Ok(ResumeOutcome::Refused)
            }
            Err(_) => {
                let mut state = lock(&account.state);
                if state.revision != revision {
                    return Ok(ResumeOutcome::NoneKept);
                }
                state.notice = Some(AccountNotice::SavedUnchecked);
                self.publish_account(&state);
                Ok(ResumeOutcome::Unavailable)
            }
        }
    }

    /// The Door43 session a step of a later unit (share, sync, open) takes: the one held, or
    /// the kept token resumed (§5.6 item 6), or `NeedsSignIn`, after which the UI shows the
    /// sign-in step and runs the step again. Check the internet switch before calling it, as
    /// dispatch does for an endpoint that declares `NET`.
    pub async fn door43_session(&self) -> Result<Session, ApiError> {
        if let Some(session) = self.held_session() {
            return Ok(session);
        }
        match self.resume_kept().await? {
            ResumeOutcome::Resumed => self
                .held_session()
                .ok_or(ApiError::NeedsSignIn(SignInReason::NotSignedIn)),
            ResumeOutcome::Refused => Err(ApiError::NeedsSignIn(SignInReason::TokenRejected)),
            ResumeOutcome::NoneKept | ResumeOutcome::Unavailable => {
                Err(ApiError::NeedsSignIn(SignInReason::NotSignedIn))
            }
        }
    }

    pub(crate) fn held_session(&self) -> Option<Session> {
        lock(&self.account().state)
            .session
            .as_ref()
            .map(|held| held.session.clone())
    }

    /// What a step calls when Door43 answered 401 to `session`'s token: both copies of that
    /// token are forgotten, in memory and in the keychain (DELTA-10-16 §3.15.1 item 6), and the
    /// step answers the returned `NeedsSignIn(TokenRejected)`.
    pub async fn door43_refused(&self, session: &Session) -> ApiError {
        let account = self.account();
        let _keys = account.keys.lock().await;
        let kept_too = {
            let mut state = lock(&account.state);
            if state
                .session
                .as_ref()
                .is_some_and(|held| held.session.token == session.token)
            {
                state.session = None;
                state.revision += 1;
            }
            state.kept_token() == Some(&session.token)
        };
        let forgot = if kept_too {
            Some(on_keychain(&account.keychain, |k| k.forget()).await)
        } else {
            None
        };
        let mut state = lock(&account.state);
        match forgot {
            Some(Ok(())) => {
                if state.kept_token() == Some(&session.token) {
                    state.kept = Kept::Nothing;
                }
            }
            Some(Err(error)) => self.report_account(
                ReportOp::Keychain,
                ReportOutcome::Failed {
                    reason: error.to_string(),
                },
                facts(json!({ "step": "refused" })),
            ),
            None => {}
        }
        state.notice = None;
        self.publish_account(&state);
        ApiError::NeedsSignIn(SignInReason::TokenRejected)
    }

    /// The account as a launch finds it, read again from the keychain: every sign-in in
    /// flight cancelled, the session and the notice gone, and an `Account` frame (the test
    /// hooks' `test/reset` and `test/keychain`).
    #[cfg(feature = "test-hooks")]
    pub(crate) async fn relaunch_account(&self) {
        let account = self.account();
        account.cancel_all();
        let _keys = account.keys.lock().await;
        let read = on_keychain(&account.keychain, |k| k.read()).await;
        let server = self.inner.config.door43.clone();
        let (fresh, unreadable) = AccountState::at_start(read, &server);
        if let Some(reason) = unreadable {
            self.report_account(
                ReportOp::Keychain,
                ReportOutcome::Failed { reason },
                Map::new(),
            );
        }
        let mut state = lock(&account.state);
        let revision = state.revision + 1;
        *state = fresh;
        state.revision = revision;
        self.publish_account(&state);
    }
}

impl Handle<tc_api::account::State> for App {
    /// From memory: it reaches neither Door43 nor the keychain.
    async fn handle(&self, _req: tc_api::account::State) -> Result<AccountView, ApiError> {
        Ok(self.account().view())
    }
}

impl Handle<SignIn> for App {
    async fn handle(&self, req: SignIn) -> Result<AccountView, ApiError> {
        self.sign_in(req).await
    }
}

impl Handle<CancelSignIn> for App {
    async fn handle(&self, req: CancelSignIn) -> Result<(), ApiError> {
        self.cancel_sign_in(req).await;
        Ok(())
    }
}

impl Handle<Resume> for App {
    async fn handle(&self, _req: Resume) -> Result<ResumeView, ApiError> {
        let outcome = self.resume_kept().await?;
        Ok(ResumeView {
            outcome,
            account: self.account().view(),
        })
    }
}

impl Handle<SignOut> for App {
    async fn handle(&self, _req: SignOut) -> Result<AccountView, ApiError> {
        Ok(self.sign_out().await)
    }
}

impl Handle<ProfileLink> for App {
    /// Door43's page of the signed-in person: the server's address and the username, which
    /// the shell opens in the system browser. The internet check already ran.
    async fn handle(&self, _req: ProfileLink) -> Result<LinkView, ApiError> {
        let Some(session) = self.held_session() else {
            return Err(ApiError::NeedsSignIn(SignInReason::NotSignedIn));
        };
        let mut url = self.inner.config.door43.as_url().clone();
        if let Ok(mut path) = url.path_segments_mut() {
            path.pop_if_empty().push(session.username.as_str());
        }
        WebUrl::parse(url.as_str())
            .map_err(|e| {
                self.internal(
                    "the profile address is not a web address",
                    facts(json!({ "reason": e.to_string() })),
                )
            })
            .map(|url| LinkView { url })
    }
}
