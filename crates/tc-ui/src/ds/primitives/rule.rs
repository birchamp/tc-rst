//! Rule.jsx: a separator, the one shape a Surface cannot make (a single edge).

use leptos::prelude::*;

use crate::ds::{Attrs, Css, Orientation, RuleWeight, spread};

#[component]
pub fn Rule(
    #[prop(optional)] orientation: Orientation,
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional)] weight: RuleWeight,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let c = "var(--line)";
    let w = match weight {
        RuleWeight::Hair => "var(--stroke-hair)",
        RuleWeight::Line => "var(--stroke)",
    };
    if orientation == Orientation::Vertical {
        let s = css! { "width": w, "align-self": "stretch", "min-height": 26, "background": c, "flex": "none" }.merge(&style);
        return view! { <span role="separator" aria-orientation="vertical" style=s.to_style() {..spread(&attrs)}></span> }
            .into_any();
    }
    if let Some(label) = label.filter(|l| !l.is_empty()) {
        let s = css! { "display": "flex", "align-items": "center", "gap": 10 }.merge(&style);
        let side = css! { "height": w, "flex": 1, "background": c }.to_style();
        let text = css! {
            "font-family": "var(--font-ui)",
            "font-size": "var(--fs-label)",
            "font-weight": "var(--fw-heavy)",
            "letter-spacing": "var(--tracking-overline)",
            "text-transform": "uppercase",
            "color": "var(--fg-muted)",
        };
        return view! {
            <span role="separator" style=s.to_style() {..spread(&attrs)}>
                <span style=side.clone()></span>
                <span style=text.to_style()>{label}</span>
                <span style=side></span>
            </span>
        }
        .into_any();
    }
    let s = css! { "display": "block", "height": w, "background": c }.merge(&style);
    view! { <span role="separator" style=s.to_style() {..spread(&attrs)}></span> }.into_any()
}
