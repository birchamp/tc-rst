//! The one question about books another program changed (D1, owner ruling OE 3,
//! `docs/STORAGE-DELTA-2026-10-21.md` §2.11): asked at open and when the window gets focus, for
//! every book found together, with *Import* and *Not now*; after *Import*, its report with a
//! link to the changes to review. A reload asks again, because `hello` lists the books.

use std::sync::Arc;

use leptos::ev::MouseEvent;
use leptos::prelude::*;

use crate::app::outside::{can_import, question, report_line};
use crate::app::web::{Actions, AppCtx, use_app};
use crate::ds::{
    Attrs, Direction, Dismiss, Fill, Justify, Scrim, ScrimToken, Stack, Surface, Text, TextRole,
    Tone, Weight, action,
};
use crate::i18n::{keys, t};
use crate::tokens::{Corner, Shadow};

/// What the dialog shows: the reports of an *Import* first, else the question.
#[derive(Clone, PartialEq)]
enum Shown {
    Reports(Vec<String>, u32),
    Question(String, Vec<String>, bool),
}

#[component]
pub fn ChangedBooks() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let shown = {
        let (reports, sessions, project) = (
            s.outside_reports.clone(),
            s.server.sessions.clone(),
            s.project.clone(),
        );
        Memo::new(move |_| {
            let reports = reports.get();
            if !reports.is_empty() {
                let review = reports.iter().map(|r| r.to_review).sum();
                return Some(Shown::Reports(
                    reports.iter().map(report_line).collect(),
                    review,
                ));
            }
            let at = project.with(|p| p.as_ref().map(|p| p.at.clone()))?;
            sessions.with(|sessions| {
                let waiting = &sessions.iter().find(|s| s.at == at)?.outside.waiting;
                let reasons = waiting
                    .iter()
                    .filter_map(|w| Some(format!("{}: {}", w.file, w.unreadable.as_ref()?)))
                    .collect();
                Some(Shown::Question(
                    question(waiting)?,
                    reasons,
                    can_import(waiting),
                ))
            })
        })
    };
    (move || shown.get().map(|shown| dialog(&actions, shown))).into_any()
}

fn button(
    weight: Weight,
    tone: Tone,
    label: String,
    run: impl Fn() + Send + Sync + 'static,
) -> impl IntoView {
    action(
        weight,
        tone,
        Some(Callback::new(move |_: MouseEvent| run())),
        label,
    )
}

fn dialog(actions: &Actions, shown: Shown) -> AnyView {
    let id = crate::ds::next_id("tc-outside");
    let label_id = id.clone();
    let reports = matches!(shown, Shown::Reports(..));
    let actions = actions.clone();
    let close = {
        let actions = actions.clone();
        Callback::new(move |_| actions.close_outside_reports())
    };
    let children: ChildrenFn = Arc::new(move || {
        let id = id.clone();
        let a = actions.clone();
        let (lines, buttons) = match shown.clone() {
            Shown::Reports(lines, review) => {
                let link = (review > 0).then(|| {
                    let a = a.clone();
                    let n = review.to_string();
                    let label = t(
                        if review == 1 {
                            keys::conflicts::REVIEW_ONE
                        } else {
                            keys::conflicts::REVIEW_MANY
                        },
                        &[("n", &n)],
                    );
                    button(Weight::Quiet, Tone::Accent, label, move || {
                        a.review_imported()
                    })
                });
                let close = {
                    let a = a.clone();
                    button(
                        Weight::Fill,
                        Tone::Accent,
                        t(keys::common::CLOSE, &[]),
                        move || a.close_outside_reports(),
                    )
                };
                (lines, view! { {link}{close} }.into_any())
            }
            Shown::Question(sentence, reasons, importable) => {
                let not_now = {
                    let a = a.clone();
                    button(
                        Weight::Quiet,
                        Tone::Neutral,
                        t(keys::outside::NOT_NOW, &[]),
                        move || a.not_now_waiting(),
                    )
                };
                let import = importable.then(|| {
                    let a = a.clone();
                    button(
                        Weight::Fill,
                        Tone::Accent,
                        t(keys::outside::IMPORT, &[]),
                        move || a.import_waiting(),
                    )
                });
                (
                    std::iter::once(sentence).chain(reasons).collect(),
                    view! { {not_now}{import} }.into_any(),
                )
            }
        };
        let mut lines = lines.into_iter();
        let head = lines.next().unwrap_or_default();
        let rest = lines
            .map(|line| view! { <Text role=TextRole::Caption>{line}</Text> })
            .collect_view();
        view! {
            <Surface fill=Fill::Card radius=Corner::X2l elevation=Shadow::Modal style=crate::css! { "width": "var(--modal-w-sm)", "max-width": "100%" }>
                <Stack direction=Direction::Column gap=16 style=crate::css! { "padding": 24 } attrs=Attrs::from([("data-testid", "changed-books")])>
                    <Stack direction=Direction::Column gap=5>
                        <Text attrs=Attrs::from([("id", id.as_str())]) role=TextRole::Body>{head}</Text>
                        {rest}
                    </Stack>
                    <Stack direction=Direction::Row gap=10 justify=Justify::End>{buttons}</Stack>
                </Stack>
            </Surface>
        }
        .into_any()
    });
    crate::ds::Layer(crate::ds::LayerProps {
        open: Some(Signal::from(true)),
        scrim: Some(Scrim::Token(ScrimToken::Modal)),
        // The question is answered only by its buttons; a report closes with Escape too.
        dismiss: Dismiss {
            scrim: false,
            escape: reports,
            outside: false,
        },
        trap_focus: true,
        lock_scroll: true,
        on_dismiss: reports.then_some(close).into(),
        role: Some("alertdialog".into()),
        labelled_by: Some(label_id),
        ..crate::ds::LayerProps::new(children)
    })
    .into_any()
}
