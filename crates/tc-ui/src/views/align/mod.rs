//! The port of tC4's Align tool (`Align.jsx`, and `AlignWorkspace` and `AlignRail` in
//! `Check.jsx`; unit B6): the rail of the book's verses beside one verse's aligner.
//!
//! The aligner is on the server. This draws the view it answers (the bank, the cards, the
//! suggestions row, the marking row) and sends what the person does: a click on a word, a click
//! on a card, a drag, a button. The decisions are in [`crate::app::align`].

mod session;

use leptos::prelude::*;
use tc_api::align::{IndexItem, RailState};
use tc_core::BookId;

use crate::app::align::{RailFilter, counts, percent, progress, same_verse};
use crate::app::shell::View;
use crate::app::web::align::Rail;
use crate::app::web::{Actions, AppState};
use crate::css::Css;
use crate::ds::{Button, ButtonVariant, ProgressBar, Size, Tone};
use crate::i18n::{keys, t, t_dyn};

pub use session::align_session;

/// tC4's `railChip`.
fn rail_chip(on: bool) -> Css {
    crate::css! {
        "border": "var(--stroke) solid",
        "cursor": "pointer",
        "font-family": "inherit",
        "font-weight": "var(--fw-bold)",
        "font-size": "var(--fs-label)",
        "letter-spacing": "var(--track-11)",
        "padding": "5px 10px",
        "border-radius": "var(--radius-pill)",
        "white-space": "nowrap",
        "background": if on { "var(--uw-ocean)" } else { "var(--surface-card)" },
        "color": if on { "var(--text-inverse)" } else { "var(--text-secondary)" },
        "border-color": if on { "var(--uw-ocean)" } else { "var(--border-input)" },
    }
}

/// tC4's `DOT`: a verse's colour on the rail.
fn dot(state: RailState) -> &'static str {
    match state {
        RailState::Valid => "var(--tc-valid)",
        RailState::Invalid => "var(--tc-invalid)",
        RailState::Todo => "var(--text-tertiary)",
        RailState::Undrafted => "var(--uw-mist)",
    }
}

fn state_name(state: RailState) -> &'static str {
    match state {
        RailState::Valid => "valid",
        RailState::Invalid => "invalid",
        RailState::Todo => "todo",
        RailState::Undrafted => "undrafted",
    }
}

fn row_button(
    actions: &Actions,
    item: &IndexItem,
    active: bool,
    dir: &'static str,
) -> impl IntoView + use<> {
    let undrafted = item.status.state == RailState::Undrafted;
    let place = item.place;
    let label = place.label();
    let style = crate::css! {
        "border": if active { "var(--stroke) solid var(--accent)" } else { "var(--stroke) solid var(--border)" },
        "text-align": "start",
        "cursor": if undrafted { "default" } else { "pointer" },
        "font-family": "inherit",
        "border-radius": "var(--radius-md)",
        "padding": "10px 12px",
        "width": "100%",
        "display": "flex",
        "align-items": "center",
        "gap": 10,
        "background": if active { "var(--surface-accent-soft)" } else { "var(--surface-card)" },
        "opacity": if undrafted { 0.55 } else { 1.0 },
    };
    let dot_style = crate::css! {
        "width": 9,
        "height": 9,
        "border-radius": "var(--radius-pill)",
        "flex": "none",
        "background": dot(item.status.state),
    };
    let ref_style = crate::css! {
        "font-size": "var(--fs-badge)",
        "letter-spacing": "var(--track-10)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--text-tertiary)",
        "white-space": "nowrap",
    };
    let text_style = crate::css! {
        "flex": 1,
        "font-size": "var(--fs-ui-sm)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--text-body)",
        "overflow": "hidden",
        "text-overflow": "ellipsis",
        "white-space": "nowrap",
        "font-style": if undrafted { "italic" } else { "normal" },
    };
    let text = if undrafted {
        t(keys::align::NOT_DRAFTED_SHORT, &[])
    } else {
        item.text.clone()
    };
    let actions = actions.clone();
    view! {
        <button
            type="button"
            disabled=undrafted
            data-i=(!undrafted).then_some("choice")
            data-tone="accent"
            data-selected=active.then_some("true")
            data-ref=label.clone()
            data-status=state_name(item.status.state)
            style=style.to_style()
            on:click=move |_| {
                if !undrafted {
                    actions.set_align_verse(place);
                }
            }
        >
            <span style=dot_style.to_style()></span>
            <span style=ref_style.to_style()>{place.label()}</span>
            <span dir=(!undrafted).then_some(dir) style=text_style.to_style()>{text}</span>
        </button>
    }
}

/// tC4's `AlignRail`.
fn align_rail(s: &AppState, actions: &Actions, book: BookId) -> impl IntoView + use<> {
    let a = s.a.clone();
    let aside = crate::css! {
        "width": "var(--rail-width-wide)",
        "flex": "none",
        "background": "var(--surface-card)",
        "border-inline-end": "var(--stroke-hair) solid var(--border)",
        "display": "flex",
        "flex-direction": "column",
        "min-height": 0,
    };
    let head = crate::css! {
        "padding": "14px 18px",
        "border-bottom": "var(--stroke-hair) solid var(--border-hair)",
        "flex": "none",
    };
    let h2 = crate::css! {
        "font-size": "var(--fs-title-sm)",
        "letter-spacing": "var(--track-16)",
        "font-weight": "var(--fw-black)",
        "color": "var(--text-heading)",
        "margin": "0 0 2px",
    };
    let unit_style = crate::css! {
        "font-size": "var(--fs-caption)",
        "letter-spacing": "var(--track-12)",
        "color": "var(--text-tertiary)",
        "margin": "0 0 10px",
        "font-weight": "var(--fw-medium)",
    };
    let progress_style = crate::css! {
        "font-size": "var(--fs-caption)",
        "letter-spacing": "var(--track-12)",
        "color": "var(--text-secondary)",
        "margin": "8px 0 12px",
        "font-weight": "var(--fw-medium)",
    };
    let error_style = crate::css! {
        "font-size": "var(--fs-caption-lg)",
        "color": "var(--tc-invalid)",
        "line-height": "var(--lh-body)",
        "margin": "10px 0 0",
        "overflow-wrap": "anywhere",
    };
    let list_style = crate::css! {
        "flex": 1,
        "overflow": "auto",
        "padding": 14,
        "min-height": 0,
        "display": "flex",
        "flex-direction": "column",
        "gap": 6,
    };
    let empty_style = crate::css! {
        "font-size": "var(--fs-caption-lg)",
        "letter-spacing": "var(--track-12-5)",
        "color": "var(--text-tertiary)",
        "font-style": "italic",
        "margin": "6px 2px",
    };
    let items = {
        let rail = a.rail.clone();
        Memo::new(move |_| {
            rail.with(|rail| match rail {
                Rail::Ready(items) => items.clone(),
                Rail::Loading | Rail::Failed(_) => Vec::new(),
            })
        })
    };
    let bar = move || {
        let (done, total) = items.with(|items| progress(items));
        view! {
            <ProgressBar
                tone=Tone::Valid
                value=percent(done, total)
                height=7.0
            />
            <p data-testid="align-rail-progress" style=progress_style.to_style()>
                {t(keys::align::PROGRESS_VERSES, &[("done", &done.to_string()), ("total", &total.to_string())])}
            </p>
        }
    };
    let chips = {
        let a = a.clone();
        move || {
            let on = a.filter.get();
            let n = items.with(|items| counts(items));
            RailFilter::ORDER
                .iter()
                .zip(n)
                .map(|(&filter, n)| {
                    let a = a.clone();
                    view! {
                        <button
                            type="button"
                            data-trim="cap"
                            data-testid=format!("align-filter-{}", filter.id())
                            style=rail_chip(on == filter).to_style()
                            on:click=move |_| a.filter.set(filter)
                        >
                            {t_dyn(&format!("check.filter.{}", filter.id()), &[], None)}" · "{n.to_string()}
                        </button>
                    }
                })
                .collect::<Vec<_>>()
        }
    };
    let failure = {
        let a = a.clone();
        move || {
            a.rail.with(|rail| match rail {
                Rail::Failed(reason) => {
                    Some(view! { <p style=error_style.to_style()>{reason.clone()}</p> })
                }
                _ => None,
            })
        }
    };
    let dir = if s.project.with_untracked(|p| {
        p.as_ref()
            .is_some_and(|p| p.dir == tc_api::shown::Direction::Rtl)
    }) {
        "rtl"
    } else {
        "ltr"
    };
    let rows = {
        let (a, actions) = (a.clone(), actions.clone());
        move || {
            let filter = a.filter.get();
            let active = a.verse.get();
            items.with(|items| {
                items
                    .iter()
                    .filter(|item| filter.matches(item))
                    .map(|item| {
                        let on = active.is_some_and(|a| same_verse(a, item.place));
                        row_button(&actions, item, on, dir)
                    })
                    .collect::<Vec<_>>()
            })
        }
    };
    let nothing = {
        let a = a.clone();
        move || {
            let filter = a.filter.get();
            let none = a.rail.with(|rail| match rail {
                Rail::Ready(items) => !items.iter().any(|i| filter.matches(i)),
                Rail::Loading | Rail::Failed(_) => false,
            });
            none.then(|| {
                view! { <p data-testid="align-rail-empty" style=empty_style.to_style()>{t(keys::check::RAIL_EMPTY, &[])}</p> }
            })
        }
    };
    let back = {
        let actions = actions.clone();
        Callback::new(move |_| actions.close_align())
    };
    let to_draft = {
        let actions = actions.clone();
        Callback::new(move |_| {
            actions.close_align();
            actions.go(View::Draft);
        })
    };
    view! {
        <aside data-testid="align-rail" style=aside.to_style()>
            <div style=head.to_style()>
                <Button variant=ButtonVariant::Ghost size=Size::Sm on_click=back style=crate::css! { "padding": 0, "margin-bottom": 8 }>
                    {t(keys::check::BACK, &[])}
                </Button>
                <h2 style=h2.to_style()>{t(keys::nav::ALIGN, &[])}</h2>
                <p style=unit_style.to_style()>{book.english_name().to_owned()}</p>
                {bar}
                <div style="display: flex; gap: 6px;">{chips}</div>
                {failure}
            </div>
            <div data-testid="align-verse-list" style=list_style.to_style()>
                {rows}
                {nothing}
            </div>
            <div style="padding: 12px 16px; border-top: var(--stroke-hair) solid var(--border-hair);">
                <Button variant=ButtonVariant::Ghost size=Size::Sm on_click=to_draft style=crate::css! { "padding": 0 }>
                    {t(keys::check::BACK_TO_TRANSLATING, &[])}
                </Button>
            </div>
        </aside>
    }
}

/// tC4's `AlignWorkspace`: the rail, and the aligner of the open verse.
pub fn align_workspace(s: &AppState, actions: &Actions, book: BookId) -> impl IntoView + use<> {
    let main = crate::css! {
        "flex": 1,
        "overflow": "auto",
        "min-width": 0,
        "background": "var(--surface-app)",
    };
    let inner = crate::css! {
        "max-width": 1280,
        "margin": "0 auto",
        "padding": "28px 32px 60px",
    };
    let rail = align_rail(s, actions, book);
    let content = align_session(s, actions, book);
    view! {
        <div data-testid="align-workspace" style="flex: 1; display: flex; min-height: 0;">
            {rail}
            <main style=main.to_style()>
                <div style=inner.to_style()>{content}</div>
            </main>
        </div>
    }
}
