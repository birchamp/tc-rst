//! The save indicator's Retry and words (X2, tC4's `retryFor` and `SaveIndicator`).

use tc_api::align::AlignSave;
use tc_api::buffer::FieldKind;
use tc_api::write::NotSaved;
use tc_ui::app::save::indicator::{IndicatorStatus, Retry, Worst, indicator, retry_for};

#[test]
fn retry_follows_tc4s_precedence() {
    use FieldKind::*;
    assert_eq!(retry_for(&[Verse, Comment, StoryRef], true), Retry::Story);
    assert_eq!(retry_for(&[Frame], false), Retry::Story);
    assert_eq!(retry_for(&[StoryTitle], false), Retry::Story);
    assert_eq!(retry_for(&[Verse, Comment], true), Retry::Note);
    assert_eq!(retry_for(&[], true), Retry::Checkpoint);
    assert_eq!(retry_for(&[Verse, Test, CheckComment], false), Retry::Save);
    assert_eq!(Retry::Story.test_id(), Some("retry-story-save"));
    assert_eq!(Retry::Note.test_id(), Some("retry-note-save"));
    assert_eq!(Retry::Checkpoint.test_id(), Some("retry-checkpoint"));
    assert_eq!(Retry::Save.test_id(), None);
}

#[test]
fn the_indicator_says_what_tc4_says() {
    let shown = |worst, checkpoint, failed: &[FieldKind]| indicator(worst, &[], checkpoint, failed);
    let saved = shown(Worst::Saved, false, &[]);
    assert_eq!(saved.label_key().as_str(), "app.saved");
    assert_eq!(saved.status(), IndicatorStatus::Valid);
    assert!(!saved.is_error());
    assert_eq!(
        shown(Worst::Saving, false, &[]).label_key().as_str(),
        "app.saving"
    );
    let dirty = shown(Worst::Dirty, false, &[]);
    assert_eq!(dirty.label_key().as_str(), "app.unsaved");
    assert_eq!(dirty.status(), IndicatorStatus::Warn);
    let error = shown(Worst::Error, true, &[FieldKind::Test]);
    assert_eq!(error.label_key().as_str(), "app.saveError");
    assert_eq!(error.status(), IndicatorStatus::Invalid);
    assert_eq!(error.retry, Retry::Save);
    assert!(error.is_error());
    // A failed checkpoint alone: its own words and its own Retry, the state stays the fields'.
    let commit = shown(Worst::Dirty, true, &[]);
    assert_eq!(commit.label_key().as_str(), "app.commitError");
    assert_eq!(commit.state.data_state(), "dirty");
    assert_eq!(commit.retry, Retry::Checkpoint);
    assert!(commit.is_error());
    // An aligner's failure is a failed save.
    let align = indicator(
        Worst::Saved,
        &[AlignSave::Failed(NotSaved::LookAgain)],
        true,
        &[],
    );
    assert_eq!(align.state, Worst::Error);
    assert!(!align.commit_error);
    assert_eq!(align.retry, Retry::Save);
}

/// J5: links the server has not stored hold the close guard, whatever the fields say; it lets
/// go only when every field and every aligner is saved (tC4's `allSaved` counts
/// `alignSaveState`).
#[test]
fn unsaved_links_hold_the_close_guard() {
    use tc_ui::app::save::indicator::all_clean;
    assert!(all_clean(Worst::Saved, &[]));
    assert!(all_clean(Worst::Saved, &[AlignSave::Saved]));
    for held in [
        AlignSave::Unsaved,
        AlignSave::Saving,
        AlignSave::Failed(NotSaved::LookAgain),
    ] {
        assert!(
            !all_clean(Worst::Saved, std::slice::from_ref(&held)),
            "{held:?}"
        );
    }
    // One aligner saved does not excuse another that is not.
    assert!(!all_clean(
        Worst::Saved,
        &[AlignSave::Saved, AlignSave::Unsaved]
    ));
    // And a failed checkpoint alone does not hold it, as in tC4.
    assert!(all_clean(Worst::Saved, &[AlignSave::Saved]));
    let unsaved = indicator(Worst::Saved, &[AlignSave::Unsaved], false, &[]);
    assert_eq!(unsaved.label_key().as_str(), "app.unsaved");
}
