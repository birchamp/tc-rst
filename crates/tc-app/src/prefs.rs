//! This person's preferences on this computer (APP-DESIGN §4.9, A-20): the app language, which
//! projects were used when, where the person last worked in each book or story, and the Resume
//! card's record. They are never part of a project.
//!
//! Two kinds of file hold them: `prefs/app.json` and `prefs/<projectId>.json`. The same writer
//! keeps `reports.json` (the last 50 operation reports, [`read_reports`]) and the Home cards'
//! summaries (`summaries/<folder hash>.json`). On a disk ([`DiskFiles`], unit C1a) each file
//! is replaced by `tc_store::atomic_write`, on the blocking pool; tests use [`MemoryFiles`].
//!
//! # What a document keeps
//!
//! A document is read leniently and rewritten carefully, as tC4 treats its settings
//! (`tC4:e2e/app-language.spec.ts:253-258`: a saved value the app does not support reads as the
//! default "and the document is left as it is") and as §3.10 asks ("Unknown keys are kept on
//! rewrite"):
//!
//! - a member the app does not know is kept, unchanged and in its place;
//! - a member it knows but cannot read reads as its default and stays in the file until the app
//!   writes that member;
//! - a write changes only the members it sets;
//! - a file that is not a JSON object reads as empty. Its bytes are first copied to
//!   `<file>.unreadable` and a report names both, so nothing the person had is dropped unseen.
//!
//! # When a document is written
//!
//! One task writes every file, so writes never overlap and each serializes the document as it
//! is when it runs, never a copy taken earlier (tC4's "reads the NEWEST pending record when it
//! runs").
//!
//! - The app language is written at once, and the call waits for the write. A failed write
//!   changes nothing and says so (tC4 keeps the dialog and saves nothing).
//! - The projects used, the aligner switch and the Translate unit are written at once.
//! - A place is written 500 ms after the last move, as tC4 does. A visit that only repeats the
//!   place it records (an edit in the verse the person is already in) changes nothing but the
//!   time and starts no write, so typing costs no write of this file.
//! - The Resume record (`lastEdit`) is written 5 s after the last change, or at once when the
//!   person leaves the project (A-20). tC4 waits 1 s; the card reads it only on Home, after
//!   the leave has written it.
//! - Every other write that fails keeps the value for the session and adds a report. The next
//!   change of that file writes again.
//! - The reports are written a second after the first change not written yet, so a run of
//!   reports costs one write a second, and at once at quit and when the person leaves.
//! - A summary is written at once.
//!
//! A write that waits on a disk runs on the blocking pool, never on a runtime worker; the task
//! awaits it, so writes still never overlap. A document's first read (`prefs/app.json` at the
//! start, a project's file when it is first asked for) is a few hundred bytes and stays inline.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tc_api::check::Tool;
use tc_api::ids::{LocaleId, ProjectId, ProjectRef, RefPart, StoryNo, WallMs};
use tc_api::prefs::{DraftUnit, PlaceChange, PlaceMode, PlaceUpdate};
use tc_core::BookId;
use tc_store::{SyncDir, Vfs, WriteError};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use crate::backend::SummaryStamp;
use crate::clock::Clock;
use crate::reports::{OperationReport, ReportLog, ReportOp, ReportOutcome, facts};

/// How long after the last move a place is written (tC4: 500 ms).
pub const PLACE_WRITE_AFTER: Duration = Duration::from_millis(500);

/// How long after the last change the Resume record is written (A-20).
pub const LAST_EDIT_WRITE_AFTER: Duration = Duration::from_secs(5);

/// How long after the first change not written yet the reports are written (C1a).
pub const REPORTS_WRITE_AFTER: Duration = Duration::from_secs(1);

/// The file format version this build writes into a document that has none.
const SCHEMA_VERSION: u64 = 1;

/// The version of `reports.json` and of a summary file this build writes.
const FILE_SCHEMA: u64 = 1;

// ---- the storage seam ----------------------------------------------------------------------

/// A file in the app-data folder, by its `/`-separated path below it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AppDataPath(String);

impl AppDataPath {
    pub fn app_prefs() -> Self {
        Self("prefs/app.json".to_owned())
    }

    pub fn project_prefs(id: &ProjectId) -> Self {
        Self(format!("prefs/{}.json", id.as_str()))
    }

    /// `reports.json`: the last 50 operation reports (§4.9).
    pub fn reports() -> Self {
        Self(REPORTS_FILE.to_owned())
    }

    /// `summaries/<32 hex>.json`: the Home card's summary of a folder (§4.5), named by the
    /// first 16 bytes of the SHA-256 of the folder's name, so any name makes a file name.
    pub fn summary(folder: &ProjectRef) -> Self {
        let digest = ring::digest::digest(&ring::digest::SHA256, folder.as_str().as_bytes());
        let hex: String = digest.as_ref()[..16]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Self(format!("{SUMMARIES}/{hex}.json"))
    }

    /// Where the bytes of this file are copied when they cannot be read.
    pub fn unreadable_copy(&self) -> Self {
        Self(format!("{}.unreadable", self.0))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

const PREFS: &str = "prefs";
const SUMMARIES: &str = "summaries";
const REPORTS_FILE: &str = "reports.json";

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct AppDataError(pub String);

/// How far a write got (tc-store's `WriteError`): in place and flushed, or in place with the
/// disk's error after the rename, which a power cut could still undo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Written {
    Durable,
    NotDurable(String),
}

/// The app-data folder. Writes are atomic: after a failure or a crash a file holds the old
/// bytes or the new, never a mixture (AGENTS.md).
pub trait AppDataFiles: Send + Sync + 'static {
    fn read(&self, path: &AppDataPath) -> Result<Option<Vec<u8>>, AppDataError>;
    fn write(&self, path: &AppDataPath, bytes: &[u8]) -> Result<Written, AppDataError>;
    /// Removes the file; one that is not there is removed already.
    fn remove(&self, path: &AppDataPath) -> Result<(), AppDataError>;
    /// Whether a write waits on a disk, so the writer runs it on the blocking pool.
    fn blocks(&self) -> bool;
}

/// The app-data folder on a disk: each file replaced atomically and durably by
/// `tc_store::atomic_write` (AGENTS.md), through tc-store's [`Vfs`].
pub struct DiskFiles {
    vfs: Arc<dyn Vfs>,
    root: PathBuf,
}

impl DiskFiles {
    /// The files under `root`. Removes what a crash left of this writer's temporary files
    /// (`.~<name>.<16 hex>.tmp` in `prefs/` and `summaries/`, which only this writer writes,
    /// and beside `reports.json`), and nothing else: the root also holds tc-store's
    /// `installation.json` and `ledger.json`, written by the same helper.
    pub fn open(vfs: Arc<dyn Vfs>, root: &Path) -> Self {
        let files = Self {
            vfs,
            root: root.to_owned(),
        };
        for (dir, only) in [
            (root.join(PREFS), None),
            (root.join(SUMMARIES), None),
            (root.to_owned(), Some(REPORTS_FILE)),
        ] {
            let Ok(entries) = files.vfs.read_dir(&dir) else {
                continue;
            };
            for entry in entries {
                let Some(name) = entry.name.to_str() else {
                    continue;
                };
                let ours = tc_store::atomic::temp_target(name)
                    .is_some_and(|target| only.is_none_or(|only| only == target));
                if ours && entry.kind == tc_store::vfs::EntryKind::File {
                    // A leftover that cannot be removed now is tried again at the next start.
                    let _ = files.vfs.remove_file(&dir.join(name));
                }
            }
        }
        files
    }

    /// The path of `path` on this computer, built part by part, so Windows gets its own
    /// separators.
    fn path(&self, path: &AppDataPath) -> PathBuf {
        let mut out = self.root.clone();
        for part in path.as_str().split('/').filter(|p| !p.is_empty()) {
            out.push(part);
        }
        out
    }
}

impl AppDataFiles for DiskFiles {
    fn read(&self, path: &AppDataPath) -> Result<Option<Vec<u8>>, AppDataError> {
        let full = self.path(path);
        match self.vfs.read(&full) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if tc_store::vfs::is_not_found(&e) => Ok(None),
            Err(e) => Err(AppDataError(format!("{}: {e}", full.display()))),
        }
    }

    fn write(&self, path: &AppDataPath, bytes: &[u8]) -> Result<Written, AppDataError> {
        let full = self.path(path);
        let (Some(dir), Some(name)) = (full.parent(), full.file_name().and_then(|n| n.to_str()))
        else {
            return Err(AppDataError(format!("{} is not a file", full.display())));
        };
        tc_store::atomic_create_dirs(self.vfs.as_ref(), dir)
            .map_err(|e| AppDataError(e.to_string()))?;
        match tc_store::atomic_write(self.vfs.as_ref(), dir, name, bytes, SyncDir::Now) {
            Ok(()) => Ok(Written::Durable),
            Err(WriteError::WrittenNotDurable(e)) => Ok(Written::NotDurable(e.to_string())),
            Err(WriteError::NotWritten(e)) => Err(AppDataError(e.to_string())),
        }
    }

    fn remove(&self, path: &AppDataPath) -> Result<(), AppDataError> {
        let full = self.path(path);
        if self.vfs.metadata(&full).is_err() {
            return Ok(());
        }
        tc_store::atomic_remove(self.vfs.as_ref(), &full, SyncDir::Now)
            .map_err(|e| AppDataError(e.to_string()))
    }

    fn blocks(&self) -> bool {
        true
    }
}

/// App data in memory, for tests: every write is in place at once.
#[derive(Default)]
pub struct MemoryFiles {
    files: Mutex<BTreeMap<AppDataPath, Vec<u8>>>,
    failing: AtomicBool,
    writes: Mutex<Vec<(AppDataPath, Instant)>>,
}

impl MemoryFiles {
    pub fn new() -> Self {
        Self::default()
    }

    /// Puts a file in as if an earlier session had written it.
    pub fn seed(&self, path: AppDataPath, bytes: Vec<u8>) {
        lock(&self.files).insert(path, bytes);
    }

    /// Makes every write fail until turned off, as a full disk does.
    pub fn fail_writes(&self, on: bool) {
        self.failing.store(on, Ordering::SeqCst);
    }

    /// Every successful write, with the time it was made, oldest first.
    pub fn writes(&self) -> Vec<(AppDataPath, Instant)> {
        lock(&self.writes).clone()
    }

    pub fn get(&self, path: &AppDataPath) -> Option<Vec<u8>> {
        lock(&self.files).get(path).cloned()
    }
}

impl AppDataFiles for MemoryFiles {
    fn read(&self, path: &AppDataPath) -> Result<Option<Vec<u8>>, AppDataError> {
        Ok(self.get(path))
    }

    fn write(&self, path: &AppDataPath, bytes: &[u8]) -> Result<Written, AppDataError> {
        if self.failing.load(Ordering::SeqCst) {
            return Err(AppDataError("the disk is full".to_owned()));
        }
        lock(&self.files).insert(path.clone(), bytes.to_vec());
        lock(&self.writes).push((path.clone(), Instant::now()));
        Ok(Written::Durable)
    }

    fn remove(&self, path: &AppDataPath) -> Result<(), AppDataError> {
        if self.failing.load(Ordering::SeqCst) {
            return Err(AppDataError("the disk is full".to_owned()));
        }
        lock(&self.files).remove(path);
        Ok(())
    }

    /// Memory never waits, so the paused-clock tests keep their exact times.
    fn blocks(&self) -> bool {
        false
    }
}

/// Writes `bytes` at `path`, on the blocking pool when the files wait on a disk. The writer
/// awaits it, so writes still never overlap and each takes the document as it was when the
/// bytes were made.
async fn write_file(
    files: &Arc<dyn AppDataFiles>,
    path: AppDataPath,
    bytes: Vec<u8>,
) -> Result<Written, AppDataError> {
    if !files.blocks() {
        return files.write(&path, &bytes);
    }
    let files = files.clone();
    tokio::task::spawn_blocking(move || files.write(&path, &bytes))
        .await
        .unwrap_or_else(|stopped| Err(AppDataError(crate::jobs::stop_reason(stopped))))
}

/// Removes `path`, on the blocking pool when the files wait on a disk.
async fn remove_file(files: &Arc<dyn AppDataFiles>, path: AppDataPath) -> Result<(), AppDataError> {
    if !files.blocks() {
        return files.remove(&path);
    }
    let files = files.clone();
    tokio::task::spawn_blocking(move || files.remove(&path))
        .await
        .unwrap_or_else(|stopped| Err(AppDataError(crate::jobs::stop_reason(stopped))))
}

// ---- summaries ------------------------------------------------------------------------------

/// A Home card's summary as app data keeps it (APP-DESIGN §4.5, C1a): the project it is of,
/// what it was computed from, and the summary itself. Its shape is B2's (`Summary`), which
/// parses it at its boundary; this layer keeps it as JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredSummary {
    pub project: tc_journal::ProjectId,
    pub stamp: SummaryStamp,
    pub summary: Value,
}

/// `summaries/<hash>.json`.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SummaryFile {
    schema_version: u64,
    folder: String,
    project_id: String,
    stamp: String,
    summary: Value,
}

impl StoredSummary {
    fn bytes(&self, folder: &ProjectRef) -> Vec<u8> {
        let file = SummaryFile {
            schema_version: FILE_SCHEMA,
            folder: folder.as_str().to_owned(),
            project_id: self.project.to_string(),
            stamp: self.stamp.to_hex(),
            summary: self.summary.clone(),
        };
        let mut bytes = serde_json::to_vec(&file).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }

    /// The summary a file keeps for `folder`; `None` for a file that cannot be read, of another
    /// schema, or of another folder (two names whose hashes met).
    fn read(bytes: &[u8], folder: &ProjectRef) -> Option<Self> {
        let file: SummaryFile = serde_json::from_slice(bytes).ok()?;
        if file.schema_version != FILE_SCHEMA || file.folder != folder.as_str() {
            return None;
        }
        Some(Self {
            project: tc_journal::ProjectId::parse(&file.project_id)?,
            stamp: SummaryStamp::parse_hex(&file.stamp)?,
            summary: file.summary,
        })
    }
}

// ---- reports.json --------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportsFile<'a> {
    schema_version: u64,
    /// Oldest first.
    reports: &'a [OperationReport],
}

fn reports_bytes(reports: &[OperationReport]) -> Vec<u8> {
    let file = ReportsFile {
        schema_version: FILE_SCHEMA,
        reports,
    };
    // Plain data with derived `Serialize`: it always serializes.
    let mut bytes = serde_json::to_vec_pretty(&file).unwrap_or_default();
    bytes.push(b'\n');
    bytes
}

/// What `reports.json` gave at the start (§4.9, C1a).
#[derive(Debug, Default)]
pub struct KeptReports {
    /// Oldest first.
    pub reports: Vec<OperationReport>,
    /// The highest id the file named, an entry this build cannot read included.
    pub last_id: u64,
    /// What was wrong with the file, for a `PrefsWrite` report.
    pub problem: Option<String>,
}

/// The reports an earlier launch kept in `reports.json` (§4.9, C1a). No file is no reports. A
/// file that is not `{"reports": […]}` is copied to `reports.json.unreadable` (unless an
/// earlier one is kept there) before anything replaces it, and gives no report; an entry this
/// build cannot read (an operation a newer build added) is left out, but its id is never given
/// again. [`App::start`](crate::App::start) reads it before the first report of the launch.
pub fn read_reports(files: &dyn AppDataFiles) -> KeptReports {
    let path = AppDataPath::reports();
    let bytes = match files.read(&path) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return KeptReports::default(),
        Err(error) => {
            return KeptReports {
                problem: Some(format!("could not be read: {error}")),
                ..KeptReports::default()
            };
        }
    };
    let entries = match serde_json::from_slice::<Value>(&bytes) {
        Ok(Value::Object(mut file)) => match file.remove("reports") {
            Some(Value::Array(entries)) => Some(entries),
            _ => None,
        },
        _ => None,
    };
    let Some(entries) = entries else {
        let copy = path.unreadable_copy();
        let kept = match files.read(&copy) {
            Ok(Some(_)) => Ok(()),
            _ => files.write(&copy, &bytes).map(|_| ()),
        };
        let problem = match kept {
            Ok(()) => format!(
                "is not a list of reports; its bytes are kept in {}",
                copy.as_str()
            ),
            Err(error) => {
                format!("is not a list of reports, and its bytes could not be kept: {error}")
            }
        };
        return KeptReports {
            problem: Some(problem),
            ..KeptReports::default()
        };
    };
    let last_id = entries
        .iter()
        .filter_map(|entry| entry.get("id")?.as_u64())
        .max()
        .unwrap_or(0);
    let reports = entries
        .into_iter()
        .filter_map(|entry| serde_json::from_value::<OperationReport>(entry).ok())
        .collect();
    KeptReports {
        reports,
        last_id,
        problem: None,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Nothing that holds one of these locks leaves its data half changed.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---- the records ---------------------------------------------------------------------------

/// Where the person last worked in one book or story (tC4's `PlaceRecord`,
/// `tC4:src/data/place.ts`). A Home tile returns there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceRecord {
    pub mode: PlaceMode,
    /// The chapter of a book, or the number of a story: tC4's `rememberPlace` passes the story
    /// as the chapter (`tC4:src/state.jsx:3834`).
    pub chapter: u16,
    pub verse: Option<PlaceVerse>,
    /// The Check tool, kept only while the mode is Check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<Tool>,
    pub at: WallMs,
}

/// tC4 keeps `number | string | null` (`src/data/place.ts`): a story's frame is a number, a
/// book's verse the text its reference writes (`4`, `4-5`, `intro`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PlaceVerse {
    Frame(u16),
    Text(String),
}

/// The Resume card's record: the last place a change was made.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastEdit {
    /// The folder, which Home's card is keyed by.
    pub project: ProjectRef,
    pub unit: EditUnit,
    pub mode: PlaceMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<Tool>,
    /// The start of the text, for the card.
    pub snippet: String,
    pub at: WallMs,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum EditUnit {
    Book {
        book: BookId,
        chapter: u16,
        verse: RefPart,
    },
    Story {
        story: StoryNo,
        frame: u16,
    },
}

impl EditUnit {
    /// The observation of the place that an edit in this unit makes.
    fn place_update(&self, mode: PlaceMode, tool: Option<Tool>) -> PlaceUpdate {
        match self {
            Self::Book {
                book,
                chapter,
                verse,
            } => PlaceUpdate::Book {
                book: *book,
                mode: Some(mode),
                chapter: Some(*chapter),
                verse: PlaceChange::Set(verse.clone()),
                tool,
            },
            Self::Story { story, frame } => PlaceUpdate::Story {
                story: *story,
                mode: Some(mode),
                frame: PlaceChange::Set(*frame),
                tool,
            },
        }
    }
}

/// The key of a unit in a project's `places`: a book's code, or `story:N` (tC4's `placeKey`;
/// one grammar, so a book code can never be read as a story).
fn place_key(update: &PlaceUpdate) -> String {
    match update {
        PlaceUpdate::Book { book, .. } => book.code().to_owned(),
        PlaceUpdate::Story { story, .. } => format!("story:{}", story.get()),
    }
}

/// tC4's `recordPlace` (`tC4:src/data/place.ts`): the record after one observation. What the
/// observation does not name comes from the stored record; the Check tool is kept only while
/// the mode is Check.
fn record_place(prior: Option<&PlaceRecord>, update: &PlaceUpdate, now: WallMs) -> PlaceRecord {
    let (mode, chapter, verse, tool) = match update {
        PlaceUpdate::Book {
            mode,
            chapter,
            verse,
            tool,
            ..
        } => (
            *mode,
            *chapter,
            match verse {
                PlaceChange::Keep => prior.and_then(|p| p.verse.clone()),
                PlaceChange::Clear => None,
                PlaceChange::Set(text) => Some(PlaceVerse::Text(text.as_str().to_owned())),
            },
            *tool,
        ),
        // A story is recorded with its number as the chapter and its frame as the verse
        // (`rememberPlace`, `tC4:src/state.jsx:3834`).
        PlaceUpdate::Story {
            story,
            mode,
            frame,
            tool,
        } => (
            *mode,
            Some(u16::from(story.get())),
            match frame {
                PlaceChange::Keep => prior.and_then(|p| p.verse.clone()),
                PlaceChange::Clear => None,
                PlaceChange::Set(frame) => Some(PlaceVerse::Frame(*frame)),
            },
            *tool,
        ),
    };
    let mode = mode.or(prior.map(|p| p.mode)).unwrap_or(PlaceMode::Draft);
    PlaceRecord {
        mode,
        chapter: chapter.or(prior.map(|p| p.chapter)).unwrap_or(1),
        verse,
        tool: if mode == PlaceMode::Check {
            tool.or(prior.and_then(|p| p.tool))
        } else {
            None
        },
        at: now,
    }
}

// ---- the documents -------------------------------------------------------------------------

/// A document's members, in their order. Unknown members are kept because the map is.
#[derive(Clone, Default)]
struct Doc {
    members: Map<String, Value>,
}

impl Doc {
    /// `None` when the bytes are not a JSON object.
    fn read(bytes: &[u8]) -> Option<Self> {
        match serde_json::from_slice(bytes) {
            Ok(Value::Object(members)) => Some(Self { members }),
            _ => None,
        }
    }

    fn bytes(&self) -> Vec<u8> {
        let mut doc = self.clone();
        doc.members
            .entry("schemaVersion")
            .or_insert_with(|| Value::from(SCHEMA_VERSION));
        // A map of JSON values always serializes.
        let mut bytes = serde_json::to_vec_pretty(&Value::Object(doc.members)).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }

    fn object_mut(&mut self, name: &str) -> &mut Map<String, Value> {
        let slot = self
            .members
            .entry(name)
            .or_insert_with(|| Value::Object(Map::new()));
        if !slot.is_object() {
            *slot = Value::Object(Map::new());
        }
        match slot {
            Value::Object(map) => map,
            // The slot is an object: it was one, or was replaced by one just above.
            _ => unreachable!("an object was just stored here"),
        }
    }

    // The app document.

    /// English unless the saved value is one of the ten interface languages. A value that is
    /// anything else (`"de"`, `"EN"`, `42`, `{"id":"fr"}`) reads as English and stays in the
    /// file.
    fn locale(&self) -> LocaleId {
        self.members
            .get("appLocale")
            .and_then(Value::as_str)
            .and_then(|id| LocaleId::parse(id).ok())
            .unwrap_or(LocaleId::ENGLISH)
    }

    fn set_locale(&mut self, locale: LocaleId) {
        self.members
            .insert("appLocale".to_owned(), Value::from(locale.as_str()));
    }

    fn last_used(&self, folder: &ProjectRef) -> Option<WallMs> {
        self.members
            .get("lastUsed")?
            .get(folder.as_str())?
            .as_i64()
            .map(WallMs)
    }

    fn set_last_used(&mut self, folder: &ProjectRef, at: WallMs) {
        self.object_mut("lastUsed")
            .insert(folder.as_str().to_owned(), Value::from(at.0));
    }

    fn align_suggestions(&self, folder: &ProjectRef) -> Option<bool> {
        self.members
            .get("alignSuggestions")?
            .get(folder.as_str())?
            .as_bool()
    }

    fn set_align_suggestions(&mut self, folder: &ProjectRef, on: bool) {
        self.object_mut("alignSuggestions")
            .insert(folder.as_str().to_owned(), Value::from(on));
    }

    /// The groups `tool`'s rail has folded (tC4 `railFolded`, keys `"<tool>|<groupId>"`).
    fn rail_folded(&self, tool: &str) -> Vec<String> {
        let prefix = format!("{tool}|");
        self.members
            .get("railFolded")
            .and_then(Value::as_object)
            .map(|folded| {
                folded
                    .iter()
                    .filter(|(_, on)| on.as_bool() == Some(true))
                    .filter_map(|(key, _)| key.strip_prefix(&prefix).map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// An unfold is kept as `false`, as tC4 writes it.
    fn set_rail_folded(&mut self, tool: &str, groups: &[String], folded: bool) {
        let map = self.object_mut("railFolded");
        for group in groups {
            map.insert(format!("{tool}|{group}"), Value::from(folded));
        }
    }

    fn last_edit(&self) -> Option<LastEdit> {
        serde_json::from_value(self.members.get("lastEdit")?.clone()).ok()
    }

    fn set_last_edit(&mut self, edit: &LastEdit) {
        if let Ok(value) = serde_json::to_value(edit) {
            self.members.insert("lastEdit".to_owned(), value);
        }
    }

    // A project's document.

    fn place(&self, key: &str) -> Option<PlaceRecord> {
        serde_json::from_value(self.members.get("places")?.get(key)?.clone()).ok()
    }

    /// Sets the record's members and keeps any other member the stored record has.
    fn set_place(&mut self, key: &str, record: &PlaceRecord) {
        let Ok(Value::Object(new)) = serde_json::to_value(record) else {
            return;
        };
        let places = self.object_mut("places");
        let stored = places
            .entry(key)
            .or_insert_with(|| Value::Object(Map::new()));
        if !stored.is_object() {
            *stored = Value::Object(Map::new());
        }
        if let Value::Object(stored) = stored {
            // `tool` is left out of the new record when there is none.
            if !new.contains_key("tool") {
                stored.remove("tool");
            }
            for (name, value) in new {
                stored.insert(name, value);
            }
        }
    }

    fn draft_unit(&self) -> Option<DraftUnit> {
        serde_json::from_value(self.members.get("draftUnit")?.clone()).ok()
    }

    fn set_draft_unit(&mut self, unit: DraftUnit) {
        if let Ok(value) = serde_json::to_value(unit) {
            self.members.insert("draftUnit".to_owned(), value);
        }
    }
}

struct Docs {
    app: Doc,
    projects: HashMap<ProjectId, Doc>,
}

/// What the task and the readers share.
struct Shared {
    files: Arc<dyn AppDataFiles>,
    reports: ReportLog,
    clock: Arc<dyn Clock>,
    docs: Mutex<Docs>,
    /// The summaries read or recorded, by folder; `None` for a folder whose file there is not.
    summaries: Mutex<HashMap<ProjectRef, Option<StoredSummary>>>,
}

impl Shared {
    /// The document at `path`: empty when there is no file, and empty again, with the bytes
    /// copied aside and a report, when the file is not a JSON object.
    fn load(&self, path: &AppDataPath) -> Doc {
        let bytes = match self.files.read(path) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return Doc::default(),
            Err(error) => {
                self.report(path, &format!("could not be read: {error}"));
                return Doc::default();
            }
        };
        if let Some(doc) = Doc::read(&bytes) {
            return doc;
        }
        let copy = path.unreadable_copy();
        // Inline, as the read was: a broken file is copied once per launch at most, and its
        // copy must exist before anything replaces the file.
        let kept = match self.files.read(&copy) {
            // An earlier unreadable file is already kept; the first one is the one to look at.
            Ok(Some(_)) => Ok(()),
            _ => self.files.write(&copy, &bytes).map(|_| ()),
        };
        self.report(
            path,
            &match kept {
                Ok(()) => format!(
                    "is not a JSON object; its bytes are kept in {}",
                    copy.as_str()
                ),
                Err(error) => {
                    format!("is not a JSON object, and its bytes could not be kept: {error}")
                }
            },
        );
        Doc::default()
    }

    fn report(&self, path: &AppDataPath, what: &str) {
        let now = self.clock.now();
        self.reports.add(
            ReportOp::PrefsWrite,
            ReportOutcome::Failed {
                reason: format!("{} {what}", path.as_str()),
            },
            facts(serde_json::json!({ "file": path.as_str() })),
            now,
            now,
        );
    }

    /// The outcome of a write in the report log: nothing for a durable one.
    fn report_write(&self, path: &AppDataPath, outcome: &Result<Written, AppDataError>) {
        match outcome {
            Ok(Written::Durable) => {}
            Ok(Written::NotDurable(reason)) => self.report(
                path,
                &format!("was written, but the disk reported an error after it: {reason}"),
            ),
            Err(error) => self.report(path, &format!("could not be written: {error}")),
        }
    }

    /// Runs `f` on a project's document, reading the file first if the app has not.
    fn with_project<R>(&self, id: &ProjectId, f: impl FnOnce(&mut Doc) -> R) -> R {
        let mut docs = lock(&self.docs);
        if !docs.projects.contains_key(id) {
            let doc = self.load(&AppDataPath::project_prefs(id));
            docs.projects.insert(id.clone(), doc);
        }
        match docs.projects.get_mut(id) {
            Some(doc) => f(doc),
            // The document was inserted above and the lock is held.
            None => unreachable!("the project document was just loaded"),
        }
    }
}

// ---- the writer ----------------------------------------------------------------------------

/// A file the task writes: the app's, one project's, or the reports.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Target {
    App,
    Project(ProjectId),
    Reports,
}

impl Target {
    fn path(&self) -> AppDataPath {
        match self {
            Self::App => AppDataPath::app_prefs(),
            Self::Project(id) => AppDataPath::project_prefs(id),
            Self::Reports => AppDataPath::reports(),
        }
    }
}

/// A tool as tC4's `railFolded` keys name it.
fn rail_tool(tool: tc_api::check::Tool) -> &'static str {
    match tool {
        tc_api::check::Tool::TranslationNotes => "translationNotes",
        tc_api::check::Tool::TranslationWords => "translationWords",
    }
}

enum Msg {
    SetLocale {
        locale: LocaleId,
        reply: oneshot::Sender<Result<(), AppDataError>>,
    },
    DraftUnit {
        project: ProjectId,
        unit: DraftUnit,
    },
    /// A member of the app document changed in place: write it at once.
    WriteApp,
    LastUsed {
        folder: ProjectRef,
        at: WallMs,
    },
    Place {
        project: ProjectId,
        update: PlaceUpdate,
    },
    LastEdit {
        project: ProjectId,
        edit: LastEdit,
    },
    Summary {
        folder: ProjectRef,
    },
    Flush {
        reply: oneshot::Sender<()>,
    },
    Reset {
        reply: oneshot::Sender<()>,
    },
}

/// The one writer of app-data files, and the readers of what it holds. Clones share one
/// writer; it writes what is pending and stops when the last clone is dropped.
#[derive(Clone)]
pub struct PrefsWriter {
    tx: mpsc::UnboundedSender<Msg>,
    shared: Arc<Shared>,
}

impl PrefsWriter {
    /// Reads `prefs/app.json` and starts the writer, which also keeps `reports` in
    /// `reports.json`. Must run inside a tokio runtime.
    pub fn start(files: Arc<dyn AppDataFiles>, reports: ReportLog, clock: Arc<dyn Clock>) -> Self {
        let shared = Arc::new(Shared {
            files,
            reports,
            clock,
            docs: Mutex::new(Docs {
                app: Doc::default(),
                projects: HashMap::new(),
            }),
            summaries: Mutex::default(),
        });
        let app = shared.load(&AppDataPath::app_prefs());
        lock(&shared.docs).app = app;
        let (tx, rx) = mpsc::unbounded_channel();
        let task = Task {
            shared: Arc::clone(&shared),
            pending: BTreeMap::new(),
            // A log starts at version 0 (`ReportLog::new`, `ReportLog::with_kept`), which is
            // what `reports.json` holds; a report added since makes the first write.
            reports_written: 0,
        };
        tokio::spawn(task.run(rx));
        Self { tx, shared }
    }

    fn send(&self, msg: Msg) {
        // The task ends only when every writer is dropped, and `self` is one.
        let _ = self.tx.send(msg);
    }

    pub fn app_locale(&self) -> LocaleId {
        lock(&self.shared.docs).app.locale()
    }

    /// Saves the app language and waits for the write. On a failed write nothing changes:
    /// the saved and the current language stay as they were.
    pub async fn set_locale(&self, locale: LocaleId) -> Result<(), AppDataError> {
        let (reply, answer) = oneshot::channel();
        self.tx
            .send(Msg::SetLocale { locale, reply })
            .map_err(|_| AppDataError("preferences are not being saved".to_owned()))?;
        answer
            .await
            .map_err(|_| AppDataError("preferences are not being saved".to_owned()))?
    }

    /// How Translate shows this project, written at once.
    pub fn set_draft_unit(&self, project: ProjectId, unit: DraftUnit) {
        self.send(Msg::DraftUnit { project, unit });
    }

    /// The aligner's suggestions switch: readers see it now, and it is written at once.
    pub fn set_align_suggestions(&self, folder: ProjectRef, on: bool) {
        lock(&self.shared.docs)
            .app
            .set_align_suggestions(&folder, on);
        self.send(Msg::WriteApp);
    }

    /// Groups of a Check tool's rail folded or unfolded: readers see it now, and it is
    /// written at once.
    pub fn set_rail_folded(&self, tool: tc_api::check::Tool, groups: &[String], folded: bool) {
        lock(&self.shared.docs)
            .app
            .set_rail_folded(rail_tool(tool), groups, folded);
        self.send(Msg::WriteApp);
    }

    /// When a folder was last used, written at once.
    pub fn record_last_used(&self, folder: ProjectRef, at: WallMs) {
        self.send(Msg::LastUsed { folder, at });
    }

    /// One observation of the place, written 500 ms after the last move.
    pub fn record_place(&self, project: ProjectId, update: PlaceUpdate) {
        self.send(Msg::Place { project, update });
    }

    /// A change was made: it becomes the Resume record, and a place observation of its unit.
    /// Written 5 s after the last change, or when the person leaves.
    pub fn record_last_edit(&self, project: ProjectId, edit: LastEdit) {
        self.send(Msg::LastEdit { project, edit });
    }

    /// The person goes Home: the Resume record, the places and the reports are written now.
    pub async fn leave_project(&self) {
        self.flush().await;
    }

    /// Everything pending is written now (quit).
    pub async fn flush(&self) {
        let (reply, answer) = oneshot::channel();
        if self.tx.send(Msg::Flush { reply }).is_ok() {
            let _ = answer.await;
        }
    }

    /// Back to the defaults, with nothing pending (`test/reset`). The reports are written as
    /// the log holds them, which the reset empties first.
    pub async fn reset(&self) {
        let (reply, answer) = oneshot::channel();
        if self.tx.send(Msg::Reset { reply }).is_ok() {
            let _ = answer.await;
        }
    }

    pub fn place(&self, project: &ProjectId, key: &str) -> Option<PlaceRecord> {
        self.shared.with_project(project, |doc| doc.place(key))
    }

    pub fn last_edit(&self) -> Option<LastEdit> {
        lock(&self.shared.docs).app.last_edit()
    }

    pub fn draft_unit(&self, project: &ProjectId) -> Option<DraftUnit> {
        self.shared.with_project(project, |doc| doc.draft_unit())
    }

    pub fn last_used(&self, folder: &ProjectRef) -> Option<WallMs> {
        lock(&self.shared.docs).app.last_used(folder)
    }

    pub fn align_suggestions(&self, folder: &ProjectRef) -> Option<bool> {
        lock(&self.shared.docs).app.align_suggestions(folder)
    }

    pub fn rail_folded(&self, tool: tc_api::check::Tool) -> Vec<String> {
        lock(&self.shared.docs).app.rail_folded(rail_tool(tool))
    }

    /// The summary kept for `folder`, read on the first ask (a file of a few hundred bytes);
    /// `None` when there is none, it cannot be read, or it is of another folder.
    pub fn summary(&self, folder: &ProjectRef) -> Option<StoredSummary> {
        if let Some(known) = lock(&self.shared.summaries).get(folder) {
            return known.clone();
        }
        let read = match self.shared.files.read(&AppDataPath::summary(folder)) {
            Ok(Some(bytes)) => StoredSummary::read(&bytes, folder),
            Ok(None) | Err(_) => None,
        };
        lock(&self.shared.summaries)
            .entry(folder.clone())
            .or_insert(read)
            .clone()
    }

    /// Keeps `summary` for `folder`: readers see it now, and it is written at once.
    pub fn record_summary(&self, folder: ProjectRef, summary: StoredSummary) {
        lock(&self.shared.summaries).insert(folder.clone(), Some(summary));
        self.send(Msg::Summary { folder });
    }
}

struct Task {
    shared: Arc<Shared>,
    /// The files with a change not written yet, and when each is due.
    pending: BTreeMap<Target, Instant>,
    /// The version of the report log that `reports.json` holds, or that the writer gave up on
    /// after a failed write (its own report of that failure included, so a disk that refuses
    /// `reports.json` costs one report per real change, never one a second).
    reports_written: u64,
}

impl Task {
    async fn run(mut self, mut inbox: mpsc::UnboundedReceiver<Msg>) {
        let reports = self.shared.reports.notifier();
        loop {
            let next = self.pending.values().min().copied();
            let due = next.unwrap_or_else(|| Instant::now() + Duration::from_secs(365 * 24 * 3600));
            tokio::select! {
                msg = inbox.recv() => match msg {
                    Some(msg) => self.handle(msg).await,
                    None => {
                        self.write_pending(true).await;
                        return;
                    }
                },
                () = tokio::time::sleep_until(due), if next.is_some() => self.write_pending(false).await,
                () = reports.notified() => self.reports_changed(),
            }
        }
    }

    async fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::SetLocale { locale, reply } => {
                let saved = self.save_locale(locale).await;
                let _ = reply.send(saved);
            }
            Msg::DraftUnit { project, unit } => {
                self.shared
                    .with_project(&project, |doc| doc.set_draft_unit(unit));
                self.write_now(&Target::Project(project)).await;
            }
            Msg::WriteApp => {
                self.write_now(&Target::App).await;
            }
            Msg::LastUsed { folder, at } => {
                lock(&self.shared.docs).app.set_last_used(&folder, at);
                self.write_now(&Target::App).await;
            }
            Msg::Place { project, update } => self.observe(&project, &update),
            Msg::LastEdit { project, edit } => {
                self.observe(&project, &edit.unit.place_update(edit.mode, edit.tool));
                lock(&self.shared.docs).app.set_last_edit(&edit);
                self.pending
                    .insert(Target::App, Instant::now() + LAST_EDIT_WRITE_AFTER);
            }
            Msg::Summary { folder } => {
                let bytes = lock(&self.shared.summaries)
                    .get(&folder)
                    .and_then(|kept| kept.as_ref().map(|s| s.bytes(&folder)));
                if let Some(bytes) = bytes {
                    let path = AppDataPath::summary(&folder);
                    let written = write_file(&self.shared.files, path.clone(), bytes).await;
                    self.shared.report_write(&path, &written);
                }
            }
            Msg::Flush { reply } => {
                self.write_pending(true).await;
                let _ = reply.send(());
            }
            Msg::Reset { reply } => {
                self.reset().await;
                let _ = reply.send(());
            }
        }
    }

    /// tC4's `rememberPlace`: folds one observation into the unit's record. A move starts
    /// the project file's 500 ms timer again; a visit that changes nothing but the time does
    /// not, so editing in the verse the person is already in costs no write.
    fn observe(&mut self, project: &ProjectId, update: &PlaceUpdate) {
        let now = self.shared.clock.now();
        let key = place_key(update);
        let moved = self.shared.with_project(project, |doc| {
            let prior = doc.place(&key);
            let record = record_place(prior.as_ref(), update, now);
            let moved = prior.as_ref().is_none_or(|prior| {
                *prior
                    != PlaceRecord {
                        at: prior.at,
                        ..record.clone()
                    }
            });
            doc.set_place(&key, &record);
            moved
        });
        if moved {
            self.pending.insert(
                Target::Project(project.clone()),
                Instant::now() + PLACE_WRITE_AFTER,
            );
        }
    }

    /// A report was added or the log cleared: `reports.json` is written a second after the
    /// first change it does not hold, and a stream of reports still writes once a second.
    fn reports_changed(&mut self) {
        if self.shared.reports.version() != self.reports_written {
            self.pending
                .entry(Target::Reports)
                .or_insert_with(|| Instant::now() + REPORTS_WRITE_AFTER);
        }
    }

    /// Writes the app document with the new language, and only on success makes it current.
    /// Only this task changes the documents, so the app document is still the one the
    /// candidate was made from when the write ends.
    async fn save_locale(&mut self, locale: LocaleId) -> Result<(), AppDataError> {
        let mut candidate = lock(&self.shared.docs).app.clone();
        candidate.set_locale(locale);
        let path = AppDataPath::app_prefs();
        let written = write_file(&self.shared.files, path.clone(), candidate.bytes()).await;
        self.shared.report_write(&path, &written);
        written?;
        lock(&self.shared.docs).app = candidate;
        // The write carried every change made so far.
        self.pending.remove(&Target::App);
        Ok(())
    }

    async fn reset(&mut self) {
        self.pending.clear();
        let targets: Vec<Target> = {
            let mut docs = lock(&self.shared.docs);
            docs.app = Doc::default();
            let projects: Vec<ProjectId> = docs.projects.keys().cloned().collect();
            for doc in docs.projects.values_mut() {
                *doc = Doc::default();
            }
            std::iter::once(Target::App)
                .chain(projects.into_iter().map(Target::Project))
                .collect()
        };
        for target in targets {
            self.write_now(&target).await;
        }
        // Every summary this writer read or wrote goes: the projects go back to their seed.
        let summaries: Vec<ProjectRef> = lock(&self.shared.summaries)
            .drain()
            .map(|(f, _)| f)
            .collect();
        for folder in summaries {
            let path = AppDataPath::summary(&folder);
            if let Err(error) = remove_file(&self.shared.files, path.clone()).await {
                self.shared
                    .report(&path, &format!("could not be removed: {error}"));
            }
        }
        if self.shared.reports.version() != self.reports_written {
            self.write_now(&Target::Reports).await;
        }
    }

    /// Writes the file now, from the document as it is. A failure keeps the value in memory
    /// and adds a report; the next change of the file writes again.
    async fn write_now(&mut self, target: &Target) {
        self.pending.remove(target);
        let path = target.path();
        if *target == Target::Reports {
            let (version, kept) = self.shared.reports.versioned();
            let written = write_file(&self.shared.files, path.clone(), reports_bytes(&kept)).await;
            self.shared.report_write(&path, &written);
            self.reports_written = match written {
                Ok(Written::Durable) => version,
                // The report of this write cannot be written either; the next real change
                // tries again with it.
                Ok(Written::NotDurable(_)) | Err(_) => self.shared.reports.version(),
            };
            return;
        }
        let bytes = {
            let docs = lock(&self.shared.docs);
            match target {
                Target::App => docs.app.bytes(),
                Target::Project(id) => docs.projects.get(id).map(Doc::bytes).unwrap_or_default(),
                Target::Reports => return,
            }
        };
        let written = write_file(&self.shared.files, path.clone(), bytes).await;
        self.shared.report_write(&path, &written);
    }

    /// Writes the files that are due, or all that are pending (a change of the reports not
    /// noticed yet included).
    async fn write_pending(&mut self, all: bool) {
        if all {
            self.reports_changed();
        }
        let now = Instant::now();
        let due: Vec<Target> = self
            .pending
            .iter()
            .filter(|(_, due)| all || **due <= now)
            .map(|(target, _)| target.clone())
            .collect();
        for target in due {
            self.write_now(&target).await;
        }
    }
}
