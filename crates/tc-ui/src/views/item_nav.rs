//! The port of tC4's `ItemNav.jsx` (#726): the Previous and Next buttons of the Check and Align
//! marking rows. Labelled quiet pills, so they read as "move", apart from the marking buttons
//! that record a decision. Each sits 4 px further from the marking buttons than the row gap,
//! and its arrow follows the interface direction.

use leptos::prelude::*;

use crate::attrs::Attrs;
use crate::css::Css;
use crate::ds::{Button, ButtonVariant, Size};
use crate::i18n::{html_lang_dir, keys, shown_locale, t};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    Prev,
    Next,
}

/// `ItemNavButton`: `testid` is the test id (`align-prev`, `check-next`).
pub fn item_nav_button(
    which: Which,
    enabled: bool,
    testid: &'static str,
    on_click: Callback<()>,
) -> impl IntoView {
    let rtl = html_lang_dir(shown_locale()).1 == "rtl";
    let prev = which == Which::Prev;
    let arrow = if prev == rtl { '→' } else { '←' };
    let label = t(
        if prev {
            keys::check::PREV_LABEL
        } else {
            keys::check::NEXT_LABEL
        },
        &[],
    );
    let title = t(
        if prev {
            keys::check::PREV_ITEM
        } else {
            keys::check::NEXT_ITEM
        },
        &[],
    );
    let text = if prev {
        format!("{arrow} {label}")
    } else {
        format!("{label} {arrow}")
    };
    let mut style = Css::new();
    style.set(
        if prev {
            "margin-inline-end"
        } else {
            "margin-inline-start"
        },
        4,
    );
    if enabled {
        style.set("color", "var(--accent)");
    }
    view! {
        <Button
            variant=ButtonVariant::Secondary
            size=Size::Sm
            attrs=Attrs::from([("data-testid", testid)])
            title=Some(title)
            disabled=!enabled
            on_click=Callback::new(move |_| {
                if enabled {
                    on_click.run(());
                }
            })
            style=style
        >
            {text}
        </Button>
    }
}
