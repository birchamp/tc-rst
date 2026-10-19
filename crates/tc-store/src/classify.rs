//! The writer's side of the placement ledger (`docs/STORAGE.md` §6.2): it gathers the facts of a
//! path (a read, a stat, the fold's answer), asks [`ledger::next`] what to do, keeps the state
//! it returns and performs the actions. O9 classifies every derived path with it, and O14's
//! results go through it again. Nothing here decides who wrote a file.

use crate::atomic::{SyncDir, atomic_create_dirs};
use crate::error::StoreError;
use crate::layout::{HEADS, METADATA};
use crate::ledger::{
    self, Action, Disk, Event, Facts, HeldCause, Projection, Stat, State, WaitFor,
};
use crate::manifest::Entry;
use crate::materialize::{FIXED, Projected, is_text_path};
use crate::open::CheckResult;
use crate::progress::{Phase, Tracker};
use crate::vfs::{FileMeta, is_not_found};
use crate::writer::Writer;
use std::collections::{BTreeMap, BTreeSet};
use tc_journal::fold::{Blocked, DerivedFile, Notice};
use tc_journal::{ChangeId, Md5, RepoPath};

fn md5_hex(bytes: &[u8]) -> String {
    Md5::of(bytes).to_hex()
}

/// What performing a read's decision left the caller to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Next {
    /// Write the projection over the bytes.
    Write,
    /// Remove the file.
    Remove,
    /// The bytes are what the project writes: only the entry changes.
    Converged,
    /// The bytes were copied and handed to the actor to be recorded (§11.1).
    Recorded,
    /// The bytes were copied and are held (§6.2).
    Held,
    /// Nothing to do now (the path left the projection and its file is gone, a newer fold
    /// placed it).
    Done,
    /// Left as it is for now, and tried again.
    Blocked(Blocked),
}

impl Writer {
    /// The ledger's state of `rel`.
    pub(crate) fn state_of(&self, rel: &str) -> State {
        self.manifest
            .paths
            .get(rel)
            .map_or(State::Unknown, |e| e.state.clone())
    }

    fn set_state(&mut self, rel: &str, state: State) {
        match (self.manifest.paths.get_mut(rel), state) {
            (Some(_), State::Unknown) => {
                self.manifest.paths.remove(rel);
                self.manifest_changed = true;
            }
            (Some(entry), state) => {
                if entry.state != state {
                    entry.state = state;
                    self.manifest_changed = true;
                }
            }
            (None, State::Unknown) => {}
            (None, state) => {
                self.manifest
                    .paths
                    .insert(rel.to_owned(), Entry::new(state));
                self.manifest_changed = true;
            }
        }
    }

    /// Hands `event` at `rel` to the ledger, keeps the state it returns, and returns what to
    /// do.
    pub(crate) fn apply(&mut self, rel: &str, event: Event, facts: &Facts) -> Vec<Action> {
        let state = self.state_of(rel);
        let (next, actions) = ledger::next(&state, &event, facts);
        if !matches!(next, State::Held { .. }) && matches!(state, State::Held { .. }) {
            // The hold ended: its notice goes, its copy stays.
            self.shared.remove_notices_of(rel);
        }
        self.set_state(rel, next);
        actions
    }

    /// An event that needs no facts (a write's steps, a stat, a recording, a file that cannot
    /// be read).
    pub(crate) fn note(&mut self, rel: &str, event: Event) {
        let facts = Facts::look(None, false);
        let _ = self.apply(rel, event, &facts);
    }

    /// Whether the fold holds what the bytes `md5` at `rel` show (§6.2): every change the
    /// ledger's heads of them name, applied (its batch complete or released, §7.4), and
    /// readable unless it is still a head the projection reads (whose readability is the
    /// projection's own fact). `None` when the ledger keeps no heads for them. A file that is
    /// no fold's projection (a fixed or carried file) shows no record.
    pub(crate) fn holds(&mut self, rel: &str, md5: &str) -> Option<bool> {
        if rel != METADATA && DerivedFile::from_path(rel).is_none() {
            return Some(true);
        }
        let heads = self.heads_of(rel)?;
        let records = heads.get(md5)?;
        let current = self.current_heads(rel);
        Some(records.iter().all(|record| {
            record
                .heads
                .iter()
                .all(|id| self.shows_held(&ChangeId::from_bytes(*id), &current))
        }))
    }

    /// Every head of every record the projection of `rel` reads.
    pub(crate) fn current_heads(&self, rel: &str) -> std::collections::HashSet<ChangeId> {
        let keys: Vec<tc_journal::RecordKey> = if rel == METADATA {
            self.fold
                .metadata_heads()
                .into_iter()
                .map(|(k, _)| k)
                .collect()
        } else if let Some(file) = DerivedFile::from_path(rel) {
            self.fold
                .file_heads(&file)
                .into_iter()
                .map(|(k, _)| k)
                .collect()
        } else {
            Vec::new()
        };
        keys.iter().flat_map(|key| self.fold.heads(key)).collect()
    }

    /// Whether the fold holds change `id` applied and can give its value: a head the
    /// projection reads (`current`) is read by the projection itself; any other version is
    /// read here (a journal file cut while open leaves the fold unable to).
    pub(crate) fn shows_held(
        &self,
        id: &ChangeId,
        current: &std::collections::HashSet<ChangeId>,
    ) -> bool {
        self.fold.change(id).is_some_and(|view| view.folded)
            && (current.contains(id) || self.fold.raw(id).is_some())
    }

    /// Whether the file's size and time, `meta`, are the ones the ledger keeps for the bytes
    /// placed or held. Never on a network drive (§6.2).
    pub(crate) fn same_stat(&self, state: &State, meta: Option<FileMeta>) -> bool {
        if self.network {
            return false;
        }
        let Some(meta) = meta else {
            return false;
        };
        let kept = match state {
            State::Placed { placed } => placed.stat(),
            State::Held { held } => held.mtime_ns.map(|mtime_ns| Stat {
                size: held.size,
                mtime_ns,
            }),
            State::Writing { .. } | State::Unknown => None,
        };
        kept.is_some_and(|s| s.size == meta.len && s.mtime_ns == meta.mtime_ns)
    }

    /// Whether `rel` is clean by stat (§6.2): the bytes placed are settled, the file is there
    /// with their size and time, and the drive is not a network drive. Its bytes are then
    /// taken to be the placed ones without a read.
    pub(crate) fn clean_by_stat(&self, rel: &str) -> bool {
        let state = self.state_of(rel);
        let State::Placed { placed } = &state else {
            return false;
        };
        placed.settled && self.same_stat(&state, self.vfs.metadata(&self.folder.path(rel)).ok())
    }

    /// Whether a read that began after the newest time stamp settles bytes whose file has
    /// this mtime (§6.2).
    pub(crate) fn settles(&self, mtime_ns: i64) -> bool {
        !self.network && self.stamp_ns >= mtime_ns.saturating_add(2_000_000_000)
    }

    /// The facts of a read of `rel` that found `disk`.
    pub(crate) fn facts_of(
        &mut self,
        rel: &str,
        disk: &Disk,
        projection: Projection,
        heads_readable: bool,
    ) -> Facts {
        let state = self.state_of(rel);
        // Bytes that are the projection are converged whatever they show (the table's first
        // row), so the ledger's heads of them are not read.
        let converged = |md5: &str| {
            heads_readable && matches!(&projection, Projection::Bytes { md5: e } if e == md5)
        };
        let holds = match disk {
            Disk::Bytes { md5 } if !converged(md5) => self.holds(rel, md5),
            Disk::Bytes { .. } | Disk::Missing | Disk::Unreadable => None,
        };
        let meta = self.vfs.metadata(&self.folder.path(rel)).ok();
        Facts {
            projection,
            heads_readable,
            holds,
            same_stat: self.same_stat(&state, meta),
            newer_fold: self.written_from_newer_fold(rel),
        }
    }

    /// A read of `rel` that found `bytes` (or no file), decided and performed. `projection`:
    /// what the project writes there, with whether every head it read could be read.
    pub(crate) fn decide_read(
        &mut self,
        rel: &str,
        bytes: Option<&[u8]>,
        projection: Projection,
        heads_readable: bool,
    ) -> Result<Next, StoreError> {
        let disk = match bytes {
            Some(bytes) => Disk::Bytes {
                md5: self.hash(bytes),
            },
            None => Disk::Missing,
        };
        let facts = self.facts_of(rel, &disk, projection, heads_readable);
        let actions = self.read_event(rel, disk, &facts)?;
        self.perform(rel, bytes, actions)
    }

    /// Hands a read to the ledger. A rename a stopped process made may be in the page cache
    /// only: once the ledger takes its bytes as placed, they are flushed, so a power cut cannot
    /// put back the bytes before it while the ledger names these (§6.4).
    pub(crate) fn read_event(
        &mut self,
        rel: &str,
        disk: Disk,
        facts: &Facts,
    ) -> Result<Vec<Action>, StoreError> {
        let renamed = matches!((&self.state_of(rel), &disk),
            (State::Writing { new, .. }, Disk::Bytes { md5 }) if new == md5);
        let actions = self.apply(rel, Event::Read(disk), facts);
        if renamed {
            let path = self.folder.path(rel);
            let (dir, _) = self.folder.dir_and_name(rel);
            self.vfs
                .sync_file(&path)
                .and_then(|()| self.vfs.sync_dir(&dir))
                .map_err(|e| StoreError::io("flush", &path, e))?;
        }
        Ok(actions)
    }

    /// Performs the actions of a read's decision.
    pub(crate) fn perform(
        &mut self,
        rel: &str,
        bytes: Option<&[u8]>,
        actions: Vec<Action>,
    ) -> Result<Next, StoreError> {
        let mut next = Next::Done;
        for action in actions {
            next = match action {
                Action::Write => Next::Write,
                Action::Remove => Next::Remove,
                Action::Rebuild { told } => {
                    if told && let Ok(path) = RepoPath::parse(rel) {
                        self.notice(Notice::MissingFileRebuilt { path }, None);
                    }
                    Next::Write
                }
                Action::Refresh => {
                    if let Some(bytes) = bytes {
                        self.take_stat(rel, bytes);
                    }
                    Next::Converged
                }
                Action::Record(base) => {
                    if let Some(bytes) = bytes {
                        self.found_outside(rel, bytes, base)?;
                    }
                    Next::Recorded
                }
                Action::Hold(cause) => {
                    if let Some(bytes) = bytes {
                        self.hold(rel, bytes, cause)?;
                    }
                    Next::Held
                }
                Action::Wait(WaitFor::FileUnreadable) => Next::Blocked(Blocked::FileUnreadable),
                Action::Wait(WaitFor::ValuesUnreadable) => {
                    Next::Blocked(Blocked::ValuesUnreadable {
                        records: u32::try_from(self.fold.unreadable_records().len())
                            .unwrap_or(u32::MAX)
                            .max(1),
                    })
                }
                Action::Wait(WaitFor::NewerFold) | Action::Forget => Next::Done,
            };
        }
        Ok(next)
    }

    /// The file's size and time after a read found the bytes the ledger says are there; they
    /// settle when the read began 2 s or more after the time (§6.2).
    fn take_stat(&mut self, rel: &str, bytes: &[u8]) {
        let meta = self.vfs.metadata(&self.folder.path(rel)).ok();
        let stat = Stat {
            size: meta.map_or(bytes.len() as u64, |m| m.len),
            mtime_ns: meta.map_or(0, |m| m.mtime_ns),
        };
        let settled = meta.is_some() && self.settles(stat.mtime_ns);
        self.note(rel, Event::Stat { stat, settled });
        // `metadata.json` declares the normalization of the text it lists.
        if is_text_path(rel)
            && let Some(entry) = self.manifest.paths.get_mut(rel)
        {
            let nfc = Some(unicode_normalization::is_nfc(&String::from_utf8_lossy(
                bytes,
            )));
            if entry.nfc != nfc {
                entry.nfc = nfc;
                self.manifest_changed = true;
            }
        }
    }

    /// Holds `rel` (§6.2): a copy of its bytes in `.tc/external/`, the file left as it is, the
    /// ledger's state held, and a notice. Holding a path again replaces its notice with one
    /// for the new bytes; both copies stay.
    pub(crate) fn hold(
        &mut self,
        rel: &str,
        bytes: &[u8],
        cause: HeldCause,
    ) -> Result<(), StoreError> {
        let copy = self.copy_external(rel, bytes)?;
        let stat = self
            .vfs
            .metadata(&self.folder.path(rel))
            .ok()
            .map(|m| Stat {
                size: m.len,
                mtime_ns: m.mtime_ns,
            });
        self.note(
            rel,
            Event::Kept {
                md5: md5_hex(bytes),
                cause,
                copy: copy.clone(),
                stat,
            },
        );
        if let Some(entry) = self.manifest.paths.get_mut(rel) {
            entry.nfc = is_text_path(rel)
                .then(|| unicode_normalization::is_nfc(&String::from_utf8_lossy(bytes)));
        }
        self.write_manifest()?;
        self.shared.remove_notices_of(rel);
        self.hold_notice(rel, cause, copy);
        Ok(())
    }

    /// O9 (§6.2): classifies every derived path, reading and projecting only what may have
    /// changed, and marks what to write. Returns the paths taken as converged without being
    /// read, for the storage check (O14).
    pub(crate) fn classify(&mut self, tracker: &Tracker) -> Result<Vec<String>, StoreError> {
        let files = self.fold.derived_files();
        tracker.phase(Phase::Checking, files.len() as u64 + 1, 0);
        let digests = self.fold.scope_digests(&files);
        let versions = self.manifest.versions_current();
        let mut unread: Vec<String> = Vec::new();
        let mut to_project: Vec<(String, DerivedFile, Option<Vec<u8>>)> = Vec::new();
        let projected: BTreeSet<String> = files.iter().map(DerivedFile::path).collect();
        for (file, digest) in files.iter().zip(&digests) {
            let rel = file.path();
            let entry = self.manifest.paths.get(&rel).cloned();
            // The projection of a path whose scope and versions are unchanged is the bytes
            // the app placed there (I1).
            let placed_projection = entry
                .as_ref()
                .filter(|e| versions && e.scope.as_deref() == Some(digest.to_hex().as_str()))
                .and_then(|e| e.md5().map(str::to_owned));
            if placed_projection.is_some() && self.clean_by_stat(&rel) {
                unread.push(rel);
                tracker.add(1, 0);
                continue;
            }
            match self.read_disk(&rel) {
                crate::materialize::Disk::Unreadable => {
                    // A file another program holds, or a placeholder: neither damaged nor
                    // gone. Nothing is written over it, and it is tried again (§4.4, §6.1).
                    self.unreadable_derived(&rel, file);
                }
                crate::materialize::Disk::Bytes(bytes)
                    if placed_projection
                        .as_deref()
                        .is_some_and(|m| *m == md5_hex(&bytes)) =>
                {
                    // Read, not projected: the bytes are the projection.
                    let md5 = md5_hex(&bytes);
                    self.counters.add(&self.counters.derived_read, 1);
                    let had = entry.clone();
                    let next =
                        self.decide_read(&rel, Some(&bytes), Projection::Bytes { md5 }, true)?;
                    self.after_classified(&rel, file, next, &bytes, None, had);
                }
                crate::materialize::Disk::Bytes(bytes) => {
                    self.counters.add(&self.counters.derived_read, 1);
                    to_project.push((rel, file.clone(), Some(bytes)));
                }
                crate::materialize::Disk::Missing => to_project.push((rel, file.clone(), None)),
            }
            tracker.add(1, 0);
        }
        // Paths only the ledger lists: a removal that stopped before M7, or files to go.
        let listed: Vec<String> = self
            .manifest
            .paths
            .iter()
            .filter(|(p, e)| {
                !projected.contains(*p)
                    && p.as_str() != METADATA
                    && !FIXED.contains(&p.as_str())
                    && e.oid.is_none()
            })
            .map(|(p, _)| p.clone())
            .collect();
        for rel in listed {
            match self.vfs.metadata(&self.folder.path(&rel)) {
                Err(e) if is_not_found(&e) => {
                    let facts = self.facts_of(&rel, &Disk::Missing, Projection::Absent, true);
                    let _ = self.apply(&rel, Event::Read(Disk::Missing), &facts);
                }
                // Still on disk: the materialization looks at it and removes it (M0, X).
                _ => self.dirty.metadata = true,
            }
        }
        if !to_project.is_empty() {
            let wanted: BTreeSet<DerivedFile> =
                to_project.iter().map(|(_, f, _)| f.clone()).collect();
            let mut rendered = self.render_files(&wanted);
            for (rel, file, disk) in to_project {
                let Some((new, readable)) = rendered.remove(&rel) else {
                    continue;
                };
                let projection = Projection::Bytes {
                    md5: md5_hex(&new.bytes),
                };
                let had = self.manifest.paths.get(&rel).cloned();
                let next = self.decide_read(&rel, disk.as_deref(), projection, readable)?;
                let bytes = disk.unwrap_or_default();
                self.after_classified(&rel, &file, next, &bytes, Some(new), had);
            }
        }
        // metadata.json: converged when the bytes placed are clean by stat and list what it
        // would. Any other state is read and decided when it is written (M6).
        let scope = self.metadata_scope();
        let metadata_ok =
            versions
                && self.manifest.paths.get(METADATA).is_some_and(|e| {
                    e.scope.as_deref() == Some(scope.as_str()) && e.md5().is_some()
                })
                && (self.clean_by_stat(METADATA) || self.metadata_matches_entry());
        if !metadata_ok {
            self.dirty.metadata = true;
        }
        tracker.add(1, 0);
        self.renew_hold_notices();
        Ok(unread)
    }

    /// What a classified path needs next: a write is marked (with its projection kept for the
    /// job), converged bytes get their scope, and their heads when the ledger did not name
    /// them with that scope before (`had`), and a blocked path is marked to be tried again.
    fn after_classified(
        &mut self,
        rel: &str,
        file: &DerivedFile,
        next: Next,
        disk: &[u8],
        new: Option<Projected>,
        had: Option<Entry>,
    ) {
        match next {
            Next::Write | Next::Remove | Next::Blocked(_) => {
                self.dirty.files.insert(file.clone());
                if let Some(new) = new {
                    self.prerendered.insert(rel.to_owned(), new);
                }
            }
            Next::Converged => {
                let scope = self.fold.scope_digest(file).to_hex();
                let md5 = md5_hex(disk);
                let named = had.as_ref().is_some_and(|e| {
                    e.md5() == Some(md5.as_str()) && e.scope.as_deref() == Some(scope.as_str())
                });
                if let Some(entry) = self.manifest.paths.get_mut(rel)
                    && entry.scope.as_deref() != Some(scope.as_str())
                {
                    entry.scope = Some(scope);
                    self.manifest_changed = true;
                }
                if !named {
                    // Bytes the ledger did not name with these heads (another program wrote
                    // the projection, a write stopped after its rename, the heads moved with
                    // the same bytes): their heads are the fold's, so a later edit of them
                    // has its base (§11.1 D7). Best effort.
                    let digests = new.map(|n| n.digests).unwrap_or_default();
                    let dir = self.folder.path(HEADS);
                    let _ = atomic_create_dirs(&*self.vfs, &dir)
                        .and_then(|()| self.write_heads(rel, &md5, file, &digests, SyncDir::Now));
                }
            }
            Next::Recorded | Next::Held | Next::Done => {}
        }
    }

    /// A derived file the open could not read: the person is told (`FileUnreadable`), the
    /// file is left as it is, and the path stays dirty, so the next materialization reads it
    /// again (M1) and decides it there.
    fn unreadable_derived(&mut self, rel: &str, file: &DerivedFile) {
        self.dirty.files.insert(file.clone());
        self.notice(
            Notice::FileUnreadable {
                path: rel.into(),
                reason: "another program has it open, or it is not on this computer yet".into(),
            },
            None,
        );
    }

    /// Whether `metadata.json` holds what the ledger says the app placed. A file that cannot
    /// be read now is not known to, so `metadata.json` is written again.
    fn metadata_matches_entry(&self) -> bool {
        let Some(md5) = self.manifest.paths.get(METADATA).and_then(|e| e.md5()) else {
            return false;
        };
        matches!(self.read_disk(METADATA), crate::materialize::Disk::Bytes(bytes) if md5_hex(&bytes) == md5)
    }

    /// Renders `files`, a book's files together, with the digests of a USFM file's records,
    /// and whether every value the rendering read could be read.
    pub(crate) fn render_files(
        &self,
        files: &BTreeSet<DerivedFile>,
    ) -> BTreeMap<String, (Projected, bool)> {
        let mut out = BTreeMap::new();
        let mut books = BTreeSet::new();
        for file in files {
            match file.book() {
                Some(book) => {
                    books.insert(book);
                }
                None => {
                    let before = self.fold.values_unreadable();
                    if let Some(projected) = self.fold.render_with_digests(file) {
                        self.counters.add(&self.counters.projected, 1);
                        let readable = self.fold.values_unreadable() == before;
                        out.insert(
                            file.path(),
                            (
                                Projected {
                                    bytes: projected.bytes,
                                    digests: projected.digests,
                                },
                                readable,
                            ),
                        );
                    }
                }
            }
        }
        for book in books {
            let before = self.fold.values_unreadable();
            let rendered = self.fold.render_book_with_digests(book);
            let readable = self.fold.values_unreadable() == before;
            for projected in rendered {
                if files.contains(&projected.file) {
                    self.counters.add(&self.counters.projected, 1);
                    out.insert(
                        projected.file.path(),
                        (
                            Projected {
                                bytes: projected.bytes,
                                digests: projected.digests,
                            },
                            readable,
                        ),
                    );
                }
            }
        }
        out
    }

    /// O14 results (§6.1): equal bytes settle a placement once its tick is over; different
    /// bytes are decided again on the writer, where nothing can change them meanwhile.
    pub(crate) fn apply_check(&mut self, results: Vec<CheckResult>) -> Result<(), StoreError> {
        for result in results {
            let Some(entry) = self.manifest.paths.get(&result.rel).cloned() else {
                continue;
            };
            let Some(placed) = entry.placement().cloned() else {
                continue;
            };
            match &result.md5 {
                Some(md5) if *md5 == placed.md5 => {
                    let settled = !self.network
                        && placed.size == result.size
                        && placed.mtime_ns == Some(result.mtime_ns)
                        && result.stamp_ns >= result.mtime_ns.saturating_add(2_000_000_000);
                    if settled && !placed.settled {
                        self.note(
                            &result.rel,
                            Event::Stat {
                                stat: Stat {
                                    size: result.size,
                                    mtime_ns: result.mtime_ns,
                                },
                                settled: true,
                            },
                        );
                    }
                }
                Some(_) => {
                    // Read again here: the file may have changed since the check read it.
                    let Some(file) =
                        DerivedFile::from_path(&result.rel).filter(|f| self.fold.projects_file(f))
                    else {
                        continue;
                    };
                    let crate::materialize::Disk::Bytes(disk) = self.read_disk(&result.rel) else {
                        continue;
                    };
                    let mut rendered = self.render_files(&BTreeSet::from([file.clone()]));
                    if let Some((new, readable)) = rendered.remove(&result.rel) {
                        let projection = Projection::Bytes {
                            md5: md5_hex(&new.bytes),
                        };
                        let had = Some(entry.clone());
                        let next =
                            self.decide_read(&result.rel, Some(&disk), projection, readable)?;
                        self.after_classified(&result.rel, &file, next, &disk, Some(new), had);
                    }
                }
                None => {}
            }
        }
        if self.manifest_changed {
            self.write_manifest()?;
        }
        Ok(())
    }
}
