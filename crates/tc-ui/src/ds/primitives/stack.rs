//! Stack.jsx: flex or grid arrangement with a real `gap`.

use leptos::ev::MouseEvent;
use leptos::prelude::*;

use crate::css::CssValue;
use crate::ds::{Align, Attrs, Columns, Css, Direction, Justify, Tag, element, spread};

/// The inline style Stack.jsx writes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn stack_css(
    direction: Direction,
    gap: CssValue,
    align: Option<Align>,
    justify: Option<Justify>,
    wrap: bool,
    inline: bool,
    columns: Option<&Columns>,
    flex: Option<CssValue>,
    grow: bool,
    style: &Css,
) -> Css {
    let mut s = Css::new();
    s.set(
        "display",
        if columns.is_some() {
            "grid"
        } else if inline {
            "inline-flex"
        } else {
            "flex"
        },
    );
    match columns {
        Some(Columns::Count(n)) => s.set(
            "grid-template-columns",
            format!("repeat({n},minmax(0,1fr))"),
        ),
        Some(Columns::Template(t)) => s.set("grid-template-columns", t.clone()),
        None => s.set("flex-direction", direction.as_str()),
    }
    s.set("gap", gap);
    s.set_opt("align-items", align.map(Align::flex));
    s.set_opt("justify-content", justify.map(Justify::flex));
    s.set_opt("flex-wrap", wrap.then_some("wrap"));
    s.set_opt(
        "flex",
        flex.or_else(|| grow.then_some(CssValue::Number(1.0))),
    );
    s.set("min-width", 0);
    s.extend(style);
    s
}

/// Flex or grid arrangement with a real `gap`. Sibling groups are laid out with gap, never
/// with per-element margins, so reordering or deleting a child cannot change the spacing.
#[component]
pub fn Stack(
    #[prop(optional)] direction: Direction,
    /// px when a number (the default 0), or any CSS gap.
    #[prop(optional, into)]
    gap: Option<CssValue>,
    #[prop(optional)] align: Option<Align>,
    #[prop(optional)] justify: Option<Justify>,
    #[prop(optional)] wrap: bool,
    #[prop(optional)] inline: bool,
    #[prop(optional)] columns: Option<Columns>,
    #[prop(optional, into)] flex: Option<CssValue>,
    #[prop(optional)] grow: bool,
    #[prop(optional, into)] tag: Option<Tag>,
    #[prop(optional)] on_click: Option<Callback<MouseEvent>>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let s = stack_css(
        direction,
        gap.unwrap_or(CssValue::Number(0.0)),
        align,
        justify,
        wrap,
        inline,
        columns.as_ref(),
        flex,
        grow,
        &style,
    );
    element(tag.unwrap_or("div".into()))
        .attr("style", s.to_style())
        .add_any_attr(spread(&attrs))
        .on(leptos::ev::click, move |e| {
            if let Some(cb) = on_click {
                cb.run(e);
            }
        })
        .child(children.map(|c| c()))
}
