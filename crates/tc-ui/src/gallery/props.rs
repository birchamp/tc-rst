//! Reading a case's JSON props into the components' typed props. This is the gallery's
//! boundary: a value of the wrong type is ignored with a console warning, as an unknown
//! keyword is in tC4.

use std::cell::RefCell;

use leptos::prelude::*;
use serde_json::{Map, Value};

use crate::attrs::Attrs;
use crate::css::{Css, CssValue, css_property, js_number};
use crate::tokens::Keyword;

use super::render::node;

/// The props of one element of the tree. Every getter marks its key as read; [`Props::rest`]
/// is everything no getter read, which is React's `...rest`.
pub struct Props {
    name: String,
    map: Map<String, Value>,
    read: RefCell<Vec<String>>,
}

pub fn warn(msg: &str) {
    web_sys::console::warn_1(&msg.into());
}

impl Props {
    pub fn new(name: &str, map: Map<String, Value>) -> Self {
        Self {
            name: name.to_string(),
            map,
            read: RefCell::new(Vec::new()),
        }
    }

    fn take(&self, key: &str) -> Option<&Value> {
        self.read.borrow_mut().push(key.to_string());
        self.map.get(key).filter(|v| !v.is_null())
    }

    pub fn str(&self, key: &str) -> Option<String> {
        match self.take(key)? {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => n.as_f64().map(js_number),
            other => {
                warn(&format!("{}: {key} is not a string: {other}", self.name));
                None
            }
        }
    }

    pub fn num(&self, key: &str) -> Option<f64> {
        match self.take(key)? {
            Value::Number(n) => n.as_f64(),
            other => {
                warn(&format!("{}: {key} is not a number: {other}", self.name));
                None
            }
        }
    }

    /// A boolean prop. JSON `true`/`false`; anything else counts as absent.
    pub fn bool(&self, key: &str) -> Option<bool> {
        match self.take(key)? {
            Value::Bool(b) => Some(*b),
            other => {
                warn(&format!("{}: {key} is not a boolean: {other}", self.name));
                None
            }
        }
    }

    /// React's truthiness of a boolean flag.
    pub fn flag(&self, key: &str) -> bool {
        self.bool(key).unwrap_or(false)
    }

    pub fn keyword<K: Keyword>(&self, key: &str) -> Option<K> {
        let s = self.str(key)?;
        let k = K::from_keyword(&s);
        if k.is_none() {
            warn(&format!("{}: unknown {key} {s:?}", self.name));
        }
        k
    }

    pub fn value(&self, key: &str) -> Option<Value> {
        self.take(key).cloned()
    }

    /// Whether the prop holds a callback (`{ "$fn": … }`).
    pub fn callback(&self, key: &str) -> bool {
        self.take(key).is_some_and(|v| v.get("$fn").is_some())
    }

    /// A React style object.
    pub fn css(&self, key: &str) -> Css {
        self.take(key).map(style_object).unwrap_or_default()
    }

    /// Elements, rendered when the component calls for them.
    pub fn children(&self, key: &str) -> Option<Children> {
        let v = self.take(key)?.clone();
        Some(Box::new(move || node(&v)))
    }

    /// Every prop no getter has read, as HTML attributes (React's `...rest`).
    pub fn rest(&self) -> Attrs {
        let read = self.read.borrow();
        let mut attrs = Attrs::new();
        for (k, v) in &self.map {
            if read.iter().any(|r| r == k) {
                continue;
            }
            if let Some((name, value)) = attribute(k, v) {
                attrs.set(name, value);
            } else if !v.is_null() {
                warn(&format!("{}: prop {k} has no attribute form", self.name));
            }
        }
        attrs
    }
}

/// A React style object as a [`Css`], in key order.
pub fn style_object(v: &Value) -> Css {
    let mut c = Css::new();
    if let Value::Object(m) = v {
        for (k, v) in m {
            let key = css_property(k);
            match v {
                Value::Number(n) => c.set(key, CssValue::Number(n.as_f64().unwrap_or(0.0))),
                Value::String(s) => c.set(key, s.clone()),
                _ => c.set_opt::<String>(key, None),
            }
        }
    }
    c
}

/// A React DOM prop as an HTML attribute: `htmlFor` is `for`, booleans of `aria-*` and
/// `data-*` are the strings "true"/"false", other booleans are present or absent.
pub fn attribute(key: &str, v: &Value) -> Option<(String, String)> {
    let name = match key {
        "htmlFor" => "for".to_string(),
        "className" => "class".to_string(),
        "tabIndex" => "tabindex".to_string(),
        "autoComplete" => "autocomplete".to_string(),
        "readOnly" => "readonly".to_string(),
        "autoFocus" => "autofocus".to_string(),
        "spellCheck" => "spellcheck".to_string(),
        "viewBox" => "viewBox".to_string(),
        k if k.starts_with("aria-") || k.starts_with("data-") => k.to_string(),
        k if k.chars().any(|c| c.is_ascii_uppercase()) => svg_attribute(k)?.to_string(),
        k => k.to_string(),
    };
    let enumerated = name.starts_with("aria-")
        || name.starts_with("data-")
        || name == "draggable"
        || name == "spellcheck";
    let value = match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => js_number(n.as_f64().unwrap_or(0.0)),
        Value::Bool(b) if enumerated => b.to_string(),
        Value::Bool(true) => String::new(),
        _ => return None,
    };
    Some((name, value))
}

/// The SVG presentation attributes React writes hyphenated.
fn svg_attribute(k: &str) -> Option<&'static str> {
    Some(match k {
        "strokeWidth" => "stroke-width",
        "strokeLinecap" => "stroke-linecap",
        "strokeLinejoin" => "stroke-linejoin",
        "strokeDasharray" => "stroke-dasharray",
        "strokeDashoffset" => "stroke-dashoffset",
        "fillRule" => "fill-rule",
        "clipRule" => "clip-rule",
        "textAnchor" => "text-anchor",
        "dominantBaseline" => "dominant-baseline",
        "fontFamily" => "font-family",
        "fontWeight" => "font-weight",
        "fontSize" => "font-size",
        "vectorEffect" => "vector-effect",
        _ => return None,
    })
}
