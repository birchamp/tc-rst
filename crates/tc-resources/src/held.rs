//! Files that another program holds open.
//!
//! On Windows a scanner (Defender, the Search indexer, OneDrive) can hold a file that was
//! just written open without `FILE_SHARE_DELETE`. Renaming or removing that file, or the
//! folder around it, fails with error 5, 32 or 33 for a moment. [`rename`] and
//! [`remove_dir_all`] run again while the error says a file is held, with a bounded
//! back-off, and return the last error when the waits are used up.
//!
//! `tc-store` keeps the same policy for project files (`HeldRetry::SAVE` and
//! `is_held_by_another_program`). The two use one schedule and one marker error so they
//! behave alike; they become one helper when the storage branch lands.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The milliseconds to wait before each further try: ten more tries, 2,004 ms in all. A
/// scanner almost always lets go within that, and an install that cannot be completed
/// still fails within about two seconds.
const WAITS_MS: [u64; 10] = [2, 4, 8, 16, 32, 64, 128, 250, 500, 1000];

/// The error of a rename or removal that another program's open handle refused. A machine
/// that is not Windows raises it to behave like one, which is how the tests simulate a
/// scanner.
#[derive(Debug, thiserror::Error)]
#[error("{} is held by another program", path.display())]
pub(crate) struct HeldByAnotherProgram {
    path: PathBuf,
}

impl HeldByAnotherProgram {
    /// Whether `error` carries this marker, which only a simulated scanner raises.
    fn is_in(error: &io::Error) -> bool {
        error
            .get_ref()
            .is_some_and(|inner| inner.is::<HeldByAnotherProgram>())
    }

    #[cfg(test)]
    fn error(path: &Path) -> io::Error {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            Self {
                path: path.to_owned(),
            },
        )
    }
}

/// Windows: the operating system's error 5 (access denied), 32 (sharing violation) or 33
/// (lock violation). Elsewhere those numbers mean other things, so only an error whose
/// payload is [`HeldByAnotherProgram`] counts.
pub(crate) fn is_held(error: &io::Error) -> bool {
    const ACCESS_DENIED: i32 = 5;
    const SHARING_VIOLATION: i32 = 32;
    const LOCK_VIOLATION: i32 = 33;
    HeldByAnotherProgram::is_in(error)
        || (cfg!(windows)
            && matches!(
                error.raw_os_error(),
                Some(ACCESS_DENIED | SHARING_VIOLATION | LOCK_VIOLATION)
            ))
}

/// Runs `op` again, after the next wait, while it fails because a file is held
/// ([`is_held`]). Any other result is returned after one call.
pub(crate) fn retry_while_held<T>(mut op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let mut waits = WAITS_MS.iter();
    loop {
        match op() {
            Err(e) if is_held(&e) => match waits.next() {
                Some(ms) => pause(Duration::from_millis(*ms), &e),
                None => return Err(e),
            },
            result => return result,
        }
    }
}

/// `fs::rename`, run again while another program holds either file.
pub(crate) fn rename(from: &Path, to: &Path) -> io::Result<()> {
    retry_while_held(|| try_rename(from, to))
}

/// `fs::remove_dir_all`, run again while another program holds a file in the folder.
pub(crate) fn remove_dir_all(path: &Path) -> io::Result<()> {
    retry_while_held(|| try_remove_dir_all(path))
}

/// One `fs::rename`, for a caller that decides for itself what to retry.
pub(crate) fn try_rename(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(test)]
    scanner::refuse(&[from, to])?;
    fs::rename(from, to)
}

/// One `fs::remove_dir_all`.
pub(crate) fn try_remove_dir_all(path: &Path) -> io::Result<()> {
    #[cfg(test)]
    scanner::refuse(&[path])?;
    fs::remove_dir_all(path)
}

/// Waits before the next try, which follows the failure `after`.
fn pause(wait: Duration, after: &io::Error) {
    #[cfg(test)]
    if scanner::note_wait(wait, after) {
        return;
    }
    #[cfg(not(test))]
    let _ = after;
    std::thread::sleep(wait);
}

/// A simulated scanner, for any platform: files it holds refuse a rename or removal of
/// themselves or of a folder around them, as Windows does, and the waits it causes are
/// recorded and not slept. A hold the operating system itself reports (Windows, with a
/// real scanner at work during a test) is waited out and not recorded, as in the app.
#[cfg(test)]
pub(crate) mod scanner {
    use super::HeldByAnotherProgram;
    use std::cell::RefCell;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    struct Hold {
        path: PathBuf,
        /// How many more operations it refuses; `None` is for ever.
        refusals: Option<usize>,
    }

    #[derive(Default)]
    struct Simulation {
        active: bool,
        holds: Vec<Hold>,
        waits: Vec<Duration>,
    }

    thread_local! {
        static SIMULATION: RefCell<Simulation> = RefCell::new(Simulation::default());
    }

    /// A scanner on the current thread, until it is dropped.
    pub(crate) struct Scanner;

    impl Scanner {
        pub(crate) fn new() -> Self {
            SIMULATION.with_borrow_mut(|s| {
                *s = Simulation {
                    active: true,
                    ..Simulation::default()
                }
            });
            Self
        }

        /// Holds `path` for the next `times` operations that touch it, then lets go.
        pub(crate) fn holds_for(&self, path: &Path, times: usize) {
            self.add(path, Some(times));
        }

        /// Holds `path` until [`Scanner::lets_go`].
        pub(crate) fn holds(&self, path: &Path) {
            self.add(path, None);
        }

        pub(crate) fn lets_go(&self, path: &Path) {
            SIMULATION.with_borrow_mut(|s| s.holds.retain(|h| h.path != path));
        }

        /// The waits so far, in order.
        pub(crate) fn waited(&self) -> Vec<Duration> {
            SIMULATION.with_borrow(|s| s.waits.clone())
        }

        fn add(&self, path: &Path, refusals: Option<usize>) {
            SIMULATION.with_borrow_mut(|s| {
                s.holds.push(Hold {
                    path: path.to_owned(),
                    refusals,
                })
            });
        }
    }

    impl Drop for Scanner {
        fn drop(&mut self) {
            SIMULATION.with_borrow_mut(|s| *s = Simulation::default());
        }
    }

    /// Refuses an operation on `paths` when a held file is one of them or lies inside one.
    pub(super) fn refuse(paths: &[&Path]) -> io::Result<()> {
        SIMULATION.with_borrow_mut(|s| {
            if !s.active {
                return Ok(());
            }
            for hold in &mut s.holds {
                if hold.refusals == Some(0) || !paths.iter().any(|p| hold.path.starts_with(p)) {
                    continue;
                }
                if let Some(left) = &mut hold.refusals {
                    *left -= 1;
                }
                return Err(HeldByAnotherProgram::error(&hold.path));
            }
            Ok(())
        })
    }

    /// Records the wait that follows `after` and answers `true`, so that it is not slept,
    /// when a scanner is simulated and `after` is its refusal. Any other error is a real
    /// one, and its wait has to pass for the real scanner to let go.
    pub(super) fn note_wait(wait: Duration, after: &io::Error) -> bool {
        SIMULATION.with_borrow_mut(|s| {
            let simulated = s.active && HeldByAnotherProgram::is_in(after);
            if simulated {
                s.waits.push(wait);
            }
            simulated
        })
    }
}

#[cfg(test)]
mod tests {
    use super::scanner::Scanner;
    use super::*;
    use std::cell::Cell;
    use std::time::Instant;

    fn held() -> io::Error {
        HeldByAnotherProgram::error(Path::new("held"))
    }

    fn ms(waits: &[u64]) -> Vec<Duration> {
        waits.iter().map(|w| Duration::from_millis(*w)).collect()
    }

    #[test]
    fn the_waits_add_up_to_about_two_seconds() {
        let total: u64 = WAITS_MS.iter().sum();
        assert_eq!(total, 2_004, "tc-store waits the same in all");
    }

    #[test]
    fn a_held_file_is_tried_again_after_each_wait_until_it_is_free() {
        let scanner = Scanner::new();
        let calls = Cell::new(0);
        let result = retry_while_held(|| {
            calls.set(calls.get() + 1);
            if calls.get() <= 3 { Err(held()) } else { Ok(7) }
        });
        assert_eq!(result.unwrap(), 7);
        assert_eq!(calls.get(), 4);
        assert_eq!(scanner.waited(), ms(&[2, 4, 8]));
    }

    #[test]
    fn a_file_that_stays_held_is_given_up_after_the_last_wait() {
        let scanner = Scanner::new();
        let calls = Cell::new(0);
        let result: io::Result<()> = retry_while_held(|| {
            calls.set(calls.get() + 1);
            Err(held())
        });
        assert!(is_held(&result.unwrap_err()));
        assert_eq!(calls.get(), WAITS_MS.len() + 1);
        assert_eq!(scanner.waited(), ms(&WAITS_MS));
    }

    #[test]
    fn only_a_simulated_refusal_is_not_slept() {
        let scanner = Scanner::new();
        let real = io::Error::from(io::ErrorKind::PermissionDenied);
        let started = Instant::now();
        pause(Duration::from_millis(60), &real);
        assert!(started.elapsed() >= Duration::from_millis(30));
        assert!(scanner.waited().is_empty());
        let started = Instant::now();
        pause(Duration::from_secs(10), &held());
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(scanner.waited(), [Duration::from_secs(10)]);
    }

    /// Opens `path` as a scanner does: others may still read and write it, but it cannot be
    /// renamed or deleted while it is open.
    #[cfg(windows)]
    fn held_like_a_scanner(path: &Path) -> fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 1;
        const FILE_SHARE_WRITE: u32 = 2;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(path)
            .unwrap()
    }

    #[cfg(windows)]
    #[test]
    fn a_real_hold_is_waited_out_while_a_scanner_is_simulated() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        fs::write(&file, b"x").unwrap();
        let handle = held_like_a_scanner(&file);
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            drop(handle);
        });
        // A simulated scanner that holds nothing: the refusals below are Windows's own.
        let scanner = Scanner::new();
        rename(&file, &dir.path().join("moved")).unwrap();
        release.join().unwrap();
        assert!(scanner.waited().is_empty(), "{:?}", scanner.waited());
    }

    #[test]
    fn any_other_error_is_returned_at_once() {
        let scanner = Scanner::new();
        let calls = Cell::new(0);
        for kind in [
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::NotFound,
            io::ErrorKind::AlreadyExists,
        ] {
            let result: io::Result<()> = retry_while_held(|| {
                calls.set(calls.get() + 1);
                Err(io::Error::from(kind))
            });
            assert_eq!(result.unwrap_err().kind(), kind);
        }
        assert_eq!(calls.get(), 3);
        assert!(scanner.waited().is_empty());
    }

    #[test]
    fn a_hold_refuses_the_file_and_the_folders_around_it_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("folder");
        let sibling = dir.path().join("sibling");
        fs::create_dir_all(&folder).unwrap();
        fs::create_dir_all(&sibling).unwrap();
        let file = folder.join("file");
        fs::write(&file, b"x").unwrap();
        let scanner = Scanner::new();
        scanner.holds_for(&file, 1);
        assert!(is_held(&try_remove_dir_all(&folder).unwrap_err()));
        // The hold refused once, and is over.
        try_rename(&sibling, &dir.path().join("moved")).unwrap();
        scanner.holds(&file);
        assert!(is_held(
            &try_rename(&folder, &dir.path().join("elsewhere")).unwrap_err()
        ));
        try_remove_dir_all(&dir.path().join("moved")).unwrap();
        scanner.lets_go(&file);
        remove_dir_all(&folder).unwrap();
    }
}
