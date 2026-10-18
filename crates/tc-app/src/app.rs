//! The application: what `tc4 serve` starts, and the one place a request enters (APP-DESIGN
//! §4.1, §5.3).
//!
//! [`App::call`] answers one request as bytes, exactly as the server sends them with HTTP
//! status 200, so the crate does no HTTP and the server routes to it. [`App::events`] is the
//! event bus the server writes as server-sent events, and [`App::quit`] and
//! [`App::quitting`] are the shutdown the server and the shell share.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{Map, Value, json};
use tc_api::event::{Event, StartupState};
use tc_api::ids::ServerInstance;
use tc_api::{ApiError, Endpoint, Reply, ReplyOf};
use tc_door43::InternetSwitch;
use tokio::sync::watch;

use crate::account::{AccountState, Keychain, MemKeychain};
use crate::clock::{Clock, SystemClock};
use crate::config::{
    AppConfig, ConfigError, HostEnv, ProjectsRoot, RootChoice, choose_projects_root,
};
use crate::events::EventBus;
use crate::flows::FlowGate;
use crate::folders::{FolderCheck, HostFolders};
use crate::jobs::JobRegistry;
use crate::prefs::{AppDataFiles, MemoryFiles, PrefsWriter};
use crate::replies::ReplyCache;
use crate::reports::{ReportLog, ReportOp, ReportOutcome, facts};
use crate::sessions::{NoSessions, SessionDirectory};

/// What a test replaces when it starts the app. [`AppDeps::production`] is what `tc4 serve`
/// runs with.
pub struct AppDeps {
    /// Stamps what people or reports see. Timers use `tokio::time`.
    pub clock: Arc<dyn Clock>,
    pub keychain: Arc<dyn Keychain>,
    /// Which folders exist and which a cloud client copies, for the projects folder (Q-4).
    pub folders: Arc<dyn FolderCheck>,
    /// The computer's home folder, local data folder and kind.
    pub host: HostEnv,
    /// The app-data files `PrefsWriter` writes.
    pub files: Arc<dyn AppDataFiles>,
    pub sessions: Arc<dyn SessionDirectory>,
}

impl AppDeps {
    /// This computer's clock, folders and variables; an in-memory keychain (the operating
    /// system's comes with unit E1a, Q-3); in-memory app data (§4.9: before unit S1); and no
    /// sessions (before unit B1b).
    pub fn production() -> Self {
        let host = HostEnv::current();
        Self {
            clock: Arc::new(SystemClock),
            keychain: Arc::new(MemKeychain::new()),
            folders: Arc::new(HostFolders::new(host.clone())),
            host,
            files: Arc::new(MemoryFiles::new()),
            sessions: Arc::new(NoSessions),
        }
    }
}

/// Why the app did not start. `tc4 serve` prints it and exits.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("the projects folder {path} cannot be used: {reason}")]
    ProjectsFolder { path: PathBuf, reason: String },
    #[error("the app-data folder {path} cannot be used: {reason}")]
    AppData { path: PathBuf, reason: String },
    #[error("this computer gave no random bytes")]
    Random,
}

/// Why a typed call through [`App::request`] has no reply.
#[derive(Debug, thiserror::Error)]
pub enum CallFailure {
    #[error("not an endpoint")]
    NotFound,
    #[error("the app ended")]
    Ended,
    #[error("unreadable reply: {0}")]
    BadReply(String),
}

/// One request's answer, as the server sends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// HTTP 200, `application/json`: the bytes of a [`Reply`].
    Reply { body: Vec<u8>, then: Then },
    /// Not an endpoint, or a test hook while the hooks are off: HTTP 404.
    NotFound,
    /// A test ender ended this app before it could reply, as a process that is gone would:
    /// the server answers 503.
    #[cfg(feature = "test-hooks")]
    Ended,
}

/// What the server does after it has sent a reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    Continue,
    /// `test/crash` at `afterReply` fired: the server ends the process once the body is sent.
    /// Only the server knows when the body was sent.
    #[cfg(feature = "test-hooks")]
    EndProcess,
}

#[derive(Clone)]
pub struct App {
    pub(crate) inner: Arc<Inner>,
}

pub(crate) struct Inner {
    pub(crate) config: AppConfig,
    pub(crate) projects: ProjectsRoot,
    pub(crate) instance: ServerInstance,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) internet: InternetSwitch,
    /// `net/set` turns the switch and publishes under this lock, so the last `Internet` frame
    /// is always the switch's state.
    pub(crate) net_lock: Mutex<()>,
    pub(crate) account: Mutex<AccountState>,
    pub(crate) bus: EventBus,
    pub(crate) jobs: JobRegistry,
    pub(crate) flows: FlowGate,
    pub(crate) prefs: PrefsWriter,
    pub(crate) reports: ReportLog,
    pub(crate) replies: Arc<ReplyCache>,
    pub(crate) sessions: Arc<dyn SessionDirectory>,
    pub(crate) startup: Mutex<StartupState>,
    /// Calls in flight, for `test/quiet`.
    pub(crate) calls: AtomicUsize,
    quit: watch::Sender<bool>,
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Every change made under these locks is a store of a whole value, so a poisoned lock
    // still guards a whole one.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Counts a call while it is in flight. A dropped connection drops the call's future, and
/// the guard with it.
pub(crate) struct InFlight<'a>(&'a AtomicUsize);

impl<'a> InFlight<'a> {
    pub(crate) fn enter(calls: &'a AtomicUsize) -> Self {
        calls.fetch_add(1, Ordering::SeqCst);
        Self(calls)
    }
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl App {
    /// Starts the app: chooses and, at a first launch, creates the projects folder (Q-4),
    /// reads the kept sign-in once, and starts the writers. Must run inside a tokio runtime.
    pub async fn start(config: AppConfig, deps: AppDeps) -> Result<App, StartError> {
        let AppDeps {
            clock,
            keychain,
            folders,
            host,
            files,
            sessions,
        } = deps;

        std::fs::create_dir_all(&config.app_data).map_err(|e| StartError::AppData {
            path: config.app_data.clone(),
            reason: e.to_string(),
        })?;

        let projects = choose_projects_root(&config.projects, &host, folders.as_ref())?;
        if projects.chosen == RootChoice::Given && !folders.is_folder(&projects.path) {
            // Nothing is made in place of a folder someone named: an empty one would look as
            // if the person's projects were gone.
            return Err(StartError::ProjectsFolder {
                path: projects.path,
                reason: "it is not a folder".to_owned(),
            });
        }
        if projects.chosen.is_first_launch() {
            std::fs::create_dir_all(&projects.path).map_err(|e| StartError::ProjectsFolder {
                path: projects.path.clone(),
                reason: e.to_string(),
            })?;
        }

        // Never a weaker source than the system's: the instance tells the UI that its
        // scopes and buffered requests belong to a dead server.
        let mut bytes = [0u8; 16];
        SystemRandom::new()
            .fill(&mut bytes)
            .map_err(|_| StartError::Random)?;
        let instance = ServerInstance::from_bytes(bytes);

        let bus = EventBus::new(instance);
        let reports = ReportLog::new();
        let now = clock.now();
        if let Some(record) = first_launch_record(&projects) {
            reports.add(
                ReportOp::ProjectsFolder,
                ReportOutcome::Ok,
                facts(record),
                now,
                now,
            );
        }

        // The keychain is read here and nowhere else: an operating-system keychain can
        // prompt the person.
        let (account, unreadable) = AccountState::at_start(keychain.read());
        if let Some(reason) = unreadable {
            reports.add(
                ReportOp::Keychain,
                ReportOutcome::Failed { reason },
                Map::new(),
                now,
                now,
            );
        }

        let prefs = PrefsWriter::start(files, reports.clone(), clock.clone());
        let jobs = JobRegistry::new(bus.clone(), reports.clone(), clock.clone());
        let inner = Inner {
            config,
            projects,
            instance,
            clock,
            internet: InternetSwitch::new(),
            net_lock: Mutex::new(()),
            account: Mutex::new(account),
            bus,
            jobs,
            flows: FlowGate::default(),
            prefs,
            reports,
            replies: ReplyCache::new(ReplyCache::FINISHED),
            sessions,
            startup: Mutex::new(StartupState::Ready),
            calls: AtomicUsize::new(0),
            quit: watch::channel(false).0,
        };
        Ok(App {
            inner: Arc::new(inner),
        })
    }

    /// One request, as tc-server receives it: `path` is `service/name`, without `/api/v1/`.
    pub async fn call(&self, path: &str, body: &[u8]) -> Answer {
        let _in_flight = (path != "test/quiet").then(|| InFlight::enter(&self.inner.calls));
        match self.route_product(path, body).await {
            Some(answer) => answer,
            None => Answer::NotFound,
        }
    }

    /// The typed form of [`App::call`], through the same bytes: what tests and later units
    /// use.
    pub async fn request<E: Endpoint>(&self, req: &E) -> Result<ReplyOf<E>, CallFailure> {
        let body = serde_json::to_vec(req).map_err(|e| CallFailure::BadReply(e.to_string()))?;
        match self.call(E::PATH, &body).await {
            Answer::Reply { body, .. } => {
                serde_json::from_slice(&body).map_err(|e| CallFailure::BadReply(e.to_string()))
            }
            Answer::NotFound => Err(CallFailure::NotFound),
            #[cfg(feature = "test-hooks")]
            Answer::Ended => Err(CallFailure::Ended),
        }
    }

    /// The bus the server writes as server-sent events.
    pub fn events(&self) -> &EventBus {
        &self.inner.bus
    }

    /// This launch's identity; the UI goes Home when it changes (§3.10).
    pub fn instance(&self) -> ServerInstance {
        self.inner.instance
    }

    pub fn projects_root(&self) -> &ProjectsRoot {
        &self.inner.projects
    }

    /// The internet switch. Clones share one state: later network clients take this one and
    /// never make their own.
    pub fn internet(&self) -> InternetSwitch {
        self.inner.internet.clone()
    }

    pub fn jobs(&self) -> &JobRegistry {
        &self.inner.jobs
    }

    pub fn flows(&self) -> &FlowGate {
        &self.inner.flows
    }

    pub fn prefs(&self) -> &PrefsWriter {
        &self.inner.prefs
    }

    pub fn reports(&self) -> &ReportLog {
        &self.inner.reports
    }

    pub fn clock(&self) -> &dyn Clock {
        self.inner.clock.as_ref()
    }

    /// Unit A9 reports first-launch seeding through this; `hello` and a `Startup` event
    /// carry it.
    pub fn set_startup(&self, state: StartupState) {
        *lock(&self.inner.startup) = state.clone();
        self.inner.bus.publish(Event::Startup(state));
    }

    /// What `app/quit`, a quit line and a closed stdin do: writes what is pending, then
    /// resolves [`App::quitting`]. Safe to call more than once (the server calls it after
    /// `app/quit` did). Later units save aligner links and checkpoint their sessions first
    /// (§5.1).
    pub async fn quit(&self) {
        self.inner.prefs.flush().await;
        self.inner.quit.send_replace(true);
    }

    /// Resolves once [`App::quit`] has finished.
    pub async fn quitting(&self) {
        let mut quit = self.inner.quit.subscribe();
        // The sender lives in `self`, so this fails only if the app is gone.
        let _ = quit.wait_for(|quit| *quit).await;
    }

    /// Turns the internet switch and tells the UI, under one lock so the last `Internet`
    /// frame always equals the switch. The frame goes out before the caller replies (§5.4).
    pub(crate) fn set_internet(&self, on: bool) {
        let _guard = lock(&self.inner.net_lock);
        let was = self.inner.internet.is_on();
        if on {
            self.inner.internet.turn_on();
        } else {
            self.inner.internet.turn_off();
        }
        if was != on {
            self.inner.bus.publish(Event::Internet { on });
        }
    }

    /// A bug in the server, as a report "Ask for help" can attach.
    pub(crate) fn internal(&self, what: &str, facts: Map<String, Value>) -> ApiError {
        let now = self.inner.clock.now();
        let report = self.inner.reports.add(
            ReportOp::Internal,
            ReportOutcome::Failed {
                reason: what.to_owned(),
            },
            facts,
            now,
            now,
        );
        ApiError::Internal { report }
    }

    /// The bytes of a reply. A reply that cannot be written is a bug, reported, and the UI
    /// gets that report instead.
    pub(crate) fn reply_bytes<T: serde::Serialize>(&self, reply: &Reply<T>, path: &str) -> Vec<u8> {
        match serde_json::to_vec(reply) {
            Ok(bytes) => bytes,
            Err(e) => {
                let error = self.internal(
                    "a reply could not be written",
                    facts(json!({"path": path, "reason": e.to_string()})),
                );
                self.error_bytes(error)
            }
        }
    }

    /// The bytes of an error reply, for any endpoint.
    pub(crate) fn error_bytes(&self, error: ApiError) -> Vec<u8> {
        // The report id is the only data in the fallback, and an `ApiError` is plain data
        // with derived `Serialize`, so the first attempt does not fail in practice.
        serde_json::to_vec(&Reply::<()>::Err(error)).unwrap_or_else(|_| {
            br#"{"type":"err","value":{"type":"internal","value":{"report":0}}}"#.to_vec()
        })
    }

    /// An answer that is an error, never cached.
    pub(crate) fn error_answer(&self, error: ApiError) -> Answer {
        Answer::Reply {
            body: self.error_bytes(error),
            then: Then::Continue,
        }
    }
}

/// The report of a first launch's folder choice (Q-4), so "Ask for help" says where the
/// projects went and why.
fn first_launch_record(projects: &ProjectsRoot) -> Option<Value> {
    let (choice, synced_by) = match &projects.chosen {
        RootChoice::FirstLaunch => ("firstLaunch", None),
        RootChoice::FirstLaunchLocal { default_synced_by } => {
            ("firstLaunchLocal", Some(default_synced_by))
        }
        RootChoice::FirstLaunchSynced { synced_by } => ("firstLaunchSynced", Some(synced_by)),
        RootChoice::Given | RootChoice::Existing | RootChoice::ExistingLocal => return None,
    };
    Some(json!({
        "path": projects.path.display().to_string(),
        "choice": choice,
        "syncedBy": synced_by,
    }))
}
