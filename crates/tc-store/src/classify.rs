//! O9 (`docs/STORAGE.md` §6.2): the classification of every derived path against the
//! manifest, and O14's results. They run on the writer, which owns the manifest, with the
//! fold the open ends with (the open) or the newest snapshot (a check's results).

use crate::atomic::{SyncDir, atomic_remove};
use crate::error::StoreError;
use crate::layout::{INTENT, METADATA};
use crate::manifest::{Entry, Intent};
use crate::open::CheckResult;
use crate::progress::{Phase, Tracker};
use crate::vfs::is_not_found;
use crate::writer::Writer;
use std::collections::{BTreeMap, BTreeSet};
use tc_journal::fold::{DerivedFile, Notice};
use tc_journal::{Md5, RepoPath};

fn md5_hex(bytes: &[u8]) -> String {
    Md5::of(bytes).to_hex()
}

impl Writer {
    /// O9 (§6.2): classifies every derived path, reading and projecting only what may have
    /// changed, and marks what to write. Returns the paths taken as converged without being
    /// read, for the storage check (O14).
    pub(crate) fn classify(&mut self, tracker: &Tracker) -> Result<Vec<String>, StoreError> {
        let files = self.fold.derived_files();
        tracker.phase(Phase::Checking, files.len() as u64 + 1, 0);
        let digests = self.fold.scope_digests(&files);
        let intent = self
            .vfs
            .read(&self.folder.path(INTENT))
            .ok()
            .and_then(|b| Intent::parse(&b));
        let versions = self.manifest.versions_current();
        let mut unread: Vec<String> = Vec::new();
        let mut to_project: Vec<(String, DerivedFile, Option<Vec<u8>>)> = Vec::new();
        let projected: BTreeSet<String> = files.iter().map(DerivedFile::path).collect();
        for (file, digest) in files.iter().zip(&digests) {
            let rel = file.path();
            let named = intent.as_ref().is_some_and(|i| i.names(&rel));
            let entry = self.manifest.paths.get(&rel).cloned();
            let same_scope = versions
                && entry
                    .as_ref()
                    .is_some_and(|e| e.scope.as_deref() == Some(digest.to_hex().as_str()));
            if same_scope && !named && self.clean_by_stat(&rel) {
                unread.push(rel);
                tracker.add(1, 0);
                continue;
            }
            let disk = match self.read_derived(&rel) {
                Ok(disk) => disk,
                Err(reason) => {
                    // A file another program holds, or a placeholder: neither damaged nor
                    // gone. Nothing is written over it, and it is tried again (§4.4, §6.1).
                    self.unreadable_derived(&rel, file, reason);
                    tracker.add(1, 0);
                    continue;
                }
            };
            match (&disk, &entry) {
                (Some(bytes), Some(entry))
                    if same_scope && entry.held.is_none() && md5_hex(bytes) == entry.md5 =>
                {
                    // Read, not projected: the bytes are the app's own.
                    let md5 = entry.md5.clone();
                    self.refresh_entry(&rel, &md5, bytes.len() as u64, None, None);
                }
                _ => to_project.push((rel, file.clone(), disk)),
            }
            tracker.add(1, 0);
        }
        // Paths only the manifest lists: a removal that stopped before M7, or files to go.
        let listed: Vec<String> = self
            .manifest
            .paths
            .keys()
            .filter(|p| {
                !projected.contains(*p)
                    && p.as_str() != METADATA
                    && !crate::materialize::FIXED.contains(&p.as_str())
            })
            .cloned()
            .collect();
        for rel in listed {
            let entry = self.manifest.paths.get(&rel).cloned();
            if entry.as_ref().is_some_and(|e| e.oid.is_some()) {
                continue;
            }
            match self.vfs.metadata(&self.folder.path(&rel)) {
                Err(e) if is_not_found(&e) => {
                    self.manifest.paths.remove(&rel);
                    self.manifest_changed = true;
                }
                // Still on disk: the materialization removes it (M0, X).
                _ => self.dirty.metadata = true,
            }
        }
        if !to_project.is_empty() {
            let wanted: BTreeSet<DerivedFile> =
                to_project.iter().map(|(_, f, _)| f.clone()).collect();
            let mut rendered = self.render_files(&wanted);
            for (rel, file, disk) in to_project {
                let Some(new) = rendered.remove(&rel) else {
                    continue;
                };
                self.classify_one(&rel, &file, disk.as_deref(), &new, intent.as_ref())?;
                if self.dirty.files.contains(&file) {
                    self.prerendered.insert(rel, new);
                }
            }
        }
        // metadata.json: converged when its entry is clean by stat and lists what it would.
        let scope = self.metadata_scope();
        let metadata_ok = versions
            && self
                .manifest
                .paths
                .get(METADATA)
                .is_some_and(|e| e.scope.as_deref() == Some(scope.as_str()))
            && !intent.as_ref().is_some_and(|i| i.names(METADATA))
            && (self.clean_by_stat(METADATA) || self.metadata_matches_entry());
        if !metadata_ok {
            self.dirty.metadata = true;
        }
        tracker.add(1, 0);
        if intent.is_some() && self.dirty.is_empty() {
            atomic_remove(&*self.vfs, &self.folder.path(INTENT), SyncDir::Now)?;
        }
        self.renew_hold_notices();
        Ok(unread)
    }

    /// The bytes of a derived file: `None` when it is gone, the reason when it cannot be
    /// read now.
    fn read_derived(&self, rel: &str) -> Result<Option<Vec<u8>>, String> {
        let path = self.folder.path(rel);
        match self.vfs.read(&path) {
            Ok(bytes) => {
                self.counters.add(&self.counters.derived_read, 1);
                Ok(Some(bytes))
            }
            Err(e) if is_not_found(&e) => Ok(None),
            Err(e) => Err(StoreError::io("read", &path, e).to_string()),
        }
    }

    /// A derived file the open could not read: the person is told (`FileUnreadable`), the
    /// file is left as it is, and the path stays dirty, so the next materialization reads it
    /// again (M1) and writes it when it can.
    fn unreadable_derived(&mut self, rel: &str, file: &DerivedFile, reason: String) {
        self.dirty.files.insert(file.clone());
        self.notice(
            Notice::FileUnreadable {
                path: rel.into(),
                reason: reason.as_str().into(),
            },
            None,
        );
    }

    /// Whether `metadata.json` holds what the manifest says the app wrote. A file that
    /// cannot be read now is not known to, so `metadata.json` is written again.
    fn metadata_matches_entry(&self) -> bool {
        let Some(entry) = self.manifest.paths.get(METADATA) else {
            return false;
        };
        matches!(self.read_derived(METADATA), Ok(Some(bytes)) if md5_hex(&bytes) == entry.md5)
    }

    /// Takes a path's md5, size and mtime again after a read found bytes the projection
    /// gives; the entry settles when the read began at least 2 s after its mtime (§6.2).
    /// `scope` replaces the entry's digest, and `nfc` its normalization, when given.
    fn refresh_entry(
        &mut self,
        rel: &str,
        md5: &str,
        size: u64,
        scope: Option<String>,
        nfc: Option<bool>,
    ) {
        let meta = self.vfs.metadata(&self.folder.path(rel)).ok();
        let stamp = self.stamp_ns;
        let network = self.network;
        if let Some(entry) = self.manifest.paths.get_mut(rel) {
            let mtime = meta.map_or(entry.mtime_ns, |m| m.mtime_ns);
            let settled = !network && stamp >= mtime.saturating_add(2_000_000_000);
            let changed = entry.md5 != md5
                || entry.size != size
                || entry.mtime_ns != mtime
                || entry.settled != settled
                || entry.held.is_some()
                || (scope.is_some() && entry.scope != scope)
                || (nfc.is_some() && entry.nfc != nfc);
            entry.md5 = md5.to_owned();
            entry.size = size;
            entry.mtime_ns = mtime;
            entry.settled = settled;
            let was_held = entry.held.take().is_some();
            if scope.is_some() {
                entry.scope = scope;
            }
            if nfc.is_some() {
                entry.nfc = nfc;
            }
            self.manifest_changed |= changed;
            if was_held {
                self.shared.remove_notices_of(rel);
            }
        }
    }

    /// The table of §6.2 for one path whose projection is `new`.
    fn classify_one(
        &mut self,
        rel: &str,
        file: &DerivedFile,
        disk: Option<&[u8]>,
        new: &[u8],
        intent: Option<&Intent>,
    ) -> Result<(), StoreError> {
        let e = md5_hex(new);
        let entry = self.manifest.paths.get(rel).cloned();
        let Some(disk) = disk else {
            // Missing: rebuilt, never read as a deletion (I11). A deletion ends a hold too,
            // which M0 does for the path it writes (§5.10).
            self.dirty.files.insert(file.clone());
            if entry.is_some()
                && let Ok(path) = RepoPath::parse(rel)
            {
                self.notice(Notice::MissingFileRebuilt { path }, None);
            }
            return Ok(());
        };
        let d = md5_hex(disk);
        if d == e {
            let scope = self.fold.scope_digest(file).to_hex();
            // `metadata.json` declares the normalization of the text it lists, so an entry
            // made from a read keeps what the read found.
            let nfc = crate::materialize::is_text_path(rel)
                .then(|| unicode_normalization::is_nfc(&String::from_utf8_lossy(disk)));
            match self.manifest.paths.contains_key(rel) {
                true => self.refresh_entry(rel, &d, disk.len() as u64, Some(scope), nfc),
                false => {
                    let meta = self.vfs.metadata(&self.folder.path(rel)).ok();
                    self.manifest.paths.insert(
                        rel.to_owned(),
                        Entry {
                            scope: Some(scope),
                            nfc,
                            ..Entry::of(d, disk.len() as u64, meta.map_or(0, |m| m.mtime_ns), false)
                        },
                    );
                    self.manifest_changed = true;
                }
            }
            return Ok(());
        }
        let ours = entry.as_ref().is_some_and(|en| en.md5 == d)
            || intent.is_some_and(|i| i.md5s_of(rel).contains(&d.as_str()));
        if let Some(held) = entry.as_ref().and_then(|en| en.held.as_ref()) {
            if held.md5 == d {
                return Ok(());
            }
            self.hold(rel, disk, false)?;
            return Ok(());
        }
        if ours {
            self.dirty.files.insert(file.clone());
            return Ok(());
        }
        let damage = self.clean_by_stat(rel);
        self.hold(rel, disk, damage)
    }

    /// Renders `files`, a book's files together.
    fn render_files(&self, files: &BTreeSet<DerivedFile>) -> BTreeMap<String, Vec<u8>> {
        let mut out = BTreeMap::new();
        let mut books = BTreeSet::new();
        for file in files {
            match file.book() {
                Some(book) => {
                    books.insert(book);
                }
                None => {
                    if let Some(bytes) = self.fold.render(file) {
                        self.counters.add(&self.counters.projected, 1);
                        out.insert(file.path(), bytes);
                    }
                }
            }
        }
        for book in books {
            for (file, bytes) in self.fold.render_book(book) {
                if files.contains(&file) {
                    self.counters.add(&self.counters.projected, 1);
                    out.insert(file.path(), bytes);
                }
            }
        }
        out
    }

    /// O14 results (§6.1): equal bytes settle an entry once its tick is over; different
    /// bytes are classified again on the actor, where nothing can change them meanwhile.
    pub(crate) fn apply_check(&mut self, results: Vec<CheckResult>) -> Result<(), StoreError> {
        for result in results {
            let Some(entry) = self.manifest.paths.get(&result.rel).cloned() else {
                continue;
            };
            if entry.held.is_some() {
                continue;
            }
            match &result.md5 {
                Some(md5) if *md5 == entry.md5 => {
                    let settled = !self.network
                        && entry.size == result.size
                        && entry.mtime_ns == result.mtime_ns
                        && result.stamp_ns >= result.mtime_ns.saturating_add(2_000_000_000);
                    if settled
                        && !entry.settled
                        && let Some(e) = self.manifest.paths.get_mut(&result.rel)
                    {
                        e.settled = true;
                        self.manifest_changed = true;
                    }
                }
                Some(_) => {
                    // Read again here: the file may have changed since the check read it.
                    let Some(file) = self
                        .fold
                        .derived_files()
                        .into_iter()
                        .find(|f| f.path() == result.rel)
                    else {
                        continue;
                    };
                    let Ok(disk) = self.read_derived(&result.rel) else {
                        continue;
                    };
                    let rendered = self.render_files(&BTreeSet::from([file.clone()]));
                    if let Some(new) = rendered.get(&result.rel) {
                        self.classify_one(&result.rel, &file, disk.as_deref(), new, None)?;
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
