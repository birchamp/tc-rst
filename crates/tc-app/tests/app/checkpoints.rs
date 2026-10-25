//! D1: what a checkpoint comes to on the screens.

use tc_api::event::CheckpointOutcome;
use tc_app::backend::store::checkpoint_outcome;
use tc_store::{CheckpointReport, CheckpointSkip};

fn skipped(skip: CheckpointSkip) -> CheckpointOutcome {
    checkpoint_outcome(Ok(CheckpointReport {
        commit: None,
        sealed: Vec::new(),
        skipped: Some(skip),
    }))
}

/// INV-A-FAILED-CHECKPOINT-IS-SHOWN-UNTIL-RETRIED: every skip has its outcome; a skip that is
/// no failure is `Unchanged`, and every other is `Failed` with words, never hidden.
#[test]
fn every_checkpoint_skip_has_an_outcome() {
    for skip in [
        CheckpointSkip::NoChanges,
        CheckpointSkip::ReadOnly,
        CheckpointSkip::SecondWriter,
    ] {
        assert_eq!(skipped(skip), CheckpointOutcome::Unchanged, "{skip:?}");
    }
    for skip in [
        CheckpointSkip::NoHistory,
        CheckpointSkip::SealFailed,
        CheckpointSkip::PackMissing,
        CheckpointSkip::TreeInvalid,
        CheckpointSkip::HeadMoved,
    ] {
        assert!(
            matches!(skipped(skip), CheckpointOutcome::Failed { reason } if !reason.is_empty()),
            "{skip:?}"
        );
    }
}

/// The app over the store world, its projects folder a temporary one.
pub(super) async fn store_app(
    world: &crate::store_world::StoreWorld,
) -> (tempfile::TempDir, super::common::TestApp) {
    use tc_app::StoreChoice;
    use tc_app::config::ProjectsFolder;
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join("projects");
    std::fs::create_dir(&projects).unwrap();
    let t = super::common::TestApp::start_with(
        dir.path(),
        &super::common::Parts::new(),
        |config, deps| {
            config.projects = ProjectsFolder::Given(projects);
            deps.store = StoreChoice::Given(world.store());
        },
    )
    .await;
    (dir, t)
}

/// What `hello` says failed.
async fn failed(t: &super::common::TestApp) -> Vec<tc_api::app::FailedCheckpoint> {
    super::common::post(&t.app, &tc_api::app::Hello {})
        .await
        .unwrap()
        .checkpoints
}

/// INV-A-FAILED-CHECKPOINT-IS-SHOWN-UNTIL-RETRIED (D1): a leave checkpoint that failed on a
/// full disk is in `hello` after it, so a reload or a Resync shows "Checkpoint not saved" again,
/// until a checkpoint of the project ends otherwise.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resync_after_a_failed_checkpoint_shows_it_again() {
    let world = crate::store_world::store_world();
    let (_dir, t) = store_app(&world).await;
    let open = t
        .app
        .sessions()
        .open_screen(&world.bible, super::common::open_progress())
        .await
        .unwrap();
    let session = t.app.read(&open.at).unwrap();
    crate::store_world::save(
        session.handle().as_ref(),
        &crate::store_world::tit(1, 2),
        "a change to commit",
    )
    .await
    .unwrap();
    session.handle().wait_idle();
    world.fs.fail_writes(Some(std::io::ErrorKind::StorageFull));
    t.app.sessions().leave_screen(&open.at).unwrap();
    t.app.sessions().settled(&world.bible).await;
    let listed = failed(&t).await;
    world.fs.fail_writes(None);
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].project, world.bible);
    assert!(!listed[0].reason.is_empty());
    // A later hello still lists it: nothing but another checkpoint clears it.
    assert_eq!(failed(&t).await, listed);
}

/// The save indicator's Retry runs the checkpoint once more: one commit, and the failure is
/// no longer listed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retry_after_a_failed_checkpoint_commits_once() {
    let world = crate::store_world::store_world();
    let (_dir, t) = store_app(&world).await;
    let open = t
        .app
        .sessions()
        .open_screen(&world.bible, super::common::open_progress())
        .await
        .unwrap();
    let session = t.app.read(&open.at).unwrap();
    crate::store_world::save(
        session.handle().as_ref(),
        &crate::store_world::tit(1, 2),
        "a change to commit",
    )
    .await
    .unwrap();
    session.handle().wait_idle();
    world.fs.fail_writes(Some(std::io::ErrorKind::StorageFull));
    let first = t
        .app
        .sessions()
        .checkpoint(
            &session,
            tc_app::backend::CheckpointReason::LeavingTranslate,
        )
        .await;
    world.fs.fail_writes(None);
    assert!(
        matches!(first, CheckpointOutcome::Failed { .. }),
        "{first:?}"
    );
    assert_eq!(failed(&t).await.len(), 1);
    let retried = super::common::post(
        &t.app,
        &tc_api::session::RetryCheckpoint {
            request: tc_api::ids::RequestId::from_bytes([7; 16]),
            at: open.at.clone(),
        },
    )
    .await
    .unwrap();
    assert_eq!(retried, CheckpointOutcome::Committed);
    assert!(
        failed(&t).await.is_empty(),
        "a committed retry clears the failure"
    );
}

mod policy {
    use std::time::Duration;

    use tc_api::ids::WallMs;
    use tc_app::backend::CheckpointReason;
    use tc_app::session::{CheckpointPolicy, MODE_SWITCH_POLICY, checkpoint_due};

    const MINUTE: i64 = 60_000;

    /// Q-5's fallback: a mode switch checkpoints at most once every two minutes.
    #[test]
    fn a_mode_switch_checkpoints_at_most_once_every_two_minutes() {
        assert_eq!(
            MODE_SWITCH_POLICY,
            CheckpointPolicy::AtMostEvery(Duration::from_secs(120))
        );
        let switch = CheckpointReason::LeavingTranslate;
        let now = WallMs(10 * MINUTE);
        assert!(
            checkpoint_due(MODE_SWITCH_POLICY, None, now, &switch),
            "the first switch"
        );
        assert!(!checkpoint_due(
            MODE_SWITCH_POLICY,
            Some(WallMs(now.0 - MINUTE)),
            now,
            &switch
        ));
        assert!(checkpoint_due(
            MODE_SWITCH_POLICY,
            Some(WallMs(now.0 - 2 * MINUTE)),
            now,
            &switch
        ));
        assert!(!checkpoint_due(
            MODE_SWITCH_POLICY,
            Some(WallMs(now.0 - MINUTE)),
            now,
            &CheckpointReason::LeavingMode("Check".into())
        ));
    }

    /// Going Home always looks for changes, however recent the last mode switch's checkpoint.
    #[test]
    fn leaving_the_project_always_checks_for_changes() {
        let now = WallMs(10 * MINUTE);
        assert!(checkpoint_due(
            MODE_SWITCH_POLICY,
            Some(now),
            now,
            &CheckpointReason::LeavingProject
        ));
    }

    /// The rule is a value: with every switch checkpointing, a second switch within two
    /// minutes checkpoints too.
    #[test]
    fn every_mode_switch_policy_checkpoints_each_switch() {
        let now = WallMs(10 * MINUTE);
        assert!(checkpoint_due(
            CheckpointPolicy::EveryModeSwitch,
            Some(WallMs(now.0 - 1_000)),
            now,
            &CheckpointReason::LeavingTranslate
        ));
    }
}
