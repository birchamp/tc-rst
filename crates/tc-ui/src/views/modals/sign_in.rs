//! The Door43 sign-in step (M12, tC4 #203, `tC4:src/views/modals/ShareSignIn.jsx`): the Door43
//! username or email, the password, "Stay signed in on this computer", the server the app signs
//! in to, and the notices about the author name and the username (D85, #680). The account
//! menu's "Sign in to Door43" opens it, and so does a step that needs a sign-in. Opening it
//! sends nothing; Sign in asks "Turn on the internet?" while the internet is off.
//!
//! The fields are not save fields: nothing of them reaches the draft buffer, the shell's mirror
//! or page storage, and the password leaves the page only in the body of `account/signIn`.
//! Without a keychain that keeps a sign-in (phase 0's product, until unit E1a) the step has no
//! "Stay signed in" box and no line about it (tC4 #496).
//! tC4's `signin-renew` line (#467, a share whose token lacks the create scopes) comes with
//! unit E1b, which meets that refusal.

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::account::KeychainState;

use crate::app::account::{SignInFlow, SignInProblem};
use crate::app::web::{AppCtx, use_app};
use crate::ds::{Attrs, Button, ButtonVariant, Callout, CalloutTone, Checkbox, TextField};
use crate::i18n::{keys, t};

/// tC4's `NOTE`.
fn note() -> String {
    crate::css! {
        "font-size": "var(--fs-caption-lg)",
        "letter-spacing": "var(--track-12-5)",
        "color": "var(--text-secondary)",
        "line-height": "var(--lh-body)",
        "margin": 0,
    }
    .to_style()
}

/// The sentence of a problem: a refusal's own words with Door43's `{reason}`, or the text of
/// another failure.
fn sentence(problem: &SignInProblem) -> String {
    match problem {
        SignInProblem::Refused(refusal) => crate::i18n::refusal_text(refusal),
        SignInProblem::Other(text) => text.clone(),
    }
}

/// The step. The app mounts it while `s.modal` is the sign-in and the step is open.
#[component]
pub fn ShareSignIn() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let step = s.sign_in.clone();
    // Memos, so a keystroke, which changes the form, draws nothing but its field.
    let busy = Memo::new({
        let step = step.clone();
        move |_| step.with(SignInFlow::busy)
    });

    let footer: ChildrenFn = {
        let actions = actions.clone();
        Arc::new(move || {
            let (cancel, submit) = (actions.clone(), actions.clone());
            view! {
                <Button
                    variant=ButtonVariant::Secondary
                    attrs=Attrs::from([("data-testid", "signin-cancel")])
                    on_click=Callback::new(move |_| cancel.cancel_sign_in())
                >
                    {move || t(keys::sign_in::CANCEL, &[])}
                </Button>
                <Button
                    disabled=Signal::from(busy)
                    attrs=Attrs::from([("data-testid", "signin-submit")])
                    on_click=Callback::new(move |_| submit.submit_sign_in())
                >
                    {move || {
                        if busy.get() {
                            t(keys::sign_in::BUSY, &[])
                        } else {
                            t(keys::sign_in::SUBMIT, &[])
                        }
                    }}
                </Button>
            }
            .into_any()
        })
    };

    let children: ChildrenFn = {
        let (actions, s) = (actions.clone(), s.clone());
        Arc::new(move || form(&actions, &s).into_any())
    };

    let close = actions.clone();
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some((move || move || t(keys::sign_in::TITLE, &[])).into()),
        subtitle: Some((move || move || t(keys::sign_in::SUBTITLE, &[])).into()),
        close_label: Some(Signal::derive(|| t(keys::common::CLOSE, &[]))),
        on_close: Some(Callback::new(move |()| close.cancel_sign_in())),
        footer: Some(footer),
        attrs: Attrs::from([("data-testid", "share-signin")]),
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}

/// The form: Enter in a field signs in, through the hidden submit button, as in tC4.
fn form(
    actions: &crate::app::web::Actions,
    s: &crate::app::web::AppState,
) -> impl IntoView + use<> {
    let step = s.sign_in.clone();
    let field = |get: fn(&SignInFlow) -> String| {
        let step = step.clone();
        Signal::from(Memo::new(move |_| step.with(get)))
    };
    let login: Signal<String> = field(|step| match step {
        SignInFlow::Open(step) => step.form.login.clone(),
        SignInFlow::Idle => String::new(),
    });
    let password: Signal<String> = field(|step| match step {
        SignInFlow::Open(step) => step.form.password.clone(),
        SignInFlow::Idle => String::new(),
    });
    let stay = Memo::new({
        let step = step.clone();
        move |_| step.with(|step| matches!(step, SignInFlow::Open(step) if step.form.stay))
    });
    let problem = Memo::new({
        let step = step.clone();
        move |_| {
            step.with(|step| match step {
                SignInFlow::Open(step) => step.error.clone(),
                SignInFlow::Idle => None,
            })
        }
    });
    let account = s.server.account.clone();
    let host = Memo::new({
        let account = account.clone();
        move |_| account.with(|a| a.server.clone())
    });
    let keychain = Memo::new(move |_| account.with(|a| a.keychain == KeychainState::Works));

    let on_login = {
        let actions = actions.clone();
        Callback::new(move |e: web_sys::Event| {
            let value = event_target_value(&e);
            actions.patch_sign_in(|form| form.login = value);
        })
    };
    let on_password = {
        let actions = actions.clone();
        Callback::new(move |e: web_sys::Event| {
            let value = event_target_value(&e);
            actions.patch_sign_in(|form| form.password = value);
        })
    };
    let on_stay = {
        let actions = actions.clone();
        Callback::new(move |e: web_sys::Event| {
            let checked = event_target_checked(&e);
            actions.patch_sign_in(|form| form.stay = checked);
        })
    };
    let submit = actions.clone();
    let on_submit = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        submit.submit_sign_in();
    };

    let stay_box = move || {
        keychain.get().then(|| {
            view! {
                <Checkbox
                    label=untrack(|| t(keys::sign_in::STAY, &[]))
                    checked=Signal::from(stay)
                    attrs=Attrs::from([("data-testid", "signin-stay")])
                    on_change=on_stay
                />
                <p data-testid="signin-reason" style=note()>
                    {move || t(keys::sign_in::SESSION_NOTE, &[])}
                </p>
            }
        })
    };
    let callout = move || {
        problem.get().map(|problem| {
            let code = problem.code();
            let words = sentence(&problem);
            view! {
                <Callout
                    tone=CalloutTone::Warn
                    style=crate::css! { "overflow-wrap": "anywhere" }
                    attrs=Attrs::from([("role", "alert"), ("data-testid", "signin-error"), ("data-code", code)])
                >
                    <strong>{move || t(keys::sign_in::FAILED, &[])}</strong>
                    " "
                    {words}
                </Callout>
            }
        })
    };
    let column = "display: flex; flex-direction: column; gap: 16px;";
    view! {
        <form style=column on:submit=on_submit>
            <p data-testid="signin-server" style=note()>
                {move || t(keys::sign_in::SERVER, &[("host", &host.get())])}
            </p>
            <TextField
                id="si-login"
                label=move || t(keys::sign_in::LOGIN, &[])
                value=login
                attrs=Attrs::from([("autocomplete", "username")])
                on_change=on_login
            />
            <TextField
                id="si-password"
                label=move || t(keys::sign_in::PASSWORD, &[])
                value=password
                attrs=Attrs::from([("type", "password"), ("autocomplete", "current-password")])
                on_change=on_password
            />
            {stay_box}
            <p data-testid="signin-notice" style=note()>{move || t(keys::sign_in::NOTICE, &[])}</p>
            <p data-testid="signin-name-notice" style=note()>
                {move || t(keys::sign_in::NAME_NOTICE, &[])}
            </p>
            {callout}
            <button type="submit" hidden=true aria-hidden="true" tabindex="-1"></button>
        </form>
    }
}
