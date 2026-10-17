//! Materialize derived files (`docs/STORAGE.md` §5.10, M0 to M8): write the derived files
//! whose projection changed and `metadata.json`, with an intent file, a manifest and a
//! compare-and-swap, so the app never writes over a file it did not write last (I8).
//!
//! Unit S1 holds a derived file that changed outside the app: a copy goes to `.tc/external/`,
//! the file stays as it is, and the person is told. Recording such an edit as changes
//! (§11.1) is unit S2's; until then nothing is written over it and nothing is recorded.
//! Carried files are listed in `metadata.json` as they are on disk; writing their bytes from
//! the object database is unit S2's too.

use crate::appdata::utc_stamp;
use crate::atomic::{DirSet, SyncDir, atomic_create_dirs, atomic_remove, atomic_write};
use crate::error::StoreError;
use crate::layout::{self, EXTERNAL, HEADS, INTENT, MANIFEST, METADATA, TC};
use crate::manifest::{Entry, HeadsFile, Intent, IntentWrite, PackEntry};
use crate::project::Project;
use crate::vfs::is_not_found;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tc_burrito::{IngredientEntry, IngredientPath};
use tc_journal::fold::{DerivedFile, Notice, ProjectKind};
use tc_journal::{FORMAT_MAJOR, FORMAT_MINOR, Md5, RepoPath};
use unicode_normalization::is_nfc;

/// How long a rename onto a file another program holds open is retried (§5.10).
pub(crate) const BUSY_RETRY: Duration = Duration::from_secs(2);

/// What one materialization did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MaterializeReport {
    pub written: Vec<String>,
    pub removed: Vec<String>,
    /// Paths held: changed outside the app, left as they are (§6.2).
    pub held: Vec<String>,
    /// Paths another program kept open for 2 s: still dirty.
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

/// `currentScope` of an OBS project: the template's table verbatim (§3.2).
pub(crate) fn obs_scope() -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(tc_obs::template::template_metadata())
        .ok()
        .and_then(|v| v.pointer("/type/flavorType/currentScope").cloned())
        .unwrap_or(serde_json::Value::Null)
}

impl Project {
    fn read_rel(&self, rel: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let path = self.folder.path(rel);
        match self.vfs.read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if is_not_found(&e) => Ok(None),
            Err(e) => Err(StoreError::io("read", &path, e)),
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

    /// Holds `rel` (§6.2): a copy of its bytes in `.tc/external/`, the file left as it is,
    /// the manifest entry marked held, and a notice.
    pub(crate) fn hold(&mut self, rel: &str, bytes: &[u8], damage: bool) -> Result<(), StoreError> {
        let copy_name = format!("{}-{}", utc_stamp(self.clock.now()), layout::escaped(rel));
        let dir = self.folder.path(EXTERNAL);
        atomic_create_dirs(&*self.vfs, &dir)?;
        atomic_write(&*self.vfs, &dir, &copy_name, bytes, SyncDir::Now)
            .map_err(|e| e.into_inner())?;
        let held = md5_hex(bytes);
        let meta = self.vfs.metadata(&self.folder.path(rel)).ok();
        let entry = self
            .manifest
            .paths
            .entry(rel.to_owned())
            .or_insert_with(|| Entry {
                md5: String::new(),
                size: 0,
                mtime_ns: 0,
                settled: false,
                scope: None,
                held: None,
                oid: None,
                nfc: None,
            });
        entry.held = Some(held);
        if let Some(meta) = meta {
            entry.size = meta.len;
            entry.mtime_ns = meta.mtime_ns;
        }
        self.write_manifest()?;
        let copy = format!("{EXTERNAL}/{copy_name}");
        if let Ok(path) = RepoPath::parse(rel) {
            let notice = if damage {
                Notice::StorageDamage {
                    path,
                    copy: copy.as_str().into(),
                }
            } else {
                Notice::OutsideEditHeld {
                    path,
                    copy: copy.as_str().into(),
                }
            };
            self.notice(notice, Some(copy));
        }
        Ok(())
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
        let Some(bytes) = self.read_rel(rel)? else {
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
        let intent_now = self
            .read_rel(INTENT)?
            .and_then(|b| Intent::parse(&b))
            .unwrap_or_default();
        // M0: what the projection holds now, and what to write and remove.
        let projection: BTreeMap<String, DerivedFile> = self
            .fold
            .derived_files()
            .into_iter()
            .map(|f| (f.path(), f))
            .collect();
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
                let derived = path != METADATA
                    && !FIXED.contains(&path.as_str())
                    && entry.oid.is_none()
                    && !projection.contains_key(path);
                if derived && entry.held.is_none() {
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
                continue;
            }
            let new = md5_hex(&bytes);
            let disk = self.read_rel(&path)?;
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
            match self.read_rel(&path)? {
                None => gone.push(path),
                Some(disk) => {
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
        // A project with no book yet (a create before its first book) has no valid
        // `metadata.json`; nothing is written until it has one (M2).
        let Ok(metadata) = self.build_metadata(&projection, &planned, &report.removed) else {
            return Ok(report);
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
            if !self.write_retrying(&path, &dir, name, &bytes, &mut dirs)? {
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
        atomic_write(
            &*self.vfs,
            self.folder.root(),
            METADATA,
            &metadata,
            SyncDir::Now,
        )
        .map_err(|e| e.into_inner())?;
        // M7: the manifest.
        let settle_by = self.stamp_ns;
        for (path, _, md5, size, nfc, scope) in written_entries {
            let meta = self.vfs.metadata(&self.folder.path(&path)).ok();
            self.manifest.paths.insert(
                path,
                Entry {
                    md5: md5.to_hex(),
                    size,
                    mtime_ns: meta.map_or(0, |m| m.mtime_ns),
                    settled: false,
                    scope: Some(scope),
                    held: None,
                    oid: None,
                    nfc,
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
                    md5,
                    size,
                    mtime_ns: mtime,
                    settled,
                    scope: Some(digest.to_hex()),
                    held: None,
                    oid: None,
                    nfc: Some(nfc),
                },
            );
        }
        for path in report.removed.iter().chain(&gone) {
            self.manifest.paths.remove(path);
        }
        let meta = self.vfs.metadata(&self.folder.path(METADATA)).ok();
        let scope = self.metadata_scope();
        self.manifest.paths.insert(
            METADATA.to_owned(),
            Entry {
                md5: md5_hex(&metadata),
                size: metadata.len() as u64,
                mtime_ns: meta.map_or(0, |m| m.mtime_ns),
                settled: false,
                scope: Some(scope),
                held: None,
                oid: None,
                nfc: None,
            },
        );
        self.write_manifest()?;
        // M8.
        atomic_remove(&*self.vfs, &self.folder.path(INTENT), SyncDir::Now)?;
        self.dirty = Default::default();
        if !report.blocked.is_empty() {
            for path in &report.blocked {
                if let Some(file) = projection.get(path) {
                    self.dirty.files.insert(file.clone());
                }
                if let Ok(path) = RepoPath::parse(path) {
                    self.notice(Notice::WriteBlocked { path }, None);
                }
            }
            self.dirty.metadata = true;
        }
        self.shared.emit(crate::handle::StoreEvent::Materialized {
            paths: report.written.clone(),
        });
        Ok(report)
    }

    /// `metadata.json` must list the files as they are, so it is always rewritten; bytes
    /// the app did not write there (a project detail edited in a text editor) are copied to
    /// `.tc/external/` first, with a notice, so nothing a person wrote is lost (I8).
    fn keep_outside_metadata(&mut self, intent: &Intent, new: &[u8]) -> Result<(), StoreError> {
        let Some(disk) = self.read_rel(METADATA)? else {
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
        if ours || !self.manifest.paths.contains_key(METADATA) {
            return Ok(());
        }
        let copy_name = format!(
            "{}-{}",
            utc_stamp(self.clock.now()),
            layout::escaped(METADATA)
        );
        let dir = self.folder.path(EXTERNAL);
        atomic_create_dirs(&*self.vfs, &dir)?;
        atomic_write(&*self.vfs, &dir, &copy_name, &disk, SyncDir::Now)
            .map_err(|e| e.into_inner())?;
        let copy = format!("{EXTERNAL}/{copy_name}");
        if let Ok(path) = RepoPath::parse(METADATA) {
            self.notice(
                Notice::OutsideEditHeld {
                    path,
                    copy: copy.as_str().into(),
                },
                Some(copy),
            );
        }
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
            hash.update(entry.held.as_deref().unwrap_or(&entry.md5).as_bytes());
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

    /// Writes one derived file; a rename another program blocks is retried for 2 s, with
    /// the compare-and-swap before each retry (§5.10). `false` when it stayed blocked or the
    /// file changed meanwhile (then it is held).
    fn write_retrying(
        &mut self,
        path: &str,
        dir: &std::path::Path,
        name: &str,
        bytes: &[u8],
        dirs: &mut DirSet,
    ) -> Result<bool, StoreError> {
        let started = Instant::now();
        let new = md5_hex(bytes);
        loop {
            match atomic_write(&*self.vfs, dir, name, bytes, SyncDir::Deferred(dirs)) {
                Ok(()) => return Ok(true),
                Err(crate::atomic::WriteError::NotWritten(e))
                    if e.kind() == Some(std::io::ErrorKind::PermissionDenied) =>
                {
                    if started.elapsed() >= BUSY_RETRY {
                        return Ok(false);
                    }
                    std::thread::sleep(Duration::from_millis(100));
                    if let Some(disk) = self.read_rel(path)? {
                        let d = md5_hex(&disk);
                        let ours = self.manifest.paths.get(path).map(|e| e.md5.clone());
                        if ours.as_deref() != Some(d.as_str()) && d != new {
                            self.hold(path, &disk, false)?;
                            return Ok(false);
                        }
                    }
                }
                Err(e) => return Err(e.into_inner()),
            }
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
                    Some(entry) => {
                        let md5 = entry
                            .held
                            .as_deref()
                            .and_then(Md5::parse)
                            .or_else(|| Md5::parse(&entry.md5));
                        let size = match &entry.held {
                            Some(_) => self
                                .vfs
                                .metadata(&self.folder.path(path))
                                .map_or(entry.size, |m| m.len),
                            None => entry.size,
                        };
                        match md5 {
                            Some(md5) => (md5, size, entry.nfc),
                            None => continue,
                        }
                    }
                    None => match self.read_rel(path)? {
                        Some(bytes) => (
                            Md5::of(&bytes),
                            bytes.len() as u64,
                            is_text(file).then(|| is_nfc(&String::from_utf8_lossy(&bytes))),
                        ),
                        None => continue,
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
        for fixed in FIXED {
            let entry = match self.manifest.paths.get(fixed) {
                Some(e) => (Md5::parse(&e.md5), e.size),
                None => match self.read_rel(fixed)? {
                    Some(bytes) => {
                        let md5 = Md5::of(&bytes);
                        self.manifest.paths.insert(
                            fixed.to_owned(),
                            Entry {
                                md5: md5.to_hex(),
                                size: bytes.len() as u64,
                                mtime_ns: 0,
                                settled: false,
                                scope: None,
                                held: None,
                                oid: None,
                                nfc: None,
                            },
                        );
                        (Some(md5), bytes.len() as u64)
                    }
                    None => (None, 0),
                },
            };
            if let (Some(md5), size) = entry {
                push(&mut entries, fixed, md5, size, Some("x-tc-project"), None);
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
            if projection.contains_key(&placement.path) {
                continue;
            }
            let entry = match self.manifest.paths.get(&placement.path) {
                Some(e) if e.oid.is_some() => Md5::parse(&e.md5).map(|m| (m, e.size)),
                _ => match self.read_rel(&placement.path)? {
                    Some(bytes) => {
                        let md5 = Md5::of(&bytes);
                        self.manifest.paths.insert(
                            placement.path.clone(),
                            Entry {
                                md5: md5.to_hex(),
                                size: bytes.len() as u64,
                                mtime_ns: 0,
                                settled: false,
                                scope: None,
                                held: None,
                                oid: Some(placement.oid.to_hex()),
                                nfc: None,
                            },
                        );
                        Some((md5, bytes.len() as u64))
                    }
                    None => None,
                },
            };
            if let Some((md5, size)) = entry
                && placement.path.starts_with("ingredients/")
            {
                push(&mut entries, &placement.path, md5, size, None, None);
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
