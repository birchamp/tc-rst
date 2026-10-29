//! Select.jsx: a select-only combobox, a button that opens a listbox in an anchored Layer
//! popover, because the approved look needs a search field inside the list, sticky group
//! headers and multi-column rows, which a native select cannot hold.

use std::sync::Arc;

use leptos::ev::{FocusEvent, KeyboardEvent, MouseEvent};
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;

use crate::ds::{
    Anchor, Attrs, Badge, BadgeSize, BadgeTone, Css, Dismiss, FieldContext, LayerPlacement, Level,
    Text, TextRole, TextTone, keyed_list, provide, spread, use_field,
};
use crate::keyed::{Slot, Slots};
use crate::text::option_matches;
use crate::tokens::{Corner, Edge, Fill, Shadow};

/// Lists this long or longer open with a search field (66 books do; 5 fonts do not).
const SEARCH_AT: usize = 10;

/// One option of a Select or a suggestion of a SuggestField.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
    /// A trailing code on the field and the row ("JHN").
    pub code: Option<String>,
    /// Small trailing text on the row.
    pub meta: Option<String>,
    /// A neutral pill on the row.
    pub badge: Option<String>,
    /// A sticky header label; consecutive options of one group form a section.
    pub group: Option<String>,
    pub disabled: bool,
}

impl From<&str> for SelectOption {
    fn from(s: &str) -> Self {
        Self {
            value: s.into(),
            label: s.into(),
            ..Self::default()
        }
    }
}

/// `Row` in Select.jsx: one row look for every list. The option is a signal: a list keeps a
/// row's node while the row's option changes, and the row redraws its text in place, as React
/// does.
pub(crate) fn row(
    opt: Signal<SelectOption>,
    id: Signal<String>,
    selected: Signal<bool>,
    highlighted: Signal<bool>,
    on_choose: impl Fn() + 'static,
    on_highlight: Option<Arc<dyn Fn() + Send + Sync>>,
) -> impl IntoView {
    let disabled = Memo::new(move |_| opt.with(|o| o.disabled));
    let s = move || {
        let (selected, highlighted) = (selected.get(), highlighted.get());
        let mut s = css! {
            "box-sizing": "border-box",
            "height": 36,
            "display": "flex",
            "align-items": "center",
            "gap": 10,
            "padding": "8px 10px",
            "border-radius": "var(--radius-sm)",
        };
        s.set_opt(
            "background",
            if selected && highlighted {
                Some("var(--uw-inspire-100)")
            } else if selected {
                Some("var(--surface-accent-soft)")
            } else if highlighted {
                Some("var(--uw-frost)")
            } else {
                None
            },
        );
        s.set("cursor", if disabled.get() { "default" } else { "pointer" });
        s.to_style()
    };
    let check = css! {
        "width": 14,
        "flex": "none",
        "color": "var(--text-accent)",
        "font-weight": 900,
        "font-size": "var(--fs-ui-sm)",
    };
    let label_style = move || {
        css! {
            "flex": "1 1 auto",
            "min-width": 0,
            "white-space": "nowrap",
            "overflow": "hidden",
            "text-overflow": "ellipsis",
            "font-size": "var(--fs-ui-sm)",
            "letter-spacing": "var(--track-13)",
            "font-weight": "var(--fw-heavy)",
            "color": if disabled.get() { "var(--disabled-fg)" } else if selected.get() { "var(--text-heading)" } else { "var(--text-body)" },
        }
        .to_style()
    };
    // A long meta gives way to the label: its shrink factor is large enough that the label's
    // own share of the shrink stays under the layout unit.
    let meta_css = css! {
        "flex": "0 1000000 auto",
        "min-width": 0,
        "white-space": "nowrap",
        "overflow": "hidden",
        "text-overflow": "ellipsis",
        "font-size": "var(--fs-label)",
        "color": "var(--text-tertiary)",
    }
    .to_style();
    // Each trailing part comes and goes with its text, and keeps its node while the text
    // changes.
    let part = move |get: fn(&SelectOption) -> &Option<String>| {
        let text = Memo::new(move |_| opt.with(|o| get(o).clone().unwrap_or_default()));
        let shown = Memo::new(move |_| text.with(|t| !t.is_empty()));
        (text, shown)
    };
    let (badge, has_badge) = part(|o| &o.badge);
    let (meta, has_meta) = part(|o| &o.meta);
    let (code, has_code) = part(|o| &o.code);
    let badge_view = move || {
        has_badge.get().then(|| {
            view! { <Badge tone=BadgeTone::Neutral size=BadgeSize::Sm style=css! { "flex": "none" }>{move || badge.get()}</Badge> }
        })
    };
    let meta_view = move || {
        has_meta
            .get()
            .then(|| view! { <span style=meta_css.clone()>{move || meta.get()}</span> })
    };
    let code_view = move || {
        has_code
            .get()
            .then(|| view! { <span style=code_css().to_style()>{move || code.get()}</span> })
    };
    view! {
        <div
            role="option"
            id=id
            aria-selected=move || if selected.get() { "true" } else { "false" }
            aria-disabled=move || disabled.get().then_some("true")
            on:mouseenter=move |_| {
                if let Some(h) = &on_highlight {
                    h();
                }
            }
            on:click=move |_| {
                if !disabled.get_untracked() {
                    on_choose();
                }
            }
            style=s
        >
            <span aria-hidden="true" style=check.to_style()>{move || if selected.get() { "✓" } else { "" }}</span>
            <span style=label_style>{move || opt.with(|o| o.label.clone())}</span>
            {badge_view}
            {meta_view}
            {code_view}
        </div>
    }
}

/// Listed options under one group header, or under none, keyed as React keys them: by the
/// group's name, or by the run's position for a run without one. `rows` are indexes into the
/// options.
#[derive(Debug, Clone, PartialEq)]
struct Section {
    key: String,
    name: Option<String>,
    rows: Vec<usize>,
}

/// The small tracked code after a label ("TIT").
fn code_css() -> Css {
    css! {
        "flex": "none",
        "font-size": "var(--fs-badge)",
        "letter-spacing": "var(--tracking-label)",
        "font-weight": "var(--fw-heavy)",
        "color": "var(--text-tertiary)",
    }
}

/// Element `el` as an HTML element, if it is one.
fn html(el: Option<web_sys::EventTarget>) -> Option<web_sys::HtmlElement> {
    el.and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
}

/// Select-only combobox for enumerations (books, script fonts, licences). Lists of 10 or more
/// options open with a search field; `on_change` receives the chosen option's value.
#[component]
pub fn Select(
    #[prop(optional, into)] label: Option<Signal<String>>,
    #[prop(optional)] options: Vec<SelectOption>,
    #[prop(optional, into)] hint: Option<Signal<String>>,
    #[prop(optional, into)] id: Option<String>,
    #[prop(optional, into)] value: Signal<Option<String>>,
    #[prop(optional)] on_change: Option<Callback<String>>,
    #[prop(optional, into)] disabled: MaybeProp<bool>,
    /// The texts of the list follow the shown language, as the label does.
    #[prop(optional, into)]
    search_placeholder: Option<Signal<String>>,
    #[prop(optional, into)] no_matches_label: Option<Signal<String>>,
    #[prop(optional, into)] clear_label: Option<Signal<String>>,
    /// The Field's style.
    #[prop(optional)]
    style: Css,
    /// Extra attributes of the combobox button.
    #[prop(optional)]
    attrs: Attrs,
) -> impl IntoView {
    let dropdown = move || {
        Dropdown(DropdownProps {
            options,
            value,
            on_change,
            disabled,
            search_placeholder,
            no_matches_label,
            clear_label,
            attrs,
        })
        .into_any()
    };
    crate::ds::Field(crate::ds::FieldProps {
        label,
        hint,
        id,
        style,
        children: Some(Box::new(dropdown)),
        ..Default::default()
    })
}

/// The combobox and its list: everything of Select inside its Field.
#[component]
fn Dropdown(
    options: Vec<SelectOption>,
    value: Signal<Option<String>>,
    on_change: Option<Callback<String>>,
    disabled: MaybeProp<bool>,
    search_placeholder: Option<Signal<String>>,
    no_matches_label: Option<Signal<String>>,
    clear_label: Option<Signal<String>>,
    attrs: Attrs,
) -> impl IntoView {
    let f = use_field();
    let f_disabled = f.as_ref().and_then(|f| f.disabled);
    let disabled = move || disabled.get().or(f_disabled).unwrap_or(false);
    let button = NodeRef::<leptos::html::Button>::new();
    let panel = NodeRef::<leptos::html::Div>::new();
    let open = RwSignal::new(false);
    let hover = RwSignal::new(false);
    let query = RwSignal::new(String::new());
    let hi = RwSignal::new(0usize);
    let width = RwSignal::new(None::<f64>);

    let searchable = options.len() >= SEARCH_AT;
    let opts = StoredValue::new(options);
    // Indexes into `opts` of the options the search keeps, in order.
    let visible = Memo::new(move |_| {
        let q = query.get();
        opts.with_value(|o| {
            (0..o.len())
                .filter(|&i| !searchable || option_matches(&o[i].label, o[i].code.as_deref(), &q))
                .collect::<Vec<_>>()
        })
    });
    let id_base = f
        .as_ref()
        .map_or_else(|| "tc-dd".to_string(), |f| f.id.clone());
    let list_id = format!("{id_base}-list");
    let opt_id = {
        let id_base = id_base.clone();
        move |i: usize| format!("{id_base}-opt-{i}")
    };
    let current = move || {
        let v = value.get();
        opts.with_value(|o| o.iter().find(|x| Some(&x.value) == v.as_ref()).cloned())
    };

    let open_list = move || {
        if disabled() {
            return;
        }
        query.set(String::new());
        let v = value.get_untracked();
        let at = opts.with_value(|o| o.iter().position(|x| Some(&x.value) == v.as_ref()));
        hi.set(at.unwrap_or(0));
        if let Some(b) = button.get_untracked() {
            width.set(Some(b.get_bounding_client_rect().width()));
        }
        open.set(true);
    };
    // One rule for every close: if the focus is in the list, it goes back to the field.
    let close = move || {
        let active = document().active_element();
        if let (Some(p), Some(b)) = (panel.get_untracked(), button.get_untracked()) {
            let inside = active
                .as_ref()
                .is_some_and(|a| p.contains(Some(a.as_ref())));
            if inside {
                let _ = b.focus();
            }
        }
        open.set(false);
    };
    let on_focus_out = move |to: Option<web_sys::EventTarget>| {
        if !open.get_untracked() {
            return;
        }
        let to: Option<web_sys::Node> = to.and_then(|t| t.dyn_into().ok());
        let to_button = match (&to, button.get_untracked()) {
            (Some(t), Some(b)) => {
                let b: &web_sys::Node = b.as_ref();
                t == b
            }
            _ => false,
        };
        let in_panel = to
            .as_ref()
            .is_some_and(|t| panel.get_untracked().is_some_and(|p| p.contains(Some(t))));
        if to_button || in_panel {
            return;
        }
        close();
    };
    let choose = move |i: usize| {
        // A Select turned off while its list is open must not change (tC4 #552), even for a
        // choice that arrives before the list has closed.
        if disabled() {
            return;
        }
        let Some(o) = opts.with_value(|o| o.get(i).cloned()) else {
            return;
        };
        if o.disabled {
            return;
        }
        if let Some(cb) = on_change {
            cb.run(o.value);
        }
        close();
    };
    // A Select that turns off closes its list: nothing in it may be chosen (tC4 #552, where the
    // list stayed and took a row click). The focus is not sent back to the field, which is off.
    Effect::new(move |_| {
        if disabled() && open.get_untracked() {
            open.set(false);
        }
    });
    // A press on the field's label is a press on the field, which the open list keeps: without
    // this the press takes the focus out of the search and lands outside the list, which closes
    // it, and the label's click then opens it again with the search cleared (tC4 #552).
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        let Some(b) = button.get_untracked() else {
            return;
        };
        let found = b.labels();
        let labels: Vec<web_sys::Node> =
            (0..found.length()).filter_map(|i| found.item(i)).collect();
        let keep = Closure::<dyn FnMut(web_sys::Event)>::new(|e: web_sys::Event| {
            e.prevent_default();
            e.stop_propagation();
        });
        for l in &labels {
            for kind in ["mousedown", "click"] {
                let _ = l.add_event_listener_with_callback(kind, keep.as_ref().unchecked_ref());
            }
        }
        let held = send_wrapper::SendWrapper::new((keep, labels));
        on_cleanup(move || {
            let (keep, labels) = held.take();
            for l in &labels {
                for kind in ["mousedown", "click"] {
                    let _ =
                        l.remove_event_listener_with_callback(kind, keep.as_ref().unchecked_ref());
                }
            }
        });
    });
    let move_hi = move |delta: i64| {
        let n = visible.with_untracked(Vec::len) as i64;
        hi.update(|i| *i = (*i as i64 + delta).clamp(0, (n - 1).max(0)) as usize);
    };
    let jump = move |ch: &str| {
        let c = ch.to_lowercase();
        let vis = visible.get_untracked();
        let from = hi.get_untracked();
        for n in 1..=vis.len() {
            let i = (from + n) % vis.len();
            if opts.with_value(|o| o[vis[i]].label.to_lowercase().starts_with(&c)) {
                hi.set(i);
                return;
            }
        }
    };
    let on_key_down = move |e: KeyboardEvent| {
        let key = e.key();
        if !open.get_untracked() {
            if matches!(key.as_str(), "Enter" | " " | "ArrowDown" | "ArrowUp") {
                e.prevent_default();
                open_list();
            }
            return;
        }
        let in_search = e
            .target()
            .is_some_and(|t| t.dyn_ref::<web_sys::HtmlInputElement>().is_some());
        match key.as_str() {
            "ArrowDown" => {
                e.prevent_default();
                move_hi(1);
            }
            "ArrowUp" => {
                e.prevent_default();
                move_hi(-1);
            }
            "Home" if !in_search => {
                e.prevent_default();
                hi.set(0);
            }
            "End" if !in_search => {
                e.prevent_default();
                hi.set(visible.with_untracked(Vec::len).saturating_sub(1));
            }
            "Enter" => {
                // Enter on another button inside the panel (the search field's Clear) is that
                // button's own activation, not a choice of the highlighted row.
                let target = e
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
                let on_button = match (&target, button.get_untracked()) {
                    (Some(t), Some(b)) => {
                        let b: &web_sys::Element = b.as_ref();
                        t == b
                    }
                    _ => false,
                };
                if !on_button
                    && target
                        .as_ref()
                        .is_some_and(|t| t.closest("button").ok().flatten().is_some())
                {
                    return;
                }
                e.prevent_default();
                if let Some(&i) = visible
                    .with_untracked(|v| v.get(hi.get_untracked()).copied())
                    .as_ref()
                {
                    choose(i);
                }
            }
            k if !searchable
                && k.chars().count() == 1
                && !k.trim().is_empty()
                && !e.ctrl_key()
                && !e.meta_key()
                && !e.alt_key() =>
            {
                jump(k);
            }
            _ => {}
        }
    };

    // The search field takes the focus when the list opens. The Layer keeps its panel hidden
    // until it has measured it, and a hidden element refuses focus, so this retries over the
    // first frames until the focus lands.
    Effect::new(move |_| {
        if !open.get() || !searchable {
            return;
        }
        let done = StoredValue::new(false);
        let attempt = move || {
            if done.get_value() {
                return;
            }
            let input = panel
                .get_untracked()
                .and_then(|p| p.query_selector("input").ok().flatten());
            if let Some(input) = input.and_then(|i| i.dyn_into::<web_sys::HtmlElement>().ok()) {
                let _ = input.focus();
                let active = document().active_element();
                let i: &web_sys::Element = input.as_ref();
                if active.as_ref() == Some(i) {
                    done.set_value(true);
                }
            }
        };
        attempt();
        let timers: Vec<_> = [0u64, 40, 160]
            .iter()
            .filter_map(|ms| {
                set_timeout_with_handle(attempt, std::time::Duration::from_millis(*ms)).ok()
            })
            .collect();
        on_cleanup(move || {
            done.set_value(true);
            for t in timers {
                t.clear();
            }
        });
    });

    // The highlighted row is kept in view as the highlight moves.
    {
        let opt_id = opt_id.clone();
        Effect::new(move |_| {
            let (is_open, h, _) = (open.get(), hi.get(), query.get());
            if !is_open {
                return;
            }
            let Some(i) = visible.with_untracked(|v| v.get(h).copied()) else {
                return;
            };
            let Some(p) = panel.get() else { return };
            if let Ok(Some(el)) = p.query_selector(&format!("[id=\"{}\"]", opt_id(i))) {
                let opts = web_sys::ScrollIntoViewOptions::new();
                opts.set_block(web_sys::ScrollLogicalPosition::Nearest);
                el.scroll_into_view_with_scroll_into_view_options(&opts);
            }
        });
    }

    let active = {
        let opt_id = opt_id.clone();
        Signal::derive(move || {
            if !open.get() {
                return None;
            }
            visible.with(|v| v.get(hi.get()).copied()).map(&opt_id)
        })
    };
    let button_style = move || {
        let off = disabled();
        css! {
            "box-sizing": "border-box",
            "width": "100%",
            "height": "var(--control-h-md)",
            "display": "flex",
            "align-items": "center",
            "gap": 8,
            "padding-inline": 12,
            "background": if off { "var(--disabled-bg)" } else { "var(--surface-card)" },
            "border": format!("var(--stroke) solid {}", if open.get() || hover.get() { "var(--accent)" } else { "var(--border-input)" }),
            "border-radius": "var(--radius-input)",
            "font-family": "var(--font-ui)",
            "cursor": if off { "default" } else { "pointer" },
            "text-align": "start",
        }
        .to_style()
    };
    let value_css = css! {
        "min-width": 0,
        "white-space": "nowrap",
        "overflow": "hidden",
        "text-overflow": "ellipsis",
        "font-size": "var(--fs-ui-md)",
        "font-weight": "var(--fw-heavy)",
        "color": "var(--text-body)",
    }
    .to_style();
    let chevron =
        css! { "font-size": "var(--fs-caption)", "color": "var(--text-tertiary)", "flex": "none" }
            .to_style();

    let anchor: Anchor = Callback::new(move |_| button.get_untracked().map(Into::into));
    let no_matches = no_matches_label.unwrap_or_else(|| Signal::from("No matches"));
    let (list_id_panel, list_id_button) = (list_id.clone(), list_id.clone());
    let opt_id = StoredValue::new(opt_id);

    // Sections in option order; options without a group form one headerless run. React keys
    // a section by its group's name, so a name that comes back later in the list repeats a
    // key: each section is a slot of its own and draws its own rows.
    let sections = Memo::new(move |_| {
        let mut sections: Vec<Section> = Vec::new();
        for &i in visible.get().iter() {
            let g = opts.with_value(|o| o[i].group.clone().filter(|g| !g.is_empty()));
            match sections.last_mut() {
                Some(s) if s.name == g => s.rows.push(i),
                _ => sections.push(Section {
                    key: match &g {
                        Some(name) => format!("group {name}"),
                        None => format!("run {}", sections.len()),
                    },
                    name: g,
                    rows: vec![i],
                }),
            }
        }
        sections
    });
    let section_slots =
        Memo::new(move |_| sections.with(|s| Slots::new(s.iter().map(|x| x.key.clone()))));
    // One row, for the option at index `i` of the options, which follows the row's slot.
    let option_row = move |i: Memo<usize>| {
        let o =
            Memo::new(move |_| opts.with_value(|o| o.get(i.get()).cloned().unwrap_or_default()));
        let pos = Memo::new(move |_| {
            let i = i.get();
            visible.with(|vis| vis.iter().position(|&j| j == i))
        });
        let selected = Signal::derive(move || {
            o.with(|o| value.with(|v| v.as_deref() == Some(o.value.as_str())))
        });
        let highlighted = Signal::derive(move || pos.get() == Some(hi.get()));
        let highlight: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            if let Some(p) = pos.get_untracked() {
                hi.set(p);
            }
        });
        let id = Signal::derive(move || opt_id.with_value(|f| f(i.get())));
        row(
            o.into(),
            id,
            selected,
            highlighted,
            move || choose(i.get_untracked()),
            Some(highlight),
        )
    };
    let section = move |_: Slot, at: Memo<usize>| {
        // keyed_list draws a section for a slot of the sections it just read.
        let Some(first) = sections.with_untracked(|s| s.get(at.get_untracked()).cloned()) else {
            return ().into_any();
        };
        let name = first.name.clone();
        let this = Memo::new(move |prev: Option<&Section>| {
            sections
                .with(|s| s.get(at.get()).cloned())
                .or_else(|| prev.cloned())
                .unwrap_or_else(|| first.clone())
        });
        // Rows keyed by value, as React keys them inside their section.
        let row_slots = Memo::new(move |_| {
            this.with(|s| {
                Slots::new(
                    s.rows
                        .iter()
                        .map(|&i| opts.with_value(|o| o[i].value.clone())),
                )
            })
        });
        let rows = keyed_list(row_slots, move |_, k: Memo<usize>| {
            let first = this.with_untracked(|s| s.rows.get(k.get_untracked()).copied());
            let i = Memo::new(move |prev: Option<&usize>| {
                this.with(|s| s.rows.get(k.get()).copied())
                    .or(prev.copied())
                    .or(first)
                    .unwrap_or_default()
            });
            option_row(i)
        });
        match name {
            Some(name) => {
                let head = css! {
                    "position": "sticky",
                    "top": 0,
                    "z-index": 1,
                    "background": "var(--surface-card)",
                    "padding": "10px 10px 6px",
                    "display": "flex",
                    "gap": 6,
                    "align-items": "baseline",
                };
                let count = move || this.with(|s| s.rows.len()).to_string();
                let group_label = name.clone();
                view! {
                    <div role="group" aria-label=group_label>
                        <div style=head.to_style()>
                            <Text role=TextRole::Overline tone=TextTone::Muted style=css! { "display": "inline" }>{name}</Text>
                            <Text role=TextRole::LabelNum>{count}</Text>
                        </div>
                        {rows}
                    </div>
                }
                .into_any()
            }
            None => rows.into_any(),
        }
    };
    let empty = Memo::new(move |_| visible.with(Vec::is_empty));
    let listbox = move || {
        if empty.get() {
            return view! {
                <div style="padding: 12px 10px;">
                    <Text role=TextRole::Caption tone=TextTone::Muted>{move || no_matches.get()}</Text>
                </div>
            }
            .into_any();
        }
        keyed_list(section_slots, section).into_any()
    };
    let listbox = ViewFn::from(listbox);

    let panel_children: ChildrenFn = Arc::new(move || {
        let surface_style = {
            let mut s = Css::new();
            s.set_opt("width", width.get_untracked());
            s.extend(&css! { "display": "flex", "flex-direction": "column", "overflow": "hidden" });
            s
        };
        let search = searchable.then(|| {
            let mut a = Attrs::new();
            a.set("aria-controls", list_id_panel.clone());
            let label = Signal::derive(move || search_placeholder.map(|p| p.get()));
            let field = provide(None::<FieldContext>, move || {
                crate::ds::SearchField(crate::ds::SearchFieldProps {
                    value: Some(query.into()),
                    on_change: Some(Callback::new(move |e: web_sys::Event| {
                        let v = e
                            .target()
                            .and_then(|t| js_sys::Reflect::get(&t, &"value".into()).ok())
                            .and_then(|v| v.as_string())
                            .unwrap_or_default();
                        query.set(v);
                        hi.set(0);
                    })),
                    on_clear: Some(Callback::new(move |_: MouseEvent| {
                        query.set(String::new());
                        hi.set(0);
                        // Clear leaves the field with the empty filter; without this the focus
                        // falls to the body and the keys stop reaching the list.
                        let input = panel
                            .get_untracked()
                            .and_then(|p| p.query_selector("input").ok().flatten());
                        if let Some(input) =
                            input.and_then(|i| i.dyn_into::<web_sys::HtmlElement>().ok())
                        {
                            let _ = input.focus();
                        }
                    })),
                    placeholder: search_placeholder,
                    clear_label,
                    attrs: a,
                    live_attrs: vec![("aria-activedescendant", active), ("aria-label", label)],
                    ..Default::default()
                })
            });
            view! { <div style="padding: 8px 8px 6px;" on:keydown=on_key_down>{field}</div> }
        });
        let list_id = list_id_panel.clone();
        let listbox = listbox.clone();
        view! {
            <div node_ref=panel on:focusout=move |e: FocusEvent| on_focus_out(e.related_target())>
                <crate::ds::Surface fill=Fill::Card border=Edge::Line radius=Corner::Lg elevation=Shadow::Hover style=surface_style>
                    {search}
                    <div role="listbox" id=list_id style=css! { "max-height": 340, "overflow-y": "auto", "padding": "0 6px 6px" }.to_style()>
                        {move || listbox.run()}
                    </div>
                </crate::ds::Surface>
            </div>
        }
        .into_any()
    });

    let layer = move || {
        open.get().then(|| {
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
                on_dismiss: Some(Callback::new(move |_| close())).into(),
                // Drawn only while open, with no exit animation: a closed list leaves the page
                // in the same update that closes it.
                animate: false,
                // A press in the popover keeps the focus where it is (the field or the search).
                on_mouse_down: Some(Callback::new(|e: MouseEvent| {
                    let on_control = html(e.target())
                        .and_then(|t| t.closest("input, button").ok().flatten())
                        .is_some();
                    if !on_control {
                        e.prevent_default();
                    }
                })),
                ..crate::ds::LayerProps::new(panel_children.clone())
            })
        })
    };

    let current_label = move || current().map(|c| c.label).unwrap_or_default();
    let current_code = move || {
        current()
            .and_then(|c| c.code)
            .filter(|c| !c.is_empty())
            .map(|c| view! { <span style=code_css().to_style()>{c}</span> })
    };
    view! {
        <button
            node_ref=button
            type="button"
            id=f.as_ref().map(|f| f.id.clone())
            disabled=disabled
            role="combobox"
            aria-expanded=move || if open.get() { "true" } else { "false" }
            aria-controls=list_id_button
            aria-haspopup="listbox"
            aria-describedby=f.as_ref().and_then(|f| f.described_by.clone())
            aria-activedescendant=move || if searchable { None } else { active.get() }
            on:mousedown=move |e: MouseEvent| {
                // The field is part of the open list: a press on it toggles the list.
                if open.get_untracked() {
                    e.stop_propagation();
                }
            }
            on:click=move |_| if open.get_untracked() { close() } else { open_list() }
            on:keydown=on_key_down
            on:blur=move |e: FocusEvent| on_focus_out(e.related_target())
            on:mouseenter=move |_| hover.set(true)
            on:mouseleave=move |_| hover.set(false)
            {..spread(&attrs)}
            style=button_style
        >
            <span style=css! { "flex": 1, "min-width": 0, "display": "flex", "align-items": "baseline", "gap": 8 }.to_style()>
                <span style=value_css>{current_label}</span>
                {current_code}
            </span>
            <span aria-hidden="true" style=chevron>{move || if open.get() { "▴" } else { "▾" }}</span>
        </button>
        {layer}
    }
}
