//! New Bible (M2) and New Open Bible Stories (M3), `tC4:src/views/modals/NewBible.jsx` and
//! `NewObs.jsx`: one form, `np`. New Open Bible Stories has no versification. Create is C6's.
//!
//! When a chosen language's Door43 code is not one a project can hold, the Code box gets one it
//! can and one sentence says so (owner's answer to Q35); when none can be made, the box stays
//! empty and the sentence says why.

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::shown::Direction;

use super::language_name_field::LanguageNameField;
use crate::app::forms::{CodeOffer, NewKind, NewProjectForm, SCRIPT_FONTS, code_offer};
use crate::app::web::{Actions, AppCtx, use_app};
use crate::ds::{
    Attrs, Button, ButtonVariant, Callout, CalloutTone, FilterChip, OptionCard, Overline, Select,
    SelectOption, TextField,
};
use crate::i18n::{keys, t};

/// The single gateway language row, selected and not interactive (`GlRow`).
pub fn gl_row() -> impl IntoView {
    view! {
        <OptionCard
            selected=true
            recommended=true
            recommended_label=t(keys::new_bible::RECOMMENDED, &[])
            title=move || t(keys::new_bible::GL_NAME, &[])
            description=move || t(keys::new_bible::GL_META, &[])
            trailing=move || t(keys::sym::TICK, &[])
            style=crate::css! { "cursor": "default" }
        />
    }
}

fn note_style() -> String {
    crate::css! {
        "font-size": "var(--fs-caption-lg)",
        "letter-spacing": "var(--track-12-5)",
        "color": "var(--text-secondary)",
        "margin": "0 0 8px",
        "line-height": "var(--lh-body)",
    }
    .to_style()
}

/// New Bible, while `s.modal` is NewProject.
#[component]
pub fn NewBible() -> impl IntoView {
    new_project(NewKind::Bible)
}

/// New Open Bible Stories, while `s.modal` is NewObs.
#[component]
pub fn NewObs() -> impl IntoView {
    new_project(NewKind::Stories)
}

fn new_project(kind: NewKind) -> AnyView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let bible = kind == NewKind::Bible;
    let np = s.new_project_form();
    let read = move |get: fn(&NewProjectForm) -> String| {
        Signal::derive(move || np.with(|np| np.as_ref().map(get).unwrap_or_default()))
    };
    let busy = {
        let np = s.new_project_form();
        Signal::derive(move || np.with(|np| np.as_ref().is_some_and(|np| np.busy)))
    };
    let footer: ChildrenFn = {
        let actions = actions.clone();
        Arc::new(move || {
            let (cancel, create) = (actions.clone(), actions.clone());
            view! {
                <Button variant=ButtonVariant::Secondary on_click=Callback::new(move |_| cancel.close_modal())>
                    {move || t(keys::new_bible::CANCEL, &[])}
                </Button>
                <Button
                    disabled=busy
                    on_click=Callback::new(move |_| if bible { create.create_bible() } else { create.create_obs() })
                >
                    {move || t(if bible { keys::new_bible::CREATE } else { keys::new_obs::CREATE }, &[])}
                </Button>
            }
            .into_any()
        })
    };
    let children: ChildrenFn = {
        let (actions, np) = (actions.clone(), s.new_project_form());
        let name = read(|np| np.name.clone());
        let code = read(|np| np.code.clone());
        let font = read(|np| np.font.clone());
        Arc::new(move || {
            let (actions, np) = (actions.clone(), np);
            let id = |part: &str| format!("{}-{part}", if bible { "nb" } else { "no" });
            let on_name = {
                let actions = actions.clone();
                Callback::new(move |e: web_sys::Event| {
                    let value = event_target_value(&e);
                    actions.patch_np(|np| np.name = value);
                })
            };
            let on_code = {
                let actions = actions.clone();
                Callback::new(move |e: web_sys::Event| {
                    let value = event_target_value(&e);
                    actions.patch_np(|np| np.type_code(value));
                })
            };
            let on_font = {
                let actions = actions.clone();
                Callback::new(move |value: String| actions.patch_np(|np| np.font = value))
            };
            let offer = {
                move || {
                    np.with(|np| {
                        let np = np.as_ref()?;
                        let text = match code_offer(np) {
                            CodeOffer::None => return None,
                            CodeOffer::Adjusted { door43, code } => t(
                                keys::new_bible::CODE_ADJUSTED,
                                &[("door43", &door43), ("code", &code)],
                            ),
                            CodeOffer::Unusable { door43 } => {
                                t(keys::new_bible::CODE_UNUSABLE, &[("door43", &door43)])
                            }
                        };
                        Some(view! {
                            <p data-testid="code-offer" style="font-size: var(--fs-caption); color: var(--text-secondary); margin: -6px 0 0; line-height: var(--lh-body);">
                                {text}
                            </p>
                        })
                    })
                }
            };
            let fonts: Vec<SelectOption> = SCRIPT_FONTS
                .iter()
                .map(|f| SelectOption::from(*f))
                .collect();
            let versification = bible.then(|| versification(&actions, &np));
            let error = {
                move || {
                    np.with(|np| np.as_ref().and_then(|np| np.error.clone())).map(|error| {
                        view! { <Callout tone=CalloutTone::Warn attrs=Attrs::from([("role", "alert")])>{error}</Callout> }
                    })
                }
            };
            view! {
                <TextField
                    id=id("name")
                    label=Signal::derive(move || t(if bible { keys::new_bible::NAME } else { keys::new_obs::NAME }, &[]))
                    value=name
                    placeholder=untrack(|| t(if bible { keys::new_bible::NAME_PLACEHOLDER } else { keys::new_obs::NAME_PLACEHOLDER }, &[]))
                    on_change=on_name
                />
                <div style="display: grid; grid-template-columns: 1fr 130px; gap: 12px;">
                    <LanguageNameField id=if bible { "nb-lang" } else { "no-lang" } />
                    <TextField
                        id=id("code")
                        label=Signal::derive(move || t(keys::new_bible::CODE, &[]))
                        value=code
                        placeholder=untrack(|| t(keys::new_bible::CODE_PLACEHOLDER, &[]))
                        on_change=on_code
                    />
                </div>
                {offer}
                {direction(&actions, &np)}
                <div>
                    <Overline tag="span" style=crate::css! { "display": "block", "margin-bottom": 6 }>
                        {move || t(keys::new_bible::CHECKING_LANGUAGE, &[])}
                    </Overline>
                    <p style=note_style()>{move || t(if bible { keys::new_bible::GL_NOTE } else { keys::new_obs::GL_NOTE }, &[])}</p>
                    {gl_row()}
                </div>
                <Select
                    id=id("font")
                    label=Signal::derive(move || t(keys::new_bible::FONT, &[]))
                    value=Signal::derive(move || Some(font.get()))
                    on_change=on_font
                    options=fonts
                />
                {versification}
                {error}
            }
            .into_any()
        })
    };
    let close = actions.clone();
    let (title, subtitle) = if bible {
        (keys::new_bible::TITLE, keys::new_bible::SUBTITLE)
    } else {
        (keys::new_obs::TITLE, keys::new_obs::SUBTITLE)
    };
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some((move || move || t(title, &[])).into()),
        subtitle: Some((move || move || t(subtitle, &[])).into()),
        close_label: Some(Signal::derive(|| t(keys::new_bible::CLOSE, &[]))),
        on_close: Some(Callback::new(move |()| close.close_modal())),
        footer: Some(footer),
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}

/// The direction chips.
fn direction(actions: &Actions, np: &Signal<Option<NewProjectForm>>) -> impl IntoView + use<> {
    let chip = |dir: Direction, label: crate::i18n::Key| {
        let (actions, np) = (actions.clone(), *np);
        let selected =
            Signal::derive(move || np.with(|np| np.as_ref().is_some_and(|np| np.dir == dir)));
        view! {
            <FilterChip
                selected=selected
                style=crate::css! { "flex": 1, "justify-content": "center", "border-radius": "var(--radius-md)" }
                on_click=Callback::new(move |_| actions.patch_np(|np| np.dir = dir))
            >
                {move || t(label, &[])}
            </FilterChip>
        }
    };
    view! {
        <div>
            <Overline tag="span" style=crate::css! { "display": "block", "margin-bottom": 6 }>
                {move || t(keys::new_bible::DIRECTION, &[])}
            </Overline>
            <div style="display: flex; gap: 8px;">
                {chip(Direction::Ltr, keys::wizard::LTR)}
                {chip(Direction::Rtl, keys::wizard::RTL)}
            </div>
        </div>
    }
}

/// The versification disclosure: available but out of the way (tC4 D25).
fn versification(actions: &Actions, np: &Signal<Option<NewProjectForm>>) -> impl IntoView + use<> {
    let open = {
        let np = *np;
        Memo::new(move |_| np.with(|np| np.as_ref().is_some_and(|np| np.show_advanced)))
    };
    let toggle = {
        let actions = actions.clone();
        Callback::new(move |_| actions.patch_np(|np| np.show_advanced = !np.show_advanced))
    };
    let body = {
        let (actions, np) = (actions.clone(), *np);
        move || {
            open.get().then(|| {
                let names = np.with_untracked(|np| np.as_ref().map(|np| np.versifications.clone()).unwrap_or_default());
                let options: Vec<SelectOption> = names
                    .iter()
                    .map(|v| SelectOption {
                        value: v.clone(),
                        label: if v == "eng" {
                            format!("{v} {}", untrack(|| t(keys::wizard::DEFAULT, &[])))
                        } else {
                            v.clone()
                        },
                        ..SelectOption::from(v.as_str())
                    })
                    .collect();
                let value = {
                    Signal::derive(move || np.with(|np| np.as_ref().map(|np| np.versification.clone())))
                };
                let failed = np.with_untracked(|np| np.as_ref().is_some_and(|np| np.versifications_error.is_some()));
                let actions = actions.clone();
                view! {
                    <div style="margin-top: 8px;">
                        <Select
                            value=value
                            on_change=Callback::new(move |v: String| actions.patch_np(|np| np.versification = v))
                            options=options
                            hint=Signal::derive(move || t(keys::wizard::VERSIFICATION_HINT, &[]))
                            attrs=Attrs::new().with("aria-label", untrack(|| t(keys::wizard::VERSIFICATION, &[])))
                        />
                        {failed.then(|| view! {
                            <p data-testid="versifications-error" style="font-size: var(--fs-caption); color: var(--tc-warn-text); margin: 6px 0 0;">
                                {move || t(keys::wizard::VERSIFICATIONS_ERROR, &[])}
                            </p>
                        })}
                    </div>
                }
            })
        }
    };
    view! {
        <div>
            <Button
                variant=ButtonVariant::Ghost
                style=crate::css! { "font-size": "var(--fs-caption)", "letter-spacing": "var(--track-12)" }
                on_click=toggle
            >
                {move || if open.get() { "▾" } else { "▸" }}" "{move || t(keys::wizard::VERSIFICATION, &[])}
            </Button>
            {body}
        </div>
    }
}
