//! Downloads streamed to a file.
//!
//! A download's bytes go to a new file in a folder the caller names, and are never held in
//! memory whole. Before each chunk is written the client reads the internet switch and asks
//! the caller's [`DownloadWatch`] whether to stop; it asks again every
//! [`STOP_CHECK_INTERVAL`] while it waits for Door43, so a stalled transfer stops promptly
//! too. After each chunk it tells the watch how many bytes have arrived.
//!
//! The file is removed on every way out except success: an error, a cancel, the switch
//! going off, a failed verification, and the dropping of the download's future (an aborted
//! task). Only a crash leaves it behind, so the caller names a folder it clears at start
//! (tc-resources uses its store's `.partial/`, which opening the store empties).

use crate::error::Door43Error;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// How often a download that waits for Door43 reads the internet switch and asks its
/// watch whether to stop. A person who presses Cancel sees the download stop within this
/// time, however slow the connection.
pub const STOP_CHECK_INTERVAL: Duration = Duration::from_millis(100);

/// The bytes the client hands to the file at a time.
const WRITE_BUFFER: usize = 64 << 10;

/// What a download reports to, and asks before it goes on. The app's download job holds
/// one per download: it keeps the count for its progress reporter and answers from its
/// cancel token.
pub trait DownloadWatch: Sync {
    /// Called after each chunk with the bytes received so far, and the size Door43
    /// announced (`Content-Length`) when it announced one. `received` only grows, and the
    /// last call before success gives the size of the whole download.
    fn received(&self, received: u64, announced: Option<u64>);

    /// Whether to stop. Asked before the request is sent, before each chunk is written,
    /// every [`STOP_CHECK_INTERVAL`] while the download waits, and once more after the
    /// download is verified. `true` ends the download with [`Door43Error::Cancelled`], and
    /// nothing of it is kept.
    fn cancelled(&self) -> bool;
}

/// A watch that hears nothing and never cancels.
#[derive(Debug, Clone, Copy, Default)]
pub struct Unwatched;

impl DownloadWatch for Unwatched {
    fn received(&self, _: u64, _: Option<u64>) {}

    fn cancelled(&self) -> bool {
        false
    }
}

/// Where a download is written and who watches it.
#[derive(Clone, Copy)]
pub struct DownloadTo<'a> {
    pub(crate) folder: &'a Path,
    pub(crate) watch: &'a dyn DownloadWatch,
}

impl<'a> DownloadTo<'a> {
    /// Into a new file in `folder`, which is created when missing. Put it on the same disk
    /// as where the content goes, in a place the app clears at start.
    pub fn folder(folder: &'a Path) -> Self {
        Self {
            folder,
            watch: &Unwatched,
        }
    }

    pub fn watched_by(self, watch: &'a dyn DownloadWatch) -> Self {
        Self { watch, ..self }
    }
}

/// A downloaded zip on disk, verified before the caller sees it. Dropping it removes the
/// file.
#[derive(Debug)]
pub struct DownloadedZip {
    path: PathBuf,
    size: u64,
}

impl DownloadedZip {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Its size in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }
}

impl Drop for DownloadedZip {
    fn drop(&mut self) {
        // A file that cannot be removed now (Windows refuses while a scanner holds it)
        // stays in the caller's folder until the app clears it at start.
        let _ = fs::remove_file(&self.path);
    }
}

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

/// A download being written. Its fields drop in order, so the file is closed before the
/// guard removes it, which Windows needs.
pub(crate) struct Writing {
    file: BufWriter<File>,
    zip: DownloadedZip,
}

impl Writing {
    /// A new, empty file in `folder`, never one that exists already.
    pub(crate) fn create(folder: &Path, route: &str) -> Result<Self, Door43Error> {
        fs::create_dir_all(folder).map_err(|e| disk(route, folder, e))?;
        loop {
            let name = format!(
                "door43-{}-{}.zip.part",
                std::process::id(),
                NEXT_FILE.fetch_add(1, Ordering::Relaxed)
            );
            let path = folder.join(name);
            match File::options().write(true).create_new(true).open(&path) {
                Ok(file) => {
                    return Ok(Self {
                        file: BufWriter::with_capacity(WRITE_BUFFER, file),
                        zip: DownloadedZip { path, size: 0 },
                    });
                }
                // Left by an earlier run of this process id that crashed.
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(disk(route, &path, e)),
            }
        }
    }

    pub(crate) fn write(&mut self, route: &str, chunk: &[u8]) -> Result<(), Door43Error> {
        self.file
            .write_all(chunk)
            .map_err(|e| disk(route, &self.zip.path, e))?;
        self.zip.size += chunk.len() as u64;
        Ok(())
    }

    /// Flushes and closes the file. It is not synced: it is read back at once for its
    /// check and removed after the install, and a crash before then loses nothing.
    pub(crate) fn finish(self, route: &str) -> Result<DownloadedZip, Door43Error> {
        let Self { file, zip } = self;
        file.into_inner()
            .map_err(|e| disk(route, &zip.path, e.into_error()))?;
        Ok(zip)
    }
}

pub(crate) fn disk(route: &str, path: &Path, source: std::io::Error) -> Door43Error {
    Door43Error::Disk {
        route: route.to_owned(),
        path: path.to_owned(),
        source,
    }
}
