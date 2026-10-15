//! The per-installation resource store: installed resources keyed by source, owner,
//! repository and commit, each installed atomically and never modified after.
//!
//! # Layout
//!
//! ```text
//! <root>/
//!   dcs/<owner>/<repo>/<sha>/                      a Door43 commit
//!   aquifer/<owner>/<repo>/<commit>/<lang>-<sha256 prefix>/   an Aquifer release asset
//!     receipt.json                                 what this is, and every file's size and SHA-256
//!     content/…                                    the resource's files
//!   .partial/                                      installs in progress
//! ```
//!
//! Owner and repository folders are lower-cased, because Door43 and GitHub resolve names
//! without regard to case; the receipt keeps the casing the pin gave. The folders nest
//! instead of joining as `<owner>--<repo>` (STORAGE §3.7), because both names may hold
//! `--` and a joined name would be ambiguous. An Aquifer commit holds an asset per
//! language, and a maintainer can re-upload an asset under the same tag, so an Aquifer
//! install is also keyed by language and asset digest; the folder carries the first 16
//! hex digits of the digest, and the receipt the whole.
//!
//! # Atomic install
//!
//! An install writes every file and then the receipt into a new folder under
//! `.partial/`, fsyncing each file and folder, then renames that folder to its key and
//! fsyncs the parent. A crash at any point leaves either no install or the complete one;
//! [`ResourceStore::open`] removes what a crash left in `.partial/`. An existing install
//! is never written again: a second install of the same identity finds it and stops.
//!
//! One process owns a store: the app's server. Two processes installing into one store
//! at once is not supported, because opening the store clears `.partial/`.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{InstallError, Refusal, StoreError};
use crate::pin::{AquiferPin, BookCoverage, ResourcePin, Sha256};
use crate::resolve::LocalCoverage;

const RECEIPT: &str = "receipt.json";
const CONTENT: &str = "content";
const PARTIAL: &str = ".partial";
const DCS: &str = "dcs";
const AQUIFER: &str = "aquifer";
const RECEIPT_VERSION: u32 = 1;

/// How a Door43 commit was downloaded, which decides its file layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Packaging {
    /// `/<owner>/<repo>/sb/<tag>.zip`: the burrito Door43 builds from the tag's commit,
    /// checked against its declared revision and ingredient checksums. Door43 builds it
    /// on each request, so two downloads of one pin can differ (D96).
    BurritoExport,
    /// `/archive/<sha>.zip`: the commit's own files, checked against its git tree, so the
    /// same sha always gives the same bytes.
    Archive,
}

/// Where an install came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "camelCase")]
pub enum Origin {
    #[serde(rename_all = "camelCase")]
    Dcs {
        /// The pin, with the flavor the resource declares when the pin named none.
        pin: ResourcePin,
        packaging: Packaging,
        /// Files the download held that nothing verified (an export's files its metadata
        /// does not list, a symbolic link in a commit): not installed.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        left_out: Vec<String>,
        /// Ingredients the export's metadata lists that it did not hold.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        missing_ingredients: Vec<String>,
    },
    #[serde(rename_all = "camelCase")]
    Aquifer {
        pin: AquiferPin,
        /// Articles the converter refused, each alone; the rest of their files load.
        #[serde(default)]
        refused_articles: usize,
    },
}

impl Origin {
    fn describe(&self) -> String {
        match self {
            Origin::Dcs { pin, .. } => pin.to_string(),
            Origin::Aquifer { pin, .. } => pin.to_string(),
        }
    }

    /// Whether two origins name the same install: the same Door43 commit, or the same
    /// Aquifer asset.
    fn same_identity(&self, other: &Origin) -> bool {
        match (self, other) {
            (Origin::Dcs { pin: a, .. }, Origin::Dcs { pin: b, .. }) => a.same_identity(b),
            (Origin::Aquifer { pin: a, .. }, Origin::Aquifer { pin: b, .. }) => {
                a.repo == b.repo
                    && a.commit == b.commit
                    && a.language == b.language
                    && a.sha256 == b.sha256
            }
            _ => false,
        }
    }
}

/// One installed file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRecord {
    pub path: String,
    pub size: u64,
    pub sha256: Sha256,
}

/// `receipt.json`: what an install is and what it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub receipt_version: u32,
    #[serde(flatten)]
    pub origin: Origin,
    /// The books the installed files hold; `["BIBLE"]` for a resource not divided into
    /// books. Absent when a book-divided resource holds no book.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub books: Option<BookCoverage>,
    /// Sorted by path.
    pub files: Vec<FileRecord>,
}

/// An install the store holds.
#[derive(Debug, Clone)]
pub struct Installed {
    dir: PathBuf,
    receipt: Receipt,
}

impl Installed {
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn receipt(&self) -> &Receipt {
        &self.receipt
    }

    pub fn origin(&self) -> &Origin {
        &self.receipt.origin
    }

    /// The Door43 pin, with the resource's flavor and the books it holds.
    pub fn dcs_pin(&self) -> Option<ResourcePin> {
        match &self.receipt.origin {
            Origin::Dcs { pin, .. } => {
                let mut pin = pin.clone();
                if pin.books.is_none() {
                    pin.books = self.receipt.books.clone();
                }
                Some(pin)
            }
            Origin::Aquifer { .. } => None,
        }
    }

    pub fn aquifer_pin(&self) -> Option<&AquiferPin> {
        match &self.receipt.origin {
            Origin::Aquifer { pin, .. } => Some(pin),
            Origin::Dcs { .. } => None,
        }
    }

    pub fn books(&self) -> Option<&BookCoverage> {
        self.receipt.books.as_ref()
    }

    /// Every file, sorted by path.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.receipt.files.iter().map(|f| f.path.as_str())
    }

    pub fn has_file(&self, path: &str) -> bool {
        self.record(path).is_some()
    }

    fn record(&self, path: &str) -> Option<&FileRecord> {
        self.receipt
            .files
            .binary_search_by(|f| f.path.as_str().cmp(path))
            .ok()
            .map(|i| &self.receipt.files[i])
    }

    /// A file's bytes. Refused when the receipt does not list the file, or when its size
    /// differs from the receipt's ([`Installed::verify`] checks every byte).
    pub fn read(&self, path: &str) -> Result<Vec<u8>, StoreError> {
        let record = self.record(path).ok_or_else(|| StoreError::NoSuchFile {
            install: self.receipt.origin.describe(),
            file: path.to_owned(),
        })?;
        let full = self.dir.join(CONTENT).join(path);
        let bytes = fs::read(&full).map_err(|e| StoreError::io("read", &full, e))?;
        if bytes.len() as u64 != record.size {
            return Err(StoreError::Damaged {
                path: self.dir.clone(),
                file: path.to_owned(),
                problem: format!("{} bytes, the receipt says {}", bytes.len(), record.size),
            });
        }
        Ok(bytes)
    }

    /// A file as UTF-8 text.
    pub fn read_text(&self, path: &str) -> Result<String, StoreError> {
        String::from_utf8(self.read(path)?).map_err(|_| StoreError::Damaged {
            path: self.dir.clone(),
            file: path.to_owned(),
            problem: "it is not UTF-8 text".to_owned(),
        })
    }

    /// Checks every file against the receipt's size and SHA-256, and that the content
    /// folder holds nothing else.
    pub fn verify(&self) -> Result<(), StoreError> {
        let damaged = |file: &str, problem: String| StoreError::Damaged {
            path: self.dir.clone(),
            file: file.to_owned(),
            problem,
        };
        for record in &self.receipt.files {
            let full = self.dir.join(CONTENT).join(&record.path);
            let bytes = fs::read(&full).map_err(|e| damaged(&record.path, e.to_string()))?;
            if bytes.len() as u64 != record.size || Sha256::of(&bytes) != record.sha256 {
                return Err(damaged(&record.path, "its bytes changed".to_owned()));
            }
        }
        let mut found = Vec::new();
        list_files(&self.dir.join(CONTENT), "", &mut found)?;
        for path in found {
            if self.record(&path).is_none() {
                return Err(damaged(&path, "the receipt does not list it".to_owned()));
            }
        }
        Ok(())
    }
}

fn list_files(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<(), StoreError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(StoreError::io("list", dir, e)),
    };
    for entry in entries {
        let entry = entry.map_err(|e| StoreError::io("list", dir, e))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let kind = entry
            .file_type()
            .map_err(|e| StoreError::io("inspect", entry.path(), e))?;
        if kind.is_dir() {
            list_files(&entry.path(), &path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// What [`ResourceStore::seed_from`] did with each install of a bundled store.
#[derive(Debug, Default)]
pub struct SeedReport {
    pub added: Vec<String>,
    pub already_installed: Vec<String>,
    /// Bundled installs whose files no longer match their receipts: not copied.
    pub refused: Vec<(String, StoreError)>,
}

/// Points an install can stop at, for crash tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// After this many files are written and synced.
    FileWritten(usize),
    ReceiptWritten,
    Renamed,
}

/// Files and a receipt ready to install. The receipt's file list is computed here.
pub(crate) struct Staged {
    pub origin: Origin,
    pub books: Option<BookCoverage>,
    pub files: Vec<(String, Vec<u8>)>,
}

static NEXT_STAGING: AtomicU64 = AtomicU64::new(0);

/// The resource store of one installation. See the module documentation.
#[derive(Debug, Clone)]
pub struct ResourceStore {
    root: PathBuf,
}

impl ResourceStore {
    /// Opens the store at `root`, creating it when missing, and removes whatever an
    /// interrupted install left behind.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let store = Self { root: root.into() };
        fs::create_dir_all(&store.root).map_err(|e| StoreError::io("create", &store.root, e))?;
        let partial = store.root.join(PARTIAL);
        if partial.exists() {
            fs::remove_dir_all(&partial).map_err(|e| StoreError::io("clear", &partial, e))?;
        }
        Ok(store)
    }

    /// Opens a store only to read it, such as the bundled store an installer ships in a
    /// read-only folder. Nothing is created or removed.
    pub fn open_read_only(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        if !root.is_dir() {
            return Err(StoreError::io(
                "open",
                &root,
                std::io::Error::from(std::io::ErrorKind::NotFound),
            ));
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn dcs_dir(&self, pin: &ResourcePin) -> PathBuf {
        self.root
            .join(DCS)
            .join(pin.repo.owner().as_str().to_ascii_lowercase())
            .join(pin.repo.name().as_str().to_ascii_lowercase())
            .join(pin.sha.as_str())
    }

    fn aquifer_dir(&self, pin: &AquiferPin) -> PathBuf {
        let digest = pin.sha256.to_hex();
        self.root
            .join(AQUIFER)
            .join(pin.repo.owner.as_str().to_ascii_lowercase())
            .join(pin.repo.name.as_str().to_ascii_lowercase())
            .join(pin.commit.as_str())
            .join(format!("{}-{}", pin.language, &digest[..16]))
    }

    fn dir_of(&self, origin: &Origin) -> PathBuf {
        match origin {
            Origin::Dcs { pin, .. } => self.dcs_dir(pin),
            Origin::Aquifer { pin, .. } => self.aquifer_dir(pin),
        }
    }

    /// The install of this Door43 commit (repository and sha; the label is not compared).
    pub fn dcs(&self, pin: &ResourcePin) -> Result<Option<Installed>, StoreError> {
        let wanted = Origin::Dcs {
            pin: pin.clone(),
            packaging: Packaging::Archive,
            left_out: Vec::new(),
            missing_ingredients: Vec::new(),
        };
        self.find(&wanted)
    }

    /// The install of this Aquifer asset.
    pub fn aquifer(&self, pin: &AquiferPin) -> Result<Option<Installed>, StoreError> {
        let wanted = Origin::Aquifer {
            pin: pin.clone(),
            refused_articles: 0,
        };
        self.find(&wanted)
    }

    /// Whether this exact Door43 commit is installed and readable.
    pub fn has_dcs(&self, pin: &ResourcePin) -> bool {
        matches!(self.dcs(pin), Ok(Some(_)))
    }

    fn find(&self, wanted: &Origin) -> Result<Option<Installed>, StoreError> {
        let dir = self.dir_of(wanted);
        if !dir.exists() {
            return Ok(None);
        }
        let installed = read_install(&dir)?;
        if installed.receipt.origin.same_identity(wanted) {
            Ok(Some(installed))
        } else {
            Err(StoreError::Occupied {
                path: dir,
                holds: installed.receipt.origin.describe(),
            })
        }
    }

    /// Every install, sorted by folder.
    pub fn installed(&self) -> Result<Vec<Installed>, StoreError> {
        let mut out = Vec::new();
        for (source, depth) in [(DCS, 3), (AQUIFER, 4)] {
            let mut dirs = Vec::new();
            dirs_at_depth(&self.root.join(source), depth, &mut dirs)?;
            dirs.sort();
            for dir in dirs {
                out.push(read_install(&dir)?);
            }
        }
        Ok(out)
    }

    /// Every installed Door43 commit as a pin, with its flavor and the books it holds:
    /// the input [`crate::gateways::language_set_from_installed`] builds sets from.
    pub fn installed_dcs_pins(&self) -> Result<Vec<ResourcePin>, StoreError> {
        Ok(self
            .installed()?
            .iter()
            .filter_map(Installed::dcs_pin)
            .collect())
    }

    /// Copies into this store every install of `bundled` that it lacks, checking each
    /// file against the bundled receipt first. An installer ships the bundled store, so a
    /// first run works offline; seeding again after an update adds the new releases and
    /// keeps the old ones, which projects may still pin.
    pub fn seed_from(&self, bundled: &ResourceStore) -> Result<SeedReport, StoreError> {
        let mut report = SeedReport::default();
        for install in bundled.installed()? {
            let name = install.origin().describe();
            if self.find(install.origin())?.is_some() {
                report.already_installed.push(name);
                continue;
            }
            if let Err(e) = install.verify() {
                report.refused.push((name, e));
                continue;
            }
            let mut files = Vec::new();
            for record in &install.receipt.files {
                files.push((record.path.clone(), install.read(&record.path)?));
            }
            let staged = Staged {
                origin: install.receipt.origin.clone(),
                books: install.receipt.books.clone(),
                files,
            };
            match self.install(staged) {
                Ok(_) => report.added.push(name),
                Err(InstallError::Store(e)) => return Err(e),
                Err(other) => report.refused.push((
                    name,
                    StoreError::BadReceipt {
                        path: install.dir.clone(),
                        problem: other.to_string(),
                    },
                )),
            }
        }
        Ok(report)
    }

    pub(crate) fn install(&self, staged: Staged) -> Result<Installed, InstallError> {
        self.install_until(staged, &mut |_| false)
    }

    /// Installs `staged`, stopping as if the process died when `crash` answers true at a
    /// step.
    pub(crate) fn install_until(
        &self,
        staged: Staged,
        crash: &mut dyn FnMut(Step) -> bool,
    ) -> Result<Installed, InstallError> {
        let target = self.dir_of(&staged.origin);
        if let Some(existing) = self.find(&staged.origin)? {
            return Ok(existing);
        }
        let mut files = staged.files;
        files.sort_by(|a, b| a.0.cmp(&b.0));
        check_paths(&files).map_err(|refusal| InstallError::Refused {
            what: staged.origin.describe(),
            refusal,
        })?;
        let receipt = Receipt {
            receipt_version: RECEIPT_VERSION,
            origin: staged.origin,
            books: staged.books,
            files: files
                .iter()
                .map(|(path, bytes)| FileRecord {
                    path: path.clone(),
                    size: bytes.len() as u64,
                    sha256: Sha256::of(bytes),
                })
                .collect(),
        };

        let partial = self.root.join(PARTIAL);
        create_dirs(&partial)?;
        let staging = partial.join(staging_name());
        create_dirs(&staging.join(CONTENT))?;
        let mut folders = BTreeSet::new();
        for (written, (path, bytes)) in files.iter().enumerate() {
            let full = staging.join(CONTENT).join(path);
            if let Some(parent) = full.parent() {
                create_dirs(parent)?;
                folders.insert(parent.to_path_buf());
            }
            write_synced(&full, bytes)?;
            if crash(Step::FileWritten(written + 1)) {
                return Err(InstallError::Interrupted);
            }
        }
        let receipt_bytes =
            serde_json::to_vec_pretty(&receipt).map_err(|e| StoreError::BadReceipt {
                path: staging.clone(),
                problem: e.to_string(),
            })?;
        write_synced(&staging.join(RECEIPT), &receipt_bytes)?;
        folders.insert(staging.join(CONTENT));
        // Deepest first, so each folder's entry is durable before its parent's.
        for folder in folders.iter().rev() {
            sync_dir(folder)?;
        }
        sync_dir(&staging)?;
        if crash(Step::ReceiptWritten) {
            return Err(InstallError::Interrupted);
        }

        let parent = target.parent().unwrap_or(&self.root);
        create_dirs(parent)?;
        if let Err(e) = fs::rename(&staging, &target) {
            // Another install of the same identity may have landed first.
            if let Some(existing) = self.find(&receipt.origin)? {
                let _ = fs::remove_dir_all(&staging);
                return Ok(existing);
            }
            return Err(StoreError::io("move into place", &target, e).into());
        }
        if crash(Step::Renamed) {
            return Err(InstallError::Interrupted);
        }
        sync_dir(parent)?;
        sync_dir(&partial)?;
        Ok(Installed {
            dir: target,
            receipt,
        })
    }
}

impl LocalCoverage for ResourceStore {
    /// The books of the exact installed commit. A store that cannot be read answers
    /// unknown, which the resolver treats as "not installed".
    fn local_books(&self, pin: &ResourcePin) -> Option<BookCoverage> {
        self.dcs(pin).ok().flatten()?.books().cloned()
    }
}

fn staging_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "{}-{nanos}-{}",
        std::process::id(),
        NEXT_STAGING.fetch_add(1, Ordering::Relaxed)
    )
}

fn read_install(dir: &Path) -> Result<Installed, StoreError> {
    let path = dir.join(RECEIPT);
    let bytes = fs::read(&path).map_err(|e| StoreError::BadReceipt {
        path: dir.to_path_buf(),
        problem: e.to_string(),
    })?;
    let receipt: Receipt = serde_json::from_slice(&bytes).map_err(|e| StoreError::BadReceipt {
        path: dir.to_path_buf(),
        problem: e.to_string(),
    })?;
    if receipt.receipt_version != RECEIPT_VERSION {
        return Err(StoreError::BadReceipt {
            path: dir.to_path_buf(),
            problem: format!("receipt version {}", receipt.receipt_version),
        });
    }
    if receipt.files.windows(2).any(|w| w[0].path >= w[1].path) {
        return Err(StoreError::BadReceipt {
            path: dir.to_path_buf(),
            problem: "the file list is not sorted".to_owned(),
        });
    }
    Ok(Installed {
        dir: dir.to_path_buf(),
        receipt,
    })
}

fn dirs_at_depth(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) -> Result<(), StoreError> {
    if depth == 0 {
        out.push(dir.to_path_buf());
        return Ok(());
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(StoreError::io("list", dir, e)),
    };
    for entry in entries {
        let entry = entry.map_err(|e| StoreError::io("list", dir, e))?;
        if entry.path().is_dir() {
            dirs_at_depth(&entry.path(), depth - 1, out)?;
        }
    }
    Ok(())
}

/// Refuses a path that could leave the content folder or that a case-insensitive file
/// system would merge with another. Downloads were checked already; this guards every
/// write, whatever its source.
fn check_paths(files: &[(String, Vec<u8>)]) -> Result<(), Refusal> {
    let mut seen = BTreeSet::new();
    for (path, _) in files {
        let bad = path.is_empty()
            || path.contains(['\\', ':', '\0'])
            || path
                .split('/')
                .any(|segment| segment.is_empty() || segment.chars().all(|c| c == '.'));
        if bad {
            return Err(Refusal::EscapesRoot(path.clone()));
        }
        if path == RECEIPT {
            continue;
        }
        if !seen.insert(path.to_lowercase()) {
            return Err(Refusal::DuplicateEntry(path.clone()));
        }
    }
    Ok(())
}

fn create_dirs(dir: &Path) -> Result<(), StoreError> {
    fs::create_dir_all(dir).map_err(|e| StoreError::io("create", dir, e))
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let mut file = fs::File::create(path).map_err(|e| StoreError::io("create", path, e))?;
    file.write_all(bytes)
        .map_err(|e| StoreError::io("write", path, e))?;
    file.sync_all().map_err(|e| StoreError::io("sync", path, e))
}

#[cfg(unix)]
fn sync_dir(dir: &Path) -> Result<(), StoreError> {
    fs::File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| StoreError::io("sync", dir, e))
}

/// Windows cannot open a folder to sync it; a rename there is durable once the call
/// returns on NTFS.
#[cfg(not(unix))]
fn sync_dir(_: &Path) -> Result<(), StoreError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(sha: char) -> ResourcePin {
        ResourcePin::new(
            "git.door43.org/Es-419_gl/es-419_tn".parse().unwrap(),
            sha.to_string().repeat(40).parse().unwrap(),
        )
        .with_version("v66".parse().unwrap())
    }

    fn staged(sha: char) -> Staged {
        Staged {
            origin: Origin::Dcs {
                pin: pin(sha),
                packaging: Packaging::BurritoExport,
                left_out: Vec::new(),
                missing_ingredients: Vec::new(),
            },
            books: BookCoverage::books([tc_core::BookId::Titus]),
            files: vec![
                ("metadata.json".to_owned(), b"{}".to_vec()),
                (
                    "ingredients/TIT.tsv".to_owned(),
                    b"Reference\tID\n".to_vec(),
                ),
                ("ingredients/LICENSE.md".to_owned(), b"CC BY-SA".to_vec()),
            ],
        }
    }

    #[test]
    fn an_install_is_found_by_identity_whatever_the_case_or_label() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResourceStore::open(dir.path()).unwrap();
        assert!(store.dcs(&pin('a')).unwrap().is_none());
        let installed = store.install(staged('a')).unwrap();
        installed.verify().unwrap();
        let mut other_label = pin('a');
        other_label.version = None;
        other_label.repo = "git.door43.org/es-419_GL/ES-419_TN".parse().unwrap();
        let found = store.dcs(&other_label).unwrap().unwrap();
        assert_eq!(
            found.read_text("ingredients/TIT.tsv").unwrap(),
            "Reference\tID\n"
        );
        assert!(found.read("ingredients/JON.tsv").is_err());
        assert_eq!(
            found.dcs_pin().unwrap().books,
            BookCoverage::books([tc_core::BookId::Titus])
        );
        assert!(store.dcs(&pin('b')).unwrap().is_none());
        assert_eq!(store.installed().unwrap().len(), 1);
    }

    #[test]
    fn a_second_install_of_one_identity_leaves_the_first_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResourceStore::open(dir.path()).unwrap();
        store.install(staged('a')).unwrap();
        let mut changed = staged('a');
        changed.files[0].1 = b"{\"other\":true}".to_vec();
        let again = store.install(changed).unwrap();
        assert_eq!(again.read("metadata.json").unwrap(), b"{}");
    }

    #[test]
    fn a_crash_at_any_step_leaves_no_install_or_the_whole_one() {
        let steps = [
            Step::FileWritten(1),
            Step::FileWritten(2),
            Step::FileWritten(3),
            Step::ReceiptWritten,
            Step::Renamed,
        ];
        for stop in steps {
            let dir = tempfile::tempdir().unwrap();
            let store = ResourceStore::open(dir.path()).unwrap();
            let result = store.install_until(staged('a'), &mut |step| step == stop);
            assert!(matches!(result, Err(InstallError::Interrupted)), "{stop:?}");
            let reopened = ResourceStore::open(dir.path()).unwrap();
            assert!(!dir.path().join(PARTIAL).exists());
            match reopened.dcs(&pin('a')).unwrap() {
                None => assert_ne!(stop, Step::Renamed, "a renamed install must survive"),
                Some(installed) => {
                    assert_eq!(stop, Step::Renamed, "only a renamed install is visible");
                    installed.verify().unwrap();
                }
            }
            // The interrupted install can be done again.
            reopened.install(staged('a')).unwrap().verify().unwrap();
        }
    }

    #[test]
    fn paths_that_escape_or_collide_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResourceStore::open(dir.path()).unwrap();
        for bad in [
            "../x",
            "a/../../x",
            "/abs",
            "C:x",
            "a\\b",
            "a//b",
            "...",
            "",
        ] {
            let mut s = staged('a');
            s.files.push((bad.to_owned(), Vec::new()));
            assert!(
                matches!(store.install(s), Err(InstallError::Refused { .. })),
                "{bad:?}"
            );
        }
        let mut s = staged('a');
        s.files.push(("METADATA.json".to_owned(), Vec::new()));
        assert!(matches!(
            store.install(s),
            Err(InstallError::Refused { .. })
        ));
        assert!(store.installed().unwrap().is_empty());
    }

    #[test]
    fn verify_finds_changed_and_extra_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResourceStore::open(dir.path()).unwrap();
        let installed = store.install(staged('a')).unwrap();
        fs::write(installed.dir().join(CONTENT).join("extra.txt"), b"x").unwrap();
        assert!(matches!(
            installed.verify(),
            Err(StoreError::Damaged { .. })
        ));
        fs::remove_file(installed.dir().join(CONTENT).join("extra.txt")).unwrap();
        fs::write(installed.dir().join(CONTENT).join("metadata.json"), b"[]").unwrap();
        assert!(matches!(
            installed.verify(),
            Err(StoreError::Damaged { .. })
        ));
    }

    #[test]
    fn seeding_copies_what_is_missing_and_refuses_damaged_installs() {
        let bundled_dir = tempfile::tempdir().unwrap();
        let bundled = ResourceStore::open(bundled_dir.path()).unwrap();
        bundled.install(staged('a')).unwrap();
        let damaged = bundled.install(staged('b')).unwrap();
        fs::write(damaged.dir().join(CONTENT).join("metadata.json"), b"[]").unwrap();

        let machine_dir = tempfile::tempdir().unwrap();
        let machine = ResourceStore::open(machine_dir.path()).unwrap();
        let read_only = ResourceStore::open_read_only(bundled_dir.path()).unwrap();
        let report = machine.seed_from(&read_only).unwrap();
        assert_eq!(report.added.len(), 1);
        assert_eq!(report.refused.len(), 1);
        machine.dcs(&pin('a')).unwrap().unwrap().verify().unwrap();
        assert!(machine.dcs(&pin('b')).unwrap().is_none());
        let again = machine.seed_from(&read_only).unwrap();
        assert_eq!(again.already_installed.len(), 1);
        assert!(again.added.is_empty());
    }
}
