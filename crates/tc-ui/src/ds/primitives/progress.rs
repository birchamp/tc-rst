//! Progress.jsx: how far along something is. `value: None` is the same rail or arc with no
//! known end, which is the only difference between a progress bar and a spinner.

use leptos::prelude::*;

use crate::css::js_number;
use crate::ds::{Attrs, Css, ProgressShape, Tone, spread};

#[component]
pub fn Progress(
    /// 0 to 100, or `None` for an indeterminate wait.
    #[prop(optional, into)]
    value: Signal<Option<f64>>,
    #[prop(optional)] shape: ProgressShape,
    #[prop(optional)] tone: Tone,
    /// The bar's height in px.
    #[prop(default = 5.0)]
    height: f64,
    /// The ring's diameter in px.
    #[prop(default = 18.0)]
    size: f64,
    #[prop(optional, into)] label: Option<String>,
    /// The bar's smallest fill in px once its value is above 0, so a small share still shows
    /// (tC4 #764, the Check bars). None draws the share as it is.
    #[prop(default = None)]
    min_fill: Option<f64>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let pct = move || value.get().map(|v| v.clamp(0.0, 100.0));
    let label = label.filter(|l| !l.is_empty());
    let aria_label = label.clone();
    let aria = move || {
        use leptos::tachys::html::attribute::custom::custom_attribute as attr;
        (
            attr("role", "progressbar"),
            attr("aria-label", aria_label.clone()),
            attr("aria-valuenow", move || pct().map(|p| js_number(p.round()))),
            attr("aria-valuemin", move || pct().map(|_| "0")),
            attr("aria-valuemax", move || pct().map(|_| "100")),
            attr("aria-busy", move || pct().is_none().then_some("true")),
        )
    };

    if shape == ProgressShape::Ring {
        let stroke = (size / 9.0).round().max(2.0);
        let r = (size - stroke) / 2.0;
        let circ = 2.0 * std::f64::consts::PI * r;
        let outer =
            css! { "display": "inline-flex", "align-items": "center", "gap": 9 }.merge(&style);
        // Only the switch between a known and an unknown end draws a new node; a new value
        // moves the arc on the same circle, so its stroke-dashoffset transition runs.
        let known = Memo::new(move |_| pct().is_some());
        let mark = move || match known.get() {
            false => {
                let s = css! {
                    "width": size,
                    "height": size,
                    "flex": "none",
                    "border-radius": "var(--radius-pill)",
                    "display": "inline-block",
                    "border": format!("{}px solid var(--surface-muted)", js_number(stroke)),
                    "border-top-color": "var(--tone)",
                    "animation": "tcSpin 800ms linear infinite",
                };
                view! { <span {..aria()} style=s.to_style()></span> }.into_any()
            }
            true => {
                let c = js_number(size / 2.0);
                let offset = move || js_number(circ * (1.0 - pct().unwrap_or(0.0) / 100.0));
                view! {
                    <svg
                        {..aria()}
                        width=js_number(size)
                        height=js_number(size)
                        viewBox=format!("0 0 {} {}", js_number(size), js_number(size))
                        style="flex: none; transform: rotate(-90deg);"
                    >
                        <circle
                            cx=c.clone()
                            cy=c.clone()
                            r=js_number(r)
                            fill="none"
                            stroke="var(--surface-muted)"
                            stroke-width=js_number(stroke)
                        ></circle>
                        <circle
                            cx=c.clone()
                            cy=c
                            r=js_number(r)
                            fill="none"
                            stroke="var(--tone)"
                            stroke-width=js_number(stroke)
                            stroke-linecap="round"
                            stroke-dasharray=js_number(circ)
                            stroke-dashoffset=offset
                            style="transition: stroke-dashoffset var(--dur-enter) var(--ease-standard);"
                        ></circle>
                    </svg>
                }
                .into_any()
            }
        };
        let label_css = css! {
            "font-family": "var(--font-ui)",
            "font-size": "var(--fs-caption-lg)",
            "letter-spacing": "var(--track-12-5)",
            "font-weight": "var(--fw-bold)",
            "color": "var(--fg-muted)",
        };
        return view! {
            <span data-tone=tone.as_str() style=outer.to_style() {..spread(&attrs)}>
                {mark}
                {label.map(|l| view! { <span style=label_css.to_style()>{l}</span> })}
            </span>
        }
        .into_any();
    }

    let outer = css! {
        "height": height,
        "border-radius": "var(--radius-pill)",
        "background": "var(--surface-muted)",
        "overflow": "hidden",
    }
    .merge(&style);
    let fill = move || {
        let p = pct();
        let mut s = css! { "height": "100%", "border-radius": "var(--radius-pill)", "background": "var(--tone)" };
        s.set(
            "width",
            p.map_or_else(|| "35%".to_string(), |p| format!("{}%", js_number(p))),
        );
        s.set_opt(
            "animation",
            p.is_none()
                .then_some("tcSlide 1200ms var(--ease-standard) infinite"),
        );
        s.set_opt(
            "transition",
            p.map(|_| "width var(--dur-enter) var(--ease-standard)"),
        );
        s.set_opt(
            "min-width",
            min_fill
                .filter(|_| p.is_some_and(|p| p > 0.0))
                .map(|px| format!("{}px", js_number(px))),
        );
        s.to_style()
    };
    view! {
        <div data-tone=tone.as_str() {..aria()} style=outer.to_style() {..spread(&attrs)}>
            <div style=fill></div>
        </div>
    }
    .into_any()
}
