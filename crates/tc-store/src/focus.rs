//! Looking at the folder after something may have changed it (`docs/STORAGE.md` §11.1): the
//! window got focus, the watcher told some paths, or the person chose to take in a held file.
//!
//! None of these projects a file. A path whose size and time are those of its settled entry
//! is the app's own and costs a stat; any other is read, and its md5 says whether it is the
//! app's (the entry's, so only its stat is taken again) or an outside edit, which is examined
//! and recorded by [`Writer::found_outside`] like any other.

use crate::error::StoreError;
use crate::layout;
use crate::materialize::{Disk, FIXED};
use crate::reply::{FocusReport, TakenIn};
use crate::writer::{Recorded, Writer};
use tc_journal::Md5;
use tc_journal::fold::{DerivedFile, Notice};

/// Where `rel` stands: a derived file of the project, `metadata.json`, a fixed file, or
/// something else (a carried file, a book file the project does not show).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Derived,
    Metadata,
    Fixed,
    Other,
}

fn kind_of(rel: &str) -> Kind {
    if rel == layout::METADATA {
        Kind::Metadata
    } else if FIXED.contains(&rel) {
        Kind::Fixed
    } else if DerivedFile::from_path(rel).is_some() {
        Kind::Derived
    } else {
        Kind::Other
    }
}

impl Writer {
    /// The window got focus (§11.1): every path of the manifest is stat'ed, those not clean
    /// by stat are read, and the folders are listed for files that are new.
    pub(crate) fn focus(&mut self) -> Result<FocusReport, StoreError> {
        if let Some(reason) = self.read_only() {
            return Err(StoreError::Other(format!(
                "the project is read-only: {reason:?}"
            )));
        }
        self.begin_newest();
        let mut report = FocusReport::default();
        let paths: Vec<String> = self.manifest.paths.keys().cloned().collect();
        for rel in paths {
            self.look_at(&rel, &mut report)?;
        }
        self.look_for_new_files(None, &mut report)?;
        self.finish_looking()?;
        Ok(report)
    }

    /// The watcher told these paths (§11.1). A path the app wrote itself is skipped by its
    /// entry.
    pub(crate) fn paths_changed(&mut self, rels: Vec<String>) {
        if self.read_only().is_some() {
            return;
        }
        self.begin_newest();
        let mut report = FocusReport::default();
        let mut new: Vec<String> = Vec::new();
        for rel in rels {
            if self.manifest.paths.contains_key(&rel) || kind_of(&rel) != Kind::Other {
                if let Err(e) = self.look_at(&rel, &mut report) {
                    self.job_failed(&e);
                }
            } else {
                new.push(rel);
            }
        }
        if !new.is_empty()
            && let Err(e) = self.look_for_new_files(Some(new), &mut report)
        {
            self.job_failed(&e);
        }
        if let Err(e) = self.finish_looking() {
            self.job_failed(&e);
        }
    }

    /// The newest snapshot is the fold the examination compares with.
    fn begin_newest(&mut self) {
        let snapshot = self.shared.snapshot();
        let packs = self.packs.clone();
        let complete = self.complete;
        self.begin(&snapshot, packs, complete);
    }

    /// What follows a look: the manifest, and whatever the look marked to be written again.
    pub(crate) fn finish_looking(&mut self) -> Result<(), StoreError> {
        if self.manifest_changed {
            self.write_manifest()?;
        }
        if !self.dirty.is_empty() {
            self.materialize_now()?;
        }
        Ok(())
    }

    /// O9's listing of the folders (§6.1): what is new is collected beside the paths the
    /// classification found changed, and recorded with them before the open ends.
    pub(crate) fn look_for_new_files_at_open(&mut self) -> Result<(), StoreError> {
        let mut report = FocusReport::default();
        // A carried file the manifest has an entry for is stat'ed; one that is not clean by
        // stat is read once the actor runs (§11.4).
        let carried: Vec<String> = self
            .manifest
            .paths
            .iter()
            .filter(|(_, e)| e.oid.is_some())
            .map(|(p, _)| p.clone())
            .collect();
        // A placement the manifest does not know yet (a version a teammate recorded) is
        // written by the first job.
        for placement in self.fold.carried_files() {
            let current = self.manifest.paths.get(&placement.path).is_some_and(|e| {
                e.oid.as_deref() == Some(placement.oid.to_hex().as_str()) && e.held.is_none()
            });
            if !current {
                self.dirty.carried = true;
            }
        }
        for rel in carried {
            let missing = self
                .vfs
                .metadata(&self.folder.path(&rel))
                .is_err_and(|e| crate::vfs::is_not_found(&e));
            if missing || !self.clean_by_stat(&rel) {
                self.deferred_paths.push(rel);
            }
        }
        self.look_for_new_files(None, &mut report)
    }

    /// One path of the manifest, or a derived, fixed or `metadata.json` path.
    fn look_at(&mut self, rel: &str, report: &mut FocusReport) -> Result<(), StoreError> {
        if self.pending_record.contains_key(rel) || self.busy.contains_key(rel) {
            return Ok(());
        }
        let kind = kind_of(rel);
        let entry = self.manifest.paths.get(rel).cloned();
        if kind == Kind::Other {
            // A carried file (§11.4), or a file the project knows nothing about.
            return self.look_at_carried_file(rel, report);
        }
        let path = self.folder.path(rel);
        let meta = match self.vfs.metadata(&path) {
            Ok(meta) => meta,
            Err(e) if crate::vfs::is_not_found(&e) => {
                // A deletion is no edit (I11): the file is written again, and a hold ends.
                if entry.is_some() {
                    self.end_hold(rel);
                    if let Some(file) = DerivedFile::from_path(rel) {
                        if self.fold.derived_files().contains(&file) {
                            if let Ok(path) = tc_journal::RepoPath::parse(rel) {
                                self.notice(Notice::MissingFileRebuilt { path }, None);
                            }
                            self.dirty.add_file(file);
                        } else {
                            self.manifest.paths.remove(rel);
                            self.manifest_changed = true;
                        }
                    } else if kind == Kind::Metadata {
                        self.dirty.metadata = true;
                    }
                }
                return Ok(());
            }
            Err(_) => return Ok(()),
        };
        if meta.kind != crate::vfs::EntryKind::File {
            return Ok(());
        }
        let held = entry.as_ref().and_then(|e| e.held.clone());
        if held.is_none() && self.clean_by_stat(rel) {
            return Ok(());
        }
        let disk = match self.read_disk(rel) {
            Disk::Bytes(bytes) => bytes,
            Disk::Missing | Disk::Unreadable => return Ok(()),
        };
        report.read += 1;
        let md5 = Md5::of(&disk);
        if let Some(held) = &held {
            if held.md5 == md5.to_hex() {
                return Ok(());
            }
            // Saved again while held: the app's version is gone, so no base (§6.2).
            self.found_outside(rel, &disk, true)?;
            report.found.push(rel.to_owned());
            return Ok(());
        }
        if entry.as_ref().is_some_and(|e| e.md5 == md5.to_hex()) {
            self.take_stat(rel, &entry, meta.len, meta.mtime_ns);
            return Ok(());
        }
        match kind {
            Kind::Metadata => {
                if self.keep_edited_metadata(&disk)? {
                    report.found.push(rel.to_owned());
                }
            }
            Kind::Fixed => {
                self.restore_fixed(rel, &disk)?;
                report.found.push(rel.to_owned());
            }
            Kind::Derived => {
                // A file that equals what the project would write is no edit.
                self.found_outside(rel, &disk, false)?;
                report.found.push(rel.to_owned());
            }
            Kind::Other => {}
        }
        Ok(())
    }

    /// The size and time of an entry whose bytes the read found to be the entry's.
    fn take_stat(
        &mut self,
        rel: &str,
        entry: &Option<crate::manifest::Entry>,
        len: u64,
        mtime: i64,
    ) {
        let settles = self.settles(mtime);
        if let Some(e) = self.manifest.paths.get_mut(rel)
            && (e.size != len || e.mtime_ns != mtime || e.settled != settles)
        {
            e.size = len;
            e.mtime_ns = mtime;
            e.settled = settles;
            self.manifest_changed = true;
        }
        let _ = entry;
    }

    /// Files of the folder the manifest does not know: the ones that cannot be ingredients
    /// are moved out, the book files of books the project does not show are examined as
    /// edits, and the others are carried files (§11.4). `only`: these paths, not the folders'
    /// listing.
    fn look_for_new_files(
        &mut self,
        only: Option<Vec<String>>,
        report: &mut FocusReport,
    ) -> Result<(), StoreError> {
        let scanned = match only {
            Some(rels) => self.consider_new(rels)?,
            None => self.scan_folder()?,
        };
        self.tell_moved(&scanned.moved);
        report
            .moved
            .extend(scanned.moved.iter().map(|m| m.to.clone()));
        for rel in scanned.new {
            self.take_in_new(&rel, report)?;
        }
        Ok(())
    }

    /// A file the manifest does not know: a book file examined as an edit, anything else a
    /// carried file.
    pub(crate) fn take_in_new(
        &mut self,
        rel: &str,
        report: &mut FocusReport,
    ) -> Result<(), StoreError> {
        if self.pending_record.contains_key(rel) {
            return Ok(());
        }
        match kind_of(rel) {
            Kind::Derived => {
                if let Disk::Bytes(bytes) = self.read_disk(rel) {
                    report.read += 1;
                    self.found_outside(rel, &bytes, false)?;
                    report.found.push(rel.to_owned());
                }
            }
            Kind::Other if self.collecting => self.deferred_paths.push(rel.to_owned()),
            Kind::Other => self.look_at_carried_file(rel, report)?,
            Kind::Metadata | Kind::Fixed => {}
        }
        Ok(())
    }

    /// *Take in the file as an edit* (§6.2, APP-DESIGN R7): the bytes of the held file are
    /// examined with no base, so each record that differs from the project's is a conflict
    /// with its text, and the hold ends. `reply` is answered when the batch is durable.
    pub(crate) fn take_in_held(
        &mut self,
        rel: &str,
        reply: crate::handle::Resolver<Result<TakenIn, StoreError>>,
    ) {
        if let Some(reason) = self.read_only() {
            reply.resolve(Err(StoreError::Other(format!(
                "the project is read-only: {reason:?}"
            ))));
            return;
        }
        self.begin_newest();
        let held = self
            .manifest
            .paths
            .get(rel)
            .and_then(|e| e.held.as_ref().map(|h| h.cause));
        if held.is_none() {
            reply.resolve(Ok(TakenIn::default()));
            return;
        }
        if kind_of(rel) == Kind::Other {
            self.take_in_held_carried(rel, reply);
            return;
        }
        let bytes = match self.read_disk(rel) {
            Disk::Bytes(bytes) => bytes,
            Disk::Missing => {
                self.end_hold(rel);
                if let Some(file) = DerivedFile::from_path(rel) {
                    self.dirty.add_file(file);
                }
                let result = self.finish_looking().map(|()| TakenIn::default());
                reply.resolve(result);
                return;
            }
            Disk::Unreadable => {
                reply.resolve(Err(StoreError::Other(format!(
                    "{rel} cannot be read now; try again"
                ))));
                return;
            }
        };
        let sent = self.found_outside_answering(rel, &bytes, reply);
        if let Err(e) = sent {
            self.job_failed(&e);
        }
    }

    /// The batch a taken-in file made, for the person who asked.
    pub(crate) fn answer_taken_in(&mut self, rel: &str, outcome: &Recorded) {
        let Some(reply) = self.waiting.remove(rel) else {
            return;
        };
        reply.resolve(match outcome {
            Recorded::Done { batch, changes, .. } => Ok(TakenIn {
                batch: *batch,
                changes: *changes,
            }),
            Recorded::Refused(_) => Ok(TakenIn::default()),
            Recorded::NotWritten => Err(StoreError::Other(
                "the disk refused the changes; the file is still held".into(),
            )),
        });
    }
}
