//! The expected pack set (`docs/STORAGE.md` §4.4): which packs exist is project state, not
//! whatever the working tree holds today. An antivirus program, a cloud client, a copy that
//! skipped long paths or a person freeing space can remove a pack without anyone deciding
//! anything. A pack that is missing or damaged is written back from a blob that passes P2 to
//! P4, its changes are folded, and the person is told; one that cannot be restored stops the
//! tree of a checkpoint and every removal of a derived file (a commit without it would remove
//! it from Door43 by fast-forward).
//!
//! Packs are immutable, so a pack whose size and modification time are those this session
//! last saw is not read again: reading 674 MiB at every checkpoint would break its budget.

use crate::atomic::{SyncDir, atomic_create_dirs, atomic_write};
use crate::error::StoreError;
use crate::git::Git;
use crate::layout::{PENDING, QUARANTINE};
use crate::project::{PackInfo, PendingInfo, Project};
use crate::vfs::is_not_found;
use std::collections::{BTreeMap, BTreeSet};
use tc_journal::fold::{Notice, PackLoss};
use tc_journal::index::{Facts, fingerprint, load_pack, load_pending};
use tc_journal::pack::{PackPath, PackStatus, read_pack};
use tc_journal::pending::{PendingHeader, PendingStatus, Purpose, encode_pending};
use tc_journal::{ChangeId, GitOid, PackId, PackName, RepoPath, Verified};

/// The folder packs live in, in HEAD's tree.
const PACKS_PREFIX: &str = "ingredients/tc/changes";

impl Project {
    /// Restores every expected pack that is missing or damaged (§4.4). At the open (O7) the
    /// expected set also holds the packs of HEAD's tree; at a checkpoint (C3) it holds the
    /// packs this session knows, watched by their size and time. A pack no blob can restore
    /// is listed as missing and blocks the tree.
    pub(crate) fn restore_expected_packs(&mut self, at_open: bool) -> Result<(), StoreError> {
        let Some(git) = self.git.take() else {
            return Ok(());
        };
        let result = self.restore_with(&git, at_open);
        self.git = Some(git);
        result
    }

    fn restore_with(&mut self, git: &Git, at_open: bool) -> Result<(), StoreError> {
        let mut candidates: Vec<(String, PackLoss)> = Vec::new();
        // The packs this session knows: gone, changed or already damaged.
        let known: Vec<(String, PackInfo)> = self
            .journal
            .packs
            .iter()
            .map(|(rel, info)| (rel.clone(), info.clone()))
            .collect();
        for (rel, info) in known {
            let abs = self.folder.path(&rel);
            let meta = match self.vfs.metadata(&abs) {
                Ok(meta) => meta,
                Err(e) if is_not_found(&e) => {
                    candidates.push((rel, PackLoss::Missing));
                    continue;
                }
                Err(_) => {
                    // A file that cannot be looked at is not damage (§4.4).
                    self.journal.unreadable.insert(rel);
                    continue;
                }
            };
            let now = (meta.len, meta.mtime_ns);
            if !info.valid {
                if info.damaged {
                    candidates.push((rel, PackLoss::Damaged));
                }
                continue;
            }
            match info.seen {
                Some(seen) if seen == now => {}
                None if meta.len == info.size => self.mark_seen(&rel, now),
                _ => match self.vfs.read(&abs) {
                    Ok(bytes)
                        if fingerprint(&bytes) == info.fingerprint
                            && read_pack(&rel, &bytes, self.id).is_valid() =>
                    {
                        self.mark_seen(&rel, now);
                    }
                    Ok(_) => candidates.push((rel, PackLoss::Damaged)),
                    Err(_) => {
                        self.journal.unreadable.insert(rel);
                    }
                },
            }
        }
        // Expected and not in the working tree at all: HEAD's packs, and the packs the
        // manifest lists. A valid pack that replaces a pack makes it redundant (§4.7).
        let replaced: BTreeSet<PackName> = self
            .journal
            .packs
            .values()
            .filter(|p| p.valid)
            .flat_map(|p| p.replaces.iter().copied())
            .collect();
        let mut head_packs: Option<BTreeMap<String, GitOid>> = None;
        let head_packs_of = |git: &Git| -> Result<BTreeMap<String, GitOid>, StoreError> {
            let Some(head) = git.head()? else {
                return Ok(BTreeMap::new());
            };
            Ok(git
                .subtree_entries(head.tree, PACKS_PREFIX)?
                .into_iter()
                .map(|(path, entry)| (path, entry.oid))
                .collect())
        };
        if at_open {
            let found = head_packs_of(git)?;
            for rel in found.keys() {
                let redundant = PackPath::parse(rel).is_some_and(|p| replaced.contains(&p.name));
                if !redundant
                    && !self.journal.packs.contains_key(rel)
                    && !self.journal.unreadable.contains(rel)
                {
                    candidates.push((rel.clone(), PackLoss::Missing));
                }
            }
            head_packs = Some(found);
        }
        for rel in self.journal.missing.clone() {
            if !candidates.iter().any(|(r, _)| *r == rel) {
                candidates.push((rel, PackLoss::Missing));
            }
        }
        for (rel, was) in candidates {
            // A pack that came back by itself (a cloud client downloaded it).
            if was == PackLoss::Missing
                && let Ok(bytes) = self.vfs.read(&self.folder.path(&rel))
                && read_pack(&rel, &bytes, self.id).is_valid()
            {
                self.fold_pack(&rel, &bytes, None)?;
                self.journal.missing.remove(&rel);
                self.clear_missing_notice(&rel);
                continue;
            }
            let blob = match self.journal.packs.get(&rel).and_then(|p| p.blob) {
                Some(blob) if git.has(blob) => Some(blob),
                _ => {
                    if head_packs.is_none() {
                        head_packs = Some(head_packs_of(git)?);
                    }
                    head_packs.as_ref().and_then(|p| p.get(&rel)).copied()
                }
            };
            let found = match blob {
                Some(blob) => git.read_blob(blob)?,
                None => None,
            };
            let restored = found.clone().filter(|bytes| {
                PackPath::parse(&rel).is_some() && read_pack(&rel, bytes, self.id).is_valid()
            });
            if restored.is_none() {
                self.salvage_all_copies(&rel, found.as_deref())?;
            }
            match restored {
                Some(bytes) => {
                    self.write_back(&rel, &bytes, was)?;
                    self.fold_pack(&rel, &bytes, blob)?;
                    self.journal.missing.remove(&rel);
                    self.clear_missing_notice(&rel);
                    if let Ok(path) = RepoPath::parse(&rel) {
                        self.notice(Notice::PackRestored { path, was }, None);
                    }
                }
                None if was == PackLoss::Missing => {
                    if self.journal.missing.insert(rel.clone())
                        && let Ok(path) = RepoPath::parse(&rel)
                    {
                        self.notice(Notice::PackMissing { path }, None);
                    }
                }
                None => {}
            }
        }
        Ok(())
    }

    /// No valid blob holds `rel` (§4.4 "All copies damaged"): the verified lines of every
    /// copy that no valid pack or pending file holds yet go into a `salvage` pending batch,
    /// which the next seal packs. Salvage at each open alone keeps a line only while the
    /// copy holding it lasts, and a later cut of that copy would take it.
    fn salvage_all_copies(&mut self, rel: &str, blob: Option<&[u8]>) -> Result<(), StoreError> {
        let working = self.vfs.read(&self.folder.path(rel)).ok();
        let valid: BTreeSet<PackId> = self
            .journal
            .packs
            .values()
            .filter(|p| p.valid)
            .map(|p| p.id)
            .collect();
        let mut held: BTreeSet<ChangeId> = self
            .journal
            .pending
            .values()
            .filter(|p| p.status == PendingStatus::Valid)
            .flat_map(|p| p.ids.iter().copied())
            .collect();
        let mut lines: Vec<Verified> = Vec::new();
        for bytes in [working.as_deref(), blob].into_iter().flatten() {
            for line in read_pack(rel, bytes, self.id).entries {
                let id = line.id();
                let packed = self
                    .fold
                    .change(&id)
                    .is_some_and(|view| view.packs.iter().any(|p| valid.contains(p)));
                if !packed && held.insert(id) {
                    lines.push(line);
                }
            }
        }
        if lines.is_empty() {
            return Ok(());
        }
        let header = PendingHeader {
            device: self.device,
            project: self.id,
            purpose: Purpose::Salvage,
            reconciles: None,
        };
        let encoded =
            encode_pending(&header, &lines).map_err(|e| StoreError::Other(e.to_string()))?;
        let dir = self.folder.path(PENDING);
        let name =
            crate::recovery::preserve_bytes(&*self.vfs, &dir, &encoded.name, &encoded.bytes, None)?;
        let new_rel = format!("{PENDING}/{name}");
        let new_abs = self.folder.path(&new_rel);
        let number = self.source.add(&new_abs);
        let loaded = load_pending(&new_rel, &encoded.bytes, self.id, number);
        self.journal.pending.insert(
            new_rel,
            PendingInfo {
                ids: lines.iter().map(Verified::id).collect(),
                reconciles: None,
                number: Some(number),
                status: PendingStatus::Valid,
            },
        );
        let delta = self.fold.add(loaded.inputs);
        let conflicted = crate::project::conflicts_touched(&self.fold, &delta);
        self.dirty.mark(self.kind, &delta, conflicted);
        self.dirty.metadata = true;
        let seq = self.publish();
        self.shared.emit(crate::handle::StoreEvent::Committed {
            seq,
            delta,
            changes: Vec::new(),
        });
        Ok(())
    }

    /// §9.3 T1: the packs a sync received, each written whole beside the project's own and
    /// folded. A pack the project has is skipped (packs are immutable). A path that holds
    /// bytes the project does not know is another program's: a valid pack there is folded as
    /// it is, else it is left untouched and listed (§5.10 M1). A damaged received pack's
    /// readable changes are salvaged into a pending batch; the merge tree keeps its blob
    /// (§9.2 N3). Packs of another project are not taken.
    pub(crate) fn take_in(
        &mut self,
        packs: Vec<crate::handle::ReceivedPack>,
    ) -> Result<crate::handle::IntakeReport, StoreError> {
        let mut report = crate::handle::IntakeReport::default();
        for pack in packs {
            let rel = pack.path;
            if PackPath::parse(&rel).is_none() {
                continue;
            }
            let received = read_pack(&rel, &pack.bytes, self.id);
            if let Some(known) = self.journal.packs.get(&rel) {
                // A damaged copy is repaired from a valid one, whichever side holds it
                // (§9.2 N3); the damaged bytes are kept in the quarantine.
                if !known.valid && received.is_valid() {
                    self.write_back(&rel, &pack.bytes, PackLoss::Damaged)?;
                    self.fold_pack(&rel, &pack.bytes, Some(pack.blob))?;
                    report.packs += 1;
                } else {
                    report.held += 1;
                }
                continue;
            }
            match self.vfs.read(&self.folder.path(&rel)) {
                Ok(there) if there == pack.bytes => {
                    self.fold_pack(&rel, &there, Some(pack.blob))?;
                    report.packs += 1;
                    continue;
                }
                Ok(there) => {
                    if read_pack(&rel, &there, self.id).is_valid() {
                        self.fold_pack(&rel, &there, None)?;
                        report.packs += 1;
                    } else {
                        report.left.push(rel);
                    }
                    continue;
                }
                Err(e) if is_not_found(&e) => {}
                Err(source) => {
                    return Err(StoreError::Io {
                        op: "read a received pack's path",
                        path: self.folder.path(&rel),
                        source,
                    });
                }
            }
            if received.is_valid() {
                self.write_back(&rel, &pack.bytes, PackLoss::Missing)?;
                self.fold_pack(&rel, &pack.bytes, Some(pack.blob))?;
                report.packs += 1;
            } else {
                self.salvage_all_copies(&rel, Some(&pack.bytes))?;
                report.salvaged += 1;
            }
        }
        Ok(report)
    }

    fn mark_seen(&mut self, rel: &str, now: (u64, i64)) {
        if let Some(info) = self.journal.packs.get_mut(rel) {
            info.seen = Some(now);
        }
    }

    fn clear_missing_notice(&self, rel: &str) {
        self.shared.remove_notices_matching(
            |n| matches!(n, Notice::PackMissing { path } if path.as_str() == rel),
        );
    }

    /// Writes a restored pack where the lost one was, durably. A damaged file that was
    /// there is kept in `.tc/quarantine/` first: its lines were salvaged, and a copy costs
    /// nothing.
    fn write_back(&mut self, rel: &str, bytes: &[u8], was: PackLoss) -> Result<(), StoreError> {
        let (dir, name) = self.folder.dir_and_name(rel);
        if was == PackLoss::Damaged
            && let Ok(damaged) = self.vfs.read(&self.folder.path(rel))
        {
            let quarantine = self.folder.path(QUARANTINE);
            atomic_create_dirs(&*self.vfs, &quarantine)?;
            let kept = format!("{}-{}", crate::appdata::utc_stamp(self.clock.now()), name);
            atomic_write(&*self.vfs, &quarantine, &kept, &damaged, SyncDir::Now)
                .map_err(|e| e.into_inner())?;
        }
        atomic_create_dirs(&*self.vfs, &dir)?;
        atomic_write(&*self.vfs, &dir, name, bytes, SyncDir::Now).map_err(|e| e.into_inner())
    }

    /// Folds a pack that is in the working tree now and was not folded as itself: the
    /// changes of a restored pack join the project, and the pack is known from then on.
    fn fold_pack(
        &mut self,
        rel: &str,
        bytes: &[u8],
        blob: Option<GitOid>,
    ) -> Result<(), StoreError> {
        let abs = self.folder.path(rel);
        let number = self.source.add(&abs);
        let loaded = load_pack(rel, bytes, self.id, number);
        if let Some(section) = loaded.section {
            self.index.sections.insert(rel.to_owned(), section);
            self.index.changed = true;
        }
        let Facts::Pack(pack) = loaded.facts else {
            return Err(StoreError::Other(format!("{rel} is not a pack")));
        };
        let seen = self.vfs.metadata(&abs).ok().map(|m| (m.len, m.mtime_ns));
        self.journal.packs.insert(
            rel.to_owned(),
            PackInfo {
                id: pack.id,
                size: loaded.key.size,
                fingerprint: loaded.key.fingerprint,
                number: Some(number),
                valid: pack.status == PackStatus::Valid,
                sealer: pack.path.map(|p| p.sealer),
                replaces: pack.header.map(|h| h.replaces).unwrap_or_default(),
                ids: Vec::new(),
                blob,
                seen,
                damaged: matches!(pack.status, PackStatus::Damaged(_)),
            },
        );
        let delta = self.fold.add(loaded.inputs);
        let conflicted = crate::project::conflicts_touched(&self.fold, &delta);
        self.dirty.mark(self.kind, &delta, conflicted);
        self.dirty.metadata = true;
        let seq = self.publish();
        self.shared.emit(crate::handle::StoreEvent::Committed {
            seq,
            delta,
            changes: Vec::new(),
        });
        Ok(())
    }
}
