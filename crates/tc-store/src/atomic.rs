//! The only helpers that write, rename or remove project files (`docs/STORAGE.md` §5.0, I6):
//! a file is replaced by renaming a complete, flushed temporary file over it, and the rename
//! is flushed before anyone is told the write happened.
//!
//! On Windows a scanner (Defender, the Search indexer, OneDrive) can hold a file just written
//! open without `FILE_SHARE_DELETE`, and a rename or removal of it fails with error 5, 32 or
//! 33 for a moment. Every rename and removal here is retried with a bounded back-off
//! ([`HeldRetry`]) while the error says the file is held.

use crate::error::{IoContext, StoreError};
use crate::vfs::{Vfs, is_not_found};
use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long a rename or removal that another program holds is retried: the milliseconds to
/// wait before each further try.
#[derive(Debug, Clone, Copy)]
pub struct HeldRetry(&'static [u64]);

impl HeldRetry {
    /// A save, a seal, a consolidation, the open and a staging folder: ten more tries,
    /// 2,004 ms in all. A scanner almost always lets go within that, and a save that cannot
    /// be written still fails within about two seconds.
    pub const SAVE: HeldRetry = HeldRetry(&[2, 4, 8, 16, 32, 64, 128, 250, 500, 1000]);
    /// A derived file, which the writer retries later on its own (a busy path, §5.10): three
    /// more tries, 35 ms in all, so one held file never stalls the other files of a book.
    pub const BACKGROUND: HeldRetry = HeldRetry(&[5, 10, 20]);

    /// The most time the waits add up to.
    pub fn total(&self) -> Duration {
        Duration::from_millis(self.0.iter().sum())
    }
}

/// Runs `op` again, after the next wait, while it fails with an error that says another
/// program holds the file. The last error is returned when the waits are used up.
fn retry_held<T>(
    vfs: &dyn Vfs,
    retry: &HeldRetry,
    mut op: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
    let mut waits = retry.0.iter();
    loop {
        match op() {
            Err(e) if vfs.is_held(&e) => match waits.next() {
                Some(ms) => vfs.wait_held(Duration::from_millis(*ms)),
                None => return Err(e),
            },
            other => return other,
        }
    }
}

/// `Vfs::rename`, retried while another program holds either file.
pub fn rename_held(vfs: &dyn Vfs, from: &Path, to: &Path, retry: &HeldRetry) -> io::Result<()> {
    retry_held(vfs, retry, || vfs.rename(from, to))
}

/// `Vfs::read`, retried while another program holds the file.
pub fn read_held(vfs: &dyn Vfs, path: &Path, retry: &HeldRetry) -> io::Result<Vec<u8>> {
    retry_held(vfs, retry, || vfs.read(path))
}

/// `Vfs::remove_file`, retried while another program holds the file.
pub fn remove_held(vfs: &dyn Vfs, path: &Path, retry: &HeldRetry) -> io::Result<()> {
    retry_held(vfs, retry, || vfs.remove_file(path))
}

/// `Vfs::remove_dir_all`, retried while another program holds a file in it.
pub fn remove_dir_all_held(vfs: &dyn Vfs, path: &Path, retry: &HeldRetry) -> io::Result<()> {
    retry_held(vfs, retry, || vfs.remove_dir_all(path))
}

/// `Vfs::rename_dir_noreplace`, retried while another program holds a file in the folder.
pub fn rename_dir_noreplace_held(
    vfs: &dyn Vfs,
    from: &Path,
    to: &Path,
    retry: &HeldRetry,
) -> io::Result<()> {
    retry_held(vfs, retry, || vfs.rename_dir_noreplace(from, to))
}

/// Why [`atomic_write`] did not complete.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// The target is unchanged.
    #[error("not written: {0}")]
    NotWritten(StoreError),
    /// The new bytes are at the target, but a flush after the rename failed, so a power cut
    /// may still lose them. The caller treats the file as written.
    #[error("written, but the disk reported an error after it: {0}")]
    WrittenNotDurable(StoreError),
}

impl WriteError {
    /// The error inside, whichever it is.
    pub fn into_inner(self) -> StoreError {
        match self {
            Self::NotWritten(e) | Self::WrittenNotDurable(e) => e,
        }
    }
}

/// When the rename is flushed: at once, or once for a batch of renames.
pub enum SyncDir<'a> {
    Now,
    Deferred(&'a mut DirSet),
}

/// Directories (and, on Windows, renamed files) to flush once, after a batch of renames or
/// removals (§5.10 M4 and M5, §5.12 Q4).
#[derive(Debug, Default)]
pub struct DirSet {
    dirs: BTreeSet<PathBuf>,
    files: BTreeSet<PathBuf>,
}

impl DirSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_dir(&mut self, dir: &Path) {
        self.dirs.insert(dir.to_owned());
    }

    fn add_renamed(&mut self, dir: &Path, file: &Path) {
        self.dirs.insert(dir.to_owned());
        if cfg!(windows) {
            self.files.insert(file.to_owned());
        }
    }

    pub fn is_empty(&self) -> bool {
        self.dirs.is_empty()
    }

    /// Flushes every directory once, then every renamed file (Windows). The set is empty
    /// after.
    pub fn sync(&mut self, vfs: &dyn Vfs) -> Result<(), StoreError> {
        let dirs = std::mem::take(&mut self.dirs);
        let files = std::mem::take(&mut self.files);
        for dir in &dirs {
            vfs.sync_dir(dir).at("flush the folder", dir)?;
        }
        for file in &files {
            vfs.sync_file(file).at("flush the file", file)?;
        }
        Ok(())
    }
}

/// The name of a temporary file for `name`: `.~<name>.<16 hex>.tmp` (§3.3), never listed in
/// `metadata.json` and never another file's name.
pub fn temp_name(name: &str) -> String {
    format!(".~{name}.{}.tmp", crate::random::hex::<8>())
}

/// Whether `name` is a temporary file of [`temp_name`]'s form, and if so the name it was
/// for.
pub fn temp_target(name: &str) -> Option<&str> {
    let rest = name.strip_prefix(".~")?.strip_suffix(".tmp")?;
    let (target, random) = rest.rsplit_once('.')?;
    (random.len() == 16 && random.bytes().all(|b| b.is_ascii_hexdigit()) && !target.is_empty())
        .then_some(target)
}

/// Replaces or creates `dir/name` atomically and durably (§5.0):
///
/// 1. create `dir/.~<name>.<16 hex>.tmp` as a new file, in the same directory;
/// 2. write every byte and flush the file;
/// 3. rename it over `dir/name`;
/// 4. flush the rename: now, or once for a batch ([`SyncDir::Deferred`]). On Windows the
///    renamed file is flushed again after its directory.
///
/// The rename is retried while another program holds a file ([`HeldRetry::SAVE`]). An error
/// before the rename removes the temporary file (retried like the rename; when a scanner
/// still holds it, it stays for the next open to decide, §6.1 O2) and leaves the target
/// unchanged: [`WriteError::NotWritten`]. An error at step 4 leaves the new bytes in place:
/// [`WriteError::WrittenNotDurable`].
pub fn atomic_write(
    vfs: &dyn Vfs,
    dir: &Path,
    name: &str,
    bytes: &[u8],
    sync_dir: SyncDir<'_>,
) -> Result<(), WriteError> {
    atomic_write_with(vfs, dir, name, bytes, sync_dir, &HeldRetry::SAVE)
}

/// Creates a complete durable file without replacing any existing name. Preservation
/// copies use this so a competing creator cannot turn a free name into overwritten work.
pub fn atomic_create(
    vfs: &dyn Vfs,
    dir: &Path,
    name: &str,
    bytes: &[u8],
    sync_dir: SyncDir<'_>,
) -> Result<(), WriteError> {
    atomic_write_streamed_to(
        vfs,
        dir,
        name,
        &mut |out| out(bytes).at("write", dir),
        sync_dir,
        &HeldRetry::SAVE,
        Destination::New,
    )
    .map(|_| ())
}

/// [`atomic_write`] with the retry policy of the caller.
pub fn atomic_write_with(
    vfs: &dyn Vfs,
    dir: &Path,
    name: &str,
    bytes: &[u8],
    sync_dir: SyncDir<'_>,
    retry: &HeldRetry,
) -> Result<(), WriteError> {
    atomic_write_checked(vfs, dir, name, bytes, sync_dir, retry, &mut || Ok(true)).map(|_| ())
}

/// What [`atomic_write_checked`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written {
    Renamed,
    /// The last look refused the rename: the temp file is removed and the target unchanged.
    Refused,
}

/// [`atomic_write_with`] with a last look before each rename attempt: `before_rename` runs
/// after the temp file is complete and flushed, and `Ok(false)` removes it and leaves
/// the target as it is (the compare-and-swap of §5.10 M4 looks at the file again at the
/// moment that matters, I8).
pub fn atomic_write_checked(
    vfs: &dyn Vfs,
    dir: &Path,
    name: &str,
    bytes: &[u8],
    sync_dir: SyncDir<'_>,
    retry: &HeldRetry,
    before_rename: &mut dyn FnMut() -> Result<bool, StoreError>,
) -> Result<Written, WriteError> {
    atomic_write_streamed(
        vfs,
        dir,
        name,
        &mut |out| out(bytes).at("write", dir),
        sync_dir,
        retry,
        before_rename,
    )
}

/// What a streamed write is given to put bytes into the new file.
pub type Sink<'a> = dyn FnMut(&[u8]) -> io::Result<()> + 'a;

/// [`atomic_write_checked`] for bytes that are not in memory (a carried file of any size, §11.4):
/// `fill` hands the file its bytes in chunks through the sink it is given. The file is
/// flushed, looked at once more by `before_rename`, and renamed, as any other.
pub fn atomic_write_streamed(
    vfs: &dyn Vfs,
    dir: &Path,
    name: &str,
    fill: &mut dyn FnMut(&mut Sink<'_>) -> Result<(), StoreError>,
    sync_dir: SyncDir<'_>,
    retry: &HeldRetry,
    before_rename: &mut dyn FnMut() -> Result<bool, StoreError>,
) -> Result<Written, WriteError> {
    atomic_write_streamed_to(
        vfs,
        dir,
        name,
        fill,
        sync_dir,
        retry,
        Destination::Replace(before_rename),
    )
}

enum Destination<'a> {
    New,
    Replace(&'a mut dyn FnMut() -> Result<bool, StoreError>),
}

fn atomic_write_streamed_to(
    vfs: &dyn Vfs,
    dir: &Path,
    name: &str,
    fill: &mut dyn FnMut(&mut Sink<'_>) -> Result<(), StoreError>,
    sync_dir: SyncDir<'_>,
    retry: &HeldRetry,
    destination: Destination<'_>,
) -> Result<Written, WriteError> {
    let temp = dir.join(temp_name(name));
    let target = dir.join(name);
    let written = (|| {
        let mut file = vfs.create_new(&temp).at("create", &temp)?;
        fill(&mut |chunk: &[u8]| file.write_all(chunk))?;
        file.sync_all().at("flush", &temp)?;
        drop(file);
        match destination {
            Destination::Replace(before_rename) => {
                let mut waits = retry.0.iter();
                loop {
                    if !before_rename()? {
                        return Ok(false);
                    }
                    match vfs.rename(&temp, &target) {
                        Err(e) if vfs.is_held(&e) => match waits.next() {
                            Some(ms) => vfs.wait_held(Duration::from_millis(*ms)),
                            None => return Err(StoreError::io("rename", &target, e)),
                        },
                        result => {
                            result.at("rename", &target)?;
                            break;
                        }
                    }
                }
            }
            Destination::New => {
                retry_held(vfs, retry, || vfs.rename_file_noreplace(&temp, &target))
                    .at("create without replacement", &target)?;
            }
        }
        Ok(true)
    })();
    match written {
        Ok(true) => {}
        Ok(false) => {
            let _ = remove_held(vfs, &temp, retry);
            return Ok(Written::Refused);
        }
        Err(error) => {
            // A temp that cannot be removed stays; a temp that was never created is not
            // there.
            let _ = remove_held(vfs, &temp, retry);
            return Err(WriteError::NotWritten(error));
        }
    }
    match sync_dir {
        SyncDir::Now => {
            vfs.sync_dir(dir)
                .at("flush the folder", dir)
                .map_err(WriteError::WrittenNotDurable)?;
            if cfg!(windows) {
                vfs.sync_file(&target)
                    .at("flush the file", &target)
                    .map_err(WriteError::WrittenNotDurable)?;
            }
        }
        SyncDir::Deferred(set) => set.add_renamed(dir, &target),
    }
    Ok(Written::Renamed)
}

/// Creates each missing directory of `path`, flushing its parent after each one. A
/// directory that already exists is not trusted to be durable: its creator may have stopped
/// before it flushed the parent, and a seal that wrote a pack into it would then remove the
/// pending files that pack holds while a power cut could still take the folder and the pack
/// with it. A filesystem that remembers what it has flushed ([`Vfs::secure_dir_chain`])
/// flushes the entries of the directories found, once.
pub fn atomic_create_dirs(vfs: &dyn Vfs, path: &Path) -> Result<(), StoreError> {
    let mut missing = Vec::new();
    let mut at = Some(path);
    while let Some(dir) = at {
        if vfs.metadata(dir).is_ok() {
            break;
        }
        missing.push(dir.to_owned());
        at = dir.parent();
    }
    let mut made = Vec::new();
    for dir in missing.into_iter().rev() {
        match vfs.create_dir(&dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(StoreError::io("create the folder", &dir, e)),
        }
        if let Some(parent) = dir.parent() {
            vfs.sync_dir(parent).at("flush the folder", parent)?;
        }
        made.push(dir);
    }
    vfs.secure_dir_chain(path, &made)
        .at("flush the folder", path)
}

/// Removes a file (retried while another program holds it, [`HeldRetry::SAVE`]) and flushes
/// its directory, now or once for a batch.
pub fn atomic_remove(vfs: &dyn Vfs, path: &Path, sync_dir: SyncDir<'_>) -> Result<(), StoreError> {
    atomic_remove_with(vfs, path, sync_dir, &HeldRetry::SAVE)
}

/// Removes a file that only waits to go (a pending file a seal has sealed, a pack a
/// consolidation replaced): `Ok(false)` when another program still holds it after the
/// retries, and the file then stays and the next open removes it (§6.1 O7); `Ok(false)` too
/// when another program removed it first, which is what the caller wanted done.
pub fn atomic_remove_if_free(
    vfs: &dyn Vfs,
    path: &Path,
    sync_dir: SyncDir<'_>,
) -> Result<bool, StoreError> {
    match atomic_remove_with(vfs, path, sync_dir, &HeldRetry::SAVE) {
        Ok(()) => Ok(true),
        Err(e)
            if e.io_error()
                .is_some_and(|io| vfs.is_held(io) || is_not_found(io)) =>
        {
            Ok(false)
        }
        Err(e) => Err(e),
    }
}

/// [`atomic_remove`] with the retry policy of the caller.
pub fn atomic_remove_with(
    vfs: &dyn Vfs,
    path: &Path,
    sync_dir: SyncDir<'_>,
    retry: &HeldRetry,
) -> Result<(), StoreError> {
    remove_held(vfs, path, retry).at("remove", path)?;
    let Some(dir) = path.parent() else {
        return Ok(());
    };
    match sync_dir {
        SyncDir::Now => vfs.sync_dir(dir).at("flush the folder", dir),
        SyncDir::Deferred(set) => {
            set.add_dir(dir);
            Ok(())
        }
    }
}

/// What [`atomic_remove_checked`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removed {
    Removed,
    /// The last look refused the removal, leaving the target unchanged.
    Refused,
}

/// Removes a file only while the caller's last look still permits it. A held file may
/// change during the wait, so the check runs before every removal attempt.
pub fn atomic_remove_checked(
    vfs: &dyn Vfs,
    path: &Path,
    sync_dir: SyncDir<'_>,
    retry: &HeldRetry,
    before_remove: &mut dyn FnMut() -> Result<bool, StoreError>,
) -> Result<Removed, StoreError> {
    let mut waits = retry.0.iter();
    loop {
        if !before_remove()? {
            return Ok(Removed::Refused);
        }
        match vfs.remove_file(path) {
            Err(e) if vfs.is_held(&e) => match waits.next() {
                Some(ms) => vfs.wait_held(Duration::from_millis(*ms)),
                None => return Err(StoreError::io("remove", path, e)),
            },
            result => {
                result.at("remove", path)?;
                break;
            }
        }
    }
    if let Some(dir) = path.parent() {
        match sync_dir {
            SyncDir::Now => vfs.sync_dir(dir).at("flush the folder", dir)?,
            SyncDir::Deferred(set) => set.add_dir(dir),
        }
    }
    Ok(Removed::Removed)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::disallowed_methods,
        reason = "test code: scratch files and another program's handle, never a project"
    )]
    use super::*;
    use crate::fault::{FaultFs, PowerLoss};

    fn setup() -> FaultFs {
        let fs = FaultFs::new();
        atomic_create_dirs(&fs, Path::new("/p/.tc")).unwrap();
        fs
    }

    #[test]
    fn temp_names_are_recognized() {
        let name = temp_name("a.batch");
        assert_eq!(temp_target(&name), Some("a.batch"));
        assert_eq!(temp_target("a.batch"), None);
        assert_eq!(temp_target(".~a.batch.zz.tmp"), None);
    }

    #[test]
    fn a_removal_rechecks_bytes_after_a_sharing_wait() {
        let fs = setup();
        let path = Path::new("/p/.tc/f");
        atomic_write(&fs, Path::new("/p/.tc"), "f", b"old", SyncDir::Now).unwrap();
        fs.set_busy(path, true);
        let mut looks = 0;
        let result =
            atomic_remove_checked(&fs, path, SyncDir::Now, &HeldRetry::BACKGROUND, &mut || {
                looks += 1;
                if looks == 2 {
                    fs.put_unseen(path, b"outside edit", None);
                    fs.set_busy(path, false);
                }
                Ok(fs.peek(path).as_deref() == Some(b"old"))
            })
            .unwrap();
        assert_eq!(result, Removed::Refused);
        assert_eq!(fs.peek(path).as_deref(), Some(b"outside edit".as_slice()));
        assert_eq!(fs.held_waits(), [Duration::from_millis(5)]);
    }

    #[test]
    fn a_checked_write_rechecks_bytes_after_a_sharing_wait() {
        use crate::vfs::{DirEntry, FileMeta, LockHandle, ReadHandle, WriteHandle};

        #[derive(Debug)]
        struct EditWhileWaiting {
            fs: FaultFs,
            target: PathBuf,
        }

        impl Vfs for EditWhileWaiting {
            fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
                self.fs.read(path)
            }
            fn open(&self, path: &Path) -> io::Result<Box<dyn ReadHandle>> {
                self.fs.open(path)
            }
            fn metadata(&self, path: &Path) -> io::Result<FileMeta> {
                self.fs.metadata(path)
            }
            fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>> {
                self.fs.read_dir(path)
            }
            fn create_new(&self, path: &Path) -> io::Result<Box<dyn WriteHandle>> {
                self.fs.create_new(path)
            }
            fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
                self.fs.rename(from, to)
            }
            fn rename_file_noreplace(&self, from: &Path, to: &Path) -> io::Result<()> {
                self.fs.rename_file_noreplace(from, to)
            }
            fn rename_dir_noreplace(&self, from: &Path, to: &Path) -> io::Result<()> {
                self.fs.rename_dir_noreplace(from, to)
            }
            fn remove_file(&self, path: &Path) -> io::Result<()> {
                self.fs.remove_file(path)
            }
            fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
                self.fs.remove_dir_all(path)
            }
            fn create_dir(&self, path: &Path) -> io::Result<()> {
                self.fs.create_dir(path)
            }
            fn sync_dir(&self, path: &Path) -> io::Result<()> {
                self.fs.sync_dir(path)
            }
            fn sync_file(&self, path: &Path) -> io::Result<()> {
                self.fs.sync_file(path)
            }
            fn lock(&self, path: &Path) -> io::Result<Option<Box<dyn LockHandle>>> {
                self.fs.lock(path)
            }
            fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
                self.fs.canonicalize(path)
            }
            fn outside_dir(&self, path: &Path) -> io::Result<PathBuf> {
                self.fs.outside_dir(path)
            }
            fn wait_held(&self, delay: Duration) {
                self.fs.wait_held(delay);
                assert_eq!(
                    self.fs.peek(&self.target).as_deref(),
                    Some(b"old".as_slice())
                );
                self.fs.put_unseen(&self.target, b"outside edit", None);
                self.fs.set_busy(&self.target, false);
            }
        }

        let fs = setup();
        let dir = Path::new("/p/.tc");
        let path = dir.join("f");
        atomic_write(&fs, dir, "f", b"old", SyncDir::Now).unwrap();
        fs.set_busy(&path, true);
        let vfs = EditWhileWaiting {
            fs: fs.clone(),
            target: path.clone(),
        };
        let mut looks = 0;
        let result = atomic_write_checked(
            &vfs,
            dir,
            "f",
            b"project's new text",
            SyncDir::Now,
            &HeldRetry::BACKGROUND,
            &mut || {
                looks += 1;
                Ok(fs.peek(&path).as_deref() == Some(b"old".as_slice()))
            },
        )
        .unwrap();
        assert_eq!(result, Written::Refused);
        assert_eq!(looks, 2);
        assert_eq!(fs.held_waits(), [Duration::from_millis(5)]);
        assert_eq!(fs.peek(&path).as_deref(), Some(b"outside edit".as_slice()));
    }

    #[test]
    fn creating_a_copy_never_replaces_an_occupied_name() {
        let fs = setup();
        let dir = Path::new("/p/.tc");
        atomic_create(&fs, dir, "copy", b"earlier work", SyncDir::Now).unwrap();
        let error = atomic_create(&fs, dir, "copy", b"later work", SyncDir::Now).unwrap_err();
        assert!(matches!(error, WriteError::NotWritten(_)));
        assert_eq!(
            fs.peek(&dir.join("copy")).as_deref(),
            Some(b"earlier work".as_slice())
        );
        assert_eq!(
            fs.crash(true).peek(&dir.join("copy")).as_deref(),
            Some(b"earlier work".as_slice())
        );
    }

    #[test]
    fn every_create_kill_point_leaves_an_absent_or_complete_durable_copy() {
        const KEPT: &[u8] = b"translator bytes kept before any replacement";
        let dir = Path::new("/p/.tc");
        let target = dir.join("copy");
        for power in [PowerLoss::Strict, PowerLoss::KeepSyncedNames] {
            let base = setup();
            base.set_power_loss(power);
            let probe = base.copy();
            let before = probe.ops();
            atomic_create(&probe, dir, "copy", KEPT, SyncDir::Now).unwrap();
            let points = probe.ops() - before;
            for k in 0..=points {
                for cut in [false, true] {
                    for torn in [false, true] {
                        let fs = base.copy();
                        fs.set_power_loss(power);
                        fs.kill_before(k, torn);
                        let result = atomic_create(&fs, dir, "copy", KEPT, SyncDir::Now);
                        let after = fs.crash(cut);
                        let got = after.peek(&target);
                        if result.is_ok() {
                            assert_eq!(got.as_deref(), Some(KEPT), "k={k} cut={cut}");
                        } else {
                            assert!(
                                got.is_none() || got.as_deref() == Some(KEPT),
                                "partial copy at k={k} cut={cut} torn={torn}: {got:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_create_flush_error_after_placement_counts_as_written() {
        let fs = setup();
        let dir = Path::new("/p/.tc");
        fs.fail_op(4, io::ErrorKind::Other);
        let result = atomic_create(&fs, dir, "copy", b"outside edit", SyncDir::Now);
        assert!(matches!(result, Err(WriteError::WrittenNotDurable(_))));
        assert_eq!(
            fs.peek(&dir.join("copy")).as_deref(),
            Some(b"outside edit".as_slice())
        );
        let result = atomic_create(&fs, dir, "copy", b"later bytes", SyncDir::Now);
        assert!(matches!(result, Err(WriteError::NotWritten(_))));
        assert_eq!(
            fs.peek(&dir.join("copy")).as_deref(),
            Some(b"outside edit".as_slice())
        );
    }

    /// Whatever write point the writer stops at, with or without a power cut, the target
    /// holds the old bytes or the new ones, and once `atomic_write` returned Ok the new
    /// bytes survive a power cut.
    #[test]
    fn every_kill_point_leaves_the_old_or_the_new_bytes() {
        for power in [PowerLoss::Strict, PowerLoss::KeepSyncedNames] {
            let base = setup();
            base.set_power_loss(power);
            atomic_write(&base, Path::new("/p/.tc"), "f", b"old", SyncDir::Now).unwrap();
            let probe = base.copy();
            let before = probe.ops();
            atomic_write(&probe, Path::new("/p/.tc"), "f", b"new!", SyncDir::Now).unwrap();
            let points = probe.ops() - before;
            for k in 0..=points {
                for cut in [false, true] {
                    for torn in [false, true] {
                        let fs = base.copy();
                        fs.set_power_loss(power);
                        fs.kill_before(k, torn);
                        let result =
                            atomic_write(&fs, Path::new("/p/.tc"), "f", b"new!", SyncDir::Now);
                        let after = fs.crash(cut);
                        let got = after.peek(Path::new("/p/.tc/f"));
                        if result.is_ok() {
                            assert_eq!(got.as_deref(), Some(&b"new!"[..]), "k={k} cut={cut}");
                        } else {
                            assert!(
                                got.as_deref() == Some(b"old") || got.as_deref() == Some(b"new!"),
                                "k={k} cut={cut} {got:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_flush_error_after_the_rename_counts_as_written() {
        let fs = setup();
        // create, write, flush, rename, then the folder flush fails.
        fs.fail_op(4, std::io::ErrorKind::Other);
        let result = atomic_write(&fs, Path::new("/p/.tc"), "f", b"x", SyncDir::Now);
        assert!(matches!(result, Err(WriteError::WrittenNotDurable(_))));
        assert_eq!(fs.peek(Path::new("/p/.tc/f")), Some(b"x".to_vec()));
    }

    /// A directory a writer made and stopped before it flushed the parent exists in the page
    /// cache only. `atomic_create_dirs` on the project's filesystem flushes its entry (once,
    /// whatever the folders below it): a power cut afterwards keeps the folder, and what was
    /// written into it.
    #[test]
    fn a_directory_found_existing_is_made_durable_once() {
        use crate::counters::{Counters, CountingVfs};
        use std::sync::Arc;
        let fs = FaultFs::new();
        fs.set_power_loss(PowerLoss::Strict);
        fs.create_dir(Path::new("/p")).unwrap();
        fs.sync_dir(Path::new("/")).unwrap();
        // The writer stopped here: both folders exist, neither entry was flushed.
        fs.create_dir(Path::new("/p/a")).unwrap();
        fs.create_dir(Path::new("/p/a/b")).unwrap();
        let counters = Arc::new(Counters::default());
        let project = CountingVfs::new(Arc::new(fs.clone()), counters.clone())
            .for_project(Path::new("/p").to_owned());
        atomic_create_dirs(&project, Path::new("/p/a/b")).unwrap();
        atomic_write(&project, Path::new("/p/a/b"), "f", b"saved", SyncDir::Now).unwrap();
        assert_eq!(
            fs.crash(true).peek(Path::new("/p/a/b/f")),
            Some(b"saved".to_vec()),
            "the folders that hold the file are durable"
        );
        // Remembered: the same folders cost no flush again.
        let flushes = counters.read().fsyncs;
        atomic_create_dirs(&project, Path::new("/p/a/b")).unwrap();
        atomic_create_dirs(&project, Path::new("/p/a")).unwrap();
        assert_eq!(counters.read().fsyncs, flushes);
        // A filesystem that remembers nothing does not flush (the default).
        let plain = FaultFs::new();
        plain.create_dir(Path::new("/p")).unwrap();
        plain.create_dir(Path::new("/p/a")).unwrap();
        atomic_create_dirs(&plain, Path::new("/p/a")).unwrap();
        assert_eq!(
            plain
                .log()
                .iter()
                .filter(|o| o.kind == crate::fault::OpKind::SyncDir)
                .count(),
            0
        );
    }

    #[test]
    fn a_failure_before_the_rename_leaves_no_temp_file() {
        let fs = setup();
        fs.fail_op(1, std::io::ErrorKind::StorageFull);
        let result = atomic_write(&fs, Path::new("/p/.tc"), "f", b"x", SyncDir::Now);
        assert!(matches!(result, Err(WriteError::NotWritten(_))));
        assert!(fs.files_under(Path::new("/p/.tc")).is_empty());
    }

    /// A scanner that lets go after a few tries: the write succeeds after exactly that many
    /// waits, which follow the back-off.
    #[test]
    fn a_held_rename_is_retried_within_the_bound() {
        let fs = setup();
        fs.hold_new_files(Path::new("/p/.tc"), Some(4));
        atomic_write(&fs, Path::new("/p/.tc"), "f", b"x", SyncDir::Now).unwrap();
        assert_eq!(fs.peek(Path::new("/p/.tc/f")), Some(b"x".to_vec()));
        let waits: Vec<u64> = fs
            .held_waits()
            .iter()
            .map(|d| u64::try_from(d.as_millis()).unwrap())
            .collect();
        assert_eq!(waits, [2, 4, 8, 16]);
        assert_eq!(HeldRetry::SAVE.total(), Duration::from_millis(2_004));
    }

    /// A scanner that never lets go: the write fails after the bound, the target is
    /// unchanged, the removal of the temp was retried like the rename, and the temp stays.
    #[test]
    fn a_held_rename_past_the_bound_is_not_written_and_its_temp_removal_was_retried() {
        let fs = setup();
        atomic_write(&fs, Path::new("/p/.tc"), "f", b"old", SyncDir::Now).unwrap();
        fs.hold_new_files(Path::new("/p/.tc"), None);
        let result = atomic_write(&fs, Path::new("/p/.tc"), "f", b"new", SyncDir::Now);
        assert!(matches!(result, Err(WriteError::NotWritten(_))));
        assert_eq!(fs.peek(Path::new("/p/.tc/f")), Some(b"old".to_vec()));
        assert_eq!(fs.held_waits().len(), 20);
        let temps: Vec<_> = fs
            .files_under(Path::new("/p/.tc"))
            .into_keys()
            .filter(|p| p.to_string_lossy().contains(".~"))
            .collect();
        assert_eq!(temps.len(), 1, "{temps:?}");
    }

    /// The background policy waits 35 ms in all.
    #[test]
    fn a_background_write_gives_up_after_three_short_waits() {
        let fs = setup();
        fs.hold_new_files(Path::new("/p/.tc"), None);
        let result = atomic_write_with(
            &fs,
            Path::new("/p/.tc"),
            "f",
            b"x",
            SyncDir::Now,
            &HeldRetry::BACKGROUND,
        );
        assert!(matches!(result, Err(WriteError::NotWritten(_))));
        assert_eq!(HeldRetry::BACKGROUND.total(), Duration::from_millis(35));
        assert_eq!(fs.held_waits().len(), 6);
    }

    /// What Defender, the Search indexer and OneDrive do: another program has the target open
    /// for reading and writing without `FILE_SHARE_DELETE`, so the replacing rename fails
    /// with error 5 or 32 until they let go. The write waits and then succeeds. Windows only;
    /// CI does not run it, the owner's Windows runs do.
    #[cfg(windows)]
    #[test]
    fn a_rename_waits_for_a_handle_without_share_delete() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("f");
        std::fs::write(&target, b"old").unwrap();
        // FILE_SHARE_READ | FILE_SHARE_WRITE, not FILE_SHARE_DELETE.
        let scanner = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(1 | 2)
            .open(&target)
            .unwrap();
        let lets_go = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            drop(scanner);
        });
        atomic_write(&crate::vfs::StdFs, dir.path(), "f", b"new", SyncDir::Now).unwrap();
        lets_go.join().unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
    }
}
