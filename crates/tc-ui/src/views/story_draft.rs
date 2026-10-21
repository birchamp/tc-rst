//! The port of tC4's `StoryDraft.jsx` (unit B7; #289, #307): Translate for an Open Bible Stories
//! project. The chrome is Bible Translate's: the rail with the same toggle, the header row (rail,
//! "Story N", helps), the two-column grid with the source on the left and the target on the
//! right per unit, one gateway source chip where the Bible's source tabs sit, the project's name
//! where the Bible page names it, the dashed "Draft frame N" pill and the helps panel scoped to
//! the unit in focus, with its Comments tab.
//!
//! The units are the story file's, in file order: the title (unit 0), every numbered frame, then
//! the reference line. C3 saves them through the typed save machine (`story/saveFrame`,
//! `story/saveRef`). An explicit Save or blur keeps the card and editing lock until its
//! durable Ack; a refusal retains the text for correction or Retry.

use std::sync::Arc;

use leptos::{html, prelude::*};
use send_wrapper::SendWrapper;
use tc_api::buffer::BufferKey;
use tc_api::helps::{PassageView, Story as HelpsStory};
use tc_api::ids::{Scope, StoryNo};
use tc_api::shown::Direction as TextDir;
use tc_api::story::{List, Open, StoryLine, StoryList, StoryView};
use wasm_bindgen::JsCast;

use super::draft_chrome::{FooterPress, editing_card};
use super::helps_panel::{HelpsPanel, HelpsToggleButton, StoryPanel};
use super::panel_icons::rail_icon;
use super::story_rail::story_rail;
use crate::app::load::Load;
use crate::app::save::{FieldSave, FieldScope, Pair, Source, View};
use crate::app::shell::Editing;
use crate::app::story::{
    StoryUnit, draft_units, image_note, is_frame_drafted, offers_download, source_notice,
    story_label, unit_key,
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
    let view = Memo::new(move |_| story.with(|loaded| loaded.value().cloned()));
    let ready = Memo::new(move |_| view.with(Option::is_some));
    let failed = Memo::new(move |_| story.with(|loaded| loaded.error().map(ToString::to_string)));
    (move || {
        match (ready.get(), failed.get()) {
            (false, None) => {
                status_line("story-draft-loading", t(keys::story_draft::LOADING, &[])).into_any()
            }
            (false, Some(message)) => {
                view! {
                    <main data-testid="story-draft-error" style=crate::css! { "flex": 1, "padding": 40 }.to_style()>
                        <Callout tone=CalloutTone::Warn>{message}</Callout>
                    </main>
                }
                .into_any()
            }
            (true, _) => {
                loaded_screen(&s, &actions, &shown, view, list, helps).into_any()
            }
        }
    })
    .into_any()
}

fn loaded_screen(
    s: &AppState,
    actions: &Actions,
    shown: &Shown,
    view: Memo<Option<Arc<StoryView>>>,
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
        let (s, actions, story) = (s.clone(), actions.clone(), shown.story);
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
                        story_rail(&s, &actions, rows, story, view.get(), on_frame)
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
    let units = {
        let (s, actions, frame, shown) = (s.clone(), actions.clone(), frame.clone(), shown.clone());
        view! { <For each=move || view.with(|view| view.as_ref().map(|view| draft_units(view)).unwrap_or_default()) key=|unit| unit.key.clone() children=move |unit| {
            unit_row(&s, &actions, &frame, unit, dir, &shown, view)
        } /> }
    };
    let comments = Memo::new(move |_| {
        Arc::new(view.with(|view| {
            view.as_ref()
                .map(|view| view.comments.clone())
                .unwrap_or_default()
        }))
    });
    let panel = StoryPanel {
        focus_frame,
        comments: Some(comments),
    };
    let content = view! {
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
                    {let actions = actions.clone(); move || view.get().map(|view| notices(&actions, &view))}
                    <div style=grid.to_style()>
                        {move || source_header(view.with(|view| view.as_ref().and_then(|view| view.source_version.clone())))}
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
    };
    let editing = s.editing.clone();
    html::div()
        .attr("style", root.to_style())
        .attr("data-testid", "story-draft")
        .on(
            leptos::ev::capture(leptos::ev::pointerdown),
            move |event: web_sys::PointerEvent| {
                if editing.with_untracked(Editing::is_open)
                    && super::translate::press_on_off_button(&event)
                {
                    event.prevent_default();
                }
            },
        )
        .child(content)
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
    s: &AppState,
    actions: &Actions,
    focus: &ArcRwSignal<Option<u16>>,
    unit: StoryUnit,
    dir: &'static str,
    shown: &Shown,
    view: Memo<Option<Arc<StoryView>>>,
) -> AnyView {
    let live_unit = {
        let unit = unit.clone();
        Memo::new(move |_| {
            view.with(|view| {
                view.as_ref()
                    .and_then(|view| draft_units(view).into_iter().find(|u| u.key == unit.key))
            })
            .unwrap_or_else(|| unit.clone())
        })
    };
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
            {move || source_cell(&live_unit.get(), dir)}
            {target_cell(s, actions, shown, live_unit, dir, view)}
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
fn target_cell(
    s: &AppState,
    actions: &Actions,
    open: &Shown,
    unit: Memo<StoryUnit>,
    dir: &'static str,
    view: Memo<Option<Arc<StoryView>>>,
) -> AnyView {
    let Some(project) = s.project.get_untracked() else {
        return ().into_any();
    };
    let frame = unit.with_untracked(|unit| unit.frame);
    let Some(key) = unit_key(&open.at.project, &project.id, open.story, frame) else {
        return ().into_any();
    };
    let editing = {
        let (editing, key) = (s.editing.clone(), key.clone());
        Memo::new(move |_| {
            editing.with(|editing| matches!(editing, Editing::Frame(open) if *open == key))
        })
    };
    let current = {
        let at = open.at.clone();
        Memo::new(move |_| {
            view.with(|view| {
                let view = view.as_ref()?;
                let (value, token) = match frame {
                    Some(0) => (&view.title, &view.title_token),
                    Some(n) => {
                        let frame = view.frames.get(usize::from(n).checked_sub(1)?)?;
                        (&frame.text, &frame.token)
                    }
                    None => (&view.reference, &view.reference_token),
                };
                Some(Pair {
                    value: value.clone(),
                    token: token.clone(),
                    view: Some(View::of(Some(&at), view.view)),
                })
            })
        })
    };
    let style = cell().merge(&crate::css! { "position": "relative" });
    let gap = crate::css! { "min-height": 14, "margin-bottom": 6 };
    let body = {
        let (actions, key, at) = (actions.clone(), key.clone(), open.at.clone());
        move || {
            if editing.get() {
                let line = match frame {
                    Some(0) => StoryLine::Title,
                    Some(_) => StoryLine::Frame,
                    None => StoryLine::Reference,
                };
                return view! { <UnitEditor key=key.clone() current=current at=at.clone() dir=dir actions=actions.clone() label=unit.with_untracked(|unit| unit.label.clone()) line=line /> }.into_any();
            }
            let edit = {
                let (actions, key) = (actions.clone(), key.clone());
                move || actions.start_story_unit(key.clone())
            };
            let unit = unit.get();
            if is_frame_drafted(&unit.draft) {
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
                on:click={let edit = edit.clone(); move |_| edit()}
                on:keydown=move |event: web_sys::KeyboardEvent| {
                    if event.key() == "Enter" || event.key() == " " { event.prevent_default(); edit(); }
                }
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
            <button type="button" data-i="choice" data-tone="accent" style=pill.to_style() on:click=move |_| edit()>
                {unit.pill.clone()}
            </button>
        }
        .into_any()
            }
        }
    };
    view! {
        <div style=style.to_style()>
            <div style=gap.to_style()></div>
            {body}
        </div>
    }
    .into_any()
}

/// One OBS unit uses the same blur/idle field as a verse, including a recovered entry
/// without a mounted editor. Its card keeps the editing lock until Save, Cancel or blur.
/// Save and blur first put the text in the form it is stored in, as tC4's `blurStoryUnit`
/// does, so the stored text is what the card shows when it closes.
#[component]
fn UnitEditor(
    key: BufferKey,
    current: Memo<Option<Pair>>,
    at: Scope,
    dir: &'static str,
    actions: Actions,
    label: String,
    line: StoryLine,
) -> impl IntoView {
    let multiline = line == StoryLine::Frame;
    let (Some(runtime), Some(pair)) = (use_runtime(), current.get_untracked()) else {
        return ().into_any();
    };
    let before = pair.value.clone();
    let saves = runtime.saves().clone();
    let shown = saves.open(&key, FieldScope::Project(Some(at)), pair);
    let finishing = RwSignal::new(false);
    let state = {
        let (saves, key, changes) = (
            SendWrapper::new(saves.clone()),
            key.clone(),
            saves.changes(),
        );
        Memo::new(move |_| {
            changes.track();
            saves.state(&key)
        })
    };
    let saving = Memo::new(move |_| {
        finishing.get() && matches!(state.get(), Some(FieldSave::Saving { .. }))
    });
    {
        let actions = actions.clone();
        Effect::new(move |_| {
            if finishing.get()
                && matches!(
                    state.get(),
                    Some(FieldSave::Clean | FieldSave::SavedWithWarning(_))
                )
            {
                actions.close_story_unit();
            }
        });
    }
    {
        let (saves, key) = (SendWrapper::new(saves.clone()), key.clone());
        Effect::new(move |_| {
            if let Some(pair) = current.get() {
                saves.refresh(&key, pair, None);
            }
        });
    }
    {
        let (saves, key) = (SendWrapper::new(saves.clone()), key.clone());
        on_cleanup(move || saves.close(&key));
    }
    let area = NodeRef::<html::Textarea>::new();
    let input = NodeRef::<html::Input>::new();
    let tidy = {
        let (saves, key, shown) = (SendWrapper::new(saves.clone()), key.clone(), shown.clone());
        move |typed: String| {
            let stored = tc_api::story::stored_text(line, &typed);
            if stored != typed {
                shown.set(stored.clone());
                saves.typed(&key, &stored, Source::Program);
            }
        }
    };
    super::each_frame(1, move || {
        if let Some(area) = area.get_untracked() {
            let _ = area.focus();
        }
        if let Some(input) = input.get_untracked() {
            let _ = input.focus();
        }
        false
    });
    let on_input = {
        let (saves, key) = (SendWrapper::new(saves.clone()), key.clone());
        move |event: leptos::ev::Event| {
            saves.typed(&key, &event_target_value(&event), Source::Input)
        }
    };
    let on_focus = {
        let (saves, key) = (SendWrapper::new(saves.clone()), key.clone());
        move |event: leptos::ev::FocusEvent| {
            if let Some(element) = event
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
            {
                saves.focus(&key, &element);
            }
        }
    };
    let on_composition = {
        let (saves, key) = (SendWrapper::new(saves.clone()), key.clone());
        move |_: leptos::ev::CompositionEvent| saves.composition(&key)
    };
    let on_blur = {
        let (saves, key, tidy) = (SendWrapper::new(saves.clone()), key.clone(), tidy.clone());
        // A card that closes takes its focused field away and blurs it after the editor is
        // gone, so the text comes from the event, never from the editor's own node.
        move |event: leptos::ev::FocusEvent| {
            tidy(event_target_value(&event));
            saves.blur(&key);
            finishing.set(true);
        }
    };
    let save = {
        let (saves, key) = (SendWrapper::new(saves.clone()), key.clone());
        move |_| {
            let typed = area
                .get_untracked()
                .map(|area| area.value())
                .or_else(|| input.get_untracked().map(|input| input.value()));
            if let Some(typed) = typed {
                tidy(typed);
            }
            saves.save(&key);
            finishing.set(true);
        }
    };
    let cancel = {
        let (saves, key) = (SendWrapper::new(saves), key.clone());
        move |_| {
            saves.typed(&key, &before, Source::Program);
            saves.blur(&key);
            finishing.set(true);
        }
    };
    let field = read().merge(&crate::css! {
        "width": "100%", "box-sizing": "border-box", "border": 0, "outline": "none",
        "resize": "vertical", "color": "var(--text-scripture)", "background": "transparent",
    });
    let field = if multiline {
        view! { <textarea node_ref=area aria-label=label.clone() dir=dir prop:value=move || shown.get() disabled=move || saving.get()
            placeholder=t(keys::story_draft::PLACEHOLDER, &[]) rows="4" style=field.to_style()
            on:input=on_input on:focus=on_focus on:blur=on_blur on:compositionstart=on_composition
        ></textarea> }.into_any()
    } else {
        view! { <input node_ref=input aria-label=label.clone() dir=dir prop:value=move || shown.get() disabled=move || saving.get()
            placeholder=t(keys::story_draft::PLACEHOLDER, &[]) style=field.to_style()
            on:input=on_input on:focus=on_focus on:blur=on_blur on:compositionstart=on_composition
        /> }.into_any()
    };
    let header = view! { <Overline tone=OverlineTone::Accent>{t(keys::draft::DRAFTING, &[])}" "{label}</Overline> }.into_any();
    let footer = view! {
        <Button size=Size::Sm disabled=Signal::derive(move || saving.get()) on_click=Callback::new(save)>{t(keys::story_draft::SAVE, &[])}</Button>
        <Button variant=ButtonVariant::Ghost style=crate::css! { "color": "var(--text-tertiary)", "font-size": "var(--fs-caption)", "letter-spacing": "var(--track-12)" }
            disabled=Signal::derive(move || saving.get())
            on_click=Callback::new(cancel)>{t(keys::draft::CANCEL_VERSE, &[])}</Button>
    }.into_any();
    editing_card(
        header,
        field,
        footer,
        FooterPress::KeepsFocus,
        Attrs::from([("data-testid", "story-unit-editor")]),
    )
}
