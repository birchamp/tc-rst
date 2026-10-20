//! tC4's `components/core`: shims that keep the old names and props and compose the
//! primitives.

use leptos::ev::{MouseEvent, PointerEvent};
use leptos::prelude::*;

use crate::ds::{
    Action, Attrs, BadgeSize, BadgeTone, ButtonVariant, Css, Direction, Fill, IconButtonVariant,
    Orientation, OverlineTone, Progress, Shape, Size, Stack, Status, Surface, Tag, Text, TextRole,
    TextTone, Tone, Weight, spread,
};
use crate::tokens::{Align, Corner};

/// Badge.jsx: an uppercase micro-pill label attached to a card or row.
#[component]
pub fn Badge(
    #[prop(optional)] tone: BadgeTone,
    #[prop(optional)] size: BadgeSize,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let (t, fill) = match tone {
        BadgeTone::Accent => (Tone::Accent, Fill::Solid),
        BadgeTone::AccentSoft => (Tone::Accent, Fill::Soft),
        BadgeTone::Cultivate => (Tone::Cultivate, Fill::Solid),
        BadgeTone::Ocean => (Tone::Ocean, Fill::Solid),
        BadgeTone::Neutral => (Tone::Neutral, Fill::Soft),
        BadgeTone::Valid => (Tone::Valid, Fill::Soft),
        BadgeTone::Warn => (Tone::Warn, Fill::Soft),
    };
    let solid = fill == Fill::Solid;
    let sm = size == BadgeSize::Sm;
    let outer =
        css! { "display": "inline-flex", "padding": if sm { "3px 8px" } else { "2px 7px" } }
            .merge(&style);
    let text_style = if solid {
        css! { "color": "var(--tone-on-fill)" }
    } else {
        Css::new()
    };
    view! {
        <Surface tag="span" tone=t fill=fill radius=Corner::Pill style=outer attrs=attrs>
            <Text
                role=if sm { TextRole::LabelMicro } else { TextRole::Label }
                tone=if solid { TextTone::Default } else { TextTone::Tone }
                style=text_style
            >
                {children.map(|c| c())}
            </Text>
        </Surface>
    }
}

/// Button.jsx: the old variant enum over Action's weight and tone. `disabled`, `title`,
/// `aria_busy` and `style` may change while the button is on screen (see [`Action`]).
#[component]
pub fn Button(
    #[prop(optional)] variant: ButtonVariant,
    #[prop(optional)] size: Size,
    #[prop(optional)] shape: Shape,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional, into)] title: Signal<Option<String>>,
    /// `aria-busy="true"` while true.
    #[prop(optional, into)]
    aria_busy: Signal<bool>,
    #[prop(optional)] on_click: Option<Callback<MouseEvent>>,
    /// React's `onPointerDownCapture`, passed through to the button.
    #[prop(optional)]
    on_pointer_down_capture: Option<Callback<PointerEvent>>,
    #[prop(optional, into)] style: Signal<Css>,
    #[prop(optional)] attrs: Attrs,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let (weight, tone) = match variant {
        ButtonVariant::Primary => (Weight::Fill, Tone::Accent),
        ButtonVariant::Ocean => (Weight::Fill, Tone::Ocean),
        ButtonVariant::Secondary => (Weight::Quiet, Tone::Neutral),
        ButtonVariant::Outline => (Weight::Outline, Tone::Ocean),
        ButtonVariant::Ghost => (Weight::Text, Tone::Accent),
        ButtonVariant::Valid => (Weight::Fill, Tone::Valid),
        ButtonVariant::Danger => (Weight::Outline, Tone::Invalid),
    };
    Action(crate::ds::ActionProps {
        weight,
        tone: Some(tone),
        size,
        shape,
        title,
        disabled,
        aria_busy,
        on_click,
        on_pointer_down_capture,
        style,
        attrs,
        children,
        ..Default::default()
    })
}

/// Divider.jsx: a hairline rule between groups. `inverse` is accepted and ignored: Rule reads
/// `--line` from the surface context, so a rule on a dark ground is already right.
#[component]
pub fn Divider(
    #[prop(optional)] orientation: Orientation,
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional)]
    #[allow(unused_variables)]
    inverse: bool,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    crate::ds::Rule(crate::ds::RuleProps {
        orientation,
        label,
        style,
        attrs,
        ..Default::default()
    })
}

/// IconButton.jsx: a square icon-only button. The old `size` is a raw number of px, kept as an
/// explicit box because most of its values are off the control height ladder.
#[component]
pub fn IconButton(
    #[prop(optional)] variant: IconButtonVariant,
    #[prop(optional)] size: Option<f64>,
    #[prop(optional, into)] title: Option<String>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional)] on_click: Option<Callback<MouseEvent>>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let (weight, radius) = match variant {
        IconButtonVariant::Outline => (Weight::Quiet, None),
        IconButtonVariant::Muted => (Weight::Soft, Some("var(--radius-pill)")),
        IconButtonVariant::Plain => (Weight::Text, None),
    };
    let mut s = Css::new();
    if let Some(r) = radius {
        s.set("border-radius", r);
    }
    if let Some(n) = size {
        s.extend(&css! { "width": n, "height": n, "min-width": n });
    }
    s.extend(&style);
    Action(crate::ds::ActionProps {
        weight,
        tone: Some(Tone::Neutral),
        size: if size.is_some_and(|n| n <= 28.0) {
            Size::Sm
        } else {
            Size::Md
        },
        shape: Shape::Square,
        icon_only: true,
        title: title.into(),
        disabled,
        on_click,
        style: Signal::from(s),
        attrs,
        children,
        ..Default::default()
    })
}

/// Overline.jsx: the tracked-out uppercase micro-heading. `inverse` maps to muted, which on a
/// dark ground resolves to white at 60%.
#[component]
pub fn Overline(
    #[prop(optional)] tone: OverlineTone,
    #[prop(optional, into)] tag: Option<Tag>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    crate::ds::Text(crate::ds::TextProps {
        role: TextRole::Overline,
        tag: Some(tag.unwrap_or("span".into())),
        tone: if tone == OverlineTone::Accent {
            TextTone::Accent
        } else {
            TextTone::Muted
        },
        style: Signal::from(css! { "display": "block" }.merge(&style)),
        attrs,
        children,
        ..Default::default()
    })
}

/// ProgressBar.jsx: a flat progress rail on a frost track.
#[component]
pub fn ProgressBar(
    #[prop(optional, into)] value: Signal<f64>,
    #[prop(optional)] tone: Tone,
    #[prop(default = 5.0)] height: f64,
    /// The name a screen reader reads for the bar.
    #[prop(default = None)]
    label: Option<String>,
    /// The smallest fill in px once the value is above 0 (see [`Progress`]).
    #[prop(default = None)]
    min_fill: Option<f64>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    match label {
        Some(label) => view! { <Progress value=Signal::derive(move || Some(value.get())) tone=tone height=height label=label min_fill=min_fill style=style attrs=attrs /> }.into_any(),
        None => view! { <Progress value=Signal::derive(move || Some(value.get())) tone=tone height=height min_fill=min_fill style=style attrs=attrs /> }.into_any(),
    }
}

/// StatusDot.jsx: a round state pip for checking items, save state and conflict rows. `todo`
/// is the palette's black, not a tone.
#[component]
pub fn StatusDot(
    #[prop(optional, into)] status: Signal<Status>,
    #[prop(default = 9.0)] size: f64,
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let tone = move || match status.get() {
        Status::Valid => Some("valid"),
        Status::Invalid => Some("invalid"),
        Status::Warn => Some("warn"),
        Status::Accent => Some("accent"),
        Status::Todo => None,
    };
    let pip = move || {
        let s = css! {
            "width": size,
            "height": size,
            "flex": "none",
            "border-radius": "var(--radius-pill)",
            "background": if tone().is_some() { "var(--tone)" } else { "var(--tc-todo)" },
        };
        view! { <span aria-hidden="true" style=s.to_style()></span> }
    };
    match label.filter(|l| !l.is_empty()) {
        None => {
            let s = css! { "display": "inline-flex" }.merge(&style);
            view! { <span data-tone=tone style=s.to_style() {..spread(&attrs)}>{pip}</span> }
                .into_any()
        }
        Some(label) => {
            // data-tone reaches the Stack through its extra attributes, which cannot be
            // reactive; a status that changes redraws the row.
            (move || {
                let mut a = Attrs::new();
                if let Some(t) = tone() {
                    a.set("data-tone", t);
                }
                a.extend(&attrs);
                let label = label.clone();
                view! {
                    <Stack direction=Direction::Row gap=6 align=Align::Center style=style.clone() attrs=a>
                        {pip()}
                        <Text role=TextRole::CaptionStrong>{label}</Text>
                    </Stack>
                }
            })
            .into_any()
        }
    }
}
