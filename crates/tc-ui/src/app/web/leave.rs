//! INV-ONE-LEAVE-PATH (UI-3): every move away from a screen or a project goes through
//! [`Actions::leave`]: the save gate first, then the checkpoint, then the caller's switch. A
//! source scan (`tests/app/lint.rs`, `only_leave_calls_the_gate`) keeps every other product
//! call of the gate out.

use leptos::prelude::*;
use tc_api::ids::StoryNo;
use tc_api::session::{LeaveMode, ScreenMode};
use tc_core::BookId;
use wasm_bindgen_futures::spawn_local;

use super::app_ctx::Actions;
use super::js;
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

    /// The checkpoint of a leave (D1, tC4 `go`): a mode switch asks the server, which
    /// checkpoints when its policy says so; the switch does not wait for the commit.
    fn checkpoint(&self, to: &LeaveTo) {
        match to {
            LeaveTo::Tab(view) => {
                let from = self.s().view.get_untracked();
                let (Some(from), true) = (screen_mode(from), from != *view) else {
                    return;
                };
                let Some(at) = self
                    .s()
                    .project
                    .with_untracked(|p| p.as_ref().map(|p| p.at.clone()))
                else {
                    return;
                };
                let this = self.clone();
                spawn_local(async move {
                    let request = LeaveMode {
                        request: js::random_request_id(),
                        at,
                        from,
                    };
                    if let Err(error) = this.runtime().api().call(&request).await {
                        web_sys::console::warn_1(&wasm_bindgen::JsValue::from_str(&format!(
                            "tc4: the mode switch's checkpoint was not asked for: {error}"
                        )));
                    }
                });
            }
            // `session/close` runs the leave checkpoint on the server; a second one here would
            // commit twice.
            LeaveTo::Home => {}
            // Within the mode: tC4 checkpoints only when the mode changes.
            LeaveTo::Book(_) | LeaveTo::Chapter(_) | LeaveTo::Story(_) => {}
            // Opening a project closes the one before through `session/close`, which checkpoints.
            LeaveTo::Open => {}
            // A plan commits its own batch; the leave only saves first.
            LeaveTo::Plan => {}
        }
    }
}

/// The mode a view is, as a checkpoint names it; Home is none.
fn screen_mode(view: View) -> Option<ScreenMode> {
    match view {
        View::Home => None,
        View::Read => Some(ScreenMode::Understand),
        View::Draft => Some(ScreenMode::Translate),
        View::Check => Some(ScreenMode::Check),
        View::Publish => Some(ScreenMode::CommunityChecking),
    }
}
