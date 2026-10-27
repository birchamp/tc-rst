//! The store's world for tc-app's tests (unit C0): tc-store on its in-memory fault-injecting
//! file system, behind [`StoreBackend`], holding the sample Bible project (Titus and Jonah,
//! the bytes the memory fixture reads), the same project with two heads on Titus 1:1, and a
//! project of another id.
//!
//! The projects are written once per test binary through tc-store's own create; each world
//! is an independent copy of those files with a store of its own, so a test can crash, fail
//! or fill its files without touching another's.
#![allow(
    dead_code,
    reason = "test code: a failed unwrap is a failed test, and not every test binary uses each shared helper"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use tc_api::ids::ProjectRef;
use tc_app::backend::store::StoreBackend;
use tc_app::backend::{EditToken, ProjectHandle, ProjectStore, Snapshot, StoreCommand, StoreReply};
use tc_journal::command::Command;
use tc_journal::fold::{ProjectKind, records_from_usfm};
use tc_journal::record::{BookPresence, PresentHow, Scope, VerseValue, VersificationValue};
use tc_journal::{BookId, ChapterNo, MetaField, MetaPath, Record, RecordKey, VerseNo};
use tc_store::fault::FaultFs;
use tc_store::{FakePlatform, Hooks, ManualClock, NewProject, OpenOptions, StoreConfig};

pub const APP: &str = "/app";
pub const PROJECTS: &str = "/projects";
/// 2026-10-17T00:00:00Z, the fault file system's own start.
pub const START_MS: i64 = 1_792_195_200_000;

/// One test's store.
pub struct StoreWorld {
    pub fs: FaultFs,
    pub hooks: Arc<Hooks>,
    pub backend: Arc<StoreBackend>,
    /// P5/INV-VALID-BURRITO: checks every state the backend wrote when the world is dropped.
    pub watch: BurritoWatch,
    /// The sample Bible project (Titus and Jonah).
    pub bible: ProjectRef,
    /// The same project with two heads on Titus 1:1.
    pub conflicted: (ProjectRef, RecordKey),
    /// A project of another id, so another token key.
    pub other: ProjectRef,
    /// The sample's USFM files' bytes, by book.
    pub sources: BTreeMap<BookId, Vec<u8>>,
}

impl StoreWorld {
    /// The project folder of `project`.
    pub fn folder(&self, project: &ProjectRef) -> PathBuf {
        Path::new(PROJECTS).join(project.as_str())
    }

    /// The store as the seam's.
    pub fn store(&self) -> Arc<dyn ProjectStore> {
        self.backend.clone()
    }

    /// Opens `project` through the seam and waits until the open's background work is done.
    pub fn open(&self, project: &ProjectRef) -> Arc<dyn ProjectHandle> {
        let handle = self.backend.open(project, progress()).unwrap();
        handle.wait_idle();
        handle
    }
}

/// INV-WAITS-CATCH-HANGS: one turn per poll of a test's wait, for a minute: a wait this long is a hang, never a slow
/// machine (rulings/2026-10-23-tests-prove-behavior-not-speed.md). The test fails after the
/// loop, as it did when the wait was a count of short sleeps.
pub fn for_a_minute() -> impl Iterator<Item = ()> {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(60);
    std::iter::from_fn(move || (std::time::Instant::now() < end).then_some(()))
}

/// Polls `future` once. An open that joins one already running registers on its first poll,
/// so a test knows it has joined without sleeping.
pub async fn poll_once<F: std::future::Future + Unpin>(future: &mut F) -> Option<F::Output> {
    std::future::poll_fn(|cx| {
        std::task::Poll::Ready(match std::pin::Pin::new(&mut *future).poll(cx) {
            std::task::Poll::Ready(out) => Some(out),
            std::task::Poll::Pending => None,
        })
    })
    .await
}

/// The open's progress sink of a test that does not watch it.
pub fn progress() -> tc_app::jobs::ProgressSink {
    tc_app::jobs::ProgressSink::new(tc_api::jobs::Phase::Open(
        tc_api::jobs::OpenPhase::ReadingChanges,
    ))
}

/// The store of installation `app` on `fs`, as the computer `platform` describes it.
pub fn backend_over(
    fs: &FaultFs,
    app: &str,
    platform: FakePlatform,
    hooks: Arc<Hooks>,
) -> (StoreBackend, BurritoWatch) {
    let mut watch = watch_over(fs, hooks.clone());
    let backend = StoreBackend::start(
        StoreConfig {
            app_data: PathBuf::from(app),
            projects: PathBuf::from(PROJECTS),
            vfs: Arc::new(fs.clone()),
            platform: Arc::new(platform),
            clock: Arc::new(ManualClock::at(START_MS)),
            threads: 2,
            watch_debounce: std::time::Duration::from_millis(20),
        },
        hooks,
    )
    .unwrap();
    // The start removes staging folders a crash left (N1); a test that plants one plants it
    // after this.
    backend.store().wait_cleanup();
    watch.follow(&backend);
    (backend, watch)
}

/// The files every world starts from, written once per test binary.
struct Base {
    fs: FaultFs,
    sources: BTreeMap<BookId, Vec<u8>>,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn json(text: &str) -> tc_journal::Json {
    tc_journal::json::parse(text.as_bytes()).unwrap()
}

/// The records of the sample project: its details, its versification, and each book through
/// tc-journal's codec. tc-store's own `AddBook` seeds tC4's blank book from a source, so the
/// sample's text comes in as records, as the fixture seed brings it (`backend/seed.rs`).
pub fn sample_records(sources: &BTreeMap<BookId, Vec<u8>>, vrs: Option<&[u8]>) -> Vec<Record> {
    let meta = |field: MetaField, value: &str| Record::ProjectMeta {
        field,
        value: Some(json(value)),
    };
    let extra = |path: &str| MetaField::Extra(MetaPath::parse(path).unwrap());
    let mut records = vec![
        meta(MetaField::Name, r#"{"en":"Sample translation"}"#),
        meta(MetaField::Abbreviation, r#"{"en":"smp"}"#),
        meta(
            MetaField::Languages,
            r#"[{"name":{"en":"English"},"scriptDirection":"ltr","tag":"en"}]"#,
        ),
        meta(extra("meta.dateCreated"), r#""2026-10-17T00:00:00.000Z""#),
        meta(extra("meta.defaultLocale"), r#""en""#),
        Record::Versification {
            value: match vrs {
                Some(bytes) => VersificationValue {
                    name: None,
                    bytes: String::from_utf8(bytes.to_vec()).unwrap(),
                },
                None => VersificationValue {
                    name: Some("eng".into()),
                    bytes: tc_versification::StandardScheme::Eng.json().to_owned(),
                },
            },
        },
    ];
    for (book, bytes) in sources {
        records.push(Record::BookPresence {
            book: *book,
            value: BookPresence::Present {
                scope: Scope::Whole,
                how: PresentHow::Added,
            },
        });
        let parts = tc_usfm::codec::decompose_bytes(bytes).unwrap();
        records.extend(records_from_usfm(*book, &parts));
    }
    records
}

fn base() -> &'static Base {
    static BASE: OnceLock<Base> = OnceLock::new();
    BASE.get_or_init(|| {
        let sample =
            tc_app::backend::scenario::ScenarioState::from_repository(&repo_root(), "sample")
                .unwrap();
        let file = |rel: &str| {
            sample
                .files()
                .get(&format!("repos/_local_/_local_/sample_burrito/{rel}"))
                .cloned()
        };
        let sources = BTreeMap::from([
            (BookId::Titus, file("ingredients/TIT.usfm").unwrap()),
            (BookId::Jonah, file("ingredients/JON.usfm").unwrap()),
        ]);
        let vrs = file("ingredients/vrs.json");
        let records = sample_records(&sources, vrs.as_deref());

        let fs = FaultFs::new();
        let (backend, _base_watch) =
            backend_over(&fs, APP, FakePlatform::default(), Arc::default());
        for name in [
            "sample_burrito",
            "sample_burrito_teammates",
            "another_project",
        ] {
            let folder = backend
                .store()
                .create(&NewProject {
                    name: name.to_owned(),
                    kind: ProjectKind::Bible,
                    records: records.clone(),
                    books: Vec::new(),
                })
                .unwrap();
            assert_valid_burrito(&fs, &folder);
        }

        // Two heads on Titus 1:1, both from one token: the second save did not see the
        // first, so the two stand as a conflict (tc-store's
        // `a_stale_token_makes_a_conflict_never_a_revert`). Both are this computer's: S1 has
        // no way yet to put a teammate's pack in a folder (AD36 gives that to C1a), and the
        // cases need only two heads.
        let folder = Path::new(PROJECTS).join("sample_burrito_teammates");
        let handle = backend
            .store()
            .open(&folder, &OpenOptions::default())
            .unwrap();
        let key = tit(1, 1);
        let shown = handle.snapshot().token(&key, None).unwrap();
        for words in ["Paul, a servant of God", "Paul, a slave of God"] {
            handle
                .submit(Command::SaveVerse {
                    shown: shown.clone(),
                    words: words.to_owned(),
                })
                .wait()
                .unwrap();
        }
        assert_eq!(handle.snapshot().fold().heads(&key).len(), 2);
        handle.close().unwrap();
        assert_valid_burrito(&fs, &folder);
        drop(backend);
        Base {
            fs: fs.crash(false),
            sources,
        }
    })
}

/// A world on its own copy of the files.
pub fn store_world() -> StoreWorld {
    store_world_with(FakePlatform::default())
}

/// Adds the real OBS fixture to a test's isolated store, through the store's Burrito writer.
pub fn add_obs(world: &StoreWorld) -> ProjectRef {
    use tc_app::backend::seed::{Tc4Folder, fixture_project_id, tc4_state_seed};
    let source = Tc4Folder::read(&repo_root().join("fixtures/tc4/sample-burrito-obs")).unwrap();
    let seed = tc4_state_seed(&source, fixture_project_id("sample_burrito_obs")).unwrap();
    let records = seed
        .fold
        .record_keys()
        .into_iter()
        .filter_map(|key| seed.fold.projected(&key))
        .collect();
    let path = world
        .backend
        .store()
        .create(&NewProject {
            name: "sample_burrito_obs".to_owned(),
            kind: ProjectKind::Stories,
            records,
            books: Vec::new(),
        })
        .unwrap();
    assert_valid_burrito(&world.fs, &path);
    folder("sample_burrito_obs")
}

/// A world whose computer `platform` describes: a cloud-synced folder, another host.
pub fn store_world_with(platform: FakePlatform) -> StoreWorld {
    let base = base();
    let fs = base.fs.copy();
    let hooks = Arc::new(Hooks::default());
    let (backend, watch) = backend_over(&fs, APP, platform, hooks.clone());
    StoreWorld {
        fs,
        hooks,
        backend: Arc::new(backend),
        watch,
        bible: folder("sample_burrito"),
        conflicted: (folder("sample_burrito_teammates"), tit(1, 1)),
        other: folder("another_project"),
        sources: base.sources.clone(),
    }
}

pub fn folder(name: &str) -> ProjectRef {
    ProjectRef::parse(name).unwrap()
}

pub fn tit(chapter: u16, verse: u16) -> RecordKey {
    RecordKey::Verse(
        BookId::Titus,
        ChapterNo::new(chapter).unwrap(),
        VerseNo::new(verse).unwrap(),
    )
}

/// The words of a verse as `snapshot` shows them.
pub fn words_of(snapshot: &Snapshot, key: &RecordKey) -> Option<String> {
    match snapshot.fold().projected(key)? {
        Record::Verse {
            value: VerseValue::Text { words, .. },
            ..
        } => Some(words),
        _ => None,
    }
}

/// An edit token for `key` as the handle's newest snapshot shows it.
pub fn token(handle: &dyn ProjectHandle, key: &RecordKey) -> EditToken {
    handle.snapshot().mint(key, None).unwrap()
}

/// Saves `words` over `key` as the newest snapshot shows it, through the seam.
pub async fn save(
    handle: &dyn ProjectHandle,
    key: &RecordKey,
    words: &str,
) -> Result<StoreReply, tc_app::backend::CommandError> {
    handle
        .submit(StoreCommand::save_verse(
            token(handle, key),
            words.to_owned(),
        ))
        .await
}

/// Every file under `folder`, by its path relative to it.
pub fn files(fs: &FaultFs, folder: &Path) -> BTreeMap<String, Vec<u8>> {
    fs.files_under(folder)
        .into_iter()
        .filter_map(|(path, bytes)| {
            let rel = path
                .strip_prefix(folder)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            Some((rel, bytes))
        })
        .collect()
}

/// Checks the folder as a Scripture Burrito (AGENTS.md, PLAN P5): metadata, ingredient sizes
/// and md5s, paths and scopes, as tc-store's own tests do.
pub fn assert_valid_burrito(fs: &FaultFs, folder: &Path) {
    assert_valid_burrito_files(&files(fs, folder));
}

/// The same check over files read from anywhere (a real folder on disk).
pub fn assert_valid_burrito_files(files: &BTreeMap<String, Vec<u8>>) {
    let problems = burrito_problems_of(files);
    assert!(problems.is_empty(), "not a valid burrito: {problems:#?}");
}

/// What `tc_burrito` finds wrong with a project's files as a Scripture Burrito (`.tc/` is the
/// store's own and left out).
pub fn burrito_problems_of(files: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    let Some(metadata) = files.get("metadata.json") else {
        return vec!["no metadata.json".to_owned()];
    };
    let mut project = tc_burrito::MemProject::new();
    for (rel, bytes) in files {
        if !rel.starts_with(".tc/") {
            project.insert(rel.clone(), bytes.clone());
        }
    }
    tc_burrito::validate_project(metadata, &project)
        .into_iter()
        .map(|p| format!("{p:?}"))
        .collect()
}

/// A Burrito's own files: `metadata.json` and everything under `ingredients/`. The store's
/// `.tc/` history, which can be large, is never read, so the probe costs what it checks.
fn burrito_files(fs: &FaultFs, folder: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = files(fs, &folder.join("ingredients"))
        .into_iter()
        .map(|(rel, bytes)| (format!("ingredients/{rel}"), bytes))
        .collect::<BTreeMap<_, _>>();
    if let Some(metadata) = fs.peek(&folder.join("metadata.json")) {
        out.insert("metadata.json".to_owned(), metadata);
    }
    out
}

/// P5/INV-VALID-BURRITO: the probe the store's writer runs after every job that wrote files,
/// outside a write group (STORAGE §16.6, I7). Each problem names its folder.
pub fn burrito_probe(fs: &FaultFs) -> impl Fn(&Path) -> Vec<String> + Send + Sync + 'static {
    let fs = fs.clone();
    move |folder: &Path| {
        burrito_problems_of(&burrito_files(&fs, folder))
            .into_iter()
            .map(|p| format!("{} is not a valid Scripture Burrito: {p}", folder.display()))
            .collect()
    }
}

/// Has the writer of the store that `hooks` go to look at every state it leaves on `fs`, for a
/// test that starts its store itself (a hold before the start, a counting filesystem, another
/// platform); [`backend_over`] calls it for every other store.
pub fn watch_over(fs: &FaultFs, hooks: Arc<Hooks>) -> BurritoWatch {
    hooks.check_burrito(burrito_probe(fs));
    BurritoWatch {
        hooks,
        fs: fs.clone(),
        stopped: false,
        open_projects: None,
    }
}

/// P5/INV-VALID-BURRITO without the test asking: every project state a store's writer leaves
/// is looked at as a Scripture Burrito, and the test fails when this watch is dropped if one
/// was not valid. Keep it until the test ends.
#[must_use = "keep the watch until the test ends: dropping it checks what the store wrote"]
pub struct BurritoWatch {
    hooks: Arc<Hooks>,
    fs: FaultFs,
    stopped: bool,
    /// Waits until each project the store still has open has written all it was asked to,
    /// and answers how many are open.
    open_projects: Option<Arc<dyn Fn() -> usize + Send + Sync>>,
}

impl BurritoWatch {
    /// Has the watch wait for the projects `backend` opens before it reads what the writer saw.
    pub fn follow(&mut self, backend: &StoreBackend) {
        self.open_projects = Some(Arc::new(backend.idle_waiter()));
    }

    /// The test leaves a folder that is not a valid Burrito on purpose (an outside edit, a
    /// damaged or removed file), so the watch stops looking; `why` says which.
    pub fn stop(&mut self, why: &str) {
        assert!(!why.trim().is_empty(), "a stopped watch says why");
        self.hooks.check_burrito(|_: &Path| Vec::new());
        self.stopped = true;
    }
}

impl Drop for BurritoWatch {
    fn drop(&mut self) {
        // A failing test, a filesystem killed on purpose, or a stopped watch says nothing more.
        if std::thread::panicking() || self.fs.is_dead() || self.stopped {
            return;
        }
        // The writer looks at the folder after each job. A project still open is waited on until
        // it has written all it was asked to; one closing lets go of its lock only after its last
        // job. So once every lock held belongs to an idle open project, every state the test's
        // saves wrote was seen. A minute is a hang, never slowness.
        let hung = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            let open = self.open_projects.clone().map_or(0, |idle| {
                // Off this thread: a store stopped at a hold never goes idle, and the wait
                // must fail here with its message rather than hang the test.
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || tx.send(idle()));
                rx.recv_timeout(hung.saturating_duration_since(std::time::Instant::now()))
                    .expect("hung: an open project's store did not go idle within a minute of the test (a save stopped at a hold no one released?)")
            });
            if self.fs.locks_held() <= open {
                break;
            }
            assert!(
                std::time::Instant::now() < hung,
                "hung: a project of this test's store was still closing a minute after the test"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let problems = self.hooks.burrito_problems();
        if !problems.is_empty() {
            let test = std::thread::current()
                .name()
                .unwrap_or("this test")
                .to_owned();
            panic!(
                "P5/INV-VALID-BURRITO: after {test}, {}",
                problems.join("; ")
            );
        }
    }
}

/// The files under `.tc/pending/`, temporary files left out.
pub fn pending_files(fs: &FaultFs, folder: &Path) -> Vec<String> {
    files(fs, folder)
        .into_keys()
        .filter(|p| p.starts_with(".tc/pending/") && !p.contains(".~"))
        .collect()
}

/// The index, among the write points of a save of `key` through the seam, of the flush of
/// `.tc/pending`: a dry run on a copy of the world's files (`FaultFs::fail_op` counts from
/// the next write point).
pub async fn pending_dir_flush(world: &StoreWorld, project: &ProjectRef, key: &RecordKey) -> u64 {
    let fs = world.fs.copy();
    let (backend, _dry_run_watch) = backend_over(&fs, APP, FakePlatform::default(), Arc::default());
    let handle = backend.open(project, progress()).unwrap();
    handle.wait_idle();
    fs.clear_log();
    let start = fs.ops();
    save(handle.as_ref(), key, "probe").await.unwrap();
    let log = fs.log();
    handle.close().await.unwrap();
    log.iter()
        .find(|op| op.kind == tc_store::fault::OpKind::SyncDir && op.path.ends_with(".tc/pending"))
        .map(|op| op.index - start)
        .unwrap()
}
