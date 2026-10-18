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
use tc_journal::consolidate::{OwnPack, consolidate, consolidation_groups};
use tc_journal::index::load_pack;
use tc_journal::line::encode_line;
use tc_journal::pack::{EncodedPack, PackPath, PackRead, PackSpec, YearMonth, read_pack, seal};
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

    /// K1 to K6 (§5.11). Pending files of a newer format, of another project or unreadable
    /// are left as they are.
    pub(crate) fn seal(&mut self) -> Result<SealReport, StoreError> {
        let mut report = SealReport::default();
        // K1: every verified line of every pending file this session knows.
        let mut files: Vec<(String, Vec<u8>, BTreeSet<ChangeId>)> = Vec::new();
        let mut lines: Vec<Verified> = Vec::new();
        let known: Vec<String> = self.journal.pending.keys().cloned().collect();
        for rel in known {
            if self.journal.unreadable.contains(&rel) {
                continue;
            }
            let abs = self.folder.path(&rel);
            let bytes = match self.vfs.read(&abs) {
                Ok(bytes) => bytes,
                Err(e) if is_not_found(&e) => {
                    self.journal.pending.remove(&rel);
                    continue;
                }
                Err(_) => continue,
            };
            let read = read_pending(&bytes, self.id);
            if read.status != PendingStatus::Valid {
                continue;
            }
            let ids = read.entries.iter().map(Verified::id).collect();
            lines.extend(read.entries);
            files.push((rel, bytes, ids));
        }
        if files.is_empty() {
            return self.consolidate(report);
        }
        let sealed = seal(&self.pack_spec(), &lines)
            .map_err(|e| StoreError::Other(format!("the seal could not build a pack: {e}")))?;
        // K2 for the lines no pack can hold.
        let mut set_aside: BTreeSet<ChangeId> = BTreeSet::new();
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
                set_aside.insert(line.id());
            }
            report.set_aside = sealed.set_aside.clone();
        }
        drop(lines);
        // K2 and K3 for each pack.
        let mut written: Vec<Written> = Vec::new();
        for pack in &sealed.packs {
            let expected: BTreeSet<ChangeId> = pack.changes.iter().copied().collect();
            let w = self.write_pack(pack, &expected)?;
            report.packs.push(w.rel.clone());
            written.push(w);
        }
        // K4: a pending file goes only when a verified pack holds every change of it.
        let held: BTreeSet<ChangeId> = written
            .iter()
            .flat_map(|w| w.ids.iter().copied())
            .chain(set_aside.iter().copied())
            .collect();
        let mut dirs = DirSet::new();
        for (rel, bytes, ids) in files {
            if !ids.is_subset(&held) {
                continue;
            }
            let number = self.journal.pending.get(&rel).and_then(|p| p.number);
            if !self.relocate(&rel, JournalKind::Pending, number, &bytes, &written) {
                continue;
            }
            // A pending file a scanner still holds stays; the next open removes it (O7).
            let removed = atomic_remove_if_free(
                &*self.vfs,
                &self.folder.path(&rel),
                SyncDir::Deferred(&mut dirs),
            )?;
            self.journal.pending.remove(&rel);
            if self.index.sections.remove(&rel).is_some() {
                self.index.changed = true;
            }
            if removed {
                report.removed.push(rel);
            }
        }
        dirs.sync(&*self.vfs)?;
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
        // K6.
        self.consolidate(report)
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
            .collect())
    }
}
