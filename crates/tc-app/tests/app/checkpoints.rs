//! D1: what a checkpoint comes to on the screens.

use tc_api::event::CheckpointOutcome;
use tc_app::backend::store::checkpoint_outcome;
use tc_store::{CheckpointReport, CheckpointSkip};

fn skipped(skip: CheckpointSkip) -> CheckpointOutcome {
    checkpoint_outcome(Ok(CheckpointReport {
        commit: None,
        sealed: Vec::new(),
        skipped: Some(skip),
        intake: tc_store::IntakeReport::default(),
    }))
}

/// INV-A-FAILED-CHECKPOINT-IS-SHOWN-UNTIL-RETRIED: every skip has its outcome; a skip that is
/// no failure is `Unchanged`, and every other is `Failed` with words, never hidden, and with a
/// code, so Home's banner offers Ask for help (a problem in the app, tC4 `askHelpFor`).
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
            matches!(skipped(skip), CheckpointOutcome::Failed { reason, code: Some(_) } if !reason.is_empty()),
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

/// INV-A-FAILED-CHECKPOINT-IS-SHOWN-UNTIL-RETRIED: a checkpoint that a full disk stopped at any
/// write point fails, and once the disk has room its retry commits and clears the failure; the
/// folder stays a valid Scripture Burrito throughout (the store's watch).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retry_after_a_full_disk_commits_and_clears_the_failure() {
    use tc_app::backend::CheckpointReason;
    let mut failed_at = Vec::new();
    for n in 0..20 {
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
        world.fs.fail_op(n, std::io::ErrorKind::StorageFull);
        let first = t
            .app
            .sessions()
            .checkpoint(&session, CheckpointReason::LeavingTranslate)
            .await;
        if !matches!(first, CheckpointOutcome::Failed { .. }) {
            continue;
        }
        assert_eq!(failed(&t).await.len(), 1, "fail point {n}");
        let retried = t
            .app
            .sessions()
            .checkpoint(&session, CheckpointReason::Other("retry".into()))
            .await;
        assert_eq!(retried, CheckpointOutcome::Committed, "fail point {n}");
        assert!(failed(&t).await.is_empty(), "fail point {n}");
        failed_at.push(n);
    }
    assert!(
        !failed_at.is_empty(),
        "no fail point stopped the checkpoint"
    );
}

/// INV-A-FAILED-CHECKPOINT-IS-SHOWN-UNTIL-RETRIED: after a failed checkpoint another computer
/// writes into the folder (a synced folder), so the project turns read-only and the retry skips
/// without looking. The failure stays shown: the saved verse is still not in the history.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_retry_skipped_by_a_second_writer_keeps_the_failure_shown() {
    use tc_app::backend::{CheckpointReason, ProjectStore};
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
        .checkpoint(&session, CheckpointReason::LeavingTranslate)
        .await;
    world.fs.fail_writes(None);
    assert!(
        matches!(first, CheckpointOutcome::Failed { .. }),
        "{first:?}"
    );
    let (theirs, _theirs_watch) = crate::store_world::backend_over(
        &world.fs.other_machine(),
        "/app-desktop",
        tc_store::FakePlatform {
            machine: "machine-2".to_owned(),
            host: "host-2".to_owned(),
            ..tc_store::FakePlatform::default()
        },
        std::sync::Arc::default(),
    );
    theirs.trust_lock(&world.bible);
    let handle = theirs
        .open(&world.bible, crate::store_world::progress())
        .unwrap();
    crate::store_world::save(handle.as_ref(), &crate::store_world::tit(1, 15), "theirs")
        .await
        .unwrap();
    handle.wait_idle();
    let retried = t
        .app
        .sessions()
        .checkpoint(&session, CheckpointReason::Other("retry".into()))
        .await;
    assert!(
        matches!(
            session.handle().facts().mode,
            tc_api::event::SessionMode::ReadOnly(_)
        ),
        "{:?}",
        session.handle().facts().mode
    );
    assert!(
        matches!(retried, CheckpointOutcome::Failed { .. }),
        "{retried:?}"
    );
    assert_eq!(failed(&t).await.len(), 1);
    drop(handle);
}

/// tC4 `retryOwedCheckpoint`: opening a project whose last checkpoint failed runs it again, so
/// Home's "Open the project again to retry" is true; a commit clears the banner.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn opening_the_project_again_retries_its_failed_checkpoint() {
    use tc_app::session::CheckpointState;
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
    drop(session);
    let before = commits(&world);
    world.fs.fail_writes(Some(std::io::ErrorKind::StorageFull));
    t.app.sessions().leave_screen(&open.at).unwrap();
    t.app.sessions().settled(&world.bible).await;
    world.fs.fail_writes(None);
    assert_eq!(failed(&t).await.len(), 1);
    t.app
        .sessions()
        .open_screen(&world.bible, super::common::open_progress())
        .await
        .unwrap();
    let stop = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let state = loop {
        let state = t.app.sessions().checkpoint_state(&world.bible);
        if !matches!(
            state,
            CheckpointState::Running | CheckpointState::Ended(CheckpointOutcome::Failed { .. })
        ) || std::time::Instant::now() > stop
        {
            break state;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    };
    // The store's open may commit the owed work itself first; the retry then finds it done.
    assert!(
        matches!(
            state,
            CheckpointState::Ended(CheckpointOutcome::Committed | CheckpointOutcome::Unchanged)
        ),
        "the open retried the owed checkpoint: {state:?}"
    );
    assert!(failed(&t).await.is_empty());
    assert_eq!(commits(&world), before + 1);
}

/// The commits of the Bible project on the store world's filesystem.
fn commits(world: &crate::store_world::StoreWorld) -> usize {
    use tc_store::Vfs;
    let path = std::path::Path::new(crate::store_world::PROJECTS).join(world.bible.as_str());
    let repo = git2::Repository::open(world.fs.outside_dir(&path).unwrap()).unwrap();
    let mut walk = repo.revwalk().unwrap();
    walk.push_head().unwrap();
    walk.count()
}

/// Q-5's two minutes count from a mode switch whose checkpoint committed: a switch with
/// nothing to commit starts no wait, so the next switch's work is committed at once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_switch_that_commits_nothing_starts_no_wait() {
    use tc_app::backend::CheckpointReason;
    let world = crate::store_world::store_world();
    let (_dir, t) = store_app(&world).await;
    let open = t
        .app
        .sessions()
        .open_screen(&world.bible, super::common::open_progress())
        .await
        .unwrap();
    let session = t.app.read(&open.at).unwrap();
    let sessions = t.app.sessions();
    // What the open itself wrote is committed first, outside the policy's clock.
    sessions
        .checkpoint(&session, CheckpointReason::Other("open".into()))
        .await;
    assert_eq!(
        sessions
            .mode_switch(&session, CheckpointReason::LeavingTranslate)
            .await,
        Some(CheckpointOutcome::Unchanged)
    );
    crate::store_world::save(
        session.handle().as_ref(),
        &crate::store_world::tit(1, 2),
        "a change to commit",
    )
    .await
    .unwrap();
    session.handle().wait_idle();
    assert_eq!(
        sessions
            .mode_switch(&session, CheckpointReason::LeavingTranslate)
            .await,
        Some(CheckpointOutcome::Committed),
        "the switch after one that committed nothing"
    );
    crate::store_world::save(
        session.handle().as_ref(),
        &crate::store_world::tit(1, 3),
        "another change",
    )
    .await
    .unwrap();
    session.handle().wait_idle();
    assert_eq!(
        sessions
            .mode_switch(&session, CheckpointReason::LeavingTranslate)
            .await,
        None,
        "within two minutes of a commit"
    );
}
