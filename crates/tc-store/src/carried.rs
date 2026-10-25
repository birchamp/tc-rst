//! Carried files (`docs/STORAGE.md` §11.4, §5.16): files the app keeps verbatim, whose bytes
//! are blobs in the project's object database and whose places the fold lists (§7.7).
//!
//! Nothing here holds a file in memory: a carried file may be an hour of audio. A file is read
//! in chunks to take its md5 and size, stored with the same chunks when it is new to the
//! project, and written from its blob in chunks when the project says it belongs at a place.
//! The blob is the copy of the person's bytes (X1 of §11.1 is skipped), so an edited carried
//! file is never copied twice.
//!
//! The rules of §6.2 hold: a path whose bytes are none the app wrote is the person's (an
//! outside edit, recorded as a new version of the file whose parent is the version the app
//! placed there, which its manifest entry names by blob and which the newest version need not
//! be), a missing file is written again and never read as a removal (I11, L43), and no file is
//! written over bytes the app did not place (I8).

use crate::atomic::{HeldRetry, SyncDir, Written, atomic_write_streamed};
use crate::error::StoreError;
use crate::git::Git;
use crate::handle::Msg;
use crate::manifest::Entry;
use crate::materialize::MaterializeReport;
use crate::reply::FocusReport;
use crate::vfs::{EntryKind, is_not_found};
use crate::writer::Writer;
use std::cell::Cell;
use std::io;
use tc_core::Md5Stream;
use tc_journal::change::ExternalSource;
use tc_journal::command::PlannedChange;
use tc_journal::fold::{CarriedPlacement, Notice};
use tc_journal::outside::OutsidePlan;
use tc_journal::record::CarriedValue;
use tc_journal::{Cause, ChangeId, GitOid, Md5, Record, RecordKey, RepoPath};

const CHUNK: usize = 64 * 1024;

/// Files over this size are listed as they are and never recorded or committed (§11.4).
pub(crate) const TOO_LARGE: u64 = 256 * 1024 * 1024;

/// A carried replacement whose old bytes and available blob have been checked. No
/// ingredient changes until the group's metadata can be safely replaced.
pub(crate) struct CarriedWrite {
    pub placement: CarriedPlacement,
    seen: Option<Md5>,
}

/// Whether the previous carried version has bytes independent of its ingredient.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PriorBytes {
    AlreadyKept,
    Recovered,
    /// Kept by an earlier try of the same replacement or removal, and still there.
    StillKept,
    Changed,
    Unavailable,
}

impl PriorBytes {
    fn kept(self) -> bool {
        matches!(self, Self::AlreadyKept | Self::Recovered | Self::StillKept)
    }
}

/// What the placed version's blob gave the file being written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Streamed {
    Nothing,
    Bytes(Md5),
    /// The object database no longer has the blob.
    Missing,
    /// The blob is there and its bytes cannot be read (a damaged object, a placeholder).
    Unreadable,
}

impl Writer {
    /// The md5 and size of the file at `rel`, read in chunks.
    pub(crate) fn hash_file(&self, rel: &str) -> io::Result<(Md5, u64)> {
        let file = self.vfs.open(&self.folder.path(rel))?;
        let len = file.len();
        let mut stream = Md5Stream::new();
        let mut buffer = vec![0u8; CHUNK];
        let mut at = 0u64;
        while at < len {
            let n = usize::try_from((len - at).min(CHUNK as u64)).unwrap_or(CHUNK);
            file.read_at(&mut buffer[..n], at)?;
            stream.update(&buffer[..n]);
            at += n as u64;
        }
        self.counters.add(&self.counters.bytes_hashed, len);
        Ok((stream.finish(), len))
    }

    /// The placement of the fold whose bytes go at `rel`.
    fn placement_at(&self, rel: &str) -> Option<CarriedPlacement> {
        self.fold
            .carried_files()
            .into_iter()
            .find(|p| p.path == rel)
    }

    /// A carried file found by a look (§11.1): a placed file whose bytes are none the app
    /// placed is an edit, a file the project has no place for is a new one, and a placed file
    /// that is gone is placed again.
    pub(crate) fn look_at_carried_file(
        &mut self,
        rel: &str,
        report: &mut FocusReport,
    ) -> Result<(), StoreError> {
        if self.pending_record.contains_key(rel) || self.busy.contains_key(rel) {
            return Ok(());
        }
        let entry = self.manifest.paths.get(rel).cloned();
        let placement = self.placement_at(rel);
        let path = self.folder.path(rel);
        let meta = match self.vfs.metadata(&path) {
            Ok(meta) if meta.kind == EntryKind::File => meta,
            Ok(_) => return Ok(()),
            Err(e) if is_not_found(&e) => {
                // A deletion is no removal (I11): the file is placed again; one the project
                // no longer places is forgotten.
                if placement.is_some() {
                    self.end_hold(rel);
                    self.dirty.carried = true;
                    self.notice_missing(rel, entry.is_some());
                } else if entry.is_some() {
                    self.end_hold(rel);
                    self.manifest.paths.remove(rel);
                    self.manifest_changed = true;
                }
                return Ok(());
            }
            Err(_) => return Ok(()),
        };
        let held = entry.as_ref().and_then(|e| e.held().cloned());
        if held.is_none() && self.clean_by_stat(rel) {
            return Ok(());
        }
        if meta.len > TOO_LARGE {
            if let Ok(path) = RepoPath::parse(rel) {
                self.notice(
                    Notice::CarriedTooLarge {
                        path,
                        size: meta.len,
                    },
                    None,
                );
            }
            return Ok(());
        }
        let Ok((md5, size)) = self.hash_file(rel) else {
            return Ok(());
        };
        report.read += 1;
        if held.as_ref().is_some_and(|h| h.md5 == md5.to_hex()) {
            return Ok(());
        }
        if held.is_none()
            && entry
                .as_ref()
                .is_some_and(|e| e.md5() == Some(md5.to_hex().as_str()))
        {
            self.take_stat_of(rel, meta.len, meta.mtime_ns);
            return Ok(());
        }
        if let Some(p) = &placement
            && p.md5 == md5
        {
            // The bytes the project places, and the entry has not caught up with them.
            self.set_carried_entry(rel, md5, size, p.oid, self.seq);
            return Ok(());
        }
        // Bytes the app did not place: an edit of a placed file, or a new one.
        let (record, parents, unknown) = match &placement {
            Some(p) => {
                // The edit starts from the version the app placed here, which the fold's
                // newest head need not be: a teammate's version the folder never showed is
                // a sibling of the edit, not something it replaces.
                let (parents, unknown) = self.parents_of_edit(&p.record, entry.as_ref());
                (p.record.clone(), parents, unknown)
            }
            None => {
                let Ok(record) = RepoPath::parse(rel) else {
                    return Ok(());
                };
                let (parents, unknown) = self.parents_for_new_version(&record);
                (record, parents, unknown)
            }
        };
        if self.record_carried(rel, record, parents, unknown, md5, size)? {
            report.found.push(rel.to_owned());
        }
        Ok(())
    }

    /// The parents of an edit of the file the app placed at a path whose record is `record`
    /// (§11.4): the head of the version the entry names, by its blob; when the project holds
    /// no such version, the rules for a version the project did not place.
    fn parents_of_edit(&self, record: &RepoPath, entry: Option<&Entry>) -> (Vec<ChangeId>, bool) {
        match self.placed_version(record, entry) {
            Some(parents) => (parents, false),
            None => self.parents_for_new_version(record),
        }
    }

    /// The change(s) of `record` whose bytes the entry says the app placed at the path: the
    /// heads among those with that blob (the token rule of §2.7: equal heads are one version),
    /// else the newest of them, which a later version replaced.
    fn placed_version(&self, record: &RepoPath, entry: Option<&Entry>) -> Option<Vec<ChangeId>> {
        let oid = GitOid::parse(entry?.oid.as_deref()?)?;
        let key = RecordKey::CarriedFile(record.clone());
        let same: Vec<ChangeId> = self
            .fold
            .history(&key)
            .into_iter()
            .filter(|id| {
                matches!(
                    self.fold.body(id).map(|b| b.record),
                    Some(Record::CarriedFile {
                        value: CarriedValue::Present { oid: held, .. },
                        ..
                    }) if held == oid
                )
            })
            .collect();
        let heads = self.fold.heads(&key);
        let current: Vec<ChangeId> = same
            .iter()
            .copied()
            .filter(|id| heads.contains(id))
            .collect();
        if current.is_empty() {
            same.last().map(|newest| vec![*newest])
        } else {
            Some(current)
        }
    }

    /// The parents of a version of the file at `record` that the project did not place: the
    /// removal it replaces, when the file was removed (a file put where a removed one was);
    /// none when the record has another version, which makes the new one another version
    /// beside it, and none when the project has never heard of it.
    fn parents_for_new_version(&self, record: &RepoPath) -> (Vec<ChangeId>, bool) {
        let heads = self.fold.heads(&RecordKey::CarriedFile(record.clone()));
        let removed = !heads.is_empty()
            && heads.iter().all(|h| {
                matches!(
                    self.fold.body(h).map(|b| b.record),
                    Some(Record::CarriedFile {
                        value: CarriedValue::Removed,
                        ..
                    })
                )
            });
        if removed {
            (heads, false)
        } else {
            let unknown = !heads.is_empty();
            (Vec::new(), unknown)
        }
    }

    /// The notice of a placed file that was gone and is written again.
    fn notice_missing(&self, rel: &str, known: bool) {
        if known && let Ok(path) = RepoPath::parse(rel) {
            self.notice(Notice::MissingFileRebuilt { path }, None);
        }
    }

    /// The size and time of bytes the read found to be the ones the ledger placed.
    fn take_stat_of(&mut self, rel: &str, len: u64, mtime: i64) {
        let settled = self.settles(mtime);
        self.note(
            rel,
            crate::ledger::Event::Stat {
                stat: crate::ledger::Stat {
                    size: len,
                    mtime_ns: mtime,
                },
                settled,
            },
        );
    }

    /// A carried path's entry: the bytes the app holds there, with their blob, placed or found
    /// equal from the fold of sequence number `seq` (a checkpoint's job on an older fold
    /// leaves the path alone, [`Writer::written_from_newer_fold`]). The carried files are
    /// listed again only when what `metadata.json` says of the path changed.
    pub(crate) fn set_carried_entry(
        &mut self,
        rel: &str,
        md5: Md5,
        size: u64,
        oid: GitOid,
        seq: u64,
    ) {
        let meta = self.vfs.metadata(&self.folder.path(rel)).ok();
        let mtime = meta.map_or(0, |m| m.mtime_ns);
        let settled = meta.is_some() && self.settles(mtime);
        let mut entry = Entry::placed(md5.to_hex(), size, meta.map(|m| m.mtime_ns), settled);
        entry.oid = Some(oid.to_hex());
        let listed_differently = self
            .manifest
            .paths
            .get(rel)
            .is_none_or(|e| e.listed() != entry.listed() || e.oid != entry.oid);
        self.manifest.paths.insert(rel.to_owned(), entry);
        self.manifest_changed = true;
        self.shared.remove_notices_of(rel);
        self.busy.remove(rel);
        if let Ok(path) = RepoPath::parse(rel) {
            self.shared.remove_blocked_notice(&path);
        }
        let seen = self.written_seq.entry(rel.to_owned()).or_insert(seq);
        *seen = (*seen).max(seq);
        if listed_differently {
            self.dirty.carried = true;
        }
    }

    /// Stores the bytes of `rel` as a blob (the same pass takes their md5 again) and hands a
    /// new version of the file to the actor to record (§11.4). `parents`: the version the
    /// bytes started from. `false` when the file changed while it was read, or this session
    /// has no repository to keep the bytes in.
    fn record_carried(
        &mut self,
        rel: &str,
        record: RepoPath,
        parents: Vec<ChangeId>,
        unknown: bool,
        md5: Md5,
        size: u64,
    ) -> Result<bool, StoreError> {
        let Some(git) = &self.git else {
            return Ok(false);
        };
        // An unreachable loose object with these bytes may be in a repack's list to prune:
        // the batch being removed ends first, and the run removes no more (§9.8). The next
        // repack finds the version in `refs/tc/carried`.
        git.commit_begin();
        let stored = self.store_file(git, rel);
        git.commit_end();
        let Some((oid, stored_md5, len)) = stored? else {
            return Ok(false);
        };
        if stored_md5 != md5 || len != size {
            // The file changed while it was read: the next look finds it again.
            return Ok(false);
        }
        let Ok(source) = RepoPath::parse(rel) else {
            return Ok(false);
        };
        let plan = OutsidePlan {
            changes: vec![PlannedChange {
                record: Record::CarriedFile {
                    path: record,
                    value: CarriedValue::Present { oid, size, md5 },
                },
                parents,
                cause: Cause::External {
                    source: ExternalSource::LocalFile {
                        path: source,
                        seen_md5: md5,
                    },
                    unknown_base: unknown,
                },
            }],
            unknown_base: unknown,
            keys_examined: 1,
            ..OutsidePlan::default()
        };
        self.pending_record
            .insert(rel.to_owned(), (md5, Vec::new()));
        self.pending_carried.insert(rel.to_owned(), (oid, size));
        let sent = self.actor.send(Msg::RecordOutside {
            rel: rel.to_owned(),
            seen: md5,
            plan: Box::new(plan),
            derived: 0,
            copy: String::new(),
        });
        if sent.is_err() {
            self.pending_record.remove(rel);
            self.pending_carried.remove(rel);
            return Ok(false);
        }
        Ok(true)
    }

    /// Stores the file at `rel` as a blob, read in chunks, taking its md5 in the same pass:
    /// the blob, the md5 and the size of the bytes that were stored. `None` when the file
    /// cannot be opened.
    fn store_file(&self, git: &Git, rel: &str) -> Result<Option<(GitOid, Md5, u64)>, StoreError> {
        let Ok(file) = self.vfs.open(&self.folder.path(rel)) else {
            return Ok(None);
        };
        let len = file.len();
        let mut stream = Md5Stream::new();
        let mut at = 0u64;
        let oid = git.write_blob_from(&mut |buffer: &mut [u8]| {
            if at >= len {
                return Ok(0);
            }
            let n = usize::try_from((len - at).min(buffer.len() as u64)).unwrap_or(buffer.len());
            file.read_at(&mut buffer[..n], at)?;
            stream.update(&buffer[..n]);
            at += n as u64;
            Ok(n)
        })?;
        Ok(Some((oid, stream.finish(), len)))
    }

    /// Stores the file at `rel` as the blob `expected`, when its bytes are that version's
    /// (§6.4 G4). `false` when they are not.
    pub(crate) fn store_file_as_blob(
        &self,
        git: &Git,
        rel: &str,
        expected: GitOid,
    ) -> Result<bool, StoreError> {
        Ok(self
            .store_file(git, rel)?
            .is_some_and(|(oid, _, _)| oid == expected))
    }

    /// Before destroying a placed carried file, recover its recorded blob if the
    /// repository lost it. The caller repeats its final comparison after a recovery read.
    /// `earlier`: what the previous try of the same replacement or removal found.
    pub(crate) fn keep_prior_carried(
        &self,
        rel: &str,
        expected: Md5,
        earlier: Option<PriorBytes>,
    ) -> Result<PriorBytes, StoreError> {
        let Some(entry) = self.manifest.paths.get(rel) else {
            return Ok(PriorBytes::Unavailable);
        };
        let Some(placement) = entry.placement() else {
            return Ok(PriorBytes::Unavailable);
        };
        if placement.md5 != expected.to_hex() {
            return Ok(PriorBytes::Changed);
        }
        let expected_size = placement.size;
        let Some(oid) = entry.oid.as_deref().and_then(GitOid::parse) else {
            // A malformed/missing carried identity cannot authorize destroying its bytes.
            return Ok(PriorBytes::Unavailable);
        };
        let Some(git) = &self.git else {
            return Ok(PriorBytes::Unavailable);
        };
        // A prune that collected before the recovery must not remove its fresh blob.
        git.commit_begin();
        // An ODB cache can keep an object after its file was lost. A new repository
        // has an independent cache and checks for the physical backup instead.
        let kept = (|| -> Result<PriorBytes, StoreError> {
            let probe = git.another()?;
            let present = probe.has(oid);
            // A try after a sharing wait reads and flushes no backup again (PERFORMANCE.md
            // rule 1); one lost during the wait is kept again.
            if present && earlier.is_some_and(PriorBytes::kept) {
                return Ok(PriorBytes::StillKept);
            }
            let recovered = !present;
            if recovered && !self.store_file_as_blob(&probe, rel, oid)? {
                return Ok(PriorBytes::Changed);
            }
            let mut digest = Md5Stream::new();
            let mut size = 0u64;
            let copied = probe.copy_blob_to_durable(oid, &mut |chunk| {
                let amount = u64::try_from(chunk.len()).map_err(io::Error::other)?;
                size = size.checked_add(amount).ok_or_else(|| {
                    io::Error::other("the stored carried file exceeds its supported size")
                })?;
                if size > expected_size {
                    return Err(io::Error::other(
                        "the stored carried file has the wrong size",
                    ));
                }
                self.counters.add(&self.counters.bytes_hashed, amount);
                digest.update(chunk);
                Ok(())
            });
            // Readability alone does not authorize destroying the working copy: the
            // exact old blob and its containing names must have completed their flushes.
            Ok(
                if matches!(copied, Ok(true))
                    && size == expected_size
                    && digest.finish() == expected
                {
                    if recovered {
                        PriorBytes::Recovered
                    } else {
                        PriorBytes::AlreadyKept
                    }
                } else {
                    PriorBytes::Unavailable
                },
            )
        })();
        git.commit_end();
        kept
    }

    /// *Take in the file as an edit* for a held carried file (§6.2): its bytes become a new
    /// version of the file, recorded with no base, and the hold ends when the batch is durable.
    pub(crate) fn take_in_held_carried(
        &mut self,
        rel: &str,
        reply: crate::handle::Resolver<Result<crate::reply::TakenIn, StoreError>>,
    ) {
        let record = self
            .placement_at(rel)
            .map(|p| p.record)
            .or_else(|| RepoPath::parse(rel).ok());
        let hashed = self.hash_file(rel);
        let (Some(record), Ok((md5, size))) = (record, hashed) else {
            reply.resolve(Err(StoreError::Other(format!(
                "{rel} cannot be read now; try again"
            ))));
            return;
        };
        let (parents, unknown) = self.parents_for_new_version(&record);
        match self.record_carried(rel, record, parents, unknown, md5, size) {
            Ok(true) => {
                self.waiting.insert(rel.to_owned(), reply);
            }
            Ok(false) => reply.resolve(Ok(crate::reply::TakenIn::default())),
            Err(e) => reply.resolve(Err(e)),
        }
    }

    /// Prepares the carried files the project holds (§11.4): each placement whose entry is not
    /// its blob, clean by stat, is looked at and, when the file is gone or holds bytes the app
    /// placed before, planned for replacement. Bytes the app did not place are the person's
    /// and are recorded, not overwritten. Preparation never replaces an ingredient.
    pub(crate) fn prepare_carried(
        &mut self,
        report: &mut MaterializeReport,
    ) -> Result<Vec<CarriedWrite>, StoreError> {
        let mut writes = Vec::new();
        for placement in self.fold.carried_files() {
            if !placement.path.starts_with("ingredients/")
                || self.pending_record.contains_key(&placement.path)
                // A checkpoint's job on an older fold never puts an older version back over
                // one a newer fold placed or recorded (§5.13 C2).
                || self.written_from_newer_fold(&placement.path)
            {
                continue;
            }
            let entry = self.manifest.paths.get(&placement.path).cloned();
            let current = entry.as_ref().is_some_and(|e| {
                e.oid.as_deref() == Some(placement.oid.to_hex().as_str()) && e.held().is_none()
            });
            if current && self.clean_by_stat(&placement.path) {
                continue;
            }
            if let Some(write) = self.prepare_one(placement, entry, report)? {
                writes.push(write);
            }
        }
        Ok(writes)
    }

    fn prepare_one(
        &mut self,
        p: CarriedPlacement,
        entry: Option<Entry>,
        report: &mut MaterializeReport,
    ) -> Result<Option<CarriedWrite>, StoreError> {
        let path = self.folder.path(&p.path);
        let seen = match self.vfs.metadata(&path) {
            Ok(meta) if meta.kind == EntryKind::File => {
                let (md5, size) = match self.hash_file(&p.path) {
                    Ok(read) => read,
                    Err(_) => {
                        self.note(
                            &p.path,
                            crate::ledger::Event::Read(crate::ledger::Disk::Unreadable),
                        );
                        self.reasons
                            .insert(p.path.clone(), tc_journal::fold::Blocked::FileUnreadable);
                        report.blocked.push(p.path.clone());
                        return Ok(None);
                    }
                };
                if md5 == p.md5 {
                    self.set_carried_entry(&p.path, md5, size, p.oid, self.seq);
                    return Ok(None);
                }
                if entry.as_ref().is_some_and(|e| e.held().is_some()) {
                    return Ok(None);
                }
                if !entry
                    .as_ref()
                    .is_some_and(|e| e.md5() == Some(md5.to_hex().as_str()))
                {
                    // Bytes the app did not place: the person's (I8).
                    let mut found = FocusReport::default();
                    self.look_at_carried_file(&p.path, &mut found)?;
                    return Ok(None);
                }
                Some(md5)
            }
            Ok(_) => return Ok(None),
            Err(e) if is_not_found(&e) => {
                self.notice_missing(&p.path, entry.is_some());
                None
            }
            Err(_) => {
                self.note(
                    &p.path,
                    crate::ledger::Event::Read(crate::ledger::Disk::Unreadable),
                );
                self.reasons
                    .insert(p.path.clone(), tc_journal::fold::Blocked::FileUnreadable);
                report.blocked.push(p.path.clone());
                return Ok(None);
            }
        };
        // The bytes come from the blob; a blob this computer lacks leaves the file as it is.
        let Some(git) = &self.git else {
            self.notice_bytes_missing(&p);
            return Ok(None);
        };
        if !git.has(p.oid) {
            self.notice_bytes_missing(&p);
            return Ok(None);
        }
        Ok(Some(CarriedWrite { placement: p, seen }))
    }

    /// Executes only after the planned metadata passed validation and preservation. A
    /// refused replacement keeps its old entry, so the final listing describes that file.
    pub(crate) fn execute_carried(
        &mut self,
        writes: Vec<CarriedWrite>,
        report: &mut MaterializeReport,
    ) -> Result<(), StoreError> {
        for write in writes {
            self.execute_carried_one(write, report)?;
        }
        Ok(())
    }

    fn execute_carried_one(
        &mut self,
        write: CarriedWrite,
        report: &mut MaterializeReport,
    ) -> Result<(), StoreError> {
        let CarriedWrite { placement: p, seen } = write;
        let Some(git) = &self.git else {
            self.notice_bytes_missing(&p);
            return Ok(());
        };
        let (dir, name) = self.folder.dir_and_name(&p.path);
        crate::atomic::atomic_create_dirs(&*self.vfs, &dir)?;
        let streamed: Cell<Streamed> = Cell::new(Streamed::Nothing);
        let prior: Cell<Option<PriorBytes>> = Cell::new(None);
        let wrote = atomic_write_streamed(
            &*self.vfs,
            &dir,
            name,
            &mut |out| {
                let mut stream = Md5Stream::new();
                let mut sink_failed = false;
                let copied = git.copy_blob_to(p.oid, &mut |chunk| {
                    stream.update(chunk);
                    out(chunk).inspect_err(|_| sink_failed = true)
                });
                streamed.set(match &copied {
                    Ok(true) => Streamed::Bytes(stream.finish()),
                    Ok(false) => Streamed::Missing,
                    Err(_) if sink_failed => Streamed::Nothing,
                    Err(_) => Streamed::Unreadable,
                });
                if !copied.map_err(StoreError::from)? {
                    return Err(StoreError::Other(format!("{} has no bytes here", p.path)));
                }
                Ok(())
            },
            SyncDir::Now,
            &HeldRetry::BACKGROUND,
            // The last look (I8): the bytes written are the blob's, and the file is as it
            // was when it was read (or still gone).
            &mut || {
                if streamed.get() != Streamed::Bytes(p.md5) {
                    return Ok(false);
                }
                Ok(match self.hash_file(&p.path) {
                    Ok((md5, _)) if md5 == p.md5 => true,
                    Ok((md5, _)) if seen == Some(md5) => {
                        let kept = self.keep_prior_carried(&p.path, md5, prior.get())?;
                        prior.set(Some(kept));
                        match kept {
                            // The file was read just now, and an earlier try kept its bytes.
                            PriorBytes::StillKept => true,
                            PriorBytes::AlreadyKept | PriorBytes::Recovered => {
                                // Proving the backup streamed its bytes; a later outside
                                // save must still pass the comparison before rename.
                                match self.hash_file(&p.path) {
                                    Ok((now, _)) => now == md5 || now == p.md5,
                                    Err(e) if is_not_found(&e) => true,
                                    Err(_) => false,
                                }
                            }
                            PriorBytes::Changed | PriorBytes::Unavailable => false,
                        }
                    }
                    Ok(_) => false,
                    Err(e) if is_not_found(&e) => true,
                    Err(_) => false,
                })
            },
        );
        match wrote {
            Ok(Written::Renamed) => {
                self.counters.add(&self.counters.derived_written, 1);
                self.set_carried_entry(&p.path, p.md5, p.size, p.oid, self.seq);
                report.written.push(p.path.clone());
            }
            Ok(Written::Refused) => {
                if prior.get() == Some(PriorBytes::Unavailable) {
                    self.reasons
                        .insert(p.path.clone(), tc_journal::fold::Blocked::FileUnreadable);
                    report.blocked.push(p.path.clone());
                }
                // The final look disproved the stat fast path. A same-stat outside save
                // must be listed from its actual bytes before this group writes metadata.
                if let crate::ledger::State::Placed { placed } = self.state_of(&p.path)
                    && let Some(stat) = placed.stat()
                {
                    self.note(
                        &p.path,
                        crate::ledger::Event::Stat {
                            stat,
                            settled: false,
                        },
                    );
                }
                self.dirty.carried = true;
            }
            Err(crate::atomic::WriteError::NotWritten(e))
                if e.io_error().is_some_and(|io| self.vfs.is_held(io)) =>
            {
                self.reasons
                    .insert(p.path.clone(), tc_journal::fold::Blocked::FileBusy);
                report.blocked.push(p.path.clone());
            }
            // R19 for the version's bytes: the file stays as it is and is tried again, and
            // the rest of the project is written (§11.4).
            Err(crate::atomic::WriteError::NotWritten(_))
                if streamed.get() == Streamed::Unreadable =>
            {
                self.reasons
                    .insert(p.path.clone(), tc_journal::fold::Blocked::FileUnreadable);
                report.blocked.push(p.path.clone());
            }
            Err(crate::atomic::WriteError::NotWritten(_))
                if streamed.get() == Streamed::Missing =>
            {
                self.notice_bytes_missing(&p);
            }
            Err(e) => return Err(e.into_inner()),
        }
        Ok(())
    }

    fn notice_bytes_missing(&self, p: &CarriedPlacement) {
        if let Ok(path) = RepoPath::parse(&p.path) {
            self.notice(Notice::CarriedBytesMissing { path, oid: p.oid }, None);
        }
    }

    /// The writer's acceptance of a carried version the actor recorded (D7): the entry names
    /// the blob and the bytes of the file, which the newest snapshot holds.
    pub(crate) fn accept_carried(&mut self, rel: &str, md5: Md5, size: u64, oid: GitOid) {
        let seq = self.newest_seq();
        self.set_carried_entry(rel, md5, size, oid, seq);
    }
}
