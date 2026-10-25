//! The detail column of a tool's session (tC4 `CheckDetail`, `CompareCard`, `OrigPane`,
//! `UltPane`, `StoryPane`, `TargetWords`, `CheckMarks`, `CheckSessionNotices`, `AcademyDrawer`;
//! units B5b and C4): the check's place and counter, its quote, "What to check", the Academy
//! link, the compare card, the marking row with Previous and Next (#726), the bookmark and the
//! check comment. The compare card is tC4's (#762: the source texts as cards in a wrapping row and
//! "Your translation" a card of its own), for a book and a story alike since tC4 rc.2.
//!
//! The detail is built once for a check and then follows its reads in place, so the focus on a
//! word, a button or the comment box survives the read that a mark makes. The words are the one
//! part rebuilt when a read changes them (the draft was edited): the caret starts before the
//! first word again.

use leptos::html;
use leptos::prelude::*;
use tc_api::check::{
    CheckNotice, CheckSources, CheckUnit, DecideEdit, DecisionStatus, GatewayText, GatewayView,
    OriginalVerse, OriginalView, Testament, Tool,
};
use tc_api::helps::ArticleView;
use tc_api::shown::Direction as TextDir;
use wasm_bindgen::JsCast;

use super::rail::category_name;
use crate::app::check::place_parts;
use crate::app::check_item::{self, MarkFailure, WordKey, WordNav, WordStep};
use crate::app::save::Source;
use crate::app::web::check::{Academy, Detail, Marking};
use crate::app::web::{Actions, AppState};
use crate::attrs::Attrs;
use crate::css::Css;
use crate::ds::{
    Button, ButtonVariant, Callout, CalloutTone, Drawer, Overline, OverlineTone, Size,
};
use crate::i18n::{TextDir as LocaleDir, keys, locale_of, shown_locale, t};
use crate::views::helps_panel::{
    block_text, expandable_note, list_indent, number_mark, quoted, with_tables,
};

fn dir_attr(dir: TextDir) -> &'static str {
    match dir {
        TextDir::Ltr => "ltr",
        TextDir::Rtl => "rtl",
    }
}

fn interface_is_rtl() -> bool {
    locale_of(shown_locale()).dir == LocaleDir::Rtl
}

/// What the detail column shows now: nothing while a check is read, the failure of a read, or
/// the check at `index`.
#[derive(Debug, Clone, PartialEq)]
enum Shown {
    Nothing,
    Failed(String),
    Check(u32),
}

/// The detail column of the open tool.
pub fn check_detail(s: &AppState, actions: &Actions, unit: CheckUnit, tool: Tool) -> AnyView {
    let detail = s.c.detail.clone();
    let shown = Memo::new(move |_| {
        detail.with(|d| match d {
            None | Some(Detail::Loading(_)) => Shown::Nothing,
            Some(Detail::Failed(_, error)) => Shown::Failed(error.clone()),
            Some(Detail::Ready(view)) => Shown::Check(view.index),
        })
    });
    let (s, actions) = (s.clone(), actions.clone());
    (move || match shown.get() {
        Shown::Nothing => ().into_any(),
        Shown::Failed(error) => {
            let p = crate::css! {
                "font-size": "var(--fs-ui)",
                "color": "var(--tc-invalid)",
                "line-height": "var(--lh-body)",
                "padding": "32px 40px",
            };
            view! { <p data-testid="check-error" style=p.to_style()>{error}</p> }.into_any()
        }
        Shown::Check(_) => item_detail(&s, &actions, unit, tool),
    })
    .into_any()
}

/// A paragraph that says what is absent (tC4's `paneAbsent`).
fn pane_absent(text: String) -> AnyView {
    let style = crate::css! {
        "font-size": "var(--fs-ui-md)",
        "color": "var(--text-tertiary)",
        "font-style": "italic",
        "margin": "8px 0 0",
    };
    view! { <p style=style.to_style()>{text}</p> }.into_any()
}

/// tC4's `sourceCardStyle` (#762): one source text of the compare card. Its `40ch` resolves in
/// the scripture font the row sets, so every card of a row is as wide as the others.
pub(super) fn source_card() -> Css {
    crate::css! {
        "flex": "1 1 calc(40ch + 40px)",
        "min-width": 0,
        "padding": "16px 20px",
        "background": "var(--surface-card)",
        "border": "var(--stroke) solid var(--border)",
        "border-radius": "var(--radius-xl)",
        "box-shadow": "var(--shadow-card)",
    }
}

fn verse_text_style() -> Css {
    crate::css! {
        "text-align": "start",
        "font-size": "var(--fs-verse)",
        "line-height": "var(--lh-verse-md)",
        "color": "var(--text-scripture)",
        "margin": "8px 0 0",
        "max-width": "80ch",
    }
}

/// A verse's tokens with the `lit` ones in a `<mark>` (tC4's `UltPane` highlight).
fn marked(tokens: &[String], lit: &[u32], testid: &'static str) -> impl IntoView + use<> {
    let mark = crate::css! {
        "background": "var(--tc-highlight-soft)",
        "color": "var(--text-heading)",
        "border-radius": "var(--radius-xs)",
        "padding": "0 .06em",
    }
    .to_style();
    tokens
        .iter()
        .enumerate()
        .map(|(i, token)| {
            if u32::try_from(i).is_ok_and(|i| lit.contains(&i)) {
                view! { <mark data-testid=testid style=mark.clone()>{token.clone()}</mark> }
                    .into_any()
            } else {
                token.clone().into_any()
            }
        })
        .collect_view()
}

/// The original-language card (tC4's `OrigPane`).
fn original_pane(original: &OriginalView) -> AnyView {
    let testament = match original {
        OriginalView::Ready { testament, .. }
        | OriginalView::CrossFrame { testament }
        | OriginalView::Unavailable { testament }
        | OriginalView::Error { testament, .. } => *testament,
    };
    let ot = testament == Testament::Ot;
    let body = match original {
        OriginalView::Ready {
            verse: Some(OriginalVerse { tokens, lit }),
            ..
        } => {
            let mut style = verse_text_style();
            style.set(
                "font-family",
                if ot {
                    "var(--font-hebrew)"
                } else {
                    "var(--font-greek)"
                },
            );
            view! {
                <p data-testid="orig-pane-text" dir=if ot { "rtl" } else { "ltr" } lang=if ot { "hbo" } else { "el" } style=style.to_style()>
                    {marked(tokens, lit, "orig-hl")}
                </p>
            }
            .into_any()
        }
        OriginalView::Ready { verse: None, .. } => pane_absent(t(keys::check::PANE_ABSENT, &[])),
        OriginalView::CrossFrame { .. } => pane_absent(t(keys::check::CROSS_FRAME, &[])),
        OriginalView::Unavailable { .. } => pane_absent(t(keys::check::ORIG_UNAVAILABLE, &[])),
        OriginalView::Error { reason, .. } => pane_absent(reason.clone()),
    };
    let label = if ot {
        keys::check::orig_label::OT
    } else {
        keys::check::orig_label::NT
    };
    view! {
        <div data-testid="orig-pane" style=source_card().to_style()>
            <Overline tone=OverlineTone::Muted>{t(label, &[])}</Overline>
            {body}
        </div>
    }
    .into_any()
}

/// The gateway card (tC4's `UltPane`): the project's literal text with the quote's words lit.
fn gateway_pane(gateway: &GatewayView) -> AnyView {
    let pane_id = match gateway {
        GatewayView::Pane { pane, .. } => Some(pane.clone()),
        _ => None,
    };
    let body = match gateway {
        GatewayView::NoPane => pane_absent(t(keys::check::NO_GATEWAY, &[])),
        GatewayView::CrossFrame => pane_absent(t(keys::check::CROSS_FRAME, &[])),
        GatewayView::Pane { text, .. } => match text {
            GatewayText::Verse { tokens, lit } => {
                let mut style = verse_text_style();
                style.set("font-family", "var(--font-scripture)");
                let words = marked(tokens, lit, "ult-hl");
                view! { <p data-testid="ult-pane-text" dir="auto" style=style.to_style()>{words}</p> }
                    .into_any()
            }
            GatewayText::Absent => pane_absent(t(keys::check::PANE_ABSENT, &[])),
            GatewayText::NotInstalled => pane_absent(t(keys::source::NOT_INSTALLED, &[])),
            GatewayText::Missing => pane_absent(t(keys::source::UNAVAILABLE, &[])),
            GatewayText::Error { reason } => pane_absent(reason.clone()),
        },
    };
    let label = match &pane_id {
        Some(id) if id != "ult" => t(keys::check::GATEWAY_LABEL, &[("name", &id.to_uppercase())]),
        _ => t(keys::check::ULT_LABEL, &[]),
    };
    view! {
        <div data-testid="ult-pane" style=source_card().to_style()>
            <Overline tone=OverlineTone::Muted>{label}</Overline>
            {body}
        </div>
    }
    .into_any()
}

/// A notice above the compare card: tC4's red box (`invalidated-notice`, `save-error`).
fn red_notice(testid: &'static str, text: String) -> AnyView {
    let style = crate::css! {
        "font-size": "var(--fs-caption-lg)",
        "color": "var(--tc-invalid)",
        "background": "var(--tc-invalid-surface)",
        "border": "var(--stroke) solid rgba(162,19,9,.25)",
        "border-radius": "var(--radius-md)",
        "padding": "10px 12px",
        "margin": "0 0 14px",
        "line-height": "var(--lh-body)",
    };
    view! { <p data-testid=testid style=style.to_style()>{text}</p> }.into_any()
}

/// A mark that was not saved, in words the person can act on: the buttons are still there.
fn failure_text(failure: &MarkFailure) -> String {
    match failure {
        MarkFailure::LookAgain => t(keys::check::MARK_LOOK_AGAIN, &[]),
        MarkFailure::ReadOnly(reason) => t(crate::app::offer::banner(*reason).sentence, &[]),
        MarkFailure::Other(reason) => t(keys::check::MARK_NOT_SAVED, &[("reason", reason)]),
    }
}

/// tC4's `triageStyle`.
fn triage_style(active: bool, tone: &'static str) -> Css {
    let (active_bg, idle_fg, idle_border) = match tone {
        "valid" => (
            "var(--tc-valid-strong)",
            "var(--tc-valid-strong)",
            "rgba(88,193,122,.55)",
        ),
        "invalid" => (
            "var(--tc-invalid)",
            "var(--tc-invalid)",
            "var(--border-danger)",
        ),
        _ => (
            "var(--uw-black)",
            "var(--text-secondary)",
            "var(--border-strong)",
        ),
    };
    crate::css! {
        "border": format!("var(--stroke-selected) solid {}", if active { active_bg } else { idle_border }),
        "background": if active { active_bg } else { "var(--surface-card)" },
        "color": if active { "var(--text-inverse)" } else { idle_fg },
        "cursor": "pointer",
        "font-family": "var(--font-ui)",
        "font-weight": "var(--fw-heavy)",
        "font-size": "var(--fs-ui-md)",
        "padding": "11px 20px",
        "border-radius": "var(--radius-md)",
    }
}

/// tC4's `ItemNavButton` (#726): a quiet pill that reads as "move", apart from the buttons
/// that record a decision. Each sits 4 px further from them than the row gap, and its arrow
/// follows the interface direction.
fn item_nav_button(
    prev: bool,
    target: Option<u32>,
    actions: &Actions,
    testid: &'static str,
) -> AnyView {
    let rtl = interface_is_rtl();
    let arrow = if prev == rtl { "→" } else { "←" };
    let label = t(
        if prev {
            keys::check::PREV_LABEL
        } else {
            keys::check::NEXT_LABEL
        },
        &[],
    );
    let text = if prev {
        format!("{arrow} {label}")
    } else {
        format!("{label} {arrow}")
    };
    let mut style = Css::new();
    style.set(
        if prev {
            "margin-inline-end"
        } else {
            "margin-inline-start"
        },
        4,
    );
    if target.is_some() {
        style.set("color", "var(--accent)");
    }
    let actions = actions.clone();
    view! {
        <Button
            variant=ButtonVariant::Secondary
            size=Size::Sm
            title=t(if prev { keys::check::PREV_ITEM } else { keys::check::NEXT_ITEM }, &[])
            disabled=target.is_none()
            style=style
            attrs=Attrs::from([("data-testid", testid)])
            on_click=Callback::new(move |_| {
                if let Some(to) = target {
                    actions.select_check(to);
                }
            })
        >
            {text}
        </Button>
    }
    .into_any()
}

fn focus_word(root: NodeRef<html::P>, at: usize) {
    let Some(root) = root.get_untracked() else {
        return;
    };
    let selector = format!("[data-testid=\"tw-{at}\"]");
    if let Some(word) = root
        .query_selector(&selector)
        .ok()
        .flatten()
        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = word.focus();
    }
}

/// The line of draft words (tC4's `TargetWords`): one Tab stop, Left and Right between the
/// words in the order drawn, Shift selects as a word processor does, a press and a drag selects
/// whole words (#737), and the text stays selectable for copying.
fn target_words(s: &AppState, words: Vec<String>, direction: TextDir) -> AnyView {
    let selection = s.c.selection.clone();
    let nav = StoredValue::new_local(WordNav::default());
    let root: NodeRef<html::P> = NodeRef::new();
    let len = words.len();
    // The word that is the line's Tab stop follows the caret (tC4's roving tab index).
    let stop_at = RwSignal::new(0usize);
    let sync_stop = move || {
        if let Some(at) = nav.try_with_value(|nav| nav.tab_stop(len)) {
            stop_at.try_set(at);
        }
    };
    let rtl = direction == TextDir::Rtl;
    let apply = {
        let selection = selection.clone();
        move |step: WordStep| {
            if let Some(next) = step.selection {
                selection.try_set(next);
            }
            if let Some(at) = step.focus {
                focus_word(root, at);
            }
        }
    };
    // The release can land anywhere, so the window hears it; the click that follows a drag must
    // not toggle the last word back, and is over by the next task.
    let released = window_event_listener(leptos::ev::mouseup, move |_| {
        if nav.try_update_value(WordNav::release).unwrap_or(false) {
            set_timeout(
                move || {
                    nav.try_update_value(WordNav::clear_dragged);
                },
                std::time::Duration::ZERO,
            );
        }
    });
    on_cleanup(move || released.remove());
    let on_keydown = {
        let (selection, apply) = (selection.clone(), apply.clone());
        move |e: web_sys::KeyboardEvent| {
            let key = WordKey::of(&e.key(), e.shift_key());
            let step = nav
                .try_update_value(|nav| nav.key(key, len, rtl, &selection.get_untracked()))
                .unwrap_or_default();
            if step.handled {
                e.prevent_default();
            }
            apply(step);
            sync_stop();
        }
    };
    let on_blur = move |e: web_sys::FocusEvent| {
        let stays = root
            .get_untracked()
            .zip(
                e.related_target()
                    .and_then(|t| t.dyn_into::<web_sys::Node>().ok()),
            )
            .is_some_and(|(line, to)| line.contains(Some(&to)));
        if !stays {
            nav.try_update_value(WordNav::left_the_line);
        }
    };
    let line = crate::css! {
        "direction": dir_attr(direction),
        "text-align": "start",
        "font-family": "var(--font-scripture)",
        "font-size": "var(--fs-title-lg)",
        "line-height": 1.9,
        "color": "var(--text-scripture)",
        "margin": "8px 0 0",
        "max-width": "80ch",
    };
    let spans = words
        .into_iter()
        .enumerate()
        .map(|(i, word)| {
            let selected = {
                let selection = selection.clone();
                move || selection.with(|s| s.contains(&(i as u32)))
            };
            let style = {
                let selected = selected.clone();
                move || {
                    crate::css! {
                        "display": "inline-block",
                        "cursor": "pointer",
                        "border-radius": 4,
                        "padding": "0 .1em",
                        "margin-inline-end": ".22em",
                        "background": if selected() { "var(--tc-highlight-soft)" } else { "transparent" },
                    }
                    .to_style()
                }
            };
            let on_down = {
                let selection = selection.clone();
                move |e: web_sys::MouseEvent| {
                    if e.button() != 0 {
                        return;
                    }
                    // A press on a word starts a word drag, never a text selection; the browser
                    // no longer focuses the word, so it is focused here.
                    e.prevent_default();
                    focus_word(root, i);
                    nav.try_update_value(|nav| nav.press(i, &selection.get_untracked()));
                    sync_stop();
                }
            };
            let on_enter = {
                let selection = selection.clone();
                move |_: web_sys::MouseEvent| {
                    if let Some(next) = nav.try_update_value(|nav| nav.enter(i)).flatten() {
                        selection.try_set(next);
                    }
                }
            };
            let on_click = {
                let selection = selection.clone();
                move |_: web_sys::MouseEvent| {
                    let next = nav
                        .try_update_value(|nav| nav.click(i, &selection.get_untracked()))
                        .flatten();
                    if let Some(next) = next {
                        selection.try_set(next);
                    }
                    sync_stop();
                }
            };
            let stop = move || stop_at.get() == i;
            view! {
                <span
                    data-testid=format!("tw-{i}")
                    data-selected={let selected = selected.clone(); move || if selected() { "1" } else { "0" }}
                    role="button"
                    aria-pressed={let selected = selected.clone(); move || if selected() { "true" } else { "false" }}
                    tabindex=move || if stop() { "0" } else { "-1" }
                    on:mousedown=on_down
                    on:mouseenter=on_enter
                    on:click=on_click
                    style=style
                >
                    {word}
                </span>
            }
        })
        .collect_view();
    view! {
        <p
            node_ref=root
            data-testid="check-target"
            data-words="target"
            data-drafted="1"
            role="group"
            aria-label=t(keys::check::TARGET_WORDS_LABEL, &[])
            on:keydown=on_keydown
            on:focusout=on_blur
            style=line.to_style()
        >
            {spans}
        </p>
    }
    .into_any()
}

/// The check's detail, once a check is shown.
fn item_detail(s: &AppState, actions: &Actions, unit: CheckUnit, tool: Tool) -> AnyView {
    let c = s.c.clone();
    let detail = c.detail.clone();
    let session = c.session.clone();
    // Each part reads the detail through a memo of its own, so a read changes only what differs.
    let index = Memo::new({
        let detail = detail.clone();
        move |_| {
            detail.with(|d| match d {
                Some(Detail::Ready(view)) => view.index,
                _ => 0,
            })
        }
    });
    // The parts that do not move with a read (the place, the title, the note) are taken once;
    // the rest follow the detail through their own memos below.
    let read = {
        let detail = detail.clone();
        move || {
            detail.with_untracked(|d| match d {
                Some(Detail::Ready(view)) => Some((**view).clone()),
                _ => None,
            })
        }
    };
    let Some(view) = read() else {
        return ().into_any();
    };
    let total = Memo::new({
        let session = session.clone();
        move |_| {
            session.with(|s| match s {
                Some(crate::app::web::check::Session::Open(rail)) => rail.rows.len() as u32,
                _ => 0,
            })
        }
    });
    let (chapter, verse) = place_parts(&view.place);
    let story = matches!(unit, CheckUnit::Story(_));
    let unit = match unit {
        CheckUnit::Book(book) => book.english_name().to_owned(),
        CheckUnit::Story(story) => crate::app::story::story_label(story),
    };
    let helps_dir = view.helps_dir;
    let direction = s
        .project
        .with_untracked(|p| p.as_ref().map(|p| p.dir))
        .unwrap_or(TextDir::Ltr);

    let history = check_history(
        s,
        actions,
        format!(
            "{unit} {}",
            t(keys::check::REF, &[("c", &chapter), ("v", &verse)])
        ),
    );

    // ---- the header
    let header = {
        let row = crate::css! { "display": "flex", "align-items": "center", "gap": 10, "margin-bottom": 14 };
        let place = crate::css! {
            "font-size": "var(--fs-ui-sm)",
            "font-weight": "var(--fw-bold)",
            "color": "var(--text-tertiary)",
        };
        let counter = crate::css! {
            "font-size": "var(--fs-caption)",
            "letter-spacing": "var(--track-12)",
            "font-weight": "var(--fw-bold)",
            "color": "var(--text-tertiary)",
            "white-space": "nowrap",
        };
        let fill = crate::css! { "flex": 1 };
        let reference = format!(
            "{unit} {}",
            t(keys::check::REF, &[("c", &chapter), ("v", &verse)])
        );
        // A decision with two versions (a teammate's check arrived) is marked, and its review
        // opens over the screen.
        let marker = {
            let (detail, actions) = (detail.clone(), actions.clone());
            let marker = Memo::new(move |_| {
                detail.with(|d| match d {
                    Some(Detail::Ready(view)) => view.decision.marker.clone(),
                    _ => None,
                })
            });
            move || {
                marker.get().map(|marker| {
                    let actions = actions.clone();
                    let conflict = marker.conflict.clone();
                    crate::views::conflict_markers::versions_marker(
                        "check-conflict-marker",
                        &reference,
                        marker.versions,
                        Callback::new(move |()| {
                            actions.open_review(
                                tc_api::conflicts::ReviewTab::Checks,
                                Some(conflict.clone()),
                            );
                        }),
                    )
                })
            }
        };
        view! {
            <div style=row.to_style()>
                <span style=place.to_style()>
                    {unit}" "{t(keys::check::REF, &[("c", &chapter), ("v", &verse)])}
                </span>
                {marker}
                <div style=fill.to_style()></div>
                <span data-testid="check-item-counter" style=counter.to_style()>
                    {move || t(keys::check::ITEM_OF, &[("n", &(index.get() + 1).to_string()), ("total", &total.get().to_string())])}
                </span>
            </div>
        }
    };

    // ---- the quote, "What to check" and the Academy link
    let title = {
        let style = crate::css! {
            "font-family": "var(--font-scripture)",
            "font-size": "var(--fs-h1)",
            "font-weight": "var(--fw-bold)",
            "color": "var(--text-heading)",
            "margin": "0 0 14px",
        };
        view! { <h1 data-testid="check-quote" style=style.to_style()>"“"{view.title.clone()}"”"</h1> }
    };
    let note = (!view.note.is_empty()).then(|| {
        let boxed = crate::css! {
            "font-size": "var(--fs-title-sm)",
            "max-width": "calc(80ch + 36px)",
            "margin-bottom": 8,
            "border-radius": "var(--radius-lg)",
            "padding": "16px 18px",
            "background": "var(--surface-warm)",
            "border": "var(--stroke) solid var(--tc-warn-border)",
        };
        let text = crate::css! {
            "font-size": "var(--fs-title-sm)",
            "letter-spacing": "var(--track-16)",
            "line-height": "var(--lh-body)",
            "font-weight": "var(--fw-medium)",
            "color": "var(--uw-ink)",
            "margin": "7px 0 0",
            "max-width": "80ch",
        };
        let overline = crate::css! { "color": "var(--tc-warn-text-2)" };
        view! {
            <div data-testid="check-note" style=boxed.to_style()>
                <Overline style=overline>{t(keys::check::WHAT_TO_CHECK, &[])}</Overline>
                <div dir=helps_dir.map(dir_attr) data-testid="check-note-text" style=text.to_style()>
                    {expandable_note(view.note.clone())}
                </div>
            </div>
        }
    });
    let learn_more = {
        let detail = detail.clone();
        let actions = actions.clone();
        let label = move || {
            let title = detail.with(|d| match d {
                Some(Detail::Ready(view)) => view.article.title.clone(),
                _ => None,
            });
            t(
                keys::check::LEARN_MORE,
                &[(
                    "title",
                    &title.unwrap_or_else(|| t(keys::check::ACADEMY_FALLBACK, &[])),
                )],
            )
        };
        view! {
            <Button
                variant=ButtonVariant::Ghost
                style=crate::css! { "margin": "0 0 24px" }
                attrs=Attrs::from([("data-testid", "open-academy")])
                on_click=Callback::new(move |_| actions.open_academy())
            >
                {label}
            </Button>
        }
    };

    // ---- the notices
    let notices = {
        let detail = detail.clone();
        let marking = c.marking.clone();
        move || {
            let counts: Vec<u32> = detail.with(|d| match d {
                Some(Detail::Ready(view)) => view
                    .notices
                    .iter()
                    .map(|CheckNotice::Invalidated { count }| *count)
                    .collect(),
                _ => Vec::new(),
            });
            let failed = marking.with(|m| match m {
                Marking::Failed(failure) => Some(failure_text(failure)),
                _ => None,
            });
            let invalidated = counts
                .into_iter()
                .map(|n| {
                    red_notice(
                        "invalidated-notice",
                        t(keys::check::INVALIDATED_NOTICE, &[("n", &n.to_string())]),
                    )
                })
                .collect_view();
            let save_error = failed.map(|text| {
                let style = crate::css! {
                    "font-size": "var(--fs-caption-lg)",
                    "color": "var(--tc-invalid)",
                    "background": "var(--tc-invalid-surface)",
                    "border": "var(--stroke) solid rgba(162,19,9,.25)",
                    "border-radius": "var(--radius-md)",
                    "padding": "10px 12px",
                    "margin": "0 0 14px",
                    "line-height": "var(--lh-body)",
                };
                view! { <div data-testid="save-error" role="alert" style=style.to_style()>{text}</div> }
            });
            view! { {invalidated}{save_error} }
        }
    };

    // ---- the compare card (#762)
    let sources = {
        let detail = detail.clone();
        move || {
            detail.with(|d| match d {
                Some(Detail::Ready(view)) => Some(match &view.sources {
                    CheckSources::Verse { original, gateway } => view! {
                        {original_pane(original)}
                        {gateway_pane(gateway)}
                    }
                    .into_any(),
                    CheckSources::Story { pane } => super::story::story_pane(pane),
                }),
                _ => None,
            })
        }
    };
    let words = Memo::new({
        let detail = detail.clone();
        move |_| {
            detail.with(|d| match d {
                Some(Detail::Ready(view)) => view.target.words.clone(),
                _ => Vec::new(),
            })
        }
    });
    let translation = {
        let s = s.clone();
        move || {
            let words = words.get();
            if words.is_empty() {
                let style = crate::css! {
                    "font-size": "var(--fs-ui-md)",
                    "color": "var(--text-tertiary)",
                    "font-style": "italic",
                    "margin": "8px 0 0",
                    "max-width": "80ch",
                };
                let line = if story {
                    keys::check::NOT_DRAFTED_FRAME
                } else {
                    keys::check::NOT_DRAFTED
                };
                return view! {
                    <p data-testid="check-target" data-drafted="0" style=style.to_style()>
                        {t(line, &[])}
                    </p>
                }
                .into_any();
            }
            let hint = crate::css! {
                "font-size": "var(--fs-caption)",
                "letter-spacing": "var(--track-12)",
                "color": "var(--text-tertiary)",
                "margin": "8px 0 0",
            };
            view! {
                {target_words(&s, words, direction)}
                <p style=hint.to_style()>{t(keys::check::SELECT_HINT, &[])}</p>
            }
            .into_any()
        }
    };
    let compare = {
        let column = crate::css! { "display": "flex", "flex-direction": "column", "gap": 24 };
        let row = crate::css! {
            "display": "flex",
            "flex-wrap": "wrap",
            "gap": 6,
            "font-family": "var(--font-scripture)",
            "font-size": "var(--fs-verse)",
        };
        let yours = crate::css! {
            "padding": "16px 20px",
            "background": "var(--surface-accent-soft)",
            "border": "var(--stroke) solid var(--accent-ring)",
            "border-top": "3px solid var(--accent)",
            "border-radius": "var(--radius-xl)",
            "box-shadow": "var(--shadow-card)",
        };
        view! {
        <div data-testid="compare-card" style=column.to_style()>
            <div data-testid="compare-sources" style=row.to_style()>{sources}</div>
            <div data-testid="compare-translation" style=yours.to_style()>
                <Overline tone=OverlineTone::Accent>{t(keys::check::YOUR_TRANSLATION, &[])}</Overline>
                {translation}
            </div>
        </div>
    }
    .into_any()
    };

    // ---- the marking row (#726)
    let status = Memo::new({
        let detail = detail.clone();
        move |_| {
            detail.with(|d| match d {
                Some(Detail::Ready(view)) => view.decision.value.status,
                _ => None,
            })
        }
    });
    let nav = Memo::new(move |_| check_item::item_nav(index.get(), total.get()));
    let marking_row = {
        let row = crate::css! {
            "display": "flex",
            "align-items": "center",
            "gap": 10,
            "margin-top": 22,
            "flex-wrap": "wrap",
        };
        let triage = |testid: &'static str,
                      tone: &'static str,
                      which: DecisionStatus,
                      label: String,
                      on: Callback<()>| {
            view! {
                <button
                    type="button"
                    data-testid=testid
                    on:click=move |_| on.run(())
                    style=move || triage_style(status.get() == Some(which), tone).to_style()
                >
                    {label}
                </button>
            }
        };
        let (valid, invalid, todo) = {
            let (a, b, c2) = (actions.clone(), actions.clone(), actions.clone());
            let selection = c.selection.clone();
            (
                Callback::new(move |()| a.mark(check_item::valid_edit(&selection.get_untracked()))),
                Callback::new(move |()| b.mark(DecideEdit::Invalid)),
                Callback::new(move |()| c2.mark(DecideEdit::Todo)),
            )
        };
        let (prev_actions, next_actions) = (actions.clone(), actions.clone());
        view! {
            <div style=row.to_style()>
                {move || item_nav_button(true, nav.get().prev, &prev_actions, "check-prev")}
                {triage("mark-valid", "valid", DecisionStatus::Valid, t(keys::check::MARK_VALID, &[]), valid)}
                {triage("mark-invalid", "invalid", DecisionStatus::Invalid, t(keys::check::MARK_INVALID, &[]), invalid)}
                {triage("mark-todo", "todo", DecisionStatus::Todo, t(keys::check::MARK_TODO, &[]), todo)}
                {move || item_nav_button(false, nav.get().next, &next_actions, "check-next")}
            </div>
        }
    };

    // ---- the bookmark and the comment
    let marks = check_marks(s, actions, tool);

    let page = crate::css! { "padding": "28px 32px 60px" };
    view! {
        <div style=page.to_style()>
            {header}
            {title}
            {note}
            {learn_more}
            {notices}
            {compare}
            {marking_row}
            {marks}
            {history}
        </div>
    }
    .into_any()
}

/// A chip button of the marks (tC4's `railChip` with the marks' padding).
fn mark_chip(active: bool) -> Css {
    crate::css! {
        "border": "var(--stroke) solid",
        "cursor": "pointer",
        "font-family": "inherit",
        "font-weight": "var(--fw-bold)",
        "letter-spacing": "var(--track-11)",
        "white-space": "nowrap",
        "background": if active { "var(--uw-ocean)" } else { "var(--surface-card)" },
        "color": if active { "var(--text-inverse)" } else { "var(--text-secondary)" },
        "border-color": if active { "var(--uw-ocean)" } else { "var(--border-input)" },
        "font-size": "var(--fs-ui-sm)",
        "padding": "8px 14px",
        "border-radius": "var(--radius-md)",
        "display": "inline-flex",
        "align-items": "center",
        "gap": 6,
    }
}

/// tC4's `CheckMarks`: the bookmark on the left, the check comment's button right-aligned, and
/// the inline comment editor. Both write the decision's record like a triage click; neither
/// moves progress.
fn check_marks(s: &AppState, actions: &Actions, tool: Tool) -> AnyView {
    let detail = s.c.detail.clone();
    let open = s.c.comment_open.clone();
    let bookmarked = Memo::new({
        let detail = detail.clone();
        move |_| {
            detail.with(|d| match d {
                Some(Detail::Ready(view)) => view.decision.value.bookmarked,
                _ => false,
            })
        }
    });
    let written = Memo::new({
        let detail = detail.clone();
        move |_| {
            detail.with(|d| match d {
                Some(Detail::Ready(view)) => view.decision.value.comment.is_some(),
                _ => false,
            })
        }
    });
    let toggle: NodeRef<html::Button> = NodeRef::new();
    let wrap = crate::css! { "margin-top": 16 };
    let row =
        crate::css! { "display": "flex", "align-items": "center", "gap": 10, "flex-wrap": "wrap" };
    let fill = crate::css! { "flex": 1 };
    let bookmark = {
        let actions = actions.clone();
        view! {
            <button
                type="button"
                data-testid="bookmark"
                data-active=move || if bookmarked.get() { "1" } else { "0" }
                title=move || t(if bookmarked.get() { keys::check::bookmark::REMOVE_HINT } else { keys::check::bookmark::HINT }, &[])
                on:click=move |_| actions.mark(DecideEdit::Bookmark { on: !bookmarked.get_untracked() })
                style=move || mark_chip(bookmarked.get()).to_style()
            >
                <span aria-hidden="true">{t(keys::check::glyph::BOOKMARK, &[])}</span>
                {move || t(if bookmarked.get() { keys::check::bookmark::ON } else { keys::check::bookmark::OFF }, &[])}
            </button>
        }
    };
    let comment_button = {
        let (open, actions) = (open.clone(), actions.clone());
        view! {
            <button
                type="button"
                node_ref=toggle
                data-testid="comment-toggle"
                data-written=move || if written.get() { "1" } else { "0" }
                data-open={let open = open.clone(); move || if open.get() { "1" } else { "0" }}
                on:click=move |_| actions.toggle_comment()
                style={let open = open.clone(); move || mark_chip(open.get()).to_style()}
            >
                <span aria-hidden="true">{t(keys::check::glyph::COMMENT, &[])}</span>
                {let open = open.clone(); move || t(
                    if open.get() { keys::check::comment::HIDE } else if written.get() { keys::check::comment::WRITTEN } else { keys::check::comment::ADD },
                    &[],
                )}
            </button>
        }
    };
    let editor = {
        let (s, actions) = (s.clone(), actions.clone());
        move || {
            open.get()
                .then(|| comment_editor(&s, &actions, tool, toggle))
        }
    };
    view! {
        <div data-testid="check-marks" style=wrap.to_style()>
            <div style=row.to_style()>
                {bookmark}
                <div style=fill.to_style()></div>
                {comment_button}
            </div>
            {editor}
        </div>
    }
    .into_any()
}

/// The inline comment editor. Its text is a field of the save machine: it saves on Done and on
/// leaving the check while the editor is open (A-10), and a draft of it is kept in the buffer
/// between. Done and Delete give the focus back to the button that opened it (#42).
fn comment_editor(
    s: &AppState,
    actions: &Actions,
    tool: Tool,
    toggle: NodeRef<html::Button>,
) -> AnyView {
    let _ = tool;
    let Some((key, pair, scope)) = actions.comment_field() else {
        return ().into_any();
    };
    let stored = pair.value.clone();
    let saves = send_wrapper::SendWrapper::new(actions.runtime().saves().clone());
    let text = saves.open(&key, scope, pair);
    // The editor leaving saves what is typed in it (A-10), whatever closed it.
    {
        let (saves, key) = (saves.clone(), key.clone());
        on_cleanup(move || saves.close(&key));
    }
    let _ = s;
    // What is in the box now, for the Delete button; the machine's own text follows the reads.
    let draft = ArcRwSignal::new(text.get_untracked());
    {
        let (draft, text) = (draft.clone(), text.clone());
        Effect::new(move |_| {
            draft.try_set(text.get());
        });
    }
    let on_input = {
        let (actions, key, draft) = (actions.clone(), key.clone(), draft.clone());
        move |e: leptos::ev::Event| {
            let typed = event_target_value(&e);
            draft.try_set(typed.clone());
            actions.comment_typed(&key, &typed);
        }
    };
    let on_focus = {
        let (saves, key) = (saves.clone(), key.clone());
        move |e: leptos::ev::FocusEvent| {
            if let Some(element) = e
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
            {
                saves.focus(&key, &element);
            }
        }
    };
    let on_blur = {
        let (saves, key) = (saves.clone(), key.clone());
        move |_: leptos::ev::FocusEvent| saves.blur(&key)
    };
    let on_composition = {
        let (saves, key) = (saves.clone(), key.clone());
        move |_: leptos::ev::CompositionEvent| saves.composition(&key)
    };
    let close = {
        let c = actions.clone();
        move || {
            c.toggle_comment();
            // The editor unmounts with the toggle; the button is still there.
            request_animation_frame(move || {
                if let Some(button) = toggle.get_untracked() {
                    let _ = button.focus();
                }
            });
        }
    };
    // What is typed is in the box: the field's signal holds only what the machine put there.
    let area_ref: NodeRef<html::Textarea> = NodeRef::new();
    let typed_now = move || {
        area_ref
            .get_untracked()
            .map(|a| a.value())
            .unwrap_or_default()
    };
    let done = {
        let (saves, key, stored, close) =
            (saves.clone(), key.clone(), stored.clone(), close.clone());
        move |_| {
            let draft = typed_now();
            match check_item::done_text(&draft, Some(stored.as_str()).filter(|s| !s.is_empty())) {
                Some(trimmed) => {
                    saves.typed(&key, &trimmed, Source::Program);
                    saves.save(&key);
                }
                // Nothing to save: only spaces were typed, or the text is the stored one.
                None => saves.typed(&key, &stored, Source::Program),
            }
            close();
        }
    };
    let delete = {
        let (saves, key, close) = (saves.clone(), key.clone(), close.clone());
        move |_| {
            saves.typed(&key, "", Source::Program);
            saves.save(&key);
            close();
        }
    };
    let can_delete = {
        let (draft, stored) = (draft.clone(), stored.clone());
        move || stored.is_empty() && draft.get().is_empty()
    };
    let boxed = crate::css! {
        "margin-top": 12,
        "background": "var(--surface-card)",
        "border": "var(--stroke) solid var(--border)",
        "border-radius": "var(--radius-lg)",
        "padding": "14px 16px",
    };
    let area = crate::css! {
        "width": "100%",
        "box-sizing": "border-box",
        "margin-top": 8,
        "border": "var(--stroke) solid var(--border-input)",
        "border-radius": "var(--radius-md)",
        "padding": "10px 12px",
        "font-family": "inherit",
        "font-size": "var(--fs-ui-md)",
        "line-height": "var(--lh-body)",
        "color": "var(--text-body)",
        "background": "var(--surface-app)",
        "resize": "vertical",
    };
    let actions_row =
        crate::css! { "display": "flex", "align-items": "center", "gap": 8, "margin-top": 10 };
    let fill = crate::css! { "flex": 1 };
    view! {
        <div data-testid="comment-editor" style=boxed.to_style()>
            <Overline tone=OverlineTone::Accent>{t(keys::check::comment::TITLE, &[])}</Overline>
            <textarea
                node_ref=area_ref
                data-testid="comment-text"
                rows="3"
                dir="auto"
                prop:value=move || text.get()
                placeholder=t(keys::check::comment::PLACEHOLDER, &[])
                on:input=on_input
                on:focus=on_focus
                on:blur=on_blur
                on:compositionstart=on_composition
                style=area.to_style()
            ></textarea>
            <div style=actions_row.to_style()>
                <Button size=Size::Sm attrs=Attrs::from([("data-testid", "comment-done")]) on_click=Callback::new(done)>
                    {t(keys::check::comment::DONE, &[])}
                </Button>
                <div style=fill.to_style()></div>
                <Button
                    size=Size::Sm
                    variant=ButtonVariant::Ghost
                    disabled=Signal::derive(can_delete)
                    attrs=Attrs::from([("data-testid", "comment-delete")])
                    on_click=Callback::new(delete)
                >
                    {t(keys::check::comment::DELETE, &[])}
                </Button>
            </div>
        </div>
    }
    .into_any()
}

/// The Academy drawer (tC4's `AcademyDrawer` and `ArticleBody`): the article behind the check,
/// in the direction of its gateway package (#650).
pub fn academy_drawer(s: &AppState, actions: &Actions) -> impl IntoView + use<> {
    let academy = s.c.academy.clone();
    let detail = s.c.detail.clone();
    let actions = actions.clone();
    move || {
        let state = academy.get()?;
        let (title, dir, category) = detail.with(|d| match d {
            Some(Detail::Ready(view)) => (
                view.article.title.clone(),
                view.helps_dir,
                category_name(view.article.category),
            ),
            _ => (None, None, ""),
        });
        let read_title = match &state {
            Academy::Read(ArticleView::Found { title, .. }) => Some(title.clone()),
            _ => None,
        };
        let title = read_title
            .or(title)
            .unwrap_or_else(|| t(keys::check::ACADEMY_FALLBACK, &[]));
        let close = {
            let actions = actions.clone();
            Callback::new(move |()| actions.close_academy())
        };
        let article_dir = match &state {
            Academy::Read(ArticleView::Found { dir, .. }) => Some(*dir),
            _ => dir,
        };
        let eyebrow = t(keys::check::ACADEMY_EYEBROW, &[("cat", &category)]);
        let attrs = Attrs::from([("data-testid", "academy-drawer")]);
        let close_label = Signal::derive(|| t(keys::common::CLOSE, &[]));
        // A title that is the article's own reads in the article's direction.
        Some(match article_dir.map(|d| dir_attr(d).to_owned()) {
            Some(title_dir) => view! {
                <Drawer eyebrow=eyebrow title=title title_dir=title_dir width="var(--drawer-width)"
                    on_close=close close_label=close_label attrs=attrs>
                    {article_body(&state)}
                </Drawer>
            }
            .into_any(),
            None => view! {
                <Drawer eyebrow=eyebrow title=title width="var(--drawer-width)"
                    on_close=close close_label=close_label attrs=attrs>
                    {article_body(&state)}
                </Drawer>
            }
            .into_any(),
        })
    }
}

/// The article inside the drawer (tC4's `ArticleBody`): loading, a stated failure, "missing",
/// or the blocks. Never an empty panel.
fn article_body(state: &Academy) -> AnyView {
    match state {
        Academy::Loading => {
            let style = crate::css! {
                "font-size": "var(--fs-caption-lg)",
                "color": "var(--text-tertiary)",
                "margin": 0,
            };
            view! { <p style=style.to_style()>{t(keys::check::ARTICLE_LOADING, &[])}</p> }
                .into_any()
        }
        Academy::Failed(error) => {
            let error = error.clone();
            view! {
                <Callout tone=CalloutTone::Warn
                    style=crate::css! { "overflow-wrap": "anywhere" }
                    attrs=Attrs::from([("role", "alert"), ("data-testid", "article-error")])>
                    {t(keys::understand::ARTICLE_ERROR, &[])}" "{error}
                </Callout>
            }
            .into_any()
        }
        Academy::Read(ArticleView::Missing) => view! {
            <Callout tone=CalloutTone::Warn attrs=Attrs::from([("data-testid", "article-missing")])>
                {t(keys::check::ARTICLE_MISSING, &[])}
            </Callout>
        }
        .into_any(),
        Academy::Read(ArticleView::Found {
            blocks, path, dir, ..
        }) => {
            let paras = with_tables(blocks, |b| {
                use tc_api::model::BlockKind;
                let mut style = Css::new();
                match b.kind {
                    BlockKind::H => {
                        style.set("font-size", "var(--fs-caption-lg)");
                        style.set("font-weight", "var(--fw-heavy)");
                        style.set("color", "var(--uw-ocean)");
                        style.set("margin", "12px 0 4px");
                        style.set("letter-spacing", "var(--track-helps)");
                    }
                    BlockKind::Li | BlockKind::Ol => {
                        style.set("font-size", "var(--fs-body)");
                        style.set("color", "var(--text-body)");
                        style.set("line-height", 1.7);
                        style.set("margin", "0 0 4px");
                        style.set("padding-inline-start", list_indent(b));
                    }
                    _ => {
                        style.set("font-size", "var(--fs-body)");
                        style.set("color", "var(--text-body)");
                        style.set("line-height", 1.7);
                        style.set("margin", "0 0 16px");
                    }
                }
                let number = number_mark(b);
                quoted(
                    b.quote,
                    view! { <p style=style.to_style()>{number}{block_text(b)}</p> }.into_any(),
                )
            });
            let footer = crate::css! {
                "font-size": "var(--fs-micro)",
                "color": "var(--text-tertiary)",
                "font-family": "var(--font-mono)",
                "margin": "10px 0 0",
            };
            view! {
                <div data-testid="article-panel" dir=dir_attr(*dir)>
                    {paras}
                    {(!path.is_empty()).then(|| view! { <p style=footer.to_style()>{path.clone()}</p> })}
                </div>
            }
            .into_any()
        }
    }
}

/// The check's History under its marks (owner ruling H5); a decision in conflict has its review.
fn check_history(s: &AppState, actions: &Actions, reference: String) -> AnyView {
    let Some(at) = s
        .project
        .with_untracked(|p| p.as_ref().map(|p| p.at.clone()))
    else {
        return ().into_any();
    };
    let c = s.c.clone();
    let detail = c.detail.clone();
    let shown = Memo::new(move |_| {
        detail.with(|d| match d {
            Some(Detail::Ready(view)) if view.decision.marker.is_none() => {
                Some(view.decision.token.clone())
            }
            _ => None,
        })
    });
    let restoring = c.restoring.clone();
    let actions = actions.clone();
    crate::views::history::record_history(crate::views::history::RecordHistory {
        test_id: "check-history",
        at,
        shown: shown.into(),
        open: c.history_open.clone(),
        busy: Signal::derive(move || restoring.get()),
        reference,
        restore: Callback::new(move |from| actions.restore_check(from)),
    })
}
