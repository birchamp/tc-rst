//! `use_app()` (APP-DESIGN 6.1, AD23): tC4's `useApp()` as later screens port it, `s` for the
//! state and `actions` for what a view asks the app to do. Server data in `s` is the
//! runtime's read-only [`ServerState`]; only view state is writable here; drafts live in the
//! save machine.
//!
//! Every async completion writes with `try_set` or `try_update`, so a reply that comes after
//! the page tore its signals down does nothing (6.5), and no `RefCell` borrow is held across
//! an `.await`.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use crate::app::jobs::JobEnd;
use leptos::prelude::*;
use send_wrapper::SendWrapper;
use tc_api::account::{
    CancelSignIn, HelpLink, Orgs, ProfileLink, RepoLink, Resume, SignOut, WebUrl,
};
use tc_api::door43::{OpenDoor43, SyncProject};
use tc_api::feedback::Compose;
use tc_api::home::{HomeCard, LanguageSuggestion};
use tc_api::ids::{LocaleId, ProjectRef, ReportId};
use tc_api::net::Set as NetSet;
use tc_api::prefs::SetLocale;
use tc_api::refusal::FeedbackRefusal;
use tc_api::refusal::Message;
use tc_api::{ApiError, Endpoint};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;

use super::leave::LeaveTo;

use super::{Runtime, ServerState, bridge, js};
use crate::app::account::{SignInFlow, SignInForm, SignInNext};
use crate::app::client::CallError;
use crate::app::feedback::{self, Attachment, FeedbackForm, HelpRequest, Opened};
use crate::app::fix::Fix;
use crate::app::forms::{AddBookForm, ImportForm, NewProjectForm};
use crate::app::internet::{InternetFlow, LinkKind, NetStep, Pending, SwitchRequests, SwitchStep};
use crate::app::latest::Latest;
pub(crate) use crate::app::net::NetEnd;
use crate::app::open_door43::OpenForm;
use crate::app::save::indicator::Retry;
use crate::app::share::{ShareForm, ShareProject, ShareStep, Target};
use crate::app::shell::{CurrentProject, Editing, View};
use crate::app::sources::SettingsForm;
use crate::app::submit::Submit;
use crate::app::sync::{SyncEnd, SyncToast};
use crate::app::update_book::UpdateBookForm;
use crate::i18n::show_locale;

/// Which dialog shows (tC4 `s.modal`, `tC4:src/state.jsx:476`): what the modal host switches
/// on, so typing in a form never rebuilds its dialog (a rebuilt input loses focus).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    Feedback,
    SignIn,
    AddProject,
    NewProject,
    NewObs,
    AddBook,
    Import,
    OpenDoor43,
    Share,
    GuidedFix,
    Settings,
    Sources,
    UpdateBook,
    /// Save work for a teammate (E2).
    SaveForTeammate,
    /// Receive work from a file (E2).
    ReceiveWork,
}

/// The dialog that shows, with its form (UI-3): a form lives exactly as long as its dialog, so
/// two forms are never set at once and none is left behind when its dialog closes.
#[derive(Debug, Clone, PartialEq)]
pub enum Dialog {
    Feedback,
    /// The Door43 sign-in step (M12), which unit A8 draws. `under` is the dialog it covers,
    /// which shows again after it with its form as the person left it.
    SignIn {
        under: Option<Box<Dialog>>,
    },
    /// "+ Add a project" (M1).
    AddProject,
    /// New Bible (M2).
    NewProject(NewProjectForm),
    /// New Open Bible Stories (M3).
    NewObs(NewProjectForm),
    /// Add a book (M4).
    AddBook(AddBookForm),
    /// Import (M5).
    Import(ImportForm),
    /// Open a project from Door43 (M6).
    OpenDoor43(OpenForm),
    /// Share on Door43.
    Share(ShareForm),
    /// The guided fix (M11, B5a).
    GuidedFix(Fix),
    /// Project settings (M7).
    Settings(SettingsForm),
    /// Get source texts (M8); its form is `src`, kept across openings as tC4 keeps it.
    Sources,
    /// Update a book from USFM (F1).
    UpdateBook(UpdateBookForm),
    /// Save work for a teammate (E2).
    SaveForTeammate(crate::app::exchange::ExportForm),
    /// Receive work from a file (E2).
    ReceiveWork(crate::app::exchange::ReceiveForm),
}

/// A dialog's form seen as the slot it used to be (UI-3): `None` while its dialog does not
/// show. A value set through it is kept only while its dialog shows; `None` closes the dialog.
pub struct FormSlot<T> {
    dialog: ArcRwSignal<Option<Dialog>>,
    pick: fn(&Dialog) -> Option<&T>,
    pick_mut: fn(&mut Dialog) -> Option<&mut T>,
}

impl<T: Clone> FormSlot<T> {
    pub fn with_untracked<R>(&self, f: impl FnOnce(&Option<T>) -> R) -> R {
        let pick = self.pick;
        let form = self
            .dialog
            .with_untracked(|d| d.as_ref().and_then(pick).cloned());
        f(&form)
    }

    pub fn try_update<R>(&self, f: impl FnOnce(&mut Option<T>) -> R) -> Option<R> {
        let (pick, pick_mut) = (self.pick, self.pick_mut);
        self.dialog.try_update(|d| {
            let mut form = d.as_ref().and_then(pick).cloned();
            let out = f(&mut form);
            match (d.as_mut().and_then(pick_mut), form) {
                (Some(shown), Some(form)) => *shown = form,
                (Some(_), None) => *d = None,
                (None, _) => {}
            }
            out
        })
    }

    pub fn try_set(&self, form: Option<T>) {
        self.try_update(|slot| *slot = form);
    }
}

/// Open from Door43's form, in the dialog that shows or under the sign-in step that covers it.
fn open_form(dialog: &mut Option<Dialog>) -> Option<&mut OpenForm> {
    match dialog.as_mut()? {
        Dialog::OpenDoor43(od) => Some(od),
        Dialog::SignIn { under } => match under.as_deref_mut()? {
            Dialog::OpenDoor43(od) => Some(od),
            _ => None,
        },
        _ => None,
    }
}

impl Dialog {
    pub fn kind(&self) -> DialogKind {
        match self {
            Self::Feedback => DialogKind::Feedback,
            Self::SignIn { .. } => DialogKind::SignIn,
            Self::AddProject => DialogKind::AddProject,
            Self::NewProject(_) => DialogKind::NewProject,
            Self::NewObs(_) => DialogKind::NewObs,
            Self::AddBook(_) => DialogKind::AddBook,
            Self::Import(_) => DialogKind::Import,
            Self::OpenDoor43(_) => DialogKind::OpenDoor43,
            Self::Share(_) => DialogKind::Share,
            Self::GuidedFix(_) => DialogKind::GuidedFix,
            Self::Settings(_) => DialogKind::Settings,
            Self::Sources => DialogKind::Sources,
            Self::UpdateBook(_) => DialogKind::UpdateBook,
            Self::SaveForTeammate(_) => DialogKind::SaveForTeammate,
            Self::ReceiveWork(_) => DialogKind::ReceiveWork,
        }
    }
}

/// A banner on Home: its text, and the "Ask for help" it offers (tC4 #378).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeBanner {
    pub text: String,
    pub help: Option<HelpRequest>,
}

/// Where a Home tile opens its project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenTarget {
    Book(tc_core::BookId),
    Story(tc_api::ids::StoryNo),
}

/// The toast an import leaves on Home (F1 sets it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportToast {
    pub message: String,
}

/// The layers over the screens (6.7). Units add the gateway and upgrade previews (C8), the
/// open progress (B3) and the drafts offer (B1b, C1b).
#[derive(Clone)]
pub struct Layers {
    pub dialog: ArcRwSignal<Option<Dialog>>,
    pub internet: ArcRwSignal<InternetFlow>,
}

/// tC4's `s`, the parts the shell owns.
#[derive(Clone)]
pub struct AppState {
    pub view: ArcRwSignal<View>,
    pub project: ArcRwSignal<Option<CurrentProject>>,
    pub editing: ArcRwSignal<Editing>,
    pub layers: Layers,
    /// The Feedback report, kept in memory only (A5 D11).
    pub fb: ArcRwSignal<Option<FeedbackForm>>,
    /// The Door43 sign-in step (M12, tC4's `si`), kept in memory only: its fields are not
    /// save fields, so the password never reaches the draft buffer or page storage (A8).
    pub sign_in: ArcRwSignal<SignInFlow>,
    /// The message of the screen that could not be shown (S15).
    pub view_error: ArcRwSignal<Option<String>>,
    /// Get source texts' form (tC4 `src`, B9a), kept across the dialog's openings as tC4
    /// keeps it.
    pub src: ArcRwSignal<crate::app::sources::SourcesForm>,
    /// The last receive's form, kept when its dialog closed while it ran, so the next
    /// Receive dialog shows what came of it (E2 step 4b).
    pub rx_last: ArcRwSignal<Option<crate::app::exchange::ReceiveForm>>,
    /// The release upgrades of Get source texts and their confirmation (tC4 `upgrade`, C8).
    pub up: ArcRwSignal<crate::app::upgrades::UpgradeState>,
    /// The change of checking language and its confirmation (tC4 `gatewayBusy`,
    /// `gatewayPreview`, `gatewayError`, C8).
    pub gw: ArcRwSignal<crate::app::upgrades::GatewayState>,
    /// A refused open, on Home (tC4 `bookError`; B3 sets it).
    pub book_error: ArcRwSignal<Option<HomeBanner>>,
    /// The project a sync runs for (tC4 `syncing`).
    pub syncing: ArcRwSignal<Option<ProjectRef>>,
    /// The result of the last sync, which Home shows (tC4 `syncResult`). Never stored.
    pub sync_result: ArcRwSignal<Option<SyncToast>>,
    /// The toast of a finished import (F1 sets it).
    pub import_toast: ArcRwSignal<Option<ImportToast>>,
    /// The reports of the *Import*s just made of books another program changed (D1), shown
    /// until the person closes them.
    pub outside_reports: ArcRwSignal<Vec<tc_api::session::KeptImported>>,
    /// An *Import* or *Not now* is on its way: a second click would send the same answer again.
    pub outside_busy: ArcRwSignal<bool>,
    /// What the last refused *Import*, *Not now* or *Dismiss* said, shown in the question and
    /// in the review's notices until the next answer goes through.
    pub outside_refused: ArcRwSignal<Option<String>>,
    /// The project an import of this page made: its card carries "Imported" (F1 sets it).
    pub imported_repo: ArcRwSignal<Option<ProjectRef>>,
    /// Home's `home.importRemoved`: the server said so in one listing of this launch, and it
    /// stays until the person opens Import.
    pub import_removed: ArcRwSignal<bool>,
    /// The open and the Understand screen (B3).
    pub u: super::understand::UnderstandState,
    /// The Check screen (B5a).
    pub c: super::check::CheckState,
    /// The Align tool (B6).
    pub a: super::align::AlignState,
    /// The conflict review over Translate (B11, tC4 `conflictReview`).
    pub review: ArcRwSignal<Option<crate::app::review::ReviewState>>,
    /// `/ui/<build>/`, where the logo and the catalogs are.
    pub base: Arc<str>,
    /// What the server holds: the internet switch, the account, the saved language, the
    /// sessions. Read-only here.
    pub server: ServerState,
}

impl AppState {
    /// The form of the dialog that shows, when `pick` finds it there.
    fn form<T: Clone + Send + Sync + 'static>(
        &self,
        pick: fn(&Dialog) -> Option<&T>,
    ) -> Signal<Option<T>> {
        let dialog = self.layers.dialog.clone();
        Signal::derive(move || dialog.with(|d| d.as_ref().and_then(pick).cloned()))
    }

    fn slot<T>(
        &self,
        pick: fn(&Dialog) -> Option<&T>,
        pick_mut: fn(&mut Dialog) -> Option<&mut T>,
    ) -> FormSlot<T> {
        FormSlot {
            dialog: self.layers.dialog.clone(),
            pick,
            pick_mut,
        }
    }

    /// New Bible's and New Open Bible Stories' form (tC4 `np`).
    pub fn np_slot(&self) -> FormSlot<NewProjectForm> {
        self.slot(
            |d| match d {
                Dialog::NewProject(np) | Dialog::NewObs(np) => Some(np),
                _ => None,
            },
            |d| match d {
                Dialog::NewProject(np) | Dialog::NewObs(np) => Some(np),
                _ => None,
            },
        )
    }

    /// Add a book's form (tC4 `ab`).
    pub fn ab_slot(&self) -> FormSlot<AddBookForm> {
        self.slot(
            |d| match d {
                Dialog::AddBook(ab) => Some(ab),
                _ => None,
            },
            |d| match d {
                Dialog::AddBook(ab) => Some(ab),
                _ => None,
            },
        )
    }

    /// Project settings' form (tC4 `st`).
    pub fn st_slot(&self) -> FormSlot<SettingsForm> {
        self.slot(
            |d| match d {
                Dialog::Settings(st) => Some(st),
                _ => None,
            },
            |d| match d {
                Dialog::Settings(st) => Some(st),
                _ => None,
            },
        )
    }

    /// The guided fix's state (tC4 `c.fix`).
    pub fn fix_slot(&self) -> FormSlot<Fix> {
        self.slot(
            |d| match d {
                Dialog::GuidedFix(fix) => Some(fix),
                _ => None,
            },
            |d| match d {
                Dialog::GuidedFix(fix) => Some(fix),
                _ => None,
            },
        )
    }

    /// Import's form (tC4 `im`).
    pub fn im_slot(&self) -> FormSlot<ImportForm> {
        self.slot(
            |d| match d {
                Dialog::Import(im) => Some(im),
                _ => None,
            },
            |d| match d {
                Dialog::Import(im) => Some(im),
                _ => None,
            },
        )
    }

    /// Update a book's form (F1).
    pub fn ub_slot(&self) -> FormSlot<UpdateBookForm> {
        self.slot(
            |d| match d {
                Dialog::UpdateBook(ub) => Some(ub),
                _ => None,
            },
            |d| match d {
                Dialog::UpdateBook(ub) => Some(ub),
                _ => None,
            },
        )
    }

    /// Update a book's form while it shows.
    pub fn update_book_form(&self) -> Signal<Option<UpdateBookForm>> {
        self.form(|d| match d {
            Dialog::UpdateBook(ub) => Some(ub),
            _ => None,
        })
    }

    /// New Bible's or New Open Bible Stories' form while it shows.
    pub fn new_project_form(&self) -> Signal<Option<NewProjectForm>> {
        self.form(|d| match d {
            Dialog::NewProject(np) | Dialog::NewObs(np) => Some(np),
            _ => None,
        })
    }

    /// Add a book's form while it shows.
    pub fn add_book_form(&self) -> Signal<Option<AddBookForm>> {
        self.form(|d| match d {
            Dialog::AddBook(ab) => Some(ab),
            _ => None,
        })
    }

    /// Project settings' form while it shows.
    pub fn settings_form(&self) -> Signal<Option<SettingsForm>> {
        self.form(|d| match d {
            Dialog::Settings(st) => Some(st),
            _ => None,
        })
    }

    /// The guided fix while it shows.
    pub fn fix_form(&self) -> Signal<Option<Fix>> {
        self.form(|d| match d {
            Dialog::GuidedFix(fix) => Some(fix),
            _ => None,
        })
    }

    /// Import's form while its dialog shows (tC4 `im`).
    pub fn import_form(&self) -> Signal<Option<ImportForm>> {
        self.form(|d| match d {
            Dialog::Import(im) => Some(im),
            _ => None,
        })
    }

    /// Open from Door43's form while its dialog shows (tC4 `od`).
    pub fn open_door43_form(&self) -> Signal<Option<OpenForm>> {
        self.form(|d| match d {
            Dialog::OpenDoor43(od) => Some(od),
            _ => None,
        })
    }

    /// The share dialog's form while it shows (tC4 `sh`).
    pub fn export_form(&self) -> Signal<Option<crate::app::exchange::ExportForm>> {
        self.form(|d| match d {
            Dialog::SaveForTeammate(ex) => Some(ex),
            _ => None,
        })
    }

    pub fn receive_form(&self) -> Signal<Option<crate::app::exchange::ReceiveForm>> {
        self.form(|d| match d {
            Dialog::ReceiveWork(rx) => Some(rx),
            _ => None,
        })
    }

    pub fn share_form(&self) -> Signal<Option<ShareForm>> {
        self.form(|d| match d {
            Dialog::Share(sh) => Some(sh),
            _ => None,
        })
    }

    pub fn new(
        base: &str,
        server: ServerState,
        view: View,
        project: Option<CurrentProject>,
        place: Option<crate::app::shell::BookPlace>,
        view_error: Option<String>,
    ) -> Self {
        Self {
            view: ArcRwSignal::new(view),
            project: ArcRwSignal::new(project),
            editing: ArcRwSignal::new(Editing::None),
            layers: Layers {
                dialog: ArcRwSignal::new(None),
                internet: ArcRwSignal::new(InternetFlow::Idle),
            },
            fb: ArcRwSignal::new(None),
            sign_in: ArcRwSignal::new(SignInFlow::Idle),
            view_error: ArcRwSignal::new(view_error),
            src: ArcRwSignal::new(crate::app::sources::SourcesForm::default()),
            rx_last: ArcRwSignal::new(None),
            up: ArcRwSignal::new(crate::app::upgrades::UpgradeState::default()),
            gw: ArcRwSignal::new(crate::app::upgrades::GatewayState::default()),
            book_error: ArcRwSignal::new(None),
            syncing: ArcRwSignal::new(None),
            sync_result: ArcRwSignal::new(None),
            import_toast: ArcRwSignal::new(None),
            outside_reports: ArcRwSignal::new(Vec::new()),
            outside_busy: ArcRwSignal::new(false),
            outside_refused: ArcRwSignal::new(None),
            imported_repo: ArcRwSignal::new(None),
            import_removed: ArcRwSignal::new(false),
            u: super::understand::UnderstandState::new(
                place.map(|p| p.book),
                place.map_or(1, |p| p.chapter),
            ),
            c: super::check::CheckState::new(),
            a: super::align::AlignState::new(),
            review: ArcRwSignal::new(None),
            base: Arc::from(base),
            server,
        }
    }
}

/// What `use_app()` answers.
#[derive(Clone)]
pub struct AppCtx {
    pub s: AppState,
    pub actions: Actions,
}

/// The app the page provides, from the reactive context.
pub fn use_app() -> Option<AppCtx> {
    use_context::<AppCtx>()
}

/// Puts `ctx` in the reactive context of the current owner.
pub(crate) fn provide_app(ctx: AppCtx) {
    provide_context(ctx);
}

struct Inner {
    runtime: Runtime,
    s: AppState,
    /// tC4's module flag `feedbackSending`: set at the click, before any await, so a second
    /// click sends nothing; the Feedback actions do nothing while it is set.
    sending: Cell<bool>,
    /// Counts the attachments asked for, so a compose reply for a report replaced meanwhile
    /// does not land in the new one.
    compose: Latest,
    /// The step that ran after a question already met `NeedsInternet` once (AD25).
    reasked: Cell<bool>,
    /// The `net/set` requests, one at a time.
    switch: Cell<SwitchRequests>,
    /// Counts the openings of the sign-in step, so a reply lands only in the one it came from.
    sign_in_flows: Cell<u64>,
    /// Counts the share flows (tC4's `startShareFlow`): a new Share or a closed dialog ends the
    /// one before, whose late answers then do nothing.
    share_flows: Latest,
    /// Each project's last Sync attempt (UI-3): a press after a lost reply sends the same
    /// request, which the server's reply cache answers.
    sync_attempts: std::cell::RefCell<Vec<(ProjectRef, Submit)>>,
}

/// tC4's `actions`, the ones the shell owns. Cheap to clone. The wrapper lets views capture
/// it in the closures Leptos asks to be `Send`; a browser runs the UI on one thread.
#[derive(Clone)]
pub struct Actions(SendWrapper<Rc<Inner>>);

impl Actions {
    pub fn new(runtime: Runtime, s: AppState) -> Self {
        Self(SendWrapper::new(Rc::new(Inner {
            runtime,
            s,
            sending: Cell::new(false),
            compose: Latest::new(),
            reasked: Cell::new(false),
            switch: Cell::new(SwitchRequests::default()),
            sign_in_flows: Cell::new(0),
            share_flows: Latest::new(),
            sync_attempts: std::cell::RefCell::default(),
        })))
    }

    pub(crate) fn s(&self) -> &AppState {
        &self.0.s
    }

    pub(crate) fn runtime(&self) -> &Runtime {
        &self.0.runtime
    }

    // ---- navigation (6.5): flush, and go only when everything could be saved -----------

    /// A mode tab (tC4 `go`, `tC4:src/state.jsx:4018-4040`): every field is drained first, and
    /// a standing failure keeps the person where they are (FR-32). D1 adds the checkpoint of a
    /// mode switch.
    pub fn go(&self, view: View) {
        let this = self.clone();
        spawn_local(async move {
            if !this.leave(LeaveTo::Tab(view)).await {
                return;
            }
            let s = this.s();
            s.view_error.try_set(None);
            // The open's overlay waits for the screen it landed on to read its book or story
            // (`book_shown`). A tab pressed before that read, inside the overlay's first 300 ms,
            // unmounts that screen, so nothing would ever take the overlay down, and its scrim
            // and focus trap would hold the whole window. The screen moved to shows its own
            // loading state instead.
            s.u.opening.try_set(None);
            s.review.try_set(None);
            s.view.try_set(view);
        });
    }

    /// The brand, the project chip and "Back to Home" (tC4 `backToProjects`,
    /// `tC4:src/state.jsx:7286-7326`): drained first; Home, no project, no editing card, and the
    /// screen's session closes behind (B3).
    pub fn back_to_projects(&self) {
        let this = self.clone();
        spawn_local(async move {
            if !this.leave(LeaveTo::Home).await {
                return;
            }
            this.leave_project(true);
        });
    }

    /// The server started again (3.10, D15): its opens and its reply cache are gone, so the
    /// page goes Home at once, with no gate, which could only save into the new run over tokens
    /// of the old one. The fields of the old run keep their entries for the offer.
    pub(crate) fn home_after_restart(&self) {
        self.leave_project(false);
    }

    /// Home, with no project. `close` ends the screen's session on the server (B3); after a
    /// restart there is none to end.
    fn leave_project(&self, close: bool) {
        let s = self.s();
        s.view_error.try_set(None);
        s.editing.try_set(Editing::None);
        s.review.try_set(None);
        let project = s.project.get_untracked();
        s.project.try_set(None);
        s.view.try_set(View::Home);
        self.close_session(project.filter(|_| close));
    }

    // ---- the internet switch (5.6) -------------------------------------------------------

    /// The menu's switch: it turns away from the state last asked for, so a second press
    /// before the server's reply turns it back, as tC4's does.
    pub fn toggle_internet(&self) {
        let confirmed = self.s().server.internet.get_untracked();
        let on = !self.0.switch.get().base(confirmed);
        self.set_internet(on);
    }

    /// Asks the server for the switch at `on`, after the request on its way if there is one.
    /// The reply and the `Internet` event both set the state the menu shows.
    pub fn set_internet(&self, on: bool) {
        let mut switch = self.0.switch.get();
        let send = switch.ask(on);
        self.0.switch.set(switch);
        if let Some(on) = send {
            let this = self.clone();
            spawn_local(async move { this.send_switch(on).await });
        }
    }

    /// Sends `net/set` until no state asked for is left, then lets "Turn on internet" go on.
    async fn send_switch(&self, mut on: bool) {
        loop {
            let state = self.net_set(on).await;
            let mut switch = self.0.switch.get();
            let step = switch.answered(state);
            self.0.switch.set(switch);
            match step {
                SwitchStep::Send(next) => on = next,
                SwitchStep::Settled(state) => {
                    let next = self
                        .s()
                        .layers
                        .internet
                        .try_update(|flow| flow.turned_on(state == Some(true)))
                        .unwrap_or(NetStep::Nothing);
                    self.carry_out(next);
                    return;
                }
            }
        }
    }

    /// One `net/set`: the server's state, or `None` when it did not answer with one.
    async fn net_set(&self, on: bool) -> Option<bool> {
        let request = NetSet {
            request: js::random_request_id(),
            on,
        };
        match self.0.runtime.api().call(&request).await {
            Ok(view) => {
                self.s().server.internet.try_set(view.on);
                Some(view.on)
            }
            Err(error) => {
                // The person is told ("tC4 could not use the internet"); a console error would
                // also be a page error to the capture and the journeys.
                console_warn(&format!("tc4: net/set: {error}"));
                None
            }
        }
    }

    /// A step that needs the internet: runs now while it is on, or asks (M15).
    pub fn internet_task(&self, then: Pending) {
        self.0.reasked.set(false);
        let on = self.s().server.internet.get_untracked();
        let step = self
            .s()
            .layers
            .internet
            .try_update(|flow| flow.ask(on, then))
            .unwrap_or(NetStep::Nothing);
        self.carry_out(step);
    }

    /// "Turn on internet".
    pub fn turn_on_internet(&self) {
        let step = self
            .s()
            .layers
            .internet
            .try_update(InternetFlow::turn_on)
            .unwrap_or(NetStep::Nothing);
        self.carry_out(step);
    }

    /// "Not now", Escape, the ✕ and the scrim: nothing is sent.
    pub fn cancel_internet(&self) {
        let step = self
            .s()
            .layers
            .internet
            .try_update(InternetFlow::not_now)
            .unwrap_or(NetStep::Nothing);
        self.carry_out(step);
    }

    pub fn close_net_failed(&self) {
        self.s()
            .layers
            .internet
            .try_update(InternetFlow::close_failed);
    }

    fn carry_out(&self, step: NetStep) {
        match step {
            NetStep::Run(then) => self.run_pending(then),
            NetStep::Dropped(then) => self.drop_pending(then),
            NetStep::TurnOn => self.set_internet(true),
            NetStep::Nothing => {}
        }
    }

    fn run_pending(&self, then: Pending) {
        match then {
            Pending::Feedback => self.send_now(),
            Pending::Link(kind, url) => self.open_link(kind, url),
            Pending::SignIn { flow } => self.send_sign_in(flow),
            Pending::CheckSignIn => self.check_now(),
            Pending::Profile => self.profile_now(),
            Pending::Share(project) => self.share_now(*project),
            Pending::OpenDoor43 => self.open_door43_now(),
            Pending::Sync(project) => self.sync_now(project),
            Pending::Catalog => self.catalog_now(),
            Pending::Download => self.download_now(),
            Pending::Updates => self.check_updates_now(),
            Pending::UpgradeSet(rung) => self.upgrade_set_now(rung),
            Pending::UpgradeText(repo) => self.upgrade_text_now(repo),
            Pending::Fix(id) => self.fix_fetch_now(id),
            Pending::ImportVersions => self.import_versions_now(),
        }
    }

    fn drop_pending(&self, then: Pending) {
        match then {
            Pending::Feedback => {
                self.s().fb.try_update(|fb| {
                    if let Some(fb) = fb {
                        feedback::dropped(fb);
                    }
                });
                self.0.sending.set(false);
            }
            // Not now: nothing was sent, and the dialog can be changed again.
            Pending::OpenDoor43 => {
                self.patch_od(|od| od.submit.not_sent());
            }
            Pending::Catalog => self.catalog_dropped(),
            Pending::ImportVersions => self.patch_im(|im| im.versions_not_looked_up()),
            // The sign-in step stays open and sends nothing (tC4: "Not now keeps the dialog").
            // Not now: the download never started, and the guided fix is as it was.
            Pending::Download
            | Pending::Fix(_)
            | Pending::Updates
            | Pending::UpgradeSet(_)
            | Pending::UpgradeText(_)
            | Pending::Link(..)
            | Pending::SignIn { .. }
            | Pending::CheckSignIn
            | Pending::Profile
            | Pending::Share(_)
            | Pending::Sync(_) => {}
        }
    }

    /// The server refused with `NeedsInternet`: the switch went off after the question
    /// (AD25). Ask once more; a second refusal for the same step drops it.
    pub(super) fn ask_again(&self, then: Pending) {
        let asked = self.0.reasked.replace(true);
        let step = self
            .s()
            .layers
            .internet
            .try_update(|flow| flow.needs_internet(then, asked))
            .unwrap_or(NetStep::Nothing);
        self.carry_out(step);
    }

    /// INV-NET-REFUSALS-HANDLED-ONCE (UI-3): the one place `NeedsInternet` and `NeedsSignIn`
    /// become the internet question and the sign-in step, each resuming `pending` after. Any
    /// other refusal comes back for the screen to show. Every variant is named, so a new one is
    /// decided here.
    pub(super) fn net_refusal(&self, pending: Pending, error: ApiError) -> Option<ApiError> {
        match error {
            ApiError::NeedsInternet(_) => {
                self.ask_again(pending);
                None
            }
            ApiError::NeedsSignIn(_) => {
                self.needs_sign_in(pending);
                None
            }
            error @ (ApiError::Version { .. }
            | ApiError::BadRequest { .. }
            | ApiError::StaleScope { .. }
            | ApiError::StaleSession
            | ApiError::ToolNotReady
            | ApiError::RequestReused
            | ApiError::NotSaved(_)
            | ApiError::Refused(_)
            | ApiError::Busy(_)
            | ApiError::Unavailable(_)
            | ApiError::Internal { .. }) => Some(error),
        }
    }

    /// One call that may need the internet or a sign-in: those two refusals are handled by
    /// [`Self::net_refusal`]; a failure below the API stays a transport failure, never a
    /// refusal the server did not make.
    pub(super) async fn run_net<E: Endpoint>(
        &self,
        pending: Pending,
        request: &E,
    ) -> NetEnd<E::Ok> {
        match self.0.runtime.api().call(request).await {
            Ok(ok) => NetEnd::Ok(ok),
            Err(CallError::Api(error)) => match self.net_refusal(pending, error) {
                None => NetEnd::Asked,
                Some(error) => NetEnd::Failed(error),
            },
            Err(CallError::Transport(transport)) => NetEnd::Transport(transport),
            Err(CallError::Dropped) => NetEnd::Dropped,
        }
    }

    /// A page of another site that the person chose (tC4 `openDoor43Page`): an internet task
    /// with no gate, because the browser opens it, not tC4.
    pub fn open_door43_page(&self, kind: LinkKind, url: WebUrl) {
        self.internet_task(Pending::Link(kind, url));
    }

    fn open_link(&self, kind: LinkKind, url: WebUrl) {
        let this = self.clone();
        spawn_local(async move {
            let pending = Pending::Link(kind, url.clone());
            let answer = match kind {
                LinkKind::Help => this.run_net(pending, &HelpLink { url }).await,
                LinkKind::Repo => this.run_net(pending, &RepoLink { url }).await,
            };
            match answer {
                NetEnd::Ok(link) => bridge::open_external(link.url.as_str()),
                NetEnd::Asked | NetEnd::Dropped => {}
                NetEnd::Failed(error) => {
                    console_error(&format!("tc4: a link did not open: {error}"))
                }
                NetEnd::Transport(error) => {
                    console_error(&format!("tc4: a link did not open: {error}"))
                }
            }
        });
    }

    // ---- the app language (M16) -----------------------------------------------------------

    /// Shows `id` across the app now, saving nothing: the App language dialog's preview.
    pub fn preview_app_locale(&self, id: LocaleId) {
        show_locale(id);
    }

    /// Saves `id` as this computer's app language (`prefs/setLocale`, a new request each time,
    /// so a retry after a failure is a new request). On success it is the applied language.
    pub async fn apply_app_locale(&self, id: LocaleId) -> Result<(), CallError> {
        let request = SetLocale {
            request: js::random_request_id(),
            locale: id,
        };
        self.0.runtime.api().call(&request).await?;
        self.s().server.saved_locale.try_set(id);
        show_locale(id);
        Ok(())
    }

    // ---- Feedback (M14) -------------------------------------------------------------------

    /// "Report a problem" in the account menu (tC4 #521).
    pub fn report_problem(&self) {
        if self.0.sending.get() {
            return;
        }
        let form = feedback::report_problem(self.s().fb.get_untracked().as_ref());
        self.open_feedback(form, None);
    }

    /// "Ask for help" on a refusal banner (tC4 #378): a Bug Report with the refusal, or the kept
    /// report of the same refusal.
    pub fn ask_for_help(&self, help: HelpRequest) {
        if self.0.sending.get() {
            return;
        }
        match feedback::ask_for_help(self.s().fb.get_untracked().as_ref(), &help) {
            Opened::Reopen => {
                self.s().layers.dialog.try_set(Some(Dialog::Feedback));
            }
            Opened::New(form) => self.open_feedback(form, help.report),
        }
    }

    /// Opens the dialog at once and asks the server for the attachment, which lands only in
    /// the report it was asked for.
    fn open_feedback(&self, form: FeedbackForm, report: Option<ReportId>) {
        let seq = self.0.compose.begin();
        self.s().fb.try_set(Some(form));
        self.s().layers.dialog.try_set(Some(Dialog::Feedback));
        let this = self.clone();
        spawn_local(async move {
            let attachment = match this.0.runtime.api().call(&Compose { report }).await {
                Ok(draft) => Attachment::Ready(draft.attachment),
                Err(error) => Attachment::Failed(error.to_string()),
            };
            if !this.0.compose.is_current(seq) {
                return;
            }
            this.s().fb.try_update(|fb| {
                if let Some(fb) = fb {
                    fb.attachment = attachment;
                }
            });
        });
    }

    /// A change in one of the dialog's fields. Not while a send runs.
    pub fn patch_fb(&self, patch: impl FnOnce(&mut FeedbackForm)) {
        if self.0.sending.get() {
            return;
        }
        self.s().fb.try_update(|fb| {
            if let Some(fb) = fb {
                patch(fb);
            }
        });
    }

    /// Cancel, Close, Escape: nothing is sent and the report is kept. Not while a send runs.
    pub fn close_feedback(&self) {
        if self.0.sending.get() {
            return;
        }
        self.s().layers.dialog.try_set(None);
    }

    /// Send: the shell sends the report as the dialog shows it, after the internet question.
    /// Without the shell's bridge it says that sending works only in the desktop app, and asks
    /// nothing (tC4's feedback case g).
    pub fn send_feedback(&self) {
        if self.0.sending.get() {
            return;
        }
        let Some(form) = self.s().fb.get_untracked() else {
            return;
        };
        if !feedback::can_send(&form) {
            return;
        }
        self.0.sending.set(true);
        self.s().fb.try_update(|fb| {
            if let Some(fb) = fb {
                fb.sending = true;
                fb.result = None;
            }
        });
        if !bridge::has_feedback_send() {
            self.finish_send(Err(FeedbackRefusal::NoDesktop));
            return;
        }
        self.internet_task(Pending::Feedback);
    }

    fn send_now(&self) {
        let report = self
            .s()
            .fb
            .get_untracked()
            .as_ref()
            .and_then(feedback::shell_report);
        let Some(report) = report else {
            self.finish_send(Err(FeedbackRefusal::Refused));
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            let answer = bridge::feedback_send(&report)
                .await
                .unwrap_or(Err(FeedbackRefusal::NoDesktop));
            this.finish_send(answer);
        });
    }

    fn finish_send(&self, answer: Result<(), FeedbackRefusal>) {
        self.s().fb.try_update(|fb| {
            if let Some(fb) = fb {
                match answer {
                    Ok(()) => feedback::sent(fb),
                    Err(why) => feedback::not_sent(fb, why),
                }
            }
        });
        self.0.sending.set(false);
    }

    // ---- the save indicator (X2) ------------------------------------------------------------

    /// The indicator's Retry. Every failed field is retried, whichever kind it is; the
    /// checkpoint's retry is `session/retryCheckpoint` (D1).
    pub fn retry(&self, which: Retry) {
        match which {
            Retry::Checkpoint => self.retry_checkpoint(),
            Retry::Story | Retry::Note | Retry::Save => {
                self.0.runtime.saves().retry();
                self.retry_align_save();
            }
        }
    }

    /// The checkpoint that failed runs again (D1, tC4 `retryCheckpoint`): its outcome comes back
    /// as the session's event, which clears or keeps the failure.
    fn retry_checkpoint(&self) {
        let Some(at) = self
            .s()
            .project
            .with_untracked(|p| p.as_ref().map(|p| p.at.clone()))
        else {
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            let request = tc_api::session::RetryCheckpoint {
                request: js::random_request_id(),
                at,
            };
            if let Err(error) = this.0.runtime.api().call(&request).await {
                console_warn(&format!("tc4: the checkpoint was not retried: {error}"));
            }
        });
    }

    // ---- Home and its dialogs (B2) ----------------------------------------------------------

    /// Home's read again: the Retry of a failed listing (tC4 `refreshProjects`).
    pub fn refresh_projects(&self) {
        self.0.runtime.cache().refresh(&tc_api::home::List {});
    }

    /// "+ Add a project" (tC4 `openAddProject`): the chooser, in place of any dialog.
    pub fn open_add_project(&self) {
        self.s().layers.dialog.try_set(Some(Dialog::AddProject));
    }

    /// New Bible (tC4 `openNewProject`): the versification list first, then the dialog, which
    /// offers `eng` alone and says so when the list could not be read.
    pub fn open_new_project(&self) {
        let this = self.clone();
        spawn_local(async move {
            let names = this
                .0
                .runtime
                .api()
                .call(&tc_api::home::Versifications {})
                .await
                .map(|view| view.names)
                .map_err(|error| error.to_string());
            let form = NewProjectForm::bible(names);
            this.s()
                .layers
                .dialog
                .try_set(Some(Dialog::NewProject(form)));
        });
    }

    /// New Open Bible Stories (tC4 `openNewObs`).
    pub fn open_new_obs(&self) {
        let form = NewProjectForm::stories();
        self.s().layers.dialog.try_set(Some(Dialog::NewObs(form)));
    }

    /// Import, at its kind step (tC4 `openImport`).
    pub fn open_import(&self) {
        let form = ImportForm::open(js::random_request_id());
        let s = self.s();
        s.import_removed.try_set(false);
        s.layers.dialog.try_set(Some(Dialog::Import(form)));
    }

    /// Add a book, at its method step (tC4 `openAddBook`).
    pub fn open_add_book(&self, card: &HomeCard) {
        let form = AddBookForm::open(card);
        self.s().layers.dialog.try_set(Some(Dialog::AddBook(form)));
    }

    /// A field of New Bible or New Open Bible Stories changed: what the next Create sends is
    /// another request.
    pub fn patch_np(&self, patch: impl FnOnce(&mut NewProjectForm)) {
        self.s().layers.dialog.try_update(|dialog| {
            if let Some(Dialog::NewProject(np) | Dialog::NewObs(np)) = dialog {
                patch(np);
                np.edited();
            }
        });
    }

    pub fn patch_ab(&self, patch: impl FnOnce(&mut AddBookForm)) {
        self.s().layers.dialog.try_update(|dialog| {
            if let Some(Dialog::AddBook(ab)) = dialog {
                patch(ab);
            }
        });
    }

    pub fn patch_im(&self, patch: impl FnOnce(&mut ImportForm)) {
        self.s().layers.dialog.try_update(|dialog| {
            if let Some(Dialog::Import(im)) = dialog {
                patch(im);
            }
        });
    }

    /// Cancel, Close, the ✕ and Escape of Home's dialogs (tC4 `closeModal`), which also ends
    /// the share flow: an answer that comes for it later does nothing.
    pub fn close_modal(&self) {
        let s = self.s();
        // A receive that runs on is kept aside: its end shows at the next Receive dialog.
        if let Some(Dialog::ReceiveWork(form)) = s.layers.dialog.try_update(Option::take).flatten()
            && matches!(form.step, crate::app::exchange::ReceiveStep::Working(_))
        {
            s.rx_last.try_set(Some(form));
        }
        s.layers.dialog.try_set(None);
        self.0.share_flows.begin();
    }

    /// The languages that match `query` (`home/languages`, ranked on the server, offline).
    pub async fn languages(&self, query: String) -> Result<Vec<LanguageSuggestion>, CallError> {
        let view = self
            .0
            .runtime
            .api()
            .call(&tc_api::home::Languages { query })
            .await?;
        Ok(view.suggestions)
    }

    /// A card's Share button (tC4 `shareProject` and `syncProject`): the internet question
    /// first. A shared project syncs (PLAN Q2); a press while a sync runs does nothing.
    pub fn share_project(&self, card: &HomeCard) {
        if self.s().syncing.get_untracked().is_some() {
            return;
        }
        self.internet_task(match card.door43 {
            tc_api::home::Door43Card::Shared { .. } => Pending::Sync(card.project.clone()),
            tc_api::home::Door43Card::Local => Pending::Share(Box::new(ShareProject::of(card))),
        });
    }

    /// "Open a project from Door43", the fourth card of Add a project (tC4 `openDoor43Dialog`).
    pub fn open_door43_dialog(&self) {
        let form = OpenForm::new(js::random_request_id());
        self.s()
            .layers
            .dialog
            .try_set(Some(Dialog::OpenDoor43(form)));
    }

    /// A field of Open from Door43 changed: the next Open sends another body, so it is a new
    /// attempt, even after a lost reply (UI-3).
    pub fn edit_od(&self, change: impl FnOnce(&mut OpenForm)) {
        self.patch_od(|od| {
            change(od);
            od.submit.edited(js::random_request_id());
        });
    }

    /// Changes Open from Door43's form, shown or under the sign-in step that covers it.
    pub fn patch_od(&self, patch: impl FnOnce(&mut OpenForm)) {
        self.s().layers.dialog.try_update(|dialog| {
            if let Some(od) = open_form(dialog) {
                patch(od);
            }
        });
    }

    /// "Open project" (tC4 `openDoor43Run`): the name check is the dialog's and comes first;
    /// then, with the internet off, the question; Not now sends nothing.
    pub fn open_door43_run(&self) {
        let ready = self.s().layers.dialog.try_update(|dialog| {
            let od = open_form(dialog)?;
            od.submit.press()?;
            od.error = None;
            Some(())
        });
        if ready.flatten().is_some() {
            self.internet_task(Pending::OpenDoor43);
        }
    }

    /// The open of the form as it stands: `door43/open`, then its job's end. A refusal shows
    /// in the dialog, and no project was added; a project opened goes to the top of Home.
    fn open_door43_now(&self) {
        let host = self.s().server.account.with_untracked(|a| a.server.clone());
        let form = self.s().layers.dialog.try_update(|dialog| {
            let od = open_form(dialog)?;
            od.submit.press();
            Some((
                od.source(&host)?,
                tc_api::door43::ProjectName::parse(&od.name).ok()?,
                od.submit.request(),
            ))
        });
        let Some(Some((repo, name, attempt))) = form else {
            self.drop_pending(Pending::OpenDoor43);
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            let request = OpenDoor43 {
                request: attempt,
                repo,
                name: name.clone(),
            };
            let ended = match this.run_net(Pending::OpenDoor43, &request).await {
                NetEnd::Ok(job) => this.follow::<OpenDoor43>(job, |_| {}, || true).await,
                // The question or the sign-in step took over: the open after it is a new attempt.
                NetEnd::Asked => {
                    this.patch_od(|od| od.submit.next(js::random_request_id()));
                    return;
                }
                NetEnd::Dropped => {
                    this.patch_od(|od| od.submit.not_sent());
                    return;
                }
                NetEnd::Failed(error) => JobEnd::Failed(error),
                // No usable answer: the next press sends the same request, which the server's
                // reply cache answers if the open was started (A-27).
                NetEnd::Transport(error) => {
                    this.patch_od(|od| od.lost(error.to_string()));
                    return;
                }
            };
            match ended {
                JobEnd::Done(project) => {
                    // Only the open's own dialog closes: a sign-in step a sync put over it stays,
                    // and Open does not come back after it.
                    this.s().layers.dialog.try_update(|dialog| match dialog {
                        Some(Dialog::OpenDoor43(_)) => *dialog = None,
                        Some(Dialog::SignIn { under })
                            if matches!(under.as_deref(), Some(Dialog::OpenDoor43(_))) =>
                        {
                            *under = None;
                        }
                        _ => {}
                    });
                    this.s().import_toast.try_set(Some(ImportToast {
                        message: crate::i18n::t(
                            crate::i18n::keys::open_door43::DONE,
                            &[("name", &name.as_str())],
                        ),
                    }));
                    this.s().imported_repo.try_set(Some(project));
                    this.refresh_projects();
                }
                // The open ended without a word on how (cancelled, or the server restarted):
                // no failure is shown; the project list says whether it took effect.
                JobEnd::Cancelled | JobEnd::Lost | JobEnd::Left => {
                    this.patch_od(|od| od.submit.next(js::random_request_id()));
                    this.refresh_projects();
                }
                JobEnd::Failed(error) => match this.net_refusal(Pending::OpenDoor43, error) {
                    None => this.patch_od(|od| od.submit.next(js::random_request_id())),
                    Some(ApiError::Refused(refusal)) => {
                        let reason = crate::i18n::refusal_text(&refusal);
                        this.open_refused(refusal.code(), reason);
                    }
                    Some(other) => this.open_refused(None, other.to_string()),
                },
            }
        });
    }

    fn open_refused(&self, code: Option<&str>, reason: String) {
        self.patch_od(|od| od.refused(code, reason, js::random_request_id()));
    }

    pub fn dismiss_import_toast(&self) {
        self.s().import_toast.try_set(None);
    }

    // ---- Sync with Door43 (B10; the sync itself is E1a's) ------------------------------------

    /// tC4's `syncRun` after the question: one sync of `project`, its result on Home. Without a
    /// sign-in the sign-in step comes first, and the sync runs after it.
    fn sync_now(&self, project: ProjectRef) {
        let s = self.s();
        if s.syncing.get_untracked().is_some() {
            return;
        }
        s.syncing.try_set(Some(project.clone()));
        s.sync_result.try_set(None);
        let Some(attempt) = self
            .sync_attempt(&project, |submit| submit.press())
            .flatten()
        else {
            s.syncing.try_set(None);
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            let request = SyncProject {
                request: attempt,
                project: project.clone(),
            };
            let started = this.run_net(Pending::Sync(project.clone()), &request).await;
            let ended = match started {
                NetEnd::Ok(job) => this.follow::<SyncProject>(job, |_| {}, || true).await,
                NetEnd::Failed(error) => JobEnd::Failed(error),
                NetEnd::Asked => {
                    this.sync_attempt(&project, |submit| submit.next(js::random_request_id()));
                    this.s().syncing.try_set(None);
                    return;
                }
                NetEnd::Dropped => {
                    this.sync_attempt(&project, Submit::not_sent);
                    this.s().syncing.try_set(None);
                    return;
                }
                // No usable answer: the next press is a new sync. Resending this request could,
                // much later, replay a sync that ran before the translator's newer work and show
                // it as done (review, races lens); a second sync of the same work is harmless.
                NetEnd::Transport(_) => {
                    this.sync_attempt(&project, |submit| submit.next(js::random_request_id()));
                    this.s().syncing.try_set(None);
                    let end = SyncEnd::Unreached;
                    this.s()
                        .sync_result
                        .try_set(Some(SyncToast { project, end }));
                    return;
                }
            };
            // The button comes back whatever the sync did, and the next press is a new attempt.
            this.s().syncing.try_set(None);
            this.sync_attempt(&project, |submit| submit.next(js::random_request_id()));
            let end = match ended {
                JobEnd::Failed(error) => {
                    match this.net_refusal(Pending::Sync(project.clone()), error) {
                        None => return,
                        Some(error) => SyncEnd::Failed(error),
                    }
                }
                JobEnd::Done(view) => SyncEnd::Synced(view),
                // The sync ended without a word on how (cancelled, or the server restarted): no
                // failure is shown; the project card says whether it took effect.
                JobEnd::Cancelled | JobEnd::Lost | JobEnd::Left => {
                    return this.refresh_projects();
                }
            };
            this.s()
                .sync_result
                .try_set(Some(SyncToast { project, end }));
        });
    }

    /// `f` on `project`'s Sync attempt, made with a fresh request the first time.
    fn sync_attempt<R>(&self, project: &ProjectRef, f: impl FnOnce(&mut Submit) -> R) -> Option<R> {
        let mut attempts = self.0.sync_attempts.try_borrow_mut().ok()?;
        if !attempts.iter().any(|(p, _)| p == project) {
            attempts.push((project.clone(), Submit::new(js::random_request_id())));
        }
        let submit = attempts.iter_mut().find(|(p, _)| p == project)?;
        Some(f(&mut submit.1))
    }

    pub fn dismiss_sync_result(&self) {
        self.s().sync_result.try_set(None);
    }

    /// The Home card's link to its last sync: that sync's toast again (owner ruling H5).
    pub fn show_last_sync(&self, project: ProjectRef, view: tc_api::jobs::SyncResultView) {
        self.s().sync_result.try_set(Some(SyncToast {
            project,
            end: SyncEnd::Synced(view),
        }));
    }

    /// The toast's Review: the synced project's conflict review. The toast stays until the
    /// review is open, so a failed open keeps its links (tC4 #724).
    pub fn sync_review(&self) {
        if let Some(toast) = self.s().sync_result.get_untracked() {
            let tab = toast.review_tab();
            self.open_project_conflicts(toast.project, None, tab);
        }
    }

    /// The toast's Download (D88 point 6): B3 opens the project and B9a its Source texts,
    /// which offer the missing versions; the toast stays until they are open (tC4 #724).
    pub fn sync_download(&self) {}

    // ---- Share on Door43 (B9b; the run is E1b's) --------------------------------------------

    /// tC4's `startShare` after the question. With a sign-in held, the dialog opens at once on
    /// its where-it-goes step and reads the organizations; without one, the organizations' read
    /// resumes a kept sign-in or answers `NeedsSignIn`, and the sign-in step comes first, then
    /// this step again.
    fn share_now(&self, project: ShareProject) {
        let flow = self.0.share_flows.begin();
        let opened = self.s().server.account.with_untracked(|a| a.user.is_some());
        if opened {
            self.open_share(&project);
        }
        let this = self.clone();
        spawn_local(async move {
            let request = Orgs {
                language: project.language.clone(),
            };
            let reply = this.0.runtime.api().call(&request).await;
            if !this.0.share_flows.is_current(flow) {
                return;
            }
            let dialog = this
                .s()
                .layers
                .dialog
                .with_untracked(|dialog| dialog.as_ref().map(Dialog::kind));
            let reply = match reply {
                Err(CallError::Api(error)) => {
                    match this.net_refusal(Pending::Share(Box::new(project.clone())), error) {
                        // The internet question or the sign-in step takes the dialog's place.
                        None => return this.end_share(),
                        Some(error) => Err(CallError::Api(error)),
                    }
                }
                reply => reply,
            };
            // The answer goes to this flow's dialog only; one that opened meanwhile over Home
            // takes its place only when nothing else is open.
            match (opened, dialog) {
                (true, Some(DialogKind::Share)) => {}
                (false, None) => this.open_share(&project),
                _ => return,
            }
            let read = reply.map_err(|error| share_problem(&error));
            this.patch_sh(|sh| sh.choices_read(read));
        });
    }

    fn open_share(&self, project: &ShareProject) {
        let form = ShareForm::open(project.clone());
        self.s().layers.dialog.try_set(Some(Dialog::Share(form)));
    }

    /// The share dialog closes for a step that takes its place.
    fn end_share(&self) {
        self.s().layers.dialog.try_update(|dialog| {
            if matches!(dialog, Some(Dialog::Share(_))) {
                *dialog = None;
            }
        });
    }

    pub fn patch_sh(&self, patch: impl FnOnce(&mut ShareForm)) {
        self.s().layers.dialog.try_update(|dialog| {
            if let Some(Dialog::Share(sh)) = dialog {
                patch(sh);
            }
        });
    }

    /// Next and Back (tC4 `shareStep`).
    pub fn share_step(&self, step: ShareStep) {
        self.patch_sh(|sh| sh.go(step));
    }

    pub fn pick_share_target(&self, target: Target) {
        self.patch_sh(|sh| sh.pick(target));
    }

    /// The check step's Share: the run is unit E1b's (`door43/share`).
    pub fn share_run(&self) {}

    /// "Change" (D86 point 7): signs out, then the sign-in step, then the dialog again.
    pub fn change_sign_in(&self) {
        let Some(project) = self
            .s()
            .layers
            .dialog
            .with_untracked(|dialog| match dialog {
                Some(Dialog::Share(sh)) => Some(sh.project.clone()),
                _ => None,
            })
        else {
            return;
        };
        self.0.share_flows.begin();
        self.end_share();
        let this = self.clone();
        spawn_local(async move {
            let request = SignOut {
                request: js::random_request_id(),
            };
            match this.0.runtime.api().call(&request).await {
                Ok(account) => {
                    this.s().server.account.try_set(account);
                }
                Err(error) => console_warn(&format!("tc4: sign-out: {error}")),
            }
            this.needs_sign_in(Pending::Share(Box::new(project)));
        });
    }

    // ---- the Door43 account (A8) ------------------------------------------------------------

    /// "Sign in to Door43" in the account menu: the sign-in step, a task of its own.
    pub fn open_sign_in(&self) {
        self.open_sign_in_for(None);
    }

    /// A step that met `NeedsSignIn` (the flow units): the sign-in step, then that step again
    /// once signed in (6.7).
    pub fn needs_sign_in(&self, then: Pending) {
        self.open_sign_in_for(Some(then));
    }

    fn open_sign_in_for(&self, then: Option<Pending>) {
        let flow = self.0.sign_in_flows.get() + 1;
        self.0.sign_in_flows.set(flow);
        self.s().sign_in.try_update(|step| step.open(flow, then));
        // The step covers the dialog under it, which shows again after it with its form as the
        // person left it: Open from Door43's open goes on once signed in, and a form typed in
        // while a sync asked for the sign-in is the person's. A step over a step keeps the one
        // dialog under both.
        self.s().layers.dialog.try_update(|dialog| {
            let under = match dialog.take() {
                Some(Dialog::SignIn { under }) => under,
                other => other.map(Box::new),
            };
            *dialog = Some(Dialog::SignIn { under });
        });
    }

    /// A change in one of the step's fields.
    pub fn patch_sign_in(&self, change: impl FnOnce(&mut SignInForm)) {
        self.s().sign_in.try_update(|step| step.edit(change));
    }

    /// Sign in (tC4's `submitSignIn`): asks the internet question while the switch is off,
    /// then sends. Nothing while a sign-in is on its way.
    pub fn submit_sign_in(&self) {
        let flow = self
            .s()
            .sign_in
            .with_untracked(|step| (!step.busy()).then(|| step.flow()).flatten());
        if let Some(flow) = flow {
            self.internet_task(Pending::SignIn { flow });
        }
    }

    fn send_sign_in(&self, flow: u64) {
        let keychain = self.s().server.account.with_untracked(|a| a.keychain);
        let request = js::random_request_id();
        let Some(req) = self
            .s()
            .sign_in
            .try_update(|step| step.send(flow, request, keychain))
            .flatten()
        else {
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            let reply = this.0.runtime.api().call(&req).await;
            if let Ok(view) = &reply
                && view.user.is_some()
                && this.s().sign_in.with_untracked(|step| step.flow()) == Some(flow)
            {
                this.s().server.account.try_set(view.clone());
            }
            let next = this
                .s()
                .sign_in
                .try_update(|step| step.answered(flow, request, reply))
                .unwrap_or(SignInNext::Nothing);
            match next {
                SignInNext::SignedIn(then) => {
                    // The dialog it covered shows again, for the step that goes on now.
                    this.s().layers.dialog.try_update(|dialog| {
                        if let Some(Dialog::SignIn { under }) = dialog {
                            *dialog = under.take().map(|under| *under);
                        }
                    });
                    if let Some(then) = then {
                        this.internet_task(then);
                    }
                }
                SignInNext::AskAgain(then) => this.ask_again(then),
                SignInNext::Nothing => {}
            }
        });
    }

    /// Cancel, Escape, the ✕ and the scrim: the step closes at once and the dialog it covered
    /// shows again as the person left it, and a sign-in on its way is cancelled on the server,
    /// which then signs nobody in whenever the cancel arrives.
    pub fn cancel_sign_in(&self) {
        let sending = self.s().sign_in.try_update(SignInFlow::cancel).flatten();
        self.s().layers.dialog.try_update(|dialog| {
            if let Some(Dialog::SignIn { under }) = dialog {
                *dialog = under.take().map(|under| *under);
            }
        });
        if let Some(sign_in) = sending {
            let this = self.clone();
            spawn_local(async move {
                let request = CancelSignIn {
                    request: js::random_request_id(),
                    sign_in,
                };
                if let Err(error) = this.0.runtime.api().call(&request).await {
                    console_warn(&format!(
                        "tc4: the cancel of a sign-in did not arrive: {error}"
                    ));
                }
            });
        }
    }

    /// "Sign-in saved on this computer": Door43 checks the kept token, after the question
    /// (tC4's `checkSavedSignIn`).
    pub fn check_saved_sign_in(&self) {
        self.internet_task(Pending::CheckSignIn);
    }

    fn check_now(&self) {
        let this = self.clone();
        spawn_local(async move {
            let request = Resume {
                request: js::random_request_id(),
            };
            match this.run_net(Pending::CheckSignIn, &request).await {
                NetEnd::Ok(resumed) => {
                    this.s().server.account.try_set(resumed.account);
                }
                NetEnd::Asked | NetEnd::Dropped => {}
                NetEnd::Failed(error) => {
                    console_warn(&format!("tc4: the saved sign-in was not checked: {error}"))
                }
                NetEnd::Transport(error) => {
                    console_warn(&format!("tc4: the saved sign-in was not checked: {error}"))
                }
            }
        });
    }

    /// "Open my page on Door43": the server gives the address, after the question; the system
    /// browser opens it.
    pub fn open_profile(&self) {
        self.internet_task(Pending::Profile);
    }

    fn profile_now(&self) {
        let this = self.clone();
        spawn_local(async move {
            match this.run_net(Pending::Profile, &ProfileLink {}).await {
                NetEnd::Ok(link) => bridge::open_external(link.url.as_str()),
                NetEnd::Asked | NetEnd::Dropped => {}
                NetEnd::Failed(error) => {
                    console_warn(&format!("tc4: the Door43 page did not open: {error}"))
                }
                NetEnd::Transport(error) => {
                    console_warn(&format!("tc4: the Door43 page did not open: {error}"))
                }
            }
        });
    }

    /// "Sign out of Door43": the token leaves memory and the keychain; nothing is sent to
    /// Door43.
    pub fn sign_out(&self) {
        let this = self.clone();
        spawn_local(async move {
            let request = SignOut {
                request: js::random_request_id(),
            };
            match this.0.runtime.api().call(&request).await {
                Ok(account) => {
                    this.s().server.account.try_set(account);
                }
                Err(error) => console_warn(&format!("tc4: sign-out: {error}")),
            }
        });
    }
}

fn console_warn(message: &str) {
    web_sys::console::warn_1(&JsValue::from_str(message));
}

fn console_error(message: &str) {
    web_sys::console::error_1(&JsValue::from_str(message));
}

/// Why the organizations could not be read, in the shown language.
fn share_problem(error: &CallError) -> String {
    match error {
        CallError::Api(ApiError::Refused(refusal)) => crate::i18n::refusal_text(refusal),
        other => other.to_string(),
    }
}
