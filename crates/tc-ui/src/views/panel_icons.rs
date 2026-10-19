//! The port of tC4's `PanelIcons.jsx` (unit B3): the panel toggles' sidebar glyph, with the
//! divider on the side the panel lives on.

use leptos::prelude::*;

use crate::ds::fixed;

fn frame(x: &'static str) -> AnyView {
    use leptos::svg;
    svg::svg()
        .add_any_attr(fixed(&[
            ("width", "16"),
            ("height", "16"),
            ("viewBox", "0 0 16 16"),
            ("fill", "none"),
            ("aria-hidden", "true"),
        ]))
        .child((
            svg::rect().add_any_attr(fixed(&[
                ("x", "1.5"),
                ("y", "2.5"),
                ("width", "13"),
                ("height", "11"),
                ("rx", "2"),
                ("stroke", "currentColor"),
                ("stroke-width", "1.5"),
            ])),
            svg::line().add_any_attr(fixed(&[
                ("x1", x),
                ("y1", "3"),
                ("x2", x),
                ("y2", "13"),
                ("stroke", "currentColor"),
                ("stroke-width", "1.5"),
            ])),
        ))
        .into_any()
}

/// The book rail's toggle.
pub fn rail_icon() -> AnyView {
    frame("6")
}

/// The helps panel's toggle.
pub fn helps_icon() -> AnyView {
    frame("10")
}
