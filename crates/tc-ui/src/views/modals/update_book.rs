//! Update a book from USFM (unit F1; STORAGE §13.6, owner ruling OE 5): the file, the book, then
//! the run and its report, "Titus: 12 verses imported, 2 to review.", with the link to the
//! conflict review. No Undo: History keeps every version (delta Q7). tC4 4.0 has no such
//! dialog; it is a declared difference (APP-DESIGN F1 row). "Update a story from Markdown"
//! waits for a backend: tc-api has no story update.

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::import::UpdateReport;
use tc_core::BookId;

use crate::app::forms::{ImportFailure, ImportUpload};
use crate::app::update_book::{UpdateBookForm, UpdateState, UpdateStep, verse_list};
use crate::app::web::app_ctx::Dialog;
use crate::app::web::{Actions, AppCtx, use_app};
use crate::ds::{
    Attrs, Button, ButtonVariant, Callout, CalloutTone, Corner, DropZone, Edge, Fill, Inset,
    Overline, ProgressBar, Select, Spinner, Surface, Text, TextRole, TextTone, Tone,
};
use crate::i18n::{Key, keys, t};

use super::source_texts::book_options;

fn read<T: Clone + PartialEq + Send + Sync + 'static>(
    ub: &ArcRwSignal<Option<UpdateBookForm>>,
    get: impl Fn(&UpdateBookForm) -> T + Send + Sync + 'static,
    none: T,
) -> Memo<T> {
    let ub = ub.clone();
    Memo::new(move |_| ub.with(|ub| ub.as_ref().map_or_else(|| none.clone(), &get)))
}

/// The dialog, while `s.modal` is Update a book and its form is set.
#[component]
pub fn UpdateBook() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let open = {
        let (dialog, ub) = (s.layers.dialog.clone(), s.ub.clone());
        Memo::new(move |_| dialog.get() == Some(Dialog::UpdateBook) && ub.with(Option::is_some))
    };
    let ub = s.ub.clone();
    let step = read(&ub, UpdateBookForm::step, UpdateStep::File);
    // The Modal reads its title once: a step draws it again.
    (move || {
        let _ = step.get();
        open.get().then(|| modal(&actions, &ub))
    })
    .into_any()
}

fn modal(actions: &Actions, ub: &ArcRwSignal<Option<UpdateBookForm>>) -> AnyView {
    let step = read(ub, UpdateBookForm::step, UpdateStep::File);
    let title = move || {
        let overline = match step.get() {
            UpdateStep::File => keys::update_book::step::FILE,
            UpdateStep::Book => keys::update_book::step::BOOK,
            UpdateStep::Working | UpdateStep::Report | UpdateStep::Failed => {
                keys::update_book::step::RUN
            }
        };
        view! {
            <Overline tag="span" style=crate::css! { "display": "block", "margin-bottom": 8 }>
                {t(overline, &[])}
            </Overline>
            {t(keys::update_book::TITLE, &[])}
        }
    };
    let subtitle = move || {
        move || {
            t(
                match step.get() {
                    UpdateStep::File => keys::update_book::subtitle::FILE,
                    UpdateStep::Book => keys::update_book::subtitle::BOOK,
                    UpdateStep::Working | UpdateStep::Report => keys::update_book::subtitle::RUN,
                    UpdateStep::Failed => keys::importer::subtitle::FAILED,
                },
                &[],
            )
        }
    };
    let footer: ChildrenFn = {
        let (actions, ub) = (actions.clone(), ub.clone());
        Arc::new(move || {
            let (actions, ub) = (actions.clone(), ub.clone());
            (move || footer(&actions, &ub, step.get())).into_any()
        })
    };
    let children: ChildrenFn = {
        let (actions, ub) = (actions.clone(), ub.clone());
        Arc::new(move || {
            let (actions, ub) = (actions.clone(), ub.clone());
            let body = {
                let ub = ub.clone();
                move || match step.get() {
                    UpdateStep::File => file_step(&actions, &ub).into_any(),
                    UpdateStep::Book => book_step(&actions, &ub).into_any(),
                    UpdateStep::Working => working_step(&ub).into_any(),
                    UpdateStep::Report => report_step(&ub).into_any(),
                    UpdateStep::Failed => failed_step(&ub).into_any(),
                }
            };
            let problem = read(
                &ub,
                |ub| match &ub.state {
                    UpdateState::Problem(problem) => Some(problem.clone()),
                    _ => None,
                },
                None,
            );
            view! {
                {body}
                {move || problem.get().map(|problem| view! {
                    <Callout tone=CalloutTone::Warn style=crate::css! { "overflow-wrap": "anywhere" } attrs=Attrs::from([("role", "alert"), ("data-testid", "update-book-error")])>
                        {problem}
                    </Callout>
                })}
            }
            .into_any()
        })
    };
    let close = actions.clone();
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some(title.into()),
        subtitle: Some(subtitle.into()),
        width: Some(560.into()),
        close_label: Some(Signal::derive(|| t(keys::common::CLOSE, &[]))),
        on_close: Some(Callback::new(move |()| close.close_modal())),
        closable: Signal::derive(move || step.get() != UpdateStep::Working),
        footer: Some(footer),
        attrs: Attrs::from([("data-testid", "update-book-modal")]),
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}

fn footer(
    actions: &Actions,
    ub: &ArcRwSignal<Option<UpdateBookForm>>,
    step: UpdateStep,
) -> AnyView {
    let spacer = || view! { <div style="flex: 1;"></div> };
    let button = |label: Key,
                  variant: ButtonVariant,
                  id: &'static str,
                  run: Callback<web_sys::MouseEvent>| {
        view! {
            <Button variant=variant attrs=Attrs::from([("data-testid", id)]) on_click=run>
                {t(label, &[])}
            </Button>
        }
    };
    let (a, b, c) = (actions.clone(), actions.clone(), actions.clone());
    let close = move || {
        let a = a.clone();
        button(
            keys::common::CLOSE,
            ButtonVariant::Secondary,
            "update-book-close",
            Callback::new(move |_| a.close_modal()),
        )
    };
    match step {
        UpdateStep::File => {
            let off = read(ub, |ub| !ub.can_choose_book(), true);
            view! {
                {spacer()}
                <Button variant=ButtonVariant::Secondary on_click=Callback::new(move |_| b.close_modal())>
                    {t(keys::new_bible::CANCEL, &[])}
                </Button>
                <Button
                    disabled=Signal::from(off)
                    attrs=Attrs::from([("data-testid", "update-book-next")])
                    on_click=Callback::new(move |_| c.patch_ub(UpdateBookForm::to_book))
                >
                    {t(keys::update_book::NEXT, &[])}
                </Button>
            }
            .into_any()
        }
        UpdateStep::Book => {
            let busy = read(ub, |ub| ub.submit.busy(), false);
            let cancel = actions.clone();
            view! {
                {button(keys::update_book::BACK, ButtonVariant::Ghost, "update-book-back", Callback::new(move |_| b.patch_ub(UpdateBookForm::back_to_file)))}
                {spacer()}
                {button(keys::new_bible::CANCEL, ButtonVariant::Secondary, "update-book-close", Callback::new(move |_| cancel.close_modal()))}
                <Button
                    disabled=Signal::from(busy)
                    attrs=Attrs::from([("data-testid", "update-book-run")])
                    on_click=Callback::new(move |_| c.update_book_run())
                >
                    {t(keys::update_book::RUN, &[])}
                </Button>
            }
            .into_any()
        }
        UpdateStep::Working => view! {
            {spacer()}
            {button(keys::new_bible::CANCEL, ButtonVariant::Secondary, "update-book-cancel", Callback::new(move |_| b.update_book_cancel()))}
        }
        .into_any(),
        UpdateStep::Report => {
            let to_review = read(
                ub,
                |ub| matches!(&ub.state, UpdateState::Done(r) if r.to_review > 0),
                false,
            );
            view! {
                {spacer()}
                {close()}
                {move || {
                    let c = c.clone();
                    to_review.get().then(|| button(keys::update_book::REVIEW, ButtonVariant::Primary, "update-book-review", Callback::new(move |_| c.update_book_review())))
                }}
            }
            .into_any()
        }
        UpdateStep::Failed => view! {
            {spacer()}
            {close()}
            {button(keys::update_book::AGAIN, ButtonVariant::Primary, "update-book-again", Callback::new(move |_| b.patch_ub(UpdateBookForm::back_to_file)))}
        }
        .into_any(),
    }
}

fn file_step(actions: &Actions, ub: &ArcRwSignal<Option<UpdateBookForm>>) -> impl IntoView + use<> {
    let input = NodeRef::<leptos::html::Input>::new();
    let choose = {
        let actions = actions.clone();
        move |list: Option<web_sys::FileList>| {
            if let Some(list) = list.filter(|list| list.length() > 0) {
                actions.update_book_file(list);
            }
        }
    };
    let on_change = {
        let choose = choose.clone();
        move |_| {
            if let Some(input) = input.get() {
                choose(input.files());
                input.set_value("");
            }
        }
    };
    let on_drop = move |event: web_sys::DragEvent| {
        event.prevent_default();
        choose(event.data_transfer().and_then(|transfer| transfer.files()));
    };
    let click = Callback::new(move |_: web_sys::Event| {
        if let Some(input) = input.get() {
            input.click();
        }
    });
    let file = read(ub, |ub| ub.file.clone(), None);
    let row = move || {
        file.get().map(|file| {
            let note = match &file.upload {
                ImportUpload::Sent(_) => None,
                ImportUpload::Sending => Some(t(keys::add_book::USFM_READING, &[])),
                ImportUpload::Failed(why) => Some(why.clone()),
            };
            view! {
                <Surface fill=Fill::Card border=Edge::Line radius=Corner::Md pad="10px 14px" attrs=Attrs::from([("data-testid", "update-book-file")])>
                    <div style="display: flex; flex-direction: column; gap: 3px;">
                        <Text role=TextRole::Strong truncate=true>{file.name.clone()}</Text>
                        {note.map(|note| view! { <Text role=TextRole::Caption tone=TextTone::Muted>{note}</Text> })}
                    </div>
                </Surface>
            }
        })
    };
    view! {
        <div
            data-testid="update-book-files"
            style="display: flex; flex-direction: column; gap: 14px;"
            on:dragover=move |event: web_sys::DragEvent| event.prevent_default()
            on:drop=on_drop
        >
            <input node_ref=input type="file" accept=".usfm,.sfm,.txt" data-testid="update-book-input" hidden on:change=on_change />
            <DropZone
                title=t(keys::update_book::DROP, &[])
                hint=t(keys::update_book::HINT, &[])
                attrs=Attrs::from([("data-testid", "update-book-drop")])
                on_click=click
            />
            {row}
        </div>
    }
}

fn book_step(actions: &Actions, ub: &ArcRwSignal<Option<UpdateBookForm>>) -> impl IntoView + use<> {
    let value = {
        let ub = ub.clone();
        Signal::derive(move || ub.with(|ub| ub.as_ref().map(|ub| ub.book.code().to_owned())))
    };
    let on_book = {
        let actions = actions.clone();
        Callback::new(move |code: String| {
            if let Ok(book) = code.parse::<BookId>() {
                actions.update_book_choose(book);
            }
        })
    };
    view! {
        <div data-testid="update-book-book" style="display: flex; flex-direction: column; gap: 8px; padding-bottom: 12px;">
            <Select
                id="update-book-select"
                label=Signal::derive(|| t(keys::update_book::BOOK_LABEL, &[]))
                value=value
                on_change=on_book
                search_placeholder=Signal::derive(|| t(keys::add_book::FIND_BOOK, &[]))
                no_matches_label=Signal::derive(|| t(keys::add_book::NO_MATCHES, &[]))
                clear_label=Signal::derive(|| t(keys::common::CLEAR, &[]))
                options=book_options(&[])
            />
            <Text role=TextRole::Caption tone=TextTone::Muted>{t(keys::update_book::BOOK_HINT, &[])}</Text>
        </div>
    }
}

fn working_step(ub: &ArcRwSignal<Option<UpdateBookForm>>) -> impl IntoView + use<> {
    let book = ub.with_untracked(|ub| ub.as_ref().map_or(BookId::Genesis, |ub| ub.book));
    let label = t(
        keys::update_book::WORKING,
        &[("book", &book.english_name())],
    );
    let fraction = read(
        ub,
        |ub| match &ub.state {
            UpdateState::Running {
                progress: Some(p), ..
            } => p.total.filter(|n| *n > 0).map(|total| {
                #[expect(clippy::cast_precision_loss, reason = "a step count far below 2^52")]
                let f = p.done as f64 / total as f64;
                f.clamp(0.0, 1.0)
            }),
            _ => None,
        },
        None,
    );
    view! {
        <div style="padding: 26px 0 30px; display: flex; flex-direction: column; align-items: center; gap: 16px;" data-testid="update-book-working">
            <Spinner label=label.clone() />
            {move || fraction.get().map(|f| view! {
                <ProgressBar value=Signal::derive(move || f) label=Some(label.clone()) style=crate::css! { "width": "100%" } />
            })}
        </div>
    }
}

/// The report's lines (delta U6): the counts, the book added, the verses left as they were and
/// those whose alignment markup was kept.
pub fn report_lines(report: &UpdateReport) -> Vec<(&'static str, String)> {
    let book = report.book.english_name();
    let mut lines = vec![(
        "update-book-counts",
        if report.imported == 1 {
            t(
                keys::update_book::REPORT_ONE,
                &[("book", &book), ("review", &report.to_review)],
            )
        } else {
            t(
                keys::update_book::REPORT,
                &[
                    ("book", &book),
                    ("n", &report.imported),
                    ("review", &report.to_review),
                ],
            )
        },
    )];
    if report.book_added {
        lines.push((
            "update-book-added",
            t(keys::update_book::ADDED, &[("book", &book)]),
        ));
    }
    if !report.left_as_it_was.is_empty() {
        lines.push((
            "update-book-left",
            t(
                keys::update_book::LEFT_AS_IT_WAS,
                &[("verses", &verse_list(&report.left_as_it_was))],
            ),
        ));
    }
    if !report.markup_kept.is_empty() {
        lines.push((
            "update-book-markup",
            t(
                keys::update_book::MARKUP_KEPT,
                &[("verses", &verse_list(&report.markup_kept))],
            ),
        ));
    }
    lines
}

fn report_step(ub: &ArcRwSignal<Option<UpdateBookForm>>) -> AnyView {
    let report = ub.with_untracked(|ub| match ub.as_ref().map(|ub| &ub.state) {
        Some(UpdateState::Done(report)) => Some(report.clone()),
        _ => None,
    });
    let Some(report) = report else {
        return ().into_any();
    };
    let lines = report_lines(&report)
        .into_iter()
        .map(|(id, text)| {
            let role = if id == "update-book-counts" {
                TextRole::Strong
            } else {
                TextRole::Ui
            };
            view! { <Text role=role attrs=Attrs::from([("data-testid", id)])>{text}</Text> }
        })
        .collect_view();
    view! {
        <div
            data-testid="update-book-report"
            data-imported=report.imported.to_string()
            data-review=report.to_review.to_string()
            style="display: flex; flex-direction: column; gap: 10px; padding-bottom: 12px;"
        >
            {lines}
            <Text role=TextRole::Caption tone=TextTone::Muted>{t(keys::update_book::HISTORY, &[])}</Text>
        </div>
    }
    .into_any()
}

fn failed_step(ub: &ArcRwSignal<Option<UpdateBookForm>>) -> AnyView {
    let failure = ub.with_untracked(|ub| match ub.as_ref().map(|ub| &ub.state) {
        Some(UpdateState::Failed(failure)) => Some(failure.clone()),
        _ => None,
    });
    let Some(ImportFailure {
        code,
        sentence,
        detail,
    }) = failure
    else {
        return ().into_any();
    };
    let below = [code, detail]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    view! {
        <Surface tone=Tone::Invalid fill=Fill::Soft border=Edge::Tone radius=Corner::Lg pad=Inset::Md attrs=Attrs::from([("role", "alert"), ("data-testid", "update-book-failed")])>
            <div style="display: flex; flex-direction: column; gap: 6px;">
                <Text role=TextRole::Strong tone=TextTone::Tone>{t(keys::update_book::FAILED_TITLE, &[])}</Text>
                <Text role=TextRole::Ui attrs=Attrs::from([("data-testid", "update-book-refusal")])>{sentence}</Text>
                <Text role=TextRole::Meta tone=TextTone::Muted style=crate::css! { "overflow-wrap": "anywhere" }>{below}</Text>
            </div>
        </Surface>
    }
    .into_any()
}
