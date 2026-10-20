//! The Check screen's state and actions (units B5a and B5b): tC4's `s.checkTool`,
//! `s.checkSession` (the rail and its place), `s.fix` and the actions `openCheckTool`,
//! `closeCheckTool`, `setCheckIndex`, `openFix` and `closeModal`; the active check's detail, its
//! selection, its marks, its comment and the Academy drawer. The decisions are in
//! [`crate::app::check`] and [`crate::app::check_item`].
//!
//! The rail arrives whole once per session, then by row (AD17): a `Changed` event whose scope
//! names the tool's list asks `check/rows` for the rows changed since the rail's view, and a
//! list derived again (its resource, numbering or scope changed) is opened again.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;

use leptos::prelude::*;
use tc_api::ApiError;
use tc_api::buffer::{BufferKey, DraftCall, FieldKey, FieldKind, FieldPlace, SentSave};
use tc_api::check::{
    CheckArticleRead, CheckItemView, CheckListRef, CheckSessionView, CheckUnit, Decide, DecideEdit,
    Item, Open, RailRow, RailRows, RailSince, Rows, RowsView, Tool,
};
use tc_api::helps::ArticleView;
use tc_api::model::rail::{self, Filter, SortMode};
use tc_api::prefs::{PlaceChange, PlaceMode, PlaceUpdate, RecordPlace};
use tc_api::query::QueryKey;
use tc_api::resources::FixState;
use tc_api::token::ShownToken;
use tc_api::write::Ack;
use wasm_bindgen_futures::spawn_local;

use super::app_ctx::{Actions, Dialog};
use super::js;
use super::saves::{FieldWriter, LocalBoxFuture};
use super::{Api, ListenerId};
use crate::app::check::CheckView;
use crate::app::check_item::{self, DecisionWrites, MarkFailure};
use crate::app::client::{CallError, Transport};
use crate::app::fix::{Fix, FixScreen};
use crate::app::save::{FieldScope, Pair, Source, View};

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

/// The active check's detail (tC4's `cs.items[activeIndex]` with its panes).
#[derive(Debug, Clone, PartialEq)]
pub enum Detail {
    Loading(u32),
    Ready(Box<CheckItemView>),
    Failed(u32, String),
}

/// The Academy drawer's article (tC4's `cs.article` while `academyOpen`).
#[derive(Debug, Clone, PartialEq)]
pub enum Academy {
    Loading,
    Read(ArticleView),
    Failed(String),
}

/// The last mark's outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum Marking {
    Idle,
    Failed(MarkFailure),
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
    /// The active check's detail.
    pub detail: ArcRwSignal<Option<Detail>>,
    /// The words tapped in the draft, one local value until a mark saves it (#757).
    pub selection: ArcRwSignal<BTreeSet<u32>>,
    /// The Academy drawer, closed or on the active check's article.
    pub academy: ArcRwSignal<Option<Academy>>,
    pub marking: ArcRwSignal<Marking>,
    /// The comment editor of the active check is open.
    pub comment_open: ArcRwSignal<bool>,
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
            detail: ArcRwSignal::new(None),
            selection: ArcRwSignal::new(BTreeSet::new()),
            academy: ArcRwSignal::new(None),
            marking: ArcRwSignal::new(Marking::Idle),
            comment_open: ArcRwSignal::new(false),
        }
    }
}

impl Default for CheckState {
    fn default() -> Self {
        Self::new()
    }
}

thread_local! {
    /// Counts the reads of the active check's detail: only the newest may land.
    static ITEMS: Cell<u64> = const { Cell::new(0) };
    /// Counts the Academy article reads asked for.
    static ARTICLES: Cell<u64> = const { Cell::new(0) };
    /// Every write of a decision, a mark or a comment, goes through here.
    static WRITES: DecisionWrites = DecisionWrites::default();
    /// The detail a write's Ack tells, when the Ack is for the check it shows.
    static DETAIL: RefCell<Option<ArcRwSignal<Option<Detail>>>> = const { RefCell::new(None) };
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

fn bump(counter: &'static std::thread::LocalKey<Cell<u64>>) -> u64 {
    counter.with(|c| {
        c.set(c.get() + 1);
        c.get()
    })
}

fn current(counter: &'static std::thread::LocalKey<Cell<u64>>) -> u64 {
    counter.with(Cell::get)
}

/// Writes a decision: a mark or a comment of one check, after the write before it, over the
/// newest token the page knows of the check. Its Ack's token and comment as stored are taken at
/// once, before any read, by the next write and by the detail when it shows that check.
async fn write_decision(api: &Api, mut request: Decide) -> Result<Ack, CallError> {
    let writes = WRITES.with(DecisionWrites::clone);
    let _turn = writes.turn().await;
    request.decision = writes.token(&request.place, &request.check_id, &request.decision);
    let ack = api.call(&request).await?;
    if let ([update], [saved]) = (ack.tokens.as_slice(), ack.saved.as_slice()) {
        writes.learn(&request.place, &request.check_id, &update.token);
        if let Some(detail) = DETAIL.with(|d| d.borrow().clone()) {
            detail.maybe_update(|d| match d {
                Some(Detail::Ready(view)) => check_item::take_ack(
                    view,
                    &request.place,
                    &request.check_id,
                    &update.token,
                    &saved.text,
                ),
                _ => false,
            });
        }
    }
    Ok(ack)
}

/// How a check comment is saved: a `check/decide` with the comment, over the newest decision
/// token the page knows, else the one the field holds. Nothing here reads the item: the place
/// the buffer entry names is the check.
/// C4 adds the comment's draft on the server.
struct CommentWriter;

impl FieldWriter for CommentWriter {
    fn save(
        &self,
        api: &Api,
        key: &BufferKey,
        sent: &SentSave,
        base: &[ShownToken],
    ) -> LocalBoxFuture<Result<Ack, CallError>> {
        let api = api.clone();
        let request = check_item::parse_comment_place(key.field.place.as_str())
            .zip(sent.scope.clone())
            .zip(base.first().cloned())
            .map(|(((list, place, check_id), at), decision)| Decide {
                at,
                request: sent.request,
                list,
                place,
                check_id,
                decision,
                verse: None,
                edit: DecideEdit::Comment {
                    text: sent.text.clone(),
                },
            });
        Box::pin(async move {
            let Some(request) = request else {
                return Err(CallError::Transport(Transport::Unreadable(
                    "a check comment with no place, open or token".to_owned(),
                )));
            };
            write_decision(&api, request).await
        })
    }

    fn draft(
        &self,
        _: &Api,
        _: &BufferKey,
        _: &str,
        _: &[ShownToken],
    ) -> Option<LocalBoxFuture<Result<(), CallError>>> {
        None
    }

    fn draft_call(&self, _: &BufferKey, _: &[ShownToken]) -> Option<DraftCall> {
        None
    }
}

/// The writers of the Check screen's fields, registered before the buffer's entries come back
/// as fields (AD22).
pub(super) fn register_writers(saves: &super::saves::Saves) {
    saves.register(FieldKind::CheckComment, Rc::new(CommentWriter));
}

/// The buffer key of the check comment of `view`'s check, in the open `at`.
pub(crate) fn comment_key(
    tool: Tool,
    project: &crate::app::shell::CurrentProject,
    view: &CheckItemView,
) -> Option<BufferKey> {
    Some(BufferKey {
        project: project.at.project.clone(),
        id: project.id.clone(),
        field: FieldKey {
            kind: FieldKind::CheckComment,
            place: FieldPlace::parse(&check_item::comment_place(
                tool,
                &view.place,
                &view.check_id,
            ))
            .ok()?,
        },
    })
}

fn rows_of(view: &CheckSessionView) -> Vec<RailRow> {
    match &view.rows {
        RailRows::Full(rows) => rows.clone(),
        RailRows::Since(_) => Vec::new(),
    }
}

impl Actions {
    fn check_list(&self, tool: Tool) -> Option<(tc_api::ids::Scope, CheckListRef)> {
        let s = self.s();
        let at = s.project.get_untracked()?.at;
        let book = s.u.book.get_untracked()?;
        Some((
            at,
            CheckListRef {
                tool,
                unit: CheckUnit::Book(book),
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
        self.forget_detail();
        c.session.try_set(Some(Session::Loading(tool)));
        self.stop_rail_listener();
        DETAIL.with(|d| *d.borrow_mut() = Some(c.detail.clone()));
        let this = self.clone();
        spawn_local(async move {
            let place = RecordPlace {
                at: at.clone(),
                request: js::random_request_id(),
                place: PlaceUpdate::Book {
                    book: match list.unit {
                        CheckUnit::Book(book) => book,
                        CheckUnit::Stories => return,
                    },
                    mode: Some(PlaceMode::Check),
                    chapter: None,
                    verse: PlaceChange::Keep,
                    tool: Some(tool),
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
                    let (start, empty) = (view.start, view.empty.is_some());
                    c.active.try_set(Some(start));
                    let rows = rows_of(&view);
                    c.session
                        .try_set(Some(Session::Open(Box::new(Rail { view, rows }))));
                    this.start_rail_listener(list);
                    if !empty {
                        this.load_item(start);
                    }
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
        self.forget_detail();
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
        self.forget_detail();
        let c = &self.s().c;
        c.view.try_set(CheckView::Picker);
        c.session.try_set(None);
        c.active.try_set(None);
        c.fix.try_set(None);
        c.return_to.try_set(None);
        c.focused_for.try_set(None);
    }

    /// A row of the rail, the arrow keys, or Previous and Next (tC4 `setCheckIndex`): the
    /// comment of the check left is saved if its editor is open (A-10), and the new check's
    /// detail is read.
    pub fn select_check(&self, index: u32) {
        let c = &self.s().c;
        if c.active.get_untracked() == Some(index) {
            return;
        }
        self.leave_item();
        c.active.try_set(Some(index));
        self.load_item(index);
    }

    pub fn set_check_filter(&self, filter: Filter) {
        self.s().c.filter.try_set(filter);
    }

    pub fn set_check_sort(&self, sort: SortMode) {
        self.s().c.sort.try_set(Some(sort));
    }

    /// Retry on a picker whose read failed (tC4's `preflight-retry`).
    pub fn reload_picker(&self, book: tc_core::BookId) {
        if let Some(project) = self.s().project.get_untracked() {
            self.runtime().cache().refresh(&tc_api::check::Picker {
                at: project.at,
                book,
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
                    let screen = FixScreen::new(super::fix::next_id(), tool, view);
                    fix.try_set(Some(Fix::Ready(Box::new(screen))));
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
                    } else {
                        this.refresh_item();
                    }
                }
                Ok(RowsView::Reopen) => this.open_check_tool(list.tool),
                Err(_) => {}
            }
        });
    }
}

// ---------------------------------------------------------------------------------------
// The active check's detail (unit B5b)
// ---------------------------------------------------------------------------------------

impl Actions {
    /// The tool and the open the detail reads from, while a tool is shown.
    fn detail_target(&self) -> Option<(tc_api::ids::Scope, CheckListRef)> {
        let CheckView::Tool(tool) = self.s().c.view.get_untracked() else {
            return None;
        };
        self.check_list(tool)
    }

    /// The detail and everything that belongs to it goes: another tool, the picker, or no
    /// project. A comment still being typed is saved by its editor leaving.
    fn forget_detail(&self) {
        let c = &self.s().c;
        bump(&ITEMS);
        bump(&ARTICLES);
        c.comment_open.try_set(false);
        c.detail.try_set(None);
        c.selection.try_set(BTreeSet::new());
        c.academy.try_set(None);
        c.marking.try_set(Marking::Idle);
    }

    /// Before another check shows: its editor closes (which saves the comment typed in it,
    /// A-10), the drawer and the last mark's failure go, and the selection is the new check's.
    fn leave_item(&self) {
        let c = &self.s().c;
        c.comment_open.try_set(false);
        c.academy.try_set(None);
        c.marking.try_set(Marking::Idle);
    }

    /// Reads check `index` for the detail (tC4 shows it from the list it holds; the server
    /// answers it here). The newest read is the one that lands.
    pub(crate) fn load_item(&self, index: u32) {
        let c = &self.s().c;
        let Some((at, list)) = self.detail_target() else {
            return;
        };
        let attempt = bump(&ITEMS);
        c.detail.try_set(Some(Detail::Loading(index)));
        self.read_item(at, list, index, attempt);
    }

    /// Reads the active check again (a change reached it, or a mark was saved): the detail
    /// stays on screen, and only a read that differs changes it.
    pub(crate) fn refresh_item(&self) {
        let c = &self.s().c;
        let (Some((at, list)), Some(index)) = (self.detail_target(), c.active.get_untracked())
        else {
            return;
        };
        if matches!(c.detail.get_untracked(), Some(Detail::Loading(_)) | None) {
            return;
        }
        let attempt = bump(&ITEMS);
        self.read_item(at, list, index, attempt);
    }

    fn read_item(&self, at: tc_api::ids::Scope, list: CheckListRef, index: u32, attempt: u64) {
        let this = self.clone();
        spawn_local(async move {
            let result = this.runtime().read_once(&Item { at, list, index }).await;
            if current(&ITEMS) != attempt {
                return;
            }
            match result {
                Ok(view) => this.apply_item(view),
                Err(CallError::Dropped) => {}
                Err(CallError::Api(ApiError::ToolNotReady)) => this.reset_check(),
                Err(error) => {
                    this.s()
                        .c
                        .detail
                        .try_set(Some(Detail::Failed(index, error.to_string())));
                }
            }
        });
    }

    /// A read of the active check lands: the selection follows the stored one only when the
    /// check or its words changed (tC4's effect on `activeIndex` and `targetText`), and the
    /// comment's field takes the decision the read shows (AD37).
    fn apply_item(&self, view: CheckItemView) {
        let c = &self.s().c;
        WRITES.with(|w| w.learn(&view.place, &view.check_id, &view.decision.token));
        let local = c.selection.get_untracked();
        let selection = c.detail.with_untracked(|shown| {
            let shown = match shown {
                Some(Detail::Ready(shown)) => Some(&**shown),
                _ => None,
            };
            check_item::selection_after_read(shown, &view, &local)
        });
        if selection != local {
            c.selection.try_set(selection);
        }
        let same = c
            .detail
            .with_untracked(|d| matches!(d, Some(Detail::Ready(shown)) if **shown == view));
        if !same {
            self.refresh_comment(&view);
            c.detail.try_set(Some(Detail::Ready(Box::new(view))));
        }
    }

    /// The comment's field, if its editor is open, learns what the read shows of the decision.
    fn refresh_comment(&self, view: &CheckItemView) {
        let (Some(project), CheckView::Tool(tool)) = (
            self.s().project.get_untracked(),
            self.s().c.view.get_untracked(),
        ) else {
            return;
        };
        let Some(key) = comment_key(tool, &project, view) else {
            return;
        };
        let pair = Pair {
            value: view.decision.value.comment.clone().unwrap_or_default(),
            token: view.decision.token.clone(),
            view: Some(View::of(Some(&project.at), view.view)),
        };
        self.runtime().saves().refresh(&key, pair, None);
    }

    /// One act on the shown check, saved at once (AD20). The writes of decisions go one at a time
    /// (`write_decision`); a reply that comes after the person moved to another check is not for
    /// the check now shown, so it changes the rail's rows and nothing on screen.
    pub fn mark(&self, edit: DecideEdit) {
        let c = self.s().c.clone();
        let (Some((at, list)), Some(Detail::Ready(view))) =
            (self.detail_target(), c.detail.get_untracked())
        else {
            return;
        };
        let (place, check_id) = (view.place.clone(), view.check_id.clone());
        let request = Decide {
            at,
            request: js::random_request_id(),
            list,
            place: place.clone(),
            check_id: check_id.clone(),
            decision: view.decision.token.clone(),
            verse: view.target.verse.clone(),
            edit,
        };
        c.marking.try_set(Marking::Idle);
        let this = self.clone();
        spawn_local(async move {
            // A write's reply is never dropped: its Ack carries the next token (AD2).
            let result = write_decision(this.runtime().api(), request).await;
            if let Ok(ack) = &result {
                this.patch_rail(ack);
            }
            let c = &this.s().c;
            let shown = c.detail.with_untracked(|d| {
                matches!(d, Some(Detail::Ready(v)) if v.place == place && v.check_id == check_id)
            });
            if !shown {
                return;
            }
            match result {
                Ok(_) => {
                    c.marking.try_set(Marking::Idle);
                    this.refresh_item();
                }
                Err(error) => {
                    let failure = MarkFailure::of(&error);
                    let look_again = failure == MarkFailure::LookAgain;
                    c.marking.try_set(Marking::Failed(failure));
                    if look_again {
                        this.refresh_item();
                    }
                }
            }
        });
    }

    /// The rows a write's `Ack` carries go into the rail at once (AD17).
    fn patch_rail(&self, ack: &Ack) {
        if ack.rows.is_empty() {
            return;
        }
        self.s().c.session.try_update(|s| {
            if let Some(Session::Open(rail)) = s {
                rail::patch(&mut rail.rows, &ack.rows);
                rail.view.progress = tc_api::check::RailProgress {
                    decided: rail.rows.iter().filter(|r| r.flags.decided).count() as u32,
                    total: rail.rows.len() as u32,
                };
            }
        });
    }

    /// "Learn more" (tC4's `setAcademyOpen(true)`): the drawer opens on the check's article.
    pub fn open_academy(&self) {
        let c = &self.s().c;
        let (Some((at, list)), Some(index)) = (self.detail_target(), c.active.get_untracked())
        else {
            return;
        };
        let attempt = bump(&ARTICLES);
        c.academy.try_set(Some(Academy::Loading));
        let this = self.clone();
        spawn_local(async move {
            let result = this
                .runtime()
                .read_once(&CheckArticleRead { at, list, index })
                .await;
            if current(&ARTICLES) != attempt {
                return;
            }
            let state = match result {
                Ok(view) => Academy::Read(view),
                Err(CallError::Dropped) => return,
                Err(error) => Academy::Failed(error.to_string()),
            };
            this.s().c.academy.try_set(Some(state));
        });
    }

    pub fn close_academy(&self) {
        bump(&ARTICLES);
        self.s().c.academy.try_set(None);
    }

    /// The comment's button: the editor opens or closes. Closing it with text typed saves the
    /// text (A-10); opening it takes the stored comment.
    pub fn toggle_comment(&self) {
        let open = &self.s().c.comment_open;
        open.try_set(!open.get_untracked());
    }

    /// The comment editor's place for the active check, while a project and a detail are shown.
    pub(crate) fn comment_field(&self) -> Option<(BufferKey, Pair, FieldScope)> {
        let project = self.s().project.get_untracked()?;
        let CheckView::Tool(tool) = self.s().c.view.get_untracked() else {
            return None;
        };
        let Some(Detail::Ready(view)) = self.s().c.detail.get_untracked() else {
            return None;
        };
        let key = comment_key(tool, &project, &view)?;
        let pair = Pair {
            value: view.decision.value.comment.clone().unwrap_or_default(),
            token: view.decision.token.clone(),
            view: Some(View::of(Some(&project.at), view.view)),
        };
        Some((key, pair, FieldScope::Project(Some(project.at))))
    }

    /// The text the person typed went into the comment's field.
    pub(crate) fn comment_typed(&self, key: &BufferKey, text: &str) {
        self.runtime().saves().typed(key, text, Source::Input);
    }
}
