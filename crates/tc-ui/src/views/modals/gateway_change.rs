//! The change of checking language (M10; unit C8; `tC4:src/views/modals/GatewayChange.jsx`).
//! This dialog is where the warning lives: changing which gateway language a project checks
//! against is the only thing that moves a checked book to another resource, so what it costs is
//! counted and shown before anything is written, with a way to decline. The numbers are the
//! outcome the person will see afterwards, per book, from the new notes' lists already derived.
//! After Change neither button answers until the change ends (tC4 #580), and a change that is
//! not saved leaves the dialog on its plan with the reason, ready to try again.

use leptos::prelude::*;
use tc_api::resources::{GatewayPackage, ImageChange, PlanView, PlanWhat};

use crate::app::upgrades::{
    GatewayPreview, GatewayState, blocked_text, gateway_headline, row_text,
};
use crate::app::web::{Actions, AppCtx, use_app};
use crate::ds::{Attrs, Button, ButtonVariant, Callout, CalloutTone, Css};
use crate::i18n::{keys, t};

/// The dialog, while the gateway state holds a preview.
#[component]
pub fn GatewayChange() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let gw = s.gw.clone();
    let shown = Memo::new(move |_| gw.with(|gw: &GatewayState| gw.preview.clone()));
    let state = {
        let gw = s.gw.clone();
        move || gw.with(|gw| (gw.busy, gw.error.clone()))
    };
    (move || {
        let (busy, error) = state();
        shown.get().map(|preview| match preview {
            GatewayPreview::Failed { package } => failed(&actions, &package, error),
            GatewayPreview::Plan { package, plan } => {
                planned(&actions, &package, &plan, busy, error)
            }
        })
    })
    .into_any()
}

fn title(package: &GatewayPackage) -> String {
    t(keys::gateway::TITLE, &[("lang", &package.autonym)])
}

/// A plan that could not be made: the error, and Close; no counts and no Change, which could
/// mistake a failed read for success.
fn failed(actions: &Actions, package: &GatewayPackage, error: Option<String>) -> AnyView {
    let (cancel, close) = (actions.clone(), actions.clone());
    let footer: ChildrenFn = std::sync::Arc::new(move || {
        let cancel = cancel.clone();
        view! {
            <Button
                variant=ButtonVariant::Secondary
                attrs=Attrs::from([("data-testid", "gateway-cancel")])
                on_click=Callback::new(move |_| cancel.cancel_gateway_change())
            >
                {t(keys::common::CLOSE, &[])}
            </Button>
        }
        .into_any()
    });
    let children: ChildrenFn = std::sync::Arc::new(move || error_callout(error.clone()).into_any());
    crate::ds::Modal(crate::ds::ModalProps {
        attrs: Attrs::from([("data-testid", "gateway-change")]),
        title: Some(title(package).into()),
        close_label: Some(Signal::derive(|| t(keys::common::CLOSE, &[]))),
        on_close: Some(Callback::new(move |()| close.cancel_gateway_change())),
        footer: Some(footer),
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}

fn error_callout(error: Option<String>) -> Option<impl IntoView + use<>> {
    error.map(|error| {
        view! {
            <Callout
                tone=CalloutTone::Warn
                attrs=Attrs::from([("role", "alert"), ("data-testid", "gateway-error")])
                style=crate::css! { "overflow-wrap": "anywhere" }
            >
                <strong>{t(keys::gateway::FAILED, &[])}</strong>" "{error}
            </Callout>
        }
    })
}

/// What the story pictures change from and to (tC4 `ImageChange`).
fn image_change(change: &ImageChange) -> impl IntoView + use<> {
    let label = |repo: &Option<String>| {
        repo.clone()
            .unwrap_or_else(|| t(keys::gateway::BUNDLED_IMAGES, &[]))
    };
    let text = t(
        keys::gateway::IMAGE_CHANGE,
        &[("from", &label(&change.from)), ("to", &label(&change.to))],
    );
    view! {
        <p data-testid="gateway-image-change" style="font-size: var(--fs-ui-sm); color: var(--text-secondary); line-height: var(--lh-body); margin: 0px;">
            {text}
        </p>
    }
}

fn planned(
    actions: &Actions,
    package: &GatewayPackage,
    plan: &PlanView,
    busy: bool,
    error: Option<String>,
) -> AnyView {
    let harmless = plan.harmless();
    let blocked = !plan.blocked.is_empty();
    let (headline, detail) = gateway_headline(plan);
    let rows: Vec<String> = plan.rows.iter().map(row_text).collect();
    let blocked_message = blocked.then(|| blocked_text(&plan.blocked));
    let (current, images) = match &plan.what {
        PlanWhat::Gateway(change) => (
            change.from.as_ref().map(|from| from.autonym.clone()),
            change.images.clone(),
        ),
        _ => (None, None),
    };
    let keep_label = t(
        keys::gateway::KEEP,
        &[(
            "lang",
            &current
                .clone()
                .unwrap_or_else(|| t(keys::gateway::CURRENT, &[])),
        )],
    );
    let change_label = t(keys::gateway::CHANGE, &[("lang", &package.autonym)]);
    let change_style = if blocked {
        crate::css! { "background": "var(--uw-haze)", "box-shadow": "none" }
    } else if !harmless {
        crate::css! { "background": "var(--uw-kindle)" }
    } else {
        Css::new()
    };
    let footer: ChildrenFn = {
        let actions = actions.clone();
        std::sync::Arc::new(move || {
            let (keep, change) = (actions.clone(), actions.clone());
            let (keep_label, change_label, style) = (
                keep_label.clone(),
                change_label.clone(),
                change_style.clone(),
            );
            view! {
                <Button
                    variant=ButtonVariant::Secondary
                    disabled=busy
                    attrs=Attrs::from([("data-testid", "gateway-cancel")])
                    on_click=Callback::new(move |_| keep.cancel_gateway_change())
                >
                    {keep_label}
                </Button>
                <Button
                    disabled=blocked || busy
                    style=style
                    attrs=Attrs::from([("data-testid", "gateway-confirm")])
                    on_click=Callback::new(move |_| change.confirm_gateway_change())
                >
                    {change_label}
                </Button>
            }
            .into_any()
        })
    };
    let children: ChildrenFn = std::sync::Arc::new(move || {
        let (headline, detail, rows) = (headline.clone(), detail.clone(), rows.clone());
        let (blocked_message, error, images) =
            (blocked_message.clone(), error.clone(), images.clone());
        view! {
            <div data-harmless=if harmless { "1" } else { "0" } style="display: flex; flex-direction: column; gap: 8px;">
                <p data-testid="gateway-headline" style=format!("font-size: var(--fs-ui); letter-spacing: var(--track-13-5); color: {}; font-weight: var(--fw-bold); line-height: var(--lh-body); margin: 0px;", if harmless { "var(--tc-valid-strong)" } else { "var(--tc-warn-text)" })>
                    {headline}
                </p>
                {detail.map(|detail| view! {
                    <p style="font-size: var(--fs-ui-sm); letter-spacing: var(--track-13); color: var(--text-secondary); line-height: var(--lh-body); margin: 0px;">{detail}</p>
                })}
                {images.as_ref().map(image_change)}
                {(!harmless).then(|| view! {
                    <ul data-testid="gateway-plan" style="margin: 6px 0px 0px; padding-inline-start: 18px;">
                        {rows.into_iter().map(|r| view! {
                            <li style="font-size: var(--fs-caption-lg); letter-spacing: var(--track-12-5); color: var(--text-secondary); line-height: 1.7;">{r}</li>
                        }).collect_view()}
                    </ul>
                })}
                {blocked_message.map(|text| view! {
                    <Callout tone=CalloutTone::Warn attrs=Attrs::from([("role", "alert"), ("data-testid", "gateway-blocked")])>
                        {text}
                    </Callout>
                })}
                {error_callout(error)}
            </div>
        }
        .into_any()
    });
    let close = actions.clone();
    crate::ds::Modal(crate::ds::ModalProps {
        attrs: Attrs::from([("data-testid", "gateway-change")]),
        title: Some(title(package).into()),
        close_label: Some(Signal::derive(|| t(keys::common::CLOSE, &[]))),
        on_close: Some(Callback::new(move |()| close.cancel_gateway_change())),
        closable: Signal::from(!busy),
        footer: Some(footer),
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}
