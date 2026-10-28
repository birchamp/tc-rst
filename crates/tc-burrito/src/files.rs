//! Access to a project's files for the validator. The validator itself does no I/O: it
//! asks a [`ProjectFiles`] about each path, so the same checks run on a folder on disk
//! ([`DirProject`]) and on bytes about to be written ([`MemProject`]).

use crate::ingredient::FileFacts;
use crate::path::IngredientPath;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The folder whose files must all be listed as ingredients, as tC4 and Pankosmia lay out a
/// project.
pub(crate) const INGREDIENTS_DIR: &str = "ingredients";

/// What is at an ingredient's path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileState {
    Present(FileFacts),
    Missing,
    /// A folder, a symbolic link or another special file. Links are refused so a project
    /// can never make the validator read outside its folder.
    NotAFile,
    Unreadable(String),
}

pub trait ProjectFiles {
    fn inspect(&self, path: &IngredientPath) -> FileState;

    /// Every file under `ingredients/`, as `/`-separated paths from the project root,
    /// leaving out hidden entries and `*.bak` files, which tC4 and Pankosmia ignore.
    fn ingredient_dir_files(&self) -> Result<Vec<String>, String>;
}

/// Files kept out of the burrito: hidden entries (`.git`, `.DS_Store`, temporary files of
/// atomic writes) and `*.bak` backups. Pankosmia's rescan skips the same names.
pub(crate) fn is_ignored(name: &str) -> bool {
    name.starts_with('.') || name.ends_with(".bak")
}

/// A project folder on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirProject {
    root: PathBuf,
}

impl DirProject {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl ProjectFiles for DirProject {
    fn inspect(&self, path: &IngredientPath) -> FileState {
        let mut full = self.root.clone();
        let segments: Vec<&str> = path.segments().collect();
        for (index, segment) in segments.iter().enumerate() {
            full.push(segment);
            let metadata = match fs::symlink_metadata(&full) {
                Ok(metadata) => metadata,
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                    ) =>
                {
                    return FileState::Missing;
                }
                Err(error) => return FileState::Unreadable(error.to_string()),
            };
            let last = index + 1 == segments.len();
            if metadata.file_type().is_symlink() || (last && !metadata.is_file()) {
                return FileState::NotAFile;
            }
            if !last && !metadata.is_dir() {
                return FileState::Missing;
            }
        }
        match fs::File::open(&full).and_then(FileFacts::of_reader) {
            Ok(facts) => FileState::Present(facts),
            Err(error) => FileState::Unreadable(error.to_string()),
        }
    }

    fn ingredient_dir_files(&self) -> Result<Vec<String>, String> {
        let start = self.root.join(INGREDIENTS_DIR);
        match fs::symlink_metadata(&start) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => return Ok(vec![INGREDIENTS_DIR.to_owned()]),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.to_string()),
        }
        let mut found = Vec::new();
        let mut pending = vec![(start, INGREDIENTS_DIR.to_owned())];
        while let Some((dir, relative)) = pending.pop() {
            let entries = fs::read_dir(&dir).map_err(|error| error.to_string())?;
            for entry in entries {
                let entry = entry.map_err(|error| error.to_string())?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if is_ignored(&name) {
                    continue;
                }
                let child = format!("{relative}/{name}");
                // `DirEntry::file_type` does not follow links, so a linked folder is
                // reported as an entry instead of being walked.
                let file_type = entry.file_type().map_err(|error| error.to_string())?;
                if file_type.is_dir() {
                    pending.push((entry.path(), child));
                } else {
                    found.push(child);
                }
            }
        }
        found.sort();
        Ok(found)
    }
}

/// Project files held in memory, keyed by their path from the project root. Use it to
/// check metadata against the exact bytes before they are written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemProject {
    files: BTreeMap<String, Vec<u8>>,
}

impl MemProject {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, path: impl Into<String>, bytes: impl Into<Vec<u8>>) {
        self.files.insert(path.into(), bytes.into());
    }

    pub fn remove(&mut self, path: &str) -> Option<Vec<u8>> {
        self.files.remove(path)
    }

    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }
}

impl From<BTreeMap<String, Vec<u8>>> for MemProject {
    fn from(files: BTreeMap<String, Vec<u8>>) -> Self {
        Self { files }
    }
}

impl ProjectFiles for MemProject {
    fn inspect(&self, path: &IngredientPath) -> FileState {
        match self.files.get(path.as_str()) {
            Some(bytes) => FileState::Present(FileFacts::of(bytes)),
            None => FileState::Missing,
        }
    }

    fn ingredient_dir_files(&self) -> Result<Vec<String>, String> {
        let prefix = format!("{INGREDIENTS_DIR}/");
        Ok(self
            .files
            .keys()
            .filter(|path| {
                path.strip_prefix(&prefix)
                    .is_some_and(|rest| !rest.split('/').any(is_ignored))
            })
            .cloned()
            .collect())
    }
}
