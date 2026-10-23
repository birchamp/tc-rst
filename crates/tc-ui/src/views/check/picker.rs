//! The tool picker (tC4 `Check`'s main, `PickerGrid`, `ToolCard`, `ToolNeeds`, `CardProgress`,
//! `OldTwlNotice`, `FallbackWarning`).

use leptos::prelude::*;
use tc_api::check::{
    AlignProgress, CardCount, CheckProgress, CheckUnit, Picker, PickerView, PinCitation, Rung,
    Tool, ToolCard, ToolState,
};
use tc_api::ids::Scope;
use wasm_bindgen::JsCast;

use crate::app::check::{
    CountLine, NextLine, align_done, bar_value, check_done, count_line, cta_key, min_fill,
    next_check, next_verse,
};
use crate::app::shell::View;
use crate::app::web::{Actions, AppState, use_runtime};
use crate::attrs::Attrs;
use crate::css::Css;
use crate::ds::{Button, ButtonVariant, Callout, CalloutTone, ProgressBar, Size, Tone};
use crate::i18n::{Key, keys, t, t_dyn};

const MONO: &str = "var(--font-mono)";

/// tC4's `openSources` buttons: the dialog opens over the picker, and a card around the button
/// does not also open its tool.
fn open_sources(actions: &Actions) -> Callback<leptos::ev::MouseEvent> {
    let actions = actions.clone();
    Callback::new(move |e: leptos::ev::MouseEvent| {
        e.stop_propagation();
        actions.open_sources();
    })
}

pub(super) fn tool_name(tool: Tool) -> Key {
    match tool {
        Tool::TranslationWords => keys::check::tool::TRANSLATION_WORDS,
        Tool::TranslationNotes => keys::check::tool::TRANSLATION_NOTES,
    }
}

pub(super) fn tool_id(tool: Tool) -> &'static str {
    match tool {
        Tool::TranslationWords => "translationWords",
        Tool::TranslationNotes => "translationNotes",
    }
}

/// tC4's preflight state as its `data-state` and keys write it. A pin with no commit has no tC4
/// state; its card reads as one with nothing pinned, whose action gets the source texts.
fn state_id(state: ToolState) -> &'static str {
    match state {
        ToolState::Ready => "ready",
        ToolState::Fetch => "fetch",
        ToolState::Unpinned | ToolState::UnusablePin => "unpinned",
        ToolState::NotCovered => "not-covered",
        ToolState::OldTwl => "old-twl",
    }
}

pub(super) fn rung_key(rung: Rung) -> Key {
    match rung {
        Rung::Primary => keys::check::rung::PRIMARY,
        Rung::Fallback => keys::check::rung::FALLBACK,
    }
}

/// The card's tint by state (tC4 `TONE`): background, border, label colour.
fn tone(state: &str) -> (&'static str, &'static str, &'static str) {
    match state {
        "ready" => (
            "var(--surface-card)",
            "var(--border)",
            "var(--tc-valid-strong)",
        ),
        "fetch" => (
            "var(--surface-accent-soft)",
            "rgba(49,173,227,.4)",
            "var(--tc-suggest-fg)",
        ),
        "old-twl" => (
            "var(--surface-warm)",
            "rgba(229,157,51,.4)",
            "var(--tc-warn-text)",
        ),
        _ => (
            "var(--surface-app)",
            "var(--border-input)",
            "var(--text-secondary)",
        ),
    }
}

fn picker_card() -> Css {
    crate::css! {
        "border": "var(--stroke) solid var(--border)",
        "background": "var(--surface-card)",
        "border-radius": "var(--radius-2xl)",
        "padding": 22,
        "text-align": "start",
        "display": "flex",
        "flex-direction": "column",
        "box-shadow": "var(--shadow-card)",
    }
}

fn picker_title() -> Css {
    crate::css! {
        "font-size": "var(--fs-title-lg)",
        "letter-spacing": "var(--track-19)",
        "font-weight": "var(--fw-black)",
        "color": "var(--text-heading)",
        "margin": "0 0 6px",
    }
}

fn picker_desc() -> Css {
    crate::css! {
        "font-size": "var(--fs-ui-sm)",
        "color": "var(--text-secondary)",
        "line-height": 1.55,
        "margin": "0 0 18px",
        "flex": 1,
    }
}

fn cta_style() -> Css {
    crate::css! {
        "display": "inline-flex",
        "align-items": "center",
        "gap": 7,
        "font-weight": "var(--fw-heavy)",
        "font-size": "var(--fs-ui)",
        "color": "var(--text-accent)",
    }
}

fn card_caption() -> Css {
    crate::css! {
        "font-size": "var(--fs-caption)",
        "letter-spacing": "var(--track-12)",
        "margin": "0 0 2px",
    }
}

/// tC4's `DroppedNote`: checks the project's numbering has no verse for, stated beside the
/// count they leave out (#15).
pub(super) fn dropped_note(
    dropped: Option<&tc_api::check::DroppedNote>,
    style: Css,
) -> Option<AnyView> {
    let dropped = dropped?;
    let s = crate::css! {
        "font-size": "var(--fs-caption-lg)",
        "color": "var(--tc-warn-text)",
        "line-height": "var(--lh-body)",
    }
    .merge(&style);
    let count = dropped.count.to_string();
    let scheme = dropped.scheme.clone().unwrap_or_else(|| "—".to_owned());
    Some(
        view! {
            <div data-testid="versification-dropped" style=s.to_style()>
                {t(keys::check::DROPPED_NOTE, &[("count", &count), ("scheme", &scheme)])}
            </div>
        }
        .into_any(),
    )
}

/// tC4's `CardProgress`: the bar, the count, the next line and the dropped note.
fn card_progress(
    kind: &'static str,
    loaded: bool,
    done: Option<(u32, u32)>,
    count: CountLine,
    next: NextLine,
    dropped: Option<&tc_api::check::DroppedNote>,
) -> impl IntoView + use<> {
    let (filled, total) = done.unwrap_or((0, 0));
    let failed = count == CountLine::Failed;
    let label = match count {
        CountLine::Pending => "—".to_owned(),
        CountLine::Failed => t(keys::check::PROGRESS_ERROR, &[]),
        CountLine::NoItems => t(keys::check::NO_ITEMS, &[]),
        CountLine::Counted { done, total } => {
            let (done, total) = (done.to_string(), total.to_string());
            if kind == "align" {
                t(
                    keys::align::PROGRESS_VERSES,
                    &[("done", &done), ("total", &total)],
                )
            } else {
                t(
                    keys::check::PROGRESS,
                    &[("decided", &done), ("total", &total)],
                )
            }
        }
    };
    let next = match next {
        NextLine::None => None,
        NextLine::AllResolved => Some(t(keys::check::ALL_RESOLVED, &[])),
        NextLine::Check {
            title,
            chapter,
            verse,
        } => Some(t(
            keys::check::NEXT,
            &[("quote", &title), ("c", &chapter), ("v", &verse)],
        )),
        NextLine::Verse(at) => Some(t(keys::align::NEXT_VERSE, &[("ref", &at)])),
    };
    let count_style = card_caption().merge(&crate::css! {
        "font-weight": "var(--fw-bold)",
        "color": if failed { "var(--tc-warn-text)" } else { "var(--text-secondary)" },
    });
    let next_style = card_caption().merge(&crate::css! {
        "color": "var(--text-tertiary)",
        "overflow": "hidden",
        "text-overflow": "ellipsis",
        "white-space": "nowrap",
    });
    let testid = format!("picker-progress-{kind}");
    // The count read to a screen reader beside a check bar (#764): only a real count, and only
    // on the helps tools' bars.
    let bar_label = (kind == "check" && !failed && total > 0).then(|| label.clone());
    view! {
        <div data-testid=testid data-loaded=if loaded { "1" } else { "0" } style="margin-bottom: 16px;">
            <ProgressBar
                tone=Tone::Valid
                value=bar_value(filled, total)
                height=6.0
                min_fill=if kind == "check" { min_fill(filled) } else { None }
                label=bar_label
                style=crate::css! { "margin-bottom": 9 }
            />
            <p style=count_style.to_style()>{label}</p>
            {next.map(|n| view! { <p style=next_style.to_style()>{n}</p> })}
            {dropped_note(dropped, crate::css! { "margin-top": 4 })}
        </div>
    }
}

/// The not-ready half of a card (tC4 `ToolNeeds`): the pin the tool wants and the one action
/// that gets it.
fn tool_needs(actions: &Actions, card: &ToolCard) -> Option<AnyView> {
    let state = state_id(card.state);
    if state == "ready" && card.fallback_for.is_none() {
        return None;
    }
    let pin = card.pin.as_ref().map(|pin| {
        let rung = card
            .rung
            .map(|r| format!(" · {}", t(rung_key(r), &[])))
            .unwrap_or_default();
        let s = crate::css! {
            "font-size": "var(--fs-meta)",
            "color": "var(--text-tertiary)",
            "font-family": MONO,
            "margin": "0 0 10px",
        };
        // tC4's text nodes: the repository, " · ", the version, the rung.
        let (repo, version) = (
            pin.repo_path.clone(),
            pin.version.clone().unwrap_or_default(),
        );
        view! { <p style=s.to_style()>{repo}" · "{version}{rung}</p> }
    });
    let tool = card.tool;
    let action = match state {
        "fetch" => {
            let actions = actions.clone();
            Some(
                view! {
                    <Button
                        size=Size::Sm
                        attrs=Attrs::from([("data-testid", format!("fix-{}", tool_id(tool)).as_str())])
                        on_click=Callback::new(move |_| actions.open_fix(tool))
                    >
                        {t(keys::check::fix::OPEN, &[])}
                    </Button>
                }
                .into_any(),
            )
        }
        "unpinned" | "not-covered" => Some(
            view! {
                <Button
                    size=Size::Sm
                    variant=ButtonVariant::Secondary
                    on_click=open_sources(actions)
                    style=crate::css! { "color": "var(--uw-ocean)" }
                >
                    {t(keys::check::fix::GET_RESOURCES, &[])}
                </Button>
            }
            .into_any(),
        ),
        _ => None,
    };
    Some(view! { {pin}{action} }.into_any())
}

/// tC4's `FallbackWarning`: the tool opens on the English fallback because the pinned primary
/// is not on this computer.
fn fallback_warning(actions: &Actions, tool: Tool, missing: &PinCitation) -> impl IntoView + use<> {
    let repo = missing.repo_path.clone();
    let version = missing.version.clone().unwrap_or_default();
    let open = open_sources(actions);
    view! {
        <Callout
            tone=CalloutTone::Warn
            style=crate::css! { "margin": "14px 0 10px" }
            attrs=Attrs::from([("data-testid", format!("fallback-warning-{}", tool_id(tool)).as_str())])
        >
            {t(keys::check::FALLBACK_WARN, &[("repo", &repo), ("version", &version)])}
            <div style="margin-top: 8px;">
                <Button
                    size=Size::Sm
                    variant=ButtonVariant::Secondary
                    attrs=Attrs::from([("data-testid", format!("fetch-primary-{}", tool_id(tool)).as_str())])
                    on_click=open
                    style=crate::css! { "color": "var(--tc-warn-text)", "border-color": "rgba(229,157,51,.5)" }
                >
                    {t(keys::check::fix::DOWNLOAD, &[])}
                </Button>
            </div>
        </Callout>
    }
}

/// What a unit is called in the picker's words: the book's name, or "Story 3".
pub(super) fn unit_label(unit: CheckUnit) -> String {
    match unit {
        CheckUnit::Book(book) => book.english_name().to_owned(),
        CheckUnit::Story(story) => crate::app::story::story_label(story),
    }
}

/// tC4's `ToolCard`.
fn tool_card(actions: &Actions, card: &ToolCard, label: &str) -> impl IntoView + use<> {
    let state = state_id(card.state);
    let ready = state == "ready";
    let (bg, border, fg) = tone(state);
    let tool = card.tool;
    let style = picker_card().merge(&crate::css! {
        "border": format!("var(--stroke) solid {border}"),
        "background": bg,
        "box-shadow": if ready { "var(--shadow-card)" } else { "none" },
        "cursor": if ready { "pointer" } else { "default" },
    });
    let title_style = crate::css! { "display": "flex", "align-items": "baseline", "gap": 10 }
        .merge(&picker_title());
    let label_style = crate::css! {
        "font-size": "var(--fs-label)",
        "font-weight": "var(--fw-heavy)",
        "letter-spacing": "var(--tracking-status)",
        "text-transform": "uppercase",
        "color": fg,
    };
    let book_name = label.to_owned();
    let desc = if ready {
        t_dyn(&format!("check.desc.{}", tool_id(tool)), &[], None)
    } else {
        t_dyn(
            &format!("check.explain.{state}"),
            &[("book", &book_name)],
            None,
        )
    };
    let progress = card.progress.as_ref();
    let done = check_done(progress);
    let open_button = {
        let actions = actions.clone();
        let label = t_dyn(cta_key(done), &[], None);
        let s = cta_style().merge(&crate::css! {
            "align-self": "flex-start",
            "border": 0,
            "background": "transparent",
            "padding": 0,
            "font-family": "inherit",
            "cursor": "pointer",
        });
        move || {
            let actions = actions.clone();
            view! {
                <button
                    type="button"
                    data-testid=format!("open-{}", tool_id(tool))
                    style=s.to_style()
                    on:click=move |e| {
                        e.stop_propagation();
                        actions.open_check_tool(tool);
                    }
                >
                    {label.clone()}" "{"→"}
                </button>
            }
        }
    };
    let progress_block = {
        let count = count_line(progress, |p: &CheckProgress| (p.decided, p.total));
        let next = next_check(progress);
        let dropped = match progress {
            Some(CardCount::Counted(p)) => p.dropped.clone(),
            _ => None,
        };
        move || {
            card_progress(
                "check",
                true,
                done,
                count.clone(),
                next.clone(),
                dropped.as_ref(),
            )
        }
    };
    let body = match (ready, card.fallback_for.as_ref()) {
        (true, Some(missing)) => view! {
            {open_button()}
            {fallback_warning(actions, tool, missing)}
            {progress_block()}
        }
        .into_any(),
        (true, None) => view! {
            {progress_block()}
            {open_button()}
        }
        .into_any(),
        (false, _) => ().into_any(),
    };
    let on_card = {
        let actions = actions.clone();
        move |_| {
            if ready {
                actions.open_check_tool(tool);
            }
        }
    };
    view! {
        <div
            data-testid=format!("preflight-{}", tool_id(tool))
            data-state=state
            data-i=ready.then_some("card")
            data-tone="accent"
            style=style.to_style()
            on:click=on_card
        >
            <div style=title_style.to_style()>
                <span>{t(tool_name(tool), &[])}</span>
                {(!ready).then(|| view! {
                    <span style=label_style.to_style()>{t_dyn(&format!("check.state.{state}"), &[], None)}</span>
                })}
            </div>
            <p style=picker_desc().to_style()>{desc}</p>
            {tool_needs(actions, card)}
            {body}
        </div>
    }
}

/// A whole card that is a button (tC4 `clickableCard`): Enter and Space on the card itself
/// press it.
fn card_keys(open: impl Fn() + Clone + 'static) -> impl Fn(web_sys::KeyboardEvent) + 'static {
    move |e: web_sys::KeyboardEvent| {
        let own = e.target() == e.current_target();
        if own && (e.key() == "Enter" || e.key() == " ") {
            e.prevent_default();
            open();
        }
    }
}

/// The Align card (Bible only).
fn align_card(actions: &Actions, progress: &CardCount<AlignProgress>) -> impl IntoView + use<> {
    let done = align_done(progress);
    let count = count_line(Some(progress), |p: &AlignProgress| (p.done, p.total));
    let next = next_verse(progress);
    let style = picker_card().merge(&crate::css! { "cursor": "pointer" });
    let open_style = cta_style().merge(&crate::css! { "align-self": "flex-start" });
    let next_place = match progress {
        CardCount::Counted(p) => p.next.as_deref().and_then(crate::app::align::parse_place),
        CardCount::Failed { .. } => None,
    };
    let open = {
        let actions = actions.clone();
        move || actions.start_aligning(next_place)
    };
    view! {
        <div
            data-testid="align-card"
            data-i="card"
            data-tone="accent"
            role="button"
            tabindex="0"
            style=style.to_style()
            on:click={
                let open = open.clone();
                move |_| open()
            }
            on:keydown=card_keys(open)
        >
            <div style=picker_title().to_style()>{t(keys::nav::ALIGN, &[])}</div>
            <p style=picker_desc().to_style()>{t(keys::align::CARD_BODY, &[])}</p>
            {card_progress("align", true, done, count, next, None)}
            <span data-testid="open-align" style=open_style.to_style()>
                {t_dyn(cta_key(done), &[], None)}" "{"→"}
            </span>
        </div>
    }
}

/// The Community Checking card: its only entry (D63).
fn community_card(actions: &Actions, unit: CheckUnit) -> impl IntoView + use<> {
    let style = picker_card().merge(&crate::css! { "cursor": "pointer" });
    let open_style = cta_style().merge(&crate::css! { "align-self": "flex-start" });
    let count_style = card_caption().merge(&crate::css! {
        "font-weight": "var(--fw-bold)",
        "color": "var(--text-secondary)",
    });
    let next_style = card_caption().merge(&crate::css! { "color": "var(--text-tertiary)" });
    // tC4 names the open chapter; the picker shows the book's first. A story's preview holds
    // every drafted story, not the open one (#454).
    let (whole, next) = match unit {
        CheckUnit::Book(book) => (
            t(keys::cc::WHOLE_CHAPTER, &[]),
            t(
                keys::cc::NEXT_READ,
                &[("ref", &format!("{} 1", book.english_name()))],
            ),
        ),
        CheckUnit::Story(_) => (
            t(keys::cc::ALL_DRAFTED_STORIES, &[]),
            t(keys::cc::NEXT_READ_STORIES, &[]),
        ),
    };
    let open = {
        let actions = actions.clone();
        move || actions.go(View::Publish)
    };
    let click = open.clone();
    view! {
        <div
            data-testid="community-checking-card"
            data-i="card"
            data-tone="accent"
            role="button"
            tabindex="0"
            style=style.to_style()
            on:click=move |_| click()
            on:keydown=card_keys(open)
        >
            <div style=picker_title().to_style()>{t(keys::cc::TITLE, &[])}</div>
            <p style=picker_desc().to_style()>{t(keys::cc::CARD_DESC, &[])}</p>
            <div style="margin-bottom: 16px;">
                <ProgressBar tone=Tone::Valid value=0.0 height=6.0 style=crate::css! { "margin-bottom": 9 } />
                <p style=count_style.to_style()>{whole}</p>
                <p style=next_style.to_style()>{next}</p>
            </div>
            <span data-testid="open-community-checking" style=open_style.to_style()>
                {t(keys::cc::OPEN, &[])}
            </span>
        </div>
    }
}

/// The focus goes back to the card that opened the session, once its cards are drawn, if
/// nothing else holds it (#42).
fn return_focus(s: &AppState) {
    let Some(tool) = s.c.return_to.get_untracked() else {
        return;
    };
    s.c.return_to.try_set(None);
    let document = document();
    let unmoved = document
        .active_element()
        .is_none_or(|el| document.body().is_some_and(|b| b.is_same_node(Some(&el))));
    if !unmoved {
        return;
    }
    let selector = format!(
        "[data-testid=\"preflight-{}\"] button:not([disabled])",
        tool_id(tool)
    );
    if let Some(button) = document
        .query_selector(&selector)
        .ok()
        .flatten()
        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = button.focus();
    }
}

fn grid(
    s: &AppState,
    actions: &Actions,
    view: &PickerView,
    unit: CheckUnit,
) -> impl IntoView + use<> {
    let style = crate::css! {
        "display": "grid",
        "grid-template-columns": "repeat(auto-fit,minmax(270px,1fr))",
        "gap": 16,
    };
    let label = unit_label(unit);
    let cards: Vec<AnyView> = view
        .tools
        .iter()
        .map(|card| tool_card(actions, card, &label).into_any())
        .collect();
    let s = s.clone();
    request_animation_frame(move || return_focus(&s));
    view! {
        <div style=style.to_style()>
            {cards}
            {view.align.as_ref().map(|progress| align_card(actions, progress))}
            {community_card(actions, unit)}
        </div>
    }
}

/// The picker's main.
pub(super) fn picker(
    s: &AppState,
    actions: &Actions,
    at: Scope,
    unit: CheckUnit,
) -> impl IntoView + use<> {
    let Some(runtime) = use_runtime() else {
        return ().into_any();
    };
    let read = runtime.cache().read(Picker { at, unit });
    let main = crate::css! {
        "flex": 1,
        "overflow": "auto",
        "padding": "44px 40px 64px",
        "background": "var(--surface-app)",
    };
    let inner = crate::css! { "max-width": "var(--measure-page)", "margin": "0 auto" };
    let h1 = crate::css! {
        "font-size": "var(--fs-h1)",
        "letter-spacing": "var(--track-32)",
        "margin": "0 0 6px",
    };
    let sub_row = crate::css! {
        "display": "flex",
        "align-items": "baseline",
        "gap": 12,
        "margin": "0 0 30px",
    };
    let sub = crate::css! {
        "font-size": "var(--fs-body)",
        "letter-spacing": "var(--track-15)",
        "color": "var(--text-secondary)",
        "margin": 0,
        "max-width": 640,
        "flex": 1,
    };
    let note = crate::css! {
        "font-size": "var(--fs-caption-lg)",
        "color": "var(--text-tertiary)",
        "line-height": "var(--lh-body)",
        "margin": "20px 0 0",
    };
    let body = {
        let (s, actions) = (s.clone(), actions.clone());
        move || {
            read.with(|load| {
                if let Some(error) = load.error() {
                    let message = error.to_string();
                    let retry = actions.clone();
                    return Some(
                        view! {
                            <Callout
                                tone=CalloutTone::Warn
                                style=crate::css! { "margin-bottom": 16, "overflow-wrap": "anywhere" }
                                attrs=Attrs::from([("role", "alert"), ("data-testid", "preflight-error")])
                            >
                                {t(keys::check::PREFLIGHT_ERROR, &[])}" "{message}" "
                                <Button
                                    size=Size::Sm
                                    variant=ButtonVariant::Outline
                                    attrs=Attrs::from([("data-testid", "preflight-retry")])
                                    on_click=Callback::new(move |_| retry.reload_picker(unit))
                                >
                                    {t(keys::app::RETRY, &[])}
                                </Button>
                            </Callout>
                        }
                        .into_any(),
                    );
                }
                let Some(view) = load.value() else {
                    let s = crate::css! { "font-size": "var(--fs-ui)", "color": "var(--text-tertiary)" };
                    return Some(view! { <p style=s.to_style()>{t(keys::check::CHECKING, &[])}</p> }.into_any());
                };
                let old_twl = view.old_twl.clone().map(|repo| {
                    view! {
                        <Callout
                            tone=CalloutTone::Warn
                            style=crate::css! { "margin-bottom": 16, "overflow-wrap": "anywhere" }
                            attrs=Attrs::from([("role", "alert"), ("data-testid", "pins-old-twl")])
                        >
                            {t(keys::check::OLD_TWL, &[])}" "{repo}
                        </Callout>
                    }
                });
                let blocked = view.tools.iter().any(|c| c.state != ToolState::Ready);
                Some(
                    view! {
                        {old_twl}
                        {grid(&s, &actions, view, unit)}
                        {blocked.then(|| view! { <p style=note.to_style()>{t(keys::check::NEVER_BLOCKS, &[])}</p> })}
                    }
                    .into_any(),
                )
            })
        }
    };
    view! {
        <main style=main.to_style()>
            <div style=inner.to_style()>
                <h1 style=h1.to_style()>{t(keys::check::TITLE, &[])}</h1>
                <div style=sub_row.to_style()>
                    <p style=sub.to_style()>{t(keys::check::SUBTITLE, &[])}</p>
                </div>
                {body}
            </div>
        </main>
    }
    .into_any()
}
