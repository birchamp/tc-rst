//! What every test of the store uses: a store on a `FaultFs`, a Titus project created
//! through the real create path, and short ways to save, read and check.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tc_journal::command::{Command, SeedSource};
use tc_journal::fold::ProjectKind;
use tc_journal::record::{Scope, VerseValue, VersificationValue};
use tc_journal::{BookId, ChapterNo, MetaField, MetaPath, Record, RecordKey, VerseNo};
use tc_store::fault::FaultFs;
use tc_store::{
    CommandError, FakePlatform, Hooks, ManualClock, NewProject, ProjectHandle, Store, StoreConfig,
    StoreReply,
};

pub const APP: &str = "/app";
pub const PROJECTS: &str = "/projects";
pub const TIT: BookId = BookId::Titus;

/// A store of one installation on `fs`.
pub struct Rig {
    pub fs: FaultFs,
    pub store: Store,
}

pub fn config(fs: &FaultFs, app: &str, host: &str) -> (StoreConfig, Arc<ManualClock>) {
    let clock = Arc::new(ManualClock::at(1_792_195_200_000));
    let platform = FakePlatform {
        host: host.to_owned(),
        machine: format!("machine-of-{host}"),
        ..FakePlatform::default()
    };
    (
        StoreConfig {
            app_data: PathBuf::from(app),
            projects: PathBuf::from(PROJECTS),
            vfs: Arc::new(fs.clone()),
            platform: Arc::new(platform),
            clock: clock.clone(),
            threads: 2,
        },
        clock,
    )
}

pub fn rig(fs: FaultFs) -> Rig {
    rig_as(fs, APP, "host-1")
}

pub fn rig_as(fs: FaultFs, app: &str, host: &str) -> Rig {
    let (config, _) = config(&fs, app, host);
    let store = Store::start(config).unwrap();
    store.wait_cleanup();
    Rig { fs, store }
}

pub fn rig_with_hooks(fs: FaultFs, hooks: Arc<Hooks>) -> Rig {
    let (config, _) = config(&fs, APP, "host-1");
    let store = Store::start_with_hooks(config, hooks).unwrap();
    Rig { fs, store }
}

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn ult(book: &str) -> String {
    std::fs::read_to_string(
        repo_root().join(format!("fixtures/tc4/test-fixtures/en_ult/{book}.usfm")),
    )
    .unwrap()
}

fn json(text: &str) -> tc_journal::Json {
    tc_journal::json::parse(text.as_bytes()).unwrap()
}

/// The records a new Bible project starts with: its details and versification.
pub fn first_records() -> Vec<Record> {
    let meta = |field: MetaField, value: &str| Record::ProjectMeta {
        field,
        value: Some(json(value)),
    };
    let extra = |path: &str| MetaField::Extra(MetaPath::parse(path).unwrap());
    vec![
        meta(MetaField::Name, r#"{"en":"Titus test translation"}"#),
        meta(MetaField::Abbreviation, r#"{"en":"ttt"}"#),
        meta(
            MetaField::Languages,
            r#"[{"name":{"en":"English"},"scriptDirection":"ltr","tag":"en"}]"#,
        ),
        meta(extra("meta.dateCreated"), r#""2026-10-17T00:00:00.000Z""#),
        meta(extra("meta.defaultLocale"), r#""en""#),
        Record::Versification {
            value: VersificationValue {
                name: Some("eng".into()),
                bytes: tc_versification::StandardScheme::Eng.json().to_owned(),
            },
        },
    ]
}

/// Creates the project `name` with Titus seeded from the ULT.
pub fn create_titus(rig: &Rig, name: &str) -> PathBuf {
    rig.store
        .create(&NewProject {
            name: name.to_owned(),
            kind: ProjectKind::Bible,
            records: first_records(),
            books: vec![(TIT, Scope::Whole, SeedSource { usfm: ult("TIT") })],
        })
        .unwrap()
}

/// A rig with the Titus project created, and the project's folder.
pub fn titus() -> (Rig, PathBuf) {
    let rig = rig(FaultFs::new());
    let folder = create_titus(&rig, "Titus");
    (rig, folder)
}

pub fn verse(c: u16, v: u16) -> RecordKey {
    RecordKey::Verse(TIT, ChapterNo::new(c).unwrap(), VerseNo::new(v).unwrap())
}

/// Saves the words of TIT c:v as the newest snapshot shows the verse.
pub fn save_verse(
    handle: &ProjectHandle,
    c: u16,
    v: u16,
    words: &str,
) -> Result<StoreReply, CommandError> {
    let shown = handle.mint(&verse(c, v), None).unwrap();
    handle
        .submit(Command::SaveVerse {
            shown,
            words: words.to_owned(),
        })
        .wait()
}

/// The words of TIT c:v in the newest snapshot.
pub fn words(handle: &ProjectHandle, c: u16, v: u16) -> Option<String> {
    match handle.snapshot().fold().projected(&verse(c, v)) {
        Some(Record::Verse {
            value: VerseValue::Text { words, .. },
            ..
        }) => Some(words),
        _ => None,
    }
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

/// The problems `tc_burrito` finds with the folder as a Scripture Burrito.
pub fn burrito_problems(fs: &FaultFs, folder: &Path) -> Vec<String> {
    let files = files(fs, folder);
    let Some(metadata) = files.get("metadata.json") else {
        return vec!["no metadata.json".into()];
    };
    let mut project = tc_burrito::MemProject::new();
    for (rel, bytes) in &files {
        if !rel.starts_with(".tc/") {
            project.insert(rel.clone(), bytes.clone());
        }
    }
    tc_burrito::validate_project(metadata, &project)
        .into_iter()
        .map(|p| format!("{p:?}"))
        .collect()
}

pub fn assert_valid_burrito(fs: &FaultFs, folder: &Path) {
    let problems = burrito_problems(fs, folder);
    assert!(problems.is_empty(), "not a valid burrito: {problems:#?}");
}

/// The files under `.tc/pending/`.
pub fn pending_files(fs: &FaultFs, folder: &Path) -> Vec<String> {
    files(fs, folder)
        .into_keys()
        .filter(|p| p.starts_with(".tc/pending/") && !p.contains(".~"))
        .collect()
}

/// The pack files.
pub fn packs(fs: &FaultFs, folder: &Path) -> Vec<String> {
    files(fs, folder)
        .into_keys()
        .filter(|p| p.starts_with("ingredients/tc/changes/") && !p.contains(".~"))
        .collect()
}
