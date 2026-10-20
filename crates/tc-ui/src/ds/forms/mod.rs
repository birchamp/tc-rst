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
    Attrs, Badge, BadgeSize, BadgeTone, Choice, ChoiceProps, Content, Css, Direction, Field,
    FieldProps, Fill, InputVariant, Interactive, Mark, NativeInput, OptionControl, Placement, Size,
    Stack, Surface, Text, TextRole, TextTone, Tone, Weight, fixed, present,
};
use crate::tokens::{Align, Corner, Edge};

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
        Choice(ChoiceProps {
            mark: Mark::Check,
            checked,
            indeterminate,
            disabled: disabled.into(),
            on_change,
            ..Default::default()
        })
        .into_any()
    };
    match label.filter(|l| !l.is_empty()) {
        None => bare(mark(), style, attrs).into_any(),
        Some(label) => Field(FieldProps {
            label: Some(Signal::from(label)),
            hint: description.map(Signal::from),
            disabled,
            placement: Placement::End,
            style,
            attrs,
            children: Some(Box::new(mark)),
            ..Default::default()
        })
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
    #[prop(optional, into)] title: MaybeProp<String>,
    #[prop(optional, into)] hint: Option<String>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional)] on_click: Option<Callback<web_sys::Event>>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let title = move || {
        title
            .get()
            .unwrap_or_else(|| "Drop your file here, or browse".into())
    };
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
        border: Some(Edge::DashedTone.into()),
        radius: Some(Corner::Xl.into()),
        interactive: Some(Interactive::Choice),
        disabled,
        on_click,
        style: s.into(),
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
        ..Default::default()
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
        interactive: Some(Interactive::Choice),
        selected,
        on_click,
        style: s.into(),
        attrs,
        children: Some(Box::new(move || text.into_any())),
        ..Default::default()
    })
}

/// OptionCard.jsx: a full-width selectable row with a title, a description and an optional
/// radio or icon tile.
#[component]
pub fn OptionCard(
    #[prop(optional, into)] selected: Signal<bool>,
    #[prop(optional)] control: OptionControl,
    #[prop(optional, into)] icon: Option<Content>,
    #[prop(optional, into)] title: Option<Content>,
    #[prop(optional, into)] description: Option<Content>,
    #[prop(optional, into)] meta: Option<Content>,
    #[prop(optional)] recommended: bool,
    #[prop(optional, into)] recommended_label: Option<String>,
    #[prop(optional, into)] trailing: Option<Content>,
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
        present(icon),
        present(description),
        present(meta),
        present(trailing),
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
                    <Text role=TextRole::LabelMicro tone=TextTone::Accent>{icon.view()}</Text>
                </Surface>
            }
        });
        view! {
            <Stack direction=Direction::Row gap=12 align=if radio { Align::Start } else { Align::Center }>
                {radio.then(|| view! { <Choice mark=Mark::Radio checked=selected /> })}
                {icon_tile}
                <Stack direction=Direction::Column gap=3 grow=true>
                    <Stack direction=Direction::Row gap=8 align=Align::Center wrap=true>
                        <Text role=TextRole::Strong>{title.map(|t| t.view())}</Text>
                        {recommended.then(|| view! {
                            <Badge tone=BadgeTone::AccentSoft size=BadgeSize::Sm>{recommended_label}</Badge>
                        })}
                    </Stack>
                    {description.map(|d| view! { <Text role=TextRole::Caption>{d.view()}</Text> })}
                    {meta.map(|m| view! { <Text role=TextRole::Meta>{m.view()}</Text> })}
                </Stack>
                {trailing.map(|t| view! { <Text role=TextRole::Ui tone=TextTone::Muted>{t.view()}</Text> })}
            </Stack>
        }
        .into_any()
    };
    crate::ds::Surface(crate::ds::SurfaceProps {
        tag: Some("button".into()),
        tone: Some(Tone::Accent),
        border: Some(Edge::Choice.into()),
        radius: Some(Corner::Md.into()),
        interactive: Some(Interactive::Choice),
        selected,
        disabled: disabled.into(),
        on_click,
        style: s.into(),
        attrs,
        children: Some(Box::new(body)),
        ..Default::default()
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
        Choice(ChoiceProps {
            mark: Mark::Radio,
            checked,
            disabled: disabled.into(),
            name,
            value,
            on_change,
            ..Default::default()
        })
        .into_any()
    };
    match label.filter(|l| !l.is_empty()) {
        None => bare(mark(), style, attrs).into_any(),
        Some(label) => Field(FieldProps {
            label: Some(Signal::from(label)),
            hint: description.map(Signal::from),
            disabled,
            placement: Placement::End,
            style,
            attrs,
            children: Some(Box::new(mark)),
            ..Default::default()
        })
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
                    Choice(ChoiceProps {
                        mark: Mark::Radio,
                        checked,
                        value: Some(v),
                        on_change: change,
                        ..Default::default()
                    })
                    .into_any()
                };
                Field(FieldProps {
                    label: Some(Signal::from(o.label)),
                    hint: o.description.map(Signal::from),
                    placement: Placement::End,
                    id: Some(id),
                    children: Some(Box::new(mark)),
                    ..Default::default()
                })
            })
            .collect_view()
    };
    crate::ds::FieldGroup(crate::ds::FieldGroupProps {
        legend,
        name: Some(group_name),
        role: Some("radiogroup".into()),
        style,
        attrs,
        children: Some(Box::new(move || items().into_any())),
        ..Default::default()
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
    /// Follows the shown language while the field is on screen, as the clear label does.
    #[prop(optional, into)]
    placeholder: Option<Signal<String>>,
    #[prop(optional, into)] clear_label: Option<Signal<String>>,
    #[prop(optional, into)] disabled: MaybeProp<bool>,
    #[prop(optional)] style: Css,
    /// Extra attributes of the native input.
    #[prop(optional)]
    attrs: Attrs,
    #[prop(optional)] live_attrs: Vec<(&'static str, Signal<Option<String>>)>,
) -> impl IntoView {
    let clear_label = clear_label.unwrap_or_else(|| Signal::from("Clear"));
    let placeholder = placeholder.unwrap_or_else(|| Signal::from("Search…"));
    let mut live_attrs = live_attrs;
    live_attrs.push((
        "placeholder",
        Signal::derive(move || Some(placeholder.get())),
    ));
    // The clear button comes and goes with the value, and stays the same node while the
    // translator types.
    let has = Memo::new(move |_| value.is_some_and(|v| v.with(|v| !v.is_empty())));
    let trailing: crate::ds::MaybeView = Arc::new(move || {
        let cb = on_clear.filter(|_| has.get())?;
        Some(
            crate::ds::feedback::close_button(
                clear_label,
                cb,
                Weight::Text,
                crate::ds::Shape::Pill,
                css! { "width": 20, "height": 20, "min-width": 20 },
            )
            .into_any(),
        )
    });
    crate::ds::Input(crate::ds::InputProps {
        leading: Some(Box::new(magnifier)),
        trailing: Some(trailing),
        disabled,
        // SearchField always hands `value` to the input, so react-dom writes its initial value
        // attribute even when there is none: an empty one.
        default_value: value.is_none().then(String::new),
        value,
        on_change,
        style,
        attrs,
        live_attrs,
        ..Default::default()
    })
}

/// TextArea.jsx: multi-line entry for translator comments and draft text. An `id` goes to the
/// Field, so the label reaches the control.
#[component]
pub fn TextArea(
    #[prop(optional, into)] label: Option<Signal<String>>,
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
            rows,
            invalid,
            disabled,
            value,
            default_value,
            placeholder,
            on_change,
            on_blur,
            input_style,
            attrs: native,
            ..Default::default()
        })
        .into_any()
    };
    field_around(label, None, id, style, input)
}

/// A Field above one control, with no disabled or error state of its own.
fn field_around(
    label: Option<Signal<String>>,
    hint: Option<Signal<String>>,
    id: Option<String>,
    style: Css,
    control: impl FnOnce() -> AnyView + Send + 'static,
) -> impl IntoView {
    crate::ds::Field(crate::ds::FieldProps {
        label,
        hint,
        id,
        style,
        children: Some(Box::new(control)),
        ..Default::default()
    })
}

/// TextField.jsx: a labelled single-line text input. `invalid` draws the red border only; a
/// Field `error` also wires the ARIA.
#[component]
pub fn TextField(
    #[prop(optional, into)] label: Option<Signal<String>>,
    #[prop(optional, into)] hint: Option<Signal<String>>,
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
            size,
            invalid,
            disabled,
            value,
            default_value,
            placeholder,
            on_change,
            on_key_down,
            on_blur,
            attrs,
            ..Default::default()
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
        Choice(ChoiceProps {
            mark: Mark::Switch,
            checked,
            disabled: disabled.into(),
            on_change,
            ..Default::default()
        })
        .into_any()
    };
    match label.filter(|l| !l.is_empty()) {
        None => bare(mark(), style, attrs).into_any(),
        Some(label) => Field(FieldProps {
            label: Some(Signal::from(label)),
            disabled,
            placement: Placement::Start,
            style,
            attrs,
            children: Some(Box::new(mark)),
            ..Default::default()
        })
        .into_any(),
    }
}
