//! tC4's `components/forms`: Checkbox, DropZone, FilterChip, OptionCard, Radio, RadioGroup,
//! SearchField, Select, SuggestField, TextArea, TextField, Toggle.

mod select;
mod suggest;

pub use select::*;
pub use suggest::*;

use std::sync::Arc;

use leptos::ev::{FocusEvent, KeyboardEvent, MouseEvent};
use leptos::prelude::*;

use crate::ds::{
    Attrs, Badge, BadgeSize, BadgeTone, Css, Direction, Fill, InputVariant, Interactive, Mark,
    NativeInput, OptionControl, Placement, Size, Stack, Surface, Text, TextRole, TextTone, Tone,
    Weight, fixed,
};
use crate::tokens::{Align, Corner, Edge};

/// A Choice for the shims, with its change handler forwarded as is.
#[allow(clippy::too_many_arguments)]
fn choice(
    mark: Mark,
    checked: Signal<bool>,
    indeterminate: Signal<bool>,
    disabled: Option<bool>,
    name: Option<String>,
    value: Option<String>,
    on_change: Option<Callback<web_sys::Event>>,
) -> impl IntoView {
    crate::ds::Choice(crate::ds::ChoiceProps {
        mark,
        size: Size::Md,
        checked,
        indeterminate,
        disabled: MaybeProp::from(disabled),
        tone: Tone::Accent,
        value,
        name,
        on_change,
        style: Css::new(),
        attrs: Attrs::new(),
    })
}

/// A Field with the label beside the control.
#[allow(clippy::too_many_arguments)]
fn labelled(
    label: String,
    hint: Option<String>,
    disabled: Option<bool>,
    placement: Placement,
    id: Option<String>,
    style: Css,
    attrs: Attrs,
    control: impl FnOnce() -> AnyView + Send + 'static,
) -> impl IntoView {
    crate::ds::Field(crate::ds::FieldProps {
        label: Some(label),
        hint,
        error: None,
        required: false,
        optional: false,
        disabled,
        size: Size::Md,
        placement,
        id,
        name: None,
        style,
        attrs,
        children: Some(Box::new(control)),
    })
}

/// Checkbox.jsx: a square check for independent options inside a form.
#[component]
pub fn Checkbox(
    #[prop(optional, into)] checked: Signal<bool>,
    #[prop(optional, into)] indeterminate: Signal<bool>,
    #[prop(optional)] disabled: Option<bool>,
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional, into)] description: Option<String>,
    #[prop(optional)] on_change: Option<Callback<web_sys::Event>>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let mark = move || {
        choice(
            Mark::Check,
            checked,
            indeterminate,
            disabled,
            None,
            None,
            on_change,
        )
        .into_any()
    };
    match label.filter(|l| !l.is_empty()) {
        None => bare(mark(), style, attrs).into_any(),
        Some(label) => labelled(
            label,
            description,
            disabled,
            Placement::End,
            None,
            style,
            attrs,
            mark,
        )
        .into_any(),
    }
}

/// A mark with no label, in a span that takes the caller's style and attributes.
fn bare(mark: impl IntoView + 'static, style: Css, attrs: Attrs) -> impl IntoView {
    view! { <span style=style.to_style() {..crate::ds::spread(&attrs)}>{mark}</span> }
}

/// DropZone.jsx: a dashed file-drop target for imports. The caller supplies the drag and drop
/// handlers.
#[component]
pub fn DropZone(
    #[prop(optional, into)] title: Option<String>,
    #[prop(optional, into)] hint: Option<String>,
    #[prop(optional)] on_click: Option<Callback<web_sys::Event>>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let title = title.unwrap_or_else(|| "Drop your file here, or browse".into());
    let s = css! {
        "width": "100%",
        "padding": "34px 20px",
        "background": "var(--tc-dropzone-bg)",
        "cursor": "pointer",
        "font": "inherit",
    }
    .merge(&style);
    let hint = hint.filter(|h| !h.is_empty());
    crate::ds::Surface(crate::ds::SurfaceProps {
        tag: Some("button".into()),
        tone: Some(Tone::Accent),
        fill: Signal::from(Fill::None),
        border: Some(Edge::DashedTone.into()),
        radius: Some(Corner::Xl.into()),
        elevation: None,
        pad: None,
        interactive: Some(Interactive::Choice),
        selected: Signal::from(false),
        disabled: false,
        on: None,
        on_click,
        style: s,
        attrs,
        children: Some(Box::new(move || {
            view! {
                <Stack direction=Direction::Column gap=8 align=Align::Center>
                    {upload_arrow()}
                    <Text role=TextRole::TitleSm>{title}</Text>
                    {hint.map(|h| view! { <Text role=TextRole::Caption>{h}</Text> })}
                </Stack>
            }
            .into_any()
        })),
    })
}

/// FilterChip.jsx: a bordered pill toggle for multi-select and filtering. A selected Ocean chip
/// is solid.
#[component]
pub fn FilterChip(
    #[prop(optional, into)] selected: Signal<bool>,
    #[prop(optional, into)] mark: Option<String>,
    #[prop(optional)] tone: Tone,
    #[prop(optional)] on_click: Option<Callback<web_sys::Event>>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
    /// Redrawn when the selection changes the label's colour.
    #[prop(optional)]
    children: Option<ChildrenFn>,
) -> impl IntoView {
    let solid = Signal::derive(move || tone == Tone::Ocean && selected.get());
    let s = css! { "padding": "7px 14px", "cursor": "pointer", "font": "inherit" }.merge(&style);
    let prefix = mark
        .filter(|m| !m.is_empty())
        .map(|m| format!("{m} "))
        .unwrap_or_default();
    let text = move || {
        let solid = solid.get();
        let tone = if solid {
            TextTone::Default
        } else if selected.get() {
            TextTone::Tone
        } else {
            TextTone::Muted
        };
        let mut st = css! { "font-weight": "var(--fw-heavy)" };
        if solid {
            st.set("color", "var(--tone-on-fill)");
        }
        let prefix = prefix.clone();
        let kids = children.as_ref().map(|c| c());
        view! { <Text role=TextRole::Caption tone=tone style=st>{prefix}{kids}</Text> }
    };
    crate::ds::Surface(crate::ds::SurfaceProps {
        tag: Some("button".into()),
        tone: Some(tone),
        fill: Signal::derive(move || if solid.get() { Fill::Solid } else { Fill::None }),
        border: Some(Edge::Choice.into()),
        radius: Some(Corner::Pill.into()),
        elevation: None,
        pad: None,
        interactive: Some(Interactive::Choice),
        selected,
        disabled: false,
        on: None,
        on_click,
        style: s,
        attrs,
        children: Some(Box::new(move || text.into_any())),
    })
}

/// OptionCard.jsx: a full-width selectable row with a title, a description and an optional
/// radio or icon tile.
#[component]
pub fn OptionCard(
    #[prop(optional, into)] selected: Signal<bool>,
    #[prop(optional)] control: OptionControl,
    #[prop(optional, into)] icon: Option<String>,
    #[prop(optional, into)] title: String,
    #[prop(optional, into)] description: Option<String>,
    #[prop(optional, into)] meta: Option<String>,
    #[prop(optional)] recommended: bool,
    #[prop(optional, into)] recommended_label: Option<String>,
    #[prop(optional, into)] trailing: Option<String>,
    #[prop(optional)] disabled: bool,
    #[prop(optional)] on_click: Option<Callback<web_sys::Event>>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let radio = control == OptionControl::Radio;
    let s = css! {
        "width": "100%",
        "padding": "14px 16px",
        "text-align": "start",
        "cursor": "pointer",
        "font": "inherit",
    }
    .merge(&style);
    let recommended_label = recommended_label.unwrap_or_else(|| "Recommended".into());
    let (icon, description, meta, trailing) = (
        icon.filter(|s| !s.is_empty()),
        description.filter(|s| !s.is_empty()),
        meta.filter(|s| !s.is_empty()),
        trailing.filter(|s| !s.is_empty()),
    );
    let body = move || {
        let icon_tile = icon.map(|icon| {
            let s = css! {
                "width": 42,
                "height": 42,
                "flex": "none",
                "display": "flex",
                "align-items": "center",
                "justify-content": "center",
            };
            view! {
                <Surface fill=Fill::Soft radius=Corner::Md style=s>
                    <Text role=TextRole::LabelMicro tone=TextTone::Accent>{icon}</Text>
                </Surface>
            }
        });
        view! {
            <Stack direction=Direction::Row gap=12 align=if radio { Align::Start } else { Align::Center }>
                {radio.then(|| choice(Mark::Radio, selected, Signal::from(false), None, None, None, None))}
                {icon_tile}
                <Stack direction=Direction::Column gap=3 grow=true>
                    <Stack direction=Direction::Row gap=8 align=Align::Center wrap=true>
                        <Text role=TextRole::Strong>{title}</Text>
                        {recommended.then(|| view! {
                            <Badge tone=BadgeTone::AccentSoft size=BadgeSize::Sm>{recommended_label}</Badge>
                        })}
                    </Stack>
                    {description.map(|d| view! { <Text role=TextRole::Caption>{d}</Text> })}
                    {meta.map(|m| view! { <Text role=TextRole::Meta>{m}</Text> })}
                </Stack>
                {trailing.map(|t| view! { <Text role=TextRole::Ui tone=TextTone::Muted>{t}</Text> })}
            </Stack>
        }
        .into_any()
    };
    crate::ds::Surface(crate::ds::SurfaceProps {
        tag: Some("button".into()),
        tone: Some(Tone::Accent),
        fill: Signal::from(Fill::None),
        border: Some(Edge::Choice.into()),
        radius: Some(Corner::Md.into()),
        elevation: None,
        pad: None,
        interactive: Some(Interactive::Choice),
        selected,
        disabled,
        on: None,
        on_click,
        style: s,
        attrs,
        children: Some(Box::new(body)),
    })
}

/// Radio.jsx: a circular single-choice control.
#[component]
pub fn Radio(
    #[prop(optional, into)] checked: Signal<bool>,
    #[prop(optional)] disabled: Option<bool>,
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional, into)] description: Option<String>,
    #[prop(optional)] on_change: Option<Callback<web_sys::Event>>,
    #[prop(optional, into)] name: Option<String>,
    #[prop(optional, into)] value: Option<String>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let mark = move || {
        choice(
            Mark::Radio,
            checked,
            Signal::from(false),
            disabled,
            name,
            value,
            on_change,
        )
        .into_any()
    };
    match label.filter(|l| !l.is_empty()) {
        None => bare(mark(), style, attrs).into_any(),
        Some(label) => labelled(
            label,
            description,
            disabled,
            Placement::End,
            None,
            style,
            attrs,
            mark,
        )
        .into_any(),
    }
}

/// One option of a RadioGroup.
#[derive(Debug, Clone, PartialEq)]
pub struct RadioOption {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

impl From<&str> for RadioOption {
    fn from(s: &str) -> Self {
        Self {
            value: s.into(),
            label: s.into(),
            description: None,
        }
    }
}

/// RadioGroup.jsx: a vertical radio set sharing one value, in a FieldGroup with the
/// radiogroup role, a fieldset and an optional legend.
#[component]
pub fn RadioGroup(
    #[prop(optional)] options: Vec<RadioOption>,
    #[prop(optional, into)] value: Signal<Option<String>>,
    #[prop(optional)] on_change: Option<Callback<String>>,
    #[prop(optional, into)] legend: Option<String>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let name = crate::ds::next_id("tc-rg");
    let group_name = name.clone();
    let items = move || {
        options
            .into_iter()
            .map(|o| {
                let v = o.value.clone();
                let checked = {
                    let v = v.clone();
                    Signal::derive(move || value.get().as_deref() == Some(v.as_str()))
                };
                let change = on_change.map(|cb| {
                    let v = v.clone();
                    Callback::new(move |_: web_sys::Event| cb.run(v.clone()))
                });
                let id = format!("{name}-{v}");
                let mark = move || {
                    choice(
                        Mark::Radio,
                        checked,
                        Signal::from(false),
                        None,
                        None,
                        Some(v),
                        change,
                    )
                    .into_any()
                };
                labelled(
                    o.label,
                    o.description,
                    None,
                    Placement::End,
                    Some(id),
                    Css::new(),
                    Attrs::new(),
                    mark,
                )
            })
            .collect_view()
    };
    crate::ds::FieldGroup(crate::ds::FieldGroupProps {
        legend,
        hint: None,
        error: None,
        required: false,
        disabled: None,
        name: Some(group_name),
        role: Some("radiogroup".into()),
        direction: Direction::Column,
        gap: None,
        columns: None,
        style,
        attrs,
        children: Some(Box::new(move || items().into_any())),
    })
}

/// The arrow into a tray of a DropZone.
fn upload_arrow() -> AnyView {
    use leptos::svg;
    svg::svg()
        .add_any_attr(fixed(&[
            ("width", "24"),
            ("height", "24"),
            ("viewBox", "0 0 24 24"),
            ("fill", "none"),
            ("stroke", "var(--uw-inspire)"),
            ("stroke-width", "1.5"),
            ("stroke-linecap", "round"),
            ("stroke-linejoin", "round"),
            ("aria-hidden", "true"),
        ]))
        .child((
            svg::path().add_any_attr(fixed(&[("d", "M12 16V4")])),
            svg::path().add_any_attr(fixed(&[("d", "M6 10l6-6 6 6")])),
            svg::path().add_any_attr(fixed(&[("d", "M4 20h16")])),
        ))
        .into_any()
}

/// The magnifier at the start of a SearchField.
fn magnifier() -> AnyView {
    use leptos::svg;
    svg::svg()
        .add_any_attr(fixed(&[
            ("width", "15"),
            ("height", "15"),
            ("viewBox", "0 0 16 16"),
            ("fill", "none"),
            ("stroke", "currentColor"),
            ("stroke-width", "1.5"),
            ("stroke-linecap", "round"),
            ("aria-hidden", "true"),
        ]))
        .child((
            svg::circle().add_any_attr(fixed(&[("cx", "7"), ("cy", "7"), ("r", "4.5")])),
            svg::path().add_any_attr(fixed(&[("d", "M10.5 10.5L14 14")])),
        ))
        .into_any()
}

/// SearchField.jsx: a search input with a leading magnifier and a clear button while it has a
/// value.
#[component]
pub fn SearchField(
    #[prop(optional, into)] value: Option<Signal<String>>,
    #[prop(optional)] on_change: Option<Callback<web_sys::Event>>,
    #[prop(optional)] on_clear: Option<Callback<MouseEvent>>,
    #[prop(optional, into)] placeholder: Option<String>,
    #[prop(optional, into)] clear_label: Option<String>,
    #[prop(optional, into)] disabled: MaybeProp<bool>,
    #[prop(optional)] style: Css,
    /// Extra attributes of the native input.
    #[prop(optional)]
    attrs: Attrs,
    #[prop(optional)] live_attrs: Vec<(&'static str, Signal<Option<String>>)>,
) -> impl IntoView {
    let clear_label = clear_label.unwrap_or_else(|| "Clear".into());
    let trailing: crate::ds::MaybeView = Arc::new(move || {
        let has = value.is_some_and(|v| !v.get().is_empty());
        let cb = on_clear.filter(|_| has)?;
        Some(
            crate::ds::feedback::close_button(
                clear_label.clone(),
                cb,
                Weight::Text,
                crate::ds::Shape::Pill,
                css! { "width": 20, "height": 20, "min-width": 20 },
            )
            .into_any(),
        )
    });
    crate::ds::Input(crate::ds::InputProps {
        tag: None,
        variant: InputVariant::Ui,
        size: None,
        leading: Some(Box::new(magnifier)),
        trailing: Some(trailing),
        rows: 3,
        invalid: MaybeProp::default(),
        disabled,
        // SearchField always hands `value` to the input, so react-dom writes its initial value
        // attribute even when there is none: an empty one.
        default_value: value.is_none().then(String::new),
        value,
        placeholder: Some(placeholder.unwrap_or_else(|| "Search…".into())),
        on_change,
        on_key_down: None,
        on_blur: None,
        style,
        input_style: Css::new(),
        attrs,
        live_attrs,
        children: None,
    })
}

/// TextArea.jsx: multi-line entry for translator comments and draft text. An `id` goes to the
/// Field, so the label reaches the control.
#[component]
pub fn TextArea(
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional)] variant: InputVariant,
    #[prop(optional, into)] dir: Option<String>,
    #[prop(default = 3)] rows: u32,
    #[prop(optional, into)] id: Option<String>,
    #[prop(optional, into)] invalid: MaybeProp<bool>,
    #[prop(optional, into)] disabled: MaybeProp<bool>,
    #[prop(optional, into)] value: Option<Signal<String>>,
    #[prop(optional, into)] default_value: Option<String>,
    #[prop(optional, into)] placeholder: Option<String>,
    #[prop(optional)] on_change: Option<Callback<web_sys::Event>>,
    #[prop(optional)] on_blur: Option<Callback<FocusEvent>>,
    #[prop(optional)] input_style: Css,
    #[prop(optional)] style: Css,
    /// Extra attributes of the native textarea.
    #[prop(optional)]
    attrs: Attrs,
) -> impl IntoView {
    let mut native = Attrs::new();
    if let Some(d) = dir {
        native.set("dir", d);
    }
    native.extend(&attrs);
    let input = move || {
        crate::ds::Input(crate::ds::InputProps {
            tag: Some(NativeInput::Textarea),
            variant,
            size: None,
            leading: None,
            trailing: None,
            rows,
            invalid,
            disabled,
            value,
            default_value,
            placeholder,
            on_change,
            on_key_down: None,
            on_blur,
            style: Css::new(),
            input_style,
            attrs: native,
            live_attrs: Vec::new(),
            children: None,
        })
        .into_any()
    };
    field_around(label, None, id, style, input)
}

/// A Field above one control, with no disabled or error state of its own.
fn field_around(
    label: Option<String>,
    hint: Option<String>,
    id: Option<String>,
    style: Css,
    control: impl FnOnce() -> AnyView + Send + 'static,
) -> impl IntoView {
    crate::ds::Field(crate::ds::FieldProps {
        label,
        hint,
        error: None,
        required: false,
        optional: false,
        disabled: None,
        size: Size::Md,
        placement: Placement::Above,
        id,
        name: None,
        style,
        attrs: Attrs::new(),
        children: Some(Box::new(control)),
    })
}

/// TextField.jsx: a labelled single-line text input. `invalid` draws the red border only; a
/// Field `error` also wires the ARIA.
#[component]
pub fn TextField(
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional, into)] hint: Option<String>,
    #[prop(optional, into)] invalid: MaybeProp<bool>,
    #[prop(optional, into)] id: Option<String>,
    #[prop(optional)] size: Option<Size>,
    #[prop(optional, into)] disabled: MaybeProp<bool>,
    #[prop(optional, into)] value: Option<Signal<String>>,
    #[prop(optional, into)] default_value: Option<String>,
    #[prop(optional, into)] placeholder: Option<String>,
    #[prop(optional)] on_change: Option<Callback<web_sys::Event>>,
    #[prop(optional)] on_key_down: Option<Callback<KeyboardEvent>>,
    #[prop(optional)] on_blur: Option<Callback<FocusEvent>>,
    #[prop(optional)] style: Css,
    /// Extra attributes of the native input.
    #[prop(optional)]
    attrs: Attrs,
) -> impl IntoView {
    let input = move || {
        crate::ds::Input(crate::ds::InputProps {
            tag: None,
            variant: InputVariant::Ui,
            size,
            leading: None,
            trailing: None,
            rows: 3,
            invalid,
            disabled,
            value,
            default_value,
            placeholder,
            on_change,
            on_key_down,
            on_blur,
            style: Css::new(),
            input_style: Css::new(),
            attrs,
            live_attrs: Vec::new(),
            children: None,
        })
        .into_any()
    };
    field_around(label, hint, id, style, input)
}

/// Toggle.jsx: a setting switch used in side panels. The label sits at the start of the row,
/// which is the whole difference from Checkbox.
#[component]
pub fn Toggle(
    #[prop(optional, into)] checked: Signal<bool>,
    #[prop(optional)] on_change: Option<Callback<web_sys::Event>>,
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional)] disabled: Option<bool>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let mark = move || {
        choice(
            Mark::Switch,
            checked,
            Signal::from(false),
            disabled,
            None,
            None,
            on_change,
        )
        .into_any()
    };
    match label.filter(|l| !l.is_empty()) {
        None => bare(mark(), style, attrs).into_any(),
        Some(label) => labelled(
            label,
            None,
            disabled,
            Placement::Start,
            None,
            style,
            attrs,
            mark,
        )
        .into_any(),
    }
}
