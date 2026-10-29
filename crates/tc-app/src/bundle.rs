//! The list of installs a bundled store promises (APP-DESIGN §4.6, unit A9).
//!
//! The installer ships an expanded store (`tc4 serve --bundle <dir>`). Its folders alone cannot
//! say what is missing: an install folder that was not shipped, or was lost on the way, leaves
//! a store that looks whole. So the build of the bundle writes `BUNDLE-MANIFEST.json` beside the
//! installs, naming every install it made, and the start compares the two. tC4 does the same
//! with the `bundled_resources` of its `BUILD-MANIFEST.json`, and stops its start when an
//! archive is missing (`tC4:scripts/desktop-bootstrap.cjs:230-290`). Whatever adds an install to
//! a bundle after the build, such as the packaging step that downloads the Open Bible Stories
//! pictures, builds the manifest again with it.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tc_resources::ResourcePin;

/// The manifest's file name, in the bundle's root.
pub const MANIFEST: &str = "BUNDLE-MANIFEST.json";

const VERSION: u32 = 1;

/// Why a bundle's manifest cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("{path}: {reason}")]
    Io { path: PathBuf, reason: String },
    #[error("{MANIFEST} is not a bundle manifest: {0}")]
    Unreadable(String),
    #[error("{MANIFEST} is for manifest version {0}, and this app reads version {VERSION}")]
    Version(u32),
    #[error("{MANIFEST} lists no install")]
    Empty,
    #[error("{MANIFEST} could not be made: {0}")]
    Unwritable(String),
    #[error(transparent)]
    Write(tc_store::StoreError),
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct File {
    manifest_version: u32,
    installs: Vec<ResourcePin>,
}

/// The Door43 installs a bundle promises, each by repository and commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleManifest {
    installs: Vec<ResourcePin>,
}

impl BundleManifest {
    /// The manifest of a bundle that holds `pins`. Only each pin's identity is kept: the
    /// repository, the commit and the label.
    pub fn new(pins: impl IntoIterator<Item = ResourcePin>) -> Self {
        Self {
            installs: pins
                .into_iter()
                .map(|pin| {
                    let identity = ResourcePin::new(pin.repo, pin.sha);
                    match pin.version {
                        Some(version) => identity.with_version(version),
                        None => identity,
                    }
                })
                .collect(),
        }
    }

    pub fn installs(&self) -> &[ResourcePin] {
        &self.installs
    }

    /// The promised installs that `held` does not say it has, in the manifest's order. Pure.
    pub fn missing(&self, held: impl Fn(&ResourcePin) -> bool) -> Vec<&ResourcePin> {
        self.installs.iter().filter(|pin| !held(pin)).collect()
    }

    /// Reads the manifest of the bundle in `bundle`.
    pub fn read(bundle: &Path) -> Result<Self, ManifestError> {
        let path = bundle.join(MANIFEST);
        let bytes = fs::read(&path).map_err(|e| ManifestError::Io {
            path: path.clone(),
            reason: if e.kind() == std::io::ErrorKind::NotFound {
                "the bundle has no manifest of its installs".to_owned()
            } else {
                e.to_string()
            },
        })?;
        let file: File =
            serde_json::from_slice(&bytes).map_err(|e| ManifestError::Unreadable(e.to_string()))?;
        if file.manifest_version != VERSION {
            return Err(ManifestError::Version(file.manifest_version));
        }
        if file.installs.is_empty() {
            return Err(ManifestError::Empty);
        }
        Ok(Self::new(file.installs))
    }

    /// Writes the manifest into `bundle` through `tc_store::atomic_write`, so a build that
    /// stops leaves the old manifest or the new one. Bytes in place whose folder flush failed
    /// count as written: a folder that cannot be flushed (Windows) is no reason to fail the
    /// build.
    pub fn write(&self, bundle: &Path) -> Result<(), ManifestError> {
        let file = File {
            manifest_version: VERSION,
            installs: self.installs.clone(),
        };
        let mut bytes = serde_json::to_vec_pretty(&file)
            .map_err(|e| ManifestError::Unwritable(e.to_string()))?;
        bytes.push(b'\n');
        match tc_store::atomic_write(
            &tc_store::StdFs,
            bundle,
            MANIFEST,
            &bytes,
            tc_store::SyncDir::Now,
        ) {
            Ok(()) | Err(tc_store::WriteError::WrittenNotDurable(_)) => Ok(()),
            Err(tc_store::WriteError::NotWritten(error)) => Err(ManifestError::Write(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(name: &str, sha: char, version: Option<&str>) -> ResourcePin {
        let repo = format!("git.door43.org/unfoldingWord/{name}");
        let pin = ResourcePin::new(
            repo.parse().unwrap(),
            sha.to_string().repeat(40).parse().unwrap(),
        );
        match version {
            Some(v) => pin.with_version(v.parse().unwrap()),
            None => pin,
        }
        .with_flavor("x")
    }

    #[test]
    fn a_manifest_round_trips_with_only_identities() {
        let dir = tempfile::tempdir().unwrap();
        let manifest =
            BundleManifest::new([pin("en_tn", 'a', Some("v91")), pin("en_ugl", 'b', None)]);
        assert!(manifest.installs().iter().all(|p| p.flavor.is_none()));
        manifest.write(dir.path()).unwrap();
        assert_eq!(BundleManifest::read(dir.path()).unwrap(), manifest);
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, [MANIFEST], "no temporary file is left");
        let text = fs::read_to_string(dir.path().join(MANIFEST)).unwrap();
        assert!(text.contains("en_tn") && !text.contains("flavor"), "{text}");
    }

    #[test]
    fn missing_names_each_promised_install_the_check_does_not_hold() {
        let a = pin("en_tn", 'a', Some("v91"));
        let b = pin("en_tw", 'b', Some("v91"));
        let c = pin("en_ult", 'c', Some("v91"));
        let manifest = BundleManifest::new([a.clone(), b.clone(), c.clone()]);
        let held = [a.clone(), c.clone()];
        let missing = manifest.missing(|p| held.iter().any(|h| h.same_identity(p)));
        assert_eq!(missing, [&manifest.installs()[1]]);
        assert!(manifest.missing(|_| true).is_empty());
        assert_eq!(manifest.missing(|_| false).len(), 3);
    }

    #[test]
    fn a_manifest_that_cannot_be_written_names_the_folder_and_leaves_none() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("gone");
        let manifest = BundleManifest::new([pin("en_tn", 'a', Some("v91"))]);
        let error = manifest.write(&gone).unwrap_err();
        assert!(matches!(error, ManifestError::Write(_)), "{error:?}");
        assert!(error.to_string().contains("gone"), "{error}");
        assert!(!gone.exists());
    }

    #[test]
    fn a_bundle_without_a_usable_manifest_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let none = BundleManifest::read(dir.path()).unwrap_err().to_string();
        assert!(none.contains("no manifest"), "{none}");

        let write = |text: &str| fs::write(dir.path().join(MANIFEST), text).unwrap();
        write("not json");
        assert!(matches!(
            BundleManifest::read(dir.path()),
            Err(ManifestError::Unreadable(_))
        ));
        write(r#"{"manifestVersion": 2, "installs": []}"#);
        assert!(matches!(
            BundleManifest::read(dir.path()),
            Err(ManifestError::Version(2))
        ));
        write(r#"{"manifestVersion": 1, "installs": []}"#);
        assert!(matches!(
            BundleManifest::read(dir.path()),
            Err(ManifestError::Empty)
        ));
    }
}
