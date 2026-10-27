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
    use std::path::Path;
    use std::sync::{Arc, Condvar, Mutex};

    /// The holds of one store.
    #[derive(Debug, Default)]
    pub struct Hooks {
        held: Mutex<BTreeMap<Point, (bool, u64)>>,
        failing: Mutex<BTreeMap<Point, Failure>>,
        /// The loose objects and bytes past which a repack is due; `None`: §9.8's.
        repack: Mutex<Option<(u64, u64)>>,
        /// The new bytes past which a commit is a chain; `None`: §9.5's.
        chain: Mutex<Option<u64>>,
        /// How a test looks at the folder as a Scripture Burrito, and what it found.
        burrito_probe: Mutex<Option<BurritoProbe>>,
        burrito_problems: Mutex<Vec<String>>,
        /// What a test does between two steps of a seal.
        seal_step: Mutex<Option<SealStepHook>>,
        /// What a test does between two batches of a repack's removals of old objects.
        prune_batch: Mutex<Option<PruneBatchHook>>,
        changed: Condvar,
    }

    /// Looks at the project folder and names what makes it an invalid burrito.
    type Probe = dyn Fn(&Path) -> Vec<String> + Send + Sync;

    /// What a test does between two steps of a seal.
    #[derive(Clone)]
    struct SealStepHook(Arc<dyn Fn() + Send + Sync>);

    impl std::fmt::Debug for SealStepHook {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("SealStepHook")
        }
    }

    /// What a test does between two batches of a repack's removals; it is told how many
    /// objects were removed so far.
    #[derive(Clone)]
    struct PruneBatchHook(Arc<dyn Fn(u64) + Send + Sync>);

    impl std::fmt::Debug for PruneBatchHook {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("PruneBatchHook")
        }
    }

    #[derive(Clone)]
    struct BurritoProbe(Arc<Probe>);

    impl std::fmt::Debug for BurritoProbe {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("BurritoProbe")
        }
    }

    impl Hooks {
        /// From now on the writer calls `probe` with the project folder after every job that
        /// wrote files, outside a write group, and keeps what it names (§16.6, I7).
        pub fn check_burrito(&self, probe: impl Fn(&Path) -> Vec<String> + Send + Sync + 'static) {
            *self.burrito_probe.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(BurritoProbe(Arc::new(probe)));
        }

        /// From now on `step` runs on the actor between two steps of a seal (§5.11), before
        /// the commands that wait are served: a test uses it to make a save wait there.
        pub fn at_seal_step(&self, step: impl Fn() + Send + Sync + 'static) {
            *self.seal_step.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(SealStepHook(Arc::new(step)));
        }

        pub(crate) fn seal_step_reached(&self) {
            let step = self
                .seal_step
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            if let Some(step) = step {
                (step.0)();
            }
        }

        /// From now on `step` runs on the repack's thread before each batch of removals of
        /// unreachable old objects (§9.8), with the number removed so far, outside any lock a
        /// commit waits for: a test begins a commit there.
        pub fn at_prune_batch(&self, step: impl Fn(u64) + Send + Sync + 'static) {
            *self.prune_batch.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(PruneBatchHook(Arc::new(step)));
        }

        pub(crate) fn prune_batch_reached(&self, pruned: u64) {
            let step = self
                .prune_batch
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            if let Some(step) = step {
                (step.0)(pruned);
            }
        }

        /// What the probe named, over every state it looked at.
        pub fn burrito_problems(&self) -> Vec<String> {
            self.burrito_problems
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        }

        pub(crate) fn looks_at_burrito(&self) -> bool {
            self.burrito_probe
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some()
        }

        /// Looks at the folder and keeps what the probe named, unless `still_outside` says a
        /// write group began while it looked: then the look saw a group's middle (I7), and
        /// it answers false so the writer looks again later.
        pub(crate) fn probe_burrito(
            &self,
            folder: &Path,
            still_outside: impl Fn() -> bool,
        ) -> bool {
            let probe = self
                .burrito_probe
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            let Some(probe) = probe else {
                return true;
            };
            let found = (probe.0)(folder);
            if !still_outside() {
                return false;
            }
            self.burrito_problems
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .extend(found);
            true
        }

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

        /// A commit whose new blobs pass `bytes` is a chain of commits, and a push sends one
        /// link per `bytes` (§9.5 says 16 MiB).
        pub fn chain_after(&self, bytes: u64) {
            *self.chain.lock().unwrap_or_else(|e| e.into_inner()) = Some(bytes);
        }

        pub(crate) fn chain_threshold(&self) -> u64 {
            self.chain
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .unwrap_or(crate::checkpoint::CHAIN_BYTES)
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
        pub(crate) fn seal_step_reached(&self) {}

        pub(crate) fn prune_batch_reached(&self, _pruned: u64) {}

        pub(crate) fn looks_at_burrito(&self) -> bool {
            false
        }

        pub(crate) fn probe_burrito(
            &self,
            _folder: &std::path::Path,
            _still_outside: impl Fn() -> bool,
        ) -> bool {
            true
        }

        pub(crate) fn repack_threshold(&self) -> (u64, u64) {
            (crate::repack::REPACK_OBJECTS, crate::repack::REPACK_BYTES)
        }

        pub(crate) fn chain_threshold(&self) -> u64 {
            crate::checkpoint::CHAIN_BYTES
        }

        pub(crate) fn wait(&self, _point: Point) {}

        pub(crate) fn check(&self, _point: Point) -> Result<(), Failure> {
            Ok(())
        }
    }
}

pub use imp::Hooks;
