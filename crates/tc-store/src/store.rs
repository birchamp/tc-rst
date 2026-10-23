//! The projects of one installation: opening a project, creating one, and copying a
//! read-only one (`docs/STORAGE.md` §6.1, §6.4 rows N1 and N2,
//! `docs/TC4-DELTA-2026-10-16.md` §3.10).
//!
//! A new project is built in `<projects>/.staging-create-<16 hex>/` through the same actor, seal and
//! materialization as any project, then renamed into place by a rename that never replaces
//! a folder: a project that appeared at the name meanwhile keeps every byte, and a failure
//! removes only the staging folder. At start the store removes staging folders a crash
//! left (N1); every create waits until that cleanup is over, so it can never remove the
//! staging folder of a create that is running.

use crate::appdata::AppData;
use crate::atomic::{
    HeldRetry, SyncDir, atomic_create_dirs, atomic_write, remove_dir_all_held, remove_held,
    rename_dir_noreplace_held,
};
use crate::clock::Clock;
use crate::error::StoreError;
use crate::handle::{Mode, ProjectHandle};
use crate::hooks::{Hooks, Point};
use crate::layout::{
    DEVICE, FORMAT_JSON, Folder, GITATTRIBUTES, GITATTRIBUTES_BYTES, GITIGNORE, GITIGNORE_BYTES,
    LOCK, PACKS, PENDING, PROJECT_JSON,
};
use crate::load::{JournalKind, list};
use crate::open::{Env, OpenError, OpenOptions, ProjectJson, open_project};
use crate::platform::Platform;
use crate::reply::CommandError;
use crate::vfs::{EntryKind, Vfs, is_not_found};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use tc_journal::command::{Command, PlannedChange, SeedSource, UpgradePlan};
use tc_journal::fold::ProjectKind;
use tc_journal::format::FormatDoc;
use tc_journal::record::Scope;
use tc_journal::{BookId, Cause, ProjectId, Record};

/// Where a store lives and what it runs on.
#[derive(Clone)]
pub struct StoreConfig {
    /// The application data folder (§3.7).
    pub app_data: PathBuf,
    /// The folder new projects are created in.
    pub projects: PathBuf,
    pub vfs: Arc<dyn Vfs>,
    pub platform: Arc<dyn Platform>,
    pub clock: Arc<dyn Clock>,
    /// Threads that read journal files at an open.
    pub threads: usize,
    /// How long the file watcher waits for a program to stop writing before it tells the
    /// writer (§11.1): one second in the app.
    pub watch_debounce: std::time::Duration,
}

impl std::fmt::Debug for StoreConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreConfig")
            .field("app_data", &self.app_data)
            .field("projects", &self.projects)
            .finish_non_exhaustive()
    }
}

/// A project to create.
#[derive(Debug, Clone)]
pub struct NewProject {
    /// The folder's name in the projects folder.
    pub name: String,
    pub kind: ProjectKind,
    /// The first records (project details, versification, settings, an OBS project's
    /// stories): one batch with cause `Seed`.
    pub records: Vec<Record>,
    /// Books to add, each seeded from a literal source book (§5.7).
    pub books: Vec<(BookId, Scope, SeedSource)>,
}

/// Why a project was not created.
#[derive(Debug, thiserror::Error)]
pub enum CreateError {
    /// "A project named … already exists." Nothing at the name was touched.
    #[error("a project named {0} already exists")]
    NameExists(String),
    #[error("the project name {0} cannot be a folder name")]
    BadName(String),
    #[error("the project could not be set up: {0}")]
    Refused(CommandError),
    #[error(transparent)]
    Open(#[from] OpenError),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// What a staging folder was being made for (§6.4 N1). The folder's name says it,
/// `.staging-<kind>-<16 hex>`, so the start cleanup can tell the person which operation did
/// not finish: a join's removal is told with `refusal.open-door43.removed`
/// (`docs/TC4-DELTA-2026-10-16.md` §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StagingKind {
    Create,
    Copy,
    Join,
    Import,
}

impl StagingKind {
    const ALL: [Self; 4] = [Self::Create, Self::Copy, Self::Join, Self::Import];

    fn name(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Copy => "copy",
            Self::Join => "join",
            Self::Import => "import",
        }
    }

    /// The kind a staging folder's name gives; `None` for a name of no known kind.
    fn of(folder: &str) -> Option<Self> {
        let rest = folder.strip_prefix(STAGING)?;
        Self::ALL.into_iter().find(|k| {
            rest.strip_prefix(k.name())
                .is_some_and(|r| r.starts_with('-'))
        })
    }
}

/// A staging folder the start cleanup removed (§6.4 N1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedStaging {
    /// The folder's name in the projects folder.
    pub folder: String,
    /// What it was being made for, when its name says.
    pub kind: Option<StagingKind>,
}

/// The start-time cleanup's state (N1).
#[derive(Debug, Default)]
struct Cleanup {
    done: Mutex<bool>,
    finished: Condvar,
    removed: Mutex<Vec<RemovedStaging>>,
}

impl Cleanup {
    fn finish(&self, removed: Vec<RemovedStaging>) {
        *self.removed.lock().unwrap_or_else(|e| e.into_inner()) = removed;
        *self.done.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.finished.notify_all();
    }

    fn wait(&self) {
        let mut done = self.done.lock().unwrap_or_else(|e| e.into_inner());
        while !*done {
            done = self.finished.wait(done).unwrap_or_else(|e| e.into_inner());
        }
    }
}

/// The projects of one installation (see the module documentation).
pub struct Store {
    env: Env,
    projects: PathBuf,
    cleanup: Arc<Cleanup>,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store")
            .field("projects", &self.projects)
            .finish_non_exhaustive()
    }
}

const STAGING: &str = ".staging-";

impl Store {
    /// Opens the application data and starts the cleanup of staging folders a crash left
    /// (N1) on a thread of its own.
    pub fn start(config: StoreConfig) -> Result<Self, StoreError> {
        Self::start_with_hooks(config, Arc::default())
    }

    /// [`Store::start`] with the holds a test places (APP-DESIGN R8).
    pub fn start_with_hooks(config: StoreConfig, hooks: Arc<Hooks>) -> Result<Self, StoreError> {
        let app = Arc::new(AppData::open(
            config.vfs.clone(),
            &config.app_data,
            &*config.clock,
        )?);
        atomic_create_dirs(&*config.vfs, &config.projects)?;
        let cleanup = Arc::new(Cleanup::default());
        let env = Env {
            vfs: config.vfs.clone(),
            app,
            platform: config.platform.clone(),
            clock: config.clock.clone(),
            threads: config.threads.max(1),
            watch_debounce: config.watch_debounce,
            hooks: hooks.clone(),
        };
        let (vfs, projects, gate) = (config.vfs.clone(), config.projects.clone(), cleanup.clone());
        let spawned = std::thread::Builder::new()
            .name("tc-store start cleanup".into())
            .spawn(move || {
                let removed = remove_staging(&*vfs, &projects, &hooks);
                gate.finish(removed);
            });
        if spawned.is_err() {
            let removed = remove_staging(&*config.vfs, &config.projects, &env.hooks);
            cleanup.finish(removed);
        }
        Ok(Self {
            env,
            projects: config.projects,
            cleanup,
        })
    }

    pub fn app_data(&self) -> &AppData {
        &self.env.app
    }

    pub fn hooks(&self) -> &Arc<Hooks> {
        &self.env.hooks
    }

    /// Waits until the start cleanup is over.
    pub fn wait_cleanup(&self) {
        self.cleanup.wait();
    }

    /// The staging folders the start cleanup removed (§6.4 N1), once it is over: the app
    /// tells the person of a join or an import that did not finish.
    pub fn removed_at_start(&self) -> Vec<RemovedStaging> {
        self.cleanup.wait();
        self.cleanup
            .removed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// A new staging folder's path for `kind`, after the start cleanup is over, so the
    /// cleanup can never remove it (N1, tC4 #676). The caller creates and fills it, then
    /// puts it in place with [`Store::place`].
    pub fn staging(&self, kind: StagingKind) -> PathBuf {
        self.cleanup.wait();
        self.projects.join(format!(
            "{STAGING}{}-{}",
            kind.name(),
            crate::random::hex::<16>()
        ))
    }

    /// Renames a filled staging folder to `<projects>/<name>` without replacing anything
    /// there (N2). On any failure only the staging folder is removed; whatever is at the
    /// name keeps every byte (`docs/TC4-DELTA-2026-10-16.md` §3.10 item 1).
    pub fn place(&self, staging: &Path, name: &str) -> Result<PathBuf, CreateError> {
        let target = self.projects.join(name);
        if let Err(e) = valid_name(name) {
            let _ = remove_dir_all_held(&*self.env.vfs, staging, &HeldRetry::SAVE);
            return Err(e);
        }
        self.env.hooks.wait(Point::BeforeStagingRename);
        match rename_dir_noreplace_held(&*self.env.vfs, staging, &target, &HeldRetry::SAVE) {
            Ok(()) => {
                self.env
                    .vfs
                    .sync_dir(&self.projects)
                    .map_err(|e| StoreError::io("flush", &self.projects, e))?;
                Ok(target)
            }
            Err(e) => {
                let _ = remove_dir_all_held(&*self.env.vfs, staging, &HeldRetry::SAVE);
                if self.env.vfs.metadata(&target).is_ok() {
                    Err(CreateError::NameExists(name.to_owned()))
                } else {
                    Err(StoreError::io("rename into place", staging, e).into())
                }
            }
        }
    }

    /// The project folders in the projects folder.
    pub fn list(&self) -> Result<Vec<PathBuf>, StoreError> {
        let entries = self
            .env
            .vfs
            .read_dir(&self.projects)
            .map_err(|e| StoreError::io("list", &self.projects, e))?;
        Ok(entries
            .into_iter()
            .filter(|e| e.kind == EntryKind::Dir)
            .map(|e| self.projects.join(&e.name))
            .filter(|p| {
                self.env
                    .vfs
                    .metadata(&Folder::new(p).path(PROJECT_JSON))
                    .is_ok()
            })
            .collect())
    }

    /// Opens the project in `folder` (§6.1).
    pub fn open(&self, folder: &Path, options: &OpenOptions) -> Result<ProjectHandle, OpenError> {
        open_project(&self.env, folder, options, None)
    }

    /// Creates a project (§6.4 N1, N2): built and materialized in a staging folder, then
    /// renamed into place without replacing anything.
    pub fn create(&self, new: &NewProject) -> Result<PathBuf, CreateError> {
        self.cleanup.wait();
        valid_name(&new.name)?;
        check_details(new)?;
        let target = self.projects.join(&new.name);
        if self.env.vfs.metadata(&target).is_ok() {
            return Err(CreateError::NameExists(new.name.clone()));
        }
        let staging = self.staging(StagingKind::Create);
        if let Err(e) = self.build(new, &staging, &target) {
            let _ = remove_dir_all_held(&*self.env.vfs, &staging, &HeldRetry::SAVE);
            return Err(e);
        }
        self.place(&staging, &new.name)
    }

    fn build(&self, new: &NewProject, staging: &Path, target: &Path) -> Result<(), CreateError> {
        let vfs = &*self.env.vfs;
        let folder = Folder::new(staging);
        vfs.create_dir(staging)
            .map_err(|e| StoreError::io("create", staging, e))?;
        let id = ProjectId::from_bytes(crate::random::bytes::<16>());
        let fixed: [(&str, Vec<u8>); 4] = [
            (GITIGNORE, GITIGNORE_BYTES.to_vec()),
            (GITATTRIBUTES, GITATTRIBUTES_BYTES.to_vec()),
            (
                PROJECT_JSON,
                ProjectJson::new(id, new.kind, self.env.clock.now()).to_bytes(),
            ),
            (FORMAT_JSON, FormatDoc::current().to_bytes()),
        ];
        for (rel, bytes) in fixed {
            let (dir, name) = folder.dir_and_name(rel);
            atomic_create_dirs(vfs, &dir)?;
            atomic_write(vfs, &dir, name, &bytes, SyncDir::Now).map_err(|e| e.into_inner())?;
        }
        // The fingerprint every later open takes is of the canonical path (§6.3). The target
        // does not exist yet, so its canonical path is the projects folder's and the name:
        // the raw path would differ from it through a symlink, or the `\\?\` prefix Windows
        // gives every canonical path, and the first open would mint another device.
        let identity = self
            .env
            .vfs
            .canonicalize(&self.projects)
            .map(|projects| projects.join(name_of(target)))
            .unwrap_or_else(|_| target.to_owned());
        let handle = open_project(&self.env, staging, &OpenOptions::default(), Some(&identity))?;
        if let Mode::ReadOnly(reason) = handle.facts().mode {
            return Err(CreateError::Store(StoreError::Other(format!(
                "the projects folder cannot keep a project safe: {reason:?}"
            ))));
        }
        if !new.records.is_empty() {
            let plan = UpgradePlan {
                changes: new
                    .records
                    .iter()
                    .map(|record| PlannedChange {
                        record: record.clone(),
                        parents: Vec::new(),
                        cause: Cause::Seed,
                    })
                    .collect(),
            };
            handle
                .submit(Command::ApplyUpgrade { plan })
                .wait()
                .map_err(CreateError::Refused)?;
        }
        for (book, scope, seed) in &new.books {
            handle
                .submit(Command::AddBook {
                    book: *book,
                    scope: scope.clone(),
                    seed: seed.clone(),
                })
                .wait()
                .map_err(CreateError::Refused)?;
        }
        handle
            .checkpoint(crate::handle::CheckpointReason::Other("created".into()))
            .wait()?;
        handle.close()?;
        Ok(())
    }

    /// Copies a project, every pending save and draft included, to a new folder in the
    /// projects folder: the offer of a read-only open (§5.0, APP-DESIGN R7). The copy gets
    /// a device of its own at its first open.
    pub fn copy_project(&self, from: &Path, name: &str) -> Result<PathBuf, CreateError> {
        self.cleanup.wait();
        valid_name(name)?;
        let target = self.projects.join(name);
        if self.env.vfs.metadata(&target).is_ok() {
            return Err(CreateError::NameExists(name.to_owned()));
        }
        let staging = self.staging(StagingKind::Copy);
        if let Err(e) = copy_tree(&*self.env.vfs, from, &staging, &self.env.hooks) {
            let _ = remove_dir_all_held(&*self.env.vfs, &staging, &HeldRetry::SAVE);
            return Err(e.into());
        }
        let staged = Folder::new(&staging);
        for rel in [LOCK, DEVICE] {
            let path = staged.path(rel);
            if self.env.vfs.metadata(&path).is_ok() {
                let _ = remove_held(&*self.env.vfs, &path, &HeldRetry::SAVE);
            }
        }
        self.place(&staging, name)
    }
}

/// Refuses a project detail a Scripture Burrito cannot hold before any folder exists (tC4
/// #582): a language tag with a part over eight letters would otherwise be stored as
/// "Undetermined", and the name and script direction with it.
fn check_details(new: &NewProject) -> Result<(), CreateError> {
    let kind = match new.kind {
        ProjectKind::Bible => tc_burrito::ProjectKind::BibleText,
        ProjectKind::Stories => tc_burrito::ProjectKind::OpenBibleStories,
    };
    for record in &new.records {
        let Record::ProjectMeta {
            field,
            value: Some(json),
        } = record
        else {
            continue;
        };
        let value: serde_json::Value =
            serde_json::from_slice(&json.to_canonical()).unwrap_or(serde_json::Value::Null);
        tc_burrito::check_field(kind, &field.path(), &value).map_err(|why| {
            CreateError::Refused(CommandError::Refused(
                crate::reply::Refusal::NotValidInBurrito(why),
            ))
        })?;
    }
    Ok(())
}

/// The last component of a path.
fn name_of(path: &Path) -> &std::ffi::OsStr {
    path.file_name().unwrap_or(path.as_os_str())
}

/// A folder name the projects folder can hold on every platform.
fn valid_name(name: &str) -> Result<(), CreateError> {
    let bad = name.is_empty()
        || name.starts_with('.')
        || name.ends_with(' ')
        || name.ends_with('.')
        || name.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        });
    if bad {
        Err(CreateError::BadName(name.to_owned()))
    } else {
        Ok(())
    }
}

/// N1: removes every `.staging-…/` in the projects folder, and says which.
fn remove_staging(vfs: &dyn Vfs, projects: &Path, hooks: &Hooks) -> Vec<RemovedStaging> {
    let Ok(entries) = vfs.read_dir(projects) else {
        return Vec::new();
    };
    let mut removed = Vec::new();
    for entry in entries {
        let name = entry.name.to_string_lossy().into_owned();
        if entry.kind == EntryKind::Dir && name.starts_with(STAGING) {
            hooks.wait(Point::BeforeStagingRemoval);
            if remove_dir_all_held(vfs, &projects.join(&entry.name), &HeldRetry::SAVE).is_ok() {
                removed.push(RemovedStaging {
                    kind: StagingKind::of(&name),
                    folder: name,
                });
            }
        }
    }
    removed
}

/// Copies `from` to the new folder `to` file by file, each file written atomically; the
/// directory flushes make the copy durable before it is renamed into place. The journal goes
/// first, by [`copy_journal`]; every other file is copied as it is found, and one that went
/// meanwhile (a derived file being rewritten) is left out: the copy's open rebuilds it.
fn copy_tree(vfs: &dyn Vfs, from: &Path, to: &Path, hooks: &Hooks) -> Result<(), StoreError> {
    vfs.create_dir(to)
        .map_err(|e| StoreError::io("create", to, e))?;
    copy_journal(vfs, from, to, hooks)?;
    let skip = [
        Folder::new(from).path(PACKS),
        Folder::new(from).path(PENDING),
    ];
    let mut stack = vec![(from.to_owned(), to.to_owned())];
    while let Some((src, dst)) = stack.pop() {
        let entries = vfs
            .read_dir(&src)
            .map_err(|e| StoreError::io("list", &src, e))?;
        for entry in entries {
            let (s, d) = (src.join(&entry.name), dst.join(&entry.name));
            match entry.kind {
                EntryKind::Dir => {
                    if entry.name == ".git" || skip.contains(&s) {
                        continue;
                    }
                    atomic_create_dirs(vfs, &d)?;
                    stack.push((s, d));
                }
                EntryKind::File => {
                    let name = entry.name.to_string_lossy().into_owned();
                    if crate::atomic::temp_target(&name).is_some() {
                        continue;
                    }
                    let bytes = match vfs.read(&s) {
                        Ok(bytes) => bytes,
                        Err(e) if is_not_found(&e) => continue,
                        Err(e) => return Err(StoreError::io("read", &s, e)),
                    };
                    atomic_write(vfs, &dst, &name, &bytes, SyncDir::Now)
                        .map_err(|e| e.into_inner())?;
                }
                EntryKind::Other => {}
            }
        }
    }
    Ok(())
}

/// How many times [`copy_journal`] lists the journal again before it gives up on a folder
/// that keeps changing.
const COPY_ROUNDS: usize = 16;

/// Copies every pack and pending file of `from` to `to` while another writer may be sealing
/// or consolidating in `from` (the copy offer is for exactly that case, §5.0), so the copy
/// keeps every change `from` held when it began.
///
/// A seal writes its pack before it removes the pending files the pack holds, and a
/// consolidation writes its pack before it removes the packs it replaces (§5.11, §5.12): a
/// change only ever leaves a file after it is in a newer one. So the pending files are
/// copied first and the packs after them, and a file that went before it was read never
/// counts as copied: the journal is listed again, and every file not copied yet is copied,
/// until a whole round finds nothing new and nothing gone. Packs and pending files are named
/// by their content, so a name copied once never needs a second read.
fn copy_journal(vfs: &dyn Vfs, from: &Path, to: &Path, hooks: &Hooks) -> Result<(), StoreError> {
    let (src, dst) = (Folder::new(from), Folder::new(to));
    let mut copied: BTreeSet<String> = BTreeSet::new();
    for _ in 0..COPY_ROUNDS {
        let mut changed = false;
        for (i, kind) in [JournalKind::Pending, JournalKind::Pack]
            .into_iter()
            .enumerate()
        {
            if i == 1 {
                hooks.wait(Point::CopyBetweenJournalFolders);
            }
            let listed = list(vfs, &src).map_err(|e| StoreError::io("list", from, e))?;
            for file in listed.into_iter().filter(|f| f.kind == kind) {
                if copied.contains(&file.rel) {
                    continue;
                }
                changed = true;
                let bytes = match vfs.read(&file.abs) {
                    Ok(bytes) => bytes,
                    // Its changes are in a newer file: the next round lists it.
                    Err(e) if is_not_found(&e) => continue,
                    Err(e) => return Err(StoreError::io("read", &file.abs, e)),
                };
                let (dir, name) = dst.dir_and_name(&file.rel);
                atomic_create_dirs(vfs, &dir)?;
                atomic_write(vfs, &dir, name, &bytes, SyncDir::Now).map_err(|e| e.into_inner())?;
                copied.insert(file.rel);
            }
        }
        if !changed {
            return Ok(());
        }
    }
    Err(StoreError::Other(
        "the project kept changing while it was copied; wait until the other computer has stopped saving and copy it again"
            .into(),
    ))
}
