//! The guided fix (M11; `tC4:src/views/modals/GuidedFix.jsx`, #9, D72 point 6): a tool's pinned
//! resource is not on this computer, and the dialog names it and the three ways to get it:
//! download the pinned version, move the pin to an installed version of the same repository, or
//! install it from a file that declares the pinned commit. Opening it changes nothing. B5a drew
//! it; B5b wires the three ways to C8's endpoints (`resources/fixFetch`, `fixSideload`,
//! `fixRepin`). One way runs at a time, and its progress and a failure show above them.

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::resources::{FixView, PinLabel};

use crate::app::fix::{Fix, Way};
use crate::app::web::{Actions, AppCtx, use_app};
use crate::attrs::Attrs;
use crate::ds::{
    Button, ButtonVariant, Callout, CalloutTone, DropZone, OptionCard, Overline, Size,
};
use crate::i18n::{keys, t};

/// `es-419_twl · v18`, or the first 12 digits of the commit with no tag (tC4's `label`).
fn label(pin: &PinLabel) -> String {
    let repo = pin.repo_path.rsplit('/').next().unwrap_or(&pin.repo_path);
    let version = pin
        .version
        .clone()
        .unwrap_or_else(|| pin.sha.chars().take(12).collect());
    format!("{repo} · {version}")
}

fn repo_name(pin: &PinLabel) -> String {
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

/// What the screen shown is doing: its way on its way, its progress and its error.
#[derive(Clone)]
struct Doing {
    busy: Signal<Option<Way>>,
    progress: Signal<Option<String>>,
    error: Signal<Option<String>>,
}

impl Doing {
    fn of(fix: &ArcRwSignal<Option<Fix>>) -> Self {
        let field = |read: fn(&crate::app::fix::FixScreen) -> Option<String>| {
            let fix = fix.clone();
            Signal::derive(move || {
                fix.with(|f| match f {
                    Some(Fix::Ready(screen)) => read(screen),
                    _ => None,
                })
            })
        };
        let busy = {
            let fix = fix.clone();
            Signal::derive(move || {
                fix.with(|f| match f {
                    Some(Fix::Ready(screen)) => screen.busy,
                    _ => None,
                })
            })
        };
        Self {
            busy,
            progress: field(|screen| screen.progress.clone()),
            error: field(|screen| screen.error.clone()),
        }
    }
}

fn options(actions: &Actions, fix: &FixView, doing: Doing) -> AnyView {
    let pin = label(&fix.pin);
    let sha12: String = fix.pin.sha.chars().take(12).collect();
    let Doing {
        busy,
        progress,
        error,
    } = doing;
    let candidates: Vec<AnyView> = fix
        .candidates
        .iter()
        .map(|c| {
            let id: String = c.sha.chars().take(12).collect();
            let sha = c.sha.clone();
            let testid = format!("fix-repin-{id}");
            let repin = {
                let (actions, sha) = (actions.clone(), sha.clone());
                Callback::new(move |_| actions.fix_repin(sha.clone()))
            };
            view! {
                <OptionCard
                    title=label(c)
                    meta=t(keys::fix::repin::INSTALLED, &[])
                    description=move || view! { <span style=mono().to_style()>{sha.clone()}</span> }
                    on_click=repin
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
    let fetch = {
        let actions = actions.clone();
        Callback::new(move |_| actions.fix_fetch())
    };
    let fetch_label = move || {
        if busy.get() == Some(Way::Fetch) {
            t(keys::upgrade::INSTALLING, &[])
        } else {
            t(keys::check::fix::DOWNLOAD, &[])
        }
    };
    let input = NodeRef::<leptos::html::Input>::new();
    let sideload = {
        let actions = actions.clone();
        move |files: Option<web_sys::FileList>| {
            if let Some(file) = files.and_then(|files| files.get(0)) {
                actions.fix_sideload(file);
            }
        }
    };
    let on_change = {
        let sideload = sideload.clone();
        move |_| {
            if let Some(input) = input.get() {
                sideload(input.files());
                // The same file can be chosen again after a failure.
                input.set_value("");
            }
        }
    };
    let on_drop = move |event: web_sys::DragEvent| {
        event.prevent_default();
        sideload(event.data_transfer().and_then(|transfer| transfer.files()));
    };
    let browse = Callback::new(move |_: web_sys::Event| {
        if let Some(input) = input.get() {
            input.click();
        }
    });
    let drop_title = Signal::derive(move || {
        if busy.get() == Some(Way::Sideload) {
            t(keys::fix::sideload::READING, &[("name", &"")])
        } else {
            t(keys::fix::sideload::DROP, &[])
        }
    });
    let progress_style = crate::css! {
        "margin": 0,
        "font-size": "var(--fs-caption-lg)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--text-secondary)",
    }
    .to_style();
    view! {
        <p data-testid="fix-pin" style=mono().merge(&crate::css! { "margin": 0 }).to_style()>
            // Three text nodes, as tC4's JSX makes them: the browser shapes each on its own.
            {fix.pin.repo_path.clone()}" · "{fix.pin.sha.clone()}
        </p>
        {move || progress.get().map(|text| view! { <p data-testid="fix-progress" style=progress_style.clone()>{text}</p> })}
        {move || error.get().map(|text| view! {
            <Callout tone=CalloutTone::Warn style=crate::css! { "overflow-wrap": "anywhere" } attrs=Attrs::from([("role", "alert"), ("data-testid", "fix-error")])>
                {text}
            </Callout>
        })}
        <section data-testid="fix-fetch" style=section_style()>
            <Overline tag="span">{t(keys::fix::fetch::TITLE, &[])}</Overline>
            <p style=body_style()>{t(keys::fix::fetch::BODY, &[("pin", &pin)])}</p>
            <div style="display: flex; gap: 8px;">
                <Button
                    size=Size::Sm
                    disabled=Signal::derive(move || busy.get().is_some())
                    on_click=fetch
                    attrs=Attrs::from([("data-testid", "fix-fetch-go")])
                >
                    {fetch_label}
                </Button>
            </div>
        </section>
        <section data-testid="fix-repin" data-candidates=fix.candidates.len().to_string() style=section_style()>
            <Overline tag="span">{t(keys::fix::repin::TITLE, &[])}</Overline>
            <p style=body_style()>{repin_body}</p>
            {candidates}
        </section>
        <section
            data-testid="fix-sideload"
            style=section_style()
            on:dragover=move |event: web_sys::DragEvent| event.prevent_default()
            on:drop=on_drop
        >
            <Overline tag="span">{t(keys::fix::sideload::TITLE, &[])}</Overline>
            <p style=body_style()>{t(keys::fix::sideload::BODY, &[("pin", &pin), ("sha", &sha12)])}</p>
            <input
                node_ref=input
                type="file"
                accept=".zip,application/zip"
                data-testid="fix-sideload-file"
                hidden=true
                on:change=on_change
            />
            <DropZone
                title=drop_title
                hint=t(keys::fix::sideload::HINT, &[])
                disabled=Signal::derive(move || busy.get().is_some())
                on_click=browse
                attrs=Attrs::from([("data-testid", "fix-sideload-drop")])
            />
        </section>
    }
    .into_any()
}

#[derive(Clone, PartialEq)]
enum Shown {
    Nothing,
    Ready(u64, FixView),
    Failed(String),
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
            let tool = fix.with(|f| f.as_ref().map(Fix::tool));
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
                Some(Fix::Ready(screen)) => t(
                    keys::fix::SUBTITLE,
                    &[("pin", &label(&screen.view.pin)), ("book", &book)],
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
    // The options are drawn once per screen; what a way is doing changes inside them, so the
    // control the person pressed keeps the focus.
    let shown = {
        let fix = fix.clone();
        Memo::new(move |_| {
            fix.with(|f| match f {
                Some(Fix::Ready(screen)) => Shown::Ready(screen.id, screen.view.clone()),
                Some(Fix::Failed(_, error)) => Shown::Failed(error.clone()),
                Some(Fix::Loading(_)) | None => Shown::Nothing,
            })
        })
    };
    let doing = Doing::of(&fix);
    let children: ChildrenFn = Arc::new(move || {
        let (actions, doing) = (actions.clone(), doing.clone());
        (move || match shown.get() {
            Shown::Ready(_, view) => options(&actions, &view, doing.clone()),
            Shown::Failed(error) => view! {
                <Callout tone=CalloutTone::Warn style=crate::css! { "overflow-wrap": "anywhere" } attrs=Attrs::from([("role", "alert"), ("data-testid", "fix-error")])>
                    {error}
                </Callout>
            }
            .into_any(),
            Shown::Nothing => ().into_any(),
        })
        .into_any()
    });
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
