//! The port of tC4's `railChrome.jsx` (unit B3): the left rail's shell, the tinted group of
//! the active row, the grid of number buttons and one number button, shared by the book rail
//! and, with B7, the story rail.

use leptos::prelude::*;

use crate::attrs::Attrs;
use crate::ds::Overline;
use crate::ds::spread;

/// tC4's `RailShell`: the rail's width, panel surface, header and scrolling list.
pub fn rail_shell(title: Signal<String>, children: AnyView) -> impl IntoView {
    let aside = crate::css! {
        "width": "var(--rail-width)",
        "flex": "none",
        "background": "var(--surface-panel)",
        "border-inline-end": "var(--stroke-hair) solid var(--border-hair)",
        "display": "flex",
        "flex-direction": "column",
        "min-height": 0,
    };
    let head = crate::css! { "padding": "16px 16px 10px" };
    let list = crate::css! {
        "padding": "0 10px 14px",
        "display": "flex",
        "flex-direction": "column",
        "gap": 4,
        "overflow": "auto",
        "flex": 1,
        "min-height": 0,
    };
    view! {
        <aside style=aside.to_style()>
            <div style=head.to_style()>
                <Overline>{move || title.get()}</Overline>
            </div>
            <div style=list.to_style()>{children}</div>
        </aside>
    }
}

/// tC4's `RailGroup`: the active row and its number grid share one tint.
pub fn rail_group(active: Signal<bool>, children: AnyView) -> impl IntoView {
    let style = move || {
        crate::css! {
            "border-radius": "var(--radius-md)",
            "background": if active.get() { "var(--surface-accent-soft)" } else { "transparent" },
        }
        .to_style()
    };
    view! { <div style=style>{children}</div> }
}

/// tC4's `RailNumbers`: the five-column grid under the active row.
pub fn rail_numbers(children: AnyView) -> impl IntoView {
    let style = crate::css! {
        "display": "grid",
        "grid-template-columns": "repeat(5,1fr)",
        "gap": 6,
        "padding": "2px 12px 12px",
    };
    view! { <div style=style.to_style()>{children}</div> }
}

/// tC4's `RailNumberButton`: a chapter or a frame; `selected` is the one in view.
pub fn rail_number_button(
    selected: Signal<bool>,
    disabled: Signal<bool>,
    on_click: Callback<()>,
    attrs: Attrs,
    label: String,
) -> impl IntoView {
    let style = move || {
        let on = selected.get();
        let mut css = crate::css! {
            "cursor": "pointer",
            "font-family": "var(--font-ui)",
            "font-weight": "var(--fw-heavy)",
            "font-size": "var(--fs-caption)",
            "letter-spacing": "var(--track-12)",
            "height": 32,
            "border-radius": "var(--radius-sm)",
            "border-width": "var(--stroke)",
            "border-style": "solid",
        };
        css.extend(&if on {
            crate::css! { "background": "var(--accent)", "color": "#fff", "border-color": "var(--accent)" }
        } else {
            crate::css! { "background": "#fff", "color": "var(--text-tertiary)", "border-color": "var(--border)" }
        });
        if disabled.get() {
            css.set("cursor", "default");
        }
        css.to_style()
    };
    leptos::html::button()
        .attr("type", "button")
        .attr("data-i", move || (!selected.get()).then_some("choice"))
        .attr("data-tone", "accent")
        .attr("disabled", move || disabled.get())
        .attr("style", style)
        .add_any_attr(spread(&attrs))
        .on(leptos::ev::click, move |_| on_click.run(()))
        .on(
            leptos::ev::capture(leptos::ev::pointerdown),
            move |e: web_sys::PointerEvent| {
                if disabled.get_untracked() {
                    e.prevent_default();
                }
            },
        )
        .child(label)
}
