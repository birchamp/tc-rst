//! SuggestField.jsx: a text field that offers suggestions for what is typed. The typed text is
//! the value; a suggestion is an offer, and the caller decides what a choice fills in. The
//! list is the Select's: the same Layer popover, the same row.

use std::sync::Arc;

use leptos::ev::{KeyboardEvent, MouseEvent};
use leptos::prelude::*;

use super::select::{SelectOption, row};
use crate::ds::{Anchor, Attrs, Css, Dismiss, LayerPlacement, Level, use_field};
use crate::tokens::{Corner, Edge, Fill, Shadow};

/// Labelled text input with a list of suggestions under it. `suggestions` are rows for the
/// current value; the list shows while the user types and there is at least one. Arrow keys
/// move, Enter or a press chooses and calls `on_choose`, Escape closes the list.
#[component]
pub fn SuggestField(
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional, into)] hint: Option<String>,
    #[prop(optional, into)] id: Option<String>,
    #[prop(optional, into)] suggestions: Signal<Vec<SelectOption>>,
    #[prop(optional, into)] value: Option<Signal<String>>,
    #[prop(optional)] on_change: Option<Callback<web_sys::Event>>,
    #[prop(optional)] on_choose: Option<Callback<SelectOption>>,
    #[prop(optional, into)] placeholder: Option<String>,
    /// The Field's style.
    #[prop(optional)]
    style: Css,
    /// Extra attributes of the native input.
    #[prop(optional)]
    attrs: Attrs,
) -> impl IntoView {
    let suggest =
        move || suggest(suggestions, value, on_change, on_choose, placeholder, attrs).into_any();
    crate::ds::Field(crate::ds::FieldProps {
        label,
        hint,
        id,
        style,
        children: Some(Box::new(suggest)),
        ..Default::default()
    })
}

fn suggest(
    suggestions: Signal<Vec<SelectOption>>,
    value: Option<Signal<String>>,
    on_change: Option<Callback<web_sys::Event>>,
    on_choose: Option<Callback<SelectOption>>,
    placeholder: Option<String>,
    attrs: Attrs,
) -> impl IntoView {
    let f = use_field();
    let base = f
        .as_ref()
        .map_or_else(|| "tc-sg".to_string(), |f| f.id.clone());
    let list_id = format!("{base}-list");
    let opt_id = move |i: usize| format!("{base}-opt-{i}");
    let boxed = NodeRef::<leptos::html::Div>::new();
    let panel = NodeRef::<leptos::html::Div>::new();
    let open = RwSignal::new(false);
    // No row is highlighted until an arrow key moves there: Enter with nothing highlighted
    // leaves the typed text alone.
    let hi = RwSignal::new(-1i64);
    let width = RwSignal::new(None::<f64>);
    let shown = Memo::new(move |_| open.get() && suggestions.with(|s| !s.is_empty()));

    let show = move || {
        if let Some(b) = boxed.get_untracked() {
            width.set(Some(b.get_bounding_client_rect().width()));
        }
        open.set(true);
    };
    let close = move || {
        open.set(false);
        hi.set(-1);
    };
    // A row drawn under a pointer at rest must not become the highlight: the first pointer
    // position seen in an open list is only remembered, and a row is highlighted when the
    // pointer is next seen somewhere else.
    let pointer = StoredValue::new(None::<(i32, i32)>);
    Effect::new(move |_| {
        if !shown.get() {
            pointer.set_value(None);
        }
    });
    let choose = move |o: SelectOption| {
        if let Some(cb) = on_choose {
            cb.run(o);
        }
        close();
    };
    let on_key_down = move |e: KeyboardEvent| {
        let n = suggestions.with_untracked(Vec::len) as i64;
        match e.key().as_str() {
            "ArrowDown" if n > 0 => {
                e.prevent_default();
                if !shown.get_untracked() {
                    show();
                }
                hi.update(|i| *i = (*i + 1).min(n - 1));
            }
            "ArrowUp" if shown.get_untracked() => {
                e.prevent_default();
                hi.update(|i| *i = (*i - 1).max(0));
            }
            "Enter" if shown.get_untracked() => {
                let h = hi.get_untracked();
                if let Some(o) = (h >= 0)
                    .then(|| suggestions.with_untracked(|s| s.get(h as usize).cloned()))
                    .flatten()
                {
                    e.prevent_default();
                    choose(o);
                }
            }
            _ => {}
        }
    };

    // The highlighted row is kept in view as the highlight moves.
    {
        let opt_id = opt_id.clone();
        Effect::new(move |_| {
            let (s, h) = (shown.get(), hi.get());
            if !s || h < 0 {
                return;
            }
            let Some(p) = panel.get() else { return };
            if let Ok(Some(el)) = p.query_selector(&format!("[id=\"{}\"]", opt_id(h as usize))) {
                let opts = web_sys::ScrollIntoViewOptions::new();
                opts.set_block(web_sys::ScrollLogicalPosition::Nearest);
                el.scroll_into_view_with_scroll_into_view_options(&opts);
            }
        });
    }

    let mut native = Attrs::new()
        .with("role", "combobox")
        .with("autocomplete", "off")
        .with("aria-autocomplete", "list")
        .with("aria-controls", list_id.clone());
    native.extend(&attrs);
    let live = {
        let opt_id = opt_id.clone();
        vec![
            (
                "aria-expanded",
                Signal::derive(move || {
                    Some(if shown.get() { "true" } else { "false" }.to_string())
                }),
            ),
            (
                "aria-activedescendant",
                Signal::derive(move || {
                    let h = hi.get();
                    (shown.get() && h >= 0).then(|| opt_id(h as usize))
                }),
            ),
        ]
    };
    let input = crate::ds::Input(crate::ds::InputProps {
        value,
        placeholder,
        on_change: Some(Callback::new(move |e: web_sys::Event| {
            if let Some(cb) = on_change {
                cb.run(e);
            }
            hi.set(-1);
            show();
        })),
        on_key_down: Some(Callback::new(on_key_down)),
        on_blur: Some(Callback::new(move |_| close())),
        attrs: native,
        live_attrs: live,
        ..Default::default()
    });

    let anchor: Anchor = Callback::new(move |_| boxed.get_untracked().map(Into::into));
    // Rows keyed by value, as React keys them: a row stays the same node while the list
    // around it changes.
    let rows = {
        let opt_id = opt_id.clone();
        move || {
            let opt_id = opt_id.clone();
            view! {
                <For
                    each=move || suggestions.get()
                    key=|o| o.value.clone()
                    children=move |o| {
                        let v = o.value.clone();
                        let pos = Memo::new(move |_| {
                            suggestions.with(|s| s.iter().position(|x| x.value == v))
                        });
                        let opt_id = opt_id.clone();
                        let id = Signal::derive(move || opt_id(pos.get().unwrap_or(0)));
                        let highlighted =
                            Signal::derive(move || pos.get().is_some_and(|p| p as i64 == hi.get()));
                        let chosen = o.clone();
                        row(o, id, Signal::from(false), highlighted, move || choose(chosen.clone()), None)
                    }
                />
            }
        }
    };
    let rows: Arc<dyn Fn() -> AnyView + Send + Sync> = Arc::new(move || rows().into_any());
    let panel_children: ChildrenFn = Arc::new(move || {
        let mut s = Css::new();
        s.set_opt("width", width.get_untracked());
        s.set("overflow", "hidden");
        let rows = rows.clone();
        let list_id = list_id.clone();
        view! {
            <div node_ref=panel>
                <crate::ds::Surface fill=Fill::Card border=Edge::Line radius=Corner::Lg elevation=Shadow::Hover style=s>
                    // tabindex: a list that scrolls is a Tab stop of its own; Tab from the
                    // field must go to the next field.
                    <div
                        role="listbox"
                        id=list_id
                        tabindex="-1"
                        on:mousemove=move |e: MouseEvent| {
                            let was = pointer.get_value();
                            let now = (e.client_x(), e.client_y());
                            pointer.set_value(Some(now));
                            if was.is_none_or(|w| w == now) {
                                return;
                            }
                            use wasm_bindgen::JsCast;
                            let row = e
                                .target()
                                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                                .and_then(|t| t.closest("[role=\"option\"]").ok().flatten());
                            if let Some(row) = row {
                                let parent = row.parent_element();
                                let at = parent.map(|p| p.children()).and_then(|c| {
                                    (0..c.length()).find(|&k| c.item(k).as_ref() == Some(&row))
                                });
                                if let Some(at) = at {
                                    hi.set(i64::from(at));
                                }
                            }
                        }
                        on:mouseleave=move |_| hi.set(-1)
                        style="max-height: 340px; overflow-y: auto; padding: 6px;"
                    >
                        {move || rows()}
                    </div>
                </crate::ds::Surface>
            </div>
        }
        .into_any()
    });
    let layer = move || {
        shown.get().then(|| {
            crate::ds::Layer(crate::ds::LayerProps {
                open: Some(Signal::from(true)),
                level: Level::Popover,
                placement: LayerPlacement::Anchor,
                anchor_to: Some(anchor),
                dismiss: Dismiss {
                    scrim: false,
                    escape: true,
                    outside: true,
                },
                restore_focus: false,
                on_dismiss: Some(Callback::new(move |_| close())),
                animate: false,
                // A press in the list keeps the focus in the field.
                on_mouse_down: Some(Callback::new(|e: MouseEvent| e.prevent_default())),
                ..crate::ds::LayerProps::new(panel_children.clone())
            })
        })
    };
    view! {
        // The field is part of the open list: a press in it moves the text cursor and leaves
        // the list open.
        <div
            node_ref=boxed
            on:mousedown=move |e: MouseEvent| {
                if shown.get_untracked() {
                    e.stop_propagation();
                }
            }
        >
            {input}
        </div>
        {layer}
    }
}
