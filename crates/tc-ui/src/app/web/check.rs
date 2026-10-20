//! The Check screen's state and actions (unit B5a): tC4's `s.checkTool`, `s.checkSession` (the
//! rail and its place), `s.fix` and the actions `openCheckTool`, `closeCheckTool`,
//! `setCheckIndex`, `openFix` and `closeModal`. The decisions are in [`crate::app::check`].
//!
//! The rail arrives whole once per session, then by row (AD17): a `Changed` event whose scope
//! names the tool's list asks `check/rows` for the rows changed since the rail's view, and a
//! list derived again (its resource, numbering or scope changed) is opened again.

use std::cell::Cell;

use leptos::prelude::*;
use tc_api::ApiError;
use tc_api::check::{
    CheckListRef, CheckSessionView, CheckUnit, Open, RailRow, RailRows, RailSince, Rows, RowsView,
    Tool,
};
use tc_api::model::rail::{self, Filter, SortMode};
use tc_api::prefs::{PlaceChange, PlaceMode, PlaceUpdate, RecordPlace};
use tc_api::query::QueryKey;
use tc_api::resources::{FixState, FixView};
use wasm_bindgen_futures::spawn_local;

use super::ListenerId;
use super::app_ctx::{Actions, Dialog};
use super::js;
use crate::app::check::CheckView;
use crate::app::client::CallError;
use crate::app::shell::ProjectFlavor;

/// An open tool's rail.
#[derive(Debug, Clone, PartialEq)]
pub struct Rail {
    pub view: CheckSessionView,
    /// The rail's rows in By verse order; the patches of `check/rows` replace rows here.
    pub rows: Vec<RailRow>,
}

/// The open tool's session (tC4 `s.checkSession`).
#[derive(Debug, Clone, PartialEq)]
pub enum Session {
    /// "Deriving the check list…".
    Loading(Tool),
    Open(Box<Rail>),
    /// The open failed: its reason (tC4's `check-error`).
    Failed(Tool, String),
}

/// The guided fix (M11, tC4 `s.fix`).
#[derive(Debug, Clone, PartialEq)]
pub enum Fix {
    Loading(Tool),
    Ready(Tool, FixView),
    Failed(Tool, String),
}

/// The Check screen's view state.
#[derive(Clone)]
pub struct CheckState {
    pub view: ArcRwSignal<CheckView>,
    pub session: ArcRwSignal<Option<Session>>,
    pub filter: ArcRwSignal<Filter>,
    /// The sort chosen; `None` shows the tool's default.
    pub sort: ArcRwSignal<Option<SortMode>>,
    /// The selected check, by its index in the list (tC4 `activeIndex`).
    pub active: ArcRwSignal<Option<u32>>,
    pub fix: ArcRwSignal<Option<Fix>>,
    /// The tool whose card gets the focus back after Back (#42).
    pub return_to: ArcRwSignal<Option<Tool>>,
    /// The tool whose rail took the focus once when it opened (#42, `useFocusOnOpen`).
    pub focused_for: ArcRwSignal<Option<Tool>>,
}

impl CheckState {
    pub fn new() -> Self {
        Self {
            view: ArcRwSignal::new(CheckView::Picker),
            session: ArcRwSignal::new(None),
            filter: ArcRwSignal::new(Filter::All),
            sort: ArcRwSignal::new(None),
            active: ArcRwSignal::new(None),
            fix: ArcRwSignal::new(None),
            return_to: ArcRwSignal::new(None),
            focused_for: ArcRwSignal::new(None),
        }
    }
}

impl Default for CheckState {
    fn default() -> Self {
        Self::new()
    }
}

thread_local! {
    /// Counts the opens asked for: only the newest may land (tC4's `checkSessionSeq`).
    static OPENS: Cell<u64> = const { Cell::new(0) };
    /// The listener that patches the open rail.
    static LISTENER: Cell<Option<ListenerId>> = const { Cell::new(None) };
}

fn next_open() -> u64 {
    OPENS.with(|c| {
        c.set(c.get() + 1);
        c.get()
    })
}

fn current_open() -> u64 {
    OPENS.with(Cell::get)
}

fn rows_of(view: &CheckSessionView) -> Vec<RailRow> {
    match &view.rows {
        RailRows::Full(rows) => rows.clone(),
        RailRows::Since(_) => Vec::new(),
    }
}

impl Actions {
    /// What the open project's checks are of: the open book, or the open story.
    pub(super) fn check_unit(&self) -> Option<CheckUnit> {
        let s = self.s();
        match s.project.get_untracked()?.flavor {
            ProjectFlavor::TextTranslation => Some(CheckUnit::Book(s.u.book.get_untracked()?)),
            ProjectFlavor::TextStories => Some(CheckUnit::Story(crate::app::story::story_of(
                s.u.chapter.get_untracked(),
            )?)),
        }
    }

    fn check_list(&self, tool: Tool) -> Option<(tc_api::ids::Scope, CheckListRef)> {
        let at = self.s().project.get_untracked()?.at;
        Some((
            at,
            CheckListRef {
                tool,
                unit: self.check_unit()?,
            },
        ))
    }

    /// A tool card's CTA or the card itself (tC4 `openCheckTool`): the tool's session opens at
    /// its first check not decided; the place is remembered with the tool.
    pub fn open_check_tool(&self, tool: Tool) {
        let Some((at, list)) = self.check_list(tool) else {
            return;
        };
        let attempt = next_open();
        let c = &self.s().c;
        c.view.try_set(CheckView::Tool(tool));
        c.filter.try_set(Filter::All);
        c.sort.try_set(None);
        c.active.try_set(None);
        c.session.try_set(Some(Session::Loading(tool)));
        self.stop_rail_listener();
        let this = self.clone();
        spawn_local(async move {
            let place = RecordPlace {
                at: at.clone(),
                request: js::random_request_id(),
                place: match list.unit {
                    CheckUnit::Book(book) => PlaceUpdate::Book {
                        book,
                        mode: Some(PlaceMode::Check),
                        chapter: None,
                        verse: PlaceChange::Keep,
                        tool: Some(tool),
                    },
                    CheckUnit::Story(story) => PlaceUpdate::Story {
                        story,
                        mode: Some(PlaceMode::Check),
                        frame: PlaceChange::Keep,
                        tool: Some(tool),
                    },
                },
            };
            let _ = this.runtime().api().call(&place).await;
            let open = Open {
                at,
                list,
                since: None,
            };
            let result = this.runtime().read_once(&open).await;
            if current_open() != attempt {
                return;
            }
            let c = &this.s().c;
            match result {
                Ok(view) => {
                    c.active.try_set(Some(view.start));
                    let rows = rows_of(&view);
                    c.session
                        .try_set(Some(Session::Open(Box::new(Rail { view, rows }))));
                    this.start_rail_listener(list);
                }
                Err(CallError::Dropped) => {}
                // A pin went missing since the tool was last open (a resume, or a teammate's
                // sync): the picker's card says why and offers the fix (tC4 `resumeCheckTool`).
                Err(CallError::Api(ApiError::ToolNotReady)) => this.reset_check(),
                Err(error) => {
                    c.session
                        .try_set(Some(Session::Failed(tool, error.to_string())));
                }
            }
        });
    }

    /// "← All checking tools" (tC4 `closeCheckTool`): the picker, whose card of this tool gets
    /// the focus back.
    pub fn close_check_tool(&self) {
        next_open();
        self.stop_rail_listener();
        let c = &self.s().c;
        if let CheckView::Tool(tool) = c.view.get_untracked() {
            c.return_to.try_set(Some(tool));
        }
        c.view.try_set(CheckView::Picker);
        c.session.try_set(None);
        c.active.try_set(None);
        c.focused_for.try_set(None);
    }

    /// The Check screen of a project or book that is no longer shown: nothing of it stays.
    pub(super) fn reset_check(&self) {
        next_open();
        self.stop_rail_listener();
        let c = &self.s().c;
        c.view.try_set(CheckView::Picker);
        c.session.try_set(None);
        c.active.try_set(None);
        c.fix.try_set(None);
        c.return_to.try_set(None);
        c.focused_for.try_set(None);
    }

    /// A row of the rail, or the arrow keys (tC4 `setCheckIndex`).
    pub fn select_check(&self, index: u32) {
        self.s().c.active.try_set(Some(index));
    }

    pub fn set_check_filter(&self, filter: Filter) {
        self.s().c.filter.try_set(filter);
    }

    pub fn set_check_sort(&self, sort: SortMode) {
        self.s().c.sort.try_set(Some(sort));
    }

    /// Retry on a picker whose read failed (tC4's `preflight-retry`).
    pub fn reload_picker(&self, unit: CheckUnit) {
        if let Some(project) = self.s().project.get_untracked() {
            self.runtime().cache().refresh(&tc_api::check::Picker {
                at: project.at,
                unit,
            });
        }
    }

    /// "Fix this…" (tC4 `openFix`): the guided fix for the tool's missing pin. Opening it
    /// changes nothing.
    pub fn open_fix(&self, tool: Tool) {
        let Some((at, list)) = self.check_list(tool) else {
            return;
        };
        let s = self.s();
        s.c.fix.try_set(Some(Fix::Loading(tool)));
        s.layers.dialog.try_set(Some(Dialog::GuidedFix));
        let this = self.clone();
        spawn_local(async move {
            let result = this
                .runtime()
                .read_once(&FixState {
                    at,
                    tool: list.tool,
                    unit: list.unit,
                })
                .await;
            let fix = &this.s().c.fix;
            if !fix.with_untracked(|f| matches!(f, Some(Fix::Loading(t)) if *t == tool)) {
                return;
            }
            match result {
                Ok(Some(view)) => {
                    fix.try_set(Some(Fix::Ready(tool, view)));
                }
                // Nothing is missing any more: there is nothing to fix.
                Ok(None) | Err(CallError::Dropped) => {
                    fix.try_set(None);
                    this.close_modal();
                }
                Err(error) => {
                    fix.try_set(Some(Fix::Failed(tool, error.to_string())));
                }
            }
        });
    }

    fn stop_rail_listener(&self) {
        if let Some(id) = LISTENER.with(Cell::take) {
            self.runtime().unlisten(id);
        }
    }

    /// Patches the open rail when a change names its list (AD17).
    fn start_rail_listener(&self, list: CheckListRef) {
        let this = self.clone();
        let id = self.runtime().listen(move |frame| {
            use tc_api::event::{Event, SessionEvent};
            let ask = match &frame.event {
                Event::Session {
                    event: SessionEvent::Changed { scope, .. },
                    ..
                } => QueryKey::CheckList(list).depends_on(scope),
                Event::Resync => true,
                _ => false,
            };
            if ask {
                this.refresh_rail(list);
            }
        });
        LISTENER.with(|l| l.set(Some(id)));
    }

    /// The rows changed since the rail's view, patched in; a list derived again opens again.
    fn refresh_rail(&self, list: CheckListRef) {
        let c = self.s().c.clone();
        let Some(since) = c.session.with_untracked(|s| match s {
            Some(Session::Open(rail)) if rail.view.list == list => Some(RailSince {
                session: rail.view.session,
                view: rail.view.view,
            }),
            _ => None,
        }) else {
            return;
        };
        let Some(at) = self.s().project.get_untracked().map(|p| p.at) else {
            return;
        };
        let attempt = current_open();
        let this = self.clone();
        spawn_local(async move {
            let result = this.runtime().read_once(&Rows { at, list, since }).await;
            if current_open() != attempt {
                return;
            }
            match result {
                Ok(RowsView::Changed {
                    view,
                    rows,
                    progress,
                }) => {
                    let mut whole = true;
                    c.session.try_update(|s| {
                        if let Some(Session::Open(rail)) = s
                            && rail.view.session == since.session
                        {
                            whole = rail::patch(&mut rail.rows, &rows);
                            rail.view.view = view;
                            rail.view.progress = progress;
                        }
                    });
                    if !whole {
                        this.open_check_tool(list.tool);
                    }
                }
                Ok(RowsView::Reopen) => this.open_check_tool(list.tool),
                Err(_) => {}
            }
        });
    }
}
