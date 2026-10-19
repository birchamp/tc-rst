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
use tc_api::account::{CancelSignIn, HelpLink, ProfileLink, Resume, SignOut, WebUrl};
use tc_api::feedback::Compose;
use tc_api::ids::{LocaleId, ReportId};
use tc_api::net::Set as NetSet;
use tc_api::prefs::SetLocale;
use tc_api::refusal::FeedbackRefusal;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;

use super::{Runtime, ServerState, bridge, js};
use crate::app::account::{SignInFlow, SignInForm, SignInNext};
use crate::app::client::CallError;
use crate::app::feedback::{self, Attachment, FeedbackForm, HelpRequest, Opened};
use crate::app::internet::{InternetFlow, LinkKind, NetStep, Pending, SwitchRequests, SwitchStep};
use crate::app::save::indicator::Retry;
use crate::app::shell::{CurrentProject, Editing, View};
use crate::i18n::show_locale;

/// The dialogs `s.modal` names (tC4 `tC4:src/state.jsx:476`). The units that port the others
/// add theirs: AddProject, NewBible, NewObs, AddBook, Import (B2, C6, F1), OpenFromDoor43,
/// Share (B9b, E1b), ProjectSettings, SourceTexts (B9a), GuidedFix (B5a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialog {
    Feedback,
    /// The Door43 sign-in step (M12), which unit A8 draws.
    SignIn,
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
        })))
    }

    fn s(&self) -> &AppState {
        &self.0.s
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
            s.view.try_set(view);
        });
    }

    /// The brand, the project chip and "Back to Home" (tC4 `backToProjects`,
    /// `tC4:src/state.jsx:7286-7326`): drained first; Home, no project, no editing card. B1b and
    /// B3 add the close of the screen session.
    pub fn back_to_projects(&self) {
        let this = self.clone();
        spawn_local(async move {
            if !this.0.runtime.saves().gate().await {
                return;
            }
            this.leave_project();
        });
    }

    /// The server started again (3.10, D15): its opens and its reply cache are gone, so the
    /// page goes Home at once, with no gate, which could only save into the new run over tokens
    /// of the old one. The fields of the old run keep their entries for the offer.
    pub(crate) fn home_after_restart(&self) {
        self.leave_project();
    }

    fn leave_project(&self) {
        let s = self.s();
        s.view_error.try_set(None);
        s.editing.try_set(Editing::None);
        s.project.try_set(None);
        s.view.try_set(View::Home);
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
            // The sign-in step stays open and sends nothing (tC4: "Not now keeps the dialog").
            Pending::Link(..)
            | Pending::SignIn { .. }
            | Pending::CheckSignIn
            | Pending::Profile => {}
        }
    }

    /// The server refused with `NeedsInternet`: the switch went off after the question
    /// (AD25). Ask once more; a second refusal for the same step drops it.
    fn ask_again(&self, then: Pending) {
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
            let LinkKind::Help = kind;
            let request = HelpLink { url: url.clone() };
            match this.0.runtime.api().call(&request).await {
                Ok(link) => bridge::open_external(link.url.as_str()),
                Err(CallError::Api(ApiError::NeedsInternet(_))) => {
                    this.ask_again(Pending::Link(kind, url));
                }
                Err(error) => console_error(&format!("tc4: a help link did not open: {error}")),
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
            Retry::Story | Retry::Note | Retry::Save => self.0.runtime.saves().retry(),
        }
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
