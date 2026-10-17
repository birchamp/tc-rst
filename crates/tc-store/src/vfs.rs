//! Every file operation of the store goes through [`Vfs`] (`docs/STORAGE.md` §5.0): [`StdFs`]
//! in production, [`crate::fault::FaultFs`] in tests, where a model filesystem tracks what is
//! durable and what is only in the page cache, so a test can stop the writer at any write
//! point and drop what a power cut would drop (§16.2).
//!
//! Only the helpers of [`crate::atomic`] write project files through it.

use std::ffi::OsString;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// What a stat of a path gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileMeta {
    pub len: u64,
    /// Modification time in nanoseconds since the Unix epoch, as the filesystem set it.
    pub mtime_ns: i64,
    pub kind: EntryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntryKind {
    File,
    Dir,
    /// A symlink, a device or anything else the store never reads as project data.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DirEntry {
    pub name: OsString,
    pub kind: EntryKind,
}

/// A file opened for reading at offsets: the fold reads values from pack and pending files
/// on demand through these (§2.6).
pub trait ReadHandle: Send + Sync {
    fn len(&self) -> u64;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Fills `buf` from `offset`; an error when the file ends first.
    fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()>;
}

/// A new file being written.
pub trait WriteHandle: Send {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()>;
    /// Linux `fsync`, macOS `F_FULLFSYNC`, Windows `FlushFileBuffers`.
    fn sync_all(&mut self) -> io::Result<()>;
}

/// The exclusive lock on `.tc/lock`, released when dropped.
pub trait LockHandle: Send + Sync {
    /// Replaces the lock file's content through the locked handle: a new file renamed over
    /// it would be a file nobody holds the lock of (§3.6).
    fn set_content(&mut self, bytes: &[u8]) -> io::Result<()>;
}

/// The file operations the store uses. Paths are absolute.
pub trait Vfs: Send + Sync + fmt::Debug {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn open(&self, path: &Path) -> io::Result<Box<dyn ReadHandle>>;
    /// The path's own metadata; a symlink is not followed.
    fn metadata(&self, path: &Path) -> io::Result<FileMeta>;
    /// The entries of a directory, sorted by name.
    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>>;
    /// Creates a file that must not exist yet (`O_CREAT | O_EXCL`, and `O_NOFOLLOW` on Unix).
    fn create_new(&self, path: &Path) -> io::Result<Box<dyn WriteHandle>>;
    /// POSIX `rename`; Windows `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Renames a directory only if nothing exists at `to`: a staging folder never replaces a
    /// folder that appeared at its name (`docs/TC4-DELTA-2026-10-16.md` §3.10).
    fn rename_dir_noreplace(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
    fn create_dir(&self, path: &Path) -> io::Result<()>;
    /// Makes the directory's entries durable (§5.0 step 4).
    fn sync_dir(&self, path: &Path) -> io::Result<()>;
    /// Opens a file and flushes it: on Windows the second half of making a rename durable.
    fn sync_file(&self, path: &Path) -> io::Result<()>;
    /// Takes the exclusive lock on `path` without waiting, creating the file if needed.
    /// `None` when another process holds it.
    fn lock(&self, path: &Path) -> io::Result<Option<Box<dyn LockHandle>>>;
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
}

/// The real filesystem.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdFs;

struct StdRead(File, u64);

impl ReadHandle for StdRead {
    fn len(&self) -> u64 {
        self.1
    }

    fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()> {
        read_exact_at(&self.0, buf, offset)
    }
}

#[cfg(unix)]
fn read_exact_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    std::os::unix::fs::FileExt::read_exact_at(file, buf, offset)
}

#[cfg(windows)]
fn read_exact_at(file: &File, mut buf: &mut [u8], mut offset: u64) -> io::Result<()> {
    while !buf.is_empty() {
        let n = std::os::windows::fs::FileExt::seek_read(file, buf, offset)?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        buf = &mut buf[n..];
        offset += n as u64;
    }
    Ok(())
}

struct StdWrite(File);

impl WriteHandle for StdWrite {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.0.write_all(bytes)
    }

    fn sync_all(&mut self) -> io::Result<()> {
        self.0.sync_all()
    }
}

struct StdLock(File);

impl LockHandle for StdLock {
    fn set_content(&mut self, bytes: &[u8]) -> io::Result<()> {
        use std::io::{Seek, SeekFrom};
        self.0.set_len(0)?;
        self.0.seek(SeekFrom::Start(0))?;
        self.0.write_all(bytes)?;
        self.0.sync_all()
    }
}

impl Drop for StdLock {
    fn drop(&mut self) {
        // Closing the handle releases the lock too; unlocking first says so plainly.
        let _ = self.0.unlock();
    }
}

fn kind_of(file_type: std::fs::FileType) -> EntryKind {
    if file_type.is_file() {
        EntryKind::File
    } else if file_type.is_dir() {
        EntryKind::Dir
    } else {
        EntryKind::Other
    }
}

fn mtime_ns(meta: &std::fs::Metadata) -> i64 {
    match meta.modified() {
        Ok(time) => match time.duration_since(std::time::UNIX_EPOCH) {
            Ok(after) => i64::try_from(after.as_nanos()).unwrap_or(i64::MAX),
            Err(before) => -i64::try_from(before.duration().as_nanos()).unwrap_or(i64::MAX),
        },
        Err(_) => 0,
    }
}

impl Vfs for StdFs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let mut file = File::open(path)?;
        let mut out = Vec::new();
        file.read_to_end(&mut out)?;
        Ok(out)
    }

    fn open(&self, path: &Path) -> io::Result<Box<dyn ReadHandle>> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        Ok(Box::new(StdRead(file, len)))
    }

    fn metadata(&self, path: &Path) -> io::Result<FileMeta> {
        let meta = std::fs::symlink_metadata(path)?;
        Ok(FileMeta {
            len: meta.len(),
            mtime_ns: mtime_ns(&meta),
            kind: kind_of(meta.file_type()),
        })
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            out.push(DirEntry {
                name: entry.file_name(),
                kind: kind_of(entry.file_type()?),
            });
        }
        out.sort();
        Ok(out)
    }

    fn create_new(&self, path: &Path) -> io::Result<Box<dyn WriteHandle>> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // The bit pattern of O_NOFOLLOW fits an i32 on every Unix.
            options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
        }
        Ok(Box::new(StdWrite(options.open(path)?)))
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }

    fn rename_dir_noreplace(&self, from: &Path, to: &Path) -> io::Result<()> {
        #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
        {
            use rustix::fs::{CWD, RenameFlags, renameat_with};
            match renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE) {
                Ok(()) => return Ok(()),
                // A filesystem without RENAME_NOREPLACE: fall back to the check below.
                Err(e) if e == rustix::io::Errno::INVAL || e == rustix::io::Errno::NOSYS => {}
                Err(e) => return Err(e.into()),
            }
        }
        // std::fs::rename refuses a non-empty directory at `to` on every platform; this
        // check also refuses an empty one, with a window of microseconds between the two.
        if std::fs::symlink_metadata(to).is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} exists", to.display()),
            ));
        }
        std::fs::rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_dir_all(path)
    }

    fn create_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir(path)
    }

    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // FILE_FLAG_BACKUP_SEMANTICS opens a directory; FlushFileBuffers needs write
            // access. NTFS commits a rename to its log lazily, so this flush is what makes it
            // survive a power cut (§5.0 step 4, probe §16.5 item 5).
            const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
            OpenOptions::new()
                .write(true)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
                .open(path)?
                .sync_all()
        }
        #[cfg(not(windows))]
        {
            File::open(path)?.sync_all()
        }
    }

    fn sync_file(&self, path: &Path) -> io::Result<()> {
        // FlushFileBuffers needs a handle with write access on Windows.
        #[cfg(windows)]
        let file = OpenOptions::new().write(true).open(path)?;
        #[cfg(not(windows))]
        let file = File::open(path)?;
        file.sync_all()
    }

    fn lock(&self, path: &Path) -> io::Result<Option<Box<dyn LockHandle>>> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Box::new(StdLock(file)))),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(std::fs::TryLockError::Error(e)) => Err(e),
        }
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

/// True when `error` says the file or folder is not there.
pub fn is_not_found(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::NotFound
}
