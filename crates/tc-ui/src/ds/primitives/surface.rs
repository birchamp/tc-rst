//! Surface.jsx: a bounded region of the interface: a fill, an edge, a corner, a shadow, a tone.

use leptos::ev::{self, KeyboardEvent, PointerEvent};
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::css::CssValue;
use crate::ds::{
    Attrs, Border, Css, Edge, Elevation, Fill, Ground, Interactive, Pad, Radius, Shadow, Tag, Tone,
    element, spread,
};
use crate::geometry::{corner_var, inner_radius};

fn fill_bg(fill: Fill) -> &'static str {
    match fill {
        Fill::None => "transparent",
        Fill::Solid => "var(--tone-fill)",
        Fill::Soft => "var(--tone-soft)",
        Fill::Card => "var(--surface-card)",
        Fill::App => "var(--surface-app)",
        Fill::Muted => "var(--surface-muted)",
        Fill::Quiet => "var(--fill-quiet)",
        Fill::Paper => "#fff",
    }
}

fn fill_fg(fill: Fill) -> Option<&'static str> {
    match fill {
        Fill::Solid => Some("var(--tone-on-fill)"),
        Fill::Soft => Some("var(--tone-on-soft)"),
        _ => None,
    }
}

/// `border(kind, selected)`. `None` is React's `0`.
fn border_value(border: &Border, selected: bool) -> CssValue {
    match border {
        Border::Edge(Edge::None) => CssValue::Number(0.0),
        Border::Edge(Edge::Hair) => "var(--stroke-hair) solid var(--line)".into(),
        Border::Edge(Edge::Line) => "var(--stroke) solid var(--line)".into(),
        Border::Edge(Edge::Input) => "var(--stroke) solid var(--line-strong)".into(),
        Border::Edge(Edge::Strong) => "var(--stroke) solid var(--border-strong)".into(),
        Border::Edge(Edge::Tone) => "var(--stroke) solid var(--tone-border)".into(),
        // Selectable things carry 1.5px at rest and selected, so choosing never shifts layout.
        Border::Edge(Edge::Choice) => {
            if selected {
                "var(--stroke-selected) solid var(--tone-border)".into()
            } else {
                "var(--stroke-selected) solid var(--line-strong)".into()
            }
        }
        Border::Edge(Edge::Dashed) => "var(--stroke-selected) dashed var(--border-dashed)".into(),
        Border::Edge(Edge::DashedTone) => "var(--stroke-selected) dashed var(--tone-border)".into(),
        Border::Css(c) => c.clone().into(),
    }
}

/// `resolveRadius`: a keyword's token, or a CSS length as is.
fn radius_value(radius: &Radius) -> String {
    match radius {
        Radius::Corner(c) => corner_var(*c).to_string(),
        Radius::Css(c) => c.clone(),
    }
}

fn shadow_value(e: &Elevation) -> String {
    match e {
        Elevation::Shadow(Shadow::None) => "none".into(),
        Elevation::Shadow(s) => format!("var(--shadow-{})", s.as_str()),
        Elevation::Css(c) => c.clone(),
    }
}

/// Everything a Surface's inline style depends on.
#[derive(Clone, Debug)]
pub(crate) struct SurfaceLook {
    pub border: Border,
    pub radius: Radius,
    pub elevation: Elevation,
    pub pad: Option<Pad>,
    pub clickable: bool,
}

impl SurfaceLook {
    /// The inline style Surface.jsx writes, before the caller's `style`.
    pub(crate) fn css(&self, fill: Fill, selected: bool, disabled: bool) -> Css {
        let mut s = css! { "box-sizing": "border-box" };
        s.set_opt(
            "--radius-inner",
            inner_radius(&self.radius, &self.border, self.pad.as_ref()),
        );
        let plain = matches!(fill, Fill::None | Fill::Card | Fill::Paper);
        s.set(
            "background",
            if selected && plain {
                "var(--tone-soft)"
            } else {
                fill_bg(fill)
            },
        );
        s.set_opt(
            "color",
            if selected && fill != Fill::Solid {
                Some("var(--tone-on-soft)")
            } else {
                fill_fg(fill)
            },
        );
        s.set("border", border_value(&self.border, selected));
        s.set("border-radius", radius_value(&self.radius));
        s.set("box-shadow", shadow_value(&self.elevation));
        let padding: Option<CssValue> = self.pad.as_ref().map(|p| match p {
            Pad::Px(n) => CssValue::Number(*n),
            Pad::Inset(i) => CssValue::Number(i.px()),
            Pad::Css(c) => c.clone().into(),
        });
        s.set_opt("padding", padding);
        s.set_opt("cursor", (self.clickable && !disabled).then_some("pointer"));
        s.set_opt("opacity", disabled.then_some(0.6));
        s
    }
}

/// A bounded region of the interface: every panel, card, pill, notice, sheet, row and bubble
/// in the product is one of these, and the differences between them are values. A Surface
/// with `on_click` that is not a button or a link is a control: it takes focus and answers
/// Enter and Space. `disabled`, `selected`, `fill` and `style` may change while it is on
/// screen: the element stays and only its attributes change.
#[component]
pub fn Surface(
    #[prop(optional, into)] tag: Option<Tag>,
    #[prop(optional)] tone: Option<Tone>,
    #[prop(optional, into)] fill: Signal<Fill>,
    #[prop(optional, into)] border: Option<Border>,
    #[prop(optional, into)] radius: Option<Radius>,
    #[prop(optional, into)] elevation: Option<Elevation>,
    #[prop(optional, into)] pad: Option<Pad>,
    #[prop(optional)] interactive: Option<Interactive>,
    #[prop(optional, into)] selected: Signal<bool>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional)] on: Option<Ground>,
    #[prop(optional)] on_click: Option<Callback<web_sys::Event>>,
    /// React's `onPointerDownCapture`, which tC4's screens pass through `...rest`.
    #[prop(optional)]
    on_pointer_down_capture: Option<Callback<PointerEvent>>,
    /// React's `onMouseEnter` and `onMouseLeave`, passed through `...rest` (the helps cards'
    /// hover focus, B3).
    #[prop(optional)]
    on_mouse_enter: Option<Callback<web_sys::MouseEvent>>,
    #[prop(optional)] on_mouse_leave: Option<Callback<web_sys::MouseEvent>>,
    #[prop(optional, into)] style: Signal<Css>,
    #[prop(optional)] attrs: Attrs,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let tag: Tag = tag.unwrap_or("div".into());
    // Solid fills dark enough that everything inside switches to the dark context.
    let dark_tone = matches!(
        tone,
        Some(Tone::Ocean | Tone::Accent | Tone::Valid | Tone::Invalid)
    );
    let ground = move || {
        on.or((dark_tone && fill.get() == Fill::Solid).then_some(Ground::Dark))
            .map(Ground::as_str)
    };
    let control = on_click.is_some() && tag != "button" && tag != "a";
    let is_button = tag == "button";
    let look = SurfaceLook {
        border: border.unwrap_or_default(),
        radius: radius.unwrap_or_default(),
        elevation: elevation.unwrap_or_default(),
        pad,
        clickable: on_click.is_some(),
    };
    element(tag)
        .attr("data-tone", tone.map(Tone::as_str))
        .attr("data-on", ground)
        .attr("data-i", move || {
            if disabled.get() {
                None
            } else {
                interactive.map(Interactive::as_str)
            }
        })
        .attr("data-selected", move || selected.get().then_some("true"))
        .attr("disabled", move || disabled.get() && is_button)
        .attr("aria-disabled", move || {
            (disabled.get() && !is_button).then_some("true")
        })
        .attr("role", control.then_some("button"))
        .attr("tabindex", move || {
            control.then_some(if disabled.get() { "-1" } else { "0" })
        })
        .attr("style", move || {
            style.with(|st| {
                look.css(fill.get(), selected.get(), disabled.get())
                    .merge(st)
                    .to_style()
            })
        })
        .add_any_attr(spread(&attrs))
        .on(ev::click, move |e| {
            if let Some(cb) = on_click
                && !disabled.get_untracked()
            {
                cb.run(e.unchecked_into());
            }
        })
        .on(ev::capture(ev::pointerdown), move |e| {
            if let Some(cb) = on_pointer_down_capture {
                cb.run(e);
            }
        })
        .on(ev::mouseenter, move |e| {
            if let Some(cb) = on_mouse_enter {
                cb.run(e);
            }
        })
        .on(ev::mouseleave, move |e| {
            if let Some(cb) = on_mouse_leave {
                cb.run(e);
            }
        })
        .on(ev::keydown, move |e: KeyboardEvent| {
            if control && (e.key() == "Enter" || e.key() == " ") {
                e.prevent_default();
                if let Some(cb) = on_click {
                    cb.run(e.unchecked_into());
                }
            }
        })
        .child(children.map(|c| c()))
}
