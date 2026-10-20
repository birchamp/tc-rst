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

use leptos::prelude::*;
use tc_api::ApiError;
use tc_api::align::{
    AlignEdit, AlignPlace, AlignSave, AlignerView, Blocked, Close, Edit, Flush, Index, IndexItem,
    KeepReflow, Open, OpenVerse, Opened,
};
use tc_api::event::{ChangeCause, Event, SessionEvent};
use tc_api::prefs::SetAlignSuggestions;
use tc_api::query::QueryKey;
use tc_core::BookId;
use wasm_bindgen_futures::spawn_local;

use super::ListenerId;
use super::app_ctx::Actions;
use super::js;
use super::saves::GateHookId;
use crate::app::align::{EditQueue, Lens, RailFilter, patch_status, same_verse};
use crate::app::check::CheckView;
use crate::app::client::CallError;

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

/// The rail's verses (tC4 `s.alignIndex`).
#[derive(Debug, Clone, PartialEq)]
pub enum Rail {
    Loading,
    Ready(Vec<IndexItem>),
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
    /// The verse each book was last at, kept in memory only (tC4 `toolPos`, §4.2).
    pub remembered: ArcRwSignal<Vec<(BookId, AlignPlace)>>,
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
            remembered: ArcRwSignal::new(Vec::new()),
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
    static OPENS: Cell<u64> = const { Cell::new(0) };
    /// Counts the reads of the rail (tC4's `alignIndexSeq`).
    static READS: Cell<u64> = const { Cell::new(0) };
    /// The edits the person made, one on their way at a time (A-14).
    static QUEUE: RefCell<EditQueue> = RefCell::new(EditQueue::default());
    /// The gate hook that asks the server to save the links, while the tool is open.
    static HOOK: Cell<Option<GateHookId>> = const { Cell::new(None) };
    /// The listener that keeps the rail and the view true to the server's events.
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

fn next_read() -> u64 {
    READS.with(|c| {
        c.set(c.get() + 1);
        c.get()
    })
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
        next_open();
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
        let Some((at, _)) = self.align_scope() else {
            self.reset_align();
            return;
        };
        let aligner = self.s().a.screen.with_untracked(|s| match s {
            Screen::Ready(view) => Some(view.aligner),
            _ => None,
        });
        let this = self.clone();
        spawn_local(async move {
            if let Some(aligner) = aligner {
                let close = Close {
                    at,
                    request: js::random_request_id(),
                    aligner,
                };
                match this.runtime().api().call(&close).await {
                    Ok(AlignSave::Saved) => this.runtime().align_forget(aligner),
                    Ok(unsaved) => {
                        this.runtime().align_save(aligner, unsaved);
                        return;
                    }
                    // The aligner was replaced or the project closed: nothing waits to be saved.
                    Err(_) => this.runtime().align_forget(aligner),
                }
            }
            this.reset_align();
            let c = &this.s().c;
            c.view.try_set(CheckView::Picker);
            c.return_to.try_set(None);
        });
    }

    /// The Align state of a project or book that is no longer shown: nothing of it stays.
    pub(super) fn reset_align(&self) {
        next_open();
        next_read();
        QUEUE.with(|q| q.borrow_mut().drop_waiting());
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
        a.verse.try_set(Some(place));
        self.open_align(Some(place));
    }

    /// Opens the aligner for `place` (the book's resume point when none), or reads the open
    /// one again. Only the newest open may land.
    pub fn open_align(&self, place: Option<AlignPlace>) {
        let Some((at, book)) = self.align_scope() else {
            return;
        };
        let attempt = next_open();
        // What waited was made on a verse that is being left; the server saves that one.
        QUEUE.with(|q| q.borrow_mut().drop_waiting());
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
            if current_open() != attempt {
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
                Err(CallError::Api(ApiError::NotSaved(_))) => this.reopen_open_verse(),
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
        let Some(place) = self.s().a.screen.with_untracked(|s| match s {
            Screen::Ready(view) => Some(view.place),
            _ => None,
        }) else {
            return;
        };
        self.open_align(Some(place));
    }

    fn reopen_open_verse(&self) {
        // With no verse left to name, `None` resumes where the book does; a refused leave has
        // one: the verse still open.
        let held = self.s().a.screen.with_untracked(|s| match s {
            Screen::Ready(view) => Some(view.place),
            _ => None,
        });
        match held {
            Some(place) => self.open_align(Some(place)),
            None => {
                self.s().a.screen.try_set(Screen::Failed(String::new()));
            }
        }
    }

    /// A view of the open aligner came: it is the screen's, its save state is the indicator's,
    /// and its row is the rail's.
    fn align_view(&self, view: AlignerView) {
        let a = &self.s().a;
        self.runtime().align_save(view.aligner, view.save.clone());
        a.verse.try_set(Some(view.place));
        a.rail.try_update(|rail| {
            if let Rail::Ready(items) = rail {
                patch_status(items, view.place, view.status);
            }
        });
        a.screen.try_set(Screen::Ready(Box::new(view)));
    }

    /// A click, a drop or a button (tC4 `applyAlignEdit`): queued, and sent when the reply to
    /// the one before has come.
    pub fn align_edit(&self, edit: AlignEdit) {
        if let Some(edit) = QUEUE.with(|q| q.borrow_mut().push(edit)) {
            self.send_align_edit(edit);
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
            QUEUE.with(|q| q.borrow_mut().drop_waiting());
            return;
        };
        let attempt = current_open();
        let this = self.clone();
        spawn_local(async move {
            let request = Edit {
                at,
                request: js::random_request_id(),
                aligner,
                seq,
                edit,
            };
            let result = this.runtime().api().call(&request).await;
            if current_open() != attempt {
                return;
            }
            match result {
                Ok(view) => {
                    this.align_view(view);
                    if let Some(next) = QUEUE.with(|q| q.borrow_mut().replied()) {
                        this.send_align_edit(next);
                    }
                }
                // The edits that waited were made on a view that is gone; the server's own is
                // read again.
                Err(CallError::Api(ApiError::StaleSession)) => {
                    QUEUE.with(|q| q.borrow_mut().drop_waiting());
                    this.reopen_open_verse();
                }
                Err(CallError::Api(ApiError::NotSaved(_))) => {
                    QUEUE.with(|q| q.borrow_mut().drop_waiting());
                }
                Err(CallError::Api(ApiError::StaleScope { .. }) | CallError::Dropped) => {}
                Err(_) => {
                    QUEUE.with(|q| q.borrow_mut().drop_waiting());
                    this.reopen_open_verse();
                }
            }
        });
    }

    /// "Keep the carried-over links" (tC4 `keepReflow`): the links the verse's new text could
    /// not keep stay in the bank, and the rest are the person's to save.
    pub fn keep_align_reflow(&self) {
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
        let this = self.clone();
        spawn_local(async move {
            let request = SetAlignSuggestions {
                at,
                request: js::random_request_id(),
                on,
            };
            let _ = this.runtime().api().call(&request).await;
            this.refresh_align();
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
        let attempt = next_read();
        let this = self.clone();
        spawn_local(async move {
            let result = this.runtime().api().call(&Index { at, book }).await;
            if READS.with(Cell::get) != attempt {
                return;
            }
            let a = &this.s().a;
            match result {
                Ok(view) => {
                    let mut items = view.items;
                    // The open verse's row is the aligner's own (a read may be older).
                    a.screen.with_untracked(|s| {
                        if let Screen::Ready(open) = s {
                            patch_status(&mut items, open.place, open.status);
                        }
                    });
                    a.rail.try_set(Rail::Ready(items));
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
                Box::pin(async move { this.flush_aligner().await })
            }));
        HOOK.with(|h| h.set(Some(id)));
    }

    /// `align/flush`: true when the links are saved. A failure keeps the person where they are.
    async fn flush_aligner(&self) -> bool {
        let Some((at, _)) = self.align_scope() else {
            return true;
        };
        let aligner = self.s().a.screen.with_untracked(|s| match s {
            Screen::Ready(view) => Some(view.aligner),
            _ => None,
        });
        let request = Flush {
            at,
            request: js::random_request_id(),
        };
        match self.runtime().api().call(&request).await {
            Ok(AlignSave::Saved) => {
                if let Some(aligner) = aligner {
                    self.runtime().align_save(aligner, AlignSave::Saved);
                }
                true
            }
            Ok(unsaved) => {
                if let Some(aligner) = aligner {
                    self.runtime().align_save(aligner, unsaved);
                }
                false
            }
            // The project is closed or replaced: there is nothing of this page's to save.
            Err(CallError::Api(ApiError::StaleScope { .. }) | CallError::Dropped) => true,
            Err(_) => false,
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
            match &frame.event {
                Event::Resync => {
                    this.read_align_rail();
                    this.refresh_align();
                }
                Event::Session {
                    event: SessionEvent::Suggest { .. },
                    ..
                } => this.refresh_align(),
                Event::Session {
                    event: SessionEvent::Changed { scope, cause, .. },
                    ..
                } if !matches!(cause, ChangeCause::LocalSave { .. } | ChangeCause::System)
                    && QueryKey::Book(book).depends_on(scope) =>
                {
                    this.read_align_rail();
                }
                _ => {}
            }
        });
        LISTENER.with(|l| l.set(Some(id)));
    }
}
