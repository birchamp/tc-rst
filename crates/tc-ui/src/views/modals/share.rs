//! Share on Door43 (unit B9b, `tC4:src/views/modals/ShareDialog.jsx`, #362, D84): the
//! where-it-goes step (the person's own account or an organization they can create in, one
//! marked Recommended), the check step (the repository name and the books or stories), then
//! the run's progress line and the done step (E1b). A Home card's Share opens it once a sign-in
//! is held.

use std::sync::Arc;

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use tc_api::account::WebUrl;
use tc_api::door43::SharedRepo;
use tc_api::jobs::SharePhase;

use crate::app::internet::LinkKind;
use crate::app::share::{Choices, ShareForm, ShareStep, SharedItems, Target};
use crate::app::web::{AppCtx, use_app};
use crate::ds::{
    Attrs, Button, ButtonVariant, Callout, CalloutTone, OptionCard, OptionControl, Spinner, Text,
    TextField, TextRole, TextTone,
};
use crate::i18n::{keys, t};

/// tC4's `CHANGE`: a link-coloured button inside the account line.
const CHANGE: &str = "border: 0px; background: transparent; padding: 0px; font: inherit; letter-spacing: inherit; color: var(--link); cursor: pointer;";

/// tC4 keeps the footer's buttons when Next or Back changes the step, so the button pressed
/// keeps the focus under its new label; the rebuild draws the step's buttons anew and gives
/// the focus to the one in the same place.
fn focus_after_step(testid: &'static str) {
    request_animation_frame(move || {
        let found = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| {
                d.query_selector(&format!("[data-testid=\"{testid}\"]"))
                    .ok()
                    .flatten()
            })
            .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok());
        if let Some(button) = found {
            let _ = button.focus();
        }
    });
}

/// The dialog, while `s.modal` is the share and its form is set.
#[component]
pub fn ShareDialog() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let sh = s.share_form();
    let read = {
        move |get: fn(&ShareForm) -> Option<String>| {
            Memo::new(move |_| sh.with(|sh| sh.as_ref().and_then(get)))
        }
    };
    let step = Memo::new(move |_| {
        sh.with(|sh| sh.as_ref().map_or(ShareStep::Target, |sh| sh.step.clone()))
    });
    // No footer and no close while the run is in progress.
    let running = Memo::new(move |_| matches!(step.get(), ShareStep::Running(_)));
    let project_name = read(|sh| Some(sh.project.name.clone()));
    let account = s.server.account.clone();
    let user = Memo::new({
        let account = account.clone();
        move |_| account.with(|a| a.user.clone())
    });

    let footer: ChildrenFn = {
        let (actions, sh) = (actions.clone(), sh);
        Arc::new(move || {
            let (actions, sh) = (actions.clone(), sh);
            let can_next = {
                Signal::derive(move || sh.with(|sh| sh.as_ref().is_some_and(ShareForm::can_next)))
            };
            let can_submit = {
                Signal::derive(move || sh.with(|sh| sh.as_ref().is_some_and(ShareForm::can_submit)))
            };
            (move || {
                let (first, second) = (actions.clone(), actions.clone());
                match step.get() {
                    ShareStep::Target => view! {
                        <Button
                            variant=ButtonVariant::Secondary
                            attrs=Attrs::from([("data-testid", "share-cancel")])
                            on_click=Callback::new(move |_| first.close_modal())
                        >
                            {move || t(keys::share_dialog::CANCEL, &[])}
                        </Button>
                        <Button
                            disabled=Signal::derive(move || !can_next.get())
                            attrs=Attrs::from([("data-testid", "share-next")])
                            on_click=Callback::new(move |_| {
                                second.share_step(ShareStep::Check);
                                focus_after_step("share-submit");
                            })
                        >
                            {move || t(keys::share_dialog::NEXT, &[])}
                        </Button>
                    }
                    .into_any(),
                    ShareStep::Check => view! {
                        <Button
                            variant=ButtonVariant::Secondary
                            attrs=Attrs::from([("data-testid", "share-back")])
                            on_click=Callback::new(move |_| {
                                first.share_step(ShareStep::Target);
                                focus_after_step("share-cancel");
                            })
                        >
                            {move || t(keys::share_dialog::BACK, &[])}
                        </Button>
                        <Button
                            disabled=Signal::derive(move || !can_submit.get())
                            attrs=Attrs::from([("data-testid", "share-submit")])
                            on_click=Callback::new(move |_| second.share_run())
                        >
                            {move || t(keys::share_dialog::SUBMIT, &[])}
                        </Button>
                    }
                    .into_any(),
                    ShareStep::Running(_) => ().into_any(),
                    ShareStep::Done(_) => view! {
                        <Button
                            attrs=Attrs::from([("data-testid", "share-close")])
                            on_click=Callback::new(move |_| first.close_modal())
                        >
                            {move || t(keys::common::CLOSE, &[])}
                        </Button>
                    }
                    .into_any(),
                }
            })
            .into_any()
        })
    };

    let children: ChildrenFn = {
        let (actions, s) = (actions.clone(), s.clone());
        Arc::new(move || {
            let (actions, s) = (actions.clone(), s.clone());
            let sharing_as = {
                let actions = actions.clone();
                let before_run =
                    Memo::new(move |_| matches!(step.get(), ShareStep::Target | ShareStep::Check));
                move || before_run.get().then(|| sharing_as(&actions, user))
            };
            let body = {
                let (actions, sh) = (actions.clone(), s.share_form());
                move || match step.get() {
                    ShareStep::Target => target_step(&actions, &sh, user).into_any(),
                    ShareStep::Check => check_step(&actions, &sh, user).into_any(),
                    ShareStep::Running(phase) => progress_step(phase, &sh).into_any(),
                    ShareStep::Done(repo) => done_step(&actions, &sh, repo).into_any(),
                }
            };
            let error = share_error(&s.share_form());
            view! { {sharing_as} {body} {error} }.into_any()
        })
    };

    let close = actions.clone();
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some(
            (move || {
                move || {
                    t(
                        match step.get() {
                            ShareStep::Target => keys::share_dialog::WHERE_TITLE,
                            ShareStep::Check => keys::share_dialog::CHECK_TITLE,
                            ShareStep::Running(_) => keys::share_dialog::PROGRESS_TITLE,
                            ShareStep::Done(_) => keys::share_dialog::DONE_TITLE,
                        },
                        &[],
                    )
                }
            })
            .into(),
        ),
        subtitle: Some(
            (move || {
                move || {
                    let name = project_name.get().unwrap_or_default();
                    t(
                        match step.get() {
                            ShareStep::Target => keys::share_dialog::WHERE_SUBTITLE,
                            ShareStep::Check => keys::share_dialog::CHECK_SUBTITLE,
                            ShareStep::Running(_) => keys::share_dialog::PROGRESS_SUBTITLE,
                            ShareStep::Done(_) => keys::share_dialog::DONE_SUBTITLE,
                        },
                        &[("name", &name)],
                    )
                }
            })
            .into(),
        ),
        close_label: Some(Signal::derive(|| t(keys::common::CLOSE, &[]))),
        on_close: Some(Callback::new(move |()| close.close_modal())),
        closable: Signal::derive(move || !running.get()),
        footer: Some(footer),
        footer_shown: Some(Signal::derive(move || !running.get())),
        attrs: Attrs::from([("data-testid", "share-dialog")]),
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}

/// "Sharing as @username · Change" (D86 point 7), and "Signed in · Change" for a kept sign-in not
/// resumed yet. No line says a sign-in is not kept (tC4 #496).
fn sharing_as(
    actions: &crate::app::web::Actions,
    user: Memo<Option<String>>,
) -> impl IntoView + use<> {
    let change = actions.clone();
    let label = move || match user.get() {
        Some(user) => t(keys::sign_in::SHARING_AS, &[("user", &user)]),
        None => t(keys::sign_in::KEPT, &[]),
    };
    let line = crate::css! {
        "font-size": "var(--fs-caption-lg)",
        "color": "var(--text-secondary)",
    }
    .to_style();
    view! {
        <div style="margin: 0px 0px 14px;">
            <span data-testid="share-account" style=line>
                {label}
                " · "
                <button
                    type="button"
                    data-testid="share-account-change"
                    style=CHANGE
                    on:click=move |_| change.change_sign_in()
                >
                    {move || t(keys::sign_in::CHANGE, &[])}
                </button>
            </span>
        </div>
    }
}

fn account_title(user: Option<String>) -> String {
    t(
        keys::share_dialog::ACCOUNT,
        &[("user", &user.unwrap_or_default())],
    )
}

/// Where it goes (`TargetStep`): an organization the person cannot create in is shown, cannot be
/// chosen, and says why (D84 point 3).
fn target_step(
    actions: &crate::app::web::Actions,
    sh: &Signal<Option<ShareForm>>,
    user: Memo<Option<String>>,
) -> impl IntoView + use<> {
    let selected = {
        let sh = *sh;
        Memo::new(move |_| sh.with(|sh| sh.as_ref().map_or(Target::User, |sh| sh.target.clone())))
    };
    let choices = {
        let sh = *sh;
        Memo::new(move |_| {
            sh.with(|sh| {
                sh.as_ref()
                    .map_or(Choices::Loading, |sh| sh.choices.clone())
            })
        })
    };
    let pick_user = actions.clone();
    let orgs = {
        let actions = actions.clone();
        move || {
            let Choices::Ready(orgs) = choices.get() else {
                return ().into_any();
            };
            orgs.into_iter()
                .map(|org| {
                    let actions = actions.clone();
                    let name = org.name.clone();
                    let is_selected = {
                        let name = name.clone();
                        Signal::derive(move || selected.get() == Target::Organization(name.clone()))
                    };
                    let title = if org.full_name.is_empty() {
                        org.name.clone()
                    } else {
                        org.full_name.clone()
                    };
                    let attrs = Attrs::new()
                        .with("data-testid", format!("share-org-{}", org.name))
                        .with("data-recommended", if org.recommended { "1" } else { "0" });
                    let recommended_label = t(keys::share_dialog::RECOMMENDED, &[]);
                    if org.can_create {
                        let description = org.name.clone();
                        view! {
                            <OptionCard
                                control=OptionControl::Radio
                                selected=is_selected
                                title=title
                                recommended=org.recommended
                                recommended_label=recommended_label
                                description=description
                                attrs=attrs
                                on_click=Callback::new(move |_| {
                                    actions.pick_share_target(Target::Organization(name.clone()))
                                })
                            />
                        }
                        .into_any()
                    } else {
                        view! {
                            <OptionCard
                                control=OptionControl::Radio
                                selected=is_selected
                                disabled=true
                                title=title
                                recommended=org.recommended
                                recommended_label=recommended_label
                                description=move || t(keys::share_dialog::ORG_CANNOT, &[])
                                attrs=attrs
                            />
                        }
                        .into_any()
                    }
                })
                .collect_view()
                .into_any()
        }
    };
    let loading = move || {
        matches!(choices.get(), Choices::Loading).then(|| {
            view! {
                <div style="padding: 10px 0px;" data-testid="share-orgs-loading">
                    <Spinner label=t(keys::share_dialog::ORGS_LOADING, &[]) />
                </div>
            }
        })
    };
    let failed = move || {
        let Choices::Failed(why) = choices.get() else {
            return None;
        };
        Some(view! {
            <Callout
                tone=CalloutTone::Warn
                style=crate::css! { "overflow-wrap": "anywhere" }
                attrs=Attrs::from([("role", "alert"), ("data-testid", "share-orgs-error")])
            >
                {move || t(keys::share_dialog::ORGS_ERROR, &[])}
                " "
                {why}
            </Callout>
        })
    };
    view! {
        <div data-testid="share-target" style="display: flex; flex-direction: column; gap: 10px;">
            <OptionCard
                control=OptionControl::Radio
                selected=Signal::derive(move || selected.get() == Target::User)
                title=move || account_title(user.get())
                description=move || t(keys::share_dialog::ACCOUNT_DESC, &[])
                attrs=Attrs::from([("data-testid", "share-target-user")])
                on_click=Callback::new(move |_| pick_user.pick_share_target(Target::User))
            />
            {loading}
            {failed}
            {orgs}
        </div>
    }
}

/// Check what will be shared (`CheckStep`): where, the repository name, and the books or the
/// stories, with no counts, no license and no private option; the whole project, always (D84
/// point 4).
fn check_step(
    actions: &crate::app::web::Actions,
    sh: &Signal<Option<ShareForm>>,
    user: Memo<Option<String>>,
) -> impl IntoView + use<> {
    let where_to = {
        let sh = *sh;
        move || {
            let org = sh.with(|sh| {
                sh.as_ref()
                    .and_then(|sh| sh.target_org().map(str::to_owned))
            });
            let target = org.unwrap_or_else(|| account_title(user.get()));
            t(keys::share_dialog::WHERE, &[("target", &target)])
        }
    };
    let name: Signal<String> = {
        let sh = *sh;
        Signal::from(Memo::new(move |_| {
            sh.with(|sh| sh.as_ref().map(|sh| sh.name.clone()).unwrap_or_default())
        }))
    };
    let invalid = {
        let sh = *sh;
        Signal::derive(move || sh.with(|sh| sh.as_ref().is_some_and(ShareForm::name_invalid)))
    };
    let on_name = {
        let actions = actions.clone();
        Callback::new(move |e: web_sys::Event| {
            let value = event_target_value(&e);
            actions.type_share_name(value);
        })
    };
    let items = sh.with_untracked(|sh| sh.as_ref().map(|sh| sh.project.items.clone()));
    let (label, items) = match items {
        Some(SharedItems::Stories) => (
            keys::share_dialog::STORIES_LABEL,
            t(keys::share_dialog::STORIES, &[]),
        ),
        Some(SharedItems::Books(books)) => (
            keys::share_dialog::BOOKS_LABEL,
            books
                .iter()
                .map(|b| b.english_name())
                .collect::<Vec<_>>()
                .join(", "),
        ),
        None => (keys::share_dialog::BOOKS_LABEL, String::new()),
    };
    view! {
        <div data-testid="share-check" style="display: flex; flex-direction: column; gap: 14px;">
            <Text role=TextRole::Body attrs=Attrs::from([("data-testid", "share-where")])>{where_to}</Text>
            <TextField
                id="sh-name"
                label=Signal::derive(|| t(keys::share_dialog::NAME, &[]))
                hint=Signal::derive(|| t(keys::share_dialog::NAME_RULE, &[]))
                value=name
                invalid=invalid
                attrs=Attrs::from([("data-testid", "share-name")])
                on_change=on_name
            />
            <div>
                <Text role=TextRole::Caption tone=TextTone::Muted>{move || t(label, &[])}</Text>
                <Text role=TextRole::Body attrs=Attrs::from([("data-testid", "share-items")])>{items}</Text>
            </div>
        </div>
    }
}

/// The lines shown so far, as tC4's `data-steps` keeps them for the journeys.
fn steps_attr(sh: &Signal<Option<ShareForm>>) -> String {
    sh.with(|sh| {
        sh.as_ref()
            .map(|sh| {
                sh.shown
                    .iter()
                    .map(|phase| match phase {
                        SharePhase::Preparing => "preparing",
                        SharePhase::Create => "create",
                        SharePhase::Push => "push",
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default()
    })
}

/// One progress line (D84 point 5, `ProgressStep`).
fn progress_step(phase: SharePhase, sh: &Signal<Option<ShareForm>>) -> impl IntoView + use<> {
    let label = t(
        match phase {
            SharePhase::Preparing => keys::share_dialog::PREPARING,
            SharePhase::Create => keys::share_dialog::CREATING,
            SharePhase::Push => keys::share_dialog::PUSHING,
        },
        &[],
    );
    view! {
        <div
            role="status"
            style="padding: 26px 0px 30px; display: flex; justify-content: center;"
            data-testid="share-progress"
            data-steps=steps_attr(sh)
        >
            <Spinner label=label />
        </div>
    }
}

/// Shared (`DoneStep`): the repository's address, what teammates need, Copy link and Open on
/// Door43.
fn done_step(
    actions: &crate::app::web::Actions,
    sh: &Signal<Option<ShareForm>>,
    repo: SharedRepo,
) -> impl IntoView + use<> {
    let sh = *sh;
    let lines = sh.with_untracked(|sh| sh.as_ref().map(ShareForm::done_lines).unwrap_or_default());
    let mut lines = lines.into_iter();
    let first = lines.next().map(|key| {
        view! {
            <Text role=TextRole::Body attrs=Attrs::from([("data-testid", "share-done-text")])>
                <span role="status">{move || t(key, &[])}</span>
            </Text>
        }
    });
    let rest = lines
        .map(|key| {
            view! {
                <Text role=TextRole::Body attrs=Attrs::from([("data-testid", "share-team-access")])>
                    {move || t(key, &[])}
                </Text>
            }
        })
        .collect::<Vec<_>>();
    let copied = Memo::new(move |_| sh.with(|sh| sh.as_ref().is_some_and(|sh| sh.copied)));
    let (copy, open) = (actions.clone(), actions.clone());
    let url: WebUrl = repo.url;
    let shown_url = url.as_str().to_owned();
    view! {
        <div data-testid="share-done" data-steps=steps_attr(&sh) style="display: flex; flex-direction: column; gap: 12px;">
            {first}
            <Text role=TextRole::Strong attrs=Attrs::from([("data-testid", "share-url")]) style=crate::css! { "overflow-wrap": "anywhere" }>
                {shown_url}
            </Text>
            {rest}
            <div style="display: flex; gap: 10px; align-items: center;">
                <Button
                    variant=ButtonVariant::Secondary
                    size=crate::ds::Size::Sm
                    attrs=Attrs::from([("data-testid", "share-copy")])
                    on_click=Callback::new(move |_| copy.share_copy_link())
                >
                    {move || t(if copied.get() { keys::share_dialog::COPIED } else { keys::share_dialog::COPY_LINK }, &[])}
                </Button>
                <Button
                    size=crate::ds::Size::Sm
                    attrs=Attrs::from([("data-testid", "share-open")])
                    on_click=Callback::new(move |_| open.open_door43_page(LinkKind::Repo, url.clone()))
                >
                    {move || t(keys::share_dialog::OPEN_ON_DOOR43, &[])}
                </Button>
            </div>
        </div>
    }
}

/// Why the last run did not share (`sh.error`): "Not shared." and the refusal's sentence,
/// picked by its code (tC4 `shareErrorText`).
fn share_error(sh: &Signal<Option<ShareForm>>) -> impl IntoView + use<> {
    let sh = *sh;
    let error = Memo::new(move |_| sh.with(|sh| sh.as_ref().and_then(|sh| sh.error.clone())));
    move || {
        error.get().map(|error| {
            let code = error.code.clone().unwrap_or_default();
            let text = error.text.clone();
            view! {
                <Callout
                    tone=CalloutTone::Warn
                    attrs=Attrs::from([("role", "alert"), ("data-testid", "share-error")])
                        .with("data-code", code)
                    style=crate::css! { "overflow-wrap": "anywhere" }
                >
                    <strong>{t(keys::share_dialog::FAILED, &[])}</strong>
                    " "
                    {text}
                </Callout>
            }
        })
    }
}
