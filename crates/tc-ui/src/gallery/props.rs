//! Reading a case's JSON props into the components' typed props. This is the gallery's
//! boundary: a value of the wrong type is ignored with a console warning, as an unknown
//! keyword is in tC4.
//!
//! A case can own state, as a screen does: its `cells` are signals, `{ "$cell": name }` reads
//! one, `{ "$if": name, "then": …, "else": … }` picks a value by one, and `{ "$fn": "set", … }`
//! writes one, so a prop the component takes as a signal follows the write on the nodes
//! already drawn.

use std::cell::RefCell;
use std::collections::HashMap;

use leptos::prelude::*;
use serde_json::{Map, Value};

use crate::attrs::Attrs;
use crate::css::{Css, CssValue, css_property, js_number};
use crate::tokens::Keyword;

use super::render::node;

/// The callback a `{ "$fn": … }` stands for.
fn callback_of<T: Arg>(v: &Value) -> Option<Callback<T>> {
    match v.get("$fn")?.as_str()? {
        "preventDefault" => Some(Callback::new(|e: T| e.prevent_default())),
        "set" => {
            let Some(cell) = v.get("cell").and_then(Value::as_str).and_then(cell_named) else {
                return Some(Callback::new(|_| {}));
            };
            let fixed = v.get("value").cloned();
            let from = v.get("from").and_then(Value::as_str).map(String::from);
            Some(Callback::new(move |arg: T| {
                let next = fixed
                    .clone()
                    .or_else(|| from.as_deref().and_then(|p| arg.pick(p)));
                match next {
                    Some(next) => cell.set(next),
                    None => warn("gallery: a set callback found no value in its arguments"),
                }
            }))
        }
        _ => Some(Callback::new(|_| {})),
    }
}

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

thread_local! {
    /// The cells of the case on the page.
    static CELLS: RefCell<HashMap<String, RwSignal<Value>>> = RefCell::new(HashMap::new());
}

/// Makes the case's `cells` (an object of first values) the cells the tree reads and writes.
pub fn init_cells(cells: &Value) {
    let map = cells
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), RwSignal::new(v.clone())))
                .collect()
        })
        .unwrap_or_default();
    CELLS.with(|c| *c.borrow_mut() = map);
}

fn cell_named(name: &str) -> Option<RwSignal<Value>> {
    let cell = CELLS.with(|c| c.borrow().get(name).copied());
    if cell.is_none() {
        warn(&format!("gallery: no cell {name}"));
    }
    cell
}

/// The cell a `{ "$cell": name }` names.
pub(super) fn cell_of(v: &Value) -> Option<RwSignal<Value>> {
    cell_named(v.get("$cell")?.as_str()?)
}

/// JavaScript's truthiness.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// The branch a `{ "$if": cell, "then": …, "else": … }` takes now (`Null` for an absent one).
fn branch(v: &Map<String, Value>, cell: RwSignal<Value>) -> Value {
    let key = if cell.with(truthy) { "then" } else { "else" };
    v.get(key).cloned().unwrap_or(Value::Null)
}

/// A value that follows the case's cells: a `$cell` or an `$if`. `None` for a fixed value.
pub(super) fn follow(v: &Value) -> Option<Signal<Value>> {
    if let Some(cell) = cell_of(v) {
        return Some(Signal::derive(move || cell.get()));
    }
    let m = v.as_object()?;
    let cell = cell_named(m.get("$if")?.as_str()?)?;
    let m = m.clone();
    // A memo: a write that keeps the branch notifies nothing, as React keeps the same value.
    Some(Memo::new(move |_| branch(&m, cell)).into())
}

/// A boolean inside a prop (an option's `disabled`), following the cells: absent is false.
pub fn flag_in(v: Option<&Value>) -> Signal<bool> {
    let Some(v) = v else {
        return Signal::from(false);
    };
    match follow(v) {
        Some(s) => Signal::derive(move || s.with(truthy)),
        None => Signal::from(v.as_bool().unwrap_or(false)),
    }
}

/// A callback's argument, as the value a `{ "$fn": "set", "from": path }` takes from it. The
/// path starts with the argument's position ("0", "0.target.checked", "0.id", "1").
pub trait Arg: 'static {
    fn pick(&self, path: &str) -> Option<Value> {
        let _ = path;
        None
    }

    /// `event.preventDefault()`, for an argument that is an event.
    fn prevent_default(&self) {
        warn("gallery: preventDefault on an argument that is not an event");
    }
}

impl Arg for () {}
impl Arg for leptos::ev::MouseEvent {
    fn prevent_default(&self) {
        web_sys::Event::prevent_default(self);
    }
}
impl Arg for leptos::ev::PointerEvent {
    fn prevent_default(&self) {
        web_sys::Event::prevent_default(self);
    }
}
impl Arg for leptos::ev::KeyboardEvent {}
impl Arg for leptos::ev::FocusEvent {}
impl Arg for crate::ds::DismissReason {}
impl Arg for crate::ds::SelectOption {
    fn pick(&self, path: &str) -> Option<Value> {
        let field = match path {
            "0.value" => Some(&self.value),
            "0.label" => Some(&self.label),
            "0.code" => self.code.as_ref(),
            "0.meta" => self.meta.as_ref(),
            _ => None,
        };
        field.map(|f| Value::String(f.clone()))
    }
}

/// The value a RadioGroup or a Select hands over. Select hands React `{ target: { value } }`,
/// as the native shim did, and the port the value itself.
impl Arg for String {
    fn pick(&self, path: &str) -> Option<Value> {
        matches!(path, "0" | "0.target.value").then(|| Value::String(self.clone()))
    }
}

impl Arg for bool {
    fn pick(&self, path: &str) -> Option<Value> {
        (path == "0").then_some(Value::Bool(*self))
    }
}

impl Arg for web_sys::Event {
    fn pick(&self, path: &str) -> Option<Value> {
        use wasm_bindgen::JsCast;
        let input = self
            .target()?
            .dyn_into::<web_sys::HtmlInputElement>()
            .ok()?;
        match path {
            "0.target.checked" => Some(Value::Bool(input.checked())),
            "0.target.value" => Some(Value::String(input.value())),
            _ => None,
        }
    }
}

impl<R: crate::ds::Record> Arg for (R, usize) {
    fn pick(&self, path: &str) -> Option<Value> {
        match path.strip_prefix("0.") {
            Some(field) => self.0.field(field).map(Value::String),
            None => (path == "1").then(|| Value::from(self.1)),
        }
    }
}

impl Props {
    pub fn new(name: &str, map: Map<String, Value>) -> Self {
        Self {
            name: name.to_string(),
            map,
            read: RefCell::new(Vec::new()),
        }
    }

    /// The prop as written in the tree, `$cell` and all.
    fn raw(&self, key: &str) -> Option<&Value> {
        self.read.borrow_mut().push(key.to_string());
        self.map.get(key).filter(|v| !v.is_null())
    }

    /// The prop's value now: a cell, or the branch of an `$if`, reads as its current value.
    fn take(&self, key: &str) -> Option<Value> {
        let v = self.raw(key)?;
        match follow(v) {
            Some(s) => Some(s.get_untracked()).filter(|v| !v.is_null()),
            None => Some(v.clone()),
        }
    }

    /// A prop the component takes as a signal: a cell or an `$if` follows the writes, anything
    /// else stays.
    fn live<T: Send + Sync + Clone + 'static>(
        &self,
        key: &str,
        read: impl Fn(&Value) -> Option<T> + Send + Sync + 'static,
    ) -> Signal<Option<T>> {
        match self.raw(key) {
            Some(v) => match follow(v) {
                Some(s) => {
                    Signal::derive(move || s.with(|v| (!v.is_null()).then(|| read(v)).flatten()))
                }
                None => Signal::from(read(v)),
            },
            None => Signal::from(None),
        }
    }

    /// A list the component takes as a signal, item by item: a cell follows its writes. A
    /// single value is a list of one.
    pub fn list_signal<T: Clone + PartialEq + Send + Sync + 'static>(
        &self,
        key: &str,
        item: fn(&Value) -> Option<T>,
    ) -> Signal<Vec<T>> {
        let s = self.live(key, move |v| {
            Some(match v {
                Value::Array(a) => a.iter().filter_map(item).collect::<Vec<_>>(),
                other => item(other).into_iter().collect(),
            })
        });
        Memo::new(move |_| s.get().unwrap_or_default()).into()
    }

    /// A boolean the component takes as a signal; absent is false.
    pub fn flag_signal(&self, key: &str) -> Signal<bool> {
        let s = self.live(key, Value::as_bool);
        Signal::derive(move || s.get().unwrap_or(false))
    }

    /// A React style object the component takes as a signal.
    pub fn css_signal(&self, key: &str) -> Signal<Css> {
        let s = self.live(key, |v| Some(style_object(v)));
        Signal::derive(move || s.get().unwrap_or_default())
    }

    /// A string the component takes as a signal.
    pub fn str_signal(&self, key: &str) -> Signal<Option<String>> {
        self.live(key, |v| match v {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => n.as_f64().map(js_number),
            _ => None,
        })
    }

    /// A number the component takes as a signal.
    pub fn num_signal(&self, key: &str) -> Signal<Option<f64>> {
        self.live(key, Value::as_f64)
    }

    /// Whether a callback prop holds a callback now: React's `onClose={busy ? undefined :
    /// close}`. Always for a plain callback.
    pub fn handler_on(&self, key: &str) -> Signal<bool> {
        let s = self.live(key, |v| Some(v.get("$fn").is_some()));
        Signal::derive(move || s.get().unwrap_or(false))
    }

    /// Whether the prop is an `$if`.
    pub fn is_if(&self, key: &str) -> bool {
        self.map.get(key).is_some_and(|v| v.get("$if").is_some())
    }

    /// Whether the prop is present, as a cell or a value.
    pub fn has(&self, key: &str) -> bool {
        self.map.get(key).is_some_and(|v| !v.is_null())
    }

    /// A callback prop: `{ "$fn": "noop" }` does nothing, `{ "$fn": "preventDefault" }` calls
    /// `preventDefault` on its event, `{ "$fn": "set", … }` writes a cell. Under an `$if`, the
    /// callback of the branch in force when it is called; [`Props::handler_on`] says when a
    /// branch holds one.
    pub fn handler<T: Arg>(&self, key: &str) -> Option<Callback<T>> {
        let raw = self.raw(key)?;
        let Some(cell) = raw.get("$if").and_then(Value::as_str).and_then(cell_named) else {
            return callback_of(raw);
        };
        let then = raw.get("then").and_then(callback_of::<T>);
        let otherwise = raw.get("else").and_then(callback_of::<T>);
        then.or(otherwise)?;
        Some(Callback::new(move |arg: T| {
            let cb = if cell.with_untracked(truthy) {
                then
            } else {
                otherwise
            };
            if let Some(cb) = cb {
                cb.run(arg);
            }
        }))
    }

    pub fn str(&self, key: &str) -> Option<String> {
        match self.take(key)? {
            Value::String(s) => Some(s),
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
            Value::Bool(b) => Some(b),
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
        self.take(key)
    }

    /// A React style object.
    pub fn css(&self, key: &str) -> Css {
        self.take(key).map(|v| style_object(&v)).unwrap_or_default()
    }

    /// A slot of text or elements: a string or a number is text, anything else is drawn.
    pub fn content(&self, key: &str) -> Option<crate::ds::Content> {
        match self.take(key)? {
            Value::String(s) => Some(s.into()),
            Value::Number(n) => Some(js_number(n.as_f64().unwrap_or(0.0)).into()),
            v => Some((move || node(&v)).into()),
        }
    }

    /// Elements, rendered when the component calls for them. A cell among them draws its
    /// value and follows its writes, as React draws its owner's state.
    pub fn children(&self, key: &str) -> Option<Children> {
        let v = self.tree(key)?;
        Some(Box::new(move || node(&v)))
    }

    /// The prop as a tree of nodes, its cells unread: [`node`] draws them live.
    pub fn tree(&self, key: &str) -> Option<Value> {
        self.raw(key).cloned()
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
