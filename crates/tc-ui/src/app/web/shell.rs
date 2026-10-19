//! The port of tC4's `App.jsx` (unit A5): the app frame, the top bar (X1) with the save
//! indicator (X2) and the account menu (X6), the boundary that shows the view-error callout in
//! place of a screen (S15), the main view, and the dialogs the app mounts.

use leptos::ev;
use leptos::prelude::*;
use tc_api::ids::ServerInstance;

use super::app_ctx::{Actions, AppState, Dialog, use_app};
use super::{ViewError, js, panic};
use crate::app::account::SignInFlow;
use crate::app::save::indicator::{Indicator, IndicatorStatus, Retry};
use crate::app::shell::{
    CurrentProject, NAV_KEY, NavSnapshot, Screen, View, boundary_key, in_project, project_initials,
    screen,
};
use crate::attrs::Attrs;
use crate::ds::{
    Button, ButtonVariant, HeaderTone, Indicator as SwitchIndicator, Size, Status, StatusDot,
    Switcher, SwitcherOption,
};
use crate::i18n::{keys, t};
use crate::views::account_menu::{AccountMenu, InternetDialog};
use crate::views::home::Home;
use crate::views::modals::add_book::AddBook;
use crate::views::modals::add_project::AddProject;
use crate::views::modals::feedback::Feedback;
use crate::views::modals::import::Import;
use crate::views::modals::new_bible::{NewBible, NewObs};
use crate::views::modals::open_door43::OpenFromDoor43;
use crate::views::modals::share::ShareDialog;
use crate::views::modals::sign_in::ShareSignIn;

/// tC4's `App`: the frame, the top bar, the screen in its boundary, and the dialogs in tC4's
/// mount order, which is their stacking order (`tC4:src/App.jsx:138-154`).
#[component]
pub fn App() -> impl IntoView {
    let Some(ctx) = use_app() else {
        return ().into_any();
    };
    remember_place(&ctx.s);
    go_home_after_a_restart(&ctx.s, &ctx.actions);
    let frame = crate::css! {
        "display": "flex",
        "flex-direction": "column",
        "height": "100vh",
        "width": "100%",
        "background": "var(--surface-app)",
        "color": "var(--text-body)",
        "font-family": "var(--font-ui)",
        "overflow": "hidden",
    };
    // Memos, so a keystroke in the report, which changes `fb`, or in the sign-in step, which
    // changes `sign_in`, never draws the dialog again.
    let feedback_open = {
        let (dialog, fb) = (ctx.s.layers.dialog.clone(), ctx.s.fb.clone());
        Memo::new(move |_| dialog.get() == Some(Dialog::Feedback) && fb.with(Option::is_some))
    };
    // A step opened again is a new dialog, as tC4's is (a new `si`).
    let sign_in_open = {
        let (dialog, step) = (ctx.s.layers.dialog.clone(), ctx.s.sign_in.clone());
        Memo::new(move |_| {
            (dialog.get() == Some(Dialog::SignIn))
                .then(|| step.with(SignInFlow::flow))
                .flatten()
        })
    };
    let share_open = {
        let (dialog, sh) = (ctx.s.layers.dialog.clone(), ctx.s.sh.clone());
        Memo::new(move |_| dialog.get() == Some(Dialog::Share) && sh.with(Option::is_some))
    };
    view! {
        <div style=frame.to_style()>
            <TopBar />
            <ViewBoundary />
            {home_dialogs(&ctx.s)}
            // ProjectSettings, SourceTexts (B9a), GatewayChange, UpgradeSet (C8), GuidedFix (B5a)
            // mount here, before ShareSignIn.
            {move || sign_in_open.get().map(|_| view! { <ShareSignIn /> })}
            {move || share_open.get().then(|| view! { <ShareDialog /> })}
            {move || feedback_open.get().then(|| view! { <Feedback /> })}
            <InternetDialog />
            // OpenProgress (B3) and the Inspector (late) mount here.
        </div>
    }
    .into_any()
}

/// Home's dialogs, in tC4's mount order (`tC4:src/App.jsx:138-143`): AddProject, NewBible,
/// NewObs, AddBook, Import, OpenFromDoor43. Each is mounted while `s.modal` names it and its
/// form is set.
fn home_dialogs(s: &AppState) -> impl IntoView + use<> {
    let dialog = {
        let (dialog, np, ab, im, od) = (
            s.layers.dialog.clone(),
            s.np.clone(),
            s.ab.clone(),
            s.im.clone(),
            s.od.clone(),
        );
        Memo::new(move |_| match dialog.get() {
            Some(Dialog::NewProject) if np.with(Option::is_none) => None,
            Some(Dialog::NewObs) if np.with(Option::is_none) => None,
            Some(Dialog::AddBook) if ab.with(Option::is_none) => None,
            Some(Dialog::Import) if im.with(Option::is_none) => None,
            Some(Dialog::OpenDoor43) if od.with(Option::is_none) => None,
            other => other,
        })
    };
    let open = move |which: Dialog| Memo::new(move |_| dialog.get() == Some(which));
    let (add_project, new_bible, new_obs, add_book, import, open_door43) = (
        open(Dialog::AddProject),
        open(Dialog::NewProject),
        open(Dialog::NewObs),
        open(Dialog::AddBook),
        open(Dialog::Import),
        open(Dialog::OpenDoor43),
    );
    view! {
        {move || add_project.get().then(|| view! { <AddProject /> })}
        {move || new_bible.get().then(|| view! { <NewBible /> })}
        {move || new_obs.get().then(|| view! { <NewObs /> })}
        {move || add_book.get().then(|| view! { <AddBook /> })}
        {move || import.get().then(|| view! { <Import /> })}
        {move || open_door43.get().then(|| view! { <OpenFromDoor43 /> })}
    }
}

/// Writes the navigation snapshot (6.6) and the panic hook's context whenever the view, the
/// project or the server run changes. A restart that leaves the page at Home still rewrites
/// the run, so a reload after it does not take the snapshot of the old run for this one.
fn remember_place(s: &AppState) {
    let (instance, view, project) = (s.server.instance.clone(), s.view.clone(), s.project.clone());
    Effect::new(move |_| {
        let snapshot = NavSnapshot {
            instance: instance.get(),
            view: view.get(),
            project: project.get(),
        };
        if let Ok(text) = serde_json::to_string(&snapshot) {
            js::session_set(NAV_KEY, &text);
        }
        panic::set_context(
            Some(snapshot.view.as_str()),
            snapshot.project.map(|p| p.at.project),
        );
    });
}

/// The server started again (3.10, D15): the page goes Home with no gate. `instance` is
/// watched, not `restarted`, which stays true after the first restart; the signal is set again
/// by every re-read of `hello`, so only a different run counts.
fn go_home_after_a_restart(s: &AppState, actions: &Actions) {
    let instance = s.server.instance.clone();
    let actions = actions.clone();
    Effect::new(move |previous: Option<ServerInstance>| {
        let now = instance.get();
        if previous.is_some_and(|previous| previous != now) {
            actions.home_after_restart();
        }
        now
    });
}

/// The identity of the project the top bar shows: a change draws the bar afresh, because
/// AppHeader reads its project props once. That also draws the account menu again, which is
/// closed whenever a project opens or closes.
fn chip_of(project: &Option<CurrentProject>) -> Option<(String, String, String, &'static str)> {
    project.as_ref().map(|p| {
        let dir = match p.dir {
            tc_api::shown::Direction::Ltr => "ltr",
            tc_api::shown::Direction::Rtl => "rtl",
        };
        (
            p.at.project.as_str().to_owned(),
            p.name.clone(),
            p.language_tag.clone(),
            dir,
        )
    })
}

/// tC4's `TopBar` (`tC4:src/App.jsx:85-127`).
#[component]
fn TopBar() -> impl IntoView {
    let Some(ctx) = use_app() else {
        return ().into_any();
    };
    let project = ctx.s.project.clone();
    let chip = Memo::new(move |_| project.with(chip_of));
    let s = ctx.s.clone();
    let actions = ctx.actions.clone();
    (move || {
        let chip = chip.get();
        let (s, actions) = (s.clone(), actions.clone());
        let editing = s.editing.clone();
        let exits_off = Signal::derive(move || editing.with(|e| e.is_open()));
        let to_projects = {
            let actions = actions.clone();
            Callback::new(move |_: web_sys::MouseEvent| actions.back_to_projects())
        };
        let to_projects_chip = {
            let actions = actions.clone();
            Callback::new(move |_: web_sys::Event| actions.back_to_projects())
        };
        let center = {
            let s = s.clone();
            let actions = actions.clone();
            move || mode_switch(&s, &actions).into_any()
        };
        let right = move || {
            view! {
                <div style="display: flex; align-items: center; gap: 12px;">
                    <SaveIndicator />
                    <AccountMenu />
                </div>
            }
            .into_any()
        };
        let (name, initials, meta, dir) = match &chip {
            Some((_, name, tag, dir)) => (
                Some(name.clone()),
                Some(project_initials(name)),
                Some(tag.clone()),
                Some((*dir).to_owned()),
            ),
            None => (None, None, None, None),
        };
        crate::ds::AppHeader(crate::ds::AppHeaderProps {
            tone: HeaderTone::Ocean,
            logo_src: Some(format!("{}images/translationcore-logo.png", s.base)),
            project_initials: initials,
            project_name: name,
            project_name_dir: dir,
            project_meta: meta,
            // Read once: the bar is drawn again for a project, never for a language, because
            // that would draw the account menu and its open dialog again under a preview.
            switch_title: Some(untrack(|| t(keys::app::SWITCH_PROJECT, &[]))),
            center: Some(Box::new(center)),
            right: Some(Box::new(right)),
            on_brand_click: Some(to_projects),
            on_project_click: Some(to_projects_chip),
            exits_off,
        })
        .into_any()
    })
    .into_any()
}

/// The mode switch, while a project's screens show (`tC4:src/App.jsx:105-119`). Publish is
/// Check's Community Checking tool, so it shows Check (D63). An open editing card turns
/// Understand and Check off, and a press on an off tab keeps the focus in the card.
fn mode_switch(s: &AppState, actions: &Actions) -> impl IntoView + use<> {
    let (view, project, editing) = (s.view.clone(), s.project.clone(), s.editing.clone());
    let actions = actions.clone();
    // A memo: a mode tab changes the view, and the switch must stay the same node, with the
    // focus on the tab.
    let shown = {
        let view = view.clone();
        Memo::new(move |_| project.with(|p| in_project(view.get(), p.as_ref())))
    };
    move || {
        if !shown.get() {
            return None;
        }
        let editing_now = editing.clone();
        let off = Signal::derive(move || editing_now.with(|e| e.is_open()));
        let value = {
            let view = view.clone();
            Signal::derive(move || {
                Some(
                    match view.get() {
                        View::Publish => "check",
                        other => other.as_str(),
                    }
                    .to_owned(),
                )
            })
        };
        // The tabs' words follow the shown language; the switch is drawn again when it changes.
        let options = vec![
            SwitcherOption {
                disabled: off,
                ..SwitcherOption::new("read", t(keys::nav::UNDERSTAND, &[]))
            },
            SwitcherOption::new("draft", t(keys::nav::DRAFT, &[])),
            SwitcherOption {
                disabled: off,
                ..SwitcherOption::new("check", t(keys::nav::CHECK, &[]))
            },
        ];
        let actions = actions.clone();
        let on_change = Callback::new(move |value: String| {
            if let Some(view) = View::parse(&value) {
                actions.go(view);
            }
        });
        let switcher = view! {
            <Switcher
                options=options
                value=value
                on_change=on_change
                indicator=SwitchIndicator::Pill
                attrs=Attrs::from([("data-testid", "mode-switch")])
            />
        };
        let editing_press = editing.clone();
        Some(
            leptos::html::span()
                .on(
                    ev::capture(ev::pointerdown),
                    move |e: web_sys::PointerEvent| {
                        if editing_press.with_untracked(|e| e.is_open()) {
                            e.prevent_default();
                        }
                    },
                )
                .child(switcher),
        )
    }
}

/// tC4's `SaveIndicator`: nothing without a project (`tC4:src/App.jsx:44`).
#[component]
fn SaveIndicator() -> impl IntoView {
    let Some(ctx) = use_app() else {
        return ().into_any();
    };
    let project = ctx.s.project.clone();
    let open = Memo::new(move |_| project.with(Option::is_some));
    let runtime = super::use_runtime().map(send_wrapper::SendWrapper::new);
    let actions = ctx.actions.clone();
    (move || {
        let runtime = runtime.clone()?;
        open.get().then(|| {
            let actions = actions.clone();
            let indicator = runtime.saves().indicator();
            view! {
                <SaveIndicatorView
                    indicator=Signal::derive(move || indicator.get())
                    on_retry=Callback::new(move |which| actions.retry(which))
                />
            }
        })
    })
    .into_any()
}

/// X2 as tC4 draws it (`tC4:src/App.jsx:41-82`): the status dot and the label, and on an error
/// a Retry for the failure tC4's `retryFor` names. `data-state` is the fields' worst state,
/// never the checkpoint's.
#[component]
pub fn SaveIndicatorView(indicator: Signal<Indicator>, on_retry: Callback<Retry>) -> impl IntoView {
    let style = move || {
        let error = indicator.with(Indicator::is_error);
        crate::css! {
            "font-size": "var(--fs-caption)",
            "letter-spacing": "var(--track-12)",
            "font-weight": "var(--fw-heavy)",
            "display": "flex",
            "align-items": "center",
            "gap": 6,
            "color": if error { "var(--tc-invalid-on-dark)" } else { "rgba(255,255,255,.66)" },
        }
        .to_style()
    };
    let status = Signal::derive(move || match indicator.with(Indicator::status) {
        IndicatorStatus::Valid => Status::Valid,
        IndicatorStatus::Warn => Status::Warn,
        IndicatorStatus::Invalid => Status::Invalid,
    });
    let retry = Memo::new(move |_| indicator.with(|i| i.is_error().then_some(i.retry)));
    let button = move || {
        retry.get().map(|which| {
            let mut attrs = Attrs::from([("data-i", "quiet")]);
            if let Some(id) = which.test_id() {
                attrs.set("data-testid", id);
            }
            let style = crate::css! {
                "background": "transparent",
                "color": "var(--tc-invalid-on-dark)",
                "border-color": "var(--tc-invalid-on-dark)",
                "padding": "3px 10px",
                "font-size": "var(--fs-label)",
            };
            view! {
                <Button
                    size=Size::Sm
                    variant=ButtonVariant::Outline
                    attrs=attrs
                    style=style
                    on_click=Callback::new(move |_: web_sys::MouseEvent| on_retry.run(which))
                >
                    {move || t(keys::app::RETRY, &[])}
                </Button>
            }
        })
    };
    view! {
        <div
            data-testid="save-indicator"
            data-state=move || indicator.with(|i| i.state.data_state())
            style=style
        >
            <StatusDot status=status size=8.0 />
            {move || t(indicator.with(Indicator::label_key), &[])}
            {button}
        </div>
    }
}

/// tC4's `ViewBoundary` (S15): the view-error callout in place of the screen, with the top bar
/// and its Retry still there; "Back to Home" leaves through the navigation gate. The screen is
/// keyed by the view and the project, so a navigation draws it afresh.
#[component]
fn ViewBoundary() -> impl IntoView {
    let Some(ctx) = use_app() else {
        return ().into_any();
    };
    // A memo: navigation clears the error even when there is none, and that must not draw the
    // screen again.
    let view_error = {
        let error = ctx.s.view_error.clone();
        Memo::new(move |_| error.get())
    };
    let actions = ctx.actions.clone();
    let s = ctx.s.clone();
    let key = {
        let (view, project) = (s.view.clone(), s.project.clone());
        Memo::new(move |_| project.with(|p| boundary_key(view.get(), p.as_ref())))
    };
    (move || match view_error.get() {
        Some(message) => {
            let actions = actions.clone();
            view! {
                <ViewError
                    message=message
                    on_home=Callback::new(move |()| actions.back_to_projects())
                />
            }
            .into_any()
        }
        None => {
            let s = s.clone();
            (move || {
                key.track();
                main_view(&s)
            })
            .into_any()
        }
    })
    .into_any()
}

/// tC4's `MainView` (`tC4:src/App.jsx:192-209`).
fn main_view(s: &AppState) -> AnyView {
    let screen = s
        .project
        .with_untracked(|p| screen(s.view.get_untracked(), p.as_ref()));
    match screen {
        Screen::Home => view! { <Home /> }.into_any(),
        // B3 Understand, B4a Translate, B5a/B5b Check, B8 Community Checking, B7 the story
        // views.
        Screen::Understand
        | Screen::Draft
        | Screen::Check
        | Screen::CommunityChecking
        | Screen::StoryUnderstand
        | Screen::StoryDraft
        | Screen::Nothing => ().into_any(),
    }
}
