//! Whether a cloud client copies a folder to other computers (STORAGE §5.0).
//!
//! This is a copy of `tc_store::platform`'s detection (`cloud_client`, `dropbox_roots` and the
//! Windows `placeholder`), which is not on this branch yet. It reads its variables from
//! [`HostEnv`] instead of the process, so a test on Linux can describe a OneDrive or Dropbox
//! computer, and it checks a folder that does not exist yet, as the first launch needs.
//! Unit C1a moves the app onto tc-store and deletes this file for
//! `tc_store::platform::StdPlatform::folder`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::config::HostEnv;

pub trait FolderCheck: Send + Sync + 'static {
    fn is_folder(&self, path: &Path) -> bool;

    /// The client that copies `path` to other computers (STORAGE §5.0). The path may not
    /// exist yet: only its existing ancestors are looked at then.
    fn synced_by(&self, path: &Path) -> Option<String>;
}

/// This computer, described by `env`.
#[derive(Debug, Clone)]
pub struct HostFolders {
    env: HostEnv,
}

impl HostFolders {
    pub fn new(env: HostEnv) -> Self {
        Self { env }
    }
}

impl FolderCheck for HostFolders {
    fn is_folder(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn synced_by(&self, path: &Path) -> Option<String> {
        cloud_client(&self.env, &resolve(path))
    }
}

/// `path` with every symbolic link that exists resolved, and the part that does not exist
/// yet kept as it is. A temporary or home folder can sit behind a link (macOS `/var` is
/// `/private/var`), and the sync roots are compared as paths.
fn resolve(path: &Path) -> PathBuf {
    let mut missing: Vec<&OsStr> = Vec::new();
    let mut existing = path;
    loop {
        if let Ok(mut real) = std::fs::canonicalize(existing) {
            real.extend(missing.iter().rev());
            return real;
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                missing.push(name);
                existing = parent;
            }
            _ => return path.to_owned(),
        }
    }
}

/// The cloud client that replicates `path`, if any (§5.0): under a known sync root, an
/// ancestor holding a sync marker, or a folder with a cloud placeholder attribute.
fn cloud_client(env: &HostEnv, path: &Path) -> Option<String> {
    let under = |root: &Path| path.starts_with(resolve(root));
    for (var, client) in [
        ("OneDrive", "OneDrive"),
        ("OneDriveConsumer", "OneDrive"),
        ("OneDriveCommercial", "OneDrive"),
    ] {
        if let Some(root) = env.var(var)
            && under(Path::new(root))
        {
            return Some(client.to_owned());
        }
    }
    for root in dropbox_roots(env) {
        if under(&root) {
            return Some("Dropbox".to_owned());
        }
    }
    if let Some(home) = env.var("HOME").or_else(|| env.var("USERPROFILE")) {
        let home = PathBuf::from(home);
        for (rel, client) in [
            ("Library/Mobile Documents", "iCloud Drive"),
            ("Library/CloudStorage", "a cloud storage client"),
            ("Google Drive", "Google Drive"),
            ("My Drive", "Google Drive"),
        ] {
            if under(&home.join(rel)) {
                return Some(client.to_owned());
            }
        }
    }
    for ancestor in path.ancestors() {
        // Dropbox keeps its settings in `~/.dropbox` on macOS and Linux; that folder holds
        // `info.json` and marks no sync root. tc-store's copy of this loop reads it as one,
        // which calls every folder under the home folder synced.
        let marker = ancestor.join(".dropbox");
        if marker.exists() && !marker.join("info.json").is_file() {
            return Some("Dropbox".to_owned());
        }
        if ancestor.join(".stfolder").exists() {
            return Some("Syncthing".to_owned());
        }
    }
    placeholder(path)
}

/// The Dropbox folders its `info.json` names.
fn dropbox_roots(env: &HostEnv) -> Vec<PathBuf> {
    let mut places = Vec::new();
    for var in ["HOME", "APPDATA", "LOCALAPPDATA"] {
        if let Some(dir) = env.var(var) {
            let dir = PathBuf::from(dir);
            places.push(dir.join(".dropbox").join("info.json"));
            places.push(dir.join("Dropbox").join("info.json"));
        }
    }
    let mut roots = Vec::new();
    for place in places {
        let Ok(text) = std::fs::read_to_string(place) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if let Some(accounts) = value.as_object() {
            for account in accounts.values() {
                if let Some(path) = account.get("path").and_then(|p| p.as_str()) {
                    roots.push(PathBuf::from(path));
                }
            }
        }
    }
    roots
}

#[cfg(windows)]
fn placeholder(path: &Path) -> Option<String> {
    use std::os::windows::fs::MetadataExt;
    const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
    const OFFLINE: u32 = 0x0000_1000;
    for probe in [path.to_owned(), path.join("metadata.json")] {
        if let Ok(meta) = std::fs::metadata(&probe)
            && meta.file_attributes() & (RECALL_ON_DATA_ACCESS | OFFLINE) != 0
        {
            return Some("a cloud storage client".to_owned());
        }
    }
    None
}

#[cfg(not(windows))]
fn placeholder(_path: &Path) -> Option<String> {
    None
}
