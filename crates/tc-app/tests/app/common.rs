//! What the test modules share.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tc_api::app::{BuildInfo, OpenSession, SessionRole};
use tc_api::event::{Event, SessionMode};
use tc_api::ids::{Epoch, JobId, ProjectId, ProjectRef, RequestId, Scope, ServerInstance, WallMs};
use tc_api::jobs::{JobKind, JobOutcome, JobState, JobView, Phase, Progress};
use tc_api::prefs::PlaceMode;
use tc_app::account::{Keychain, KeychainError, MemKeychain};
use tc_app::clock::ManualClock;
use tc_app::config::{AppConfig, HostEnv, HostOs, ProjectsFolder};
use tc_app::events::{EventBus, Next, Subscription};
use tc_app::folders::{FolderCheck, HostFolders};
use tc_app::jobs::{JobCtx, JobEnd, JobRegistry, JobSpec};
use tc_app::prefs::{EditUnit, LastEdit, MemoryFiles, PrefsWriter};
use tc_app::reports::ReportLog;
use tc_app::sessions::{SessionDirectory, SessionInfo};
use tc_app::{App, AppDeps};
use tc_door43::Token;

/// A computer that says which folders exist and which a cloud client copies.
#[derive(Default)]
pub struct FakeFolders {
    pub folders: BTreeSet<PathBuf>,
    /// A path is copied when it, or an ancestor, is in this map.
    pub synced: BTreeMap<PathBuf, String>,
}

impl FolderCheck for FakeFolders {
    fn is_folder(&self, path: &Path) -> bool {
        self.folders.contains(path)
    }

    fn synced_by(&self, path: &Path) -> Option<String> {
        path.ancestors()
            .find_map(|ancestor| self.synced.get(ancestor).cloned())
    }
}

pub fn env(os: HostOs, vars: &[(&str, &str)]) -> HostEnv {
    HostEnv::new(
        os,
        vars.iter()
            .map(|(name, value)| ((*name).to_owned(), OsString::from(*value))),
    )
}

pub fn instance() -> ServerInstance {
    ServerInstance::from_bytes([7; 16])
}

pub fn progress(done: u64) -> Progress {
    Progress {
        phase: Phase::Seed,
        done,
        total: Some(1000),
        item: None,
    }
}

pub fn job_event(job: u64, state: JobState) -> Event {
    Event::Job(JobView {
        id: JobId::new(job),
        kind: JobKind::Download,
        project: None,
        state,
    })
}

/// A progress frame of a job.
pub fn running(job: u64, done: u64) -> Event {
    job_event(job, JobState::Running(progress(done)))
}

/// A job registry with its bus and report log.
pub struct Jobs {
    pub jobs: JobRegistry,
    pub bus: EventBus,
    pub reports: ReportLog,
}

pub fn jobs() -> Jobs {
    let bus = EventBus::new(instance());
    let reports = ReportLog::new();
    let jobs = JobRegistry::new(
        bus.clone(),
        reports.clone(),
        Arc::new(ManualClock::new(1_000)),
    );
    Jobs { jobs, bus, reports }
}

/// A job a test drives by hand: it hands its `JobCtx` out, and ends when the test says so.
pub struct ManualJob {
    pub id: JobId,
    pub ctx: JobCtx,
    end: tokio::sync::oneshot::Sender<Result<JobOutcome, JobEnd>>,
}

impl ManualJob {
    /// Starts the job and lets it run until it has given its context out. Never awaits a
    /// channel, which would let a paused clock jump ahead.
    pub async fn start(jobs: &JobRegistry, spec: JobSpec) -> ManualJob {
        let (ctx_tx, ctx_rx) = std::sync::mpsc::channel();
        let (end, end_rx) = tokio::sync::oneshot::channel();
        let id = jobs.start_async(spec, move |ctx| async move {
            let given = JobCtx {
                progress: ctx.progress.clone(),
                cancel: ctx.cancel.clone(),
            };
            let _ = ctx_tx.send(given);
            end_rx.await.unwrap_or(Err(JobEnd::Cancelled))
        });
        for _ in 0..100 {
            tokio::task::yield_now().await;
            if let Ok(ctx) = ctx_rx.try_recv() {
                return ManualJob { id, ctx, end };
            }
        }
        panic!("the job never started");
    }

    pub fn finish(self, result: Result<JobOutcome, JobEnd>) {
        let _ = self.end.send(result);
    }
}

/// Lets every ready task run, without moving a paused clock.
pub async fn settle() {
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
}

/// Moves a paused clock forward and lets the tasks it woke run.
pub async fn step(ms: u64) {
    tokio::time::advance(Duration::from_millis(ms)).await;
    settle().await;
}

/// The job frames a reader has, each with the time the test says it was read at.
pub fn drain_jobs(subscription: &mut Subscription, at_ms: u64, into: &mut Vec<(u64, JobState)>) {
    while let Some(next) = subscription.try_next() {
        match next {
            Next::Frame(frame) => {
                if let Event::Job(view) = frame.event {
                    into.push((at_ms, view.state));
                }
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

/// The count of a `Running` state.
pub fn done_of(state: &JobState) -> u64 {
    match state {
        JobState::Running(progress) => progress.done,
        other => panic!("not running: {other:?}"),
    }
}

/// A prefs writer over memory files, with the clock and report log it was started with.
pub struct Prefs {
    pub prefs: PrefsWriter,
    pub files: Arc<MemoryFiles>,
    pub reports: ReportLog,
    pub clock: Arc<ManualClock>,
}

/// Starts a prefs writer over `files`. Must run inside a tokio runtime.
pub fn prefs_over(files: Arc<MemoryFiles>) -> Prefs {
    let reports = ReportLog::new();
    let clock = Arc::new(ManualClock::new(10_000));
    let prefs = PrefsWriter::start(files.clone(), reports.clone(), clock.clone());
    Prefs {
        prefs,
        files,
        reports,
        clock,
    }
}

pub fn prefs() -> Prefs {
    prefs_over(Arc::new(MemoryFiles::new()))
}

pub fn rid(n: u8) -> RequestId {
    RequestId::from_bytes([n; 16])
}

pub fn token(secret: &str) -> Token {
    Token::new(secret.to_owned()).unwrap()
}

/// Sessions a test opens by hand.
#[derive(Default)]
pub struct FakeSessions {
    pub open: Mutex<Vec<(Scope, SessionInfo, OpenSession)>>,
}

impl FakeSessions {
    pub fn add(&self, scope: Scope, info: SessionInfo, open: OpenSession) {
        self.open.lock().unwrap().push((scope, info, open));
    }
}

impl SessionDirectory for FakeSessions {
    fn resolve(&self, at: &Scope) -> Option<SessionInfo> {
        let open = self.open.lock().unwrap();
        open.iter()
            .find(|(scope, _, _)| scope == at)
            .map(|(_, info, _)| info.clone())
    }

    fn open_sessions(&self) -> Vec<OpenSession> {
        let open = self.open.lock().unwrap();
        open.iter().map(|(_, _, session)| session.clone()).collect()
    }
}

/// A keychain that counts its reads.
pub struct CountingKeychain {
    pub inner: MemKeychain,
    pub reads: AtomicUsize,
}

impl CountingKeychain {
    pub fn new(inner: MemKeychain) -> Self {
        Self {
            inner,
            reads: AtomicUsize::new(0),
        }
    }
}

impl Keychain for CountingKeychain {
    fn keep(&self, token: &Token) -> Result<(), KeychainError> {
        self.inner.keep(token)
    }

    fn read(&self) -> Result<Option<Token>, KeychainError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.inner.read()
    }

    fn forget(&self) -> Result<(), KeychainError> {
        self.inner.forget()
    }
}

/// What an app is started over, so a test can seed it before the start and start a second
/// app over the same things.
#[derive(Clone)]
pub struct Parts {
    pub files: Arc<MemoryFiles>,
    pub keychain: Arc<MemKeychain>,
    pub sessions: Arc<FakeSessions>,
    pub clock: Arc<ManualClock>,
}

impl Parts {
    pub fn new() -> Self {
        Self {
            files: Arc::new(MemoryFiles::new()),
            keychain: Arc::new(MemKeychain::new()),
            sessions: Arc::new(FakeSessions::default()),
            clock: Arc::new(ManualClock::new(1_000_000)),
        }
    }
}

pub fn build() -> BuildInfo {
    BuildInfo {
        version: "4.0.0-test".to_owned(),
        commit: "0123abc".to_owned(),
    }
}

/// An app on real temporary folders, over a described computer.
pub struct TestApp {
    pub app: App,
    pub app_data: PathBuf,
    pub home: PathBuf,
    pub local: PathBuf,
    pub files: Arc<MemoryFiles>,
    pub keychain: Arc<MemKeychain>,
    pub sessions: Arc<FakeSessions>,
    pub clock: Arc<ManualClock>,
}

impl TestApp {
    /// An app on `dir/app-data`, with `HOME` and `USERPROFILE` at `dir/home`, `XDG_DATA_HOME`
    /// at `dir/local`, the real folder check over that description, and the default projects
    /// folder. Test hooks are on. The caller owns the temporary folder, so a test can start
    /// a second app on it.
    ///
    /// The computer is described and never read from the process, so a developer's OneDrive
    /// variable cannot change a result.
    pub async fn start_in(dir: &Path, edit: impl FnOnce(&mut AppConfig, &mut AppDeps)) -> TestApp {
        Self::start_with(dir, &Parts::new(), edit).await
    }

    pub async fn start_with(
        dir: &Path,
        parts: &Parts,
        edit: impl FnOnce(&mut AppConfig, &mut AppDeps),
    ) -> TestApp {
        let app_data = dir.join("app-data");
        let home = dir.join("home");
        let local = dir.join("local");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&local).unwrap();
        let host = env(
            HostOs::Linux,
            &[
                ("HOME", home.to_str().unwrap()),
                ("USERPROFILE", home.to_str().unwrap()),
                ("XDG_DATA_HOME", local.to_str().unwrap()),
            ],
        );
        let mut config = AppConfig {
            app_data: app_data.clone(),
            projects: ProjectsFolder::Default,
            door43: "http://127.0.0.1:9".parse().unwrap(),
            build: build(),
            test_hooks: true,
        };
        let mut deps = AppDeps {
            clock: parts.clock.clone(),
            keychain: parts.keychain.clone(),
            folders: Arc::new(HostFolders::new(host.clone())),
            host,
            files: parts.files.clone(),
            sessions: parts.sessions.clone(),
        };
        edit(&mut config, &mut deps);
        let app = App::start(config, deps).await.unwrap();
        TestApp {
            app,
            app_data,
            home,
            local,
            files: parts.files.clone(),
            keychain: parts.keychain.clone(),
            sessions: parts.sessions.clone(),
            clock: parts.clock.clone(),
        }
    }
}

pub const PROJECT_ID: &str = "6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50";

pub fn scope(epoch: u64) -> Scope {
    Scope {
        project: ProjectRef::parse("kr_ulb").unwrap(),
        epoch: Epoch::new(epoch),
    }
}

/// An open session of folder `kr_ulb` at `epoch`, as the directory and `hello` see it.
pub fn open_session(epoch: u64) -> (Scope, SessionInfo, OpenSession) {
    let at = scope(epoch);
    let id = ProjectId::parse(PROJECT_ID).unwrap();
    let info = SessionInfo {
        project: at.project.clone(),
        id: id.clone(),
    };
    let open = OpenSession {
        at: at.clone(),
        id,
        mode: SessionMode::Writable,
        role: SessionRole::Screen,
    };
    (at, info, open)
}

/// The `value` of `app/hello`, as the UI reads it.
pub async fn hello_value(app: &App) -> serde_json::Value {
    match app.call("app/hello", b"{}").await {
        tc_app::Answer::Reply { body, .. } => {
            let mut json: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(json["type"], "ok");
            json["value"].take()
        }
        other => panic!("{other:?}"),
    }
}

pub fn last_edit() -> LastEdit {
    LastEdit {
        project: ProjectRef::parse("titus").unwrap(),
        unit: EditUnit::Book {
            book: "TIT".parse().unwrap(),
            chapter: 1,
            verse: tc_api::ids::RefPart::parse("4").unwrap(),
        },
        mode: PlaceMode::Draft,
        tool: None,
        snippet: "Paul".to_owned(),
        at: WallMs(5),
    }
}
