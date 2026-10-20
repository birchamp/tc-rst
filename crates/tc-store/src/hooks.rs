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
    /// The command committed and notified the draft writer; its Ack has not been sent.
    BeforeCommandReply,
    /// The actor is about to seal and materialize for a checkpoint (§5.13).
    BeforeCheckpoint,
    /// The actor is about to write derived files (§5.10).
    BeforeMaterialize,
    /// The start cleanup is about to remove a staging folder (N1).
    BeforeStagingRemoval,
    /// A create or copy is about to rename its staging folder into place (N2).
    BeforeStagingRename,
    /// A copy has copied one journal folder (the pending files) and is about to list the
    /// other (the packs) (§5.0).
    CopyBetweenJournalFolders,
}

#[cfg(feature = "test-hooks")]
mod imp {
    use super::Point;
    use std::collections::BTreeMap;
    use std::sync::{Condvar, Mutex};

    /// The holds of one store.
    #[derive(Debug, Default)]
    pub struct Hooks {
        held: Mutex<BTreeMap<Point, (bool, u64)>>,
        changed: Condvar,
    }

    impl Hooks {
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
    use super::Point;

    /// Without the `test-hooks` feature: no hold.
    #[derive(Debug, Default)]
    pub struct Hooks;

    impl Hooks {
        pub(crate) fn wait(&self, _point: Point) {}
    }
}

pub use imp::Hooks;
