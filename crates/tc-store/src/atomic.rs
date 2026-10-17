//! The only helpers that write project files (`docs/STORAGE.md` §5.0, I6): a file is
//! replaced by renaming a complete, flushed temporary file over it, and the rename is
//! flushed before anyone is told the write happened.

use crate::error::{IoContext, StoreError};
use crate::vfs::Vfs;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

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
/// An error before the rename removes the temporary file (best effort) and leaves the
/// target unchanged: [`WriteError::NotWritten`]. An error at step 4 leaves the new bytes in
/// place: [`WriteError::WrittenNotDurable`].
pub fn atomic_write(
    vfs: &dyn Vfs,
    dir: &Path,
    name: &str,
    bytes: &[u8],
    sync_dir: SyncDir<'_>,
) -> Result<(), WriteError> {
    let temp = dir.join(temp_name(name));
    let target = dir.join(name);
    let written = (|| {
        let mut file = vfs.create_new(&temp).at("create", &temp)?;
        file.write_all(bytes).at("write", &temp)?;
        file.sync_all().at("flush", &temp)?;
        drop(file);
        vfs.rename(&temp, &target).at("rename", &target)
    })();
    if let Err(error) = written {
        let _ = vfs.remove_file(&temp);
        return Err(WriteError::NotWritten(error));
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
    Ok(())
}

/// Creates each missing directory of `path`, flushing its parent after each one.
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
    for dir in missing.into_iter().rev() {
        match vfs.create_dir(&dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(StoreError::io("create the folder", &dir, e)),
        }
        if let Some(parent) = dir.parent() {
            vfs.sync_dir(parent).at("flush the folder", parent)?;
        }
    }
    Ok(())
}

/// Removes a file and flushes its directory, now or once for a batch.
pub fn atomic_remove(vfs: &dyn Vfs, path: &Path, sync_dir: SyncDir<'_>) -> Result<(), StoreError> {
    vfs.remove_file(path).at("remove", path)?;
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

#[cfg(test)]
mod tests {
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

    #[test]
    fn a_failure_before_the_rename_leaves_no_temp_file() {
        let fs = setup();
        fs.fail_op(1, std::io::ErrorKind::StorageFull);
        let result = atomic_write(&fs, Path::new("/p/.tc"), "f", b"x", SyncDir::Now);
        assert!(matches!(result, Err(WriteError::NotWritten(_))));
        assert!(fs.files_under(Path::new("/p/.tc")).is_empty());
    }
}
