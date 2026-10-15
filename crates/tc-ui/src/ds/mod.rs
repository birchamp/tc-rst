//! tC4's design system as Leptos components.
//!
//! One component per React component of tC4's `src/ds/components`, with the same name and
//! the same props, rendering the same DOM: tags, `data-*` hooks, inline styles in the same
//! key order, ARIA attributes and test ids. tC4's components carry no stylesheet of their own;
//! their look is the inline styles plus the token CSS (`assets/ds`), which keys on those
//! attributes, so the same DOM draws the same pixels.
//!
//! Naming: a React prop `fooBar` is `foo_bar`; `as` is `tag`; `...rest` is `attrs` (extra
//! attributes, placed on the element React spreads them onto); `style` is a [`Css`]. Props
//! that change while a component is on screen (`checked`, `selected`, `value`, `open`, …) are
//! `Signal`s, so a plain value or a signal both work.
//!
//! Layers render in place, as they do in tC4, where `window.ReactDOM` is unset and Layer's
//! portal falls back to the component's own position (fixed positioning still escapes flow).

use std::borrow::Cow;

use leptos::prelude::*;
use leptos::tachys::html::attribute::any_attribute::{AnyAttribute, IntoAnyAttribute};
use leptos::tachys::html::attribute::custom::custom_attribute;
use leptos::tachys::html::element::{Custom, HtmlElement};
use leptos::tachys::reactive_graph::OwnedView;

pub use crate::attrs::Attrs;
pub use crate::css::Css;
pub use crate::tokens::*;

pub mod core;
pub mod data;
pub mod feedback;
pub mod forms;
pub mod navigation;
pub mod primitives;
pub mod surfaces;
pub mod translation;

pub use self::core::*;
pub use data::*;
pub use feedback::*;
pub use forms::*;
pub use navigation::*;
pub use primitives::*;
pub use surfaces::*;
pub use translation::*;

/// A tag name given as a prop (React's `as`).
pub type Tag = Cow<'static, str>;

/// A slot that takes elements (React's `footer`, `trigger`, `leading`, …), rendered once.
pub type Slot = Children;

/// A slot whose content may come and go while the component is on screen (React's
/// `trailing={value && onClear ? <Clear/> : null}`): `None` draws nothing, not even the slot's
/// wrapper. Called inside a reactive scope.
pub type MaybeView = std::sync::Arc<dyn Fn() -> Option<AnyView> + Send + Sync>;

/// An HTML element whose tag is decided at run time.
pub(crate) fn element(tag: impl Into<Tag>) -> HtmlElement<Custom<Tag>, (), ()> {
    leptos::html::custom(tag.into())
}

/// `{...rest}`: the extra attributes as attributes Leptos can add to an element.
pub(crate) fn spread(attrs: &Attrs) -> Vec<AnyAttribute> {
    attrs
        .iter()
        .map(|(n, v)| custom_attribute(n.to_string(), v.to_string()).into_any_attr())
        .collect()
}

/// Fixed attributes, for elements built without `view!`.
///
/// Static SVG goes through this rather than `view!`: tachys 0.2.19 builds an SVG child whose
/// attributes are all literals from an HTML template wrapped in `<svg><g>…</g></svg>` and
/// mounts the `<g>`, so the DOM would gain a `<g>` that tC4's does not have.
pub(crate) fn fixed(list: &[(&'static str, &'static str)]) -> Vec<AnyAttribute> {
    list.iter()
        .map(|(n, v)| custom_attribute(*n, *v).into_any_attr())
        .collect()
}

/// Attributes whose values follow signals; `None` removes the attribute.
pub(crate) fn live(list: Vec<(&'static str, Signal<Option<String>>)>) -> Vec<AnyAttribute> {
    list.into_iter()
        .map(|(n, v)| custom_attribute(n, move || v.get()).into_any_attr())
        .collect()
}

/// Runs `children` under a child owner that provides `value` as context: React's
/// `<Context.Provider value={value}>`, scoped to those children only.
pub(crate) fn provide<T: Send + Sync + 'static, V: IntoView + 'static>(
    value: T,
    children: impl FnOnce() -> V,
) -> impl IntoView {
    let owner = Owner::current().map_or_else(Owner::new, |o| o.child());
    let view = owner.with(|| {
        provide_context(value);
        children()
    });
    OwnedView::new_with_owner(view, owner)
}

/// The next value of one of the design system's id sequences (`tc-f1`, `tc-g1`, …). React keeps
/// one module-level counter per sequence; ids only need to be unique on the page.
pub(crate) fn next_id(prefix: &'static str) -> String {
    use std::cell::RefCell;
    use std::collections::HashMap;
    thread_local! {
        static SEQ: RefCell<HashMap<&'static str, u32>> = RefCell::new(HashMap::new());
    }
    SEQ.with(|s| {
        let mut s = s.borrow_mut();
        let n = s.entry(prefix).or_insert(0);
        *n += 1;
        format!("{prefix}{n}")
    })
}
