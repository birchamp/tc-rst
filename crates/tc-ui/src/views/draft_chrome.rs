//! The port of tC4's `draftChrome.jsx` (unit B4a): the Translate chrome Bible Translate and OBS
//! Translate share, the cell and hairline of the two-column grid, the source-tab chip style,
//! the dashed "Draft …" pill that opens an editor, and the editing card's shell.

use leptos::prelude::*;

use crate::attrs::Attrs;
use crate::css::Css;

/// tC4's `hair`.
pub const HAIR: &str = "var(--stroke-hair) solid var(--border-hair)";

/// tC4's `CELL`.
pub fn cell() -> Css {
    crate::css! { "padding": "14px 26px 20px", "border-top": HAIR }
}

/// tC4's `chipStyle`: the source-tab chip, selected or not.
pub fn chip_style(active: bool) -> Css {
    let mut style = crate::css! {
        "display": "inline-block",
        "padding": "3px 9px",
        "font-size": "var(--fs-label)",
        "letter-spacing": "var(--track-11)",
        "border-width": 1,
    };
    style.extend(&if active {
        crate::css! { "background": "var(--accent)", "color": "var(--text-inverse)", "border-color": "var(--accent)" }
    } else {
        crate::css! { "background": "var(--surface-card)", "color": "var(--text-heading)", "border-color": "var(--border-input)" }
    });
    style
}

/// tC4's `DraftPill`: the inline dashed pill of an undrafted unit, "Draft verse N".
pub fn draft_pill(label: String, aria: String, style: Css, on_click: Callback<()>) -> AnyView {
    let mut css = crate::css! {
        "border": "var(--stroke-selected) dashed var(--border-strong)",
        "background": "transparent",
        "border-radius": "var(--radius-sm)",
        "padding": "2px 10px",
        "cursor": "pointer",
        "font-family": "var(--font-ui)",
        "font-size": "var(--fs-caption-lg)",
        "letter-spacing": "var(--track-12-5)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--text-tertiary)",
        "vertical-align": "middle",
    };
    css.extend(&style);
    view! {
        <button type="button" data-i="choice" data-tone="accent" aria-label=aria
            style=css.to_style() on:click=move |_| on_click.run(())>
            {label}
        </button>
    }
    .into_any()
}

/// tC4's `EditingCard`: the accent border, a header row, the field, and the footer with Save
/// and Cancel. A press in the footer keeps the focus in the field (tC4 puts `onMouseDown`
/// `preventDefault` on each button), so the field's blur does not close the card before the
/// click lands.
pub fn editing_card(header: AnyView, field: AnyView, footer: AnyView, attrs: Attrs) -> AnyView {
    let card = crate::css! {
        "border": "var(--stroke-selected) solid var(--accent)",
        "border-radius": "var(--radius-md)",
        "padding": "12px 14px",
        "background": "var(--surface-card)",
        "box-shadow": "0 2px 8px rgba(49,173,227,.15)",
    };
    let head = crate::css! {
        "display": "flex",
        "align-items": "center",
        "gap": 10,
        "margin-bottom": 8,
        "flex-wrap": "wrap",
    };
    let foot = crate::css! {
        "display": "flex",
        "align-items": "center",
        "gap": 10,
        "margin-top": 8,
    };
    leptos::html::div()
        .attr("style", card.to_style())
        .add_any_attr(crate::ds::spread(&attrs))
        .child((
            view! { <div style=head.to_style()>{header}</div> },
            field,
            view! {
                <div style=foot.to_style() on:mousedown=|e: web_sys::MouseEvent| e.prevent_default()>
                    {footer}
                </div>
            },
        ))
        .into_any()
}
