//! The port of tC4's `BookRail.jsx` (unit B3): the project's books, the active one with its
//! chapter grid, every row with its drafted share and the share's tooltip (#572, #603). An
//! open editing card turns the rail off (#238), which Translate's cards set (B4a).

use leptos::prelude::*;
use tc_api::editor::BookView;
use tc_api::shown::Known;
use tc_core::BookId;

use super::rail_chrome::{rail_group, rail_number_button, rail_numbers, rail_shell};
use crate::app::web::{Actions, AppState};
use crate::attrs::Attrs;
use crate::ds::{BookTile, TileLayout};
use crate::i18n::{keys, t};

/// tC4's `BookRail` over the open book's read.
pub fn book_rail(s: &AppState, actions: &Actions, book: &BookView) -> AnyView {
    let count = book.rail.len();
    let title = Signal::derive(move || format!("{} · {count}", t(keys::draft::BOOKS, &[])));
    let editing = s.editing.clone();
    let off = Signal::derive(move || editing.with(|e| e.is_open()));
    // tC4's `off`: a tile that is off shows no hand.
    let cursor = Signal::derive(move || {
        if off.get() {
            crate::css! { "cursor": "default" }
        } else {
            crate::css::Css::new()
        }
    });
    let chapter = s.u.chapter.clone();
    let rows = book
        .rail
        .iter()
        .map(|row| {
            let code: BookId = row.book;
            let active = code == book.book;
            let pct = match row.drafted {
                Known::Known(n) => Some(n),
                Known::Unknown => None,
            };
            let open = {
                let actions = actions.clone();
                Callback::new(move |_: web_sys::Event| actions.open_book(code))
            };
            // tC4 #603: the share's tooltip, only when the share is known.
            let tile = match pct {
                Some(n) => view! {
                    <BookTile
                        layout=TileLayout::Row
                        active=active
                        name=code.english_name().to_owned()
                        percent=f64::from(n)
                        meta=Some(format!("{n}%"))
                        meta_title=t(keys::draft::BOOK_PERCENT_TITLE, &[("n", &n.to_string())])
                        disabled=off
                        style=cursor
                        on_click=open
                    />
                }
                .into_any(),
                None => view! {
                    <BookTile
                        layout=TileLayout::Row
                        active=active
                        name=code.english_name().to_owned()
                        percent=0.0
                        meta=Some(String::new())
                        disabled=off
                        style=cursor
                        on_click=open
                    />
                }
                .into_any(),
            };
            let numbers = active.then(|| {
                let buttons = book
                    .chapters
                    .iter()
                    .map(|&c| {
                        let chapter = chapter.clone();
                        let actions = actions.clone();
                        rail_number_button(
                            Signal::derive(move || chapter.get() == c),
                            off,
                            Callback::new(move |()| actions.set_chapter(c)),
                            Attrs::default(),
                            c.to_string(),
                        )
                    })
                    .collect_view();
                rail_numbers(buttons.into_any())
            });
            rail_group(Signal::from(active), view! { {tile} {numbers} }.into_any())
        })
        .collect_view();
    rail_shell(title, rows.into_any()).into_any()
}
