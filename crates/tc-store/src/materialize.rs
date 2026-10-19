//! Materialize derived files (`docs/STORAGE.md` §5.10, M0 to M8): write the derived files
//! whose projection changed and `metadata.json`, with an intent file, a manifest and a
//! compare-and-swap, so the app never writes over a file it did not write last (I8).
//!
//! A derived file that changed outside the app is not written over: a copy of its bytes goes
//! to `.tc/external/`, and the file is either held (its time says nothing was saved, so the
//! bytes are storage damage, §6.2) or examined and recorded as changes (§11.1, `outside`),
//! and then written again from the project. Carried files are placed from the object database
//! (`carried`); they and the fixed files are listed in `metadata.json` as their bytes are on
//! disk (I7), read or verified by stat as a derived file is.
//!
//! This runs on the writer, never on the actor: a path whose rename another program refuses
//! stays dirty, and the next attempt starts at M0 again after a delay that grows to 3 s
//! (§5.10).

use crate::appdata::utc_stamp;
use crate::atomic::{
    DirSet, HeldRetry, SyncDir, Written, atomic_create_dirs, atomic_remove, atomic_write,
    atomic_write_checked,
};
use crate::error::StoreError;
use crate::layout::{self, EXTERNAL, HEADS, INTENT, MANIFEST, METADATA, TC};
use crate::manifest::{Entry, HeadsFile, Held, HeldCause, Intent, IntentWrite, PackEntry};
use crate::vfs::{FileMeta, is_not_found};
use crate::writer::Writer;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};
use tc_burrito::{IngredientEntry, IngredientPath};
use tc_journal::fold::{Blocked, DerivedFile, Notice, ProjectKind, Resolved};
use tc_journal::record::CarriedValue;
use tc_journal::{BookId, FORMAT_MAJOR, FORMAT_MINOR, Md5, Record, RecordKey, RepoPath};
use unicode_normalization::is_nfc;

/// How long a path has been refused before the person is told (§5.10).
pub(crate) const BUSY_NOTICE_AFTER: Duration = Duration::from_secs(2);
/// The first and the longest wait before a refused path is tried again (§5.10).
pub(crate) const BUSY_RETRY_FIRST: Duration = Duration::from_millis(200);
pub(crate) const BUSY_RETRY_LAST: Duration = Duration::from_secs(3);

/// A derived path another program refused the app (§5.10): since when, and whether the
/// person was told.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Busy {
    pub since: Instant,
    pub told: bool,
}

/// What a read of a path found. A file that cannot be read is neither damaged nor gone: the
/// app writes nothing over it and removes nothing, and retries (§4.4, §6.1).
#[derive(Debug)]
pub(crate) enum Disk {
    Missing,
    Unreadable,
    Bytes(Vec<u8>),
}

/// What the write of one derived file did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wrote {
    Done,
    /// Another program holds the file.
    Busy,
    /// The file is not as M1 found it.
    Refused,
}

/// A file M1 found already equal to the projection: only its entry changes, and its heads
/// file when that does not describe the bytes (a write that stopped after its rename).
struct Converged {
    file: DerivedFile,
    md5: String,
    nfc: bool,
    digests: Vec<(RecordKey, tc_journal::outside::ShownDigest)>,
}

/// What one materialization did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MaterializeReport {
    pub written: Vec<String>,
    pub removed: Vec<String>,
    /// Paths held: changed outside the app, left as they are (§6.2).
    pub held: Vec<String>,
    /// Paths changed outside the app that were examined and sent to be recorded (§11.1): they
    /// are written again once the batch is durable.
    pub recorded: Vec<String>,
    /// Paths another program refused or kept unreadable: still dirty, tried again.
    pub blocked: Vec<String>,
}

/// Files a project lists in `metadata.json` that it writes once or derives apart from the
/// fold's files: `project.json` and `format.json` (§3.5, §12).
pub(crate) const FIXED: [&str; 2] = [layout::PROJECT_JSON, layout::FORMAT_JSON];

fn md5_hex(bytes: &[u8]) -> String {
    Md5::of(bytes).to_hex()
}

/// Whether a derived file's bytes are text whose NFC form `metadata.json` declares.
fn is_text(file: &DerivedFile) -> bool {
    matches!(file, DerivedFile::Usfm(_) | DerivedFile::Story(_))
}

/// Whether a path holds text whose NFC form `metadata.json` declares (a USFM book or a
/// story).
pub(crate) fn is_text_path(rel: &str) -> bool {
    rel.ends_with(".usfm") || rel.ends_with(".md")
}

/// `currentScope` of an OBS project: the template's table verbatim (§3.2).
pub(crate) fn obs_scope() -> serde_json::Value {
    tc_obs::template::template_current_scope()
}

impl Writer {
    /// A read of `rel` that tells a file gone from a file that cannot be read now.
    pub(crate) fn read_disk(&self, rel: &str) -> Disk {
        let path = self.folder.path(rel);
        match self.vfs.read(&path) {
            Ok(bytes) => Disk::Bytes(bytes),
            Err(e) if is_not_found(&e) => Disk::Missing,
            Err(_) => Disk::Unreadable,
        }
    }

    /// Whether `rel` is clean by stat (§6.2): its entry settled and not held, the file
    /// there with the entry's size and mtime, and not on a network drive.
    pub(crate) fn clean_by_stat(&self, rel: &str) -> bool {
        if self.network {
            return false;
        }
        let Some(entry) = self.manifest.paths.get(rel) else {
            return false;
        };
        if !entry.settled || entry.held.is_some() {
            return false;
        }
        match self.vfs.metadata(&self.folder.path(rel)) {
            Ok(meta) => meta.len == entry.size && meta.mtime_ns == entry.mtime_ns,
            Err(_) => false,
        }
    }

    /// Whether a read that began after the newest time stamp settles an entry whose file
    /// has this mtime (§6.2).
    pub(crate) fn settles(&self, mtime_ns: i64) -> bool {
        !self.network && self.stamp_ns >= mtime_ns.saturating_add(2_000_000_000)
    }

    /// Holds `rel` (§6.2): a copy of its bytes in `.tc/external/`, the file left as it is,
    /// the manifest entry marked held, and a notice. Holding a path again replaces its
    /// notice with one for the new bytes; both copies stay.
    pub(crate) fn hold(&mut self, rel: &str, bytes: &[u8], damage: bool) -> Result<(), StoreError> {
        let copy = self.copy_external(rel, bytes)?;
        let cause = if damage {
            HeldCause::Damage
        } else {
            HeldCause::Edit
        };
        let meta = self.vfs.metadata(&self.folder.path(rel)).ok();
        let entry = self
            .manifest
            .paths
            .entry(rel.to_owned())
            .or_insert_with(|| Entry::of(String::new(), 0, 0, false));
        entry.held = Some(Held {
            md5: md5_hex(bytes),
            cause,
            copy: copy.clone(),
        });
        // `metadata.json` declares the normalization of the bytes it lists.
        entry.nfc = is_text_path(rel).then(|| is_nfc(&String::from_utf8_lossy(bytes)));
        if let Some(meta) = meta {
            entry.size = meta.len;
            entry.mtime_ns = meta.mtime_ns;
        }
        self.write_manifest()?;
        self.shared.remove_notices_of(rel);
        self.hold_notice(rel, cause, copy);
        Ok(())
    }

    /// The notice of a held path, once per session and path.
    pub(crate) fn hold_notice(&self, rel: &str, cause: HeldCause, copy: String) {
        let Ok(path) = RepoPath::parse(rel) else {
            return;
        };
        let notice = match cause {
            HeldCause::Damage => Notice::StorageDamage {
                path,
                copy: copy.as_str().into(),
            },
            HeldCause::Edit => Notice::OutsideEditHeld {
                path,
                copy: copy.as_str().into(),
            },
        };
        self.notice(notice, Some(copy));
    }

    /// A byte copy of `bytes` in `.tc/external/`, named by the time and the path; the path
    /// relative to the project folder. No copy replaces another: a name taken by other bytes
    /// (two edits in one second) gets `-2`, `-3`, …, and a name taken by these bytes is
    /// the copy already there.
    pub(crate) fn copy_external(&self, rel: &str, bytes: &[u8]) -> Result<String, StoreError> {
        let base = format!("{}-{}", utc_stamp(self.clock.now()), layout::escaped(rel));
        let dir = self.folder.path(EXTERNAL);
        atomic_create_dirs(&*self.vfs, &dir)?;
        let mut name = base.clone();
        for n in 2..1000 {
            match self.vfs.read(&dir.join(&name)) {
                Err(e) if is_not_found(&e) => break,
                Ok(existing) if existing == bytes => return Ok(format!("{EXTERNAL}/{name}")),
                _ => name = format!("{base}-{n}"),
            }
        }
        atomic_write(&*self.vfs, &dir, &name, bytes, SyncDir::Now).map_err(|e| e.into_inner())?;
        Ok(format!("{EXTERNAL}/{name}"))
    }

    /// Ends the hold of `rel`: the file it held is gone or equals the projection, or the
    /// person chose (§6.2). The notice of the hold goes with it; the byte copy stays in
    /// `.tc/external/`.
    pub(crate) fn end_hold(&mut self, rel: &str) {
        if let Some(entry) = self.manifest.paths.get_mut(rel)
            && entry.held.take().is_some()
        {
            entry.settled = false;
            self.manifest_changed = true;
        }
        self.shared.remove_notices_of(rel);
    }

    /// The notice of every hold the manifest keeps whose notice is not listed: the notices
    /// live in memory, so after a restart this brings them back, each with its own words
    /// and its copy (§6.2 "the notice stays listed"). A held file that is gone ends its
    /// hold.
    pub(crate) fn renew_hold_notices(&mut self) {
        let held: Vec<(String, Held)> = self
            .manifest
            .paths
            .iter()
            .filter_map(|(p, e)| Some((p.clone(), e.held.clone()?)))
            .collect();
        for (rel, held) in held {
            if let Err(e) = self.vfs.metadata(&self.folder.path(&rel))
                && is_not_found(&e)
            {
                self.end_hold(&rel);
                continue;
            }
            if !self.shared.has_hold_notice(&rel) {
                self.hold_notice(&rel, held.cause, held.copy);
            }
        }
    }

    /// Takes the bytes just read at `rel` as what the app last saw there: a new entry, or
    /// the old one with the file's size and mtime (settled when the read began after the
    /// tick, §6.2). A carried file keeps its blob id.
    fn take_listed(&mut self, rel: &str, md5: Md5, size: u64, oid: Option<String>) {
        let meta = self.vfs.metadata(&self.folder.path(rel)).ok();
        let mtime = meta.map_or(0, |m| m.mtime_ns);
        let settled = meta.is_some() && self.settles(mtime);
        let entry = self
            .manifest
            .paths
            .entry(rel.to_owned())
            .or_insert_with(|| Entry::of(md5.to_hex(), size, mtime, settled));
        let changed = entry.md5 != md5.to_hex()
            || entry.size != size
            || entry.mtime_ns != mtime
            || entry.settled != settled
            || (oid.is_some() && entry.oid != oid);
        entry.md5 = md5.to_hex();
        entry.size = size;
        entry.mtime_ns = mtime;
        entry.settled = settled;
        if oid.is_some() {
            entry.oid = oid;
        }
        self.manifest_changed |= changed;
    }

    /// What `metadata.json` lists for a file the fold does not project: a fixed file, a
    /// carried file, or a held file whose path left the projection. The burrito's checks read
    /// the bytes on disk, so those are what is listed (I7). The manifest stands for them
    /// while the file is clean by stat; otherwise the file is read, and bytes the app did not
    /// write are held with a copy and a notice, not overwritten and not left out (I8).
    fn listing_of_file(
        &mut self,
        rel: &str,
        oid: Option<String>,
    ) -> Result<Option<(Md5, u64)>, StoreError> {
        let entry = self.manifest.paths.get(rel).cloned();
        let listed = |e: &Entry| Md5::parse(&e.md5).map(|m| (m, e.size));
        if let Some(entry) = &entry
            && let Some(held) = &entry.held
        {
            return self.listing_of_held(rel, entry, held);
        }
        if let Some(entry) = &entry
            && self.clean_by_stat(rel)
        {
            return Ok(listed(entry));
        }
        if oid.is_some() {
            // A carried file is read in chunks, never whole (§11.4).
            return match self.vfs.metadata(&self.folder.path(rel)) {
                Err(e) if is_not_found(&e) => Ok(None),
                Err(_) => Ok(entry.as_ref().and_then(listed)),
                Ok(_) => match self.hash_file(rel) {
                    Err(_) => Ok(entry.as_ref().and_then(listed)),
                    Ok((md5, size)) => {
                        if entry.as_ref().is_none_or(|e| e.md5 == md5.to_hex()) {
                            self.take_listed(rel, md5, size, oid);
                        } else {
                            // Bytes the app did not place: the list says what is there, and
                            // the look at the carried files records or places it.
                            self.dirty.carried = true;
                        }
                        Ok(Some((md5, size)))
                    }
                },
            };
        }
        match self.read_disk(rel) {
            Disk::Missing => Ok(None),
            Disk::Unreadable => Ok(entry.as_ref().and_then(listed)),
            Disk::Bytes(bytes) => {
                let md5 = Md5::of(&bytes);
                let size = bytes.len() as u64;
                if entry.as_ref().is_some_and(|e| e.md5 != md5.to_hex()) {
                    if FIXED.contains(&rel) {
                        // Never recorded: written again after a copy (§3.5).
                        return self.restore_fixed(rel, &bytes).map(|()| {
                            self.manifest
                                .paths
                                .get(rel)
                                .and_then(|e| Md5::parse(&e.md5).map(|m| (m, e.size)))
                        });
                    }
                    self.hold(rel, &bytes, false)?;
                } else {
                    self.take_listed(rel, md5, size, oid);
                }
                Ok(Some((md5, size)))
            }
        }
    }

    /// What `metadata.json` lists for a held file: the bytes it was held with while its size
    /// and time are those of the hold; bytes that changed again are read and held again,
    /// since the burrito must list what is in the folder (I7). A file that is gone ends its
    /// hold and is not listed.
    fn listing_of_held(
        &mut self,
        rel: &str,
        entry: &Entry,
        held: &Held,
    ) -> Result<Option<(Md5, u64)>, StoreError> {
        let same = |size: u64| Md5::parse(&held.md5).map(|m| (m, size));
        match self.vfs.metadata(&self.folder.path(rel)) {
            Err(e) if is_not_found(&e) => {
                self.end_hold(rel);
                Ok(None)
            }
            Err(_) => Ok(same(entry.size)),
            Ok(meta) if meta.len == entry.size && meta.mtime_ns == entry.mtime_ns => {
                Ok(same(entry.size))
            }
            Ok(meta) => match self.read_disk(rel) {
                Disk::Missing => {
                    self.end_hold(rel);
                    Ok(None)
                }
                Disk::Unreadable => Ok(same(entry.size)),
                Disk::Bytes(bytes) => {
                    let md5 = Md5::of(&bytes);
                    if md5.to_hex() == held.md5 {
                        if let Some(e) = self.manifest.paths.get_mut(rel) {
                            e.size = meta.len;
                            e.mtime_ns = meta.mtime_ns;
                            self.manifest_changed = true;
                        }
                    } else {
                        // Saved again while held: the app's version is gone, so the bytes
                        // are recorded with no base (§6.2).
                        self.found_outside(rel, &bytes, true)?;
                    }
                    Ok(Some((md5, bytes.len() as u64)))
                }
            },
        }
    }

    /// Writes the manifest and takes its mtime as the newest time stamp (§6.2, M7).
    pub(crate) fn write_manifest(&mut self) -> Result<(), StoreError> {
        let dir = self.folder.path(TC);
        atomic_create_dirs(&*self.vfs, &dir)?;
        let bytes = self.manifest.to_bytes();
        atomic_write(&*self.vfs, &dir, "materialized.json", &bytes, SyncDir::Now)
            .map_err(|e| e.into_inner())?;
        if let Ok(meta) = self.vfs.metadata(&self.folder.path(MANIFEST)) {
            self.stamp_ns = self.stamp_ns.max(meta.mtime_ns);
        }
        self.manifest_changed = false;
        Ok(())
    }

    /// M0: a hold ends when its file is gone, and the file is written again (§6.2): a
    /// deletion is never read as an edit. A path the projection no longer has is dropped by
    /// the removal rules after it.
    fn release_missing_holds(&mut self) {
        let held: Vec<String> = self
            .manifest
            .paths
            .iter()
            .filter(|(_, e)| e.held.is_some())
            .map(|(p, _)| p.clone())
            .collect();
        for rel in held {
            let gone = self
                .vfs
                .metadata(&self.folder.path(&rel))
                .is_err_and(|e| is_not_found(&e));
            if !gone {
                continue;
            }
            self.end_hold(&rel);
            if let Some(file) = DerivedFile::from_path(&rel) {
                self.dirty.add_file(file);
            }
            self.dirty.metadata = true;
        }
    }

    /// The facts `metadata.json` lists for a pack: from the manifest when its size and
    /// fingerprint are the ones recorded, else read once.
    pub(crate) fn pack_entry(&mut self, rel: &str) -> Result<Option<PackEntry>, StoreError> {
        let Some(info) = self.packs.get(rel).cloned() else {
            return Ok(None);
        };
        let fingerprint = format!("{:016x}", info.fingerprint);
        if let Some(entry) = self.manifest.packs.get(rel)
            && entry.size == info.size
            && entry.fingerprint == fingerprint
        {
            return Ok(Some(entry.clone()));
        }
        let Disk::Bytes(bytes) = self.read_disk(rel) else {
            return Ok(None);
        };
        let entry = PackEntry {
            md5: md5_hex(&bytes),
            size: bytes.len() as u64,
            fingerprint: format!("{:016x}", tc_journal::index::fingerprint(&bytes)),
            blob: info.blob.map(|b| b.to_hex()),
        };
        self.manifest.packs.insert(rel.to_owned(), entry.clone());
        Ok(Some(entry))
    }

    /// M0 to M8 (§5.10) for the dirty paths, one write group for each book and one for the
    /// files of no book: after each group the folder is a valid Scripture Burrito (I7), and a
    /// crash leaves at most one group half done (§6.4). Writes nothing while a value of the
    /// fold cannot be read on this computer (§2.6), and removes nothing while an expected
    /// pack is missing or a journal file unreadable (§4.4).
    pub(crate) fn materialize(&mut self) -> Result<MaterializeReport, StoreError> {
        let mut report = MaterializeReport::default();
        if self.dirty.is_empty() {
            return Ok(report);
        }
        let unreadable = self.fold.unreadable_records().len();
        if unreadable > 0 {
            self.block_values(unreadable);
            return Ok(report);
        }
        self.unblock_values();
        self.shared
            .hooks
            .wait(crate::hooks::Point::BeforeMaterialize);
        self.release_missing_holds();
        let progress = self.dirty.progress;
        let groups = self.plan_groups();
        let show = groups.len() > 1 && (progress || self.opening);
        let tracker = show.then(|| {
            let shared = self.shared.clone();
            let sink: crate::progress::ProgressSink = std::sync::Arc::new(move |progress| {
                shared.emit(crate::handle::StoreEvent::Progress {
                    work: crate::handle::Work::Materialize,
                    progress,
                });
            });
            let (tracker, reporter) = crate::progress::start(sink, Instant::now());
            tracker.phase(crate::progress::Phase::Working, groups.len() as u64, 0);
            (tracker, reporter)
        });
        self.reasons.clear();
        let mut processed: BTreeSet<String> = BTreeSet::new();
        let mut failed = None;
        for (i, group) in groups.iter().enumerate() {
            if let Err(e) = self.run_group(group, &mut report, &mut processed) {
                // What was not done stays dirty, and is tried again.
                for left in &groups[i..] {
                    self.restore_group(left);
                }
                self.dirty.metadata = true;
                failed = Some(e);
                break;
            }
            if let Some((tracker, _)) = &tracker {
                tracker.add(1, 0);
            }
        }
        if let Some((tracker, reporter)) = tracker {
            reporter.finish(&tracker);
        }
        if failed.is_none()
            && let Some(left) = self.open_intent.clone()
        {
            // The intent a stopped write left is finished: every group is done, but for the
            // paths that are still refused, whose bytes may be the ones it names.
            let refused: BTreeSet<&str> = report.blocked.iter().map(String::as_str).collect();
            self.settle_intent(left.remaining(|path| !refused.contains(path)))?;
        }
        self.retry_blocked(&report, &processed);
        if failed.is_none() {
            // A job that could not write told the person once (R16); the one that wrote
            // what was left takes the notice away.
            self.shared.remove_notices_matching(|n| {
                matches!(
                    n,
                    Notice::MaterializeBlocked {
                        reason: Blocked::Failed { .. },
                        ..
                    }
                )
            });
        }
        self.shared.emit(crate::handle::StoreEvent::Materialized {
            paths: report.written.clone(),
        });
        match failed {
            Some(e) => Err(e),
            None => Ok(report),
        }
    }

    /// The write groups of the dirty marks, the books in order and then the files of no book.
    /// Taking the marks off `self.dirty` leaves it empty; what a group cannot do is put back.
    fn plan_groups(&mut self) -> Vec<Group> {
        let dirty = std::mem::take(&mut self.dirty);
        let mut books: BTreeMap<BookId, Group> = BTreeMap::new();
        let mut project = Group::project();
        if dirty.everything {
            // The one place the whole listing is asked for: every file may have changed.
            for file in self.fold.derived_files() {
                match file.book() {
                    Some(book) => {
                        books.entry(book).or_insert_with(|| Group::book(book)).whole = true
                    }
                    None => {
                        project.files.insert(file);
                    }
                }
            }
            // Books the manifest holds files of and the projection no longer has.
            for path in self.manifest.paths.keys() {
                if let Some(book) = DerivedFile::from_path(path).and_then(|f| f.book()) {
                    books.entry(book).or_insert_with(|| Group::book(book)).whole = true;
                }
            }
            project.whole = true;
            project.carried = true;
        }
        for book in dirty.books {
            books.entry(book).or_insert_with(|| Group::book(book)).whole = true;
        }
        for file in dirty.files {
            match file.book() {
                Some(book) => {
                    books
                        .entry(book)
                        .or_insert_with(|| Group::book(book))
                        .files
                        .insert(file);
                }
                None => {
                    project.files.insert(file);
                }
            }
        }
        if dirty.comments_all {
            for book in self.fold.projected_books() {
                books
                    .entry(book)
                    .or_insert_with(|| Group::book(book))
                    .files
                    .insert(DerivedFile::Comments(book));
            }
            project.files.insert(DerivedFile::StoryComments);
            project.files.insert(DerivedFile::Conflicts);
        }
        project.carried |= dirty.carried;
        let mut groups: Vec<Group> = books.into_values().collect();
        let needed = project.whole
            || project.carried
            || !project.files.is_empty()
            || (groups.is_empty() && dirty.metadata);
        if needed {
            groups.push(project);
        }
        groups
    }

    /// The files of `group` the projection holds now (M0).
    fn wanted(&self, group: &Group) -> Vec<DerivedFile> {
        match group.book {
            Some(book) => {
                let projected = self.fold.book_files(book);
                if group.whole {
                    projected
                } else {
                    projected
                        .into_iter()
                        .filter(|f| group.files.contains(f))
                        .collect()
                }
            }
            None if group.whole => group.files.iter().cloned().collect(),
            None => group
                .files
                .iter()
                .filter(|f| self.fold.projects_file(f))
                .cloned()
                .collect(),
        }
    }

    /// The paths of `group` to remove (X, M0): files the app wrote that the projection no
    /// longer holds, and carried files a command removed (§5.16). A held path stays until the
    /// person chooses. Nothing is removed while an expected pack is missing or a journal file
    /// unreadable (§4.4); the person is told what waits.
    fn removals(&mut self, group: &Group, wanted: &[DerivedFile]) -> Vec<String> {
        let keep: BTreeSet<String> = wanted.iter().map(DerivedFile::path).collect();
        let mut candidates: Vec<String> = Vec::new();
        let ours = |s: &Self, path: &String| {
            s.manifest
                .paths
                .get(path)
                .is_some_and(|e| e.oid.is_none() && e.held.is_none())
        };
        match group.book {
            Some(book) => {
                let named: Vec<String> = if group.whole {
                    book_paths(book)
                } else {
                    group.files.iter().map(DerivedFile::path).collect()
                };
                for path in named {
                    if !keep.contains(&path) && ours(self, &path) {
                        candidates.push(path);
                    }
                }
            }
            None => {
                if group.whole {
                    // Every manifest entry that is no book's file, no fixed file and not
                    // projected: a story that went, a file of a kind this format dropped.
                    for (path, entry) in &self.manifest.paths {
                        let derived = DerivedFile::from_path(path);
                        let ours_path = path != METADATA
                            && !FIXED.contains(&path.as_str())
                            && entry.oid.is_none()
                            && entry.held.is_none()
                            && derived.as_ref().is_none_or(|f| f.book().is_none())
                            && !keep.contains(path);
                        if ours_path {
                            candidates.push(path.clone());
                        }
                    }
                } else {
                    for file in &group.files {
                        let path = file.path();
                        if !keep.contains(&path) && ours(self, &path) {
                            candidates.push(path);
                        }
                    }
                }
                if group.carried {
                    // A carried file a command removed, and one the project no longer places
                    // at that path (the copy of a version that was kept apart or moved):
                    // the app's own file goes, after the compare-and-swap (§11.4).
                    let placed: BTreeSet<String> = self
                        .fold
                        .carried_files()
                        .into_iter()
                        .map(|p| p.path)
                        .collect();
                    for (path, entry) in &self.manifest.paths {
                        if entry.oid.is_some()
                            && entry.held.is_none()
                            && (self.carried_removed(path) || !placed.contains(path))
                        {
                            candidates.push(path.clone());
                        }
                    }
                }
            }
        }
        if !self.complete && !candidates.is_empty() {
            self.withhold_removals(&candidates);
            return Vec::new();
        }
        candidates
    }

    /// A look at the folder or a retry wrote `path` from a fold newer than this job's. Only a
    /// checkpoint's job runs on an older fold than one the writer has run on (it stands on the
    /// snapshot its packs hold, §5.13 C1): it leaves the path as it is, since the newer bytes
    /// are the app's own and hold saves the older projection lacks; the commit's tree takes the
    /// path from the snapshot ([`Writer::stale_paths`]) and the next job, whose marks include
    /// every save served meanwhile, finds the file equal to the projection.
    pub(crate) fn written_from_newer_fold(&self, path: &str) -> bool {
        self.written_seq
            .get(path)
            .is_some_and(|written| *written > self.seq)
    }

    /// The sequence number of the newest snapshot, which holds every batch the actor has
    /// answered for: the fold a version the writer accepts now stands for.
    pub(crate) fn newest_seq(&self) -> u64 {
        self.shared.snapshot().seq().max(self.seq)
    }

    /// M0 to M8 for one group.
    fn run_group(
        &mut self,
        group: &Group,
        report: &mut MaterializeReport,
        processed: &mut BTreeSet<String>,
    ) -> Result<(), StoreError> {
        let wanted = self.wanted(group);
        let removes = self.removals(group, &wanted);
        let rendered = self.render(group.book, &wanted);
        let intent_now = self.open_intent.clone().unwrap_or_default();
        // M1: compare and swap.
        let mut writes: Vec<(String, Rendered)> = Vec::new();
        // What each path looked like when M1 read it, which M4 checks again at the rename.
        let mut seen: BTreeMap<String, Option<FileMeta>> = BTreeMap::new();
        // Paths whose bytes are the new ones: read (`converged`), or taken from a clean
        // entry without a read (`unread`). Only their entries change.
        let mut converged: BTreeMap<String, Converged> = BTreeMap::new();
        let mut unread: Vec<DerivedFile> = Vec::new();
        for (path, r) in rendered {
            processed.insert(path.clone());
            if self.pending_record.contains_key(&path) {
                // The actor is recording what was found there: nothing is written meanwhile.
                continue;
            }
            if self.written_from_newer_fold(&path) {
                continue;
            }
            let entry = self.manifest.paths.get(&path).cloned();
            if entry.as_ref().is_some_and(|e| e.held.is_some()) {
                // Held: left as it is until the person chooses (§6.2). A deletion ended the
                // hold in M0.
                continue;
            }
            // The bytes are what the app wrote and the file is as it left it: nothing to
            // read, nothing to write.
            if let Some(e) = &entry
                && e.md5 == r.md5.to_hex()
                && self.clean_by_stat(&path)
                && !self.heads_lag(&path)
            {
                unread.push(r.file);
                continue;
            }
            let stat = self.vfs.metadata(&self.folder.path(&path)).ok();
            let disk = match self.read_disk(&path) {
                Disk::Unreadable => {
                    self.reasons.insert(path.clone(), Blocked::FileUnreadable);
                    report.blocked.push(path);
                    continue;
                }
                Disk::Missing => None,
                Disk::Bytes(disk) => Some(disk),
            };
            self.counters
                .add(&self.counters.derived_read, u64::from(disk.is_some()));
            match disk {
                None => {
                    seen.insert(path.clone(), None);
                    writes.push((path, r));
                }
                Some(disk) => {
                    let d = self.hash(&disk);
                    let new = r.md5.to_hex();
                    let ours = entry.as_ref().map(|e| e.md5.as_str());
                    // The open could not read this path (§6.2): what it asks of every other path
                    // is asked here, once.
                    let unclassified = self.unclassified.remove(&path);
                    if d == new {
                        let nfc = is_text(&r.file).then(|| is_nfc(&String::from_utf8_lossy(&disk)));
                        converged.insert(
                            path,
                            Converged {
                                file: r.file,
                                md5: d,
                                nfc: nfc.unwrap_or(true),
                                digests: r.digests,
                            },
                        );
                    } else if ours == Some(d.as_str())
                        || intent_now.md5s_of(&path).contains(&d.as_str())
                    {
                        // The app's bytes may be the only copy of changes a journal file lost.
                        if self.manifest.ahead.contains(&path)
                            || (unclassified && self.shows_lost_changes(&path))
                        {
                            self.found_outside(&path, &disk, false)?;
                            report.recorded.push(path);
                        } else {
                            seen.insert(path.clone(), stat);
                            writes.push((path, r));
                        }
                    } else if self.clean_by_stat(&path) {
                        // Changed although its size and time did not: storage damage, held.
                        self.hold(&path, &disk, true)?;
                        report.held.push(path);
                    } else {
                        self.found_outside(&path, &disk, false)?;
                        report.recorded.push(path);
                    }
                }
            }
        }
        let mut gone: Vec<String> = Vec::new();
        let mut removing: Vec<String> = Vec::new();
        for path in removes {
            processed.insert(path.clone());
            if self.written_from_newer_fold(&path) {
                continue;
            }
            match self.read_disk(&path) {
                Disk::Missing => gone.push(path),
                Disk::Unreadable => {
                    self.reasons.insert(path.clone(), Blocked::FileUnreadable);
                    report.blocked.push(path);
                }
                Disk::Bytes(disk) => {
                    let d = self.hash(&disk);
                    let ours = self.manifest.paths.get(&path).map(|e| e.md5.as_str());
                    if ours == Some(d.as_str()) || intent_now.md5s_of(&path).contains(&d.as_str()) {
                        removing.push(path);
                    } else if self.clean_by_stat(&path) {
                        self.hold(&path, &disk, true)?;
                        report.held.push(path);
                    } else {
                        self.found_outside(&path, &disk, false)?;
                        report.recorded.push(path);
                    }
                }
            }
        }
        // Carried files are placed before the listing is built, so it names them (§11.4).
        if group.carried {
            self.place_carried(report)?;
        }
        // M2: metadata.json for the planned result.
        let planned: BTreeMap<String, (Md5, u64, Option<bool>)> = writes
            .iter()
            .map(|(p, r)| {
                let nfc = is_text(&r.file).then(|| is_nfc(&String::from_utf8_lossy(&r.bytes)));
                (p.clone(), (r.md5, r.bytes.len() as u64, nfc))
            })
            .collect();
        // A Bible project with no book yet (a create before its first book) has no valid
        // `metadata.json`; nothing is written until it has one (M2). Any other failure is a
        // bug, reported, and nothing is written either.
        let departing: Vec<String> = removing.iter().chain(&gone).cloned().collect();
        let metadata = match self.build_metadata(&planned, &departing) {
            Ok(metadata) => metadata,
            Err(_) if self.kind == ProjectKind::Bible && self.fold.projected_books().is_empty() => {
                // The paths stay dirty until a book is there.
                self.restore_group(group);
                self.dirty.metadata = true;
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        let metadata_md5 = self.hash(&metadata);
        let metadata_current = self.metadata_is_current(&metadata_md5);
        if writes.is_empty() && removing.is_empty() && metadata_current {
            // Nothing to write: only entries (scope digests, settled marks) may change.
            let mut done: BTreeSet<String> = converged.keys().cloned().collect();
            done.extend(unread.iter().map(DerivedFile::path));
            done.extend(gone.iter().cloned());
            done.insert(METADATA.to_owned());
            self.finish_entries(Vec::new(), converged, unread, &gone, &[], None)?;
            if self.open_intent.is_some() {
                let left = intent_now.remaining(|path| !done.contains(path));
                self.settle_intent(left)?;
            }
            return Ok(());
        }
        // M3: the intent.
        let mut intent = Intent::default();
        for (path, r) in &writes {
            intent.writes.push(IntentWrite {
                path: path.clone(),
                old_md5: self.manifest.paths.get(path).map(|e| e.md5.clone()),
                new_md5: r.md5.to_hex(),
            });
        }
        intent.writes.push(IntentWrite {
            path: METADATA.to_owned(),
            old_md5: self.manifest.paths.get(METADATA).map(|e| e.md5.clone()),
            new_md5: metadata_md5.clone(),
        });
        intent.removes = removing.clone();
        // Whatever an earlier write left (a stopped group, an open's intent) stays named: its
        // bytes may be on disk until a group has written the path (§5.10).
        let named = intent_now.merged(&intent);
        let tc = self.folder.path(TC);
        atomic_create_dirs(&*self.vfs, &tc)?;
        atomic_write(
            &*self.vfs,
            &tc,
            "materialize.intent.json",
            &named.to_bytes(),
            SyncDir::Now,
        )
        .map_err(|e| e.into_inner())?;
        self.open_intent = Some(named);
        // M4: the files and their heads files, each folder flushed once (M5).
        let heads_dir = self.folder.path(HEADS);
        atomic_create_dirs(&*self.vfs, &heads_dir)?;
        let mut dirs = DirSet::new();
        let files: Vec<DerivedFile> = writes.iter().map(|(_, r)| r.file.clone()).collect();
        let digests = self.fold.scope_digests(&files);
        let mut written: Vec<(String, DerivedFile, Md5, u64, Option<bool>, String)> = Vec::new();
        for ((path, r), digest) in writes.into_iter().zip(digests) {
            let (dir, name) = self.folder.dir_and_name(&path);
            atomic_create_dirs(&*self.vfs, &dir)?;
            let mut outside: Option<Vec<u8>> = None;
            let wrote = self.write_once(
                &dir,
                name,
                &r.bytes,
                r.md5.to_hex(),
                &mut dirs,
                &path,
                seen.get(&path).copied().flatten(),
                &intent_now,
                &mut outside,
            )?;
            match wrote {
                Wrote::Done => {}
                Wrote::Busy => {
                    self.reasons.insert(path.clone(), Blocked::FileBusy);
                    report.blocked.push(path);
                    continue;
                }
                Wrote::Refused => {
                    // The file changed after M1 read it: bytes the app did not write are an
                    // outside edit, held with a copy (I8); a file that cannot be read now
                    // waits.
                    match outside {
                        Some(disk) => {
                            self.found_outside(&path, &disk, false)?;
                            report.recorded.push(path);
                        }
                        None => {
                            self.reasons.insert(path.clone(), Blocked::FileUnreadable);
                            report.blocked.push(path);
                        }
                    }
                    continue;
                }
            }
            let nfc = is_text(&r.file).then(|| is_nfc(&String::from_utf8_lossy(&r.bytes)));
            self.note_renamed(&path, r.md5.to_hex(), r.bytes.len() as u64, nfc);
            self.written_seq.insert(path.clone(), self.seq);
            self.write_heads(&path, &r.file, &r.digests, SyncDir::Deferred(&mut dirs))?;
            self.counters.add(&self.counters.derived_written, 1);
            written.push((
                path.clone(),
                r.file,
                r.md5,
                r.bytes.len() as u64,
                nfc,
                digest.to_hex(),
            ));
            report.written.push(path);
        }
        for path in &removing {
            atomic_remove(
                &*self.vfs,
                &self.folder.path(path),
                SyncDir::Deferred(&mut dirs),
            )?;
            self.heads_unwritten.remove(path);
            self.manifest_changed |= self.manifest.heads_due.remove(path);
            let heads = self.folder.path(&layout::heads_file(path));
            if self.vfs.metadata(&heads).is_ok() {
                atomic_remove(&*self.vfs, &heads, SyncDir::Deferred(&mut dirs))?;
            }
            report.removed.push(path.clone());
        }
        dirs.sync(&*self.vfs)?;
        // M6: metadata.json from the files as they now are (a path skipped in M4 keeps its
        // old bytes).
        let skipped = written.len() != planned.len();
        let (metadata, metadata_md5) = if skipped {
            let planned: BTreeMap<String, (Md5, u64, Option<bool>)> = written
                .iter()
                .map(|(p, _, m, s, n, _)| (p.clone(), (*m, *s, *n)))
                .collect();
            let metadata = self.build_metadata(&planned, &departing)?;
            // The bytes written are not the ones M2 checked, and the entry names these.
            let md5 = self.hash(&metadata);
            (metadata, md5)
        } else {
            (metadata, metadata_md5)
        };
        self.keep_outside_metadata(&intent_now, &metadata)?;
        let metadata_written = match atomic_write(
            &*self.vfs,
            self.folder.root(),
            METADATA,
            &metadata,
            SyncDir::Now,
        ) {
            Ok(()) => true,
            Err(crate::atomic::WriteError::NotWritten(e))
                if e.io_error().is_some_and(|io| self.vfs.is_held(io)) =>
            {
                self.reasons.insert(METADATA.to_owned(), Blocked::FileBusy);
                report.blocked.push(METADATA.to_owned());
                false
            }
            Err(e) => return Err(e.into_inner()),
        };
        if metadata_written {
            self.note_renamed(METADATA, metadata_md5.clone(), metadata.len() as u64, None);
            self.metadata_heads_due = true;
            self.write_metadata_heads(&heads_dir)?;
            self.wrote_group = true;
        }
        // M7: the manifest.
        let metadata_entry = metadata_written.then_some((metadata, metadata_md5));
        let mut done: BTreeSet<String> = written.iter().map(|w| w.0.clone()).collect();
        done.extend(converged.keys().cloned());
        done.extend(unread.iter().map(DerivedFile::path));
        done.extend(removing.iter().chain(&gone).cloned());
        if metadata_entry.is_some() {
            done.insert(METADATA.to_owned());
        }
        self.finish_entries(written, converged, unread, &gone, &removing, metadata_entry)?;
        // M8: what this group named is settled; what an earlier write left unsettled stays.
        self.settle_intent(intent_now.remaining(|path| !done.contains(path)))?;
        Ok(())
    }

    /// M4: `rel` was renamed into place, so the bytes there are the app's `md5`, whatever the
    /// group does next. The entry names them at once (unsettled, with no scope and no time,
    /// which M7 fills), and the session's intent names only them: the bytes before are no
    /// longer the app's own, so a program that puts them back has made an edit (§11.1 X3),
    /// also when the group stops before M7 (a full disk at the heads file). The manifest file
    /// is not written here; an open that finds the older one finds the intent beside it.
    fn note_renamed(&mut self, rel: &str, md5: String, size: u64, nfc: Option<bool>) {
        if let Some(intent) = &mut self.open_intent {
            intent.narrow_to(rel, &md5);
        }
        self.manifest_changed |= self.manifest.ahead.remove(rel);
        // No time yet: an unsettled entry is never clean by stat, so the next look reads the
        // file, and M7 stats it once for every path.
        self.manifest.paths.insert(
            rel.to_owned(),
            Entry {
                nfc,
                ..Entry::of(md5, size, 0, false)
            },
        );
    }

    /// The heads file of a derived file written (or found equal) from the fold in use: for
    /// every record the file was built from, the heads its bytes stand for and how it shows
    /// the record (§3.6). Its directory exists.
    pub(crate) fn write_heads(
        &mut self,
        rel: &str,
        file: &DerivedFile,
        digests: &[(RecordKey, tc_journal::outside::ShownDigest)],
        sync: SyncDir<'_>,
    ) -> Result<(), StoreError> {
        let heads = HeadsFile::of(&self.fold.file_heads(file), digests);
        self.put_heads(rel, heads, sync)
    }

    /// Writes the heads file of `rel`. When that fails the bytes the path now holds still stand
    /// for `heads`, so the writer keeps them ([`Writer::heads_of`]) until a write goes through:
    /// an edit of those bytes made in the meantime is compared with them (§11.1 D7). The
    /// directory of the heads files exists.
    pub(crate) fn put_heads(
        &mut self,
        rel: &str,
        heads: HeadsFile,
        sync: SyncDir<'_>,
    ) -> Result<(), StoreError> {
        let name = layout::heads_file(rel);
        let name = name.rsplit('/').next().unwrap_or_default();
        let written = atomic_write(
            &*self.vfs,
            &self.folder.path(HEADS),
            name,
            &heads.to_bytes(),
            sync,
        )
        .map_err(|e| e.into_inner());
        match written {
            Ok(()) => {
                self.heads_unwritten.remove(rel);
                self.manifest_changed |= self.manifest.heads_due.remove(rel);
                Ok(())
            }
            Err(e) => {
                self.heads_unwritten.insert(rel.to_owned(), heads);
                self.manifest_changed |= self.manifest.heads_due.insert(rel.to_owned());
                Err(e)
            }
        }
    }

    /// The heads file of `rel` describes older bytes than the path holds: a write could not
    /// put it on disk, in this session or before a restart.
    pub(crate) fn heads_lag(&self, rel: &str) -> bool {
        self.heads_unwritten.contains_key(rel) || self.manifest.heads_due.contains(rel)
    }

    /// The heads file of `rel` as it should be: the one a write could not put on disk, else the
    /// file in the folder. `None` when there is none or it does not read.
    pub(crate) fn heads_of(&self, rel: &str) -> Option<HeadsFile> {
        if let Some(heads) = self.heads_unwritten.get(rel) {
            return Some(heads.clone());
        }
        let bytes = self
            .vfs
            .read(&self.folder.path(&layout::heads_file(rel)))
            .ok()?;
        HeadsFile::parse(&bytes)
    }

    /// M8: the intent file names `left`, or goes when nothing is left. The md5s of a path
    /// whose write stopped, or that another program still refuses, stay the app's own until
    /// a group has written it, so the next try does not take the app's bytes for an outside
    /// edit (§6.2).
    pub(crate) fn settle_intent(&mut self, left: Intent) -> Result<(), StoreError> {
        let path = self.folder.path(INTENT);
        if left.is_empty() {
            self.open_intent = None;
            return match atomic_remove(&*self.vfs, &path, SyncDir::Now) {
                Err(e) if e.io_error().is_some_and(is_not_found) => Ok(()),
                other => other,
            };
        }
        atomic_write(
            &*self.vfs,
            &self.folder.path(TC),
            "materialize.intent.json",
            &left.to_bytes(),
            SyncDir::Now,
        )
        .map_err(|e| e.into_inner())?;
        self.open_intent = Some(left);
        Ok(())
    }

    /// The heads file of `metadata.json`: the records its project details were written from,
    /// which an outside edit of the file is compared with (§11.1). It follows the file, as
    /// every heads file does, so a lost one makes a later edit conflict rather than replace.
    fn write_metadata_heads(&mut self, heads_dir: &std::path::Path) -> Result<(), StoreError> {
        let bytes = HeadsFile::of(&self.fold.metadata_heads(), &[]).to_bytes();
        if self.metadata_heads_written.as_deref() == Some(bytes.as_slice()) {
            self.metadata_heads_due = false;
            return Ok(());
        }
        let name = layout::heads_file(METADATA);
        let name = name.rsplit('/').next().unwrap_or_default();
        atomic_write(&*self.vfs, heads_dir, name, &bytes, SyncDir::Now)
            .map_err(|e| e.into_inner())?;
        self.metadata_heads_written = Some(bytes);
        self.metadata_heads_due = false;
        Ok(())
    }

    /// M7: the entries of what was written, found equal or removed, and the manifest.
    #[allow(clippy::too_many_arguments)]
    fn finish_entries(
        &mut self,
        written: Vec<(String, DerivedFile, Md5, u64, Option<bool>, String)>,
        converged: BTreeMap<String, Converged>,
        unread: Vec<DerivedFile>,
        gone: &[String],
        removed: &[String],
        metadata: Option<(Vec<u8>, String)>,
    ) -> Result<(), StoreError> {
        let settle_by = self.stamp_ns;
        let mut files: Vec<DerivedFile> = Vec::new();
        files.extend(converged.values().map(|c| c.file.clone()));
        files.extend(unread.iter().cloned());
        let digests = self.fold.scope_digests(&files);
        for (path, _, md5, size, nfc, scope) in written {
            let meta = self.vfs.metadata(&self.folder.path(&path)).ok();
            self.written_seq.insert(path.clone(), self.seq);
            self.manifest.paths.insert(
                path,
                Entry {
                    scope: Some(scope),
                    nfc,
                    ..Entry::of(md5.to_hex(), size, meta.map_or(0, |m| m.mtime_ns), false)
                },
            );
            self.manifest_changed = true;
        }
        let mut digests = digests.into_iter();
        for (path, found) in converged {
            let Converged {
                file,
                md5,
                nfc,
                digests: shown,
            } = found;
            let digest = digests.next().map(|d| d.to_hex());
            self.written_seq.insert(path.clone(), self.seq);
            // The file holds the projection, but the heads file describes the bytes the entry
            // names: after a write that stopped between its rename and its heads file, or a
            // projection that changed with the same bytes, it is stale, and a later edit
            // would be compared with a base the file was never made from (§11.1 D7).
            let stale = self.heads_lag(&path)
                || self
                    .manifest
                    .paths
                    .get(&path)
                    .is_none_or(|e| e.md5 != md5 || e.scope != digest);
            if stale {
                let dir = self.folder.path(HEADS);
                atomic_create_dirs(&*self.vfs, &dir)?;
                self.write_heads(&path, &file, &shown, SyncDir::Now)?;
            }
            let meta = self.vfs.metadata(&self.folder.path(&path)).ok();
            let size = meta.map_or(0, |m| m.len);
            let mtime = meta.map_or(0, |m| m.mtime_ns);
            let settled = !self.network && settle_by >= mtime.saturating_add(2_000_000_000);
            self.manifest.paths.insert(
                path,
                Entry {
                    scope: digest,
                    nfc: Some(nfc),
                    ..Entry::of(md5, size, mtime, settled)
                },
            );
            self.manifest_changed = true;
        }
        for file in unread {
            let digest = digests.next().map(|d| d.to_hex());
            self.written_seq.insert(file.path(), self.seq);
            if let Some(entry) = self.manifest.paths.get_mut(&file.path())
                && entry.scope != digest
            {
                entry.scope = digest;
                self.manifest_changed = true;
            }
        }
        for path in removed.iter().chain(gone) {
            if self.manifest.paths.remove(path).is_some() {
                self.manifest_changed = true;
            }
        }
        if let Some((metadata, md5)) = metadata {
            let meta = self.vfs.metadata(&self.folder.path(METADATA)).ok();
            let scope = self.metadata_scope();
            self.manifest.paths.insert(
                METADATA.to_owned(),
                Entry {
                    scope: Some(scope),
                    ..Entry::of(
                        md5,
                        metadata.len() as u64,
                        meta.map_or(0, |m| m.mtime_ns),
                        false,
                    )
                },
            );
            self.manifest_changed = true;
        }
        if self.manifest_changed {
            self.write_manifest()?;
        }
        Ok(())
    }

    /// Whether `metadata.json` on disk is what these bytes would write: its entry holds the
    /// same md5, and the file is as the app left it.
    fn metadata_is_current(&self, md5: &str) -> bool {
        // The file was renamed but its heads file is still due: the next group goes on from M3.
        if self.metadata_heads_due {
            return false;
        }
        let same_entry = self
            .manifest
            .paths
            .get(METADATA)
            .is_some_and(|e| e.md5 == md5 && e.held.is_none());
        if !same_entry {
            return false;
        }
        if self.clean_by_stat(METADATA) {
            return true;
        }
        // A file written in the tick of its own time stamp is not settled: its bytes are read.
        match self.read_disk(METADATA) {
            Disk::Bytes(bytes) => {
                self.counters.add(&self.counters.derived_read, 1);
                md5_hex(&bytes) == md5
            }
            _ => false,
        }
    }

    /// The md5 of `bytes`, counted.
    pub(crate) fn hash(&self, bytes: &[u8]) -> String {
        self.counters
            .add(&self.counters.bytes_hashed, bytes.len() as u64);
        md5_hex(bytes)
    }

    /// Marks again what a group could not do.
    fn restore_group(&mut self, group: &Group) {
        match group.book {
            Some(book) if group.whole => {
                self.dirty.books.insert(book);
            }
            Some(_) | None => self.dirty.files.extend(group.files.iter().cloned()),
        }
        if group.whole && group.book.is_none() {
            self.dirty.everything = true;
        }
        self.dirty.carried |= group.carried;
    }

    /// `metadata.json` must list the files as they are, so it is always rewritten; bytes
    /// the app did not write there (a project detail edited in a text editor, or any bytes
    /// when no manifest says what the app wrote) are copied to `.tc/external/` first, with
    /// a notice, so nothing a person wrote is lost (I8).
    fn keep_outside_metadata(&mut self, intent: &Intent, new: &[u8]) -> Result<(), StoreError> {
        let Disk::Bytes(disk) = self.read_disk(METADATA) else {
            return Ok(());
        };
        let d = md5_hex(&disk);
        if d == md5_hex(new) {
            return Ok(());
        }
        let ours = self
            .manifest
            .paths
            .get(METADATA)
            .is_some_and(|e| e.md5 == d)
            || intent.md5s_of(METADATA).contains(&d.as_str());
        // The app's own bytes are stale, unless they show project details no journal file
        // holds any more: then they are the only copy of them (§6.2). The open left the
        // question of its heads file to the first write that reads the file.
        let unclassified = self.unclassified.remove(METADATA);
        let lost = self.manifest.ahead.contains(METADATA)
            || (ours && unclassified && self.shows_lost_changes(METADATA));
        if ours && !lost {
            return Ok(());
        }
        self.keep_edited_metadata(&disk).map(|_| ())
    }

    /// Bytes of `metadata.json` the app did not write: a copy, a notice, and the fields that
    /// are records sent to be recorded (§11.1 X2); the file is written over right after, and
    /// again from the records once the batch is durable. A retry of a blocked write finds the
    /// same outside bytes: one copy of them. `false` when these bytes were seen already.
    pub(crate) fn keep_edited_metadata(&mut self, disk: &[u8]) -> Result<bool, StoreError> {
        if !self.copied_outside.insert(md5_hex(disk)) {
            return Ok(false);
        }
        let copy = self.copy_external(METADATA, disk)?;
        self.notice(
            Notice::MetadataEditedOutside {
                copy: copy.as_str().into(),
            },
            Some(copy.clone()),
        );
        self.record_metadata_edit(disk, copy);
        self.dirty.metadata = true;
        Ok(true)
    }

    /// The digest `metadata.json`'s entry keeps: its records' and its ingredient table's.
    pub(crate) fn metadata_scope(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        hash.update(self.fold.metadata_scope_digest().0);
        for (path, entry) in &self.manifest.paths {
            if path == METADATA {
                continue;
            }
            hash.update(path.as_bytes());
            hash.update([0]);
            hash.update(
                entry
                    .held
                    .as_ref()
                    .map_or(&entry.md5, |h| &h.md5)
                    .as_bytes(),
            );
            hash.update(entry.size.to_le_bytes());
        }
        // The packs as this session knows them: a pack sealed or received since the last
        // write of `metadata.json` changes the digest.
        for (path, pack) in &self.packs {
            hash.update(path.as_bytes());
            hash.update([0]);
            hash.update(pack.size.to_le_bytes());
            hash.update(pack.fingerprint.to_le_bytes());
        }
        crate::random::to_hex(&hash.finalize())
    }

    /// Writes one derived file. [`Wrote::Busy`] when a program holds the file and the rename
    /// was refused (a sharing violation, Windows): the writer does not wait for it. The path
    /// stays dirty, and the next attempt starts at M0 and so repeats the compare-and-swap
    /// (§5.10). Just before the rename the file is looked at again (I8): when it is not as M1
    /// found it and its bytes are none the app wrote, nothing is renamed over it
    /// ([`Wrote::Refused`], the bytes in `outside`).
    #[allow(clippy::too_many_arguments)]
    fn write_once(
        &self,
        dir: &std::path::Path,
        name: &str,
        bytes: &[u8],
        new_md5: String,
        dirs: &mut DirSet,
        rel: &str,
        seen: Option<FileMeta>,
        intent: &Intent,
        outside: &mut Option<Vec<u8>>,
    ) -> Result<Wrote, StoreError> {
        let abs = self.folder.path(rel);
        let ours: Vec<String> = self
            .manifest
            .paths
            .get(rel)
            .map(|e| e.md5.clone())
            .into_iter()
            .chain(intent.md5s_of(rel).into_iter().map(str::to_owned))
            .chain([new_md5])
            .collect();
        let mut look = || -> Result<bool, StoreError> {
            self.shared.hooks.wait(crate::hooks::Point::BeforeRename);
            let now = self.vfs.metadata(&abs).ok();
            let unchanged = match (&seen, &now) {
                (None, None) => true,
                (Some(a), Some(b)) => a.len == b.len && a.mtime_ns == b.mtime_ns,
                _ => false,
            };
            if unchanged {
                return Ok(true);
            }
            match self.vfs.read(&abs) {
                Ok(disk) => {
                    if ours.contains(&md5_hex(&disk)) {
                        Ok(true)
                    } else {
                        *outside = Some(disk);
                        Ok(false)
                    }
                }
                // Gone meanwhile: a deletion is never read as an edit (§6.2).
                Err(e) if is_not_found(&e) => Ok(true),
                Err(_) => Ok(false),
            }
        };
        // Three short retries: the file is tried again by the next run, and one held file
        // must not stall the other files of its book.
        match atomic_write_checked(
            &*self.vfs,
            dir,
            name,
            bytes,
            SyncDir::Deferred(dirs),
            &HeldRetry::BACKGROUND,
            &mut look,
        ) {
            Ok(Written::Renamed) => Ok(Wrote::Done),
            Ok(Written::Refused) => Ok(Wrote::Refused),
            Err(crate::atomic::WriteError::NotWritten(e))
                if e.io_error().is_some_and(|io| self.vfs.is_held(io)) =>
            {
                Ok(Wrote::Busy)
            }
            Err(e) => Err(e.into_inner()),
        }
    }

    /// What the paths this job could not write need next: they stay dirty and are tried again,
    /// and the person is told once a path has been refused for 2 s ("translationCore can't
    /// update TIT.usfm because another program has it open. Your work is saved.", R16). A
    /// path that was written, found equal or removed this time is no longer busy, and its
    /// notice goes.
    fn retry_blocked(&mut self, report: &MaterializeReport, processed: &BTreeSet<String>) {
        let now = Instant::now();
        let cleared: Vec<String> = self
            .busy
            .keys()
            .filter(|p| processed.contains(*p) && !report.blocked.contains(*p))
            .cloned()
            .collect();
        for path in cleared {
            self.busy.remove(&path);
            if let Ok(repo) = RepoPath::parse(&path) {
                self.shared.remove_blocked_notice(&repo);
            }
        }
        let mut to_tell: Vec<(String, Blocked)> = Vec::new();
        for path in &report.blocked {
            let reason = self.reasons.get(path).cloned().unwrap_or(Blocked::FileBusy);
            let state = self.busy.entry(path.clone()).or_insert(Busy {
                since: now,
                told: false,
            });
            if !state.told && now.duration_since(state.since) >= BUSY_NOTICE_AFTER {
                state.told = true;
                to_tell.push((path.clone(), reason));
            }
            if path == METADATA {
                self.dirty.metadata = true;
            } else if let Some(file) = DerivedFile::from_path(path) {
                self.dirty.add_file(file);
            } else {
                self.dirty.carried = true;
            }
        }
        for (path, reason) in to_tell {
            if let Ok(path) = RepoPath::parse(&path) {
                self.notice(
                    Notice::MaterializeBlocked {
                        paths: vec![path],
                        reason,
                    },
                    None,
                );
            }
        }
        if !report.blocked.is_empty() {
            self.dirty.metadata = true;
        }
    }

    /// How long until what a job could not write is tried again: 200 ms, then as long as the
    /// oldest path has waited, up to 3 s; 3 s for paths that wait for something else (values
    /// not yet readable, a job that failed).
    pub(crate) fn retry_delay(&self) -> Option<Duration> {
        if self.dirty.is_empty() {
            return None;
        }
        match self.busy.values().map(|b| b.since).min() {
            Some(oldest) => Some(oldest.elapsed().clamp(BUSY_RETRY_FIRST, BUSY_RETRY_LAST)),
            None => Some(BUSY_RETRY_LAST),
        }
    }

    /// Nothing is written while values of the fold cannot be read (§2.6): the person is told
    /// how many wait, once for each count, and the paths stay dirty.
    fn block_values(&mut self, records: usize) {
        let records = u32::try_from(records).unwrap_or(u32::MAX);
        if self.values_told == Some(records) {
            return;
        }
        self.unblock_values();
        self.values_told = Some(records);
        self.notice(
            Notice::MaterializeBlocked {
                paths: Vec::new(),
                reason: Blocked::ValuesUnreadable { records },
            },
            None,
        );
    }

    fn unblock_values(&mut self) {
        if self.values_told.take().is_some() {
            self.shared.remove_notices_matching(|n| {
                matches!(
                    n,
                    Notice::MaterializeBlocked {
                        reason: Blocked::ValuesUnreadable { .. },
                        ..
                    }
                )
            });
        }
    }

    /// Removals wait for an expected pack (§4.4): the person is told which paths, once.
    fn withhold_removals(&mut self, candidates: &[String]) {
        let paths: Vec<RepoPath> = candidates
            .iter()
            .filter_map(|p| RepoPath::parse(p).ok())
            .collect();
        if paths.is_empty()
            || self
                .shared
                .has_blocked_notice(&paths, &Blocked::PackMissing)
        {
            return;
        }
        self.notice(
            Notice::MaterializeBlocked {
                paths,
                reason: Blocked::PackMissing,
            },
            None,
        );
    }

    /// Whether a command removed the carried file at `rel` and no other version of it
    /// stands beside the removal (§5.16): a conflict between a removal and an edit removes
    /// nothing.
    fn carried_removed(&self, rel: &str) -> bool {
        let Ok(path) = RepoPath::parse(rel) else {
            return false;
        };
        let key = RecordKey::CarriedFile(path);
        match self.fold.key_state(&key) {
            Some(state) if !matches!(state.state, Resolved::Conflict(_)) => matches!(
                self.fold.projected(&key),
                Some(Record::CarriedFile {
                    value: CarriedValue::Removed,
                    ..
                })
            ),
            _ => false,
        }
    }

    /// Renders the wanted files of a group: a book's files together (§14: a book's records are
    /// decoded once for its files), a file of no book on its own. Each buffer is hashed once
    /// here, and the md5 stands for it from then on. A book's USFM comes with the digest of
    /// each record it shows (the heads file keeps them, §11.1 X3).
    fn render(
        &mut self,
        book: Option<BookId>,
        wanted: &[DerivedFile],
    ) -> BTreeMap<String, Rendered> {
        let mut out = BTreeMap::new();
        if self.prerendered_seq != self.seq {
            self.prerendered.clear();
        }
        let mut left: Vec<&DerivedFile> = Vec::new();
        for file in wanted {
            match self.prerendered.remove(&file.path()) {
                Some(projected) => {
                    let rendered = self.rendered(file.clone(), projected.bytes, projected.digests);
                    out.insert(file.path(), rendered);
                }
                None => left.push(file),
            }
        }
        let whole_book = book.is_some()
            && (left.iter().any(|f| matches!(f, DerivedFile::Usfm(_))) || left.len() >= 3);
        if let (true, Some(book)) = (whole_book, book) {
            for projected in self.fold.render_book_with_digests(book) {
                if left.contains(&&projected.file) {
                    self.counters.add(&self.counters.projected, 1);
                    let rendered =
                        self.rendered(projected.file.clone(), projected.bytes, projected.digests);
                    out.insert(projected.file.path(), rendered);
                }
            }
        } else {
            for file in left {
                if let Some(projected) = self.fold.render_with_digests(file) {
                    self.counters.add(&self.counters.projected, 1);
                    let rendered = self.rendered(file.clone(), projected.bytes, projected.digests);
                    out.insert(file.path(), rendered);
                }
            }
        }
        out
    }

    fn rendered(
        &self,
        file: DerivedFile,
        bytes: Vec<u8>,
        digests: Vec<(RecordKey, tc_journal::outside::ShownDigest)>,
    ) -> Rendered {
        self.counters
            .add(&self.counters.bytes_hashed, bytes.len() as u64);
        if !digests.is_empty() {
            self.counters
                .add(&self.counters.digest_bytes, bytes.len() as u64);
        }
        Rendered {
            file,
            md5: Md5::of(&bytes),
            bytes,
            digests,
        }
    }

    /// The fold's `currentScope`, from which a book file's scope is cut.
    pub(crate) fn scope_probe(&self) -> serde_json::Value {
        self.fold
            .metadata_source_nfc((u64::from(FORMAT_MAJOR), u64::from(FORMAT_MINOR)), true)
            .scope
    }

    /// `metadata.json` for the files as they will be once `planned` are written and
    /// `removed` gone (M2, M6): the manifest's entries, which stand for every file the app
    /// wrote or holds, with the planned bytes in place of the old, and the fixed files, the
    /// packs and the carried files. Every value passed `check_field` already (§3.4); a
    /// document that fails is a bug, and nothing is written. It costs the entries, never a
    /// projection of the project.
    pub(crate) fn build_metadata(
        &mut self,
        planned: &BTreeMap<String, (Md5, u64, Option<bool>)>,
        removed: &[String],
    ) -> Result<Vec<u8>, StoreError> {
        let source_probe = self.scope_probe();
        let book_ranges = |book: BookId| book_ranges(&source_probe, book);
        let removed: BTreeSet<&str> = removed.iter().map(String::as_str).collect();
        let mut entries: Vec<IngredientEntry> = Vec::new();
        let mut nfc = true;
        let push = |entries: &mut Vec<IngredientEntry>,
                    path: &str,
                    md5: Md5,
                    size: u64,
                    role: Option<&str>,
                    scope: Option<serde_json::Value>| {
            if let Ok(path) = IngredientPath::parse(path) {
                entries.push(IngredientEntry {
                    path,
                    md5: tc_burrito::Md5::parse(&md5.to_hex())
                        .unwrap_or_else(|| tc_burrito::Md5::of(b"")),
                    size,
                    role: role.map(str::to_owned),
                    scope,
                });
            }
        };
        let mut paths: BTreeSet<String> = self.manifest.paths.keys().cloned().collect();
        paths.extend(planned.keys().cloned());
        let mut listed: BTreeSet<String> = BTreeSet::new();
        for path in paths {
            if removed.contains(path.as_str())
                || path == METADATA
                || FIXED.contains(&path.as_str())
                || self.packs.contains_key(&path)
            {
                continue;
            }
            let entry = self.manifest.paths.get(&path).cloned();
            if entry
                .as_ref()
                .is_some_and(|e| e.oid.is_some() && e.held.is_none())
            {
                // Carried files are listed with the fold's placements below.
                continue;
            }
            let derived = DerivedFile::from_path(&path);
            if derived.is_none() && entry.as_ref().is_none_or(|e| e.held.is_none()) {
                // A path the format does not derive is listed only while it is held: it is
                // in the folder, so the list says so (I7).
                continue;
            }
            let (md5, size, text_nfc) = match planned.get(&path) {
                Some(p) => *p,
                // Bytes found outside the app wait in the folder for the actor to record
                // them; the list says what is there (I7).
                None if self.pending_record.contains_key(&path) => {
                    let (md5, bytes) = &self.pending_record[&path];
                    // A carried file's bytes are never kept in memory: its size is the one
                    // it was stored with (§11.4).
                    let size = self
                        .pending_carried
                        .get(&path)
                        .map_or(bytes.len() as u64, |(_, size)| *size);
                    (
                        *md5,
                        size,
                        derived
                            .as_ref()
                            .filter(|f| is_text(f))
                            .map(|_| is_nfc(&String::from_utf8_lossy(bytes))),
                    )
                }
                None => match entry {
                    Some(e) if e.held.is_some() => match self.listing_of_file(&path, None)? {
                        Some((md5, size)) => (
                            md5,
                            size,
                            self.manifest.paths.get(&path).and_then(|e| e.nfc),
                        ),
                        None => continue,
                    },
                    Some(e) => match Md5::parse(&e.md5) {
                        Some(md5) => (md5, e.size, e.nfc),
                        None => continue,
                    },
                    None => match self.read_disk(&path) {
                        Disk::Bytes(bytes) => (
                            Md5::of(&bytes),
                            bytes.len() as u64,
                            derived
                                .as_ref()
                                .filter(|f| is_text(f))
                                .map(|_| is_nfc(&String::from_utf8_lossy(&bytes))),
                        ),
                        _ => continue,
                    },
                },
            };
            let (role, scope) = match &derived {
                Some(file) => {
                    if is_text(file) {
                        nfc &= text_nfc.unwrap_or(false);
                    }
                    let ranges = file.book().map(book_ranges).unwrap_or_default();
                    (file.role(), file.scope(&ranges))
                }
                None => (None, None),
            };
            push(&mut entries, &path, md5, size, role, scope);
            listed.insert(path);
        }
        for fixed in FIXED {
            if let Some((md5, size)) = self.listing_of_file(fixed, None)? {
                push(&mut entries, fixed, md5, size, Some("x-tc-project"), None);
            }
        }
        let packs: Vec<String> = self.packs.keys().cloned().collect();
        for rel in packs {
            if let Some(entry) = self.pack_entry(&rel)?
                && let Some(md5) = Md5::parse(&entry.md5)
            {
                push(
                    &mut entries,
                    &rel,
                    md5,
                    entry.size,
                    Some("x-tc-changes"),
                    None,
                );
            }
        }
        // Carried files, as they are on disk (the writer places their bytes, §11.4). Every
        // path the app placed or recorded is listed until the job of the carried files removes
        // its file: one the fold no longer places (a removed file, a version moved apart) is
        // still in the folder until then. The fold's placements give the blob of the rest,
        // but a path a newer fold placed or recorded (seen by a checkpoint's job on an older
        // fold) keeps the blob its entry names.
        let mut carried: BTreeMap<String, Option<String>> = self
            .manifest
            .paths
            .iter()
            .filter(|(_, e)| e.oid.is_some() && e.held.is_none())
            .map(|(path, e)| (path.clone(), e.oid.clone()))
            .collect();
        for p in self.fold.carried_files() {
            if !self.written_from_newer_fold(&p.path) {
                carried.insert(p.path, Some(p.oid.to_hex()));
            }
        }
        for (path, oid) in carried {
            if listed.contains(&path)
                || !path.starts_with("ingredients/")
                || removed.contains(path.as_str())
            {
                continue;
            }
            if let Some((md5, size)) = self.listing_of_file(&path, oid)? {
                push(&mut entries, &path, md5, size, None, None);
            }
        }
        self.assemble_metadata(&entries, nfc)
    }

    /// `metadata.json` for the entries it lists: the project's details, books and pins from
    /// the fold, and `entries` as the ingredients. `nfc`: every text file listed is in NFC.
    pub(crate) fn assemble_metadata(
        &self,
        entries: &[IngredientEntry],
        nfc: bool,
    ) -> Result<Vec<u8>, StoreError> {
        let mut source = self
            .fold
            .metadata_source_nfc((u64::from(FORMAT_MAJOR), u64::from(FORMAT_MINOR)), nfc);
        match self.kind {
            ProjectKind::Stories => source.scope = obs_scope(),
            // Between the groups of a job a book the fold holds may have no file yet: the
            // scope lists the books the ingredients cover, so the folder is a valid burrito
            // after each group (I7). Once every group is done, they are all there.
            ProjectKind::Bible => {
                if let serde_json::Value::Object(scope) = &mut source.scope {
                    let covered: BTreeSet<&str> = entries
                        .iter()
                        .filter_map(|e| e.scope.as_ref()?.as_object())
                        .flat_map(|o| o.keys().map(String::as_str))
                        .collect();
                    scope.retain(|book, _| covered.contains(book.as_str()));
                }
            }
        }
        tc_burrito::build_metadata(&source, entries).map_err(|problems| {
            StoreError::Other(format!(
                "metadata.json would be invalid, so nothing was written (a bug): {problems:?}"
            ))
        })
    }
}

/// The ranges of the book's scope in `scope` (the fold's `currentScope`).
pub(crate) fn book_ranges(scope: &serde_json::Value, book: BookId) -> Vec<String> {
    scope
        .get(book.code())
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|r| r.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// What a group writes: one book's files, or the files of no book.
#[derive(Debug)]
struct Group {
    /// The book, or `None` for the files of no book.
    book: Option<BookId>,
    /// Every file of the book is reconciled: written when it differs, removed when the
    /// projection no longer holds it. For the group of no book: `files` is the whole
    /// projection of those files, and the manifest is searched for the rest.
    whole: bool,
    files: BTreeSet<DerivedFile>,
    /// Carried files are placed and removed with this group (§11.4).
    carried: bool,
}

impl Group {
    fn book(book: BookId) -> Self {
        Self {
            book: Some(book),
            whole: false,
            files: BTreeSet::new(),
            carried: false,
        }
    }

    fn project() -> Self {
        Self {
            book: None,
            whole: false,
            files: BTreeSet::new(),
            carried: false,
        }
    }
}

/// A derived file rendered, with its md5 taken once.
struct Rendered {
    file: DerivedFile,
    bytes: Vec<u8>,
    md5: Md5,
    /// How the file shows each record, for a book's USFM (§11.1 X3).
    digests: Vec<(RecordKey, tc_journal::outside::ShownDigest)>,
}

/// A file projected before the writer's job: the bytes and the digests of its records.
#[derive(Debug, Default)]
pub(crate) struct Projected {
    pub bytes: Vec<u8>,
    pub digests: Vec<(RecordKey, tc_journal::outside::ShownDigest)>,
}

/// The paths a book can have files at.
fn book_paths(book: BookId) -> Vec<String> {
    let mut files = vec![
        DerivedFile::Usfm(book),
        DerivedFile::Alignments(book),
        DerivedFile::Comments(book),
    ];
    for tool in tc_journal::ToolId::ALL {
        files.push(DerivedFile::Decisions(tool, book));
    }
    files.iter().map(DerivedFile::path).collect()
}
