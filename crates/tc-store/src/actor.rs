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
use crate::handle::{Mode, Msg, ProjectHandle, StoreEvent};
use crate::layout::{INDEX, TC};
use crate::load::JournalKind;
use crate::project::{PackInfo, PendingInfo, Project};
use crate::reply::{CommandError, ReadOnlyReason};
use crate::vfs::LockHandle;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
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
}

/// Starts the actor of an opened project and returns its handle.
pub(crate) fn spawn(
    project: Project,
    lock: Option<Box<dyn LockHandle>>,
    drafts: Arc<DraftWriter>,
) -> ProjectHandle {
    let (tx, rx) = mpsc::channel();
    let shared = project.shared.clone();
    let index_lock = Arc::new(Mutex::new(()));
    let dirty = !project.dirty.is_empty();
    let actor = Actor {
        project,
        rx,
        drafts: drafts.clone(),
        lock,
        due: dirty.then(|| Instant::now() + MATERIALIZE_AFTER),
        last_checkpoint: Instant::now(),
        index_lock: index_lock.clone(),
    };
    let thread = std::thread::Builder::new()
        .name("tc-store actor".into())
        .spawn(move || actor.run())
        .ok();
    ProjectHandle {
        shared,
        tx: Mutex::new(tx),
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
            Msg::Materialize { reply } => {
                let result = self.materialize();
                if let Some(reply) = reply {
                    reply.resolve(result);
                }
            }
            Msg::Checkpoint { reply } => {
                let result = self.checkpoint();
                reply.resolve(result);
            }
            Msg::DismissNotice { id, reply } => {
                let result = self.dismiss(id);
                reply.resolve(result);
            }
            Msg::RestoreHeld { rel, reply } => {
                let result = self.restore_held(&rel);
                reply.resolve(result);
            }
            Msg::StorageCheck(results) => {
                let was_dirty = !self.project.dirty.is_empty();
                if self.read_only().is_none()
                    && let Err(e) = self.project.apply_check(results)
                {
                    self.project.notice(
                        Notice::FileUnreadable {
                            path: ".tc/materialized.json".into(),
                            reason: e.to_string().into(),
                        },
                        None,
                    );
                }
                // What the check found is written now, within this message, so `wait_idle`
                // covers it; paths a save made dirty keep their own 3 s.
                if !was_dirty && !self.project.dirty.is_empty() {
                    let _ = self.materialize();
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
            let _ = self.materialize();
        }
        if self.last_checkpoint.elapsed() >= CHECKPOINT_EVERY {
            self.last_checkpoint = Instant::now();
            if !self.project.journal.pending.is_empty() {
                let _ = self.checkpoint();
            }
        }
    }

    /// §5.10. A path another program refused, or a write that failed, stays dirty and is
    /// tried again: the actor never waits for it, so commands go on being served.
    fn materialize(&mut self) -> Result<(), StoreError> {
        self.due = None;
        if self.read_only().is_some() {
            return Ok(());
        }
        let result = self.project.materialize();
        if !self.project.dirty.is_empty() {
            let wait = match &result {
                Ok(_) => self.project.retry_delay().unwrap_or(MATERIALIZE_AFTER),
                Err(_) => MATERIALIZE_AFTER,
            };
            self.due = Some(Instant::now() + wait);
        }
        result.map(|_| ())
    }

    /// §5.13 C1 and C2 (the commit is unit S2's), after the listing that finds a second
    /// writer.
    fn checkpoint(&mut self) -> Result<(), StoreError> {
        self.last_checkpoint = Instant::now();
        if self.read_only().is_some() {
            return Ok(());
        }
        if self.second_writer()? {
            return Ok(());
        }
        self.project
            .shared
            .hooks
            .wait(crate::hooks::Point::BeforeCheckpoint);
        // A seal that failed after it renamed a pack leaves `metadata.json` without that
        // pack (I7), so the materialization runs anyway; the seal's error comes first.
        let sealed = self.project.seal();
        let materialized = self.materialize();
        sealed.map(|_| ()).and(materialized)
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
            self.project.prerendered.clear();
            let delta = self.project.fold.add(loaded.inputs);
            let (fold, kind) = (&self.project.fold, self.project.kind);
            self.project.dirty.mark(fold, kind, &delta);
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

    fn dismiss(&mut self, id: u64) -> Result<(), StoreError> {
        let Some(notice) = self.project.shared.remove_notice(id) else {
            return Ok(());
        };
        if self.read_only().is_some() {
            return Ok(());
        }
        // The copy of a hold that stands is the only other trace of the person's bytes, and
        // the notice comes back at the next open (§6.2): dismissing it hides it for this
        // session and keeps the copy.
        let held = self.project.manifest.paths.values().any(|e| {
            e.held
                .as_ref()
                .is_some_and(|h| Some(&h.copy) == notice.copy.as_ref())
        });
        if held {
            return Ok(());
        }
        if let Some(copy) = notice.copy {
            let path = self.project.folder.path(&copy);
            if self.project.vfs.metadata(&path).is_ok() {
                crate::atomic::atomic_remove(
                    &*self.project.vfs,
                    &path,
                    crate::atomic::SyncDir::Now,
                )?;
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
        let Some(file) = self
            .project
            .fold
            .derived_files()
            .into_iter()
            .find(|f| f.path() == rel)
        else {
            return Ok(());
        };
        // The held bytes are the ones the compare-and-swap expects to find (§6.2); the hold
        // and its notice end, the copy stays.
        let held = self
            .project
            .manifest
            .paths
            .get(rel)
            .and_then(|e| e.held.as_ref().map(|h| h.md5.clone()));
        if let Some(held) = held {
            self.project.end_hold(rel);
            if let Some(entry) = self.project.manifest.paths.get_mut(rel) {
                entry.md5 = held;
            }
        }
        self.project.dirty.files.insert(file);
        self.materialize()
    }

    /// Seal and materialize what is due, write the fold cache, clear the lock's content and
    /// release the lock (§3.6). The pending files hold every save already.
    fn close(&mut self) -> Result<(), StoreError> {
        let mut result = Ok(());
        if self.read_only().is_none() {
            result = self.checkpoint();
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
        // Everything the project folder gets from this session is written before the lock
        // is released: the draft writer ends first (§5.0).
        self.drafts.stop();
        if self.read_only().is_none()
            && let Some(lock) = self.lock.as_mut()
        {
            let _ = lock.set_content(b"");
        }
        self.lock = None;
        result
    }
}
