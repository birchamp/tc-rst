//! The port of tC4's `StoryRail.jsx` (unit B7): story navigation in the book rail's chrome
//! (#330). One row per story with its title and drafted share (Home's summary, #328); the active
//! story's row and its frame grid share one tint, the frames are number buttons with the frame
//! in view filled and a drafted frame tinted. The list is the project's story catalogue; the
//! frame grid describes the open story only.

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::ids::StoryNo;
use tc_api::shown::Known;
use tc_api::story::{StoryList, StoryView};

use super::rail_chrome::{rail_group, rail_number_button, rail_numbers, rail_shell};
use crate::app::story::{FrameMarker, frame_markers, row_name};
use crate::app::web::{Actions, AppState};
use crate::attrs::Attrs;
use crate::ds::{BookTile, TileLayout};
use crate::i18n::{keys, t};

/// tC4's `StoryRail`: `open` is the open story once it is read; `current_frame` the frame in
/// view.
pub fn story_rail(
    s: &AppState,
    actions: &Actions,
    list: &StoryList,
    active: StoryNo,
    open: Option<Arc<StoryView>>,
    on_frame: Callback<u16>,
) -> AnyView {
    let count = list.stories.len();
    let title = Signal::derive(move || format!("{} · {count}", t(keys::story_draft::STORIES, &[])));
    let dir = match s.project.with_untracked(|p| p.as_ref().map(|p| p.dir)) {
        Some(tc_api::shown::Direction::Rtl) => "rtl",
        _ => "ltr",
    };
    let frame = s.u.story_frame.clone();
    let rows = list
        .stories
        .iter()
        .map(|row| {
            let number = row.story;
            let selected = number == active;
            let name = row_name(number, open.as_deref(), Some(row));
            let (percent, meta) = match row.drafted {
                Known::Known(n) => (f64::from(n), format!("{n}%")),
                Known::Unknown => (0.0, String::new()),
            };
            let select = {
                let actions = actions.clone();
                Callback::new(move |_: web_sys::Event| actions.open_story(number))
            };
            let mut attrs =
                Attrs::from([("data-testid", format!("story-{}", number.get()).as_str())]);
            attrs.set("dir", dir);
            if selected {
                attrs.set("aria-current", "page");
            }
            let tile = view! {
                <BookTile
                    layout=TileLayout::Row
                    active=selected
                    name=name
                    percent=percent
                    meta=Some(meta)
                    attrs=attrs
                    on_click=select
                />
            };
            let numbers = selected
                .then(|| {
                    open.as_ref().map(|story| {
                        let buttons = frame_markers(story)
                            .into_iter()
                            .map(|marker| frame_button(marker, frame.clone(), on_frame))
                            .collect_view();
                        rail_numbers(
                            Attrs::from([("data-testid", "story-frame-markers")]),
                            buttons.into_any(),
                        )
                    })
                })
                .flatten();
            rail_group(
                Signal::from(selected),
                view! { {tile} {numbers} }.into_any(),
            )
        })
        .collect_view();
    rail_shell(
        title,
        Attrs::from([("data-testid", "story-rail")]),
        rows.into_any(),
    )
    .into_any()
}

/// A frame's number button: the frame in view is filled; a drafted one is tinted.
fn frame_button(
    marker: FrameMarker,
    focus: leptos::prelude::ArcRwSignal<Option<u16>>,
    on_frame: Callback<u16>,
) -> impl IntoView + use<> {
    let FrameMarker { frame: n, drafted } = marker;
    let tip = t(
        if drafted {
            keys::story_draft::FRAME_DRAFTED
        } else {
            keys::story_draft::FRAME_EMPTY
        },
        &[("n", &n.to_string())],
    );
    let mut attrs = Attrs::from([
        ("data-testid", format!("frame-marker-{n}").as_str()),
        ("data-drafted", if drafted { "true" } else { "false" }),
    ]);
    attrs.set("title", tip);
    rail_number_button(
        Signal::derive(move || focus.get() == Some(n)),
        Signal::from(drafted),
        Signal::from(false),
        Callback::new(move |()| on_frame.run(n)),
        attrs,
        n.to_string(),
    )
}
