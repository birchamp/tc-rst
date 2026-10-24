//! A parity scenario's projects on disk, through tc-store (APP-DESIGN §4.2, unit C1a; feature
//! `fixture-backend`): what `tc4 serve --scenario` runs on, and what `test/reset` puts back.
//!
//! - **The seed's own packs, through tc-store's staging and placing.** Each project is seeded
//!   with B1a's `tc4_state_seed` (refused projects are written nowhere, AD7) and written as an
//!   import is: a `Store::staging` folder (which waits for the start cleanup, tC4 #746), the
//!   fixed files, the packs the seed sealed (AD36: the same changes, the same device and the
//!   same project id), the carried files, then `Store::place` and one open at the final path,
//!   which materializes every derived file. `Store::create` would give the project a random id
//!   and write its records at the run's time as this computer's changes, so the screens, the
//!   decisions' `modifiedTimestamp` and STORAGE §13.2's check would no longer be tC4's.
//! - **`project.json`** is the one file whose writer tc-store keeps private (R29): this module
//!   writes its four fields in tc-store's order, which C0's listing and tc-store's open read.
//! - **Every file** goes through `tc_store::atomic_write` on the store's own file system, so
//!   the tests run on `FaultFs`.
//! - **A scenario never writes into, or resets, a folder that holds anyone's work**: the
//!   projects folder must be empty, and a reset removes only entries of that folder.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use tc_api::ids::ProjectRef;
use tc_journal::fold::{FoldState, ProjectKind};
use tc_journal::pack::{EncodedPack, PackSpec, encode_pack};
use tc_journal::{BookId, Change, ChangeId, ProjectId, RecordKey, Verified, WallTime};
use tc_store::{SyncDir, Vfs, atomic_create_dirs, atomic_write};

use super::memory::MemoryError;
use super::memory::teammates::Teammates;
use super::scenario::{ScenarioError, ScenarioReport, ScenarioState};
use super::seed::{StateSeed, fixture_project_id, tc4_state_seed};
use super::store::StoreBackend;
use super::{
    Listing, OpenFailure, ProjectHandle, ProjectPeek, ProjectStore, StoreError, SummaryFiles,
    SummaryWatch,
};
use crate::jobs::ProgressSink;

/// The prefix of tc-store's staging folders (its private `STAGING`).
#[cfg(feature = "test-hooks")]
const STAGING: &str = ".staging-";

/// A project as the scenario wrote it, so a reset can write it again.
#[derive(Debug, Clone)]
struct WrittenProject {
    folder: ProjectRef,
    id: ProjectId,
    kind: ProjectKind,
    created: WallTime,
    packs: Vec<EncodedPack>,
    /// Carried files: the path in the folder and the bytes.
    carried: Vec<(String, Vec<u8>)>,
    /// The Door43 repository a shared project syncs with (the scenario's `origin`).
    remote: Option<super::Door43Remote>,
}

impl WrittenProject {
    fn of(
        folder: ProjectRef,
        seed: &StateSeed,
        remote: Option<super::Door43Remote>,
    ) -> Result<Self, ScenarioError> {
        let mut carried = Vec::new();
        for placement in seed.fold.carried_files() {
            let bytes = seed
                .carried
                .get(&placement.oid)
                .ok_or_else(|| ScenarioError::Write {
                    folder: folder.to_string(),
                    problem: format!("the seed holds no bytes for {}", placement.path),
                })?;
            carried.push((placement.path, bytes.clone()));
        }
        Ok(Self {
            folder,
            id: seed.project,
            kind: seed.kind,
            created: seed.created,
            packs: seed.packs.clone(),
            carried,
            remote,
        })
    }

    /// `ingredients/tc/project.json`: the bytes `Store::create` writes for the same values.
    fn project_json(&self) -> Vec<u8> {
        let kind = match self.kind {
            ProjectKind::Bible => "bible",
            ProjectKind::Stories => "obs",
        };
        let mut out = format!(
            "{{\"schemaVersion\":1,\"projectId\":\"{}\",\"kind\":\"{kind}\",\"created\":\"{}\"}}",
            self.id,
            tc_store::appdata::rfc3339(self.created)
        )
        .into_bytes();
        out.push(b'\n');
        out
    }

    /// Every file the folder starts with, by its `/`-separated path.
    fn files(&self) -> Vec<(String, Vec<u8>)> {
        use tc_store::layout::{
            FORMAT_JSON, GITATTRIBUTES, GITATTRIBUTES_BYTES, GITIGNORE, GITIGNORE_BYTES,
            PROJECT_JSON,
        };
        let mut files = vec![
            (GITIGNORE.to_owned(), GITIGNORE_BYTES.to_vec()),
            (GITATTRIBUTES.to_owned(), GITATTRIBUTES_BYTES.to_vec()),
            (
                FORMAT_JSON.to_owned(),
                tc_journal::format::FormatDoc::current().to_bytes(),
            ),
            (PROJECT_JSON.to_owned(), self.project_json()),
        ];
        files.extend(
            self.packs
                .iter()
                .map(|pack| (pack.path.to_string(), pack.bytes.clone())),
        );
        files.extend(self.carried.iter().cloned());
        // A shared project's last confirmed Door43 state (STORAGE §3.2): its remote and branch,
        // which Home reads for "On Door43" and the Sync button. No sync has confirmed a commit.
        if let Some(remote) = &self.remote {
            let sync = serde_json::json!({
                "remoteUrl": format!("https://{}/{}/{}.git", remote.host, remote.owner, remote.repo),
                "branch": "main",
            });
            files.push((".tc/sync.json".to_owned(), format!("{sync}\n").into_bytes()));
        }
        files
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    // Every change under these locks is a store of a whole value.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `rel`, a `/`-separated path, under `root`, built by components so Windows gets its own
/// separators.
fn under(root: &Path, rel: &str) -> PathBuf {
    rel.split('/')
        .fold(root.to_owned(), |path, part| path.join(part))
}

/// Writes `bytes` at `rel` under `root`, atomically and durably.
fn put(vfs: &dyn Vfs, root: &Path, rel: &str, bytes: &[u8]) -> Result<(), String> {
    let path = under(root, rel);
    let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else {
        return Err(format!("{rel} is not a file path"));
    };
    atomic_create_dirs(vfs, dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    atomic_write(vfs, dir, name, bytes, SyncDir::Now)
        .map_err(|e| format!("{rel}: {}", e.into_inner()))?;
    Ok(())
}

/// Every file under `root` but `.tc/`, by its `/`-separated path.
fn files_under(vfs: &dyn Vfs, root: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    use tc_store::vfs::EntryKind;
    let mut out = BTreeMap::new();
    let mut dirs = vec![(root.to_owned(), String::new())];
    while let Some((dir, prefix)) = dirs.pop() {
        let entries = vfs
            .read_dir(&dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries {
            let name = entry.name.to_string_lossy();
            let rel = format!("{prefix}{name}");
            match entry.kind {
                EntryKind::Dir if rel == tc_store::layout::TC => {}
                EntryKind::Dir => dirs.push((dir.join(&entry.name), format!("{rel}/"))),
                EntryKind::File => {
                    let bytes = vfs
                        .read(&dir.join(&entry.name))
                        .map_err(|e| format!("{rel}: {e}"))?;
                    out.insert(rel, bytes);
                }
                EntryKind::Other => return Err(format!("{rel} is neither a file nor a folder")),
            }
        }
    }
    Ok(out)
}

/// A parity scenario's projects on disk (see the module documentation).
pub struct ScenarioStore {
    backend: StoreBackend,
    written: Vec<WrittenProject>,
    /// The written projects opened, or given teammates, since they were written: what a reset
    /// writes again.
    touched: Mutex<BTreeSet<ProjectRef>>,
    teammates: Mutex<BTreeMap<ProjectRef, Teammates>>,
    /// Door43's copy of each project: as the scenario wrote it, then as each scripted sync
    /// left it. Teammates edit from it, as they edit what they pulled, so a change this
    /// computer has not synced yet becomes a version of its own beside theirs.
    door43: Mutex<BTreeMap<ProjectRef, FoldState>>,
}

impl std::fmt::Debug for ScenarioStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScenarioStore")
            .field("projects", &self.backend.projects())
            .finish_non_exhaustive()
    }
}

impl ScenarioStore {
    /// Seeds every project of `state` (B1a's check) and writes those that pass into the
    /// projects folder of `backend`, which must be empty; the others are reported and written
    /// nowhere. It waits for tc-store's start cleanup first, so the cleanup never removes a
    /// folder this writes.
    pub fn write(
        backend: StoreBackend,
        state: &ScenarioState,
    ) -> Result<(Self, ScenarioReport), ScenarioError> {
        backend.store().wait_cleanup();
        let projects = backend.projects().to_owned();
        let entries = backend
            .vfs()
            .read_dir(&projects)
            .map_err(|e| ScenarioError::Io {
                path: projects.display().to_string(),
                problem: e.to_string(),
            })?;
        if !entries.is_empty() {
            return Err(ScenarioError::ProjectsFolderNotEmpty {
                path: projects.display().to_string(),
            });
        }
        let mut report = ScenarioReport {
            loaded: Vec::new(),
            refused: Vec::new(),
        };
        let mut written = Vec::new();
        let mut door43 = BTreeMap::new();
        for project in state.projects()? {
            let seeded = project
                .tc4()
                .and_then(|tc4| tc4_state_seed(&tc4, fixture_project_id(project.folder.as_str())));
            match seeded {
                Ok(seed) => {
                    let one =
                        WrittenProject::of(project.folder.clone(), &seed, project.remote.clone())?;
                    door43.insert(one.folder.clone(), write_project(&backend, &one)?);
                    written.push(one);
                    report.loaded.push((project.folder, seed.report));
                }
                Err(refusal) => report.refused.push((project.folder, refusal)),
            }
        }
        Ok((
            Self {
                backend,
                written,
                touched: Mutex::default(),
                teammates: Mutex::default(),
                door43: Mutex::new(door43),
            },
            report,
        ))
    }

    pub fn backend(&self) -> &StoreBackend {
        &self.backend
    }

    /// Teammates' edits of `key` (AD36, B10): one change per text, each by its own teammate,
    /// all from the record's heads in Door43's copy, written into the project as
    /// [`Self::teammates_write`] does.
    pub fn teammates_edit_verse(
        &self,
        folder: &ProjectRef,
        key: &RecordKey,
        texts: &[&str],
    ) -> Result<Vec<ChangeId>, ScenarioError> {
        self.teammates_write(folder, |next, base| next.edit_verse(base, key, texts))
    }

    /// Teammate 1's join of the verses `first` to `last` (C7a), one batch from the verses'
    /// heads in Door43's copy, written into the project as [`Self::teammates_write`] does.
    pub fn teammates_join_verses(
        &self,
        folder: &ProjectRef,
        book: BookId,
        chapter: tc_journal::ChapterNo,
        first: tc_journal::VerseNo,
        last: tc_journal::VerseNo,
    ) -> Result<Vec<ChangeId>, ScenarioError> {
        self.teammates_write(folder, |next, base| {
            next.join_verses(base, book, chapter, first, last)
        })
    }

    /// Teammate `n` signed in to Door43 as `username` (STORAGE §8.5), written into the project
    /// as [`Self::teammates_write`] does.
    pub fn teammates_sign_in(
        &self,
        folder: &ProjectRef,
        n: u8,
        username: &str,
    ) -> Result<Vec<ChangeId>, ScenarioError> {
        self.teammates_write(folder, |next, base| {
            next.sign_in(base, n, username).map(|change| vec![change])
        })
    }

    /// Teammates' changes made by `make` over Door43's copy of `folder`'s project, each batch
    /// sealed into a pack by the teammate's own device and written into the project while no
    /// session holds it. A project a session holds is refused and nothing is written: S1's
    /// only intake while a project is open is a second writer, which would turn it read-only.
    fn teammates_write(
        &self,
        folder: &ProjectRef,
        make: impl FnOnce(&mut Teammates, &FoldState) -> Result<Vec<Change>, MemoryError>,
    ) -> Result<Vec<ChangeId>, ScenarioError> {
        let path = self.backend.projects().join(folder.as_str());
        let handle = self
            .backend
            .store()
            .open(&path, &tc_store::OpenOptions::default())
            .map_err(|e| ScenarioError::Open {
                folder: folder.to_string(),
                problem: e.to_string(),
            })?;
        let fold: FoldState = handle.snapshot().fold().clone();
        let project = handle.facts().id;
        handle.wait_idle();
        handle.close().map_err(|e| ScenarioError::Open {
            folder: folder.to_string(),
            problem: e.to_string(),
        })?;
        let base = lock(&self.door43).get(folder).cloned().unwrap_or(fold);
        let mut teammates = lock(&self.teammates);
        let mut next = teammates.get(folder).cloned().unwrap_or_default();
        let changes =
            make(&mut next, &base).map_err(|e| ScenarioError::Teammates(e.to_string()))?;
        let vfs = self.backend.vfs();
        lock(&self.touched).insert(folder.clone());
        let ids: Vec<ChangeId> = changes.iter().map(Change::id).collect();
        // A batch stays whole in one pack, as a teammate's sealer keeps it.
        let mut packs: Vec<(PackSpec, Vec<Verified>)> = Vec::new();
        for change in changes {
            let spec = PackSpec {
                project,
                sealer: change.body().device,
                sealed: change.body().at,
            };
            let batch = change.body().batch.id;
            match packs.last_mut() {
                Some((last, verified))
                    if *last == spec
                        && verified.last().is_some_and(|v| batch_of(v) == Some(batch)) =>
                {
                    verified.push(Verified::from(change));
                }
                _ => packs.push((spec, vec![Verified::from(change)])),
            }
        }
        for (spec, verified) in packs {
            let pack = encode_pack(&spec, &verified, &[])
                .map_err(|e| ScenarioError::Teammates(e.to_string()))?;
            put(&**vfs, &path, &pack.path.to_string(), &pack.bytes).map_err(|problem| {
                ScenarioError::Write {
                    folder: folder.to_string(),
                    problem,
                }
            })?;
        }
        teammates.insert(folder.clone(), next);
        Ok(ids)
    }
}

/// The batch of a verified change, for keeping a batch in one pack.
fn batch_of(verified: &Verified) -> Option<tc_journal::BatchId> {
    match verified {
        Verified::Change(change) => Some(change.body().batch.id),
        _ => None,
    }
}

/// Writes one project into the projects folder: staged, placed, opened once and validated.
/// Answers the fold the open left, which Door43's copy starts as.
fn write_project(
    backend: &StoreBackend,
    project: &WrittenProject,
) -> Result<FoldState, ScenarioError> {
    let store = backend.store();
    let vfs = backend.vfs();
    let folder = project.folder.to_string();
    let failed = |problem: String| ScenarioError::Write {
        folder: folder.clone(),
        problem,
    };
    let staging = store.staging(tc_store::StagingKind::Import);
    let filled = vfs
        .create_dir(&staging)
        .map_err(|e| format!("{}: {e}", staging.display()))
        .and_then(|()| {
            project
                .files()
                .iter()
                .try_for_each(|(rel, bytes)| put(&**vfs, &staging, rel, bytes))
        });
    if let Err(problem) = filled {
        let _ = vfs.remove_dir_all(&staging);
        return Err(failed(problem));
    }
    let path = store
        .place(&staging, project.folder.as_str())
        .map_err(|e| failed(e.to_string()))?;
    // The first open materializes every derived file and mints this computer's device; the
    // checkpoint seals and consolidates as an import's first open does.
    let handle = store
        .open(&path, &tc_store::OpenOptions::default())
        .map_err(|e| failed(e.to_string()))?;
    if let tc_store::Mode::ReadOnly(reason) = handle.facts().mode {
        let _ = handle.close();
        return Err(ScenarioError::ReadOnly {
            folder,
            reason: format!("{reason:?}"),
        });
    }
    let checkpoint = handle
        .checkpoint(tc_store::CheckpointReason::AfterImport)
        .wait();
    handle.wait_idle();
    let fold = handle.snapshot().fold().clone();
    let closed = handle.close();
    checkpoint.map_err(|e| failed(e.to_string()))?;
    closed.map_err(|e| failed(e.to_string()))?;
    // A Scripture Burrito (P5): B1a's check compares the projection with tC4's files, not
    // `metadata.json` with the folder.
    let files = files_under(&**vfs, &path).map_err(failed)?;
    let metadata = files
        .get(tc_store::layout::METADATA)
        .ok_or_else(|| failed("no metadata.json after the first open".to_owned()))?;
    let mut burrito = tc_burrito::MemProject::new();
    for (rel, bytes) in &files {
        burrito.insert(rel.clone(), bytes.clone());
    }
    let problems = tc_burrito::validate_project(metadata, &burrito);
    if !problems.is_empty() {
        return Err(ScenarioError::NotABurrito {
            folder,
            problems: problems.iter().map(|p| format!("{p:?}")).collect(),
        });
    }
    Ok(fold)
}

impl ProjectStore for ScenarioStore {
    fn list(&self) -> Result<Listing, StoreError> {
        self.backend.list()
    }

    fn peek(&self, folder: &ProjectRef) -> Result<ProjectPeek, StoreError> {
        self.backend.peek(folder)
    }

    fn summary_files(
        &self,
        folder: &ProjectRef,
        books: &[tc_journal::BookId],
        watch: &SummaryWatch,
    ) -> Result<SummaryFiles, StoreError> {
        self.backend.summary_files(folder, books, watch)
    }

    fn derived_file(
        &self,
        folder: &ProjectRef,
        file: &tc_journal::fold::DerivedFile,
    ) -> Result<Option<Vec<u8>>, StoreError> {
        self.backend.derived_file(folder, file)
    }

    fn open(
        &self,
        folder: &ProjectRef,
        progress: ProgressSink,
    ) -> Result<Arc<dyn ProjectHandle>, OpenFailure> {
        // Marked before the open: an open that fails half way may still have written.
        if self.written.iter().any(|p| &p.folder == folder) {
            lock(&self.touched).insert(folder.clone());
        }
        self.backend.open(folder, progress)
    }

    fn door43_join(
        self: Arc<Self>,
        client: tc_door43::Door43Client,
        session: Option<tc_door43::Session>,
        repo: tc_door43::RepoRef,
        name: String,
    ) -> super::BoxFuture<Option<Result<std::path::PathBuf, tc_sync::JoinError>>> {
        super::store::join_on(self, |s| s.backend.store(), client, session, repo, name)
    }

    fn copy(&self, folder: &ProjectRef, name: &ProjectRef) -> Result<(), super::CopyFailure> {
        self.backend.copy(folder, name)
    }

    fn create(&self, new: super::NewProjectSpec) -> Result<(), super::CreateFailure> {
        self.backend.create(new)
    }

    fn trust_lock(&self, folder: &ProjectRef) {
        self.backend.trust_lock(folder);
    }

    fn name_run(&self, run: tc_api::ids::ServerInstance) {
        self.backend.name_run(run);
    }

    #[cfg(feature = "test-hooks")]
    fn teammates_edit_verse(
        &self,
        folder: &ProjectRef,
        key: &RecordKey,
        texts: &[&str],
    ) -> Result<Vec<ChangeId>, StoreError> {
        ScenarioStore::teammates_edit_verse(self, folder, key, texts)
            .map_err(|e| StoreError::Other(e.to_string()))
    }

    #[cfg(feature = "test-hooks")]
    fn teammates_join_verses(
        &self,
        folder: &ProjectRef,
        book: BookId,
        chapter: tc_journal::ChapterNo,
        first: tc_journal::VerseNo,
        last: tc_journal::VerseNo,
    ) -> Result<Vec<ChangeId>, StoreError> {
        ScenarioStore::teammates_join_verses(self, folder, book, chapter, first, last)
            .map_err(|e| StoreError::Other(e.to_string()))
    }

    #[cfg(feature = "test-hooks")]
    fn synced(&self, folder: &ProjectRef, fold: FoldState) {
        lock(&self.door43).insert(folder.clone(), fold);
    }

    /// Every written project opened or given teammates since it was written is written
    /// again, and every other entry of the projects folder (a project a journey created) is
    /// removed. The registry has closed every session, and each close waited for the store's
    /// background work, so nothing writes into a folder this removes.
    #[cfg(feature = "test-hooks")]
    fn reset(&self) -> Result<(), StoreError> {
        let vfs = self.backend.vfs();
        let projects = self.backend.projects();
        let other = |e: String| StoreError::Other(e);
        let entries = vfs
            .read_dir(projects)
            .map_err(|e| other(format!("{}: {e}", projects.display())))?;
        let touched = lock(&self.touched).clone();
        for entry in entries {
            let name = entry.name.to_string_lossy();
            if name.starts_with(STAGING) {
                continue;
            }
            let ours = self.written.iter().any(|p| p.folder.as_str() == name);
            if !ours {
                let path = projects.join(&entry.name);
                // On a link this removes the link, never what it points to.
                let removed = match entry.kind {
                    tc_store::vfs::EntryKind::File => vfs.remove_file(&path),
                    _ => vfs.remove_dir_all(&path),
                };
                removed.map_err(|e| other(format!("{}: {e}", path.display())))?;
            }
        }
        for project in &self.written {
            let path = projects.join(project.folder.as_str());
            let gone = vfs.metadata(&path).is_err();
            if !gone && !touched.contains(&project.folder) {
                continue;
            }
            if !gone {
                vfs.remove_dir_all(&path)
                    .map_err(|e| other(format!("{}: {e}", path.display())))?;
            }
            let fold = write_project(&self.backend, project).map_err(|e| other(e.to_string()))?;
            lock(&self.door43).insert(project.folder.clone(), fold);
        }
        lock(&self.touched).clear();
        lock(&self.teammates).clear();
        Ok(())
    }

    #[cfg(feature = "test-hooks")]
    fn store_hooks(&self) -> Option<Arc<tc_store::Hooks>> {
        self.backend.store_hooks()
    }
}
