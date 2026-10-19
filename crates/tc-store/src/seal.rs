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
    DirSet, HeldRetry, SyncDir, atomic_create_dirs, atomic_remove, atomic_remove_if_free,
    atomic_write, rename_held,
};
use crate::error::StoreError;
use crate::layout::{PACKS, QUARANTINE, escaped};
use crate::load::{JournalFile, JournalKind, relocation};
use crate::project::{PackInfo, Project};
use crate::vfs::is_not_found;
use std::collections::BTreeSet;
use tc_journal::change::parse_change;
use tc_journal::consolidate::{OwnPack, consolidate, consolidation_groups};
use tc_journal::fold::Notice;
use tc_journal::index::load_pack;
use tc_journal::line::encode_line;
use tc_journal::pack::{
    EncodedPack, PackError, PackPath, PackRead, PackSpec, YearMonth, read_pack, seal,
};
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
    /// holds the file open, or the file lost a change whose bytes the fold cannot give either.
    /// They stay as they are and the snapshot shows their changes while no pack holds them, so
    /// the checkpoint makes no commit (§5.13 C3); the next checkpoint reads them again.
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
    /// The consolidation (K6).
    #[default]
    Consolidate,
    Done,
}

/// A seal in progress ([`Project::seal_start`], [`Project::seal_step`]).
#[derive(Default)]
pub(crate) struct SealRun {
    pub(crate) report: SealReport,
    phase: SealPhase,
    /// The pending files not read yet (the next at the end).
    unread: Vec<String>,
    /// The pending files read at K1.
    files: Vec<ReadFile>,
    /// Their verified lines.
    lines: Vec<Verified>,
    packs: Vec<EncodedPack>,
    written: Vec<Written>,
    set_aside: BTreeSet<ChangeId>,
    /// The changes a verified pack holds or that were set aside: a pending file whose changes
    /// are all in it may go.
    held: BTreeSet<ChangeId>,
    dirs: DirSet,
}

/// A pending file K1 took.
struct ReadFile {
    rel: String,
    found: Found,
    /// The changes the seal packs for it: the lines it holds, and those the fold took from it
    /// that it lost, from the fold's own bytes.
    ids: BTreeSet<ChangeId>,
}

/// What K1 found of a pending file.
enum Found {
    /// Its bytes, which hold every change the fold took from it.
    Whole(Vec<u8>),
    /// It lost changes the fold took from it: the fold reads their values where it always did
    /// (memory, or the handle the open kept), so nothing is moved to the new pack's offsets.
    Partial,
    /// Its lines no longer verify (cut inside a line, or changed in one). The changes it lost
    /// are read as for a partial file, and K4 moves its bytes to `.tc/quarantine/`, as the open
    /// does with a damaged pending file (§4.4).
    Damaged { reason: PackError, salvaged: u32 },
    /// Another program put a file of a newer format or of another project at its name. The
    /// fold's changes of it are sealed from memory; the file stays, never removed, and the fold
    /// takes nothing from it (the next open decides it, §4.2).
    NotOurs(PendingStatus),
    /// Another program removed it.
    Gone,
}

/// A pack written and read back.
struct Written {
    rel: String,
    number: u32,
    bytes: Vec<u8>,
    ids: BTreeSet<ChangeId>,
    /// The names its header says it replaces.
    replaces: Vec<PackName>,
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
            replaces,
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

    /// K1: some of the pending files are read and their verified lines kept. What the seal
    /// takes of a file is compared with what the fold took from it, whatever happened to the
    /// file: a change the file no longer holds (another program removed the file, cut it short
    /// at a line or inside one, changed a line, or put another kind of file at its name) is
    /// taken from the fold's own bytes when it can give them, and else the file is left
    /// unsealed, so no commit shows a change no pack holds (§5.13 C3).
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
            // A damaged file gives the lines that still verify; one of a newer format or of
            // another project gives none.
            let (bytes, mut lines, status) = match self.vfs.read(&abs) {
                Ok(bytes) => {
                    let read = read_pending(&bytes, self.id);
                    (Some(bytes), read.entries, read.status)
                }
                Err(e) if is_not_found(&e) => (None, Vec::new(), PendingStatus::Valid),
                Err(e) => {
                    self.leave_unsealed(run, &rel, &StoreError::io("read", &abs, e).to_string());
                    continue;
                }
            };
            let not_ours = matches!(
                status,
                PendingStatus::Newer { .. } | PendingStatus::Foreign { .. }
            );
            if not_ours && self.folded_from(&rel).is_empty() {
                // A file of a newer format the fold never read: nothing of it is lacking.
                continue;
            }
            let salvaged = u32::try_from(lines.len()).unwrap_or(u32::MAX);
            let mut ids: BTreeSet<ChangeId> = lines.iter().map(Verified::id).collect();
            let lost: Vec<ChangeId> = self
                .folded_from(&rel)
                .into_iter()
                .filter(|id| !ids.contains(id))
                .collect();
            if !lost.is_empty() {
                // The derived files that show a change the fold can no longer read are kept by
                // the ledger, which asks the fold for every version they show (§6.2).
                let Some(again) = self.lines_in_memory(&lost) else {
                    let reason = if bytes.is_none() {
                        "another program removed it"
                    } else {
                        "another program removed saves from it"
                    };
                    self.leave_unsealed(run, &rel, reason);
                    continue;
                };
                self.notice(
                    Notice::PendingWrittenAgain {
                        file: rel.as_str().into(),
                        changes: u32::try_from(again.len()).unwrap_or(u32::MAX),
                    },
                    None,
                );
                ids.extend(lost.iter().copied());
                lines.extend(again);
            }
            let found = match (bytes, status) {
                (None, _) if ids.is_empty() => {
                    // Gone, and the fold holds nothing of it (a file of a newer format).
                    self.journal.pending.remove(&rel);
                    continue;
                }
                (None, _) => Found::Gone,
                (Some(_), PendingStatus::Damaged(reason)) => Found::Damaged { reason, salvaged },
                (
                    Some(_),
                    status @ (PendingStatus::Newer { .. } | PendingStatus::Foreign { .. }),
                ) => Found::NotOurs(status),
                (Some(_), PendingStatus::Valid) if !lost.is_empty() => Found::Partial,
                (Some(bytes), PendingStatus::Valid) => Found::Whole(bytes),
            };
            self.shared.remove_notices_matching(
                |n| matches!(n, Notice::FileUnreadable { path, .. } if **path == *rel),
            );
            run.lines.extend(lines);
            run.files.push(ReadFile { rel, found, ids });
        }
        if run.unread.is_empty() {
            run.phase = if run.files.is_empty() {
                SealPhase::Consolidate
            } else {
                SealPhase::Build
            };
        }
    }

    /// K4 for a pending file whose lines no longer verify: its bytes move to `.tc/quarantine/`,
    /// as the open moves a damaged pending file (§4.4). `false` when another program holds it
    /// (the next open quarantines it) or removed it.
    fn quarantine_if_free(&mut self, rel: &str, dirs: &mut DirSet) -> Result<bool, StoreError> {
        let dir = self.folder.path(QUARANTINE);
        atomic_create_dirs(&*self.vfs, &dir)?;
        let from = self.folder.path(rel);
        let to = dir.join(escaped(rel));
        match rename_held(&*self.vfs, &from, &to, &HeldRetry::SAVE) {
            Ok(()) => {
                dirs.add_dir(&dir);
                if let Some(parent) = from.parent() {
                    dirs.add_dir(parent);
                }
                Ok(true)
            }
            Err(e) if self.vfs.is_held(&e) || is_not_found(&e) => Ok(false),
            Err(e) => Err(StoreError::io("move to quarantine", &from, e)),
        }
    }

    /// The changes the fold took from the pending file `rel`: none from a file it did not read
    /// (a newer format).
    fn folded_from(&self, rel: &str) -> Vec<ChangeId> {
        match self.journal.pending.get(rel) {
            Some(p) if p.status == PendingStatus::Valid => p.ids.clone(),
            _ => Vec::new(),
        }
    }

    /// The lines of `ids` from the fold's own bytes, each checked against its id; `None` when
    /// the fold cannot give one of them (its value lay in the file that lost it).
    fn lines_in_memory(&self, ids: &[ChangeId]) -> Option<Vec<Verified>> {
        ids.iter()
            .map(|id| {
                let raw = self.fold.raw(id)?;
                (ChangeId::of(&raw) == *id).then(|| parse_change(*id, raw))
            })
            .collect()
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
                    let Some(ReadFile { rel, found, ids }) = run.files.pop() else {
                        break;
                    };
                    if !ids.is_subset(&run.held) {
                        continue;
                    }
                    if let Found::Whole(bytes) = &found {
                        let number = self.journal.pending.get(&rel).and_then(|p| p.number);
                        if !self.relocate(&rel, JournalKind::Pending, number, bytes, &run.written) {
                            continue;
                        }
                    }
                    // A pending file a scanner still holds stays; the next open removes it (O7),
                    // or quarantines it when it does not verify.
                    let removed = match found {
                        Found::Whole(_) | Found::Partial => atomic_remove_if_free(
                            &*self.vfs,
                            &self.folder.path(&rel),
                            SyncDir::Deferred(&mut run.dirs),
                        )?,
                        Found::Damaged { reason, salvaged } => {
                            let moved = self.quarantine_if_free(&rel, &mut run.dirs)?;
                            if moved {
                                self.notice(
                                    Notice::QuarantinedPending {
                                        file: rel.as_str().into(),
                                        reason,
                                        salvaged,
                                    },
                                    None,
                                );
                            }
                            moved
                        }
                        Found::NotOurs(status) => {
                            if let Some(info) = self.journal.pending.get_mut(&rel) {
                                info.status = status;
                                info.ids.clear();
                            }
                            if self.index.sections.remove(&rel).is_some() {
                                self.index.changed = true;
                            }
                            continue;
                        }
                        Found::Gone => false,
                    };
                    self.journal.pending.remove(&rel);
                    if self.index.sections.remove(&rel).is_some() {
                        self.index.changed = true;
                    }
                    if removed {
                        run.report.removed.push(rel);
                    } else {
                        self.journal.left.insert(rel);
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
                let last_seq = self.next_seq.saturating_sub(1);
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
                // K6.
                let report = std::mem::take(&mut run.report);
                run.report = self.consolidate(report)?;
                run.phase = SealPhase::Done;
                Ok(false)
            }
            SealPhase::Done => Ok(false),
        }
    }

    /// Q1 to Q4 (§5.12) when §4.7 says so.
    fn consolidate(&mut self, mut report: SealReport) -> Result<SealReport, StoreError> {
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
        let groups = consolidation_groups(&listing, YearMonth::of(self.clock.now()));
        for group in groups {
            let rels: Vec<String> = own
                .iter()
                .filter(|(_, path, _)| group.contains(&path.name))
                .map(|(rel, _, _)| rel.clone())
                .collect();
            // Q1: read and verify each; a pack that fails stays as it is.
            let mut reads: Vec<(String, Vec<u8>, PackRead)> = Vec::new();
            for rel in rels {
                let abs = self.folder.path(&rel);
                let Ok(bytes) = self.vfs.read(&abs) else {
                    continue;
                };
                let read = read_pack(&rel, &bytes, self.id);
                if read.is_valid() {
                    reads.push((rel, bytes, read));
                }
            }
            if reads.len() < 2 {
                continue;
            }
            let refs: Vec<&PackRead> = reads.iter().map(|(_, _, r)| r).collect();
            let new_packs = consolidate(&self.pack_spec(), &refs)
                .map_err(|e| StoreError::Other(format!("consolidation: {e}")))?;
            for pack in new_packs {
                // Q2, Q3: the packs it absorbed are those whose every change it holds; the
                // pack read back must hold them all, and only packs its header lists go.
                let holds: BTreeSet<ChangeId> = pack.changes.iter().copied().collect();
                let absorbed: Vec<&(String, Vec<u8>, PackRead)> = reads
                    .iter()
                    .filter(|(_, _, r)| r.entries.iter().all(|e| holds.contains(&e.id())))
                    .collect();
                let expected: BTreeSet<ChangeId> = absorbed
                    .iter()
                    .flat_map(|(_, _, r)| r.entries.iter().map(Verified::id))
                    .collect();
                let w = self.write_pack(&pack, &expected)?;
                let replaced: Vec<&(String, Vec<u8>, PackRead)> = absorbed
                    .into_iter()
                    .filter(|(_, _, r)| r.path.is_some_and(|p| w.replaces.contains(&p.name)))
                    .collect();
                let written = vec![w];
                // Q4.
                let mut dirs = DirSet::new();
                let mut gone = Vec::new();
                for (rel, bytes, _) in replaced {
                    let number = self.journal.packs.get(rel).and_then(|p| p.number);
                    if !self.relocate(rel, JournalKind::Pack, number, bytes, &written) {
                        continue;
                    }
                    // A pack a scanner still holds stays; the next open removes it (O7).
                    let removed = atomic_remove_if_free(
                        &*self.vfs,
                        &self.folder.path(rel),
                        SyncDir::Deferred(&mut dirs),
                    )?;
                    self.journal.packs.remove(rel);
                    if self.index.sections.remove(rel).is_some() {
                        self.index.changed = true;
                    }
                    if removed {
                        gone.push(rel.clone());
                    } else {
                        self.journal.left.insert(rel.clone());
                    }
                }
                dirs.sync(&*self.vfs)?;
                report.consolidated.push((written[0].rel.clone(), gone));
            }
        }
        Ok(report)
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
            .filter(|f| !self.journal.left.contains(&f.rel))
            .collect())
    }
}
