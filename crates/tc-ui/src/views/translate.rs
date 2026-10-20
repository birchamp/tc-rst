//! The port of tC4's `Draft.jsx` (unit B4a): Translate, the source beside the translation, one
//! row per translator's section, verses running on as paragraphs; Section or Verse; the verse
//! card that revises one verse; the conflict markers; the book rail and the helps panel with
//! its Comments tab.
//!
//! The screen holds four reads: `editor/book` (the rail, the chapters, the comments) and
//! `editor/chapters` (the translation of every chapter, AD16) per book, and `editor/sources`
//! and `helps/passage` per chapter. The verse card is a save-machine field (6.5): its value and
//! token are the pair it opened with (AD37), a refresh of the chapter's read goes to the machine,
//! and the card says when a change came in while the person typed. The section card is
//! `section_editor`'s: it replaces the cell of the row that holds its first verse.

use std::sync::Arc;

use leptos::html;
use leptos::prelude::*;
use send_wrapper::SendWrapper;
use tc_api::buffer::BufferKey;
use tc_api::editor::{
    Book as BookRead, BookView, Chapters, ChaptersView, PaneChapter, PaneText, SourceFrame,
    SourcePane, Sources, SourcesView, VerseRow,
};
use tc_api::event::{ChangeScope, Event, SessionEvent};
use tc_api::helps::Passage;
use tc_api::ids::{Scope, ViewSeq};
use tc_api::shown::{Direction as TextDir, PersonLabel};
use tc_core::{BookId, Testament};
use wasm_bindgen::JsCast;

use super::book_rail::book_rail;
use super::conflict_markers::{conflict_verse, conflicts_button};
use super::draft_chrome::{HAIR, cell, chip_style, draft_pill, editing_card};
use super::helps_panel::{HelpsPanel, HelpsToggleButton};
use super::panel_icons::rail_icon;
use super::section_editor::SectionEditor;
use super::source_verse::source_verse;
use crate::app::save::{FieldScope, Pair, Source, View};
use crate::app::shell::Editing;
use crate::app::translate::{
    SourcePlace, box_text, changed_while_typing, editing_focus, editing_section, editing_verse,
    range_span, sections, shown_words, source_place, source_unit, target_paragraphs,
};
use crate::app::understand::{HelpFocus, UnitMode, literal_pane};
use crate::app::web::{Actions, AppCtx, AppState, use_app, use_runtime};
use crate::attrs::Attrs;
use crate::ds::{
    Button, ButtonVariant, Callout, CalloutTone, FilterChip, IconButton, Indicator, Overline,
    OverlineTone, Size, Switcher, SwitcherOption, Tone,
};
use crate::i18n::{keys, t, t_dyn};

/// The reads of one open book.
#[derive(Clone, PartialEq)]
struct Open {
    at: Scope,
    book: BookId,
}

/// tC4's `SUP`: a verse number.
fn sup() -> String {
    crate::css! {
        "font-size": "var(--fs-label)",
        "letter-spacing": "var(--track-11)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--text-tertiary)",
        "margin-inline-end": 3,
        "vertical-align": "super",
    }
    .to_style()
}

/// The translation's type at the design's reading step (tC4's `targetTypeFor(s, 'lg')`).
fn target_type() -> crate::css::Css {
    crate::css! {
        "font-family": "var(--font-scripture)",
        "font-size": "var(--fs-verse-lg)",
        "line-height": "var(--lh-verse-lg)",
    }
}

fn dir_of(s: &AppState) -> &'static str {
    match s.project.with_untracked(|p| p.as_ref().map(|p| p.dir)) {
        Some(TextDir::Rtl) => "rtl",
        _ => "ltr",
    }
}

/// tC4's `Draft`.
#[component]
pub fn Translate() -> impl IntoView {
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
        Some(open) => translate_book(&s, &actions, open).into_any(),
        None => centred(t(keys::draft::LOADING, &[])).into_any(),
    })
    .into_any()
}

/// tC4's "Loading book…" and its load error.
fn centred(text: String) -> impl IntoView {
    let style = crate::css! {
        "flex": 1,
        "display": "flex",
        "align-items": "center",
        "justify-content": "center",
        "color": "var(--text-tertiary)",
        "font-size": "var(--fs-ui)",
    };
    view! { <div style=style.to_style()>{text}</div> }
}

fn translate_book(s: &AppState, actions: &Actions, open: Open) -> AnyView {
    let Some(runtime) = use_runtime() else {
        return ().into_any();
    };
    let book_read = runtime.cache().read(BookRead {
        at: open.at.clone(),
        book: open.book,
    });
    let text_read = runtime.cache().read(Chapters {
        at: open.at.clone(),
        book: open.book,
    });
    {
        let actions = actions.clone();
        Effect::new(move |_| {
            if book_read.with(|b| !b.is_pending()) && text_read.with(|b| !b.is_pending()) {
                actions.book_shown();
            }
        });
    }
    let book = Memo::new(move |_| book_read.with(|b| b.value().cloned()));
    let text = Memo::new(move |_| text_read.with(|b| b.value().cloned()));
    // Only whether the reads are in: a refreshed read (after a save) changes what the screen
    // shows in place, never the screen, so an open card keeps its field.
    let ready = Memo::new(move |_| book.with(Option::is_some) && text.with(Option::is_some));
    let failed = Memo::new(move |_| {
        book_read
            .with(|b| b.error().map(ToString::to_string))
            .or_else(|| text_read.with(|b| b.error().map(ToString::to_string)))
    });
    let s = s.clone();
    let actions = actions.clone();
    (move || match (ready.get(), failed.get()) {
        (true, _) => screen(&s, &actions, &open, book, text).into_any(),
        (false, Some(error)) => {
            centred(format!("{} {error}", t(keys::draft::LOAD_ERROR, &[]))).into_any()
        }
        (false, None) => centred(t(keys::draft::LOADING, &[])).into_any(),
    })
    .into_any()
}

/// The screen over the open book's reads.
fn screen(
    s: &AppState,
    actions: &Actions,
    open: &Open,
    book: Memo<Option<Arc<BookView>>>,
    text: Memo<Option<Arc<ChaptersView>>>,
) -> AnyView {
    let root = crate::css! { "flex": 1, "display": "flex", "min-height": 0 };
    let rail = {
        let (s2, actions) = (s.clone(), actions.clone());
        let on = s.u.rail.clone();
        move || {
            on.get()
                .then(|| book.get().map(|view| book_rail(&s2, &actions, &view)))
                .flatten()
        }
    };
    let chapter = s.u.chapter.clone();
    let rest = {
        let (s, actions, open) = (s.clone(), actions.clone(), open.clone());
        move || {
            let c = chapter.get();
            chapter_view(&s, &actions, &open, book, text, c)
        }
    };
    // While a card is open its exits are off; a press on one keeps the focus in the card. An
    // off control takes no pointer event (`[data-i][disabled]` has `pointer-events: none`), so
    // the press lands on what is under it: it is stopped on its way down when it falls on an
    // off button there. A press anywhere else leaves the card, which saves it, as in tC4.
    let editing = s.editing.clone();
    let keep_focus = move |e: web_sys::PointerEvent| {
        if editing.with_untracked(Editing::is_open) && press_on_off_button(&e) {
            e.prevent_default();
        }
    };
    leptos::html::div()
        .attr("style", root.to_style())
        .on(leptos::ev::capture(leptos::ev::pointerdown), keep_focus)
        .child((rail, rest))
        .into_any()
}

/// Whether a press falls on an off button: the one it reached, or one that takes no pointer
/// event inside the element it reached.
fn press_on_off_button(e: &web_sys::PointerEvent) -> bool {
    let Some(target) = e
        .target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
    else {
        return false;
    };
    if target.closest("button:disabled").ok().flatten().is_some() {
        return true;
    }
    let (x, y) = (f64::from(e.client_x()), f64::from(e.client_y()));
    let Ok(buttons) = target.query_selector_all("button:disabled") else {
        return false;
    };
    (0..buttons.length())
        .filter_map(|i| buttons.item(i))
        .filter_map(|n| n.dyn_into::<web_sys::Element>().ok())
        .any(|b| {
            let r = b.get_bounding_client_rect();
            x >= r.left() && x <= r.right() && y >= r.top() && y <= r.bottom()
        })
}

/// The pane a source tab shows: a pane of the project, or the original-language text.
fn chosen_pane<'a>(view: &'a SourcesView, tab: &str) -> Option<&'a SourcePane> {
    if tab == "orig" {
        return view.original.as_ref();
    }
    view.panes.iter().find(|p| p.id == tab)
}

/// The tab shown: the chosen one when the project has it, else its literal pane.
fn shown_tab(view: &SourcesView, tab: &str) -> Option<String> {
    if chosen_pane(view, tab).is_some() {
        return Some(tab.to_owned());
    }
    let ids: Vec<String> = view.panes.iter().map(|p| p.id.clone()).collect();
    literal_pane(&ids)
}

fn testament_key(book: BookId) -> &'static str {
    match book.testament() {
        Testament::Old => "ot",
        Testament::New => "nt",
    }
}

#[allow(clippy::too_many_lines)]
fn chapter_view(
    s: &AppState,
    actions: &Actions,
    open: &Open,
    book: Memo<Option<Arc<BookView>>>,
    text: Memo<Option<Arc<ChaptersView>>>,
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
    let panes = Memo::new(move |_| sources.with(|l| l.value().cloned()));
    let tab = s.u.source_tab.clone();
    let chosen = Memo::new(move |_| {
        let tab = tab.get();
        panes.with(|p| p.as_ref().and_then(|v| shown_tab(v, &tab)))
    });
    let pane_text = Memo::new(move |_| {
        let id = chosen.get();
        panes.with(|p| {
            p.as_ref()
                .and_then(|v| id.as_deref().and_then(|id| chosen_pane(v, id)))
                .map(|pane| pane.text.clone())
        })
    });
    let frame = Memo::new(move |_| {
        panes.with(|p| p.as_ref().map_or(SourceFrame::Same, |v| v.frame.clone()))
    });
    let cross_frame = Memo::new(move |_| frame.with(|f| !matches!(f, SourceFrame::Same)));
    let chapter_text = Memo::new(move |_| {
        text.with(|v| {
            v.as_ref()
                .and_then(|v| v.chapters.iter().find(|c| c.chapter == chapter).cloned())
        })
    });
    let mode = {
        let unit = s.u.unit_mode.clone();
        Memo::new(move |_| {
            if cross_frame.get() {
                UnitMode::Verse
            } else {
                unit.get()
            }
        })
    };
    let all = Memo::new(move |_| {
        let ready = pane_text.with(|p| match p {
            Some(PaneText::Ready(c)) => Some(c.clone()),
            _ => None,
        });
        let cross = cross_frame.get();
        chapter_text.with(|c| {
            c.as_ref()
                .map(|c| sections(ready.as_ref(), c, cross))
                .unwrap_or_default()
        })
    });
    let editing = s.editing.clone();
    let off = {
        let editing = editing.clone();
        Signal::derive(move || editing.with(Editing::is_open))
    };
    let focus_verses = {
        let editing = editing.clone();
        Memo::new(move |_| editing.with(|e| editing_focus(e, book_id, chapter)))
    };
    // tC4's `crossFrameSafeFocus`: in another numbering no help lights the pane.
    let pane_focus = {
        let (hover, active) = (s.u.helps_hover.clone(), s.u.helps_active.clone());
        Memo::new(move |_| {
            if cross_frame.get() {
                None
            } else {
                hover.get().or_else(|| active.get())
            }
        })
    };
    let conflicts = Memo::new(move |_| text.with(|v| v.as_ref().map_or(0, |v| v.conflicts)));
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
    let scroll = crate::css! { "flex": 1, "overflow": "auto", "min-height": 0 };
    let grid = crate::css! {
        "display": "grid",
        "grid-template-columns": "1fr 1fr",
        "max-width": 1100,
        "margin": "0 auto",
    };
    let toggle = {
        let actions = actions.clone();
        Callback::new(move |_| actions.toggle_rail())
    };
    // B11 opens the conflict review from these; until then they show where conflicts are.
    let review = Callback::new(|()| {});
    let button = move || conflicts_button(conflicts.get(), off, review);
    let tabs = source_tabs(actions, book_id, panes, chosen);
    let head = target_head(s, actions, cross_frame, mode, off);
    let rows = {
        let (s, actions, open) = (s.clone(), actions.clone(), open.clone());
        move || {
            all.with(|groups| {
                groups
                    .iter()
                    .map(|keys| {
                        let span = range_span(keys);
                        let label = format!("{} {chapter}:{span}", book_id.english_name());
                        let source = {
                            let keys = keys.clone();
                            move || {
                                let pane = chosen.get().unwrap_or_default();
                                let place = frame.with(|f| match keys.as_slice() {
                                    [key] => source_place(book_id, chapter, key, f),
                                    _ => SourcePlace::Same,
                                });
                                pane_text.with(|text| {
                                    source_cell(book_id, &pane, text.as_ref(), &keys, &label, &place, pane_focus)
                                })
                            }
                        };
                        view! {
                            {source}
                            {target_cell(&s, &actions, book_id, chapter, text, keys, &span, mode, off, &open)}
                        }
                    })
                    .collect_view()
            })
        }
    };
    let comments = Signal::derive(move || {
        book.with(|b| b.as_ref().map(|b| b.comments.clone()).unwrap_or_default())
    });
    let panel = move || {
        let pane = chosen
            .get()
            .filter(|id| id != "orig")
            .or_else(|| {
                panes.with(|p| {
                    p.as_ref().and_then(|v| {
                        literal_pane(&v.panes.iter().map(|p| p.id.clone()).collect::<Vec<_>>())
                    })
                })
            })
            .unwrap_or_default();
        view! { <HelpsPanel chapter=chapter focus_verses=focus_verses helps=helps pane=pane comments=comments /> }
    };
    view! {
        <main style=main.to_style()>
            <div style=bar.to_style()>
                <IconButton title=t(keys::draft::TOGGLE_RAIL, &[]) on_click=toggle>{rail_icon()}</IconButton>
                <h2 style=h2.to_style()>{book_id.english_name()}" "{chapter}</h2>
                <div style="flex: 1;"></div>
                {button}
                <HelpsToggleButton />
            </div>
            <div style=scroll.to_style()>
                <div style=grid.to_style()>
                    {tabs}
                    {head}
                    {rows}
                </div>
            </div>
        </main>
        {panel}
    }
    .into_any()
}

/// tC4's `SourceTabs`: a chip per source pane and the original's, the chosen text's name and
/// pinned version.
fn source_tabs(
    actions: &Actions,
    book: BookId,
    panes: Memo<Option<Arc<SourcesView>>>,
    chosen: Memo<Option<String>>,
) -> impl IntoView + use<> {
    let style = crate::css! {
        "position": "sticky",
        "top": 0,
        "background": "var(--surface-app)",
        "z-index": 2,
        "padding": "13px 26px 8px",
        "border-inline-end": HAIR,
    };
    let row = crate::css! {
        "display": "flex",
        "align-items": "center",
        "gap": 6,
        "margin-bottom": 6,
        "flex-wrap": "wrap",
    };
    let testament = testament_key(book);
    let chips = {
        let actions = actions.clone();
        move || {
            let tab = chosen.get();
            panes.with(|p| {
                p.as_ref().map(|v| {
                    let mut chips: Vec<AnyView> = v
                        .panes
                        .iter()
                        .map(|pane| {
                            let label = t_dyn(
                                &format!("source.{}", pane.id),
                                &[],
                                Some(&pane.id.to_uppercase()),
                            );
                            chip(
                                &actions,
                                &pane.id,
                                label,
                                tab.as_deref() == Some(pane.id.as_str()),
                            )
                        })
                        .collect();
                    if v.original.is_some() {
                        let label = t_dyn(&format!("source.orig.{testament}"), &[], None);
                        chips.push(chip(
                            &actions,
                            "orig",
                            label,
                            tab.as_deref() == Some("orig"),
                        ));
                    }
                    chips.collect_view()
                })
            })
        }
    };
    let caption = move || {
        let tab = chosen.get()?;
        let version = panes.with(|p| {
            p.as_ref()
                .and_then(|v| chosen_pane(v, &tab))
                .and_then(|pane| pane.version.clone())
        });
        let name = if tab == "orig" {
            t_dyn(&format!("source.orig.{testament}.name"), &[], None)
        } else {
            t_dyn(
                &format!("source.{tab}.name"),
                &[],
                Some(&tab.to_uppercase()),
            )
        };
        let pinned = version
            .map(|v| format!(" · {}", t(keys::draft::PINNED, &[("version", &v)])))
            .unwrap_or_default();
        let style = crate::css! {
            "font-size": "var(--fs-label)",
            "letter-spacing": "var(--track-11)",
            "color": "var(--text-tertiary)",
            "font-weight": "var(--fw-medium)",
        };
        Some(view! { <span data-testid="source-name" style=style.to_style()>{name}{pinned}</span> })
    };
    view! {
        <div style=style.to_style()>
            <div style=row.to_style()>{chips}</div>
            {caption}
        </div>
    }
}

fn chip(actions: &Actions, id: &str, label: String, selected: bool) -> AnyView {
    let actions = actions.clone();
    let click = id.to_owned();
    let test_id = format!("source-tab-{id}");
    view! {
        <FilterChip
            selected=selected
            style=chip_style(selected)
            attrs=Attrs::from([("data-testid", test_id.as_str())])
            on_click=Callback::new(move |_| actions.set_source_tab(click.clone()))
        >
            {label.clone()}
        </FilterChip>
    }
    .into_any()
}

/// The translation's column head: the project's name and language, and the Section/Verse
/// switch, which an open card turns off (tC4's `DraftUnitSwitch`).
fn target_head(
    s: &AppState,
    actions: &Actions,
    cross_frame: Memo<bool>,
    mode: Memo<UnitMode>,
    off: Signal<bool>,
) -> impl IntoView + use<> {
    let style = crate::css! {
        "position": "sticky",
        "top": 0,
        "background": "var(--surface-app)",
        "z-index": 2,
        "padding": "13px 26px 8px",
        "display": "flex",
        "align-items": "center",
    };
    let project = s.project.clone();
    let name = move || {
        project.with(|p| {
            p.as_ref()
                .map(|p| format!("{} · {}", p.name, p.language_tag))
                .unwrap_or_default()
        })
    };
    let actions = actions.clone();
    let switch = move || {
        if cross_frame.get() {
            return view! {
                <Overline attrs=Attrs::from([("data-testid", "draft-verse-only")])>
                    {t(keys::understand::CROSS_FRAME_VERSE_ONLY, &[])}
                </Overline>
            }
            .into_any();
        }
        let value = Signal::derive(move || {
            Some(
                match mode.get() {
                    UnitMode::Section => "section",
                    UnitMode::Verse => "verse",
                }
                .to_owned(),
            )
        });
        let options = vec![
            SwitcherOption {
                disabled: off,
                ..SwitcherOption::new("section", t(keys::understand::BY_SECTION, &[]))
            },
            SwitcherOption {
                disabled: off,
                ..SwitcherOption::new("verse", t(keys::understand::BY_VERSE, &[]))
            },
        ];
        let actions = actions.clone();
        let on_change = Callback::new(move |v: String| {
            actions.set_unit_mode(if v == "verse" {
                UnitMode::Verse
            } else {
                UnitMode::Section
            });
        });
        let switcher = view! {
            <Switcher indicator=Indicator::Pill size=Size::Sm tone=Tone::Ocean value=value on_change=on_change options=options />
        };
        // Chromium sends no mousedown to a disabled button: a press is stopped at pointerdown
        // so the focus stays in the card (tC4 wraps the switch only while it is off).
        if off.get() {
            leptos::html::span()
                .on(
                    leptos::ev::capture(leptos::ev::pointerdown),
                    |e: web_sys::PointerEvent| e.prevent_default(),
                )
                .child(switcher)
                .into_any()
        } else {
            switcher.into_any()
        }
    };
    view! {
        <div style=style.to_style()>
            <Overline tone=OverlineTone::Accent>{name}</Overline>
            <div style="flex: 1;"></div>
            {switch}
        </div>
    }
}

/// tC4's `SourceCell`: the section's place and its source verses.
fn source_cell(
    book: BookId,
    pane: &str,
    text: Option<&PaneText>,
    keys: &[String],
    label: &str,
    place: &SourcePlace,
    focus: Memo<Option<HelpFocus>>,
) -> AnyView {
    let mut style = cell();
    style.set("border-inline-end", HAIR);
    let italic = crate::css! {
        "font-size": "var(--fs-ui-sm)",
        "color": "var(--uw-haze)",
        "font-style": "italic",
        "margin": "6px 0 0",
    };
    // In another numbering the section is one project verse, labelled with the source verse it
    // shows, or stating why it shows none (#131, as Understand's units do).
    let (label, number) = match place {
        SourcePlace::Verse { label, number } => (label.clone(), Some(number.clone())),
        _ => (label.to_owned(), None),
    };
    let overline = view! { <Overline style=crate::css! { "margin-bottom": 6 }>{label}</Overline> };
    let stated = match place {
        SourcePlace::OtherBook { at, to } => Some(t(
            keys::understand::VERSE_CROSS_BOOK,
            &[
                ("ref", at as &dyn std::fmt::Display),
                ("to", to as &dyn std::fmt::Display),
            ],
        )),
        SourcePlace::Unmapped { at } => Some(t(
            keys::understand::VERSE_UNMAPPED,
            &[("ref", at as &dyn std::fmt::Display)],
        )),
        SourcePlace::Unknown => Some(t(keys::understand::FRAME_UNAVAILABLE, &[])),
        SourcePlace::Same | SourcePlace::Verse { .. } => None,
    };
    if let Some(stated) = stated {
        return view! {
            <div style=style.to_style()>
                {overline}
                <p data-testid="draft-source-stated" style=italic.to_style()>{stated}</p>
            </div>
        }
        .into_any();
    }
    let body = match text {
        Some(PaneText::NotInstalled) => {
            view! { <p style=italic.to_style()>{t(keys::source::NOT_INSTALLED, &[])}</p> }.into_any()
        }
        Some(PaneText::Missing) => {
            view! { <p style=italic.to_style()>{t(keys::source::UNAVAILABLE, &[])}</p> }.into_any()
        }
        Some(PaneText::Error { reason }) => view! {
            <p style=italic.to_style()>{t(keys::understand::SOURCE_ERROR, &[])}" "{reason.clone()}</p>
        }
        .into_any(),
        Some(PaneText::Ready(chapter)) => {
            source_paragraphs(book, pane, chapter, keys, number.as_deref(), focus)
        }
        None => ().into_any(),
    };
    view! { <div style=style.to_style()>{overline}{body}</div> }.into_any()
}

fn source_paragraphs(
    book: BookId,
    pane: &str,
    chapter: &PaneChapter,
    keys: &[String],
    number: Option<&str>,
    focus: Memo<Option<HelpFocus>>,
) -> AnyView {
    let number = number.map(str::to_owned);
    let orig = pane == "orig";
    let ot = book.testament() == Testament::Old;
    let (dir, lang, family) = match (orig, ot) {
        (true, true) => (Some("rtl"), Some("hbo"), "var(--font-hebrew)"),
        (true, false) => (Some("ltr"), Some("el"), "var(--font-greek)"),
        _ => (None, None, "var(--font-scripture)"),
    };
    let style = crate::css! {
        "direction": dir.unwrap_or("ltr"),
        "text-align": "start",
        "font-family": family,
        "font-size": "var(--fs-verse-lg)",
        "line-height": "var(--lh-verse-lg)",
        "color": "var(--text-scripture)",
        "margin": "0 0 10px",
    }
    .to_style();
    let waiting = crate::css! {
        "font-size": "var(--fs-ui-sm)",
        "color": "var(--uw-haze)",
        "font-style": "italic",
        "margin": 0,
        "margin-inline-end": ".3em",
    }
    .to_style();
    // tC4's `paragraphsOf` over the source keys, in order: a verse opens a paragraph when it
    // is the section's first, or a paragraph mark stands before its text; a key the pane lacks
    // runs on, saying the source is loading.
    let mut shown: Vec<Vec<String>> = Vec::new();
    for (i, key) in source_unit(keys, chapter).verses.into_iter().enumerate() {
        let opens = i == 0
            || chapter
                .verses
                .iter()
                .find(|v| v.key.as_str() == key)
                .is_some_and(|v| v.before.is_some() || v.lead.is_some());
        match shown.last_mut() {
            Some(para) if !opens => para.push(key),
            _ => shown.push(vec![key]),
        }
    }
    let pane = pane.to_owned();
    shown
        .into_iter()
        .map(|keys| {
            let body = keys
                .into_iter()
                .map(|k| {
                    let verse = chapter.verses.iter().find(|v| v.key.as_str() == k).cloned();
                    let text = match verse.filter(|v| !v.text.is_empty()) {
                        Some(verse) => {
                            let pane = pane.clone();
                            view! { {move || focus.with(|f| source_verse(&verse, &pane, f.as_ref()))}" " }.into_any()
                        }
                        None => view! { <span style=waiting.clone()>{t(keys::draft::SOURCES_LOAD, &[])}</span> }.into_any(),
                    };
                    let shown = number.clone().unwrap_or(k);
                    view! { <sup style=sup()>{shown}</sup>{text} }
                })
                .collect_view();
            view! { <p data-testid="draft-source-text" dir=dir lang=lang style=style.clone()>{body}</p> }
        })
        .collect_view()
        .into_any()
}

/// tC4's `TargetCell`: the section's "Draft section" pill, its verses as paragraphs, and the
/// verse card under them while one of its verses is open. The paragraphs follow the chapter's
/// read; the card follows only which verse is open, so a refresh never closes it.
#[allow(clippy::too_many_arguments)]
fn target_cell(
    s: &AppState,
    actions: &Actions,
    book: BookId,
    chapter: u16,
    text: Memo<Option<Arc<ChaptersView>>>,
    keys: &[String],
    span: &str,
    mode: Memo<UnitMode>,
    off: Signal<bool>,
    open: &Open,
) -> AnyView {
    let mut style = cell();
    style.set("position", "relative");
    let head = crate::css! {
        "display": "flex",
        "align-items": "center",
        "justify-content": "flex-end",
        "margin-bottom": 6,
        "min-height": 14,
    };
    let editing = s.editing.clone();
    let open_verse = {
        let editing = editing.clone();
        let keys = keys.to_vec();
        Memo::new(move |_| {
            editing
                .with(|e| editing_verse(e, book, chapter))
                .filter(|v| keys.contains(v))
        })
    };
    // The row that holds a section card's first verse hosts the card, whatever the rows regroup
    // to meanwhile: the card keeps the verse set it was opened with (tC4's `startSection`).
    let open_section = {
        let (editing, keys) = (editing.clone(), keys.to_vec());
        Memo::new(move |_| {
            editing.with(|e| {
                editing_section(e, book, chapter)
                    .filter(|open| open.first().is_some_and(|first| keys.contains(first)))
                    .map(<[String]>::to_vec)
            })
        })
    };
    let pill = {
        let (span, keys, actions) = (span.to_owned(), keys.to_vec(), actions.clone());
        move || {
            let (span, keys, actions) = (span.clone(), keys.clone(), actions.clone());
            (mode.get() == UnitMode::Section).then(move || {
                // #728: a quiet pill at rest, so it reads as a button. It opens the section
                // card; while a card is open it is off, as the switch is.
                let style = Signal::derive(move || {
                    if off.get() {
                        crate::css::Css::new()
                    } else {
                        crate::css! { "color": "var(--accent)" }
                    }
                });
                // A press on an off pill keeps the focus in the open card.
                let hold = Callback::new(move |e: web_sys::PointerEvent| {
                    if off.get_untracked() {
                        e.prevent_default();
                    }
                });
                view! {
                    <Button
                        variant=ButtonVariant::Secondary
                        size=Size::Sm
                        disabled=off
                        style=style
                        on_pointer_down_capture=hold
                        on_click=Callback::new(move |_| actions.start_section(chapter, &keys))
                    >
                        {t(keys::draft::DRAFT_SECTION, &[("span", &span)])}
                    </Button>
                }
            })
        }
    };
    let dir = dir_of(s);
    let rows = {
        let keys = keys.to_vec();
        Memo::new(move |_| {
            text.with(|v| {
                let verses = v
                    .as_ref()
                    .and_then(|v| v.chapters.iter().find(|c| c.chapter == chapter))
                    .map(|c| c.verses.as_slice())
                    .unwrap_or_default();
                keys.iter()
                    .filter_map(|k| verses.iter().find(|v| v.key.as_str() == k).cloned())
                    .collect::<Vec<VerseRow>>()
            })
        })
    };
    let paras = {
        let actions = actions.clone();
        move || {
            rows.with(|rows| {
                let refs: Vec<&VerseRow> = rows.iter().collect();
                target_paragraphs(&refs)
                    .into_iter()
                    .map(|(verses, level)| {
                        let mut p = crate::css! { "direction": dir, "text-align": "start" };
                        p.extend(&target_type());
                        p.extend(
                            &crate::css! { "color": "var(--text-scripture)", "margin": "0 0 10px" },
                        );
                        match level {
                            1 => p.set("padding-inline-start", "1.5em"),
                            2 => p.set("padding-inline-start", "3em"),
                            _ => {}
                        }
                        let body = verses
                            .into_iter()
                            .map(|v| {
                                target_verse(&actions, book, chapter, v, mode, open_verse, off)
                            })
                            .collect_view();
                        view! { <p data-testid="draft-target-text" style=p.to_style()>{body}</p> }
                    })
                    .collect_view()
            })
        }
    };
    let card = {
        let (actions, open, editing) = (actions.clone(), open.clone(), editing.clone());
        move || {
            let verse = open_verse.get()?;
            let key = editing.with_untracked(|e| match e {
                Editing::Verse(key) => Some(key.clone()),
                _ => None,
            })?;
            Some(view! {
                <VerseEditor key=key verse=verse text=text at=open.at.clone() chapter=chapter dir=dir actions=actions.clone() />
            })
        }
    };
    let section_card = {
        let (actions, open) = (actions.clone(), open.clone());
        move |keys: Vec<String>| {
            let key = editing.with_untracked(|e| match e {
                Editing::Section { key, .. } => Some(key.clone()),
                _ => None,
            })?;
            Some(view! {
                <SectionEditor key=key keys=keys text=text at=open.at.clone() chapter=chapter dir=dir actions=actions.clone() />
            })
        }
    };
    // A section card replaces the whole cell, the pill and the paragraphs with it (tC4).
    let content = move || match open_section.get() {
        Some(keys) => section_card(keys).into_any(),
        None => view! {
            <div style=head.to_style()>{pill.clone()}</div>
            {paras.clone()}
            {card.clone()}
        }
        .into_any(),
    };
    view! { <div style=style.to_style()>{content}</div> }.into_any()
}

/// tC4's `TargetVerse` with its number: the drafted words (a click revises the verse in Verse
/// mode), "Draft verse N", or "Editing below" while its card is open; a verse in conflict
/// carries the marker.
fn target_verse(
    actions: &Actions,
    book: BookId,
    chapter: u16,
    v: &VerseRow,
    mode: Memo<UnitMode>,
    open_verse: Memo<Option<String>>,
    off: Signal<bool>,
) -> AnyView {
    let key = v.key.as_str().to_owned();
    let number = view! { <sup style=sup()>{key.clone()}</sup> }.into_any();
    let verse = {
        let (actions, v, key) = (actions.clone(), v.clone(), key.clone());
        move || {
            if open_verse.get().as_deref() == Some(key.as_str()) {
                let style = crate::css! {
                    "display": "inline-block",
                    "border-radius": "var(--radius-sm)",
                    "padding": "2px 10px",
                    "margin-inline-end": ".3em",
                    "background": "var(--surface-accent-soft)",
                    "font-family": "var(--font-ui)",
                    "font-size": "var(--fs-caption-lg)",
                    "letter-spacing": "var(--track-12-5)",
                    "font-weight": "var(--fw-bold)",
                    "color": "var(--text-accent)",
                    "vertical-align": "middle",
                };
                return Some(view! { <span style=style.to_style()>{t(keys::draft::EDITING_BELOW, &[])}</span> }.into_any());
            }
            let verse_mode = mode.get() == UnitMode::Verse;
            if v.drafted {
                let word = crate::css! {
                    "display": "inline-block",
                    "border-radius": "var(--radius-xs)",
                    "padding": "0 .06em",
                }
                .to_style();
                let words = shown_words(&v.text)
                    .into_iter()
                    .map(|w| view! { <span data-i="quiet" style=word.clone()>{w.to_owned()}</span>" " })
                    .collect_view();
                let span = if verse_mode {
                    let (actions, key) = (actions.clone(), key.clone());
                    leptos::html::span()
                        .attr("title", t(keys::draft::EDIT_VERSE, &[]))
                        .attr("style", "cursor: text;")
                        .on(leptos::ev::click, move |_| {
                            actions.start_verse(chapter, &key)
                        })
                        .child(words)
                        .into_any()
                } else {
                    view! { <span>{words}</span> }.into_any()
                };
                return Some(span);
            }
            if !verse_mode {
                return None;
            }
            let (actions, key) = (actions.clone(), key.clone());
            Some(draft_pill(
                t(keys::draft::DRAFT_VERSE, &[("n", &key)]),
                t(keys::draft::START_VERSE, &[]),
                crate::css! { "margin-inline-end": ".3em" },
                Callback::new(move |()| actions.start_verse(chapter, &key)),
            ))
        }
    };
    match &v.words.marker {
        Some(marker) => {
            let place = format!("{chapter}:{key}");
            let reference = format!("{} {place}", book.english_name());
            view! {
                {conflict_verse(place, reference, marker.versions, number, verse.into_any(), off, Callback::new(|()| {}))}
                " "
            }
            .into_any()
        }
        None => view! { {number}{verse} }.into_any(),
    }
}

/// The author of the verse's record in each `Changed` frame of its open, for the notice.
fn listen_for_authors(
    at: Scope,
    record: tc_api::ids::RecordRef,
) -> StoredValue<Option<PersonLabel>> {
    let by: StoredValue<Option<PersonLabel>> = StoredValue::new(None);
    if let Some(runtime) = use_runtime() {
        let id = runtime.listen(move |frame| {
            let Event::Session {
                at: frame_at,
                event: SessionEvent::Changed { scope, .. },
            } = &frame.event
            else {
                return;
            };
            if *frame_at != at {
                return;
            }
            if let ChangeScope::Parts(parts) = scope
                && let Some(change) = parts.records.iter().find(|r| r.record == record)
            {
                by.try_set_value(change.by.clone());
            }
        });
        let runtime = SendWrapper::new(runtime);
        on_cleanup(move || runtime.unlisten(id));
    }
    by
}

/// The open verse's row and the session view of the read it came from.
fn row_of(
    text: &Option<Arc<ChaptersView>>,
    chapter: u16,
    verse: &str,
) -> Option<(VerseRow, ViewSeq)> {
    let view = text.as_ref()?;
    let row = view
        .chapters
        .iter()
        .find(|c| c.chapter == chapter)?
        .verses
        .iter()
        .find(|v| v.key.as_str() == verse)?
        .clone();
    Some((row, view.view))
}

/// tC4's `VerseEditor`: the editing card of one verse. Its text is the verse's whole words
/// (#786); it saves on blur or 2 s after the last keystroke, Save closes it as a blur does,
/// and Cancel puts back the text it opened with.
#[component]
fn VerseEditor(
    key: BufferKey,
    verse: String,
    text: Memo<Option<Arc<ChaptersView>>>,
    at: Scope,
    chapter: u16,
    dir: &'static str,
    actions: Actions,
) -> impl IntoView {
    let (Some(runtime), Some((row, view))) = (
        use_runtime(),
        text.with_untracked(|t| row_of(t, chapter, &verse)),
    ) else {
        return ().into_any();
    };
    let saves = runtime.saves().clone();
    let pair = {
        let at = at.clone();
        move |row: &VerseRow, view: ViewSeq| Pair {
            value: box_text(&row.words.value).to_owned(),
            token: row.words.token.clone(),
            view: Some(View::of(Some(&at), view)),
        }
    };
    // The card opens on the pair of the read its row came from (AD37).
    let before = box_text(&row.words.value).to_owned();
    let opened = (row.words.value.clone(), row.words.token.clone());
    let shown = saves.open(
        &key,
        FieldScope::Project(Some(at.clone())),
        pair(&row, view),
    );
    let by = listen_for_authors(at.clone(), row.record.clone());
    // A refresh of the chapter's read: the machine swaps the pair, keeps it back, or shows the
    // notice (6.3).
    {
        let saves = SendWrapper::new(saves.clone());
        let key = key.clone();
        let current = Memo::new(move |_| text.with(|t| row_of(t, chapter, &verse)));
        Effect::new(move |_| {
            let Some((row, view)) = current.get() else {
                return;
            };
            if (row.words.value.clone(), row.words.token.clone()) == opened {
                return;
            }
            saves.refresh(&key, pair(&row, view), by.get_value());
        });
    }
    let changes = saves.changes();
    let notice = {
        let saves = SendWrapper::new(saves.clone());
        let key = key.clone();
        Memo::new(move |_| {
            changes.get();
            saves
                .notice(&key)
                .map(|n| changed_while_typing(n.by.as_ref()))
        })
    };
    let area = NodeRef::<html::Textarea>::new();
    {
        crate::views::each_frame(1, move || {
            if let Some(el) = area.get_untracked() {
                // As tC4's field, whose text is its default value: the caret at the start.
                let _ = el.set_selection_range(0, 0);
                el.set_scroll_top(0);
                let _ = el.focus();
            }
            false
        });
    }
    {
        let saves = SendWrapper::new(saves.clone());
        let key = SendWrapper::new(key.clone());
        on_cleanup(move || saves.close(&key));
    }
    let on_input = {
        let saves = SendWrapper::new(saves.clone());
        let key = SendWrapper::new(key.clone());
        move |event: leptos::ev::Event| {
            saves.typed(&key, &event_target_value(&event), Source::Input)
        }
    };
    let on_focus = {
        let saves = SendWrapper::new(saves.clone());
        let key = SendWrapper::new(key.clone());
        move |event: leptos::ev::FocusEvent| {
            if let Some(element) = event
                .target()
                .and_then(|target| target.dyn_into::<web_sys::HtmlElement>().ok())
            {
                saves.focus(&key, &element);
            }
        }
    };
    let on_composition = {
        let saves = SendWrapper::new(saves.clone());
        let key = SendWrapper::new(key.clone());
        move |_: leptos::ev::CompositionEvent| saves.composition(&key)
    };
    let close = {
        let actions = actions.clone();
        move || actions.close_verse()
    };
    let on_blur = {
        let close = close.clone();
        move |_: leptos::ev::FocusEvent| close()
    };
    // tC4's field holds its text as its default value: the text the card opened with.
    let initial = before.clone();
    let cancel = {
        let saves = SendWrapper::new(saves.clone());
        let key = SendWrapper::new(key.clone());
        let close = close.clone();
        move |_: web_sys::MouseEvent| {
            saves.typed(&key, &before, Source::Program);
            close();
        }
    };
    let n = row.key.as_str().to_owned();
    let mut field = crate::css! {
        "width": "100%",
        "box-sizing": "border-box",
        "border": 0,
        "outline": "none",
        "resize": "vertical",
    };
    field.extend(&target_type());
    field.extend(&crate::css! { "color": "var(--text-scripture)", "background": "transparent" });
    let label = t(keys::draft::VERSE_LABEL, &[("n", &n)]);
    let header = view! {
        <Overline tone=OverlineTone::Accent>{t(keys::draft::DRAFTING, &[])}" "{n}</Overline>
    }
    .into_any();
    let value = move || shown.get();
    let notice_view = move || {
        notice.get().map(|text| {
            view! {
                <Callout tone=CalloutTone::Warn style=crate::css! { "margin-top": 8 } attrs=Attrs::from([("role", "status"), ("data-testid", "verse-changed-notice")])>
                    {text}
                </Callout>
            }
        })
    };
    let textarea = view! {
        <textarea
            node_ref=area
            aria-label=label
            dir=dir
            prop:value=value
            placeholder=t(keys::draft::PLACEHOLDER, &[])
            rows="3"
            style=field.to_style()
            on:input=on_input
            on:focus=on_focus
            on:blur=on_blur
            on:compositionstart=on_composition
        >
            {initial}
        </textarea>
        {notice_view}
    }
    .into_any();
    let cancel_style = crate::css! {
        "color": "var(--text-tertiary)",
        "font-size": "var(--fs-caption)",
        "letter-spacing": "var(--track-12)",
    };
    let footer = view! {
        <Button size=Size::Sm on_click=Callback::new(move |_| close())>{t(keys::draft::SAVE_VERSE, &[])}</Button>
        <Button variant=ButtonVariant::Ghost style=cancel_style on_click=Callback::new(cancel)>{t(keys::draft::CANCEL_VERSE, &[])}</Button>
    }
    .into_any();
    editing_card(header, textarea, footer, Attrs::default())
}
