//! `random_syncs_lose_no_work`: the machine driven against a model of one project and its
//! Door43 branch, with teammates pushing, saves made and the app killed between any two steps.
//! Changes are numbers; a commit holds a set of them. What a kill keeps is what is on disk or
//! on Door43: the project's changes, its last commit and the fetched branch; the machine's
//! state is lost and the next sync starts again.

use proptest::prelude::*;
use std::collections::BTreeSet;
use tc_sync::machine::{Ended, Received, Remote, SyncAction, SyncEvent, SyncState, next};

type Changes = BTreeSet<u32>;

/// What lasts across a kill.
#[derive(Debug, Default)]
struct World {
    /// The project's changes: packs and pending saves.
    local: Changes,
    /// The project's last commit.
    committed: Changes,
    /// `refs/remotes/origin/main`, written by the fetch.
    fetched: Option<Changes>,
    /// Door43's branch.
    remote: Changes,
    next_id: u32,
    /// Every change ever saved on this computer, and every one a teammate pushed.
    saved: Changes,
    pushed_by_teammates: Changes,
}

impl World {
    fn id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }
}

/// What happens between two steps.
#[derive(Debug, Clone, Copy)]
enum Between {
    Nothing,
    Save,
    TeammatePushes,
    Kill,
}

fn between() -> impl Strategy<Value = Between> {
    prop_oneof![
        6 => Just(Between::Nothing),
        2 => Just(Between::Save),
        2 => Just(Between::TeammatePushes),
        1 => Just(Between::Kill),
    ]
}

/// Runs the action, as the store and Door43 would, and gives its answer. Checks the
/// invariants each step must keep.
fn act(world: &mut World, action: &SyncAction) -> Option<SyncEvent> {
    Some(match action {
        SyncAction::Gate => SyncEvent::GatePassed,
        SyncAction::Checkpoint => {
            world.committed = world.local.clone();
            SyncEvent::CheckpointMade
        }
        SyncAction::Fetch => {
            world.fetched = Some(world.remote.clone());
            if world.remote.is_subset(&world.committed) {
                SyncEvent::Fetched(Remote::NothingNew)
            } else {
                SyncEvent::Fetched(Remote::New)
            }
        }
        SyncAction::ReadRemote => SyncEvent::RemoteRead,
        SyncAction::TakeIn => {
            let fetched = world.fetched.clone().expect("a fetch before intake");
            // Whole or absent: one step.
            world.local.extend(fetched);
            SyncEvent::TakenIn
        }
        SyncAction::MergeCommit => {
            let fetched = world.fetched.clone().unwrap_or_default();
            assert!(
                fetched.is_subset(&world.local),
                "INV-RECEIVED-WORK-IS-KEPT-WHOLE-OR-ABSENT: merging without the received work"
            );
            world.committed = world.local.clone();
            SyncEvent::Merged
        }
        SyncAction::Push => {
            let fetched = world.fetched.clone().unwrap_or_default();
            if world.remote != fetched {
                // Door43 refuses a push that does not fast-forward its branch.
                SyncEvent::PushRaced
            } else {
                assert!(
                    world.remote.is_subset(&world.committed),
                    "INV-PUSH-IS-NEVER-FORCED: the push would drop {:?}",
                    world
                        .remote
                        .difference(&world.committed)
                        .collect::<Vec<_>>()
                );
                world.remote = world.committed.clone();
                SyncEvent::Pushed
            }
        }
        SyncAction::Wait { .. } => SyncEvent::WaitOver,
        SyncAction::Confirm => SyncEvent::Confirmed {
            equal: world.remote == world.committed,
        },
        SyncAction::Repack => SyncEvent::Repacked,
        SyncAction::Report(_) => return None,
    })
}

fn run(world: &mut World, script: &mut impl Iterator<Item = Between>) -> Option<Ended> {
    let (mut state, mut actions) = next(&SyncState::Idle, &SyncEvent::Start);
    loop {
        let action = actions.pop()?;
        assert!(actions.is_empty(), "one action at a time");
        match script.next().unwrap_or(Between::Nothing) {
            Between::Nothing => {}
            Between::Save => {
                let id = world.id();
                world.local.insert(id);
                world.saved.insert(id);
            }
            Between::TeammatePushes => {
                let id = world.id();
                world.remote.insert(id);
                world.pushed_by_teammates.insert(id);
            }
            Between::Kill => return None,
        }
        let before_local = world.local.clone();
        let before_remote = world.remote.clone();
        let Some(event) = act(world, &action) else {
            let SyncState::Done(ended) = state else {
                panic!("a report before the end: {state:?}");
            };
            return Some(ended);
        };
        assert!(
            before_local.is_subset(&world.local),
            "INV-OWN-WORK-NEVER-REVERTED: {action:?} dropped local work"
        );
        assert!(
            before_remote.is_subset(&world.remote),
            "INV-PUSH-IS-NEVER-FORCED: {action:?} dropped Door43's work"
        );
        let (after, more) = next(&state, &event);
        if let (SyncState::Done(Ended::Failed(f)), Some(true)) =
            (&after, received(&state).map(|r| r == Received::Integrated))
        {
            assert!(
                matches!(f, tc_api::refusal::SyncFailure::AfterIntake(_)),
                "{f:?} after intake"
            );
        }
        state = after;
        actions = more;
    }
}

fn received(state: &SyncState) -> Option<Received> {
    match state {
        SyncState::Fetch(at)
        | SyncState::Intake(at)
        | SyncState::Integrate(at)
        | SyncState::Merge(at)
        | SyncState::Push(at)
        | SyncState::Backoff(at) => Some(at.received),
        SyncState::Confirm(r) | SyncState::Repack { received: r, .. } => Some(*r),
        SyncState::Idle | SyncState::Gate | SyncState::Checkpoint | SyncState::Done(_) => None,
    }
}

fn lose_no_work(script: Vec<Between>) -> Result<(), TestCaseError> {
    let mut world = World::default();
    let mut script = script.into_iter();
    // Syncs until the script is used up: each one may be killed or race teammates.
    while script.len() > 0 {
        run(&mut world, &mut script);
    }
    // Then a quiet sync: everything meets.
    let ended = run(&mut world, &mut std::iter::empty());
    prop_assert!(
        matches!(
            ended,
            Some(Ended::Synced {
                confirmed: true,
                ..
            })
        ),
        "{ended:?}"
    );
    prop_assert!(
        world.saved.is_subset(&world.remote),
        "SAVES: Door43 lacks {:?}",
        world.saved.difference(&world.remote).collect::<Vec<_>>()
    );
    prop_assert!(
        world.pushed_by_teammates.is_subset(&world.local),
        "RECEIVED: the project lacks {:?}",
        world
            .pushed_by_teammates
            .difference(&world.local)
            .collect::<Vec<_>>()
    );
    prop_assert!(world.saved.is_subset(&world.local));
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    /// INV-PUSH-IS-NEVER-FORCED, INV-RECEIVED-WORK-IS-KEPT-WHOLE-OR-ABSENT,
    /// INV-OWN-WORK-NEVER-REVERTED and INV-SAVES-CONTINUE-DURING-SYNC over random syncs.
    #[test]
    fn random_syncs_lose_no_work(script in prop::collection::vec(between(), 0..120)) {
        lose_no_work(script)?;
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 5_000, ..ProptestConfig::default() })]

    #[test]
    #[ignore = "nightly: 5,000 cases"]
    fn random_syncs_lose_no_work_long(script in prop::collection::vec(between(), 0..400)) {
        lose_no_work(script)?;
    }
}
