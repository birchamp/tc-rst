//! Whether a cloud client copies a folder to other computers (STORAGE §5.0), for the first
//! launch: tc-store's one rule (`tc_store::platform::synced_by`), with the variables of
//! [`HostEnv`] rather than the process, so a test on Linux can describe a OneDrive or Dropbox
//! computer.

use std::path::Path;

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
        tc_store::platform::synced_by(&|name| self.env.var(name).map(ToOwned::to_owned), path)
    }
}
