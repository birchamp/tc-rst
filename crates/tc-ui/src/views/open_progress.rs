//! The port of tC4's `OpenProgress.jsx` (S14, unit B3): nothing for the first 300 ms of an
//! open, then a dialog with the stage and a bar, determinate while the saved changes are read
//! and indeterminate for the stages with no count. Its own timer shows it, whether or not the
//! server has reported (A-22); a failed or superseded open clears it, so it never stays up.

use std::time::Duration;

use leptos::prelude::*;

use crate::app::understand::Opening;
use crate::app::web::use_app;
use crate::ds::{
    Attrs, Corner, Direction, Dismiss, Fill, Inset, LayerPlacement, Level, Progress, Scrim,
    ScrimToken, Shadow, Stack, Surface, Text, TextRole,
};
use crate::i18n::{keys, t};

/// tC4's `OpenProgress`: shown 300 ms after the open was asked for.
#[component]
pub fn OpenProgress() -> impl IntoView {
    let Some(ctx) = use_app() else {
        return ().into_any();
    };
    let opening = ctx.s.u.opening.clone();
    let started = Memo::new({
        let opening = opening.clone();
        move |_| opening.with(|o| o.as_ref().map(|o| o.started_at))
    });
    let shown = RwSignal::new(false);
    Effect::new(move |_| {
        let Some(started_at) = started.get() else {
            shown.set(false);
            return;
        };
        let wait = Opening::new(started_at).wait(crate::app::web::now_ms());
        if wait <= 0.0 {
            shown.set(true);
            return;
        }
        shown.set(false);
        let handle = set_timeout_with_handle(
            move || {
                shown.try_set(true);
            },
            Duration::from_millis(wait.ceil() as u64),
        );
        if let Ok(handle) = handle {
            on_cleanup(move || handle.clear());
        }
    });
    let visible = Memo::new(move |_| shown.get() && started.get().is_some());
    (move || visible.get().then(|| overlay(opening.clone()))).into_any()
}

fn overlay(opening: ArcRwSignal<Option<Opening>>) -> AnyView {
    let caption = {
        let opening = opening.clone();
        Signal::derive(move || {
            opening.with(|o| o.as_ref().map(Opening::caption).unwrap_or_default())
        })
    };
    let stage_caption = {
        let opening = opening.clone();
        Signal::derive(move || {
            opening.with(|o| {
                o.as_ref()
                    .map(|o| t(o.stage.caption_key(), &[]))
                    .unwrap_or_default()
            })
        })
    };
    let percent = {
        let opening = opening.clone();
        Signal::derive(move || {
            opening.with(|o| o.as_ref().and_then(Opening::percent).map(f64::from))
        })
    };
    let stage = {
        let opening = opening.clone();
        Signal::derive(move || opening.with(|o| o.as_ref().map(|o| o.stage.as_str().to_owned())))
    };
    let content: ChildrenFn = std::sync::Arc::new(move || {
        let aria = Signal::derive(move || stage_caption.get());
        view! {
            <Surface
                fill=Fill::Card
                radius=Corner::X2l
                elevation=Shadow::Modal
                pad=Inset::Lg
                style=crate::css! { "width": 420, "max-width": "100%" }
            >
                <Stack direction=Direction::Column gap=12>
                    <Text role=TextRole::H3>{move || t(keys::open::TITLE, &[])}</Text>
                    <Text
                        role=TextRole::Caption
                        attrs=Attrs::from([("aria-live", "polite"), ("data-testid", "open-progress-stage")])
                    >
                        {move || caption.get()}
                    </Text>
                    {move || {
                        let label = aria.get();
                        view! {
                            <Progress value=percent height=6.0 attrs=Attrs::from([("aria-label", label.as_str())]) />
                        }
                    }}
                </Stack>
            </Surface>
        }
        .into_any()
    });
    crate::ds::Layer(crate::ds::LayerProps {
        level: Level::Overlay,
        scrim: Some(Scrim::Token(ScrimToken::Modal)),
        placement: LayerPlacement::Center,
        role: Some("dialog".to_owned()),
        label: Some(Signal::derive(|| t(keys::open::TITLE, &[]))),
        dismiss: Dismiss::NONE,
        animate: false,
        trap_focus: true,
        // Nothing inside takes the focus, so Layer's trap focuses its panel; Tab is kept there
        // until the open ends, as tC4's `onKeyDown` does.
        on_key_down: Some(Callback::new(|e: web_sys::KeyboardEvent| {
            if e.key() == "Tab" {
                e.prevent_default();
            }
        })),
        scrim_attrs: Attrs::from([("data-testid", "open-progress")]),
        scrim_live: vec![("data-stage", stage)],
        ..crate::ds::LayerProps::new(content)
    })
    .into_any()
}
