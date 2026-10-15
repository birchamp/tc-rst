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
    // The mark's inner node stays while the state changes, so the knob's slide and the dot's
    // fill transition run, as they do on the node React keeps.
    let inner = match mark {
        Mark::Check => (move || {
            if indeterminate.get() {
                "\u{2013}"
            } else if checked.get() {
                "\u{2713}"
            } else {
                ""
            }
        })
        .into_any(),
        Mark::Radio => {
            let d = js_round(sz * 0.45);
            let s = move || {
                css! {
                    "width": d,
                    "height": d,
                    "border-radius": "var(--radius-pill)",
                    "background": if on() { "var(--tone-fill)" } else { "transparent" },
                }
                .to_style()
            };
            view! { <span style=s></span> }.into_any()
        }
        Mark::Switch => {
            let s = move || {
                css! {
                    "position": "absolute",
                    "top": 2,
                    "inset-inline-start": if on() { sw - knob - 2.0 } else { 2.0 },
                    "width": knob,
                    "height": knob,
                    "border-radius": "var(--radius-pill)",
                    "background": "#fff",
                    "transition": "inset-inline-start var(--dur-hover) var(--ease-standard)",
                }
                .to_style()
            };
            view! { <span style=s></span> }.into_any()
        }
    };
    let readonly = on_change.is_none();
    // React writes the initial state as the `checked` attribute (defaultChecked) and keeps the
    // property in step with the prop.
    let initially = checked.get_untracked();
    let node = NodeRef::<leptos::html::Input>::new();
    let group = (mark == Mark::Radio).then_some(name.clone()).flatten();
    if let Some(group) = group.clone() {
        let entry = radios::join(group, node, checked);
        on_cleanup(move || radios::leave(entry));
    }
    view! {
        <span style=outer>
            <input
                node_ref=node
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
                    // turn into a new value is undone, as React restores controlled state. The
                    // browser also unchecked the group's other radio, so React restores every
                    // radio of the group, not only this one.
                    if let Some(input) = input {
                        input.set_checked(checked.get_untracked());
                        input.set_indeterminate(indeterminate.get_untracked());
                        if let Some(group) = group.as_deref() {
                            radios::restore(group, &input);
                        }
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

/// The controlled radios on the page by group name, so a change on one restores the others.
mod radios {
    use std::cell::{Cell, RefCell};

    use leptos::prelude::*;

    struct Radio {
        id: u64,
        name: String,
        node: NodeRef<leptos::html::Input>,
        checked: Signal<bool>,
    }

    thread_local! {
        static RADIOS: RefCell<Vec<Radio>> = const { RefCell::new(Vec::new()) };
        static NEXT: Cell<u64> = const { Cell::new(0) };
    }

    /// Adds a radio of group `name`; the id given back takes it out again.
    pub(super) fn join(
        name: String,
        node: NodeRef<leptos::html::Input>,
        checked: Signal<bool>,
    ) -> u64 {
        let id = NEXT.with(|n| {
            n.set(n.get() + 1);
            n.get()
        });
        RADIOS.with(|r| {
            r.borrow_mut().push(Radio {
                id,
                name,
                node,
                checked,
            })
        });
        id
    }

    pub(super) fn leave(id: u64) {
        RADIOS.with(|r| r.borrow_mut().retain(|x| x.id != id));
    }

    /// React's `restoreControlledState` for a radio: every radio of the same name in the same
    /// form as `changed` shows its owner's state again.
    pub(super) fn restore(name: &str, changed: &web_sys::HtmlInputElement) {
        let form = changed.form();
        // Read outside the borrow: a derived `checked` runs the owner's code.
        let group: Vec<_> = RADIOS.with(|r| {
            r.borrow()
                .iter()
                .filter(|x| x.name == name)
                .map(|x| (x.node, x.checked))
                .collect()
        });
        for (node, checked) in group {
            if let Some(input) = node.get_untracked()
                && input.form() == form
            {
                input.set_checked(checked.get_untracked());
            }
        }
    }
}
