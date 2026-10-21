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

use leptos::prelude::*;
use send_wrapper::SendWrapper;
use tc_api::ApiError;
use tc_api::account::{
    CancelSignIn, HelpLink, Orgs, ProfileLink, RepoLink, Resume, SignOut, WebUrl,
};
use tc_api::door43::{OpenDoor43, SyncProject};
use tc_api::feedback::Compose;
use tc_api::home::{HomeCard, LanguageSuggestion};
use tc_api::ids::{LocaleId, ProjectRef, ReportId};
use tc_api::jobs::{JobOutcome, JobState};
use tc_api::net::Set as NetSet;
use tc_api::prefs::SetLocale;
use tc_api::refusal::FeedbackRefusal;
use tc_api::refusal::Message;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;

use super::{Runtime, ServerState, bridge, js};
use crate::app::account::{SignInFlow, SignInForm, SignInNext};
use crate::app::client::CallError;
use crate::app::feedback::{self, Attachment, FeedbackForm, HelpRequest, Opened};
use crate::app::forms::{AddBookForm, ImportForm, ImportKind, NewProjectForm};
use crate::app::internet::{InternetFlow, LinkKind, NetStep, Pending, SwitchRequests, SwitchStep};
use crate::app::open_door43::OpenForm;
use crate::app::save::indicator::Retry;
use crate::app::share::{ShareForm, ShareProject, ShareStep, Target};
use crate::app::shell::{CurrentProject, Editing, View};
use crate::app::sync::{SyncEnd, SyncToast};
use crate::i18n::show_locale;

/// The dialogs `s.modal` names (tC4 `tC4:src/state.jsx:476`). The units that port the others
/// add theirs: ProjectSettings, SourceTexts (B9a), GuidedFix (B5a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialog {
    Feedback,
    /// The Door43 sign-in step (M12), which unit A8 draws.
    SignIn,
    /// "+ Add a project" (M1).
    AddProject,
    /// New Bible (M2); its form is `np`.
    NewProject,
    /// New Open Bible Stories (M3); its form is `np`.
    NewObs,
    /// Add a book (M4); its form is `ab`.
    AddBook,
    /// Import (M5); its form is `im`.
    Import,
    /// Open a project from Door43 (M6); its form is `od`.
    OpenDoor43,
    /// Share on Door43; its form is `sh`.
    Share,
    /// The guided fix (M11, B5a); its state is `c.fix`.
    GuidedFix,
    /// Project settings (M7); its form is `st`.
    Settings,
    /// Get source texts (M8); its form is `src`.
    Sources,
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
    /// New Bible's and New Open Bible Stories' form (tC4 `np`).
    pub np: ArcRwSignal<Option<NewProjectForm>>,
    /// Add a book's form (tC4 `ab`).
    pub ab: ArcRwSignal<Option<AddBookForm>>,
    /// Import's form (tC4 `im`).
    pub im: ArcRwSignal<Option<ImportForm>>,
    /// The share dialog's form (tC4 `sh`, B9b).
    pub sh: ArcRwSignal<Option<ShareForm>>,
    /// Open a project from Door43's form (tC4 `od`, B9b).
    pub od: ArcRwSignal<Option<OpenForm>>,
    /// Project settings' form (tC4 `st`, B9a).
    pub st: ArcRwSignal<Option<crate::app::sources::SettingsForm>>,
    /// Get source texts' form (tC4 `src`, B9a), kept across the dialog's openings as tC4
    /// keeps it.
    pub src: ArcRwSignal<crate::app::sources::SourcesForm>,
    /// The release upgrades of Get source texts and their confirmation (tC4 `upgrade`, C8).
    pub up: ArcRwSignal<crate::app::upgrades::UpgradeState>,
    /// The change of checking language and its confirmation (tC4 `gatewayBusy`,
    /// `gatewayPreview`, `gatewayError`, C8).
    pub gw: ArcRwSignal<crate::app::upgrades::GatewayState>,
    /// A refused open, on Home (tC4 `bookError`; B3 sets it).
    pub book_error: ArcRwSignal<Option<HomeBanner>>,
    /// The checkpoint of the project just left failed (tC4 `commitError`; D1 sets it).
    pub commit_error: ArcRwSignal<Option<HomeBanner>>,
    /// The project a sync runs for (tC4 `syncing`).
    pub syncing: ArcRwSignal<Option<ProjectRef>>,
    /// The result of the last sync, which Home shows (tC4 `syncResult`). Never stored.
    pub sync_result: ArcRwSignal<Option<SyncToast>>,
    /// The toast of a finished import (F1 sets it).
    pub import_toast: ArcRwSignal<Option<ImportToast>>,
    /// The project an import of this page made: its card carries "Imported" (F1 sets it).
    pub imported_repo: ArcRwSignal<Option<ProjectRef>>,
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
            np: ArcRwSignal::new(None),
            ab: ArcRwSignal::new(None),
            im: ArcRwSignal::new(None),
            sh: ArcRwSignal::new(None),
            od: ArcRwSignal::new(None),
            st: ArcRwSignal::new(None),
            src: ArcRwSignal::new(crate::app::sources::SourcesForm::default()),
            up: ArcRwSignal::new(crate::app::upgrades::UpgradeState::default()),
            gw: ArcRwSignal::new(crate::app::upgrades::GatewayState::default()),
            book_error: ArcRwSignal::new(None),
            commit_error: ArcRwSignal::new(None),
            syncing: ArcRwSignal::new(None),
            sync_result: ArcRwSignal::new(None),
            import_toast: ArcRwSignal::new(None),
            imported_repo: ArcRwSignal::new(None),
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
    compose: Cell<u64>,
    /// The step that ran after a question already met `NeedsInternet` once (AD25).
    reasked: Cell<bool>,
    /// The `net/set` requests, one at a time.
    switch: Cell<SwitchRequests>,
    /// Counts the openings of the sign-in step, so a reply lands only in the one it came from.
    sign_in_flows: Cell<u64>,
    /// Counts the share flows (tC4's `startShareFlow`): a new Share or a closed dialog ends the
    /// one before, whose late answers then do nothing.
    share_flows: Cell<u64>,
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
            compose: Cell::new(0),
            reasked: Cell::new(false),
            switch: Cell::new(SwitchRequests::default()),
            sign_in_flows: Cell::new(0),
            share_flows: Cell::new(0),
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
            if !this.0.runtime.saves().gate().await {
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
            if !this.0.runtime.saves().gate().await {
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
                self.s().od.try_update(|od| {
                    if let Some(od) = od {
                        od.busy = false;
                    }
                });
            }
            Pending::Catalog => self.catalog_dropped(),
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

    /// A page of another site that the person chose (tC4 `openDoor43Page`): an internet task
    /// with no gate, because the browser opens it, not tC4.
    pub fn open_door43_page(&self, kind: LinkKind, url: WebUrl) {
        self.internet_task(Pending::Link(kind, url));
    }

    fn open_link(&self, kind: LinkKind, url: WebUrl) {
        let this = self.clone();
        spawn_local(async move {
            let api = this.0.runtime.api();
            let answer = match kind {
                LinkKind::Help => api.call(&HelpLink { url: url.clone() }).await,
                LinkKind::Repo => api.call(&RepoLink { url: url.clone() }).await,
            };
            match answer {
                Ok(link) => bridge::open_external(link.url.as_str()),
                Err(CallError::Api(ApiError::NeedsInternet(_))) => {
                    this.ask_again(Pending::Link(kind, url));
                }
                Err(error) => console_error(&format!("tc4: a link did not open: {error}")),
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
        let seq = self.0.compose.get() + 1;
        self.0.compose.set(seq);
        self.s().fb.try_set(Some(form));
        self.s().layers.dialog.try_set(Some(Dialog::Feedback));
        let this = self.clone();
        spawn_local(async move {
            let attachment = match this.0.runtime.api().call(&Compose { report }).await {
                Ok(draft) => Attachment::Ready(draft.attachment),
                Err(error) => Attachment::Failed(error.to_string()),
            };
            if this.0.compose.get() != seq {
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
    /// checkpoint's retry is D1's `session/retryCheckpoint`, which no phase-0 state can reach.
    pub fn retry(&self, which: Retry) {
        match which {
            Retry::Checkpoint => {}
            Retry::Story | Retry::Note | Retry::Save => {
                self.0.runtime.saves().retry();
                self.retry_align_save();
            }
        }
    }

    // ---- Home and its dialogs (B2) ----------------------------------------------------------

    /// Home's read again: the Retry of a failed listing (tC4 `refreshProjects`).
    pub fn refresh_projects(&self) {
        self.0.runtime.cache().refresh(&tc_api::home::List {});
    }

    /// "+ Add a project" (tC4 `openAddProject`): the chooser, in place of any dialog.
    pub fn open_add_project(&self) {
        let s = self.s();
        s.np.try_set(None);
        s.im.try_set(None);
        s.layers.dialog.try_set(Some(Dialog::AddProject));
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
            let s = this.s();
            s.np.try_set(Some(NewProjectForm::bible(names)));
            s.layers.dialog.try_set(Some(Dialog::NewProject));
        });
    }

    /// New Open Bible Stories (tC4 `openNewObs`).
    pub fn open_new_obs(&self) {
        let s = self.s();
        s.np.try_set(Some(NewProjectForm::stories()));
        s.layers.dialog.try_set(Some(Dialog::NewObs));
    }

    /// Import, at its kind step (tC4 `openImport`).
    pub fn open_import(&self) {
        let s = self.s();
        s.im.try_set(Some(ImportForm::open()));
        s.layers.dialog.try_set(Some(Dialog::Import));
    }

    /// Add a book, at its method step (tC4 `openAddBook`).
    pub fn open_add_book(&self, card: &HomeCard) {
        let s = self.s();
        s.np.try_set(None);
        s.ab.try_set(Some(AddBookForm::open(card)));
        s.layers.dialog.try_set(Some(Dialog::AddBook));
    }

    /// A field of New Bible or New Open Bible Stories changed: what the next Create sends is
    /// another request.
    pub fn patch_np(&self, patch: impl FnOnce(&mut NewProjectForm)) {
        self.s().np.try_update(|np| {
            if let Some(np) = np {
                patch(np);
                np.edited();
            }
        });
    }

    pub fn patch_ab(&self, patch: impl FnOnce(&mut AddBookForm)) {
        self.s().ab.try_update(|ab| {
            if let Some(ab) = ab {
                patch(ab);
            }
        });
    }

    pub fn patch_im(&self, patch: impl FnOnce(&mut ImportForm)) {
        self.s().im.try_update(|im| {
            if let Some(im) = im {
                patch(im);
            }
        });
    }

    /// Cancel, Close, the ✕ and Escape of Home's dialogs (tC4 `closeModal`), which also ends
    /// the share flow: an answer that comes for it later does nothing.
    pub fn close_modal(&self) {
        let s = self.s();
        s.layers.dialog.try_set(None);
        s.np.try_set(None);
        s.ab.try_set(None);
        s.im.try_set(None);
        s.sh.try_set(None);
        s.od.try_set(None);
        s.c.fix.try_set(None);
        s.st.try_set(None);
        self.0.share_flows.set(self.0.share_flows.get() + 1);
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

    /// Import's kind step: F1 draws the files step it leads to.
    pub fn import_pick_kind(&self, _kind: ImportKind) {}

    /// "Open a project from Door43", the fourth card of Add a project (tC4 `openDoor43Dialog`).
    pub fn open_door43_dialog(&self) {
        let s = self.s();
        s.od.try_set(Some(OpenForm::default()));
        s.layers.dialog.try_set(Some(Dialog::OpenDoor43));
    }

    pub fn patch_od(&self, patch: impl FnOnce(&mut OpenForm)) {
        self.s().od.try_update(|od| {
            if let Some(od) = od {
                patch(od);
            }
        });
    }

    /// "Open project" (tC4 `openDoor43Run`): the name check is the dialog's and comes first;
    /// then, with the internet off, the question; Not now sends nothing.
    pub fn open_door43_run(&self) {
        let ready = self.s().od.try_update(|od| {
            let od = od.as_mut()?;
            if od.busy {
                return None;
            }
            od.busy = true;
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
        let Some(Some((repo, name))) = self.s().od.with_untracked(|od| {
            od.as_ref().map(|od| {
                Some((
                    od.source(&host)?,
                    tc_api::door43::ProjectName::parse(&od.name).ok()?,
                ))
            })
        }) else {
            self.drop_pending(Pending::OpenDoor43);
            return;
        };
        // After a sign-in step took its place, the dialog shows again for the open's end.
        self.patch_od(|od| od.busy = true);
        self.s().layers.dialog.try_set(Some(Dialog::OpenDoor43));
        let this = self.clone();
        spawn_local(async move {
            let request = OpenDoor43 {
                request: js::random_request_id(),
                repo,
                name: name.clone(),
            };
            let ended = match this.0.runtime.api().call(&request).await {
                Ok(job) => this.job_end(job).await,
                Err(CallError::Api(error)) => Err(error),
                Err(other) => Err(ApiError::BadRequest {
                    field: "door43/open".into(),
                    reason: other.to_string(),
                }),
            };
            match ended {
                Ok(JobOutcome::Created { project }) => {
                    this.close_modal();
                    this.s().import_toast.try_set(Some(ImportToast {
                        message: crate::i18n::t(
                            crate::i18n::keys::open_door43::DONE,
                            &[("name", &name.as_str())],
                        ),
                    }));
                    this.s().imported_repo.try_set(Some(project));
                    this.refresh_projects();
                }
                Ok(_) => this.open_refused(None, String::new()),
                Err(ApiError::NeedsInternet(_)) => this.ask_again(Pending::OpenDoor43),
                Err(ApiError::NeedsSignIn(_)) => {
                    this.patch_od(|od| od.busy = false);
                    this.needs_sign_in(Pending::OpenDoor43);
                }
                Err(ApiError::Refused(refusal)) => {
                    let reason = refusal
                        .vars()
                        .into_iter()
                        .map(|(_, v)| v)
                        .collect::<Vec<_>>()
                        .join(" ");
                    this.open_refused(refusal.code(), reason);
                }
                Err(other) => this.open_refused(None, other.to_string()),
            }
        });
    }

    fn open_refused(&self, code: Option<&str>, reason: String) {
        self.patch_od(|od| od.refused(code, reason));
    }

    /// The end of a job this page started: its outcome, or why it did not finish. Its state
    /// comes with the event stream, which the runtime reads again after a gap; a job the page
    /// has not heard of for a second is asked for, and one the server no longer knows (a new
    /// server run) ended without an outcome.
    async fn job_end(&self, job: tc_api::ids::JobId) -> Result<JobOutcome, ApiError> {
        let mut unheard = 0u32;
        loop {
            let mut state = self
                .s()
                .server
                .jobs
                .with_untracked(|jobs| jobs.get(&job).map(|view| view.state.clone()));
            if state.is_none() {
                unheard += 1;
                if unheard.is_multiple_of(20) {
                    match self.0.runtime.api().call(&tc_api::jobs::Get { job }).await {
                        Ok(Some(view)) => state = Some(view.state),
                        Ok(None) => {
                            return Err(ApiError::Unavailable(tc_api::error::Feature::Sync));
                        }
                        Err(_) => {}
                    }
                }
            }
            match state {
                Some(JobState::Done(outcome)) => return Ok(outcome),
                Some(JobState::Failed(error)) => return Err(error),
                Some(JobState::Cancelled) => {
                    return Err(ApiError::Unavailable(tc_api::error::Feature::Sync));
                }
                Some(JobState::Queued | JobState::Running(_) | JobState::Cancelling) | None => {
                    js::sleep(50).await;
                }
            }
        }
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
        let this = self.clone();
        spawn_local(async move {
            let request = SyncProject {
                request: js::random_request_id(),
                project: project.clone(),
            };
            let ended = match this.0.runtime.api().call(&request).await {
                Ok(job) => this.job_end(job).await,
                Err(CallError::Api(error)) => Err(error),
                Err(other) => Err(ApiError::BadRequest {
                    field: "door43/sync".into(),
                    reason: other.to_string(),
                }),
            };
            // The button comes back whatever the sync did.
            this.s().syncing.try_set(None);
            let end = match ended {
                Err(ApiError::NeedsInternet(_)) => return this.ask_again(Pending::Sync(project)),
                Err(ApiError::NeedsSignIn(_)) => {
                    return this.needs_sign_in(Pending::Sync(project));
                }
                Ok(JobOutcome::Synced(view)) => SyncEnd::Synced(view),
                Ok(_) => SyncEnd::Failed(ApiError::Unavailable(tc_api::error::Feature::Sync)),
                Err(error) => SyncEnd::Failed(error),
            };
            this.s()
                .sync_result
                .try_set(Some(SyncToast { project, end }));
        });
    }

    pub fn dismiss_sync_result(&self) {
        self.s().sync_result.try_set(None);
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
        let flow = self.0.share_flows.get() + 1;
        self.0.share_flows.set(flow);
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
            if this.0.share_flows.get() != flow {
                return;
            }
            let dialog = this.s().layers.dialog.get_untracked();
            match reply {
                Err(CallError::Api(ApiError::NeedsInternet(_))) => {
                    this.end_share();
                    this.ask_again(Pending::Share(Box::new(project)));
                }
                Err(CallError::Api(ApiError::NeedsSignIn(_))) => {
                    this.end_share();
                    this.needs_sign_in(Pending::Share(Box::new(project)));
                }
                reply => {
                    // The answer goes to this flow's dialog only; one that opened meanwhile
                    // over Home takes its place only when nothing else is open.
                    match (opened, dialog) {
                        (true, Some(Dialog::Share)) => {}
                        (false, None) => this.open_share(&project),
                        _ => return,
                    }
                    let read = reply.map_err(|error| share_problem(&error));
                    this.patch_sh(|sh| sh.choices_read(read));
                }
            }
        });
    }

    fn open_share(&self, project: &ShareProject) {
        let s = self.s();
        s.sh.try_set(Some(ShareForm::open(project.clone())));
        s.layers.dialog.try_set(Some(Dialog::Share));
    }

    /// The share dialog closes for a step that takes its place.
    fn end_share(&self) {
        let s = self.s();
        s.layers.dialog.try_update(|dialog| {
            if *dialog == Some(Dialog::Share) {
                *dialog = None;
            }
        });
        s.sh.try_set(None);
    }

    pub fn patch_sh(&self, patch: impl FnOnce(&mut ShareForm)) {
        self.s().sh.try_update(|sh| {
            if let Some(sh) = sh {
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
            .sh
            .with_untracked(|sh| sh.as_ref().map(|sh| sh.project.clone()))
        else {
            return;
        };
        self.0.share_flows.set(self.0.share_flows.get() + 1);
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
        self.s().layers.dialog.try_set(Some(Dialog::SignIn));
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
                    this.s().layers.dialog.try_update(|dialog| {
                        if *dialog == Some(Dialog::SignIn) {
                            *dialog = None;
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

    /// Cancel, Escape, the ✕ and the scrim: the step closes at once, and a sign-in on its way
    /// is cancelled on the server, which then signs nobody in whenever the cancel arrives.
    pub fn cancel_sign_in(&self) {
        let sending = self.s().sign_in.try_update(SignInFlow::cancel).flatten();
        self.s().layers.dialog.try_update(|dialog| {
            if *dialog == Some(Dialog::SignIn) {
                *dialog = None;
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
            match this.0.runtime.api().call(&request).await {
                Ok(resumed) => {
                    this.s().server.account.try_set(resumed.account);
                }
                Err(CallError::Api(ApiError::NeedsInternet(_))) => {
                    this.ask_again(Pending::CheckSignIn);
                }
                Err(error) => {
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
            match this.0.runtime.api().call(&ProfileLink {}).await {
                Ok(link) => bridge::open_external(link.url.as_str()),
                Err(CallError::Api(ApiError::NeedsInternet(_))) => {
                    this.ask_again(Pending::Profile);
                }
                Err(CallError::Api(ApiError::NeedsSignIn(_))) => {
                    this.needs_sign_in(Pending::Profile);
                }
                Err(error) => console_warn(&format!("tc4: the Door43 page did not open: {error}")),
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
        CallError::Api(ApiError::Refused(refusal)) => crate::i18n::t_dyn(refusal.key(), &[], None),
        other => other.to_string(),
    }
}
