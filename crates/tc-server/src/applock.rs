//! The app-data lock of the restart case (APP-DESIGN §5.1, AD39).
//!
//! The single instance on a machine is the shell's lock. This one covers a `tc4` that is
//! still closing after a crash or after its shell died: a new `tc4` waits for it, up to 10 s,
//! and then names it. `app.lock` is an operating-system lock the process holds until it ends,
//! a crash or a kill included; it is never read, renamed or deleted. The holder's pid is in
//! `app-lock.json` beside it, because Windows lets no other process read a locked file.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tc_api::ids::WallMs;

use crate::log::warn;

const LOCK: &str = "app.lock";
const RECORD: &str = "app-lock.json";

/// How often a waiting process asks again.
const POLL: Duration = Duration::from_millis(100);

/// The lock, held until the process ends. The operating system releases it then.
pub struct AppLock {
    _file: File,
}

/// Who holds the lock, as far as the holder said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holder {
    /// `None` when the record is missing or unreadable, or the holder has not written it yet.
    pub pid: Option<u32>,
}

impl std::fmt::Display for Holder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.pid {
            Some(pid) => write!(f, "process {pid}"),
            None => f.write_str("another process"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppLockError {
    /// Still held when the wait ended.
    #[error("{0} still holds the app-data lock")]
    Held(Holder),
    #[error("cannot use {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// `app-lock.json`: who took the lock, and when.
#[derive(Serialize, Deserialize)]
struct Record {
    pid: u32,
    at: WallMs,
}

impl AppLock {
    /// Takes `<app_data>/app.lock`, waiting up to `wait` for an old `tc4` that is still
    /// closing, then records this process's pid in `app-lock.json`. Blocking: call it before
    /// the runtime starts. A waiting process prints nothing and binds nothing.
    pub fn take(app_data: &Path, wait: Duration) -> Result<AppLock, AppLockError> {
        let path = app_data.join(LOCK);
        let io_error = |source| AppLockError::Io {
            path: path.clone(),
            source,
        };
        // `truncate(false)` says what is meant: the file's content is never used.
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(io_error)?;
        let began = Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) => {
                    let left = wait.saturating_sub(began.elapsed());
                    if left.is_zero() {
                        return Err(AppLockError::Held(read_holder(app_data)));
                    }
                    std::thread::sleep(POLL.min(left));
                }
                // Not a held lock: a file system that cannot lock, say. Waiting would not help.
                Err(TryLockError::Error(source)) => return Err(io_error(source)),
            }
        }
        record_holder(app_data);
        Ok(AppLock { _file: file })
    }
}

/// The pid in `app-lock.json`, if it is there and readable.
fn read_holder(app_data: &Path) -> Holder {
    let pid = std::fs::read(app_data.join(RECORD))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Record>(&bytes).ok())
        .map(|record| record.pid);
    Holder { pid }
}

/// Writes this process's record. The old record goes first: it names the process that held
/// the lock before, which is gone, and a record that names a process that no longer holds the
/// lock could lead a person to end an unrelated program. In the instant between the two a
/// reader finds no record and says "another process", which is true. A failure to write is
/// logged, never fatal: the lock is what matters.
fn record_holder(app_data: &Path) {
    let record = app_data.join(RECORD);
    if let Err(error) = std::fs::remove_file(&record)
        && error.kind() != io::ErrorKind::NotFound
    {
        warn(format_args!("could not remove the old {RECORD}: {error}"));
    }
    if let Err(error) = write_record(app_data, &record) {
        warn(format_args!("could not write {RECORD}: {error}"));
        // Do not leave half of one behind.
        let _ = std::fs::remove_file(&record);
    }
}

/// Temporary file, `sync_all`, rename, then the folder, as every file of app data is written.
fn write_record(app_data: &Path, record: &Path) -> io::Result<()> {
    let pid = std::process::id();
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        });
    let text = serde_json::to_vec(&Record {
        pid,
        at: WallMs(at),
    })
    .map_err(io::Error::other)?;
    let temporary = app_data.join(format!("{RECORD}.{pid}.tmp"));
    let written = (|| {
        let mut file = File::create(&temporary)?;
        file.write_all(&text)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, record)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written?;
    #[cfg(unix)]
    File::open(app_data)?.sync_all()?;
    Ok(())
}
