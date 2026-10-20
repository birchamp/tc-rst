//! The guided fix (M11, unit B5a; `tC4:src/views/modals/GuidedFix.jsx`, #9, D72 point 6): a
//! tool's pinned resource is not on this computer, and the dialog names it and the three ways to
//! get it: download the pinned version, move the pin to an installed version of the same
//! repository, or install it from a file that declares the pinned commit. Opening it changes
//! nothing. The three ways run with C8; until then their controls are drawn and do nothing.

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::check::{FixView, PinCitation};

use crate::app::web::check::Fix;
use crate::app::web::{AppCtx, use_app};
use crate::attrs::Attrs;
use crate::ds::{
    Button, ButtonVariant, Callout, CalloutTone, DropZone, OptionCard, Overline, Size,
};
use crate::i18n::{keys, t};

/// `es-419_twl · v18`, or the first 12 digits of the commit with no tag (tC4's `label`).
fn label(pin: &PinCitation) -> String {
    let repo = pin.repo_path.rsplit('/').next().unwrap_or(&pin.repo_path);
    let version = pin
        .version
        .clone()
        .unwrap_or_else(|| pin.sha.chars().take(12).collect());
    format!("{repo} · {version}")
}

fn repo_name(pin: &PinCitation) -> String {
    pin.repo_path
        .rsplit('/')
        .next()
        .unwrap_or(&pin.repo_path)
        .to_owned()
}

fn section_style() -> String {
    crate::css! { "display": "flex", "flex-direction": "column", "gap": 8 }.to_style()
}

fn body_style() -> String {
    crate::css! {
        "margin": 0,
        "font-size": "var(--fs-ui-sm)",
        "color": "var(--text-secondary)",
        "line-height": "var(--lh-body)",
    }
    .to_style()
}

fn mono() -> crate::css::Css {
    crate::css! {
        "font-family": "var(--font-mono)",
        "font-size": "var(--fs-caption)",
        "color": "var(--text-tertiary)",
    }
}

fn options(fix: &FixView) -> AnyView {
    let pin = label(&fix.pin);
    let sha12: String = fix.pin.sha.chars().take(12).collect();
    let candidates: Vec<AnyView> = fix
        .candidates
        .iter()
        .map(|c| {
            let id: String = c.sha.chars().take(12).collect();
            let sha = c.sha.clone();
            let testid = format!("fix-repin-{id}");
            view! {
                <OptionCard
                    title=label(c)
                    meta=t(keys::fix::repin::INSTALLED, &[])
                    description=move || view! { <span style=mono().to_style()>{sha.clone()}</span> }
                    attrs=Attrs::from([("data-testid", testid.as_str())])
                />
            }
            .into_any()
        })
        .collect();
    let repin_body = if fix.candidates.is_empty() {
        t(keys::fix::repin::NONE, &[("repo", &repo_name(&fix.pin))])
    } else {
        t(keys::fix::repin::BODY, &[])
    };
    view! {
        <p data-testid="fix-pin" style=mono().merge(&crate::css! { "margin": 0 }).to_style()>
            // Three text nodes, as tC4's JSX makes them: the browser shapes each on its own.
            {fix.pin.repo_path.clone()}" · "{fix.pin.sha.clone()}
        </p>
        <section data-testid="fix-fetch" style=section_style()>
            <Overline tag="span">{t(keys::fix::fetch::TITLE, &[])}</Overline>
            <p style=body_style()>{t(keys::fix::fetch::BODY, &[("pin", &pin)])}</p>
            <div style="display: flex; gap: 8px;">
                <Button size=Size::Sm attrs=Attrs::from([("data-testid", "fix-fetch-go")])>
                    {t(keys::check::fix::DOWNLOAD, &[])}
                </Button>
            </div>
        </section>
        <section data-testid="fix-repin" data-candidates=fix.candidates.len().to_string() style=section_style()>
            <Overline tag="span">{t(keys::fix::repin::TITLE, &[])}</Overline>
            <p style=body_style()>{repin_body}</p>
            {candidates}
        </section>
        <section data-testid="fix-sideload" style=section_style()>
            <Overline tag="span">{t(keys::fix::sideload::TITLE, &[])}</Overline>
            <p style=body_style()>{t(keys::fix::sideload::BODY, &[("pin", &pin), ("sha", &sha12)])}</p>
            <input type="file" accept=".zip,application/zip" data-testid="fix-sideload-file" hidden=true />
            <DropZone
                title=t(keys::fix::sideload::DROP, &[])
                hint=t(keys::fix::sideload::HINT, &[])
                attrs=Attrs::from([("data-testid", "fix-sideload-drop")])
            />
        </section>
    }
    .into_any()
}

/// The dialog, while `s.modal` is the guided fix.
#[component]
pub fn GuidedFix() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let fix = s.c.fix.clone();
    let book = s.u.book.clone();
    let title = {
        let fix = fix.clone();
        Signal::derive(move || {
            let tool = fix.with(|f| match f {
                Some(Fix::Ready(view)) => Some(view.tool),
                Some(Fix::Loading(tool) | Fix::Failed(tool, _)) => Some(*tool),
                None => None,
            });
            let tool = tool
                .map(|tool| t(crate::views::check::tool_name_key(tool), &[]))
                .unwrap_or_default();
            t(keys::fix::TITLE, &[("tool", &tool)])
        })
    };
    let subtitle = {
        let fix = fix.clone();
        Signal::derive(move || {
            let book = book
                .get()
                .map(|b| b.english_name().to_owned())
                .unwrap_or_default();
            fix.with(|f| match f {
                Some(Fix::Ready(view)) => t(
                    keys::fix::SUBTITLE,
                    &[("pin", &label(&view.pin)), ("book", &book)],
                ),
                _ => String::new(),
            })
        })
    };
    let close = {
        let actions = actions.clone();
        Callback::new(move |()| actions.close_modal())
    };
    let footer: ChildrenFn = {
        let actions = actions.clone();
        Arc::new(move || {
            let actions = actions.clone();
            view! {
                <Button
                    variant=ButtonVariant::Secondary
                    attrs=Attrs::from([("data-testid", "fix-close")])
                    on_click=Callback::new(move |_| actions.close_modal())
                >
                    {t(keys::common::CLOSE, &[])}
                </Button>
            }
            .into_any()
        })
    };
    let children: ChildrenFn = {
        let fix = fix.clone();
        Arc::new(move || {
            let fix = fix.clone();
            (move || {
                fix.with(|f| match f {
                    Some(Fix::Ready(view)) => options(view),
                    Some(Fix::Failed(_, error)) => {
                        let error = error.clone();
                        view! {
                            <Callout tone=CalloutTone::Warn style=crate::css! { "overflow-wrap": "anywhere" } attrs=Attrs::from([("role", "alert"), ("data-testid", "fix-error")])>
                                {error}
                            </Callout>
                        }
                        .into_any()
                    }
                    _ => ().into_any(),
                })
            })
            .into_any()
        })
    };
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some((move || move || title.get()).into()),
        subtitle: Some((move || move || subtitle.get()).into()),
        width: Some(620.into()),
        close_label: Some(Signal::derive(|| t(keys::common::CLOSE, &[]))),
        on_close: Some(close),
        footer: Some(footer),
        attrs: Attrs::from([("data-testid", "guided-fix")]),
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}
