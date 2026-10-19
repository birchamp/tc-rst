//! Seal (`docs/STORAGE.md` §5.11, K1 to K6) and consolidate (§5.12, Q1 to Q4).
//!
//! A seal moves every verified line of the pending files into packs of this device, reads
//! each new pack back from disk before any pending file goes, and removes only pending files
//! all of whose changes a verified pack holds (or that K2 set aside). A consolidation merges
//! this device's packs the same way: the new pack is read back and must hold every change of
//! the packs it replaces before they go. Either way a crash leaves the old files, the new
//! ones or both, and the open's rules (§6.1 O7) finish the work; no step can lose a change.
//!
//! The fold's values of a file that goes are read from the new pack from then on
//! ([`crate::source::FileSource::moved`]): the actor keeps where each change lies, not its
//! bytes (§2.6).

use crate::atomic::{
    DirSet, SyncDir, atomic_create_dirs, atomic_remove, atomic_remove_if_free, atomic_write,
};
use crate::error::StoreError;
use crate::layout::{PACKS, QUARANTINE};
use crate::load::{JournalFile, JournalKind, relocation};
use crate::project::{PackInfo, Project};
use crate::vfs::is_not_found;
use std::collections::BTreeSet;
use tc_journal::consolidate::{OwnPack, consolidation_groups};
use tc_journal::fold::Notice;
use tc_journal::index::load_pack;
use tc_journal::line::encode_line;
use tc_journal::pack::{EncodedPack, PackPath, PackSpec, YearMonth, read_pack, seal};
use tc_journal::pending::{PendingStatus, read_pending};
use tc_journal::{ChangeId, PackName, Verified};

/// What a seal did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SealReport {
    /// The packs written, by repository path.
    pub packs: Vec<String>,
    /// The pending files removed.
    pub removed: Vec<String>,
    /// Lines no pack can hold, set aside in `.tc/quarantine/` (K1).
    pub set_aside: Vec<ChangeId>,
    /// The packs a consolidation wrote and the packs they replaced.
    pub consolidated: Vec<(String, Vec<String>)>,
    /// Pending files the fold holds the changes of that the seal could not take: a program
    /// holds the file open, or its lines no longer verify. They stay as they are and the
    /// snapshot shows their changes while no pack holds them, so the checkpoint makes no
    /// commit (§5.13 C3); the next checkpoint reads them again.
    pub unsealed: Vec<String>,
}

/// Pending files one step of the seal's K4 removes (each is an unlink; the folder is flushed
/// once at the end).
const REMOVALS_PER_STEP: usize = 4;
/// Pending files one step of the seal's K1 reads.
const READS_PER_STEP: usize = 2;

/// Where a seal is.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum SealPhase {
    /// Pending files to read (K1).
    Read,
    /// The packs to build (K1, K2 for what no pack can hold).
    Build,
    /// Packs to write (K2, K3).
    Pack,
    /// Pending files to remove (K4).
    Remove,
    /// The ledger (K5).
    Ledger,
    /// The consolidation (K6): the groups to merge are listed.
    #[default]
    Consolidate,
    /// The next group goes to a worker thread (Q1 to Q3).
    ConsNext,
    /// The worker is reading, merging, writing and verifying; saves are served meanwhile.
    ConsWait,
    /// The packs the new pack absorbed go, a few a step (Q4).
    ConsApply,
    Done,
}

/// A seal in progress ([`Project::seal_start`], [`Project::seal_step`]).
#[derive(Default)]
pub(crate) struct SealRun {
    pub(crate) report: SealReport,
    phase: SealPhase,
    /// The pending files not read yet (the next at the end).
    unread: Vec<String>,
    /// The pending files read at K1, with their bytes and change ids.
    files: Vec<(String, Vec<u8>, BTreeSet<ChangeId>)>,
    /// Their verified lines.
    lines: Vec<Verified>,
    packs: Vec<EncodedPack>,
    written: Vec<Written>,
    set_aside: BTreeSet<ChangeId>,
    /// The changes a verified pack holds or that were set aside: a pending file whose changes
    /// are all in it may go.
    held: BTreeSet<ChangeId>,
    dirs: DirSet,
    /// The groups of packs the consolidation still has to merge (repository paths).
    groups: std::collections::VecDeque<Vec<String>>,
    /// The worker merging the current group.
    worker: Option<std::sync::mpsc::Receiver<Result<crate::cons::ConsDone, StoreError>>>,
    /// What the worker made, to be applied.
    applying: Option<crate::cons::ConsDone>,
    /// The old packs removed so far, by the new pack that absorbed them.
    gone: Vec<(String, String)>,
}

/// Old packs one step of Q4 removes.
const CONS_REMOVALS_PER_STEP: usize = 8;

/// A pack written and read back.
struct Written {
    rel: String,
    number: u32,
    bytes: Vec<u8>,
    ids: BTreeSet<ChangeId>,
}

impl Project {
    /// The spec of a pack this device seals now.
    fn pack_spec(&self) -> PackSpec {
        PackSpec {
            project: self.id,
            sealer: self.device,
            sealed: self.clock.now(),
        }
    }

    /// K2 and K3 (Q2 and Q3) for one pack: written durably, read back, verified, and its
    /// ids compared with `expected`. A pack that does not read back as written is removed
    /// and the seal stops with nothing else removed.
    fn write_pack(
        &mut self,
        pack: &EncodedPack,
        expected: &BTreeSet<ChangeId>,
    ) -> Result<Written, StoreError> {
        let rel = pack.path.to_string();
        let (dir, name) = self.folder.dir_and_name(&rel);
        atomic_create_dirs(&*self.vfs, &dir)?;
        atomic_write(&*self.vfs, &dir, name, &pack.bytes, SyncDir::Now)
            .map_err(|e| e.into_inner())?;
        let abs = self.folder.path(&rel);
        let back = self
            .vfs
            .read(&abs)
            .map_err(|e| StoreError::io("read back", &abs, e))?;
        let read = read_pack(&rel, &back, self.id);
        let ids: BTreeSet<ChangeId> = read.entries.iter().map(Verified::id).collect();
        let replaces: Vec<PackName> = read
            .header
            .as_ref()
            .map(|h| h.replaces.clone())
            .unwrap_or_default();
        if !read.is_valid() || !expected.is_subset(&ids) {
            // A pack whose bytes on disk are not the ones written is no copy of anything.
            let _ = atomic_remove(&*self.vfs, &abs, SyncDir::Now);
            return Err(StoreError::Other(format!(
                "the new pack {rel} did not read back as written; the pending files are kept"
            )));
        }
        // K3: the verified pack is stored as a blob, so it can be restored even before a
        // commit holds it. A blob that cannot be stored now is stored by the checkpoint.
        let blob = self.git.as_ref().and_then(|git| git.write_blob(&back).ok());
        let number = self.source.add(&abs);
        let loaded = load_pack(&rel, &back, self.id, number);
        if let Some(section) = loaded.section {
            self.index.sections.insert(rel.clone(), section);
            self.index.changed = true;
        }
        let replaces_copy = replaces.clone();
        self.journal.packs.insert(
            rel.clone(),
            PackInfo {
                id: read.id,
                size: back.len() as u64,
                fingerprint: tc_journal::index::fingerprint(&back),
                number: Some(number),
                valid: true,
                sealer: Some(self.device),
                replaces: replaces_copy,
                ids: Vec::new(),
                blob,
                seen: None,
                damaged: false,
            },
        );
        self.dirty.metadata = true;
        Ok(Written {
            rel,
            number,
            bytes: back,
            ids,
        })
    }

    /// Moves the values of `rel` (file `number`, bytes `bytes`) to the packs `to` before
    /// `rel` goes. `false` when some change of `rel` is in none of them: then `rel` stays.
    fn relocate(
        &self,
        rel: &str,
        kind: JournalKind,
        number: Option<u32>,
        bytes: &[u8],
        to: &[Written],
    ) -> bool {
        let Some(number) = number else {
            // A file this session wrote: the fold holds its values in memory.
            return true;
        };
        let from = JournalFile {
            rel: rel.to_owned(),
            abs: self.folder.path(rel),
            kind,
            size: bytes.len() as u64,
        };
        let targets: Vec<(JournalFile, u32, &[u8])> = to
            .iter()
            .map(|w| {
                (
                    JournalFile {
                        rel: w.rel.clone(),
                        abs: self.folder.path(&w.rel),
                        kind: JournalKind::Pack,
                        size: w.bytes.len() as u64,
                    },
                    w.number,
                    w.bytes.as_slice(),
                )
            })
            .collect();
        let refs: Vec<(&JournalFile, u32, &[u8])> =
            targets.iter().map(|(f, n, b)| (f, *n, *b)).collect();
        match relocation(self.id, (&from, bytes), &refs) {
            Some(map) => {
                self.source.moved(number, map);
                true
            }
            None => false,
        }
    }

    /// The start of a seal: the pending files this session knows are the ones it will read, a
    /// few at a time ([`Project::seal_step`]), then pack, write, verify and remove. The actor
    /// serves the commands that wait between steps, so a save waits for one step and not for
    /// the seal (PERFORMANCE.md: 25 ms p95 while a checkpoint runs). Saves that arrive meanwhile
    /// are pending files this run never saw, so no step touches them.
    pub(crate) fn seal_start(&mut self) -> Result<SealRun, StoreError> {
        let mut run = SealRun::default();
        let mut known: Vec<String> = self.journal.pending.keys().cloned().collect();
        known.reverse();
        run.unread = known;
        run.phase = SealPhase::Read;
        Ok(run)
    }

    /// K1: some of the pending files are read and their verified lines kept.
    fn seal_read(&mut self, run: &mut SealRun) {
        for _ in 0..READS_PER_STEP {
            let Some(rel) = run.unread.pop() else {
                break;
            };
            if self.journal.unreadable.contains(&rel) {
                self.leave_unsealed(run, &rel, "it cannot be read");
                continue;
            }
            let abs = self.folder.path(&rel);
            let bytes = match self.vfs.read(&abs) {
                Ok(bytes) => bytes,
                Err(e) if is_not_found(&e) => {
                    self.journal.pending.remove(&rel);
                    continue;
                }
                Err(e) => {
                    self.leave_unsealed(run, &rel, &StoreError::io("read", &abs, e).to_string());
                    continue;
                }
            };
            let read = read_pending(&bytes, self.id);
            if read.status != PendingStatus::Valid {
                self.leave_unsealed(run, &rel, "its lines no longer verify");
                continue;
            }
            self.shared.remove_notices_matching(
                |n| matches!(n, Notice::FileUnreadable { path, .. } if **path == *rel),
            );
            let ids = read.entries.iter().map(Verified::id).collect();
            run.lines.extend(read.entries);
            run.files.push((rel, bytes, ids));
        }
        if run.unread.is_empty() {
            run.phase = if run.files.is_empty() {
                SealPhase::Consolidate
            } else {
                SealPhase::Build
            };
        }
    }

    /// K1 could not take `rel`. When the fold holds its changes (it was read, valid, at the
    /// open or when this session wrote it) the seal is incomplete: the file is reported, the
    /// person is told once, and the checkpoint makes no commit. A file of a newer format was
    /// never read, so the fold holds nothing of it and nothing is lacking.
    fn leave_unsealed(&mut self, run: &mut SealRun, rel: &str, reason: &str) {
        let folded = self
            .journal
            .pending
            .get(rel)
            .is_some_and(|p| p.status == PendingStatus::Valid);
        if !folded {
            return;
        }
        run.report.unsealed.push(rel.to_owned());
        let told =
            self.shared.notices().iter().any(
                |n| matches!(&n.notice, Notice::FileUnreadable { path, .. } if **path == *rel),
            );
        if !told {
            self.notice(
                Notice::FileUnreadable {
                    path: rel.into(),
                    reason: reason.into(),
                },
                None,
            );
        }
    }

    /// The pack of the lines read, and K2 for the lines no pack can hold.
    fn seal_build(&mut self, run: &mut SealRun) -> Result<(), StoreError> {
        let lines = std::mem::take(&mut run.lines);
        let sealed = seal(&self.pack_spec(), &lines)
            .map_err(|e| StoreError::Other(format!("the seal could not build a pack: {e}")))?;
        if !sealed.set_aside.is_empty() {
            let dir = self.folder.path(QUARANTINE);
            atomic_create_dirs(&*self.vfs, &dir)?;
            for line in lines.iter().filter(|l| sealed.set_aside.contains(&l.id())) {
                let name = format!("{}.line", line.id().to_hex());
                atomic_write(
                    &*self.vfs,
                    &dir,
                    &name,
                    &encode_line(line.id(), line.raw()),
                    SyncDir::Now,
                )
                .map_err(|e| e.into_inner())?;
                run.set_aside.insert(line.id());
            }
            run.report.set_aside = sealed.set_aside.clone();
        }
        drop(lines);
        run.packs = sealed.packs;
        run.phase = if run.packs.is_empty() {
            run.held = run.set_aside.clone();
            SealPhase::Remove
        } else {
            SealPhase::Pack
        };
        Ok(())
    }

    /// One step of a seal: a pack written and read back (K2, K3), a few pending files removed
    /// (K4), the ledger (K5), or the consolidation (K6). `false` when the seal is done.
    pub(crate) fn seal_step(&mut self, run: &mut SealRun) -> Result<bool, StoreError> {
        match run.phase {
            SealPhase::Read => {
                self.seal_read(run);
                Ok(true)
            }
            SealPhase::Build => {
                self.seal_build(run)?;
                Ok(true)
            }
            SealPhase::Pack => {
                // K2 and K3 for each pack.
                let pack = run.packs[run.written.len()].clone();
                let expected: BTreeSet<ChangeId> = pack.changes.iter().copied().collect();
                let w = self.write_pack(&pack, &expected)?;
                run.report.packs.push(w.rel.clone());
                run.written.push(w);
                if run.written.len() == run.packs.len() {
                    run.held = run
                        .written
                        .iter()
                        .flat_map(|w| w.ids.iter().copied())
                        .chain(run.set_aside.iter().copied())
                        .collect();
                    run.phase = SealPhase::Remove;
                }
                Ok(true)
            }
            SealPhase::Remove => {
                // K4: a pending file goes only when a verified pack holds every change of it.
                for _ in 0..REMOVALS_PER_STEP {
                    let Some((rel, bytes, ids)) = run.files.pop() else {
                        break;
                    };
                    if !ids.is_subset(&run.held) {
                        continue;
                    }
                    let number = self.journal.pending.get(&rel).and_then(|p| p.number);
                    if !self.relocate(&rel, JournalKind::Pending, number, &bytes, &run.written) {
                        continue;
                    }
                    // A pending file a scanner still holds stays; the next open removes it (O7).
                    let removed = atomic_remove_if_free(
                        &*self.vfs,
                        &self.folder.path(&rel),
                        SyncDir::Deferred(&mut run.dirs),
                    )?;
                    self.journal.pending.remove(&rel);
                    if self.index.sections.remove(&rel).is_some() {
                        self.index.changed = true;
                    }
                    if removed {
                        run.report.removed.push(rel);
                    }
                }
                if run.files.is_empty() {
                    run.dirs.sync(&*self.vfs)?;
                    run.phase = SealPhase::Ledger;
                }
                Ok(true)
            }
            SealPhase::Ledger => {
                // K5.
                let last_seq = self.durable_max_seq;
                if last_seq > 0 {
                    self.app.record_ledger(
                        self.device,
                        self.id,
                        self.folder.root(),
                        last_seq,
                        self.clock.now(),
                    )?;
                }
                run.phase = SealPhase::Consolidate;
                Ok(true)
            }
            SealPhase::Consolidate => {
                // K6: which groups, then each group's Q1 to Q3 on a worker.
                run.groups = self.consolidation_plan();
                run.phase = SealPhase::ConsNext;
                Ok(true)
            }
            SealPhase::ConsNext => {
                let Some(rels) = run.groups.pop_front() else {
                    run.phase = SealPhase::Done;
                    return Ok(false);
                };
                run.worker = Some(self.start_consolidation(rels));
                run.phase = SealPhase::ConsWait;
                Ok(true)
            }
            SealPhase::ConsWait => {
                let Some(worker) = run.worker.as_ref() else {
                    run.phase = SealPhase::ConsNext;
                    return Ok(true);
                };
                match worker.recv_timeout(std::time::Duration::from_millis(2)) {
                    Ok(done) => {
                        run.worker = None;
                        let done = done?;
                        self.cons_adopt(&done);
                        run.applying = Some(done);
                        run.phase = SealPhase::ConsApply;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        run.worker = None;
                        return Err(StoreError::Other(
                            "the consolidation stopped before it ended".into(),
                        ));
                    }
                }
                Ok(true)
            }
            SealPhase::ConsApply => {
                self.cons_apply(run)?;
                Ok(true)
            }
            SealPhase::Done => Ok(false),
        }
    }

    /// Which of this device's packs to merge (§4.7 "When"), as groups of repository paths.
    fn consolidation_plan(&self) -> std::collections::VecDeque<Vec<String>> {
        let own: Vec<(String, PackPath, u64)> = self
            .journal
            .packs
            .iter()
            .filter(|(_, p)| p.valid && p.sealer == Some(self.device))
            .filter_map(|(rel, p)| Some((rel.clone(), PackPath::parse(rel)?, p.size)))
            .collect();
        let listing: Vec<OwnPack> = own
            .iter()
            .map(|(_, path, size)| OwnPack {
                name: path.name,
                month: path.month,
                size: *size,
            })
            .collect();
        consolidation_groups(&listing, YearMonth::of(self.clock.now()))
            .into_iter()
            .map(|group| {
                own.iter()
                    .filter(|(_, path, _)| group.contains(&path.name))
                    .map(|(rel, _, _)| rel.clone())
                    .collect()
            })
            .collect()
    }

    /// Q1 to Q3 of one group on a thread of its own: the actor serves saves while it reads,
    /// merges, writes (in flushed pieces, behind the saves in flight) and reads back.
    fn start_consolidation(
        &self,
        rels: Vec<String>,
    ) -> std::sync::mpsc::Receiver<Result<crate::cons::ConsDone, StoreError>> {
        let (tx, rx) = std::sync::mpsc::channel();
        let job = crate::cons::ConsJob {
            vfs: std::sync::Arc::new(crate::pace::PacedVfs::new(
                self.vfs.clone(),
                self.shared.gate.clone(),
                self.counters.clone(),
            )),
            folder: self.folder.clone(),
            project: self.id,
            spec: self.pack_spec(),
            rels,
            source: self.source.clone(),
            hooks: self.shared.hooks.clone(),
        };
        let spawned = std::thread::Builder::new()
            .name("tc-store consolidation".into())
            .spawn({
                let tx = tx.clone();
                move || {
                    let _ = tx.send(crate::cons::run(job));
                }
            });
        if spawned.is_err() {
            let _ = tx.send(Err(StoreError::Other(
                "a thread for the consolidation could not start".into(),
            )));
        }
        rx
    }

    /// The new packs a worker wrote are known to the session before any old pack goes.
    fn cons_adopt(&mut self, done: &crate::cons::ConsDone) {
        for pack in &done.new {
            self.journal.packs.insert(
                pack.rel.clone(),
                PackInfo {
                    id: pack.id,
                    size: pack.size,
                    fingerprint: pack.fingerprint,
                    number: Some(pack.number),
                    valid: true,
                    sealer: Some(self.device),
                    replaces: pack.replaces.clone(),
                    ids: Vec::new(),
                    // The checkpoint that follows stores the pack as a blob with the rest.
                    blob: None,
                    seen: None,
                    damaged: false,
                },
            );
            if let Some(section) = &pack.section {
                self.index
                    .sections
                    .insert(pack.rel.clone(), section.clone());
                self.index.changed = true;
            }
        }
        if !done.new.is_empty() {
            self.dirty.metadata = true;
        }
    }

    /// Q4, a few old packs a step: the fold's values move to the new pack, then the old file
    /// goes. A pack a scanner still holds stays; the next open removes it (O7).
    fn cons_apply(&mut self, run: &mut SealRun) -> Result<(), StoreError> {
        let Some(done) = run.applying.as_mut() else {
            run.phase = SealPhase::ConsNext;
            return Ok(());
        };
        for _ in 0..CONS_REMOVALS_PER_STEP {
            let Some(old) = done.old.pop() else {
                break;
            };
            let Some(map) = old.map else {
                continue;
            };
            if let Some(number) = self.journal.packs.get(&old.rel).and_then(|p| p.number) {
                self.source.moved(number, map);
            }
            let removed = atomic_remove_if_free(
                &*self.vfs,
                &self.folder.path(&old.rel),
                SyncDir::Deferred(&mut run.dirs),
            )?;
            self.journal.packs.remove(&old.rel);
            if self.index.sections.remove(&old.rel).is_some() {
                self.index.changed = true;
            }
            if removed {
                run.gone.push((old.new_rel, old.rel));
            }
        }
        if done.old.is_empty() {
            run.dirs.sync(&*self.vfs)?;
            let done = run.applying.take().unwrap_or_default();
            for pack in done.new {
                let gone: Vec<String> = run
                    .gone
                    .iter()
                    .filter(|(new, _)| *new == pack.rel)
                    .map(|(_, old)| old.clone())
                    .collect();
                run.report.consolidated.push((pack.rel, gone));
            }
            run.gone.clear();
            run.phase = SealPhase::ConsNext;
        }
        Ok(())
    }

    /// The journal files in the folder now that this session does not know: another
    /// installation writing here (§5.0, a second writer), or files a crash left that the
    /// open already took.
    pub(crate) fn unknown_files(&self) -> Result<Vec<JournalFile>, StoreError> {
        let listed = crate::load::list(&*self.vfs, &self.folder)
            .map_err(|e| StoreError::io("list", &self.folder.path(PACKS), e))?;
        Ok(listed
            .into_iter()
            .filter(|f| match f.kind {
                JournalKind::Pack => !self.journal.packs.contains_key(&f.rel),
                JournalKind::Pending => !self.journal.pending.contains_key(&f.rel),
            })
            .filter(|f| !self.journal.unreadable.contains(&f.rel))
            .collect())
    }
}
