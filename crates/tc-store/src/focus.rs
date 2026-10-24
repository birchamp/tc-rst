//! Looking at the folder after something may have changed it (`docs/STORAGE.md` §11.1): the
//! window got focus, the watcher told some paths, or the person chose to take in a held file.
//!
//! None of these projects a file. A path whose size and time are those of the bytes the ledger
//! placed, settled, is the app's own and costs a stat; any other is read and decided by the
//! ledger (§6.2): the app's own bytes take their stat again, others are recorded or held.

use crate::classify::Next;
use crate::error::StoreError;
use crate::layout;
use crate::ledger::{self, Projection, State};
use crate::materialize::{Disk, FIXED};
use crate::reply::{FocusReport, TakenIn};
use crate::writer::{Recorded, Writer};
use tc_journal::fold::DerivedFile;

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
                e.oid.as_deref() == Some(placement.oid.to_hex().as_str()) && e.held().is_none()
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

    /// One path of the ledger, or a derived, fixed or `metadata.json` path: read unless clean
    /// by stat (or held with the hold's size and time), and decided by the ledger (§6.2). A
    /// look does not project, so it writes nothing itself: what it marks, the job after it
    /// writes.
    fn look_at(&mut self, rel: &str, report: &mut FocusReport) -> Result<(), StoreError> {
        if self.pending_record.contains_key(rel) || self.busy.contains_key(rel) {
            return Ok(());
        }
        let kind = kind_of(rel);
        if kind == Kind::Other {
            // A carried file (§11.4), or a file the project knows nothing about.
            return self.look_at_carried_file(rel, report);
        }
        let path = self.folder.path(rel);
        let meta = match self.vfs.metadata(&path) {
            Ok(meta) => meta,
            Err(e) if crate::vfs::is_not_found(&e) => {
                // A deletion is no edit (I11): the file is written again, and a hold ends.
                if self.manifest.paths.contains_key(rel) {
                    let projection = match DerivedFile::from_path(rel) {
                        Some(file) if !self.fold.projects_file(&file) => Projection::Absent,
                        _ => Projection::NotAsked,
                    };
                    if self.decide_read(rel, None, projection, true)? == Next::Write {
                        match DerivedFile::from_path(rel) {
                            Some(file) => self.dirty.add_file(file),
                            None => self.dirty.metadata = true,
                        }
                    }
                }
                return Ok(());
            }
            Err(_) => return Ok(()),
        };
        if meta.kind != crate::vfs::EntryKind::File {
            return Ok(());
        }
        let state = self.state_of(rel);
        let held_as_is = matches!(state, State::Held { .. }) && self.same_stat(&state, Some(meta));
        if held_as_is || self.clean_by_stat(rel) {
            return Ok(());
        }
        let disk = match self.read_disk(rel) {
            Disk::Bytes(bytes) => bytes,
            Disk::Missing | Disk::Unreadable => return Ok(()),
        };
        report.read += 1;
        match kind {
            Kind::Derived => {
                if matches!(
                    self.decide_read(rel, Some(&disk), Projection::NotAsked, true)?,
                    Next::Recorded | Next::Held
                ) {
                    report.found.push(rel.to_owned());
                }
            }
            Kind::Metadata | Kind::Fixed => {
                let md5 = ledger::Disk::Bytes {
                    md5: self.hash(&disk),
                };
                let facts = self.facts_of(rel, &md5, Projection::NotAsked, true);
                let actions = self.read_event(rel, md5, &facts)?;
                for action in actions {
                    match action {
                        // `metadata.json` is never held: it must list the files as they are; a
                        // fixed file is never recorded, only restored after a copy (§3.5).
                        ledger::Action::Record(base) if kind == Kind::Metadata => {
                            self.keep_edited_metadata(&disk, base)?;
                            report.found.push(rel.to_owned());
                        }
                        ledger::Action::Hold(_) if kind == Kind::Metadata => {
                            self.keep_edited_metadata(&disk, ledger::Base::Unknown)?;
                            report.found.push(rel.to_owned());
                        }
                        ledger::Action::Record(_) | ledger::Action::Hold(_) => {
                            self.restore_fixed(rel, &disk)?;
                            report.found.push(rel.to_owned());
                        }
                        other => {
                            self.perform(rel, Some(&disk), vec![other])?;
                        }
                    }
                }
            }
            Kind::Other => {}
        }
        Ok(())
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
                    if matches!(
                        self.decide_read(rel, Some(&bytes), Projection::NotAsked, true)?,
                        Next::Recorded | Next::Held
                    ) {
                        report.found.push(rel.to_owned());
                    }
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
            .and_then(|e| e.held().map(|h| h.cause));
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
        // The table's TakeIn: the held bytes are recorded with no base.
        self.note(rel, crate::ledger::Event::TakeIn);
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
