//! The port of tC4's `StoryUnderstand.jsx` (unit B7; #290, J25, amended #332): the open story's
//! frames in order with their picture and the gateway text, a user comment box for each, and the
//! helps panel scoped to the frame in focus. The screen reads `story/list` (the rail),
//! `story/open` (the story) and `helps/story` (the helps); the comment box shows the newest
//! comment of its frame and is read-only until C3 saves it, so nothing typed into it can be
//! lost (a declared difference until then).

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::helps::{PassageView, Story as HelpsStory};
use tc_api::ids::{Scope, StoryNo};
use tc_api::shown::Direction as TextDir;
use tc_api::story::{List, Open, StoryList, StoryView};

use super::helps_panel::{HelpsPanel, HelpsToggleButton, StoryPanel};
use super::panel_icons::rail_icon;
use super::story_rail::story_rail;
use crate::app::load::Load;
use crate::app::story::{StoryUnit, frame_comment, story_label, understand_units};
use crate::app::web::{Actions, AppCtx, AppState, use_app, use_runtime};
use crate::ds::{Callout, CalloutTone, IconButton, Overline, OverlineTone, Status, StatusDot};
use crate::i18n::{keys, t};

const READ_TEXT: &str = "var(--font-scripture)";

/// The story the screen shows, once the page knows the project and its story.
#[derive(Clone, PartialEq)]
struct Shown {
    at: Scope,
    story: StoryNo,
}

/// tC4's `StoryUnderstand`.
#[component]
pub fn StoryUnderstand() -> impl IntoView {
    let (Some(AppCtx { s, actions }), Some(_)) = (use_app(), use_runtime()) else {
        return ().into_any();
    };
    let shown = {
        let (project, chapter) = (s.project.clone(), s.u.chapter.clone());
        Memo::new(move |_| {
            let at = project.with(|p| p.as_ref().map(|p| p.at.clone()))?;
            Some(Shown {
                at,
                story: crate::app::story::story_of(chapter.get())?,
            })
        })
    };
    (move || match shown.get() {
        Some(shown) => screen(&s, &actions, shown).into_any(),
        None => status_line(
            "story-understand-empty",
            t(keys::story_draft::NO_STORY, &[]),
        )
        .into_any(),
    })
    .into_any()
}

fn status_line(test_id: &'static str, text: String) -> impl IntoView {
    let style = crate::css! { "flex": 1, "padding": 40 };
    view! { <main data-testid=test_id style=style.to_style()>{text}</main> }
}

fn screen(s: &AppState, actions: &Actions, shown: Shown) -> AnyView {
    let Some(runtime) = use_runtime() else {
        return ().into_any();
    };
    let list = runtime.cache().read(List {
        at: shown.at.clone(),
    });
    let story = runtime.cache().read(Open {
        at: shown.at.clone(),
        story: shown.story,
    });
    let helps = runtime.cache().read(HelpsStory {
        at: shown.at.clone(),
        story: shown.story,
    });
    // The open is over once the screen has its story, or could not read it (tC4 clears
    // `s.opening` after the story loads).
    {
        let actions = actions.clone();
        Effect::new(move |_| {
            if story.with(|s| !s.is_pending()) {
                actions.book_shown();
            }
        });
    }
    let (s, actions) = (s.clone(), actions.clone());
    (move || {
        story.with(|loaded| match loaded {
            Load::Loading => status_line("story-understand-loading", t(keys::story_draft::LOADING, &[]))
                .into_any(),
            Load::Failed(error) => {
                let message = error.to_string();
                view! {
                    <main data-testid="story-understand-error" style=crate::css! { "flex": 1, "padding": 40 }.to_style()>
                        <Callout tone=CalloutTone::Warn>{message}</Callout>
                    </main>
                }
                .into_any()
            }
            Load::Ready(view) | Load::Refreshing(view) => {
                loaded_screen(&s, &actions, &shown, view.clone(), list, helps).into_any()
            }
        })
    })
    .into_any()
}

fn loaded_screen(
    s: &AppState,
    actions: &Actions,
    shown: &Shown,
    view: Arc<StoryView>,
    list: Signal<Load<Arc<StoryList>>>,
    helps: Signal<Load<Arc<PassageView>>>,
) -> AnyView {
    let frame = s.u.story_frame.clone();
    let focus_frame = {
        let frame = frame.clone();
        Memo::new(move |_| frame.get())
    };
    let root = crate::css! { "flex": 1, "display": "flex", "min-height": 0 };
    let rail = {
        let (s, actions, view, story) = (s.clone(), actions.clone(), view.clone(), shown.story);
        let on = s.u.rail.clone();
        move || {
            on.get().then(|| {
                list.with(|loaded| {
                    loaded.value().map(|rows| {
                        let on_frame = {
                            let actions = actions.clone();
                            Callback::new(move |n: u16| {
                                actions.set_story_frame(n);
                                scroll_to(&format!("story-understand-unit-{n}"));
                            })
                        };
                        story_rail(&s, &actions, rows, story, Some(view.clone()), on_frame)
                    })
                })
            })
        }
    };
    let main = crate::css! {
        "flex": 1,
        "display": "flex",
        "flex-direction": "column",
        "min-width": 0,
        "min-height": 0,
    };
    let scroll = crate::css! {
        "flex": 1,
        "overflow": "auto",
        "min-height": 0,
        "background": "var(--surface-app)",
    };
    let column = crate::css! {
        "max-width": "var(--measure-read)",
        "margin": "0 auto",
        "padding": "22px 26px 60px",
    };
    let dir = match s.project.with_untracked(|p| p.as_ref().map(|p| p.dir)) {
        Some(TextDir::Rtl) => "rtl",
        _ => "ltr",
    };
    let units = understand_units(&view);
    let view_for_units = view.clone();
    let unit_views = {
        let (actions, frame) = (actions.clone(), frame.clone());
        units
            .into_iter()
            .map(|unit| {
                let comment = unit
                    .frame
                    .and_then(|f| frame_comment(&view_for_units.comments, f))
                    .map(|c| c.text.clone());
                story_unit(&actions, &frame, unit, dir, comment)
            })
            .collect_view()
    };
    let panel = StoryPanel {
        focus_frame,
        comments: None,
    };
    view! {
        <div style=root.to_style() data-testid="story-understand">
            {rail}
            <main style=main.to_style()>
                {header(actions, shown.story)}
                <div style=scroll.to_style()>
                    <div style=column.to_style()>{unit_views}</div>
                </div>
            </main>
            <HelpsPanel
                chapter=u16::from(shown.story.get())
                focus_verses=Memo::new(|_| None)
                helps=helps
                pane=String::new()
                story=panel
            />
        </div>
    }
    .into_any()
}

/// Brings a unit into view, as a rail frame button brings its frame (tC4 `scrollIntoView`).
fn scroll_to(test_id: &str) {
    let selector = format!("[data-testid=\"{test_id}\"]");
    if let Some(el) = document().query_selector(&selector).ok().flatten() {
        el.scroll_into_view();
    }
}

fn header(actions: &Actions, story: StoryNo) -> impl IntoView + use<> {
    let bar = crate::css! {
        "display": "flex",
        "align-items": "center",
        "gap": 14,
        "padding": "10px 26px",
        "border-bottom": "var(--stroke-hair) solid var(--border-hair)",
        "background": "var(--surface-card)",
        "flex": "none",
        "min-width": 0,
        "overflow": "hidden",
    };
    let h2 = crate::css! {
        "font-size": "var(--fs-title)",
        "letter-spacing": "var(--track-17)",
        "margin": 0,
        "flex": "none",
    };
    let note = crate::css! {
        "font-size": "var(--fs-caption-lg)",
        "letter-spacing": "var(--track-12-5)",
        "color": "var(--text-tertiary)",
        "flex": 1,
        "min-width": 0,
        "white-space": "nowrap",
        "overflow": "hidden",
        "text-overflow": "ellipsis",
    };
    let toggle = {
        let actions = actions.clone();
        Callback::new(move |_| actions.toggle_rail())
    };
    view! {
        <div style=bar.to_style()>
            <IconButton title=t(keys::draft::TOGGLE_RAIL, &[]) on_click=toggle>{rail_icon()}</IconButton>
            <h2 style=h2.to_style()>{story_label(story)}</h2>
            <span style=note.to_style()>{move || t(keys::understand::STORY_NOTE, &[])}</span>
            <HelpsToggleButton />
        </div>
    }
}

/// One frame of the story (tC4 `StoryUnit`): its picture and the gateway text, a comment box
/// below both.
fn story_unit(
    actions: &Actions,
    focus: &ArcRwSignal<Option<u16>>,
    unit: StoryUnit,
    dir: &'static str,
    comment: Option<String>,
) -> AnyView {
    let n = unit.frame.unwrap_or_default();
    let focused = {
        let focus = focus.clone();
        Memo::new(move |_| focus.get() == Some(n))
    };
    let style = move || {
        let mut style = crate::css! {
            "margin-bottom": 18,
            "border-radius": "var(--radius-xl)",
            "padding": "12px 16px 14px",
            "cursor": "pointer",
        };
        style.extend(&if focused.get() {
            crate::css! {
                "background": "var(--surface-card)",
                "border": "var(--stroke) solid var(--accent-ring)",
                "box-shadow": "0 2px 10px rgba(1,66,99,.07)",
            }
        } else {
            crate::css! {
                "background": "transparent",
                "border": "var(--stroke) solid transparent",
                "box-shadow": "none",
            }
        });
        style.to_style()
    };
    let head = crate::css! {
        "display": "flex",
        "align-items": "center",
        "gap": 10,
        "padding-bottom": 8,
        "border-bottom": "var(--stroke-hair) solid var(--border)",
    };
    let on_focus = {
        let actions = actions.clone();
        move || actions.set_story_frame(n)
    };
    let on_click = {
        let on_focus = on_focus.clone();
        move |_: web_sys::MouseEvent| on_focus()
    };
    let on_key = move |e: web_sys::KeyboardEvent| {
        let is_self = e
            .target()
            .zip(e.current_target())
            .is_some_and(|(a, b)| a == b);
        if is_self && (e.key() == "Enter" || e.key() == " ") {
            e.prevent_default();
            on_focus();
        }
    };
    let image = unit.image.clone().map(|uri| {
        let style = crate::css! {
            "float": "inline-start",
            "width": "30%",
            "min-width": 160,
            "max-height": 210,
            "object-fit": "cover",
            "margin-inline-end": 22,
            "margin-bottom": 10,
            "border-radius": "var(--radius-md)",
            "background": "var(--surface-sunken)",
        };
        let alt = t(keys::story_draft::IMAGE_ALT, &[("n", &n.to_string())]);
        view! { <img src=uri alt=alt style=style.to_style() /> }
    });
    let text = crate::css! {
        "font-family": READ_TEXT,
        "font-size": "var(--fs-verse-lg)",
        "line-height": "var(--lh-verse-lg)",
        "margin": 0,
        "white-space": "pre-wrap",
        "color": "var(--text-secondary)",
        "margin-bottom": 0,
    };
    let gateway = unit
        .gateway
        .clone()
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| t(keys::story_draft::SOURCE_MISSING, &[]));
    let dot = comment
        .is_some()
        .then(|| view! { <StatusDot status=Status::Valid size=7.0 /> });
    let test_id = format!("story-understand-unit-{n}");
    let label = unit.label.clone();
    view! {
        <div
            data-testid=test_id
            data-focused=move || focused.get().then_some("true")
            on:click=on_click
            tabindex="0"
            aria-current=move || focused.get().then_some("true")
            on:keydown=on_key
            style=style
        >
            <div style=head.to_style()>
                <Overline>{label}</Overline>
                <div style="flex: 1;"></div>
                {dot}
            </div>
            <div dir=dir style=crate::css! { "margin-top": 10 }.to_style()>
                <Overline tone=OverlineTone::Muted style=crate::css! { "margin-bottom": 6 }>
                    {t(keys::story_draft::SOURCE, &[])}
                </Overline>
                {image}
                <p dir=dir data-testid="story-understand-gateway" style=text.to_style()>{gateway}</p>
            </div>
            <div data-testid="story-understand-comment" style="clear: both;">
                {comment_box(dir, comment)}
            </div>
        </div>
    }
    .into_any()
}

/// The user-comment box of a frame: its newest comment, read-only until C3 writes it.
fn comment_box(dir: &'static str, comment: Option<String>) -> AnyView {
    let style = crate::css! {
        "width": "100%",
        "box-sizing": "border-box",
        "margin-top": 4,
        "border": "var(--stroke) solid var(--border-input)",
        "border-radius": "var(--radius-md)",
        "padding": "11px 13px",
        "outline": "none",
        "resize": "vertical",
        "font-family": "var(--font-scripture)",
        "font-size": "var(--fs-verse)",
        "line-height": "var(--lh-verse)",
        "color": "var(--text-scripture)",
        "background": "var(--surface-app)",
    };
    view! {
        <textarea
            data-i="field"
            rows="3"
            dir=dir
            readonly=true
            prop:value=comment.unwrap_or_default()
            style=style.to_style()
            placeholder=t(keys::understand::COMMENTS_PLACEHOLDER_FRAME, &[])
        ></textarea>
    }
    .into_any()
}
