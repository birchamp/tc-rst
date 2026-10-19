//! This person's preferences on this computer (APP-DESIGN §4.9, A-20): the app language, which
//! projects were used when, where the person last worked in each book or story, and the Resume
//! card's record. They are never part of a project.
//!
//! Two kinds of file hold them: `prefs/app.json` and `prefs/<projectId>.json`. Before unit S1
//! (C1a moves the app onto `tc_store::atomic_write`) the files live in [`MemoryFiles`].
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

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tc_api::check::Tool;
use tc_api::ids::{LocaleId, ProjectId, ProjectRef, RefPart, StoryNo, WallMs};
use tc_api::prefs::{DraftUnit, PlaceChange, PlaceMode, PlaceUpdate};
use tc_core::BookId;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use crate::clock::Clock;
use crate::reports::{ReportLog, ReportOp, ReportOutcome, facts};

/// How long after the last move a place is written (tC4: 500 ms).
pub const PLACE_WRITE_AFTER: Duration = Duration::from_millis(500);

/// How long after the last change the Resume record is written (A-20).
pub const LAST_EDIT_WRITE_AFTER: Duration = Duration::from_secs(5);

/// The file format version this build writes into a document that has none.
const SCHEMA_VERSION: u64 = 1;

// ---- the storage seam ----------------------------------------------------------------------

/// A file in the app-data folder, by its path below it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AppDataPath(String);

impl AppDataPath {
    pub fn app_prefs() -> Self {
        Self("prefs/app.json".to_owned())
    }

    pub fn project_prefs(id: &ProjectId) -> Self {
        Self(format!("prefs/{}.json", id.as_str()))
    }

    /// Where the bytes of this file are copied when they cannot be read.
    pub fn unreadable_copy(&self) -> Self {
        Self(format!("{}.unreadable", self.0))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct AppDataError(pub String);

/// The app-data folder. Writes are atomic: after a failure or a crash a file holds the old
/// bytes or the new, never a mixture (AGENTS.md).
pub trait AppDataFiles: Send + Sync + 'static {
    fn read(&self, path: &AppDataPath) -> Result<Option<Vec<u8>>, AppDataError>;
    fn write(&self, path: &AppDataPath, bytes: &[u8]) -> Result<(), AppDataError>;
}

/// Phase 0's app data (§4.9: "Before S1 these files live in memory").
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

    fn write(&self, path: &AppDataPath, bytes: &[u8]) -> Result<(), AppDataError> {
        if self.failing.load(Ordering::SeqCst) {
            return Err(AppDataError("the disk is full".to_owned()));
        }
        lock(&self.files).insert(path.clone(), bytes.to_vec());
        lock(&self.writes).push((path.clone(), Instant::now()));
        Ok(())
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
        let kept = match self.files.read(&copy) {
            // An earlier unreadable file is already kept; the first one is the one to look at.
            Ok(Some(_)) => Ok(()),
            _ => self.files.write(&copy, &bytes),
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

/// A file the task writes: the app's, or one project's.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Target {
    App,
    Project(ProjectId),
}

impl Target {
    fn path(&self) -> AppDataPath {
        match self {
            Self::App => AppDataPath::app_prefs(),
            Self::Project(id) => AppDataPath::project_prefs(id),
        }
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
    AlignSuggestions {
        folder: ProjectRef,
        on: bool,
    },
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
    /// Reads `prefs/app.json` and starts the writer. Must run inside a tokio runtime.
    pub fn start(files: Arc<dyn AppDataFiles>, reports: ReportLog, clock: Arc<dyn Clock>) -> Self {
        let shared = Arc::new(Shared {
            files,
            reports,
            clock,
            docs: Mutex::new(Docs {
                app: Doc::default(),
                projects: HashMap::new(),
            }),
        });
        let app = shared.load(&AppDataPath::app_prefs());
        lock(&shared.docs).app = app;
        let (tx, rx) = mpsc::unbounded_channel();
        let task = Task {
            shared: Arc::clone(&shared),
            pending: BTreeMap::new(),
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

    /// The aligner's suggestions switch for a folder, written at once.
    pub fn set_align_suggestions(&self, folder: ProjectRef, on: bool) {
        self.send(Msg::AlignSuggestions { folder, on });
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

    /// The person goes Home: the Resume record and the places are written now.
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

    /// Back to the defaults, with nothing pending (`test/reset`).
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
}

struct Task {
    shared: Arc<Shared>,
    /// The files with a change not written yet, and when each is due.
    pending: BTreeMap<Target, Instant>,
}

impl Task {
    async fn run(mut self, mut inbox: mpsc::UnboundedReceiver<Msg>) {
        loop {
            let next = self.pending.values().min().copied();
            let due = next.unwrap_or_else(|| Instant::now() + Duration::from_secs(365 * 24 * 3600));
            tokio::select! {
                msg = inbox.recv() => match msg {
                    Some(msg) => self.handle(msg),
                    None => {
                        self.write_pending(true);
                        return;
                    }
                },
                () = tokio::time::sleep_until(due), if next.is_some() => self.write_pending(false),
            }
        }
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::SetLocale { locale, reply } => {
                let _ = reply.send(self.save_locale(locale));
            }
            Msg::DraftUnit { project, unit } => {
                self.shared
                    .with_project(&project, |doc| doc.set_draft_unit(unit));
                self.write_now(&Target::Project(project));
            }
            Msg::AlignSuggestions { folder, on } => {
                lock(&self.shared.docs)
                    .app
                    .set_align_suggestions(&folder, on);
                self.write_now(&Target::App);
            }
            Msg::LastUsed { folder, at } => {
                lock(&self.shared.docs).app.set_last_used(&folder, at);
                self.write_now(&Target::App);
            }
            Msg::Place { project, update } => self.observe(&project, &update),
            Msg::LastEdit { project, edit } => {
                self.observe(&project, &edit.unit.place_update(edit.mode, edit.tool));
                lock(&self.shared.docs).app.set_last_edit(&edit);
                self.pending
                    .insert(Target::App, Instant::now() + LAST_EDIT_WRITE_AFTER);
            }
            Msg::Flush { reply } => {
                self.write_pending(true);
                let _ = reply.send(());
            }
            Msg::Reset { reply } => {
                self.reset();
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

    /// Writes the app document with the new language, and only on success makes it current.
    fn save_locale(&mut self, locale: LocaleId) -> Result<(), AppDataError> {
        let mut candidate = lock(&self.shared.docs).app.clone();
        candidate.set_locale(locale);
        let path = AppDataPath::app_prefs();
        if let Err(error) = self.shared.files.write(&path, &candidate.bytes()) {
            self.shared
                .report(&path, &format!("could not be written: {error}"));
            return Err(error);
        }
        lock(&self.shared.docs).app = candidate;
        // The write carried every change made so far.
        self.pending.remove(&Target::App);
        Ok(())
    }

    fn reset(&mut self) {
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
            self.write_now(&target);
        }
    }

    /// Writes the file now, from the document as it is. A failure keeps the value in memory
    /// and adds a report; the next change of the file writes again.
    // C1a's disk writer must snapshot the bytes under the lock and write them on a blocking
    // thread instead; the in-memory files need no wait.
    fn write_now(&mut self, target: &Target) {
        let bytes = {
            let docs = lock(&self.shared.docs);
            match target {
                Target::App => docs.app.bytes(),
                Target::Project(id) => docs.projects.get(id).map(Doc::bytes).unwrap_or_default(),
            }
        };
        self.pending.remove(target);
        let path = target.path();
        if let Err(error) = self.shared.files.write(&path, &bytes) {
            self.shared
                .report(&path, &format!("could not be written: {error}"));
        }
    }

    /// Writes the files that are due, or all that are pending.
    fn write_pending(&mut self, all: bool) {
        let now = Instant::now();
        let due: Vec<Target> = self
            .pending
            .iter()
            .filter(|(_, due)| all || **due <= now)
            .map(|(target, _)| target.clone())
            .collect();
        for target in due {
            self.write_now(&target);
        }
    }
}
