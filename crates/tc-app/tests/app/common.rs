//! What the test modules share.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use tc_app::config::{HostEnv, HostOs};
use tc_app::folders::FolderCheck;

/// A computer that says which folders exist and which a cloud client copies.
#[derive(Default)]
pub struct FakeFolders {
    pub folders: BTreeSet<PathBuf>,
    /// A path is copied when it, or an ancestor, is in this map.
    pub synced: BTreeMap<PathBuf, String>,
}

impl FolderCheck for FakeFolders {
    fn is_folder(&self, path: &Path) -> bool {
        self.folders.contains(path)
    }

    fn synced_by(&self, path: &Path) -> Option<String> {
        path.ancestors()
            .find_map(|ancestor| self.synced.get(ancestor).cloned())
    }
}

pub fn env(os: HostOs, vars: &[(&str, &str)]) -> HostEnv {
    HostEnv::new(
        os,
        vars.iter()
            .map(|(name, value)| ((*name).to_owned(), OsString::from(*value))),
    )
}
