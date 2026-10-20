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
use tc_api::ids::{JobId, ServerInstance};
use tc_api::{ApiError, Endpoint, Reply, ReplyOf};
use tc_door43::{Door43Client, InternetSwitch};
use tc_resources::ResourceStore;
use tokio::sync::watch;

use crate::account::{Account, AccountState, Keychain, MemKeychain, MemKeychainMode};
use crate::backend::ProjectStore;
use crate::backend::store::StoreBackend;
use crate::clock::{Clock, SystemClock};
use crate::config::{
    AppConfig, ConfigError, HostEnv, ProjectsRoot, RootChoice, choose_projects_root,
};
use crate::counters::{AppCounters, AppCounts};
use crate::events::EventBus;
use crate::flows::FlowGate;
use crate::folders::{FolderCheck, HostFolders};
#[cfg(feature = "test-hooks")]
use crate::hooks::{Ender, ExitProcess, TestHooks};
use crate::jobs::JobRegistry;
use crate::prefs::{AppDataFiles, DiskFiles, PrefsWriter, read_reports};
use crate::replies::ReplyCache;
use crate::reports::{ReportLog, ReportOp, ReportOutcome, facts};
use crate::seed::{self, RESOURCES_FOLDER};
use crate::session::{ProjectSession, SessionRegistry};
use crate::startup::Startup;

/// Where the app's projects come from.
pub enum StoreChoice {
    /// tc-store over the projects folder [`App::start`] chooses, with the app-data folder
    /// (unit C0). It is built only once the folder is chosen and checked, the first moment
    /// it is known, so a named folder that is missing is refused before anything makes it.
    OnDisk,
    /// A test's backend, or a parity scenario's (feature `fixture-backend`).
    Given(Arc<dyn ProjectStore>),
}

/// Where the app-data files are kept.
pub enum FilesChoice {
    /// The app-data folder on this computer's disk, each file written by
    /// `tc_store::atomic_write` (unit C1a). Built by [`App::start`], which first makes the
    /// folder.
    OnDisk,
    /// A test's files.
    Given(Arc<dyn AppDataFiles>),
}

/// What a test replaces when it starts the app. [`AppDeps::production`] is what `tc4 serve`
/// runs with.
pub struct AppDeps {
    /// Stamps what people or reports see. Timers use `tokio::time`.
    pub clock: Arc<dyn Clock>,
    /// Where "Stay signed in" keeps a Door43 token.
    pub keychain: Arc<dyn Keychain>,
    /// Which folders exist and which a cloud client copies, for the projects folder (Q-4).
    pub folders: Arc<dyn FolderCheck>,
    /// The computer's home folder, local data folder and kind.
    pub host: HostEnv,
    /// The app-data files `PrefsWriter` writes: the app-data folder on disk, or a test's.
    pub files: FilesChoice,
    /// The projects: tc-store on disk; the memory backend in tests and parity captures
    /// (feature `fixture-backend`).
    pub store: StoreChoice,
    /// What ends the process at a test crash point.
    #[cfg(feature = "test-hooks")]
    pub ender: Arc<dyn Ender>,
    /// The keychain the test hooks set (`test/keychain`, A8). When it is given and the hooks
    /// are on (`--test-hooks`), it is the app's keychain in place of `keychain`.
    #[cfg(feature = "test-hooks")]
    pub test_keychain: Option<Arc<MemKeychain>>,
}

impl AppDeps {
    /// This computer's clock, folders and variables; no keychain (the operating system's comes
    /// with unit E1a, Q-3: a keychain in memory, which forgets at quit, is not "Stay signed in
    /// on this computer", so the sign-in step hides that box until then, tC4 #496); the
    /// app-data folder and the projects through tc-store, both built by [`App::start`]; and,
    /// with test hooks, an ender that ends the process. It builds nothing itself:
    /// `..AppDeps::production()` evaluates it in full.
    pub fn production() -> Self {
        let host = HostEnv::current();
        Self {
            clock: Arc::new(SystemClock),
            keychain: Arc::new(MemKeychain::with_mode(MemKeychainMode::NoKeychain)),
            folders: Arc::new(HostFolders::new(host.clone())),
            host,
            files: FilesChoice::OnDisk,
            store: StoreChoice::OnDisk,
            #[cfg(feature = "test-hooks")]
            ender: Arc::new(ExitProcess),
            #[cfg(feature = "test-hooks")]
            test_keychain: None,
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
    #[cfg(feature = "test-hooks")]
    #[error("the test fields cannot be read: {0}")]
    TestFields(String),
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
    pub(crate) account: Account,
    pub(crate) bus: EventBus,
    pub(crate) jobs: JobRegistry,
    pub(crate) flows: FlowGate,
    pub(crate) prefs: PrefsWriter,
    pub(crate) reports: ReportLog,
    /// The reply cache of writes that name no project; a project's writes use its session's.
    pub(crate) replies: Arc<ReplyCache>,
    pub(crate) sessions: SessionRegistry,
    pub(crate) counters: Arc<AppCounters>,
    pub(crate) startup: Startup,
    /// This machine's resource store, under app data (STORAGE §3.7).
    pub(crate) resources: ResourceStore,
    /// The `Seed` job, when this launch started one.
    pub(crate) seed_job: Mutex<Option<JobId>>,
    /// Home's summaries (B2).
    pub(crate) home: crate::home::SummaryCache,
    /// The open from Door43 until unit X (B9b).
    pub(crate) open_door43: crate::share::ScriptedOpenDoor43,
    /// The sync with Door43 until unit X (B10): only a test scripts it.
    #[cfg(feature = "test-hooks")]
    pub(crate) sync: crate::sync::ScriptedSync,
    /// The client resources are read and downloaded with, on the production server whatever
    /// the account's server is (tC4 #120, B9a).
    pub(crate) resources_door43: Result<Door43Client, String>,
    /// The (package, book) pairs this app session downloaded, which Get source texts counts
    /// per package (tC4 `installedSrc`, B9a).
    pub(crate) downloaded: Mutex<std::collections::BTreeSet<(String, tc_core::BookId)>>,
    /// The plans a person is reading: upgrades, gateway changes and re-pins (C8).
    pub(crate) plans: crate::upgrades::KeptPlans,
    /// The server's uploads, which the guided fix's file install reads (C8). Given once, by the
    /// binary that runs the server.
    pub(crate) uploads: std::sync::OnceLock<crate::fixes::Uploads>,
    /// Calls in flight, for `test/quiet`.
    pub(crate) calls: AtomicUsize,
    quit: watch::Sender<bool>,
    /// `Some` only when `tc4 serve` was started with `--test-hooks`.
    #[cfg(feature = "test-hooks")]
    pub(crate) hooks: Option<TestHooks>,
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
    /// reads the kept sign-in once, opens the machine's resource store and plans its seeding
    /// from receipts alone, and starts the writers and, if the plan says so, the `Seed` job.
    /// Must run inside a tokio runtime.
    pub async fn start(config: AppConfig, deps: AppDeps) -> Result<App, StartError> {
        let AppDeps {
            clock,
            keychain,
            folders,
            host,
            files,
            store,
            #[cfg(feature = "test-hooks")]
            ender,
            #[cfg(feature = "test-hooks")]
            test_keychain,
        } = deps;
        // The test hooks set the test keychain; without them it is never the app's.
        #[cfg(feature = "test-hooks")]
        let test_keychain = test_keychain.filter(|_| config.test_hooks);
        #[cfg(feature = "test-hooks")]
        let keychain: Arc<dyn Keychain> = match &test_keychain {
            Some(test) => test.clone(),
            None => keychain,
        };

        std::fs::create_dir_all(&config.app_data).map_err(|e| StartError::AppData {
            path: config.app_data.clone(),
            reason: e.to_string(),
        })?;
        let files: Arc<dyn AppDataFiles> = match files {
            FilesChoice::OnDisk => {
                Arc::new(DiskFiles::open(Arc::new(tc_store::StdFs), &config.app_data))
            }
            FilesChoice::Given(files) => files,
        };

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
        // Only now: tc-store makes a projects folder that is missing, and a folder someone
        // named was checked above.
        let (store, on_disk): (Arc<dyn ProjectStore>, _) = match store {
            StoreChoice::OnDisk => {
                let backend = Arc::new(
                    StoreBackend::on_disk(&config.app_data, &projects.path).map_err(|e| {
                        StartError::AppData {
                            path: config.app_data.clone(),
                            reason: e.to_string(),
                        }
                    })?,
                );
                (backend.clone(), Some(backend))
            }
            StoreChoice::Given(store) => (store, None),
        };

        // Never a weaker source than the system's: the instance tells the UI that its
        // scopes and buffered requests belong to a dead server.
        let mut bytes = [0u8; 16];
        SystemRandom::new()
            .fill(&mut bytes)
            .map_err(|_| StartError::Random)?;
        let instance = ServerInstance::from_bytes(bytes);
        store.name_run(instance);

        let bus = EventBus::new(instance);
        // The reports an earlier launch kept come first, so this launch's ids follow theirs.
        let kept = read_reports(files.as_ref());
        let reports = ReportLog::with_kept(kept.reports, kept.last_id);
        let now = clock.now();
        if let Some(problem) = kept.problem {
            let file = crate::prefs::AppDataPath::reports();
            reports.add(
                ReportOp::PrefsWrite,
                ReportOutcome::Failed {
                    reason: format!("{} {problem}", file.as_str()),
                },
                facts(json!({ "file": file.as_str() })),
                now,
                now,
            );
        }
        if let Some(record) = projects_folder_record(&projects) {
            reports.add(
                ReportOp::ProjectsFolder,
                ReportOutcome::Ok,
                facts(record),
                now,
                now,
            );
        }

        if let Some(backend) = on_disk {
            crate::backend::store::report_removed_staging(backend, reports.clone(), clock.clone());
        }

        // The keychain is read here and nowhere else (but a test relaunch): an
        // operating-system keychain can prompt the person.
        let (account, unreadable) = AccountState::at_start(keychain.read(), &config.door43);
        if let Some(reason) = unreadable {
            reports.add(
                ReportOp::Keychain,
                ReportOutcome::Failed { reason },
                Map::new(),
                now,
                now,
            );
        }

        // Without the flag no `test-hooks/` folder is made.
        #[cfg(feature = "test-hooks")]
        let hooks = if config.test_hooks {
            Some(
                TestHooks::new(&config.app_data, ender, test_keychain, store.store_hooks())
                    .map_err(StartError::TestFields)?,
            )
        } else {
            None
        };

        // Opening the store clears what an interrupted install left in `.partial/`. A leftover
        // that cannot be removed (a scanner holds one of its files) stops nothing: installs
        // land by rename, each from a staging folder of its own, so it is reported and the
        // next start tries again. Only a store folder that cannot be made stops the start,
        // as an app-data folder that cannot be made does. The plan reads receipts and check
        // records, never a content file, so `hello` is exact from the first request (unit A9).
        let resources_dir = config.app_data.join(RESOURCES_FOLDER);
        let (resources, leftover) =
            ResourceStore::open_keeping_leftovers(&resources_dir).map_err(|e| {
                StartError::AppData {
                    path: resources_dir.clone(),
                    reason: e.to_string(),
                }
            })?;
        if let Some(left) = leftover {
            reports.add(
                ReportOp::Resources,
                ReportOutcome::Failed {
                    reason: left.to_string(),
                },
                facts(json!({"folder": resources_dir.display().to_string()})),
                now,
                now,
            );
        }
        let seed_start = seed::plan(&resources, config.bundle.as_deref());

        let prefs = PrefsWriter::start(files, reports.clone(), clock.clone());
        let jobs = JobRegistry::new(bus.clone(), reports.clone(), clock.clone());
        let counters = Arc::new(AppCounters::default());
        let sessions = SessionRegistry::new(
            store,
            bus.clone(),
            reports.clone(),
            clock.clone(),
            counters.clone(),
            tokio::runtime::Handle::current(),
        );
        let startup = Startup::new(seed::initial_state(&seed_start), bus.clone());
        // One client for every account call, on the server the build names (§5.6 item 9). The
        // switch is the app's own, so turning it off stops the client's next request.
        let internet = InternetSwitch::new();
        let door43 =
            Door43Client::new(config.door43.clone(), internet.clone()).map_err(|e| e.to_string());
        let resources_door43 = Door43Client::new(
            crate::resources::resources_server(&config.door43),
            internet.clone(),
        )
        .map_err(|e| e.to_string());
        let inner = Inner {
            config,
            projects,
            instance,
            clock,
            internet,
            net_lock: Mutex::new(()),
            account: Account::new(account, keychain, door43),
            bus,
            jobs,
            flows: FlowGate::default(),
            prefs,
            reports,
            replies: ReplyCache::new(ReplyCache::FINISHED),
            sessions,
            counters,
            startup,
            resources,
            seed_job: Mutex::new(None),
            home: crate::home::SummaryCache::default(),
            open_door43: crate::share::ScriptedOpenDoor43::default(),
            #[cfg(feature = "test-hooks")]
            sync: crate::sync::ScriptedSync::default(),
            resources_door43,
            downloaded: Mutex::new(std::collections::BTreeSet::new()),
            plans: crate::upgrades::KeptPlans::default(),
            uploads: std::sync::OnceLock::new(),
            calls: AtomicUsize::new(0),
            quit: watch::channel(false).0,
            #[cfg(feature = "test-hooks")]
            hooks,
        };
        let app = App {
            inner: Arc::new(inner),
        };
        app.begin_seed(seed_start);
        Ok(app)
    }

    /// One request, as tc-server receives it: `path` is `service/name`, without `/api/v1/`.
    pub async fn call(&self, path: &str, body: &[u8]) -> Answer {
        #[cfg(feature = "test-hooks")]
        if self.is_dead() {
            return Answer::Ended;
        }
        // `test/quiet` asks whether a call is in flight, so it does not count itself. Its name is
        // written only in a build with the hooks: a release binary holds no hook name (AD35).
        #[cfg(feature = "test-hooks")]
        let counted = path != <tc_api::test::Quiet as Endpoint>::PATH;
        #[cfg(not(feature = "test-hooks"))]
        let counted = true;
        let _in_flight = counted.then(|| InFlight::enter(&self.inner.calls));
        if let Some(answer) = self.route_product(path, body).await {
            return answer;
        }
        #[cfg(feature = "test-hooks")]
        if self.inner.hooks.is_some()
            && let Some(answer) = self.route_test_hooks(path, body).await
        {
            return answer;
        }
        Answer::NotFound
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

    /// Every open project of this run.
    pub fn sessions(&self) -> &SessionRegistry {
        &self.inner.sessions
    }

    /// What the reads of projects have cost so far (§8.1).
    pub fn counters(&self) -> AppCounts {
        self.inner.counters.read()
    }

    /// Gives the app the server's uploaded files: the guided fix installs a file the person
    /// chose. Once; the first call wins.
    pub fn provide_uploads(&self, uploads: crate::fixes::Uploads) {
        let _ = self.inner.uploads.set(uploads);
    }

    /// The request guard of a project-scoped query (§4.3): the screen's session at `at`, or
    /// `StaleScope` naming the folder's current epoch. It runs before any work.
    pub fn read(&self, at: &tc_api::Scope) -> Result<Arc<ProjectSession>, ApiError> {
        self.inner
            .sessions
            .reader(at)
            .map_err(|current| ApiError::StaleScope { current })
    }

    /// The first-launch state `hello` answers: `Ready` unless a seeding is under way or
    /// stopped (unit A9).
    pub fn startup(&self) -> StartupState {
        self.inner.startup.get()
    }

    /// What `app/quit`, a quit line and a closed stdin do: stops the first-launch seed and
    /// waits for it, closes every session after its leave checkpoint, writes what is
    /// pending, then resolves [`App::quitting`]. Safe to call more than once (the server
    /// calls it after `app/quit` did). Later units save aligner links first (§5.1).
    pub async fn quit(&self) {
        // Close is a quit, and a quit stops the seed first: the process lets go of
        // `app.lock` only when no seed writer runs (unit A9).
        self.stop_seed().await;
        self.inner.sessions.close_all().await;
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

/// The report of a choice of projects folder that "Ask for help" must be able to explain: a
/// first launch (Q-4), and a launch that finds a default folder beside the local one it
/// listed before.
fn projects_folder_record(projects: &ProjectsRoot) -> Option<Value> {
    let path = projects.path.display().to_string();
    match &projects.chosen {
        RootChoice::FirstLaunch => Some(json!({
            "path": path, "choice": "firstLaunch", "syncedBy": null,
        })),
        RootChoice::FirstLaunchLocal { default_synced_by } => Some(json!({
            "path": path, "choice": "firstLaunchLocal", "syncedBy": default_synced_by,
        })),
        RootChoice::FirstLaunchSynced { synced_by } => Some(json!({
            "path": path, "choice": "firstLaunchSynced", "syncedBy": synced_by,
        })),
        RootChoice::LocalBesideDefault { default } => Some(json!({
            "path": path, "choice": "localBesideDefault", "notListed": default.display().to_string(),
        })),
        RootChoice::Given | RootChoice::Existing | RootChoice::ExistingLocal => None,
    }
}
