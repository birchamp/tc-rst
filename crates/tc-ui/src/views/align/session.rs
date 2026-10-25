//! tC4's `Align` (embedded): the header, the lens bar, the reference band, the suggestions
//! row, the word bank, the cards and the marking row of one open verse, or why there is none.

use leptos::prelude::*;
use tc_api::align::{
    AlignEdit, AlignerView, Blocked, Card, Freshness, SuggestStatus, SuggestView, Testament, Word,
};
use tc_api::shown::Direction;
use tc_core::BookId;
use wasm_bindgen::JsCast;

use crate::app::align::{
    Drag, Lens, armed, bank_click, bank_key, card_key_places, drop_on_bank, drop_on_card,
    effective_lens, nav, occurrence, occurrence_name,
};
use crate::app::web::align::{Rail, Screen};
use crate::app::web::{Actions, AppState, WriteCap};
use crate::attrs::Attrs;
use crate::css::Css;
use crate::ds::{
    Button, ButtonVariant, Callout, CalloutTone, ChipState, Overline, OverlineTone, Size, WordChip,
};
use crate::i18n::{keys, t, t_dyn};
use crate::views::item_nav::{Which, item_nav_button};

/// What is on the screen where the aligner goes: loading, a failure with a Retry, why there is
/// nothing to align, or the verse (tC4's `Align` and `AlignBlocked`).
pub fn align_session(s: &AppState, actions: &Actions, book: BookId) -> impl IntoView + use<> {
    let (s, actions) = (s.clone(), actions.clone());
    // Through a Memo, so that the verse redraws when the cap changes, not on every read of the
    // open sessions (a drag in progress or the focus would go with it).
    let cap = {
        let actions = send_wrapper::SendWrapper::new(actions.clone());
        Memo::new(move |_| actions.write_cap())
    };
    let read_only = {
        let actions = send_wrapper::SendWrapper::new(actions.clone());
        Memo::new(move |_| actions.read_only_words())
    };
    let read_only = move || {
        read_only.get().map(|text| view! {
            <Callout tone=CalloutTone::Warn attrs=Attrs::from([("data-testid", "align-read-only")])>
                {text}
            </Callout>
        })
    };
    let error = s.a.edit_error.clone();
    let notice = move || {
        error.get().map(|message| view! {
        <Callout tone=CalloutTone::Warn attrs=Attrs::from([("role", "alert"), ("data-testid", "align-edit-error")])>
            {message}
        </Callout>
    })
    };
    let screen = move || {
        let screen = s.a.screen.get();
        match screen {
            Screen::Loading => {
                let style = crate::css! {
                    "font-size": "var(--fs-ui-md)",
                    "color": "var(--text-tertiary)",
                };
                view! { <p style=style.to_style()>{t(keys::align::LOADING, &[])}</p> }.into_any()
            }
            Screen::Failed(reason) => failed(&actions, reason),
            Screen::Blocked(why) => blocked(why, book),
            Screen::Ready(view) => ready(&s, &actions, cap.get(), book, *view),
        }
    };
    (read_only, notice, screen).into_any()
}

fn notice_box() -> Css {
    crate::css! {
        "border": "var(--stroke-selected) dashed var(--border-strong)",
        "border-radius": "var(--radius-xl)",
        "padding": "30px 24px",
        "text-align": "center",
    }
}

fn notice_title() -> Css {
    crate::css! {
        "font-size": "var(--fs-body)",
        "font-weight": "var(--fw-heavy)",
        "color": "var(--text-heading)",
        "margin": "0 0 8px",
    }
}

/// tC4's `align-error`: a source read that failed is stated and retryable, never the
/// "download it" prompt (D30).
fn failed(actions: &Actions, reason: String) -> AnyView {
    let body = crate::css! {
        "font-size": "var(--fs-ui)",
        "color": "var(--text-secondary)",
        "line-height": 1.6,
        "margin": "0 0 12px",
        "overflow-wrap": "anywhere",
    };
    let retry = {
        let actions = actions.clone();
        Callback::new(move |_| actions.retry_align())
    };
    view! {
        <div data-testid="align-error">
            <div style="margin-bottom: 16px;"></div>
            <div style=notice_box().to_style()>
                <p style=notice_title().to_style()>{t(keys::align::error::TITLE, &[])}</p>
                <p style=body.to_style()>{reason}</p>
                <Button size=Size::Sm variant=ButtonVariant::Outline attrs=Attrs::from([("data-testid", "align-retry")]) on_click=retry>
                    {t(keys::app::RETRY, &[])}
                </Button>
            </div>
        </div>
    }
    .into_any()
}

fn blocked_id(why: Blocked) -> &'static str {
    match why {
        Blocked::Undrafted => "undrafted",
        Blocked::Unpinned => "unpinned",
        Blocked::Missing => "missing",
        Blocked::Unreadable => "unreadable",
        Blocked::VersificationUnavailable => "versification-unavailable",
        Blocked::VersificationUnknown => "versification-unknown",
        Blocked::NoCounterpart => "no-counterpart",
        Blocked::AlignedInText => "alignedInText",
    }
}

/// tC4's `align-unavailable`: there is nothing to align here, and nothing is lost.
fn blocked(why: Blocked, book: BookId) -> AnyView {
    let id = blocked_id(why);
    let body = crate::css! {
        "font-size": "var(--fs-ui)",
        "color": "var(--text-secondary)",
        "line-height": 1.6,
        "margin": 0,
    };
    let name = book.english_name().to_owned();
    view! {
        <div data-testid="align-unavailable">
            <div style="margin-bottom: 16px;"></div>
            <div style=notice_box().to_style()>
                <p style=notice_title().to_style()>
                    {t_dyn(&format!("align.unavailable.{id}.title"), &[], None)}
                </p>
                <p style=body.to_style()>
                    {t_dyn(&format!("align.unavailable.{id}.body"), &[("book", &name)], None)}
                </p>
            </div>
        </div>
    }
    .into_any()
}

/// The raised number of a repeated word (tC4's `Occurrence`). `named`: the host's own
/// accessible name already carries the number, so the number is hidden from readers.
fn occurrence_sup(word: &Word, named: bool) -> Option<AnyView> {
    let n = occurrence(word)?;
    Some(
        view! {
            <sup aria-hidden=named.then_some("true") style="line-height: 0;">{n.to_string()}</sup>
        }
        .into_any(),
    )
}

thread_local! {
    /// The element the person acted on, as a selector and the accessible name that tells
    /// apart the chips of one repeated word.
    static FOCUS: std::cell::RefCell<Option<(String, Option<String>)>> = const { std::cell::RefCell::new(None) };
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// A click replaces the chips, and a keyboard user must not lose their place: the element they
/// acted on gets the focus back when the view is drawn again. A card is found by its test id, a
/// bank chip by its word (and its name, for a repeated word).
fn remember_focus(target: Option<web_sys::EventTarget>) {
    let Some(el) = target.and_then(|t| t.dyn_into::<web_sys::Element>().ok()) else {
        return;
    };
    let selector = match (
        el.get_attribute("data-testid"),
        el.get_attribute("data-word"),
    ) {
        (Some(id), _) => format!("[data-testid={}]", quoted(&id)),
        (None, Some(word)) => format!("[data-testid=\"align-bank\"] [data-word={}]", quoted(&word)),
        (None, None) => return,
    };
    FOCUS.with(|f| *f.borrow_mut() = Some((selector, el.get_attribute("aria-label"))));
}

/// Gives the focus back to the element `remember_focus` named, once the view is in the page.
fn restore_focus() {
    let Some((selector, label)) = FOCUS.with(|f| f.borrow_mut().take()) else {
        return;
    };
    request_animation_frame(move || {
        let Ok(found) = document().query_selector_all(&selector) else {
            return;
        };
        let target = (0..found.length())
            .filter_map(|i| found.item(i))
            .filter_map(|n| n.dyn_into::<web_sys::HtmlElement>().ok())
            .find(|el| el.get_attribute("aria-label") == label);
        if let Some(el) = target {
            let _ = el.focus();
        }
    });
}

fn with_off_look(mut css: Css, cap: Option<WriteCap>) -> Css {
    if cap.is_none() {
        css.extend(&crate::views::off_look());
    }
    css
}

/// Only a writable open lets a word or a card be picked up (6.4).
fn draggable(cap: Option<WriteCap>) -> &'static str {
    if cap.is_some() { "true" } else { "false" }
}

fn drag_over(e: web_sys::DragEvent) {
    e.prevent_default();
}

fn drag_data(e: &web_sys::DragEvent) -> Option<Drag> {
    let text = e.data_transfer()?.get_data("text/plain").ok()?;
    Drag::parse(&text)
}

fn set_drag(e: &web_sys::DragEvent, drag: &Drag) {
    if let Some(transfer) = e.data_transfer() {
        let _ = transfer.set_data("text/plain", &drag.text());
    }
}

/// tC4's `CardHeader`: the anchor label (gateway lens or original), the original under a
/// gateway word, and SPLIT on a phrase. Draggable onto another card to merge.
fn card_header(
    actions: &Actions,
    cap: Option<WriteCap>,
    card: &Card,
    lens: Lens,
    rtl_orig: bool,
) -> impl IntoView + use<> {
    // The original is right to left only in the Old Testament, which is Hebrew.
    let (orig_lang, orig_font) = if rtl_orig {
        ("hbo", "var(--font-hebrew)")
    } else {
        ("el", "var(--font-greek)")
    };
    let index = card.index;
    let gateway = (lens == Lens::Gateway)
        .then(|| card.gateway.clone())
        .flatten();
    let using_gateway = gateway.is_some();
    let top = card.top.clone();
    let orig_label = move || {
        top.iter()
            .enumerate()
            .map(|(i, w)| {
                view! {
                    {(i > 0).then_some(" ")}{w.word.clone()}{occurrence_sup(w, false)}
                }
            })
            .collect::<Vec<_>>()
    };
    let header = crate::css! {
        "padding": "7px 10px 6px",
        "background": "var(--surface-app)",
        "border-bottom": "var(--stroke-hair) solid var(--border)",
        "text-align": "center",
        "cursor": if cap.is_some() { "grab" } else { "default" },
        "display": "flex",
        "flex-direction": "column",
        "align-items": "center",
        "gap": 2,
    };
    let label = crate::css! {
        "font-family": if using_gateway { "var(--font-scripture)" } else { orig_font },
        "font-size": "var(--fs-title)",
        "letter-spacing": "var(--track-17)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--text-heading)",
        "white-space": "nowrap",
    };
    let sub = crate::css! {
        "font-family": orig_font,
        "font-size": "var(--fs-meta)",
        "letter-spacing": "var(--track-11-5)",
        "color": "var(--text-tertiary)",
        "white-space": "nowrap",
    };
    let split = (card.top.len() > 1).then(|| {
        let actions = actions.clone();
        let testid = format!("align-split-{index}");
        view! {
            <Button
                variant=ButtonVariant::Secondary
                size=Size::Sm
                attrs=Attrs::from([("data-testid", testid.as_str())])
                title=Some(t(keys::align::SPLIT_HINT, &[]))
                disabled=cap.is_none()
                on_click=Callback::new(move |e: web_sys::MouseEvent| {
                    e.stop_propagation();
                    if let Some(cap) = cap {
                        actions.align_edit(cap, AlignEdit::Split { card: index });
                    }
                })
            >
                {t(keys::align::SPLIT, &[])}
            </Button>
        }
    });
    let first_line = if let Some(text) = gateway.clone() {
        view! { <span style=label.to_style()>{text}</span> }.into_any()
    } else {
        view! { <span lang=orig_lang style=label.to_style()>{orig_label()}</span> }.into_any()
    };
    view! {
        <div
            draggable=draggable(cap)
            title=cap.map(|_| t(keys::align::MERGE_HINT, &[]))
            style=header.to_style()
            on:dragstart=move |e: web_sys::DragEvent| {
                e.stop_propagation();
                set_drag(&e, &Drag::Card { from: index });
            }
        >
            <span style="display: flex; align-items: center; gap: 7px;">
                {first_line}
                {split}
            </span>
            {using_gateway.then(|| view! {
                <span lang=orig_lang title=t(keys::align::SUB_HINT, &[]) style=sub.to_style()>{orig_label()}</span>
            })}
        </div>
    }
}

/// tC4's `WordCard`: the three states of the design: a drop hovering over it, a word armed for
/// placement (any card may take it), and at rest.
fn word_card(
    actions: &Actions,
    cap: Option<WriteCap>,
    card: &Card,
    armed_now: bool,
    lens: Lens,
    rtl_orig: bool,
    target_dir: Direction,
) -> impl IntoView + use<> {
    let index = card.index;
    let armed_now = armed_now && cap.is_some();
    let over = RwSignal::new(false);
    let style = move || {
        let mut css = crate::css! {
            "border": "var(--stroke-selected) solid",
            "border-radius": "var(--radius-md)",
            "display": "flex",
            "flex-direction": "column",
            "min-width": 78,
            "overflow": "hidden",
        };
        css.extend(&if over.get() {
            crate::css! {
                "border-color": "var(--accent)",
                "background": "var(--surface-accent-soft)",
            }
        } else if armed_now {
            crate::css! {
                "border-color": "rgba(49,173,227,.55)",
                "background": "var(--surface-card)",
                "cursor": "pointer",
            }
        } else {
            crate::css! {
                "border-color": "var(--border-input)",
                "background": "var(--surface-card)",
            }
        });
        css.to_style()
    };
    let body = crate::css! {
        "padding": 6,
        "display": "flex",
        "flex-wrap": "wrap",
        "gap": 4,
        "flex": 1,
        "min-height": 38,
        "align-items": "flex-start",
        "align-content": "flex-start",
    };
    let placed_style = crate::css! {
        "font-family": "var(--font-scripture)",
        "font-size": "var(--fs-ui-md)",
        "color": "var(--text-scripture)",
        "background": "var(--surface-card)",
        "border": "var(--stroke) solid var(--border-input)",
        "border-radius": "var(--radius-chip)",
        "padding": "3px 9px",
        "cursor": "pointer",
        "white-space": "nowrap",
        "box-shadow": "0 1px 2px rgba(1,66,99,.08)",
    };
    let placed_style = with_off_look(placed_style, cap);
    let placed: Vec<AnyView> = card
        .placed
        .iter()
        .map(|w| {
            let (word, from) = (w.clone(), index);
            let drag_word = w.clone();
            let click_actions = actions.clone();
            view! {
                <button
                    type="button"
                    draggable=draggable(cap)
                    disabled=cap.is_none()
                    title=cap.map(|_| t(keys::align::CLICK_TO_UNALIGN, &[]))
                    aria-label=occurrence_name(w)
                    data-word=w.word.clone()
                    style=placed_style.to_style()
                    on:dragstart=move |e: web_sys::DragEvent| {
                        set_drag(&e, &Drag::Placed { from, word: drag_word.clone() });
                    }
                    on:click=move |e: web_sys::MouseEvent| {
                        e.stop_propagation();
                        if let Some(cap) = cap {
                            click_actions.align_edit(cap, AlignEdit::Unplace { card: from, word: word.clone() });
                        }
                    }
                >
                    {w.word.clone()}{occurrence_sup(w, true)}
                </button>
            }
            .into_any()
        })
        .collect();
    let suggested: Vec<AnyView> = card
        .suggested
        .iter()
        .map(|w| {
            let w = w.clone();
            let (word, actions) = (w.clone(), actions.clone());
            let (testid, title, name) = (
                format!("align-suggested-{index}"),
                cap.map(|_| t(keys::align::suggest::CONFIRM, &[])).unwrap_or_default(),
                occurrence_name(&w).unwrap_or_default(),
            );
            view! {
                <WordChip
                    state=ChipState::Suggested
                    attrs=Attrs::from([
                        ("data-testid", testid.as_str()),
                        ("title", title.as_str()),
                        ("aria-label", name.as_str()),
                        ("data-word", w.word.as_str()),
                    ])
                >
                    <span
                        on:click=move |e: web_sys::MouseEvent| {
                            e.stop_propagation();
                            if let Some(cap) = cap {
                                actions.align_edit(cap, AlignEdit::Accept { card: index, word: word.clone() });
                            }
                        }
                    >
                        {w.word.clone()}{occurrence_sup(&w, false)}
                        <span aria-hidden="true" style="font-weight: var(--fw-heavy);">
                            {t(keys::align::suggest::CONFIRM_MARK, &[])}
                        </span>
                    </span>
                </WordChip>
            }
            .into_any()
        })
        .collect();
    let empty = (card.placed.is_empty() && card.suggested.is_empty()).then(|| {
        let s = crate::css! {
            "flex": 1,
            "border": "var(--stroke-selected) dashed var(--border-input)",
            "border-radius": "var(--radius-chip)",
            "min-height": 26,
        };
        view! { <span style=s.to_style()></span> }
    });
    let header = card_header(actions, cap, card, lens, rtl_orig);
    let (click, key, drop_actions) = (actions.clone(), actions.clone(), actions.clone());
    view! {
        <div
            data-testid=format!("align-card-{index}")
            data-count=card.placed.len().to_string()
            tabindex=if armed_now { "0" } else { "-1" }
            style=style
            on:click=move |e: web_sys::MouseEvent| {
                if let Some(cap) = cap.filter(|_| armed_now) {
                    remember_focus(e.current_target());
                    click.align_edit(cap, AlignEdit::PlaceSelection { card: index });
                }
            }
            on:keydown=move |e: web_sys::KeyboardEvent| {
                if let Some(cap) = cap.filter(|_| card_key_places(&e.key(), armed_now)) {
                    e.prevent_default();
                    remember_focus(e.current_target());
                    key.align_edit(cap, AlignEdit::PlaceSelection { card: index });
                }
            }
            on:dragenter=move |_| over.set(true)
            // Drag events bubble from the header and the placed-word chips: only a leave that
            // actually exits the card clears the highlight.
            on:dragleave=move |e: web_sys::DragEvent| {
                let inside = e
                    .current_target()
                    .and_then(|t| t.dyn_into::<web_sys::Node>().ok())
                    .zip(e.related_target().and_then(|t| t.dyn_into::<web_sys::Node>().ok()))
                    .is_some_and(|(card, related)| card.contains(Some(&related)));
                if !inside {
                    over.set(false);
                }
            }
            on:dragover=drag_over
            on:drop=move |e: web_sys::DragEvent| {
                e.prevent_default();
                over.set(false);
                if let (Some(cap), Some(drag)) = (cap, drag_data(&e)) {
                    drop_actions.align_edit(cap, drop_on_card(index, drag));
                }
            }
        >
            {header}
            // The placed words are the translation's: they read in its direction, whatever
            // the original's (tC4 #830).
            <div style=body.to_style() dir=dir_name(target_dir)>{placed}{suggested}{empty}</div>
        </div>
    }
}

/// The lens bar (tC4's `ModeBar`, mockup L1075–1081).
fn mode_bar(
    s: &AppState,
    shown: Lens,
    gateway_available: bool,
    testament: Testament,
) -> impl IntoView + use<> {
    let bar = crate::css! {
        "padding": "12px 20px",
        "border-bottom": "var(--stroke-hair) solid var(--border-hair)",
        "background": "var(--surface-app)",
        "display": "flex",
        "align-items": "center",
        "gap": 12,
        "flex-wrap": "wrap",
    };
    let pills = crate::css! {
        "display": "flex",
        "gap": 3,
        "padding": 3,
        "background": "var(--surface-muted)",
        "border-radius": "var(--radius-pill)",
    };
    let chip = |on: bool| {
        crate::css! {
            "border": 0,
            "cursor": "pointer",
            "font-family": "inherit",
            "font-weight": "var(--fw-heavy)",
            "font-size": "var(--fs-caption)",
            "letter-spacing": "var(--track-12)",
            "padding": "6px 13px",
            "border-radius": "var(--radius-pill)",
            "background": if on { "var(--accent)" } else { "transparent" },
            "color": if on { "var(--text-inverse)" } else { "var(--text-heading)" },
            "box-shadow": if on { "var(--shadow-raised)" } else { "none" },
        }
    };
    let gw = {
        let mut css = chip(shown == Lens::Gateway);
        css.set("opacity", if gateway_available { 1.0 } else { 0.5 });
        css.set(
            "cursor",
            if gateway_available {
                "pointer"
            } else {
                "default"
            },
        );
        css
    };
    let orig = chip(shown == Lens::Original);
    let hint = crate::css! {
        "font-size": "var(--fs-caption)",
        "letter-spacing": "var(--track-12)",
        "color": "var(--text-tertiary)",
        "line-height": 1.4,
        "flex": 1,
        "min-width": 200,
    };
    let orig_key = match testament {
        Testament::Nt => keys::align::mode_orig::NT,
        Testament::Ot => keys::align::mode_orig::OT,
    };
    let hint_key = if !gateway_available {
        keys::align::MODE_GW_UNAVAILABLE
    } else if shown == Lens::Gateway {
        keys::align::MODE_GW_HINT
    } else {
        keys::align::MODE_ORIG_HINT
    };
    let (a1, a2) = (s.a.clone(), s.a.clone());
    view! {
        <div style=bar.to_style()>
            <div style=pills.to_style()>
                <button
                    type="button"
                    data-trim="cap"
                    data-testid="align-mode-gw"
                    disabled=!gateway_available
                    title=t(keys::align::MODE_GW_HINT, &[])
                    style=gw.to_style()
                    on:click=move |_| a1.lens.set(Lens::Gateway)
                >
                    {t(keys::align::MODE_GW, &[])}
                </button>
                <button
                    type="button"
                    data-trim="cap"
                    data-testid="align-mode-orig"
                    title=t(keys::align::MODE_ORIG_HINT, &[])
                    style=orig.to_style()
                    on:click=move |_| a2.lens.set(Lens::Original)
                >
                    {t(orig_key, &[])}
                </button>
            </div>
            <span style=hint.to_style()>{t(hint_key, &[])}</span>
        </div>
    }
}

/// The anchor language's full verse (mockup's reference band).
fn ref_band(view: &AlignerView, shown: Lens) -> Option<AnyView> {
    let text = match (shown, &view.gateway) {
        (Lens::Gateway, Some(gateway)) => gateway.text.clone(),
        _ => view.original.clone(),
    };
    if text.is_empty() {
        return None;
    }
    let outer = crate::css! {
        "padding": "13px 20px",
        "border-bottom": "var(--stroke-hair) solid var(--border-hair)",
    };
    // The original of the Old Testament is Hebrew, of the New Testament Greek.
    let (orig_lang, orig_font) = match view.testament {
        Testament::Ot => ("hbo", "var(--font-hebrew)"),
        Testament::Nt => ("el", "var(--font-greek)"),
    };
    let lang = (shown == Lens::Original).then_some(orig_lang);
    let p = crate::css! {
        "text-align": "start",
        "font-family": if shown == Lens::Original { orig_font } else { "var(--font-scripture)" },
        "font-size": "var(--fs-verse)",
        "line-height": "var(--lh-verse)",
        "color": "var(--text-secondary)",
        "margin": "6px 0 0",
    };
    let label = match (shown, view.testament) {
        (Lens::Gateway, _) => keys::check::ULT_LABEL,
        (Lens::Original, Testament::Nt) => keys::check::orig_label::NT,
        (Lens::Original, Testament::Ot) => keys::check::orig_label::OT,
    };
    let dir = if shown == Lens::Original {
        dir_name(view.orig_dir)
    } else {
        "ltr"
    };
    Some(
        view! {
            <div data-testid="align-ref-text" style=outer.to_style()>
                <Overline>{t(label, &[])}</Overline>
                <p dir=dir lang=lang style=p.to_style()>{text}</p>
            </div>
        }
        .into_any(),
    )
}

fn dir_name(dir: Direction) -> &'static str {
    match dir {
        Direction::Ltr => "ltr",
        Direction::Rtl => "rtl",
    }
}

/// The suggestions row above the word bank (tC4's `SuggestionsRow`, D72).
fn suggestions_row(
    s: &AppState,
    actions: &Actions,
    cap: Option<WriteCap>,
    suggest: &SuggestView,
) -> impl IntoView + use<> {
    let row = crate::css! {
        "display": "flex",
        "align-items": "center",
        "gap": 10,
        "flex-wrap": "wrap",
        "padding": "12px 20px",
        "border-bottom": "var(--stroke-hair) solid var(--border-hair)",
        "background": "var(--surface-card)",
    };
    let label = crate::css! {
        "display": "inline-flex",
        "align-items": "center",
        "gap": 6,
        "font-size": "var(--fs-ui-sm)",
        "color": "var(--text-secondary)",
        "cursor": "pointer",
    };
    let status_style = crate::css! {
        "font-size": "var(--fs-caption-lg)",
        "color": "var(--text-tertiary)",
        "flex": 1,
        "min-width": 160,
    };
    let (status_id, text) = match (&suggest.status, suggest.on) {
        (_, false) | (SuggestStatus::Off, _) => ("off", t(keys::align::suggest::OFF, &[])),
        (SuggestStatus::Reading, true) => ("reading", t(keys::align::suggest::READING, &[])),
        (SuggestStatus::None, true) => ("none", t(keys::align::suggest::NONE, &[])),
        (SuggestStatus::Ready { verses, boosted }, true) => {
            let n = verses.to_string();
            let text = if *verses == 1 {
                t(keys::align::suggest::memory::ONE, &[])
            } else if *boosted {
                t(keys::align::suggest::READY, &[("n", &n)])
            } else {
                t(keys::align::suggest::MEMORY, &[("n", &n)])
            };
            ("ready", text)
        }
        (SuggestStatus::Error { reason }, true) => (
            "error",
            t(keys::align::suggest::ERROR, &[("error", reason)]),
        ),
    };
    let ready = suggest.on && matches!(suggest.status, SuggestStatus::Ready { .. });
    let standing = suggest.standing > 0;
    let nothing =
        (suggest.asked && !standing).then(|| format!(" {}", t(keys::align::suggest::NOTHING, &[])));
    let on = suggest.on;
    let a = actions.clone();
    let buttons = on.then(|| {
        let (accept, reject, reset, ask) = (
            actions.clone(),
            actions.clone(),
            actions.clone(),
            actions.clone(),
        );
        let (asked, suggesting, asking) = (suggest.asked, suggest.suggesting, s.a.asking.clone());
        view! {
            <span style="display: inline-flex; gap: 6px;">
                <Button size=Size::Sm variant=ButtonVariant::Secondary attrs=Attrs::from([("data-testid", "align-suggest")]) disabled=Signal::derive(move || !ready || suggesting || asking.get())
                    on_click=Callback::new(move |_| ask.suggest_align())>
                    {t(keys::align::suggest::SUGGEST, &[])}
                </Button>
                <Button size=Size::Sm attrs=Attrs::from([("data-testid", "align-suggest-accept-all")]) disabled=!standing || cap.is_none()
                    on_click=Callback::new(move |_| if let Some(cap) = cap { accept.align_edit(cap, AlignEdit::AcceptAll) })>
                    {t(keys::align::suggest::ACCEPT_ALL, &[])}
                </Button>
                <Button size=Size::Sm variant=ButtonVariant::Outline attrs=Attrs::from([("data-testid", "align-suggest-reject-all")]) disabled=!standing || cap.is_none()
                    on_click=Callback::new(move |_| if let Some(cap) = cap { reject.align_edit(cap, AlignEdit::RejectAll) })>
                    {t(keys::align::suggest::REJECT_ALL, &[])}
                </Button>
                <Button size=Size::Sm variant=ButtonVariant::Ghost attrs=Attrs::from([("data-testid", "align-suggest-reset")]) disabled=!asked || cap.is_none()
                    on_click=Callback::new(move |_| if let Some(cap) = cap { reset.align_edit(cap, AlignEdit::RejectAll) })>
                    {t(keys::align::suggest::RESET, &[])}
                </Button>
            </span>
        }
    });
    view! {
        <div data-testid="align-suggestions" data-status=status_id style=row.to_style()>
            <Overline tone=OverlineTone::Accent>{t(keys::align::suggest::TITLE, &[])}</Overline>
            <label style=label.to_style()>
                <input
                    type="checkbox"
                    data-testid="align-suggest-switch"
                    prop:checked=on
                    on:change=move |e| a.set_align_suggestions(event_target_checked(&e))
                />
                {t(keys::align::suggest::SWITCH, &[])}
            </label>
            <span data-testid="align-suggest-status" style=status_style.to_style()>
                {text}{nothing}
            </span>
            {buttons}
        </div>
    }
}

/// The word bank: the target words not yet placed, on the paper tint. A word with a standing
/// suggestion shows on its card as a dashed chip instead (tC4 #1).
fn bank(actions: &Actions, cap: Option<WriteCap>, view: &AlignerView) -> impl IntoView + use<> {
    let armed_now = armed(view);
    let outer = crate::css! {
        "padding": "14px 20px 16px",
        "border-bottom": "var(--stroke-hair) solid var(--border-hair)",
        "background": "var(--surface-app)",
    };
    let head = crate::css! {
        "display": "flex",
        "align-items": "center",
        "gap": 10,
        "margin-bottom": 10,
    };
    let progress = crate::css! {
        "font-size": "var(--fs-label)",
        "letter-spacing": "var(--track-11)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--text-tertiary)",
    };
    let (placed, total) = (view.placed.to_string(), view.total.to_string());
    let selected: Vec<u32> = match &view.selection {
        tc_api::align::BankSelection::Armed(chosen) => chosen.clone(),
        tc_api::align::BankSelection::None => Vec::new(),
    };
    let chips: Vec<AnyView> = view
        .bank
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let on = u32::try_from(i).is_ok_and(|i| selected.contains(&i));
            let style = {
                let mut css = crate::css! {
                    "font-family": "var(--font-scripture)",
                    "font-size": "var(--fs-body)",
                    "border-radius": "var(--radius-sm)",
                    "padding": "5px 11px",
                    "cursor": "pointer",
                    "border": "var(--stroke) solid var(--accent)",
                };
                css.extend(&if on {
                    crate::css! {
                        "background": "var(--accent)",
                        "color": "var(--text-inverse)",
                        "box-shadow": "var(--shadow-focus)",
                    }
                } else {
                    crate::css! {
                        "background": "var(--surface-card)",
                        "color": "var(--text-heading)",
                        "box-shadow": "var(--shadow-chip)",
                    }
                });
                with_off_look(css, cap)
            };
            let word = w.clone();
            let (click, key, drag) = (actions.clone(), actions.clone(), view.clone());
            let (click_word, key_word) = (word.clone(), word.clone());
            view! {
                <button
                    type="button"
                    draggable=draggable(cap)
                    disabled=cap.is_none()
                    title=cap.map(|_| t(if armed_now { keys::align::HINT_ARMED } else { keys::align::HINT_IDLE }, &[]))
                    data-armed=if on { "1" } else { "0" }
                    aria-label=occurrence_name(w)
                    data-word=w.word.clone()
                    style=style.to_style()
                    on:dragstart=move |e: web_sys::DragEvent| {
                        set_drag(&e, &Drag::Bank { words: crate::app::align::dragged_words(&drag, i) });
                    }
                    on:click=move |e: web_sys::MouseEvent| {
                        let Some(cap) = cap else { return };
                        remember_focus(e.current_target());
                        click.align_edit(cap, AlignEdit::ClickBank {
                            word: click_word.clone(),
                            click: bank_click(e.ctrl_key() || e.meta_key(), e.shift_key()),
                        });
                    }
                    // The click a key fires carries no modifier, so Ctrl, Cmd or Shift with
                    // Enter or Space selects from the keyboard here.
                    on:keydown=move |e: web_sys::KeyboardEvent| {
                        if let (Some(cap), Some(click)) = (cap, bank_key(&e.key(), e.ctrl_key() || e.meta_key(), e.shift_key())) {
                            e.prevent_default();
                            remember_focus(e.current_target());
                            key.align_edit(cap, AlignEdit::ClickBank { word: key_word.clone(), click });
                        }
                    }
                >
                    {w.word.clone()}{occurrence_sup(w, true)}
                </button>
            }
            .into_any()
        })
        .collect();
    let empty_text = crate::css! {
        "font-size": "var(--fs-ui-sm)",
        "color": "var(--text-tertiary)",
        "font-style": "italic",
        "margin": 0,
    };
    let body = if view.bank.is_empty() && view.suggest.standing == 0 {
        view! { <p data-testid="align-bank-empty" style=empty_text.to_style()>{t(keys::align::BANK_EMPTY, &[])}</p> }
            .into_any()
    } else {
        view! {
            <div data-testid="align-bank" style="display: flex; flex-wrap: wrap; gap: 7px;" dir=dir_name(view.target_dir)>
                {chips}
            </div>
        }
        .into_any()
    };
    let drop_actions = actions.clone();
    view! {
        <div
            on:dragover=drag_over
            on:drop=move |e: web_sys::DragEvent| {
                e.prevent_default();
                if let (Some(cap), Some(edit)) = (cap, drag_data(&e).and_then(drop_on_bank)) {
                    drop_actions.align_edit(cap, edit);
                }
            }
            style=outer.to_style()
        >
            <div style=head.to_style()>
                <Overline tone=OverlineTone::Accent>{t(keys::align::BANK, &[])}</Overline>
                <div style="flex: 1;"></div>
                <span data-testid="align-progress" style=progress.to_style()>
                    {t(keys::align::PLACED, &[("placed", &placed), ("total", &total)])}
                </span>
            </div>
            {body}
        </div>
    }
}

/// tC4's `MarkValidBar` (#271, D73, #726): Previous and Next sit either side of Mark valid.
fn mark_valid_bar(
    s: &AppState,
    actions: &Actions,
    cap: Option<WriteCap>,
    view: &AlignerView,
) -> impl IntoView + use<> {
    let items = s.a.rail.with_untracked(|rail| match rail {
        Rail::Ready(rows) => Some(rows.items.clone()),
        Rail::Loading | Rail::Failed(_) => None,
    });
    let neighbours = nav(items.as_deref().map_or(&[], Vec::as_slice), view.place);
    let done = view.done;
    let row = crate::css! {
        "display": "flex",
        "align-items": "center",
        "gap": 10,
        "margin-top": 22,
        "flex-wrap": "wrap",
    };
    let refused = view.mark_refused.then(|| {
        let s = crate::css! {
            "font-size": "var(--fs-caption-lg)",
            "color": "var(--tc-warn-text)",
        };
        view! {
            <span data-testid="align-mark-valid-refused" style=s.to_style()>
                {t(keys::align::MARK_VALID_REFUSED, &[])}
            </span>
        }
    });
    let button = crate::css! {
        "border": if done { "var(--stroke-selected) solid var(--tc-valid-strong)" } else { "var(--stroke-selected) solid rgba(88,193,122,.55)" },
        "background": if done { "var(--tc-valid-strong)" } else { "var(--surface-card)" },
        "color": if done { "var(--text-inverse)" } else { "var(--tc-valid-strong)" },
        "cursor": "pointer",
        "font-family": "var(--font-ui)",
        "font-weight": "var(--fw-heavy)",
        "font-size": "var(--fs-ui-md)",
        "padding": "11px 20px",
        "border-radius": "var(--radius-md)",
    };
    let button = with_off_look(button, cap);
    let go = |place: Option<tc_api::align::AlignPlace>| {
        let actions = actions.clone();
        Callback::new(move |()| {
            if let Some(place) = place {
                actions.set_align_verse(place);
            }
        })
    };
    let mark = actions.clone();
    view! {
        <div style=row.to_style()>
            {refused}
            {item_nav_button(Which::Prev, neighbours.prev.is_some(), "align-prev", go(neighbours.prev))}
            <button
                type="button"
                data-testid="align-mark-valid"
                data-active=if done { "1" } else { "0" }
                title=cap.map(|_| t(keys::align::MARK_VALID_HINT, &[]))
                style=button.to_style()
                disabled=cap.is_none()
                on:click=move |_| if let Some(cap) = cap { mark.align_edit(cap, AlignEdit::MarkValid) }
            >
                {t(keys::align::MARK_VALID, &[])}
            </button>
            {item_nav_button(Which::Next, neighbours.next.is_some(), "align-next", go(neighbours.next))}
        </div>
    }
}

/// The header: the reference, and "Item n of total" (tC4's `AlignHeader`).
fn header(
    s: &AppState,
    actions: &Actions,
    view: &AlignerView,
    book: BookId,
) -> impl IntoView + use<> {
    let items = s.a.rail.with_untracked(|rail| match rail {
        Rail::Ready(rows) => Some(rows.items.clone()),
        Rail::Loading | Rail::Failed(_) => None,
    });
    let neighbours = nav(items.as_deref().map_or(&[], Vec::as_slice), view.place);
    let name = book.english_name().to_owned();
    let label = t(
        keys::align::REF,
        &[("book", &name), ("ref", &view.place.label())],
    );
    let row = crate::css! {
        "display": "flex",
        "align-items": "center",
        "gap": 10,
        "margin-bottom": 14,
    };
    let small = crate::css! {
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
    let h1 = crate::css! {
        "font-family": "var(--font-scripture)",
        "font-size": "var(--fs-h1)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--text-heading)",
        "margin": "0 0 14px",
    };
    let item_of = neighbours.at.map(|at| {
        t(
            keys::check::ITEM_OF,
            &[
                ("n", &(at + 1).to_string()),
                ("total", &neighbours.drafted.to_string()),
            ],
        )
    });
    let marker = view.marker.clone().map(|marker| {
        let actions = actions.clone();
        let conflict = marker.conflict.clone();
        crate::views::conflict_markers::versions_marker(
            "align-conflict-marker",
            &label,
            marker.versions,
            Callback::new(move |()| {
                actions.open_review(
                    tc_api::conflicts::ReviewTab::Alignments,
                    Some(conflict.clone()),
                );
            }),
        )
    });
    view! {
        <div style=row.to_style()>
            <span style=small.to_style()>{label.clone()}</span>
            {marker}
            <div style="flex: 1;"></div>
            {item_of.map(|text| view! { <span data-testid="align-item-counter" style=counter.to_style()>{text}</span> })}
        </div>
        <h1 style=h1.to_style()>"“"{label}"”"</h1>
    }
}

/// "How alignment works" (tC4's `HowToggle`): the mockup's info toggle.
fn how_toggle(s: &AppState) -> impl IntoView + use<> {
    let how = s.a.how.clone();
    let button = crate::css! {
        "border": 0,
        "background": "transparent",
        "cursor": "pointer",
        "font-family": "inherit",
        "font-weight": 700,
        "font-size": "var(--fs-caption-lg)",
        "letter-spacing": "var(--track-12-5)",
        "color": "var(--text-tertiary)",
        "padding": 0,
        "margin": "0 0 14px",
        "display": "inline-flex",
        "align-items": "center",
        "gap": 6,
    };
    let circle = crate::css! {
        "width": 16,
        "height": 16,
        "border-radius": "var(--radius-pill)",
        "border": "var(--stroke-selected) solid currentColor",
        "display": "inline-flex",
        "align-items": "center",
        "justify-content": "center",
        "font-size": "var(--fs-badge)",
        "font-weight": 800,
        "flex": "none",
    };
    let body = crate::css! {
        "font-size": "var(--fs-ui)",
        "line-height": 1.6,
        "color": "var(--text-secondary)",
        "margin": "-6px 0 14px",
        "max-width": "62ch",
    };
    let (toggle, open) = (how.clone(), how.clone());
    view! {
        <button
            type="button"
            data-testid="align-how"
            aria-expanded=move || how.get().to_string()
            style=button.to_style()
            on:click=move |_| toggle.update(|v| *v = !*v)
        >
            <span aria-hidden="true" style=circle.to_style()>"ℹ"</span>
            {t(keys::align::HOW_TITLE, &[])}
        </button>
        {move || open.get().then(|| view! { <p style=body.to_style()>{t(keys::align::HOW_BODY, &[])}</p> })}
    }
}

/// The verse itself, with its lens and bank and cards and marking row.
fn ready(
    s: &AppState,
    actions: &Actions,
    cap: Option<WriteCap>,
    book: BookId,
    view: AlignerView,
) -> AnyView {
    restore_focus();
    let lens = effective_lens(s.a.lens.get(), view.gateway.is_some());
    let stale = (view.freshness == Freshness::Stale).then(|| {
        let style = crate::css! {
            "font-size": "var(--fs-caption-lg)",
            "color": "var(--tc-warn-text)",
            "background": "var(--surface-warm)",
            "border": "var(--stroke) solid var(--tc-warn-border)",
            "border-radius": "var(--radius-md)",
            "padding": "10px 12px",
            "margin": "0 0 14px",
            "line-height": 1.5,
        };
        view! { <p data-testid="align-stale" style=style.to_style()>{t(keys::align::STALE, &[])}</p> }
    });
    let panel = crate::css! {
        "background": "var(--surface-card)",
        "border": "var(--stroke) solid var(--border)",
        "border-radius": "var(--radius-xl)",
        "overflow": "hidden",
        "box-shadow": "var(--shadow-card)",
    };
    let cards_style = crate::css! {
        "display": "flex",
        "flex-wrap": "wrap",
        "gap": 8,
        "padding": "16px 20px 20px",
    };
    let armed_now = armed(&view);
    let cards: Vec<AnyView> = view
        .cards
        .iter()
        .map(|card| {
            word_card(
                actions,
                cap,
                card,
                armed_now,
                lens,
                view.orig_dir == Direction::Rtl,
                view.target_dir,
            )
            .into_any()
        })
        .collect();
    // Links a save or an open sent back to the word bank are listed, never silently gone
    // (owner answer Q30 (a), tC4 #825).
    let dropped = (!view.dropped.is_empty()).then(|| {
        let words = view
            .dropped
            .iter()
            .map(|d| d.word.word.clone())
            .collect::<Vec<_>>()
            .join(", ");
        let style = crate::css! {
            "font-size": "var(--fs-caption-lg)",
            "color": "var(--tc-warn-text)",
            "background": "var(--surface-warm)",
            "border": "var(--stroke) solid var(--tc-warn-border)",
            "border-radius": "var(--radius-md)",
            "padding": "10px 12px",
            "margin": "0 0 14px",
            "line-height": 1.5,
        };
        view! { <p data-testid="align-dropped" style=style.to_style()>{t(keys::align::DROPPED_LINKS, &[("words", &words)])}</p> }
    });
    let session = view! {
        <div data-testid="align-session">
            {header(s, actions, &view, book)}
            {how_toggle(s)}
            {stale}
            {dropped}
            <div style=panel.to_style()>
                {mode_bar(s, lens, view.gateway.is_some(), view.testament)}
                {ref_band(&view, lens)}
                {suggestions_row(s, actions, cap, &view.suggest)}
                {bank(actions, cap, &view)}
                <div style=cards_style.to_style() dir=dir_name(view.orig_dir)>{cards}</div>
            </div>
            {mark_valid_bar(s, actions, cap, &view)}
        </div>
    };
    session.into_any()
}
