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
//! `Signal`s, so a plain value or a signal both work, and a change restyles the nodes already
//! drawn rather than drawing new ones, as React does. A text slot that tC4's screens also fill
//! with elements (a title, a subtitle, a description) takes [`Content`]: a string or a view.
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
use crate::keyed::{self, Slots};
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

/// A slot React renders as a node that call sites mostly fill with text: a title, a subtitle,
/// a description. tC4's screens also pass elements there (`title={<span dir="rtl">…</span>}`,
/// `subtitle={<>To <strong>…</strong></>}`), so a closure that draws a view converts too, and
/// `title="Settings"` still works.
#[derive(Clone)]
pub enum Content {
    Text(String),
    View(ViewFn),
}

impl Content {
    /// React's truthiness: an empty string draws nothing, elements always draw.
    pub fn is_empty(&self) -> bool {
        matches!(self, Content::Text(s) if s.is_empty())
    }

    /// The text, when the content is text (Drawer names its dialog by a text title only).
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Content::Text(s) => Some(s),
            Content::View(_) => None,
        }
    }

    pub fn view(&self) -> AnyView {
        match self {
            Content::Text(s) => s.clone().into_any(),
            Content::View(f) => f.run(),
        }
    }
}

impl From<&str> for Content {
    fn from(s: &str) -> Self {
        Content::Text(s.to_string())
    }
}

impl From<String> for Content {
    fn from(s: String) -> Self {
        Content::Text(s)
    }
}

impl<F, V> From<F> for Content
where
    F: Fn() -> V + Send + Sync + 'static,
    V: IntoView + 'static,
{
    fn from(f: F) -> Self {
        Content::View(ViewFn::from(move || f().into_any()))
    }
}

/// `{x ? <Wrap>{x}</Wrap> : null}`: the slot's content when it draws anything.
pub(crate) fn present(c: Option<Content>) -> Option<Content> {
    c.filter(|c| !c.is_empty())
}

/// `XProps::default()`: every prop at the component's own default (its `#[prop]`
/// attributes, through the builder), so a shim that composes a primitive names only the props
/// it sets and ends with `..XProps::default()`.
macro_rules! props_default {
    ($($props:ident),* $(,)?) => {
        $(impl Default for $props {
            fn default() -> Self {
                Self::builder().build()
            }
        })*
    };
}

props_default!(
    ActionProps,
    AvatarProps,
    ChoiceProps,
    FieldGroupProps,
    FieldProps,
    InputProps,
    KeyValueGridProps,
    ModalProps,
    ProgressBarProps,
    ProgressProps,
    RuleProps,
    ScripturePassageProps,
    SearchFieldProps,
    SpinnerProps,
    StackProps,
    StatusDotProps,
    SurfaceProps,
    TextAreaProps,
    TextProps,
);

impl VerseMarkerProps {
    /// The marker of verse `n` with every other prop at its default.
    pub fn new(n: u32) -> Self {
        Self::builder().n(n).build()
    }
}

impl LayerProps {
    /// A Layer of `children` with every other prop at its default.
    pub fn new(children: ChildrenFn) -> Self {
        Self::builder().children(children).build()
    }
}

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

/// React's keyed list: one child per slot of `slots`, drawn once and kept while its slot stays
/// in the list. `child` gets its slot and its position, which follows the list, so a child
/// reads its own data by position and never another child's that shares its key.
pub(crate) fn keyed_list<V: IntoView + 'static>(
    slots: Memo<Slots>,
    child: impl Fn(keyed::Slot, Memo<usize>) -> V + Clone + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <For
            each=move || {
                refocus_after_moves();
                slots.with(|s| s.order().to_vec())
            }
            key=Clone::clone
            children=move |slot: keyed::Slot| {
                // For draws a child for a slot of the list it just read, so the slot is there;
                // once it leaves the list, the child keeps its last position until it goes.
                let first = slots.with_untracked(|s| s.index(&slot)).unwrap_or_default();
                let at = {
                    let slot = slot.clone();
                    Memo::new(move |prev: Option<&usize>| {
                        slots
                            .with(|s| s.index(&slot))
                            .or(prev.copied())
                            .unwrap_or(first)
                    })
                };
                child(slot, at)
            }
        />
    }
}

/// React focuses again, after every commit, the element that had the focus before it, when the
/// commit's moves dropped it (`restoreSelection`). A browser blurs the node that `insertBefore`
/// moves, and For moves a kept child that way, so a focused row that a re-sort moves would lose
/// the focus. Called as For reads its list: For applies the update in the same run, so a
/// microtask queued here runs once the update is in. The element that had the focus then gets it
/// back if it is still on the page and the focus fell to the body; focus that something else took
/// meanwhile stays where it is. A text control keeps its caret across the move, so focusing it
/// again is enough.
fn refocus_after_moves() {
    use wasm_bindgen::JsCast;

    let doc = document();
    let Some(prior) = doc.active_element() else {
        return;
    };
    if doc.body().is_some_and(|b| prior == *b) {
        return;
    }
    leptos::tachys::renderer::dom::queue_microtask(move || {
        let doc = document();
        let dropped = match (doc.active_element(), doc.body()) {
            (None, _) => true,
            (Some(now), Some(body)) => now == *body,
            (Some(_), None) => false,
        };
        if !dropped || !prior.is_connected() {
            return;
        }
        if let Ok(el) = prior.dyn_into::<web_sys::HtmlElement>() {
            let options = web_sys::FocusOptions::new();
            // React puts back every ancestor's scroll position after it focuses; not scrolling
            // at all is the same.
            options.set_prevent_scroll(true);
            let _ = el.focus_with_options(&options);
        }
    });
}
