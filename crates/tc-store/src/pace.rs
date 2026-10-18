//! Pacing the writer behind saves (PERFORMANCE.md: a save is durable within 25 ms p95 while
//! background work runs, a whole-project materialization included).
//!
//! A save's flush shares the disk with the writer's. The actor raises a [`SaveGate`] while it
//! writes a pending file; [`PacedVfs`] makes the writer wait, for at most [`MAX_YIELD`], before
//! each flush and each rename while a save is in flight. The wait is bounded, so a stream of
//! saves slows the writer and never stops it.

use crate::counters::Counters;
use crate::vfs::{DirEntry, FileMeta, LockHandle, ReadHandle, Vfs, WriteHandle};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// The longest the writer waits for the saves in flight before one flush or rename.
pub(crate) const MAX_YIELD: Duration = Duration::from_millis(50);

/// How many saves are being written now.
#[derive(Debug, Default)]
pub(crate) struct SaveGate {
    in_flight: Mutex<u32>,
    changed: Condvar,
}

/// A save in flight; the gate lowers when it is dropped.
pub(crate) struct InFlight<'a> {
    gate: &'a SaveGate,
}

impl SaveGate {
    /// A save starts writing.
    pub(crate) fn enter(&self) -> InFlight<'_> {
        *self.in_flight.lock().unwrap_or_else(|e| e.into_inner()) += 1;
        InFlight { gate: self }
    }

    /// Waits while a save is in flight, for at most `max`. Returns how long it waited.
    pub(crate) fn yield_to_saves(&self, max: Duration) -> Duration {
        let started = Instant::now();
        let mut in_flight = self.in_flight.lock().unwrap_or_else(|e| e.into_inner());
        while *in_flight > 0 {
            let left = max.saturating_sub(started.elapsed());
            if left.is_zero() {
                break;
            }
            in_flight = self
                .changed
                .wait_timeout(in_flight, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        started.elapsed()
    }
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        let mut in_flight = self
            .gate
            .in_flight
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *in_flight = in_flight.saturating_sub(1);
        drop(in_flight);
        self.gate.changed.notify_all();
    }
}

/// The writer's view of the filesystem: [`Vfs`] with a pause before each flush and rename
/// while a save is in flight.
#[derive(Debug)]
pub(crate) struct PacedVfs {
    inner: Arc<dyn Vfs>,
    gate: Arc<SaveGate>,
    counters: Arc<Counters>,
}

impl PacedVfs {
    pub(crate) fn new(inner: Arc<dyn Vfs>, gate: Arc<SaveGate>, counters: Arc<Counters>) -> Self {
        Self {
            inner,
            gate,
            counters,
        }
    }

    fn pace(&self) {
        pace(&self.gate, &self.counters);
    }
}

fn pace(gate: &SaveGate, counters: &Counters) {
    let waited = gate.yield_to_saves(MAX_YIELD);
    if !waited.is_zero() {
        counters.writer_yield_ns.fetch_add(
            u64::try_from(waited.as_nanos()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }
}

struct PacedWrite {
    inner: Box<dyn WriteHandle>,
    gate: Arc<SaveGate>,
    counters: Arc<Counters>,
}

impl WriteHandle for PacedWrite {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.inner.write_all(bytes)
    }

    fn sync_all(&mut self) -> io::Result<()> {
        pace(&self.gate, &self.counters);
        self.inner.sync_all()
    }
}

impl Vfs for PacedVfs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.inner.read(path)
    }

    fn open(&self, path: &Path) -> io::Result<Box<dyn ReadHandle>> {
        self.inner.open(path)
    }

    fn metadata(&self, path: &Path) -> io::Result<FileMeta> {
        self.inner.metadata(path)
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>> {
        self.inner.read_dir(path)
    }

    fn create_new(&self, path: &Path) -> io::Result<Box<dyn WriteHandle>> {
        Ok(Box::new(PacedWrite {
            inner: self.inner.create_new(path)?,
            gate: self.gate.clone(),
            counters: self.counters.clone(),
        }))
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.pace();
        self.inner.rename(from, to)
    }

    fn rename_dir_noreplace(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.pace();
        self.inner.rename_dir_noreplace(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.inner.remove_file(path)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        self.inner.remove_dir_all(path)
    }

    fn create_dir(&self, path: &Path) -> io::Result<()> {
        self.inner.create_dir(path)
    }

    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        self.pace();
        self.inner.sync_dir(path)
    }

    fn sync_file(&self, path: &Path) -> io::Result<()> {
        self.pace();
        self.inner.sync_file(path)
    }

    fn lock(&self, path: &Path) -> io::Result<Option<Box<dyn LockHandle>>> {
        self.inner.lock(path)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.inner.canonicalize(path)
    }

    fn secure_dir_chain(&self, dir: &Path, made: &[PathBuf]) -> io::Result<()> {
        self.inner.secure_dir_chain(dir, made)
    }

    fn is_held(&self, error: &io::Error) -> bool {
        self.inner.is_held(error)
    }

    fn wait_held(&self, delay: Duration) {
        self.inner.wait_held(delay);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With no save in flight the writer does not wait; with one it waits, and no longer than
    /// the bound however long the save takes.
    #[test]
    fn the_writer_waits_for_a_save_in_flight_but_never_for_long() {
        let gate = SaveGate::default();
        assert!(gate.yield_to_saves(MAX_YIELD) < Duration::from_millis(5));
        let save = gate.enter();
        let waited = gate.yield_to_saves(Duration::from_millis(30));
        assert!(
            waited >= Duration::from_millis(30) && waited < Duration::from_millis(500),
            "{waited:?}"
        );
        drop(save);
        assert!(gate.yield_to_saves(MAX_YIELD) < Duration::from_millis(5));
    }

    /// A save that ends lets the waiting writer go at once.
    #[test]
    fn a_save_that_ends_releases_the_writer() {
        let gate = Arc::new(SaveGate::default());
        let save = gate.enter();
        let waiter = {
            let gate = gate.clone();
            std::thread::spawn(move || gate.yield_to_saves(Duration::from_secs(10)))
        };
        std::thread::sleep(Duration::from_millis(50));
        drop(save);
        let waited = waiter.join().unwrap();
        assert!(waited < Duration::from_secs(5), "{waited:?}");
    }
}
