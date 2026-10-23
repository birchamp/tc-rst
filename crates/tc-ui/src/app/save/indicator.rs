//! The save indicator (X2) and the close guard's value (APP-DESIGN 6.5).

use tc_api::align::AlignSave;
use tc_api::buffer::FieldKind;

use crate::i18n::{Key, keys};

/// The worst state of the fields, in tC4's rank (`tC4:src/App.jsx:57-62`): an error, then a
/// save in flight, then unsaved text, then saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Worst {
    Saved,
    Dirty,
    Saving,
    Error,
}

impl Worst {
    /// tC4's `data-state` value of the indicator.
    pub fn data_state(self) -> &'static str {
        match self {
            Self::Saved => "saved",
            Self::Dirty => "dirty",
            Self::Saving => "saving",
            Self::Error => "error",
        }
    }

    fn of_align(save: &AlignSave) -> Self {
        match save {
            AlignSave::Saved => Self::Saved,
            AlignSave::Unsaved => Self::Dirty,
            AlignSave::Saving => Self::Saving,
            AlignSave::Failed(_) => Self::Error,
        }
    }
}

/// Which failure the indicator's Retry retries (tC4's `retryFor`, `tC4:src/App.jsx:33-38`): a
/// story save, a note save, the checkpoint, or the other saves. Every one but the checkpoint
/// retries every failed field (A4b's per-field rule), which loses nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Retry {
    Story,
    Note,
    Checkpoint,
    Save,
}

impl Retry {
    /// The Retry button's `data-testid`; none for the other saves, as in tC4.
    pub fn test_id(self) -> Option<&'static str> {
        match self {
            Self::Story => Some("retry-story-save"),
            Self::Note => Some("retry-note-save"),
            Self::Checkpoint => Some("retry-checkpoint"),
            Self::Save => None,
        }
    }
}

/// tC4's precedence: a failed story save (a frame, a title, a reference), then a failed note
/// (a comment), then the checkpoint when only it failed, then the other saves.
pub fn retry_for(failed: &[FieldKind], commit_only: bool) -> Retry {
    let story = failed.iter().any(|kind| {
        matches!(
            kind,
            FieldKind::Frame | FieldKind::StoryTitle | FieldKind::StoryRef
        )
    });
    if story {
        Retry::Story
    } else if failed.contains(&FieldKind::Comment) {
        Retry::Note
    } else if commit_only {
        Retry::Checkpoint
    } else {
        Retry::Save
    }
}

/// What X2 shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Indicator {
    pub state: Worst,
    /// "Checkpoint not saved", shown only when no save failed (`tC4:src/App.jsx:63-67`).
    pub commit_error: bool,
    pub retry: Retry,
}

impl Indicator {
    /// An error shows Retry: a failed save, or the checkpoint alone.
    pub fn is_error(&self) -> bool {
        self.state == Worst::Error || self.commit_error
    }

    /// tC4's label of the indicator.
    pub fn label_key(&self) -> Key {
        if self.commit_error {
            return keys::app::COMMIT_ERROR;
        }
        match self.state {
            Worst::Saved => keys::app::SAVED,
            Worst::Saving => keys::app::SAVING,
            Worst::Dirty => keys::app::UNSAVED,
            Worst::Error => keys::app::SAVE_ERROR,
        }
    }

    /// The status dot's colour, by tC4's map: valid when saved, warn while saving or unsaved,
    /// invalid on an error.
    pub fn status(&self) -> IndicatorStatus {
        match (self.commit_error, self.state) {
            (true, _) | (false, Worst::Error) => IndicatorStatus::Invalid,
            (false, Worst::Saving | Worst::Dirty) => IndicatorStatus::Warn,
            (false, Worst::Saved) => IndicatorStatus::Valid,
        }
    }
}

/// The status dot of X2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorStatus {
    Valid,
    Warn,
    Invalid,
}

/// The worst of the fields and of every open aligner's save (A-4); `failed` are the kinds of
/// the fields whose save failed.
pub fn indicator(
    fields: Worst,
    aligns: &[AlignSave],
    checkpoint_failed: bool,
    failed: &[FieldKind],
) -> Indicator {
    let state = aligns.iter().map(Worst::of_align).fold(fields, Worst::max);
    let commit_error = checkpoint_failed && state != Worst::Error;
    Indicator {
        state,
        commit_error,
        retry: retry_for(failed, commit_error),
    }
}

/// The close guard's value: every field and aligner saved. A failed checkpoint does not count,
/// as tC4's `allSaved` does not (`tC4:src/state.jsx:3716-3720`).
pub fn all_clean(fields: Worst, aligns: &[AlignSave]) -> bool {
    indicator(fields, aligns, false, &[]).state == Worst::Saved
}
