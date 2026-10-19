//! What the test modules share.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde::de::DeserializeOwned;
use tc_api::app::{BuildInfo, OpenSession};
use tc_api::event::Event;
use tc_api::ids::{Epoch, JobId, ProjectRef, RequestId, Scope, ServerInstance, WallMs};
use tc_api::jobs::{JobKind, JobOutcome, JobState, JobView, Phase, Progress};
use tc_api::prefs::PlaceMode;
use tc_api::test::{FieldLog, FieldLogView, FieldStore};
use tc_api::write::Ack;
use tc_api::{ApiError, Endpoint, Reply, ShownToken};
use tc_app::account::{Keychain, KeychainError, MemKeychain};
use tc_app::backend::memory::{MemoryBackend, MemoryProject};
use tc_app::clock::ManualClock;
use tc_app::config::{AppConfig, HostEnv, HostOs, ProjectsFolder};
use tc_app::events::{EventBus, Next, Subscription};
use tc_app::folders::{FolderCheck, HostFolders};
use tc_app::hooks::RecordingEnder;
use tc_app::jobs::{JobCtx, JobEnd, JobRegistry, JobSpec};
use tc_app::prefs::{EditUnit, LastEdit, MemoryFiles, PrefsWriter};
use tc_app::reports::ReportLog;
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
    /// The projects: `kr_ulb`, an empty Bible project.
    pub store: Arc<MemoryBackend>,
    pub clock: Arc<ManualClock>,
    pub ender: Arc<RecordingEnder>,
}

impl Parts {
    pub fn new() -> Self {
        let store = MemoryBackend::new();
        store
            .add(
                ProjectRef::parse("kr_ulb").unwrap(),
                MemoryProject::empty(
                    tc_journal::ProjectId::parse(PROJECT_ID).unwrap(),
                    tc_journal::fold::ProjectKind::Bible,
                ),
            )
            .unwrap();
        Self {
            files: Arc::new(MemoryFiles::new()),
            keychain: Arc::new(MemKeychain::new()),
            store: Arc::new(store),
            clock: Arc::new(ManualClock::new(1_000_000)),
            ender: Arc::new(RecordingEnder::default()),
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
    pub store: Arc<MemoryBackend>,
    pub clock: Arc<ManualClock>,
    pub ender: Arc<RecordingEnder>,
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
        let app = try_start(dir, parts, edit).await.unwrap();
        TestApp {
            app,
            app_data: dir.join("app-data"),
            home: dir.join("home"),
            local: dir.join("local"),
            files: parts.files.clone(),
            keychain: parts.keychain.clone(),
            store: parts.store.clone(),
            clock: parts.clock.clone(),
            ender: parts.ender.clone(),
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

/// Opens folder `kr_ulb` for the screen, as B3's open will.
pub async fn open_kr_ulb(app: &App) -> OpenSession {
    app.sessions()
        .open_screen(&ProjectRef::parse("kr_ulb").unwrap(), open_progress())
        .await
        .unwrap()
}

/// A progress sink for an open.
pub fn open_progress() -> tc_app::jobs::ProgressSink {
    tc_app::jobs::ProgressSink::new(Phase::Open(tc_api::jobs::OpenPhase::ReadingChanges))
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

/// The request as the body the server receives.
pub fn body_of<E: Endpoint>(req: &E) -> Vec<u8> {
    serde_json::to_vec(req).unwrap()
}

/// The raw answer to a request.
pub async fn answer<E: Endpoint>(app: &App, req: &E) -> tc_app::Answer {
    app.call(E::PATH, &body_of(req)).await
}

/// The typed reply inside an answer that has one.
pub fn reply_in<T: DeserializeOwned>(answer: &tc_app::Answer) -> Reply<T> {
    match answer {
        tc_app::Answer::Reply { body, .. } => serde_json::from_slice(body).unwrap(),
        other => panic!("{other:?}"),
    }
}

/// A request and its reply, as a result.
pub async fn post<E: Endpoint>(app: &App, req: &E) -> Result<E::Ok, ApiError> {
    app.request(req).await.unwrap().into_result()
}

pub async fn field_log(app: &App, field: &str) -> FieldLogView {
    post(
        app,
        &FieldLog {
            field: field.to_owned(),
        },
    )
    .await
    .unwrap()
}

/// The token of a field's current version.
pub async fn field_token(app: &App, field: &str) -> ShownToken {
    field_log(app, field).await.current.token
}

pub fn store_req(id: u8, field: &str, token: &ShownToken, text: &str) -> FieldStore {
    FieldStore {
        request: rid(id),
        field: field.to_owned(),
        shown: token.clone(),
        text: text.to_owned(),
    }
}

pub async fn store(
    app: &App,
    id: u8,
    field: &str,
    token: &ShownToken,
    text: &str,
) -> Result<Ack, ApiError> {
    post(app, &store_req(id, field, token, text)).await
}

/// Waits, in real time, until `cond` holds.
pub async fn until(what: &str, mut cond: impl FnMut() -> bool) {
    for _ in 0..5000 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!("never happened: {what}");
}

pub async fn is_busy(app: &App) -> bool {
    post(app, &tc_api::test::Quiet {}).await.unwrap().busy
}

/// Waits until `test/quiet` says nothing is in flight.
pub async fn until_quiet(app: &App) {
    for _ in 0..5000 {
        if !is_busy(app).await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!("the app never went quiet");
}

/// The bytes of the test fields' file.
pub fn fields_file(app_data: &Path) -> Vec<u8> {
    std::fs::read(app_data.join("test-hooks").join("fields.json")).unwrap()
}

/// Waits until exactly `count` operations are held.
pub async fn until_held_count(app: &App, count: usize) {
    until("an operation reaches its hold", || app.holding() == count).await;
}

/// Starts an app as `TestApp::start_with` does, and keeps a start error.
pub async fn try_start(
    dir: &Path,
    parts: &Parts,
    edit: impl FnOnce(&mut AppConfig, &mut AppDeps),
) -> Result<App, tc_app::StartError> {
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
        app_data: dir.join("app-data"),
        projects: ProjectsFolder::Default,
        door43: "http://127.0.0.1:9".parse().unwrap(),
        build: build(),
        bundle: None,
        test_hooks: true,
        seed_pace: None,
    };
    let mut deps = AppDeps {
        clock: parts.clock.clone(),
        keychain: parts.keychain.clone(),
        folders: Arc::new(HostFolders::new(host.clone())),
        host,
        files: parts.files.clone(),
        store: parts.store.clone(),
        ender: parts.ender.clone(),
    };
    edit(&mut config, &mut deps);
    App::start(config, deps).await
}

// ---- Projects of the memory backend -------------------------------------------------------

/// The repository's root.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The sample Bible project (`fixtures/tc4/sample-burrito`, Titus and Jonah) through B1a's
/// seed, folded once per test binary. Each backend it is added to counts its own work.
pub fn sample_bible() -> tc_app::backend::memory::MemoryProject {
    use std::sync::OnceLock;
    use tc_app::backend::memory::MemoryProject;
    use tc_app::backend::seed::{Tc4Folder, fixture_project_id, tc4_state_seed};
    static SEED: OnceLock<MemoryProject> = OnceLock::new();
    SEED.get_or_init(|| {
        let folder = Tc4Folder::read(&repo_root().join("fixtures/tc4/sample-burrito")).unwrap();
        let seed = tc4_state_seed(&folder, fixture_project_id("sample_burrito")).unwrap();
        MemoryProject::from_seed(seed, None)
    })
    .clone()
}

/// A verse of Titus.
pub fn tit(chapter: u16, verse: u16) -> tc_journal::RecordKey {
    tc_journal::RecordKey::Verse(
        tc_journal::BookId::Titus,
        tc_journal::ChapterNo::new(chapter).unwrap(),
        tc_journal::VerseNo::new(verse).unwrap(),
    )
}

/// A project folder name.
pub fn folder(name: &str) -> ProjectRef {
    ProjectRef::parse(name).unwrap()
}

/// A memory backend holding the sample Bible project as `sample_burrito`.
pub fn sample_backend() -> tc_app::backend::memory::MemoryBackend {
    let backend = tc_app::backend::memory::MemoryBackend::new();
    backend
        .add(folder("sample_burrito"), sample_bible())
        .unwrap();
    backend
}
