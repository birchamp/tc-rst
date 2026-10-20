//! The port of tC4's `StoryDraft.jsx` (unit B7; #289, #307): Translate for an Open Bible Stories
//! project. The chrome is Bible Translate's: the rail with the same toggle, the header row (rail,
//! "Story N", helps), the two-column grid with the source on the left and the target on the
//! right per unit, one gateway source chip where the Bible's source tabs sit, the project's name
//! where the Bible page names it, the dashed "Draft frame N" pill and the helps panel scoped to
//! the unit in focus, with its Comments tab.
//!
//! The units are the story file's, in file order: the title (unit 0), every numbered frame, then
//! the reference line. C3 saves them (`story/saveFrame`, `story/saveRef`, with the editing
//! lock); until then a unit shows its text and a click only brings it into focus, so no card
//! opens with nowhere to save what is typed (a declared difference until C3).

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
use crate::app::story::{
    StoryUnit, draft_units, image_note, is_frame_drafted, offers_download, source_notice,
    story_label,
};
use crate::app::web::{Actions, AppCtx, AppState, use_app, use_runtime};
use crate::attrs::Attrs;
use crate::ds::{
    Badge, BadgeSize, BadgeTone, Button, ButtonVariant, Callout, CalloutTone, IconButton, Overline,
    OverlineTone, Size,
};
use crate::i18n::{keys, t};

/// `var(--stroke-hair) solid var(--border-hair)` (tC4's `hair`).
const HAIR: &str = "var(--stroke-hair) solid var(--border-hair)";

fn cell() -> crate::css::Css {
    crate::css! { "padding": "14px 26px 20px", "border-top": HAIR }
}

fn read() -> crate::css::Css {
    crate::css! {
        "font-family": "var(--font-scripture)",
        "font-size": "var(--fs-verse-lg)",
        "line-height": "var(--lh-verse-lg)",
        "margin": 0,
        "white-space": "pre-wrap",
        "text-align": "start",
    }
}

#[derive(Clone, PartialEq)]
struct Shown {
    at: Scope,
    story: StoryNo,
}

/// tC4's `StoryDraft`.
#[component]
pub fn StoryDraft() -> impl IntoView {
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
        None => status_line("story-draft-empty", t(keys::story_draft::NO_STORY, &[])).into_any(),
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
    let (s, actions) = (s.clone(), actions.clone());
    (move || {
        story.with(|loaded| match loaded {
            Load::Loading => {
                status_line("story-draft-loading", t(keys::story_draft::LOADING, &[])).into_any()
            }
            Load::Failed(error) => {
                let message = error.to_string();
                view! {
                    <main data-testid="story-draft-error" style=crate::css! { "flex": 1, "padding": 40 }.to_style()>
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
    let dir = match s.project.with_untracked(|p| p.as_ref().map(|p| p.dir)) {
        Some(TextDir::Rtl) => "rtl",
        _ => "ltr",
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
                                scroll_to(&format!("story-frame-{n}"));
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
    let bar = crate::css! {
        "display": "flex",
        "align-items": "center",
        "gap": 12,
        "padding": "10px 22px",
        "border-bottom": HAIR,
        "background": "var(--surface-card)",
        "flex": "none",
    };
    let h2 = crate::css! {
        "font-size": "var(--fs-title)",
        "letter-spacing": "var(--track-17)",
        "margin": 0,
    };
    let toggle = {
        let actions = actions.clone();
        Callback::new(move |_| actions.toggle_rail())
    };
    let scroll = crate::css! { "flex": 1, "overflow": "auto", "min-height": 0 };
    let grid = crate::css! {
        "display": "grid",
        "grid-template-columns": "1fr 1fr",
        "max-width": 1100,
        "margin": "0 auto",
    };
    let sticky = crate::css! {
        "position": "sticky",
        "top": 0,
        "background": "var(--surface-app)",
        "z-index": 2,
        "padding": "13px 26px 8px",
        "display": "flex",
        "align-items": "center",
    };
    let project_line = {
        let project = s.project.get_untracked();
        project.map(|p| format!("{} · {}", p.name, p.language_tag))
    };
    let units = draft_units(&view)
        .into_iter()
        .map(|unit| unit_row(actions, &frame, unit, dir))
        .collect_view();
    let comments = Memo::new({
        let view = view.clone();
        move |_| Arc::new(view.comments.clone())
    });
    let panel = StoryPanel {
        focus_frame,
        comments: Some(comments),
    };
    view! {
        <div style=root.to_style() data-testid="story-draft">
            {rail}
            <main style=main.to_style()>
                <div style=bar.to_style()>
                    <IconButton
                        title=t(keys::story_draft::TOGGLE_RAIL, &[])
                        attrs=Attrs::from([("data-testid", "toggle-story-rail")])
                        on_click=toggle
                    >
                        {rail_icon()}
                    </IconButton>
                    <h2 style=h2.to_style()>{story_label(shown.story)}</h2>
                    <div style="flex: 1;"></div>
                    <HelpsToggleButton />
                </div>
                <div style=scroll.to_style()>
                    {notices(actions, &view)}
                    <div style=grid.to_style()>
                        {source_header(view.source_version.clone())}
                        <div style=sticky.to_style()>
                            <Overline tone=OverlineTone::Accent>{project_line}</Overline>
                            <div style="flex: 1;"></div>
                        </div>
                        {units}
                    </div>
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

/// Brings a unit into view, as a rail frame button brings its unit.
fn scroll_to(test_id: &str) {
    let selector = format!("[data-testid=\"{test_id}\"]");
    if let Some(el) = document().query_selector(&selector).ok().flatten() {
        el.scroll_into_view();
    }
}

/// The source column's head: the OBS mark where the Bible's source tabs sit, and the pinned
/// version under it.
fn source_header(version: Option<String>) -> impl IntoView {
    let style = crate::css! {
        "position": "sticky",
        "top": 0,
        "background": "var(--surface-app)",
        "z-index": 2,
        "padding": "13px 26px 8px",
        "border-inline-end": HAIR,
    };
    let chips = crate::css! {
        "display": "flex",
        "align-items": "center",
        "gap": 6,
        "margin-bottom": 6,
        "flex-wrap": "wrap",
    };
    let name = crate::css! {
        "font-size": "var(--fs-label)",
        "letter-spacing": "var(--track-11)",
        "color": "var(--text-tertiary)",
        "font-weight": "var(--fw-medium)",
    };
    let pinned = version.map(|version| t(keys::draft::PINNED, &[("version", &version)]));
    view! {
        <div style=style.to_style()>
            <div style=chips.to_style()>
                <Badge size=BadgeSize::Sm tone=BadgeTone::Accent attrs=Attrs::from([("data-testid", "source-tab-obs")])>
                    {t(keys::home::OBS_MARKER, &[])}
                </Badge>
            </div>
            <span data-testid="source-name" style=name.to_style()>
                {t(keys::story_draft::SOURCE, &[])}
                {pinned.map(|p| format!(" · {p}"))}
            </span>
        </div>
    }
}

/// The two notices above the grid: why the gateway story is absent, and why no frame has a
/// picture.
fn notices(actions: &Actions, view: &StoryView) -> AnyView {
    let source = view.source.as_ref().map(|notice| {
        let text = source_notice(notice);
        let install = offers_download(notice).then(|| {
            let actions = actions.clone();
            view! {
                <Button
                    size=Size::Sm
                    variant=ButtonVariant::Secondary
                    attrs=Attrs::from([("data-testid", "story-source-install")])
                    on_click=Callback::new(move |_| actions.open_sources())
                >
                    {t(keys::story_draft::GET_SOURCE, &[])}
                </Button>
            }
        });
        let style = crate::css! {
            "display": "flex",
            "align-items": "center",
            "gap": 10,
            "margin": "14px 26px 0",
        };
        let text_style = crate::css! { "flex": 1, "overflow-wrap": "anywhere" };
        view! {
            <Callout tone=CalloutTone::Warn style=style attrs=Attrs::from([("data-testid", "story-source-notice")])>
                <span style=text_style.to_style()>{text}</span>
                {install}
            </Callout>
        }
    });
    let pictures = view.image_note.as_ref().map(|note| {
        let style = crate::css! { "margin": "14px 26px 0", "overflow-wrap": "anywhere" };
        let text = image_note(note);
        view! {
            <Callout tone=CalloutTone::Info style=style attrs=Attrs::from([("data-testid", "story-image-note")])>
                {text}
            </Callout>
        }
    });
    view! { {source} {pictures} }.into_any()
}

/// One unit: the source cell and the target cell, a click bringing it into focus.
fn unit_row(
    actions: &Actions,
    focus: &ArcRwSignal<Option<u16>>,
    unit: StoryUnit,
    dir: &'static str,
) -> AnyView {
    let focused = {
        let (focus, frame) = (focus.clone(), unit.frame);
        Memo::new(move |_| frame.is_some() && focus.get() == frame)
    };
    let row = crate::css! {
        "grid-column": "1 / -1",
        "display": "grid",
        "grid-template-columns": "1fr 1fr",
    };
    let on_click = {
        let (actions, frame) = (actions.clone(), unit.frame);
        move |_: web_sys::MouseEvent| {
            if let Some(frame) = frame {
                actions.set_story_frame(frame);
            }
        }
    };
    let test_id = unit.test_id.clone();
    view! {
        <div
            data-testid=test_id
            data-focused=move || focused.get().then_some("true")
            style=row.to_style()
            on:click=on_click
        >
            {source_cell(&unit, dir)}
            {target_cell(&unit, dir)}
        </div>
    }
    .into_any()
}

/// The unit's label, its picture (a frame only) and the gateway text or the unavailable notice.
fn source_cell(unit: &StoryUnit, dir: &'static str) -> AnyView {
    let style = cell().merge(&crate::css! { "border-inline-end": HAIR });
    let image = unit.image.clone().map(|uri| {
        let style = crate::css! {
            "display": "block",
            "width": "100%",
            "max-height": 210,
            "object-fit": "cover",
            "border-radius": "var(--radius-md)",
            "background": "var(--surface-sunken)",
            "margin-bottom": 10,
        };
        let alt = t(
            keys::story_draft::IMAGE_ALT,
            &[("n", &unit.frame.unwrap_or_default().to_string())],
        );
        view! { <img src=uri alt=alt style=style.to_style() /> }
    });
    let text = unit.gateway.clone().filter(|g| !g.is_empty());
    let mut p = read();
    match &text {
        Some(_) => {
            p.extend(&crate::css! { "color": "var(--text-scripture)", "font-style": "normal" })
        }
        None => p.extend(&crate::css! {
            "color": "var(--uw-haze)",
            "font-style": "italic",
            "font-size": "var(--fs-ui-sm)",
        }),
    }
    let text = text.unwrap_or_else(|| t(keys::story_draft::SOURCE_MISSING, &[]));
    let label = unit.label.clone();
    view! {
        <div style=style.to_style()>
            <Overline tone=OverlineTone::Muted style=crate::css! { "margin-bottom": 6 }>{label}</Overline>
            {image}
            <p dir=dir style=p.to_style()>{text}</p>
        </div>
    }
    .into_any()
}

/// The target cell: the drafted text, or the dashed pill of an undrafted unit. Drafted is the
/// rail marker's predicate, so the cell and the marker never disagree.
fn target_cell(unit: &StoryUnit, dir: &'static str) -> AnyView {
    let style = cell().merge(&crate::css! { "position": "relative" });
    let gap = crate::css! { "min-height": 14, "margin-bottom": 6 };
    let body = if is_frame_drafted(&unit.draft) {
        let mut p = read();
        p.extend(&crate::css! { "color": "var(--text-scripture)", "cursor": "text" });
        view! {
            <p
                dir=dir
                title=t(keys::story_draft::EDIT_UNIT, &[])
                role="button"
                tabindex="0"
                data-testid="story-unit-text"
                style=p.to_style()
            >
                {unit.draft.clone()}
            </p>
        }
        .into_any()
    } else {
        let pill = crate::css! {
            "border": "var(--stroke-selected) dashed var(--border-strong)",
            "background": "transparent",
            "border-radius": "var(--radius-sm)",
            "padding": "2px 10px",
            "cursor": "pointer",
            "font-family": "var(--font-ui)",
            "font-size": "var(--fs-caption-lg)",
            "letter-spacing": "var(--track-12-5)",
            "font-weight": "var(--fw-bold)",
            "color": "var(--text-tertiary)",
            "vertical-align": "middle",
        };
        view! {
            <button type="button" data-i="choice" data-tone="accent" style=pill.to_style()>
                {unit.pill.clone()}
            </button>
        }
        .into_any()
    };
    view! {
        <div style=style.to_style()>
            <div style=gap.to_style()></div>
            {body}
        </div>
    }
    .into_any()
}
