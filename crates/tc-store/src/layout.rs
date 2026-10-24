//! Where things are in a project folder (`docs/STORAGE.md` §3).

use std::path::{Path, PathBuf};

/// `.tc/`: local state, never exchanged, not an ingredient (§3.6).
pub const TC: &str = ".tc";
pub const PENDING: &str = ".tc/pending";
pub const QUARANTINE: &str = ".tc/quarantine";
pub const EXTERNAL: &str = ".tc/external";
pub const HEADS: &str = ".tc/heads";
pub const LOCK: &str = ".tc/lock";
pub const DEVICE: &str = ".tc/device.json";
pub const MANIFEST: &str = ".tc/materialized.json";
pub const INDEX: &str = ".tc/index.bin";
pub const DRAFTS: &str = ".tc/drafts.json";
pub const QUARANTINE_LIST: &str = ".tc/quarantine.json";
pub const INTAKE: &str = ".tc/intake.json";

pub const INGREDIENTS: &str = "ingredients";
pub const PROJECT_JSON: &str = "ingredients/tc/project.json";
pub const FORMAT_JSON: &str = tc_journal::format::FORMAT_PATH;
pub const PACKS: &str = tc_journal::pack::PACKS_DIR;
pub const METADATA: &str = "metadata.json";
pub const GITIGNORE: &str = ".gitignore";
pub const GITATTRIBUTES: &str = ".gitattributes";

/// The app-owned files at the root (§3.1), never ingredients.
pub const GITIGNORE_BYTES: &[u8] = b"/.tc/\n.~*\n**/*.bak\n";
pub const GITATTRIBUTES_BYTES: &[u8] = b"* -text\n";

/// A project folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    root: PathBuf,
}

impl Folder {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The absolute path of a `/`-separated path relative to the folder.
    pub fn path(&self, rel: &str) -> PathBuf {
        let mut out = self.root.clone();
        for part in rel.split('/').filter(|p| !p.is_empty()) {
            out.push(part);
        }
        out
    }

    /// The directory and file name of a relative path.
    pub fn dir_and_name<'a>(&self, rel: &'a str) -> (PathBuf, &'a str) {
        match rel.rsplit_once('/') {
            Some((dir, name)) => (self.path(dir), name),
            None => (self.root.clone(), rel),
        }
    }
}

/// The name a derived path's heads file has in `.tc/heads/` (§3.6): the path with `%` and `/`
/// escaped. Ingredient paths never hold `%` (§3.3), so the escape is one to one.
pub fn escaped(path: &str) -> String {
    path.replace('%', "%25").replace('/', "%2F")
}

/// The relative path of the heads file of `path`.
pub fn heads_file(path: &str) -> String {
    format!("{HEADS}/{}.bin", escaped(path))
}
