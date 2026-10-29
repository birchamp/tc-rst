//! `consolidation_loses_nothing` (`docs/STORAGE.md` §16.1): seals and consolidations
//! interleaved with crashes at any write point, power cuts, and packs damaged or deleted in
//! the working tree between steps. No seal or consolidation, finished or stopped, ever
//! shrinks the set of changes the folder's files hold, and every change the folder holds
//! is in the fold of the next open.

use crate::common::*;
use proptest::prelude::*;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tc_journal::ChangeId;
use tc_store::fault::{FaultFs, OpKind, PowerLoss};
use tc_store::{Clock, ManualClock, OpenOptions, Store};

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

fn store_on(fs: &FaultFs, clock: &Arc<ManualClock>) -> Store {
    let (mut config, _) = config(fs, APP, "host-1");
    config.clock = clock.clone();
    let store = Store::start(config).unwrap();
    store.wait_cleanup();
    watch_burrito(&store, fs);
    store
}

/// Where a checkpoint's kill point lies: anywhere in its write points, or inside a window
/// the census must see, found by running the same checkpoint on a copy first.
#[derive(Debug, Clone, Copy)]
enum Window {
    Anywhere,
    /// After a seal's pack is renamed into place (K2), before its pending files go (K4).
    Seal,
    /// After a consolidation's pack is renamed into place (Q2), before the packs it replaces
    /// go (Q4).
    Consolidation,
}

#[derive(Debug, Clone)]
enum Step {
    Save(u16),
    /// A checkpoint, killed before a write point inside the window when given (the `u16`
    /// picks which), then a crash with or without a power cut.
    Checkpoint(Option<(Window, u16, bool)>),
    /// The clock moves to the next month: the last month's packs are consolidated at the
    /// next seal.
    NextMonth,
    /// A byte of a pack flipped outside the app.
    Damage(u8, u16),
    /// A pack deleted outside the app.
    Delete(u8),
}

fn window() -> impl Strategy<Value = Window> {
    prop_oneof![
        1 => Just(Window::Anywhere),
        2 => Just(Window::Seal),
        1 => Just(Window::Consolidation)
    ]
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        4 => (1u16..=15).prop_map(Step::Save),
        5 => proptest::option::weighted(0.6, (window(), any::<u16>(), any::<bool>()))
            .prop_map(Step::Checkpoint),
        3 => Just(Step::NextMonth),
        1 => (any::<u8>(), any::<u16>()).prop_map(|(p, at)| Step::Damage(p, at)),
        1 => any::<u8>().prop_map(Step::Delete),
    ]
}

/// The states the property asserts on, counted over a run (§16.1: a counter at zero fails
/// the run).
#[derive(Debug, Default)]
struct Census {
    /// A checkpoint ended with a new pack that replaces others and none of them left.
    consolidations: AtomicU64,
    /// A kill left a valid pack that replaces others beside a pack it replaces (Q2 to Q4).
    killed_in_consolidation: AtomicU64,
    /// A kill left a pending file all of whose changes a valid pack holds (K2 to K4).
    killed_in_seal: AtomicU64,
}

impl Census {
    fn check(&self) {
        let counts = [
            ("consolidations", &self.consolidations),
            ("kills between Q2 and Q4", &self.killed_in_consolidation),
            ("kills between K2 and K4", &self.killed_in_seal),
        ];
        for (name, count) in counts {
            assert!(
                count.load(Ordering::Relaxed) > 0,
                "the generator never reached: {name}"
            );
        }
    }
}

/// The valid packs on disk: path, ids, and the names their header replaces.
fn valid_packs(fs: &FaultFs, folder: &Path) -> Vec<(String, BTreeSet<ChangeId>, Vec<String>)> {
    let project = project_of(fs, folder);
    files(fs, folder)
        .into_iter()
        .filter(|(rel, _)| rel.starts_with("ingredients/tc/changes/") && !rel.contains("/.~"))
        .filter_map(|(rel, bytes)| {
            let read = tc_journal::pack::read_pack(&rel, &bytes, project);
            read.is_valid().then(|| {
                let replaces = read
                    .header
                    .as_ref()
                    .map(|h| h.replaces.iter().map(|n| n.to_string()).collect())
                    .unwrap_or_default();
                (rel, read.entries.iter().map(|e| e.id()).collect(), replaces)
            })
        })
        .collect()
}

/// The file name of a pack path without its extension: the name `replaces` lists.
fn pack_name(rel: &str) -> &str {
    let file = rel.rsplit('/').next().unwrap_or(rel);
    file.strip_suffix(".ndjson").unwrap_or(file)
}

/// Counts what a checkpoint left on disk.
fn count(census: &Census, fs: &FaultFs, folder: &Path, before: &BTreeSet<String>, killed: bool) {
    let packs = valid_packs(fs, folder);
    let names: BTreeSet<&str> = packs.iter().map(|(rel, _, _)| pack_name(rel)).collect();
    for (rel, _, replaces) in &packs {
        if replaces.is_empty() {
            continue;
        }
        let left = replaces.iter().any(|r| names.contains(r.as_str()));
        if left && killed {
            census
                .killed_in_consolidation
                .fetch_add(1, Ordering::Relaxed);
        }
        if !left && !before.contains(rel) {
            census.consolidations.fetch_add(1, Ordering::Relaxed);
        }
    }
    if killed {
        let held: BTreeSet<ChangeId> = packs.iter().flat_map(|(_, ids, _)| ids.clone()).collect();
        let project = project_of(fs, folder);
        let sealed_pending = files(fs, folder).into_iter().any(|(rel, bytes)| {
            rel.starts_with(".tc/pending/") && !rel.contains("/.~") && {
                let read = tc_journal::pending::read_pending(&bytes, project);
                !read.entries.is_empty() && read.entries.iter().all(|e| held.contains(&e.id()))
            }
        });
        if sealed_pending {
            census.killed_in_seal.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// The write point to stop a checkpoint before, from a run of the same checkpoint on a copy:
/// inside `window` when the checkpoint has one, anywhere otherwise.
fn kill_point(
    fs: &FaultFs,
    folder: &Path,
    clock: &Arc<ManualClock>,
    window: Window,
    pick: u16,
) -> u64 {
    let probe = fs.copy();
    let at = Arc::new(ManualClock::at(clock.now().0));
    let store = store_on(&probe, &at);
    // From before the open: it seals what the last session left (§6.1 O15), consolidation
    // included, so the windows below lie in the open as often as in the checkpoint.
    let start = probe.ops();
    probe.clear_log();
    let handle = store.open(folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let _ = handle
        .checkpoint(tc_store::CheckpointReason::Periodic)
        .wait();
    let log = probe.log();
    let total = probe.ops() - start;
    drop(handle);
    let changes = folder.join("ingredients/tc/changes");
    let pending = folder.join(".tc/pending");
    let rel = |op: &tc_store::fault::Op| op.index - start;
    let pack_renames: Vec<u64> = log
        .iter()
        .filter(|op| op.kind == OpKind::Rename && op.path.starts_with(&changes))
        .map(rel)
        .collect();
    let first_after = |from: u64, under: &Path| {
        log.iter()
            .filter(|op| op.kind == OpKind::RemoveFile && op.path.starts_with(under))
            .map(rel)
            .find(|i| *i > from)
    };
    let span = match window {
        Window::Anywhere => None,
        Window::Seal => pack_renames
            .first()
            .and_then(|r| Some((*r + 1, first_after(*r, &pending)?))),
        Window::Consolidation => pack_renames
            .iter()
            .find_map(|r| Some((*r + 1, first_after(*r, &changes)?))),
    };
    let (lo, hi) = span.unwrap_or((0, total.max(1)));
    lo + u64::from(pick) % (hi - lo + 1)
}

fn run(steps: &[Step], model: PowerLoss, census: &Census) {
    let clock = Arc::new(ManualClock::at(1_788_000_000_000));
    let fs = FaultFs::new();
    fs.set_power_loss(model);
    let store = store_on(&fs, &clock);
    let folder = create_titus(
        &Rig {
            fs: fs.clone(),
            store,
        },
        "Titus",
    );
    let mut fs = fs.crash(false);
    let mut text = 0u32;
    for step in steps {
        match step {
            Step::Save(v) => {
                let store = store_on(&fs, &clock);
                let handle = store.open(&folder, &OpenOptions::default()).unwrap();
                text += 1;
                let _ = save_verse(&handle, 1, *v, &format!("text {text}"));
                handle.wait_idle();
                // Killed after the save: what is due is not sealed by a close.
                fs = fs.crash(false);
                drop(handle);
            }
            Step::Checkpoint(kill) => {
                let before = ids_on_disk(&fs, &folder);
                let packs_before: BTreeSet<String> = packs(&fs, &folder).into_iter().collect();
                let point =
                    kill.map(|(window, pick, _)| kill_point(&fs, &folder, &clock, window, pick));
                let store = store_on(&fs, &clock);
                if let Some(k) = point {
                    fs.kill_before(k, false);
                }
                let handle = store.open(&folder, &OpenOptions::default());
                assert!(
                    handle.is_ok() || fs.is_dead(),
                    "{:?}",
                    handle.as_ref().err()
                );
                if let Ok(handle) = &handle {
                    handle.wait_idle();
                    let _ = handle
                        .checkpoint(tc_store::CheckpointReason::Periodic)
                        .wait();
                }
                let killed = fs.is_dead();
                let power_cut = kill.is_some_and(|(_, _, p)| p);
                let next = fs.crash(power_cut);
                drop(handle);
                drop(store);
                fs = next;
                let after = ids_on_disk(&fs, &folder);
                let lost: Vec<&ChangeId> = before.difference(&after).collect();
                assert!(lost.is_empty(), "a checkpoint lost {lost:?}");
                count(census, &fs, &folder, &packs_before, killed);
            }
            Step::NextMonth => clock.advance(31 * DAY_MS),
            Step::Damage(p, at) => {
                let packs = packs(&fs, &folder);
                if packs.is_empty() {
                    continue;
                }
                let path = folder.join(&packs[*p as usize % packs.len()]);
                let mut bytes = fs.peek(&path).unwrap();
                let at = *at as usize % bytes.len();
                bytes[at] ^= 0x01;
                fs.put(&path, &bytes, None);
            }
            Step::Delete(p) => {
                let packs = packs(&fs, &folder);
                if packs.is_empty() {
                    continue;
                }
                fs.delete(&folder.join(&packs[*p as usize % packs.len()]))
                    .unwrap();
            }
        }
        // Every change the folder holds is in the fold of the next open. The open looks at a
        // copy: it seals what a step left (§6.1 O15), which is the next step's to do.
        let on_disk = ids_on_disk(&fs, &folder);
        let looked = fs.copy();
        let store = store_on(&looked, &clock);
        let handle = store.open(&folder, &OpenOptions::default()).unwrap();
        let fold = handle.snapshot();
        let missing: Vec<&ChangeId> = on_disk
            .iter()
            .filter(|id| fold.fold().change(id).is_none())
            .collect();
        assert!(missing.is_empty(), "the open lost {missing:?}");
        handle.wait_idle();
        drop(handle);
    }
}

#[test]
fn consolidation_loses_nothing() {
    histories(8);
}

#[test]
#[ignore = "nightly: more histories; consolidation_loses_nothing runs the same property on 8 in the gate"]
fn consolidation_loses_nothing_nightly() {
    histories(32);
}

fn histories(cases: u32) {
    let census = Census::default();
    let config = ProptestConfig {
        cases,
        max_shrink_iters: 200,
        failure_persistence: None,
        ..ProptestConfig::default()
    };
    let mut runner = proptest::test_runner::TestRunner::new(config);
    // Each history ends with two seals (the second killed between its pack and the removal
    // of its pending files in half the cases) and a new month, then a checkpoint that
    // consolidates the month's packs, killed inside the consolidation in most cases, then
    // one that finishes: so every run reaches each state the census counts, not only when
    // the steps happen to line up.
    let kill = proptest::option::weighted(
        0.7,
        (Just(Window::Consolidation), any::<u16>(), any::<bool>()),
    );
    let seal_kill =
        proptest::option::weighted(0.5, (Just(Window::Seal), any::<u16>(), any::<bool>()));
    let model = prop_oneof![Just(PowerLoss::KeepSyncedNames), Just(PowerLoss::Strict)];
    let steps = (
        proptest::collection::vec(step(), 4..16),
        1u16..=15,
        seal_kill,
        kill,
    )
        .prop_map(|(mut steps, v, seal_kill, kill)| {
            steps.extend([
                Step::Save(v),
                Step::Checkpoint(None),
                Step::Save(v % 15 + 1),
                Step::Checkpoint(seal_kill),
                Step::NextMonth,
                Step::Checkpoint(kill),
                Step::Checkpoint(None),
            ]);
            steps
        });
    if let Err(error) = runner.run(&(steps, model), |(steps, model)| {
        run(&steps, model, &census);
        Ok(())
    }) {
        panic!("{error}");
    }
    eprintln!("census {census:?}");
    census.check();
}

#[test]
fn a_month_of_packs_is_consolidated_and_loses_nothing() {
    let census = Census::default();
    let steps = [
        Step::Save(1),
        Step::Checkpoint(None),
        Step::Save(2),
        Step::Checkpoint(None),
        Step::Save(3),
        Step::Checkpoint(None),
        Step::NextMonth,
        Step::Save(4),
        Step::Checkpoint(None),
    ];
    run(&steps, PowerLoss::Strict, &census);
    assert!(census.consolidations.load(Ordering::Relaxed) > 0);
}
