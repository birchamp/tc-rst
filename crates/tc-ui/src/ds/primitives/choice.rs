//! Choice.jsx: a binary or exclusive mark backed by a real input. The drawn mark is a sibling
//! of a transparent native control, so the browser supplies the value, the keyboard, the form
//! and the accessibility tree, and the system supplies only the appearance.

use leptos::ev;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::field::use_field;
use super::field_group::use_field_group;
use crate::ds::{Attrs, Css, Mark, Size, Tone, spread};

/// One of the three mark sizes, as px: the box, the switch track's width and height.
fn dims(size: Size) -> (f64, f64, f64) {
    match size {
        Size::Sm => (15.0, 28.0, 17.0),
        Size::Md => (18.0, 34.0, 20.0),
        Size::Lg => (24.0, 44.0, 26.0),
    }
}

/// `Math.round`, which rounds halves up.
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

#[component]
pub fn Choice(
    #[prop(optional)] mark: Mark,
    #[prop(optional)] size: Size,
    #[prop(optional, into)] checked: Signal<bool>,
    #[prop(optional, into)] indeterminate: Signal<bool>,
    /// Unset means: the enclosing Field's, else the enclosing FieldGroup's.
    #[prop(optional, into)]
    disabled: MaybeProp<bool>,
    #[prop(optional)] tone: Tone,
    #[prop(optional, into)] value: Option<String>,
    #[prop(optional, into)] name: Option<String>,
    #[prop(optional)] on_change: Option<Callback<web_sys::Event>>,
    #[prop(optional)] style: Css,
    /// Extra attributes of the native input.
    #[prop(optional)]
    attrs: Attrs,
) -> impl IntoView {
    let f = use_field();
    let g = use_field_group();
    let ctx_disabled = f
        .as_ref()
        .and_then(|f| f.disabled)
        .or_else(|| g.as_ref().and_then(|g| g.disabled));
    let disabled = move || disabled.get().or(ctx_disabled).unwrap_or(false);
    let on = move || checked.get() || indeterminate.get();
    let (sz, sw, sh) = dims(size);
    let knob = sh - 4.0;
    let hit = (size == Size::Lg).then_some("var(--control-h-lg)");
    let name = name
        .or_else(|| f.as_ref().and_then(|f| f.name.clone()))
        .or_else(|| g.as_ref().and_then(|g| g.name.clone()));
    let invalid = f.as_ref().is_some_and(|f| f.invalid) || g.as_ref().is_some_and(|g| g.invalid);

    let outer = move || {
        css! {
            "position": "relative",
            "display": "inline-flex",
            "flex": "none",
            "align-items": "center",
            "justify-content": "center",
            "min-width": hit,
            "min-height": hit,
            "cursor": if disabled() { "default" } else { "pointer" },
        }
        .merge(&style)
        .to_style()
    };
    let hidden_style = css! {
        "position": "absolute",
        "inset": 0,
        "width": "100%",
        "height": "100%",
        "margin": 0,
        "opacity": 0,
        "cursor": "pointer",
        "z-index": 1,
    }
    .to_style();
    let mark_style = move || {
        let on = on();
        let edge = if on {
            "var(--tone-border)"
        } else {
            "var(--border-strong)"
        };
        let mut s = css! {
            "display": "flex",
            "align-items": "center",
            "justify-content": "center",
            "flex": "none",
            "font-weight": "var(--fw-black)",
            "line-height": 1,
            "transition": "background var(--dur-hover) var(--ease-standard), border-color var(--dur-hover) var(--ease-standard)",
        };
        let b = match mark {
            Mark::Check => css! {
                "width": sz,
                "height": sz,
                "border-radius": "var(--radius-xs)",
                "border": format!("var(--stroke-control) solid {edge}"),
                "background": if on { "var(--tone-fill)" } else { "#fff" },
                "color": "var(--tone-on-fill)",
                "font-size": js_round(sz * 0.62),
            },
            Mark::Radio => css! {
                "width": sz,
                "height": sz,
                "border-radius": "var(--radius-pill)",
                "border": format!("var(--stroke-control) solid {edge}"),
                "background": "#fff",
            },
            Mark::Switch => css! {
                "width": sw,
                "height": sh,
                "border-radius": "var(--radius-pill)",
                "background": if on { "var(--tone-fill)" } else { "var(--uw-mist)" },
                "position": "relative",
            },
        };
        s.extend(&b);
        s.to_style()
    };
    let inner = move || match mark {
        Mark::Check => {
            let glyph = if indeterminate.get() {
                "\u{2013}"
            } else if checked.get() {
                "\u{2713}"
            } else {
                ""
            };
            glyph.into_any()
        }
        Mark::Radio => {
            let d = js_round(sz * 0.45);
            let s = css! {
                "width": d,
                "height": d,
                "border-radius": "var(--radius-pill)",
                "background": if on() { "var(--tone-fill)" } else { "transparent" },
            };
            view! { <span style=s.to_style()></span> }.into_any()
        }
        Mark::Switch => {
            let s = css! {
                "position": "absolute",
                "top": 2,
                "inset-inline-start": if on() { sw - knob - 2.0 } else { 2.0 },
                "width": knob,
                "height": knob,
                "border-radius": "var(--radius-pill)",
                "background": "#fff",
                "transition": "inset-inline-start var(--dur-hover) var(--ease-standard)",
            };
            view! { <span style=s.to_style()></span> }.into_any()
        }
    };
    let readonly = on_change.is_none();
    // React writes the initial state as the `checked` attribute (defaultChecked) and keeps the
    // property in step with the prop.
    let initially = checked.get_untracked();
    view! {
        <span style=outer>
            <input
                data-choice-input="true"
                type=if mark == Mark::Radio { "radio" } else { "checkbox" }
                role=(mark == Mark::Switch).then_some("switch")
                id=f.as_ref().map(|f| f.id.clone())
                name=name
                value=value.unwrap_or_default()
                checked=initially
                prop:checked=move || checked.get()
                prop:indeterminate=move || indeterminate.get()
                disabled=disabled
                aria-describedby=f.as_ref().and_then(|f| f.described_by.clone())
                aria-invalid=invalid.then_some("true")
                readonly=readonly
                on:change=move |e: ev::Event| {
                    let input = e.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok());
                    if let Some(cb) = on_change {
                        cb.run(e);
                    }
                    // A controlled input: the mark shows the prop, so a click the owner did not
                    // turn into a new value is undone, as React restores controlled state.
                    if let Some(input) = input {
                        input.set_checked(checked.get_untracked());
                        input.set_indeterminate(indeterminate.get_untracked());
                    }
                }
                style={hidden_style}
                {..spread(&attrs)}
            />
            <span data-choice="true" aria-hidden="true" data-tone=tone.as_str() style=mark_style>
                {inner}
            </span>
        </span>
    }
}
