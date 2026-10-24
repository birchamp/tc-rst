//! The port of tC4's `CommunityChecking.jsx`, `ExportMenu.jsx` and `print/PrintPages.jsx`
//! (unit B8): the typeset preview of the open book on page sheets, or of the open story
//! project's drafted stories, with the page setup and the export menu.
//!
//! The book's print is `export/book` (tc-export's printed items, read on the server); the page
//! setup is this screen's own state, at tC4's default each time the screen opens. As tC4 does,
//! the pages are measured in the page: a hidden sheet takes verses until the next would
//! overflow, a verse that does not fit is split between the browser's word breaks
//! (`Intl.Segmenter`), and each page is measured again when a web font arrives
//! (`tc_api::model::print::paginate`). The export menu lists what the project can be exported
//! as; each item but the Burrito zip, which E2 turns on, runs its export (D2).

use std::sync::Arc;

use js_sys::{Array, Object, Reflect};
use leptos::prelude::*;
use send_wrapper::SendWrapper;
use tc_api::export::{
    Book as PrintBookRead, BookPrint, PrintFrame, PrintStory, PrintStoryItem, Stories,
    StoriesPrint, Subject,
};
use tc_api::home::ProjectKind;
use tc_api::ids::Scope;
use tc_api::model::print::{
    Atom, ChapterOpening, Columns, ObsLayout, PageSetup, Paper, Pictures, Spacing, VerseNumbers,
    atoms_of, continues, paginate, parts_of, print_parts,
};
use tc_api::shown::Direction;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;

use crate::app::load::Load;
use crate::app::print::{
    ExportOf, ExportPanel, Sheet, download_done_panel, export_items, flow_markup, flow_style,
    frame_gap_text, page_number_style, producer_of, sheet_style, story_gap_text,
};
use crate::app::shell::{ProjectFlavor, View};
use crate::app::web::{Actions, AppCtx, AppState, use_app, use_runtime};
use crate::attrs::Attrs;
use crate::css::Css;
use crate::ds::{
    Button, ButtonVariant, Callout, CalloutTone, FilterChip, Menu, MenuItem, Overline, Toast,
    ToastProps, Toggle,
};
use crate::i18n::{Key, keys, t};
use crate::tokens::Size;

/// tC4's `CommunityChecking`.
#[component]
pub fn CommunityChecking() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let setup = RwSignal::new(PageSetup::default());
    let project = s.project.with_untracked(|p| {
        p.as_ref()
            .map(|p| (p.at.clone(), p.flavor, p.dir, p.name.clone()))
    });
    let Some((at, flavor, dir, name)) = project else {
        return ().into_any();
    };
    match flavor {
        ProjectFlavor::TextStories => story_checking(&actions, at, dir, setup, name).into_any(),
        ProjectFlavor::TextTranslation => book_checking(&s, &actions, at, dir, setup).into_any(),
    }
}

fn note_style() -> Css {
    crate::css! {
        "flex": 1,
        "display": "flex",
        "align-items": "center",
        "justify-content": "center",
        "color": "var(--text-tertiary)",
        "font-size": "var(--fs-ui)",
    }
}

/// The screen while its print is read, or when it could not be read.
fn note(text: String) -> AnyView {
    view! { <div style=note_style().to_style() data-testid="community-checking">{text}</div> }
        .into_any()
}

fn root_style() -> Css {
    crate::css! { "flex": 1, "display": "flex", "min-height": 0 }
}

/// The preview's scroll (a block, not a flex column: a flex column would shrink a short page
/// to its content).
fn main_style() -> Css {
    crate::css! {
        "flex": 1,
        "overflow": "auto",
        "min-width": 0,
        "background": "var(--surface-muted)",
        "padding": "34px 24px 60px",
    }
}

fn aside_style() -> Css {
    crate::css! {
        "width": "var(--rail-width-wide)",
        "flex": "none",
        "background": "var(--surface-card)",
        "border-inline-start": "var(--stroke-hair) solid var(--border)",
        "padding": 22,
        "display": "flex",
        "flex-direction": "column",
        "gap": 16,
        "overflow": "auto",
    }
}

fn dir_attr(dir: Direction) -> &'static str {
    crate::app::print::dir_attr(dir)
}

// ---------------------------------------------------------------------------------------------
// A Bible book
// ---------------------------------------------------------------------------------------------

fn book_checking(
    s: &AppState,
    actions: &Actions,
    at: Scope,
    dir: Direction,
    setup: RwSignal<PageSetup>,
) -> impl IntoView + use<> {
    let book = s.u.book.clone();
    let actions = actions.clone();
    move || {
        let Some(book) = book.get() else {
            return note(t(keys::draft::LOADING, &[]));
        };
        let Some(runtime) = use_runtime() else {
            return ().into_any();
        };
        let read = runtime.cache().read(PrintBookRead {
            at: at.clone(),
            book,
        });
        let actions = actions.clone();
        let title = book.english_name().to_owned();
        let of = ExportOf {
            at: at.clone(),
            subject: Subject::Book { book },
            title: title.clone(),
        };
        (move || {
            read.with(|loaded| match loaded {
                Load::Loading => note(t(keys::draft::LOADING, &[])),
                Load::Failed(error) => note(format!("{} {error}", t(keys::draft::LOAD_ERROR, &[]))),
                Load::Ready(print) | Load::Refreshing(print) => book_screen(
                    &actions,
                    print.clone(),
                    title.clone(),
                    dir,
                    setup,
                    of.clone(),
                ),
            })
        })
        .into_any()
    }
}

fn book_screen(
    actions: &Actions,
    print: Arc<BookPrint>,
    title: String,
    dir: Direction,
    setup: RwSignal<PageSetup>,
    of: ExportOf,
) -> AnyView {
    let undrafted = print.undrafted;
    view! {
        <div style=root_style().to_style() data-testid="community-checking">
            <main style=main_style().to_style()>{print_pages(print, title, dir, setup)}</main>
            <aside style=aside_style().to_style()>
                {side_head(actions, ProjectKind::Bible, of, setup)}
                {setup_box(book_rows(setup))}
                {undrafted.then(|| incomplete(keys::cc::INCOMPLETE_BODY))}
            </aside>
        </div>
    }
    .into_any()
}

/// The browser's word breaks of `text` (`wordsOf`): where each segment ends, in bytes.
fn word_ends(segmenter: &js_sys::Intl::Segmenter, text: &str) -> Vec<usize> {
    let mut ends = Vec::new();
    let mut at = 0;
    if let Ok(Some(segments)) = js_sys::try_iter(&segmenter.segment(text)) {
        for segment in segments.flatten() {
            let piece: js_sys::Intl::SegmentData = segment.unchecked_into();
            at += String::from(piece.segment()).len();
            ends.push(at);
        }
    }
    if ends.last() != Some(&text.len()) {
        // A text the segmenter could not read is one word: it is never cut.
        return vec![text.len()];
    }
    ends
}

fn segmenter() -> js_sys::Intl::Segmenter {
    let options = Object::new();
    let _ = Reflect::set(&options, &"granularity".into(), &"word".into());
    js_sys::Intl::Segmenter::new(&Array::new(), &options)
}

/// A web font that arrives after the measurement changes every line: the epoch moves on
/// `document.fonts.ready` and on each `loadingdone`, and the pages are measured again.
fn fonts_epoch() -> RwSignal<u32> {
    let epoch = RwSignal::new(0u32);
    let Some(fonts) = web_sys::window()
        .and_then(|w| w.document())
        .map(|d| d.fonts())
    else {
        return epoch;
    };
    let again = Closure::<dyn Fn()>::new(move || epoch.try_update(|n| *n += 1).unwrap_or(()));
    if let Ok(ready) = fonts.ready() {
        let once = Closure::<dyn FnMut(wasm_bindgen::JsValue)>::new(move |_| {
            epoch.try_update(|n| *n += 1);
        });
        let _ = ready.then(&once);
        once.forget();
    }
    let _ = fonts.add_event_listener_with_callback("loadingdone", again.as_ref().unchecked_ref());
    let kept = SendWrapper::new((fonts, again));
    on_cleanup(move || {
        let (fonts, again) = kept.take();
        let _ = fonts
            .remove_event_listener_with_callback("loadingdone", again.as_ref().unchecked_ref());
    });
    epoch
}

/// tC4's `PrintPages`: the measuring sheet and the page sheets.
fn print_pages(
    print: Arc<BookPrint>,
    title: String,
    dir: Direction,
    setup: RwSignal<PageSetup>,
) -> AnyView {
    let measure = NodeRef::<leptos::html::Div>::new();
    // Each page's flow, as the markup the sheet shows.
    let pages: RwSignal<Option<Vec<String>>> = RwSignal::new(None);
    let epoch = fonts_epoch();
    let nothing = print.items.is_empty();
    {
        let print = SendWrapper::new(print.clone());
        Effect::new(move |_| {
            let setup = setup.get();
            epoch.track();
            let Some(sheet) = measure.get() else {
                return;
            };
            let (Ok(Some(heading)), Ok(Some(flow))) = (
                sheet.query_selector(".print-title"),
                sheet.query_selector(".print-flow"),
            ) else {
                return;
            };
            let (Ok(heading), Ok(flow)) = (
                heading.dyn_into::<web_sys::HtmlElement>(),
                flow.dyn_into::<web_sys::HtmlElement>(),
            ) else {
                return;
            };
            let atoms = atoms_of(&print.items);
            let segmenter = segmenter();
            let mut fits = |page: usize, list: &[Atom<'_>], carries: bool| {
                let _ = if page == 0 {
                    heading.style().remove_property("display").map(|_| ())
                } else {
                    heading.style().set_property("display", "none")
                };
                flow.set_inner_html(&flow_markup(
                    &print_parts(&parts_of(list), &setup, carries),
                    dir,
                    Sheet::Measure,
                ));
                flow.scroll_width() <= flow.client_width() + 1
                    && flow.scroll_height() <= flow.client_height() + 1
            };
            let laid = paginate(&atoms, &mut |text| word_ends(&segmenter, text), &mut fits);
            flow.set_inner_html("");
            let markup = laid
                .iter()
                .enumerate()
                .map(|(i, page)| {
                    let next = laid.get(i + 1).map(Vec::as_slice);
                    flow_markup(
                        &print_parts(&parts_of(page), &setup, continues(next)),
                        dir,
                        Sheet::Page,
                    )
                })
                .collect();
            pages.set(Some(markup));
        });
    }
    let d = dir_attr(dir);
    let hidden = {
        let title = title.clone();
        view! {
            <div
                node_ref=measure
                class="print-book"
                style=move || sheet_style(&setup.get(), true).to_style()
                aria-hidden="true"
            >
                <h1 class="print-title" dir=d>{title}</h1>
                <div class="print-flow" dir=d style=flow_style(false).to_style()></div>
            </div>
        }
    };
    let sheets = move || {
        if nothing {
            let empty_style = crate::css! {
                "text-align": "center",
                "color": "var(--text-tertiary)",
                "font-size": "var(--fs-ui)",
            };
            let empty = view! {
                <p data-testid="cc-nothing-drafted" style=empty_style.to_style()>
                    {t(keys::cc::NOTHING_DRAFTED, &[])}
                </p>
            }
            .into_any();
            return sheet(
                setup,
                d,
                Some(title.clone()),
                1,
                false,
                SheetFlow::Empty(empty),
            )
            .into_any();
        }
        let shown = pages.get().unwrap_or_default();
        let last = shown.len().saturating_sub(1);
        shown
            .into_iter()
            .enumerate()
            .map(|(i, markup)| {
                sheet(
                    setup,
                    d,
                    (i == 0).then(|| title.clone()),
                    i + 1,
                    i == last,
                    SheetFlow::Markup(markup),
                )
            })
            .collect_view()
            .into_any()
    };
    view! { <div style="position: relative;">{hidden}{sheets}</div> }.into_any()
}

enum SheetFlow {
    Markup(String),
    Empty(AnyView),
}

/// tC4's `Sheet`: a page, with the title on the first and its number at the foot.
fn sheet(
    setup: RwSignal<PageSetup>,
    d: &'static str,
    title: Option<String>,
    number: usize,
    last: bool,
    flow: SheetFlow,
) -> impl IntoView + use<> {
    let flow = match flow {
        SheetFlow::Markup(markup) => view! {
            <div class="print-flow" dir=d style=flow_style(last).to_style() data-testid="cc-flow" inner_html=markup></div>
        }
        .into_any(),
        SheetFlow::Empty(empty) => view! {
            <div class="print-flow" dir=d style=flow_style(last).to_style() data-testid="cc-flow">{empty}</div>
        }
        .into_any(),
    };
    view! {
        <div
            class="print-book"
            style=move || sheet_style(&setup.get(), false).to_style()
            data-testid="cc-page"
        >
            {title.map(|title| view! { <h1 class="print-title" dir=d>{title}</h1> })}
            {flow}
            <div style=page_number_style().to_style()>{number}</div>
        </div>
    }
}

/// The page setup rows of a Bible book.
fn book_rows(setup: RwSignal<PageSetup>) -> AnyView {
    let columns = choice_row(
        keys::cc::COLUMNS,
        "cc-columns-label",
        vec![
            (ChoiceLabel::Plain("1"), Columns::One),
            (ChoiceLabel::Plain("2"), Columns::Two),
        ],
        Signal::derive(move || setup.get().columns),
        Callback::new(move |columns| setup.update(|s| s.columns = columns)),
    );
    let spacing = choice_row(
        keys::cc::SPACING,
        "cc-spacing-label",
        vec![
            (ChoiceLabel::Key(keys::cc::SPACING_SINGLE), Spacing::Single),
            (ChoiceLabel::Key(keys::cc::SPACING_DOUBLE), Spacing::Double),
        ],
        Signal::derive(move || setup.get().spacing),
        Callback::new(move |spacing| setup.update(|s| s.spacing = spacing)),
    );
    let paper = choice_row(
        keys::cc::PAPER,
        "cc-paper-label",
        vec![
            (ChoiceLabel::Key(keys::cc::PAPER_A4), Paper::A4),
            (ChoiceLabel::Key(keys::cc::PAPER_LETTER), Paper::Letter),
        ],
        Signal::derive(move || setup.get().paper),
        Callback::new(move |paper| setup.update(|s| s.paper = paper)),
    );
    let drop_cap = Signal::derive(move || setup.get().chapter_opening == ChapterOpening::DropCap);
    let numbers = Signal::derive(move || setup.get().verse_numbers == VerseNumbers::Shown);
    view! {
        {columns}
        {spacing}
        {paper}
        <Toggle
            label=t(keys::cc::DROP_CAP, &[])
            checked=drop_cap
            on_change=Callback::new(move |_| setup.update(|s| {
                s.chapter_opening = match s.chapter_opening {
                    ChapterOpening::DropCap => ChapterOpening::Heading,
                    ChapterOpening::Heading => ChapterOpening::DropCap,
                };
            }))
        />
        <Toggle
            label=t(keys::cc::VERSE_NUMBERS, &[])
            checked=numbers
            on_change=Callback::new(move |_| setup.update(|s| {
                s.verse_numbers = match s.verse_numbers {
                    VerseNumbers::Shown => VerseNumbers::Hidden,
                    VerseNumbers::Hidden => VerseNumbers::Shown,
                };
            }))
        />
        <Toggle label=t(keys::cc::FOOTNOTES, &[]) disabled=true />
    }
    .into_any()
}

// ---------------------------------------------------------------------------------------------
// The side panel
// ---------------------------------------------------------------------------------------------

/// Back to the checking tools, the title and the export menu.
fn side_head(
    actions: &Actions,
    kind: ProjectKind,
    of: ExportOf,
    setup: RwSignal<PageSetup>,
) -> impl IntoView + use<> {
    let back = {
        let actions = actions.clone();
        Callback::new(move |_| actions.go(View::Check))
    };
    let h2 = crate::css! {
        "font-size": "var(--fs-title-sm)",
        "letter-spacing": "var(--track-16)",
        "margin": 0,
    };
    view! {
        <Button variant=ButtonVariant::Ghost on_click=back style=crate::css! { "align-self": "flex-start" }>
            {t(keys::cc::BACK, &[])}
        </Button>
        <h2 style=h2.to_style()>{t(keys::cc::TITLE, &[])}</h2>
        {export_menu(actions, kind, of, setup)}
    }
}

/// tC4's `ExportMenu`: one item for each producer that applies to the project, and under it
/// the conflicts question, the "Export ready" toast with the verses the file left out, or why
/// the export failed. The Burrito zip stays off until E2 builds the exchange file.
fn export_menu(
    actions: &Actions,
    kind: ProjectKind,
    of: ExportOf,
    setup: RwSignal<PageSetup>,
) -> impl IntoView + use<> {
    let panel = RwSignal::new(ExportPanel::Idle);
    let running = Signal::derive(move || panel.get() == ExportPanel::Running);
    let items = export_items(kind, crate::app::web::bridge::has_print_pdf())
        .into_iter()
        .map(|key| {
            let producer = producer_of(key);
            let on_click = producer.map(|producer| {
                let (actions, of) = (actions.clone(), of.clone());
                Callback::new(move |()| {
                    actions.export_press(panel, &of, producer, setup.get_untracked());
                })
            });
            MenuItem {
                label: t(key, &[]),
                disabled: on_click.is_none(),
                on_click,
                ..MenuItem::default()
            }
        })
        .collect::<Vec<_>>();
    let trigger: crate::ds::Slot = Box::new(move || {
        view! {
            <Button
                size=Size::Lg
                disabled=running
                attrs=Attrs::from([("data-testid", "export-menu-trigger")])
            >
                {t(keys::cc::EXPORT, &[])}
            </Button>
        }
        .into_any()
    });
    let actions = actions.clone();
    let below = move || match panel.get() {
        ExportPanel::Idle | ExportPanel::Running => ().into_any(),
        ExportPanel::Ask { question, run } => {
            let review = {
                let actions = actions.clone();
                Callback::new(move |_| {
                    panel.set(ExportPanel::Idle);
                    actions.open_review(tc_api::conflicts::ReviewTab::Text, None);
                })
            };
            let anyway = {
                let actions = actions.clone();
                Callback::new(move |_| actions.export_run(panel, (*run).clone()))
            };
            view! {
                <Callout
                    tone=CalloutTone::Warn
                    style=crate::css! { "margin-top": 10 }
                    attrs=Attrs::from([("data-testid", "export-conflicts")])
                >
                    <p style="margin: 0 0 8px">{question}</p>
                    <Button size=Size::Sm on_click=review>{t(keys::export::REVIEW, &[])}</Button>
                    " "
                    <Button size=Size::Sm variant=ButtonVariant::Ghost on_click=anyway>
                        {t(keys::export::ANYWAY, &[])}
                    </Button>
                </Callout>
            }
            .into_any()
        }
        ExportPanel::Ready(notice) => {
            let download = notice.download.map(|(id, name)| {
                Callback::new(move |_| crate::app::web::export::deliver(&id, &name))
            });
            let action_label = download.is_some().then(|| t(keys::export::DOWNLOAD, &[]));
            // The side panel is narrower than the toast's 300px minimum width (tC4).
            let toast = Toast(ToastProps {
                tone: notice.tone,
                message: notice.ready,
                action: download,
                action_label,
                on_dismiss: Some(Callback::new(move |_| panel.set(ExportPanel::Idle))),
                dismiss_label: Some(t(keys::common::DISMISS, &[])),
                style: crate::css! { "margin-top": 10, "--toast-w-min": "0px" },
                attrs: Attrs::from([("data-testid", "export-toast")]),
            });
            view! {
                {toast}
                {notice.skipped.map(skipped_callout)}
            }
            .into_any()
        }
        ExportPanel::Saving(skipped) => skipped.map(skipped_callout).into_any(),
        ExportPanel::Failed(why) => view! {
            <Callout
                tone=CalloutTone::Warn
                style=crate::css! { "margin-top": 10 }
                attrs=Attrs::from([("data-testid", "export-failure")])
            >
                {why}
            </Callout>
        }
        .into_any(),
    };
    crate::app::web::bridge::on_download_done(move |done| {
        let shown = panel.get_untracked();
        panel.try_set(download_done_panel(done, &shown));
    });
    view! {
        <div data-testid="export-menu">
            <Menu trigger=trigger items=items />
            {below}
        </div>
    }
}

/// The verses an export left its alignment out of (tC4 e2bfd8c3).
fn skipped_callout(skipped: String) -> impl IntoView {
    view! {
        <Callout
            tone=CalloutTone::Warn
            style=crate::css! { "margin-top": 10 }
            attrs=Attrs::from([("data-testid", "export-skipped")])
        >
            {skipped}
        </Callout>
    }
}

/// The page setup box.
fn setup_box(rows: AnyView) -> impl IntoView {
    let boxed = crate::css! {
        "border": "var(--stroke) solid var(--border)",
        "border-radius": "var(--radius-lg)",
        "padding": 16,
        "background": "var(--surface-app)",
    };
    let list = crate::css! {
        "display": "flex",
        "flex-direction": "column",
        "gap": 10,
        "margin-top": 12,
        "font-size": "var(--fs-ui-sm)",
        "letter-spacing": "var(--track-13)",
    };
    view! {
        <div style=boxed.to_style()>
            <Overline style=crate::css! { "letter-spacing": "var(--tracking-section)" }>
                {t(keys::cc::PAGE_SETUP, &[])}
            </Overline>
            <div style=list.to_style()>{rows}</div>
        </div>
    }
}

#[derive(Clone, Copy)]
enum ChoiceLabel {
    Plain(&'static str),
    Key(Key),
}

/// tC4's `PageSetupChoiceRow`: a label and its choices, each a pressed or unpressed chip.
fn choice_row<T: Copy + PartialEq + Send + Sync + 'static>(
    label: Key,
    label_id: &'static str,
    options: Vec<(ChoiceLabel, T)>,
    value: Signal<T>,
    on_change: Callback<T>,
) -> AnyView {
    let row = crate::css! {
        "display": "flex",
        "align-items": "center",
        "justify-content": "space-between",
        "gap": 8,
    };
    let chips = options
        .into_iter()
        .map(|(text, option)| {
            let selected = Signal::derive(move || value.get() == option);
            let style = Signal::derive(move || choice_style(selected.get()));
            let text = match text {
                ChoiceLabel::Plain(text) => text.to_owned(),
                ChoiceLabel::Key(key) => t(key, &[]),
            };
            view! {
                <FilterChip
                    selected=selected
                    style=style
                    attrs=Attrs::from([("type", "button")])
                    attr:aria-pressed=move || if selected.get() { "true" } else { "false" }
                    on_click=Callback::new(move |_| on_change.run(option))
                >
                    {text.clone()}
                </FilterChip>
            }
        })
        .collect_view();
    view! {
        <div style=row.to_style()>
            <span id=label_id>{t(label, &[])}</span>
            <span role="group" aria-labelledby=label_id style="display: flex; gap: 4px;">
                {chips}
            </span>
        </div>
    }
    .into_any()
}

fn choice_style(selected: bool) -> Css {
    let mut style = crate::css! {
        "display": "inline-block",
        "padding": "5px 11px",
        "font-size": "var(--fs-meta)",
        "letter-spacing": "var(--track-11-5)",
        "border-width": 1,
    };
    style.extend(&if selected {
        crate::css! {
            "background": "var(--accent)",
            "color": "var(--text-inverse)",
            "border-color": "var(--accent)",
        }
    } else {
        crate::css! {
            "background": "var(--surface-card)",
            "color": "var(--text-secondary)",
            "border-color": "var(--border-input)",
        }
    });
    style
}

/// "Incomplete draft." and what the print does with what is not drafted.
fn incomplete(body: Key) -> impl IntoView {
    view! {
        <Callout tone=CalloutTone::Kindle>
            <strong style="color: var(--uw-kindle);">{t(keys::cc::INCOMPLETE_TITLE, &[])}</strong>
            " "
            {t(body, &[])}
        </Callout>
    }
}

// ---------------------------------------------------------------------------------------------
// An OBS project
// ---------------------------------------------------------------------------------------------

/// tC4's `StoryCommunityChecking`.
fn story_checking(
    actions: &Actions,
    at: Scope,
    dir: Direction,
    setup: RwSignal<PageSetup>,
    title: String,
) -> impl IntoView + use<> {
    let Some(runtime) = use_runtime() else {
        return ().into_any();
    };
    let of = ExportOf {
        at: at.clone(),
        subject: Subject::Stories,
        title,
    };
    let read = runtime.cache().read(Stories { at });
    let actions = actions.clone();
    (move || {
        read.with(|loaded| match loaded {
            Load::Loading => note(t(keys::cc::STORIES_LOADING, &[])),
            Load::Failed(error) => note(
                format!("{} {error}", t(keys::cc::STORIES_LOAD_ERROR, &[]))
                    .trim()
                    .to_owned(),
            ),
            Load::Ready(print) | Load::Refreshing(print) => {
                stories_screen(&actions, print.clone(), dir, setup, of.clone())
            }
        })
    })
    .into_any()
}

fn stories_screen(
    actions: &Actions,
    print: Arc<StoriesPrint>,
    dir: Direction,
    setup: RwSignal<PageSetup>,
    of: ExportOf,
) -> AnyView {
    let d = dir_attr(dir);
    let incomplete_note = (print.undrafted && !print.items.is_empty())
        .then(|| incomplete(keys::cc::INCOMPLETE_BODY_OBS));
    let pages = {
        let print = print.clone();
        move || {
            let setup = setup.get();
            let mut out: Vec<AnyView> = Vec::new();
            if print.items.is_empty() {
                let style = crate::css! {
                    "text-align": "center",
                    "color": "var(--text-tertiary)",
                    "font-size": "var(--fs-ui)",
                };
                out.push(
                    view! {
                        <p data-testid="cc-nothing-drafted" style=style.to_style()>
                            {t(keys::cc::NOTHING_DRAFTED_OBS, &[])}
                        </p>
                    }
                    .into_any(),
                );
            }
            for item in &print.items {
                out.push(match item {
                    PrintStoryItem::Gap(run) => {
                        let style = gap_line().merge(&crate::css! {
                            "text-align": "center",
                            "margin": "0 0 24px",
                        });
                        view! {
                            <p dir=d data-testid="cc-story-gap" style=style.to_style()>
                                {story_gap_text(*run)}
                            </p>
                        }
                        .into_any()
                    }
                    PrintStoryItem::Story(story) => story_page(story, &setup, dir).into_any(),
                });
            }
            out
        }
    };
    let pictures = Signal::derive(move || setup.get().pictures == Pictures::Shown);
    let rows = {
        let layout = choice_row(
            keys::cc::LAYOUT,
            "cc-layout-label",
            vec![
                (ChoiceLabel::Key(keys::cc::LAYOUT_ABOVE), ObsLayout::Above),
                (
                    ChoiceLabel::Key(keys::cc::LAYOUT_WRAPPED),
                    ObsLayout::Wrapped,
                ),
            ],
            Signal::derive(move || setup.get().obs_layout),
            Callback::new(move |layout| setup.update(|s| s.obs_layout = layout)),
        );
        view! {
            {layout}
            <Toggle
                label=t(keys::cc::PICTURES, &[])
                checked=pictures
                attrs=Attrs::from([("data-testid", "cc-pictures")])
                on_change=Callback::new(move |_| setup.update(|s| {
                    s.pictures = match s.pictures {
                        Pictures::Shown => Pictures::Hidden,
                        Pictures::Hidden => Pictures::Shown,
                    };
                }))
            />
        }
        .into_any()
    };
    view! {
        <div style=root_style().to_style() data-testid="community-checking">
            <main style=main_style().to_style()>{pages}</main>
            <aside style=aside_style().to_style()>
                {side_head(actions, ProjectKind::Stories, of, setup)}
                {setup_box(rows)}
                {incomplete_note}
            </aside>
        </div>
    }
    .into_any()
}

fn gap_line() -> Css {
    crate::css! {
        "font-family": "var(--font-scripture)",
        "font-size": "var(--fs-verse-md)",
        "line-height": "var(--lh-verse-md)",
        "color": "var(--text-tertiary)",
        "margin": "0 0 26px",
    }
}

/// tC4's `StoryPage`: the title, each drafted frame's picture and text, one line for each run
/// of undrafted frames, then the reference line.
fn story_page(story: &PrintStory, setup: &PageSetup, dir: Direction) -> impl IntoView + use<> {
    let d = dir_attr(dir);
    let wrapped = setup.obs_layout == ObsLayout::Wrapped;
    let pictures = setup.pictures == Pictures::Shown;
    let page = crate::css! {
        "max-width": 680,
        "margin": "0 auto",
        "background": "#fff",
        "box-shadow": "var(--shadow-page)",
        "border-radius": 4,
        "padding": "64px 72px",
    }
    .merge(&crate::css! { "margin": "0 auto 24px" });
    let eyebrow = crate::css! {
        "text-align": "center",
        "font-size": "var(--fs-label)",
        "font-weight": "var(--fw-heavy)",
        "letter-spacing": "var(--tracking-eyebrow)",
        "text-transform": "uppercase",
        "color": "var(--text-tertiary)",
        "margin": "0 0 4px",
    };
    let h1 = crate::css! {
        "text-align": "center",
        "font-family": "var(--font-scripture)",
        "font-size": "var(--fs-display)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--text-heading)",
        "margin": "0 0 6px",
    };
    let rule = crate::css! {
        "height": 1,
        "background": "var(--border)",
        "margin": "0 auto 30px",
        "width": 70,
    };
    let picture_style = if wrapped {
        let (float, margin) = match dir {
            Direction::Rtl => ("right", "0 0 6px 12px"),
            Direction::Ltr => ("left", "0 12px 6px 0"),
        };
        crate::css! { "float": float, "width": "25%", "margin": margin }
    } else {
        crate::css! { "display": "block", "width": "100%", "margin-bottom": 12 }
    }
    .merge(&crate::css! {
        "border-radius": "var(--radius-md)",
        "background": "var(--surface-sunken)",
    });
    let text_style = crate::css! {
        "font-family": "var(--font-scripture)",
        "font-size": "var(--fs-verse-md)",
        "line-height": "var(--lh-verse-md)",
        "color": "var(--text-scripture)",
        "text-align": "justify",
        "white-space": "pre-wrap",
        "margin": 0,
    };
    let title = if story.title.is_empty() {
        t(
            keys::story_draft::STORY_NUMBER,
            &[("n", &story.story.get().to_string())],
        )
    } else {
        story.title.clone()
    };
    let frames = story
        .frames
        .iter()
        .map(|frame| match frame {
            PrintFrame::Gap(run) => view! {
                <p dir=d data-testid="cc-frame-gap" style=gap_line().to_style()>
                    {frame_gap_text(*run)}
                </p>
            }
            .into_any(),
            PrintFrame::Drafted(frame) => {
                let n = frame.frame.to_string();
                let mut box_style = crate::css! { "margin-bottom": 26 };
                if wrapped {
                    box_style.set("display", "flow-root");
                }
                let picture = frame.picture.clone().filter(|_| pictures).map(|src| {
                    view! {
                        <img
                            data-testid=format!("cc-picture-{n}")
                            src=src
                            alt=t(keys::story_draft::IMAGE_ALT, &[("n", &n)])
                            style=picture_style.to_style()
                        />
                    }
                });
                view! {
                    <div data-testid=format!("cc-frame-{n}") style=box_style.to_style()>
                        {picture}
                        <p dir=d style=text_style.to_style()>{frame.text.clone()}</p>
                    </div>
                }
                .into_any()
            }
        })
        .collect_view();
    let reference_style = crate::css! {
        "font-family": "var(--font-scripture)",
        "font-size": "var(--fs-verse-md)",
        "font-style": "italic",
        "color": "var(--text-secondary)",
        "margin": 0,
    };
    let reference = story.reference.clone().map(|reference| {
        view! {
            <p dir=d data-testid="cc-reference" style=reference_style.to_style()>{reference}</p>
        }
    });
    view! {
        <div
            style=page.to_style()
            data-testid="cc-story"
            data-story=story.story.get().to_string()
            data-pictures=if pictures { "1" } else { "0" }
            data-layout=if wrapped { "wrapped" } else { "above" }
        >
            <p style=eyebrow.to_style()>{t(keys::cc::EYEBROW, &[])}</p>
            <h1 style=h1.to_style() dir=d>{title}</h1>
            <div style=rule.to_style()></div>
            {frames}
            {reference}
        </div>
    }
}
