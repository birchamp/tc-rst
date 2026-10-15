//! Input.jsx: the box a value is typed or picked in. It is the box only: the label, the hint
//! and the error belong to the enclosing Field, which it reads through context.

use leptos::ev::{self, FocusEvent, KeyboardEvent};
use leptos::prelude::*;
use leptos::tachys::html::element::Custom;
use wasm_bindgen::JsCast;

use super::field::use_field;
use crate::ds::{
    Attrs, Css, InputVariant, MaybeView, NativeInput, Size, Slot, Tag, element, live, spread,
};

/// `node.value = v` when it differs, as React restores a controlled field.
fn set_value(el: &web_sys::HtmlElement, want: &str) {
    let key = wasm_bindgen::JsValue::from_str("value");
    let now = js_sys::Reflect::get(el, &key)
        .ok()
        .and_then(|v| v.as_string());
    if now.as_deref() != Some(want) {
        let _ = js_sys::Reflect::set(el, &key, &wasm_bindgen::JsValue::from_str(want));
    }
}

fn by_size(size: Size, sm: &'static str, md: &'static str, lg: &'static str) -> &'static str {
    match size {
        Size::Sm => sm,
        Size::Md => md,
        Size::Lg => lg,
    }
}

/// Wraps a native input, textarea or select, keeps the height ladder, and takes a glyph at
/// either end. `value` makes it controlled (React's `value`): what is typed reaches
/// `on_change`, and the field shows `value` again unless the owner changed it.
#[component]
pub fn Input(
    #[prop(optional, into)] tag: Option<NativeInput>,
    #[prop(optional)] variant: InputVariant,
    /// Unset means the enclosing Field's, else md.
    #[prop(optional)]
    size: Option<Size>,
    #[prop(optional)] leading: Option<Slot>,
    /// Drawn only while it returns a view.
    #[prop(optional)]
    trailing: Option<MaybeView>,
    #[prop(default = 3)] rows: u32,
    #[prop(optional, into)] invalid: MaybeProp<bool>,
    #[prop(optional, into)] disabled: MaybeProp<bool>,
    #[prop(optional, into)] value: Option<Signal<String>>,
    #[prop(optional, into)] default_value: Option<String>,
    #[prop(optional, into)] placeholder: Option<String>,
    #[prop(optional)] on_change: Option<Callback<web_sys::Event>>,
    #[prop(optional)] on_key_down: Option<Callback<KeyboardEvent>>,
    #[prop(optional)] on_blur: Option<Callback<FocusEvent>>,
    /// The outer box's style.
    #[prop(optional)]
    style: Css,
    /// The native element's style.
    #[prop(optional)]
    input_style: Css,
    /// Extra attributes of the native element.
    #[prop(optional)]
    attrs: Attrs,
    /// Attributes of the native element that change while it has the focus (`aria-expanded`,
    /// `aria-activedescendant`).
    #[prop(optional)]
    live_attrs: Vec<(&'static str, Signal<Option<String>>)>,
    /// A select's options.
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    let tag = tag.unwrap_or_default();
    let f = use_field();
    let sz = size
        .or_else(|| f.as_ref().map(|f| f.size))
        .unwrap_or(Size::Md);
    let f_invalid = f.as_ref().map(|f| f.invalid);
    let f_disabled = f.as_ref().and_then(|f| f.disabled);
    let invalid = move || invalid.get().or(f_invalid).unwrap_or(false);
    let disabled = move || disabled.get().or(f_disabled).unwrap_or(false);
    let multiline = tag == NativeInput::Textarea;
    let scripture = variant == InputVariant::Scripture;

    let outer = move || {
        let off = disabled();
        css! {
            "box-sizing": "border-box",
            "display": "flex",
            "align-items": if multiline { "stretch" } else { "center" },
            "gap": 8,
            "background": if off { "var(--disabled-bg)" } else if scripture { "var(--surface-app)" } else { "#fff" },
            "border": format!("var(--stroke) solid {}", if invalid() { "var(--tc-invalid)" } else { "var(--line-strong)" }),
            "border-radius": "var(--radius-input)",
            "padding-inline": by_size(sz, "var(--field-px-sm)", "var(--field-px-md)", "var(--field-px-lg)"),
            "height": (!multiline).then(|| by_size(sz, "var(--control-h-sm)", "var(--control-h-md)", "var(--control-h-lg)")),
            "padding-block": if multiline { 10 } else { 0 },
        }
        .merge(&style)
        .to_style()
    };
    let native_style = move || {
        css! {
            "flex": 1,
            "min-width": 0,
            "width": "100%",
            "border": 0,
            "outline": "none",
            "background": "transparent",
            "padding": 0,
            "margin": 0,
            "appearance": (tag != NativeInput::Select).then_some("none"),
            "font-family": if scripture { "var(--font-scripture)" } else { "var(--font-ui)" },
            "font-size": if scripture { "var(--fs-verse-lg)" } else { by_size(sz, "var(--fs-ui-sm)", "var(--fs-ui-md)", "var(--fs-ui-md)") },
            "line-height": if scripture { "var(--lh-verse-lg)" } else { "var(--lh-ui)" },
            "color": if scripture { "var(--text-scripture)" } else { "var(--fg)" },
            "resize": multiline.then_some("vertical"),
            "cursor": disabled().then_some("default"),
        }
        .merge(&input_style)
        .to_style()
    };

    // React writes a text field's initial value as its `value` attribute and a textarea's as
    // its text; a controlled field keeps both the attribute and the property in step.
    let initial = value.map(|v| v.get_untracked()).or(default_value);
    let text = multiline.then(|| initial.clone()).flatten();
    let value_attr = move || {
        if multiline {
            None
        } else {
            value.map(|v| v.get()).or_else(|| initial.clone())
        }
    };
    let node = NodeRef::<Custom<Tag>>::new();
    if let Some(v) = value {
        Effect::new(move |_| {
            let want = v.get();
            if let Some(el) = node.get() {
                set_value(&el, &want);
            }
        });
    }
    let native = element(tag.as_str())
        .node_ref(node)
        .attr("id", f.as_ref().map(|f| f.id.clone()))
        .attr("name", f.as_ref().and_then(|f| f.name.clone()))
        .attr("rows", multiline.then(|| rows.to_string()))
        .attr("disabled", disabled)
        .attr(
            "aria-describedby",
            f.as_ref().and_then(|f| f.described_by.clone()),
        )
        .attr("aria-invalid", move || invalid().then_some("true"))
        .attr("style", native_style)
        .attr("placeholder", placeholder)
        .attr("value", value_attr)
        .add_any_attr(spread(&attrs))
        .add_any_attr(live(live_attrs))
        .on(ev::input, move |e| {
            let el = e
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok());
            if let Some(cb) = on_change {
                cb.run(e);
            }
            if let (Some(v), Some(el)) = (value, el) {
                set_value(&el, &v.get_untracked());
            }
        })
        .on(ev::keydown, move |e| {
            if let Some(cb) = on_key_down {
                cb.run(e);
            }
        })
        .on(ev::blur, move |e| {
            if let Some(cb) = on_blur {
                cb.run(e);
            }
        })
        .child(text)
        .child(children.map(|c| c()));

    let leading = leading.map(|l| {
        view! { <span aria-hidden="true" style="display: flex; flex: none; color: var(--fg-muted);">{l()}</span> }
    });
    let trailing = move || {
        trailing
            .as_ref()
            .and_then(|t| t())
            .map(|t| view! { <span style="display: flex; flex: none;">{t}</span> })
    };
    view! {
        <div data-i=move || (!disabled()).then_some("field") aria-invalid=move || invalid().then_some("true") style=outer>
            {leading}
            {native}
            {trailing}
        </div>
    }
}
