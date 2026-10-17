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
use tc_journal::{ChangeId, ProjectId};
use tc_store::fault::FaultFs;
use tc_store::{ManualClock, OpenOptions, Store};

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

fn store_on(fs: &FaultFs, clock: &Arc<ManualClock>) -> Store {
    let (mut config, _) = config(fs, APP, "host-1");
    config.clock = clock.clone();
    let store = Store::start(config).unwrap();
    store.wait_cleanup();
    store
}

fn project_of(fs: &FaultFs, folder: &Path) -> ProjectId {
    let bytes = fs
        .peek(&folder.join("ingredients/tc/project.json"))
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    ProjectId::parse(json["projectId"].as_str().unwrap()).unwrap()
}

/// Every change a verified line of a pack or pending file in the folder holds.
fn ids_on_disk(fs: &FaultFs, folder: &Path) -> BTreeSet<ChangeId> {
    let project = project_of(fs, folder);
    let mut out = BTreeSet::new();
    for (rel, bytes) in files(fs, folder) {
        if rel.contains("/.~") {
            continue;
        }
        if rel.starts_with("ingredients/tc/changes/") {
            let read = tc_journal::pack::read_pack(&rel, &bytes, project);
            out.extend(read.entries.iter().map(|e| e.id()));
        } else if rel.starts_with(".tc/pending/") {
            let read = tc_journal::pending::read_pending(&bytes, project);
            out.extend(read.entries.iter().map(|e| e.id()));
        }
    }
    out
}

#[derive(Debug, Clone)]
enum Step {
    Save(u16),
    /// A checkpoint, killed before its k-th write point when given, then a crash with or
    /// without a power cut.
    Checkpoint(Option<(u8, bool)>),
    /// The clock moves to the next month: the last month's packs are consolidated at the
    /// next seal.
    NextMonth,
    /// A byte of a pack flipped outside the app.
    Damage(u8, u16),
    /// A pack deleted outside the app.
    Delete(u8),
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        4 => (1u16..=15).prop_map(Step::Save),
        4 => proptest::option::of((0u8..40, any::<bool>())).prop_map(Step::Checkpoint),
        2 => Just(Step::NextMonth),
        1 => (any::<u8>(), any::<u16>()).prop_map(|(p, at)| Step::Damage(p, at)),
        1 => any::<u8>().prop_map(Step::Delete),
    ]
}

fn run(steps: &[Step]) {
    let clock = Arc::new(ManualClock::at(1_788_000_000_000));
    let fs = FaultFs::new();
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
                let store = store_on(&fs, &clock);
                let handle = store.open(&folder, &OpenOptions::default()).unwrap();
                handle.wait_idle();
                if let Some((k, _)) = kill {
                    fs.kill_before(u64::from(*k), false);
                }
                let _ = handle
                    .checkpoint(tc_store::CheckpointReason::Periodic)
                    .wait();
                let power_cut = kill.is_some_and(|(_, p)| p);
                let next = fs.crash(power_cut);
                drop(handle);
                drop(store);
                fs = next;
                let after = ids_on_disk(&fs, &folder);
                let lost: Vec<&ChangeId> = before.difference(&after).collect();
                assert!(lost.is_empty(), "a checkpoint lost {lost:?}");
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
        // Every change the folder holds is in the fold of the next open.
        let on_disk = ids_on_disk(&fs, &folder);
        let store = store_on(&fs, &clock);
        let handle = store.open(&folder, &OpenOptions::default()).unwrap();
        let fold = handle.snapshot();
        let missing: Vec<&ChangeId> = on_disk
            .iter()
            .filter(|id| fold.fold().change(id).is_none())
            .collect();
        assert!(missing.is_empty(), "the open lost {missing:?}");
        handle.wait_idle();
        fs = fs.crash(false);
        drop(handle);
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, max_shrink_iters: 50, .. ProptestConfig::default() })]

    #[test]
    fn consolidation_loses_nothing(steps in proptest::collection::vec(step(), 1..14)) {
        run(&steps);
    }
}

#[test]
fn a_month_of_packs_is_consolidated_and_loses_nothing() {
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
    run(&steps);
}
