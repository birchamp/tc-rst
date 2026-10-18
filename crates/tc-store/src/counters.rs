//! The store's operation counters (PERFORMANCE.md rule 2, APP-DESIGN R14): files opened and
//! written, flushes, renames, bytes, files projected, and how long commands waited in the
//! actor's queue. Every file operation of the store passes through [`CountingVfs`], so the
//! counts are those of the real work, whichever filesystem is under it. On Windows with
//! Defender on, every file opened costs a scan, so the count of files opened per action is a
//! budget of its own.

use crate::vfs::{DirEntry, FileMeta, LockHandle, ReadHandle, Vfs, WriteHandle};
use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

macro_rules! counters {
    ($($(#[$doc:meta])* $name:ident),* $(,)?) => {
        /// Live counters, shared by the store's threads.
        #[derive(Debug, Default)]
        pub struct Counters {
            $($(#[$doc])* pub $name: AtomicU64,)*
        }

        /// The counters at one moment.
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
        pub struct StoreCounters {
            $($(#[$doc])* pub $name: u64,)*
        }

        impl Counters {
            pub fn read(&self) -> StoreCounters {
                StoreCounters {
                    $($name: self.$name.load(Ordering::Relaxed),)*
                }
            }
        }

        impl StoreCounters {
            /// The change from `earlier` to `self`.
            pub fn since(&self, earlier: &Self) -> Self {
                Self {
                    $($name: self.$name.saturating_sub(earlier.$name),)*
                }
            }
        }
    };
}

counters! {
    /// Files opened for reading.
    files_opened,
    /// Files created.
    files_created,
    /// Bytes read from files, by whole reads and reads at offsets.
    bytes_read,
    /// Bytes written.
    bytes_written,
    /// File and folder flushes.
    fsyncs,
    renames,
    removes,
    /// Folder listings.
    listings,
    /// Stats of a path.
    stats,
    /// Derived files whose bytes were read by an open's classification (O9) or a
    /// compare-and-swap (M1): the reads that happen before the project is usable or when a
    /// file is written.
    derived_read,
    /// Derived files whose bytes the storage check read in the background (O14).
    check_read,
    /// Derived files projected (rendered from the fold).
    projected,
    /// Derived files written.
    derived_written,
    /// Heads files read.
    heads_read,
    /// Pack and pending files an open read, in full or to fingerprint them.
    journal_files_read,
    /// Of those, the files whose changes came from the fold cache (§6.1 O8).
    journal_files_cached,
    /// Commands the actor ran.
    commands,
    /// Nanoseconds commands waited in the actor's queue before it took them.
    queue_wait_ns,
    /// The longest wait of one command, in nanoseconds.
    queue_wait_max_ns,
    /// Draft files written.
    drafts_written,
    /// Renames and removals tried again because another program held the file.
    held_retries,
    /// Nanoseconds spent waiting for those files.
    held_wait_ns,
    /// Bytes the writer took an md5 of: each rendered buffer once, and each file it read to
    /// compare before it overwrote it.
    bytes_hashed,
    /// Nanoseconds the writer waited for saves in flight before a flush or a rename
    /// (PERFORMANCE.md: derived-file flushes are paced behind saves).
    writer_yield_ns,
    /// Blobs written to the object database (packs, derived files, carried files).
    git_blobs_written,
    /// Their bytes.
    git_blob_bytes,
    /// Trees written: the directories on the paths a commit changed, and the root.
    git_trees_written,
    git_commits_written,
    /// Repacks of the project's objects (§9.8).
    git_repacks,
    /// Objects a repack wrote into its pack.
    git_objects_packed,
    /// Loose object files a repack removed.
    git_loose_removed,
    /// Bytes of rendered USFM the per-record digests of the heads files were taken of
    /// (§11.1 X3: the base of an outside edit).
    digest_bytes,
    /// Files an outside edit was decoded from (§11.1 X2).
    outside_decodes,
    /// Records an outside edit compared value by value (§11.1 X3): the changed ones.
    outside_keys_examined,
}

impl Counters {
    pub(crate) fn add(&self, counter: &AtomicU64, n: u64) {
        let _ = self;
        counter.fetch_add(n, Ordering::Relaxed);
    }

    pub(crate) fn max(&self, counter: &AtomicU64, n: u64) {
        let _ = self;
        counter.fetch_max(n, Ordering::Relaxed);
    }
}

/// A [`Vfs`] that counts what passes through it and, for a project folder, remembers which
/// of its directories this session has made durable ([`Vfs::secure_dir_chain`]).
#[derive(Debug, Clone)]
pub struct CountingVfs {
    inner: Arc<dyn Vfs>,
    counters: Arc<Counters>,
    /// The project folder: its own entry in the projects folder was flushed when it was
    /// placed (§6.4 N2). Directories below it are flushed once per session.
    root: Option<PathBuf>,
    secured: Arc<Mutex<BTreeSet<PathBuf>>>,
}

impl CountingVfs {
    pub fn new(inner: Arc<dyn Vfs>, counters: Arc<Counters>) -> Self {
        Self {
            inner,
            counters,
            root: None,
            secured: Arc::default(),
        }
    }

    /// Makes this the filesystem of the project folder `root`.
    #[must_use]
    pub fn for_project(mut self, root: PathBuf) -> Self {
        self.root = Some(root);
        self
    }

    pub fn counters(&self) -> &Arc<Counters> {
        &self.counters
    }
}

struct CountedRead {
    inner: Box<dyn ReadHandle>,
    counters: Arc<Counters>,
}

impl ReadHandle for CountedRead {
    fn len(&self) -> u64 {
        self.inner.len()
    }

    fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()> {
        self.inner.read_at(buf, offset)?;
        self.counters
            .bytes_read
            .fetch_add(buf.len() as u64, Ordering::Relaxed);
        Ok(())
    }
}

struct CountedWrite {
    inner: Box<dyn WriteHandle>,
    counters: Arc<Counters>,
}

impl WriteHandle for CountedWrite {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.inner.write_all(bytes)?;
        self.counters
            .bytes_written
            .fetch_add(bytes.len() as u64, Ordering::Relaxed);
        Ok(())
    }

    fn sync_all(&mut self) -> io::Result<()> {
        self.counters.fsyncs.fetch_add(1, Ordering::Relaxed);
        self.inner.sync_all()
    }
}

impl Vfs for CountingVfs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.counters.files_opened.fetch_add(1, Ordering::Relaxed);
        let bytes = self.inner.read(path)?;
        self.counters
            .bytes_read
            .fetch_add(bytes.len() as u64, Ordering::Relaxed);
        Ok(bytes)
    }

    fn open(&self, path: &Path) -> io::Result<Box<dyn ReadHandle>> {
        self.counters.files_opened.fetch_add(1, Ordering::Relaxed);
        Ok(Box::new(CountedRead {
            inner: self.inner.open(path)?,
            counters: self.counters.clone(),
        }))
    }

    fn metadata(&self, path: &Path) -> io::Result<FileMeta> {
        self.counters.stats.fetch_add(1, Ordering::Relaxed);
        self.inner.metadata(path)
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>> {
        self.counters.listings.fetch_add(1, Ordering::Relaxed);
        self.inner.read_dir(path)
    }

    fn create_new(&self, path: &Path) -> io::Result<Box<dyn WriteHandle>> {
        self.counters.files_created.fetch_add(1, Ordering::Relaxed);
        Ok(Box::new(CountedWrite {
            inner: self.inner.create_new(path)?,
            counters: self.counters.clone(),
        }))
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.counters.renames.fetch_add(1, Ordering::Relaxed);
        self.inner.rename(from, to)
    }

    fn rename_dir_noreplace(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.counters.renames.fetch_add(1, Ordering::Relaxed);
        self.inner.rename_dir_noreplace(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.counters.removes.fetch_add(1, Ordering::Relaxed);
        self.inner.remove_file(path)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        self.counters.removes.fetch_add(1, Ordering::Relaxed);
        self.inner.remove_dir_all(path)
    }

    fn create_dir(&self, path: &Path) -> io::Result<()> {
        self.inner.create_dir(path)
    }

    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        self.counters.fsyncs.fetch_add(1, Ordering::Relaxed);
        self.inner.sync_dir(path)
    }

    fn sync_file(&self, path: &Path) -> io::Result<()> {
        self.counters.fsyncs.fetch_add(1, Ordering::Relaxed);
        self.inner.sync_file(path)
    }

    fn lock(&self, path: &Path) -> io::Result<Option<Box<dyn LockHandle>>> {
        self.inner.lock(path)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.inner.canonicalize(path)
    }

    fn is_held(&self, error: &io::Error) -> bool {
        self.inner.is_held(error)
    }

    fn wait_held(&self, delay: Duration) {
        self.counters.held_retries.fetch_add(1, Ordering::Relaxed);
        self.counters.held_wait_ns.fetch_add(
            u64::try_from(delay.as_nanos()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        self.inner.wait_held(delay);
    }

    fn outside_dir(&self, folder: &Path) -> io::Result<PathBuf> {
        self.inner.outside_dir(folder)
    }

    fn watch(
        &self,
        root: &Path,
        recursive: bool,
        sink: crate::vfs::WatchSink,
    ) -> io::Result<Option<crate::vfs::WatchGuard>> {
        self.inner.watch(root, recursive, sink)
    }

    fn secure_dir_chain(&self, dir: &Path, made: &[PathBuf]) -> io::Result<()> {
        let Some(root) = self
            .root
            .as_deref()
            .filter(|r| dir.starts_with(r) && dir != *r)
        else {
            return self.inner.secure_dir_chain(dir, made);
        };
        let todo: Vec<PathBuf> = {
            let mut secured = self.secured.lock().unwrap_or_else(|e| e.into_inner());
            secured.extend(made.iter().cloned());
            let mut todo = Vec::new();
            let mut at = dir;
            while at != root && !secured.contains(at) {
                todo.push(at.to_owned());
                match at.parent() {
                    Some(parent) => at = parent,
                    None => break,
                }
            }
            todo
        };
        // Outermost first; a directory is remembered only once its entry was flushed, so a
        // failed flush is tried again.
        for d in todo.iter().rev() {
            if let Some(parent) = d.parent() {
                self.sync_dir(parent)?;
            }
            self.secured
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(d.clone());
        }
        Ok(())
    }
}
