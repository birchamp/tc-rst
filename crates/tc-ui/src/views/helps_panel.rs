//! The port of tC4's `HelpsPanel.jsx` (unit B3): the helps of the open chapter in tabs
//! (Notes, Words, Questions, the simplified text), each tab's absent states stated, long notes
//! cut with "Show more", the article pane with its header kept in view (#601), and the divider
//! that resizes the panel (#602). Translate mounts the same panel with its Comments tab (B4a).

use std::sync::Arc;

use leptos::html;
use leptos::prelude::*;
use tc_api::editor::CommentView;
use tc_api::helps::{ArticleLink, ArticleView, PassageView, ReadySlot, Slot};
use tc_api::model::{ArticleBlock, BlockKind};
use tc_api::shown::Direction as TextDir;
use tc_api::story::FrameComment;

use super::panel_icons::helps_icon;
use crate::app::load::Load;
use crate::app::understand::{
    HelpFocus, HelpsTab, PREVIEW_CHARS, blocks_len, drag_width, in_focus, preview_blocks,
    simplified_label, slot_state,
};
use crate::app::web::understand::ArticleState;
use crate::app::web::{Actions, AppCtx, use_app};
use crate::attrs::Attrs;
use crate::ds::{
    Button, ButtonVariant, Callout, CalloutTone, HelpCard, HelpKind, IconButton, Indicator,
    Overline, Size, Switcher, SwitcherOption,
};
use crate::i18n::{keys, t};

fn dir_str(dir: Option<TextDir>) -> Option<&'static str> {
    dir.map(|d| match d {
        TextDir::Ltr => "ltr",
        TextDir::Rtl => "rtl",
    })
}

/// The show/hide button of the helps panel (#602), on every screen with the panel.
#[component]
pub fn HelpsToggleButton() -> impl IntoView {
    let Some(ctx) = use_app() else {
        return ().into_any();
    };
    let actions = ctx.actions.clone();
    view! {
        <IconButton
            title=t(keys::draft::TOGGLE_HELPS, &[])
            attrs=Attrs::from([("data-testid", "toggle-helps")])
            on_click=Callback::new(move |_| actions.toggle_helps())
        >
            {helps_icon()}
        </IconButton>
    }
    .into_any()
}

/// What makes the panel a story's (tC4 `story`, #331): the whole story is listed in frame
/// order with the frame in focus marked and scrolled into view, there is no simplified tab, and
/// Translate adds the Comments tab.
#[derive(Clone)]
pub struct StoryPanel {
    /// The frame in focus (0 is the title).
    pub focus_frame: Memo<Option<u16>>,
    /// The story's comments, when the panel has its Comments tab.
    pub comments: Option<Memo<Arc<Vec<FrameComment>>>>,
}

/// tC4's `HelpsPanel` for a chapter: `focus_verses` are the focused unit's verses (none: the
/// whole chapter), `pane` the source pane whose words title the cards, `comments` the book's
/// user comments for Translate's Comments tab (none: no such tab). A story's panel is `story`.
#[component]
pub fn HelpsPanel(
    chapter: u16,
    focus_verses: Memo<Option<Vec<String>>>,
    helps: Signal<Load<Arc<PassageView>>>,
    pane: String,
    #[prop(optional)] comments: Option<Signal<Vec<CommentView>>>,
    #[prop(optional)] story: Option<StoryPanel>,
) -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let shown = s.u.helps.clone();
    let width = s.u.helps_width.clone();
    let has_comments = comments.is_some() || story.as_ref().is_some_and(|s| s.comments.is_some());
    let is_story = story.is_some();
    let tab = {
        let tab = s.u.helps_tab.clone();
        Memo::new(move |_| tab.get().shown(has_comments, is_story))
    };
    let panel = NodeRef::<html::Aside>::new();
    let style = move || {
        let mut css = crate::css! { "position": "relative" };
        css.set(
            "width",
            width
                .get()
                .map_or_else(|| "var(--helps-width)".to_owned(), |w| format!("{w}px")),
        );
        css.extend(&crate::css! {
            "flex": "none",
            "background": "var(--surface-panel)",
            "border-inline-start": "var(--stroke-hair) solid var(--border)",
            "display": "flex",
            "flex-direction": "column",
            "min-height": 0,
        });
        css.to_style()
    };
    let label = Memo::new(move |_| {
        helps.with(|h| simplified_label(h.value().and_then(|v| v.simplified_pin.as_deref())))
    });
    // The frame in focus leads (owner ruling 2026-09-18, #331): its first card scrolls into view.
    let list_ref = NodeRef::<html::Div>::new();
    if let Some(story) = story.clone() {
        Effect::new(move |_| {
            let frame = story.focus_frame.get();
            let _ = tab.get();
            let loaded = helps.with(|h| h.value().is_some());
            let (Some(frame), true, Some(list)) = (frame, loaded, list_ref.get()) else {
                return;
            };
            if let Ok(Some(card)) = list.query_selector(&format!("[data-frame=\"{frame}\"]")) {
                card.scroll_into_view_with_bool(true);
            }
        });
    }
    let switcher = {
        let actions = actions.clone();
        move || {
            let mut options = vec![
                SwitcherOption::new("notes", t(keys::helps::NOTES, &[])),
                SwitcherOption::new("words", t(keys::helps::WORDS, &[])),
                SwitcherOption::new("questions", t(keys::helps::QUESTIONS, &[])),
            ];
            if !is_story {
                options.push(SwitcherOption::new("simplified", label.get()));
            }
            if has_comments {
                options.push(SwitcherOption::new(
                    "comments",
                    t(keys::helps::COMMENTS, &[]),
                ));
            }
            let value = Signal::derive(move || Some(tab.get().as_str().to_owned()));
            let actions = actions.clone();
            let on_change = Callback::new(move |v: String| {
                if let Some(tab) = HelpsTab::parse(&v) {
                    actions.set_helps_tab(tab);
                }
            });
            view! { <Switcher indicator=Indicator::Underline value=value on_change=on_change options=options /> }
        }
    };
    let list = crate::css! {
        "flex": 1,
        "overflow": "auto",
        "padding": 16,
        "min-height": 0,
        "display": "flex",
        "flex-direction": "column",
        "gap": 10,
    };
    let content = {
        let (s, actions, story) = (s.clone(), actions.clone(), story.clone());
        move || {
            let tab = tab.get();
            helps.with(|loaded| match loaded {
                Load::Loading => {
                    let style = crate::css! {
                        "font-size": "var(--fs-caption-lg)",
                        "color": "var(--text-tertiary)",
                        "margin": 0,
                    };
                    view! { <p data-testid="helps-loading" style=style.to_style()>{t(keys::understand::LOADING, &[])}</p> }.into_any()
                }
                Load::Failed(error) => {
                    let text = error.to_string();
                    view! {
                        <Callout tone=CalloutTone::Warn style=crate::css! { "overflow-wrap": "anywhere" }
                            attrs=Attrs::from([("role", "alert"), ("data-testid", "understand-error")])>
                            {text}
                        </Callout>
                    }
                    .into_any()
                }
                Load::Ready(view) | Load::Refreshing(view) => match (tab, comments) {
                    (HelpsTab::Comments, Some(comments)) => {
                        comments_tab(&s, comments, chapter, focus_verses).into_any()
                    }
                    _ => helps_tab(&s, &actions, tab, view, chapter, focus_verses, &pane, story.as_ref())
                        .into_any(),
                },
            })
        }
    };
    let article = {
        let actions = actions.clone();
        let article = s.u.article.clone();
        move || article.get().map(|state| article_view(&actions, state))
    };
    (move || {
        shown.get().then(|| {
            view! {
                <aside node_ref=panel data-testid="helps-panel" style=style.clone()>
                    {helps_divider(&actions, panel)}
                    {switcher.clone()}
                    <div node_ref=list_ref style=list.to_style()>{content.clone()}</div>
                    {article.clone()}
                </aside>
            }
        })
    })
    .into_any()
}

/// A drag of the divider: where it started, the panel's width then, whether the page reads
/// right to left, and the width it has reached.
#[derive(Clone, Copy)]
struct Drag {
    start_x: f64,
    start_width: f64,
    rtl: bool,
    width: Option<f64>,
}

/// The panel's divider (#602): a drag sets the width in the page, the release keeps it.
fn helps_divider(actions: &Actions, panel: NodeRef<html::Aside>) -> impl IntoView + use<> {
    let drag: StoredValue<Option<Drag>> = StoredValue::new(None);
    let style = crate::css! {
        "position": "absolute",
        "inset-block": 0,
        "inset-inline-start": -3,
        "width": 6,
        "cursor": "col-resize",
        "touch-action": "none",
        "z-index": 3,
    };
    let end = {
        let actions = actions.clone();
        move || {
            let width = drag.get_value().and_then(|d| d.width);
            drag.set_value(None);
            if let Some(width) = width {
                actions.set_helps_width(width);
            }
        }
    };
    let (end_up, end_cancel, end_lost) = (end.clone(), end.clone(), end);
    view! {
        <div
            role="separator"
            aria-orientation="vertical"
            aria-label=move || t(keys::helps::RESIZE, &[])
            data-testid="helps-divider"
            style=style.to_style()
            on:pointerdown=move |e: web_sys::PointerEvent| {
                if e.button() != 0 {
                    return;
                }
                e.prevent_default();
                if let Some(target) = e
                    .current_target()
                    .and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(t).ok())
                {
                    let _ = target.set_pointer_capture(e.pointer_id());
                }
                let Some(aside) = panel.get_untracked() else {
                    return;
                };
                let width = aside.get_bounding_client_rect().width();
                let rtl = web_sys::window()
                    .and_then(|w| w.get_computed_style(&aside).ok().flatten())
                    .and_then(|style| style.get_property_value("direction").ok())
                    .is_some_and(|d| d == "rtl");
                drag.set_value(Some(Drag {
                    start_x: f64::from(e.client_x()),
                    start_width: width,
                    rtl,
                    width: None,
                }));
            }
            on:pointermove=move |e: web_sys::PointerEvent| {
                let Some(started) = drag.get_value() else {
                    return;
                };
                let width = drag_width(
                    started.start_x,
                    started.start_width,
                    started.rtl,
                    f64::from(e.client_x()),
                );
                drag.set_value(Some(Drag {
                    width: Some(width),
                    ..started
                }));
                if let Some(aside) = panel.get_untracked() {
                    let _ = aside.style().set_property("width", &format!("{width}px"));
                }
            }
            on:pointerup=move |_| end_up()
            on:pointercancel=move |_| end_cancel()
            on:lostpointercapture=move |_| end_lost()
        ></div>
    }
}

/// tC4's `HelpsTab`.
#[allow(clippy::too_many_arguments)]
fn helps_tab(
    s: &crate::app::web::AppState,
    actions: &Actions,
    tab: HelpsTab,
    view: &PassageView,
    chapter: u16,
    focus_verses: Memo<Option<Vec<String>>>,
    pane: &str,
    story: Option<&StoryPanel>,
) -> AnyView {
    // tC4's `chapterAllCrossBook`: a chapter the sources have in another book has its helps
    // there.
    if let Some(to) = &view.other_book {
        let text = t(
            keys::understand::HELPS_CROSS_BOOK,
            &[("to", to as &dyn std::fmt::Display)],
        );
        return view! {
            <Callout tone=CalloutTone::Info style=crate::css! { "overflow-wrap": "anywhere" } attrs=Attrs::from([("data-testid", "helps-cross-book")])>
                {text}
            </Callout>
        }
        .into_any();
    }
    let focus = focus_verses.get();
    let focus = focus.as_deref();
    let frame = story.map(|s| s.focus_frame);
    match tab {
        HelpsTab::Notes => notes_tab(s, actions, &view.notes, chapter, focus, pane, frame),
        HelpsTab::Words => words_tab(s, actions, &view.words, focus, pane, frame),
        HelpsTab::Questions => questions_tab(s, &view.questions, focus, frame),
        HelpsTab::Simplified => simplified_tab(&view.simplified, focus),
        HelpsTab::Comments => match story.and_then(|s| s.comments) {
            Some(comments) => story_comments_tab(s, comments),
            None => ().into_any(),
        },
    }
}

/// Whether a card at `verse` is lit: it is the focused help (a click), or, in a story, a card of
/// the frame in focus (tC4 `isActive`). Read inside the tab's render, so the tab is drawn again
/// when the focus moves.
fn card_lit(
    active: &ArcRwSignal<Option<HelpFocus>>,
    id: &str,
    verse: &str,
    frame: Option<Memo<Option<u16>>>,
) -> bool {
    active.with(|a| a.as_ref().is_some_and(|a| a.id == id))
        || frame.is_some_and(|f| crate::app::story::card_in_focus(verse, f.get()))
}

/// tC4's `CommentsTab` of a story: the newest comment of each frame, in frame order.
fn story_comments_tab(
    s: &crate::app::web::AppState,
    comments: Memo<Arc<Vec<FrameComment>>>,
) -> AnyView {
    let dir = match s.project.with_untracked(|p| p.as_ref().map(|p| p.dir)) {
        Some(TextDir::Rtl) => "rtl",
        _ => "ltr",
    };
    let list = comments.get();
    let mut frames: Vec<u16> = list.iter().map(|c| c.frame).collect();
    frames.sort_unstable();
    frames.dedup();
    if frames.is_empty() {
        return empty_tab(None);
    }
    let boxed = crate::css! {
        "border": "var(--stroke) solid var(--border)",
        "border-radius": "var(--radius-lg)",
        "padding": 14,
        "background": "#fff",
    };
    let text = crate::css! { "white-space": "pre-wrap", "text-align": "start" };
    frames
        .into_iter()
        .filter_map(|frame| {
            let comment = crate::app::story::frame_comment(&list, frame)?;
            let label = crate::app::story::comment_label(frame);
            Some(view! {
                <div data-testid="helps-comment" data-frame=frame.to_string() style=boxed.to_style()>
                    <Overline>{label}</Overline>
                    <p dir=dir style=text.to_style()>{comment.text.clone()}</p>
                </div>
            })
        })
        .collect_view()
        .into_any()
}

/// tC4's `CommentsTab`: each verse's newest comment in focus, as typed, in the project's
/// direction (#657).
fn comments_tab(
    s: &crate::app::web::AppState,
    comments: Signal<Vec<CommentView>>,
    chapter: u16,
    focus_verses: Memo<Option<Vec<String>>>,
) -> AnyView {
    let dir = match s.project.with_untracked(|p| p.as_ref().map(|p| p.dir)) {
        Some(TextDir::Rtl) => "rtl",
        _ => "ltr",
    };
    let card = crate::css! {
        "border": "var(--stroke) solid var(--border)",
        "border-radius": "var(--radius-lg)",
        "padding": 14,
        "background": "#fff",
    }
    .to_style();
    let text = crate::css! { "white-space": "pre-wrap", "text-align": "start" }.to_style();
    let focus = focus_verses.get();
    comments.with(|all| {
        let shown = crate::app::translate::comments_in_focus(all, chapter, focus.as_deref());
        if shown.is_empty() {
            return empty_tab(focus.as_deref());
        }
        shown
            .into_iter()
            .map(|c| {
                let n = c.verse.to_string();
                let label = t(keys::understand::VERSE_ONE, &[("n", &n)]);
                view! {
                    <div data-testid="helps-comment" data-frame=n style=card.clone()>
                        <Overline>{label}</Overline>
                        <p dir=dir style=text.clone()>{c.text.clone()}</p>
                    </div>
                }
            })
            .collect_view()
            .into_any()
    })
}

/// tC4's `SlotState`: a tab with nothing to show says why.
fn slot_status<T>(actions: &Actions, slot: &Slot<T>) -> AnyView {
    if let Slot::Error { reason } = slot {
        let reason = reason.clone();
        let actions = actions.clone();
        return view! {
            <Callout tone=CalloutTone::Warn style=crate::css! { "overflow-wrap": "anywhere" }
                attrs=Attrs::from([("role", "alert"), ("data-testid", "helps-state-error")])>
                {t(keys::understand::HELP_ERROR, &[])}" "{reason}" "
                <Button size=Size::Sm variant=ButtonVariant::Outline attrs=Attrs::from([("data-testid", "helps-retry")])
                    on_click=Callback::new(move |_| actions.reload_helps())>
                    {t(keys::app::RETRY, &[])}
                </Button>
            </Callout>
        }
        .into_any();
    }
    match slot_state(slot) {
        Some((state, key)) => {
            let test_id = format!("helps-state-{state}");
            view! {
                <Callout tone=CalloutTone::Info attrs=Attrs::from([("data-testid", test_id.as_str())])>{t(key, &[])}</Callout>
            }
            .into_any()
        }
        None => ().into_any(),
    }
}

/// tC4's `SlotBanners`: English answered for an absent primary.
fn banners<T>(ready: &ReadySlot<T>) -> Option<AnyView> {
    ready.fallback.then(|| {
        view! {
            <Callout tone=CalloutTone::Warn attrs=Attrs::from([("data-testid", "understand-fallback-warning")])>
                {t(keys::understand::HELP_FALLBACK, &[])}
            </Callout>
        }
        .into_any()
    })
}

/// tC4's `emptyTab`.
fn empty_tab(focus: Option<&[String]>) -> AnyView {
    let style = crate::css! {
        "font-size": "var(--fs-caption-lg)",
        "color": "var(--text-tertiary)",
        "font-style": "italic",
        "margin": 0,
    };
    let key = if focus.is_some() {
        keys::understand::NONE_IN_FOCUS
    } else {
        keys::understand::NONE_FOR_CHAPTER
    };
    view! { <p style=style.to_style()>{t(key, &[])}</p> }.into_any()
}

fn focus_of(id: &str, verse: &str, panes: &[tc_api::helps::PaneFocus]) -> HelpFocus {
    HelpFocus {
        id: id.to_owned(),
        verse: verse.to_owned(),
        panes: panes.to_vec(),
    }
}

/// The card handlers tC4's `cardFocus` gives: a click toggles the sticky focus, hover is
/// transient.
fn card_events(
    actions: &Actions,
    focus: HelpFocus,
) -> (
    Callback<web_sys::Event>,
    Callback<web_sys::MouseEvent>,
    Callback<web_sys::MouseEvent>,
) {
    let (a1, a2, a3) = (actions.clone(), actions.clone(), actions.clone());
    let (f1, f2) = (focus.clone(), focus);
    (
        Callback::new(move |_| a1.focus_help(Some(f1.clone()))),
        Callback::new(move |_| a2.hover_help(Some(f2.clone()))),
        Callback::new(move |_| a3.hover_help(None)),
    )
}

fn notes_tab(
    s: &crate::app::web::AppState,
    actions: &Actions,
    slot: &Slot<tc_api::helps::NoteCard>,
    chapter: u16,
    focus: Option<&[String]>,
    pane: &str,
    frame: Option<Memo<Option<u16>>>,
) -> AnyView {
    let Slot::Ready(ready) = slot else {
        return slot_status(actions, slot);
    };
    let _ = chapter;
    let notes: Vec<_> = ready
        .items
        .iter()
        .filter(|n| in_focus(n.verse.as_str(), focus))
        .collect();
    let dir = dir_str(ready.dir);
    let active = s.u.helps_active.clone();
    let cards = if notes.is_empty() {
        empty_tab(focus)
    } else {
        notes
            .into_iter()
            .map(|n| {
                let verse = n.verse.as_str().to_owned();
                let focus = focus_of(&n.id, &verse, &n.panes);
                let (click, enter, leave) = card_events(actions, focus);
                let lit = card_lit(&active, &n.id, &verse, frame);
                let is_active = Signal::from(lit);
                let title = HelpFocus::title(&n.panes, pane)
                    .or_else(|| (!n.quote.is_empty()).then(|| n.quote.clone()))
                    .or_else(|| n.academy.clone())
                    .unwrap_or_default();
                let mut attrs = Attrs::from([("data-testid", "helps-note")]);
                if let Some(dir) = dir {
                    attrs.set("dir", dir);
                }
                attrs.set("data-frame", verse.as_str());
                if lit {
                    attrs.set("data-focused", "true");
                }
                let note = n.note.clone();
                let body: crate::ds::Slot =
                    Box::new(move || expandable_note(note.clone()).into_any());
                match n.academy.clone() {
                    Some(slug) => {
                        let actions = actions.clone();
                        let academy = Callback::new(move |()| {
                            actions.load_help_article(ArticleLink::Academy { slug: slug.clone() })
                        });
                        view! {
                            <HelpCard
                                kind=HelpKind::Note
                                attrs=attrs
                                verse=verse
                                active=is_active
                                on_click=click
                                on_mouse_enter=enter
                                on_mouse_leave=leave
                                title=title
                                body=body
                                action_label=t(keys::understand::ACADEMY_LINK, &[])
                                on_action=academy
                            />
                        }
                        .into_any()
                    }
                    None => view! {
                        <HelpCard
                            kind=HelpKind::Note
                            attrs=attrs
                            verse=verse
                            active=is_active
                            on_click=click
                            on_mouse_enter=enter
                            on_mouse_leave=leave
                            title=title
                            body=body
                            action_label=t(keys::understand::ACADEMY_LINK, &[])
                        />
                    }
                    .into_any(),
                }
            })
            .collect_view()
            .into_any()
    };
    view! { {banners(ready)} {cards} }.into_any()
}

fn words_tab(
    s: &crate::app::web::AppState,
    actions: &Actions,
    slot: &Slot<tc_api::helps::WordCard>,
    focus: Option<&[String]>,
    pane: &str,
    frame: Option<Memo<Option<u16>>>,
) -> AnyView {
    let Slot::Ready(ready) = slot else {
        return slot_status(actions, slot);
    };
    let words: Vec<_> = ready
        .items
        .iter()
        .filter(|w| in_focus(w.verse.as_str(), focus))
        .collect();
    let dir = dir_str(ready.dir);
    let active = s.u.helps_active.clone();
    let cards = if words.is_empty() {
        empty_tab(focus)
    } else {
        words
            .into_iter()
            .map(|w| {
                let verse = w.verse.as_str().to_owned();
                let (click, enter, leave) = card_events(actions, focus_of(&w.id, &verse, &w.panes));
                let lit = card_lit(&active, &w.id, &verse, frame);
                let is_active = Signal::from(lit);
                let title = HelpFocus::title(&w.panes, pane)
                    .or_else(|| (!w.quote.is_empty()).then(|| w.quote.clone()))
                    .unwrap_or_else(|| w.slug.clone());
                let mut attrs = Attrs::from([("data-testid", "helps-word")]);
                if let Some(dir) = dir {
                    attrs.set("dir", dir);
                }
                attrs.set("data-frame", verse.as_str());
                if lit {
                    attrs.set("data-focused", "true");
                }
                let slug = w.slug.clone();
                let link = ArticleLink::Words {
                    category: w.category.clone(),
                    slug: w.slug.clone(),
                };
                let actions = actions.clone();
                view! {
                        <HelpCard
                            kind=HelpKind::Word
                            attrs=attrs
                            verse=verse
                            active=is_active
                            on_click=click
                            on_mouse_enter=enter
                            on_mouse_leave=leave
                            title=title
                            body=Box::new(move || slug.clone().into_any())
                            action_label=t(keys::understand::WORD_LINK, &[])
                            on_action=Callback::new(move |()| actions.load_help_article(link.clone()))
                        />
                }
                .into_any()
            })
            .collect_view()
            .into_any()
    };
    view! { {banners(ready)} {cards} }.into_any()
}

fn questions_tab(
    s: &crate::app::web::AppState,
    slot: &Slot<tc_api::helps::QuestionCard>,
    focus: Option<&[String]>,
    frame: Option<Memo<Option<u16>>>,
) -> AnyView {
    let Slot::Ready(ready) = slot else {
        return match slot_state(slot) {
            Some((state, key)) => {
                let test_id = format!("helps-state-{state}");
                view! { <Callout tone=CalloutTone::Info attrs=Attrs::from([("data-testid", test_id.as_str())])>{t(key, &[])}</Callout> }.into_any()
            }
            None => ().into_any(),
        };
    };
    let questions: Vec<_> = ready
        .items
        .iter()
        .filter(|q| in_focus(q.verse.as_str(), focus))
        .collect();
    if questions.is_empty() {
        return view! { {banners(ready)} {empty_tab(focus)} }.into_any();
    }
    let dir = dir_str(ready.dir);
    let active = s.u.helps_active.clone();
    let q_style = crate::css! {
        "font-size": "var(--fs-ui-md)",
        "letter-spacing": "var(--track-14)",
        "font-weight": "var(--fw-heavy)",
        "color": "var(--uw-ocean)",
        "margin": "0 0 6px",
    }
    .to_style();
    let a_style = crate::css! {
        "font-size": "var(--fs-ui-sm)",
        "letter-spacing": "var(--track-13)",
        "line-height": "var(--lh-body)",
        "color": "var(--text-secondary)",
        "margin": 0,
    }
    .to_style();
    let answer =
        crate::css! { "color": "var(--text-tertiary)", "font-weight": "var(--fw-bold)" }.to_style();
    let cards = questions
        .into_iter()
        .map(|q| {
            let lit = card_lit(&active, &q.id, q.verse.as_str(), frame);
            let is_active = Memo::new(move |_| lit);
            let style = move || {
                crate::css! {
                    "border": format!("var(--stroke) solid {}", if is_active.get() { "var(--accent)" } else { "var(--border)" }),
                    "border-radius": "var(--radius-lg)",
                    "padding": 14,
                    "background": "#fff",
                }
                .to_style()
            };
            view! {
                <div data-testid="understand-question" dir=dir data-frame=q.verse.as_str().to_owned()
                    data-focused=move || is_active.get().then_some("true") style=style>
                    <p style=q_style.clone()>{q.question.clone()}</p>
                    <p style=a_style.clone()>
                        <span style=answer.clone()>{t(keys::understand::ANSWER, &[])}" · "</span>
                        {q.response.clone()}
                    </p>
                </div>
            }
        })
        .collect_view();
    view! { {banners(ready)} {cards} }.into_any()
}

fn simplified_tab(
    slot: &Slot<tc_api::helps::SimplifiedVerse>,
    focus: Option<&[String]>,
) -> AnyView {
    let Slot::Ready(ready) = slot else {
        return match slot_state(slot) {
            Some((state, key)) => {
                let test_id = format!("helps-state-{state}");
                view! { <Callout tone=CalloutTone::Info attrs=Attrs::from([("data-testid", test_id.as_str())])>{t(key, &[])}</Callout> }.into_any()
            }
            None => ().into_any(),
        };
    };
    let verses: Vec<_> = ready
        .items
        .iter()
        .filter(|v| in_focus(v.key.as_str(), focus))
        .collect();
    let boxed = crate::css! {
        "border": "var(--stroke) solid var(--border)",
        "border-radius": "var(--radius-lg)",
        "padding": 16,
        "background": "var(--surface-app)",
    };
    let p = crate::css! {
        "text-align": "start",
        "font-family": "var(--font-scripture)",
        "font-size": "var(--fs-verse)",
        "line-height": "var(--lh-verse)",
        "color": "var(--text-scripture)",
        "margin": "10px 0 0",
    };
    // #621: the verse numbers are superscript, as in the main passage.
    let sup = crate::css! {
        "font-size": "var(--fs-label)",
        "letter-spacing": "var(--track-11)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--text-tertiary)",
        "margin-inline-end": 3,
        "vertical-align": "super",
    }
    .to_style();
    let dir = (!verses.is_empty()).then(|| dir_str(ready.dir)).flatten();
    let body = if verses.is_empty() {
        t(
            if focus.is_some() {
                keys::understand::NONE_IN_FOCUS
            } else {
                keys::understand::NONE_FOR_CHAPTER
            },
            &[],
        )
        .into_any()
    } else {
        verses
            .into_iter()
            .map(|v| {
                // In another numbering, the source verse's place (tC4's `simplifiedVerses`).
                let n = v
                    .source
                    .clone()
                    .unwrap_or_else(|| v.key.as_str().to_owned());
                view! { <sup style=sup.clone()>{n}</sup>{v.text.clone()}" " }
            })
            .collect_view()
            .into_any()
    };
    view! {
        {banners(ready)}
        <div style=boxed.to_style() data-testid="understand-simplified">
            <Overline>{t(keys::understand::SIMPLIFIED_TITLE, &[])}</Overline>
            <p dir=dir style=p.to_style()>{body}</p>
        </div>
    }
    .into_any()
}

/// A block's spans, bold and italic as elements (tC4's `BlockText`, #619).
fn block_text(block: &ArticleBlock) -> AnyView {
    block
        .spans
        .iter()
        .map(|span| {
            let text = span.text.clone();
            match (span.bold, span.italic) {
                (true, true) => view! { <strong><em>{text}</em></strong> }.into_any(),
                (true, false) => view! { <strong>{text}</strong> }.into_any(),
                (false, true) => view! { <em>{text}</em> }.into_any(),
                (false, false) => text.into_any(),
            }
        })
        .collect_view()
        .into_any()
}

/// A table's rows (#628): the heading row's cells as `th`, the rest as `td`, each cell's marks
/// drawn.
fn table_view(rows: &[&ArticleBlock]) -> AnyView {
    let table = crate::css! {
        "border-collapse": "collapse",
        "margin": "4px 0 10px",
        "font-size": "inherit",
    };
    let cell = crate::css! {
        "border": "var(--stroke-hair) solid var(--border)",
        "padding": "4px 8px",
        "text-align": "start",
        "vertical-align": "top",
    }
    .to_style();
    let quote = rows.first().map_or(0, |r| r.quote);
    let body = rows
        .iter()
        .map(|row| {
            let cells = row
                .cells
                .iter()
                .map(|spans| {
                    let inner = block_text(&ArticleBlock::new(BlockKind::P, 0, spans.clone()));
                    if row.header {
                        view! { <th style=cell.clone()>{inner}</th> }.into_any()
                    } else {
                        view! { <td style=cell.clone()>{inner}</td> }.into_any()
                    }
                })
                .collect_view();
            view! { <tr>{cells}</tr> }
        })
        .collect_view();
    quoted(
        quote,
        view! { <table style=table.to_style()><tbody>{body}</tbody></table> }.into_any(),
    )
}

/// `blocks` drawn one by one with `one`, a run of table rows as one table (#628).
fn with_tables(blocks: &[ArticleBlock], one: impl Fn(&ArticleBlock) -> AnyView) -> AnyView {
    let mut out: Vec<AnyView> = Vec::new();
    let mut rows: Vec<&ArticleBlock> = Vec::new();
    for b in blocks {
        if b.kind == BlockKind::Tr {
            if b.header && !rows.is_empty() {
                out.push(table_view(&rows));
                rows.clear();
            }
            rows.push(b);
            continue;
        }
        if !rows.is_empty() {
            out.push(table_view(&rows));
            rows.clear();
        }
        out.push(one(b));
    }
    if !rows.is_empty() {
        out.push(table_view(&rows));
    }
    out.into_iter().collect_view().into_any()
}

/// How far a list item is set in: tC4's 14 px, and 14 more for each level it is nested (#628).
fn list_indent(b: &ArticleBlock) -> u32 {
    match b.kind {
        BlockKind::Li | BlockKind::Ol => 14 + 14 * u32::from(b.depth),
        _ => 0,
    }
}

/// A numbered item's number, before its text (#628).
fn number_mark(b: &ArticleBlock) -> Option<String> {
    (b.kind == BlockKind::Ol).then(|| format!("{}. ", b.number.as_deref().unwrap_or_default()))
}

/// A block inside `depth` quotes (tC4's `Quoted`): one bar per level.
fn quoted(depth: u8, inner: AnyView) -> AnyView {
    if depth == 0 {
        return inner;
    }
    let style = crate::css! {
        "display": "flow-root",
        "margin": 0,
        "padding-inline-start": 12,
        "border-inline-start": "3px solid var(--border-strong)",
    };
    view! { <blockquote style=style.to_style()>{quoted(depth - 1, inner)}</blockquote> }.into_any()
}

/// A note's blocks as spans, so the last sits beside "Show more" (tC4's `NoteBlocks`).
fn note_blocks(blocks: &[ArticleBlock]) -> AnyView {
    with_tables(blocks, |b| {
        let heading = b.kind == BlockKind::H;
        let mut style = crate::css! { "display": "block" };
        style.set(
            "font-weight",
            if heading {
                "var(--fw-heavy)"
            } else {
                "var(--fw-regular)"
            },
        );
        style.set(
            "color",
            if heading {
                "var(--uw-ocean)"
            } else {
                "inherit"
            },
        );
        style.set("margin", if heading { "8px 0 2px" } else { "0 0 6px" });
        style.set("padding-inline-start", list_indent(b));
        let bullet = (b.kind == BlockKind::Li).then_some("• ");
        let number = number_mark(b);
        let span = view! { <span style=style.to_style()>{bullet}{number}{block_text(b)}</span> }
            .into_any();
        quoted(b.quote, span)
    })
}

/// A note, cut to a preview with "Show more" when long (tC4's `ExpandableNote`).
pub(crate) fn expandable_note(blocks: Vec<ArticleBlock>) -> impl IntoView {
    if blocks_len(&blocks) <= PREVIEW_CHARS {
        return note_blocks(&blocks);
    }
    let expanded = RwSignal::new(false);
    let preview = preview_blocks(&blocks, PREVIEW_CHARS);
    let button = crate::css! {
        "border": 0,
        "background": "transparent",
        "cursor": "pointer",
        "font-family": "var(--font-ui)",
        "font-weight": "var(--fw-bold)",
        "font-size": "var(--fs-caption)",
        "letter-spacing": "var(--track-12)",
        "color": "var(--accent)",
        "padding": 0,
    };
    view! {
        {move || if expanded.get() { note_blocks(&blocks) } else { note_blocks(&preview) }}
        <button
            type="button"
            data-testid="note-expand"
            aria-expanded=move || if expanded.get() { "true" } else { "false" }
            on:click=move |e: web_sys::MouseEvent| {
                e.stop_propagation();
                expanded.update(|v| *v = !*v);
            }
            style=button.to_style()
        >
            {move || t(if expanded.get() { keys::understand::SHOW_LESS } else { keys::understand::SHOW_MORE }, &[])}
        </button>
    }
    .into_any()
}

/// The article pane (tC4's `ArticleView`): its header stays in view while it scrolls (#601).
fn article_view(actions: &Actions, state: ArticleState) -> AnyView {
    let pane = crate::css! {
        "border-top": "var(--stroke) solid var(--border-strong)",
        "max-height": "45%",
        "flex": "none",
        "display": "flex",
        "flex-direction": "column",
        "background": "#fff",
    };
    let head = crate::css! {
        "display": "flex",
        "align-items": "center",
        "gap": 8,
        "padding": "8px 16px",
        "flex": "none",
        "background": "var(--surface-muted)",
        "border-bottom": "var(--stroke-hair) solid var(--border-hair)",
    };
    let title_style = crate::css! {
        "flex": 1,
        "font-size": "var(--fs-ui-sm)",
        "letter-spacing": "var(--track-13)",
        "font-weight": "var(--fw-heavy)",
        "color": "var(--uw-ocean)",
    };
    let body = crate::css! {
        "flex": "1 1 auto",
        "min-height": 0,
        "overflow": "auto",
        "padding": 16,
    };
    let (title, title_dir) = match &state {
        ArticleState::Loading(_) => (t(keys::check::ARTICLE_LOADING, &[]), None),
        ArticleState::Read(_, ArticleView::Found { title, dir, .. }) => {
            (title.clone(), dir_str(Some(*dir)))
        }
        ArticleState::Read(_, ArticleView::Missing) | ArticleState::Failed(..) => {
            (String::new(), None)
        }
    };
    let content = match state {
        ArticleState::Loading(_) => ().into_any(),
        ArticleState::Failed(link, error) => {
            let actions = actions.clone();
            view! {
                <Callout tone=CalloutTone::Warn style=crate::css! { "overflow-wrap": "anywhere" }
                    attrs=Attrs::from([("role", "alert"), ("data-testid", "understand-article-error")])>
                    {t(keys::understand::ARTICLE_ERROR, &[])}" "{error}" "
                    <Button size=Size::Sm variant=ButtonVariant::Outline attrs=Attrs::from([("data-testid", "article-retry")])
                        on_click=Callback::new(move |_| actions.load_help_article(link.clone()))>
                        {t(keys::app::RETRY, &[])}
                    </Button>
                </Callout>
            }
            .into_any()
        }
        ArticleState::Read(_, ArticleView::Missing) => view! {
            <Callout tone=CalloutTone::Warn attrs=Attrs::from([("data-testid", "understand-article-missing")])>
                {t(keys::check::ARTICLE_MISSING, &[])}
            </Callout>
        }
        .into_any(),
        ArticleState::Read(_, ArticleView::Found { blocks, dir, .. }) => {
            let paras = with_tables(&blocks, |b| {
                    let heading = b.kind == BlockKind::H;
                    let mut style = crate::css! {};
                    style.set("font-size", if heading { "var(--fs-caption-lg)" } else { "var(--fs-ui-sm)" });
                    style.set("font-weight", if heading { "var(--fw-heavy)" } else { "var(--fw-regular)" });
                    style.set("color", if heading { "var(--uw-ocean)" } else { "var(--text-secondary)" });
                    style.set("line-height", "var(--lh-body)");
                    style.set("margin", if heading { "12px 0 4px" } else { "0 0 8px" });
                    style.set("padding-inline-start", list_indent(b));
                    let number = number_mark(b);
                    quoted(b.quote, view! { <p style=style.to_style()>{number}{block_text(b)}</p> }.into_any())
                });
            view! { <div data-testid="understand-article" dir=dir_str(Some(dir))>{paras}</div> }.into_any()
        }
    };
    let close = {
        let actions = actions.clone();
        Callback::new(move |_| actions.close_help_article())
    };
    view! {
        <div data-testid="helps-article-pane" style=pane.to_style()>
            <div style=head.to_style()>
                <span data-testid="helps-article-title" dir=title_dir style=title_style.to_style()>{title}</span>
                <IconButton size=26.0 title=t(keys::common::CLOSE, &[]) attrs=Attrs::from([("data-testid", "helps-article-close")]) on_click=close>
                    "✕"
                </IconButton>
            </div>
            <div style=body.to_style()>{content}</div>
        </div>
    }
    .into_any()
}
