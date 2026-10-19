//! The save indicator (X2) and the close guard's value (APP-DESIGN 6.5).

use tc_api::align::AlignSave;

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

/// What X2 shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Indicator {
    pub state: Worst,
    /// "Checkpoint not saved", shown only when no save failed (`tC4:src/App.jsx:63-67`).
    pub commit_error: bool,
}

/// The worst of the fields and of every open aligner's save (A-4).
pub fn indicator(fields: Worst, aligns: &[AlignSave], checkpoint_failed: bool) -> Indicator {
    let state = aligns.iter().map(Worst::of_align).fold(fields, Worst::max);
    Indicator {
        state,
        commit_error: checkpoint_failed && state != Worst::Error,
    }
}

/// The close guard's value: every field and aligner saved. A failed checkpoint does not count,
/// as tC4's `allSaved` does not (`tC4:src/state.jsx:3716-3720`).
pub fn all_clean(fields: Worst, aligns: &[AlignSave]) -> bool {
    indicator(fields, aligns, false).state == Worst::Saved
}
