//! A case's tree (`gallery-spec.json`, "nodes") as Leptos views.
//!
//!   `{ "$el": "Button", "props": {…} }`  a DS component (capitalised) or an HTML/SVG tag
//!   `{ "$fn": "noop" }`                   a callback that does nothing; only its presence matters
//!   `{ "$asset": "public/assets/x.png" }` a file of the tC4 checkout, served by the gallery
//!
//! Everything else is plain JSON.

use leptos::prelude::*;
use serde_json::{Map, Value};

use super::components;
use super::props::{Props, warn};
use crate::css::js_number;
use crate::ds::{element, spread};

/// The URL the gallery serves a tC4 asset at.
pub fn asset_url(path: &str) -> Option<&'static str> {
    match path {
        "public/assets/translationcore-logo.png" => Some("assets/images/translationcore-logo.png"),
        "public/assets/logo-mark-color.png" => Some("assets/images/logo-mark-color.png"),
        _ => None,
    }
}

/// The case's frame: `<div data-case tabindex="-1">`, sized and painted as the spec says.
pub fn frame(case: Value, mounted: RwSignal<bool>, inline: bool) -> impl IntoView {
    let id = case["id"].as_str().unwrap_or("").to_string();
    let f = &case["frame"];
    let num = |k: &str| f[k].as_f64().unwrap_or(0.0);
    let style = css! {
        "width": num("width"),
        "height": num("height"),
        "padding": num("padding"),
        "background": f["background"].as_str().unwrap_or("").to_string(),
        "overflow": "hidden",
        "box-sizing": "border-box",
    }
    .to_style();
    let tree = case.get("render").cloned().unwrap_or_else(|| {
        let mut m = Map::new();
        m.insert("$el".into(), case["component"].clone());
        m.insert("props".into(), case["props"].clone());
        Value::Object(m)
    });
    view! {
        <div data-case=id tabindex=(!inline).then_some("-1") style=style>
            {move || mounted.get().then(|| node(&tree))}
        </div>
    }
}

/// Any value of the tree as a view: text, a list, or an element.
pub fn node(v: &Value) -> AnyView {
    match v {
        Value::String(s) => s.clone().into_any(),
        Value::Number(n) => js_number(n.as_f64().unwrap_or(0.0)).into_any(),
        Value::Array(list) => list.iter().map(node).collect::<Vec<_>>().into_any(),
        Value::Object(m) if m.contains_key("$el") => {
            let name = m["$el"].as_str().unwrap_or("");
            let props = m
                .get("props")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            if name.starts_with(|c: char| c.is_ascii_lowercase()) {
                html(name, Props::new(name, props))
            } else {
                components::render(name, Props::new(name, props))
            }
        }
        _ => ().into_any(),
    }
}

/// An HTML or SVG element with its props as attributes.
fn html(tag: &str, p: Props) -> AnyView {
    let style = p.css("style");
    let children = p.value("children");
    let mut attrs = p.rest();
    if !style.is_empty() {
        attrs.set("style", style.to_style());
    }
    let kids = children.as_ref().map(node);
    match tag {
        "svg" => leptos::svg::svg()
            .add_any_attr(spread(&attrs))
            .child(kids)
            .into_any(),
        "circle" => leptos::svg::circle()
            .add_any_attr(spread(&attrs))
            .child(kids)
            .into_any(),
        "path" => leptos::svg::path()
            .add_any_attr(spread(&attrs))
            .child(kids)
            .into_any(),
        "g" => leptos::svg::g()
            .add_any_attr(spread(&attrs))
            .child(kids)
            .into_any(),
        "img" => {
            if let Some(src) = attrs.get("src").and_then(asset_url) {
                attrs.set("src", src);
            }
            element("img").add_any_attr(spread(&attrs)).into_any()
        }
        other => {
            if !matches!(
                other,
                "div" | "p" | "span" | "section" | "header" | "nav" | "label" | "h1" | "h2" | "h3"
            ) {
                warn(&format!("gallery: unexpected tag {other}"));
            }
            element(other.to_string())
                .add_any_attr(spread(&attrs))
                .child(kids)
                .into_any()
        }
    }
}
