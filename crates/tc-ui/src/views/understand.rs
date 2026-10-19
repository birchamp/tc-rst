//! The port of tC4's `Understand.jsx` (unit B3): the source passage of the open chapter, by
//! section or by verse, each unit with its user-comment box, beside the book rail and the
//! helps panel. Focusing a unit focuses its first help card.
//!
//! The screen holds three reads: `editor/book` (the rail, the chapters, the comments) per
//! book, and `editor/sources` and `helps/passage` per chapter. The comment box shows the
//! comment tC4's rule picks and is read-only until C3 saves it, so nothing typed into it can
//! be lost (a declared difference until then).

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::editor::{
    Book as BookRead, BookView, CommentView, PaneChapter, PaneText, Sources, SourcesView,
};
use tc_api::helps::{Passage, PassageView, Slot};
use tc_api::ids::Scope;
use tc_api::shown::Direction as TextDir;
use tc_core::BookId;

use super::book_rail::book_rail;
use super::helps_panel::{HelpsPanel, HelpsToggleButton};
use super::panel_icons::rail_icon;
use super::source_verse::source_verse;
use crate::app::load::Load;
use crate::app::understand::{
    HelpFocus, Unit, UnitMode, literal_pane, paragraphs, shown_comment, units,
};
use crate::app::web::{Actions, AppCtx, AppState, use_app, use_runtime};
use crate::attrs::Attrs;
use crate::ds::{
    Button, ButtonVariant, Callout, CalloutTone, FilterChip, IconButton, Indicator, Overline, Size,
    Status, StatusDot, Switcher, SwitcherOption, Tone,
};
use crate::i18n::{keys, t, t_dyn};

/// The reads of one open book.
#[derive(Clone, PartialEq)]
struct Open {
    at: Scope,
    book: BookId,
}

/// tC4's `Understand`.
#[component]
pub fn Understand() -> impl IntoView {
    let (Some(AppCtx { s, actions }), Some(_)) = (use_app(), use_runtime()) else {
        return ().into_any();
    };
    let open = {
        let (project, book) = (s.project.clone(), s.u.book.clone());
        Memo::new(move |_| {
            let at = project.with(|p| p.as_ref().map(|p| p.at.clone()))?;
            Some(Open {
                at,
                book: book.get()?,
            })
        })
    };
    (move || match open.get() {
        Some(open) => understand_book(&s, &actions, open).into_any(),
        None => no_book().into_any(),
    })
    .into_any()
}

/// No book yet: tC4's "Loading book…".
fn no_book() -> impl IntoView {
    let style = crate::css! {
        "flex": 1,
        "display": "flex",
        "align-items": "center",
        "justify-content": "center",
        "color": "var(--text-tertiary)",
        "font-size": "var(--fs-ui)",
    };
    view! { <div style=style.to_style()>{move || t(keys::draft::LOADING, &[])}</div> }
}

fn understand_book(s: &AppState, actions: &Actions, open: Open) -> AnyView {
    let Some(runtime) = use_runtime() else {
        return ().into_any();
    };
    let book_read = runtime.cache().read(BookRead {
        at: open.at.clone(),
        book: open.book,
    });
    // The open is over once the screen has its book (tC4 clears `s.opening` after
    // `openBook`), or could not read it.
    {
        let actions = actions.clone();
        Effect::new(move |_| {
            if book_read.with(|b| !b.is_pending()) {
                actions.book_shown();
            }
        });
    }
    let book = Memo::new(move |_| book_read.with(|b| b.value().cloned()));
    let failed = Memo::new(move |_| book_read.with(|b| b.error().map(ToString::to_string)));
    let s = s.clone();
    let actions = actions.clone();
    (move || match (book.get(), failed.get()) {
        (Some(view), _) => screen(&s, &actions, &open, view).into_any(),
        (None, Some(error)) => {
            let style = crate::css! {
                "flex": 1,
                "display": "flex",
                "align-items": "center",
                "justify-content": "center",
                "color": "var(--text-tertiary)",
                "font-size": "var(--fs-ui)",
            };
            view! { <div style=style.to_style()>{move || t(keys::draft::LOAD_ERROR, &[])}" "{error.clone()}</div> }.into_any()
        }
        (None, None) => no_book().into_any(),
    })
    .into_any()
}

/// The screen over the open book's read.
fn screen(s: &AppState, actions: &Actions, open: &Open, view: Arc<BookView>) -> AnyView {
    let root = crate::css! { "flex": 1, "display": "flex", "min-height": 0 };
    let rail = {
        let (s2, actions, view) = (s.clone(), actions.clone(), view.clone());
        let on = s.u.rail.clone();
        move || on.get().then(|| book_rail(&s2, &actions, &view))
    };
    let chapter = s.u.chapter.clone();
    let rest = {
        let (s, actions, open, view) = (s.clone(), actions.clone(), open.clone(), view.clone());
        move || {
            let c = chapter.get();
            chapter_view(&s, &actions, &open, &view, c)
        }
    };
    view! {
        <div style=root.to_style() data-testid="understand">
            {rail}
            {rest}
        </div>
    }
    .into_any()
}

/// `main` (the header and the passage of one chapter) and the helps panel beside it.
fn chapter_view(
    s: &AppState,
    actions: &Actions,
    open: &Open,
    book: &BookView,
    chapter: u16,
) -> AnyView {
    let Some(runtime) = use_runtime() else {
        return ().into_any();
    };
    let sources = runtime.cache().read(Sources {
        at: open.at.clone(),
        book: open.book,
        chapter,
    });
    let helps = runtime.cache().read(Passage {
        at: open.at.clone(),
        book: open.book,
        chapter,
    });
    let book_id = open.book;
    let (mode, tab) = (s.u.unit_mode.clone(), s.u.source_tab.clone());
    let panes = Memo::new(move |_| sources.with(|l| l.value().cloned()));
    let chosen = Memo::new(move |_| {
        let tab = tab.get();
        panes.with(|p| p.as_ref().and_then(|v| shown_tab(v, &tab)))
    });
    let chapter_text = Memo::new(move |_| {
        let id = chosen.get();
        panes.with(|p| {
            p.as_ref()
                .and_then(|v| v.panes.iter().find(|pane| Some(&pane.id) == id.as_ref()))
                .and_then(|pane| match &pane.text {
                    PaneText::Ready(c) => Some(c.clone()),
                    _ => None,
                })
        })
    });
    let all = Memo::new(move |_| {
        let mode = mode.get();
        chapter_text.with(|c| units(mode, book_id, chapter, c.as_ref()))
    });
    let active_key: RwSignal<Option<String>> = RwSignal::new(None);
    let focused_key = Memo::new(move |_| {
        let picked = active_key.get();
        all.with(|units| {
            picked
                .filter(|k| units.iter().any(|u| &u.key == k))
                .or_else(|| units.first().map(|u| u.key.clone()))
        })
    });
    let focus_verses = Memo::new(move |_| {
        let key = focused_key.get();
        all.with(|units| {
            units
                .iter()
                .find(|u| Some(&u.key) == key.as_ref())
                .map(|u| u.verses.clone())
        })
    });
    let focus = {
        let (hover, active) = (s.u.helps_hover.clone(), s.u.helps_active.clone());
        Memo::new(move |_| hover.get().or_else(|| active.get()))
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
    let controls = crate::css! {
        "display": "flex",
        "align-items": "center",
        "gap": 6,
        "margin-bottom": 4,
        "flex-wrap": "wrap",
    };
    let chips = {
        let actions = actions.clone();
        move || {
            let tab = chosen.get();
            panes.with(|p| {
                p.as_ref().map(|v| {
                    v.panes
                        .iter()
                        .map(|pane| {
                            source_chip(
                                &actions,
                                &pane.id,
                                tab.as_deref() == Some(pane.id.as_str()),
                            )
                        })
                        .collect_view()
                })
            })
        }
    };
    let switch = unit_switch(s, actions);
    let source_name = move || {
        chosen.get().map(|id| {
            let style = crate::css! {
                "font-size": "var(--fs-meta)",
                "letter-spacing": "var(--track-11-5)",
                "color": "var(--text-tertiary)",
                "font-weight": "var(--fw-medium)",
                "margin": "0 0 14px",
            };
            let name = t_dyn(&format!("source.{id}.name"), &[], Some(&id.to_uppercase()));
            view! { <p data-testid="understand-source-name" style=style.to_style()>{name}</p> }
        })
    };
    let status = {
        let actions = actions.clone();
        move || {
            let tab = chosen.get();
            sources.with(|loaded| {
                panes.with(|p| passage_status(&actions, loaded, p.as_ref(), tab.as_deref()))
            })
        }
    };
    let comments = Arc::new(book.comments.clone());
    let unit_views = {
        let (s, actions) = (s.clone(), actions.clone());
        move || {
            let pane = chosen.get().unwrap_or_default();
            chapter_text.with(|text| {
                text.as_ref().map(|text| {
                    all.with(|units| {
                        units
                            .iter()
                            .map(|unit| {
                                understand_unit(
                                    &s,
                                    &actions,
                                    chapter,
                                    unit,
                                    text,
                                    &pane,
                                    focus,
                                    focused_key,
                                    active_key,
                                    &comments,
                                    helps,
                                )
                            })
                            .collect_view()
                    })
                })
            })
        }
    };
    let panel = move || {
        let pane = chosen.get().unwrap_or_default();
        view! { <HelpsPanel chapter=chapter focus_verses=focus_verses helps=helps pane=pane /> }
    };
    view! {
        <main style=main.to_style()>
            {header(actions, book_id, chapter)}
            <div style=scroll.to_style()>
                <div style=column.to_style()>
                    <div style=controls.to_style()>
                        {chips}
                        <div style="flex: 1;"></div>
                        {switch}
                    </div>
                    {source_name}
                    {status}
                    {unit_views}
                </div>
            </div>
        </main>
        {panel}
    }
    .into_any()
}

/// The source text's pill (tC4's `FilterChip` of a pane).
fn source_chip(actions: &Actions, id: &str, selected: bool) -> AnyView {
    let mut style = crate::css! {
        "display": "inline-block",
        "padding": "4px 10px",
        "font-size": "var(--fs-label)",
        "letter-spacing": "var(--track-11)",
        "border-width": 1,
    };
    style.extend(&if selected {
        crate::css! { "background": "var(--accent)", "color": "var(--text-inverse)", "border-color": "var(--accent)" }
    } else {
        crate::css! { "background": "var(--surface-card)", "color": "var(--text-heading)", "border-color": "var(--border-input)" }
    });
    let label = t_dyn(&format!("source.{id}"), &[], Some(&id.to_uppercase()));
    let actions = actions.clone();
    let click = id.to_owned();
    let test_id = format!("source-tab-{id}");
    view! {
        <FilterChip
            selected=selected
            style=style
            attrs=Attrs::from([("data-testid", test_id.as_str())])
            on_click=Callback::new(move |_| actions.set_source_tab(click.clone()))
        >
            {label.clone()}
        </FilterChip>
    }
    .into_any()
}

/// "Comment by" with the Section/Verse switch.
fn unit_switch(s: &AppState, actions: &Actions) -> impl IntoView + use<> {
    let mode = s.u.unit_mode.clone();
    let actions = actions.clone();
    let value = Signal::derive(move || {
        Some(match mode.get() {
            UnitMode::Section => "section".to_owned(),
            UnitMode::Verse => "verse".to_owned(),
        })
    });
    let options = vec![
        SwitcherOption::new("section", t(keys::understand::BY_SECTION, &[])),
        SwitcherOption::new("verse", t(keys::understand::BY_VERSE, &[])),
    ];
    let on_change = Callback::new(move |v: String| {
        actions.set_unit_mode(if v == "verse" {
            UnitMode::Verse
        } else {
            UnitMode::Section
        });
    });
    view! {
        <Overline>{t(keys::understand::COMMENTS_BY, &[])}</Overline>
        <Switcher indicator=Indicator::Pill size=Size::Sm tone=Tone::Ocean value=value on_change=on_change options=options />
    }
}

/// The source tab the passage shows: the chosen one when the project has it, else its
/// literal pane (tC4's `literalPaneId`).
fn shown_tab(view: &SourcesView, tab: &str) -> Option<String> {
    let ids: Vec<String> = view.panes.iter().map(|p| p.id.clone()).collect();
    if ids.iter().any(|id| id == tab) {
        return Some(tab.to_owned());
    }
    literal_pane(&ids)
}

fn header(actions: &Actions, book: BookId, chapter: u16) -> impl IntoView + use<> {
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
            <h2 style=h2.to_style()>{book.english_name()}" "{chapter}</h2>
            <span style=note.to_style()>{move || t(keys::understand::NOTE, &[])}</span>
            <HelpsToggleButton />
        </div>
    }
}

/// tC4's `PassageStatus`: no pane, an absent or failed pane, or the passage still loading.
fn passage_status(
    actions: &Actions,
    sources: &Load<Arc<SourcesView>>,
    panes: Option<&Arc<SourcesView>>,
    tab: Option<&str>,
) -> AnyView {
    let callout = crate::css! { "margin-top": 10 };
    let pane = panes.and_then(|v| v.panes.iter().find(|p| Some(p.id.as_str()) == tab));
    let none = panes.filter(|v| v.panes.is_empty()).map(|_| {
        view! {
            <Callout tone=CalloutTone::Info style=callout.clone() attrs=Attrs::from([("data-testid", "no-source-panes")])>
                {t(keys::understand::NO_SOURCE_PANES, &[])}
            </Callout>
        }
    });
    let absent = pane.and_then(|p| match &p.text {
        PaneText::NotInstalled => Some(keys::source::NOT_INSTALLED),
        PaneText::Missing => Some(keys::source::UNAVAILABLE),
        _ => None,
    }).map(|key| view! { <Callout tone=CalloutTone::Info style=callout.clone()>{t(key, &[])}</Callout> });
    let error = pane
        .and_then(|p| match &p.text {
            PaneText::Error { reason } => Some(reason.clone()),
            _ => None,
        })
        .or_else(|| sources.error().map(ToString::to_string))
        .map(|reason| {
            let mut style = callout.clone();
            style.set("overflow-wrap", "anywhere");
            let actions = actions.clone();
            view! {
                <Callout tone=CalloutTone::Warn style=style attrs=Attrs::from([("role", "alert"), ("data-testid", "source-pane-error")])>
                    {t(keys::understand::SOURCE_ERROR, &[])}" "{reason}" "
                    <Button size=Size::Sm variant=ButtonVariant::Outline attrs=Attrs::from([("data-testid", "source-retry")])
                        on_click=Callback::new(move |_| actions.reload_sources())>
                        {t(keys::app::RETRY, &[])}
                    </Button>
                </Callout>
            }
        });
    let loading =
        (pane.is_none() && panes.is_none_or(|v| !v.panes.is_empty()) && sources.error().is_none())
            .then(|| {
                let style = crate::css! {
                    "font-size": "var(--fs-ui-sm)",
                    "color": "var(--text-tertiary)",
                    "margin-top": 10,
                };
                view! { <p style=style.to_style()>{t(keys::understand::LOADING, &[])}</p> }
            });
    view! { {none} {absent} {error} {loading} }.into_any()
}

/// The first note or key word in a unit's verses (tC4's `firstHelpIn`).
fn first_help_in(helps: Option<&PassageView>, unit: &Unit) -> Option<HelpFocus> {
    let helps = helps?;
    let carries = |verse: &str| {
        unit.verses
            .iter()
            .any(|k| crate::app::understand::key_carries(k, verse))
    };
    if let Slot::Ready(notes) = &helps.notes
        && let Some(n) = notes.items.iter().find(|n| carries(n.verse.as_str()))
    {
        return Some(HelpFocus {
            id: n.id.clone(),
            verse: n.verse.as_str().to_owned(),
            panes: n.panes.clone(),
        });
    }
    if let Slot::Ready(words) = &helps.words
        && let Some(w) = words.items.iter().find(|w| carries(w.verse.as_str()))
    {
        return Some(HelpFocus {
            id: w.id.clone(),
            verse: w.verse.as_str().to_owned(),
            panes: w.panes.clone(),
        });
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn understand_unit(
    s: &AppState,
    actions: &Actions,
    chapter: u16,
    unit: &Unit,
    text: &PaneChapter,
    pane: &str,
    focus: Memo<Option<HelpFocus>>,
    focused_key: Memo<Option<String>>,
    active_key: RwSignal<Option<String>>,
    comments: &[CommentView],
    helps: Signal<Load<Arc<PassageView>>>,
) -> AnyView {
    let shown = shown_comment(comments, chapter, unit);
    let key = unit.key.clone();
    let focused = Memo::new({
        let key = key.clone();
        move |_| focused_key.get().as_deref() == Some(key.as_str())
    });
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
    let sup = crate::css! {
        "font-size": "var(--fs-label)",
        "letter-spacing": "var(--track-11)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--text-tertiary)",
        "margin-inline-end": 3,
        "vertical-align": "super",
    }
    .to_style();
    let paras = paragraphs(unit, text)
        .into_iter()
        .map(|(verses, level)| {
            let mut p = crate::css! {
                "direction": "ltr",
                "text-align": "start",
                "font-family": "var(--font-scripture)",
                "font-size": "var(--fs-verse-lg)",
                "line-height": "var(--lh-verse-lg)",
                "color": "var(--text-scripture)",
                "margin": "10px 0",
            };
            match level {
                1 => p.set("padding-inline-start", "1.5em"),
                2 => p.set("padding-inline-start", "3em"),
                _ => {}
            }
            let body = verses
                .into_iter()
                .map(|v| {
                    let verse = v.clone();
                    let pane = pane.to_owned();
                    view! {
                        <sup style=sup.clone()>{v.key.as_str().to_owned()}</sup>
                        {move || focus.with(|f| source_verse(&verse, &pane, f.as_ref()))}
                        " "
                    }
                })
                .collect_view();
            view! { <p style=p.to_style()>{body}</p> }
        })
        .collect_view();
    let on_focus = {
        let s = s.clone();
        let unit = unit.clone();
        move || {
            active_key.set(Some(unit.key.clone()));
            let next =
                helps.with_untracked(|h| first_help_in(h.value().map(|h| h.as_ref()), &unit));
            let had = s.u.helps_active.get_untracked();
            let changed = match (&next, &had) {
                (Some(item), Some(active)) => item.id != active.id,
                (Some(_), None) => true,
                (None, had) => had.is_some(),
            };
            if changed {
                s.u.helps_active.try_set(next);
            }
        }
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
    let test_id = format!("understand-unit-{}", unit.key);
    let label = unit.label.clone();
    let dot = shown
        .any
        .then(|| view! { <StatusDot status=Status::Valid size=7.0 /> });
    let _ = actions;
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
            {paras}
            {comment_box(s, &shown)}
        </div>
    }
    .into_any()
}

/// The user-comment box, as tC4's `ComprehensionBox` shows it, read-only until C3 writes it.
fn comment_box(s: &AppState, shown: &crate::app::understand::ShownComment) -> AnyView {
    let dir = match s.project.with_untracked(|p| p.as_ref().map(|p| p.dir)) {
        Some(TextDir::Rtl) => "rtl",
        _ => "ltr",
    };
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
    let placeholder = t(
        match s.u.unit_mode.get_untracked() {
            UnitMode::Verse => keys::understand::COMMENTS_PLACEHOLDER_VERSE,
            UnitMode::Section => keys::understand::COMMENTS_PLACEHOLDER,
        },
        &[],
    );
    let hidden = (shown.hidden > 0).then(|| {
        let style = crate::css! {
            "font-size": "var(--fs-caption)",
            "letter-spacing": "var(--track-12)",
            "color": "var(--text-tertiary)",
            "margin": "6px 0 0",
        };
        let n = shown.hidden.to_string();
        view! {
            <p data-testid="understand-notes-in-section" style=style.to_style()>
                {t(keys::understand::NOTES_IN_SECTION, &[("n", &n)])}
            </p>
        }
    });
    let text = shown.text.clone();
    view! {
        <textarea
            data-i="field"
            rows="3"
            dir=dir
            readonly=true
            prop:value=text
            style=style.to_style()
            placeholder=placeholder
        ></textarea>
        {hidden}
    }
    .into_any()
}
