//! The project actor (`docs/STORAGE.md` §5.0): the one thread that writes the project
//! folder. It takes commands in order, writes each batch durably before it replies, folds
//! it and publishes the new snapshot; it materializes derived files 3 s after the last
//! batch, seals at checkpoints, and at close writes what is due and releases the lock.
//!
//! A second installation writing into the same folder (§5.0) is seen by a checkpoint's
//! listing: its files are folded, so nothing either installation wrote is lost, and the
//! project turns read-only with `SecondWriter`.

use crate::commands;
use crate::drafts::{DraftWriter, cleared_by};
use crate::error::StoreError;
use crate::handle::{
    CheckpointReason, CheckpointReport, CheckpointSkip, Mode, Msg, ProjectHandle, Resolver,
    StoreEvent, pending,
};
use crate::layout::TC;
use crate::load::JournalKind;
use crate::project::{PackInfo, PendingInfo, Project};
use crate::reply::{CommandError, ReadOnlyReason};
use crate::vfs::LockHandle;
use crate::writer::{CheckpointJob, Job, Origin};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tc_journal::fold::Notice;
use tc_journal::index::{Facts, load_pack, load_pending};
use tc_journal::pack::PackStatus;

/// Derived files are written this long after the last batch (§5.0 B5).
pub(crate) const MATERIALIZE_AFTER: Duration = Duration::from_secs(3);
/// A checkpoint runs this often while batches wait to be sealed (§5.13).
pub(crate) const CHECKPOINT_EVERY: Duration = Duration::from_secs(30 * 60);

struct Actor {
    project: Project,
    rx: Receiver<Msg>,
    drafts: Arc<DraftWriter>,
    lock: Option<Box<dyn LockHandle>>,
    /// When the next materialization is due.
    due: Option<Instant>,
    last_checkpoint: Instant,
    /// Held while `.tc/index.bin` is written, by the background job or at close.
    index_lock: Arc<Mutex<()>>,
    /// The writer's thread, joined at close.
    writer_thread: Option<std::thread::JoinHandle<()>>,
    /// Messages that arrived while a seal ran and are not commands: handled first, in order,
    /// when it is over.
    deferred: std::collections::VecDeque<Msg>,
}

/// Starts the actor of an opened project and returns its handle. `writer` is the queue of
/// the writer thread, which the open has started (§6.1 O10: its first job is the open's
/// materialization).
pub(crate) fn spawn(
    mut project: Project,
    lock: Option<Box<dyn LockHandle>>,
    drafts: Arc<DraftWriter>,
    writer: Sender<Job>,
    writer_thread: Option<std::thread::JoinHandle<()>>,
    actor_tx: Sender<Msg>,
    rx: Receiver<Msg>,
) -> ProjectHandle {
    let tx = actor_tx;
    project.writer = Some(writer.clone());
    let shared = project.shared.clone();
    let index_lock = Arc::new(Mutex::new(()));
    // O10: the open's materialization is the writer's first job, and the open does not wait
    // for it (a failure of it turns the project read-only, §6.1).
    if matches!(shared.facts().mode, Mode::Writable) {
        let job = project.materialize_job(Origin::Open, None, None);
        let _ = writer.send(Job::Materialize(Box::new(job)));
    }
    let dirty = !project.dirty.is_empty();
    let actor = Actor {
        project,
        rx,
        drafts: drafts.clone(),
        lock,
        due: dirty.then(|| Instant::now() + MATERIALIZE_AFTER),
        last_checkpoint: Instant::now(),
        index_lock: index_lock.clone(),
        writer_thread,
        deferred: std::collections::VecDeque::new(),
    };
    let thread = std::thread::Builder::new()
        .name("tc-store actor".into())
        .spawn(move || actor.run())
        .ok();
    ProjectHandle {
        shared,
        tx: Mutex::new(tx),
        writer: Mutex::new(writer),
        drafts,
        actor: Mutex::new(thread),
        index_lock,
        watcher: Mutex::new(None),
    }
}

impl Actor {
    fn run(mut self) {
        loop {
            let wait = self.wait();
            let received = match self.deferred.pop_front() {
                Some(msg) => Ok(msg),
                None => self.rx.recv_timeout(wait),
            };
            match received {
                Ok(Msg::Close { reply }) => {
                    let result = self.close();
                    reply.resolve(result);
                    return;
                }
                Ok(msg) => self.handle(msg),
                Err(RecvTimeoutError::Timeout) => self.on_timer(),
                Err(RecvTimeoutError::Disconnected) => {
                    let _ = self.close();
                    return;
                }
            }
        }
    }

    fn read_only(&self) -> Option<ReadOnlyReason> {
        match self.project.shared.facts().mode {
            Mode::ReadOnly(reason) => Some(reason),
            Mode::Writable => None,
        }
    }

    fn wait(&self) -> Duration {
        let now = Instant::now();
        let mut wait = CHECKPOINT_EVERY.saturating_sub(self.last_checkpoint.elapsed());
        if let Some(due) = self.due {
            wait = wait.min(due.saturating_duration_since(now));
        }
        wait
    }

    fn handle(&mut self, msg: Msg) {
        if !matches!(msg, Msg::Sync { .. }) {
            self.project.shared.note_traffic();
        }
        match msg {
            Msg::Command {
                command,
                reply,
                sent,
            } => {
                let waited = u64::try_from(sent.elapsed().as_nanos()).unwrap_or(u64::MAX);
                let counters = &self.project.counters;
                counters.add(&counters.queue_wait_ns, waited);
                counters.max(&counters.queue_wait_max_ns, waited);
                counters.add(&counters.commands, 1);
                if let Some(reason) = self.read_only() {
                    reply.resolve(Err(CommandError::ReadOnly(reason)));
                    return;
                }
                let cleared = cleared_by(&command, &self.project.fold);
                let result = commands::plan(&self.project.ctx(), &command)
                    .and_then(|plan| self.project.commit(plan));
                if result.is_ok() {
                    self.drafts.saved(cleared);
                    if !self.project.dirty.is_empty() {
                        self.due = Some(Instant::now() + MATERIALIZE_AFTER);
                    }
                }
                reply.resolve(result);
            }
            Msg::Materialize { reply } => self.materialize(Origin::Leave, reply),
            Msg::Checkpoint { reason, reply } => self.checkpoint(reason, Some(reply)),
            Msg::MoveHead {
                commit,
                expected,
                tree,
                report,
                reply,
            } => reply.resolve(self.move_head(commit, expected, tree, report)),
            Msg::CheckpointEnded { result, reply } => reply.resolve(result),
            Msg::RecordOutside {
                rel,
                seen,
                plan,
                derived,
                copy,
            } => self.record_outside(rel, seen, *plan, derived, copy),
            Msg::WritesFailed { reason } => {
                // The open's first materialization failed: the writer made the project
                // read-only; a project that cannot be written holds no lock.
                self.project.dirty = Default::default();
                self.due = None;
                self.lock = None;
                let _ = reason;
            }
            Msg::JournalChanged => {
                if self.read_only().is_none() {
                    // A failure to list is told by the next checkpoint's own listing.
                    let _ = self.second_writer();
                }
            }
            Msg::Sync { reply } => reply.resolve(()),
            Msg::Close { reply } => {
                let result = self.close();
                reply.resolve(result);
            }
        }
    }

    fn on_timer(&mut self) {
        if self.due.is_some_and(|d| d <= Instant::now()) {
            self.materialize(Origin::Timer, None);
        }
        if self.last_checkpoint.elapsed() >= CHECKPOINT_EVERY {
            self.last_checkpoint = Instant::now();
            if !self.project.journal.pending.is_empty() {
                self.checkpoint(CheckpointReason::Periodic, None);
            }
        }
    }

    /// §5.10. Hands the dirty paths to the writer with the newest snapshot, and goes on: the
    /// actor never waits for a derived file. `reply` resolves when the writer is done.
    fn materialize(&mut self, origin: Origin, reply: Option<Resolver<Result<(), StoreError>>>) {
        self.due = None;
        if self.read_only().is_some() {
            if let Some(reply) = reply {
                reply.resolve(Ok(()));
            }
            return;
        }
        let job = self.project.materialize_job(origin, reply, None);
        if let Some(writer) = &self.project.writer {
            // A writer that is gone drops the job, and its reply resolves as closed.
            let _ = writer.send(Job::Materialize(Box::new(job)));
        }
    }

    /// §5.13. The actor lists the journal folders for a second writer, checks the packs this
    /// session knows, seals, and hands the writer the snapshot after the seal: the files, the
    /// tree and the commit object are the writer's work, so saves go on meanwhile (R21). The
    /// branch moves when the writer sends [`Msg::MoveHead`].
    fn checkpoint(
        &mut self,
        reason: CheckpointReason,
        reply: Option<Resolver<Result<CheckpointReport, StoreError>>>,
    ) {
        self.last_checkpoint = Instant::now();
        let skipped = |why| CheckpointReport {
            skipped: Some(why),
            ..CheckpointReport::default()
        };
        let done = |reply: Option<Resolver<Result<CheckpointReport, StoreError>>>, result| {
            if let Some(reply) = reply {
                reply.resolve(result);
            }
        };
        if self.read_only().is_some() {
            return done(reply, Ok(skipped(CheckpointSkip::ReadOnly)));
        }
        match self.second_writer() {
            Ok(false) => {}
            Ok(true) => return done(reply, Ok(skipped(CheckpointSkip::SecondWriter))),
            Err(e) => return done(reply, Err(e)),
        }
        // A pack an antivirus program or a cloud client took is written back from its blob
        // before anything is derived from a project without it (§4.4). From here to the end
        // of the writer's materialization the folder may hold packs `metadata.json` does not
        // list yet: a write group (I7).
        self.project.shared.open_group();
        let restored = self.project.restore_expected_packs(false);
        // The commit shows the project as its packs hold it: the fold as the seal starts. A
        // save served between the seal's steps is a pending file the seal never reads, so
        // neither the snapshot nor the marks the writer gets include it (C3); it belongs to
        // the next checkpoint and to a materialization of its own.
        let at_seal = self.project.shared.snapshot();
        let marks = std::mem::take(&mut self.project.dirty);
        // A seal that failed after it renamed a pack leaves `metadata.json` without that
        // pack (I7), so the files are written anyway; the seal's error comes first.
        let sealed = restored.and_then(|()| self.seal_serving());
        let served = self.project.shared.seq() != at_seal.seq();
        // What a served command marked waits for a job of its own; what the seal marked is the
        // new packs, which the writer's job lists.
        self.due = served.then(|| Instant::now() + MATERIALIZE_AFTER);
        if !served {
            self.project.dirty = Default::default();
        }
        let device = self.project.device;
        let Some(reply) = reply else {
            // The periodic checkpoint nobody waits for: the writer's answer is dropped.
            let (_unused, resolver) = pending(|| Err(StoreError::Other("no one waits".into())));
            return self.send_checkpoint(reason, sealed, device, resolver, at_seal, marks);
        };
        self.send_checkpoint(reason, sealed, device, reply, at_seal, marks);
    }

    fn send_checkpoint(
        &mut self,
        reason: CheckpointReason,
        sealed: Result<crate::seal::SealReport, StoreError>,
        device: tc_journal::DeviceId,
        reply: Resolver<Result<CheckpointReport, StoreError>>,
        snapshot: crate::handle::Snapshot,
        marks: crate::project::Dirty,
    ) {
        let checkpoint = CheckpointJob {
            reason,
            sealed,
            device,
            reply,
        };
        let job = self.project.checkpoint_job(snapshot, marks, checkpoint);
        let sent = self
            .project
            .writer
            .as_ref()
            .is_some_and(|writer| writer.send(Job::Materialize(Box::new(job))).is_ok());
        if !sent {
            self.project.shared.close_group();
        }
    }

    /// §11.1 X4: commits the writer's plan of an outside edit as one batch, and tells the
    /// writer (which writes the file again) and the timer (the book's other files).
    fn record_outside(
        &mut self,
        rel: String,
        seen: tc_journal::Md5,
        plan: tc_journal::outside::OutsidePlan,
        derived: u32,
        copy: String,
    ) {
        // A project that turned read-only meanwhile (a second writer, a disk that filled)
        // records nothing: the bytes stay in the copy the writer made.
        let outcome = if self.read_only().is_some() {
            crate::writer::Recorded::NotWritten
        } else {
            self.project.record_outside(&rel, seen, plan, derived)
        };
        if !self.project.dirty.is_empty() {
            self.due = Some(Instant::now() + MATERIALIZE_AFTER);
        }
        if let Some(writer) = &self.project.writer {
            let _ = writer.send(Job::Recorded {
                rel,
                seen,
                copy,
                outcome,
            });
        }
    }

    /// §5.13 C5 and C6: the writer wrote the commit object; the branch moves to it if it is
    /// still where the writer read it, and the index follows so `git status` is clean.
    fn move_head(
        &mut self,
        commit: tc_journal::GitOid,
        expected: Option<tc_journal::GitOid>,
        tree: tc_journal::GitOid,
        report: CheckpointReport,
    ) -> Result<CheckpointReport, StoreError> {
        let result = self.move_branch_to(commit, expected, tree, report);
        // The commit is referenced or was given up: a repack may remove unreachable objects
        // again (§9.8).
        if let Some(git) = &self.project.git {
            git.commit_end();
        }
        result
    }

    fn move_branch_to(
        &mut self,
        commit: tc_journal::GitOid,
        expected: Option<tc_journal::GitOid>,
        tree: tc_journal::GitOid,
        mut report: CheckpointReport,
    ) -> Result<CheckpointReport, StoreError> {
        let Some(git) = &self.project.git else {
            report.skipped = Some(CheckpointSkip::NoHistory);
            return Ok(report);
        };
        match git.move_branch(expected, commit) {
            Ok(()) => {}
            Err(crate::git::GitError::HeadMoved) => {
                report.skipped = Some(CheckpointSkip::HeadMoved);
                return Ok(report);
            }
            Err(e) => return Err(e.into()),
        }
        // The index is a courtesy to a terminal; a failure to write it costs nothing.
        let _ = git.write_index(tree);
        report.commit = Some(commit);
        self.project
            .shared
            .emit(StoreEvent::Checkpointed { commit });
        // The commit's objects are reachable now: the writer repacks if the loose ones are
        // many (§9.8).
        if let Some(writer) = &self.project.writer {
            let _ = writer.send(Job::Repack);
        }
        Ok(report)
    }

    /// Folds journal files another installation wrote into this folder while it was open,
    /// and turns the project read-only (§5.0). `true` when there were any.
    fn second_writer(&mut self) -> Result<bool, StoreError> {
        let unknown = self.project.unknown_files()?;
        if unknown.is_empty() {
            return Ok(false);
        }
        let mut writer = None;
        for file in unknown {
            let Ok(bytes) = self.project.vfs.read(&file.abs) else {
                self.project.journal.unreadable.insert(file.rel.clone());
                continue;
            };
            let number = self.project.source.add(&file.abs);
            let loaded = match file.kind {
                JournalKind::Pack => load_pack(&file.rel, &bytes, self.project.id, number),
                JournalKind::Pending => load_pending(&file.rel, &bytes, self.project.id, number),
            };
            match loaded.facts {
                Facts::Pack(pack) => {
                    if let Some(path) = pack.path {
                        writer = writer.or(Some(path.sealer));
                    }
                    self.project.journal.packs.insert(
                        file.rel.clone(),
                        PackInfo {
                            id: pack.id,
                            size: loaded.key.size,
                            fingerprint: loaded.key.fingerprint,
                            number: Some(number),
                            valid: pack.status == PackStatus::Valid,
                            sealer: pack.path.map(|p| p.sealer),
                            replaces: pack.header.map(|h| h.replaces).unwrap_or_default(),
                            ids: Vec::new(),
                            blob: None,
                            seen: None,
                            damaged: matches!(pack.status, PackStatus::Damaged(_)),
                        },
                    );
                }
                Facts::Pending(pending) => {
                    let device = pending.header.as_ref().map(|h| h.device);
                    writer = writer.or(device);
                    self.project.journal.pending.insert(
                        file.rel.clone(),
                        PendingInfo {
                            ids: pending.ids,
                            reconciles: pending
                                .header
                                .as_ref()
                                .and_then(|h| h.reconciles.as_ref())
                                .map(|r| (r.path.as_str().to_owned(), r.seen_md5)),
                            number: Some(number),
                            status: pending.status,
                        },
                    );
                }
            }
            let delta = self.project.fold.add(loaded.inputs);
            let kind = self.project.kind;
            let conflicted = crate::project::conflicts_touched(&self.project.fold, &delta);
            self.project.dirty.mark(kind, &delta, conflicted);
            let seq = self.project.publish();
            self.project.shared.emit(StoreEvent::Committed {
                seq,
                delta,
                changes: Vec::new(),
            });
        }
        let device = writer.unwrap_or(self.project.device);
        self.project.notice(Notice::SecondWriter { device }, None);
        self.project
            .shared
            .set_mode(Mode::ReadOnly(ReadOnlyReason::SecondWriter { device }));
        Ok(true)
    }

    /// Seal and materialize what is due, write the fold cache, stop the writer, clear the
    /// lock's content and release the lock (§3.6). The pending files hold every save
    /// already.
    fn close(&mut self) -> Result<(), StoreError> {
        self.project.shared.begin_closing();
        let mut result = Ok(());
        if self.read_only().is_none() {
            let (done, resolver) = pending(|| {
                Err(StoreError::Other(
                    "the checkpoint at close was dropped".into(),
                ))
            });
            self.checkpoint(CheckpointReason::LeavingProject, Some(resolver));
            result = self.wait_serving(done).map(|_| ());
            if self.read_only().is_none()
                && let Some(sections) = self.project.index_bytes_later()
            {
                let _guard = self.index_lock.lock().unwrap_or_else(|e| e.into_inner());
                let dir = self.project.folder.path(TC);
                crate::open::write_index_file(&*self.project.vfs, &dir, self.project.id, &sections);
            }
        }
        self.stop_writer();
        // Everything the project folder gets from this session is written before the lock
        // is released: the writer and the draft writer end first (§5.0).
        self.drafts.stop();
        if self.read_only().is_none()
            && let Some(lock) = self.lock.as_mut()
        {
            let _ = lock.set_content(b"");
        }
        self.lock = None;
        result
    }

    /// K1 to K6 (§5.11) in steps. Between two steps the actor serves the commands that wait,
    /// so a save waits for one step (a pack written and read back, a few pending files
    /// removed) and not for the seal: 25 ms p95 while a checkpoint runs (PERFORMANCE.md). A
    /// save made meanwhile is a pending file this seal never saw, so no step touches it.
    fn seal_serving(&mut self) -> Result<crate::seal::SealReport, StoreError> {
        let mut run = self.project.seal_start()?;
        while self.project.seal_step(&mut run)? {
            self.serve_between_steps();
        }
        Ok(run.report)
    }

    /// Commands (and the edits the writer records) that wait are served now; whatever else
    /// has arrived waits until the seal is over.
    fn serve_between_steps(&mut self) {
        self.project.shared.hooks.seal_step_reached();
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Command { reply, .. } if self.project.shared.is_closing() => {
                    reply.resolve(Err(CommandError::Closed));
                }
                msg @ (Msg::Command { .. } | Msg::RecordOutside { .. }) => self.handle(msg),
                other => self.deferred.push_back(other),
            }
        }
    }

    /// Waits for the writer's answer to a checkpoint while serving what the writer sends the
    /// actor (the branch to move, the answer itself), so the actor never waits for a writer
    /// that waits for the actor (D2). Commands that arrive meanwhile are refused: the project
    /// is closing.
    fn wait_serving<T>(&mut self, done: crate::handle::Pending<T>) -> T {
        while !done.is_ready() {
            match self.rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Msg::Command { reply, .. }) => reply.resolve(Err(CommandError::Closed)),
                Ok(Msg::MoveHead {
                    commit,
                    expected,
                    tree,
                    report,
                    reply,
                }) => reply.resolve(self.move_head(commit, expected, tree, report)),
                Ok(Msg::CheckpointEnded { result, reply }) => reply.resolve(result),
                Ok(Msg::RecordOutside {
                    rel,
                    seen,
                    plan,
                    derived,
                    copy,
                }) => self.record_outside(rel, seen, *plan, derived, copy),
                Ok(Msg::Sync { reply }) => reply.resolve(()),
                Ok(Msg::Materialize { reply }) => {
                    if let Some(reply) = reply {
                        reply.resolve(Ok(()));
                    }
                }
                Ok(Msg::Checkpoint { reply, .. }) => {
                    reply.resolve(Err(StoreError::Other("the project is closing".into())));
                }
                Ok(Msg::WritesFailed { .. } | Msg::Close { .. } | Msg::JournalChanged) => {}
                Err(_) => {}
            }
        }
        done.wait()
    }

    /// Ends the writer thread after the jobs already sent.
    fn stop_writer(&mut self) {
        if let Some(writer) = self.project.writer.take() {
            let (done, resolver) = pending(|| ());
            if writer.send(Job::Close { reply: resolver }).is_ok() {
                done.wait();
            }
        }
        if let Some(thread) = self.writer_thread.take() {
            let _ = thread.join();
        }
    }
}
