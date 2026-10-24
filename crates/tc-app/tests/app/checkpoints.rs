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
