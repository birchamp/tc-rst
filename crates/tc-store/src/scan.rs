//! Files that are new to the project or cannot be one (`docs/STORAGE.md` §3.3, §11.1): a
//! listing of the folders another program may put a file in. It lists, it opens no file.
//!
//! A file whose name breaks the path grammar, a file of the operating system and anything
//! under tC4's exchange folders cannot be an ingredient of the burrito: it is moved to
//! `.tc/external/` (a rename, flushed), never deleted (L62). Every other file the manifest
//! does not know is returned, for the caller to examine as an outside edit (a book file) or
//! to take in as a carried file (§11.4).

use crate::appdata::utc_stamp;
use crate::error::StoreError;
use crate::layout::{self, EXTERNAL, INGREDIENTS};
use crate::vfs::EntryKind;
use crate::writer::Writer;
use std::path::Path;
use tc_journal::RepoPath;
use tc_journal::fold::{DerivedFile, Notice};

/// Folders tC4 keeps for its own exchange under `ingredients/` (TC4-DELTA §3.3): not a
/// translator's files, and not ingredients of this format.
const TC4_FOLDERS: [&str; 2] = ["ingredients/tc4-sync", "ingredients/tc4-open"];

/// What the listing found.
#[derive(Debug, Default)]
pub(crate) struct Scanned {
    /// Files the manifest has no entry for, that may be ingredients.
    pub new: Vec<String>,
    /// Files moved to `.tc/external/`, with the notice they call for.
    pub moved: Vec<Moved>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Moved {
    pub from: String,
    pub to: String,
    /// Junk of the operating system is moved without a word.
    pub told: bool,
}

/// Files an operating system or an editor leaves, which no one wants in a burrito.
fn is_junk(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "thumbs.db"
        || lower == "desktop.ini"
        || lower.ends_with(".bak")
        || lower.ends_with('~')
        || lower.ends_with(".tmp")
}

impl Writer {
    /// Lists the root and `ingredients/` (not `tc/changes/`, which is the journal's) and
    /// moves what cannot be an ingredient out of the folder.
    pub(crate) fn scan_folder(&mut self) -> Result<Scanned, StoreError> {
        let mut found: Vec<String> = Vec::new();
        self.list_into(self.folder.root(), "", false, &mut found);
        self.list_into(
            &self.folder.path(INGREDIENTS),
            INGREDIENTS,
            true,
            &mut found,
        );
        self.consider_new(found)
    }

    /// Of these paths, the ones that cannot be ingredients are moved out of the folder, and
    /// the ones the project knows nothing about are returned. A path of the manifest, of a
    /// pack or of a file the project writes is not new; one the open found changed is being
    /// recorded already.
    pub(crate) fn consider_new(&mut self, mut rels: Vec<String>) -> Result<Scanned, StoreError> {
        rels.sort();
        rels.dedup();
        let mut out = Scanned::default();
        for rel in rels {
            if let Some(told) = self.unfit(&rel) {
                if self.vfs.metadata(&self.folder.path(&rel)).is_err() {
                    continue;
                }
                if let Some(to) = self.move_out(&rel)? {
                    out.moved.push(Moved {
                        from: rel,
                        to,
                        told,
                    });
                }
                continue;
            }
            if self.manifest.paths.contains_key(&rel)
                || self.packs.contains_key(&rel)
                || DerivedFile::from_path(&rel).is_some_and(|f| self.fold.projects_file(&f))
                || self.found.iter().any(|f| f.rel == rel)
            {
                continue;
            }
            match self.vfs.metadata(&self.folder.path(&rel)) {
                Ok(meta) if meta.kind == EntryKind::File => out.new.push(rel),
                _ => {}
            }
        }
        Ok(out)
    }

    /// Lists `dir` (named `rel` in the project), and its folders when `deep`, into `out`.
    fn list_into(&self, dir: &Path, rel: &str, deep: bool, out: &mut Vec<String>) {
        let Ok(entries) = self.vfs.read_dir(dir) else {
            return;
        };
        for entry in entries {
            let Some(name) = entry.name.to_str() else {
                continue;
            };
            let path = if rel.is_empty() {
                name.to_owned()
            } else {
                format!("{rel}/{name}")
            };
            // Folders and files of the app, the repository and the person's own dot files.
            if name.starts_with('.') && !is_junk(name) {
                continue;
            }
            match entry.kind {
                EntryKind::File => {
                    if rel.is_empty()
                        && matches!(
                            name,
                            layout::METADATA | layout::GITIGNORE | layout::GITATTRIBUTES
                        )
                    {
                        continue;
                    }
                    out.push(path);
                }
                EntryKind::Dir if deep => {
                    if path == layout::PACKS.trim_end_matches('/')
                        || path.starts_with("ingredients/tc/changes")
                    {
                        continue;
                    }
                    self.list_into(&dir.join(name), &path, true, out);
                }
                EntryKind::Dir | EntryKind::Other => {}
            }
        }
    }

    /// Whether `rel` cannot be an ingredient: `Some(told)` when it cannot, and whether the
    /// person is told when it is moved.
    fn unfit(&self, rel: &str) -> Option<bool> {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        if is_junk(name) {
            return Some(false);
        }
        if TC4_FOLDERS
            .iter()
            .any(|folder| rel.starts_with(&format!("{folder}/")))
        {
            return Some(true);
        }
        if RepoPath::parse(rel).is_err() {
            return Some(true);
        }
        None
    }

    /// Moves `rel` to `.tc/external/<utc>-<escaped rel>` (a second file of the same name in
    /// the same second is `-2`, `-3`, …) and flushes both folders. `None` when another
    /// program holds the file: it stays, and the next scan tries again.
    fn move_out(&mut self, rel: &str) -> Result<Option<String>, StoreError> {
        let external = self.folder.path(EXTERNAL);
        crate::atomic::atomic_create_dirs(&*self.vfs, &external)?;
        let base = format!("{}-{}", utc_stamp(self.clock.now()), layout::escaped(rel));
        let mut name = base.clone();
        for n in 2..1000 {
            match self.vfs.metadata(&external.join(&name)) {
                Err(e) if crate::vfs::is_not_found(&e) => break,
                _ => name = format!("{base}-{n}"),
            }
        }
        let from = self.folder.path(rel);
        let moved = crate::atomic::rename_held(
            &*self.vfs,
            &from,
            &external.join(&name),
            &crate::atomic::HeldRetry::BACKGROUND,
        );
        match moved {
            Ok(()) => {}
            Err(e) if self.vfs.is_held(&e) => return Ok(None),
            Err(e) if crate::vfs::is_not_found(&e) => return Ok(None),
            Err(e) => return Err(StoreError::io("move", &from, e)),
        }
        let (dir, _) = self.folder.dir_and_name(rel);
        self.vfs
            .sync_dir(&dir)
            .map_err(|e| StoreError::io("flush", &dir, e))?;
        self.vfs
            .sync_dir(&external)
            .map_err(|e| StoreError::io("flush", &external, e))?;
        Ok(Some(format!("{EXTERNAL}/{name}")))
    }

    /// Tells the person of the files a scan moved.
    pub(crate) fn tell_moved(&self, moved: &[Moved]) {
        for m in moved.iter().filter(|m| m.told) {
            self.notice(
                Notice::FileMoved {
                    from: m.from.as_str().into(),
                    to: m.to.as_str().into(),
                },
                None,
            );
        }
    }
}
