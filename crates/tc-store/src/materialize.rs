//! Materialize derived files (`docs/STORAGE.md` §5.10, M0 to M8): write the derived files
//! whose projection changed and `metadata.json`, with an intent file, a manifest and a
//! compare-and-swap, so the app never writes over a file it did not write last (I8).
//!
//! Unit S1 holds a derived file that changed outside the app: a copy goes to `.tc/external/`,
//! the file stays as it is, and the person is told. Recording such an edit as changes
//! (§11.1) is unit S2's; until then nothing is written over it and nothing is recorded.
//! Carried files and the fixed files are listed in `metadata.json` as their bytes are on disk
//! (I7), read or verified by stat as a derived file is; writing a carried file's bytes from
//! the object database is unit S2's too.
//!
//! The actor never waits for a file another program holds: a path whose rename is refused
//! stays dirty, and the next attempt starts at M0 again after a delay that grows to 3 s
//! (§5.10).

use crate::appdata::utc_stamp;
use crate::atomic::{DirSet, SyncDir, atomic_create_dirs, atomic_remove, atomic_write};
use crate::error::StoreError;
use crate::layout::{self, EXTERNAL, HEADS, INTENT, MANIFEST, METADATA, TC};
use crate::manifest::{Entry, HeadsFile, Held, HeldCause, Intent, IntentWrite, PackEntry};
use crate::project::Project;
use crate::vfs::is_not_found;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tc_burrito::{IngredientEntry, IngredientPath};
use tc_journal::fold::{DerivedFile, Notice, ProjectKind, Resolved};
use tc_journal::record::CarriedValue;
use tc_journal::{FORMAT_MAJOR, FORMAT_MINOR, Md5, Record, RecordKey, RepoPath};
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

/// What one materialization did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MaterializeReport {
    pub written: Vec<String>,
    pub removed: Vec<String>,
    /// Paths held: changed outside the app, left as they are (§6.2).
    pub held: Vec<String>,
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

impl Project {
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
        match self.read_disk(rel) {
            Disk::Missing => Ok(None),
            Disk::Unreadable => Ok(entry.as_ref().and_then(listed)),
            Disk::Bytes(bytes) => {
                let md5 = Md5::of(&bytes);
                let size = bytes.len() as u64;
                if entry.as_ref().is_some_and(|e| e.md5 != md5.to_hex()) {
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
                        self.hold(rel, &bytes, false)?;
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
    fn release_missing_holds(&mut self, projection: &BTreeMap<String, DerivedFile>) {
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
            if let Some(file) = projection.get(&rel) {
                self.dirty.files.insert(file.clone());
            }
            self.dirty.metadata = true;
        }
    }

    /// The facts `metadata.json` lists for a pack: from the manifest when its size and
    /// fingerprint are the ones recorded, else read once.
    fn pack_entry(&mut self, rel: &str) -> Result<Option<PackEntry>, StoreError> {
        let Some(info) = self.journal.packs.get(rel).cloned() else {
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
        };
        self.manifest.packs.insert(rel.to_owned(), entry.clone());
        Ok(Some(entry))
    }

    /// M0 to M8 (§5.10) for the dirty paths. Writes nothing while a value of the fold cannot
    /// be read on this computer (§2.6), and removes nothing while an expected pack is
    /// missing or a journal file unreadable (§4.4).
    pub(crate) fn materialize(&mut self) -> Result<MaterializeReport, StoreError> {
        let mut report = MaterializeReport::default();
        if self.dirty.is_empty() {
            return Ok(report);
        }
        if !self.fold.unreadable_records().is_empty() {
            return Ok(report);
        }
        self.shared
            .hooks
            .wait(crate::hooks::Point::BeforeMaterialize);
        let intent_now = match self.read_disk(INTENT) {
            Disk::Bytes(b) => Intent::parse(&b).unwrap_or_default(),
            _ => Intent::default(),
        };
        // M0: what the projection holds now, and what to write and remove.
        let projection: BTreeMap<String, DerivedFile> = self
            .fold
            .derived_files()
            .into_iter()
            .map(|f| (f.path(), f))
            .collect();
        self.release_missing_holds(&projection);
        let dirty: BTreeSet<DerivedFile> = if self.dirty.everything {
            projection.values().cloned().collect()
        } else {
            self.dirty
                .files
                .iter()
                .filter(|f| projection.contains_key(&f.path()))
                .cloned()
                .collect()
        };
        let mut removes: Vec<String> = Vec::new();
        if self.journal.complete() {
            for (path, entry) in &self.manifest.paths {
                let ours = path != METADATA
                    && !FIXED.contains(&path.as_str())
                    && !projection.contains_key(path);
                // A derived file the projection no longer has, or a carried file a
                // command removed (§5.16). A carried file merely missing from the fold's
                // placements is not removed: only a settled `Removed` is a removal.
                let gone = match &entry.oid {
                    None => true,
                    Some(_) => self.carried_removed(path),
                };
                if ours && gone && entry.held.is_none() {
                    removes.push(path.clone());
                }
            }
        }
        let rendered = self.render(&dirty);
        // M1: compare and swap.
        let mut writes: Vec<(String, DerivedFile, Vec<u8>)> = Vec::new();
        let mut converged: BTreeMap<String, (DerivedFile, String, bool)> = BTreeMap::new();
        for (path, (file, bytes)) in rendered {
            if self
                .manifest
                .paths
                .get(&path)
                .is_some_and(|e| e.held.is_some())
            {
                // Held: left as it is until the person chooses (§6.2). A deletion ended
                // the hold in M0.
                continue;
            }
            let new = md5_hex(&bytes);
            let disk = match self.read_disk(&path) {
                Disk::Unreadable => {
                    report.blocked.push(path);
                    continue;
                }
                Disk::Missing => None,
                Disk::Bytes(disk) => Some(disk),
            };
            self.counters
                .add(&self.counters.derived_read, u64::from(disk.is_some()));
            match disk {
                None => writes.push((path, file, bytes)),
                Some(disk) => {
                    let d = md5_hex(&disk);
                    let ours = self.manifest.paths.get(&path).map(|e| e.md5.as_str());
                    if d == new {
                        let nfc = is_text(&file).then(|| is_nfc(&String::from_utf8_lossy(&disk)));
                        converged.insert(path, (file, d, nfc.unwrap_or(true)));
                    } else if ours == Some(d.as_str())
                        || intent_now.md5s_of(&path).contains(&d.as_str())
                    {
                        writes.push((path, file, bytes));
                    } else {
                        let damage = self.clean_by_stat(&path);
                        self.hold(&path, &disk, damage)?;
                        report.held.push(path);
                    }
                }
            }
        }
        let mut gone: Vec<String> = Vec::new();
        for path in removes {
            match self.read_disk(&path) {
                Disk::Missing => gone.push(path),
                Disk::Unreadable => report.blocked.push(path),
                Disk::Bytes(disk) => {
                    let d = md5_hex(&disk);
                    let ours = self.manifest.paths.get(&path).map(|e| e.md5.as_str());
                    if ours == Some(d.as_str()) || intent_now.md5s_of(&path).contains(&d.as_str()) {
                        report.removed.push(path);
                    } else {
                        let damage = self.clean_by_stat(&path);
                        self.hold(&path, &disk, damage)?;
                        report.held.push(path);
                    }
                }
            }
        }
        // M2: metadata.json for the planned result.
        let planned: BTreeMap<String, (Md5, u64, Option<bool>)> = writes
            .iter()
            .map(|(p, f, b)| {
                let nfc = is_text(f).then(|| is_nfc(&String::from_utf8_lossy(b)));
                (p.clone(), (Md5::of(b), b.len() as u64, nfc))
            })
            .collect();
        // A Bible project with no book yet (a create before its first book) has no valid
        // `metadata.json`; nothing is written until it has one (M2). Any other failure is a
        // bug, reported, and nothing is written either.
        let metadata = match self.build_metadata(&projection, &planned, &report.removed) {
            Ok(metadata) => metadata,
            Err(_) if self.kind == ProjectKind::Bible && self.fold.projected_books().is_empty() => {
                return Ok(report);
            }
            Err(e) => return Err(e),
        };
        // M3: the intent.
        let mut intent = Intent::default();
        for (path, _, bytes) in &writes {
            intent.writes.push(IntentWrite {
                path: path.clone(),
                old_md5: self.manifest.paths.get(path).map(|e| e.md5.clone()),
                new_md5: md5_hex(bytes),
            });
        }
        intent.writes.push(IntentWrite {
            path: METADATA.to_owned(),
            old_md5: self.manifest.paths.get(METADATA).map(|e| e.md5.clone()),
            new_md5: md5_hex(&metadata),
        });
        intent.removes = report.removed.clone();
        let tc = self.folder.path(TC);
        atomic_create_dirs(&*self.vfs, &tc)?;
        atomic_write(
            &*self.vfs,
            &tc,
            "materialize.intent.json",
            &intent.to_bytes(),
            SyncDir::Now,
        )
        .map_err(|e| e.into_inner())?;
        // M4: the files and their heads files, each folder flushed once (M5).
        let heads_dir = self.folder.path(HEADS);
        atomic_create_dirs(&*self.vfs, &heads_dir)?;
        let mut dirs = DirSet::new();
        let digests = self
            .fold
            .scope_digests(&writes.iter().map(|(_, f, _)| f.clone()).collect::<Vec<_>>());
        let mut written_entries: Vec<(String, DerivedFile, Md5, u64, Option<bool>, String)> =
            Vec::new();
        for ((path, file, bytes), digest) in writes.into_iter().zip(digests) {
            let (dir, name) = self.folder.dir_and_name(&path);
            atomic_create_dirs(&*self.vfs, &dir)?;
            if !self.write_once(&dir, name, &bytes, &mut dirs)? {
                report.blocked.push(path);
                continue;
            }
            let heads = HeadsFile::of(&self.fold.file_heads(&file));
            let heads_name = layout::heads_file(&path);
            let heads_name = heads_name.rsplit('/').next().unwrap_or_default();
            atomic_write(
                &*self.vfs,
                &heads_dir,
                heads_name,
                &heads.to_bytes(),
                SyncDir::Deferred(&mut dirs),
            )
            .map_err(|e| e.into_inner())?;
            let nfc = is_text(&file).then(|| is_nfc(&String::from_utf8_lossy(&bytes)));
            self.counters.add(&self.counters.derived_written, 1);
            written_entries.push((
                path.clone(),
                file,
                Md5::of(&bytes),
                bytes.len() as u64,
                nfc,
                digest.to_hex(),
            ));
            report.written.push(path);
        }
        for path in &report.removed {
            atomic_remove(
                &*self.vfs,
                &self.folder.path(path),
                SyncDir::Deferred(&mut dirs),
            )?;
            let heads = self.folder.path(&layout::heads_file(path));
            if self.vfs.metadata(&heads).is_ok() {
                atomic_remove(&*self.vfs, &heads, SyncDir::Deferred(&mut dirs))?;
            }
        }
        dirs.sync(&*self.vfs)?;
        // M6: metadata.json from the files as they now are.
        let metadata = if report.blocked.is_empty() {
            metadata
        } else {
            let planned: BTreeMap<String, (Md5, u64, Option<bool>)> = written_entries
                .iter()
                .map(|(p, _, m, s, n, _)| (p.clone(), (*m, *s, *n)))
                .collect();
            self.build_metadata(&projection, &planned, &report.removed)?
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
                if e.kind() == Some(std::io::ErrorKind::PermissionDenied) =>
            {
                report.blocked.push(METADATA.to_owned());
                false
            }
            Err(e) => return Err(e.into_inner()),
        };
        // M7: the manifest.
        let settle_by = self.stamp_ns;
        for (path, _, md5, size, nfc, scope) in written_entries {
            let meta = self.vfs.metadata(&self.folder.path(&path)).ok();
            self.manifest.paths.insert(
                path,
                Entry {
                    scope: Some(scope),
                    nfc,
                    ..Entry::of(md5.to_hex(), size, meta.map_or(0, |m| m.mtime_ns), false)
                },
            );
        }
        let converged_files: Vec<DerivedFile> =
            converged.values().map(|(f, _, _)| f.clone()).collect();
        let converged_digests = self.fold.scope_digests(&converged_files);
        for ((path, (_, md5, nfc)), digest) in converged.into_iter().zip(converged_digests) {
            let meta = self.vfs.metadata(&self.folder.path(&path)).ok();
            let size = meta.map_or(0, |m| m.len);
            let mtime = meta.map_or(0, |m| m.mtime_ns);
            let settled = !self.network && settle_by >= mtime.saturating_add(2_000_000_000);
            self.manifest.paths.insert(
                path,
                Entry {
                    scope: Some(digest.to_hex()),
                    nfc: Some(nfc),
                    ..Entry::of(md5, size, mtime, settled)
                },
            );
        }
        for path in report.removed.iter().chain(&gone) {
            self.manifest.paths.remove(path);
        }
        if metadata_written {
            let meta = self.vfs.metadata(&self.folder.path(METADATA)).ok();
            let scope = self.metadata_scope();
            self.manifest.paths.insert(
                METADATA.to_owned(),
                Entry {
                    scope: Some(scope),
                    ..Entry::of(
                        md5_hex(&metadata),
                        metadata.len() as u64,
                        meta.map_or(0, |m| m.mtime_ns),
                        false,
                    )
                },
            );
        }
        self.write_manifest()?;
        // M8.
        atomic_remove(&*self.vfs, &self.folder.path(INTENT), SyncDir::Now)?;
        self.dirty = Default::default();
        self.retry_blocked(&report, &projection);
        self.shared.emit(crate::handle::StoreEvent::Materialized {
            paths: report.written.clone(),
        });
        Ok(report)
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
        let ours = self
            .manifest
            .paths
            .get(METADATA)
            .is_some_and(|e| e.md5 == d)
            || intent.md5s_of(METADATA).contains(&d.as_str())
            || d == md5_hex(new);
        // A retry of a blocked write finds the same outside bytes: one copy of them.
        if ours || !self.copied_outside.insert(d) {
            return Ok(());
        }
        let copy = self.copy_external(METADATA, &disk)?;
        self.notice(
            Notice::MetadataEditedOutside {
                copy: copy.as_str().into(),
            },
            Some(copy),
        );
        Ok(())
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
        for (path, pack) in &self.journal.packs {
            hash.update(path.as_bytes());
            hash.update([0]);
            hash.update(pack.size.to_le_bytes());
            hash.update(pack.fingerprint.to_le_bytes());
        }
        crate::random::to_hex(&hash.finalize())
    }

    /// Writes one derived file. `false` when a program holds the file and the rename was
    /// refused (a sharing violation, Windows): the actor does not wait for it. The path
    /// stays dirty, and the next attempt starts at M0 and so repeats the compare-and-swap
    /// (§5.10).
    fn write_once(
        &self,
        dir: &std::path::Path,
        name: &str,
        bytes: &[u8],
        dirs: &mut DirSet,
    ) -> Result<bool, StoreError> {
        match atomic_write(&*self.vfs, dir, name, bytes, SyncDir::Deferred(dirs)) {
            Ok(()) => Ok(true),
            Err(crate::atomic::WriteError::NotWritten(e))
                if e.kind() == Some(std::io::ErrorKind::PermissionDenied) =>
            {
                Ok(false)
            }
            Err(e) => Err(e.into_inner()),
        }
    }

    /// What the paths this run could not write need next: they stay dirty, and the person
    /// is told once a path has been refused for 2 s ("translationCore can't update TIT.usfm
    /// because another program has it open. Your work is saved.").
    fn retry_blocked(
        &mut self,
        report: &MaterializeReport,
        projection: &BTreeMap<String, DerivedFile>,
    ) {
        let now = Instant::now();
        self.busy.retain(|path, _| report.blocked.contains(path));
        let mut to_tell: Vec<String> = Vec::new();
        for path in &report.blocked {
            let state = self.busy.entry(path.clone()).or_insert(Busy {
                since: now,
                told: false,
            });
            if !state.told && now.duration_since(state.since) >= BUSY_NOTICE_AFTER {
                state.told = true;
                to_tell.push(path.clone());
            }
            if let Some(file) = projection.get(path) {
                self.dirty.files.insert(file.clone());
            }
        }
        for path in to_tell {
            if let Ok(path) = RepoPath::parse(&path) {
                self.notice(Notice::WriteBlocked { path }, None);
            }
        }
        if !report.blocked.is_empty() {
            self.dirty.metadata = true;
        }
    }

    /// How long until the paths a run could not write are tried again: 200 ms, then as long
    /// as the oldest has waited, up to 3 s.
    pub(crate) fn retry_delay(&self) -> Option<Duration> {
        let oldest = self.busy.values().map(|b| b.since).min()?;
        Some(oldest.elapsed().clamp(BUSY_RETRY_FIRST, BUSY_RETRY_LAST))
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

    /// Renders the dirty files, a book's files together (§14: a book's records are decoded
    /// once for its files).
    fn render(
        &mut self,
        files: &BTreeSet<DerivedFile>,
    ) -> BTreeMap<String, (DerivedFile, Vec<u8>)> {
        let mut out = BTreeMap::new();
        let mut books: BTreeSet<tc_journal::BookId> = BTreeSet::new();
        let mut prerendered = std::mem::take(&mut self.prerendered);
        let mut left: BTreeSet<&DerivedFile> = BTreeSet::new();
        for file in files {
            match prerendered.remove(&file.path()) {
                Some(bytes) => {
                    out.insert(file.path(), (file.clone(), bytes));
                }
                None => {
                    left.insert(file);
                }
            }
        }
        for file in left.iter().copied() {
            match file.book() {
                Some(book) => {
                    books.insert(book);
                }
                None => {
                    if let Some(bytes) = self.fold.render(file) {
                        self.counters.projected.fetch_add(1, Ordering::Relaxed);
                        out.insert(file.path(), (file.clone(), bytes));
                    }
                }
            }
        }
        for book in books {
            for (file, bytes) in self.fold.render_book(book) {
                if left.contains(&file) {
                    self.counters.projected.fetch_add(1, Ordering::Relaxed);
                    out.insert(file.path(), (file, bytes));
                }
            }
        }
        out
    }

    /// `metadata.json` for the files as they will be once `planned` are written and
    /// `removed` gone (M2, M6). Every value passed `check_field` already (§3.4); a document
    /// that fails is a bug, and nothing is written.
    fn build_metadata(
        &mut self,
        projection: &BTreeMap<String, DerivedFile>,
        planned: &BTreeMap<String, (Md5, u64, Option<bool>)>,
        removed: &[String],
    ) -> Result<Vec<u8>, StoreError> {
        let source_probe = self
            .fold
            .metadata_source_nfc((u64::from(FORMAT_MAJOR), u64::from(FORMAT_MINOR)), true);
        let book_ranges = |book: tc_journal::BookId| -> Vec<String> {
            source_probe
                .scope
                .get(book.code())
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|r| r.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
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
        for (path, file) in projection {
            if removed.contains(path) {
                continue;
            }
            let (md5, size, text_nfc) = match planned.get(path) {
                Some(p) => *p,
                None => match self.manifest.paths.get(path).cloned() {
                    Some(entry) if entry.held.is_some() => {
                        match self.listing_of_file(path, None)? {
                            Some((md5, size)) => {
                                (md5, size, self.manifest.paths.get(path).and_then(|e| e.nfc))
                            }
                            None => continue,
                        }
                    }
                    Some(entry) => match Md5::parse(&entry.md5) {
                        Some(md5) => (md5, entry.size, entry.nfc),
                        None => continue,
                    },
                    None => match self.read_disk(path) {
                        Disk::Bytes(bytes) => (
                            Md5::of(&bytes),
                            bytes.len() as u64,
                            is_text(file).then(|| is_nfc(&String::from_utf8_lossy(&bytes))),
                        ),
                        _ => continue,
                    },
                },
            };
            if is_text(file) {
                nfc &= text_nfc.unwrap_or(false);
            }
            let ranges = file.book().map(book_ranges).unwrap_or_default();
            push(
                &mut entries,
                path,
                md5,
                size,
                file.role(),
                file.scope(&ranges),
            );
        }
        let mut listed: BTreeSet<String> = projection
            .keys()
            .filter(|p| !removed.contains(*p))
            .cloned()
            .collect();
        for fixed in FIXED {
            if let Some((md5, size)) = self.listing_of_file(fixed, None)? {
                push(&mut entries, fixed, md5, size, Some("x-tc-project"), None);
                listed.insert(fixed.to_owned());
            }
        }
        let packs: Vec<String> = self.journal.packs.keys().cloned().collect();
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
        // Carried files, as they are on disk (unit S2 writes their bytes).
        for placement in self.fold.carried_files() {
            if projection.contains_key(&placement.path)
                || !placement.path.starts_with("ingredients/")
            {
                continue;
            }
            if let Some((md5, size)) =
                self.listing_of_file(&placement.path, Some(placement.oid.to_hex()))?
            {
                push(&mut entries, &placement.path, md5, size, None, None);
                listed.insert(placement.path.clone());
            }
        }
        // A file held at a path the projection no longer has (a removed book edited
        // outside the app, a carried file removed after it was edited) is still in the
        // folder, so it is listed as it is.
        let leftovers: Vec<String> = self
            .manifest
            .paths
            .iter()
            .filter(|(p, e)| {
                e.held.is_some()
                    && p.starts_with("ingredients/")
                    && !listed.contains(*p)
                    && !self.journal.packs.contains_key(*p)
            })
            .map(|(p, _)| p.clone())
            .collect();
        for rel in leftovers {
            if let Some((md5, size)) = self.listing_of_file(&rel, None)? {
                push(&mut entries, &rel, md5, size, None, None);
            }
        }
        let mut source = self
            .fold
            .metadata_source_nfc((u64::from(FORMAT_MAJOR), u64::from(FORMAT_MINOR)), nfc);
        if self.kind == ProjectKind::Stories {
            source.scope = obs_scope();
        }
        tc_burrito::build_metadata(&source, &entries).map_err(|problems| {
            StoreError::Other(format!(
                "metadata.json would be invalid, so nothing was written (a bug): {problems:?}"
            ))
        })
    }
}
