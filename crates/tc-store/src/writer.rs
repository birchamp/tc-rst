//! The writer of an open project (`docs/STORAGE.md` §5.0 "One writer per project", §5.10).
//!
//! The actor commits batches and seals; this thread owns everything about derived files: the
//! manifest, the holds, the busy paths, the byte copies in `.tc/external/`, the heads files,
//! the placement ledger and `metadata.json`. A materialization runs here, from a snapshot
//! the actor published, so a save never waits for a derived file, however long a book takes
//! or however slowly a scanner lets go of a file (owner ruling 2026-10-18).
//!
//! Message passing only. The actor sends [`Job`]s and never waits for one but at close; the
//! writer sends the actor a [`Msg`] when the open's first job fails. A job runs with the
//! snapshot it was sent, or a newer one when the writer has already run newer ones: a job
//! never writes older bytes over newer. A checkpoint's job runs on exactly the snapshot its
//! packs stand for, and leaves alone the files a newer snapshot wrote.

use crate::atomic::{HeldRetry, Removed, SyncDir, atomic_remove_checked};
use crate::clock::Clock;
use crate::counters::Counters;
use crate::error::StoreError;
use crate::handle::{
    CheckpointReason, CheckpointReport, Msg, Resolver, Shared, Snapshot, StoreEvent,
};
use crate::hooks::Point;
use crate::layout::Folder;
use crate::manifest::Manifest;
use crate::materialize::Busy;
use crate::notices::CopyIdentity;
use crate::open::CheckResult;
use crate::outside::Answered;
use crate::project::Dirty;
use crate::reply::ReadOnlyReason;
use crate::vfs::Vfs;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use tc_journal::fold::{Blocked, DerivedFile, FoldState, Notice, ProjectKind};

/// What `metadata.json` and the manifest need of a pack the actor knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PackFacts {
    pub size: u64,
    pub fingerprint: u64,
    /// It passed P2 to P4.
    pub valid: bool,
    /// The packs its header says it replaces (§4.7): redundant in a tree.
    pub replaces: Vec<tc_journal::PackName>,
    /// The blob that holds its bytes, when the actor stored it at the seal.
    pub blob: Option<tc_journal::GitOid>,
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
    /// Why it is not complete: the expected packs that are missing or unreadable.
    pub missing: Vec<String>,
    pub origin: Origin,
    pub reply: Option<Resolver<Result<(), StoreError>>>,
    /// The rest of a checkpoint: the writer builds and commits the tree after the files.
    pub checkpoint: Option<CheckpointJob>,
}

/// What the writer does for a checkpoint after the seal and the files (§5.13 C3 to C5).
pub(crate) struct CheckpointJob {
    pub reason: CheckpointReason,
    pub intake: crate::handle::IntakeReport,
    /// The seal's result, which comes before the files' in the reply.
    pub sealed: Result<crate::seal::SealReport, StoreError>,
    pub device: tc_journal::DeviceId,
    pub reply: Resolver<Result<CheckpointReport, StoreError>>,
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
    /// The window got focus: every path is looked at (§11.1).
    Focus {
        reply: Resolver<Result<crate::reply::FocusReport, StoreError>>,
    },
    /// The watcher told these paths changed (§11.1).
    /// The paths the watcher told, and how many changes it had heard of when it told them.
    Paths(Vec<String>, u64),
    /// *Take in the file as an edit* for a held path (§6.2).
    TakeInHeld {
        rel: String,
        reply: Resolver<Result<crate::reply::TakenIn, StoreError>>,
    },
    /// The bytes of a carried file's version, its blob (§11.4), for an export.
    ReadCarried {
        oid: tc_journal::GitOid,
        reply: Resolver<Result<Option<Vec<u8>>, StoreError>>,
    },
    /// The branch moved: the objects of the commit are reachable now, so a repack due may
    /// run (§9.8).
    Repack,
    /// *Import* of a kept copy (delta §2.11 Q4).
    ImportKept {
        copy: String,
        reply: Resolver<Result<crate::kept::ImportReport, StoreError>>,
    },
    /// The actor's answer to an *Import*: its batches are durable, or why not.
    Imported {
        copy: String,
        bytes: Vec<u8>,
        result: Result<crate::kept::ImportReport, StoreError>,
        reply: Resolver<Result<crate::kept::ImportReport, StoreError>>,
    },
    /// *Not now* for a held book file or story (delta §2.11 Q5).
    NotNow {
        rel: String,
        reply: Resolver<Result<(), StoreError>>,
    },
    /// *Dismiss* of a kept copy.
    DismissKept {
        copy: String,
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
    /// The actor's answer to a plan the writer sent it (§11.1 X4).
    Recorded {
        rel: String,
        seen: tc_journal::Md5,
        copy: String,
        outcome: Recorded,
    },
}

/// What the actor did with a plan of an outside edit.
#[derive(Debug)]
pub(crate) enum Recorded {
    /// One batch holds the changes (or a batch that reconciles the same bytes did already).
    Done {
        batch: Option<tc_journal::BatchId>,
        changes: u32,
        unknown_base: bool,
        whitespace_only: u32,
        kept: u32,
        derived: u32,
        /// The change that now stands for each record the batch changed: the base the bytes
        /// on disk are the next edit's (§11.1 D7).
        heads: Vec<(tc_journal::RecordKey, tc_journal::ChangeId)>,
    },
    /// The rules refuse the plan (a non-NFC identity in a sidecar): nothing is recorded.
    Refused(String),
    /// The disk refused the batch: nothing is recorded, and the file is looked at again.
    NotWritten,
}

/// See the module documentation.
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts the writer keeps about the folder and its open; together they are not one state"
)]
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
    /// Each outside version of `metadata.json` copied to `.tc/external/` whose recording has not
    /// ended, by md5, with its copy: a retry after the disk refused its batch sends it again
    /// with the same copy and notice. Its bytes are accepted when the recording ends, and a
    /// version that comes back after that is a new edit (§11.1 X3).
    pub unrecorded_metadata: BTreeMap<String, String>,
    /// The fold the job runs on, and its sequence number.
    pub fold: FoldState,
    pub seq: u64,
    /// The sequence number of the fold each derived path was last written or found equal
    /// from, so a checkpoint's tree never takes the bytes of a newer fold for its own.
    pub written_seq: BTreeMap<String, u64>,
    /// The paths the running group named at M3 (from `Placed` or `Unknown`) whose rename has
    /// certainly not happened yet (not tried, or refused with the target unchanged): when the
    /// group stops with an error, the ledger learns the bytes before it are still there.
    pub unattempted: BTreeSet<String>,
    pub packs: BTreeMap<String, PackFacts>,
    pub complete: bool,
    /// The expected packs that are missing or unreadable, which is why `complete` is false.
    pub packs_missing: Vec<String>,
    /// What to write: the actor's marks of the job, the open's classification, and the paths
    /// the last run could not write.
    pub dirty: Dirty,
    /// Bytes O9 projected to classify a path, kept for the materialization that follows it
    /// with the same fold, so the open projects each file once.
    pub prerendered: BTreeMap<String, crate::materialize::Projected>,
    /// The sequence number of the fold they were projected from.
    pub prerendered_seq: u64,
    /// Why each blocked path of the job could not be written (R16).
    pub reasons: BTreeMap<String, Blocked>,
    /// The count of unreadable values the person was last told of.
    pub values_told: Option<u32>,
    /// The open's first job runs: a long one shows progress.
    pub opening: bool,
    /// The open sealed (§6.1 O15): its write group ends with the open's materialization.
    pub sealed_at_open: bool,
    /// The writer's own handle on the project's repository: blobs, trees and commit objects.
    pub git: Option<crate::git::Git>,
    /// This computer's device, for the trailers of a commit.
    pub device: tc_journal::DeviceId,
    /// A damaged repository was replaced at the open and set aside here: the new one's first
    /// commit is the project as it is (§6.4 G4), made once the open's files are written, and
    /// the carried blobs and packs of the old one are copied first.
    pub recover: Option<std::path::PathBuf>,
    /// How many loose objects the repository held when they were last counted (§9.8).
    pub loose: Option<crate::repack::LooseCount>,
    /// The repack that runs on a thread of its own, if one does (§9.8).
    pub repack: Option<crate::repack::Run>,
    /// The blobs `refs/tc/carried` is known to keep (§9.8), once the writer has read or
    /// written it.
    pub carried_kept: Option<BTreeSet<tc_journal::GitOid>>,
    /// A group wrote files since the folder was last looked at as a burrito (test hooks).
    pub wrote_group: bool,
    /// Paths examined and sent to the actor, not yet recorded: nothing is written to them
    /// until the actor answers (D7).
    pub pending_record: BTreeMap<String, (tc_journal::Md5, Vec<u8>)>,
    /// How each file examined and sent to the actor shows its records (§11.1 X3), kept until
    /// the actor answers.
    pub shown_by_path:
        BTreeMap<String, Vec<(tc_journal::RecordKey, tc_journal::outside::ShownDigest)>>,
    /// The base each file examined and sent to the actor was compared with, kept until the
    /// actor answers: the heads of the bytes it recorded start from it (§11.1 D7).
    pub bases: BTreeMap<String, crate::ledger::Base>,
    /// The heads file of each path as this session last wrote or read it (§3.6).
    pub heads_cache: BTreeMap<String, crate::manifest::HeadsFile>,
    /// Carried files handed to the actor to be recorded, with the blob and size they were
    /// stored as, until the actor answers (§11.4).
    pub pending_carried: BTreeMap<String, (tc_journal::GitOid, u64)>,
    /// Paths the open found that need a read, looked at once the actor runs (§6.1 O9).
    pub deferred_paths: Vec<String>,
    /// Who asked for a held file to be taken in, answered when the batch is durable.
    pub waiting: BTreeMap<String, Resolver<Result<crate::reply::TakenIn, StoreError>>>,
    /// The open classifies: a changed path is collected, and the open records it itself.
    pub collecting: bool,
    pub found: Vec<crate::outside::Found>,
    pub actor: Sender<Msg>,
}

impl Writer {
    /// A notice for the person, typed (R16).
    pub(crate) fn notice(&self, notice: Notice, copy: Option<String>) {
        let identity =
            crate::notices::CopyIdentity::capture(&*self.vfs, &self.folder, copy.as_deref());
        let notice = self.shared.add_notice(notice, copy, identity);
        self.shared.emit(StoreEvent::Notice(notice));
    }

    pub(crate) fn read_only(&self) -> Option<ReadOnlyReason> {
        match self.shared.facts().mode {
            crate::handle::Mode::ReadOnly(reason) => Some(reason),
            crate::handle::Mode::Writable => None,
        }
    }

    /// Takes the view of a job: the fold it renders from and the packs the actor knows. The
    /// writer never goes back to an older fold than one it has run on.
    pub(crate) fn begin(
        &mut self,
        snapshot: &Snapshot,
        packs: BTreeMap<String, PackFacts>,
        complete: bool,
    ) {
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

    /// Takes the view of a checkpoint's job: exactly the snapshot its packs stand for, even
    /// when a look at the folder ran on a newer one meanwhile. The commit shows the project
    /// as the packs hold it, never a save they lack (§5.13 C3); the paths a newer snapshot
    /// wrote are rendered again for the tree ([`Writer::stale_paths`]) and left as they are in
    /// the folder ([`Writer::written_from_newer_fold`]).
    fn begin_exact(
        &mut self,
        snapshot: &Snapshot,
        packs: BTreeMap<String, PackFacts>,
        complete: bool,
    ) {
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
            missing,
            origin,
            reply,
            checkpoint,
        } = job;
        if checkpoint.is_some() {
            self.begin_exact(&snapshot, packs, complete);
        } else {
            self.begin(&snapshot, packs, complete);
        }
        self.packs_missing = missing;
        self.dirty.absorb(dirty);
        self.opening = origin == Origin::Open;
        if origin == Origin::Checkpoint {
            self.shared.hooks.wait(Point::BeforeCheckpoint);
        }
        let result = self.materialize_now();
        self.opening = false;
        if checkpoint.is_some()
            || (origin == Origin::Open && std::mem::take(&mut self.sealed_at_open))
        {
            // The seal's packs are listed now, or the job failed and the next open repairs it.
            self.shared.close_group();
        }
        if let Err(e) = &result {
            self.job_failed(e);
        }
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
        if origin == Origin::Open && self.recover.is_some() && result.is_ok() {
            self.recover_history();
        }
        if origin == Origin::Open && result.is_ok() {
            // Counted once here; what the writer writes after is added (§9.8).
            self.maybe_repack();
        }
        if let Some(checkpoint) = checkpoint {
            self.finish_checkpoint(checkpoint, result);
        } else if let Some(reply) = reply {
            reply.resolve(result);
        }
    }

    /// A job that failed: the paths stay dirty and are tried again; the person is told once
    /// that derived files could not be written and the work is saved (R16).
    pub(crate) fn job_failed(&mut self, error: &StoreError) {
        let reason: Box<str> = error.to_string().into();
        let blocked = Blocked::Failed {
            reason: reason.clone(),
        };
        if !self.shared.has_blocked_notice(&[], &blocked) {
            self.notice(
                Notice::MaterializeBlocked {
                    paths: Vec::new(),
                    reason: blocked,
                },
                None,
            );
        }
    }

    /// §5.10 for what is dirty. A path another program refused, or a write that failed,
    /// stays dirty and is tried again by [`Writer::retry`].
    pub(crate) fn materialize_now(&mut self) -> Result<(), StoreError> {
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
        let Some(retained) = self.shared.retained_notice(id) else {
            return Ok(());
        };
        let notice = retained.visible;
        if self.read_only().is_some() {
            self.shared.remove_notice(id);
            return Ok(());
        }
        // The copy of a hold that stands is the only other trace of the person's bytes, and
        // the notice comes back at the next open (§6.2): dismissing it hides it for this
        // session and keeps the copy.
        let held = self.manifest.paths.values().any(|e| {
            e.held()
                .is_some_and(|h| Some(&h.copy) == notice.copy.as_ref())
        });
        let shared_copy = self
            .shared
            .notices()
            .iter()
            .any(|other| other.id != id && other.copy.is_some() && other.copy == notice.copy);
        // So is the copy of a version of `metadata.json` whose recording has not ended, once
        // the file changes again.
        let unrecorded = self
            .unrecorded_metadata
            .values()
            .any(|copy| Some(copy) == notice.copy.as_ref());
        // A kept copy goes only with its own entry (`dismiss_kept`).
        let kept = self
            .manifest
            .kept
            .iter()
            .any(|k| Some(&k.copy) == notice.copy.as_ref());
        if held || shared_copy || unrecorded || kept {
            self.shared.remove_notice(id);
            return Ok(());
        }
        if let Some(copy) = notice.copy {
            let path = self.folder.path(&copy);
            // Only the bytes the notice was raised for may go; a denied look is not absence,
            // and a copy changed since is someone's other work. Either keeps the notice.
            let mut missing = false;
            let removed = atomic_remove_checked(
                &*self.vfs,
                &path,
                SyncDir::Now,
                &HeldRetry::SAVE,
                &mut || match crate::notices::CopyDigest::read(&*self.vfs, &path) {
                    Ok(actual) => match retained.identity {
                        CopyIdentity::Verified(expected) if actual == expected => Ok(true),
                        CopyIdentity::Verified(_) => Err(StoreError::Other(format!(
                            "{copy} changed since its notice; the copy and the notice are kept"
                        ))),
                        CopyIdentity::NoCopy | CopyIdentity::Unavailable => {
                            Err(StoreError::Other(format!(
                                "{copy} could not be read when its notice was raised; the copy and the notice are kept"
                            )))
                        }
                    },
                    Err(e) if crate::vfs::is_not_found(&e) => {
                        missing = true;
                        Ok(false)
                    }
                    Err(e) => Err(StoreError::io("read the copy", &path, e)),
                },
            )?;
            if removed == Removed::Refused && !missing {
                return Err(StoreError::Other(format!(
                    "{copy} was not removed; its notice is kept"
                )));
            }
        }
        self.shared.remove_notice(id);
        Ok(())
    }

    /// *Restore from the project* for a held path (§6.2): the file is written from the
    /// fold, with the held bytes as the ones the compare-and-swap expects.
    pub(crate) fn restore_held(&mut self, rel: &str) -> Result<(), StoreError> {
        if let Some(reason) = self.read_only() {
            return Err(StoreError::Other(format!(
                "the project is read-only: {reason:?}"
            )));
        }
        let snapshot = self.shared.snapshot();
        let packs = self.packs.clone();
        let complete = self.complete;
        self.begin(&snapshot, packs, complete);
        let Some(file) = DerivedFile::from_path(rel).filter(|f| self.fold.projects_file(f)) else {
            return Ok(());
        };
        // The held bytes become the app's to write over; their copy stays. They stand for
        // nothing the project lacks (the person gave them up), so their heads are the fold's.
        if let Some(held) = self.manifest.paths.get(rel).and_then(|e| e.held()).cloned() {
            let records = crate::manifest::heads_records(
                &self.placed_heads(self.fold.file_heads(&file)),
                &[],
            );
            let dir = self.folder.path(crate::layout::HEADS);
            crate::atomic::atomic_create_dirs(&*self.vfs, &dir)?;
            self.put_heads(rel, &held.md5, records, SyncDir::Now)?;
        }
        self.note(rel, crate::ledger::Event::Restore);
        self.dirty.files.insert(file);
        self.materialize_now()
    }

    fn handle(&mut self, job: Job) -> bool {
        if !matches!(job, Job::Sync { .. }) {
            self.shared.note_traffic();
        }
        match job {
            Job::Materialize(job) => self.run(*job),
            Job::StorageCheck(results) => self.storage_check(results),
            Job::RestoreHeld { rel, reply } => {
                let result = self.restore_held(&rel);
                reply.resolve(result);
            }
            Job::Focus { reply } => {
                let result = self.focus();
                reply.resolve(result);
            }
            Job::Paths(rels, upto) => {
                self.paths_changed(rels);
                self.shared.examined(upto);
            }
            Job::TakeInHeld { rel, reply } => self.take_in_held(&rel, reply),
            Job::DismissNotice { id, reply } => {
                let result = self.dismiss(id);
                reply.resolve(result);
            }
            Job::Recorded {
                rel,
                seen,
                copy,
                outcome,
            } => {
                let bytes = self
                    .pending_record
                    .get(&rel)
                    .map(|(_, bytes)| bytes.clone())
                    .unwrap_or_default();
                // The batch is durable: the file is written again from the project now. Bytes
                // not accepted wait for the writer's own retry, which would otherwise send them
                // again at once for as long as the disk refuses.
                if self.recorded(rel, seen, copy, outcome, &bytes) == Answered::Accepted {
                    self.retry();
                }
            }
            Job::ReadCarried { oid, reply } => {
                // The writer owns the repository; a project with none has no carried bytes.
                let result = match &self.git {
                    Some(git) => git.read_blob(oid).map_err(StoreError::from),
                    None => Ok(None),
                };
                reply.resolve(result);
            }
            Job::Repack => self.maybe_repack(),
            Job::ImportKept { copy, reply } => self.import_kept(&copy, reply),
            Job::Imported {
                copy,
                bytes,
                result,
                reply,
            } => self.imported(&copy, &bytes, result, reply),
            Job::NotNow { rel, reply } => {
                let result = self.not_now(&rel);
                reply.resolve(result);
            }
            Job::DismissKept { copy, reply } => {
                let result = self.dismiss_kept(&copy);
                reply.resolve(result);
            }
            Job::Sync { reply } => reply.resolve(()),
            Job::Close { reply } => {
                self.stop_repack();
                reply.resolve(());
                return false;
            }
        }
        self.probe_written_state();
        true
    }

    /// Test hooks (§16.6): after a job that wrote files, outside a write group and with no
    /// outside edit waiting to be recorded, the folder is looked at as a Scripture Burrito.
    /// Without a probe this costs nothing.
    fn probe_written_state(&mut self) {
        if !self.wrote_group || !self.shared.hooks.looks_at_burrito() {
            return;
        }
        if !self.pending_record.is_empty() || self.shared.group_open() || self.shared.unexamined() {
            // Looked at after a later job, when the group is closed.
            return;
        }
        let begun = self.shared.groups_begun();
        let shared = &self.shared;
        let looked = shared.hooks.probe_burrito(self.folder.root(), || {
            !shared.group_open() && shared.groups_begun() == begun
        });
        // A seal on the actor may begin its group while the probe reads the folder.
        self.wrote_group = !looked;
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
                Err(RecvTimeoutError::Timeout) => {
                    self.retry();
                    self.probe_written_state();
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.stop_repack();
                    return;
                }
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

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        reason = "test code: a failed unwrap is a failed test"
    )]

    use super::*;
    use crate::counters::Counters;
    use crate::fault::FaultFs;
    use crate::handle::{Mode, ProjectFacts};
    use crate::hooks::Hooks;
    use std::sync::Mutex;
    use tc_journal::fold::{FoldInput, ValueAt, ValueSource};
    use tc_journal::record::{MarkerWs, VerseValue};
    use tc_journal::{
        BatchId, BatchRef, BookId, Cause, Change, ChangeBody, ChapterNo, DeviceId, DeviceSeq,
        ProjectId, Record, TokenKey, VerseNo, WallTime,
    };

    /// A value source that can be made to fail, as a file a program holds.
    #[derive(Debug)]
    struct Flaky {
        files: Vec<Vec<u8>>,
        blocked: Mutex<bool>,
    }

    impl ValueSource for Flaky {
        fn read(&self, at: ValueAt) -> Option<Vec<u8>> {
            if *self.blocked.lock().unwrap() {
                return None;
            }
            let file = self.files.get(at.file as usize)?;
            let start = at.offset as usize;
            file.get(start..start + at.len as usize).map(<[u8]>::to_vec)
        }
    }

    /// Two devices edit one verse from no common base, each with its own words: a conflict
    /// whose versions can only be compared by reading them.
    fn two_edits() -> (FoldState, Arc<Flaky>) {
        let verse = |words: &str| Record::Verse {
            book: BookId::Titus,
            chapter: ChapterNo::new(1).unwrap(),
            verse: VerseNo::new(1).unwrap(),
            value: VerseValue::Text {
                marker: MarkerWs::canonical(),
                words: words.into(),
                layout: "\n".into(),
                last: VerseNo::new(1).unwrap(),
                from: None,
            },
        };
        let change = |n: u8, words: &str| {
            let mut random = [0u8; 15];
            random[0] = n;
            let mut batch = [0u8; 16];
            batch[0] = n;
            Change::new(ChangeBody {
                v: 0,
                record: verse(words),
                parents: Vec::new(),
                device: DeviceId::local(random),
                seq: DeviceSeq(1),
                at: WallTime(1_760_000_000_000 + i64::from(n)),
                batch: BatchRef::new(BatchId::from_bytes(batch), 1).unwrap(),
                cause: Cause::Edit,
            })
            .unwrap()
        };
        let changes = [change(1, "Juan's words"), change(2, "Ana's words")];
        let flaky = Arc::new(Flaky {
            files: changes.iter().map(|c| c.raw().to_vec()).collect(),
            blocked: Mutex::new(true),
        });
        let inputs: Vec<FoldInput> = changes
            .iter()
            .enumerate()
            .map(|(i, c)| {
                FoldInput::from(c.clone()).prepare_at(ValueAt {
                    offset: 0,
                    file: i as u32,
                    len: c.raw().len() as u32,
                })
            })
            .collect();
        let source: Arc<dyn ValueSource> = flaky.clone();
        (
            FoldState::fold_from(ProjectKind::Bible, Some(source), inputs),
            flaky,
        )
    }

    fn writer_over(fold: FoldState) -> Writer {
        let fs = FaultFs::new();
        let counters = Arc::new(Counters::default());
        let id = ProjectId::from_bytes([7; 16]);
        let signing = Arc::new(TokenKey::derive(&[1; 32], id));
        let facts = ProjectFacts {
            id,
            kind: ProjectKind::Bible,
            device: DeviceId::local([9; 15]),
            folder: "/p".into(),
            mode: Mode::Writable,
            git_dir: None,
        };
        let shared = Arc::new(Shared::new(
            fold.clone(),
            signing,
            facts,
            counters.clone(),
            Arc::new(Hooks::default()),
        ));
        let (actor, _rx) = std::sync::mpsc::channel();
        Writer {
            vfs: Arc::new(fs),
            counters,
            folder: Folder::new("/p"),
            kind: ProjectKind::Bible,
            shared,
            clock: Arc::new(crate::clock::ManualClock::at(1_792_195_200_000)),
            network: false,
            manifest: Manifest::default(),
            manifest_changed: false,
            stamp_ns: 0,
            busy: BTreeMap::new(),
            unrecorded_metadata: BTreeMap::new(),
            fold,
            seq: 1,
            written_seq: BTreeMap::new(),
            unattempted: BTreeSet::new(),
            packs: BTreeMap::new(),
            complete: true,
            packs_missing: Vec::new(),
            dirty: Dirty::default(),
            prerendered: BTreeMap::new(),
            prerendered_seq: 0,
            reasons: BTreeMap::new(),
            values_told: None,
            opening: false,
            sealed_at_open: false,
            git: None,
            device: DeviceId::local([9; 15]),
            recover: None,
            loose: None,
            repack: None,
            carried_kept: None,
            wrote_group: false,
            pending_record: BTreeMap::new(),
            shown_by_path: BTreeMap::new(),
            bases: BTreeMap::new(),
            heads_cache: BTreeMap::new(),
            pending_carried: BTreeMap::new(),
            deferred_paths: Vec::new(),
            waiting: BTreeMap::new(),
            collecting: false,
            found: Vec::new(),
            actor,
        }
    }

    fn blocked(writer: &Writer) -> Vec<Blocked> {
        writer
            .shared
            .notices()
            .into_iter()
            .filter_map(|n| match n.notice {
                Notice::MaterializeBlocked { reason, .. } => Some(reason),
                _ => None,
            })
            .collect()
    }

    /// R16: values the fold cannot read block a materialization (§2.6), and the person is
    /// told how many, once; the notice goes when they can be read.
    #[test]
    fn values_the_fold_cannot_read_block_a_materialize_with_a_typed_notice() {
        let (fold, flaky) = two_edits();
        assert_eq!(fold.unreadable_records().len(), 1);
        let mut writer = writer_over(fold);
        writer.dirty.metadata = true;
        let report = writer.materialize().unwrap();
        assert!(report.written.is_empty());
        assert_eq!(
            blocked(&writer),
            vec![Blocked::ValuesUnreadable { records: 1 }]
        );
        // The job is tried again: it says nothing new.
        writer.materialize().unwrap();
        assert_eq!(blocked(&writer).len(), 1, "listed once");
        assert!(!writer.dirty.is_empty(), "the work waits");
        // The values can be read now.
        *flaky.blocked.lock().unwrap() = false;
        writer.fold.retry_unreadable();
        assert!(writer.fold.unreadable_records().is_empty());
        writer.materialize().unwrap();
        assert!(blocked(&writer).is_empty(), "the notice went");
    }

    /// A job never goes back to an older fold than one the writer has run on.
    #[test]
    fn a_job_from_an_older_snapshot_never_writes_over_newer_bytes() {
        let (fold, _) = two_edits();
        let mut writer = writer_over(fold.clone());
        // The actor published newer snapshots: the writer ran on the third.
        let first = writer.shared.snapshot();
        writer.shared.publish(fold.clone());
        writer.shared.publish(fold);
        let newest = writer.shared.snapshot();
        writer.begin(&newest, BTreeMap::new(), true);
        assert_eq!(writer.seq, newest.seq());
        // A queued job that carries the first snapshot runs with the newest.
        writer.begin(&first, BTreeMap::new(), true);
        assert_eq!(
            writer.seq,
            newest.seq(),
            "never back to seq {}",
            first.seq()
        );
    }
}
