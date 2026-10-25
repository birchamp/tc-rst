//! The port of tC4's `SyncResult.jsx` (X10, unit B10): the result of one sync at the bottom of
//! Home, a white toast with a dot, the line, the notes, the Ocean links and the dismiss button
//! (the owner's mockup "HomeConflicts", 2026-10-13). Its decisions are `app::sync`'s.

use leptos::prelude::*;

use crate::app::sync::{Counted, SyncToast};
use crate::i18n::{keys, t};

fn line_style() -> String {
    crate::css! {
        "font-size": "var(--fs-ui-sm)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--uw-ink)",
        "overflow-wrap": "anywhere",
    }
    .to_style()
}

fn note_style() -> String {
    crate::css! { "font-size": "var(--fs-meta)", "color": "var(--text-secondary)" }.to_style()
}

fn count(part: Counted) -> impl IntoView {
    view! {
        <span data-testid=part.test_id data-count=part.n.to_string()>{part.text}</span>
    }
}

fn note(part: Counted) -> impl IntoView {
    view! {
        <span data-testid=part.test_id data-count=part.n.to_string() style=note_style()>{part.text}</span>
    }
}

fn link(test_id: &'static str, label: String, on_click: Callback<()>) -> impl IntoView {
    let style = crate::css! {
        "border": 0,
        "background": "transparent",
        "padding": 0,
        "font": "inherit",
        "font-size": "var(--fs-ui-sm)",
        "font-weight": "var(--fw-heavy)",
        "color": "var(--uw-ocean)",
        "text-decoration": "underline",
        "cursor": "pointer",
        "white-space": "nowrap",
    };
    view! {
        <button type="button" data-testid=test_id on:click=move |_| on_click.run(()) style=style.to_style()>
            {label}
        </button>
    }
}

/// The toast of `toast`. The texts are read when it is drawn: the toast lives for one sync's
/// result, and a language change redraws Home.
#[component]
pub fn SyncResult(
    toast: SyncToast,
    on_review: Callback<()>,
    on_download: Callback<()>,
    on_dismiss: Callback<()>,
) -> impl IntoView {
    let ok = toast.synced().is_some();
    let root = crate::css! {
        "box-sizing": "border-box",
        "display": "flex",
        "align-items": "center",
        "gap": 12,
        "padding": "12px 14px 12px 16px",
        "background": "var(--surface-card)",
        "border": "var(--stroke-hair) solid var(--border-hair)",
        "border-radius": "var(--radius-lg)",
        "box-shadow": "var(--shadow-hover)",
        "font-family": "var(--font-ui)",
    };
    let dot = crate::css! {
        "width": 8,
        "height": 8,
        "border-radius": "var(--radius-pill)",
        "background": toast.dot().color(),
        "flex": "none",
    };
    let line = match toast.counts() {
        Some([received, newest, conflicts]) => view! {
            <span style=line_style()>
                {t(keys::sync_result::DONE, &[])}
                " · "{count(received)}
                " · "{count(newest)}
                " · "{count(conflicts)}
            </span>
        }
        .into_any(),
        None => view! {
            <span style=line_style()>
                <strong>{t(keys::sync_result::FAILED, &[])}</strong>
                " "
                <span data-testid="sync-message">{toast.failure().unwrap_or_default()}</span>
            </span>
        }
        .into_any(),
    };
    let from = toast.from().map(|names| {
        view! { <span data-testid="sync-received-from" style=note_style()>{names}</span> }
    });
    let notes = toast.notes().into_iter().map(note).collect_view();
    // tC4's list of replaced alignments (#791), one item per record, right after "Changes
    // from"; it scrolls so the toast's line and buttons stay on screen.
    let replaced = toast.replaced();
    let replaced = (!replaced.is_empty()).then(|| {
        let items = replaced
            .into_iter()
            .map(|part| view! { <li data-testid=part.test_id style=note_style()>{part.text}</li> })
            .collect_view();
        view! {
            <ul data-testid="sync-replaced-list" style="margin: 0; padding-inline-start: 18px; max-height: 30vh; overflow-y: auto;">
                {items}
            </ul>
        }
    });
    let review = toast
        .review()
        .then(|| link("sync-review", t(keys::sync_result::REVIEW, &[]), on_review));
    let download = toast.download().then(|| {
        link(
            "sync-download",
            t(keys::sync_result::DOWNLOAD, &[]),
            on_download,
        )
    });
    let column = crate::css! {
        "display": "flex",
        "flex-direction": "column",
        "gap": 4,
        "min-width": 0,
    };
    let dismiss = t(keys::common::DISMISS, &[]);
    let dismiss_style = crate::css! {
        "width": 26,
        "height": 26,
        "border-radius": "var(--radius-pill)",
        "border": 0,
        "background": "var(--uw-frost)",
        "color": "var(--uw-ink)",
        "display": "inline-flex",
        "align-items": "center",
        "justify-content": "center",
        "padding": 0,
        "cursor": "pointer",
        "flex": "none",
    };
    view! {
        <div
            role=if ok { "status" } else { "alert" }
            data-testid="sync-result"
            data-ok=if ok { "1" } else { "0" }
            data-code=toast.code()
            style=root.to_style()
        >
            <span aria-hidden="true" style=dot.to_style()></span>
            <div style=column.to_style()>
                {line}
                {from}
                {replaced}
                {notes}
            </div>
            {review}
            {download}
            <button
                type="button"
                data-testid="sync-dismiss"
                aria-label=dismiss.clone()
                title=dismiss
                on:click=move |_| on_dismiss.run(())
                style=dismiss_style.to_style()
            >
                {cross()}
            </button>
        </div>
    }
}

/// The dismiss button's ✕. Built with the SVG builders: `view!` would wrap each path in a
/// group.
fn cross() -> AnyView {
    use crate::ds::fixed;
    use leptos::svg;
    svg::svg()
        .add_any_attr(fixed(&[
            ("aria-hidden", "true"),
            ("width", "12"),
            ("height", "12"),
            ("viewBox", "0 0 24 24"),
            ("fill", "none"),
            ("stroke", "currentColor"),
            ("stroke-width", "2.2"),
            ("stroke-linecap", "round"),
            ("stroke-linejoin", "round"),
            ("style", "display: block;"),
        ]))
        .child((
            svg::path().add_any_attr(fixed(&[("d", "M6 6l12 12")])),
            svg::path().add_any_attr(fixed(&[("d", "M18 6L6 18")])),
        ))
        .into_any()
}
