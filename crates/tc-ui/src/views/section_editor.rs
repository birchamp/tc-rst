//! The port of tC4's `SectionEditor.jsx` (unit B4b): the section card, "Drafting 9–10" with a
//! `Type | Place verse numbers` switch. Type holds the section as text; Place shows every word
//! and every verse as a pin, with a "To place" bank for the pins no word begins. Nothing is
//! written until Save, which sends the whole card at once (A-1): typing, placing a pin and
//! leaving the textarea send only the card's draft, Cancel discards the card, its buffer entry
//! and its draft, and a Save that leaves the section with no words asks first (#600).
//!
//! The card's one value is its Type text (`tc_api::model::section`); Place reads it once, as
//! tC4's session does, and each drop writes the session out again, so the buffer holds the
//! placements too. Its base is the tokens of
//! every verse it displays (STORAGE §5.2).

use std::collections::BTreeMap;
use std::sync::Arc;

use leptos::html;
use leptos::prelude::*;
use send_wrapper::SendWrapper;
use tc_api::buffer::BufferKey;
use tc_api::editor::{ChaptersView, VerseRow};
use tc_api::ids::{Scope, ViewSeq};
use tc_api::model::section::{SectionBuffer, expand_keys, indent_line, initial_text};
use wasm_bindgen::JsCast;

use super::draft_chrome::{FooterPress, editing_card};
use crate::app::save::{FieldSave, FieldScope, Pair, Source, View};
use crate::app::section::{CancelState, can_save, hints, word_title};
use crate::app::translate::{card_tokens, card_verses, range_span};
use crate::app::web::{Actions, use_runtime};
use crate::attrs::Attrs;
use crate::ds::{
    Button, ButtonVariant, Indicator, MarkerState, Overline, OverlineTone, Size, Switcher,
    SwitcherOption, Tone, VerseMarker,
};
use crate::i18n::{keys, t};

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

/// tC4's `editType`: the translation's type one step down from the reading step, at the
/// reading leading.
fn edit_type() -> crate::css::Css {
    crate::css! {
        "font-family": "var(--font-scripture)",
        "font-size": "var(--fs-verse-md)",
        "line-height": "var(--lh-verse-lg)",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Type,
    Place,
}

/// The rows of the card's keys in the chapter's read, and the session view of that read.
fn rows_of(
    text: &Option<Arc<ChaptersView>>,
    chapter: u16,
    keys: &[String],
) -> Option<(Vec<VerseRow>, ViewSeq)> {
    let view = text.as_ref()?;
    let verses = &view.chapters.iter().find(|c| c.chapter == chapter)?.verses;
    // The card keeps the verse set it was opened with, whatever the rows regroup to; a key
    // the chapter no longer has is not shown (tC4's `filter(Boolean)`).
    let rows: Vec<VerseRow> = keys
        .iter()
        .filter_map(|k| verses.iter().find(|v| v.key.as_str() == k).cloned())
        .collect();
    Some((rows, view.view))
}

/// tC4's `SectionEditor`: the editing card of one section.
#[component]
pub fn SectionEditor(
    key: BufferKey,
    keys: Vec<String>,
    text: Memo<Option<Arc<ChaptersView>>>,
    at: Scope,
    chapter: u16,
    dir: &'static str,
    actions: Actions,
) -> impl IntoView {
    let (Some(runtime), Some((rows, view))) = (
        use_runtime(),
        text.with_untracked(|t| rows_of(t, chapter, &keys)),
    ) else {
        return ().into_any();
    };
    let saves = runtime.saves().clone();
    let refs: Vec<&VerseRow> = rows.iter().collect();
    let Some(first) = card_tokens(&refs).first().cloned() else {
        return ().into_any();
    };
    let rest: Vec<_> = card_tokens(&refs).into_iter().skip(1).collect();
    let pins = expand_keys(&keys);
    let opening = initial_text(&card_verses(&refs), &pins);
    let any_drafted = rows.iter().any(|r| r.drafted);
    let span = range_span(&keys);
    // The card opens on the pair of the read its rows came from (AD37); a field the machine
    // already holds, a buffer restored after a reload, keeps its text and tokens.
    let shown = saves.open_over(
        &key,
        FieldScope::Project(Some(at.clone())),
        Pair {
            value: opening,
            token: first,
            view: Some(View::of(Some(&at), view)),
        },
        rest,
    );
    let typed = RwSignal::new(shown.get_untracked());
    {
        let shown = SendWrapper::new(shown);
        Effect::new(move |_| typed.set(shown.get()));
    }
    // tC4's Place session: Place reads the text once and a drop moves only the pins of that
    // reading. Reading the written text again after each drop made every verse boundary a line
    // break the next move kept inside a verse.
    let session: RwSignal<Option<SectionBuffer>> = RwSignal::new(None);
    let buffer = {
        let keys = keys.clone();
        Memo::new(move |_| {
            session
                .get()
                .unwrap_or_else(|| typed.with(|text| SectionBuffer::parse(text, &keys)))
        })
    };
    let empties = Memo::new(move |_| buffer.with(SectionBuffer::is_empty));
    let mode = RwSignal::new(Mode::Type);
    let asking = RwSignal::new(false);
    let requested = RwSignal::new(false);
    let cancel_state = RwSignal::new(CancelState::Idle);
    let changes = saves.changes();
    let saving = {
        let (saves, key) = (
            SendWrapper::new(saves.clone()),
            SendWrapper::new(key.clone()),
        );
        let changes = changes.clone();
        Memo::new(move |_| {
            changes.get();
            matches!(saves.state(&key), Some(FieldSave::Saving { .. }))
                || cancel_state.with(|state| matches!(state, CancelState::Pending))
        })
    };
    // The card closes when its save has stored the text; a failure or a hold keeps it open
    // with the text, and the indicator offers Retry.
    {
        let (saves, key) = (
            SendWrapper::new(saves.clone()),
            SendWrapper::new(key.clone()),
        );
        let actions = actions.clone();
        let changes = changes.clone();
        Effect::new(move |_| {
            changes.get();
            if requested.get()
                && !matches!(
                    saves.state(&key),
                    Some(
                        FieldSave::Dirty { .. }
                            | FieldSave::Saving { .. }
                            | FieldSave::Failed(_)
                            | FieldSave::Held(_)
                    )
                )
            {
                actions.close_section();
            }
        });
    }
    {
        let saves = SendWrapper::new(saves.clone());
        let key = SendWrapper::new(key.clone());
        on_cleanup(move || saves.close(&key));
    }
    // A new text of the card: a drop in Place, a Tab in Type, or the Place-to-Type reading.
    let set_text = {
        let (saves, key) = (
            SendWrapper::new(saves.clone()),
            SendWrapper::new(key.clone()),
        );
        Callback::new(move |next: String| {
            typed.set(next.clone());
            saves.typed(&key, &next, Source::Program);
        })
    };
    let area = NodeRef::<html::Textarea>::new();
    let focus_area = move || {
        crate::views::each_frame(1, move || {
            if let Some(el) = area.get_untracked() {
                let _ = el.focus();
            }
            false
        });
    };
    focus_area();
    let switch = {
        let saves = SendWrapper::new(saves.clone());
        let key = SendWrapper::new(key.clone());
        move |next: String| {
            let to = if next == "place" {
                Mode::Place
            } else {
                Mode::Type
            };
            if to == mode.get_untracked() {
                return;
            }
            if to == Mode::Type {
                // Back to Type writes the section as the Place reading has it: one line per
                // verse that has text.
                let written = buffer.with_untracked(SectionBuffer::serialize);
                session.set(None);
                if typed.with_untracked(|now| *now != written) {
                    set_text.run(written);
                }
                focus_area();
            } else {
                session.set(Some(buffer.get_untracked()));
                saves.blur(&key);
            }
            mode.set(to);
        }
    };
    let save = {
        let (saves, key) = (
            SendWrapper::new(saves.clone()),
            SendWrapper::new(key.clone()),
        );
        move || {
            asking.set(false);
            requested.set(true);
            saves.save(&key);
        }
    };
    let on_save = {
        let save = save.clone();
        Callback::new(move |_| {
            if !can_save(empties.get_untracked(), any_drafted) {
                return;
            }
            if empties.get_untracked() {
                asking.set(true);
            } else {
                save();
            }
        })
    };
    let cancel = {
        let (saves, key) = (
            SendWrapper::new(saves.clone()),
            SendWrapper::new(key.clone()),
        );
        let actions = actions.clone();
        Callback::new(move |_| {
            if cancel_state.with_untracked(|state| matches!(state, CancelState::Pending)) {
                return;
            }
            cancel_state.set(CancelState::Pending);
            let (saves, key, actions) = (saves.clone(), key.clone(), actions.clone());
            wasm_bindgen_futures::spawn_local(async move {
                match saves.discard_confirmed(&key).await {
                    Ok(true) => actions.close_section(),
                    Ok(false) => cancel_state.set(CancelState::Idle),
                    Err(error) => cancel_state.set(CancelState::Error(error)),
                }
            });
        })
    };

    let header = {
        let options = vec![
            SwitcherOption::new("type", t(keys::draft::MODE_TYPE, &[])).disabled(saving),
            SwitcherOption::new("place", t(keys::draft::MODE_PLACE, &[])).disabled(saving),
        ];
        let value = Signal::derive(move || {
            Some(
                match mode.get() {
                    Mode::Type => "type",
                    Mode::Place => "place",
                }
                .to_owned(),
            )
        });
        let hint_style = crate::css! {
            "font-size": "var(--fs-label)",
            "letter-spacing": "var(--track-11)",
            "color": "var(--text-tertiary)",
        }
        .to_style();
        let title = span.clone();
        view! {
            <Overline tone=OverlineTone::Accent>{t(keys::draft::DRAFTING, &[])}" "{title}</Overline>
            <Switcher
                indicator=Indicator::Pill
                size=Size::Sm
                tone=Tone::Ocean
                value=value
                label=t(keys::draft::DRAFTING, &[])
                on_change=Callback::new(switch)
                options=options
            />
            {move || {
                (mode.get() == Mode::Type)
                    .then(|| view! { <span style=hint_style.clone()>{t(keys::draft::TYPE_HINT, &[])}</span> })
            }}
        }
        .into_any()
    };

    // A drop: the session moves on, and the card's text is the session written out, so the
    // buffer, the mirror and Save carry the placements.
    let place = Callback::new(move |next: SectionBuffer| {
        let written = next.serialize();
        session.set(Some(next));
        set_text.run(written);
    });
    let field = {
        let (saves, key) = (
            SendWrapper::new(saves.clone()),
            SendWrapper::new(key.clone()),
        );
        let span = span.clone();
        let pins = pins.clone();
        move || match mode.get() {
            Mode::Type => type_field(
                &saves,
                &key,
                typed,
                set_text,
                area,
                dir,
                &span,
                &pins,
                typed.get_untracked(),
                saving,
            ),
            Mode::Place => place_view(buffer, place, dir, saving),
        }
    };

    let footer = {
        let off = Signal::derive(move || saving.get() || !can_save(empties.get(), any_drafted));
        let cancel_style = crate::css! {
            "color": "var(--text-tertiary)",
            "font-size": "var(--fs-caption)",
            "letter-spacing": "var(--track-12)",
        };
        view! {
            <Button size=Size::Sm disabled=off on_click=on_save>{t(keys::draft::SAVE_SECTION, &[])}</Button>
            <Button variant=ButtonVariant::Ghost style=cancel_style disabled=saving on_click=cancel>{move || {
                if cancel_state.with(|state| matches!(state, CancelState::Error(_))) {
                    t(keys::app::RETRY, &[])
                } else { t(keys::draft::CANCEL_VERSE, &[]) }
            }}</Button>
            {move || cancel_state.with(|state| match state {
                CancelState::Error(error) => Some(view! { <span role="alert" data-testid="section-cancel-error">{error.clone()}</span> }),
                CancelState::Idle | CancelState::Pending => None,
            })}
        }
        .into_any()
    };

    let question = {
        let save = save.clone();
        let span = span.clone();
        move || {
            asking
                .get()
                .then(|| empty_question(&span, asking, save.clone()))
        }
    };
    let card = editing_card(
        header,
        view! { {field} }.into_any(),
        footer,
        FooterPress::TakesFocus,
        Attrs::from([("data-testid", "section-editor")]),
    );
    view! { {card} {question} }.into_any()
}

/// The Type field: the section as text, one verse a line; Tab and Shift+Tab indent the line for
/// poetry (#54).
#[expect(
    clippy::too_many_arguments,
    reason = "a view's inputs: the state, handles and signals it reads"
)]
fn type_field(
    saves: &SendWrapper<crate::app::web::Saves>,
    key: &SendWrapper<BufferKey>,
    typed: RwSignal<String>,
    set_text: Callback<String>,
    area: NodeRef<html::Textarea>,
    dir: &'static str,
    span: &str,
    pins: &[String],
    initial: String,
    saving: Memo<bool>,
) -> AnyView {
    let pins = pins.to_vec();
    let on_input = {
        let (saves, key) = (saves.clone(), key.clone());
        move |event: leptos::ev::Event| {
            let value = event_target_value(&event);
            typed.set(value.clone());
            saves.typed(&key, &value, Source::Input);
        }
    };
    let on_focus = {
        let (saves, key) = (saves.clone(), key.clone());
        move |event: leptos::ev::FocusEvent| {
            if let Some(element) = event
                .target()
                .and_then(|target| target.dyn_into::<web_sys::HtmlElement>().ok())
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
    let on_key = move |event: leptos::ev::KeyboardEvent| {
        // The card is read-only while its save runs (D7), and `readonly` stops only input.
        let field = area.get_untracked().filter(|_| !saving.get_untracked());
        if let Some(next) = tab_indent(&event, field, |text, caret, delta| {
            indent_line(text, caret, delta, &pins)
        }) {
            set_text.run(next);
        }
    };
    let mut style = crate::css! {
        "width": "100%",
        "box-sizing": "border-box",
        "border": 0,
        "outline": "none",
        "resize": "vertical",
        "tab-size": "2ch",
    };
    style.extend(&edit_type());
    style.extend(&crate::css! { "color": "var(--text-scripture)", "background": "transparent" });
    let value = move || typed.get();
    view! {
        <textarea
            node_ref=area
            aria-label=t(keys::draft::SECTION_LABEL, &[("span", &span)])
            dir=dir
            prop:value=value
            readonly=move || saving.get()
            placeholder=t(keys::draft::SECTION_PLACEHOLDER, &[])
            rows="6"
            style=style.to_style()
            on:input=on_input
            on:focus=on_focus
            on:blur=on_blur
            on:compositionstart=on_composition
            on:keydown=on_key
        >
            {initial}
        </textarea>
    }
    .into_any()
}

/// Tab and Shift+Tab in a poem field (#54, tC4 #829): every Tab stays in the field, and
/// `indent` gives the caret's line one level more or less. The new text, when it changed; the
/// field already shows it, with its caret.
pub(crate) fn tab_indent(
    event: &web_sys::KeyboardEvent,
    field: Option<web_sys::HtmlTextAreaElement>,
    indent: impl Fn(&str, usize, i32) -> (String, usize),
) -> Option<String> {
    // An IME may pick a candidate with Tab; the key is its own until the composition ends.
    if event.key() != "Tab" || event.is_composing() {
        return None;
    }
    event.prevent_default();
    let el = field?;
    let caret = el.selection_start().ok().flatten().unwrap_or(0) as usize;
    let delta = if event.shift_key() { -1 } else { 1 };
    let now = el.value();
    let (next, at) = indent(&now, caret, delta);
    if next == now {
        return None;
    }
    // The value and the caret together, before the signal writes the same value again.
    el.set_value(&next);
    let at = u32::try_from(at).unwrap_or(0);
    let _ = el.set_selection_range(at, at);
    Some(next)
}

/// Place mode: the bank of unplaced pins, then the words with their pins.
fn place_view(
    buffer: Memo<SectionBuffer>,
    place: Callback<SectionBuffer>,
    dir: &'static str,
    inert: Memo<bool>,
) -> AnyView {
    let held: RwSignal<Option<String>> = RwSignal::new(None);
    let hover: RwSignal<Option<usize>> = RwSignal::new(None);
    // Pick a pin up (or put the held one back); Escape on a word also puts it back.
    let pick = Callback::new(move |pin: String| {
        if inert.get_untracked() {
            return;
        }
        held.update(|h| {
            *h = if h.as_deref() == Some(pin.as_str()) {
                None
            } else {
                Some(pin)
            }
        });
        hover.set(None);
    });
    let cancel = Callback::new(move |()| {
        held.set(None);
        hover.set(None);
    });
    let drop_at = Callback::new(move |index: usize| {
        let Some(pin) = held.get_untracked().filter(|_| !inert.get_untracked()) else {
            return;
        };
        let next = buffer.with_untracked(|b| b.drop_pin(&pin, index));
        if let Some(next) = next {
            place.run(next);
        }
        held.set(None);
        hover.set(None);
    });
    let bank = Memo::new(move |_| {
        buffer.with(|b| {
            b.bank()
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<String>>()
        })
    });
    let word_count = Memo::new(move |_| buffer.with(|b| b.words().len()));
    let last_pin =
        Memo::new(move |_| buffer.with(|b| b.pins().last().cloned().unwrap_or_default()));
    let hint = Memo::new(move |_| {
        held.with(|h| hints(h.as_deref(), bank.with(Vec::len), word_count.get()))
    });
    // Where each word's pins sit: the words are re-read only when a pin moves.
    let by_word = Memo::new(move |_| {
        buffer.with(|b| {
            let mut at: BTreeMap<usize, Vec<String>> = BTreeMap::new();
            for pin in b.pins() {
                if let Some(index) = b.marker(pin) {
                    at.entry(index).or_default().push(pin.clone());
                }
            }
            at
        })
    });
    let bank_style = crate::css! {
        "display": "flex",
        "align-items": "center",
        "gap": 10,
        "flex-wrap": "wrap",
        "padding": "8px 12px",
        "border-radius": "var(--radius-md)",
        "background": "var(--surface-warm)",
        "border": "var(--stroke) solid var(--tc-warn-border)",
        "margin-bottom": 12,
    }
    .to_style();
    let label_style = crate::css! {
        "font-size": "var(--fs-label)",
        "letter-spacing": "var(--tracking-overline)",
        "text-transform": "uppercase",
        "font-weight": "var(--fw-heavy)",
        "color": "var(--tc-warn-text-2)",
        "flex": "none",
    }
    .to_style();
    let pins_style = crate::css! {
        "display": "flex",
        "align-items": "flex-end",
        "gap": 6,
        "flex-wrap": "wrap",
        "min-height": 32,
    }
    .to_style();
    let quiet = crate::css! {
        "font-style": "italic",
        "font-size": "var(--fs-ui-sm)",
        "color": "var(--text-tertiary)",
    }
    .to_style();
    // The instruction keeps its own row, as tall as its tallest text, in every state: a row
    // that came and went on a pick-up moved the words under the pointer, and the click placed
    // the number on the word that arrived there (#598).
    let instruction = crate::css! {
        "display": "grid",
        "flex-basis": "100%",
        "text-align": "end",
        "font-size": "var(--fs-caption)",
        "letter-spacing": "var(--track-12)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--tc-warn-text-2)",
    }
    .to_style();
    let pins = move || {
        let held_now = held.get();
        bank.get()
            .into_iter()
            .map(|pin| {
                let id = format!("pin-{pin}");
                let n = pin.parse::<u32>().unwrap_or(0);
                let state = if held_now.as_deref() == Some(pin.as_str()) {
                    MarkerState::Dragging
                } else {
                    MarkerState::Idle
                };
                let pick_up = {
                    let pin = pin.clone();
                    Callback::new(move |_: web_sys::Event| pick.run(pin.clone()))
                };
                view! {
                    <VerseMarker
                        n=n
                        label=t(keys::draft::MOVE_VERSE, &[("n", &pin)])
                        state=state
                        on_pick_up=pick_up
                        attrs=Attrs::from([("data-testid", id.as_str())])
                    />
                }
            })
            .collect_view()
    };
    let note_style = quiet.clone();
    let bank_note = move || {
        hint.with(|h| h.bank_empty).then(
            || view! { <span style=note_style.clone()>{t(keys::draft::BANK_EMPTY, &[])}</span> },
        )
    };
    let pick_hint = move || t(keys::draft::PICK_HINT, &[]);
    let drop_hint = move || {
        let n = hint
            .with(|h| h.drop.clone())
            .unwrap_or_else(|| last_pin.get());
        t(keys::draft::DROP_HINT, &[("n", &n)])
    };
    let visible = |on: bool| if on { "visible" } else { "hidden" };
    let words = move || {
        let list = buffer.with(|b| b.words().to_vec());
        if list.is_empty() {
            return view! { <span style=quiet.clone()>{t(keys::draft::MARKER_EMPTY, &[])}</span> }
                .into_any();
        }
        let mut style = crate::css! { "text-align": "start" };
        style.extend(&edit_type());
        style.extend(&crate::css! { "color": "var(--text-scripture)", "margin": 0 });
        let preview = move |i: usize| {
            let (held, hover) = (held.get(), hover.get());
            held.zip(hover)
                .map(|(h, from)| (from, buffer.with(|b| b.span_end(from, Some(&h)))))
                .is_some_and(|(from, to)| i >= from && i < to)
        };
        let body = list
            .into_iter()
            .enumerate()
            .map(|(i, word)| {
                let wrap = move || {
                    if preview(i) {
                        "position: relative; background: var(--tc-highlight-soft); border-radius: var(--radius-xs);"
                    } else {
                        "position: relative;"
                    }
                };
                view! {
                    <span style=wrap>
                        {place_word(i, word, buffer, by_word, held, hover, drop_at, pick, cancel, dir)}
                    </span>
                    " "
                }
            })
            .collect_view();
        let select = move || {
            if held.with(Option::is_some) {
                "user-select: none;"
            } else {
                ""
            }
        };
        view! {
            <p dir=dir data-testid="place-words" style=move || format!("{}{}", style.to_style(), select())>
                {body}
            </p>
        }
        .into_any()
    };
    view! {
        <div>
            <div data-testid="pin-bank" style=bank_style>
                <span style=label_style>{t(keys::draft::TO_PLACE, &[])}</span>
                <div style=pins_style>{pins}{bank_note}</div>
                <span style=instruction>
                    <span style=move || format!("grid-area: 1 / 1; visibility: {};", visible(hint.with(|h| h.pick)))>{pick_hint}</span>
                    <span style=move || format!("grid-area: 1 / 1; visibility: {};", visible(hint.with(|h| h.drop.is_some())))>{drop_hint}</span>
                </span>
            </div>
            {words}
        </div>
    }
    .into_any()
}

/// One word in Place mode: the pins before it (the held pin's ghost where it may land, the
/// fixed first number as a plain superscript, each pin of a verse that begins here), then the
/// word itself, a real control while a pin is in hand and may land here.
#[expect(
    clippy::too_many_arguments,
    reason = "a view's inputs: the state, handles and signals it reads"
)]
fn place_word(
    i: usize,
    word: String,
    buffer: Memo<SectionBuffer>,
    by_word: Memo<BTreeMap<usize, Vec<String>>>,
    held: RwSignal<Option<String>>,
    hover: RwSignal<Option<usize>>,
    drop_at: Callback<usize>,
    pick: Callback<String>,
    cancel: Callback<()>,
    dir: &'static str,
) -> AnyView {
    let here = move || by_word.with(|m| m.get(&i).cloned().unwrap_or_default());
    let ok = move || {
        held.with(|h| {
            h.as_deref()
                .is_some_and(|h| buffer.with(|b| b.can_drop(h, i)))
        })
    };
    // The pin a held one would join or find in its way: the last that is not the held one.
    let pin_at = move || {
        let held = held.get();
        here().into_iter().rev().find(|p| Some(p) != held.as_ref())
    };
    let first = move || buffer.with(|b| b.pins().first().cloned().unwrap_or_default());
    let before = move || {
        let held_now = held.get();
        let ghost = (hover.get() == Some(i) && ok())
            .then(|| held_now.clone())
            .flatten()
            .filter(|h| !here().contains(h))
            .map(|h| {
                let n = h.parse::<u32>().unwrap_or(0);
                // The ghost shows where the held pin would land. It is out of the flow: an
                // in-flow ghost shifted the text under the pointer on every hover, so a click
                // aimed at a word landed beside it. It takes the drop too (press or release).
                view! {
                    <VerseMarker
                        n=n
                        state=MarkerState::Dragging
                        on_pick_up=Callback::new(move |_: web_sys::Event| drop_at.run(i))
                        on_pointer_up=Callback::new(move |_: web_sys::PointerEvent| drop_at.run(i))
                        style=crate::css! {
                            "position": "absolute",
                            "inset-inline-end": "100%",
                            "top": "50%",
                            "transform": "translateY(-60%)",
                            "z-index": 1,
                        }
                        attrs=Attrs::from([("aria-hidden", "true"), ("tabindex", "-1")])
                    />
                }
            });
        let pins = here()
            .into_iter()
            .map(|pin| {
                if pin == first() {
                    let title = t(keys::draft::FIRST_FIXED, &[("n", &pin)]);
                    view! { <sup title=title style=sup()>{pin}</sup> }.into_any()
                } else {
                    let id = format!("pin-{pin}");
                    let n = pin.parse::<u32>().unwrap_or(0);
                    let dragging = held_now.as_deref() == Some(pin.as_str());
                    let label = t(keys::draft::MOVE_VERSE, &[("n", &pin)]);
                    let pick_up = {
                        let pin = pin.clone();
                        Callback::new(move |_: web_sys::Event| pick.run(pin.clone()))
                    };
                    view! {
                        <VerseMarker
                            n=n
                            label=label
                            state=if dragging { MarkerState::Dragging } else { MarkerState::Idle }
                            on_pick_up=pick_up
                            style=crate::css! { "margin-inline-end": ".12em" }
                            attrs=Attrs::from([("data-testid", id.as_str())])
                        />
                    }
                    .into_any()
                }
            })
            .collect_view();
        view! { {ghost}{pins} }
    };
    let title = {
        let word = word.clone();
        move || word_title(held.get().as_deref(), pin_at().as_deref(), ok(), &word)
    };
    let on_enter = move || {
        if held.with_untracked(Option::is_some) {
            hover.set(Some(i));
        }
    };
    let on_key = move |event: leptos::ev::KeyboardEvent| {
        if held.with_untracked(Option::is_none) {
            return;
        }
        let key = event.key();
        if (key == "Enter" || key == " ") && ok() {
            event.prevent_default();
            drop_at.run(i);
        } else if key == "Escape" {
            event.prevent_default();
            cancel.run(());
        }
    };
    let cursor = move || {
        let cursor = if held.with(Option::is_none) {
            "default"
        } else if ok() {
            "copy"
        } else {
            "not-allowed"
        };
        format!(
            "display: inline-block; border-radius: var(--radius-xs); padding: 0 .06em; cursor: {cursor};"
        )
    };
    view! {
        {before}
        <span
            dir=dir
            role=move || ok().then_some("button")
            tabindex=move || if ok() { "0" } else { "-1" }
            aria-label={
                let title = title.clone();
                move || ok().then(&title)
            }
            title=title
            on:mouseenter=move |_| on_enter()
            on:focus=move |_| on_enter()
            on:click=move |_| {
                if ok() {
                    drop_at.run(i);
                }
            }
            on:pointerup=move |_| {
                // D70.3 is a drag: press the pin, move, release on the word. The pin's
                // pointerdown picks it up, so the release here places it.
                if ok() {
                    drop_at.run(i);
                }
            }
            on:keydown=on_key
            style=cursor
        >
            {word}
        </span>
    }
    .into_any()
}

/// M18, #600: "Remove all the text of section 9–10?" The save has no words to write, so every
/// verse of the section would go back to not drafted. Keep editing, Escape, the ✕ and a press
/// on the scrim write nothing.
fn empty_question(
    span: &str,
    asking: RwSignal<bool>,
    save: impl Fn() + Clone + 'static,
) -> AnyView {
    let save = SendWrapper::new(save);
    let title = t(keys::draft::EMPTY_TITLE, &[("span", &span)]);
    let footer: ChildrenFn = Arc::new(move || {
        let confirm = {
            let save = save.clone();
            Callback::new(move |_| (*save)())
        };
        view! {
            <Button variant=ButtonVariant::Secondary on_click=Callback::new(move |_| asking.set(false))>
                {t(keys::draft::EMPTY_KEEP, &[])}
            </Button>
            <Button on_click=confirm>{t(keys::draft::EMPTY_CONFIRM, &[])}</Button>
        }
        .into_any()
    });
    let children: ChildrenFn = Arc::new(move || {
        let body = crate::css! {
            "font-size": "var(--fs-ui-sm)",
            "letter-spacing": "var(--track-13)",
            "color": "var(--text-secondary)",
            "line-height": "var(--lh-body)",
            "margin": 0,
        }
        .to_style();
        view! { <p style=body>{t(keys::draft::EMPTY_BODY, &[])}</p> }.into_any()
    });
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some((move || title.clone()).into()),
        width: Some(460.into()),
        close_label: Some(Signal::derive(|| t(keys::common::CLOSE, &[]))),
        on_close: Some(Callback::new(move |()| asking.set(false))),
        footer: Some(footer),
        attrs: Attrs::from([("data-testid", "section-empty-confirm")]),
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}
