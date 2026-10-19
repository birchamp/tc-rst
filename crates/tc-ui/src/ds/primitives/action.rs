//! Action.jsx: anything the user activates.

use leptos::ev::{self, MouseEvent, PointerEvent};
use leptos::prelude::*;

use crate::ds::{Attrs, Css, Shape, Size, Tag, Tone, Weight, element, live, spread};

/// The weight's own declarations (`WEIGHT` in Action.jsx) and its `data-i` role.
fn weight_css(weight: Weight) -> (Css, &'static str) {
    match weight {
        Weight::Fill => (
            css! { "background": "var(--tone-fill)", "color": "var(--tone-on-fill)", "border": "0", "box-shadow": "var(--shadow-raised)" },
            "fill",
        ),
        Weight::Soft => (
            css! { "background": "var(--fill-quiet-hover)", "color": "var(--fg)", "border": "0" },
            "quiet",
        ),
        Weight::Outline => (
            css! { "background": "var(--fill-plain)", "color": "var(--tone-text)", "border": "var(--stroke-selected) solid var(--tone-border)" },
            "outline",
        ),
        Weight::Quiet => (
            css! { "background": "var(--fill-plain)", "color": "var(--fg-muted)", "border": "var(--stroke) solid var(--line-strong)" },
            "quiet",
        ),
        Weight::Text => (
            css! { "background": "transparent", "color": "var(--tone-text)", "border": "0" },
            "text",
        ),
        Weight::Row => (
            css! { "background": "transparent", "color": "var(--fg)", "border": "0" },
            "row",
        ),
    }
}

fn by_size(size: Size, sm: &'static str, md: &'static str, lg: &'static str) -> &'static str {
    match size {
        Size::Sm => sm,
        Size::Md => md,
        Size::Lg => lg,
    }
}

/// The inline style Action.jsx writes.
pub(crate) fn action_css(
    weight: Weight,
    tone: Option<Tone>,
    size: Size,
    shape: Shape,
    icon_only: bool,
    disabled: bool,
    style: &Css,
) -> Css {
    let toned = tone.is_some_and(|t| t != Tone::Neutral);
    let square = shape == Shape::Square || icon_only;
    let flat = matches!(weight, Weight::Text | Weight::Row);
    let h = by_size(
        size,
        "var(--control-h-sm)",
        "var(--control-h-md)",
        "var(--control-h-lg)",
    );
    let text = weight == Weight::Text;
    let mut s = css! {
        "box-sizing": "border-box",
        "font-family": "var(--font-ui)",
        "font-weight": "var(--fw-heavy)",
        "letter-spacing": if text {
            by_size(size, "var(--track-12)", "var(--track-12-5)", "var(--track-13)")
        } else {
            by_size(size, "var(--track-12-5)", "var(--track-13-5)", "var(--track-14-5)")
        },
        "display": "inline-flex",
        "align-items": "center",
        "gap": "var(--control-gap)",
        "justify-content": if weight == Weight::Row { "flex-start" } else { "center" },
        "text-align": if weight == Weight::Row { "start" } else { "center" },
        "text-decoration": "none",
        "cursor": if disabled { "default" } else { "pointer" },
        "flex": "none",
        "line-height": 1.2,
        "white-space": "nowrap",
        "font-size": if text {
            by_size(size, "var(--fs-caption)", "var(--fs-caption-lg)", "var(--fs-ui-sm)")
        } else {
            by_size(size, "var(--control-fs-sm)", "var(--control-fs-md)", "var(--control-fs-lg)")
        },
    };
    s.extend(&weight_css(weight).0);
    if toned && matches!(weight, Weight::Row | Weight::Soft) {
        s.set("color", "var(--tone-text)");
    }
    s.set(
        "border-radius",
        match shape {
            Shape::Block => "var(--radius-md)",
            Shape::Square => "var(--radius-sm)",
            Shape::Pill => "var(--radius-pill)",
        },
    );
    if square {
        s.extend(&css! { "width": h, "height": h, "padding": 0 });
    }
    if !square && !flat {
        s.extend(&css! {
            "height": h,
            "padding-inline": by_size(size, "var(--action-px-sm)", "var(--action-px-md)", "var(--action-px-lg)"),
            "padding-block": 0,
        });
    }
    if text {
        s.extend(&css! { "padding": 0, "height": "auto", "border-radius": 0 });
    }
    if weight == Weight::Row {
        s.extend(&css! {
            "width": "100%",
            "flex": "initial",
            "padding": "8px 10px",
            "height": "auto",
            "border-radius": "var(--radius-sm)",
            "font-size": "var(--fs-ui-sm)",
            "letter-spacing": "var(--track-13)",
            "font-weight": "var(--fw-bold)",
        });
    }
    if shape == Shape::Block {
        s.extend(&css! { "width": "100%", "flex": "initial" });
    }
    if disabled {
        s.extend(&css! {
            "background": if flat { "transparent" } else { "var(--disabled-bg)" },
            "color": "var(--disabled-fg)",
            "border": "0",
            "box-shadow": "none",
        });
    }
    s.extend(style);
    s
}

/// Anything the user activates: a filled primary, a hairline alternative, a text link, an
/// icon square, a menu row. Weight is how loud it is; tone is what it means.
///
/// `title`, `disabled`, `aria_busy` and `style` may change while the action is on screen; the
/// element stays, so it keeps the focus. tC4's sync button reads "Syncing…" in its title and
/// is busy while a sync runs, and the Previous and Next pills drop their accent colour at the
/// ends.
#[component]
pub fn Action(
    #[prop(optional)] weight: Weight,
    #[prop(optional)] tone: Option<Tone>,
    #[prop(optional)] size: Size,
    #[prop(optional)] shape: Shape,
    #[prop(optional)] icon_only: bool,
    #[prop(optional, into)] title: Signal<Option<String>>,
    #[prop(optional, into)] disabled: Signal<bool>,
    /// `aria-busy="true"` while true; absent otherwise (React's `aria-busy={busy || undefined}`).
    #[prop(optional, into)]
    aria_busy: Signal<bool>,
    #[prop(optional, into)] tag: Option<Tag>,
    #[prop(optional, into)] href: Option<String>,
    #[prop(optional)] on_click: Option<Callback<MouseEvent>>,
    /// React's `onPointerDownCapture`. tC4 calls `preventDefault` there on a control that is
    /// off while an editing card is open, so a press keeps the focus in the card (#698).
    #[prop(optional)]
    on_pointer_down_capture: Option<Callback<PointerEvent>>,
    #[prop(optional, into)] style: Signal<Css>,
    #[prop(optional)] attrs: Attrs,
    /// Attributes whose values follow signals (`None` removes one), such as the account
    /// menu's `aria-checked` on its internet row, which stays open and keeps the focus.
    #[prop(optional)]
    live_attrs: Vec<(&'static str, Signal<Option<String>>)>,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let tag: Tag = if href.is_some() {
        "a".into()
    } else {
        tag.unwrap_or("button".into())
    };
    let is_button = tag == "button";
    let square = shape == Shape::Square || icon_only;
    let i = weight_css(weight).1;
    element(tag)
        .attr("type", is_button.then_some("button"))
        .attr("href", href)
        .attr("data-tone", tone.unwrap_or(Tone::Accent).as_str())
        .attr("data-i", move || (!disabled.get()).then_some(i))
        .attr("disabled", move || is_button && disabled.get())
        .attr("aria-disabled", move || {
            (!is_button && disabled.get()).then_some("true")
        })
        .attr("title", move || title.get())
        .attr("aria-label", move || {
            icon_only.then(|| title.get()).flatten()
        })
        .attr("aria-busy", move || aria_busy.get().then_some("true"))
        .attr("data-trim", (!square).then_some("cap"))
        .attr("style", move || {
            style.with(|st| {
                action_css(weight, tone, size, shape, icon_only, disabled.get(), st).to_style()
            })
        })
        .add_any_attr(spread(&attrs))
        .add_any_attr(live(live_attrs))
        .on(ev::click, move |e| {
            if let Some(cb) = on_click
                && !disabled.get_untracked()
            {
                cb.run(e);
            }
        })
        .on(ev::capture(ev::pointerdown), move |e| {
            if let Some(cb) = on_pointer_down_capture {
                cb.run(e);
            }
        })
        .child(children.map(|c| c()))
}
