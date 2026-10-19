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
//!     checked.json                                 the SHA-256 of the receipt every file was last checked against
//!     content/…                                    the resource's files
//!   .partial/                                      installs and downloads in progress
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
//! [`ResourceStore::open`] removes what a crash left in `.partial/`. On Windows, a leftover
//! that another program (a virus scanner, the search indexer) still holds open stays for
//! the next open. An existing install's content and receipt are never written again: a
//! second install of the same identity finds it and stops.
//!
//! # Checked once
//!
//! An install's files are checked when it is made, and `checked.json` records the receipt
//! they were checked against. A later start trusts the copy while that record names its
//! receipt, and opens none of its files (tC4 #740). A copy whose receipt changed, or whose
//! record is missing or unreadable, is checked again against its own receipt, and its
//! record is replaced atomically. A copy that fails that check stops the seeding, as it
//! stops tC4's start, and is left as it is. Like tC4, this does not see a change to a
//! content file made after its check: a reader that finds a file whose size differs from
//! the receipt's refuses it ([`Installed::read`]).
//!
//! One process owns a store: the app's server. Two processes installing into one store
//! at once is not supported, because opening the store clears `.partial/`.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tc_core::BookId;
use tc_door43::check_names;

use crate::error::{InstallError, Refusal, StoreError};
use crate::held;
use crate::pin::{AquiferPin, BookCoverage, ResourcePin, Sha256};
use crate::resolve::LocalCoverage;

const RECEIPT: &str = "receipt.json";
const CHECKED: &str = "checked.json";
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
    /// A lexicon's archive, checked as [`Packaging::Archive`] is, then repackaged into one
    /// file, [`crate::LEXICON_FILE`] ([`crate::repackage_lexicon`]). Every lexicon is
    /// installed this way (owner ruling, 2026-10-15).
    Lexicon,
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
        /// For notes and questions: the article files that hold each book's items, by
        /// the passage each is placed at, whatever file it is filed in. Empty for words
        /// and academy, which are read whole.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        book_files: BTreeMap<BookId, Vec<String>>,
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
    /// The books the install holds: a Door43 resource's book files (a `_tw` repository's
    /// articles serve every book), or the passages of an Aquifer asset's notes or
    /// questions, whatever file holds them. `["BIBLE"]` for a resource not divided into
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
    /// Of the receipt's bytes as they were read or written: what `checked.json` names.
    receipt_sha256: Sha256,
}

/// `checked.json`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckRecord {
    receipt_sha256: Sha256,
}

thread_local! {
    static CONTENT_FILES_OPENED: Cell<u64> = const { Cell::new(0) };
    static CONTENT_FILES_WRITTEN: Cell<u64> = const { Cell::new(0) };
    /// The paths of the content files this thread opens while [`with_opened_paths`] runs.
    static OPENED_PATHS: std::cell::RefCell<Option<Vec<std::path::PathBuf>>> =
        const { std::cell::RefCell::new(None) };
}

/// Runs `f` and returns, with its result, the path of every installed content file the
/// calling thread opened meanwhile, in the order opened (a file opened twice is listed twice).
/// What the loading and seeding tests assert: which files an action opened, not only how
/// many. Nothing is recorded outside this call.
pub fn with_opened_paths<T>(f: impl FnOnce() -> T) -> (T, Vec<std::path::PathBuf>) {
    OPENED_PATHS.with(|paths| *paths.borrow_mut() = Some(Vec::new()));
    let out = f();
    let paths = OPENED_PATHS
        .with(|paths| paths.borrow_mut().take())
        .unwrap_or_default();
    (out, paths)
}

/// How many installed content files this thread has opened since it started, in any
/// store: the operation count the start-up tests assert (docs/PERFORMANCE.md rule 2).
/// Receipts and check records are not content files.
pub fn content_files_opened() -> u64 {
    CONTENT_FILES_OPENED.with(Cell::get)
}

/// How many content files this thread has written into any store since it started: with
/// [`content_files_opened`], the counts the start-up tests assert (docs/PERFORMANCE.md
/// "Start-up"). Receipts and check records are not content files.
pub fn content_files_written() -> u64 {
    CONTENT_FILES_WRITTEN.with(Cell::get)
}

fn read_content(path: &Path) -> std::io::Result<Vec<u8>> {
    CONTENT_FILES_OPENED.with(|opened| opened.set(opened.get() + 1));
    OPENED_PATHS.with(|paths| {
        if let Some(paths) = paths.borrow_mut().as_mut() {
            paths.push(path.to_owned());
        }
    });
    fs::read(path)
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

    /// The article files of an Aquifer notes or questions asset that hold items of
    /// `book`, wherever the release filed them. `None` when the receipt keeps no such
    /// index (a Door43 install, or Aquifer words and academy, which are read whole).
    pub fn aquifer_book_files(&self, book: BookId) -> Option<&[String]> {
        match &self.receipt.origin {
            Origin::Aquifer { book_files, .. } if !book_files.is_empty() => {
                Some(book_files.get(&book).map_or(&[], Vec::as_slice))
            }
            _ => None,
        }
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
        let bytes = read_content(&full).map_err(|e| StoreError::io("read", &full, e))?;
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
        self.check_listing()?;
        self.check_listed()
    }

    /// Checks each file the receipt lists against its size and SHA-256, opening each
    /// once. Seeding checks only these, as tC4's `validateTree`
    /// (`scripts/desktop-bootstrap.cjs:164-176`) checks only its inventory's files, so a
    /// file manager's leftover such as `.DS_Store` never stops a start.
    fn check_listed(&self) -> Result<(), StoreError> {
        self.check_listed_watched(&NoWatch, &mut 0).map(|_| ())
    }

    /// [`Installed::check_listed`] that counts each file into `done` and tells `watch`, and
    /// stops with `Ok(false)` when `watch` asks, before the next file.
    fn check_listed_watched(
        &self,
        watch: &dyn SeedWatch,
        done: &mut u64,
    ) -> Result<bool, StoreError> {
        for record in &self.receipt.files {
            if watch.cancelled() {
                return Ok(false);
            }
            self.read_checked(record)?;
            *done += 1;
            watch.done(*done);
        }
        Ok(true)
    }

    /// The bytes of every file the receipt lists, checked as [`Installed::check_listed`]
    /// checks them, opening each file once.
    fn read_all_checked(&self) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
        self.receipt
            .files
            .iter()
            .map(|record| Ok((record.path.clone(), self.read_checked(record)?)))
            .collect()
    }

    fn read_checked(&self, record: &FileRecord) -> Result<Vec<u8>, StoreError> {
        let full = self.dir.join(CONTENT).join(&record.path);
        let bytes = read_content(&full).map_err(|e| self.damaged(&record.path, e.to_string()))?;
        if bytes.len() as u64 != record.size {
            return Err(self.damaged(
                &record.path,
                format!("{} bytes, the receipt says {}", bytes.len(), record.size),
            ));
        }
        if Sha256::of(&bytes) != record.sha256 {
            return Err(self.damaged(&record.path, "its SHA-256 is not the receipt's".to_owned()));
        }
        Ok(bytes)
    }

    /// That the content folder holds no file the receipt does not list.
    fn check_listing(&self) -> Result<(), StoreError> {
        let mut found = Vec::new();
        list_files(&self.dir.join(CONTENT), "", &mut found)?;
        match found.into_iter().find(|path| self.record(path).is_none()) {
            Some(path) => Err(self.damaged(&path, "the receipt does not list it".to_owned())),
            None => Ok(()),
        }
    }

    fn damaged(&self, file: &str, problem: String) -> StoreError {
        StoreError::Damaged {
            path: self.dir.clone(),
            file: file.to_owned(),
            problem,
        }
    }

    /// How a seeding reports this bundled install when copying it was refused.
    fn refused(&self, why: InstallError) -> StoreError {
        StoreError::BadReceipt {
            path: self.dir.clone(),
            problem: why.to_string(),
        }
    }

    /// Whether `checked.json` names this receipt. A missing or unreadable record is no
    /// check.
    fn is_checked(&self) -> bool {
        fs::read(self.dir.join(CHECKED))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<CheckRecord>(&bytes).ok())
            .is_some_and(|record| record.receipt_sha256 == self.receipt_sha256)
    }

    /// Records that every file was checked against this receipt.
    fn record_check(&self) -> Result<(), StoreError> {
        replace_synced(
            &self.dir,
            CHECKED,
            &check_record(&self.dir, self.receipt_sha256)?,
        )
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

/// What a store holds.
#[derive(Debug, Default)]
pub struct Inventory {
    pub installs: Vec<Installed>,
    /// Install folders whose receipt cannot be read: listed, never used or repaired.
    pub unreadable: Vec<StoreError>,
}

/// What [`ResourceStore::seed_from`] did with each install of a bundled store.
#[derive(Debug, Default)]
pub struct SeedReport {
    pub added: Vec<String>,
    pub already_installed: Vec<String>,
    /// Of `already_installed`: copies whose receipt was not the one their files were last
    /// checked against, checked again now against their own receipt.
    pub checked_again: Vec<String>,
    /// Bundled installs whose files no longer match their receipts: not copied.
    pub refused: Vec<(String, StoreError)>,
}

/// Who watches a seeding (APP-DESIGN §4.6): hears how many content files are done, and is
/// asked whether to stop.
pub trait SeedWatch {
    /// After each content file is checked or written: the files done so far. It only
    /// grows, and reaches [`SeedPlan::files`] when the seeding finishes.
    fn done(&self, files: u64);

    /// Whether to stop. Asked before each install is copied and before each of the files
    /// of a copy or of a check, once per file. Never asked between the last file of a copy
    /// and its receipt: a copy whose files are all written is finished.
    fn cancelled(&self) -> bool;
}

/// What seeding must do, found from receipts and check records alone.
#[derive(Debug, Default)]
pub struct SeedPlan {
    steps: Vec<SeedStep>,
    files: u64,
    /// The copies already checked and the unreadable bundled folders are known now.
    report: SeedReport,
}

#[derive(Debug)]
enum SeedStep {
    /// The store lacks this bundled install: read it from the bundle, check it, copy it.
    Copy { install: Installed, name: String },
    /// The store's copy was not checked against its present receipt: check it again.
    Check { copy: Installed, name: String },
}

impl SeedPlan {
    /// The content files seeding will open: each file of every install to copy and of
    /// every copy to check again.
    pub fn files(&self) -> u64 {
        self.files
    }

    /// Nothing to copy or check. The report may still list refused bundled folders.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// What is known before anything is done: the copies already trusted
    /// (`already_installed`), and in `refused` the bundled folders that could not be read
    /// and the bundled installs whose receipt names a path an install refuses.
    pub fn report(&self) -> &SeedReport {
        &self.report
    }
}

/// How a seeding ended.
#[derive(Debug)]
pub enum Seeding {
    Finished(SeedReport),
    /// The watch asked to stop. Every install in `added` is whole and checked. The copy
    /// being made is not in the store; its files stay under `.partial/`, and the next
    /// [`ResourceStore::open`] removes them, waiting while a scanner holds one. What a
    /// scanner still holds after the waits stays until a later open:
    /// [`ResourceStore::open`] keeps it, and [`ResourceStore::open_keeping_leftovers`],
    /// which the app uses, also returns it to be reported. A leftover never reaches an
    /// install. Seeding again copies what is missing.
    Cancelled(SeedReport),
}

/// The watch of [`ResourceStore::seed_from`]: it hears nothing and never stops.
struct NoWatch;

impl SeedWatch for NoWatch {
    fn done(&self, _: u64) {}

    fn cancelled(&self) -> bool {
        false
    }
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
    /// interrupted install left behind. On Windows, a leftover that another program still
    /// holds open stays for the next open: staging names are unique, so it never meets a
    /// new install.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        match Self::open_keeping_leftovers(root)? {
            (store, None) => Ok(store),
            // A scanner still holds the leftover after the waits: it stays for the next open.
            (store, Some(StoreError::Io { source, .. })) if held::is_held(&source) => Ok(store),
            (_, Some(left)) => Err(left),
        }
    }

    /// [`ResourceStore::open`] for the app, which must start although something an
    /// interrupted install left in `.partial/` cannot be removed (on Windows a scanner may
    /// hold one of its files). An install lands only by renaming a staging folder with a
    /// name of its own, so the leftovers cannot reach an install; they come back as the
    /// second value for the caller to report, and the next open tries again. Only a store
    /// folder that cannot be made is an error.
    pub fn open_keeping_leftovers(
        root: impl Into<PathBuf>,
    ) -> Result<(Self, Option<StoreError>), StoreError> {
        let store = Self { root: root.into() };
        fs::create_dir_all(&store.root).map_err(|e| StoreError::io("create", &store.root, e))?;
        let partial = store.root.join(PARTIAL);
        let left = if partial.exists() {
            held::remove_dir_all(&partial)
                .err()
                .map(|e| StoreError::io("clear", &partial, e))
        } else {
            None
        };
        Ok((store, left))
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

    /// Where downloads are written before they are checked and installed: `.partial/`,
    /// which [`ResourceStore::open`] empties, so a download a crash cut short is removed at
    /// the next start.
    pub(crate) fn download_folder(&self) -> PathBuf {
        self.root.join(PARTIAL)
    }

    fn dcs_dir(&self, pin: &ResourcePin) -> PathBuf {
        self.root
            .join(DCS)
            .join(folder_name(pin.repo.owner().as_str()))
            .join(folder_name(pin.repo.name().as_str()))
            .join(pin.sha.to_hex())
    }

    fn aquifer_dir(&self, pin: &AquiferPin) -> PathBuf {
        let digest = pin.sha256.to_hex();
        self.root
            .join(AQUIFER)
            .join(folder_name(pin.repo.owner.as_str()))
            .join(folder_name(pin.repo.name.as_str()))
            .join(pin.commit.to_hex())
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
            book_files: BTreeMap::new(),
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

    /// Every install, sorted by folder, and every install folder that cannot be read,
    /// with why. One damaged folder never hides the others.
    pub fn installed(&self) -> Result<Inventory, StoreError> {
        let mut out = Inventory::default();
        for (source, depth) in [(DCS, 3), (AQUIFER, 4)] {
            let mut dirs = Vec::new();
            dirs_at_depth(&self.root.join(source), depth, &mut dirs)?;
            dirs.sort();
            for dir in dirs {
                match read_install(&dir) {
                    Ok(install) => out.installs.push(install),
                    Err(e) => out.unreadable.push(e),
                }
            }
        }
        Ok(out)
    }

    /// Every installed Door43 commit as a pin, with its flavor and the books it holds:
    /// the input [`crate::gateways::language_set_from_installed`] builds sets from.
    pub fn installed_dcs_pins(&self) -> Result<Vec<ResourcePin>, StoreError> {
        Ok(self
            .installed()?
            .installs
            .iter()
            .filter_map(Installed::dcs_pin)
            .collect())
    }

    /// Copies into this store every install of `bundled` that it lacks, checking each
    /// file against the bundled receipt first. An installer ships the bundled store, so a
    /// first run works offline; seeding again after an update adds the new releases and
    /// keeps the old ones, which projects may still pin. Seeding checks and copies only
    /// the files a receipt lists: another file in a bundled install or a copy, such as a
    /// file manager's `.DS_Store`, is left where it is and stops nothing, as in tC4.
    ///
    /// The app seeds at every start, so a copy this store already holds is not read
    /// again while its check record names its receipt: a repeat start reads receipts and
    /// check records only. A copy whose receipt changed since its check is checked again
    /// against its own receipt, never the bundle's (tC4 #740).
    ///
    /// A copy of a bundled install that cannot be trusted stops the seeding with its error,
    /// as tC4 stops its start (`scripts/desktop-resources.test.cjs:453-463, 586-589`): a
    /// copy whose files fail that check, or whose receipt cannot be read or names another
    /// resource. The copy is left as it is, never overwritten from the bundle.
    ///
    /// [`ResourceStore::seed_plan`] and [`ResourceStore::seed`] are the same seeding in two
    /// steps, with progress and a stop.
    pub fn seed_from(&self, bundled: &ResourceStore) -> Result<SeedReport, StoreError> {
        match self.seed(self.seed_plan(bundled)?, &NoWatch)? {
            Seeding::Finished(report) | Seeding::Cancelled(report) => Ok(report),
        }
    }

    /// What [`ResourceStore::seed`] would do, found without opening a content file: a
    /// repeat start reads receipts and check records only (tC4 #740). An install this
    /// store holds whose receipt cannot be read or names another resource stops the plan
    /// with its error, before anything is copied. A bundled install to copy whose receipt
    /// names a path an install refuses is refused here, as the install would refuse it,
    /// before any of its files is opened: on Windows a device name would open the device,
    /// and a `../` path would read a file outside the bundle.
    pub fn seed_plan(&self, bundled: &ResourceStore) -> Result<SeedPlan, StoreError> {
        let inventory = bundled.installed()?;
        let mut plan = SeedPlan::default();
        for e in inventory.unreadable {
            plan.report
                .refused
                .push(("an unreadable bundled install".to_owned(), e));
        }
        for install in inventory.installs {
            let name = install.origin().describe();
            match self.find(install.origin())? {
                Some(copy) if copy.is_checked() => plan.report.already_installed.push(name),
                Some(copy) => {
                    plan.files += copy.receipt.files.len() as u64;
                    plan.steps.push(SeedStep::Check { copy, name });
                }
                None => match check_paths(install.files()) {
                    Ok(()) => {
                        plan.files += install.receipt.files.len() as u64;
                        plan.steps.push(SeedStep::Copy { install, name });
                    }
                    Err(refusal) => {
                        let refused = install.refused(InstallError::Refused {
                            what: name.clone(),
                            refusal,
                        });
                        plan.report.refused.push((name, refused));
                    }
                },
            }
        }
        Ok(plan)
    }

    /// Does what `plan` says, telling `watch` after each content file and stopping when it
    /// asks. Errors as [`ResourceStore::seed_from`] errs: a copy that cannot be trusted
    /// stops the seeding with its error, and a bundled install that fails its own check is
    /// left out and named in the report's `refused`. Install by install, a copy is in the
    /// store whole and checked or not at all, so a stop at any point leaves only whole
    /// installs.
    pub fn seed(&self, plan: SeedPlan, watch: &dyn SeedWatch) -> Result<Seeding, StoreError> {
        let SeedPlan {
            steps, mut report, ..
        } = plan;
        let mut done = 0u64;
        for step in steps {
            match step {
                SeedStep::Check { copy, name } => {
                    if !copy.check_listed_watched(watch, &mut done)? {
                        return Ok(Seeding::Cancelled(report));
                    }
                    copy.record_check()?;
                    report.checked_again.push(name.clone());
                    report.already_installed.push(name);
                }
                SeedStep::Copy { install, name } => {
                    if watch.cancelled() {
                        return Ok(Seeding::Cancelled(report));
                    }
                    let count = install.receipt.files.len() as u64;
                    let base = done;
                    done = base + count;
                    let files = match install.read_all_checked() {
                        Ok(files) => files,
                        Err(e) => {
                            report.refused.push((name, e));
                            watch.done(done);
                            continue;
                        }
                    };
                    let staged = Staged {
                        origin: install.receipt.origin.clone(),
                        books: install.receipt.books.clone(),
                        files,
                    };
                    let mut reported = base;
                    let mut stopped = false;
                    let installed = self.install_until(staged, &mut |step| {
                        let Step::FileWritten(n) = step else {
                            return false;
                        };
                        reported = base + n as u64;
                        watch.done(reported);
                        // The last file is never a place to stop: a copy with all its
                        // files written is finished.
                        stopped = reported < done && watch.cancelled();
                        stopped
                    });
                    if stopped {
                        return Ok(Seeding::Cancelled(report));
                    }
                    if reported < done {
                        // Nothing was written: the copy was refused, or landed already.
                        watch.done(done);
                    }
                    match installed {
                        Ok(_) => report.added.push(name),
                        Err(InstallError::Store(e)) => return Err(e),
                        Err(other) => report.refused.push((name, install.refused(other))),
                    }
                }
            }
        }
        Ok(Seeding::Finished(report))
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
        self.install_in(staged, &staging_name(), crash)
    }

    /// [`ResourceStore::install_until`] writing into the staging folder `staging_name`,
    /// which must be new.
    fn install_in(
        &self,
        staged: Staged,
        staging_name: &str,
        crash: &mut dyn FnMut(Step) -> bool,
    ) -> Result<Installed, InstallError> {
        let target = self.dir_of(&staged.origin);
        if let Some(existing) = self.find(&staged.origin)? {
            return Ok(existing);
        }
        let mut files = staged.files;
        files.sort_by(|a, b| a.0.cmp(&b.0));
        check_paths(files.iter().map(|(path, _)| path.as_str())).map_err(|refusal| {
            InstallError::Refused {
                what: staged.origin.describe(),
                refusal,
            }
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
        let staging = partial.join(staging_name);
        // Not `create_dirs`: a staging name already taken must fail here, before any file
        // is written into the other install's folder.
        fs::create_dir(&staging).map_err(|e| StoreError::io("create", &staging, e))?;
        create_dirs(&staging.join(CONTENT))?;
        let mut folders = BTreeSet::new();
        for (written, (path, bytes)) in files.iter().enumerate() {
            let full = staging.join(CONTENT).join(path);
            if let Some(parent) = full.parent() {
                create_dirs(parent)?;
                folders.insert(parent.to_path_buf());
            }
            match write_new_synced(&full, bytes) {
                Err(StoreError::Io { source, .. })
                    if source.kind() == io::ErrorKind::AlreadyExists =>
                {
                    let _ = held::remove_dir_all(&staging);
                    return Err(InstallError::Refused {
                        what: receipt.origin.describe(),
                        refusal: Refusal::DuplicateEntry(path.clone()),
                    });
                }
                result => result?,
            }
            CONTENT_FILES_WRITTEN.with(|count| count.set(count.get() + 1));
            if crash(Step::FileWritten(written + 1)) {
                return Err(InstallError::Interrupted);
            }
        }
        let receipt_bytes =
            serde_json::to_vec_pretty(&receipt).map_err(|e| StoreError::BadReceipt {
                path: staging.clone(),
                problem: e.to_string(),
            })?;
        write_new_synced(&staging.join(RECEIPT), &receipt_bytes)?;
        // The files were checked before they were staged, and are written from the
        // checked bytes.
        let receipt_sha256 = Sha256::of(&receipt_bytes);
        write_new_synced(
            &staging.join(CHECKED),
            &check_record(&staging, receipt_sha256)?,
        )?;
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
        let renamed = held::retry_while_held(|| match held::try_rename(&staging, &target) {
            // Some Windows versions answer a rename onto a folder that exists with "access
            // denied", which looks like a file held open. Waiting would not help.
            Err(e) if target.exists() => Err(io::Error::new(io::ErrorKind::AlreadyExists, e)),
            result => result,
        });
        if let Err(e) = renamed {
            // Another install of the same identity may have landed first.
            if let Some(existing) = self.find(&receipt.origin)? {
                let _ = held::try_remove_dir_all(&staging);
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
            receipt_sha256,
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

/// The folder of an account or repository name: lower-cased, because Door43 and GitHub
/// resolve names without regard to case. Both allow names that Windows would change or
/// refuse as a folder (a leading or trailing dot, `con`, `nul.x`), and those take a `_`
/// prefix. Two names that end up in one folder are told apart by the receipt, which the
/// store reads before it uses an install.
fn folder_name(name: &str) -> String {
    const DEVICES: [&str; 22] = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    let lower = name.to_ascii_lowercase();
    let stem = lower.split('.').next().unwrap_or_default();
    if lower.starts_with('.') || lower.ends_with('.') || DEVICES.contains(&stem) {
        format!("_{lower}")
    } else {
        lower
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
        receipt_sha256: Sha256::of(&bytes),
    })
}

fn check_record(dir: &Path, receipt_sha256: Sha256) -> Result<Vec<u8>, StoreError> {
    serde_json::to_vec(&CheckRecord { receipt_sha256 }).map_err(|e| StoreError::BadReceipt {
        path: dir.to_path_buf(),
        problem: e.to_string(),
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

/// Refuses a path that could leave the content folder, that Windows would write under
/// another name, or that a case-insensitive file system would merge with another.
/// Downloads were checked already; this guards every write, whatever its source.
fn check_paths<'a>(paths: impl IntoIterator<Item = &'a str>) -> Result<(), Refusal> {
    let paths: Vec<&str> = paths.into_iter().collect();
    for path in &paths {
        let bad = path.is_empty()
            || path.contains(['\\', ':', '\0'])
            || path
                .split('/')
                .any(|segment| segment.is_empty() || segment.chars().all(|c| c == '.'));
        if bad {
            return Err(Refusal::EscapesRoot((*path).to_owned()));
        }
    }
    Ok(check_names(paths)?)
}

fn create_dirs(dir: &Path) -> Result<(), StoreError> {
    fs::create_dir_all(dir).map_err(|e| StoreError::io("create", dir, e))
}

/// Replaces `dir/name`: a temporary file in `dir`, synced, renamed over it, then `dir`
/// synced.
fn replace_synced(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), StoreError> {
    let temporary = dir.join(format!("{name}.partial"));
    write_synced(&temporary, bytes)?;
    let target = dir.join(name);
    held::rename(&temporary, &target).map_err(|e| StoreError::io("replace", &target, e))?;
    sync_dir(dir)
}

/// Writes `path` anew or over what is there: a crash can leave the temporary of
/// [`replace_synced`] behind.
fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let file = fs::File::create(path).map_err(|e| StoreError::io("create", path, e))?;
    write_all_synced(file, path, bytes)
}

/// Writes a file that must not exist yet. A name the volume folds into one already written
/// (an 8.3 alias, a stripped trailing dot, macOS normalization) fails with `AlreadyExists`
/// instead of overwriting that file.
fn write_new_synced(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| StoreError::io("create", path, e))?;
    write_all_synced(file, path, bytes)
}

fn write_all_synced(mut file: fs::File, path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
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
    use std::time::Duration;

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
        assert_eq!(store.installed().unwrap().installs.len(), 1);
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

    /// A name the store accepts. `[a-z]{1,6}` alone makes `prn`, `con`, `aux` and `nul` now
    /// and then, which are device names on Windows, and the store refuses them.
    fn portable_name() -> impl proptest::strategy::Strategy<Value = String> {
        use proptest::prelude::*;
        "[a-z]{1,6}".prop_filter("a name Windows accepts", |name| {
            check_names([name.as_str()]).is_ok()
        })
    }

    fn file_tree() -> impl proptest::strategy::Strategy<Value = Vec<(String, Vec<u8>)>> {
        use proptest::prelude::*;
        let path =
            proptest::collection::vec(portable_name(), 1..4).prop_map(|parts| parts.join("/"));
        proptest::collection::btree_map(path, proptest::collection::vec(any::<u8>(), 0..64), 1..12)
            .prop_map(|tree| {
                // A file and a folder of one name cannot both exist.
                let paths: Vec<String> = tree.keys().cloned().collect();
                tree.into_iter()
                    .filter(|(p, _)| !paths.iter().any(|q| q.starts_with(&format!("{p}/"))))
                    .collect()
            })
    }

    /// A fixed sequence of trees, drawn as the two properties below draw them. Before the
    /// names were filtered, about one property run in twenty-five drew a device name and
    /// failed; this sequence draws one in its 4,933rd tree.
    #[test]
    fn the_generated_trees_are_never_refused_for_their_names() {
        use proptest::strategy::{Strategy, ValueTree};
        let mut runner = proptest::test_runner::TestRunner::deterministic();
        let strategy = file_tree();
        for _ in 0..10_000 {
            let files = strategy.new_tree(&mut runner).unwrap().current();
            let names = files.iter().map(|(path, _)| path.as_str());
            assert_eq!(check_names(names), Ok(()), "{files:?}");
        }
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(32))]

        #[test]
        fn any_install_reads_back_byte_for_byte(files in file_tree()) {
            let dir = tempfile::tempdir().unwrap();
            let store = ResourceStore::open(dir.path()).unwrap();
            let mut s = staged('a');
            s.files = files.clone();
            store.install(s).unwrap();
            let installed = store.dcs(&pin('a')).unwrap().unwrap();
            installed.verify().unwrap();
            for (path, bytes) in &files {
                proptest::prop_assert_eq!(&installed.read(path).unwrap(), bytes);
            }
            proptest::prop_assert_eq!(installed.files().count(), files.len());
        }

        #[test]
        fn a_crash_anywhere_leaves_nothing_or_everything(
            files in file_tree(),
            stop in 0usize..16,
        ) {
            let dir = tempfile::tempdir().unwrap();
            let store = ResourceStore::open(dir.path()).unwrap();
            let steps: Vec<Step> = (1..=files.len())
                .map(Step::FileWritten)
                .chain([Step::ReceiptWritten, Step::Renamed])
                .collect();
            let stop = steps[stop % steps.len()];
            let mut s = staged('a');
            s.files = files.clone();
            let crashed = store.install_until(s, &mut |step| step == stop);
            proptest::prop_assert!(matches!(crashed, Err(InstallError::Interrupted)));
            let reopened = ResourceStore::open(dir.path()).unwrap();
            match reopened.dcs(&pin('a')).unwrap() {
                None => proptest::prop_assert_ne!(stop, Step::Renamed),
                Some(installed) => {
                    installed.verify().unwrap();
                    for (path, bytes) in &files {
                        proptest::prop_assert_eq!(&installed.read(path).unwrap(), bytes);
                    }
                }
            }
            proptest::prop_assert!(!dir.path().join(PARTIAL).exists());
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
        for unportable in [
            "notes./x",
            "x/a ",
            "x/a.",
            "PROGRA~1",
            "ABCDEF~1.JSN",
            "x/abcdef~1.tsv",
            "A~9",
            "CON.json",
            "nul.json",
        ] {
            let mut s = staged('a');
            s.files.push((unportable.to_owned(), Vec::new()));
            assert!(
                matches!(
                    store.install(s),
                    Err(InstallError::Refused {
                        refusal: Refusal::NotPortable { .. },
                        ..
                    })
                ),
                "{unportable:?}"
            );
        }
        // Names one volume would store as one file or folder: letter case, `σ` and `ς`
        // (macOS folds them as it folds case), NFC and NFD, folders that differ in case,
        // and a file and a folder of one name.
        for added in [
            &["METADATA.json"][..],
            &["σ", "ς"],
            &["café.md", "cafe\u{301}.md"],
            &["Notes/a.json", "notes/b.json"],
            &["A", "a/x"],
            &["receipt.json", "RECEIPT.json"],
        ] {
            let mut s = staged('a');
            s.files
                .extend(added.iter().map(|path| ((*path).to_owned(), Vec::new())));
            assert!(
                matches!(
                    store.install(s),
                    Err(InstallError::Refused {
                        refusal: Refusal::DuplicateEntry(_),
                        ..
                    })
                ),
                "{added:?}"
            );
        }
        assert!(store.installed().unwrap().installs.is_empty());
        // `ı` stays apart from `i` on every supported file system, and only the shape of an
        // 8.3 short name is refused as one.
        let mut s = staged('a');
        s.files.extend(
            [
                "ı",
                "i",
                "notes~1.json",
                "abcdef~12x",
                "a~1.tar.gz",
                "notes~2024-draft.md",
            ]
            .map(|path| (path.to_owned(), Vec::new())),
        );
        store.install(s).unwrap().verify().unwrap();
    }

    #[test]
    fn folder_names_are_safe_on_every_platform() {
        assert_eq!(folder_name("Es-419_gl"), "es-419_gl");
        assert_eq!(folder_name("..."), "_...");
        assert_eq!(folder_name(".hidden"), "_.hidden");
        assert_eq!(folder_name("CON"), "_con");
        assert_eq!(folder_name("nul.tn"), "_nul.tn");
        assert_eq!(folder_name("console"), "console");
    }

    #[test]
    fn two_names_sharing_a_folder_never_read_each_others_install() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResourceStore::open(dir.path()).unwrap();
        let mut a = staged('a');
        if let Origin::Dcs { pin, .. } = &mut a.origin {
            pin.repo = "git.door43.org/_con/x_tn".parse().unwrap();
        }
        store.install(a).unwrap();
        let other = ResourcePin::new(
            "git.door43.org/con/x_tn".parse().unwrap(),
            "a".repeat(40).parse().unwrap(),
        );
        assert!(matches!(
            store.dcs(&other),
            Err(StoreError::Occupied { .. })
        ));
    }

    #[test]
    fn an_unreadable_install_folder_is_listed_and_hides_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResourceStore::open(dir.path()).unwrap();
        store.install(staged('a')).unwrap();
        let stray = dir
            .path()
            .join(DCS)
            .join("someone")
            .join("x_tn")
            .join("b".repeat(40));
        fs::create_dir_all(&stray).unwrap();
        let inventory = store.installed().unwrap();
        assert_eq!(inventory.installs.len(), 1);
        assert!(matches!(
            inventory.unreadable[..],
            [StoreError::BadReceipt { .. }]
        ));
        assert_eq!(store.installed_dcs_pins().unwrap().len(), 1);
    }

    #[test]
    fn a_file_whose_size_changed_reads_as_damaged() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResourceStore::open(dir.path()).unwrap();
        let installed = store.install(staged('a')).unwrap();
        let file = installed.dir().join(CONTENT).join("ingredients/TIT.tsv");
        fs::write(&file, b"Reference").unwrap();
        for read in [
            installed.read("ingredients/TIT.tsv").map(|_| ()),
            installed.read_text("ingredients/TIT.tsv").map(|_| ()),
        ] {
            assert!(matches!(read, Err(StoreError::Damaged { .. })), "{read:?}");
        }
        assert_eq!(installed.read("metadata.json").unwrap(), b"{}");
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

    /// The one staging folder of an install that has started and not finished.
    fn the_staging_folder(root: &Path) -> PathBuf {
        let mut stagings = fs::read_dir(root.join(PARTIAL)).unwrap();
        let staging = stagings.next().unwrap().unwrap().path();
        assert!(stagings.next().is_none());
        staging
    }

    fn no_staging_folder_left(store: &ResourceStore) {
        let leftovers = fs::read_dir(store.root().join(PARTIAL)).map_or(0, Iterator::count);
        assert_eq!(leftovers, 0, "no staging folder may remain");
    }

    /// Bundles an install whose receipt lists `renamed` in place of its `metadata.json`,
    /// with the file moved to match: what an installer built by another tool could ship.
    fn bundled_with_file_named(bundled_dir: &Path, renamed: &str) {
        let bundled = ResourceStore::open(bundled_dir).unwrap();
        let install = bundled.install(staged('a')).unwrap();
        let content = install.dir().join(CONTENT);
        fs::rename(content.join("metadata.json"), content.join(renamed)).unwrap();
        let mut receipt = install.receipt.clone();
        for record in &mut receipt.files {
            if record.path == "metadata.json" {
                record.path = renamed.to_owned();
            }
        }
        receipt.files.sort_by(|a, b| a.path.cmp(&b.path));
        fs::write(
            install.dir().join(RECEIPT),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn seeding_refuses_a_bundled_file_name_not_every_platform_accepts_and_installs_nothing_of_it() {
        let bundled_dir = tempfile::tempdir().unwrap();
        // An 8.3 short name, which every platform stores as a plain file. A device name such
        // as `aux.json` would be refused too, but on Windows putting it on disk opens the
        // device.
        bundled_with_file_named(bundled_dir.path(), "ABCDEF~1.JSN");
        let bundled = ResourceStore::open_read_only(bundled_dir.path()).unwrap();
        let machine_dir = tempfile::tempdir().unwrap();
        let machine = ResourceStore::open(machine_dir.path()).unwrap();

        let opened = content_files_opened();
        let report = machine.seed_from(&bundled).unwrap();

        assert_eq!(
            content_files_opened() - opened,
            0,
            "the names are refused before a file is opened"
        );
        assert!(report.added.is_empty(), "{:?}", report.added);
        assert_eq!(report.refused.len(), 1, "{:?}", report.refused);
        assert!(
            report.refused[0].1.to_string().contains("ABCDEF~1.JSN"),
            "the refusal names the file: {}",
            report.refused[0].1
        );
        assert!(machine.installed().unwrap().installs.is_empty());
        no_staging_folder_left(&machine);
    }

    #[test]
    fn seeding_opens_no_file_of_a_bundled_install_whose_receipt_names_a_path_outside_it() {
        let dir = tempfile::tempdir().unwrap();
        let bundled_dir = dir.path().join("bundle");
        // Six folders up from the content folder (`dcs/<owner>/<repo>/<sha>/content`) is
        // `dir`, outside the bundle. The file moved there keeps the receipt's size and
        // SHA-256, so reading it would pass the check of its bytes.
        let escaping = "../../../../../../outside.json";
        bundled_with_file_named(&bundled_dir, escaping);
        assert!(dir.path().join("outside.json").is_file());
        let bundled = ResourceStore::open_read_only(&bundled_dir).unwrap();
        let machine_dir = tempfile::tempdir().unwrap();
        let machine = ResourceStore::open(machine_dir.path()).unwrap();

        let opened = content_files_opened();
        let report = machine.seed_from(&bundled).unwrap();

        assert_eq!(
            content_files_opened() - opened,
            0,
            "the path is refused before a file is opened"
        );
        assert!(report.added.is_empty(), "{:?}", report.added);
        assert_eq!(report.refused.len(), 1, "{:?}", report.refused);
        assert!(
            report.refused[0].1.to_string().contains(escaping),
            "the refusal names the path: {}",
            report.refused[0].1
        );
        assert!(machine.installed().unwrap().installs.is_empty());
        no_staging_folder_left(&machine);
    }

    #[test]
    fn seeding_installs_a_bundled_file_name_every_platform_accepts() {
        let bundled_dir = tempfile::tempdir().unwrap();
        bundled_with_file_named(bundled_dir.path(), "notes.json");
        let bundled = ResourceStore::open_read_only(bundled_dir.path()).unwrap();
        let machine_dir = tempfile::tempdir().unwrap();
        let machine = ResourceStore::open(machine_dir.path()).unwrap();

        let report = machine.seed_from(&bundled).unwrap();

        assert_eq!(report.added.len(), 1);
        assert!(report.refused.is_empty(), "{:?}", report.refused);
        machine.dcs(&pin('a')).unwrap().unwrap().verify().unwrap();
    }

    #[test]
    fn a_staged_file_never_overwrites_a_name_the_volume_already_holds() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResourceStore::open(dir.path()).unwrap();
        let outside = dir.path().join("planted");
        // Stands in for a volume that shows an earlier file under this file's name: the
        // second file's name exists before the install creates it. The staging folder is
        // removed when the install is refused, so a second name for the planted file's
        // bytes outside it shows whether they were written over.
        let result = store.install_until(staged('a'), &mut |step| {
            if step == Step::FileWritten(1) {
                let file = the_staging_folder(dir.path())
                    .join(CONTENT)
                    .join("ingredients/TIT.tsv");
                fs::write(&file, b"planted").unwrap();
                fs::hard_link(&file, &outside).unwrap();
            }
            false
        });
        assert!(
            matches!(
                &result,
                Err(InstallError::Refused { refusal: Refusal::DuplicateEntry(path), .. })
                    if path == "ingredients/TIT.tsv"
            ),
            "{result:?}"
        );
        assert_eq!(fs::read(outside).unwrap(), b"planted");
        assert!(store.dcs(&pin('a')).unwrap().is_none());
        assert!(store.installed().unwrap().installs.is_empty());
        no_staging_folder_left(&store);
    }

    #[test]
    fn an_install_that_lost_the_race_returns_the_winner_and_removes_its_staging() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResourceStore::open(dir.path()).unwrap();
        // The same install lands between this one's last write and its rename.
        let installed = store
            .install_until(staged('a'), &mut |step| {
                if step == Step::ReceiptWritten {
                    store.install(staged('a')).unwrap();
                }
                false
            })
            .unwrap();
        installed.verify().unwrap();
        assert_eq!(store.installed().unwrap().installs.len(), 1);
        no_staging_folder_left(&store);
    }

    #[test]
    fn opening_fails_when_a_leftover_cannot_be_cleared_for_a_reason_no_scanner_causes() {
        let dir = tempfile::tempdir().unwrap();
        // A file where the staging folders go: clearing it as a folder fails with "not a
        // directory".
        fs::write(dir.path().join(PARTIAL), b"x").unwrap();
        let result = ResourceStore::open(dir.path());
        assert!(
            matches!(
                result,
                Err(StoreError::Io {
                    action: "clear",
                    ..
                })
            ),
            "{result:?}"
        );
    }

    #[test]
    fn an_install_never_writes_into_a_staging_folder_that_exists() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResourceStore::open(dir.path()).unwrap();
        let taken = dir.path().join(PARTIAL).join("taken");
        fs::create_dir_all(&taken).unwrap();
        fs::write(taken.join("other"), b"another install's file").unwrap();
        let result = store.install_in(staged('a'), "taken", &mut |_| false);
        assert!(
            matches!(
                &result,
                Err(InstallError::Store(StoreError::Io { action: "create", source, .. }))
                    if source.kind() == io::ErrorKind::AlreadyExists
            ),
            "{result:?}"
        );
        let names: Vec<_> = fs::read_dir(&taken)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(
            names,
            ["other"],
            "nothing was written beside the other file"
        );
        assert_eq!(
            fs::read(taken.join("other")).unwrap(),
            b"another install's file"
        );
        assert!(store.installed().unwrap().installs.is_empty());
    }

    /// The Windows tests below hold real files. These hold them in a simulated scanner
    /// (`crate::held::scanner`), so every platform runs the same retries.
    mod a_scanner_holds_files {
        use super::*;
        use crate::held::scanner::Scanner;

        fn ms(waits: &[u64]) -> Vec<Duration> {
            waits.iter().map(|w| Duration::from_millis(*w)).collect()
        }

        fn staged_metadata(root: &Path) -> PathBuf {
            the_staging_folder(root).join(CONTENT).join("metadata.json")
        }

        /// A leftover of a crashed install, with a file in it.
        fn leftover_file(root: &Path) -> PathBuf {
            let content = root.join(PARTIAL).join("1-2-3").join(CONTENT);
            fs::create_dir_all(&content).unwrap();
            let file = content.join("metadata.json");
            fs::write(&file, b"{}").unwrap();
            file
        }

        #[test]
        fn an_install_waits_for_a_scanner_to_let_go_of_a_staged_file() {
            let dir = tempfile::tempdir().unwrap();
            let store = ResourceStore::open(dir.path()).unwrap();
            let scanner = Scanner::new();
            let installed = store
                .install_until(staged('a'), &mut |step| {
                    if step == Step::ReceiptWritten {
                        scanner.holds_for(&staged_metadata(dir.path()), 3);
                    }
                    false
                })
                .unwrap();
            installed.verify().unwrap();
            assert_eq!(scanner.waited(), ms(&[2, 4, 8]));
            no_staging_folder_left(&store);
        }

        #[test]
        fn an_install_gives_up_after_the_last_wait_and_the_next_open_clears_what_it_left() {
            let dir = tempfile::tempdir().unwrap();
            let store = ResourceStore::open(dir.path()).unwrap();
            let scanner = Scanner::new();
            let mut held_file = PathBuf::new();
            let result = store.install_until(staged('a'), &mut |step| {
                if step == Step::ReceiptWritten {
                    held_file = staged_metadata(dir.path());
                    scanner.holds(&held_file);
                }
                false
            });
            let source = match result {
                Err(InstallError::Store(StoreError::Io {
                    action: "move into place",
                    source,
                    ..
                })) => source,
                other => panic!("{other:?}"),
            };
            assert!(held::is_held(&source), "{source:?}");
            let waited: Duration = scanner.waited().iter().sum();
            assert_eq!(waited, Duration::from_millis(2_004));
            assert!(store.dcs(&pin('a')).unwrap().is_none());
            scanner.lets_go(&held_file);
            let store = ResourceStore::open(dir.path()).unwrap();
            assert!(!dir.path().join(PARTIAL).exists());
            store.install(staged('a')).unwrap().verify().unwrap();
        }

        #[test]
        fn an_install_that_lost_the_race_does_not_wait_for_a_scanner() {
            let dir = tempfile::tempdir().unwrap();
            let store = ResourceStore::open(dir.path()).unwrap();
            let scanner = Scanner::new();
            let mut held_file = PathBuf::new();
            // The scanner holds a staged file while the same install lands first. Whatever
            // error the rename reports, the folder it was renamed onto exists, so waiting
            // for the scanner would not help.
            let installed = store
                .install_until(staged('a'), &mut |step| {
                    if step == Step::ReceiptWritten {
                        held_file = staged_metadata(dir.path());
                        scanner.holds(&held_file);
                        store.install(staged('a')).unwrap();
                    }
                    false
                })
                .unwrap();
            installed.verify().unwrap();
            assert!(scanner.waited().is_empty(), "{:?}", scanner.waited());
            // The loser's staging folder was held, so it stays until the next open.
            assert_eq!(fs::read_dir(dir.path().join(PARTIAL)).unwrap().count(), 1);
            scanner.lets_go(&held_file);
            ResourceStore::open(dir.path()).unwrap();
            assert!(!dir.path().join(PARTIAL).exists());
        }

        #[test]
        fn opening_waits_for_a_scanner_to_let_go_of_a_leftover() {
            let dir = tempfile::tempdir().unwrap();
            let file = leftover_file(dir.path());
            let scanner = Scanner::new();
            scanner.holds_for(&file, 3);
            ResourceStore::open(dir.path()).unwrap();
            assert!(!dir.path().join(PARTIAL).exists());
            assert_eq!(scanner.waited(), ms(&[2, 4, 8]));
        }

        #[test]
        fn opening_succeeds_when_a_scanner_holds_a_leftover_throughout() {
            let dir = tempfile::tempdir().unwrap();
            let file = leftover_file(dir.path());
            let scanner = Scanner::new();
            scanner.holds(&file);
            let store = ResourceStore::open(dir.path()).unwrap();
            assert!(file.exists(), "a held leftover stays");
            let waited: Duration = scanner.waited().iter().sum();
            assert_eq!(waited, Duration::from_millis(2_004));
            store.install(staged('a')).unwrap().verify().unwrap();
            scanner.lets_go(&file);
            ResourceStore::open(dir.path()).unwrap();
            assert!(!dir.path().join(PARTIAL).exists());
        }

        #[test]
        fn seeding_records_a_check_when_a_scanner_briefly_holds_the_old_record() {
            let bundled_dir = tempfile::tempdir().unwrap();
            let bundled = ResourceStore::open(bundled_dir.path()).unwrap();
            bundled.install(staged('a')).unwrap();
            let read_only = ResourceStore::open_read_only(bundled_dir.path()).unwrap();
            let machine_dir = tempfile::tempdir().unwrap();
            let machine = ResourceStore::open(machine_dir.path()).unwrap();
            machine.seed_from(&read_only).unwrap();
            let record = machine.dcs(&pin('a')).unwrap().unwrap().dir().join(CHECKED);
            let recorded = fs::read(&record).unwrap();
            // A record that names no receipt: the copy is checked again and the record
            // replaced, while the scanner still has the old one open.
            fs::write(&record, b"stale").unwrap();
            let scanner = Scanner::new();
            scanner.holds_for(&record, 3);
            let report = machine.seed_from(&read_only).unwrap();
            assert_eq!(report.checked_again.len(), 1);
            assert_eq!(fs::read(&record).unwrap(), recorded);
            assert_eq!(scanner.waited(), ms(&[2, 4, 8]));
            assert!(
                machine
                    .seed_from(&read_only)
                    .unwrap()
                    .checked_again
                    .is_empty()
            );
        }

        /// Takes the staged metadata file when the last file of a copy is written, as a
        /// scanner does, before the copy is renamed into place.
        struct ScannerAtTheLastFile<'a> {
            scanner: &'a Scanner,
            root: &'a Path,
            files: u64,
        }

        impl SeedWatch for ScannerAtTheLastFile<'_> {
            fn done(&self, files: u64) {
                if files == self.files {
                    self.scanner.holds_for(&staged_metadata(self.root), 3);
                }
            }

            fn cancelled(&self) -> bool {
                false
            }
        }

        #[test]
        fn seeding_waits_for_a_scanner_to_let_go_of_a_staged_file() {
            let bundled_dir = tempfile::tempdir().unwrap();
            ResourceStore::open(bundled_dir.path())
                .unwrap()
                .install(staged('a'))
                .unwrap();
            let bundled = ResourceStore::open_read_only(bundled_dir.path()).unwrap();
            let machine_dir = tempfile::tempdir().unwrap();
            let machine = ResourceStore::open(machine_dir.path()).unwrap();
            let scanner = Scanner::new();
            let plan = machine.seed_plan(&bundled).unwrap();
            let watch = ScannerAtTheLastFile {
                scanner: &scanner,
                root: machine_dir.path(),
                files: plan.files(),
            };

            let Seeding::Finished(report) = machine.seed(plan, &watch).unwrap() else {
                panic!("the seeding stopped")
            };

            assert_eq!(report.added.len(), 1);
            assert!(report.refused.is_empty(), "{:?}", report.refused);
            assert_eq!(scanner.waited(), ms(&[2, 4, 8]));
            machine.dcs(&pin('a')).unwrap().unwrap().verify().unwrap();
            no_staging_folder_left(&machine);
        }

        #[test]
        fn the_app_opens_beside_a_leftover_a_scanner_holds_and_reports_it() {
            let dir = tempfile::tempdir().unwrap();
            let file = leftover_file(dir.path());
            let scanner = Scanner::new();
            scanner.holds(&file);

            let (store, left) = ResourceStore::open_keeping_leftovers(dir.path()).unwrap();

            let Some(StoreError::Io { action, source, .. }) = left else {
                panic!("the leftover was not reported: {left:?}")
            };
            assert_eq!(action, "clear");
            assert!(held::is_held(&source), "{source:?}");
            let waited: Duration = scanner.waited().iter().sum();
            assert_eq!(waited, Duration::from_millis(2_004));
            assert!(file.exists(), "a held leftover stays");
            store.install(staged('a')).unwrap().verify().unwrap();
            scanner.lets_go(&file);
            let (_, left) = ResourceStore::open_keeping_leftovers(dir.path()).unwrap();
            assert!(left.is_none(), "{left:?}");
            assert!(!dir.path().join(PARTIAL).exists());
        }
    }

    #[cfg(windows)]
    const FILE_SHARE_READ: u32 = 1;
    #[cfg(windows)]
    const FILE_SHARE_WRITE: u32 = 2;

    /// Opens `path` as a scanner does: others may still read and write it, but a folder
    /// that holds it cannot be renamed or deleted.
    #[cfg(windows)]
    fn held_like_a_scanner(path: &Path) -> fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(path)
            .unwrap()
    }

    #[cfg(windows)]
    fn release_after(file: fs::File, pause: Duration) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            std::thread::sleep(pause);
            drop(file);
        })
    }

    #[cfg(windows)]
    #[test]
    fn an_install_succeeds_when_a_scanner_briefly_holds_a_staged_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResourceStore::open(dir.path()).unwrap();
        let mut scanner = None;
        let installed = store
            .install_until(staged('a'), &mut |step| {
                if step == Step::ReceiptWritten {
                    let held = held_like_a_scanner(
                        &the_staging_folder(dir.path())
                            .join(CONTENT)
                            .join("metadata.json"),
                    );
                    scanner = Some(release_after(held, Duration::from_millis(300)));
                }
                false
            })
            .unwrap();
        installed.verify().unwrap();
        scanner.unwrap().join().unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn opening_clears_a_leftover_that_a_scanner_releases_soon() {
        let dir = tempfile::tempdir().unwrap();
        let leftover = dir.path().join(PARTIAL).join("1-2-3").join(CONTENT);
        fs::create_dir_all(&leftover).unwrap();
        let file = leftover.join("metadata.json");
        fs::write(&file, b"{}").unwrap();
        let scanner = release_after(held_like_a_scanner(&file), Duration::from_millis(300));
        ResourceStore::open(dir.path()).unwrap();
        scanner.join().unwrap();
        assert!(!dir.path().join(PARTIAL).exists());
    }

    #[cfg(windows)]
    #[test]
    fn opening_succeeds_when_a_scanner_holds_a_leftover_throughout() {
        let dir = tempfile::tempdir().unwrap();
        let leftover = dir.path().join(PARTIAL).join("1-2-3").join(CONTENT);
        fs::create_dir_all(&leftover).unwrap();
        let file = leftover.join("metadata.json");
        fs::write(&file, b"{}").unwrap();
        let held = held_like_a_scanner(&file);
        let store = ResourceStore::open(dir.path()).unwrap();
        store.install(staged('a')).unwrap().verify().unwrap();
        drop(held);
        ResourceStore::open(dir.path()).unwrap();
        assert!(!dir.path().join(PARTIAL).exists());
    }

    #[cfg(windows)]
    #[test]
    fn an_install_that_lost_the_race_does_not_wait_for_a_scanner_to_let_go_of_its_staging() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResourceStore::open(dir.path()).unwrap();
        let mut held = None;
        let started = std::time::Instant::now();
        // The scanner holds a staged file while the same install lands first. The rename
        // fails with a held-file error either way; the folder it was renamed onto exists,
        // so waiting for the scanner would not help.
        let installed = store
            .install_until(staged('a'), &mut |step| {
                if step == Step::ReceiptWritten {
                    held = Some(held_like_a_scanner(
                        &the_staging_folder(dir.path())
                            .join(CONTENT)
                            .join("metadata.json"),
                    ));
                    store.install(staged('a')).unwrap();
                }
                false
            })
            .unwrap();
        let waited = started.elapsed();
        drop(held);
        assert!(waited < Duration::from_millis(1500), "{waited:?}");
        installed.verify().unwrap();
        // The loser's staging folder was held, so it stays until the next open.
        ResourceStore::open(dir.path()).unwrap();
        assert!(!dir.path().join(PARTIAL).exists());
    }

    #[cfg(windows)]
    #[test]
    fn seeding_records_a_check_when_a_scanner_briefly_holds_the_old_record() {
        let bundled_dir = tempfile::tempdir().unwrap();
        let bundled = ResourceStore::open(bundled_dir.path()).unwrap();
        bundled.install(staged('a')).unwrap();
        let read_only = ResourceStore::open_read_only(bundled_dir.path()).unwrap();
        let machine_dir = tempfile::tempdir().unwrap();
        let machine = ResourceStore::open(machine_dir.path()).unwrap();
        machine.seed_from(&read_only).unwrap();
        let record = machine.dcs(&pin('a')).unwrap().unwrap().dir().join(CHECKED);
        let recorded = fs::read(&record).unwrap();
        // A record that names no receipt: the copy is checked again and the record
        // replaced, while the scanner still has the old one open.
        fs::write(&record, b"stale").unwrap();
        let scanner = release_after(held_like_a_scanner(&record), Duration::from_millis(300));
        let report = machine.seed_from(&read_only).unwrap();
        scanner.join().unwrap();
        assert_eq!(report.checked_again.len(), 1);
        assert_eq!(fs::read(&record).unwrap(), recorded);
        assert!(
            machine
                .seed_from(&read_only)
                .unwrap()
                .checked_again
                .is_empty()
        );
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
