//! Add a book (M4, `tC4:src/views/modals/AddBook.jsx`): the method step, a blank book picked one
//! at a time or several at once, and books from USFM files, each file shown with its verdict
//! the moment the server has read it (unit C6, tC4 #484).
//!
//! Two differences from tC4, declared: the warning under a pick of a book the project holds
//! does not add that "creating it again will replace the existing book and its drafts", since
//! Add a book never replaces a book (tC4 filters the held books out too); and the files are read
//! on the server, so a row shows "Reading the file…" until it answers.

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::create::UsfmProblem;
use tc_core::BookId;

use super::source_texts::{CHAPTERS, book_options};
use crate::app::forms::{AddBookForm, AddBookStep, RowState, UsfmRow};
use crate::app::web::{Actions, AppCtx, use_app};
use crate::ds::{
    Attrs, Button, ButtonVariant, Callout, CalloutTone, Corner, DropZone, Edge, Fill, FilterChip,
    IconButton, IconButtonVariant, OptionCard, Overline, Select, Status, StatusDot, Surface, Text,
    TextRole, TextTone,
};
use crate::i18n::{keys, t, t_dyn};

/// The drafting sources the pick step names (tC4 `SUITE_VERSION`).
fn suite_version() -> &'static str {
    tc_api::create::SUITE_VERSION
}

fn chapters(book: BookId) -> u16 {
    CHAPTERS
        .get(usize::from(book.ordinal()) - 1)
        .copied()
        .unwrap_or(0)
}

fn row_words(row: &UsfmRow) -> String {
    match &row.state {
        RowState::Reading => t(keys::add_book::USFM_READING, &[]),
        RowState::Adds(book) => t(
            keys::add_book::USFM_WILL_ADD,
            &[("name", &book.english_name())],
        ),
        RowState::Refused(problem) => problem_words(problem, &row.name),
        RowState::Unreadable(reason) => reason.clone(),
    }
}

/// The sentence of a refused file.
pub fn problem_words(problem: &UsfmProblem, file: &str) -> String {
    let owned = problem.vars(file);
    let vars: Vec<(&str, &dyn std::fmt::Display)> = owned
        .iter()
        .map(|(k, v)| (*k, v as &dyn std::fmt::Display))
        .collect();
    t_dyn(problem.key(), &vars, None)
}

/// The dialog, while `s.modal` is AddBook.
#[component]
pub fn AddBook() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let ab = s.ab.clone();
    let read = {
        let ab = ab.clone();
        move |get: fn(&AddBookForm) -> bool| {
            let ab = ab.clone();
            Signal::derive(move || ab.with(|ab| ab.as_ref().is_some_and(get)))
        }
    };
    let step = {
        let ab = ab.clone();
        Memo::new(move |_| ab.with(|ab| ab.as_ref().map_or(AddBookStep::Method, |ab| ab.step)))
    };
    let busy = read(|ab| ab.busy);
    let add_disabled = read(AddBookForm::add_disabled);
    let footer: ChildrenFn = {
        let (actions, ab) = (actions.clone(), ab.clone());
        Arc::new(move || {
            let (actions, ab) = (actions.clone(), ab.clone());
            (move || match step.get() {
                AddBookStep::Method => ().into_any(),
                AddBookStep::Pick => {
                    let (back, create, ab) = (actions.clone(), actions.clone(), ab.clone());
                    let label = move || {
                        ab.with(|ab| {
                            ab.as_ref().map(|ab| {
                                if ab.multi {
                                    t(keys::add_book::CREATE_N, &[("n", &ab.picked().len())])
                                } else {
                                    t(keys::add_book::CREATE, &[])
                                }
                            })
                        })
                        .unwrap_or_default()
                    };
                    view! {
                        <Button variant=ButtonVariant::Secondary on_click=Callback::new(move |_| back.ab_choose(AddBookStep::Method))>
                            {move || t(keys::add_book::BACK, &[])}
                        </Button>
                        <Button disabled=busy on_click=Callback::new(move |_| create.add_books())>
                            {label}
                        </Button>
                    }
                    .into_any()
                }
                AddBookStep::Usfm => {
                    let (back, add, ab) = (actions.clone(), actions.clone(), ab.clone());
                    let label = move || {
                        let n = ab.with(|ab| ab.as_ref().map_or(0, AddBookForm::addable));
                        if n > 1 {
                            t(keys::add_book::ADD_N, &[("n", &n)])
                        } else {
                            t(keys::add_book::ADD_ONE, &[])
                        }
                    };
                    view! {
                        <Button variant=ButtonVariant::Secondary on_click=Callback::new(move |_| back.ab_choose(AddBookStep::Method))>
                            {move || t(keys::add_book::BACK, &[])}
                        </Button>
                        <Button
                            disabled=add_disabled
                            attrs=Attrs::from([("data-testid", "ab-usfm-add")])
                            on_click=Callback::new(move |_| add.add_usfm_books())
                        >
                            {label}
                        </Button>
                    }
                    .into_any()
                }
            })
            .into_any()
        })
    };
    let children: ChildrenFn = {
        let (actions, ab) = (actions.clone(), ab.clone());
        Arc::new(move || {
            let (actions, ab) = (actions.clone(), ab.clone());
            (move || match step.get() {
                AddBookStep::Method => method_step(&actions).into_any(),
                AddBookStep::Usfm => usfm_step(&actions, &ab).into_any(),
                AddBookStep::Pick => pick_step(&actions, &ab).into_any(),
            })
            .into_any()
        })
    };
    let name = {
        let ab = ab.clone();
        move || {
            ab.with(|ab| {
                ab.as_ref()
                    .map(|ab: &AddBookForm| ab.project_name.clone())
                    .unwrap_or_default()
            })
        }
    };
    let subtitle = move || {
        let name = name.clone();
        view! {
            {move || t(keys::add_book::TO, &[])}" "
            <strong style="color: var(--uw-ocean);">{name}</strong>
        }
    };
    let close = actions.clone();
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some((move || move || t(keys::add_book::TITLE, &[])).into()),
        subtitle: Some(subtitle.into()),
        width: Some(600.into()),
        close_label: Some(Signal::derive(|| t(keys::new_bible::CLOSE, &[]))),
        on_close: Some(Callback::new(move |()| close.close_modal())),
        footer: Some(footer),
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}

fn method_step(actions: &Actions) -> impl IntoView + use<> {
    let (blank, usfm) = (actions.clone(), actions.clone());
    view! {
        <div style="display: flex; flex-direction: column; gap: 10px; padding-bottom: 18px;">
            <OptionCard
                icon="+"
                title=move || t(keys::add_book::BLANK_TITLE, &[])
                description=move || t(keys::add_book::BLANK_DESC, &[])
                trailing="→"
                on_click=Callback::new(move |_| blank.ab_choose(AddBookStep::Pick))
            />
            <OptionCard
                icon=move || t(keys::importer::kind::usfm::ICON, &[])
                title=move || t(keys::add_book::USFM_TITLE, &[])
                description=move || t(keys::add_book::USFM_DESC, &[])
                trailing="→"
                attrs=Attrs::from([("data-testid", "ab-usfm-option")])
                on_click=Callback::new(move |_| usfm.ab_choose(AddBookStep::Usfm))
            />
        </div>
    }
}

/// The error under a step, kept until the next add (#564).
fn error_callout(ab: &ArcRwSignal<Option<AddBookForm>>) -> impl IntoView + use<> {
    let ab = ab.clone();
    move || {
        ab.with(|ab| ab.as_ref().and_then(|ab| ab.error.clone())).map(|error| {
            view! { <Callout tone=CalloutTone::Warn attrs=Attrs::from([("role", "alert")])>{error}</Callout> }
        })
    }
}

/// The from-USFM-files step (tC4 `UsfmStep`).
fn usfm_step(actions: &Actions, ab: &ArcRwSignal<Option<AddBookForm>>) -> impl IntoView + use<> {
    let input = NodeRef::<leptos::html::Input>::new();
    let add = {
        let actions = actions.clone();
        move |list: Option<web_sys::FileList>| {
            if let Some(list) = list.filter(|list| list.length() > 0) {
                actions.ab_add_files(list);
            }
        }
    };
    let on_change = {
        let add = add.clone();
        move |_| {
            if let Some(input) = input.get() {
                add(input.files());
                // The same file can be chosen again after it was removed.
                input.set_value("");
            }
        }
    };
    let on_drop = move |event: web_sys::DragEvent| {
        event.prevent_default();
        add(event.data_transfer().and_then(|transfer| transfer.files()));
    };
    let click = Callback::new(move |_: web_sys::Event| {
        if let Some(input) = input.get() {
            input.click();
        }
    });
    let rows = {
        let (actions, ab) = (actions.clone(), ab.clone());
        move || {
            let rows = ab.with(|ab| ab.as_ref().map(AddBookForm::rows).unwrap_or_default());
            rows.into_iter()
                .map(|row| usfm_row(&actions, row))
                .collect_view()
        }
    };
    view! {
        <div data-testid="ab-usfm" style="display: flex; flex-direction: column; gap: 14px; padding-bottom: 18px;">
            <input
                node_ref=input
                type="file"
                multiple
                accept=".usfm,.sfm,.txt"
                data-testid="ab-usfm-input"
                hidden
                on:change=on_change
            />
            <div
                on:dragover=move |event: web_sys::DragEvent| event.prevent_default()
                on:drop=on_drop
            >
                <DropZone
                    title=t(keys::importer::kind::usfm::DROP, &[])
                    hint=t(keys::importer::kind::usfm::HINT, &[])
                    attrs=Attrs::from([("data-testid", "ab-usfm-drop")])
                    on_click=click
                />
            </div>
            {rows}
            {error_callout(ab)}
        </div>
    }
}

fn usfm_row(actions: &Actions, row: UsfmRow) -> impl IntoView + use<> {
    let valid = matches!(row.state, RowState::Adds(_) | RowState::Reading);
    let remove = {
        let actions = actions.clone();
        let id = row.id;
        Callback::new(move |_| actions.ab_remove_file(id))
    };
    let words = row_words(&row);
    let tone = if valid {
        TextTone::Muted
    } else {
        TextTone::Default
    };
    view! {
        <Surface
            fill=Fill::Card
            border=Edge::Line
            radius=Corner::Md
            pad="10px 10px 10px 14px"
            attrs=Attrs::from([
                ("data-testid", "ab-usfm-file"),
                ("data-status", if valid { "valid" } else { "refused" }),
            ])
        >
            <div style="display: flex; align-items: baseline; gap: 10px;">
                <StatusDot status=if valid { Status::Valid } else { Status::Warn } size=8.0 />
                <div style="flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;">
                    <Text role=TextRole::Strong truncate=true>{row.name.clone()}</Text>
                    <Text role=TextRole::Caption tone=tone>{words}</Text>
                </div>
                <IconButton
                    variant=IconButtonVariant::Plain
                    title=t(keys::importer::files::REMOVE, &[])
                    on_click=Callback::new(move |_| remove.run(()))
                >
                    "✕"
                </IconButton>
            </div>
        </Surface>
    }
}

/// The pick step: one book from the list, or several from the grid (tC4's `pick`).
fn pick_step(actions: &Actions, ab: &ArcRwSignal<Option<AddBookForm>>) -> impl IntoView + use<> {
    let multi = {
        let ab = ab.clone();
        Memo::new(move |_| ab.with(|ab| ab.as_ref().is_some_and(|ab| ab.multi)))
    };
    let body = {
        let (actions, ab) = (actions.clone(), ab.clone());
        move || {
            if multi.get() {
                grid(&actions, &ab).into_any()
            } else {
                single(&actions, &ab).into_any()
            }
        }
    };
    let toggle = {
        let actions = actions.clone();
        Callback::new(move |_| actions.ab_toggle_multi())
    };
    let held = {
        let ab = ab.clone();
        move || {
            let held = ab.with(|ab| ab.as_ref().map(AddBookForm::held_picks).unwrap_or_default());
            let text = match held.as_slice() {
                [] => return None,
                [one] => t(keys::add_book::EXISTS_ONE, &[("name", &one.english_name())]),
                many => {
                    let names = many
                        .iter()
                        .map(|book| book.english_name())
                        .collect::<Vec<_>>()
                        .join(" · ");
                    t(keys::add_book::EXISTS_MANY, &[("names", &names)])
                }
            };
            Some(view! { <Callout tone=CalloutTone::Warn><strong>{text}</strong></Callout> })
        }
    };
    view! {
        <div>
            {body}
            <Button
                variant=ButtonVariant::Ghost
                style=crate::css! { "font-size": "var(--fs-caption)", "letter-spacing": "var(--track-12)", "margin": "10px 0 0" }
                on_click=toggle
            >
                {move || t(if multi.get() { keys::add_book::SINGLE_TOGGLE } else { keys::add_book::MULTI_TOGGLE }, &[])}
            </Button>
            <Callout tone=CalloutTone::Info style=crate::css! { "margin-top": 10 }>
                {move || t(keys::add_book::SOURCES, &[("version", &suite_version())])}
            </Callout>
        </div>
        {held}
        {error_callout(ab)}
    }
}

fn single(actions: &Actions, ab: &ArcRwSignal<Option<AddBookForm>>) -> impl IntoView + use<> {
    let (book, existing) = ab.with_untracked(|ab| {
        ab.as_ref()
            .map(|ab| (ab.book, ab.existing.clone()))
            .unwrap_or((BookId::Genesis, Vec::new()))
    });
    let value = {
        let ab = ab.clone();
        Signal::derive(move || ab.with(|ab| ab.as_ref().map(|ab| ab.book.code().to_owned())))
    };
    let on_book = {
        let actions = actions.clone();
        Callback::new(move |code: String| {
            if let Ok(book) = code.parse::<BookId>() {
                actions.ab_pick_book(book);
            }
        })
    };
    let info = {
        let ab = ab.clone();
        move || {
            let book = ab.with(|ab| ab.as_ref().map_or(book, |ab| ab.book));
            let testament = t(
                if book >= BookId::Matthew {
                    keys::add_book::NT
                } else {
                    keys::add_book::OT
                },
                &[],
            );
            let n = chapters(book);
            t(
                if n == 1 {
                    keys::add_book::INFO_ONE
                } else {
                    keys::add_book::INFO
                },
                &[
                    ("name", &book.english_name()),
                    ("chapters", &n),
                    ("testament", &testament),
                ],
            )
        }
    };
    view! {
        <Select
            id="ab-book"
            label=Signal::derive(|| t(keys::add_book::BOOK, &[]))
            value=value
            on_change=on_book
            search_placeholder=Signal::derive(|| t(keys::add_book::FIND_BOOK, &[]))
            no_matches_label=Signal::derive(|| t(keys::add_book::NO_MATCHES, &[]))
            clear_label=Signal::derive(|| t(keys::common::CLEAR, &[]))
            options=book_options(&existing)
        />
        <p style="font-size: var(--fs-caption-lg); letter-spacing: var(--track-12-5); color: var(--text-tertiary); margin: 8px 0px 0px; line-height: var(--lh-body);">
            {info}
        </p>
    }
}

/// The several-at-once grid: the New Testament, then the Old, a chip for each book.
fn grid(actions: &Actions, ab: &ArcRwSignal<Option<AddBookForm>>) -> impl IntoView + use<> {
    let section = |title: crate::i18n::Key, books: &'static [BookId]| {
        let chips = books
            .iter()
            .map(|book| chip(actions, ab, *book))
            .collect_view();
        view! {
            <div style="margin-top: 10px;">
                <Overline tag="div" style=crate::css! { "letter-spacing": "var(--tracking-label)", "margin": "8px 0 6px" }>
                    {move || t(title, &[])}
                </Overline>
                <div style="display: grid; grid-template-columns: repeat(auto-fill,minmax(118px,1fr)); gap: 6px;">
                    {chips}
                </div>
            </div>
        }
    };
    view! {
        <Overline tag="span" style=crate::css! { "display": "block", "margin-bottom": 6 }>
            {move || t(keys::add_book::BOOK, &[])}
        </Overline>
        {section(keys::add_book::NT, &BookId::ALL[39..])}
        {section(keys::add_book::OT, &BookId::ALL[..39])}
    }
}

fn chip(
    actions: &Actions,
    ab: &ArcRwSignal<Option<AddBookForm>>,
    book: BookId,
) -> impl IntoView + use<> {
    let held = ab.with_untracked(|ab| ab.as_ref().is_some_and(|ab| ab.existing.contains(&book)));
    let name = book.english_name();
    if held {
        return view! {
            <span
                title=move || t(keys::add_book::ALREADY_IN, &[])
                style="border: var(--stroke-selected) solid var(--border-hair); background: var(--surface-muted); color: var(--uw-haze); border-radius: var(--radius-sm); padding: 7px 8px; font-size: var(--fs-caption); letter-spacing: var(--track-12); font-weight: var(--fw-bold); text-align: start; font-family: var(--font-ui);"
            >
                {name}" "{move || t(keys::sym::TICK, &[])}
            </span>
        }
        .into_any();
    }
    let selected = {
        let ab = ab.clone();
        Signal::derive(move || ab.with(|ab| ab.as_ref().is_some_and(|ab| ab.books.contains(&book))))
    };
    let actions = actions.clone();
    view! {
        <FilterChip
            selected=selected
            style=crate::css! { "border-radius": "var(--radius-sm)", "padding": "7px 8px", "font-size": "var(--fs-caption)", "justify-content": "flex-start" }
            on_click=Callback::new(move |_| actions.ab_toggle_book(book))
        >
            {name}
        </FilterChip>
    }
    .into_any()
}
