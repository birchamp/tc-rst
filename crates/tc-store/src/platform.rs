//! What the store asks of the computer it runs on: the machine id of a device's fingerprint
//! (§6.3), the host and process that hold a lock (§3.6), and whether a folder can keep a
//! project safe (§5.0: FAT32 and exFAT do not make a rename atomic; a folder a cloud client
//! replicates can get a second writer; a network drive answers sizes and times from a cache,
//! §6.2).

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// Why a folder opens read-only before anything is read (§5.0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnsafeFolder {
    /// "This drive cannot keep the project safe if the power fails."
    Filesystem { name: String },
    /// "This folder is copied to other computers by OneDrive."
    CloudSynced { client: String },
}

/// What a folder is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FolderFacts {
    pub unsafe_folder: Option<UnsafeFolder>,
    /// On a network drive no path is clean by stat and no entry settles (§6.2).
    pub network: bool,
}

pub trait Platform: Send + Sync + std::fmt::Debug {
    /// The operating system's machine id (`/etc/machine-id`, `MachineGuid`,
    /// `IOPlatformUUID`).
    fn machine_id(&self) -> String;
    fn host(&self) -> String;
    fn pid(&self) -> u32;
    fn folder(&self, path: &Path) -> FolderFacts;
}

/// This computer.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdPlatform;

impl Platform for StdPlatform {
    fn machine_id(&self) -> String {
        machine_uid::get().unwrap_or_else(|_| "unknown-machine".to_owned())
    }

    fn host(&self) -> String {
        sysinfo::System::host_name().unwrap_or_else(|| "unknown-host".to_owned())
    }

    fn pid(&self) -> u32 {
        std::process::id()
    }

    fn folder(&self, path: &Path) -> FolderFacts {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
        let filesystem = filesystem_of(&path);
        let fs_name = filesystem.as_deref().unwrap_or("").to_ascii_lowercase();
        let unsafe_folder = if matches!(
            fs_name.as_str(),
            "vfat" | "msdos" | "fat" | "fat32" | "exfat"
        ) {
            Some(UnsafeFolder::Filesystem {
                name: fs_name.clone(),
            })
        } else {
            synced_by(&process_env, &path).map(|client| UnsafeFolder::CloudSynced { client })
        };
        let network = matches!(
            fs_name.as_str(),
            "nfs"
                | "nfs4"
                | "cifs"
                | "smb"
                | "smbfs"
                | "smb2"
                | "smb3"
                | "afpfs"
                | "webdav"
                | "davfs"
                | "fuse.sshfs"
                | "9p"
        ) || is_unc(&path);
        FolderFacts {
            unsafe_folder,
            network,
        }
    }
}

/// The filesystem of the mount that holds `path`.
fn filesystem_of(path: &Path) -> Option<String> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| path.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.file_system().to_string_lossy().into_owned())
}

fn is_unc(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.starts_with(r"\\") && !text.starts_with(r"\\?\") || text.starts_with(r"\\?\UNC\")
}

/// The client that copies `path` to other computers (§5.0), with the environment variables
/// `env` names: under a known sync root, an ancestor holding a sync marker, or a folder with a
/// cloud placeholder attribute. The path may not exist yet: only its existing ancestors are
/// looked at then. The one rule of the store and of the app's first-launch check.
pub fn synced_by(env: &dyn Fn(&str) -> Option<OsString>, path: &Path) -> Option<String> {
    cloud_client(env, &resolve(path))
}

/// The process's own environment, as [`synced_by`] reads it: an empty variable is none.
fn process_env(name: &str) -> Option<OsString> {
    std::env::var_os(name).filter(|value| !value.is_empty())
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
fn cloud_client(env: &dyn Fn(&str) -> Option<OsString>, path: &Path) -> Option<String> {
    let under = |root: &Path| path.starts_with(resolve(root));
    for (var, client) in [
        ("OneDrive", "OneDrive"),
        ("OneDriveConsumer", "OneDrive"),
        ("OneDriveCommercial", "OneDrive"),
    ] {
        if let Some(root) = env(var)
            && under(Path::new(&root))
        {
            return Some(client.to_owned());
        }
    }
    for root in dropbox_roots(env) {
        if under(&root) {
            return Some("Dropbox".to_owned());
        }
    }
    if let Some(home) = env("HOME").or_else(|| env("USERPROFILE")) {
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
        // `info.json` and marks no sync root (INV-SYNCED-FOLDER-DETECTION-IS-EXACT).
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
fn dropbox_roots(env: &dyn Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    let mut places = Vec::new();
    for var in ["HOME", "APPDATA", "LOCALAPPDATA"] {
        if let Some(dir) = env(var) {
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

/// A computer a test describes.
#[derive(Debug, Clone)]
pub struct FakePlatform {
    pub machine: String,
    pub host: String,
    pub pid: u32,
    pub facts: FolderFacts,
}

impl Default for FakePlatform {
    fn default() -> Self {
        Self {
            machine: "machine-1".to_owned(),
            host: "host-1".to_owned(),
            pid: 4242,
            facts: FolderFacts::default(),
        }
    }
}

impl Platform for FakePlatform {
    fn machine_id(&self) -> String {
        self.machine.clone()
    }

    fn host(&self) -> String {
        self.host.clone()
    }

    fn pid(&self) -> u32 {
        self.pid
    }

    fn folder(&self, _path: &Path) -> FolderFacts {
        self.facts.clone()
    }
}
