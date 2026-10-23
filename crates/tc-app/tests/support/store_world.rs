//! The store's world for tc-app's tests (unit C0): tc-store on its in-memory fault-injecting
//! file system, behind [`StoreBackend`], holding the sample Bible project (Titus and Jonah,
//! the bytes the memory fixture reads), the same project with two heads on Titus 1:1, and a
//! project of another id.
//!
//! The projects are written once per test binary through tc-store's own create; each world
//! is an independent copy of those files with a store of its own, so a test can crash, fail
//! or fill its files without touching another's.
#![allow(dead_code)]

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
) -> StoreBackend {
    let backend = StoreBackend::start(
        StoreConfig {
            app_data: PathBuf::from(app),
            projects: PathBuf::from(PROJECTS),
            vfs: Arc::new(fs.clone()),
            platform: Arc::new(platform),
            clock: Arc::new(ManualClock::at(START_MS)),
            threads: 2,
        },
        hooks,
    )
    .unwrap();
    // The start removes staging folders a crash left (N1); a test that plants one plants it
    // after this.
    backend.store().wait_cleanup();
    backend
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
        let backend = backend_over(&fs, APP, FakePlatform::default(), Arc::default());
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
    let backend = Arc::new(backend_over(&fs, APP, platform, hooks.clone()));
    StoreWorld {
        fs,
        hooks,
        backend,
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
    let Some(metadata) = files.get("metadata.json") else {
        panic!("not a valid burrito: no metadata.json");
    };
    let mut project = tc_burrito::MemProject::new();
    for (rel, bytes) in files {
        if !rel.starts_with(".tc/") {
            project.insert(rel.clone(), bytes.clone());
        }
    }
    let problems: Vec<String> = tc_burrito::validate_project(metadata, &project)
        .into_iter()
        .map(|p| format!("{p:?}"))
        .collect();
    assert!(problems.is_empty(), "not a valid burrito: {problems:#?}");
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
    let backend = backend_over(&fs, APP, FakePlatform::default(), Arc::default());
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
