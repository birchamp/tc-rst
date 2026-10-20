//! The port of tC4's `Check.jsx` (units B5a and B5b): the tool picker of the open book, and a
//! tool's session, its rail beside the item detail.
//!
//! The picker holds one read, `check/picker`: each tool's preflight with its progress, and the
//! Align card's count. A ready card opens its tool; a card whose pinned resource is missing
//! opens the guided fix (M11). Nothing here writes the project (tC4 #501).

mod picker;
mod rail;
mod story;

use leptos::prelude::*;

use tc_api::check::CheckUnit;

use crate::app::check::CheckView;
use crate::app::shell::ProjectFlavor;
use crate::app::web::{AppCtx, use_app};

pub use rail::check_session;

/// A tool's name (`check.tool.*`).
pub fn tool_name_key(tool: tc_api::check::Tool) -> crate::i18n::Key {
    picker::tool_name(tool)
}

/// tC4's `Check`: the session of the open tool, else the picker, of the open book or the open
/// story.
#[component]
pub fn Check() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let shown = {
        let (project, book, chapter, view) = (
            s.project.clone(),
            s.u.book.clone(),
            s.u.chapter.clone(),
            s.c.view.clone(),
        );
        Memo::new(move |_| {
            let (at, flavor) = project.with(|p| p.as_ref().map(|p| (p.at.clone(), p.flavor)))?;
            let unit = match flavor {
                ProjectFlavor::TextTranslation => CheckUnit::Book(book.get()?),
                ProjectFlavor::TextStories => {
                    CheckUnit::Story(crate::app::story::story_of(chapter.get())?)
                }
            };
            Some((at, unit, view.get()))
        })
    };
    (move || match shown.get() {
        None => ().into_any(),
        Some((at, unit, CheckView::Picker)) => picker::picker(&s, &actions, at, unit).into_any(),
        Some((at, unit, CheckView::Tool(tool))) => {
            check_session(&s, &actions, at, unit, tool).into_any()
        }
    })
    .into_any()
}
