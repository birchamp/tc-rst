//! Holds a test places at named points (APP-DESIGN R8): before a book read, a draft write,
//! the durability point of a save, a checkpoint, a materialization, a staging folder's
//! removal and its rename, and a copy between its pending files and its packs. Only with the
//! `test-hooks` feature; without it every point passes at once and the type holds nothing.

/// A point a test can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Point {
    BeforeBookRead,
    BeforeDraftWrite,
    BeforeDurablePending,
    /// The writer has been handed a checkpoint's work, after the seal (§5.13, R21).
    BeforeCheckpoint,
    /// The writer is about to write derived files (§5.10).
    BeforeMaterialize,
    /// The writer has a complete temp file of a derived file and is about to look at the
    /// target one last time and rename (§5.10 M4, I8).
    BeforeRename,
    /// The start cleanup is about to remove a staging folder (N1).
    BeforeStagingRemoval,
    /// A create or copy is about to rename its staging folder into place (N2).
    BeforeStagingRename,
    /// A copy has copied one journal folder (the pending files) and is about to list the
    /// other (the packs) (§5.0).
    CopyBetweenJournalFolders,
    /// The open is about to open the project's git repository (§6.1 O3); a test fails it
    /// with [`Failure::Owner`].
    GitOpen,
    /// A checkpoint's commit object is written and the branch is about to move (§5.13 C5);
    /// a test stops it with [`Failure::Stop`].
    BeforeHeadUpdate,
    /// The writer has planned an outside edit and is about to send it to the actor (§11.1).
    BeforeRecordOutside,
    /// A repack has written its pack and index and has not removed a loose object (§9.8,
    /// §6.4 R1); a test stops it with [`Failure::Stop`].
    AfterPackWritten,
}

/// What a test makes a [`Point`] do instead of passing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// libgit2 refuses the folder because another account owns it.
    Owner,
    /// The step stops here, as if the process had died: nothing after it happens.
    Stop,
}

#[cfg(feature = "test-hooks")]
mod imp {
    use super::{Failure, Point};
    use std::collections::BTreeMap;
    use std::sync::{Condvar, Mutex};

    /// The holds of one store.
    #[derive(Debug, Default)]
    pub struct Hooks {
        held: Mutex<BTreeMap<Point, (bool, u64)>>,
        failing: Mutex<BTreeMap<Point, Failure>>,
        /// The loose objects and bytes past which a repack is due; `None`: §9.8's.
        repack: Mutex<Option<(u64, u64)>>,
        changed: Condvar,
    }

    impl Hooks {
        /// A repack is due when the repository holds more than `objects` loose objects or
        /// `bytes` bytes of them (§9.8 says 2,000 and 64 MiB; `repack_after(0, 0)` makes
        /// every checkpoint repack).
        pub fn repack_after(&self, objects: u64, bytes: u64) {
            *self.repack.lock().unwrap_or_else(|e| e.into_inner()) = Some((objects, bytes));
        }

        pub(crate) fn repack_threshold(&self) -> (u64, u64) {
            self.repack
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .unwrap_or((crate::repack::REPACK_OBJECTS, crate::repack::REPACK_BYTES))
        }

        /// From now on, `point` fails with `failure` until [`Hooks::heal`].
        pub fn fail(&self, point: Point, failure: Failure) {
            self.failing
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(point, failure);
        }

        /// Ends the failure placed at `point`.
        pub fn heal(&self, point: Point) {
            self.failing
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&point);
        }

        pub(crate) fn check(&self, point: Point) -> Result<(), Failure> {
            match self
                .failing
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&point)
            {
                Some(failure) => Err(*failure),
                None => Ok(()),
            }
        }

        /// From now on, the store's threads wait at `point` until it is released.
        pub fn hold(&self, point: Point) {
            let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
            held.entry(point).or_insert((false, 0)).0 = true;
        }

        pub fn release(&self, point: Point) {
            let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(entry) = held.get_mut(&point) {
                entry.0 = false;
            }
            self.changed.notify_all();
        }

        /// How many times a thread reached `point` while it was held.
        pub fn reached(&self, point: Point) -> u64 {
            self.held
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&point)
                .map_or(0, |e| e.1)
        }

        /// Waits until a thread has reached `point` at least `n` times.
        pub fn wait_reached(&self, point: Point, n: u64) {
            let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
            while held.get(&point).map_or(0, |e| e.1) < n {
                held = self.changed.wait(held).unwrap_or_else(|e| e.into_inner());
            }
        }

        pub(crate) fn wait(&self, point: Point) {
            let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
            let Some(entry) = held.get_mut(&point) else {
                return;
            };
            if !entry.0 {
                return;
            }
            entry.1 += 1;
            self.changed.notify_all();
            while held.get(&point).is_some_and(|e| e.0) {
                held = self.changed.wait(held).unwrap_or_else(|e| e.into_inner());
            }
        }
    }
}

#[cfg(not(feature = "test-hooks"))]
mod imp {
    use super::{Failure, Point};

    /// Without the `test-hooks` feature: no hold.
    #[derive(Debug, Default)]
    pub struct Hooks;

    impl Hooks {
        pub(crate) fn repack_threshold(&self) -> (u64, u64) {
            (crate::repack::REPACK_OBJECTS, crate::repack::REPACK_BYTES)
        }

        pub(crate) fn wait(&self, _point: Point) {}

        pub(crate) fn check(&self, _point: Point) -> Result<(), Failure> {
            Ok(())
        }
    }
}

pub use imp::Hooks;
