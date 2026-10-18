//! What every test of the store uses: a store on a `FaultFs`, a Titus project created
//! through the real create path, and short ways to save, read and check.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tc_journal::command::{Command, SeedSource};
use tc_journal::fold::ProjectKind;
use tc_journal::record::{Scope, VerseValue, VersificationValue};
use tc_journal::{
    BookId, ChangeId, ChapterNo, MetaField, MetaPath, ProjectId, Record, RecordKey, VerseNo,
};
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

thread_local! {
    /// How far every store this test thread makes starts past the usual start time.
    static CLOCK_OFFSET_MS: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
}

/// Runs `f` with every store it makes (a reopen included) one month later than the project
/// was made: a seal then writes its pack into a month folder that does not exist yet.
pub fn in_a_later_month<T>(f: impl FnOnce() -> T) -> T {
    CLOCK_OFFSET_MS.with(|c| c.set(31 * 24 * 60 * 60 * 1000));
    let out = f();
    CLOCK_OFFSET_MS.with(|c| c.set(0));
    out
}

pub fn config(fs: &FaultFs, app: &str, host: &str) -> (StoreConfig, Arc<ManualClock>) {
    let offset = CLOCK_OFFSET_MS.with(std::cell::Cell::get);
    let clock = Arc::new(ManualClock::at(1_792_195_200_000 + offset));
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

pub fn project_of(fs: &FaultFs, folder: &Path) -> ProjectId {
    let bytes = fs
        .peek(&folder.join("ingredients/tc/project.json"))
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    ProjectId::parse(json["projectId"].as_str().unwrap()).unwrap()
}

/// Every change a verified line of a pack or pending file in the folder holds.
pub fn ids_on_disk(fs: &FaultFs, folder: &Path) -> BTreeSet<ChangeId> {
    let project = project_of(fs, folder);
    let mut out = BTreeSet::new();
    for (rel, bytes) in files(fs, folder) {
        // A complete temporary file is what the next open's O2 renames into place.
        let rel = match rel.rsplit_once("/.~") {
            Some((dir, name)) => match tc_store::atomic::temp_target(&format!(".~{name}")) {
                Some(target) => format!("{dir}/{target}"),
                None => continue,
            },
            None => rel,
        };
        if rel.starts_with("ingredients/tc/changes/") {
            let read = tc_journal::pack::read_pack(&rel, &bytes, project);
            out.extend(read.entries.iter().map(|e| e.id()));
        } else if rel.starts_with(".tc/pending/") {
            let read = tc_journal::pending::read_pending(&bytes, project);
            out.extend(read.entries.iter().map(|e| e.id()));
        }
    }
    out
}

// ---------------------------------------------------------------------------------------
// The project's repository (docs/STORAGE.md §9, §16.6)
// ---------------------------------------------------------------------------------------

/// The repository of the project at `folder`, as git itself reads it.
pub fn repo_of(fs: &FaultFs, folder: &Path) -> git2::Repository {
    use tc_store::Vfs;
    git2::Repository::open(fs.outside_dir(folder).unwrap()).unwrap()
}

/// The `.git` folder of the project at `folder`.
pub fn git_dir_of(fs: &FaultFs, folder: &Path) -> PathBuf {
    use tc_store::Vfs;
    fs.outside_dir(folder).unwrap().join(".git")
}

/// The commits reachable from HEAD, newest first; none when the branch has no commit.
pub fn commits_of(repo: &git2::Repository) -> Vec<git2::Oid> {
    let Ok(head) = repo.head() else {
        return Vec::new();
    };
    let mut walk = repo.revwalk().unwrap();
    walk.push(head.target().unwrap()).unwrap();
    walk.map(|oid| oid.unwrap()).collect()
}

/// Every file of a commit's tree, by path.
pub fn commit_files(repo: &git2::Repository, commit: git2::Oid) -> BTreeMap<String, Vec<u8>> {
    let tree = repo.find_commit(commit).unwrap().tree().unwrap();
    let mut out = BTreeMap::new();
    tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
        if entry.kind() == Some(git2::ObjectType::Blob) {
            let blob = repo.find_blob(entry.id()).unwrap();
            out.insert(
                format!("{root}{}", entry.name().unwrap()),
                blob.content().to_vec(),
            );
        }
        git2::TreeWalkResult::Ok
    })
    .unwrap();
    out
}

/// What `tc_burrito` finds wrong with the tree of a commit (§16.6).
pub fn commit_problems(repo: &git2::Repository, commit: git2::Oid) -> Vec<String> {
    let files = commit_files(repo, commit);
    let Some(metadata) = files.get("metadata.json") else {
        return vec!["no metadata.json".into()];
    };
    let mut project = tc_burrito::MemProject::new();
    for (rel, bytes) in &files {
        project.insert(rel.clone(), bytes.clone());
    }
    tc_burrito::validate_project(metadata, &project)
        .into_iter()
        .map(|p| format!("{p:?}"))
        .collect()
}

/// Every commit of the project's history is a valid Scripture Burrito (§16.6). A project with
/// no repository or no commit has nothing to check.
pub fn assert_commits_valid(fs: &FaultFs, folder: &Path) {
    if !git_dir_of(fs, folder).exists() {
        return;
    }
    let repo = repo_of(fs, folder);
    for commit in commits_of(&repo) {
        let problems = commit_problems(&repo, commit);
        assert!(
            problems.is_empty(),
            "commit {commit} is not a valid burrito: {problems:#?}"
        );
    }
}
