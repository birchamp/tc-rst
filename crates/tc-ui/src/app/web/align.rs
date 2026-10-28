//! The Align screen's state and actions (unit B6): tC4's `s.aligning`, `s.alignSession`,
//! `s.alignVerse`, `s.alignIndex` and the actions `startAligning`, `closeAlign`,
//! `setAlignVerse`, `applyAlignEdit` and their kin. The decisions are in [`crate::app::align`].
//!
//! The aligner is on the server (AD18). Every reply is the whole view, and the view's `save`
//! is what the indicator and the close guard count, so each reply, and the read that a
//! `Resync` makes, brings the save state again and a lost `AlignSave` event cannot leave
//! "Saving" standing (5.4). The page asks the server to save before it leaves the verse, the
//! tool or the project: the navigation gate calls `align/flush` (6.5).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::Arc;

use js_sys::{Function, Promise};
use leptos::prelude::*;
use tc_api::ApiError;
use tc_api::align::{
    AlignEdit, AlignPlace, AlignSave, AlignerView, Blocked, Close, Edit, Flush, Index, IndexItem,
    KeepReflow, Open, OpenVerse, Opened, RailState, RailStatus,
};
use tc_api::app::Hello;
use tc_api::prefs::SetAlignSuggestions;
use tc_core::BookId;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::{JsFuture, spawn_local};

use super::ListenerId;
use super::app_ctx::Actions;
use super::js;
use super::saves::{GateHookId, WriteCap};
use crate::app::align::{
    Closed, EditFailure, EditQueue, LeaveWait, Lens, RailFilter, SentEdit, closed, refresh_for,
    same_verse,
};
use crate::app::check::CheckView;
use crate::app::client::CallError;
use crate::app::latest::Latest;

/// The aligner of the screen (tC4 `s.alignSession`).
#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    /// "Opening the alignment…".
    Loading,
    Ready(Box<AlignerView>),
    /// There is nothing to align here; the screen says why (tC4's `unavailable`).
    Blocked(Blocked),
    /// The open failed: its reason, with a Retry (tC4's `error`).
    Failed(String),
}

/// The rail's verses (tC4 `s.alignIndex`): their texts and places once, and the state of each
/// verse as a signal of its own, so an edit changes the one row it belongs to and not the
/// list, and only when the verse's state changes: placing a word on a verse that is still to
/// do changes nothing on the rail (a Psalms rail has 2,461 rows).
#[derive(Clone)]
pub struct RailRows {
    pub items: Arc<Vec<IndexItem>>,
    /// Each verse's state, for its own row.
    pub state: Arc<Vec<ArcRwSignal<RailState>>>,
    /// Every verse's state together, for the bar, the chips' counts and the filter.
    pub states: ArcRwSignal<Vec<RailState>>,
    at: Arc<HashMap<(u16, u16), usize>>,
}

impl PartialEq for RailRows {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.items, &other.items)
    }
}

impl RailRows {
    pub fn new(items: Vec<IndexItem>) -> Self {
        let states: Vec<RailState> = items.iter().map(|item| item.status.state).collect();
        let state = states.iter().map(|s| ArcRwSignal::new(*s)).collect();
        let at = items
            .iter()
            .enumerate()
            .map(|(i, item)| ((item.place.chapter, item.place.verse), i))
            .collect();
        Self {
            items: Arc::new(items),
            state: Arc::new(state),
            states: ArcRwSignal::new(states),
            at: Arc::new(at),
        }
    }

    /// The open verse's row follows each edit without asking for the rail again; only a change
    /// of its state is announced.
    pub fn patch(&self, place: AlignPlace, status: RailStatus) {
        let Some(&i) = self.at.get(&(place.chapter, place.verse)) else {
            return;
        };
        let Some(row) = self.state.get(i) else {
            return;
        };
        if row.get_untracked() == status.state {
            return;
        }
        row.set(status.state);
        self.states.update(|states| {
            if let Some(slot) = states.get_mut(i) {
                *slot = status.state;
            }
        });
    }
}

pub enum Rail {
    Loading,
    Ready(RailRows),
    Failed(String),
}

/// The Align screen's view state.
#[derive(Clone)]
pub struct AlignState {
    pub screen: ArcRwSignal<Screen>,
    pub rail: ArcRwSignal<Rail>,
    /// The verse chosen on the rail, or the one that opened (tC4 `alignVerse`).
    pub verse: ArcRwSignal<Option<AlignPlace>>,
    pub lens: ArcRwSignal<Lens>,
    pub filter: ArcRwSignal<RailFilter>,
    /// "How alignment works" is open.
    pub how: ArcRwSignal<bool>,
    /// A Suggest is on its way: the button waits for its answer.
    pub asking: ArcRwSignal<bool>,
    /// A queued edit failed; the workspace stays mounted with its pending actions retained.
    pub edit_error: ArcRwSignal<Option<String>>,
    /// The suggestions switch was not set; it shows its saved state again until the next view.
    pub suggest_error: ArcRwSignal<Option<String>>,
    /// The aligner the server holds for this page, the open it belongs to and its verse: what
    /// the tool asks the server to close when it is left for another book, and the verse shown
    /// again when the server refuses to leave it.
    pub open: ArcRwSignal<Option<(tc_api::ids::Scope, tc_api::ids::AlignerId, AlignPlace)>>,
    /// The verse each book was last at, kept in memory only (tC4 `toolPos`, §4.2).
    pub remembered: ArcRwSignal<Vec<(BookId, AlignPlace)>>,
    /// The open verse's History is open (owner ruling H5).
    pub history_open: ArcRwSignal<bool>,
    /// A *Restore this version* of the open verse's alignment is on its way.
    pub restoring: ArcRwSignal<bool>,
    /// The last *Restore this version* of the open verse was not saved.
    pub restore_failed: ArcRwSignal<bool>,
    /// Home asks whether to leave without the clicks that cannot be saved.
    pub leave_ask: ArcRwSignal<bool>,
}

impl AlignState {
    pub fn new() -> Self {
        Self {
            screen: ArcRwSignal::new(Screen::Loading),
            rail: ArcRwSignal::new(Rail::Loading),
            verse: ArcRwSignal::new(None),
            lens: ArcRwSignal::new(Lens::Gateway),
            filter: ArcRwSignal::new(RailFilter::All),
            how: ArcRwSignal::new(false),
            asking: ArcRwSignal::new(false),
            edit_error: ArcRwSignal::new(None),
            suggest_error: ArcRwSignal::new(None),
            open: ArcRwSignal::new(None),
            remembered: ArcRwSignal::new(Vec::new()),
            history_open: ArcRwSignal::new(false),
            restoring: ArcRwSignal::new(false),
            restore_failed: ArcRwSignal::new(false),
            leave_ask: ArcRwSignal::new(false),
        }
    }
}

impl Default for AlignState {
    fn default() -> Self {
        Self::new()
    }
}

thread_local! {
    /// Counts the opens asked for: only the newest may land (tC4's `alignSessionSeq`).
    static OPENS: Latest = const { Latest::new() };
    /// Counts the reads of the rail (tC4's `alignIndexSeq`).
    static READS: Latest = const { Latest::new() };
    /// The edits the person made, one on their way at a time (A-14).
    static QUEUE: RefCell<EditQueue> = RefCell::new(EditQueue::default());
    /// Refreshes and navigation wait for replies instead of abandoning clicks.
    static EDIT_WAITERS: RefCell<Vec<Function>> = const { RefCell::new(Vec::new()) };
    /// Only the newest status refresh may replace the current view.
    static REFRESHES: Latest = const { Latest::new() };
    /// A newer navigation wins while earlier requests wait for page edits to drain.
    static NAVIGATIONS: Latest = const { Latest::new() };
    /// Only the newest press of Home waits and asks; an older one gives way.
    static SETTLES: Latest = const { Latest::new() };
    /// The answer Home waits for while it asks Stay or Leave without it.
    static LEAVE_ANSWER: RefCell<Option<Function>> = const { RefCell::new(None) };
    /// The gate hook that asks the server to save the links, while the tool is open.
    static HOOK: Cell<Option<GateHookId>> = const { Cell::new(None) };
    /// The listener that keeps the rail and the view true to the server's events.
    static LISTENER: Cell<Option<ListenerId>> = const { Cell::new(None) };
}

/// Pending page commands count as unsaved even before the server has seen their links.
pub(super) fn edits_pending() -> bool {
    QUEUE.with(|q| !q.borrow().is_idle())
}

pub(super) fn edit_failure() -> Option<String> {
    QUEUE.with(|q| q.borrow().failure().map(|f| f.message().to_owned()))
}

fn open_verse(place: AlignPlace) -> OpenVerse {
    OpenVerse {
        chapter: place.chapter,
        verse: place.verse,
    }
}

impl Actions {
    fn align_scope(&self) -> Option<(tc_api::ids::Scope, BookId)> {
        let s = self.s();
        Some((s.project.get_untracked()?.at, s.u.book.get_untracked()?))
    }

    /// The Align card (tC4 `startAligning`): the tool opens at the verse the book was last at,
    /// else the card's next verse, else where the server resumes (the first verse not done).
    pub fn start_aligning(&self, next: Option<AlignPlace>) {
        let Some((_, book)) = self.align_scope() else {
            return;
        };
        let s = self.s();
        OPENS.with(Latest::begin);
        s.c.view.try_set(CheckView::Align);
        s.c.return_to.try_set(None);
        let a = &s.a;
        a.screen.try_set(Screen::Loading);
        a.rail.try_set(Rail::Loading);
        a.filter.try_set(RailFilter::All);
        a.how.try_set(false);
        let remembered = a
            .remembered
            .with_untracked(|r| r.iter().find(|(b, _)| *b == book).map(|(_, p)| *p))
            .or(next);
        a.verse.try_set(remembered);
        self.start_align_hook();
        self.start_align_listener();
        self.read_align_rail();
        self.open_align(remembered);
    }

    /// "← All checking tools" (tC4 `closeAlign`): the links are saved first; a save that fails
    /// keeps the person in the tool, with the indicator saying so.
    pub fn close_align(&self) {
        let navigation = NAVIGATIONS.with(Latest::begin);
        let this = self.clone();
        spawn_local(async move {
            if !this.drain_align_edits().await || !NAVIGATIONS.with(|l| l.is_current(navigation)) {
                return;
            }
            let Some((at, _)) = this.align_scope() else {
                this.reset_align();
                return;
            };
            let view = this.s().a.screen.with_untracked(|s| match s {
                Screen::Ready(view) => Some(view.clone()),
                _ => None,
            });
            if let Some(mut view) = view {
                // Once every click has arrived, stop offering controls while Close saves the
                // verse. An edit sent after Close could otherwise name an aligner that is gone.
                OPENS.with(Latest::begin);
                this.s().a.screen.try_set(Screen::Loading);
                let aligner = view.aligner;
                let close = Close {
                    at,
                    request: js::random_request_id(),
                    aligner,
                };
                match closed(this.runtime().api().call(&close).await) {
                    Closed::Saved => this.runtime().align_forget(aligner),
                    Closed::Kept(unsaved) => {
                        view.save = unsaved;
                        this.align_view(*view);
                        return;
                    }
                    Closed::Unanswered | Closed::Gone => {
                        // An unanswered close is not proof that the links were saved.
                        this.s().a.screen.try_set(Screen::Ready(view));
                        this.refresh_align();
                        return;
                    }
                }
            }
            this.s().a.open.try_set(None);
            this.reset_align();
            let c = &this.s().c;
            c.view.try_set(CheckView::Picker);
            c.return_to.try_set(None);
        });
    }

    /// The Align state of a project or book that is no longer shown: nothing of it stays. The
    /// aligner the server holds is closed, so its links are saved and it does not wait for the
    /// project to be left (at Home the server's own leave saves it).
    pub(super) fn reset_align(&self) {
        self.s().a.suggest_error.try_set(None);
        if let Some((at, aligner, _)) = self.s().a.open.try_update(Option::take).flatten() {
            let this = self.clone();
            spawn_local(async move {
                let close = Close {
                    at,
                    request: js::random_request_id(),
                    aligner,
                };
                match closed(this.runtime().api().call(&close).await) {
                    Closed::Saved | Closed::Gone => this.runtime().align_forget(aligner),
                    Closed::Kept(unsaved) => this.runtime().align_save(aligner, unsaved),
                    Closed::Unanswered => {}
                }
            });
        }
        OPENS.with(Latest::begin);
        READS.with(Latest::begin);
        NAVIGATIONS.with(Latest::begin);
        QUEUE.with(|q| q.borrow_mut().drop_waiting());
        self.align_edits_changed();
        if let Some(id) = HOOK.with(Cell::take) {
            self.runtime().saves().remove_gate_hook(id);
        }
        if let Some(id) = LISTENER.with(Cell::take) {
            self.runtime().unlisten(id);
        }
        let a = &self.s().a;
        a.screen.try_set(Screen::Loading);
        a.rail.try_set(Rail::Loading);
        a.verse.try_set(None);
    }

    /// The rail picks a verse, or a marking row button moves to a neighbour (tC4
    /// `setAlignVerse`). The server saves the verse that is left before the next one opens.
    pub fn set_align_verse(&self, place: AlignPlace) {
        let a = &self.s().a;
        if a.verse
            .get_untracked()
            .is_some_and(|v| same_verse(v, place))
        {
            return;
        }
        if let Some((_, book)) = self.align_scope() {
            a.remembered.try_update(|r| {
                r.retain(|(b, _)| *b != book);
                r.push((book, place));
            });
        }
        a.history_open.try_set(false);
        a.restore_failed.try_set(false);
        a.verse.try_set(Some(place));
        self.open_align(Some(place));
    }

    /// Opens the aligner for `place` (the book's resume point when none), or reads the open
    /// one again. Only the newest open may land.
    pub fn open_align(&self, place: Option<AlignPlace>) {
        let navigation = NAVIGATIONS.with(Latest::begin);
        let this = self.clone();
        spawn_local(async move {
            if this.drain_align_edits().await && NAVIGATIONS.with(|l| l.is_current(navigation)) {
                this.open_align_after_edits(place);
            }
        });
    }

    fn open_align_after_edits(&self, place: Option<AlignPlace>) {
        let Some((at, book)) = self.align_scope() else {
            return;
        };
        let attempt = OPENS.with(Latest::begin);
        // Every page edit has reached this verse before the server is asked to leave it.
        let a = self.s().a.clone();
        a.screen.try_set(Screen::Loading);
        let this = self.clone();
        spawn_local(async move {
            let open = Open {
                at,
                request: js::random_request_id(),
                book,
                verse: place.map(open_verse),
            };
            let result = this.runtime().api().call(&open).await;
            if !OPENS.with(|l| l.is_current(attempt)) {
                return;
            }
            match result {
                Ok(Opened::Ready(view)) => this.align_view(*view),
                Ok(Opened::Blocked { why, at }) => {
                    // The rail shows the verse that does not open as the open one, as tC4's
                    // `alignVerse` does.
                    if let Some(at) = at {
                        a.verse.try_set(Some(AlignPlace {
                            chapter: at.chapter,
                            verse: at.verse,
                            last: at.verse,
                        }));
                    }
                    a.screen.try_set(Screen::Blocked(why));
                }
                // The verse that was open could not be saved, so it stays open: the server
                // answers its own view, and the indicator says why it did not save.
                Err(CallError::Api(ApiError::NotSaved(_))) => this.reopen_open_verse(place),
                Err(CallError::Api(ApiError::StaleScope { .. }) | CallError::Dropped) => {}
                Err(error) => {
                    a.screen.try_set(Screen::Failed(error.to_string()));
                }
            }
        });
    }

    /// The verse the server holds open, read again: after a gap, a refused leave or a changed
    /// switch. Its view carries the save state and the suggestions' state.
    pub fn refresh_align(&self) {
        let Some((at, book)) = self.align_scope() else {
            return;
        };
        let Some(aligner) = self.s().a.screen.with_untracked(|s| match s {
            Screen::Ready(view) => Some(view.aligner),
            _ => None,
        }) else {
            return;
        };
        let attempt = OPENS.with(Latest::current);
        let refresh = REFRESHES.with(Latest::begin);
        let this = self.clone();
        spawn_local(async move {
            if !this.drain_align_edits().await
                || !OPENS.with(|l| l.is_current(attempt))
                || !REFRESHES.with(|l| l.is_current(refresh))
            {
                return;
            }
            let place = this.s().a.screen.with_untracked(|s| match s {
                Screen::Ready(view) if view.aligner == aligner => Some(view.place),
                _ => None,
            });
            let Some(place) = place else { return };
            let result = this
                .runtime()
                .api()
                .call(&Open {
                    at,
                    request: js::random_request_id(),
                    book,
                    verse: Some(open_verse(place)),
                })
                .await;
            if !OPENS.with(|l| l.is_current(attempt)) || !REFRESHES.with(|l| l.is_current(refresh))
            {
                return;
            }
            if let Ok(Opened::Ready(view)) = result {
                let current = this.s().a.screen.with_untracked(|s| match s {
                    Screen::Ready(now) if now.aligner == aligner => Some(now.seq),
                    _ => None,
                });
                match current {
                    Some(seq)
                        if !edits_pending() && (view.aligner != aligner || view.seq >= seq) =>
                    {
                        this.align_view(*view);
                    }
                    // A click made during the read wins. Read its answer's status afterwards.
                    Some(_) => this.refresh_align(),
                    None => {}
                }
            }
        });
    }

    /// The verse the server holds, opened again. Read from `open`, not the screen: an open on
    /// its way has already put the screen at Loading. `asked` is the open that was refused, so
    /// a refusal of the held verse itself ends here.
    fn reopen_open_verse(&self, asked: Option<AlignPlace>) {
        let held = self
            .s()
            .a
            .open
            .with_untracked(|o| o.as_ref().map(|(_, _, place)| *place));
        match held {
            Some(place) if asked != Some(place) => self.open_align(Some(place)),
            _ => {
                self.s().a.screen.try_set(Screen::Failed(String::new()));
            }
        }
    }

    /// *Restore this version* from the open verse's History (owner ruling H5). The aligner holds
    /// the verse's links while it is open, so the links typed are saved first, the restore is
    /// written over the alignment as now stored, and the verse opens again from it.
    pub fn restore_alignment(&self, from: tc_api::ids::ChangeRef) {
        let Some((at, book)) = self.align_scope() else {
            return;
        };
        let Some(place) = self.s().a.screen.with_untracked(|s| match s {
            Screen::Ready(view) => Some(view.place),
            _ => None,
        }) else {
            return;
        };
        let a = self.s().a.clone();
        a.restoring.try_set(true);
        let this = self.clone();
        spawn_local(async move {
            // Every field and the verse's links are saved first, and no click reaches the
            // aligner until it opens again: a link placed meanwhile would be saved over the
            // restore (loss lens M1).
            let saved = this.runtime().saves().gate().await;
            a.screen.try_set(Screen::Loading);
            let mut restored = false;
            if saved && this.flush_aligner().await {
                let api = this.runtime().api();
                let open = Open {
                    at: at.clone(),
                    request: js::random_request_id(),
                    book,
                    verse: Some(open_verse(place)),
                };
                if let Ok(Opened::Ready(view)) = api.call(&open).await
                    && let Some(shown) = view.history.clone()
                {
                    let restore = tc_api::conflicts::Restore {
                        at: at.clone(),
                        request: js::random_request_id(),
                        shown,
                        from,
                    };
                    restored = api.call(&restore).await.is_ok();
                    let close = Close {
                        at,
                        request: js::random_request_id(),
                        aligner: view.aligner,
                    };
                    let _ = api.call(&close).await;
                }
            }
            a.restoring.try_set(false);
            a.restore_failed.try_set(!restored);
            this.open_align(Some(place));
        });
    }

    /// A view of the open aligner came: it is the screen's, its save state is the indicator's,
    /// and its row is the rail's.
    fn align_view(&self, view: AlignerView) {
        let a = &self.s().a;
        if let Some((at, _)) = self.align_scope() {
            a.open.try_set(Some((at, view.aligner, view.place)));
        }
        self.runtime().align_save(view.aligner, view.save.clone());
        // Only a change is announced: every row of the rail reads the open verse.
        if a.verse.get_untracked() != Some(view.place) {
            a.verse.try_set(Some(view.place));
        }
        a.rail.with_untracked(|rail| {
            if let Rail::Ready(rows) = rail {
                rows.patch(view.place, view.status);
            }
        });
        a.screen.try_set(Screen::Ready(Box::new(view)));
    }

    /// A click, a drop or a button (tC4 `applyAlignEdit`): queued, and sent when the reply to
    /// the one before has come.
    pub fn align_edit(&self, _cap: WriteCap, edit: AlignEdit) {
        if !self
            .s()
            .a
            .screen
            .with_untracked(|s| matches!(s, Screen::Ready(_)))
        {
            return;
        }
        let next = QUEUE.with(|q| q.borrow_mut().push(edit));
        self.align_edits_changed();
        if let Some(edit) = next {
            self.send_align_edit(edit);
        }
    }

    fn align_edits_changed(&self) {
        self.s().a.edit_error.try_set(edit_failure());
        // A click that saves while Home asks about it was not lost after all: Home goes on.
        if !edits_pending() {
            self.answer_align_leave(true);
        }
        self.runtime().saves().server_states_changed();
        let waiting = EDIT_WAITERS.with(|w| std::mem::take(&mut *w.borrow_mut()));
        for resolve in waiting {
            let _ = resolve.call0(&JsValue::NULL);
        }
    }

    async fn drain_align_edits(&self) -> bool {
        let attempt = OPENS.with(Latest::current);
        while edits_pending() {
            if edit_failure().is_some() {
                return false;
            }
            let changed = Promise::new(&mut |resolve, _| {
                EDIT_WAITERS.with(|w| w.borrow_mut().push(resolve));
            });
            let _ = JsFuture::from(changed).await;
            if !OPENS.with(|l| l.is_current(attempt)) {
                return false;
            }
        }
        true
    }

    /// Before Home: the clicks not yet saved get a few seconds to save, sent again when a send
    /// fails on the way; only a click lost for sure makes Home ask, Stay or Leave without it
    /// (owner ruling ALIGN-LEAVE). Answers whether Home may go on.
    pub(super) async fn settle_align_clicks(&self) -> bool {
        // A send that fails at once (no connection) is sent again at most this often.
        const RETRY_EVERY_MS: f64 = 250.0;
        let press = SETTLES.with(Latest::begin);
        let start = js::now_ms();
        let mut retried = f64::NEG_INFINITY;
        loop {
            if !SETTLES.with(|l| l.is_current(press)) {
                return false;
            }
            let now = js::now_ms();
            let mut wake = crate::app::align::LEAVE_WAIT_MS - (now - start);
            match QUEUE.with(|q| q.borrow().on_leave(now - start)) {
                LeaveWait::Leave => return true,
                LeaveWait::Ask => return self.ask_leave_without_clicks().await,
                LeaveWait::Retry if now - retried >= RETRY_EVERY_MS => {
                    retried = now;
                    if let Some(sent) = QUEUE.with(|q| q.borrow_mut().retry()) {
                        self.align_edits_changed();
                        self.send_align_request(sent, true);
                    }
                }
                LeaveWait::Retry => wake = wake.min(retried + RETRY_EVERY_MS - now),
                LeaveWait::Wait => {}
            }
            // Woken by the click's answer, or when there is something to do again.
            let changed = Promise::new(&mut |resolve, _| {
                if let Ok(window) = js::window() {
                    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                        &resolve,
                        wake.max(0.0) as i32 + 1,
                    );
                }
                EDIT_WAITERS.with(|w| w.borrow_mut().push(resolve));
            });
            let _ = JsFuture::from(changed).await;
        }
    }

    async fn ask_leave_without_clicks(&self) -> bool {
        let answer = Promise::new(&mut |resolve, _| {
            // An older question still open gives way: its wait ends as Stay.
            if let Some(older) = LEAVE_ANSWER.with(|a| a.borrow_mut().replace(resolve)) {
                let _ = older.call1(&JsValue::NULL, &JsValue::FALSE);
            }
        });
        self.s().a.leave_ask.try_set(true);
        let leave = JsFuture::from(answer)
            .await
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        self.s().a.leave_ask.try_set(false);
        if leave {
            QUEUE.with(|q| q.borrow_mut().drop_waiting());
            self.align_edits_changed();
        }
        leave
    }

    /// The person's answer to Home's question: Leave without it, or Stay.
    pub fn answer_align_leave(&self, leave: bool) {
        if let Some(resolve) = LEAVE_ANSWER.with(|a| a.borrow_mut().take()) {
            let _ = resolve.call1(&JsValue::NULL, &JsValue::from_bool(leave));
        }
    }

    fn send_align_edit(&self, edit: AlignEdit) {
        let Some((at, _)) = self.align_scope() else {
            return;
        };
        let Some((aligner, seq)) = self.s().a.screen.with_untracked(|s| match s {
            Screen::Ready(view) => Some((view.aligner, view.seq)),
            _ => None,
        }) else {
            return;
        };
        let sent = SentEdit {
            request: Edit {
                at,
                request: js::random_request_id(),
                aligner,
                seq,
                edit,
            },
            run: self.runtime().state().instance.get_untracked(),
        };
        QUEUE.with(|q| q.borrow_mut().sent(sent.clone()));
        self.send_align_request(sent, false);
    }

    fn fail_align_request(&self, reason: EditFailure) {
        QUEUE.with(|q| q.borrow_mut().failed(reason));
        self.align_edits_changed();
    }

    fn send_align_request(&self, sent: SentEdit, retry: bool) {
        let attempt = OPENS.with(Latest::current);
        let this = self.clone();
        spawn_local(async move {
            if retry {
                let hello = this.runtime().api().call(&Hello {}).await;
                if !OPENS.with(|l| l.is_current(attempt)) {
                    return;
                }
                let run = match hello {
                    Ok(hello) => hello.instance,
                    Err(error) => {
                        this.fail_align_request(EditFailure::Retryable(error.to_string()));
                        return;
                    }
                };
                if !OPENS.with(|l| l.is_current(attempt)) {
                    return;
                }
                if run != sent.run || this.runtime().state().instance.get_untracked() != sent.run {
                    this.fail_align_request(EditFailure::Refused(
                        ApiError::StaleSession.to_string(),
                    ));
                    return;
                }
            }
            let result = this.runtime().api().call(&sent.request).await;
            if !OPENS.with(|l| l.is_current(attempt)) {
                return;
            }
            match result {
                Ok(view) => {
                    this.align_view(view);
                    let next = QUEUE.with(|q| q.borrow_mut().replied());
                    this.align_edits_changed();
                    if let Some(next) = next {
                        this.send_align_edit(next);
                    }
                }
                Err(CallError::Transport(error)) => {
                    this.fail_align_request(EditFailure::Retryable(error.to_string()));
                }
                Err(error) => {
                    // A refusal is not an acknowledgement, and a new actor has no right to
                    // reinterpret these queued card positions. Leave them in the window.
                    this.fail_align_request(EditFailure::Refused(error.to_string()));
                }
            }
            this.align_edits_changed();
        });
    }

    /// "Keep the carried-over links" (tC4 `keepReflow`): the links the verse's new text could
    /// not keep stay in the bank, and the rest are the person's to save.
    pub fn keep_align_reflow(&self, _cap: WriteCap) {
        let Some((at, _)) = self.align_scope() else {
            return;
        };
        let Some((aligner, seq)) = self.s().a.screen.with_untracked(|s| match s {
            Screen::Ready(view) => Some((view.aligner, view.seq)),
            _ => None,
        }) else {
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            let request = KeepReflow {
                at,
                request: js::random_request_id(),
                aligner,
                seq,
            };
            if let Ok(view) = this.runtime().api().call(&request).await {
                this.align_view(view);
            }
        });
    }

    /// The suggestions switch (tC4 `setAlignSuggestions`): the project's, kept on this computer;
    /// the aligner's view carries it, so the view is read again.
    pub fn set_align_suggestions(&self, on: bool) {
        let Some((at, _)) = self.align_scope() else {
            return;
        };
        // A failure stays shown until the person tries again or leaves the screen: the refresh
        // that follows it must not clear it.
        self.s().a.suggest_error.try_set(None);
        let this = self.clone();
        spawn_local(async move {
            let request = SetAlignSuggestions {
                at,
                request: js::random_request_id(),
                on,
            };
            match this.runtime().api().call(&request).await {
                // The tool is being left: nothing to say on it.
                Ok(()) | Err(CallError::Api(ApiError::StaleScope { .. }) | CallError::Dropped) => {}
                Err(error) => {
                    this.s().a.suggest_error.try_set(Some(error.to_string()));
                }
            }
            this.refresh_align();
        });
    }

    /// "Suggest" (tC4 `suggestAlign`): the engine proposes links for the words in the bank; they
    /// stand on their cards until the person accepts or rejects them.
    pub fn suggest_align(&self) {
        let Some((at, _)) = self.align_scope() else {
            return;
        };
        let Some(aligner) = self.s().a.screen.with_untracked(|s| match s {
            Screen::Ready(view) => Some(view.aligner),
            _ => None,
        }) else {
            return;
        };
        let a = self.s().a.clone();
        if a.asking.get_untracked() {
            return;
        }
        a.asking.try_set(true);
        let attempt = OPENS.with(Latest::current);
        let this = self.clone();
        spawn_local(async move {
            let request = tc_api::align::Suggest {
                at,
                request: js::random_request_id(),
                aligner,
            };
            let result = this.runtime().api().call(&request).await;
            a.asking.try_set(false);
            if !OPENS.with(|l| l.is_current(attempt)) {
                return;
            }
            if let Ok(view) = result {
                this.align_view(view);
            }
        });
    }

    /// "Retry" on a verse that could not open.
    pub fn retry_align(&self) {
        let place = self.s().a.verse.get_untracked();
        self.open_align(place);
    }

    /// The rail of the book (tC4 `loadAlignIndex`). Only the newest read may land.
    pub fn read_align_rail(&self) {
        let Some((at, book)) = self.align_scope() else {
            return;
        };
        let attempt = READS.with(Latest::begin);
        let this = self.clone();
        spawn_local(async move {
            let result = this.runtime().api().call(&Index { at, book }).await;
            if !READS.with(|l| l.is_current(attempt)) {
                return;
            }
            let a = &this.s().a;
            match result {
                Ok(view) => {
                    let rows = RailRows::new(view.items);
                    // The open verse's row is the aligner's own (a read may be older).
                    a.screen.with_untracked(|s| {
                        if let Screen::Ready(open) = s {
                            rows.patch(open.place, open.status);
                        }
                    });
                    a.rail.try_set(Rail::Ready(rows));
                }
                Err(CallError::Api(ApiError::StaleScope { .. }) | CallError::Dropped) => {}
                Err(error) => {
                    a.rail.try_set(Rail::Failed(error.to_string()));
                }
            }
        });
    }

    /// While the tool is open, the navigation gate asks the server to save the links (6.5).
    fn start_align_hook(&self) {
        if HOOK.with(|h| h.get().is_some()) {
            return;
        }
        let this = self.clone();
        let id = self
            .runtime()
            .saves()
            .add_gate_hook(std::rc::Rc::new(move || {
                let this = this.clone();
                Box::pin(async move {
                    this.flush_aligner().await || (!edits_pending() && this.refused_for_good())
                })
            }));
        HOOK.with(|h| h.set(Some(id)));
    }

    /// The open aligner's last save was refused in a way no retry in this open can fix. It
    /// does not keep the person from Home: closing keeps the links' draft for the next open
    /// (INV-A-REFUSED-SAVE-NEVER-TRAPS-THE-TRANSLATOR, tC4 #807).
    fn refused_for_good(&self) -> bool {
        let Some(aligner) = self
            .s()
            .a
            .open
            .with_untracked(|open| open.as_ref().map(|(_, aligner, _)| *aligner))
        else {
            return false;
        };
        self.runtime().0.state.align_saves.with_untracked(|saves| {
            matches!(saves.get(&aligner), Some(AlignSave::Failed(reason))
                if !crate::app::save::classify::retry_can_fix(reason))
        })
    }

    /// Retry includes server-owned alignment saves, which are not fields of SaveMachine.
    pub(super) fn retry_align_save(&self) {
        if self.s().a.open.with_untracked(Option::is_none) {
            return;
        }
        if let Some(sent) = QUEUE.with(|q| q.borrow_mut().retry()) {
            self.align_edits_changed();
            self.send_align_request(sent, true);
        }
        let this = self.clone();
        spawn_local(async move {
            this.flush_aligner().await;
        });
    }

    /// Every queued click reaches the server before `align/flush`. Clicks made while the save
    /// runs join the next drain, so navigation cannot leave them behind.
    async fn flush_aligner(&self) -> bool {
        let attempt = OPENS.with(Latest::current);
        loop {
            if !self.drain_align_edits().await || !OPENS.with(|l| l.is_current(attempt)) {
                return false;
            }
            let Some((at, _)) = self.align_scope() else {
                return true;
            };
            let aligner = self
                .s()
                .a
                .open
                .with_untracked(|open| open.as_ref().map(|(_, aligner, _)| *aligner));
            let request = Flush {
                at,
                request: js::random_request_id(),
            };
            let generation = QUEUE.with(|q| q.borrow().generation());
            let result = self.runtime().api().call(&request).await;
            if !OPENS.with(|l| l.is_current(attempt)) {
                return false;
            }
            match result {
                Ok(AlignSave::Saved) => {
                    if QUEUE.with(|q| q.borrow().generation()) != generation {
                        continue;
                    }
                    if let Some(aligner) = aligner {
                        self.runtime().align_save(aligner, AlignSave::Saved);
                    }
                    if !edits_pending() {
                        return true;
                    }
                }
                Ok(unsaved) => {
                    if let Some(aligner) = aligner {
                        self.runtime().align_save(aligner, unsaved);
                    }
                    return false;
                }
                Err(CallError::Api(ApiError::StaleScope { .. }) | CallError::Dropped) => {
                    return !edits_pending();
                }
                Err(_) => return false,
            }
        }
    }

    /// What the server says outside the person's own requests: a teammate's change to the
    /// book reads the rail again, and a gap or a change of the suggestion engine reads the
    /// aligner again.
    fn start_align_listener(&self) {
        if LISTENER.with(|l| l.get().is_some()) {
            return;
        }
        let this = self.clone();
        let id = self.runtime().listen(move |frame| {
            let Some((_, book)) = this.align_scope() else {
                return;
            };
            let refresh = refresh_for(&frame.event, book);
            if refresh.rail {
                this.read_align_rail();
            }
            if refresh.aligner {
                this.refresh_align();
            }
        });
        LISTENER.with(|l| l.set(Some(id)));
    }
}
