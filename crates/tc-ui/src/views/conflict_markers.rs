//! The port of tC4's `conflictMarkers.jsx` (unit B4a, read side; tC4 #679): a marker on each
//! verse in conflict, and a toolbar button with the number of open verse conflicts. Both open
//! the conflict review, which B11 builds; while an editing card is open both are off, and the
//! screen keeps a press on them from taking the focus out of the card.

use leptos::prelude::*;

use crate::i18n::{keys, t};

const FORK: [&str; 2] = ["M6 7v2a4 4 0 0 0 4 4h4a4 4 0 0 0 4-4V7", "M12 13v4"];

fn fork_icon(size: u32) -> AnyView {
    let paths = FORK.map(|d| view! { <path d=d></path> }).collect_view();
    view! {
        <svg aria-hidden="true" width=size height=size viewBox="0 0 24 24" fill="none" stroke="currentColor"
            stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="flex: none; display: block;">
            <circle cx="6" cy="5" r="2"></circle>
            <circle cx="18" cy="5" r="2"></circle>
            <circle cx="12" cy="19" r="2"></circle>
            {paths}
        </svg>
    }
    .into_any()
}

fn warn() -> crate::css::Css {
    crate::css! {
        "border": "var(--stroke-selected) solid var(--uw-kindle)",
        "color": "var(--tc-warn-text)",
        "font-family": "var(--font-ui)",
        "font-weight": "var(--fw-heavy)",
        "cursor": "pointer",
    }
}

/// tC4's `ConflictsButton`: "N conflicts · Review" while the project has open verse conflicts.
pub fn conflicts_button(count: u32, off: Signal<bool>, on_click: Callback<()>) -> Option<AnyView> {
    if count == 0 {
        return None;
    }
    let style = move || {
        let mut css = warn();
        css.extend(&crate::css! {
            "display": "inline-flex",
            "align-items": "center",
            "gap": 7,
            "height": 30,
            "padding": "0 14px",
            "border-radius": "var(--radius-pill)",
            "background": "var(--tc-warn-surface)",
            "font-size": "var(--fs-caption-lg)",
        });
        if off.get() {
            css.set("cursor", "default");
        }
        css.to_style()
    };
    let n = count.to_string();
    let label = t(
        if count == 1 {
            keys::conflicts::REVIEW_ONE
        } else {
            keys::conflicts::REVIEW_MANY
        },
        &[("n", &n)],
    );
    Some(
        leptos::html::button()
            .attr("type", "button")
            .attr("data-testid", "conflicts-review")
            .attr("data-count", n)
            .attr("disabled", move || off.get())
            .attr("style", style)
            .on(leptos::ev::click, move |_| on_click.run(()))
            .child((fork_icon(14), label))
            .into_any(),
    )
}

/// tC4's `ConflictVerse`: the warn tint, then the verse number, the marker button and the
/// verse.
pub fn conflict_verse(
    place: String,
    reference: String,
    versions: u8,
    number: AnyView,
    verse: AnyView,
    off: Signal<bool>,
    on_review: Callback<()>,
) -> AnyView {
    let tint = crate::css! {
        "background": "var(--tc-warn-surface)",
        "box-shadow": "0 0 0 4px var(--tc-warn-surface)",
        "border-radius": "var(--radius-xs)",
    };
    let style = move || {
        let mut css = warn();
        css.extend(&crate::css! {
            "display": "inline-flex",
            "align-items": "center",
            "gap": 6,
            "height": 26,
            "padding": "0 10px",
            "margin": "0 4px 0 2px",
            "vertical-align": 3,
            "border-radius": "var(--radius-pill)",
            "background": "var(--surface-card)",
            "font-size": "var(--fs-meta)",
            "line-height": 1,
        });
        if off.get() {
            css.set("cursor", "default");
        }
        css.to_style()
    };
    let marker = marker_button(
        format!("conflict-marker-{place}"),
        &reference,
        versions,
        off,
        on_review,
        style,
    );
    view! {
        <span data-testid=format!("conflict-verse-{place}") style=tint.to_style()>
            <span style="white-space: nowrap;">{number}{marker}</span>
            {verse}
        </span>
    }
    .into_any()
}

fn marker_button(
    test_id: String,
    reference: &str,
    versions: u8,
    off: Signal<bool>,
    on_review: Callback<()>,
    style: impl Fn() -> String + Send + Sync + 'static,
) -> AnyView {
    let n = versions.to_string();
    let aria = t(
        keys::conflicts::MARKER_LABEL,
        &[("ref", &reference), ("n", &n)],
    );
    let label = t(keys::conflicts::MARKER, &[("n", &n)]);
    leptos::html::button()
        .attr("type", "button")
        .attr("data-testid", test_id)
        .attr("aria-label", aria)
        .attr("disabled", move || off.get())
        .attr("style", style)
        .on(leptos::ev::click, move |_| on_review.run(()))
        .child((fork_icon(13), label))
        .into_any()
}

/// "2 versions · Review" beside an aligner's verse or a check's decision that has two
/// versions (#791, a rebuild-only marker): tC4's verse marker on the Check screen.
pub fn versions_marker(
    test_id: &'static str,
    reference: &str,
    versions: u8,
    on_review: Callback<()>,
) -> AnyView {
    let style = || {
        let mut css = warn();
        css.extend(&crate::css! {
            "display": "inline-flex",
            "align-items": "center",
            "gap": 6,
            "height": 26,
            "padding": "0 10px",
            "border-radius": "var(--radius-pill)",
            "background": "var(--tc-warn-surface)",
            "font-size": "var(--fs-meta)",
            "line-height": 1,
        });
        css.to_style()
    };
    marker_button(
        test_id.to_owned(),
        reference,
        versions,
        Signal::from(false),
        on_review,
        style,
    )
}
