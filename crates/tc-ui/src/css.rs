//! Inline styles with React's semantics.
//!
//! tC4's design system draws almost everything with inline `style={{…}}` objects built by
//! spreading one object into another. React writes the properties in the object's key order,
//! one assignment each, so a later shorthand resets the longhands written before it and a later
//! longhand overrides its shorthand. A spread that repeats a key keeps the key at its first
//! position with the new value, and a key whose value is `undefined` still holds its position.
//!
//! [`Css`] keeps that order. It serialises to one declaration list, in key order, and the
//! browser's cascade inside a declaration block gives the same result as React's assignments,
//! so a component can be ported spread for spread.

use std::borrow::Cow;
use std::fmt::Write as _;

/// A value of one inline-style property, before it is written.
#[derive(Clone, Debug, PartialEq)]
pub enum CssValue {
    /// Written as is.
    Text(Cow<'static, str>),
    /// A JavaScript number. React writes `px` after it unless the property is unitless,
    /// custom, or the number is zero.
    Number(f64),
}

impl From<&'static str> for CssValue {
    fn from(v: &'static str) -> Self {
        Self::Text(Cow::Borrowed(v))
    }
}
impl From<String> for CssValue {
    fn from(v: String) -> Self {
        Self::Text(Cow::Owned(v))
    }
}
impl From<Cow<'static, str>> for CssValue {
    fn from(v: Cow<'static, str>) -> Self {
        Self::Text(v)
    }
}
impl From<f64> for CssValue {
    fn from(v: f64) -> Self {
        Self::Number(v)
    }
}
impl From<i32> for CssValue {
    fn from(v: i32) -> Self {
        Self::Number(f64::from(v))
    }
}
impl From<u32> for CssValue {
    fn from(v: u32) -> Self {
        Self::Number(f64::from(v))
    }
}

/// An ordered set of inline-style properties: a React style object.
///
/// Keys are CSS property names (`padding-inline`, `--radius-inner`), not React's camelCase.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Css(Vec<(Cow<'static, str>, Option<CssValue>)>);

impl Css {
    pub fn new() -> Self {
        Self::default()
    }

    /// `{ ...this, [key]: value }`.
    pub fn set(&mut self, key: impl Into<Cow<'static, str>>, value: impl Into<CssValue>) {
        self.put(key.into(), Some(value.into()));
    }

    /// `{ ...this, [key]: value }` where `None` is `undefined`: React writes nothing for it,
    /// but the key keeps its place for a later spread.
    pub fn set_opt<V: Into<CssValue>>(
        &mut self,
        key: impl Into<Cow<'static, str>>,
        value: Option<V>,
    ) {
        self.put(key.into(), value.map(Into::into));
    }

    fn put(&mut self, key: Cow<'static, str>, value: Option<CssValue>) {
        match self.0.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => self.0.push((key, value)),
        }
    }

    /// [`Css::set`], by value.
    #[must_use]
    pub fn with(mut self, key: impl Into<Cow<'static, str>>, value: impl Into<CssValue>) -> Self {
        self.set(key, value);
        self
    }

    /// `{ ...this, ...other }`.
    pub fn extend(&mut self, other: &Css) {
        for (k, v) in &other.0 {
            match self.0.iter_mut().find(|(key, _)| key == k) {
                Some(slot) => slot.1 = v.clone(),
                None => self.0.push((k.clone(), v.clone())),
            }
        }
    }

    /// `{ ...this, ...other }`, by value.
    #[must_use]
    pub fn merge(mut self, other: &Css) -> Self {
        self.extend(other);
        self
    }

    /// The value React would write for `key`, if any.
    pub fn get(&self, key: &str) -> Option<String> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(k, v)| v.as_ref().map(|v| written(k, v)))
    }

    /// Whether the object has the key, even with an `undefined` value.
    pub fn has(&self, key: &str) -> bool {
        self.0.iter().any(|(k, _)| k == key)
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|(_, v)| v.is_none())
    }

    /// The declarations React would write, in order, as a `style` attribute.
    pub fn to_style(&self) -> String {
        let mut out = String::new();
        for (k, v) in &self.0 {
            let Some(v) = v else { continue };
            let value = written(k, v);
            if value.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.push(' ');
            }
            let _ = write!(out, "{k}: {value};");
        }
        out
    }
}

/// `dangerousStyleValue` in react-dom: a number gets `px` unless it is zero, the property is
/// unitless, or it is a custom property.
fn written(key: &str, value: &CssValue) -> String {
    match value {
        CssValue::Text(t) => t.trim().to_string(),
        CssValue::Number(n) => {
            if *n == 0.0 || key.starts_with("--") || is_unitless(key) {
                js_number(*n)
            } else {
                format!("{}px", js_number(*n))
            }
        }
    }
}

/// A number as JavaScript's `String(n)` writes it, for the values styles use.
pub fn js_number(n: f64) -> String {
    if n.is_finite() && n == n.trunc() && n.abs() < 1e15 {
        // Integral: no decimal point, and no "-0".
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// react-dom's `isUnitlessNumber`, as CSS property names, with its vendor-prefixed forms.
fn is_unitless(key: &str) -> bool {
    const UNITLESS: &[&str] = &[
        "animation-iteration-count",
        "aspect-ratio",
        "border-image-outset",
        "border-image-slice",
        "border-image-width",
        "box-flex",
        "box-flex-group",
        "box-ordinal-group",
        "column-count",
        "columns",
        "flex",
        "flex-grow",
        "flex-positive",
        "flex-shrink",
        "flex-negative",
        "flex-order",
        "grid-area",
        "grid-row",
        "grid-row-end",
        "grid-row-span",
        "grid-row-start",
        "grid-column",
        "grid-column-end",
        "grid-column-span",
        "grid-column-start",
        "font-weight",
        "line-clamp",
        "line-height",
        "opacity",
        "order",
        "orphans",
        "tab-size",
        "widows",
        "z-index",
        "zoom",
        "fill-opacity",
        "flood-opacity",
        "stop-opacity",
        "stroke-dasharray",
        "stroke-dashoffset",
        "stroke-miterlimit",
        "stroke-opacity",
        "stroke-width",
    ];
    let bare = ["-webkit-", "-moz-", "-ms-", "-o-"]
        .iter()
        .find_map(|p| key.strip_prefix(p))
        .unwrap_or(key);
    UNITLESS.contains(&bare)
}

/// A React style key (`paddingInline`, `WebkitBoxDecorationBreak`, `--tone`) as a CSS property
/// name, as react-dom's `hyphenateStyleName` writes it.
pub fn css_property(react_key: &str) -> String {
    if react_key.starts_with("--") {
        return react_key.to_string();
    }
    let mut out = String::with_capacity(react_key.len() + 4);
    for c in react_key.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    if out.starts_with("ms-") {
        out.insert(0, '-');
    }
    out
}

/// `css! { "display": "flex", "gap": 8 }` builds a [`Css`] in key order. A value may be an
/// `Option`, which is `undefined` when `None`.
#[macro_export]
macro_rules! css {
    ($($k:literal : $v:expr),* $(,)?) => {{
        #[allow(unused_mut, reason = "a css! with no pairs never sets a key")]
        let mut c = $crate::css::Css::new();
        $( $crate::css::CssSet::set_into($v, &mut c, $k); )*
        c
    }};
}

/// What `css!` accepts as a value: anything that converts into [`CssValue`], or an `Option`
/// of it.
pub trait CssSet {
    fn set_into(self, css: &mut Css, key: &'static str);
}

impl<T: Into<CssValue>> CssSet for T {
    fn set_into(self, css: &mut Css, key: &'static str) {
        css.set(key, self);
    }
}

impl<T: Into<CssValue>> CssSet for Option<T> {
    fn set_into(self, css: &mut Css, key: &'static str) {
        css.set_opt(key, self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_get_px_except_unitless_custom_and_zero() {
        let c = css! {
            "width": 44,
            "line-height": 1.2,
            "padding": 0,
            "opacity": 0.6,
            "--radius-inner": 4,
            "flex": 1,
            "z-index": 3,
            "margin-top": 1.5,
        };
        assert_eq!(
            c.to_style(),
            "width: 44px; line-height: 1.2; padding: 0; opacity: 0.6; --radius-inner: 4; flex: 1; z-index: 3; margin-top: 1.5px;"
        );
    }

    #[test]
    fn a_spread_keeps_the_first_position_of_a_repeated_key() {
        // { padding: 0, paddingInline: 10, ...{ padding: '5px' } } writes padding first, so the
        // inline padding stays 10px, exactly as React's assignments leave it.
        let base = css! { "padding": 0, "padding-inline": 10 };
        let merged = base.merge(&css! { "padding": "5px" });
        assert_eq!(merged.to_style(), "padding: 5px; padding-inline: 10px;");
    }

    #[test]
    fn undefined_holds_its_place_and_writes_nothing() {
        let mut c = Css::new();
        c.set_opt::<&str>("padding", None);
        c.set("color", "red");
        assert_eq!(c.to_style(), "color: red;");
        let c = c.merge(&css! { "font": "inherit", "padding": "10px 12px" });
        assert_eq!(
            c.to_style(),
            "padding: 10px 12px; color: red; font: inherit;"
        );
        assert!(c.has("padding"));
    }

    #[test]
    fn empty_strings_are_not_written() {
        let c = css! { "color": "", "display": "block" };
        assert_eq!(c.to_style(), "display: block;");
        assert_eq!(c.get("color"), Some(String::new()));
    }

    #[test]
    fn react_keys_become_css_properties() {
        assert_eq!(css_property("paddingInline"), "padding-inline");
        assert_eq!(
            css_property("WebkitBoxDecorationBreak"),
            "-webkit-box-decoration-break"
        );
        assert_eq!(css_property("msTransform"), "-ms-transform");
        assert_eq!(css_property("--tone"), "--tone");
        assert_eq!(css_property("float"), "float");
    }

    #[test]
    fn numbers_print_like_javascript() {
        assert_eq!(js_number(1.0), "1");
        assert_eq!(js_number(-0.0), "0");
        assert_eq!(js_number(0.6), "0.6");
        assert_eq!(js_number(15.48), "15.48");
        assert_eq!(js_number(17.0 / 2.0), "8.5");
    }

    #[test]
    fn prefixed_unitless_properties_stay_unitless() {
        let c = css! { "-webkit-line-clamp": 2, "-webkit-flex": 1 };
        assert_eq!(c.to_style(), "-webkit-line-clamp: 2; -webkit-flex: 1;");
    }
}
