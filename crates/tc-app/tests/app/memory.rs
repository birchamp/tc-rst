//! The memory backend (unit B1b, APP-DESIGN §4.2, AD6): projects in memory behind the
//! storage seam, commands that answer only what a test scripted, and teammates' changes
//! folded as an intake would fold them.

use std::sync::Arc;
use std::time::Duration;

use tc_api::event::{CheckpointOutcome, SessionMode};
use tc_api::write::ReadOnlyReason;
use tc_app::backend::memory::{MemoryBackend, Scripted, teammates};
use tc_app::backend::{
    CheckpointReason, CommandError, OpenFailure, ProjectHandle, ProjectStore, StoreCause,
    StoreCommand, StoreEvent, Stored,
};
use tc_app::jobs::ProgressSink;
use tc_journal::command::Command;
use tc_journal::fold::Resolved;
use tc_journal::{ChangeId, DeviceSeq, RecordKey};

use super::common::{folder, sample_backend, tit};

fn progress() -> ProgressSink {
    ProgressSink::new(tc_api::jobs::Phase::Open(
        tc_api::jobs::OpenPhase::ReadingChanges,
    ))
}

fn open(backend: &MemoryBackend) -> Arc<dyn ProjectHandle> {
    backend.open(&folder("sample_burrito"), progress()).unwrap()
}

fn save(handle: &dyn ProjectHandle, key: &RecordKey, words: &str) -> StoreCommand {
    let token = handle.snapshot().mint(key, None).unwrap();
    StoreCommand::save_verse(token, words.to_owned())
}

fn two_teammates(backend: &MemoryBackend) -> Vec<tc_journal::Change> {
    backend
        .teammates_edit_verse(
            &folder("sample_burrito"),
            &tit(1, 1),
            &["Paul, a servant", "Paul, a slave"],
        )
        .unwrap()
}

#[tokio::test]
async fn a_second_open_of_a_folder_with_a_live_handle_is_lock_held() {
    let backend = sample_backend();
    let first = open(&backend);
    let second = backend.open(&folder("sample_burrito"), progress());
    assert!(
        matches!(&second, Err(OpenFailure::LockHeld { holder: Some((host, pid)) })
            if host == "this computer" && *pid == std::process::id()),
        "{:?}",
        second.map(|_| ())
    );
    first.close().await.unwrap();
    let again = open(&backend);
    assert_eq!(
        again.snapshot().seq(),
        1,
        "a new open starts its own sequence"
    );
    // A handle dropped without close lets go too, as tc-store's does.
    drop(again);
    open(&backend);
}

#[tokio::test]
async fn an_unknown_folder_is_not_a_project() {
    let backend = sample_backend();
    assert!(matches!(
        backend.open(&folder("nothing_here"), progress()),
        Err(OpenFailure::NotAProject)
    ));
    let listed: Vec<String> = backend
        .list()
        .unwrap()
        .projects
        .into_iter()
        .map(|e| e.folder.to_string())
        .collect();
    assert_eq!(listed, ["sample_burrito"]);
}

#[tokio::test]
async fn commands_are_unavailable_without_a_script() {
    let backend = sample_backend();
    let handle = open(&backend);
    let before = handle.snapshot();
    let reply = handle
        .submit(save(handle.as_ref(), &tit(1, 1), "Paul"))
        .await;
    assert_eq!(reply.unwrap_err(), CommandError::Unavailable);
    let after = handle.snapshot();
    assert_eq!(after.seq(), before.seq());
    assert_eq!(
        after.fold().heads(&tit(1, 1)),
        before.fold().heads(&tit(1, 1))
    );
    // The command reached the backend as the store would have received it.
    let submitted = backend.submitted(&folder("sample_burrito"));
    assert!(
        matches!(submitted.as_slice(), [Command::SaveVerse { words, .. }] if words == "Paul"),
        "{submitted:?}"
    );
}

#[tokio::test]
async fn scripted_written_returns_a_fresh_token_for_the_displayed_value_and_changes_nothing() {
    let backend = sample_backend();
    let handle = open(&backend);
    backend
        .script(&folder("sample_burrito"), Scripted::Written)
        .unwrap();
    let shown = handle.snapshot().mint(&tit(1, 1), None).unwrap();
    let shown_heads = shown.heads().to_vec();
    let reply = handle
        .submit(StoreCommand::save_verse(shown, "Paul".to_owned()))
        .await
        .unwrap();
    assert_eq!(reply.stored, Stored::Durable);
    assert_eq!(reply.tokens.len(), 1);
    assert_eq!(reply.tokens[0].0, tit(1, 1));
    assert_eq!(reply.tokens[0].1.heads(), shown_heads.as_slice());
    assert_eq!(reply.seq, 1);
    assert!(reply.changes.is_empty() && reply.new_conflicts.is_empty());
    assert_eq!(handle.snapshot().seq(), 1);
    assert_eq!(handle.snapshot().fold().heads(&tit(1, 1)), shown_heads);
    // The script was used once: the next command has none.
    let next = handle
        .submit(save(handle.as_ref(), &tit(1, 1), "Paul"))
        .await;
    assert_eq!(next.unwrap_err(), CommandError::Unavailable);
}

#[tokio::test]
async fn scripted_not_written_and_written_not_durable_are_returned_as_scripted() {
    let backend = sample_backend();
    let handle = open(&backend);
    let project = folder("sample_burrito");
    backend
        .script(
            &project,
            Scripted::NotWritten("the disk is full".to_owned()),
        )
        .unwrap();
    backend
        .script(
            &project,
            Scripted::WrittenNotDurable("fsync failed".to_owned()),
        )
        .unwrap();
    let first = handle.submit(save(handle.as_ref(), &tit(1, 2), "x")).await;
    assert_eq!(
        first.unwrap_err(),
        CommandError::NotWritten("the disk is full".to_owned())
    );
    let second = handle
        .submit(save(handle.as_ref(), &tit(1, 2), "x"))
        .await
        .unwrap();
    assert_eq!(
        second.stored,
        Stored::WrittenNotDurable("fsync failed".to_owned())
    );
    assert_eq!(second.tokens.len(), 1);
}

#[tokio::test]
async fn scripted_read_only_switches_the_mode_and_refuses_every_later_command() {
    let backend = sample_backend();
    let handle = open(&backend);
    let mut events = handle.subscribe();
    backend
        .script(
            &folder("sample_burrito"),
            Scripted::ReadOnly(ReadOnlyReason::CloudSynced),
        )
        .unwrap();
    let first = handle.submit(save(handle.as_ref(), &tit(1, 1), "x")).await;
    assert_eq!(
        first.unwrap_err(),
        CommandError::ReadOnly(ReadOnlyReason::CloudSynced)
    );
    assert!(matches!(
        events.recv().await,
        Some(StoreEvent::Mode(SessionMode::ReadOnly(
            ReadOnlyReason::CloudSynced
        )))
    ));
    assert_eq!(
        handle.facts().mode,
        SessionMode::ReadOnly(ReadOnlyReason::CloudSynced)
    );
    // No script now, and still read-only rather than unavailable.
    backend
        .script(&folder("sample_burrito"), Scripted::Written)
        .unwrap();
    let second = handle.submit(save(handle.as_ref(), &tit(1, 1), "x")).await;
    assert_eq!(
        second.unwrap_err(),
        CommandError::ReadOnly(ReadOnlyReason::CloudSynced)
    );
}

#[tokio::test]
async fn scripted_teammate_heads_reach_the_fold_before_the_reply() {
    let backend = sample_backend();
    let handle = open(&backend);
    let mut events = handle.subscribe();
    let changes = two_teammates(&backend);
    let devices: Vec<_> = changes.iter().map(|c| c.body().device).collect();
    assert_eq!(devices, [teammates::device(1), teammates::device(2)]);
    backend
        .script(
            &folder("sample_burrito"),
            Scripted::TeammateHeads(changes, Box::new(Scripted::Written)),
        )
        .unwrap();
    let reply = handle
        .submit(save(handle.as_ref(), &tit(1, 2), "x"))
        .await
        .unwrap();
    assert_eq!(reply.seq, 2);
    let newest = handle.snapshot();
    assert_eq!(newest.seq(), 2);
    let state = newest.fold().key_state(&tit(1, 1)).unwrap();
    assert_eq!(state.heads.len(), 2);
    assert!(matches!(state.state, Resolved::Conflict(_)));
    match events.recv().await {
        Some(StoreEvent::Committed { seq, delta, cause }) => {
            assert_eq!(seq, 2);
            assert_eq!(cause, StoreCause::Intake { devices });
            assert!(delta.keys.contains(&tit(1, 1)), "{delta:?}");
            assert!(!delta.new_conflicts.is_empty());
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_reply_lists_only_what_the_screen_showed_when_a_teammate_changed_that_record() {
    let backend = sample_backend();
    let project = folder("sample_burrito");
    let handle = open(&backend);
    let shown = handle.snapshot().mint(&tit(1, 1), None).unwrap();
    let shown_heads = shown.heads().to_vec();
    // One teammate's change continues the version the screen shows; two make a conflict.
    for texts in [
        &["a teammate's words"][..],
        &["one teammate", "another teammate"],
    ] {
        let theirs = backend
            .teammates_edit_verse(&project, &tit(1, 1), texts)
            .unwrap();
        let ids: Vec<ChangeId> = theirs.iter().map(|c| c.id()).collect();
        backend
            .script(
                &project,
                Scripted::TeammateHeads(theirs, Box::new(Scripted::Written)),
            )
            .unwrap();
        let reply = handle
            .submit(StoreCommand::save_verse(
                shown.clone(),
                "the person's words".to_owned(),
            ))
            .await
            .unwrap();
        let newest = handle.snapshot();
        let mut heads = newest.fold().heads(&tit(1, 1));
        heads.retain(|head| ids.contains(head));
        assert!(!heads.is_empty(), "the teammates' change reached the fold");
        assert_eq!(reply.seq, newest.seq());
        // The token names the version the person saw, never the teammates' change: a save
        // with it makes a conflict with their words instead of replacing them unseen (I4).
        assert_eq!(reply.tokens.len(), 1);
        assert_eq!(reply.tokens[0].0, tit(1, 1));
        assert_eq!(
            reply.tokens[0].1.heads(),
            shown_heads.as_slice(),
            "{texts:?}"
        );
        assert!(reply.tokens[0].1.heads().iter().all(|h| !ids.contains(h)));
        // It is this project's token, as a minted one is.
        let wire = reply.tokens[0].1.to_wire().unwrap();
        assert_eq!(
            handle.parse_edit(&wire).unwrap().heads(),
            shown_heads.as_slice()
        );
    }
}

#[tokio::test]
async fn a_teammate_who_edits_again_continues_their_sequence() {
    let backend = sample_backend();
    let project = folder("sample_burrito");
    let first = backend
        .teammates_edit_verse(&project, &tit(1, 1), &["first words"])
        .unwrap();
    backend.teammate_edits(&project, first.clone()).unwrap();
    let second = backend
        .teammates_edit_verse(&project, &tit(1, 1), &["second words"])
        .unwrap();
    backend.teammate_edits(&project, second.clone()).unwrap();
    let fold = backend.fold(&project).unwrap();
    assert_eq!(
        fold.anomalies().collect::<Vec<_>>(),
        Vec::<&tc_journal::fold::Anomaly>::new(),
        "no notice that a screen of tC4 would not show"
    );
    let (first, second) = (first[0].body(), second[0].body());
    assert_eq!(first.device, teammates::device(1));
    assert_eq!(second.device, teammates::device(1));
    assert_eq!((first.seq, second.seq), (DeviceSeq(1), DeviceSeq(2)));
    assert_ne!(first.batch.id, second.batch.id);
    assert!(first.at < second.at);
    let second_id = fold.projected_change(&tit(1, 1)).unwrap();
    assert_eq!(fold.heads(&tit(1, 1)), [second_id]);
    let history = fold.history(&tit(1, 1));
    assert_eq!(history.last(), Some(&second_id));
    // A fold built by hand keeps its own sequences the same way.
    let mut by_hand = teammates::Teammates::new();
    let seed = sample_backend().fold(&project).unwrap();
    let one = by_hand.edit_verse(&seed, &tit(1, 2), &["x"]).unwrap();
    let two = by_hand.edit_verse(&seed, &tit(1, 3), &["y"]).unwrap();
    assert_eq!(
        (one[0].body().seq, two[0].body().seq),
        (DeviceSeq(1), DeviceSeq(2))
    );
}

#[tokio::test(start_paused = true)]
async fn scripted_delay_holds_the_reply_not_the_caller() {
    let backend = sample_backend();
    let handle = open(&backend);
    backend
        .script(
            &folder("sample_burrito"),
            Scripted::After(Duration::from_secs(5), Box::new(Scripted::Written)),
        )
        .unwrap();
    // `submit` itself returns at once: it is not async.
    let mut reply = handle.submit(save(handle.as_ref(), &tit(1, 1), "x"));
    tokio::time::sleep(Duration::from_millis(4_900)).await;
    assert!(
        futures_poll_once(&mut reply).await.is_none(),
        "the reply waits for the delay"
    );
    tokio::time::sleep(Duration::from_millis(200)).await;
    let done = futures_poll_once(&mut reply).await;
    assert!(matches!(done, Some(Ok(_))), "{done:?}");
}

/// Polls `future` once: `Some` with its output when it is ready.
async fn futures_poll_once<F: std::future::Future + Unpin>(future: &mut F) -> Option<F::Output> {
    std::future::poll_fn(|cx| {
        std::task::Poll::Ready(match std::pin::Pin::new(&mut *future).poll(cx) {
            std::task::Poll::Ready(out) => Some(out),
            std::task::Poll::Pending => None,
        })
    })
    .await
}

#[tokio::test]
async fn teammate_edits_of_a_closed_project_reach_the_next_open() {
    let backend = sample_backend();
    let changes = two_teammates(&backend);
    let delta = backend
        .teammate_edits(&folder("sample_burrito"), changes)
        .unwrap();
    assert!(delta.keys.contains(&tit(1, 1)));
    let handle = open(&backend);
    assert_eq!(handle.snapshot().fold().heads(&tit(1, 1)).len(), 2);
}

#[cfg(feature = "test-hooks")]
#[tokio::test]
async fn reset_restores_every_project_to_its_seed() {
    let backend = sample_backend();
    let project = folder("sample_burrito");
    let seed_heads = backend.fold(&project).unwrap().heads(&tit(1, 1));
    let handle = open(&backend);
    backend
        .script(&project, Scripted::ReadOnly(ReadOnlyReason::UnsafeDrive))
        .unwrap();
    let _ = handle.submit(save(handle.as_ref(), &tit(1, 1), "x")).await;
    backend
        .teammate_edits(&project, two_teammates(&backend))
        .unwrap();
    backend.script(&project, Scripted::Written).unwrap();
    // An open project is not reset under its session.
    assert!(backend.reset().is_err());
    handle.close().await.unwrap();
    backend.reset().unwrap();
    let handle = open(&backend);
    assert_eq!(handle.facts().mode, SessionMode::Writable);
    assert_eq!(handle.snapshot().fold().heads(&tit(1, 1)), seed_heads);
    assert!(backend.submitted(&project).is_empty());
    // The queued script went with the reset.
    let reply = handle.submit(save(handle.as_ref(), &tit(1, 1), "x")).await;
    assert_eq!(reply.unwrap_err(), CommandError::Unavailable);
}

#[tokio::test]
async fn tokens_are_the_same_in_every_backend() {
    let wire = |backend: &MemoryBackend| {
        let handle = open(backend);
        let token = handle.snapshot().mint(&tit(1, 1), None).unwrap();
        serde_json::to_string(&token.to_wire().unwrap()).unwrap()
    };
    let first = wire(&sample_backend());
    assert_eq!(first, wire(&sample_backend()));
    assert!(first.starts_with("\"st1."), "{first}");
}

#[tokio::test]
async fn a_snapshot_does_not_move_when_the_project_does() {
    let backend = sample_backend();
    let handle = open(&backend);
    let before = handle.snapshot();
    backend
        .teammate_edits(&folder("sample_burrito"), two_teammates(&backend))
        .unwrap();
    let after = handle.snapshot();
    assert_eq!(before.seq(), 1);
    assert_eq!(before.fold().heads(&tit(1, 1)).len(), 1);
    assert_eq!(after.seq(), 2);
    assert_eq!(after.fold().heads(&tit(1, 1)).len(), 2);
    // A token minted from the snapshot the screen read names only what it showed.
    let shown: Vec<ChangeId> = before.mint(&tit(1, 1), None).unwrap().heads().to_vec();
    assert_eq!(shown, before.fold().heads(&tit(1, 1)));
}

#[tokio::test]
async fn a_memory_project_checkpoints_nothing_and_closes_its_events() {
    let backend = sample_backend();
    let handle = open(&backend);
    let mut events = handle.subscribe();
    assert_eq!(
        handle.checkpoint(CheckpointReason::LeavingProject).await,
        CheckpointOutcome::Unchanged
    );
    handle.clone().close().await.unwrap();
    assert!(
        events.recv().await.is_none(),
        "the event channel ends at close"
    );
    let after = handle.submit(save(handle.as_ref(), &tit(1, 1), "x")).await;
    assert_eq!(after.unwrap_err(), CommandError::Closed);
}
