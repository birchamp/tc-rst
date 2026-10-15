//! tC4's `components/feedback`: ConfirmDialog, EmptyState, InlineError, Spinner, Toast,
//! Tooltip.

use std::sync::Arc;

use leptos::ev::MouseEvent;
use leptos::prelude::*;

use crate::ds::primitives::error_glyph;
use crate::ds::{
    Anchor, Attrs, Css, Direction, Dismiss, EmptyVariant, Fill, Justify, LayerAlign,
    LayerPlacement, Level, Measure, MeasureToken, ProgressShape, Scrim, ScrimToken, Shape, Side,
    Size, Slot, SpinnerTone, Stack, Surface, Text, TextRole, TextTone, ToastTone, Tone, Weight,
};
use crate::tokens::{Align, Corner, Edge, Inset, Shadow};

/// ConfirmDialog.jsx: a confirmation for irreversible actions. Dismissal is Escape only, so a
/// stray click on the backdrop cannot cancel something consequential.
#[component]
pub fn ConfirmDialog(
    #[prop(optional, into)] open: Signal<bool>,
    #[prop(optional, into)] title: String,
    #[prop(optional, into)] description: Option<String>,
    #[prop(optional, into)] consequence: Option<String>,
    #[prop(optional, into)] confirm_label: Option<String>,
    #[prop(optional, into)] cancel_label: Option<String>,
    #[prop(optional)] destructive: bool,
    #[prop(optional)] on_confirm: Option<Callback<()>>,
    #[prop(optional)] on_cancel: Option<Callback<()>>,
) -> impl IntoView {
    let id = crate::ds::next_id("tc-cfm");
    let confirm_label = confirm_label.unwrap_or_else(|| "Confirm".into());
    let cancel_label = cancel_label.unwrap_or_else(|| "Cancel".into());
    let description = description.filter(|s| !s.is_empty());
    let consequence = consequence.filter(|s| !s.is_empty());
    let cancel_click = on_cancel.map(|cb| Callback::new(move |_: MouseEvent| cb.run(())));
    let confirm_click = on_confirm.map(|cb| Callback::new(move |_: MouseEvent| cb.run(())));
    let dismiss = on_cancel.map(|cb| Callback::new(move |_| cb.run(())));
    let label_id = id.clone();
    let children: ChildrenFn = Arc::new(move || {
        let (title, description, consequence) =
            (title.clone(), description.clone(), consequence.clone());
        let (cancel_label, confirm_label) = (cancel_label.clone(), confirm_label.clone());
        let id = id.clone();
        view! {
            <Surface fill=Fill::Card radius=Corner::X2l elevation=Shadow::Modal style=css! { "width": "var(--modal-w-sm)", "max-width": "100%" }>
                <Stack direction=Direction::Column gap=16 style=css! { "padding": 24 }>
                    <Stack direction=Direction::Column gap=5>
                        <Text attrs=Attrs::from([("id", id.as_str())]) role=TextRole::H3>{title}</Text>
                        {description.map(|d| view! { <Text role=TextRole::Caption>{d}</Text> })}
                    </Stack>
                    {consequence.map(|c| view! {
                        <Surface tone=Tone::Warn fill=Fill::Soft border=Edge::Tone radius=Corner::Lg pad=Inset::Sm>
                            <Text role=TextRole::Caption tone=TextTone::Tone>{c}</Text>
                        </Surface>
                    })}
                    <Stack direction=Direction::Row gap=10 justify=Justify::End>
                        {action(Weight::Quiet, Tone::Neutral, cancel_click, cancel_label)}
                        {action(Weight::Fill, if destructive { Tone::Invalid } else { Tone::Accent }, confirm_click, confirm_label)}
                    </Stack>
                </Stack>
            </Surface>
        }
        .into_any()
    });
    crate::ds::Layer(crate::ds::LayerProps {
        open: Some(open),
        level: Level::Overlay,
        scrim: Some(Scrim::Token(ScrimToken::Modal)),
        placement: LayerPlacement::Center,
        anchor_to: None,
        offset: 6.0,
        align: LayerAlign::Start,
        side: Side::Bottom,
        dismiss: Dismiss {
            scrim: false,
            escape: true,
            outside: false,
        },
        trap_focus: true,
        lock_scroll: true,
        restore_focus: true,
        on_dismiss: dismiss,
        navigate: None,
        animate: true,
        scrim_attrs: Attrs::new(),
        role: Some("alertdialog".into()),
        label: None,
        labelled_by: Some(label_id),
        on_mouse_down: None,
        style: Css::new(),
        attrs: Attrs::new(),
        children,
    })
}

/// An Action with an optional click handler (Leptos props cannot forward an `Option` through
/// `view!`).
pub(crate) fn action(
    weight: Weight,
    tone: Tone,
    on_click: Option<Callback<MouseEvent>>,
    label: String,
) -> impl IntoView {
    crate::ds::Action(crate::ds::ActionProps {
        weight,
        tone: Some(tone),
        size: Size::Md,
        shape: Default::default(),
        icon_only: false,
        title: None,
        disabled: Signal::from(false),
        tag: None,
        href: None,
        on_click,
        style: Signal::from(Css::new()),
        attrs: Attrs::new(),
        children: Some(Box::new(move || label.into_any())),
    })
}

/// EmptyState.jsx: an empty list, filter or panel.
#[component]
pub fn EmptyState(
    #[prop(optional, into)] title: Option<String>,
    #[prop(optional, into)] description: Option<String>,
    #[prop(optional)] action: Option<Slot>,
    #[prop(optional)] variant: EmptyVariant,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let title = title.filter(|s| !s.is_empty());
    let description = description.filter(|s| !s.is_empty());
    if variant == EmptyVariant::Inline {
        let s = css! { "font-style": "italic", "margin": "6px 2px" }.merge(&style);
        let text = title.or(description);
        return view! { <Text role=TextRole::Caption style=s attrs=attrs>{text}</Text> }.into_any();
    }
    view! {
        <Surface border=Edge::Dashed radius=Corner::Xl pad="38px 24px" style=style attrs=attrs>
            <Stack direction=Direction::Column gap=6 align=Align::Center>
                {title.map(|t| view! { <Text role=TextRole::TitleSm>{t}</Text> })}
                {description.map(|d| view! {
                    <Text role=TextRole::Caption align="center" measure=Measure::Token(MeasureToken::Narrow)>{d}</Text>
                })}
                {action.map(|a| view! { <span style="margin-top: 10px;">{a()}</span> })}
            </Stack>
        </Surface>
    }
    .into_any()
}

/// InlineError.jsx: an error message attached to one field. `Field error=…` is the
/// replacement, and it also wires the ARIA this cannot.
#[component]
pub fn InlineError(
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let s = css! { "display": "flex", "align-items": "flex-start", "gap": 6 }.merge(&style);
    let mut all = Attrs::from([("data-tone", "invalid")]);
    all.extend(&attrs);
    let attrs = all;
    view! {
        <Text role=TextRole::CaptionStrong tag="p" tone=TextTone::Tone style=s attrs=attrs>
            {error_glyph()}
            <span role="alert">{children.map(|c| c())}</span>
        </Text>
    }
}

/// Spinner.jsx: an activity indicator for waits with no known duration. `inverse` is neutral,
/// which is translucent white on a dark ground.
#[component]
pub fn Spinner(
    #[prop(default = 18.0)] size: f64,
    #[prop(optional)] tone: SpinnerTone,
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let tone = match tone {
        SpinnerTone::Tone(t) => t,
        SpinnerTone::Inverse => Tone::Neutral,
    };
    crate::ds::Progress(crate::ds::ProgressProps {
        value: Signal::from(None),
        shape: ProgressShape::Ring,
        tone,
        height: 5.0,
        size,
        label,
        style,
        attrs,
    })
}

/// Toast.jsx: a transient confirmation that states what happened, in the past tense.
#[component]
pub fn Toast(
    #[prop(optional)] tone: ToastTone,
    #[prop(optional, into)] message: String,
    #[prop(optional)] action: Option<Callback<MouseEvent>>,
    #[prop(optional, into)] action_label: Option<String>,
    #[prop(optional)] on_dismiss: Option<Callback<MouseEvent>>,
    #[prop(optional, into)] dismiss_label: Option<String>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let t = match tone {
        ToastTone::Success => Tone::Valid,
        ToastTone::Info => Tone::Accent,
        ToastTone::Warn => Tone::Warn,
        ToastTone::Error => Tone::Invalid,
    };
    let dismiss_label = dismiss_label.unwrap_or_else(|| "Dismiss".into());
    let action_view = action.map(|cb| {
        crate::ds::Action(crate::ds::ActionProps {
            weight: Weight::Text,
            tone: Some(t),
            size: Size::Sm,
            shape: Default::default(),
            icon_only: false,
            title: None,
            disabled: Signal::from(false),
            tag: None,
            href: None,
            on_click: Some(cb),
            style: Signal::from(Css::new()),
            attrs: Attrs::new(),
            children: action_label.map(|l| Box::new(move || l.into_any()) as Children),
        })
    });
    let dismiss_view = on_dismiss.map(|cb| {
        close_button(
            dismiss_label,
            cb,
            Weight::Text,
            Shape::Pill,
            css! { "width": 20, "height": 20, "min-width": 20 },
        )
    });
    let dot = css! { "width": 9, "height": 9, "border-radius": "var(--radius-pill)", "background": "var(--tone)", "flex": "none" };
    view! {
        <Surface tone=t fill=Fill::Card border=Edge::Line radius=Corner::Lg elevation=Shadow::Hover style=style attrs=attrs>
            <Stack
                direction=Direction::Row
                gap=11
                align=Align::Center
                attrs=Attrs::from([("role", "status")])
                style=css! { "padding": "12px 14px", "min-width": "var(--toast-w-min)", "max-width": "var(--toast-w-max)" }
            >
                <span aria-hidden="true" style=dot.to_style()></span>
                <Text role=TextRole::Ui style=css! { "flex": 1 }>{message}</Text>
                {action_view}
                {dismiss_view}
            </Stack>
        </Surface>
    }
}

/// The small ✕ icon-only Action that closes a toast, a modal or a drawer.
pub(crate) fn close_button(
    title: String,
    on_click: Callback<MouseEvent>,
    weight: Weight,
    shape: Shape,
    style: Css,
) -> impl IntoView {
    crate::ds::Action(crate::ds::ActionProps {
        weight,
        tone: Some(Tone::Neutral),
        size: Size::Sm,
        shape,
        icon_only: true,
        title: Some(title),
        disabled: Signal::from(false),
        tag: None,
        href: None,
        on_click: Some(on_click),
        style: Signal::from(style),
        attrs: Attrs::new(),
        children: Some(Box::new(|| "✕".into_any())),
    })
}

/// Tooltip.jsx: the Ocean hover label. It shows on focus as well as on hover, because a tooltip
/// that only appears on hover is a keyboard loss.
#[component]
pub fn Tooltip(
    #[prop(into)] label: String,
    /// The preferred side; the layer flips when there is no room.
    #[prop(optional)]
    placement: Side,
    #[prop(optional)] style: Css,
    children: Children,
) -> impl IntoView {
    let anchor = NodeRef::<leptos::html::Span>::new();
    let on = RwSignal::new(false);
    let surface_style = css! { "pointer-events": "none" }.merge(&style);
    let content: ChildrenFn = Arc::new(move || {
        let label = label.clone();
        view! {
            <Surface tone=Tone::Ocean fill=Fill::Solid radius=Corner::Sm elevation=Shadow::Hover style=surface_style.clone()>
                <Text role=TextRole::Ui style=css! { "color": "var(--tone-on-fill)", "padding": "6px 9px", "white-space": "nowrap" }>
                    {label}
                </Text>
            </Surface>
        }
        .into_any()
    });
    let anchor_to: Anchor = Callback::new(move |_| anchor.get_untracked().map(Into::into));
    view! {
        <span
            node_ref=anchor
            style="display: inline-flex;"
            on:mouseenter=move |_| on.set(true)
            on:mouseleave=move |_| on.set(false)
            on:focusin=move |_| on.set(true)
            on:focusout=move |_| on.set(false)
        >
            {children()}
        </span>
        {crate::ds::Layer(crate::ds::LayerProps {
            open: Some(on.into()),
            level: Level::Tooltip,
            scrim: None,
            placement: LayerPlacement::Anchor,
            anchor_to: Some(anchor_to),
            offset: 7.0,
            align: LayerAlign::Center,
            side: placement,
            dismiss: Dismiss::NONE,
            trap_focus: false,
            lock_scroll: false,
            restore_focus: false,
            on_dismiss: None,
            navigate: None,
            animate: true,
            scrim_attrs: Attrs::new(),
            role: Some("tooltip".into()),
            label: None,
            labelled_by: None,
            on_mouse_down: None,
            style: Css::new(),
            attrs: Attrs::new(),
            children: content,
        })}
    }
}
