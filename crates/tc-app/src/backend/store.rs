//! The projects folder through tc-store (APP-DESIGN §4.2, unit C0): the store adapter.
//!
//! tc-store's `ProjectHandle` already has the seam's shape (R4, R5, R14, R16): an O(1)
//! snapshot that reads never wait behind, an asynchronous submit whose reply comes after the
//! durability point with the sequence number of the snapshot that holds it, typed events with
//! each batch's `Delta`, drafts beside the actor, counters and close. So this adapter is thin:
//! each seam method calls the matching store method and maps its types. It adds what the
//! store cannot know:
//!
//! - **Who asked for a command.** The store's `Committed` names the batch's changes, not the
//!   request, and it is sent before the reply resolves. The relay thread waits for every
//!   reply in the order the actor takes the commands and records which request wrote which
//!   change, whether or not anyone still awaits the reply; the events thread waits for that
//!   record before it passes a command's `Committed` on, so a person's own save is announced
//!   as their request's (AD37). A batch no submit asked for is passed on as the store's own
//!   as soon as it cannot be a submit's (`Origins::take`).
//! - **Nothing blocks a tokio worker.** tc-store's close and drop wait for the actor's
//!   background work and its join; both run on a thread of their own.
//! - **The listing hides no folder that holds a project's files.** It reads the projects
//!   folder itself, where tc-store's `Store::list` keeps only folders with a `project.json`:
//!   a folder that has lost that file, a translationCore 4.0 project copied in, and a link to
//!   a project elsewhere are listed as unusable with the reason. It opens only the exact
//!   names it lists.
//! - **What the start removed is recorded.** tc-store's start removes the staging folders a
//!   create, copy, join or import cut short left; each goes into the report log
//!   (`report_removed_staging`), for "Ask for help" and for the units that tell the person.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError, RwLock};

use serde::Deserialize;
use tc_api::event::{CheckpointOutcome, SessionMode};
use tc_api::ids::{ProjectRef, WallMs};
use tc_api::jobs::{OpenPhase, Phase};
use tc_api::write::ReadOnlyReason;
use tc_journal::command::{Command, Draft, DraftField};
use tc_journal::fold::{FoldState, ProjectKind};
use tc_journal::keys::PinGroup;
use tc_journal::record::PinGroupValue;
use tc_journal::token::TokenError;
use tc_journal::{ChangeId, RecordKey, ShownToken};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::oneshot;

use super::{
    BoxFuture, CheckpointReason, CommandError, CopyFailure, DraftToken, EditToken, Listing,
    OfferedDraft, OpenFailure, Origin, ProjectEntry, ProjectFacts, ProjectHandle, ProjectStore,
    Published, ReviewToken, Snapshot, StoreCause, StoreCommand, StoreCounters, StoreError,
    StoreEvent, StoreNotice, StoreReply, Stored, SummaryFiles, SummaryStamp, SummaryWatch,
    UnusableFolder, UnusableReason,
};
use crate::clock::Clock;
use crate::jobs::ProgressSink;
use crate::reports::{ReportLog, ReportOp, ReportOutcome};

/// The prefix of the staging folders tc-store builds a project in before it renames it into
/// place (tc-store's private `STAGING`, `store.rs`). Such a folder holds a project's files
/// while a create, copy, join or import still writes them; tc-store's start removes those a
/// crash left (STORAGE §6.4 N1), and `report_removed_staging` records each.
const STAGING: &str = ".staging-";

/// `ingredients/tc/project.json`, relative to a project folder (STORAGE §3.5).
const PROJECT_JSON: &str = tc_store::layout::PROJECT_JSON;

/// What a folder holds of a project when its `project.json` is gone: every Scripture Burrito
/// has `metadata.json` and `ingredients/` (a translationCore 4.0 project has nothing more),
/// and `.tc/` holds this app's saves not yet sealed and its unsaved drafts (STORAGE §3.6).
/// A folder with none of them holds no one's work, and the listing leaves it out.
const PROJECT_PARTS: [&str; 3] = [
    tc_store::layout::METADATA,
    tc_store::layout::INGREDIENTS,
    tc_store::layout::TC,
];

/// `.tc/sync.json` (STORAGE §3.6): the last confirmed Door43 state, with the remote.
const SYNC_JSON: &str = ".tc/sync.json";

/// The conflicts file, whose verse conflicts a card counts.
const CONFLICTS: &str = "ingredients/tc/conflicts.json";

/// `rel`, a path of the layout with `/` between its parts, under `root`.
fn under(root: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(root.to_owned(), |p, part| p.join(part))
}

/// Whether a failed read found nothing there: no file, or a path through something that is
/// not a folder (a link to a file).
fn absent(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
    )
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Every change made under these locks is a store of a whole value.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn store_error(error: impl std::fmt::Display) -> StoreError {
    StoreError::Other(error.to_string())
}

/// Starts a named thread; `None` when the system has none to give.
fn spawn(name: &str, f: impl FnOnce() + Send + 'static) -> Option<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(f)
        .ok()
}

// ---------------------------------------------------------------------------------------
// The backend
// ---------------------------------------------------------------------------------------

/// The projects folder through tc-store (see the module documentation).
pub struct StoreBackend {
    store: tc_store::Store,
    /// What the store started with, for the store of the local projects folder.
    config: tc_store::StoreConfig,
    projects: PathBuf,
    vfs: Arc<dyn tc_store::Vfs>,
    hooks: Arc<tc_store::Hooks>,
    /// What the projects folder is: a copy of a read-only project is made only where it can
    /// be written safely (C1b).
    platform: Arc<dyn tc_store::Platform>,
    /// Folders whose lock the person said names a computer that is closed (C1b): the next
    /// open of each takes the lock.
    trusted: Mutex<std::collections::BTreeSet<ProjectRef>>,
    /// This server run, which each open's drafts record (C1b).
    run: Mutex<Option<String>>,
    /// Where this computer's local projects folder is, when it is not the projects folder
    /// itself (Q-30).
    local: Option<PathBuf>,
    /// The store over the local projects folder: started with this one when the folder exists,
    /// else by the first copy that needs it. Home lists its projects beside this folder's.
    beside: Mutex<Option<Arc<StoreBackend>>>,
    /// The projects this store opened, so a test can wait until each has written what it was
    /// asked to (P5's watch reads what the writer saw only then).
    #[cfg(feature = "test-hooks")]
    opened: Arc<Mutex<Vec<std::sync::Weak<StoreHandle>>>>,
    /// The names the local projects folder listed last, so a read of one project goes to the
    /// right folder without listing either.
    beside_names: Mutex<std::collections::BTreeSet<ProjectRef>>,
}

impl std::fmt::Debug for StoreBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreBackend")
            .field("projects", &self.projects)
            .finish_non_exhaustive()
    }
}

impl StoreBackend {
    /// A store over `config`; `hooks` are the holds a test places (none in production).
    pub fn start(
        config: tc_store::StoreConfig,
        hooks: Arc<tc_store::Hooks>,
    ) -> Result<Self, StoreError> {
        let projects = config.projects.clone();
        let vfs = config.vfs.clone();
        let platform = config.platform.clone();
        let store = tc_store::Store::start_with_hooks(config.clone(), hooks.clone())
            .map_err(store_error)?;
        Ok(Self {
            store,
            config,
            projects,
            vfs,
            hooks,
            platform,
            trusted: Mutex::default(),
            run: Mutex::default(),
            local: None,
            beside: Mutex::default(),
            beside_names: Mutex::default(),
            #[cfg(feature = "test-hooks")]
            opened: Arc::default(),
        })
    }

    /// Waits until every project this store has open has written all it was asked to, and
    /// answers how many were open. A test hook, never called by the product.
    #[cfg(feature = "test-hooks")]
    pub fn idle_waiter(&self) -> impl Fn() -> usize + Send + Sync + 'static {
        let opened = self.opened.clone();
        move || {
            let open: Vec<Arc<StoreHandle>> = lock(&opened)
                .iter()
                .filter_map(std::sync::Weak::upgrade)
                .collect();
            for handle in &open {
                ProjectHandle::wait_idle(handle.as_ref());
            }
            open.iter()
                .filter(|handle| handle.inner().is_some())
                .count()
        }
    }

    /// Names the other projects folder (Q-30): the local one beside the projects folder, or the
    /// default one where the local one is the projects folder. A copy of a project goes there
    /// when the projects folder itself cannot keep a copy safe. A folder that exists already
    /// holds projects, so it is started now and Home lists them.
    pub fn with_local_projects(mut self, local: PathBuf) -> Result<Self, StoreError> {
        let exists = self
            .vfs
            .metadata(&local)
            .is_ok_and(|meta| meta.kind == tc_store::vfs::EntryKind::Dir);
        self.local = Some(local);
        if exists {
            let beside = self.start_beside()?;
            *lock(&self.beside) = Some(beside);
        }
        Ok(self)
    }

    /// The store over the local projects folder, which has no local folder of its own.
    fn start_beside(&self) -> Result<Arc<StoreBackend>, StoreError> {
        let Some(local) = &self.local else {
            return Err(store_error("this computer has no local projects folder"));
        };
        let config = tc_store::StoreConfig {
            projects: local.clone(),
            ..self.config.clone()
        };
        Self::start(config, self.hooks.clone()).map(Arc::new)
    }

    /// The store of the local projects folder, when it holds `folder`.
    fn beside_of(&self, folder: &ProjectRef) -> Option<Arc<StoreBackend>> {
        if lock(&self.beside_names).contains(folder) {
            lock(&self.beside).clone()
        } else {
            None
        }
    }

    /// The store a copy is written by: this one where its folder can keep a copy safe, else the
    /// local projects folder's, made now if no copy has needed it yet (Q-30). `Err` says why no
    /// folder of this computer can: the reason the projects folder cannot.
    fn copy_target(&self) -> Result<Option<Arc<StoreBackend>>, CopyFailure> {
        let Some(place) = self.platform.folder(&self.projects).unsafe_folder else {
            return Ok(None);
        };
        let unsafe_here = || {
            CopyFailure::Unsafe(read_only_reason(&tc_store::ReadOnlyReason::UnsafeFolder(
                place.clone(),
            )))
        };
        let Some(local) = &self.local else {
            return Err(unsafe_here());
        };
        let mut beside = lock(&self.beside);
        if let Some(beside) = &*beside {
            // The folder was checked when it was made, and a cloud client may have come since.
            return if self.platform.folder(local).unsafe_folder.is_none() {
                Ok(Some(beside.clone()))
            } else {
                Err(unsafe_here())
            };
        }
        tc_store::atomic_create_dirs(&*self.vfs, local)
            .map_err(|e| CopyFailure::Failed(format!("{}: {e}", local.display())))?;
        if self.platform.folder(local).unsafe_folder.is_some() {
            return Err(unsafe_here());
        }
        let started = self
            .start_beside()
            .map_err(|e| CopyFailure::Failed(e.to_string()))?;
        *beside = Some(started.clone());
        Ok(Some(started))
    }

    /// Production: this computer's files, clock and processors.
    pub fn on_disk(app_data: &Path, projects: &Path) -> Result<Self, StoreError> {
        let threads = std::thread::available_parallelism().map_or(1, usize::from);
        Self::start(
            tc_store::StoreConfig {
                app_data: app_data.to_owned(),
                projects: projects.to_owned(),
                vfs: Arc::new(tc_store::StdFs),
                platform: Arc::new(tc_store::StdPlatform),
                clock: Arc::new(tc_store::SystemClock),
                threads,
                // STORAGE §11.1: a program is done writing a file when it has been quiet this long.
                watch_debounce: std::time::Duration::from_secs(1),
            },
            Arc::default(),
        )
    }

    /// The store itself, for a test's set-up and C1a's scenario writer.
    #[cfg(any(feature = "test-hooks", feature = "fixture-backend"))]
    pub fn store(&self) -> &tc_store::Store {
        &self.store
    }

    /// The store's file system: C1a's scenario writer writes through it, so its tests run on
    /// tc-store's `FaultFs`.
    #[cfg(any(feature = "test-hooks", feature = "fixture-backend"))]
    pub fn vfs(&self) -> &Arc<dyn tc_store::Vfs> {
        &self.vfs
    }

    /// The holds a test places on the store's threads.
    pub fn hooks(&self) -> &Arc<tc_store::Hooks> {
        &self.hooks
    }

    /// The projects folder.
    pub fn projects(&self) -> &Path {
        &self.projects
    }

    /// What a Home card's summary of `folder` is computed from (APP-DESIGN §4.5, C1a D8): one
    /// listing of `.tc/pending/` and one stat of `.tc/materialized.json`, no file read, no lock
    /// and no fold, at any history. A temporary file a crash left is no save and is left out.
    /// With no manifest (a copy never opened here), also a stat of each file the summary reads.
    pub fn summary_stamp(&self, folder: &ProjectRef) -> Result<SummaryStamp, StoreError> {
        // The card's folder comes from the listing, so its name is the listed one: no second
        // listing of the projects folder here.
        if folder.as_str().starts_with(STAGING) {
            return Err(store_error(format!("{folder} is a staging folder")));
        }
        let root = self.projects.join(folder.as_str());
        let pending_dir = under(&root, tc_store::layout::PENDING);
        let pending: Vec<String> = match self.vfs.read_dir(&pending_dir) {
            Ok(entries) => entries
                .into_iter()
                .filter(|e| e.kind == tc_store::vfs::EntryKind::File)
                .filter_map(|e| e.name.to_str().map(str::to_owned))
                .filter(|name| tc_store::atomic::temp_target(name).is_none())
                .collect(),
            Err(e) if absent(&e) => Vec::new(),
            Err(e) => return Err(store_error(format!("{}: {e}", pending_dir.display()))),
        };
        let mut files = vec![self.stat(&root, tc_store::layout::MANIFEST)?];
        if files[0].1.is_none() {
            for rel in self.summary_sources(&root)? {
                files.push(self.stat(&root, &rel)?);
            }
        }
        Ok(SummaryStamp::of(&pending, &files))
    }

    /// The size and modification time of `rel` under `root`; `None` when it is missing.
    fn stat(&self, root: &Path, rel: &str) -> Result<(String, Option<(u64, i64)>), StoreError> {
        let path = under(root, rel);
        match self.vfs.metadata(&path) {
            Ok(meta) => Ok((rel.to_owned(), Some((meta.len, meta.mtime_ns)))),
            Err(e) if absent(&e) => Ok((rel.to_owned(), None)),
            Err(e) => Err(store_error(format!("{}: {e}", path.display()))),
        }
    }

    /// The files under `root` a summary reads (`summary_files`): `metadata.json`, which names
    /// the books, `conflicts.json`, and each book and story file. One listing of
    /// `ingredients/`, and one of `ingredients/content/` in a story project.
    fn summary_sources(&self, root: &Path) -> Result<Vec<String>, StoreError> {
        use tc_store::vfs::EntryKind;
        let list = |rel: &str| match self.vfs.read_dir(&under(root, rel)) {
            Ok(entries) => Ok(entries),
            Err(e) if absent(&e) => Ok(Vec::new()),
            Err(e) => Err(store_error(format!("{rel}: {e}"))),
        };
        let mut sources = vec![tc_store::layout::METADATA.to_owned(), CONFLICTS.to_owned()];
        for entry in list(tc_store::layout::INGREDIENTS)? {
            let Some(name) = entry.name.to_str() else {
                continue;
            };
            match entry.kind {
                EntryKind::File if name.ends_with(".usfm") => {
                    sources.push(format!("ingredients/{name}"));
                }
                EntryKind::Dir if name == "content" => {
                    for story in list("ingredients/content")? {
                        if let Some(name) =
                            story.name.to_str().filter(|n| story_number(n).is_some())
                        {
                            sources.push(format!("ingredients/content/{name}"));
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(sources)
    }

    /// The staging folders tc-store's start removed (STORAGE §6.4 N1). It waits for that
    /// cleanup, which runs on a thread of tc-store's own.
    pub fn removed_at_start(&self) -> Vec<tc_store::RemovedStaging> {
        self.store.removed_at_start()
    }

    /// The folder's entry in the projects folder, by the exact name the listing holds: on a
    /// file system that ignores letter case, `titus` and `Titus` would otherwise be two
    /// sessions over one folder, and the second would meet this process's own lock.
    fn listed(&self, folder: &ProjectRef) -> Result<PathBuf, OpenFailure> {
        let entries = self
            .vfs
            .read_dir(&self.projects)
            .map_err(|e| OpenFailure::Unreadable {
                reason: format!("{}: {e}", self.projects.display()),
            })?;
        let name = folder.as_str();
        let found = entries.iter().any(|entry| {
            entry.kind == tc_store::vfs::EntryKind::Dir
                && entry.name.to_str() == Some(name)
                && !name.starts_with(STAGING)
        });
        if found {
            Ok(self.projects.join(name))
        } else {
            Err(OpenFailure::NotAProject)
        }
    }

    /// One entry of the projects folder, read as tc-store will read it at an open: a project;
    /// a folder that holds a project's files but cannot be opened, with the reason; or `None`
    /// for an entry that holds no project (a file, a folder with none of a project's files, a
    /// staging folder a create, copy, join or import is still writing).
    fn entry(
        &self,
        entry: &tc_store::vfs::DirEntry,
    ) -> Option<Result<ProjectEntry, UnusableFolder>> {
        use tc_store::vfs::EntryKind;
        let lossy = entry.name.to_string_lossy().into_owned();
        match entry.kind {
            EntryKind::File => return None,
            EntryKind::Dir if lossy.starts_with(STAGING) => return None,
            EntryKind::Dir | EntryKind::Other => {}
        }
        let path = self.projects.join(&entry.name);
        let read = self.vfs.read(&path.join(PROJECT_JSON));
        let holds_a_project = match &read {
            Err(e) if absent(e) => PROJECT_PARTS
                .iter()
                .any(|part| self.vfs.metadata(&path.join(part)).is_ok()),
            // A file that is there, or a folder that cannot be looked into: either may hold
            // someone's work.
            Ok(_) | Err(_) => true,
        };
        if !holds_a_project {
            return None;
        }
        let unusable = |reason: UnusableReason| {
            Some(Err(UnusableFolder {
                name: lossy.clone(),
                reason,
            }))
        };
        if entry.kind == EntryKind::Other {
            return unusable(UnusableReason::Link);
        }
        let Some(name) = entry.name.to_str() else {
            return unusable(UnusableReason::NameNotText);
        };
        let Ok(folder) = ProjectRef::parse(name) else {
            return unusable(UnusableReason::NameNotAllowed);
        };
        // The open writes a missing or damaged project.json back from the project's history
        // (§3.5); the listing reads what the open would.
        let repaired = || {
            self.store
                .repaired_project_json(&path)
                .and_then(|b| project_json(&b).ok())
        };
        let bytes = match read {
            Ok(bytes) => bytes,
            Err(e) if absent(&e) => {
                return match repaired() {
                    Some((id, kind)) => Some(Ok(ProjectEntry { folder, id, kind })),
                    None => unusable(UnusableReason::NoProjectFile),
                };
            }
            Err(e) => return unusable(UnusableReason::ProjectFileUnreadable(e.to_string())),
        };
        match project_json(&bytes) {
            Ok((id, kind)) => Some(Ok(ProjectEntry { folder, id, kind })),
            Err(reason) => match repaired() {
                Some((id, kind)) => Some(Ok(ProjectEntry { folder, id, kind })),
                None => unusable(UnusableReason::ProjectFileDamaged(reason)),
            },
        }
    }

    /// Reads the projects folder itself: tc-store's `Store::list` keeps only the folders with a
    /// `project.json`, and would hide one that holds someone's work without it.
    fn list_here(&self) -> Result<Listing, StoreError> {
        let entries = self
            .vfs
            .read_dir(&self.projects)
            .map_err(|e| store_error(format!("{}: {e}", self.projects.display())))?;
        let mut listing = Listing::default();
        for entry in &entries {
            match self.entry(entry) {
                Some(Ok(entry)) => listing.projects.push(entry),
                Some(Err(unusable)) => listing.unusable.push(unusable),
                None => listing
                    .others
                    .push(entry.name.to_string_lossy().into_owned()),
            }
        }
        listing.projects.sort_by(|a, b| a.folder.cmp(&b.folder));
        listing.unusable.sort_by(|a, b| a.name.cmp(&b.name));
        listing.others.sort();
        Ok(listing)
    }
}

/// `NN.md`, a story of Open Bible Stories (1 to 50), as tC4's `listStories` reads the content
/// folder.
fn story_number(name: &str) -> Option<u8> {
    let digits = name.strip_suffix(".md")?;
    if digits.len() != 2 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u8 = digits.parse().ok()?;
    (1..=50).contains(&n).then_some(n)
}

/// What a listing reads of `project.json`, read as tc-store's open reads it (its private
/// `ProjectJson::parse`, `open.rs`): the same four fields required, with the same types, of
/// which only `projectId` and `kind` are used. A folder the open would refuse as not a project
/// is so listed as unusable with the reason, never offered as a project that then fails to
/// open; `tests/store_adapter.rs`'s `a_folder_is_listed_as_a_project_exactly_when_tc_store_opens_it`
/// holds the two readers together.
fn project_json(bytes: &[u8]) -> Result<(tc_journal::ProjectId, ProjectKind), String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct File {
        // Required because tc-store's reader requires them; their values are tc-store's.
        #[serde(rename = "schemaVersion")]
        _schema_version: u32,
        project_id: String,
        kind: String,
        #[serde(rename = "created")]
        _created: String,
    }
    let file: File = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let kind = match file.kind.as_str() {
        "bible" => ProjectKind::Bible,
        "obs" => ProjectKind::Stories,
        other => return Err(format!("unknown project kind {other:?}")),
    };
    let id = tc_journal::ProjectId::parse(&file.project_id)
        .ok_or_else(|| format!("{:?} is not a project id", file.project_id))?;
    Ok((id, kind))
}

impl StoreBackend {
    fn read_text(&self, path: &Path) -> Result<String, String> {
        let bytes = self.vfs.read(path).map_err(|e| e.to_string())?;
        String::from_utf8(bytes).map_err(|_| "the file is not valid UTF-8".to_owned())
    }
}

/// The Door43 remote `.tc/sync.json` names. A file that is missing or does not read names
/// none: the card then says the project is only on this computer, never that it is shared.
fn sync_remote(bytes: &[u8]) -> Option<super::Door43Remote> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Sync {
        remote_url: String,
    }
    let sync: Sync = serde_json::from_slice(bytes).ok()?;
    super::Door43Remote::parse_url(&sync.remote_url)
}

impl ProjectStore for StoreBackend {
    fn peek(&self, folder: &ProjectRef) -> Result<super::ProjectPeek, StoreError> {
        if let Some(beside) = self.beside_of(folder) {
            return beside.peek(folder);
        }
        let path = self.listed(folder).map_err(|failure| match failure {
            OpenFailure::NotAProject => StoreError::NotAProject,
            OpenFailure::Unreadable { reason } => StoreError::Other(reason),
            other => store_error(format!("{other:?}")),
        })?;
        let metadata = self
            .vfs
            .read(&path.join(tc_store::layout::METADATA))
            .map_err(|e| e.to_string())
            .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string()));
        let remote = self
            .vfs
            .read(&path.join(SYNC_JSON))
            .ok()
            .and_then(|bytes| sync_remote(&bytes));
        let text_direction = self
            .vfs
            .read(&path.join(tc_journal::fold::DerivedFile::Settings.path()))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|doc| doc.get("textDirection")?.as_str().map(str::to_owned));
        Ok(super::ProjectPeek {
            metadata,
            text_direction,
            remote,
            stamp: self.summary_stamp(folder).ok(),
        })
    }

    fn summary_files(
        &self,
        folder: &ProjectRef,
        books: &[tc_journal::BookId],
        watch: &SummaryWatch,
    ) -> Result<SummaryFiles, StoreError> {
        if let Some(beside) = self.beside_of(folder) {
            return beside.summary_files(folder, books, watch);
        }
        let cancelled = || store_error("cancelled");
        let path = self
            .listed(folder)
            .map_err(|_| store_error("not a project"))?;
        let ingredients = path.join(tc_store::layout::INGREDIENTS);
        let mut files = SummaryFiles::default();
        for book in books {
            watch.cancel.check().map_err(|_| cancelled())?;
            let usfm = self.read_text(&ingredients.join(format!("{}.usfm", book.code())));
            files.books.push((*book, usfm));
        }
        let kind = self
            .vfs
            .read(&path.join(PROJECT_JSON))
            .ok()
            .and_then(|bytes| project_json(&bytes).ok())
            .map(|(_, kind)| kind);
        if kind == Some(ProjectKind::Stories) {
            let content = ingredients.join("content");
            files.stories = Some(match self.vfs.read_dir(&content) {
                Err(e) => Err(e.to_string()),
                Ok(entries) => {
                    let mut numbers: Vec<u8> = entries
                        .iter()
                        .filter_map(|entry| story_number(entry.name.to_str()?))
                        .collect();
                    numbers.sort_unstable();
                    let mut stories = Vec::new();
                    for n in numbers {
                        watch.cancel.check().map_err(|_| cancelled())?;
                        stories.push((n, self.read_text(&content.join(format!("{n:02}.md")))));
                    }
                    Ok(stories)
                }
            });
            files.resources = Some(
                self.vfs
                    .read(&ingredients.join("checking/resources.json"))
                    .map_err(|e| e.to_string()),
            );
        }
        files.conflicts = match self.vfs.read(&under(&path, CONFLICTS)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if absent(&e) => Ok(None),
            Err(e) => Err(e.to_string()),
        };
        Ok(files)
    }

    fn derived_file(
        &self,
        folder: &ProjectRef,
        file: &tc_journal::fold::DerivedFile,
    ) -> Result<Option<Vec<u8>>, StoreError> {
        if let Some(beside) = self.beside_of(folder) {
            return beside.derived_file(folder, file);
        }
        let path = self.listed(folder).map_err(|_| StoreError::NotAProject)?;
        let rel = file.path();
        match self.vfs.read(&under(&path, &rel)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if absent(&e) => Ok(None),
            Err(e) => Err(store_error(format!("{rel}: {e}"))),
        }
    }

    /// Both projects folders: the chosen one, and the local one beside it when copies were made
    /// there (Q-30).
    fn list(&self) -> Result<Listing, StoreError> {
        let mut listing = self.list_here()?;
        let beside = lock(&self.beside).clone();
        if let Some(beside) = beside {
            let there = beside.list_here()?;
            // A name this folder holds is this folder's: one name never opens two folders.
            let taken = |name: &str| {
                listing.projects.iter().any(|p| p.folder.as_str() == name)
                    || listing.unusable.iter().any(|u| u.name == name)
                    || listing.others.iter().any(|o| o == name)
            };
            let mut names = std::collections::BTreeSet::new();
            let mut merged = Listing::default();
            for project in there.projects {
                if !taken(project.folder.as_str()) {
                    names.insert(project.folder.clone());
                    merged.projects.push(project);
                }
            }
            for unusable in there.unusable {
                if !taken(&unusable.name) {
                    merged.unusable.push(unusable);
                }
            }
            for other in there.others {
                if !taken(&other) {
                    merged.others.push(other);
                }
            }
            *lock(&self.beside_names) = names;
            listing.projects.extend(merged.projects);
            listing.unusable.extend(merged.unusable);
            listing.others.extend(merged.others);
            listing.projects.sort_by(|a, b| a.folder.cmp(&b.folder));
            listing.unusable.sort_by(|a, b| a.name.cmp(&b.name));
            listing.others.sort();
        }
        Ok(listing)
    }

    fn open(
        &self,
        folder: &ProjectRef,
        progress: ProgressSink,
    ) -> Result<Arc<dyn ProjectHandle>, OpenFailure> {
        if let Some(beside) = self.beside_of(folder) {
            return beside.open(folder, progress);
        }
        let path = self.listed(folder)?;
        let options = tc_store::OpenOptions {
            progress: forward_open(progress),
            // Once: a later open meets the lock afresh, since another computer may open the
            // project again meanwhile.
            other_computer_closed: lock(&self.trusted).remove(folder),
            creation_pins: creation_pins(),
            run: lock(&self.run).clone(),
        };
        let handle = self.store.open(&path, &options).map_err(open_failure)?;
        let handle = StoreHandle::start(handle).map_err(|reason| OpenFailure::Unreadable {
            reason: format!("the project could not be served: {reason}"),
        })?;
        #[cfg(feature = "test-hooks")]
        {
            let mut opened = lock(&self.opened);
            opened.retain(|handle| handle.strong_count() > 0);
            opened.push(Arc::downgrade(&handle));
        }
        Ok(handle)
    }

    /// The copy goes into the projects folder, or into the local projects folder where the
    /// projects folder itself is what made the project read-only (Q-30): a copy beside it would
    /// be read-only too. It is refused only where this computer has no folder that is safe.
    fn copy(&self, folder: &ProjectRef, name: &ProjectRef) -> Result<(), CopyFailure> {
        let from = match self.beside_of(folder) {
            Some(beside) => beside.listed(folder),
            None => self.listed(folder),
        }
        .map_err(|_| CopyFailure::Failed(format!("{folder} is not in the projects folder")))?;
        let beside = self.copy_target()?;
        let target = beside.as_deref().unwrap_or(self);
        // One name never opens two folders: the other folder's names are taken too.
        let listing = self
            .list()
            .map_err(|e| CopyFailure::Failed(e.to_string()))?;
        let taken = |n: &str| n == name.as_str();
        if listing.projects.iter().any(|p| taken(p.folder.as_str()))
            || listing.unusable.iter().any(|u| taken(&u.name))
            || listing.others.iter().any(|o| taken(o))
        {
            return Err(CopyFailure::NameExists);
        }
        match target.store.copy_project(&from, name.as_str()) {
            Ok(_) => {
                if beside.is_some() {
                    lock(&self.beside_names).insert(name.clone());
                }
                Ok(())
            }
            Err(tc_store::CreateError::NameExists(_)) => Err(CopyFailure::NameExists),
            Err(error) => Err(CopyFailure::Failed(error.to_string())),
        }
    }

    fn create(&self, new: super::NewProjectSpec) -> Result<(), super::CreateFailure> {
        use super::CreateFailure;
        let created = self.store.create(&tc_store::NewProject {
            name: new.folder.as_str().to_owned(),
            kind: new.kind,
            records: new.records,
            books: Vec::new(),
        });
        match created {
            Ok(_) => Ok(()),
            Err(tc_store::CreateError::NameExists(_)) => Err(CreateFailure::NameExists),
            Err(tc_store::CreateError::Refused(tc_store::CommandError::Refused(
                tc_store::Refusal::NotValidInBurrito(why),
            ))) => Err(CreateFailure::NotHeld(why)),
            Err(error) => Err(CreateFailure::Failed(error.to_string())),
        }
    }

    fn trust_lock(&self, folder: &ProjectRef) {
        if let Some(beside) = self.beside_of(folder) {
            return beside.trust_lock(folder);
        }
        lock(&self.trusted).insert(folder.clone());
    }

    fn name_run(&self, run: tc_api::ids::ServerInstance) {
        *lock(&self.run) = Some(run.to_string());
    }

    /// Projects on disk have no seed to go back to: a parity scenario's store
    /// (`scenario_store`) writes its projects again.
    #[cfg(feature = "test-hooks")]
    fn reset(&self) -> Result<(), StoreError> {
        Ok(())
    }

    #[cfg(feature = "test-hooks")]
    fn store_hooks(&self) -> Option<Arc<tc_store::Hooks>> {
        Some(self.hooks.clone())
    }
}

/// Records in the report log each staging folder tc-store's start removed (STORAGE §6.4 N1):
/// a create, copy, join or import that a crash or a quit cut short. tc-store removes them on a
/// thread of its own, so the reports are written from another, and the start never waits for
/// them. "Ask for help" attaches them; E1b tells the person of a join that did not finish, and
/// F1 of an import (APP-DESIGN §10).
pub(crate) fn report_removed_staging(
    backend: Arc<StoreBackend>,
    reports: ReportLog,
    clock: Arc<dyn Clock>,
) {
    fn record(backend: &StoreBackend, reports: &ReportLog, clock: &dyn Clock, started: WallMs) {
        for removed in backend.removed_at_start() {
            let (op, outcome, facts) = staging_report(&removed);
            reports.add(
                op,
                outcome,
                crate::reports::facts(facts),
                started,
                clock.now(),
            );
        }
    }
    let started = clock.now();
    let spawned = {
        let (backend, reports, clock) = (backend.clone(), reports.clone(), clock.clone());
        spawn("tc-app staging report", move || {
            record(&backend, &reports, clock.as_ref(), started);
        })
    };
    if spawned.is_none() {
        // No thread to be had: wait for the cleanup here rather than lose the reports.
        record(&backend, &reports, clock.as_ref(), started);
    }
}

/// The report of one staging folder the start removed. An import and a join are reported as
/// tC4's start reports the same moment (`tC4:src/data/journal/opsLog.ts:603-614, 677-691`):
/// under their own operation, with `interrupted` and `rolledBack`, an import with tC4's
/// refusal `import.write-failed` and a join with tC4's sentence, so "Ask for help" names
/// them as tC4's Feedback dialog does ("Report: import, …"). tC4 makes no staging folder for a
/// create or a copy, so those, and a folder whose name does not say its work, are this app's
/// `staging-removed`, with the folder's kind.
fn staging_report(
    removed: &tc_store::RemovedStaging,
) -> (ReportOp, ReportOutcome, serde_json::Value) {
    use tc_api::Message;
    use tc_api::refusal::ImportRefusal;
    use tc_store::StagingKind;
    let folder = &removed.folder;
    let rolled_back =
        || serde_json::json!({ "folder": folder, "interrupted": true, "rolledBack": true });
    let removed_work = |kind: Option<&str>, work: &str| {
        (
            ReportOp::StagingRemoved,
            ReportOutcome::Failed {
                reason: format!("{work} did not finish, so its part copy was removed"),
            },
            serde_json::json!({ "folder": folder, "kind": kind }),
        )
    };
    match removed.kind {
        Some(StagingKind::Import) => (
            ReportOp::Import,
            ReportOutcome::Refused {
                code: ImportRefusal::WriteFailed.code().map(str::to_owned),
                key: ImportRefusal::WriteFailed.key().to_owned(),
            },
            rolled_back(),
        ),
        Some(StagingKind::Join) => (
            ReportOp::OpenDoor43,
            ReportOutcome::Failed {
                reason: "the open from Door43 was interrupted; no project was kept".to_owned(),
            },
            rolled_back(),
        ),
        Some(StagingKind::Create) => removed_work(Some("create"), "creating a project"),
        Some(StagingKind::Copy) => removed_work(Some("copy"), "copying a project"),
        None => removed_work(None, "work in the projects folder"),
    }
}

// ---------------------------------------------------------------------------------------
// The mappings, pure
// ---------------------------------------------------------------------------------------

/// Why a project did not open, as the seam says it.
pub(crate) fn open_failure(error: tc_store::OpenError) -> OpenFailure {
    use tc_store::OpenError;
    match error {
        OpenError::NotAProject => OpenFailure::NotAProject,
        OpenError::Unreadable(e) | OpenError::Store(e) => OpenFailure::Unreadable {
            reason: e.to_string(),
        },
        OpenError::LockHeld { holder } => OpenFailure::LockHeld {
            holder: holder.map(|h| (h.host, h.pid)),
        },
        // A major past what a u16 holds is newer all the same, and refused all the same.
        OpenError::NewerMajor { major } => OpenFailure::NewerFormat {
            major: major.parse().unwrap_or(u16::MAX),
        },
        // Unreachable while the format's major is 1 (tc-journal `format.rs`); unit V adds
        // the migration step.
        error @ OpenError::NeedsMigration { .. } => OpenFailure::Unreadable {
            reason: error.to_string(),
        },
    }
}

/// Why a project is read-only, as the wire says it. The host and the failed write's reason
/// stay in tc-store's facts and notices.
pub(crate) fn read_only_reason(reason: &tc_store::ReadOnlyReason) -> ReadOnlyReason {
    use tc_store::{ReadOnlyReason as Store, UnsafeFolder};
    match reason {
        Store::OpenOnAnotherComputer { .. } => ReadOnlyReason::OpenOnAnotherComputer,
        Store::SecondWriter { .. } => ReadOnlyReason::SecondWriter,
        Store::UnsafeFolder(UnsafeFolder::Filesystem { .. }) => ReadOnlyReason::UnsafeDrive,
        Store::UnsafeFolder(UnsafeFolder::CloudSynced { .. }) => ReadOnlyReason::CloudSynced,
        Store::NewerMinor { .. } => ReadOnlyReason::NeedsNewerApp,
        Store::WriteFailed(_) => ReadOnlyReason::FolderNotWritable,
    }
}

pub(crate) fn session_mode(mode: &tc_store::Mode) -> SessionMode {
    match mode {
        tc_store::Mode::Writable => SessionMode::Writable,
        tc_store::Mode::ReadOnly(reason) => SessionMode::ReadOnly(read_only_reason(reason)),
    }
}

pub(crate) fn command_error(error: tc_store::CommandError) -> CommandError {
    use tc_store::CommandError as Store;
    match error {
        Store::Refused(refusal) => CommandError::Refused(refusal),
        Store::NotWritten(reason) => CommandError::NotWritten(reason),
        Store::LookAgain => CommandError::LookAgain,
        Store::PlanStale => CommandError::PlanStale,
        Store::ReadOnly(reason) => CommandError::ReadOnly(read_only_reason(&reason)),
        Store::BadToken(error) => CommandError::BadToken(error),
        Store::Closed => CommandError::Closed,
    }
}

pub(crate) fn open_phase(phase: tc_store::Phase) -> OpenPhase {
    use tc_store::Phase as Store;
    match phase {
        Store::Finishing => OpenPhase::FinishingWork,
        Store::Reading => OpenPhase::ReadingChanges,
        Store::Folding => OpenPhase::Folding,
        Store::Checking => OpenPhase::CheckingFiles,
        Store::Writing => OpenPhase::WritingFiles,
        // An open never reports it: tc-store uses it for work no open does.
        Store::Working => OpenPhase::WritingFiles,
    }
}

/// The pins a new project gets (tC4 `INSTALLED_SUITE`), which an open writes into a project
/// that holds none (tC4 #766): every group or, should one not convert, none.
pub(crate) fn creation_pins() -> Vec<(PinGroup, PinGroupValue)> {
    let pins = tc_resources::suite::project_pins();
    PinGroup::ALL
        .into_iter()
        .map(|group| Ok((group, pins.group_value(group)?)))
        .collect::<Result<_, tc_resources::GroupError>>()
        .unwrap_or_default()
}

/// tc-store's reports of an open, forwarded unchanged into the open's sink: the store's
/// reporter keeps the 300 ms clock (AD8), so tc-app adds no timer of its own.
pub(crate) fn forward_open(sink: ProgressSink) -> tc_store::ProgressSink {
    Arc::new(move |report: tc_store::Progress| {
        sink.forward(
            Phase::Open(open_phase(report.phase)),
            report.done,
            Some(report.total),
        );
    })
}

fn facts(facts: &tc_store::ProjectFacts) -> ProjectFacts {
    ProjectFacts {
        id: facts.id,
        kind: facts.kind,
        device: facts.device,
        mode: session_mode(&facts.mode),
    }
}

/// A store reply as the seam's. A token the store gives that is not of the edit kind would be
/// a store bug: it is left out, and the reply stays a reply, because a "not saved" for a
/// write that is on disk would make the person retry with the old token and conflict with
/// their own save.
fn reply(reply: tc_store::StoreReply) -> StoreReply {
    StoreReply {
        tokens: reply
            .tokens
            .into_iter()
            .filter_map(|(key, token)| Some((key, EditToken::from_journal(token).ok()?)))
            .collect(),
        new_conflicts: reply.new_conflicts,
        stored: match reply.stored {
            tc_store::Stored::Durable => Stored::Durable,
            tc_store::Stored::WrittenNotDurable(reason) => Stored::WrittenNotDurable(reason),
            tc_store::Stored::Unchanged => Stored::Unchanged,
        },
        seq: reply.seq,
        changes: reply.changes,
        skipped: reply.skipped,
    }
}

fn offered(draft: tc_store::OfferedDraft) -> OfferedDraft {
    let run = draft
        .run
        .as_deref()
        .and_then(|run| tc_api::ids::ServerInstance::parse(run).ok());
    OfferedDraft::new(draft.id, run, draft.verified, draft.at, draft.draft)
}

// ---------------------------------------------------------------------------------------
// The handle
// ---------------------------------------------------------------------------------------

/// tc-store's snapshot behind the seam's: its token key stays inside tc-store.
struct StoreSnapshot(tc_store::Snapshot);

impl Published for StoreSnapshot {
    fn fold(&self) -> &FoldState {
        self.0.fold()
    }

    fn seq(&self) -> u64 {
        self.0.seq()
    }

    fn token(
        &self,
        key: &RecordKey,
        displayed: Option<ChangeId>,
    ) -> Result<ShownToken, TokenError> {
        self.0.token(key, displayed)
    }

    fn review_token(&self, key: &RecordKey) -> Result<ShownToken, TokenError> {
        self.0.review_token(key)
    }
}

type StorePending = tc_store::Pending<Result<tc_store::StoreReply, tc_store::CommandError>>;
type Answer = Result<StoreReply, CommandError>;

/// A submitted command whose reply the relay waits for.
struct Job {
    pending: StorePending,
    origin: Origin,
    reply: oneshot::Sender<Answer>,
}

/// Which request wrote which change, from the replies, for the events thread.
#[derive(Default)]
struct Origins {
    state: Mutex<OriginState>,
    changed: Condvar,
}

#[derive(Default)]
struct OriginState {
    /// By a batch's first change.
    by_change: HashMap<ChangeId, Origin>,
    /// Submits handed to the actor whose reply is not settled yet.
    in_flight: usize,
    /// The highest `seq` of a stored or unchanged reply the relay settled; an error reply has
    /// none. Only the relay raises it, because only the relay settles replies in the order the
    /// actor gave them.
    relay_seq: u64,
    /// The relay has stopped: every reply it will ever see is recorded.
    relay_gone: bool,
}

/// Who settled a reply: the relay, which takes the replies in the order the actor gives
/// them, or the submit itself, whose reply was there at once (a draft) or after the relay
/// was gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettledBy {
    Relay,
    Submit,
}

impl Origins {
    /// A submit is about to reach the actor. Counted before it does, so its batch's event
    /// always finds it either recorded or still counted.
    fn begin(&self) {
        lock(&self.state).in_flight += 1;
    }

    /// A submit's reply is settled: a stored batch's origin is recorded and the submit is no
    /// longer in flight, both under one lock.
    fn settled(&self, reply: Option<&tc_store::StoreReply>, origin: &Origin, by: SettledBy) {
        let mut state = lock(&self.state);
        if let Some(reply) = reply {
            if let Some(first) = reply.changes.first() {
                state.by_change.insert(*first, origin.clone());
            }
            if by == SettledBy::Relay {
                state.relay_seq = state.relay_seq.max(reply.seq);
            }
        }
        state.in_flight = state.in_flight.saturating_sub(1);
        drop(state);
        self.changed.notify_all();
    }

    fn relay_gone(&self) {
        lock(&self.state).relay_gone = true;
        self.changed.notify_all();
    }

    /// Who asked for the batch whose first change is `change` and whose snapshot is `seq`;
    /// `None` when no submit of the seam's did. The store sends a command's `Committed` just
    /// before it resolves that command's reply, so the record may come a moment after the
    /// event; the wait for it ends as soon as the batch cannot be a submit's:
    ///
    /// - no submit is in flight (each is counted before it reaches the actor and settled
    ///   under the same lock as its record), or
    /// - the relay settled a stored or unchanged reply with this `seq` or a later one: the
    ///   actor gave that reply after this batch, and the relay settles in the actor's order,
    ///   so a submit that wrote this batch would have been recorded first.
    ///
    /// So a batch the store folds on its own (S2's outside edits, E1a's intakes) holds the
    /// project's events back until the next stored or unchanged reply the actor gives after
    /// it, or until no submit is in flight. An error reply carries no `seq`, so it says nothing
    /// of where the batch stands in the actor's order and does not end the wait.
    fn take(&self, change: &ChangeId, seq: u64) -> Option<Origin> {
        let mut state = lock(&self.state);
        loop {
            if let Some(origin) = state.by_change.remove(change) {
                return Some(origin);
            }
            if state.in_flight == 0 || state.relay_seq >= seq || state.relay_gone {
                return None;
            }
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// The seam's subscribers of one project.
#[derive(Default)]
struct Subscribers {
    senders: Vec<UnboundedSender<StoreEvent>>,
    /// The store's events have ended: the project is closed.
    ended: bool,
}

/// What a closed handle still answers: a stray read after close gets the last state, never a
/// panic.
struct Closed {
    snapshot: Snapshot,
    facts: ProjectFacts,
    counters: StoreCounters,
}

/// One open project through tc-store.
struct StoreHandle {
    /// `None` once `close` took it.
    inner: RwLock<Option<tc_store::ProjectHandle>>,
    closed: OnceLock<Closed>,
    /// Hands each submitted command's reply to the relay, in the order the actor takes the
    /// commands; `None` once closed.
    relay: Mutex<Option<mpsc::Sender<Job>>>,
    origins: Arc<Origins>,
    subscribers: Arc<Mutex<Subscribers>>,
}

impl StoreHandle {
    /// Serves an opened project: subscribes to its events before anything can be submitted,
    /// then starts the relay and the events thread. A thread that cannot start closes the
    /// project again.
    fn start(handle: tc_store::ProjectHandle) -> Result<Arc<Self>, String> {
        let events = handle.subscribe();
        let origins = Arc::new(Origins::default());
        let subscribers = Arc::new(Mutex::new(Subscribers::default()));
        let (jobs, rx) = mpsc::channel::<Job>();
        let relay = {
            let origins = origins.clone();
            spawn("tc-app store relay", move || relay_loop(&rx, &origins))
        };
        let forwarder = relay.as_ref().and_then(|_| {
            let origins = origins.clone();
            let subscribers = subscribers.clone();
            spawn("tc-app store events", move || {
                events_loop(&events, &origins, &subscribers);
            })
        });
        if relay.is_none() || forwarder.is_none() {
            // This runs on the open's blocking thread, so closing here blocks no worker.
            drop(jobs);
            let _ = handle.close();
            return Err("a thread could not be started".to_owned());
        }
        Ok(Arc::new(Self {
            inner: RwLock::new(Some(handle)),
            closed: OnceLock::new(),
            relay: Mutex::new(Some(jobs)),
            origins,
            subscribers,
        }))
    }

    fn inner(&self) -> std::sync::RwLockReadGuard<'_, Option<tc_store::ProjectHandle>> {
        self.inner.read().unwrap_or_else(PoisonError::into_inner)
    }

    /// Hands tc-journal's command to the actor and its reply to the relay. Every submit of the
    /// seam comes here, so every reply's origin is recorded.
    fn submit_journal(&self, command: Command, origin: Origin) -> BoxFuture<Answer> {
        let inner = self.inner();
        let Some(handle) = inner.as_ref() else {
            return Box::pin(async { Err(CommandError::Closed) });
        };
        // Under the relay's lock, so the relay waits for the replies in the order the actor
        // takes the commands, and a reply settled here is recorded before a later command
        // can reach the actor.
        let relay = lock(&self.relay);
        self.origins.begin();
        let pending = handle.submit(command);
        if pending.is_ready() {
            // A draft, which the store answers at once beside the actor (R22), must never
            // wait in the relay behind a long command; nor need any reply already there.
            let answer = settle(pending.wait(), &origin, &self.origins, SettledBy::Submit);
            drop(relay);
            return Box::pin(async move { answer });
        }
        let (tx, rx) = oneshot::channel();
        let job = Job {
            pending,
            origin,
            reply: tx,
        };
        let sent = match relay.as_ref() {
            Some(relay) => relay.send(job).map_err(|e| e.0),
            None => Err(job),
        };
        drop(relay);
        if let Err(job) = sent {
            // The relay is gone while the store is open, which nothing but a panic in it could
            // do: the reply is awaited here instead (tc-store's reply is a future too), so it
            // is never lost and no thread blocks on it.
            let origins = self.origins.clone();
            return Box::pin(async move {
                let result = job.pending.await;
                settle(result, &job.origin, &origins, SettledBy::Submit)
            });
        }
        Box::pin(async move { rx.await.unwrap_or(Err(CommandError::Closed)) })
    }
}

/// The seam's answer to one store reply; a stored batch's origin is recorded for its event.
fn settle(
    result: Result<tc_store::StoreReply, tc_store::CommandError>,
    origin: &Origin,
    origins: &Origins,
    by: SettledBy,
) -> Answer {
    origins.settled(result.as_ref().ok(), origin, by);
    match result {
        Ok(stored) => Ok(reply(stored)),
        Err(error) => Err(command_error(error)),
    }
}

/// Tells the events thread that the relay has stopped, however it stops: a relay that
/// panicked must not leave the events thread waiting for a record that never comes.
struct RelayGone<'a>(&'a Origins);

impl Drop for RelayGone<'_> {
    fn drop(&mut self) {
        self.0.relay_gone();
    }
}

/// The relay: every reply, in the actor's order, whether or not anyone still awaits it (an
/// HTTP caller that went away). It needs no runtime: a oneshot sends from any thread.
fn relay_loop(jobs: &mpsc::Receiver<Job>, origins: &Origins) {
    let _gone = RelayGone(origins);
    for job in jobs {
        let answer = settle(job.pending.wait(), &job.origin, origins, SettledBy::Relay);
        let _ = job.reply.send(answer);
    }
}

/// The events thread: the store's events as the seam's, each command's batch with its
/// origin, until the store's sender is gone (its actor stopped and its handle dropped).
fn events_loop(
    events: &mpsc::Receiver<tc_store::StoreEvent>,
    origins: &Origins,
    subscribers: &Mutex<Subscribers>,
) {
    while let Ok(event) = events.recv() {
        let event = match event {
            tc_store::StoreEvent::Committed {
                seq,
                delta,
                changes,
            } => {
                let cause = match changes.first().and_then(|first| origins.take(first, seq)) {
                    Some(origin) => StoreCause::Command { origin },
                    // No submit of the seam's asked for the batch. It is a second writer's
                    // fold, which has no change list and also sends a notice and a mode (the
                    // pump names each record's writer from the snapshot), or a batch the
                    // store folded on its own (S2's outside edits; E1a maps its intakes to
                    // `Intake` once tc-store names their devices).
                    None => StoreCause::System,
                };
                StoreEvent::Committed { seq, delta, cause }
            }
            tc_store::StoreEvent::Notice(notice) => StoreEvent::Notice(notice),
            tc_store::StoreEvent::Mode(mode) => StoreEvent::Mode(session_mode(&mode)),
            tc_store::StoreEvent::Materialized { paths } => StoreEvent::Materialized { paths },
            // D1 shows the open's progress and the commit; nothing listens yet.
            tc_store::StoreEvent::Progress { .. } | tc_store::StoreEvent::Checkpointed { .. } => {
                continue;
            }
        };
        lock(subscribers)
            .senders
            .retain(|tx| tx.send(event.clone()).is_ok());
    }
    let mut subscribers = lock(subscribers);
    subscribers.ended = true;
    // Every pump's channel ends with the project ("The channel ends when the handle closes").
    subscribers.senders.clear();
}

/// tc-store's close and drop wait for the open's background work, a checkpoint and the
/// actor's join (up to seconds on a whole Bible): never on a tokio worker.
fn close_off_thread(handle: tc_store::ProjectHandle) -> oneshot::Receiver<Result<(), StoreError>> {
    let (tx, rx) = oneshot::channel();
    let slot = Arc::new(Mutex::new(Some((handle, tx))));
    let run = {
        let slot = slot.clone();
        move || {
            if let Some((handle, tx)) = lock(&slot).take() {
                let _ = tx.send(handle.close().map_err(store_error));
            }
        }
    };
    if spawn("tc-app store close", run).is_none() {
        // No thread to be had: close here, which blocks the caller, rather than leave the
        // project's lock held.
        if let Some((handle, tx)) = lock(&slot).take() {
            let _ = tx.send(handle.close().map_err(store_error));
        }
    }
    rx
}

/// A store call of a handle that may have closed.
fn store_result<P>(pending: Option<P>) -> Result<P, StoreError> {
    pending.ok_or_else(|| StoreError::Other("the project is closed".to_owned()))
}

fn other(error: tc_store::StoreError) -> StoreError {
    match error {
        tc_store::StoreError::NotKept { .. } => StoreError::NotKept,
        error => StoreError::Other(error.to_string()),
    }
}

/// The store's waiting books and kept copies in the screens' terms (D1).
pub fn outside_edits(
    waiting: Vec<tc_store::WaitingBook>,
    kept: Vec<tc_store::Kept>,
    notices: &[StoreNotice],
) -> tc_api::session::OutsideEdits {
    use tc_api::session::{KeptCopy, OutsideEdits, WaitingBook};
    use tc_journal::fold::DerivedFile;
    OutsideEdits {
        waiting: waiting
            .into_iter()
            .map(|w| WaitingBook {
                book: match w.file {
                    DerivedFile::Usfm(book) => Some(book),
                    _ => None,
                },
                file: w.path,
                added: w.added,
                copy: w.kept.copy,
                unreadable: w.kept.unreadable,
                why: kept_why(w.kept.why),
            })
            .collect(),
        kept: kept
            .into_iter()
            .map(|k| KeptCopy {
                file: k.path,
                copy: k.copy,
                why: kept_why(k.why),
            })
            .collect(),
        read_only_changed: notices
            .iter()
            .filter_map(|n| match &n.notice {
                tc_journal::fold::Notice::ChangedWhileReadOnly { path } => {
                    Some(path.as_str().to_owned())
                }
                _ => None,
            })
            .collect(),
    }
}

fn kept_why(why: tc_store::KeptWhy) -> tc_api::session::KeptWhy {
    use tc_api::session::KeptWhy;
    match why {
        tc_store::KeptWhy::Changed => KeptWhy::Changed,
        tc_store::KeptWhy::Lost => KeptWhy::Lost,
    }
}

/// What a checkpoint came to, for the screens (D1): a commit is `Committed`; no changes, a
/// read-only project and a second writer are `Unchanged` (the last two have their own banner or
/// notice); every other skip and every store error is `Failed`, which stays on Home until a
/// later checkpoint of the project ends otherwise. One arm per skip, so a new one is decided.
pub fn checkpoint_outcome(
    report: Result<tc_store::CheckpointReport, tc_store::StoreError>,
) -> CheckpointOutcome {
    use tc_store::CheckpointSkip;
    // A skip is a problem in the app, so the banner offers Ask for help with its code; an I/O
    // error (a full disk) is the computer's, and tC4 offers none for it either.
    let failed = |code: &str, reason: &str| CheckpointOutcome::Failed {
        reason: reason.to_owned(),
        code: Some(format!("checkpoint.{code}")),
    };
    match report {
        Ok(report) if report.commit.is_some() => CheckpointOutcome::Committed,
        Ok(report) => match report.skipped {
            None
            | Some(
                CheckpointSkip::NoChanges | CheckpointSkip::ReadOnly | CheckpointSkip::SecondWriter,
            ) => CheckpointOutcome::Unchanged,
            Some(CheckpointSkip::NoHistory) => {
                failed("no-history", "the project has no history to commit to")
            }
            Some(CheckpointSkip::SealFailed) => {
                failed("seal-failed", "the saved changes could not be sealed")
            }
            Some(CheckpointSkip::PackMissing) => {
                failed("pack-missing", "a saved pack is missing or unreadable")
            }
            Some(CheckpointSkip::TreeInvalid) => failed(
                "tree-invalid",
                "the project's files are not a valid Scripture Burrito",
            ),
            Some(CheckpointSkip::HeadMoved) => failed(
                "head-moved",
                "the history moved while the checkpoint was made",
            ),
        },
        Err(error) => CheckpointOutcome::Failed {
            reason: error.to_string(),
            code: None,
        },
    }
}

impl ProjectHandle for StoreHandle {
    fn facts(&self) -> ProjectFacts {
        match self.inner().as_ref() {
            Some(handle) => facts(&handle.facts()),
            None => self
                .closed
                .get()
                .map(|c| c.facts.clone())
                .unwrap_or(ProjectFacts {
                    id: tc_journal::ProjectId::from_bytes([0; 16]),
                    kind: ProjectKind::Bible,
                    device: tc_journal::DeviceId::local([0; 15]),
                    mode: SessionMode::Writable,
                }),
        }
    }

    fn snapshot(&self) -> Snapshot {
        if let Some(handle) = self.inner().as_ref() {
            return Snapshot::new(StoreSnapshot(handle.snapshot()));
        }
        // `close` records what the store showed under the write lock in which it takes the
        // store, so a read that finds no store finds the record (and `Drop` has the handle to
        // itself): the wait returns at once.
        self.closed.wait().snapshot.clone()
    }

    fn parse_edit(&self, token: &tc_api::ShownToken) -> Result<EditToken, TokenError> {
        match self.inner().as_ref() {
            Some(handle) => EditToken::from_journal(handle.parse_edit(token.body())?),
            None => Err(TokenError::Malformed("the project is closed".to_owned())),
        }
    }

    fn parse_review(&self, token: &tc_api::ReviewToken) -> Result<ReviewToken, TokenError> {
        match self.inner().as_ref() {
            Some(handle) => ReviewToken::from_journal(handle.parse_review(token.body())?),
            None => Err(TokenError::Malformed("the project is closed".to_owned())),
        }
    }

    fn submit(&self, command: StoreCommand) -> BoxFuture<Answer> {
        let (command, origin) = command.into_parts();
        self.submit_journal(command, origin)
    }

    fn subscribe(&self) -> UnboundedReceiver<StoreEvent> {
        let (tx, rx) = unbounded_channel();
        let mut subscribers = lock(&self.subscribers);
        if !subscribers.ended {
            subscribers.senders.push(tx);
        }
        rx
    }

    fn checkpoint(&self, reason: CheckpointReason) -> BoxFuture<CheckpointOutcome> {
        let Some(pending) = self.inner().as_ref().map(|handle| {
            handle.checkpoint(match reason {
                CheckpointReason::LeavingProject => tc_store::CheckpointReason::LeavingProject,
                CheckpointReason::LeavingTranslate => tc_store::CheckpointReason::LeavingTranslate,
                CheckpointReason::LeavingMode(mode) => {
                    tc_store::CheckpointReason::Other(format!("leaving {mode}"))
                }
                CheckpointReason::BeforeExport => tc_store::CheckpointReason::BeforeExport,
                CheckpointReason::Other(why) => tc_store::CheckpointReason::Other(why),
            })
        }) else {
            return Box::pin(async {
                CheckpointOutcome::Failed {
                    reason: "the project is closed".to_owned(),
                    code: None,
                }
            });
        };
        Box::pin(async move { checkpoint_outcome(pending.await) })
    }

    fn carried_bytes(
        &self,
        oid: tc_journal::GitOid,
    ) -> BoxFuture<Result<Option<Vec<u8>>, StoreError>> {
        let Some(pending) = self.inner().as_ref().map(|handle| handle.read_carried(oid)) else {
            return Box::pin(async { Err(StoreError::Other("the project is closed".to_owned())) });
        };
        Box::pin(async move { pending.await.map_err(store_error) })
    }

    fn counters(&self) -> StoreCounters {
        match self.inner().as_ref() {
            Some(handle) => handle.counters(),
            None => self.closed.get().map(|c| c.counters).unwrap_or_default(),
        }
    }

    fn notices(&self) -> Vec<StoreNotice> {
        self.inner()
            .as_ref()
            .map(|handle| handle.notices())
            .unwrap_or_default()
    }

    fn outside_edits(&self) -> tc_api::session::OutsideEdits {
        self.inner()
            .as_ref()
            .map(|handle| {
                outside_edits(
                    handle.waiting_books(),
                    handle.kept_copies(),
                    &handle.notices(),
                )
            })
            .unwrap_or_default()
    }

    fn focus(&self) -> BoxFuture<Result<(), StoreError>> {
        let pending = self.inner().as_ref().map(tc_store::ProjectHandle::focus);
        Box::pin(async move { store_result(pending)?.await.map(drop).map_err(other) })
    }

    fn import_kept(&self, copy: &str) -> BoxFuture<Result<tc_store::ImportReport, StoreError>> {
        let pending = self.inner().as_ref().map(|handle| handle.import_kept(copy));
        Box::pin(async move { store_result(pending)?.await.map_err(other) })
    }

    fn not_now(&self, file: &str) -> BoxFuture<Result<(), StoreError>> {
        let pending = self.inner().as_ref().map(|handle| handle.not_now(file));
        Box::pin(async move { store_result(pending)?.await.map_err(other) })
    }

    fn dismiss_kept(&self, copy: &str) -> BoxFuture<Result<(), StoreError>> {
        let pending = self
            .inner()
            .as_ref()
            .map(|handle| handle.dismiss_kept(copy));
        Box::pin(async move { store_result(pending)?.await.map_err(other) })
    }

    fn offered_drafts(&self) -> Vec<OfferedDraft> {
        self.inner()
            .as_ref()
            .map(|handle| handle.offered_drafts().into_iter().map(offered).collect())
            .unwrap_or_default()
    }

    fn discard_draft(&self, id: u64) -> bool {
        self.inner()
            .as_ref()
            .is_some_and(|handle| handle.discard_draft(id))
    }

    fn discard_current_draft(
        &self,
        field: DraftField,
        shown: Vec<EditToken>,
        text: String,
        at: tc_journal::WallTime,
    ) -> bool {
        let draft = Draft {
            field,
            shown: shown.into_iter().map(|token| token.0).collect(),
            text,
        };
        self.inner()
            .as_ref()
            .is_some_and(|handle| handle.discard_current_draft(draft, at))
    }

    fn offer_draft(
        &self,
        field: DraftField,
        shown: Vec<DraftToken>,
        text: String,
        verified: bool,
        at: tc_journal::WallTime,
    ) -> Option<u64> {
        let draft = Draft {
            field,
            shown: shown.into_iter().map(|token| token.0).collect(),
            text,
        };
        self.inner()
            .as_ref()
            .and_then(|handle| handle.offer_draft(draft, verified, at))
    }

    /// Holds the handle's read lock while it waits, so a close meanwhile waits too: a test's
    /// hook, never called by the product.
    #[cfg(feature = "test-hooks")]
    fn wait_idle(&self) {
        if let Some(handle) = self.inner().as_ref() {
            handle.wait_idle();
        }
    }

    fn close(self: Arc<Self>) -> BoxFuture<Result<(), StoreError>> {
        let taken = {
            let mut inner = self.inner.write().unwrap_or_else(PoisonError::into_inner);
            let taken = inner.take();
            if let Some(handle) = &taken {
                let _ = self.closed.set(Closed {
                    snapshot: Snapshot::new(StoreSnapshot(handle.snapshot())),
                    facts: facts(&handle.facts()),
                    counters: handle.counters(),
                });
            }
            taken
        };
        let Some(handle) = taken else {
            // A second close.
            return Box::pin(async { Ok(()) });
        };
        // The relay waits for the replies already queued: the actor answers every command
        // queued before its close.
        lock(&self.relay).take();
        let closed = close_off_thread(handle);
        Box::pin(async move {
            closed
                .await
                .unwrap_or_else(|_| Err(store_error("the close stopped without an outcome")))
        })
    }
}

impl Drop for StoreHandle {
    fn drop(&mut self) {
        let taken = self
            .inner
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(handle) = taken {
            // tc-store's own drop stops the actor and waits for it; what was saved is in
            // pending files, and the next open seals it. Off this thread, which may be a
            // tokio worker.
            let rx = close_off_thread(handle);
            drop(rx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tc_store::fault::FaultFs;

    #[test]
    fn every_open_error_maps_to_its_failure() {
        use tc_store::{LockHolder, OpenError};
        assert_eq!(
            open_failure(OpenError::NotAProject),
            OpenFailure::NotAProject
        );
        assert_eq!(
            open_failure(OpenError::Unreadable(tc_store::StoreError::Other(
                "a bad disk".into()
            ))),
            OpenFailure::Unreadable {
                reason: "a bad disk".into()
            }
        );
        assert_eq!(
            open_failure(OpenError::Store(tc_store::StoreError::Other(
                "no thread".into()
            ))),
            OpenFailure::Unreadable {
                reason: "no thread".into()
            }
        );
        assert_eq!(
            open_failure(OpenError::LockHeld {
                holder: Some(LockHolder {
                    host: "laptop".into(),
                    pid: 7
                })
            }),
            OpenFailure::LockHeld {
                holder: Some(("laptop".into(), 7))
            }
        );
        assert_eq!(
            open_failure(OpenError::LockHeld { holder: None }),
            OpenFailure::LockHeld { holder: None }
        );
        assert_eq!(
            open_failure(OpenError::NewerMajor { major: "2".into() }),
            OpenFailure::NewerFormat { major: 2 }
        );
        assert_eq!(
            open_failure(OpenError::NewerMajor {
                major: "70000".into()
            }),
            OpenFailure::NewerFormat { major: u16::MAX }
        );
        assert!(matches!(
            open_failure(OpenError::NeedsMigration { major: "0".into() }),
            OpenFailure::Unreadable { reason } if reason.contains("migration")
        ));
    }

    #[test]
    fn every_read_only_reason_maps_to_its_wire_reason() {
        use tc_store::{ReadOnlyReason as Store, UnsafeFolder};
        let cases = [
            (
                Store::OpenOnAnotherComputer {
                    host: "laptop".into(),
                },
                ReadOnlyReason::OpenOnAnotherComputer,
            ),
            (
                Store::SecondWriter {
                    device: tc_journal::DeviceId::local([3; 15]),
                },
                ReadOnlyReason::SecondWriter,
            ),
            (
                Store::UnsafeFolder(UnsafeFolder::Filesystem {
                    name: "exfat".into(),
                }),
                ReadOnlyReason::UnsafeDrive,
            ),
            (
                Store::UnsafeFolder(UnsafeFolder::CloudSynced {
                    client: "OneDrive".into(),
                }),
                ReadOnlyReason::CloudSynced,
            ),
            (
                Store::NewerMinor {
                    min_reader_minor: "3".into(),
                },
                ReadOnlyReason::NeedsNewerApp,
            ),
            (
                Store::WriteFailed("the disk is full".into()),
                ReadOnlyReason::FolderNotWritable,
            ),
        ];
        for (store, wire) in cases {
            assert_eq!(read_only_reason(&store), wire, "{store:?}");
            assert_eq!(
                session_mode(&tc_store::Mode::ReadOnly(store)),
                SessionMode::ReadOnly(wire)
            );
        }
        assert_eq!(
            session_mode(&tc_store::Mode::Writable),
            SessionMode::Writable
        );
    }

    #[test]
    fn every_store_command_error_maps() {
        use tc_store::CommandError as Store;
        assert_eq!(
            command_error(Store::Refused(tc_store::Refusal::NotText)),
            CommandError::Refused(tc_store::Refusal::NotText)
        );
        assert_eq!(
            command_error(Store::NotWritten("full".into())),
            CommandError::NotWritten("full".into())
        );
        assert_eq!(command_error(Store::LookAgain), CommandError::LookAgain);
        assert_eq!(command_error(Store::PlanStale), CommandError::PlanStale);
        assert_eq!(
            command_error(Store::ReadOnly(tc_store::ReadOnlyReason::WriteFailed(
                "gone".into()
            ))),
            CommandError::ReadOnly(ReadOnlyReason::FolderNotWritable)
        );
        assert_eq!(
            command_error(Store::BadToken(TokenError::BadMac)),
            CommandError::BadToken(TokenError::BadMac)
        );
        assert_eq!(command_error(Store::Closed), CommandError::Closed);
    }

    #[test]
    fn every_open_phase_maps() {
        use tc_store::Phase as Store;
        let cases = [
            (Store::Finishing, OpenPhase::FinishingWork),
            (Store::Reading, OpenPhase::ReadingChanges),
            (Store::Folding, OpenPhase::Folding),
            (Store::Checking, OpenPhase::CheckingFiles),
            (Store::Writing, OpenPhase::WritingFiles),
            (Store::Working, OpenPhase::WritingFiles),
        ];
        for (store, wire) in cases {
            assert_eq!(open_phase(store), wire);
        }
    }

    #[test]
    fn project_json_requires_the_four_fields_tc_store_requires() {
        let id = "6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50";
        let read = |text: &str| project_json(text.as_bytes());
        let bible = read(&format!(
            r#"{{"schemaVersion":1,"projectId":"{id}","kind":"bible","created":"x"}}"#
        ))
        .unwrap();
        assert_eq!(bible.1, ProjectKind::Bible);
        assert_eq!(bible.0, tc_journal::ProjectId::parse(id).unwrap());
        let obs = read(&format!(
            r#"{{"schemaVersion":1,"projectId":"{id}","kind":"obs","created":"x","more":0}}"#
        ))
        .unwrap();
        assert_eq!(obs.1, ProjectKind::Stories);
        for refused in [
            "{}".to_owned(),
            "not json".to_owned(),
            format!(r#"{{"projectId":"{id}","kind":"bible"}}"#),
            format!(r#"{{"schemaVersion":1,"projectId":"{id}","kind":"bible"}}"#),
            format!(r#"{{"projectId":"{id}","kind":"bible","created":"x"}}"#),
            format!(r#"{{"schemaVersion":"1","projectId":"{id}","kind":"bible","created":"x"}}"#),
            format!(r#"{{"schemaVersion":1,"projectId":"{id}","kind":"bible","created":5}}"#),
            format!(r#"{{"schemaVersion":1,"projectId":"{id}","kind":"psalter","created":"x"}}"#),
            r#"{"schemaVersion":1,"projectId":"not an id","kind":"bible","created":"x"}"#
                .to_owned(),
        ] {
            assert!(read(&refused).is_err(), "{refused}");
        }
    }

    /// A removed import and join are reported under tC4's operations (`import`,
    /// `open-door43`), interrupted and rolled back; a create, a copy and a folder whose name
    /// says no work, which tC4 has no staging folder for, are `staging-removed` with the kind.
    #[test]
    fn each_removed_staging_folder_is_reported_under_its_operation() {
        use tc_store::{RemovedStaging, StagingKind};
        let report = |folder: &str, kind| {
            staging_report(&RemovedStaging {
                folder: folder.to_owned(),
                kind,
            })
        };
        let rolled_back = |folder: &str| serde_json::json!({ "folder": folder, "interrupted": true, "rolledBack": true });
        assert_eq!(
            report(".staging-import-01", Some(StagingKind::Import)),
            (
                ReportOp::Import,
                ReportOutcome::Refused {
                    code: Some("import.write-failed".to_owned()),
                    key: "refusal.import.write-failed".to_owned(),
                },
                rolled_back(".staging-import-01"),
            )
        );
        assert_eq!(
            report(".staging-join-02", Some(StagingKind::Join)),
            (
                ReportOp::OpenDoor43,
                ReportOutcome::Failed {
                    reason: "the open from Door43 was interrupted; no project was kept".to_owned(),
                },
                rolled_back(".staging-join-02"),
            )
        );
        for (folder, kind, name, work) in [
            (
                ".staging-create-03",
                Some(StagingKind::Create),
                Some("create"),
                "creating a project",
            ),
            (
                ".staging-copy-04",
                Some(StagingKind::Copy),
                Some("copy"),
                "copying a project",
            ),
            (".staging-05", None, None, "work in the projects folder"),
        ] {
            assert_eq!(
                report(folder, kind),
                (
                    ReportOp::StagingRemoved,
                    ReportOutcome::Failed {
                        reason: format!("{work} did not finish, so its part copy was removed"),
                    },
                    serde_json::json!({ "folder": folder, "kind": name }),
                ),
                "{folder}"
            );
        }
    }

    // ---- through the store -------------------------------------------------------------

    use std::time::Duration;
    use tc_journal::fold::ProjectKind as Kind;
    use tc_journal::record::VersificationValue;
    use tc_journal::{MetaField, Record};

    fn json(text: &str) -> tc_journal::Json {
        tc_journal::json::parse(text.as_bytes()).unwrap()
    }

    /// A store on `fs` with one Bible project, `titus`, holding Titus 1:1.
    fn backend_with_titus(fs: &FaultFs, hooks: Arc<tc_store::Hooks>) -> StoreBackend {
        let backend = StoreBackend::start(
            tc_store::StoreConfig {
                app_data: "/app".into(),
                projects: "/projects".into(),
                vfs: Arc::new(fs.clone()),
                platform: Arc::new(tc_store::FakePlatform::default()),
                clock: Arc::new(tc_store::ManualClock::at(1_792_195_200_000)),
                threads: 2,
                watch_debounce: std::time::Duration::from_millis(20),
            },
            hooks,
        )
        .unwrap();
        backend.store.wait_cleanup();
        let usfm = "\\id TIT\n\\c 1\n\\p\n\\v 1 Paul, a servant of God\n\\v 2 in hope\n";
        let parts = tc_usfm::codec::decompose(usfm).unwrap();
        let mut records = vec![
            Record::ProjectMeta {
                field: MetaField::Name,
                value: Some(json(r#"{"en":"Titus"}"#)),
            },
            Record::Versification {
                value: VersificationValue {
                    name: Some("eng".into()),
                    bytes: tc_versification::StandardScheme::Eng.json().to_owned(),
                },
            },
            Record::BookPresence {
                book: tc_journal::BookId::Titus,
                value: tc_journal::record::BookPresence::Present {
                    scope: tc_journal::record::Scope::Whole,
                    how: tc_journal::record::PresentHow::Added,
                },
            },
        ];
        records.extend(tc_journal::fold::records_from_usfm(
            tc_journal::BookId::Titus,
            &parts,
        ));
        backend
            .store
            .create(&tc_store::NewProject {
                name: "titus".into(),
                kind: Kind::Bible,
                records,
                books: Vec::new(),
            })
            .unwrap();
        backend
    }

    fn verse(n: u16) -> RecordKey {
        RecordKey::Verse(
            tc_journal::BookId::Titus,
            tc_journal::ChapterNo::new(1).unwrap(),
            tc_journal::VerseNo::new(n).unwrap(),
        )
    }

    fn progress() -> ProgressSink {
        ProgressSink::new(Phase::Open(OpenPhase::ReadingChanges))
    }

    fn rid(n: u8) -> tc_api::ids::RequestId {
        tc_api::ids::RequestId::from_bytes([n; 16])
    }

    fn current_thread() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
    }

    /// The opened `titus` behind the seam's handle.
    fn open_titus(backend: &StoreBackend) -> Arc<StoreHandle> {
        let handle = StoreHandle::start(
            backend
                .store
                .open(
                    &backend.projects.join("titus"),
                    &tc_store::OpenOptions::default(),
                )
                .unwrap(),
        )
        .unwrap();
        handle.wait_idle();
        handle
    }

    /// A save of Titus 1:`v` through tc-store's own handle, as S2's outside edits and E1a's
    /// intakes will fold a batch: no submit of the seam's asks for it.
    fn save_outside_the_seam(handle: &StoreHandle, v: u16, words: &str) -> StorePending {
        let inner = handle.inner();
        let inner = inner.as_ref().unwrap();
        inner.submit(Command::SaveVerse {
            shown: inner.snapshot().token(&verse(v), None).unwrap(),
            words: words.to_owned(),
        })
    }

    /// The `(seq, cause)` of the next `n` batches a subscriber gets, each within 5 s.
    async fn next_batches(
        events: &mut UnboundedReceiver<StoreEvent>,
        n: usize,
    ) -> Vec<(u64, StoreCause)> {
        let mut out = Vec::new();
        while out.len() < n {
            let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
                .await
                .expect("the event comes within 5 s")
                .expect("the channel is open");
            if let StoreEvent::Committed { seq, cause, .. } = event {
                out.push((seq, cause));
            }
        }
        out
    }

    /// A batch no submit of the seam's asked for (S2's outside edits, E1a's intakes) is
    /// passed on as the store's own at once, and never holds back the events after it: the
    /// person's next save is still announced as theirs.
    #[test]
    fn a_batch_no_submit_asked_for_never_holds_the_events_back() {
        let fs = FaultFs::new();
        let backend = backend_with_titus(&fs, Arc::default());
        let handle = open_titus(&backend);
        let mut events = handle.subscribe();
        let runtime = current_thread();
        let outside = save_outside_the_seam(&handle, 1, "folded by the store").wait();
        let outside = outside.unwrap();
        let own = runtime
            .block_on(
                handle.submit(
                    StoreCommand::save_verse(
                        handle.snapshot().mint(&verse(2), None).unwrap(),
                        "the person's".to_owned(),
                    )
                    .requested_by(rid(7)),
                ),
            )
            .unwrap();
        assert_eq!(
            runtime.block_on(next_batches(&mut events, 2)),
            vec![
                (outside.seq, StoreCause::System),
                (
                    own.seq,
                    StoreCause::Command {
                        origin: Origin::Request(rid(7))
                    }
                ),
            ]
        );
        runtime.block_on(handle.close()).unwrap();
    }

    /// The same batch while a submit waits behind it in the actor: the events thread waits
    /// for that submit's stored reply, which the actor gives after the batch, and then passes
    /// both on, each with its own cause.
    #[test]
    fn a_batch_no_submit_asked_for_waits_for_the_next_stored_reply() {
        let fs = FaultFs::new();
        let hooks: Arc<tc_store::Hooks> = Arc::default();
        let backend = backend_with_titus(&fs, hooks.clone());
        let handle = open_titus(&backend);
        let mut events = handle.subscribe();
        let runtime = current_thread();
        hooks.hold(tc_store::Point::BeforeDurablePending);
        let outside = save_outside_the_seam(&handle, 1, "folded by the store");
        hooks.wait_reached(tc_store::Point::BeforeDurablePending, 1);
        let own = handle.submit(
            StoreCommand::save_verse(
                handle.snapshot().mint(&verse(2), None).unwrap(),
                "queued behind it".to_owned(),
            )
            .requested_by(rid(8)),
        );
        hooks.release(tc_store::Point::BeforeDurablePending);
        let outside = outside.wait().unwrap();
        let own = runtime.block_on(own).unwrap();
        assert!(outside.seq < own.seq);
        assert_eq!(
            runtime.block_on(next_batches(&mut events, 2)),
            vec![
                (outside.seq, StoreCause::System),
                (
                    own.seq,
                    StoreCause::Command {
                        origin: Origin::Request(rid(8))
                    }
                ),
            ]
        );
        runtime.block_on(handle.close()).unwrap();
    }

    fn written(change: ChangeId, seq: u64) -> tc_store::StoreReply {
        tc_store::StoreReply {
            tokens: Vec::new(),
            new_conflicts: Vec::new(),
            stored: tc_store::Stored::Durable,
            seq,
            changes: vec![change],
            skipped: Vec::new(),
            kept: Vec::new(),
            dropped_links: Vec::new(),
        }
    }

    /// `Origins::take` on a thread of its own, so a take that waits forever fails the test
    /// instead of hanging it.
    fn take_on_a_thread(
        origins: &Arc<Origins>,
        change: ChangeId,
        seq: u64,
    ) -> std::sync::mpsc::Receiver<Option<Origin>> {
        let (tx, rx) = std::sync::mpsc::channel();
        let origins = origins.clone();
        std::thread::spawn(move || {
            let _ = tx.send(origins.take(&change, seq));
        });
        rx
    }

    const AT_ONCE: Duration = Duration::from_secs(5);
    const A_WHILE: Duration = Duration::from_millis(50);

    /// `Origins::take` waits for the record of a batch while a submit may have written it,
    /// and stops waiting the moment the batch cannot be a submit's: nothing in flight, or a
    /// later reply settled by the relay, which takes the replies in the actor's order.
    #[test]
    fn origins_wait_only_while_the_batch_may_be_a_submits() {
        let (mine, outside, later) = (
            ChangeId::of(b"mine"),
            ChangeId::of(b"outside"),
            ChangeId::of(b"later"),
        );
        let request = Origin::Request(rid(9));

        let origins = Arc::new(Origins::default());
        assert_eq!(
            take_on_a_thread(&origins, outside, 1).recv_timeout(AT_ONCE),
            Ok(None),
            "nothing in flight"
        );

        // One submit in flight: the event of its batch waits for its record.
        origins.begin();
        let taken = take_on_a_thread(&origins, mine, 2);
        assert!(
            taken.recv_timeout(A_WHILE).is_err(),
            "waits while the submit is in flight"
        );
        origins.settled(Some(&written(mine, 2)), &request, SettledBy::Relay);
        assert_eq!(taken.recv_timeout(AT_ONCE), Ok(Some(request.clone())));

        // Two more in flight, and the relay settles a reply the actor gave after batch 3: a
        // batch 3 no record names is no submit's, though one submit is still in flight.
        origins.begin();
        origins.begin();
        origins.settled(Some(&written(later, 4)), &request, SettledBy::Relay);
        assert_eq!(
            take_on_a_thread(&origins, outside, 3).recv_timeout(AT_ONCE),
            Ok(None)
        );
        assert_eq!(
            take_on_a_thread(&origins, later, 4).recv_timeout(AT_ONCE),
            Ok(Some(request.clone()))
        );

        // A reply settled by its submit (a draft, answered beside the actor) proves nothing
        // about the actor's order: batch 6 may still be the submit's that is in flight.
        let drafted = Arc::new(Origins::default());
        drafted.begin();
        drafted.begin();
        drafted.settled(Some(&written(later, 7)), &request, SettledBy::Submit);
        let taken = take_on_a_thread(&drafted, mine, 6);
        assert!(
            taken.recv_timeout(A_WHILE).is_err(),
            "a submit's own settle does not end the wait"
        );
        drafted.settled(Some(&written(mine, 6)), &request, SettledBy::Relay);
        assert_eq!(taken.recv_timeout(AT_ONCE), Ok(Some(request.clone())));

        // An error reply (refused, read-only, not written) carries no seq, so it says nothing of
        // the batch's place in the actor's order: the wait goes on while another submit is in
        // flight, and ends with a later stored reply (APP-DESIGN §4.2's note, the C0 row's D7).
        let refused = Arc::new(Origins::default());
        refused.begin();
        refused.begin();
        let taken = take_on_a_thread(&refused, outside, 3);
        refused.settled(None, &request, SettledBy::Relay);
        assert!(
            taken.recv_timeout(A_WHILE).is_err(),
            "an error reply does not end the wait"
        );
        refused.settled(Some(&written(later, 4)), &request, SettledBy::Relay);
        assert_eq!(taken.recv_timeout(AT_ONCE), Ok(None));

        // ... or when the error reply leaves no submit in flight.
        let drained = Arc::new(Origins::default());
        drained.begin();
        let taken = take_on_a_thread(&drained, outside, 3);
        assert!(taken.recv_timeout(A_WHILE).is_err());
        drained.settled(None, &request, SettledBy::Relay);
        assert_eq!(
            taken.recv_timeout(AT_ONCE),
            Ok(None),
            "nothing in flight after the error reply"
        );
    }

    /// AD3, R19: a review token in a save cannot be built through the seam (two
    /// `compile_fail` doctests on `StoreCommand`), so the raw command goes the way every
    /// submit goes after `into_parts`, and the actor refuses it; the wire reports it as a
    /// bad token, never a save.
    #[test]
    fn review_token_in_a_save_is_refused_by_the_actor() {
        let fs = FaultFs::new();
        let backend = backend_with_titus(&fs, Arc::default());
        let handle = StoreHandle::start(
            backend
                .store
                .open(
                    &backend.projects.join("titus"),
                    &tc_store::OpenOptions::default(),
                )
                .unwrap(),
        )
        .unwrap();
        handle.wait_idle();
        let review = handle
            .inner()
            .as_ref()
            .unwrap()
            .snapshot()
            .review_token(&verse(1))
            .unwrap();
        let before = handle.snapshot().seq();
        let pending_before = fs
            .files_under(&backend.projects.join("titus/.tc/pending"))
            .len();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let refused = runtime.block_on(handle.submit_journal(
            Command::SaveVerse {
                shown: review,
                words: "a review token's save".into(),
            },
            Origin::Server,
        ));
        assert_eq!(
            refused.unwrap_err(),
            CommandError::Refused(tc_store::Refusal::WrongTokenKind)
        );
        assert_eq!(handle.snapshot().seq(), before, "nothing was folded");
        assert_eq!(
            fs.files_under(&backend.projects.join("titus/.tc/pending"))
                .len(),
            pending_before,
            "nothing was written"
        );
        assert_eq!(
            crate::wire::error::command_error(CommandError::Refused(
                tc_store::Refusal::WrongTokenKind
            )),
            Ok(tc_api::ApiError::NotSaved(tc_api::NotSaved::BadToken))
        );
        runtime.block_on(handle.close()).unwrap();
    }

    /// Trap 4: tc-store's drop waits for a checkpoint and the actor's join; the seam's handle
    /// hands it to a thread, so dropping the last handle never blocks its thread. The drop
    /// runs on a thread of the test's own, and the hold is released before the verdict, so a
    /// drop that blocks fails the test instead of hanging it.
    #[test]
    fn a_handle_dropped_without_close_never_blocks_its_thread() {
        let fs = FaultFs::new();
        let hooks: Arc<tc_store::Hooks> = Arc::default();
        let backend = backend_with_titus(&fs, hooks.clone());
        let handle = backend
            .open(&ProjectRef::parse("titus").unwrap(), progress())
            .unwrap();
        handle.wait_idle();
        hooks.hold(tc_store::Point::BeforeCheckpoint);
        let checkpoint = handle.checkpoint(CheckpointReason::Other("held".into()));
        hooks.wait_reached(tc_store::Point::BeforeCheckpoint, 1);
        let (tx, rx) = std::sync::mpsc::channel();
        let dropper = std::thread::spawn(move || {
            // As a tokio worker would drop it.
            current_thread().block_on(async move { drop(handle) });
            let _ = tx.send(());
        });
        let returned = rx.recv_timeout(Duration::from_secs(1)).is_ok();
        hooks.release(tc_store::Point::BeforeCheckpoint);
        dropper.join().unwrap();
        assert!(returned, "the drop returned while the checkpoint is held");
        // The open's first checkpoint commits the project's first tree (S2).
        let outcome = current_thread().block_on(checkpoint);
        assert_eq!(outcome, CheckpointOutcome::Committed);
    }
}
