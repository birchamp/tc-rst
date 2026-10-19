//! The port of tC4's `Check.jsx` (units B5a and B5b): the tool picker of the open book, and a
//! tool's session, its rail beside the item detail.
//!
//! The picker holds one read, `check/picker`: each tool's preflight with its progress, and the
//! Align card's count. A ready card opens its tool; a card whose pinned resource is missing
//! opens the guided fix (M11). Nothing here writes the project (tC4 #501).

mod picker;
mod rail;

use leptos::prelude::*;

use crate::app::check::CheckView;
use crate::app::shell::ProjectFlavor;
use crate::app::web::{AppCtx, use_app};

pub use rail::check_session;

/// A tool's name (`check.tool.*`).
pub fn tool_name_key(tool: tc_api::check::Tool) -> crate::i18n::Key {
    picker::tool_name(tool)
}

/// tC4's `Check`: the session of the open tool, else the picker. A story project's Check is
/// B7's.
#[component]
pub fn Check() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let shown = {
        let (project, book, view) = (s.project.clone(), s.u.book.clone(), s.c.view.clone());
        Memo::new(move |_| {
            let bible = project.with(|p| {
                p.as_ref()
                    .is_some_and(|p| p.flavor == ProjectFlavor::TextTranslation)
            });
            let book = book.get().filter(|_| bible)?;
            let at = project.with(|p| p.as_ref().map(|p| p.at.clone()))?;
            Some((at, book, view.get()))
        })
    };
    (move || match shown.get() {
        None => ().into_any(),
        Some((at, book, CheckView::Picker)) => picker::picker(&s, &actions, at, book).into_any(),
        Some((_, book, CheckView::Tool(tool))) => {
            check_session(&s, &actions, book, tool).into_any()
        }
    })
    .into_any()
}
