//! FieldGroup.jsx: a set of controls answering one question: legend, group hint, group-level
//! error, and the shared `name` handed to each Choice through [`FieldGroupContext`].

use leptos::prelude::*;

use super::field::{error_line, required_mark};
use crate::css::CssValue;
use crate::ds::{Attrs, Columns, Css, Direction, Stack, Text, TextRole, TextTone, provide, spread};

/// What a Choice inside a FieldGroup reads: React's `FieldGroupContext`.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldGroupContext {
    pub name: Option<String>,
    pub disabled: Option<bool>,
    pub invalid: bool,
    pub described_by: Option<String>,
}

/// `useFieldGroup()`: the enclosing group's context, if any.
pub fn use_field_group() -> Option<FieldGroupContext> {
    use_context::<Option<FieldGroupContext>>().flatten()
}

/// A real `<fieldset>`, because that is what carries the legend to assistive technology. The
/// group's `role` and ARIA live on an inner div: a fieldset with role="radiogroup" loses its
/// legend in several screen readers.
#[component]
pub fn FieldGroup(
    #[prop(optional, into)] legend: Option<String>,
    #[prop(optional, into)] hint: Option<String>,
    #[prop(optional, into)] error: Option<String>,
    #[prop(optional)] required: bool,
    #[prop(optional)] disabled: Option<bool>,
    #[prop(optional, into)] name: Option<String>,
    /// `group` unless given (`radiogroup` for a set of radios).
    #[prop(optional, into)]
    role: Option<String>,
    #[prop(optional)] direction: Direction,
    /// 10 px unless given.
    #[prop(optional, into)]
    gap: Option<CssValue>,
    #[prop(optional)] columns: Option<Columns>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let id = crate::ds::next_id("tc-g");
    let legend = legend.filter(|s| !s.is_empty());
    let hint = hint.filter(|s| !s.is_empty());
    let error = error.filter(|s| !s.is_empty());
    let hint_id = hint.as_ref().map(|_| format!("{id}-hint"));
    let err_id = error.as_ref().map(|_| format!("{id}-err"));
    let described_by = [hint_id.clone(), err_id.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    let described_by = (!described_by.is_empty()).then_some(described_by);
    let off = disabled == Some(true);
    let has_legend = legend.is_some();
    let labelled_by = (!has_legend)
        .then(|| attrs.get("aria-labelledby").map(String::from))
        .flatten();
    let aria_label = (!has_legend)
        .then(|| attrs.get("aria-label").map(String::from))
        .flatten();
    let ctx = FieldGroupContext {
        name,
        disabled,
        invalid: error.is_some(),
        described_by: described_by.clone(),
    };
    let fieldset_style =
        css! { "border": 0, "margin": 0, "padding": 0, "min-inline-size": 0 }.merge(&style);

    let legend_node = legend.map(|legend| {
        view! {
            <Text role=TextRole::Overline tone=if off { TextTone::Faint } else { TextTone::Muted }>
                {legend}
                {required.then(required_mark)}
            </Text>
        }
    });
    let hint_node = hint.map(|hint| {
        let hid = hint_id.clone().unwrap_or_default();
        view! { <Text attrs=Attrs::from([("id", hid.as_str())]) role=TextRole::Caption tone=TextTone::Muted>{hint}</Text> }
    });
    let err_node = error.map(|e| error_line(err_id.clone().unwrap_or_default(), e));
    let gap = gap.unwrap_or(CssValue::Number(10.0));
    let body = provide(Some(ctx), move || {
        Stack(crate::ds::StackProps {
            direction,
            gap: Some(gap),
            align: None,
            justify: None,
            wrap: false,
            inline: false,
            columns,
            flex: None,
            grow: false,
            tag: None,
            on_click: None,
            style: Css::new(),
            attrs: Attrs::new(),
            children,
        })
    });
    let legend_style = css! { "padding": 0, "float": "none" }.to_style();
    let legend_node = legend_node.map(|l| view! { <legend style=legend_style>{l}</legend> });
    view! {
        <fieldset disabled=off style=fieldset_style.to_style() {..spread(&attrs)}>
            <Stack direction=Direction::Column gap=8>
                {legend_node}
                {hint_node}
                <div
                    role=role.unwrap_or_else(|| "group".to_string())
                    aria-labelledby=labelled_by
                    aria-label=aria_label
                    aria-describedby=described_by
                    aria-invalid=err_node.is_some().then_some("true")
                    aria-required=required.then_some("true")
                >
                    {body}
                </div>
                {err_node}
            </Stack>
        </fieldset>
    }
}
