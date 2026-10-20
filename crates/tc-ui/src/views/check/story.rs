//! A story's check detail (unit B7; tC4 `CheckDetail` with `StoryPane`): the check's quote and
//! its note, then one compare card holding the gateway story's frame, with the quote's words
//! lit, and the person's own text of the frame. Reading it writes nothing.
//!
//! B5b draws the rest of tC4's detail for a book (the words a person selects, the marks, the
//! bookmark and the comment) over `check/item`; a story's decisions are written with C4, so
//! this detail has no marking row until then: nothing is drawn that cannot be saved.

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::check::{CheckListRef, CheckPlace, RailRow, StoryItem, StoryItemView, StoryPane};
use tc_api::ids::Scope;

use crate::app::check::place_parts;
use crate::app::load::Load;
use crate::app::web::{Actions, AppState, use_runtime};
use crate::attrs::Attrs;
use crate::ds::{Button, ButtonVariant, Callout, CalloutTone, Overline, Size};
use crate::i18n::{keys, t};

const SCRIPTURE: &str = "var(--font-scripture)";

/// The detail beside the rail: the active check's card, or a line while it is read.
pub(super) fn story_detail(
    s: &AppState,
    actions: &Actions,
    at: Scope,
    list: CheckListRef,
    rows: Vec<RailRow>,
    order: Arc<Vec<u32>>,
) -> AnyView {
    let active = s.c.active.clone();
    let rows = Arc::new(rows);
    let actions = actions.clone();
    (move || {
        let runtime = use_runtime()?;
        let index = active.get()?;
        let item = runtime.cache().read(StoryItem {
            at: at.clone(),
            list,
            index,
        });
        let (rows, order, actions) = (rows.clone(), order.clone(), actions.clone());
        Some(move || {
            item.with(|loaded| match loaded {
                Load::Loading => loading(),
                Load::Failed(error) => failed(error.to_string()),
                Load::Ready(view) | Load::Refreshing(view) => {
                    detail(&actions, view, &rows, &order, index)
                }
            })
        })
    })
    .into_any()
}

fn loading() -> AnyView {
    let style = crate::css! {
        "font-size": "var(--fs-ui)",
        "color": "var(--text-tertiary)",
        "padding": "28px 32px",
    };
    view! { <p data-testid="check-item-loading" style=style.to_style()>{t(keys::check::DERIVING, &[])}</p> }
        .into_any()
}

fn failed(error: String) -> AnyView {
    view! {
        <Callout tone=CalloutTone::Warn style=crate::css! { "margin": "28px 32px" } attrs=Attrs::from([("role", "alert"), ("data-testid", "check-item-error")])>
            {error}
        </Callout>
    }
    .into_any()
}

/// A pane's label row (tC4 `paneLabelRow`).
fn pane_row() -> crate::css::Css {
    crate::css! {
        "padding": "16px 20px",
        "border-bottom": "var(--stroke-hair) solid var(--border-hair)",
        "background": "var(--surface-app)",
    }
}

/// The compare card's gateway row (tC4 `StoryPane`).
fn story_pane(pane: &StoryPane) -> AnyView {
    let body = match pane {
        StoryPane::Text { tokens, lit } => {
            let text = crate::css! {
                "text-align": "start",
                "font-family": SCRIPTURE,
                "font-size": "var(--fs-verse)",
                "line-height": "var(--lh-verse-md)",
                "color": "var(--text-scripture)",
                "white-space": "pre-wrap",
                "margin": "8px 0 0",
            };
            let mark = crate::css! {
                "background": "var(--tc-highlight-soft)",
                "color": "var(--text-heading)",
                "border-radius": "var(--radius-xs)",
                "padding": "0 .06em",
            };
            let parts = tokens
                .iter()
                .enumerate()
                .map(|(at, token)| {
                    let token = token.clone();
                    let on = u32::try_from(at).is_ok_and(|at| lit.contains(&at));
                    if on {
                        view! { <mark data-testid="story-hl" style=mark.to_style()>{token}</mark> }
                            .into_any()
                    } else {
                        token.into_any()
                    }
                })
                .collect_view();
            view! { <p data-testid="story-pane-text" dir="auto" style=text.to_style()>{parts}</p> }
                .into_any()
        }
        StoryPane::Missing => {
            let style = crate::css! {
                "font-size": "var(--fs-ui-sm)",
                "color": "var(--uw-haze)",
                "font-style": "italic",
                "margin": "8px 0 0",
            };
            view! { <p style=style.to_style()>{t(keys::story_draft::SOURCE_MISSING, &[])}</p> }
                .into_any()
        }
    };
    view! {
        <div data-testid="story-pane" style=pane_row().to_style()>
            <Overline tone=crate::ds::OverlineTone::Muted>{t(keys::check::GATEWAY_STORY_LABEL, &[])}</Overline>
            {body}
        </div>
    }
    .into_any()
}

/// "Your translation": the frame's text, or the line that says it is not drafted yet.
fn your_translation(target: &str, dir: &'static str) -> AnyView {
    let body = if crate::app::story::is_frame_drafted(target) {
        let style = crate::css! {
            "text-align": "start",
            "font-family": SCRIPTURE,
            "font-size": "var(--fs-verse)",
            "line-height": "var(--lh-verse-md)",
            "color": "var(--text-scripture)",
            "white-space": "pre-wrap",
            "margin": "8px 0 0",
        };
        view! { <p data-testid="check-target" data-drafted="1" dir=dir style=style.to_style()>{target.to_owned()}</p> }
            .into_any()
    } else {
        let style = crate::css! {
            "font-size": "var(--fs-ui-md)",
            "color": "var(--text-tertiary)",
            "font-style": "italic",
            "margin": "8px 0 0",
        };
        view! {
            <p data-testid="check-target" data-drafted="0" style=style.to_style()>
                {t(keys::check::NOT_DRAFTED_FRAME, &[])}
            </p>
        }
        .into_any()
    };
    let style = crate::css! { "padding": "16px 20px" };
    view! {
        <div style=style.to_style()>
            <Overline tone=crate::ds::OverlineTone::Accent>{t(keys::check::YOUR_TRANSLATION, &[])}</Overline>
            {body}
        </div>
    }
    .into_any()
}

/// A Previous or Next button of the item navigation (tC4 `ItemNavButton`).
fn nav_button(actions: &Actions, prev: bool, to: Option<u32>) -> impl IntoView + use<> {
    let actions = actions.clone();
    let label = t(
        if prev {
            keys::check::PREV_LABEL
        } else {
            keys::check::NEXT_LABEL
        },
        &[],
    );
    let arrow = if prev { "←" } else { "→" };
    let text = if prev {
        format!("{arrow} {label}")
    } else {
        format!("{label} {arrow}")
    };
    let id = if prev { "check-prev" } else { "check-next" };
    let title = t(
        if prev {
            keys::check::PREV_ITEM
        } else {
            keys::check::NEXT_ITEM
        },
        &[],
    );
    let side = if prev {
        "margin-inline-end"
    } else {
        "margin-inline-start"
    };
    let mut style = crate::css! {};
    style.set(side, 4);
    if to.is_some() {
        style.set("color", "var(--accent)");
    }
    view! {
        <Button
            variant=ButtonVariant::Secondary
            size=Size::Sm
            attrs=Attrs::from([("data-testid", id), ("title", title.as_str())])
            disabled=to.is_none()
            style=style
            on_click=Callback::new(move |_| {
                if let Some(to) = to {
                    actions.select_check(to);
                }
            })
        >
            {text}
        </Button>
    }
}

fn detail(
    actions: &Actions,
    view: &StoryItemView,
    rows: &[RailRow],
    order: &[u32],
    index: u32,
) -> AnyView {
    let (story, frame) = place_parts(&view.place);
    let position = order.iter().position(|i| *i == index);
    let (prev, next) = match position {
        Some(at) => (
            at.checked_sub(1).and_then(|p| order.get(p)).copied(),
            order.get(at + 1).copied(),
        ),
        None => (None, None),
    };
    let total = rows.len().to_string();
    let n = position.map_or(0, |at| at + 1).to_string();
    let wrap = crate::css! { "max-width": 760, "margin": "0 auto", "padding": "28px 32px 60px" };
    let head = crate::css! {
        "display": "flex",
        "align-items": "center",
        "gap": 10,
        "margin-bottom": 14,
    };
    let place = crate::css! {
        "font-size": "var(--fs-ui-sm)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--text-tertiary)",
    };
    let counter = crate::css! {
        "font-size": "var(--fs-caption)",
        "letter-spacing": "var(--track-12)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--text-tertiary)",
        "white-space": "nowrap",
    };
    let h1 = crate::css! {
        "font-family": SCRIPTURE,
        "font-size": "var(--fs-h1)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--text-heading)",
        "margin": "0 0 14px",
    };
    let note = (!view.note.is_empty()).then(|| {
        let boxed = crate::css! {
            "max-width": "62ch",
            "margin-bottom": 8,
            "border-radius": "var(--radius-lg)",
            "padding": "16px 18px",
            "background": "var(--surface-warm)",
            "border": "var(--stroke) solid var(--tc-warn-border)",
        };
        let text = crate::css! {
            "font-size": "var(--fs-title-sm)",
            "letter-spacing": "var(--track-16)",
            "line-height": "var(--lh-body)",
            "font-weight": "var(--fw-medium)",
            "color": "var(--uw-ink)",
            "margin": "7px 0 0",
        };
        let blocks = view.note.clone();
        view! {
            <div data-testid="check-note" style=boxed.to_style()>
                <Overline style=crate::css! { "color": "var(--tc-warn-text-2)" }>{t(keys::check::WHAT_TO_CHECK, &[])}</Overline>
                <div data-testid="check-note-text" style=text.to_style()>
                    {crate::views::helps_panel::expandable_note(blocks)}
                </div>
            </div>
        }
    });
    let card = crate::css! {
        "background": "var(--surface-card)",
        "border": "var(--stroke) solid var(--border)",
        "border-radius": "var(--radius-xl)",
        "overflow": "hidden",
        "box-shadow": "var(--shadow-card)",
        "margin-top": 24,
    };
    let nav = crate::css! {
        "display": "flex",
        "align-items": "center",
        "gap": 10,
        "margin-top": 22,
        "flex-wrap": "wrap",
    };
    let unit = match &view.place {
        CheckPlace::Frame { story, .. } => crate::app::story::story_label(*story),
        CheckPlace::Verse { .. } => String::new(),
    };
    let reference = t(keys::check::REF, &[("c", &story), ("v", &frame)]);
    let target_dir = "auto";
    view! {
        <div data-testid="check-detail" style=wrap.to_style()>
            <div style=head.to_style()>
                <span style=place.to_style()>{unit}" "{reference}</span>
                <div style="flex: 1;"></div>
                <span data-testid="check-item-counter" style=counter.to_style()>
                    {t(keys::check::ITEM_OF, &[("n", &n), ("total", &total)])}
                </span>
            </div>
            <h1 data-testid="check-quote" style=h1.to_style()>"“"{view.title.clone()}"”"</h1>
            {note}
            <div style=card.to_style()>
                {story_pane(&view.gateway)}
                {your_translation(&view.target, target_dir)}
            </div>
            <div style=nav.to_style()>
                {nav_button(actions, true, prev)}
                {nav_button(actions, false, next)}
            </div>
        </div>
    }
    .into_any()
}
