//! tC4's timing per field (APP-DESIGN 6.5 "Timing", AD20): when a field saves, and when its
//! draft goes to the server between saves.

use tc_api::buffer::FieldKind;

/// When a field saves by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveRule {
    /// On blur, or `idle_ms` after the last input: a verse, layout text, an OBS frame, title or
    /// reference, and the test fields (`tC4:src/data/saveScheduler.ts`, 2000 ms).
    BlurOrIdle { idle_ms: u32 },
    /// On blur only, never on a pause: a user comment, which STORAGE keeps append-only, so a
    /// save per pause would leave a comment per pause (A-18).
    OnBlur,
    /// On Done, and on leaving the item while its editor is open: a check comment (A-10).
    OnLeave,
    /// Only by the Save button: the section editor, whose save is a passage batch that can join,
    /// split or renumber verses (A-1). Nothing else saves it, the navigation gate included.
    OnSaveOnly,
}

/// When a dirty field's draft goes to the server (`Draft`, STORAGE §5.14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftRule {
    /// At most once per `gap_ms` while the field is dirty, and only when its text changed since
    /// the last draft (STORAGE:1752; R22, A-20).
    Throttled { gap_ms: u32 },
    /// On blur, and `idle_ms` after the last input: the section editor (A-1).
    OnBlurAndIdle { idle_ms: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldRules {
    pub save: SaveRule,
    pub draft: DraftRule,
}

/// The idle time of tC4's scheduler (`saveScheduler.ts`, `debounceMs`).
pub const IDLE_MS: u32 = 2000;
/// At most one draft a second (STORAGE:1752).
pub const DRAFT_GAP_MS: u32 = 1000;

/// The rules of a kind of field: one match with no wildcard, so a kind tc-api adds does not
/// compile until it has its timing.
pub fn rules(kind: FieldKind) -> FieldRules {
    let throttled = DraftRule::Throttled {
        gap_ms: DRAFT_GAP_MS,
    };
    match kind {
        FieldKind::Verse
        | FieldKind::Layout
        | FieldKind::Frame
        | FieldKind::StoryTitle
        | FieldKind::StoryRef
        | FieldKind::Test => FieldRules {
            save: SaveRule::BlurOrIdle { idle_ms: IDLE_MS },
            draft: throttled,
        },
        FieldKind::Section => FieldRules {
            save: SaveRule::OnSaveOnly,
            draft: DraftRule::OnBlurAndIdle { idle_ms: IDLE_MS },
        },
        FieldKind::Comment => FieldRules {
            save: SaveRule::OnBlur,
            draft: throttled,
        },
        FieldKind::CheckComment => FieldRules {
            save: SaveRule::OnLeave,
            draft: throttled,
        },
    }
}
