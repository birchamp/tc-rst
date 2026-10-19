//! The open sequence (`docs/STORAGE.md` §6.1, O1 to O14) and the classification of derived
//! files (§6.2).
//!
//! Unit S1 has no git: the repository steps (O3, the blobs of O8, restoring a pack or
//! `format.json` from HEAD) are unit S2's. An expected pack (§4.4) is known from the
//! manifest's pack list; when one is missing, nothing derived is removed until it is back
//! (`PackMissing`). A derived file changed outside the app is held (§6.2), not recorded:
//! recording outside edits (§11.1) is unit S2's.
//!
//! The open reads each journal file once (O8), takes unchanged files' changes from the fold
//! cache, and classifies each derived file without reading it when its entry is clean by
//! stat and its scope digest unchanged (O9), so an open after no change projects no file.
//! Progress is reported from a thread of its own from 300 ms on (§6.1 "Progress").

use crate::appdata::{AppData, rfc3339};
use crate::atomic::{
    DirSet, SyncDir, atomic_create_dirs, atomic_remove, atomic_write, temp_target,
};
use crate::clock::Clock;
use crate::counters::{Counters, CountingVfs};
use crate::device::{self, Identity};
use crate::drafts::DraftWriter;
use crate::error::StoreError;
use crate::handle::{Mode, ProjectFacts, ProjectHandle, Shared};
use crate::hooks::Hooks;
use crate::layout::{
    self, FORMAT_JSON, Folder, INDEX, INTENT, LOCK, MANIFEST, METADATA, PACKS, PENDING,
    PROJECT_JSON, QUARANTINE, TC,
};
use crate::load::{
    FileRead, JournalFile, JournalKind, LoadSetup, list, load_streaming, relocation,
};
use crate::manifest::{Entry, Intent, Manifest};
use crate::platform::Platform;
use crate::progress::{Phase, ProgressSink, Tracker, no_progress};
use crate::project::{Dirty, IndexCache, Journal, PackInfo, PendingInfo, Project};
use crate::reply::ReadOnlyReason;
use crate::source::FileSource;
use crate::vfs::{EntryKind, LockHandle, Vfs, is_not_found};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tc_journal::fold::{Anomaly, DerivedFile, FoldState, Notice, ProjectKind, RotationReason};
use tc_journal::format::{FormatDoc, FormatGate};
use tc_journal::index::{Facts, Index};
use tc_journal::keys::{PinGroup, RecordKey};
use tc_journal::pack::{PackStatus, read_pack};
use tc_journal::pending::{PendingHeader, PendingStatus, Purpose, encode_pending, read_pending};
use tc_journal::record::PinGroupValue;
use tc_journal::{ChangeId, DeviceId, Md5, PackId, ProjectId, RepoPath, TokenKey, Verified};

/// How to open a project.
#[derive(Clone)]
pub struct OpenOptions {
    /// Where progress reports go (§6.1 "Progress").
    pub progress: ProgressSink,
    /// The person confirmed that the computer `.tc/lock` names is closed (§5.0).
    pub other_computer_closed: bool,
    /// The pins a new project gets. A writable open of a project that holds no pin writes
    /// them, one change per group in one batch with cause `System(CreationPins)` (tC4 #766).
    pub creation_pins: Vec<(PinGroup, PinGroupValue)>,
    /// The app's server run whose page sends this open's drafts (APP-DESIGN §6.6): each draft
    /// records it, and a draft of this same run is that page's own typing, never offered to it.
    pub run: Option<String>,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            progress: no_progress(),
            other_computer_closed: false,
            creation_pins: Vec::new(),
            run: None,
        }
    }
}

impl std::fmt::Debug for OpenOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenOptions")
            .field("other_computer_closed", &self.other_computer_closed)
            .finish_non_exhaustive()
    }
}

/// Why a project did not open (§6.1: only these refuse; content never does).
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("this folder is not a translationCore project of this format")]
    NotAProject,
    #[error("the project folder cannot be read: {0}")]
    Unreadable(StoreError),
    /// O1: another process holds `.tc/lock`. translationCore runs once per computer (tC4
    /// D39), so the holder is a process a crash left running or another program; the open is
    /// refused and writes nothing.
    #[error("this project is already open: {}", holder_words(.holder))]
    LockHeld { holder: Option<LockHolder> },
    #[error("this project was made by a newer translationCore (format {major}); update to open it")]
    NewerMajor { major: String },
    #[error("this project's format {major} needs a migration first")]
    NeedsMigration { major: String },
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Who `.tc/lock` names as its holder (§3.6): the process that wrote its name there after
/// it took the lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockHolder {
    pub host: String,
    pub pid: u32,
}

fn holder_words(holder: &Option<LockHolder>) -> String {
    match holder {
        Some(h) => format!("process {} on {} holds it", h.pid, h.host),
        None => "another program holds it".to_owned(),
    }
}

/// `ingredients/tc/project.json` (§3.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectJson {
    pub schema_version: u32,
    pub project_id: String,
    pub kind: String,
    pub created: String,
}

impl ProjectJson {
    pub(crate) fn new(id: ProjectId, kind: ProjectKind, created: tc_journal::WallTime) -> Self {
        Self {
            schema_version: 1,
            project_id: id.to_string(),
            kind: match kind {
                ProjectKind::Bible => "bible",
                ProjectKind::Stories => "obs",
            }
            .to_owned(),
            created: rfc3339(created),
        }
    }

    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut out = serde_json::to_vec(self).unwrap_or_default();
        out.push(b'\n');
        out
    }

    fn parse(bytes: &[u8]) -> Option<(ProjectId, ProjectKind)> {
        let file: ProjectJson = serde_json::from_slice(bytes).ok()?;
        let kind = match file.kind.as_str() {
            "bible" => ProjectKind::Bible,
            "obs" => ProjectKind::Stories,
            _ => return None,
        };
        Some((ProjectId::parse(&file.project_id)?, kind))
    }
}

/// `.tc/lock`'s content (§3.6): who holds the project open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LockContent {
    pub host: String,
    pub pid: u32,
    pub device: String,
    pub at: String,
}

/// What the background storage check (O14) found at one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CheckResult {
    pub rel: String,
    /// `None` when the file is missing or could not be read.
    pub md5: Option<String>,
    pub size: u64,
    pub mtime_ns: i64,
    /// The newest time stamp before the read began (§6.2).
    pub stamp_ns: i64,
}

/// What a store passes to every open.
pub(crate) struct Env {
    pub vfs: Arc<dyn Vfs>,
    pub app: Arc<AppData>,
    pub platform: Arc<dyn Platform>,
    pub clock: Arc<dyn Clock>,
    pub threads: usize,
    pub hooks: Arc<Hooks>,
}

fn md5_hex(bytes: &[u8]) -> String {
    Md5::of(bytes).to_hex()
}

/// Opens the project in `path` (see the module documentation). `identity` is the canonical
/// folder path the device fingerprint is taken of, when it is not `path`'s (a project being
/// created in a staging folder is fingerprinted at its final path, §6.3).
pub(crate) fn open_project(
    env: &Env,
    path: &Path,
    options: &OpenOptions,
    identity: Option<&Path>,
) -> Result<ProjectHandle, OpenError> {
    let started = Instant::now();
    let (tracker, reporter) = crate::progress::start(options.progress.clone(), started);
    let result = open_inner(env, path, options, identity, &tracker);
    reporter.finish(&tracker);
    result
}

/// What O1 to O6 decided.
struct Gates {
    read_only: Option<ReadOnlyReason>,
    lock: Option<Box<dyn LockHandle>>,
    network: bool,
    format: Option<FormatDoc>,
}

fn open_inner(
    env: &Env,
    path: &Path,
    options: &OpenOptions,
    identity: Option<&Path>,
    tracker: &Tracker,
) -> Result<ProjectHandle, OpenError> {
    tracker.phase(Phase::Finishing, 6, 0);
    let counters = Arc::new(Counters::default());
    let vfs: Arc<dyn Vfs> =
        Arc::new(CountingVfs::new(env.vfs.clone(), counters.clone()).for_project(path.to_owned()));
    let folder = Folder::new(path);
    vfs.metadata(path)
        .map_err(|e| OpenError::Unreadable(StoreError::io("stat", path, e)))?;
    let project_json = match vfs.read(&folder.path(PROJECT_JSON)) {
        Ok(bytes) => bytes,
        Err(e) if is_not_found(&e) => return Err(OpenError::NotAProject),
        Err(e) => {
            return Err(OpenError::Unreadable(StoreError::io(
                "read",
                &folder.path(PROJECT_JSON),
                e,
            )));
        }
    };
    let (id, kind) = ProjectJson::parse(&project_json).ok_or(OpenError::NotAProject)?;
    let mut gates = gates(env, &*vfs, &folder, options)?;
    tracker.add(1, 0);
    // A write at open that fails (a full disk, a folder that went read-only) ends the
    // writes: the project opens read-only with `WriteFailed` and never refuses (§6.1).
    let mut write_failed: Option<String> = None;
    // O6: device identity.
    let machine = env.platform.machine_id();
    let canonical = match identity {
        Some(canonical) => canonical.to_owned(),
        None => vfs.canonicalize(path).unwrap_or_else(|_| path.to_owned()),
    };
    let print = device::fingerprint(env.app.secret(), &machine, &canonical.to_string_lossy());
    let read_only = gates.read_only.is_some();
    let mut identity = match device::identify(&*vfs, &folder, print, &*env.clock, read_only) {
        Ok(identity) => identity,
        Err(e) if !read_only => {
            write_failed = Some(e.to_string());
            device::identify(&*vfs, &folder, print, &*env.clock, true)?
        }
        Err(e) => return Err(e.into()),
    };
    tracker.add(1, 0);
    // O1's content, now that the device is known.
    let mut stamp_ns = 0;
    if let Some(lock) = gates.lock.as_mut()
        && write_failed.is_none()
    {
        let content = LockContent {
            host: env.platform.host(),
            pid: env.platform.pid(),
            device: identity.device().to_string(),
            at: rfc3339(env.clock.now()),
        };
        let bytes = serde_json::to_vec(&content).unwrap_or_default();
        match lock.set_content(&bytes) {
            Ok(()) => {
                if let Ok(meta) = vfs.metadata(&folder.path(LOCK)) {
                    stamp_ns = meta.mtime_ns;
                }
            }
            Err(e) => {
                write_failed = Some(StoreError::io("write", &folder.path(LOCK), e).to_string());
            }
        }
    }
    // O2.
    if !read_only && write_failed.is_none() {
        // O4's repair: unit S1 has no HEAD to restore `format.json` from, so a missing or
        // damaged one is written as this app's; a newer one was never damaged here.
        let repaired = finish_temps(&*vfs, &folder, id).and_then(|()| {
            // The folder every save goes into is made durable now (once, §5.0), not by the
            // first save.
            atomic_create_dirs(&*vfs, &folder.path(PENDING))?;
            if gates.format.is_some() {
                return Ok(());
            }
            let (dir, name) = folder.dir_and_name(FORMAT_JSON);
            atomic_create_dirs(&*vfs, &dir)?;
            atomic_write(
                &*vfs,
                &dir,
                name,
                &FormatDoc::current().to_bytes(),
                SyncDir::Now,
            )
            .map_err(|e| e.into_inner())
        });
        if let Err(e) = repaired {
            write_failed = Some(e.to_string());
        }
    }
    tracker.add(1, 0);
    // O8: the journal, in parallel, from the fold cache where it stands.
    let files = list(&*vfs, &folder)
        .map_err(|e| OpenError::Unreadable(StoreError::io("list", &folder.path(PACKS), e)))?;
    let index = vfs
        .read(&folder.path(INDEX))
        .ok()
        .and_then(|bytes| Index::parse(bytes, id).ok())
        .map(Arc::new);
    let bytes_total: u64 = files.iter().map(|f| f.size).sum();
    tracker.phase(Phase::Reading, files.len() as u64, bytes_total);
    let source = Arc::new(FileSource::new(vfs.clone()));
    let value_source: Arc<dyn tc_journal::fold::ValueSource> = source.clone();
    let fold_tracker = tracker.clone();
    let (fold, reads) = load_streaming(
        LoadSetup {
            vfs: vfs.clone(),
            project: id,
            index: index.clone(),
            threads: env.threads,
        },
        files,
        &source,
        tracker,
        move |inputs| {
            let mut folding = false;
            FoldState::fold_from_with_steps(kind, Some(value_source), inputs, &mut |done, total| {
                if !folding {
                    fold_tracker.phase(Phase::Folding, u64::from(total), 0);
                    folding = true;
                }
                fold_tracker.set(u64::from(done));
            })
        },
    );
    let mut notices: Vec<(Notice, Option<String>)> = Vec::new();
    for read in &reads {
        if let Ok(facts) = &read.outcome {
            counters.add(&counters.journal_files_read, 1);
            counters.add(&counters.journal_files_cached, u64::from(facts.cached));
        }
    }
    let (mut journal, index_cache) = journal_of(reads, index.as_deref(), &mut notices);
    if !journal.unreadable.is_empty() && vfs.read(&folder.path(PROJECT_JSON)).is_err() {
        // Not one file a cloud client holds back: the folder itself stopped being readable.
        return Err(OpenError::Unreadable(StoreError::Other(
            "the project folder stopped being readable while it opened".into(),
        )));
    }
    let mut manifest = vfs
        .read(&folder.path(MANIFEST))
        .ok()
        .and_then(|b| Manifest::parse(&b))
        .unwrap_or_default();
    // An expected pack (§4.4) that is gone: missing, unless a valid pack replaces it (a
    // consolidation that stopped after Q4, before the manifest was written).
    let replaced: BTreeSet<tc_journal::PackName> = journal
        .packs
        .values()
        .filter(|p| p.valid)
        .flat_map(|p| p.replaces.iter().copied())
        .collect();
    let listed: Vec<String> = manifest.packs.keys().cloned().collect();
    for rel in listed {
        if journal.packs.contains_key(&rel) || journal.unreadable.contains(&rel) {
            continue;
        }
        if tc_journal::pack::PackPath::parse(&rel).is_some_and(|p| replaced.contains(&p.name)) {
            manifest.packs.remove(&rel);
            continue;
        }
        journal.missing.insert(rel.clone());
        if let Ok(path) = RepoPath::parse(&rel) {
            notices.push((Notice::PackMissing { path }, None));
        }
    }
    // O6, second half: the ledger and duplicate devices need the fold (§6.3).
    let stats = fold.device_stats();
    let now = env.clock.now().0;
    for (device, stat) in &stats {
        let by_ms = stat.max_at.0 - now;
        if by_ms > 24 * 60 * 60 * 1000 {
            notices.push((
                Notice::ClockAhead {
                    device: *device,
                    by_ms,
                },
                None,
            ));
        }
    }
    if !read_only && write_failed.is_none() {
        let pending_unreadable = journal
            .unreadable
            .iter()
            .any(|rel| rel.starts_with(&format!("{PENDING}/")));
        match check_identity(
            env,
            &*vfs,
            &folder,
            identity,
            &fold,
            &stats,
            &print,
            pending_unreadable,
        ) {
            Ok((checked, notice)) => {
                identity = checked;
                notices.extend(notice.map(|n| (n, None)));
            }
            Err(e) => write_failed = Some(e.to_string()),
        }
    }
    if let Identity::Minted {
        device,
        old: Some(old),
        reason,
    } = identity
    {
        notices.push((
            Notice::DeviceRotated {
                old,
                new: device,
                reason,
            },
            None,
        ));
    }
    let device = identity.device();
    let next_seq = stats.get(&device).map_or(1, |s| s.max_seq.0 + 1);
    let signing = Arc::new(TokenKey::derive(env.app.secret(), id));
    let hooks = env.hooks.clone();
    let facts = ProjectFacts {
        id,
        kind,
        device,
        folder: path.to_owned(),
        mode: match (&gates.read_only, &write_failed) {
            (Some(reason), _) => Mode::ReadOnly(reason.clone()),
            (None, Some(reason)) => Mode::ReadOnly(ReadOnlyReason::WriteFailed(reason.clone())),
            (None, None) => Mode::Writable,
        },
    };
    let shared = Arc::new(Shared::new(
        fold.clone(),
        signing.clone(),
        facts,
        counters.clone(),
        hooks.clone(),
    ));
    let mut project = Project {
        vfs: vfs.clone(),
        counters: counters.clone(),
        folder: folder.clone(),
        id,
        kind,
        device,
        next_seq,
        signing: signing.clone(),
        fold,
        source,
        clock: env.clock.clone(),
        app: env.app.clone(),
        network: gates.network,
        journal,
        index: index_cache,
        manifest,
        dirty: Dirty::default(),
        stamp_ns,
        shared: shared.clone(),
        manifest_changed: false,
        prerendered: BTreeMap::new(),
        busy: BTreeMap::new(),
        copied_outside: BTreeSet::new(),
    };
    for (notice, copy) in notices {
        project.notice(notice, copy);
    }
    let mut unchecked: Vec<String> = Vec::new();
    if !read_only && write_failed.is_none() {
        match repair_and_materialize(&mut project, tracker, identity, &options.creation_pins) {
            Ok(paths) => unchecked = paths,
            Err(e) => {
                let reason = e.to_string();
                project
                    .shared
                    .set_mode(Mode::ReadOnly(ReadOnlyReason::WriteFailed(reason.clone())));
                write_failed = Some(reason);
            }
        }
    } else {
        project.renew_hold_notices();
    }
    // A project that cannot be written holds no lock: it writes nothing, so there is
    // nothing to guard, and the person can open it again once the disk works.
    let read_only = read_only || write_failed.is_some();
    if write_failed.is_some() {
        gates.lock = None;
    }
    // O12.
    let drafts = Arc::new(DraftWriter::start(
        vfs.clone(),
        folder.clone(),
        &project.fold,
        &signing,
        options.run.clone(),
        hooks.clone(),
        counters.clone(),
        read_only,
    ));
    project.publish();
    // O14 and the fold cache, in the background.
    let background = (!read_only).then(|| {
        let mut paths = unchecked;
        paths.extend(unsettled(&project.manifest));
        paths.sort();
        paths.dedup();
        crate::handle::Background {
            vfs: vfs.clone(),
            counters: counters.clone(),
            folder: folder.clone(),
            index: project.index_bytes_later(),
            project: id,
            check: paths,
            stamp_ns: project.stamp_ns,
        }
    });
    let handle = crate::actor::spawn(project, gates.lock.take(), drafts);
    if let Some(background) = background {
        handle.start_background(background);
    }
    Ok(handle)
}

/// O7, O9 and O10: finishes interrupted work, classifies the derived files and writes what
/// differs. Returns the paths O9 took as converged without reading them (for O14). A write
/// that fails ends the open's writes (see the caller).
fn repair_and_materialize(
    project: &mut Project,
    tracker: &Tracker,
    identity: Identity,
    creation_pins: &[(PinGroup, PinGroupValue)],
) -> Result<Vec<String>, StoreError> {
    // O7.
    project.finish_interrupted()?;
    // A new device names its person as the old one did (§6.3).
    if let Identity::Minted { old: Some(old), .. } = identity {
        project.copy_profile(old)?;
    }
    project.write_creation_pins(creation_pins)?;
    // O9, O10.
    let unchecked = project.classify(tracker)?;
    let paths = project.dirty_count();
    tracker.phase(Phase::Writing, paths as u64, 0);
    project.materialize()?;
    if project.manifest_changed {
        project.write_manifest()?;
    }
    tracker.set(paths as u64);
    Ok(unchecked)
}

/// O1, O4 and O5: the lock, the format gate and the filesystem gate. A folder that must
/// open read-only is never written, not even `.tc/` (§6.1 "A read-only open only reads").
fn gates(
    env: &Env,
    vfs: &dyn Vfs,
    folder: &Folder,
    options: &OpenOptions,
) -> Result<Gates, OpenError> {
    let mut read_only: Option<ReadOnlyReason> = None;
    // O4.
    let format = match vfs.read(&folder.path(FORMAT_JSON)) {
        Ok(bytes) => FormatDoc::parse(&bytes).ok(),
        Err(e) if is_not_found(&e) => None,
        Err(e) => {
            return Err(OpenError::Unreadable(StoreError::io(
                "read",
                &folder.path(FORMAT_JSON),
                e,
            )));
        }
    };
    if let Some(doc) = &format {
        match doc.gate() {
            FormatGate::Writable => {}
            FormatGate::ReadOnly { min_reader_minor } => {
                read_only = Some(ReadOnlyReason::NewerMinor {
                    min_reader_minor: min_reader_minor.as_str().to_owned(),
                });
            }
            FormatGate::Refused { major } => {
                return Err(OpenError::NewerMajor {
                    major: major.as_str().to_owned(),
                });
            }
            FormatGate::NeedsMigration { major } => {
                return Err(OpenError::NeedsMigration {
                    major: major.as_str().to_owned(),
                });
            }
        }
    }
    // O5.
    let facts = env.platform.folder(folder.root());
    if read_only.is_none()
        && let Some(unsafe_folder) = facts.unsafe_folder.clone()
    {
        read_only = Some(ReadOnlyReason::UnsafeFolder(unsafe_folder));
    }
    // O1.
    let mut lock = None;
    if read_only.is_none() {
        let before = vfs
            .read(&folder.path(LOCK))
            .ok()
            .and_then(|b| serde_json::from_slice::<LockContent>(&b).ok());
        // A folder that holds `.tc` is not touched before the lock is ours: a refused open
        // writes nothing, not even a directory flush.
        let tc = folder.path(TC);
        let made = if vfs.metadata(&tc).is_ok() {
            Ok(())
        } else {
            atomic_create_dirs(vfs, &tc)
        };
        let taken = made
            .map_err(|e| e.to_string())
            .and_then(|()| vfs.lock(&folder.path(LOCK)).map_err(|e| e.to_string()));
        match taken {
            Ok(Some(handle)) => {
                let elsewhere = before
                    .filter(|c| c.host != env.platform.host())
                    .filter(|_| !options.other_computer_closed);
                match elsewhere {
                    Some(content) => {
                        read_only =
                            Some(ReadOnlyReason::OpenOnAnotherComputer { host: content.host });
                    }
                    None => lock = Some(handle),
                }
            }
            Ok(None) => {
                // The holder wrote its name after it took the lock, so this read is the
                // holder's. Unnamed when it wrote none, or when the platform will not let
                // another handle read a locked file.
                let holder = vfs
                    .read(&folder.path(LOCK))
                    .ok()
                    .and_then(|b| serde_json::from_slice::<LockContent>(&b).ok())
                    .map(|c| LockHolder {
                        host: c.host,
                        pid: c.pid,
                    });
                return Err(OpenError::LockHeld { holder });
            }
            Err(reason) => read_only = Some(ReadOnlyReason::WriteFailed(reason)),
        }
    }
    Ok(Gates {
        read_only,
        lock,
        network: facts.network,
        format,
    })
}

/// O2: a temporary file of a pending file or pack whose bytes verify is renamed into place
/// durably (a power cut on Windows can keep the temp and lose the rename after "Saved");
/// every other temporary file is removed.
fn finish_temps(vfs: &dyn Vfs, folder: &Folder, project: ProjectId) -> Result<(), StoreError> {
    let mut stack: Vec<(PathBuf, String)> = vec![(folder.root().to_owned(), String::new())];
    while let Some((dir, rel)) = stack.pop() {
        let entries = match vfs.read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if is_not_found(&e) => continue,
            Err(e) => return Err(StoreError::io("list", &dir, e)),
        };
        let mut dirs = DirSet::new();
        for entry in entries {
            let name = entry.name.to_string_lossy().into_owned();
            let child = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if entry.kind == EntryKind::Dir {
                if child != ".git" && !name.starts_with(".staging-") {
                    stack.push((dir.join(&entry.name), child));
                }
                continue;
            }
            let Some(target) = temp_target(&name) else {
                continue;
            };
            let abs = dir.join(&entry.name);
            let target_rel = if rel.is_empty() {
                target.to_owned()
            } else {
                format!("{rel}/{target}")
            };
            let complete = vfs
                .read(&abs)
                .ok()
                .is_some_and(|bytes| verifies(&target_rel, &bytes, project));
            if complete {
                vfs.rename(&abs, &dir.join(target))
                    .map_err(|e| StoreError::io("rename", &abs, e))?;
                dirs.add_dir(&dir);
                // Windows keeps a rename in its log until the renamed file is flushed.
                let _ = vfs.sync_file(&dir.join(target));
            } else {
                atomic_remove(vfs, &abs, SyncDir::Deferred(&mut dirs))?;
            }
        }
        dirs.sync(vfs)?;
    }
    Ok(())
}

/// Whether `bytes` are a complete file for `rel`: a pending file whose name its bytes give,
/// or a pack that passes P2 to P4 at its path (its name is the hash of its header and ids).
fn verifies(rel: &str, bytes: &[u8], project: ProjectId) -> bool {
    if let Some(name) = rel.strip_prefix(&format!("{PENDING}/")) {
        let read = read_pending(bytes, project);
        let Some(header) = read.header.clone() else {
            return false;
        };
        if read.status != PendingStatus::Valid {
            return false;
        }
        return encode_pending(&header, &read.entries)
            .is_ok_and(|encoded| encoded.name == name && encoded.bytes == bytes);
    }
    if rel.starts_with(PACKS) {
        return read_pack(rel, bytes, project).is_valid();
    }
    false
}

/// What the open's reads say of each journal file, the notices they call for, and the fold
/// cache's sections that stand.
fn journal_of(
    reads: Vec<FileRead>,
    index: Option<&Index>,
    notices: &mut Vec<(Notice, Option<String>)>,
) -> (Journal, IndexCache) {
    let mut journal = Journal::default();
    let mut cache = IndexCache::default();
    let mut used = 0usize;
    for read in reads {
        let rel = read.file.rel.clone();
        let facts = match read.outcome {
            Ok(facts) => facts,
            Err(e) => {
                notices.push((
                    Notice::FileUnreadable {
                        path: rel.as_str().into(),
                        reason: e.to_string().into(),
                    },
                    None,
                ));
                journal.unreadable.insert(rel);
                continue;
            }
        };
        let section = match (facts.cached, facts.made) {
            (true, _) => index.and_then(|i| i.section(&facts.key)).cloned(),
            (false, made) => {
                cache.changed |= made.is_some();
                made
            }
        };
        if let Some(section) = section {
            used += 1;
            cache.sections.insert(rel.clone(), section);
        }
        match facts.facts {
            Facts::Pack(pack) => {
                match &pack.status {
                    PackStatus::Valid => {}
                    PackStatus::Damaged(reason) => {
                        if let Ok(path) = RepoPath::parse(&rel) {
                            notices.push((
                                Notice::QuarantinedPack {
                                    path,
                                    reason: reason.clone(),
                                    salvaged: facts.changes as u32,
                                },
                                None,
                            ));
                        }
                    }
                    PackStatus::Newer { tcpack } => {
                        if let Ok(path) = RepoPath::parse(&rel) {
                            notices.push((
                                Notice::NewerPack {
                                    path,
                                    tcpack: *tcpack,
                                },
                                None,
                            ));
                        }
                    }
                    PackStatus::Foreign { .. } => {}
                }
                journal.packs.insert(
                    rel,
                    PackInfo {
                        id: pack.id,
                        size: facts.key.size,
                        fingerprint: facts.key.fingerprint,
                        number: Some(read.number),
                        valid: pack.is_valid(),
                        sealer: pack.path.map(|p| p.sealer),
                        replaces: pack
                            .header
                            .as_ref()
                            .map(|h| h.replaces.clone())
                            .unwrap_or_default(),
                        ids: pack.ids,
                    },
                );
            }
            Facts::Pending(pending) => {
                journal.pending.insert(
                    rel,
                    PendingInfo {
                        ids: pending.ids,
                        number: Some(read.number),
                        status: pending.status,
                    },
                );
            }
        }
    }
    let cached = index.map_or(0, Index::len);
    cache.changed |= used != cached;
    (journal, cache)
}

/// O6 after the load: a device that wrote more than the folder holds (an older backup put
/// back) or that another computer also saves as is replaced by a new one (§6.3).
#[allow(clippy::too_many_arguments)]
fn check_identity(
    env: &Env,
    vfs: &dyn Vfs,
    folder: &Folder,
    identity: Identity,
    fold: &FoldState,
    stats: &BTreeMap<DeviceId, tc_journal::fold::DeviceStat>,
    print: &[u8; 32],
    pending_unreadable: bool,
) -> Result<(Identity, Option<Notice>), StoreError> {
    let device = identity.device();
    let held = stats.get(&device).map_or(0, |s| s.max_seq.0);
    let reason = if let Some(entry) = env.app.ledger_entry(device)
        && entry.last_seq > held
    {
        Some((
            RotationReason::BackupRestored,
            entry.last_seq - held,
            entry.last_at,
        ))
    } else if fold
        .anomalies()
        .any(|a| matches!(a, Anomaly::DuplicateDevice { device: d, .. } if *d == device))
    {
        Some((RotationReason::DuplicateDevice, 0, 0))
    } else if pending_unreadable && matches!(identity, Identity::Kept(_)) {
        // A file of saved changes that cannot be read may hold seq numbers this device used:
        // a new device cannot reuse one (§6.3). A device minted at this open has none.
        Some((RotationReason::PendingUnreadable, 0, 0))
    } else {
        None
    };
    let Some((reason, missing, last_at)) = reason else {
        return Ok((identity, None));
    };
    let new = device::mint();
    device::write(vfs, folder, new, &crate::random::to_hex(print), &*env.clock)?;
    // "This copy is missing N changes this computer made (last on 12 Oct)."
    let notice = (reason == RotationReason::BackupRestored).then_some(Notice::RestoredBackup {
        missing,
        last_at: tc_journal::WallTime(last_at),
    });
    Ok((
        Identity::Minted {
            device: new,
            old: Some(device),
            reason,
        },
        notice,
    ))
}

impl Project {
    /// The number of derived paths a materialization would write now.
    pub(crate) fn dirty_count(&self) -> usize {
        if self.dirty.everything {
            self.fold.derived_files().len() + 1
        } else {
            self.dirty.files.len() + usize::from(self.dirty.metadata)
        }
    }

    /// O7 (§6.1): pending files a valid pack holds whole, packs a valid pack replaces, and
    /// damaged or foreign pending files. Each file goes only once every change it holds is
    /// in another verified file, whose values the fold reads from then on.
    pub(crate) fn finish_interrupted(&mut self) -> Result<(), StoreError> {
        let valid_packs: BTreeMap<PackId, String> = self
            .journal
            .packs
            .iter()
            .filter(|(_, p)| p.valid)
            .map(|(rel, p)| (p.id, rel.clone()))
            .collect();
        let mut dirs = DirSet::new();
        let mut flushed: BTreeSet<String> = BTreeSet::new();
        // Pending files whose every change a valid pack holds: a seal that stopped after K3.
        let pending: Vec<(String, PendingInfo)> = self
            .journal
            .pending
            .iter()
            .map(|(rel, p)| (rel.clone(), p.clone()))
            .collect();
        for (rel, info) in pending {
            match &info.status {
                PendingStatus::Valid => {}
                PendingStatus::Damaged(reason) => {
                    self.salvage_pending(&rel, reason.clone(), &mut dirs)?;
                    continue;
                }
                PendingStatus::Foreign { .. } => {
                    self.quarantine(&rel, &mut dirs)?;
                    self.journal.pending.remove(&rel);
                    continue;
                }
                PendingStatus::Newer { .. } => continue,
            }
            let mut holders: BTreeSet<String> = BTreeSet::new();
            let whole = !info.ids.is_empty()
                && info.ids.iter().all(|id| {
                    let found = self.fold.change(id).and_then(|view| {
                        view.packs.iter().find_map(|p| valid_packs.get(p).cloned())
                    });
                    match found {
                        Some(rel) => {
                            holders.insert(rel);
                            true
                        }
                        None => false,
                    }
                });
            if !whole {
                continue;
            }
            self.flush_holders(&holders, &mut flushed)?;
            if self.relocate_to_packs(&rel, JournalKind::Pending, info.number, &holders)? {
                atomic_remove(
                    &*self.vfs,
                    &self.folder.path(&rel),
                    SyncDir::Deferred(&mut dirs),
                )?;
                self.journal.pending.remove(&rel);
                if self.index.sections.remove(&rel).is_some() {
                    self.index.changed = true;
                }
            }
        }
        // Packs a valid pack's `replaces` lists and whose every change it holds: a
        // consolidation that stopped before Q4 (§4.7).
        let listing: Vec<(String, PackInfo)> = self
            .journal
            .packs
            .iter()
            .filter(|(_, p)| p.valid && !p.replaces.is_empty())
            .map(|(rel, p)| (rel.clone(), p.clone()))
            .collect();
        for (by_rel, by) in listing {
            let have: BTreeSet<ChangeId> = by.ids.iter().copied().collect();
            let listed: Vec<String> = self
                .journal
                .packs
                .iter()
                .filter(|(rel, _)| **rel != by_rel)
                .filter(|(rel, _)| {
                    tc_journal::pack::PackPath::parse(rel)
                        .is_some_and(|p| by.replaces.contains(&p.name))
                })
                .map(|(rel, _)| rel.clone())
                .collect();
            for rel in listed {
                let abs = self.folder.path(&rel);
                let Ok(bytes) = self.vfs.read(&abs) else {
                    continue;
                };
                let read = read_pack(&rel, &bytes, self.id);
                if !read.entries.iter().all(|e| have.contains(&e.id())) {
                    // A pack listed by one that lacks its changes is kept (a bug, §4.7).
                    continue;
                }
                let number = self.journal.packs.get(&rel).and_then(|p| p.number);
                let holders = BTreeSet::from([by_rel.clone()]);
                self.flush_holders(&holders, &mut flushed)?;
                if self.relocate_to_packs(&rel, JournalKind::Pack, number, &holders)? {
                    atomic_remove(&*self.vfs, &abs, SyncDir::Deferred(&mut dirs))?;
                    self.journal.packs.remove(&rel);
                    self.manifest.packs.remove(&rel);
                    if self.index.sections.remove(&rel).is_some() {
                        self.index.changed = true;
                    }
                    self.dirty.metadata = true;
                }
            }
        }
        dirs.sync(&*self.vfs)?;
        // The ids of the packs that list others are not needed any more.
        for pack in self.journal.packs.values_mut() {
            pack.ids = Vec::new();
        }
        Ok(())
    }

    /// Makes the packs `holders` durable, the file and the rename that named it, before a
    /// file whose changes they hold goes: a crashed seal or consolidation may have renamed a
    /// pack into place without flushing its folder, and a power cut after the removal would
    /// then take both copies of those changes.
    fn flush_holders(
        &self,
        holders: &BTreeSet<String>,
        flushed: &mut BTreeSet<String>,
    ) -> Result<(), StoreError> {
        for holder in holders {
            if !flushed.insert(holder.clone()) {
                continue;
            }
            let abs = self.folder.path(holder);
            self.vfs
                .sync_file(&abs)
                .map_err(|e| StoreError::io("flush", &abs, e))?;
            if let Some(dir) = abs.parent() {
                self.vfs
                    .sync_dir(dir)
                    .map_err(|e| StoreError::io("flush the folder", dir, e))?;
            }
        }
        Ok(())
    }

    /// Moves the fold's values of `rel` to the packs `holders` (read again: only an
    /// interrupted seal or consolidation leaves this to do). `false` when a change of `rel`
    /// is in none of them.
    fn relocate_to_packs(
        &mut self,
        rel: &str,
        kind: JournalKind,
        number: Option<u32>,
        holders: &BTreeSet<String>,
    ) -> Result<bool, StoreError> {
        let Some(number) = number else {
            return Ok(true);
        };
        let abs = self.folder.path(rel);
        let bytes = self
            .vfs
            .read(&abs)
            .map_err(|e| StoreError::io("read", &abs, e))?;
        let mut targets: Vec<(JournalFile, u32, Vec<u8>)> = Vec::new();
        for holder in holders {
            let habs = self.folder.path(holder);
            let hbytes = self
                .vfs
                .read(&habs)
                .map_err(|e| StoreError::io("read", &habs, e))?;
            let hnumber = match self.journal.packs.get(holder).and_then(|p| p.number) {
                Some(n) => n,
                None => self.source.add(&habs),
            };
            targets.push((
                JournalFile {
                    rel: holder.clone(),
                    abs: habs,
                    kind: JournalKind::Pack,
                    size: hbytes.len() as u64,
                },
                hnumber,
                hbytes,
            ));
        }
        let from = JournalFile {
            rel: rel.to_owned(),
            abs,
            kind,
            size: bytes.len() as u64,
        };
        let refs: Vec<(&JournalFile, u32, &[u8])> = targets
            .iter()
            .map(|(f, n, b)| (f, *n, b.as_slice()))
            .collect();
        match relocation(self.id, (&from, &bytes), &refs) {
            Some(map) => {
                self.source.moved(number, map);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// A damaged pending file (§4.4): its verified lines go into a new pending file of this
    /// device (purpose `salvage`, the lines exactly as they were), and the file moves to
    /// `.tc/quarantine/`.
    fn salvage_pending(
        &mut self,
        rel: &str,
        reason: tc_journal::pack::PackError,
        dirs: &mut DirSet,
    ) -> Result<(), StoreError> {
        let abs = self.folder.path(rel);
        let Ok(bytes) = self.vfs.read(&abs) else {
            return Ok(());
        };
        let read = read_pending(&bytes, self.id);
        let salvaged = read.entries.len() as u32;
        if !read.entries.is_empty() {
            let header = PendingHeader {
                device: self.device,
                project: self.id,
                purpose: Purpose::Salvage,
                reconciles: None,
            };
            let lines: Vec<Verified> = read.entries.clone();
            let encoded =
                encode_pending(&header, &lines).map_err(|e| StoreError::Other(e.to_string()))?;
            let dir = self.folder.path(PENDING);
            atomic_write(
                &*self.vfs,
                &dir,
                &encoded.name,
                &encoded.bytes,
                SyncDir::Now,
            )
            .map_err(|e| e.into_inner())?;
            let new_rel = format!("{PENDING}/{}", encoded.name);
            let new_abs = self.folder.path(&new_rel);
            let number = self.source.add(&new_abs);
            if let Some(old) = self.journal.pending.get(rel).and_then(|p| p.number) {
                let from = JournalFile {
                    rel: rel.to_owned(),
                    abs: abs.clone(),
                    kind: JournalKind::Pending,
                    size: bytes.len() as u64,
                };
                let to = JournalFile {
                    rel: new_rel.clone(),
                    abs: new_abs,
                    kind: JournalKind::Pending,
                    size: encoded.bytes.len() as u64,
                };
                match relocation(self.id, (&from, &bytes), &[(&to, number, &encoded.bytes)]) {
                    Some(map) => self.source.moved(old, map),
                    None => return Ok(()),
                }
            }
            self.journal.pending.insert(
                new_rel,
                PendingInfo {
                    ids: lines.iter().map(Verified::id).collect(),
                    number: Some(number),
                    status: PendingStatus::Valid,
                },
            );
        }
        self.quarantine(rel, dirs)?;
        self.journal.pending.remove(rel);
        self.notice(
            Notice::QuarantinedPending {
                file: rel.into(),
                reason,
                salvaged,
            },
            None,
        );
        Ok(())
    }

    /// Moves a file to `.tc/quarantine/`, where it stays until the person dismisses the
    /// notice (§3.6).
    fn quarantine(&mut self, rel: &str, dirs: &mut DirSet) -> Result<(), StoreError> {
        let dir = self.folder.path(QUARANTINE);
        atomic_create_dirs(&*self.vfs, &dir)?;
        let name = layout::escaped(rel);
        let from = self.folder.path(rel);
        let to = dir.join(&name);
        self.vfs
            .rename(&from, &to)
            .map_err(|e| StoreError::io("move to quarantine", &from, e))?;
        dirs.add_dir(&dir);
        if let Some(parent) = from.parent() {
            dirs.add_dir(parent);
        }
        Ok(())
    }

    /// A new device's profile copies the old one's names (§6.3).
    fn copy_profile(&mut self, old: DeviceId) -> Result<(), StoreError> {
        let key = tc_journal::RecordKey::DeviceProfile(old);
        let Some(tc_journal::Record::DeviceProfile { value, .. }) = self.fold.projected(&key)
        else {
            return Ok(());
        };
        let plan = crate::commands::Plan {
            changes: vec![tc_journal::command::PlannedChange {
                record: tc_journal::Record::DeviceProfile {
                    device: self.device,
                    value,
                },
                parents: Vec::new(),
                cause: tc_journal::Cause::Edit,
            }],
            skipped: Vec::new(),
        };
        self.commit(plan)
            .map(|_| ())
            .map_err(|e| StoreError::Other(e.to_string()))
    }

    /// tC4 #766: a project that holds no pin gets the pins a new project gets. A journal
    /// file that is missing or cannot be read may hold its pins, so nothing is written then.
    fn write_creation_pins(
        &mut self,
        pins: &[(PinGroup, PinGroupValue)],
    ) -> Result<(), StoreError> {
        let holds_pin = PinGroup::ALL
            .iter()
            .any(|group| !self.fold.heads(&RecordKey::Pins(*group)).is_empty());
        if holds_pin || !self.journal.complete() {
            return Ok(());
        }
        let plan = crate::commands::Plan {
            changes: pins
                .iter()
                .map(|(group, value)| tc_journal::command::PlannedChange {
                    record: tc_journal::Record::Pins {
                        group: *group,
                        value: value.clone(),
                    },
                    parents: Vec::new(),
                    cause: tc_journal::Cause::System(tc_journal::SystemReason::CreationPins),
                })
                .collect(),
            skipped: Vec::new(),
        };
        self.commit(plan)
            .map(|_| ())
            .map_err(|e| StoreError::Other(e.to_string()))
    }

    /// O9 (§6.2): classifies every derived path, reading and projecting only what may have
    /// changed, and marks what to write. Returns the paths taken as converged without being
    /// read, for the storage check (O14).
    pub(crate) fn classify(&mut self, tracker: &Tracker) -> Result<Vec<String>, StoreError> {
        let files = self.fold.derived_files();
        tracker.phase(Phase::Checking, files.len() as u64 + 1, 0);
        let digests = self.fold.scope_digests(&files);
        let intent = self
            .vfs
            .read(&self.folder.path(INTENT))
            .ok()
            .and_then(|b| Intent::parse(&b));
        let versions = self.manifest.versions_current();
        let mut unread: Vec<String> = Vec::new();
        let mut to_project: Vec<(String, DerivedFile, Option<Vec<u8>>)> = Vec::new();
        let projected: BTreeSet<String> = files.iter().map(DerivedFile::path).collect();
        for (file, digest) in files.iter().zip(&digests) {
            let rel = file.path();
            let named = intent.as_ref().is_some_and(|i| i.names(&rel));
            let entry = self.manifest.paths.get(&rel).cloned();
            let same_scope = versions
                && entry
                    .as_ref()
                    .is_some_and(|e| e.scope.as_deref() == Some(digest.to_hex().as_str()));
            if same_scope && !named && self.clean_by_stat(&rel) {
                unread.push(rel);
                tracker.add(1, 0);
                continue;
            }
            let disk = match self.read_derived(&rel) {
                Ok(disk) => disk,
                Err(reason) => {
                    // A file another program holds, or a placeholder: neither damaged nor
                    // gone. Nothing is written over it, and it is tried again (§4.4, §6.1).
                    self.unreadable_derived(&rel, file, reason);
                    tracker.add(1, 0);
                    continue;
                }
            };
            match (&disk, &entry) {
                (Some(bytes), Some(entry))
                    if same_scope && entry.held.is_none() && md5_hex(bytes) == entry.md5 =>
                {
                    // Read, not projected: the bytes are the app's own.
                    let md5 = entry.md5.clone();
                    self.refresh_entry(&rel, &md5, bytes.len() as u64, None, None);
                }
                _ => to_project.push((rel, file.clone(), disk)),
            }
            tracker.add(1, 0);
        }
        // Paths only the manifest lists: a removal that stopped before M7, or files to go.
        let listed: Vec<String> = self
            .manifest
            .paths
            .keys()
            .filter(|p| {
                !projected.contains(*p)
                    && p.as_str() != METADATA
                    && !crate::materialize::FIXED.contains(&p.as_str())
            })
            .cloned()
            .collect();
        for rel in listed {
            let entry = self.manifest.paths.get(&rel).cloned();
            if entry.as_ref().is_some_and(|e| e.oid.is_some()) {
                continue;
            }
            match self.vfs.metadata(&self.folder.path(&rel)) {
                Err(e) if is_not_found(&e) => {
                    self.manifest.paths.remove(&rel);
                    self.manifest_changed = true;
                }
                // Still on disk: the materialization removes it (M0, X).
                _ => self.dirty.metadata = true,
            }
        }
        if !to_project.is_empty() {
            let wanted: BTreeSet<DerivedFile> =
                to_project.iter().map(|(_, f, _)| f.clone()).collect();
            let mut rendered = self.render_files(&wanted);
            for (rel, file, disk) in to_project {
                let Some(new) = rendered.remove(&rel) else {
                    continue;
                };
                self.classify_one(&rel, &file, disk.as_deref(), &new, intent.as_ref())?;
                if self.dirty.files.contains(&file) {
                    self.prerendered.insert(rel, new);
                }
            }
        }
        // metadata.json: converged when its entry is clean by stat and lists what it would.
        let scope = self.metadata_scope();
        let metadata_ok = versions
            && self
                .manifest
                .paths
                .get(METADATA)
                .is_some_and(|e| e.scope.as_deref() == Some(scope.as_str()))
            && !intent.as_ref().is_some_and(|i| i.names(METADATA))
            && (self.clean_by_stat(METADATA) || self.metadata_matches_entry());
        if !metadata_ok {
            self.dirty.metadata = true;
        }
        tracker.add(1, 0);
        if intent.is_some() && self.dirty.is_empty() {
            atomic_remove(&*self.vfs, &self.folder.path(INTENT), SyncDir::Now)?;
        }
        self.renew_hold_notices();
        Ok(unread)
    }

    /// The bytes of a derived file: `None` when it is gone, the reason when it cannot be
    /// read now.
    fn read_derived(&self, rel: &str) -> Result<Option<Vec<u8>>, String> {
        let path = self.folder.path(rel);
        match self.vfs.read(&path) {
            Ok(bytes) => {
                self.counters.add(&self.counters.derived_read, 1);
                Ok(Some(bytes))
            }
            Err(e) if is_not_found(&e) => Ok(None),
            Err(e) => Err(StoreError::io("read", &path, e).to_string()),
        }
    }

    /// A derived file the open could not read: the person is told (`FileUnreadable`), the
    /// file is left as it is, and the path stays dirty, so the next materialization reads it
    /// again (M1) and writes it when it can.
    fn unreadable_derived(&mut self, rel: &str, file: &DerivedFile, reason: String) {
        self.dirty.files.insert(file.clone());
        self.notice(
            Notice::FileUnreadable {
                path: rel.into(),
                reason: reason.as_str().into(),
            },
            None,
        );
    }

    /// Whether `metadata.json` holds what the manifest says the app wrote. A file that
    /// cannot be read now is not known to, so `metadata.json` is written again.
    fn metadata_matches_entry(&self) -> bool {
        let Some(entry) = self.manifest.paths.get(METADATA) else {
            return false;
        };
        matches!(self.read_derived(METADATA), Ok(Some(bytes)) if md5_hex(&bytes) == entry.md5)
    }

    /// Takes a path's md5, size and mtime again after a read found bytes the projection
    /// gives; the entry settles when the read began at least 2 s after its mtime (§6.2).
    /// `scope` replaces the entry's digest, and `nfc` its normalization, when given.
    fn refresh_entry(
        &mut self,
        rel: &str,
        md5: &str,
        size: u64,
        scope: Option<String>,
        nfc: Option<bool>,
    ) {
        let meta = self.vfs.metadata(&self.folder.path(rel)).ok();
        let stamp = self.stamp_ns;
        let network = self.network;
        if let Some(entry) = self.manifest.paths.get_mut(rel) {
            let mtime = meta.map_or(entry.mtime_ns, |m| m.mtime_ns);
            let settled = !network && stamp >= mtime.saturating_add(2_000_000_000);
            let changed = entry.md5 != md5
                || entry.size != size
                || entry.mtime_ns != mtime
                || entry.settled != settled
                || entry.held.is_some()
                || (scope.is_some() && entry.scope != scope)
                || (nfc.is_some() && entry.nfc != nfc);
            entry.md5 = md5.to_owned();
            entry.size = size;
            entry.mtime_ns = mtime;
            entry.settled = settled;
            let was_held = entry.held.take().is_some();
            if scope.is_some() {
                entry.scope = scope;
            }
            if nfc.is_some() {
                entry.nfc = nfc;
            }
            self.manifest_changed |= changed;
            if was_held {
                self.shared.remove_notices_of(rel);
            }
        }
    }

    /// The table of §6.2 for one path whose projection is `new`.
    fn classify_one(
        &mut self,
        rel: &str,
        file: &DerivedFile,
        disk: Option<&[u8]>,
        new: &[u8],
        intent: Option<&Intent>,
    ) -> Result<(), StoreError> {
        let e = md5_hex(new);
        let entry = self.manifest.paths.get(rel).cloned();
        let Some(disk) = disk else {
            // Missing: rebuilt, never read as a deletion (I11). A deletion ends a hold too,
            // which M0 does for the path it writes (§5.10).
            self.dirty.files.insert(file.clone());
            if entry.is_some()
                && let Ok(path) = RepoPath::parse(rel)
            {
                self.notice(Notice::MissingFileRebuilt { path }, None);
            }
            return Ok(());
        };
        let d = md5_hex(disk);
        if d == e {
            let scope = self.fold.scope_digest(file).to_hex();
            // `metadata.json` declares the normalization of the text it lists, so an entry
            // made from a read keeps what the read found.
            let nfc = crate::materialize::is_text_path(rel)
                .then(|| unicode_normalization::is_nfc(&String::from_utf8_lossy(disk)));
            match self.manifest.paths.contains_key(rel) {
                true => self.refresh_entry(rel, &d, disk.len() as u64, Some(scope), nfc),
                false => {
                    let meta = self.vfs.metadata(&self.folder.path(rel)).ok();
                    self.manifest.paths.insert(
                        rel.to_owned(),
                        Entry {
                            scope: Some(scope),
                            nfc,
                            ..Entry::of(d, disk.len() as u64, meta.map_or(0, |m| m.mtime_ns), false)
                        },
                    );
                    self.manifest_changed = true;
                }
            }
            return Ok(());
        }
        let ours = entry.as_ref().is_some_and(|en| en.md5 == d)
            || intent.is_some_and(|i| i.md5s_of(rel).contains(&d.as_str()));
        if let Some(held) = entry.as_ref().and_then(|en| en.held.as_ref()) {
            if held.md5 == d {
                return Ok(());
            }
            self.hold(rel, disk, false)?;
            return Ok(());
        }
        if ours {
            self.dirty.files.insert(file.clone());
            return Ok(());
        }
        let damage = self.clean_by_stat(rel);
        self.hold(rel, disk, damage)
    }

    /// Renders `files`, a book's files together.
    fn render_files(&self, files: &BTreeSet<DerivedFile>) -> BTreeMap<String, Vec<u8>> {
        let mut out = BTreeMap::new();
        let mut books = BTreeSet::new();
        for file in files {
            match file.book() {
                Some(book) => {
                    books.insert(book);
                }
                None => {
                    if let Some(bytes) = self.fold.render(file) {
                        self.counters.add(&self.counters.projected, 1);
                        out.insert(file.path(), bytes);
                    }
                }
            }
        }
        for book in books {
            for (file, bytes) in self.fold.render_book(book) {
                if files.contains(&file) {
                    self.counters.add(&self.counters.projected, 1);
                    out.insert(file.path(), bytes);
                }
            }
        }
        out
    }

    /// O14 results (§6.1): equal bytes settle an entry once its tick is over; different
    /// bytes are classified again on the actor, where nothing can change them meanwhile.
    pub(crate) fn apply_check(&mut self, results: Vec<CheckResult>) -> Result<(), StoreError> {
        for result in results {
            let Some(entry) = self.manifest.paths.get(&result.rel).cloned() else {
                continue;
            };
            if entry.held.is_some() {
                continue;
            }
            match &result.md5 {
                Some(md5) if *md5 == entry.md5 => {
                    let settled = !self.network
                        && entry.size == result.size
                        && entry.mtime_ns == result.mtime_ns
                        && result.stamp_ns >= result.mtime_ns.saturating_add(2_000_000_000);
                    if settled
                        && !entry.settled
                        && let Some(e) = self.manifest.paths.get_mut(&result.rel)
                    {
                        e.settled = true;
                        self.manifest_changed = true;
                    }
                }
                Some(_) => {
                    // Read again here: the file may have changed since the check read it.
                    let Some(file) = self
                        .fold
                        .derived_files()
                        .into_iter()
                        .find(|f| f.path() == result.rel)
                    else {
                        continue;
                    };
                    let Ok(disk) = self.read_derived(&result.rel) else {
                        continue;
                    };
                    let rendered = self.render_files(&BTreeSet::from([file.clone()]));
                    if let Some(new) = rendered.get(&result.rel) {
                        self.classify_one(&result.rel, &file, disk.as_deref(), new, None)?;
                    }
                }
                None => {}
            }
        }
        if self.manifest_changed {
            self.write_manifest()?;
        }
        Ok(())
    }

    /// The fold cache's bytes (§3.6) if its sections changed since it was last written.
    pub(crate) fn index_bytes(&mut self) -> Option<Vec<u8>> {
        if !self.index.changed {
            return None;
        }
        self.index.changed = false;
        Some(Index::encode(self.id, self.index.sections.values()))
    }

    /// The sections to encode off the actor, if they changed: sections share their bytes,
    /// so this copies no change.
    pub(crate) fn index_bytes_later(&mut self) -> Option<Vec<tc_journal::index::Section>> {
        if !self.index.changed {
            return None;
        }
        self.index.changed = false;
        Some(self.index.sections.values().cloned().collect())
    }
}

/// O14's reads, off the actor: each path's md5, size and mtime, and the newest time stamp
/// before the read began.
pub(crate) fn storage_check(
    vfs: &dyn Vfs,
    counters: &Counters,
    folder: &Folder,
    paths: &[String],
    stamp_ns: i64,
) -> Vec<CheckResult> {
    let stamp = vfs
        .metadata(&folder.path(MANIFEST))
        .map_or(stamp_ns, |m| m.mtime_ns.max(stamp_ns));
    paths
        .iter()
        .map(|rel| {
            let path = folder.path(rel);
            let meta = vfs.metadata(&path).ok();
            let md5 = vfs.read(&path).ok().map(|b| {
                counters.add(&counters.check_read, 1);
                md5_hex(&b)
            });
            CheckResult {
                rel: rel.clone(),
                md5,
                size: meta.map_or(0, |m| m.len),
                mtime_ns: meta.map_or(0, |m| m.mtime_ns),
                stamp_ns: stamp,
            }
        })
        .collect()
}

/// The paths O14 reads besides those O9 took unread: every entry not yet settled.
pub(crate) fn unsettled(manifest: &Manifest) -> Vec<String> {
    manifest
        .paths
        .iter()
        .filter(|(_, e)| !e.settled && e.held.is_none() && e.oid.is_none())
        .map(|(p, _)| p.clone())
        .collect()
}
