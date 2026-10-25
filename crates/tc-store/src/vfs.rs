//! Every file operation of the store goes through [`Vfs`] (`docs/STORAGE.md` §5.0): [`StdFs`]
//! in production, [`crate::fault::FaultFs`] in tests, where a model filesystem tracks what is
//! durable and what is only in the page cache, so a test can stop the writer at any write
//! point and drop what a power cut would drop (§16.2).
//!
//! Only the helpers of [`crate::atomic`] write, rename or remove project files through it.

#![expect(
    clippy::disallowed_methods,
    reason = "StdFs is the real filesystem under tc_store::atomic: the one place project files are written"
)]
use std::ffi::OsString;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

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

/// What a watch tells: the absolute paths another program changed. It is called on the
/// watcher's own thread and must not wait.
pub type WatchSink = Arc<dyn Fn(Vec<PathBuf>) + Send + Sync>;

/// A running watch. Dropping it stops the watch.
pub struct WatchGuard(#[allow(dead_code)] Box<dyn std::any::Any + Send + Sync>);

impl WatchGuard {
    pub fn new<T: Send + Sync + 'static>(inner: T) -> Self {
        Self(Box::new(inner))
    }
}

impl fmt::Debug for WatchGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WatchGuard")
    }
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
    /// Places a regular file only if nothing exists at `to`, including a concurrent
    /// creation. A hard-link fallback may retain `from` if removing it fails: the target
    /// is installed, and the caller still flushes its file and directory.
    fn rename_file_noreplace(&self, from: &Path, to: &Path) -> io::Result<()>;
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
    /// Called by [`crate::atomic::atomic_create_dirs`] once `dir` exists; `made` lists the
    /// directories that call created and flushed. A filesystem that remembers which
    /// directories it has made durable flushes the entry of every other directory on the
    /// way to `dir` that it has not flushed yet: a directory found existing may have been
    /// made by a process that stopped before it flushed its parent, and only the page cache
    /// holds its entry (§5.0). The default does nothing.
    fn secure_dir_chain(&self, _dir: &Path, _made: &[PathBuf]) -> io::Result<()> {
        Ok(())
    }
    /// Whether `error` says another program holds the file for a moment (Windows: Defender,
    /// the Search indexer and OneDrive open a file just written without `FILE_SHARE_DELETE`,
    /// and a rename or removal of it fails until they let go).
    fn is_held(&self, error: &io::Error) -> bool {
        is_held_by_another_program(error)
    }
    /// Waits before the next try of a rename or removal that was refused as held. The test
    /// filesystem records the wait instead of sleeping.
    fn wait_held(&self, delay: Duration) {
        std::thread::sleep(delay);
    }
    /// The real folder libgit2 works in for the project at `folder`, which libgit2 reaches
    /// through the operating system and not through this trait (§9, §16.2): the folder itself
    /// for a real filesystem, a real mirror folder for the test filesystem.
    fn outside_dir(&self, folder: &Path) -> io::Result<PathBuf>;
    /// Tells `sink` which paths below `root` another program changes (`recursive`: in the
    /// folders below it too), until the guard is dropped (§11.1). `Ok(None)`: this
    /// filesystem does not watch, and the person's window focus is the only trigger.
    fn watch(
        &self,
        _root: &Path,
        _recursive: bool,
        _sink: WatchSink,
    ) -> io::Result<Option<WatchGuard>> {
        Ok(None)
    }
}

/// The error of a rename or removal that another program's open handle refused. A
/// filesystem that is not Windows raises it to behave like one (the test filesystem).
#[derive(Debug, thiserror::Error)]
#[error("{} is held by another program", path.display())]
pub struct HeldByAnotherProgram {
    pub path: PathBuf,
}

impl HeldByAnotherProgram {
    /// The `io::Error` that carries this as its payload.
    pub fn error(path: &Path) -> io::Error {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            Self {
                path: path.to_owned(),
            },
        )
    }
}

/// Windows: the operating system's error 5 (access denied), 32 (sharing violation) or 33
/// (lock violation). Anywhere: an error whose payload is [`HeldByAnotherProgram`].
pub fn is_held_by_another_program(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|inner| inner.is::<HeldByAnotherProgram>())
        || (cfg!(windows) && matches!(error.raw_os_error(), Some(5 | 32 | 33)))
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

/// Claims a file name when the filesystem lacks a native no-replace rename. Once the
/// link exists, the caller must flush its directory even if the redundant source remains.
fn link_file_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::hard_link(from, to)?;
    let _ = std::fs::remove_file(from);
    Ok(())
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

    fn rename_file_noreplace(&self, from: &Path, to: &Path) -> io::Result<()> {
        if !std::fs::symlink_metadata(from)?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "rename_file_noreplace requires a regular source file",
            ));
        }
        #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
        {
            use rustix::fs::{CWD, RenameFlags, renameat_with};
            match renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE) {
                Ok(()) => return Ok(()),
                Err(e)
                    if e == rustix::io::Errno::INVAL
                        || e == rustix::io::Errno::NOSYS
                        || e == rustix::io::Errno::OPNOTSUPP => {}
                Err(e) => return Err(e.into()),
            }
        }
        link_file_noreplace(from, to)
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

    fn outside_dir(&self, folder: &Path) -> io::Result<PathBuf> {
        Ok(folder.to_owned())
    }

    fn watch(
        &self,
        root: &Path,
        recursive: bool,
        sink: WatchSink,
    ) -> io::Result<Option<WatchGuard>> {
        use notify::Watcher;
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                // An error of the watcher (a queue that overflowed) is a missed event: the
                // window focus looks at every path again.
                if let Ok(event) = event
                    && !matches!(event.kind, notify::EventKind::Access(_))
                    && !event.paths.is_empty()
                {
                    sink(event.paths);
                }
            })
            .map_err(io::Error::other)?;
        let mode = if recursive {
            notify::RecursiveMode::Recursive
        } else {
            notify::RecursiveMode::NonRecursive
        };
        watcher.watch(root, mode).map_err(io::Error::other)?;
        // The watcher is not `Sync`; a guard is only dropped.
        Ok(Some(WatchGuard::new(std::sync::Mutex::new(watcher))))
    }
}

/// True when `error` says the file or folder is not there.
pub fn is_not_found(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::NotFound
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::fault::FaultFs;

    fn write(fs: &dyn Vfs, path: &Path, bytes: &[u8]) {
        let mut file = fs.create_new(path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }

    fn occupied_target_is_kept(
        fs: &dyn Vfs,
        folder: &Path,
        claim: impl Fn(&Path, &Path) -> io::Result<()> + Sync,
    ) {
        let from = folder.join("source");
        let to = folder.join("target");
        write(fs, &from, b"new bytes");
        write(fs, &to, b"existing translator bytes");
        let error = claim(&from, &to).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs.read(&to).unwrap(), b"existing translator bytes");
        assert_eq!(fs.read(&from).unwrap(), b"new bytes");
    }

    fn fresh_target_is_installed(
        fs: &dyn Vfs,
        folder: &Path,
        claim: impl Fn(&Path, &Path) -> io::Result<()> + Sync,
    ) {
        let from = folder.join("source");
        let to = folder.join("target");
        write(fs, &from, b"new bytes");
        claim(&from, &to).unwrap();
        assert_eq!(fs.read(&to).unwrap(), b"new bytes");
        assert_eq!(
            fs.metadata(&from).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    fn concurrent_target_claims_keep_both_files(
        fs: &dyn Vfs,
        folder: &Path,
        claim: impl Fn(&Path, &Path) -> io::Result<()> + Sync,
    ) {
        let sources = [folder.join("first"), folder.join("second")];
        let contents: [&[u8]; 2] = [b"first translator bytes", b"second translator bytes"];
        let to = folder.join("target");
        for (source, bytes) in sources.iter().zip(contents) {
            write(fs, source, bytes);
        }
        let ready = std::sync::Barrier::new(2);
        let results = std::thread::scope(|scope| {
            let first = scope.spawn(|| {
                ready.wait();
                claim(&sources[0], &to)
            });
            let second = scope.spawn(|| {
                ready.wait();
                claim(&sources[1], &to)
            });
            [first.join().unwrap(), second.join().unwrap()]
        });
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        for (index, result) in results.into_iter().enumerate() {
            match result {
                Ok(()) => assert_eq!(fs.read(&to).unwrap(), contents[index]),
                Err(error) => {
                    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
                    assert_eq!(fs.read(&sources[index]).unwrap(), contents[index]);
                }
            }
        }
    }

    #[test]
    fn std_file_noreplace_keeps_an_occupied_target() {
        let folder = tempfile::tempdir().unwrap();
        occupied_target_is_kept(&StdFs, folder.path(), |from, to| {
            StdFs.rename_file_noreplace(from, to)
        });
    }

    #[test]
    fn fault_file_noreplace_keeps_an_occupied_target() {
        let fs = FaultFs::new();
        let folder = Path::new("/vfs");
        fs.create_dir(folder).unwrap();
        occupied_target_is_kept(&fs, folder, |from, to| fs.rename_file_noreplace(from, to));
    }

    #[test]
    fn std_file_noreplace_installs_a_fresh_target() {
        let folder = tempfile::tempdir().unwrap();
        fresh_target_is_installed(&StdFs, folder.path(), |from, to| {
            StdFs.rename_file_noreplace(from, to)
        });
    }

    #[test]
    fn fault_file_noreplace_installs_a_fresh_target() {
        let fs = FaultFs::new();
        let folder = Path::new("/vfs");
        fs.create_dir(folder).unwrap();
        fresh_target_is_installed(&fs, folder, |from, to| fs.rename_file_noreplace(from, to));
    }

    #[test]
    fn std_concurrent_file_noreplace_keeps_both_contenders() {
        let folder = tempfile::tempdir().unwrap();
        concurrent_target_claims_keep_both_files(&StdFs, folder.path(), |from, to| {
            StdFs.rename_file_noreplace(from, to)
        });
    }

    #[test]
    fn fault_concurrent_file_noreplace_keeps_both_contenders() {
        let fs = FaultFs::new();
        let folder = Path::new("/vfs");
        fs.create_dir(folder).unwrap();
        concurrent_target_claims_keep_both_files(&fs, folder, |from, to| {
            fs.rename_file_noreplace(from, to)
        });
    }

    #[test]
    fn native_link_fallback_keeps_an_occupied_target() {
        let folder = tempfile::tempdir().unwrap();
        occupied_target_is_kept(&StdFs, folder.path(), link_file_noreplace);
    }

    #[test]
    fn native_link_fallback_installs_a_fresh_target() {
        let folder = tempfile::tempdir().unwrap();
        fresh_target_is_installed(&StdFs, folder.path(), link_file_noreplace);
    }

    #[test]
    fn native_link_fallback_keeps_both_concurrent_contenders() {
        let folder = tempfile::tempdir().unwrap();
        concurrent_target_claims_keep_both_files(&StdFs, folder.path(), link_file_noreplace);
    }

    /// A successful link is an installation even if source cleanup is denied. Separate
    /// parents let the test deny only cleanup, while the target remains writable.
    #[cfg(unix)]
    #[test]
    fn native_link_fallback_keeps_the_installation_when_source_cleanup_is_denied() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let folder = tempfile::tempdir().unwrap();
        let source_dir = folder.path().join("source-dir");
        let target_dir = folder.path().join("target-dir");
        std::fs::create_dir(&source_dir).unwrap();
        std::fs::create_dir(&target_dir).unwrap();
        let source_meta = std::fs::metadata(&source_dir).unwrap();
        assert_ne!(
            source_meta.uid(),
            0,
            "cleanup-denial proof requires an unprivileged user; root bypasses mode bits"
        );
        let original_permissions = source_meta.permissions();
        let from = source_dir.join("source");
        let to = target_dir.join("target");
        let bytes = b"translator bytes survive denied cleanup";
        write(&StdFs, &from, bytes);

        std::fs::set_permissions(&source_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let denied_removal = std::fs::remove_file(&from);
        let installed = if denied_removal
            .as_ref()
            .is_err_and(|error| error.kind() == io::ErrorKind::PermissionDenied)
        {
            Some(link_file_noreplace(&from, &to))
        } else {
            None
        };
        let source_bytes = std::fs::read(&from);
        let target_bytes = std::fs::read(&to);
        // Restore access before any assertions, including unexpected permission behavior.
        let restored = std::fs::set_permissions(&source_dir, original_permissions);

        restored.unwrap();
        assert_eq!(
            denied_removal.unwrap_err().kind(),
            io::ErrorKind::PermissionDenied,
            "the setup must really deny source cleanup"
        );
        installed
            .expect("the cleanup-denial precondition was established")
            .unwrap();
        assert_eq!(source_bytes.unwrap(), bytes);
        assert_eq!(target_bytes.unwrap(), bytes);
    }

    #[test]
    fn fault_file_noreplace_waits_for_either_held_path() {
        let fs = FaultFs::new();
        let folder = Path::new("/vfs");
        fs.create_dir(folder).unwrap();
        let from = folder.join("source");
        let to = folder.join("target");
        write(&fs, &from, b"new bytes");
        for held in [&from, &to] {
            fs.set_busy(held, true);
            let error = fs.rename_file_noreplace(&from, &to).unwrap_err();
            assert!(fs.is_held(&error));
            assert_eq!(fs.read(&from).unwrap(), b"new bytes");
            assert_eq!(
                fs.metadata(&to).unwrap_err().kind(),
                io::ErrorKind::NotFound
            );
            fs.set_busy(held, false);
        }
        fs.rename_file_noreplace(&from, &to).unwrap();
        assert_eq!(fs.read(&to).unwrap(), b"new bytes");
    }
}
