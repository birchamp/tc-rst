//! INV-ONE-LEAVE-PATH (UI-3): every move away from a screen or a project goes through
//! [`Actions::leave`]: the save gate first, then the checkpoint, then the caller's switch. A
//! source scan (`tests/app/lint.rs`, `only_leave_calls_the_gate`) keeps every other product
//! call of the gate out.

use tc_api::ids::StoryNo;
use tc_core::BookId;

use super::app_ctx::Actions;
use crate::app::shell::View;

/// Where a person is going when they leave what they are on. D1's checkpoint reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LeaveTo {
    /// A mode tab of the open project.
    Tab(View),
    /// Home, closing the project.
    Home,
    /// Another project, or this one again.
    Open,
    /// A book of the rail.
    Book(BookId),
    /// A chapter of the open book.
    Chapter(u16),
    /// A story of the story rail.
    Story(StoryNo),
    /// An upgrade, a gateway change or a re-pin: a plan reads the saved project.
    Plan,
}

impl Actions {
    /// Drains every field, then takes the checkpoint of `to`; answers whether the caller may
    /// switch. A standing failure keeps the person where they are (FR-32).
    pub(crate) async fn leave(&self, to: LeaveTo) -> bool {
        if !self.runtime().saves().gate().await {
            return false;
        }
        self.checkpoint(&to);
        true
    }

    /// The checkpoint of a leave (tC4's "leaving the project" and mode-switch checkpoints).
    /// D1 fills this in.
    fn checkpoint(&self, _to: &LeaveTo) {}
}
