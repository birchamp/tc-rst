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
use crate::handle::{Mode, Msg, ProjectHandle, Resolver, StoreEvent, pending};
use crate::layout::{INDEX, TC};
use crate::load::JournalKind;
use crate::project::{PackInfo, PendingInfo, Project};
use crate::reply::{CommandError, ReadOnlyReason};
use crate::vfs::LockHandle;
use crate::writer::{Job, Origin};
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
        let job = project.materialize_job(Origin::Open, Ok(()), None);
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
    }
}

impl Actor {
    fn run(mut self) {
        loop {
            let wait = self.wait();
            match self.rx.recv_timeout(wait) {
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
            Msg::Materialize { reply } => self.materialize(Origin::Leave, Ok(()), reply),
            Msg::Checkpoint { reply } => self.checkpoint(Some(reply)),
            Msg::WritesFailed { reason } => {
                // The open's first materialization failed: the writer made the project
                // read-only; a project that cannot be written holds no lock.
                self.project.dirty = Default::default();
                self.due = None;
                self.lock = None;
                let _ = reason;
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
            self.materialize(Origin::Timer, Ok(()), None);
        }
        if self.last_checkpoint.elapsed() >= CHECKPOINT_EVERY {
            self.last_checkpoint = Instant::now();
            if !self.project.journal.pending.is_empty() {
                self.checkpoint(None);
            }
        }
    }

    /// §5.10. Hands the dirty paths to the writer with the newest snapshot, and goes on: the
    /// actor never waits for a derived file. `reply` resolves when the writer is done.
    fn materialize(
        &mut self,
        origin: Origin,
        sealed: Result<(), StoreError>,
        reply: Option<Resolver<Result<(), StoreError>>>,
    ) {
        self.due = None;
        if self.read_only().is_some() {
            if let Some(reply) = reply {
                reply.resolve(sealed);
            }
            return;
        }
        let job = self.project.materialize_job(origin, sealed, reply);
        if let Some(writer) = &self.project.writer {
            // A writer that is gone drops the job, and its reply resolves as closed.
            let _ = writer.send(Job::Materialize(Box::new(job)));
        }
    }

    /// §5.13 C1 and C2 (the commit is unit S2's), after the listing that finds a second
    /// writer. The seal runs here; the materialization that follows it runs on the writer.
    fn checkpoint(&mut self, reply: Option<Resolver<Result<(), StoreError>>>) {
        self.last_checkpoint = Instant::now();
        let done = |reply: Option<Resolver<Result<(), StoreError>>>, result| {
            if let Some(reply) = reply {
                reply.resolve(result);
            }
        };
        if self.read_only().is_some() {
            return done(reply, Ok(()));
        }
        match self.second_writer() {
            Ok(false) => {}
            Ok(true) => return done(reply, Ok(())),
            Err(e) => return done(reply, Err(e)),
        }
        // A seal that failed after it renamed a pack leaves `metadata.json` without that
        // pack (I7), so the materialization runs anyway; the seal's error comes first.
        let sealed = self.project.seal().map(|_| ());
        self.materialize(Origin::Checkpoint, sealed, reply);
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
        let mut result = Ok(());
        if self.read_only().is_none() {
            let (done, resolver) = pending(|| Ok(()));
            self.checkpoint(Some(resolver));
            result = done.wait();
            if self.read_only().is_none() {
                let bytes = self.project.index_bytes();
                if let Some(bytes) = bytes {
                    let _guard = self.index_lock.lock().unwrap_or_else(|e| e.into_inner());
                    let dir = self.project.folder.path(TC);
                    let _ = crate::atomic::atomic_write(
                        &*self.project.vfs,
                        &dir,
                        INDEX.rsplit('/').next().unwrap_or("index.bin"),
                        &bytes,
                        crate::atomic::SyncDir::Now,
                    );
                }
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
