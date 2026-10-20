//! The release upgrade confirmation (M9; unit C8; `tC4:src/views/modals/UpgradeSet.jsx`): one
//! language set's new release, or one scripture text's, with what moving the pins costs. The
//! release is already installed and verified when this opens; what the person confirms is the
//! pin move and its exact cost: per (tool, book), how many decisions carry over and how many
//! come back as work. "Keep the current release" leaves the pins, the decisions and the old
//! release as they are. The guided fix's re-pin is the same confirmation under another title.

use leptos::prelude::*;
use tc_api::resources::{PlanView, PlanWhat, RepoMove, SetRung, TextKind};

use crate::app::upgrades::{Preview, blocked_text, move_text, row_text, text_headline};
use crate::app::web::{Actions, AppCtx, use_app};
use crate::ds::{Attrs, Button, ButtonVariant, Callout, CalloutTone, Css};
use crate::i18n::{keys, t};

/// The dialog, while the upgrade state holds a plan to confirm.
#[component]
pub fn UpgradeSet() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let up = s.up.clone();
    let preview = Memo::new(move |_| up.with(|up| up.preview.clone()));
    let error = {
        let up = s.up.clone();
        move || up.with(|up| up.error.clone())
    };
    (move || preview.get().map(|p| modal(&actions, p, error()))).into_any()
}

fn set_title(rung: SetRung, language: &str) -> String {
    let key = match rung {
        SetRung::Primary => keys::upgrade::set::PRIMARY,
        SetRung::Fallback => keys::upgrade::set::FALLBACK,
    };
    t(key, &[("lang", &language)])
}

fn modal(actions: &Actions, preview: Preview, error: Option<String>) -> AnyView {
    let Preview { plan, applying } = preview;
    let harmless = plan.harmless();
    let blocked = !plan.blocked.is_empty();
    let (kind, title, footer_apply, body) = match &plan.what {
        PlanWhat::Set {
            rung,
            language,
            moves,
        } => (
            "upgrade",
            t(
                keys::upgrade::CONFIRM_TITLE,
                &[("set", &set_title(*rung, language))],
            ),
            t(keys::upgrade::APPLY, &[]),
            set_body(&plan, moves, harmless, error),
        ),
        PlanWhat::Repin {
            rung,
            language,
            moves,
        } => (
            "repin",
            t(
                keys::fix::repin::CONFIRM_TITLE,
                &[("set", &set_title(*rung, language))],
            ),
            t(keys::fix::repin::APPLY, &[]),
            set_body(&plan, moves, harmless, error),
        ),
        PlanWhat::Text { kind, change } => (
            "text",
            t(keys::upgrade::TEXT_CONFIRM_TITLE, &[("repo", &change.repo)]),
            t(keys::upgrade::APPLY, &[]),
            text_body(*kind, change, plan.alignments, error),
        ),
        // A gateway plan has its own dialog.
        PlanWhat::Gateway(_) => return ().into_any(),
    };
    // A text's Upgrade is not disabled by a block (it has none) and is kindled when it marks
    // alignments; a set's is disabled while blocked and kindled unless harmless.
    let marks = plan.alignments.is_some_and(|n| n > 0);
    let kindled = match &plan.what {
        PlanWhat::Text { .. } => marks,
        _ => !harmless && !blocked,
    };
    let apply_style = if blocked {
        crate::css! { "background": "var(--uw-haze)", "box-shadow": "none" }
    } else if kindled {
        crate::css! { "background": "var(--uw-kindle)" }
    } else {
        Css::new()
    };
    let footer: ChildrenFn = {
        let actions = actions.clone();
        std::sync::Arc::new(move || {
            let (keep, apply) = (actions.clone(), actions.clone());
            let (label, style) = (footer_apply.clone(), apply_style.clone());
            view! {
                <Button
                    variant=ButtonVariant::Secondary
                    disabled=applying
                    attrs=Attrs::from([("data-testid", "upgrade-cancel")])
                    on_click=Callback::new(move |_| keep.cancel_upgrade())
                >
                    {t(keys::upgrade::KEEP, &[])}
                </Button>
                <Button
                    disabled=blocked || applying
                    style=style
                    attrs=Attrs::from([("data-testid", "upgrade-apply")])
                    on_click=Callback::new(move |_| apply.confirm_upgrade())
                >
                    {label}
                </Button>
            }
            .into_any()
        })
    };
    let children = body;
    let close = actions.clone();
    crate::ds::Modal(crate::ds::ModalProps {
        attrs: Attrs::from([("data-testid", "upgrade-confirm"), ("data-kind", kind)]),
        title: Some(title.into()),
        close_label: Some(Signal::derive(|| t(keys::common::CLOSE, &[]))),
        on_close: Some(Callback::new(move |()| close.cancel_upgrade())),
        closable: Signal::from(!applying),
        footer: Some(footer),
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}

const MOVE: &str = "margin: 0px; padding-inline-start: 18px;";
const MOVE_ROW: &str = "font-size: var(--fs-caption-lg); font-family: var(--font-mono); color: var(--text-secondary); line-height: 1.7;";

fn headline_style(good: bool) -> String {
    format!(
        "font-size: var(--fs-ui); letter-spacing: var(--track-13-5); color: {}; font-weight: var(--fw-bold); line-height: var(--lh-body); margin: 0px;",
        if good {
            "var(--tc-valid-strong)"
        } else {
            "var(--tc-warn-text)"
        }
    )
}

const DETAIL: &str = "font-size: var(--fs-ui-sm); letter-spacing: var(--track-13); color: var(--text-secondary); line-height: var(--lh-body); margin: 0px;";

fn error_callout(error: Option<String>) -> Option<impl IntoView + use<>> {
    error.map(|error| {
        view! {
            <Callout
                tone=CalloutTone::Warn
                attrs=Attrs::from([("role", "alert"), ("data-testid", "upgrade-confirm-error")])
                style=crate::css! { "overflow-wrap": "anywhere" }
            >
                {error}
            </Callout>
        }
    })
}

/// A set's body (tC4 `Consequences`, `BlockedCallout`).
fn set_body(
    plan: &PlanView,
    moves: &[RepoMove],
    harmless: bool,
    error: Option<String>,
) -> ChildrenFn {
    let moves: Vec<String> = moves.iter().map(move_text).collect();
    let rows: Vec<String> = plan.rows.iter().map(row_text).collect();
    let headline = if harmless {
        t(keys::upgrade::HARMLESS, &[])
    } else {
        t(keys::upgrade::HEADLINE, &[("n", &plan.decisions_at_risk)])
    };
    let blocked = (!plan.blocked.is_empty()).then(|| blocked_text(&plan.blocked));
    std::sync::Arc::new(move || {
        let moves = moves.clone();
        let rows = rows.clone();
        let (headline, blocked, error) = (headline.clone(), blocked.clone(), error.clone());
        view! {
            <div data-harmless=if harmless { "1" } else { "0" } style="display: flex; flex-direction: column; gap: 8px;">
                <ul data-testid="upgrade-moves" style=MOVE>
                    {moves.into_iter().map(|m| view! { <li style=MOVE_ROW>{m}</li> }).collect_view()}
                </ul>
                <p data-testid="upgrade-headline" style=headline_style(harmless)>{headline}</p>
                {(!harmless).then(|| view! { <p style=DETAIL>{t(keys::upgrade::DETAIL, &[])}</p> })}
                {(!harmless).then(|| view! {
                    <ul data-testid="upgrade-plan" style="margin: 6px 0px 0px; padding-inline-start: 18px;">
                        {rows.into_iter().map(|r| view! {
                            <li style="font-size: var(--fs-caption-lg); letter-spacing: var(--track-12-5); color: var(--text-secondary); line-height: 1.7;">{r}</li>
                        }).collect_view()}
                    </ul>
                })}
                {blocked.map(|text| view! {
                    <Callout tone=CalloutTone::Warn attrs=Attrs::from([("role", "alert"), ("data-testid", "upgrade-blocked")])>
                        {text}
                    </Callout>
                })}
                {error_callout(error)}
            </div>
        }
        .into_any()
    })
}

/// A scripture text's body (tC4 `TextUpgrade`).
fn text_body(
    kind: TextKind,
    change: &RepoMove,
    alignments: Option<u32>,
    error: Option<String>,
) -> ChildrenFn {
    let moved = move_text(change);
    let (headline, marks) = text_headline(kind, alignments);
    let verses = alignments.unwrap_or(0);
    std::sync::Arc::new(move || {
        let (moved, headline, error) = (moved.clone(), headline.clone(), error.clone());
        view! {
            <div data-harmless=if marks { "0" } else { "1" } style="display: flex; flex-direction: column; gap: 8px;">
                <ul data-testid="upgrade-moves" style=MOVE>
                    <li style=MOVE_ROW>{moved}</li>
                </ul>
                <p data-testid="upgrade-headline" data-verses=verses.to_string() style=headline_style(!marks)>{headline}</p>
                {marks.then(|| view! { <p style=DETAIL>{t(keys::upgrade::TEXT_INVALIDATES_DETAIL, &[])}</p> })}
                {error_callout(error)}
            </div>
        }
        .into_any()
    })
}
