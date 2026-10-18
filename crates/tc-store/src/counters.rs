//! The store's operation counters (PERFORMANCE.md rule 2, APP-DESIGN R14): files opened and
//! written, flushes, renames, bytes, files projected, and how long commands waited in the
//! actor's queue. Every file operation of the store passes through [`CountingVfs`], so the
//! counts are those of the real work, whichever filesystem is under it. On Windows with
//! Defender on, every file opened costs a scan, so the count of files opened per action is a
//! budget of its own.

use crate::vfs::{DirEntry, FileMeta, LockHandle, ReadHandle, Vfs, WriteHandle};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

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

/// A [`Vfs`] that counts what passes through it.
#[derive(Debug, Clone)]
pub struct CountingVfs {
    inner: Arc<dyn Vfs>,
    counters: Arc<Counters>,
}

impl CountingVfs {
    pub fn new(inner: Arc<dyn Vfs>, counters: Arc<Counters>) -> Self {
        Self { inner, counters }
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
}
