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
    CommandError, FakePlatform, FolderFacts, Hooks, ManualClock, NewProject, OpenOptions,
    ProjectHandle, Store, StoreConfig, StoreReply, UnsafeFolder,
};

pub const APP: &str = "/app";
pub const PROJECTS: &str = "/projects";
pub const TIT: BookId = BookId::Titus;

/// A store of one installation on `fs`. Every state its writer finishes a job in is looked at
/// as a Scripture Burrito, and the rig fails its test when one was not valid (§16.6,
/// `every_written_state_is_a_valid_burrito`), unless the filesystem was killed on purpose.
pub struct Rig {
    pub fs: FaultFs,
    pub store: Store,
}

impl Drop for Rig {
    fn drop(&mut self) {
        if std::thread::panicking() || self.fs.is_dead() {
            return;
        }
        let problems = self.store.hooks().burrito_problems();
        assert!(
            problems.is_empty(),
            "the store wrote a state that is not a valid burrito: {problems:#?}"
        );
    }
}

/// Has the store's writer look at the folder after every job that wrote files.
pub fn watch_burrito(store: &Store, fs: &FaultFs) {
    let fs = fs.clone();
    store
        .hooks()
        .check_burrito(move |folder: &Path| burrito_problems(&fs, folder));
}

/// A test that takes a file out of the folder for good (a pack the app cannot write back)
/// leaves a burrito that is not valid by the test's own doing: the rig stops looking.
pub fn damaged_on_purpose(rig: &Rig) {
    rig.store.hooks().check_burrito(|_: &Path| Vec::new());
}

/// [`watch_burrito`] for a store on the real filesystem.
pub fn watch_real_burrito(store: &Store) {
    store.hooks().check_burrito(|folder: &Path| {
        tc_burrito::validate_dir(folder)
            .into_iter()
            .map(|p| format!("{p:?}"))
            .collect()
    });
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
            watch_debounce: std::time::Duration::from_millis(20),
        },
        clock,
    )
}

/// `path` with `/` between its parts, so tests can match folder names on Windows too.
pub fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn rig(fs: FaultFs) -> Rig {
    rig_as(fs, APP, "host-1")
}

pub fn rig_as(fs: FaultFs, app: &str, host: &str) -> Rig {
    let (config, _) = config(&fs, app, host);
    let store = Store::start(config).unwrap();
    store.wait_cleanup();
    watch_burrito(&store, &fs);
    Rig { fs, store }
}

/// This computer's store when a cloud client syncs the projects folder: every open is
/// read-only (§5.0). One instance runs per computer, so a test opens with it only after the
/// writable session closed.
pub fn cloud_synced_rig(fs: FaultFs) -> Rig {
    let (mut config, _) = config(&fs, APP, "host-1");
    config.platform = Arc::new(FakePlatform {
        host: "host-1".into(),
        machine: "machine-of-host-1".into(),
        facts: FolderFacts {
            unsafe_folder: Some(UnsafeFolder::CloudSynced {
                client: "OneDrive".into(),
            }),
            network: false,
        },
        ..FakePlatform::default()
    });
    let store = Store::start(config).unwrap();
    store.wait_cleanup();
    watch_burrito(&store, &fs);
    Rig { fs, store }
}

pub fn rig_with_hooks(fs: FaultFs, hooks: Arc<Hooks>) -> Rig {
    let (config, _) = config(&fs, APP, "host-1");
    let store = Store::start_with_hooks(config, hooks).unwrap();
    watch_burrito(&store, &fs);
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

// ---------------------------------------------------------------------------------------
// Storage damage: bytes that changed although the file's size and time did not (§6.2)
// ---------------------------------------------------------------------------------------

/// Settles the entries of the project at `folder`: the clock moves past the tick of the
/// files' time stamps and an open reads them once (O14). A later change that keeps a file's
/// size and mtime is then storage damage no stat can see, not an edit someone made.
pub fn settle_entries(rig: &Rig, folder: &Path) {
    rig.fs.advance(std::time::Duration::from_secs(10));
    let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle.close().unwrap();
}

/// Changes the case of one letter in the second half of the file at `rel`, keeping its size
/// and its modification time, and returns the bytes now in the file.
pub fn damage_file(rig: &Rig, folder: &Path, rel: &str) -> Vec<u8> {
    let path = folder.join(rel);
    let mut bytes = rig.fs.peek(&path).unwrap();
    let at = (bytes.len() / 2..bytes.len())
        .find(|i| bytes[*i].is_ascii_lowercase())
        .unwrap();
    bytes[at] = bytes[at].to_ascii_uppercase();
    let mtime = tc_store::Vfs::metadata(&rig.fs, &path).unwrap().mtime_ns;
    rig.fs.put(&path, &bytes, Some(mtime));
    bytes
}

/// The paths of the byte copies the notices of a handle keep for `rel`.
pub fn copies_of(handle: &ProjectHandle, rel: &str) -> Vec<String> {
    let encoded = rel.replace('/', "%2F");
    handle
        .notices()
        .into_iter()
        .filter_map(|n| n.copy)
        .filter(|copy| copy.contains(&encoded))
        .collect()
}

/// [`damage_file`] for the exact text `from`, which becomes `to` of the same length.
pub fn damage_text(rig: &Rig, folder: &Path, rel: &str, from: &str, to: &str) -> Vec<u8> {
    assert_eq!(from.len(), to.len(), "damage keeps the size");
    let path = folder.join(rel);
    let bytes = rig.fs.peek(&path).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains(from), "{rel} has no {from}");
    let damaged = text.replacen(from, to, 1).into_bytes();
    let mtime = tc_store::Vfs::metadata(&rig.fs, &path).unwrap().mtime_ns;
    rig.fs.put(&path, &damaged, Some(mtime));
    damaged
}

/// The derived files of a commit's tree whose bytes are not the projection of the packs the
/// same tree holds (§5.13 C3): what another computer's anchor check (§11.2 D1) compares, so a
/// difference would be read there as an edit made on Door43.
pub fn tree_differs_from_its_packs(
    repo: &git2::Repository,
    commit: git2::Oid,
    project: ProjectId,
    kind: ProjectKind,
) -> Vec<String> {
    use tc_journal::fold::{FoldInput, FoldState};
    let files = commit_files(repo, commit);
    let mut inputs: Vec<FoldInput> = Vec::new();
    for (rel, bytes) in &files {
        if rel.starts_with("ingredients/tc/changes/") {
            let read = tc_journal::pack::read_pack(rel, bytes, project);
            inputs.extend(read.entries.into_iter().map(FoldInput::from));
        }
    }
    let fold = FoldState::fold(kind, inputs);
    let mut differs = Vec::new();
    for file in fold.derived_files() {
        let path = file.path();
        if path == "ingredients/tc/conflicts.json" {
            continue;
        }
        match (fold.render(&file), files.get(&path)) {
            (Some(projected), Some(in_tree)) if &projected == in_tree => {}
            (Some(_), _) => differs.push(path),
            (None, _) => {}
        }
    }
    differs
}
