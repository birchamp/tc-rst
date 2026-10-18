//! The writer of an open project (`docs/STORAGE.md` §5.0 "One writer per project", §5.10).
//!
//! The actor commits batches and seals; this thread owns everything about derived files: the
//! manifest, the holds, the busy paths, the byte copies in `.tc/external/`, the heads files,
//! the materialize intent and `metadata.json`. A materialization runs here, from a snapshot
//! the actor published, so a save never waits for a derived file, however long a book takes
//! or however slowly a scanner lets go of a file (owner ruling 2026-10-18).
//!
//! Message passing only. The actor sends [`Job`]s and never waits for one but at close; the
//! writer sends the actor a [`Msg`] when the open's first job fails. A job runs with the
//! snapshot it was sent, or a newer one when the writer has already run newer ones: a job
//! never writes older bytes over newer.

use crate::atomic::{SyncDir, atomic_remove};
use crate::clock::Clock;
use crate::counters::Counters;
use crate::error::StoreError;
use crate::handle::{Msg, Resolver, Shared, Snapshot, StoreEvent};
use crate::hooks::Point;
use crate::layout::Folder;
use crate::manifest::Manifest;
use crate::materialize::Busy;
use crate::open::CheckResult;
use crate::project::Dirty;
use crate::reply::ReadOnlyReason;
use crate::vfs::Vfs;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use tc_journal::fold::{FoldState, Notice, ProjectKind};

/// What `metadata.json` and the manifest need of a pack the actor knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PackFacts {
    pub size: u64,
    pub fingerprint: u64,
}

/// Why a materialization runs (§5.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    /// The open's first job (O10).
    Open,
    /// The 3 s after the last batch.
    Timer,
    /// The person left a book or a mode.
    Leave,
    /// A checkpoint: a seal ran first.
    Checkpoint,
}

/// What the actor sends the writer.
pub(crate) struct MaterializeJob {
    /// The fold to render from.
    pub snapshot: Snapshot,
    /// What the actor marked dirty since the last job.
    pub dirty: Dirty,
    /// The packs the actor knows, with their size and fingerprint.
    pub packs: BTreeMap<String, PackFacts>,
    /// No expected pack is missing and no journal file is unreadable (§5.10 M0).
    pub complete: bool,
    pub origin: Origin,
    /// A checkpoint's seal result, which the reply carries before the materialization's.
    pub sealed: Result<(), StoreError>,
    pub reply: Option<Resolver<Result<(), StoreError>>>,
}

pub(crate) enum Job {
    Materialize(Box<MaterializeJob>),
    /// O14's results (§6.1).
    StorageCheck(Vec<CheckResult>),
    /// *Restore from the project* for a held path (§6.2).
    RestoreHeld {
        rel: String,
        reply: Resolver<Result<(), StoreError>>,
    },
    DismissNotice {
        id: u64,
        reply: Resolver<Result<(), StoreError>>,
    },
    /// Resolves when every job sent before it has finished.
    Sync {
        reply: Resolver<()>,
    },
    /// Resolves, and the thread ends.
    Close {
        reply: Resolver<()>,
    },
}

/// See the module documentation.
pub(crate) struct Writer {
    pub vfs: Arc<dyn Vfs>,
    pub counters: Arc<Counters>,
    pub folder: Folder,
    pub kind: ProjectKind,
    pub shared: Arc<Shared>,
    pub clock: Arc<dyn Clock>,
    /// On a network drive no entry is clean by stat (§6.2).
    pub network: bool,
    pub manifest: Manifest,
    /// Manifest entries were refreshed or settled since it was last written.
    pub manifest_changed: bool,
    /// The newest time stamp (§6.2): `.tc/lock` at O1, `materialized.json` at each M7.
    pub stamp_ns: i64,
    /// Derived paths another program refused the app (§5.10), by path.
    pub busy: BTreeMap<String, Busy>,
    /// The md5 of each outside version of `metadata.json` already copied to `.tc/external/`
    /// this session, so a blocked write that is tried again copies it once.
    pub copied_outside: BTreeSet<String>,
    /// The fold the job runs on, and its sequence number.
    pub fold: FoldState,
    pub seq: u64,
    pub packs: BTreeMap<String, PackFacts>,
    pub complete: bool,
    /// What to write: the actor's marks of the job, the open's classification, and the paths
    /// the last run could not write.
    pub dirty: Dirty,
    /// Bytes O9 projected to classify a path, kept for the materialization that follows it
    /// with the same fold, so the open projects each file once.
    pub prerendered: BTreeMap<String, Vec<u8>>,
    /// The sequence number of the fold they were projected from.
    pub prerendered_seq: u64,
    pub actor: Sender<Msg>,
}

impl Writer {
    /// A notice for the person, typed (R16).
    pub(crate) fn notice(&self, notice: Notice, copy: Option<String>) {
        let notice = self.shared.add_notice(notice, copy);
        self.shared.emit(StoreEvent::Notice(notice));
    }

    fn read_only(&self) -> Option<ReadOnlyReason> {
        match self.shared.facts().mode {
            crate::handle::Mode::ReadOnly(reason) => Some(reason),
            crate::handle::Mode::Writable => None,
        }
    }

    /// Takes the view of a job: the fold it renders from and the packs the actor knows. The
    /// writer never goes back to an older fold than one it has run on.
    fn begin(&mut self, snapshot: &Snapshot, packs: BTreeMap<String, PackFacts>, complete: bool) {
        let newest;
        let snapshot = if snapshot.seq() < self.seq {
            newest = self.shared.snapshot();
            &newest
        } else {
            snapshot
        };
        self.fold = snapshot.fold().clone();
        self.seq = snapshot.seq();
        self.packs = packs;
        self.complete = complete;
    }

    /// Runs a materialization job.
    fn run(&mut self, job: MaterializeJob) {
        let MaterializeJob {
            snapshot,
            dirty,
            packs,
            complete,
            origin,
            sealed,
            reply,
        } = job;
        self.begin(&snapshot, packs, complete);
        self.dirty.absorb(dirty);
        if origin == Origin::Checkpoint {
            self.shared.hooks.wait(Point::BeforeCheckpoint);
        }
        let result = self.materialize_now();
        if origin == Origin::Open
            && let Err(e) = &result
        {
            // Every write of the open is best effort (§6.1): the project opens read-only,
            // and the actor lets go of the lock.
            let reason = e.to_string();
            self.shared
                .set_mode(crate::handle::Mode::ReadOnly(ReadOnlyReason::WriteFailed(
                    reason.clone(),
                )));
            let _ = self.actor.send(Msg::WritesFailed { reason });
        }
        if let Some(reply) = reply {
            reply.resolve(sealed.and(result));
        }
    }

    /// §5.10 for what is dirty. A path another program refused, or a write that failed,
    /// stays dirty and is tried again by [`Writer::retry`].
    fn materialize_now(&mut self) -> Result<(), StoreError> {
        if self.read_only().is_some() {
            self.dirty = Dirty::default();
            return Ok(());
        }
        self.materialize().map(|_| ())
    }

    /// The writer's own retry of the paths it could not write, from the newest snapshot.
    fn retry(&mut self) {
        if self.dirty.is_empty() {
            return;
        }
        let snapshot = self.shared.snapshot();
        let packs = self.packs.clone();
        let complete = self.complete;
        self.begin(&snapshot, packs, complete);
        let _ = self.materialize_now();
    }

    /// O14's results: equal bytes settle an entry, different bytes are classified again
    /// against the newest snapshot (the compare-and-swap of the write that follows repeats
    /// the check at the moment that matters).
    fn storage_check(&mut self, results: Vec<CheckResult>) {
        if self.read_only().is_some() {
            return;
        }
        let snapshot = self.shared.snapshot();
        let packs = self.packs.clone();
        let complete = self.complete;
        self.begin(&snapshot, packs, complete);
        let was_dirty = !self.dirty.is_empty();
        if let Err(e) = self.apply_check(results) {
            self.notice(
                Notice::FileUnreadable {
                    path: ".tc/materialized.json".into(),
                    reason: e.to_string().into(),
                },
                None,
            );
        }
        // What the check found is written now, within this job, so `wait_idle` covers it.
        if !was_dirty && !self.dirty.is_empty() {
            let _ = self.materialize_now();
        }
    }

    fn dismiss(&mut self, id: u64) -> Result<(), StoreError> {
        let Some(notice) = self.shared.remove_notice(id) else {
            return Ok(());
        };
        if self.read_only().is_some() {
            return Ok(());
        }
        // The copy of a hold that stands is the only other trace of the person's bytes, and
        // the notice comes back at the next open (§6.2): dismissing it hides it for this
        // session and keeps the copy.
        let held = self.manifest.paths.values().any(|e| {
            e.held
                .as_ref()
                .is_some_and(|h| Some(&h.copy) == notice.copy.as_ref())
        });
        if held {
            return Ok(());
        }
        if let Some(copy) = notice.copy {
            let path = self.folder.path(&copy);
            if self.vfs.metadata(&path).is_ok() {
                atomic_remove(&*self.vfs, &path, SyncDir::Now)?;
            }
        }
        Ok(())
    }

    /// *Restore from the project* for a held path (§6.2): the file is written from the
    /// fold, with the held bytes as the ones the compare-and-swap expects.
    fn restore_held(&mut self, rel: &str) -> Result<(), StoreError> {
        if let Some(reason) = self.read_only() {
            return Err(StoreError::Other(format!(
                "the project is read-only: {reason:?}"
            )));
        }
        let snapshot = self.shared.snapshot();
        let packs = self.packs.clone();
        let complete = self.complete;
        self.begin(&snapshot, packs, complete);
        let Some(file) = self
            .fold
            .derived_files()
            .into_iter()
            .find(|f| f.path() == rel)
        else {
            return Ok(());
        };
        let held = self
            .manifest
            .paths
            .get(rel)
            .and_then(|e| e.held.as_ref().map(|h| h.md5.clone()));
        if let Some(held) = held {
            self.end_hold(rel);
            if let Some(entry) = self.manifest.paths.get_mut(rel) {
                entry.md5 = held;
            }
        }
        self.dirty.files.insert(file);
        self.materialize_now()
    }

    fn handle(&mut self, job: Job) -> bool {
        match job {
            Job::Materialize(job) => self.run(*job),
            Job::StorageCheck(results) => self.storage_check(results),
            Job::RestoreHeld { rel, reply } => {
                let result = self.restore_held(&rel);
                reply.resolve(result);
            }
            Job::DismissNotice { id, reply } => {
                let result = self.dismiss(id);
                reply.resolve(result);
            }
            Job::Sync { reply } => reply.resolve(()),
            Job::Close { reply } => {
                reply.resolve(());
                return false;
            }
        }
        true
    }

    fn work(mut self, rx: Receiver<Job>) {
        loop {
            let received = match self.retry_delay() {
                Some(delay) => rx.recv_timeout(delay),
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            match received {
                Ok(job) => {
                    if !self.handle(job) {
                        return;
                    }
                }
                Err(RecvTimeoutError::Timeout) => self.retry(),
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}

/// Starts the writer thread.
pub(crate) fn spawn(writer: Writer, rx: Receiver<Job>) -> Option<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("tc-store writer".into())
        .spawn(move || writer.work(rx))
        .ok()
}
