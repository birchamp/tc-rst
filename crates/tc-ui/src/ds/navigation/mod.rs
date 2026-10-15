//! tC4's `components/navigation`: AppHeader, Breadcrumb, Menu.

use std::sync::Arc;

use leptos::ev::MouseEvent;
use leptos::prelude::*;

use crate::ds::{
    Anchor, Attrs, Css, Direction, Dismiss, Fill, HeaderTone, Interactive, Justify, LayerAlign,
    LayerPlacement, Level, Navigate, Orientation, Side, Slot, Stack, Status, StatusDot, Surface,
    Text, TextRole, TextTone, Tone, Weight,
};
use crate::tokens::{Align, Corner, Edge, Shadow};

/// AppHeader.jsx: the 56 px application title bar: brand, project chip, mode switch, save
/// state. The two side columns are equal, so the switch stays in the centre of the bar; a
/// project name that does not fit is cut with an ellipsis, and the chip's tooltip gives it in
/// full.
#[component]
pub fn AppHeader(
    #[prop(optional)] tone: HeaderTone,
    #[prop(optional, into)] logo_src: Option<String>,
    #[prop(optional, into)] project_initials: Option<String>,
    #[prop(optional, into)] project_name: Option<String>,
    /// The direction of the project's own name.
    #[prop(optional, into)]
    project_name_dir: Option<String>,
    #[prop(optional, into)] project_meta: Option<String>,
    #[prop(optional, into)] switch_title: Option<String>,
    #[prop(optional)] center: Option<Slot>,
    /// The end of the bar; the save state "Saved" when absent.
    #[prop(optional)]
    right: Option<Slot>,
    #[prop(optional)] on_brand_click: Option<Callback<MouseEvent>>,
    #[prop(optional)] on_project_click: Option<Callback<web_sys::Event>>,
) -> impl IntoView {
    let dark = tone == HeaderTone::Ocean;
    let switch_title = switch_title.unwrap_or_else(|| "Switch project".into());
    let outer = css! {
        "display": "grid",
        "grid-template-columns": "minmax(0,1fr) auto minmax(0,1fr)",
        "align-items": "center",
        "gap": 14,
        "flex": "none",
        "height": "var(--header-height)",
        "z-index": "var(--z-header)",
    };
    let brand_style = css! { "flex": "none", "cursor": on_brand_click.map(|_| "pointer") };
    let logo = logo_src.filter(|s| !s.is_empty()).map(|src| {
        view! { <img src=src alt="translationCore" style="height: 28px; width: auto; display: block;" /> }
    });
    let wordmark =
        css! { "font-size": "var(--fs-wordmark)", "letter-spacing": "var(--track-15-5)" };
    let reg = css! { "vertical-align": "super", "margin-inline-start": 1, "letter-spacing": "var(--track-9)" };
    let chip = project_name.filter(|n| !n.is_empty()).map(|name| {
        let title = format!("{name}. {switch_title}");
        let initials = project_initials.clone().unwrap_or_default();
        let meta = project_meta.clone();
        let dir = project_name_dir.clone();
        let tile = css! {
            "width": 26,
            "height": 26,
            "flex": "none",
            "display": "inline-flex",
            "align-items": "center",
            "justify-content": "center",
        };
        let meta_style = css! { "text-transform": "none", "letter-spacing": "var(--track-11)" };
        crate::ds::Surface(crate::ds::SurfaceProps {
            tag: Some("button".into()),
            tone: None,
            fill: Signal::from(Fill::Quiet),
            border: Some(Edge::Line.into()),
            radius: Some(Corner::Md.into()),
            elevation: None,
            pad: None,
            interactive: Some(Interactive::Quiet),
            selected: Signal::from(false),
            disabled: false,
            on: None,
            on_click: on_project_click,
            style: css! { "padding": "5px 10px 5px 8px", "cursor": "pointer", "font": "inherit", "min-width": 0 },
            attrs: Attrs::new().with("title", title).with("data-testid", "project-switch"),
            children: Some(Box::new(move || {
                let name_text = crate::ds::Text(crate::ds::TextProps {
                    role: TextRole::Ui,
                    tag: None,
                    tone: TextTone::Strong,
                    script: None,
                    align: None,
                    truncate: true,
                    trim: None,
                    measure: None,
                    dir,
                    style: Css::new(),
                    attrs: Attrs::new(),
                    children: Some(Box::new(move || name.into_any())),
                });
                view! {
                    <Stack direction=Direction::Row gap=9 align=Align::Center>
                        <Surface tag="span" tone=if dark { Tone::Accent } else { Tone::Ocean } fill=Fill::Solid radius=Corner::Xs style=tile>
                            <Text role=TextRole::CaptionStrong style=css! { "color": "var(--tone-on-fill)" }>{initials}</Text>
                        </Surface>
                        <Stack direction=Direction::Column gap=1 style=css! { "text-align": "start" }>
                            {name_text}
                            <Text role=TextRole::Overline tone=TextTone::Muted truncate=true style=meta_style>{meta}</Text>
                        </Stack>
                    </Stack>
                }
                .into_any()
            })),
        })
    });
    let right = match right {
        Some(r) => r(),
        None => view! { <StatusDot status=Status::Valid size=8.0 label="Saved" /> }.into_any(),
    };

    crate::ds::Surface(crate::ds::SurfaceProps {
        tag: Some("header".into()),
        tone: dark.then_some(Tone::Ocean),
        fill: Signal::from(if dark { Fill::Solid } else { Fill::Card }),
        border: Some(if dark { Edge::None } else { Edge::Hair }.into()),
        radius: Some(Corner::None.into()),
        elevation: None,
        pad: Some("0 16px".into()),
        interactive: None,
        selected: Signal::from(false),
        disabled: false,
        on: None,
        on_click: None,
        style: outer,
        attrs: Attrs::new(),
        children: Some(Box::new(move || {
            let brand = crate::ds::Stack(crate::ds::StackProps {
                direction: Direction::Row,
                gap: Some(9.into()),
                align: Some(Align::Center),
                justify: None,
                wrap: false,
                inline: false,
                columns: None,
                flex: None,
                grow: false,
                tag: None,
                on_click: on_brand_click,
                style: brand_style,
                attrs: Attrs::new(),
                children: Some(Box::new(move || {
                    view! {
                        {logo}
                        <Text role=TextRole::Strong style=wordmark>
                            "translationCore"
                            <Text tag="span" role=TextRole::LabelMicro tone=TextTone::Muted style=reg>"®"</Text>
                        </Text>
                    }
                    .into_any()
                })),
            });
            view! {
                <Stack direction=Direction::Row gap=14 align=Align::Center style=css! { "align-self": "stretch" }>
                    {brand}
                    <crate::ds::Rule orientation=Orientation::Vertical />
                    {chip}
                </Stack>
                <span style="display: flex;">{center.map(|c| c())}</span>
                <Stack direction=Direction::Row align=Align::Center justify=Justify::End>
                    {right}
                </Stack>
            }
            .into_any()
        })),
    })
}

/// One step of a Breadcrumb. A step with `on_click` is a link; the last is the current page.
#[derive(Clone)]
pub struct Crumb {
    pub label: String,
    pub on_click: Option<Callback<MouseEvent>>,
}

/// Breadcrumb.jsx: the ancestry trail Bible · Book · Chapter · Tool.
#[component]
pub fn Breadcrumb(
    #[prop(optional)] items: Vec<Crumb>,
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let mut a = Attrs::new().with("aria-label", label.unwrap_or_else(|| "Breadcrumb".into()));
    a.extend(&attrs);
    let steps = move || {
        items
            .into_iter()
            .enumerate()
            .map(|(i, it)| {
                let sep = (i > 0).then(|| {
                    view! { <Text role=TextRole::Caption tone=TextTone::Faint attrs=Attrs::from([("aria-hidden", "true")])>"·"</Text> }
                });
                let step = match it.on_click {
                    Some(cb) => {
                        let label = it.label;
                        view! { <crate::ds::Action weight=Weight::Text on_click=cb>{label}</crate::ds::Action> }.into_any()
                    }
                    None => view! {
                        <Text role=TextRole::CaptionStrong attrs=Attrs::from([("aria-current", "page")])>{it.label}</Text>
                    }
                    .into_any(),
                };
                view! { {sep} {step} }
            })
            .collect_view()
    };
    crate::ds::Stack(crate::ds::StackProps {
        direction: Direction::Row,
        gap: Some(7.into()),
        align: Some(Align::Center),
        justify: None,
        wrap: true,
        inline: false,
        columns: None,
        flex: None,
        grow: false,
        tag: Some("nav".into()),
        on_click: None,
        style,
        attrs: a,
        children: Some(Box::new(move || steps().into_any())),
    })
}

/// One row of a Menu.
#[derive(Clone, Default)]
pub struct MenuItem {
    pub label: String,
    pub meta: Option<String>,
    pub on_click: Option<Callback<()>>,
    pub disabled: bool,
    pub destructive: bool,
    /// A rule between groups of rows; the other fields are ignored.
    pub divider: bool,
}

/// Menu.jsx: an anchored dropdown of actions. It answers arrow keys, Home/End and typeahead,
/// which role="menu" requires.
#[component]
pub fn Menu(
    trigger: Slot,
    #[prop(optional)] items: Vec<MenuItem>,
    #[prop(optional)] align: LayerAlign,
    /// Controlled when given; otherwise the menu keeps its own state.
    #[prop(optional, into)]
    open: Option<Signal<bool>>,
    #[prop(optional)] on_open_change: Option<Callback<bool>>,
    #[prop(optional)] style: Css,
) -> impl IntoView {
    let anchor = NodeRef::<leptos::html::Span>::new();
    let state = RwSignal::new(false);
    let is_open = Signal::derive(move || open.map_or_else(|| state.get(), |o| o.get()));
    let set = move |v: bool| {
        state.set(v);
        if let Some(cb) = on_open_change {
            cb.run(v);
        }
    };
    let items = StoredValue::new(items);
    let panel_style = css! { "min-width": 200 }.merge(&style);
    let content: ChildrenFn = Arc::new(move || {
        let rows = items.with_value(|items| {
            items
                .iter()
                .map(|it| {
                    if it.divider {
                        return view! { <crate::ds::Rule style=css! { "margin": "5px 4px" } /> }.into_any();
                    }
                    let on_click = it.on_click;
                    let meta = it.meta.clone().filter(|m| !m.is_empty());
                    let label = it.label.clone();
                    crate::ds::Action(crate::ds::ActionProps {
                        weight: Weight::Row,
                        tone: it.destructive.then_some(Tone::Invalid),
                        size: Default::default(),
                        shape: Default::default(),
                        icon_only: false,
                        title: None,
                        disabled: Signal::from(it.disabled),
                        tag: None,
                        href: None,
                        on_click: Some(Callback::new(move |_| {
                            set(false);
                            if let Some(cb) = on_click {
                                cb.run(());
                            }
                        })),
                        style: Signal::from(Css::new()),
                        attrs: Attrs::from([("role", "menuitem")]),
                        children: Some(Box::new(move || {
                            view! {
                                <span style=css! { "flex": 1 }.to_style()>{label}</span>
                                {meta.map(|m| view! { <Text role=TextRole::Meta tone=TextTone::Faint>{m}</Text> })}
                            }
                            .into_any()
                        })),
                    })
                    .into_any()
                })
                .collect_view()
        });
        view! {
            <Surface fill=Fill::Card border=Edge::Line radius=Corner::Lg elevation=Shadow::Hover pad=6.0 style=panel_style.clone()>
                {rows}
            </Surface>
        }
        .into_any()
    });
    let anchor_to: Anchor = Callback::new(move |_| anchor.get_untracked().map(Into::into));
    view! {
        <span node_ref=anchor style="display: inline-flex;" on:click=move |_| set(!is_open.get_untracked())>
            {trigger()}
        </span>
        {crate::ds::Layer(crate::ds::LayerProps {
            open: Some(is_open),
            level: Level::Popover,
            scrim: None,
            placement: LayerPlacement::Anchor,
            anchor_to: Some(anchor_to),
            offset: 6.0,
            align,
            side: Side::Bottom,
            dismiss: Dismiss { scrim: false, escape: true, outside: true },
            trap_focus: false,
            lock_scroll: false,
            restore_focus: true,
            on_dismiss: Some(Callback::new(move |_| set(false))),
            navigate: Some(Navigate::Vertical),
            animate: true,
            scrim_attrs: Attrs::new(),
            role: Some("menu".into()),
            label: None,
            labelled_by: None,
            on_mouse_down: None,
            style: Css::new(),
            attrs: Attrs::new(),
            children: content,
        })}
    }
}
