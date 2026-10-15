//! tC4's `components/data`: Accordion, Avatar, KeyValueGrid.

use std::collections::HashSet;

use leptos::prelude::*;

use crate::ds::{
    Action, Attrs, AvatarShape, AvatarTone, Columns, Css, Fill, Rule, Stack, Surface, Text,
    TextRole, TextTone, Tone, Weight,
};
use crate::tokens::{Corner, Edge};

/// One section of an Accordion.
#[derive(Debug, Clone, PartialEq)]
pub struct AccordionItem {
    pub key: String,
    pub title: String,
    pub meta: Option<String>,
    pub content: String,
}

/// Accordion.jsx: a disclosure list for optional or reference detail.
#[component]
pub fn Accordion(
    #[prop(optional)] items: Vec<AccordionItem>,
    /// The keys open at first.
    #[prop(optional)]
    default_open: Vec<String>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let open = RwSignal::new(default_open.into_iter().collect::<HashSet<_>>());
    let outer = css! { "overflow": "hidden" }.merge(&style);
    let rows = items
        .into_iter()
        .enumerate()
        .map(|(i, it)| {
            let key = it.key.clone();
            let on = {
                let key = key.clone();
                Signal::derive(move || open.with(|o| o.contains(&key)))
            };
            let toggle = {
                let key = key.clone();
                Callback::new(move |_| {
                    open.update(|o| {
                        if !o.remove(&key) {
                            o.insert(key.clone());
                        }
                    })
                })
            };
            let row_style = Signal::derive(move || {
                css! {
                    "padding": "12px 14px",
                    "border-radius": 0,
                    "background": if on.get() { "var(--surface-app)" } else { "transparent" },
                }
            });
            let attrs = Attrs::new().with("aria-controls", format!("{key}-p"));
            let title = it.title.clone();
            let meta = it.meta.clone().filter(|m| !m.is_empty());
            let content = it.content.clone();
            let panel_id = format!("{key}-p");
            view! {
                <div>
                    {(i > 0).then(|| view! { <Rule /> })}
                    <Action
                        weight=Weight::Row
                        attrs=attrs
                        attr:aria-expanded=move || if on.get() { "true" } else { "false" }
                        on_click=toggle
                        style=row_style
                    >
                        <Text role=TextRole::Strong style=css! { "flex": 1 }>{title}</Text>
                        {meta.map(|m| view! { <Text role=TextRole::LabelNum>{m}</Text> })}
                        {move || {
                            let s = css! {
                                "transform": if on.get() { "rotate(90deg)" } else { "none" },
                                "transition": "transform var(--dur-hover) var(--ease-standard)",
                            };
                            view! { <Text role=TextRole::Caption attrs=Attrs::from([("aria-hidden", "true")]) style=s>"›"</Text> }
                        }}
                    </Action>
                    {move || {
                        on.get().then(|| {
                            let content = content.clone();
                            view! {
                                <Stack attrs=Attrs::from([("id", panel_id.as_str())]) style=css! { "padding": "2px 14px 14px" }>
                                    <Text role=TextRole::Body tone=TextTone::Muted style=css! { "font-size": "var(--fs-ui-sm)" }>
                                        {content}
                                    </Text>
                                </Stack>
                            }
                        })
                    }}
                </div>
            }
        })
        .collect_view();
    view! {
        <Surface fill=Fill::Card border=Edge::Line radius=Corner::Lg style=outer attrs=attrs>
            {rows}
        </Surface>
    }
}

/// Avatar.jsx: an initials tile identifying a Bible, a book or a person. Its corner follows the
/// old rule: small at 32 px and under, large above, a pill when round.
#[component]
pub fn Avatar(
    #[prop(optional, into)] initials: String,
    #[prop(default = 44.0)] size: f64,
    #[prop(optional)] tone: AvatarTone,
    #[prop(optional)] shape: AvatarShape,
    #[prop(optional, into)] src: Option<String>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let radius = if shape == AvatarShape::Circle {
        Corner::Pill
    } else if size <= 32.0 {
        Corner::Sm
    } else {
        Corner::Lg
    };
    let tone = match tone {
        AvatarTone::Ocean => Tone::Ocean,
        AvatarTone::Accent => Tone::Accent,
        AvatarTone::Cultivate => Tone::Cultivate,
        AvatarTone::Muted => Tone::Neutral,
    };
    let outer = css! {
        "width": size,
        "height": size,
        "flex": "none",
        "display": "inline-flex",
        "align-items": "center",
        "justify-content": "center",
        "overflow": "hidden",
    }
    .merge(&style);
    let inner = match src {
        Some(src) => {
            view! { <img src=src alt="" style="width: 100%; height: 100%; object-fit: cover;" /> }
                .into_any()
        }
        None => {
            let s = css! { "color": "var(--tone-on-fill)", "font-size": ((size * 0.386) + 0.5).floor() };
            view! { <Text role=TextRole::Strong style=s>{initials}</Text> }.into_any()
        }
    };
    view! {
        <Surface tag="span" tone=tone fill=Fill::Solid radius=radius style=outer attrs=attrs>
            {inner}
        </Surface>
    }
}

/// One fact of a KeyValueGrid.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyValue {
    pub k: String,
    pub v: String,
}

/// KeyValueGrid.jsx: a grid of small labelled facts.
#[component]
pub fn KeyValueGrid(
    #[prop(optional)] items: Vec<KeyValue>,
    #[prop(default = 2)] columns: u32,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let cells = items
        .into_iter()
        .map(|it| {
            view! {
                <Surface border=Edge::Line radius=Corner::Md pad="10px 12px">
                    <Text role=TextRole::Label tag="div" style=css! { "margin-bottom": 3 }>{it.k}</Text>
                    <Text role=TextRole::Strong style=css! { "font-size": "var(--fs-ui-sm)" }>{it.v}</Text>
                </Surface>
            }
        })
        .collect_view();
    view! {
        <Stack columns=Columns::Count(columns) gap=10 style=style attrs=attrs>
            {cells}
        </Stack>
    }
}
