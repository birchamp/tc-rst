//! Field.jsx: the chrome and the wiring around one control: label, required marker, hint,
//! error. The control reads its id, described-by list and invalid flag from [`FieldContext`].

use leptos::prelude::*;

use crate::ds::{
    Align, Attrs, Css, Direction, Justify, Placement, Size, Stack, Text, TextRole, TextTone,
    provide,
};

/// What a control inside a Field reads: React's `FieldContext`.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldContext {
    pub id: String,
    pub described_by: Option<String>,
    pub invalid: bool,
    pub disabled: Option<bool>,
    pub size: Size,
    pub name: Option<String>,
}

/// `useField()`: the enclosing Field's context, if any. A subtree can hide it by providing
/// `None` (React's `<FieldContext.Provider value={null}>`).
pub fn use_field() -> Option<FieldContext> {
    use_context::<Option<FieldContext>>().flatten()
}

/// The `!` disc in front of a field or group error.
pub(crate) fn error_glyph() -> impl IntoView {
    let s = css! {
        "width": 14,
        "height": 14,
        "flex": "none",
        "margin-top": 1,
        "border-radius": "var(--radius-pill)",
        "border": "var(--stroke-selected) solid currentColor",
        "display": "inline-flex",
        "align-items": "center",
        "justify-content": "center",
        "font-size": 9,
        "font-weight": "var(--fw-black)",
        "line-height": 1,
    };
    view! { <span aria-hidden="true" style=s.to_style()>"!"</span> }
}

/// The error line under a field or a group: a `p` in the invalid tone with the glyph and an
/// alert.
pub(crate) fn error_line(id: String, error: String) -> impl IntoView {
    view! {
        <Text
            role=TextRole::CaptionStrong
            tag="p"
            tone=TextTone::Tone
            attrs=Attrs::from([("id", id.as_str()), ("data-tone", "invalid")])
            style=css! { "display": "flex", "align-items": "flex-start", "gap": 6 }
        >
            {error_glyph()}
            <span role="alert">{error}</span>
        </Text>
    }
}

/// The marker after a required label.
pub(crate) fn required_mark() -> impl IntoView {
    view! { <span aria-hidden="true" style="color: var(--tc-invalid); margin-inline-start: 3px;">"*"</span> }
}

/// One control's label, hint and error, wired through context. `placement` puts the label
/// above the control or beside it at its start or end.
///
/// The label and the hint follow the shown language while the field is on screen (the App
/// language preview rewrites them in place, as React re-renders them); whether each is drawn
/// is decided by its text when the field is drawn.
#[component]
pub fn Field(
    #[prop(optional, into)] label: Option<Signal<String>>,
    #[prop(optional, into)] hint: Option<Signal<String>>,
    #[prop(optional, into)] error: Option<String>,
    #[prop(optional)] required: bool,
    #[prop(optional)] optional: bool,
    #[prop(optional)] disabled: Option<bool>,
    #[prop(optional)] size: Size,
    #[prop(optional)] placement: Placement,
    #[prop(optional, into)] id: Option<String>,
    #[prop(optional, into)] name: Option<String>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    // React's useId: the sequence advances once per Field, given id or not.
    let generated = crate::ds::next_id("tc-f");
    let id = id.unwrap_or(generated);
    let label = label.filter(|s| s.with_untracked(|s| !s.is_empty()));
    let hint = hint.filter(|s| s.with_untracked(|s| !s.is_empty()));
    let error = error.filter(|s| !s.is_empty());
    let hint_id = hint.as_ref().map(|_| format!("{id}-hint"));
    let err_id = error.as_ref().map(|_| format!("{id}-err"));
    let described_by = [hint_id.clone(), err_id.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    let ctx = FieldContext {
        id: id.clone(),
        described_by: (!described_by.is_empty()).then_some(described_by),
        invalid: error.is_some(),
        disabled,
        size,
        name,
    };
    let above = placement == Placement::Above;
    let off = disabled == Some(true);

    let label_node = label.map(|label| {
        let label_style = css! { "cursor": if off { "default" } else { "pointer" } };
        let opt_style = "color: var(--fg-faint); margin-inline-start: 5px; text-transform: none; letter-spacing: 0;";
        view! {
            <Text
                tag="label"
                attrs=Attrs::from([("for", id.as_str())])
                role=if above { TextRole::Overline } else { TextRole::Ui }
                tone=if off { TextTone::Faint } else if above { TextTone::Muted } else { TextTone::Default }
                style=label_style
            >
                {move || label.get()}
                {required.then(required_mark)}
                {optional.then(|| view! { <span style=opt_style>"optional"</span> })}
            </Text>
        }
    });
    let hint_node = hint.map(|hint| {
        let hid = hint_id.clone().unwrap_or_default();
        view! { <Text attrs=Attrs::from([("id", hid.as_str())]) role=TextRole::Caption tone=TextTone::Muted>{move || hint.get()}</Text> }
    });
    let err_node = error.map(|e| error_line(err_id.clone().unwrap_or_default(), e));
    let body = provide(Some(ctx), move || children.map(|c| c()));

    match placement {
        Placement::End | Placement::Start => {
            let start = placement == Placement::Start;
            let align = if hint_node.is_some() {
                Align::Start
            } else {
                Align::Center
            };
            let side = move |grow: bool| {
                view! {
                    <Stack direction=Direction::Column gap=2 grow=grow>
                        {label_node}
                        {hint_node}
                    </Stack>
                }
            };
            let row = if start {
                view! { {side(true)} {body} }.into_any()
            } else {
                view! { {body} {side(false)} }.into_any()
            };
            let justify = start.then_some(Justify::Between);
            view! {
                <Stack direction=Direction::Column gap=6 style=style attrs=attrs>
                    {match justify {
                        Some(j) => view! { <Stack direction=Direction::Row gap=10 align=align justify=j>{row}</Stack> }.into_any(),
                        None => view! { <Stack direction=Direction::Row gap=10 align=align>{row}</Stack> }.into_any(),
                    }}
                    {err_node}
                </Stack>
            }
            .into_any()
        }
        Placement::Above => view! {
            <Stack direction=Direction::Column gap=6 style=style attrs=attrs>
                {label_node}
                {body}
                {hint_node}
                {err_node}
            </Stack>
        }
        .into_any(),
    }
}
