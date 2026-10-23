//! tC4's `components/surfaces`: BookTile, Callout, Card, Drawer, HelpCard, Modal.

use std::sync::Arc;

use leptos::ev::{MouseEvent, PointerEvent};
use leptos::prelude::*;

use crate::css::{CssValue, js_number};
use crate::ds::feedback::close_button;
use crate::ds::{
    Attrs, CalloutTone, CardVariant, Content, Css, Direction, Fill, HelpKind, Interactive, Justify,
    Layer, LayerPlacement, LayerProps, Pad, Progress, Scrim, ScrimToken, Shape, Slot, Stack,
    Surface, Text, TextRole, TextTone, TileLayout, Tone, Weight, present,
};
use crate::tokens::{Align, Border, Corner, Edge, Elevation, Inset, Radius, Shadow};

/// BookTile.jsx: a book button with its drafting progress. Children render below the bar.
///
/// `disabled`, `on_pointer_down_capture` and `style` reach the button, as tC4's `...rest` and
/// `style` do. tC4's book rail turns every tile off while an editing card is open (#698): it
/// passes `disabled`, a pointerdown-capture handler that calls `preventDefault` so the focus
/// stays in the card, and `cursor: default`. All three may change while the tile is on screen.
#[component]
pub fn BookTile(
    /// A signal, so a tile's words follow a language preview in place (B2).
    #[prop(into)]
    name: Signal<String>,
    #[prop(optional, into)] percent: Signal<f64>,
    /// Replaces the percentage ("—" when the progress is unknown).
    #[prop(optional, into)]
    meta: Signal<Option<String>>,
    #[prop(optional, into)] meta_title: Option<String>,
    #[prop(optional)] layout: TileLayout,
    #[prop(optional, into)] active: Signal<bool>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional)] on_click: Option<Callback<web_sys::Event>>,
    #[prop(optional)] on_pointer_down_capture: Option<Callback<PointerEvent>>,
    #[prop(optional, into)] style: Signal<Css>,
    #[prop(optional)] attrs: Attrs,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let row = layout == TileLayout::Row;
    let mut base = css! { "padding": "10px 12px" };
    base.set_opt("width", row.then_some("100%"));
    base.extend(&css! { "text-align": "start", "cursor": "pointer", "font": "inherit" });
    let s = Signal::derive(move || style.with(|st| base.clone().merge(st)));
    let mut title = Attrs::new();
    if let Some(t) = meta_title {
        title.set("title", t);
    }
    let figure = move || {
        meta.get()
            .unwrap_or_else(|| format!("{}%", js_number(percent.get())))
    };
    crate::ds::Surface(crate::ds::SurfaceProps {
        tag: Some("button".into()),
        tone: Some(Tone::Accent),
        fill: Signal::from(if row { Fill::None } else { Fill::App }),
        border: Some(if row { Edge::None } else { Edge::Line }.into()),
        radius: Some(Corner::Md.into()),
        interactive: Some(if row {
            Interactive::Row
        } else {
            Interactive::Choice
        }),
        selected: active,
        disabled,
        on_click,
        on_pointer_down_capture,
        style: s,
        attrs,
        children: Some(Box::new(move || {
            view! {
                <Stack
                    direction=Direction::Row
                    gap=6
                    justify=Justify::Between
                    align=Align::Baseline
                    style=css! { "margin-bottom": if row { 7 } else { 8 } }
                >
                    <Text role=TextRole::Ui truncate=true style=css! { "font-size": if row { "var(--fs-ui-md)" } else { "var(--fs-ui)" } }>
                        {move || name.get()}
                    </Text>
                    <Text role=TextRole::LabelNum attrs=title>{figure}</Text>
                </Stack>
                <Progress value=Signal::derive(move || Some(percent.get())) height=if row { 5.0 } else { 4.0 } />
                {children.map(|c| c())}
            }
            .into_any()
        })),
        ..Default::default()
    })
}

/// Callout.jsx: an inline notice explaining a consequence or a state. The inner Text is a
/// `display: contents` div, so a layout the caller puts on the callout reaches the children.
#[component]
pub fn Callout(
    #[prop(optional)] tone: CalloutTone,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let (t, fill, border) = match tone {
        CalloutTone::Warn => (Some(Tone::Warn), Fill::Soft, Some(Edge::Tone)),
        CalloutTone::Kindle => (Some(Tone::Warn), Fill::Soft, None),
        CalloutTone::Success => (Some(Tone::Valid), Fill::Soft, None),
        CalloutTone::Info => (None, Fill::App, Some(Edge::Line)),
    };
    crate::ds::Surface(crate::ds::SurfaceProps {
        tone: t,
        fill: Signal::from(fill),
        border: border.map(Border::from),
        radius: Some(Corner::Lg.into()),
        pad: Some(Inset::Sm.into()),
        style: style.into(),
        attrs,
        children: Some(Box::new(move || {
            view! {
                <Text
                    tag="div"
                    role=TextRole::Caption
                    tone=if t.is_some() { TextTone::Tone } else { TextTone::Muted }
                    style=css! { "display": "contents" }
                >
                    {children.map(|c| c())}
                </Text>
            }
            .into_any()
        })),
        ..Default::default()
    })
}

/// Card.jsx: a white surface container on the paper background. `interactive` lifts on hover;
/// Surface derives the button role and keys from `on_click`.
#[component]
pub fn Card(
    #[prop(optional)] variant: CardVariant,
    #[prop(optional)] interactive: bool,
    /// `lg` unless given.
    #[prop(optional, into)]
    padding: Option<Pad>,
    #[prop(optional)] on_click: Option<Callback<web_sys::Event>>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let (tone, fill, border, radius, elevation) = match variant {
        CardVariant::Default => (
            None,
            Fill::Card,
            Some(Edge::Line),
            Corner::X2l,
            Some(Shadow::Card),
        ),
        CardVariant::Flat => (None, Fill::Card, Some(Edge::Line), Corner::Lg, None),
        CardVariant::Muted => (None, Fill::App, Some(Edge::Line), Corner::Lg, None),
        CardVariant::Ocean => (
            Some(Tone::Ocean),
            Fill::Solid,
            None,
            Corner::Xl,
            Some(Shadow::Hero),
        ),
        CardVariant::Paper => (None, Fill::Paper, None, Corner::Xs, Some(Shadow::Page)),
    };
    crate::ds::Surface(crate::ds::SurfaceProps {
        tone,
        fill: Signal::from(fill),
        border: border.map(Border::from),
        radius: Some(Radius::Corner(radius)),
        elevation: elevation.map(Elevation::from),
        pad: Some(padding.unwrap_or(Pad::Inset(Inset::Lg))),
        interactive: interactive.then_some(Interactive::Card),
        on_click,
        style: style.into(),
        attrs,
        children,
        ..Default::default()
    })
}

/// What Modal and Drawer share: a dialog Layer with a scrim that traps the focus, locks the
/// page's scroll and, while `closable`, closes on Escape or a press on the scrim.
fn dialog_layer(
    open: Option<Signal<bool>>,
    on_close: Option<Callback<()>>,
    closable: Signal<bool>,
    children: ChildrenFn,
) -> LayerProps {
    let dismiss = on_close.map(|cb| Callback::new(move |_| cb.run(())));
    LayerProps {
        open: Some(open.unwrap_or_else(|| Signal::from(true))),
        trap_focus: true,
        lock_scroll: true,
        on_dismiss: Signal::derive(move || dismiss.filter(|_| closable.get())),
        role: Some("dialog".into()),
        ..LayerProps::new(children)
    }
}

/// The round ✕ at the top of a dialog or a drawer.
fn dialog_close(label: Signal<String>, on_close: Callback<()>) -> impl IntoView {
    close_button(
        label,
        Callback::new(move |_: MouseEvent| on_close.run(())),
        Weight::Soft,
        Shape::Square,
        css! { "border-radius": "var(--radius-pill)" },
    )
}

/// Drawer.jsx: an end-edge slide-over panel for reference content. Extra attributes land on
/// the scrim.
#[component]
pub fn Drawer(
    /// Open unless given: the app mounts a drawer only while it is open.
    #[prop(optional, into)]
    open: Option<Signal<bool>>,
    #[prop(optional, into)] eyebrow: Option<Content>,
    /// Names the dialog when it is text; a dialog with an element title is "Panel".
    #[prop(optional, into)]
    title: Option<Content>,
    /// The direction of a title that is content, not interface text.
    #[prop(optional, into)]
    title_dir: Option<String>,
    #[prop(optional, into)] width: Option<CssValue>,
    #[prop(optional)] on_close: Option<Callback<()>>,
    /// The ✕'s title, which follows the shown language while the drawer is on screen.
    #[prop(optional, into)]
    close_label: Option<Signal<String>>,
    /// Extra attributes of the scrim.
    #[prop(optional)]
    attrs: Attrs,
    #[prop(optional)] children: Option<ChildrenFn>,
) -> impl IntoView {
    let close_label = close_label.unwrap_or_else(|| Signal::from("Close"));
    let label = Some(Signal::from(
        title
            .as_ref()
            .and_then(Content::as_text)
            .unwrap_or("Panel")
            .to_string(),
    ));
    let content: ChildrenFn = Arc::new(move || {
        let mut s = Css::new();
        s.set(
            "width",
            width.clone().unwrap_or_else(|| "var(--drawer-w)".into()),
        );
        s.extend(&css! { "max-width": "100%", "height": "100%", "display": "flex", "flex-direction": "column" });
        let (eyebrow, title, title_dir) = (eyebrow.clone(), title.clone(), title_dir.clone());
        let children = children.clone();
        let mut title_attrs = Attrs::new();
        if let Some(d) = title_dir {
            title_attrs.set("dir", d);
        }
        title_attrs.set("data-testid", "drawer-title");
        view! {
            <Surface fill=Fill::Card radius=Corner::None elevation=Shadow::Drawer style=s>
                <Stack
                    direction=Direction::Row
                    gap=12
                    justify=Justify::Between
                    align=Align::Start
                    style=css! { "padding": "20px 24px", "border-bottom": "var(--stroke-hair) solid var(--border-hair)" }
                >
                    <Stack direction=Direction::Column gap=6>
                        {present(eyebrow).map(|e| view! { <Text role=TextRole::Overline tone=TextTone::Accent>{e.view()}</Text> })}
                        <Text role=TextRole::H2 attrs=title_attrs>{title.map(|t| t.view())}</Text>
                    </Stack>
                    {on_close.map(|cb| dialog_close(close_label, cb))}
                </Stack>
                <Stack direction=Direction::Column gap=14 style=css! { "padding": 24, "flex": 1, "overflow": "auto" }>
                    {children.map(|c| c())}
                </Stack>
            </Surface>
        }
        .into_any()
    });
    Layer(LayerProps {
        scrim: Some(Scrim::Token(ScrimToken::Drawer)),
        placement: LayerPlacement::End,
        label,
        scrim_attrs: attrs,
        ..dialog_layer(open, on_close, Signal::from(true), content)
    })
}

/// HelpCard.jsx: a helps-panel card carrying one translation note or key word. A key word is
/// named by its title and carries no verse label; a note with no quoted phrase prints no bare
/// quotes; the body is a div because it carries rendered markdown blocks.
#[component]
pub fn HelpCard(
    #[prop(optional)] kind: HelpKind,
    #[prop(optional, into)] verse: Option<String>,
    #[prop(optional, into)] title: Option<String>,
    #[prop(optional)] body: Option<Slot>,
    #[prop(optional, into)] active: Signal<bool>,
    #[prop(optional, into)] action_label: Option<String>,
    #[prop(optional)] on_action: Option<Callback<()>>,
    #[prop(optional)] on_click: Option<Callback<web_sys::Event>>,
    #[prop(optional)] on_mouse_enter: Option<Callback<MouseEvent>>,
    #[prop(optional)] on_mouse_leave: Option<Callback<MouseEvent>>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let word = kind == HelpKind::Word;
    let action_label = action_label.unwrap_or_else(|| "Translation Academy →".into());
    crate::ds::Surface(crate::ds::SurfaceProps {
        tone: Some(if word { Tone::Cultivate } else { Tone::Accent }),
        fill: Signal::from(Fill::Card),
        border: Some(Edge::Line.into()),
        radius: Some(Corner::Lg.into()),
        pad: Some(Inset::Md.into()),
        interactive: Some(Interactive::Choice),
        selected: active,
        on_click,
        on_mouse_enter,
        on_mouse_leave,
        style: style.into(),
        attrs,
        children: Some(Box::new(move || {
            let badge = css! { "display": "inline-flex", "padding": "2px 7px" };
            let title = title.filter(|t| !t.is_empty()).map(|t| {
                let text = if word { t } else { format!("\u{201C}{t}\u{201D}") };
                view! { <Text role=TextRole::Quote tag="p" style=css! { "margin-bottom": 5 }>{text}</Text> }
            });
            let action = on_action.map(|cb| {
                crate::ds::Action(crate::ds::ActionProps {
                    weight: Weight::Text,
                    size: crate::ds::Size::Sm,
                    on_click: Some(Callback::new(move |e: MouseEvent| {
                        e.stop_propagation();
                        cb.run(());
                    })),
                    style: Signal::from(css! { "margin-top": 9 }),
                    children: Some(Box::new(move || action_label.into_any())),
                    ..Default::default()
                })
            });
            view! {
                <Stack direction=Direction::Row gap=7 align=Align::Center style=css! { "margin-bottom": 6 }>
                    <Surface fill=Fill::Solid radius=Corner::Pill style=badge>
                        <Text role=TextRole::Label style=css! { "color": "var(--tone-on-fill)" }>{if word { "Key word" } else { "Note" }}</Text>
                    </Surface>
                    {verse.filter(|_| !word).map(|v| view! { <Text role=TextRole::LabelNum>"v"{v}</Text> })}
                </Stack>
                {title}
                <Text role=TextRole::Body tag="div" tone=TextTone::Muted style=css! { "font-size": "var(--fs-ui-sm)" }>
                    {body.map(|b| b())}
                </Text>
                {action}
            }
            .into_any()
        })),
        ..Default::default()
    })
}

/// Modal.jsx: a centred dialog over an Ocean scrim. Extra attributes land on the scrim; the
/// old `zIndex` is gone (Layer stacks by nesting depth).
#[component]
pub fn Modal(
    /// Open unless given: the app mounts a modal only while it is open.
    #[prop(optional, into)]
    open: Option<Signal<bool>>,
    #[prop(optional, into)] title: Option<Content>,
    #[prop(optional, into)] subtitle: Option<Content>,
    #[prop(optional, into)] width: Option<CssValue>,
    /// The ✕'s title, which also names it. It follows the shown language while the dialog is
    /// on screen: the App language dialog previews a language under its own ✕.
    #[prop(optional, into)]
    close_label: Option<Signal<String>>,
    #[prop(optional)] on_close: Option<Callback<()>>,
    /// Whether `on_close` is in force: the ✕, Escape and a press on the scrim. tC4's dialogs
    /// that run an action pass no `onClose` while it runs (`onClose={busy ? undefined :
    /// close}`, Open from Door43 and GatewayChange), so the dialog cannot close under it.
    #[prop(into, default = Signal::from(true))]
    closable: Signal<bool>,
    #[prop(optional)] footer: Option<ChildrenFn>,
    /// Whether the footer row is drawn; always when not given. tC4 passes `footer={null}` on a
    /// step with no buttons (Add a book's first step), and the dialog then has no footer row.
    #[prop(optional, into)]
    footer_shown: Option<Signal<bool>>,
    /// Extra attributes of the scrim.
    #[prop(optional)]
    attrs: Attrs,
    /// Attributes of the scrim whose values follow signals (tC4's App language dialog sets
    /// `data-saving` while its save runs); `None` removes one.
    #[prop(optional)]
    live_attrs: Vec<(&'static str, Signal<Option<String>>)>,
    #[prop(optional)] children: Option<ChildrenFn>,
) -> impl IntoView {
    let id = crate::ds::next_id("tc-dlg");
    let close_label = close_label.unwrap_or_else(|| Signal::from("Close"));
    let labelled_by = Some(id.clone());
    let content: ChildrenFn = Arc::new(move || {
        let mut s = Css::new();
        s.set(
            "width",
            width.clone().unwrap_or_else(|| "var(--modal-w)".into()),
        );
        s.extend(&css! { "max-width": "100%", "max-height": "82vh", "overflow": "auto" });
        let (title, subtitle) = (title.clone(), subtitle.clone());
        let (children, footer) = (children.clone(), footer.clone());
        let id = id.clone();
        view! {
            <Surface fill=Fill::Card radius=Corner::X2l elevation=Shadow::Modal style=s>
                <Stack direction=Direction::Row gap=12 align=Align::Start style=css! { "padding": "22px 24px 0" }>
                    <Stack direction=Direction::Column gap=4 grow=true>
                        <Text attrs=Attrs::from([("id", id.as_str())]) role=TextRole::H3>{title.map(|t| t.view())}</Text>
                        {present(subtitle).map(|s| view! { <Text role=TextRole::Caption>{s.view()}</Text> })}
                    </Stack>
                    {move || on_close.filter(|_| closable.get()).map(|cb| dialog_close(close_label, cb))}
                </Stack>
                <Stack direction=Direction::Column gap=16 style=css! { "padding": "18px 24px 4px" }>
                    {children.map(|c| c())}
                </Stack>
                {footer.map(|f| move || {
                    let f = f.clone();
                    footer_shown.is_none_or(|shown| shown.get()).then(move || view! {
                        <Stack direction=Direction::Row gap=10 justify=Justify::End style=css! { "padding": "18px 24px 22px" }>
                            {f()}
                        </Stack>
                    })
                })}
            </Surface>
        }
        .into_any()
    });
    Layer(LayerProps {
        scrim: Some(Scrim::Token(ScrimToken::Modal)),
        placement: LayerPlacement::Center,
        labelled_by,
        scrim_attrs: attrs,
        scrim_live: live_attrs,
        ..dialog_layer(open, on_close, closable, content)
    })
}
