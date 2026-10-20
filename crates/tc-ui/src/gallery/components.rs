//! One function per DS export: the case's JSON props as the component's typed props. Each
//! builds the props struct field by field, `attrs` last, so `Props::rest` sees every prop the
//! component reads as read.

use std::sync::Arc;

use leptos::prelude::*;
use serde_json::{Map, Value};

use super::props::{Arg, Props, flag_in, warn};
use super::render::{asset_url, node};
use crate::css::{CssValue, js_number};
use crate::ds::*;

pub fn render(name: &str, p: Props) -> AnyView {
    match name {
        // primitives
        "Action" => action(p),
        "Choice" => choice(p),
        "Field" => field(p),
        "FieldGroup" => field_group(p),
        "Input" => input(p),
        "Layer" => layer(p),
        "Progress" => progress(p),
        "Rule" => rule(p),
        "Stack" => stack(p),
        "Surface" => surface(p),
        "Switcher" => switcher(p),
        "Table" => table(p),
        "Text" => text(p),
        // core
        "Badge" => badge(p),
        "Button" => button(p),
        "Divider" => divider(p),
        "IconButton" => icon_button(p),
        "Overline" => overline(p),
        "ProgressBar" => progress_bar(p),
        "StatusDot" => status_dot(p),
        // data
        "Accordion" => accordion(p),
        "Avatar" => avatar(p),
        "KeyValueGrid" => key_value_grid(p),
        // feedback
        "ConfirmDialog" => confirm_dialog(p),
        "EmptyState" => empty_state(p),
        "InlineError" => inline_error(p),
        "Spinner" => spinner(p),
        "Toast" => toast(p),
        "Tooltip" => tooltip(p),
        // forms
        "Checkbox" => checkbox(p),
        "DropZone" => drop_zone(p),
        "FilterChip" => filter_chip(p),
        "OptionCard" => option_card(p),
        "Radio" => radio(p),
        "RadioGroup" => radio_group(p),
        "SearchField" => search_field(p),
        "Select" => select(p),
        "SuggestField" => suggest_field(p),
        "TextArea" => text_area(p),
        "TextField" => text_field(p),
        "Toggle" => toggle(p),
        // navigation
        "AppHeader" => app_header(p),
        "Breadcrumb" => breadcrumb(p),
        "Menu" => menu(p),
        // surfaces
        "BookTile" => book_tile(p),
        "Callout" => callout(p),
        "Card" => card(p),
        "Drawer" => drawer(p),
        "HelpCard" => help_card(p),
        "Modal" => modal(p),
        // translation
        "ScripturePassage" => scripture_passage(p),
        "VerseMarker" => verse_marker(p),
        "WordChip" => word_chip(p),
        other => {
            warn(&format!("gallery: no component {other}"));
            ().into_any()
        }
    }
}

// ---------------------------------------------------------------- reading values

fn handler<T: Arg>(p: &Props, key: &str) -> Option<Callback<T>> {
    p.handler(key)
}

/// A callback the component takes as a signal: the callback while it is in force.
fn live_handler<T: Arg>(p: &Props, key: &str) -> Signal<Option<Callback<T>>> {
    let cb = p.screen_handler(key);
    let on = p.handler_on(key);
    Signal::derive(move || cb.filter(|_| on.get()))
}

fn css_value(v: Option<Value>) -> Option<CssValue> {
    match v? {
        Value::Number(n) => Some(CssValue::Number(n.as_f64().unwrap_or(0.0))),
        Value::String(s) => Some(s.into()),
        _ => None,
    }
}

/// A string, or a number written as JavaScript would.
fn text_of(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => n.as_f64().map(js_number),
        _ => None,
    }
}

fn field_of(o: &Map<String, Value>, key: &str) -> Option<String> {
    o.get(key).and_then(text_of)
}

fn flag_of(o: &Map<String, Value>, key: &str) -> bool {
    o.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// `resolveRadius` reads a keyword, a number of px or a CSS length.
fn radius(p: &Props, key: &str) -> Option<Radius> {
    match p.value(key)? {
        Value::String(s) => Some(Radius::parse(&s).unwrap_or_else(|| {
            // tC4 falls back to the scale rather than drop the declaration, which would draw
            // square corners.
            warn(&format!(
                "Surface: unknown radius {s:?}, falling back to --radius-lg"
            ));
            Radius::Corner(Corner::Lg)
        })),
        Value::Number(n) => Some(Radius::Css(format!(
            "{}px",
            js_number(n.as_f64().unwrap_or(0.0))
        ))),
        _ => None,
    }
}

fn pad(p: &Props, key: &str) -> Option<Pad> {
    match p.value(key)? {
        Value::String(s) => Some(Pad::from(s.as_str())),
        Value::Number(n) => Some(Pad::Px(n.as_f64().unwrap_or(0.0))),
        _ => None,
    }
}

fn measure(p: &Props, key: &str) -> Option<Measure> {
    match p.value(key)? {
        Value::Bool(true) => Some(Measure::Token(MeasureToken::Read)),
        Value::String(s) => Some(MeasureToken::parse(&s).map_or(Measure::Css(s), Measure::Token)),
        _ => None,
    }
}

fn columns(p: &Props, key: &str) -> Option<Columns> {
    match p.value(key)? {
        Value::Number(n) => Some(Columns::Count(n.as_u64().unwrap_or(1) as u32)),
        Value::String(s) => Some(Columns::Template(s)),
        _ => None,
    }
}

/// A `{ "$asset": … }` as the URL the gallery serves it at, or a plain string URL.
fn asset(p: &Props, key: &str) -> Option<String> {
    match p.value(key)? {
        Value::Object(o) => {
            let path = o.get("$asset").and_then(Value::as_str).unwrap_or("");
            let url = asset_url(path);
            if url.is_none() {
                warn(&format!("gallery: unknown asset {path}"));
            }
            url.map(String::from)
        }
        Value::String(s) => Some(s),
        _ => None,
    }
}

/// Elements that may render more than once (layers mount and unmount their content).
fn children_fn(p: &Props, key: &str) -> Option<ChildrenFn> {
    let v = p.value(key)?;
    Some(Arc::new(move || node(&v)))
}

/// A slot whose content is fixed.
fn maybe_view(p: &Props, key: &str) -> Option<MaybeView> {
    let v = p.value(key)?;
    Some(Arc::new(move || Some(node(&v))))
}

fn list(p: &Props, key: &str) -> Vec<Value> {
    match p.value(key) {
        Some(Value::Array(a)) => a,
        Some(other) => vec![other],
        None => Vec::new(),
    }
}

fn select_option(o: &Value) -> Option<SelectOption> {
    match o {
        Value::String(s) => Some(SelectOption::from(s.as_str())),
        Value::Object(o) => Some(SelectOption {
            value: field_of(o, "value").unwrap_or_default(),
            label: field_of(o, "label").unwrap_or_default(),
            code: field_of(o, "code"),
            meta: field_of(o, "meta"),
            badge: field_of(o, "badge"),
            group: field_of(o, "group"),
            disabled: flag_of(o, "disabled"),
        }),
        _ => None,
    }
}

fn select_options(p: &Props, key: &str) -> Vec<SelectOption> {
    list(p, key).iter().filter_map(select_option).collect()
}

fn strings(p: &Props, key: &str) -> Vec<String> {
    list(p, key).iter().filter_map(text_of).collect()
}

/// A number the component takes as a signal, with React's default when absent.
fn num_or(p: &Props, key: &str, default: f64) -> Signal<f64> {
    let v = p.num_signal(key);
    Signal::derive(move || v.get().unwrap_or(default))
}

/// A controlled text value: present means controlled.
fn controlled(p: &Props, key: &str) -> Option<Signal<String>> {
    p.has(key).then(|| {
        let v = p.str_signal(key);
        Signal::derive(move || v.get().unwrap_or_default())
    })
}

// ---------------------------------------------------------------- primitives

fn text(p: Props) -> AnyView {
    Text(TextProps {
        role: p.keyword("role").unwrap_or_default(),
        tag: p.str("as").map(Into::into),
        tone: p.keyword("tone").unwrap_or_default(),
        script: p.keyword("script"),
        align: p.str("align"),
        truncate: p.flag("truncate"),
        trim: p.bool("trim"),
        measure: measure(&p, "measure"),
        dir: p.str("dir"),
        style: Signal::from(p.css("style")),
        children: p.children("children"),
        attrs: p.rest(),
    })
    .into_any()
}

fn stack(p: Props) -> AnyView {
    Stack(StackProps {
        direction: p.keyword("direction").unwrap_or_default(),
        gap: css_value(p.value("gap")),
        align: p.keyword("align"),
        justify: p.keyword("justify"),
        wrap: p.flag("wrap"),
        inline: p.flag("inline"),
        columns: columns(&p, "columns"),
        flex: css_value(p.value("flex")),
        grow: p.flag("grow"),
        tag: p.str("as").map(Into::into),
        on_click: handler(&p, "onClick"),
        style: p.css_signal("style"),
        children: p.children("children"),
        attrs: p.rest(),
    })
    .into_any()
}

fn surface(p: Props) -> AnyView {
    Surface(SurfaceProps {
        tag: p.str("as").map(Into::into),
        tone: p.keyword("tone"),
        fill: Signal::from(p.keyword::<Fill>("fill").unwrap_or_default()),
        border: p.str("border").map(|b| Border::parse(&b)),
        radius: radius(&p, "radius"),
        elevation: p.str("elevation").map(|e| Elevation::parse(&e)),
        pad: pad(&p, "pad"),
        interactive: p.keyword("interactive"),
        selected: p.flag_signal("selected"),
        disabled: p.flag_signal("disabled"),
        on: p.keyword("on"),
        on_click: handler(&p, "onClick"),
        on_pointer_down_capture: handler(&p, "onPointerDownCapture"),
        on_mouse_enter: handler(&p, "onMouseEnter"),
        on_mouse_leave: handler(&p, "onMouseLeave"),
        style: p.css_signal("style"),
        children: p.children("children"),
        attrs: p.rest(),
    })
    .into_any()
}

fn action(p: Props) -> AnyView {
    Action(ActionProps {
        weight: p.keyword("weight").unwrap_or_default(),
        tone: p.keyword("tone"),
        size: p.keyword("size").unwrap_or_default(),
        shape: p.keyword("shape").unwrap_or_default(),
        icon_only: p.flag("iconOnly"),
        title: p.str_signal("title"),
        disabled: p.flag_signal("disabled"),
        aria_busy: p.flag_signal("aria-busy"),
        tag: p.str("as").map(Into::into),
        href: p.str("href"),
        on_click: handler(&p, "onClick"),
        on_pointer_down_capture: handler(&p, "onPointerDownCapture"),
        style: p.css_signal("style"),
        children: p.children("children"),
        attrs: p.rest(),
        live_attrs: Vec::new(),
    })
    .into_any()
}

fn rule(p: Props) -> AnyView {
    Rule(RuleProps {
        orientation: p.keyword("orientation").unwrap_or_default(),
        label: p.str("label"),
        weight: p.keyword("weight").unwrap_or_default(),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn progress(p: Props) -> AnyView {
    let d = ProgressProps::default();
    Progress(ProgressProps {
        value: p.num_signal("value"),
        shape: p.keyword("shape").unwrap_or_default(),
        tone: p.keyword("tone").unwrap_or_default(),
        height: p.num("height").unwrap_or(d.height),
        size: p.num("size").unwrap_or(d.size),
        label: p.str("label"),
        min_fill: None,
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn choice(p: Props) -> AnyView {
    Choice(ChoiceProps {
        mark: p.keyword("mark").unwrap_or_default(),
        size: p.keyword("size").unwrap_or_default(),
        checked: p.flag_signal("checked"),
        indeterminate: p.flag_signal("indeterminate"),
        disabled: MaybeProp::from(p.bool("disabled")),
        tone: p.keyword("tone").unwrap_or_default(),
        value: p.str("value"),
        name: p.str("name"),
        on_change: handler(&p, "onChange"),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn field(p: Props) -> AnyView {
    Field(FieldProps {
        label: p.str("label").map(Signal::from),
        hint: p.str("hint").map(Signal::from),
        error: p.str("error"),
        live_error: None,
        required: p.flag("required"),
        optional: p.flag("optional"),
        disabled: p.bool("disabled"),
        size: p.keyword("size").unwrap_or_default(),
        placement: p.keyword("placement").unwrap_or_default(),
        id: p.str("id"),
        name: p.str("name"),
        style: p.css("style"),
        children: p.children("children"),
        attrs: p.rest(),
    })
    .into_any()
}

fn field_group(p: Props) -> AnyView {
    FieldGroup(FieldGroupProps {
        legend: p.str("legend"),
        hint: p.str("hint"),
        error: p.str("error"),
        required: p.flag("required"),
        disabled: p.bool("disabled"),
        name: p.str("name"),
        role: p.str("role"),
        direction: p.keyword("direction").unwrap_or_default(),
        gap: css_value(p.value("gap")),
        columns: columns(&p, "columns"),
        style: p.css("style"),
        children: p.children("children"),
        attrs: p.rest(),
    })
    .into_any()
}

fn input(p: Props) -> AnyView {
    let d = InputProps::default();
    Input(InputProps {
        tag: p.keyword("as"),
        variant: p.keyword("variant").unwrap_or_default(),
        size: p.keyword("size"),
        leading: p.children("leading"),
        trailing: maybe_view(&p, "trailing"),
        rows: p.num("rows").map_or(d.rows, |n| n as u32),
        invalid: MaybeProp::from(p.bool("invalid")),
        disabled: MaybeProp::from(p.bool("disabled")),
        value: controlled(&p, "value"),
        default_value: p.str("defaultValue"),
        placeholder: p.str("placeholder"),
        on_change: handler(&p, "onChange"),
        on_key_down: handler(&p, "onKeyDown"),
        on_blur: handler(&p, "onBlur"),
        style: p.css("style"),
        input_style: p.css("inputStyle"),
        children: p.children("children"),
        live_attrs: Vec::new(),
        attrs: p.rest(),
    })
    .into_any()
}

fn layer(p: Props) -> AnyView {
    let Some(children) = children_fn(&p, "children") else {
        warn("gallery: Layer without children");
        return ().into_any();
    };
    let d = LayerProps::new(children.clone());
    Layer(LayerProps {
        open: Some(if p.has("open") {
            p.flag_signal("open")
        } else {
            Signal::from(true)
        }),
        level: p.keyword("level").unwrap_or_default(),
        scrim: p.str("scrim").map(|s| Scrim::parse(&s)),
        placement: p.keyword("placement").unwrap_or_default(),
        anchor_to: None,
        offset: p.num("offset").unwrap_or(d.offset),
        align: p.keyword("align").unwrap_or_default(),
        side: p.keyword("side").unwrap_or_default(),
        dismiss: p.str("dismiss").map_or(d.dismiss, |x| Dismiss::parse(&x)),
        trap_focus: p.flag("trapFocus"),
        lock_scroll: p.flag("lockScroll"),
        restore_focus: p.bool("restoreFocus").unwrap_or(d.restore_focus),
        on_dismiss: live_handler(&p, "onDismiss"),
        navigate: p.keyword("navigate"),
        animate: p.bool("animate").unwrap_or(d.animate),
        scrim_attrs: Attrs::new(),
        scrim_live: Vec::new(),
        role: p.str("role"),
        label: p.str("label").map(Signal::from),
        labelled_by: p.str("labelledBy"),
        on_mouse_down: handler(&p, "onMouseDown"),
        on_key_down: handler(&p, "onKeyDown"),
        style: p.css("style"),
        children,
        attrs: p.rest(),
    })
    .into_any()
}

fn switcher(p: Props) -> AnyView {
    let options = list(&p, "options")
        .iter()
        .filter_map(Value::as_object)
        .map(|o| SwitcherOption {
            value: field_of(o, "value").unwrap_or_default(),
            label: field_of(o, "label").unwrap_or_default(),
            count: field_of(o, "count"),
            disabled: flag_in(o.get("disabled")),
        })
        .collect();
    Switcher(SwitcherProps {
        options,
        value: p.str_signal("value"),
        on_change: handler(&p, "onChange"),
        indicator: p.keyword("indicator").unwrap_or_default(),
        tone: p.keyword("tone").unwrap_or_default(),
        size: p.keyword("size").unwrap_or_default(),
        label: p.str("label"),
        grow: p.flag("grow"),
        tag: p.str("as").map(Into::into),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

/// A table row from the case: an object of fields.
#[derive(Clone, PartialEq)]
struct JsonRow(Map<String, Value>);

impl Record for JsonRow {
    fn field(&self, key: &str) -> Option<String> {
        self.0.get(key).and_then(text_of)
    }
}

fn table(p: Props) -> AnyView {
    let columns = list(&p, "columns")
        .iter()
        .filter_map(Value::as_object)
        .map(|o| Column::<JsonRow> {
            key: field_of(o, "key").unwrap_or_default(),
            header: field_of(o, "header").unwrap_or_default(),
            align: o
                .get("align")
                .and_then(Value::as_str)
                .and_then(ColumnAlign::parse)
                .unwrap_or_default(),
            width: css_value(o.get("width").cloned()),
            wrap: flag_of(o, "wrap"),
            row_header: o.get("rowHeader").and_then(Value::as_bool) != Some(false),
            cell: None,
        })
        .collect();
    let empty = p.value("empty").map(|v| ViewFn::from(move || node(&v)));
    Table(TableProps {
        columns,
        rows: p.list_signal("rows", |r| r.as_object().cloned().map(JsonRow)),
        caption: p.str("caption"),
        caption_visible: p.flag("captionVisible"),
        row_key: None,
        on_row_click: handler(&p, "onRowClick"),
        selected_key: p.str_signal("selectedKey"),
        empty,
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

// ---------------------------------------------------------------- core

fn badge(p: Props) -> AnyView {
    Badge(BadgeProps {
        tone: p.keyword("tone").unwrap_or_default(),
        size: p.keyword("size").unwrap_or_default(),
        style: p.css("style"),
        children: p.children("children"),
        attrs: p.rest(),
    })
    .into_any()
}

fn button(p: Props) -> AnyView {
    Button(ButtonProps {
        variant: p.keyword("variant").unwrap_or_default(),
        size: p.keyword("size").unwrap_or_default(),
        shape: p.keyword("shape").unwrap_or_default(),
        disabled: p.flag_signal("disabled"),
        title: p.str_signal("title"),
        aria_busy: p.flag_signal("aria-busy"),
        on_click: handler(&p, "onClick"),
        on_pointer_down_capture: handler(&p, "onPointerDownCapture"),
        style: p.css_signal("style"),
        children: p.children("children"),
        attrs: p.rest(),
    })
    .into_any()
}

fn divider(p: Props) -> AnyView {
    Divider(DividerProps {
        orientation: p.keyword("orientation").unwrap_or_default(),
        label: p.str("label"),
        inverse: p.flag("inverse"),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn icon_button(p: Props) -> AnyView {
    IconButton(IconButtonProps {
        variant: p.keyword("variant").unwrap_or_default(),
        size: p.num("size"),
        title: p.str("title"),
        disabled: p.flag_signal("disabled"),
        on_click: handler(&p, "onClick"),
        style: p.css("style"),
        children: p.children("children"),
        attrs: p.rest(),
    })
    .into_any()
}

fn overline(p: Props) -> AnyView {
    Overline(OverlineProps {
        tone: p.keyword("tone").unwrap_or_default(),
        tag: p.str("as").map(Into::into),
        style: p.css("style"),
        children: p.children("children"),
        attrs: p.rest(),
    })
    .into_any()
}

fn progress_bar(p: Props) -> AnyView {
    let d = ProgressBarProps::default();
    ProgressBar(ProgressBarProps {
        value: num_or(&p, "value", 0.0),
        tone: p.keyword("tone").unwrap_or_default(),
        height: p.num("height").unwrap_or(d.height),
        label: None,
        min_fill: None,
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn status_dot(p: Props) -> AnyView {
    let d = StatusDotProps::default();
    StatusDot(StatusDotProps {
        status: Signal::from(p.keyword::<Status>("status").unwrap_or_default()),
        size: p.num("size").unwrap_or(d.size),
        label: p.str("label"),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

// ---------------------------------------------------------------- data

fn accordion(p: Props) -> AnyView {
    let items = list(&p, "items")
        .iter()
        .filter_map(Value::as_object)
        .map(|o| AccordionItem {
            key: field_of(o, "key").unwrap_or_default(),
            title: field_of(o, "title").unwrap_or_default(),
            meta: field_of(o, "meta"),
            content: field_of(o, "content").unwrap_or_default(),
        })
        .collect();
    Accordion(AccordionProps {
        items,
        default_open: strings(&p, "defaultOpen"),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn avatar(p: Props) -> AnyView {
    let d = AvatarProps::default();
    Avatar(AvatarProps {
        initials: p.str("initials").unwrap_or_default(),
        size: p.num("size").unwrap_or(d.size),
        tone: p.keyword("tone").unwrap_or_default(),
        shape: p.keyword("shape").unwrap_or_default(),
        src: asset(&p, "src"),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn key_value_grid(p: Props) -> AnyView {
    let items = list(&p, "items")
        .iter()
        .filter_map(Value::as_object)
        .map(|o| KeyValue {
            k: field_of(o, "k").unwrap_or_default(),
            v: field_of(o, "v").unwrap_or_default(),
        })
        .collect();
    let d = KeyValueGridProps::default();
    KeyValueGrid(KeyValueGridProps {
        items,
        columns: p.num("columns").map_or(d.columns, |n| n as u32),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

// ---------------------------------------------------------------- feedback

fn confirm_dialog(p: Props) -> AnyView {
    let props = ConfirmDialogProps {
        open: p.flag_signal("open"),
        title: p.str("title").unwrap_or_default(),
        description: p.str("description"),
        consequence: p.str("consequence"),
        confirm_label: p.str("confirmLabel"),
        cancel_label: p.str("cancelLabel"),
        destructive: p.flag("destructive"),
        on_confirm: handler(&p, "onConfirm"),
        on_cancel: handler(&p, "onCancel"),
    };
    unread(&p);
    ConfirmDialog(props).into_any()
}

/// Warns about props a component without `...rest` was given and ignores.
fn unread(p: &Props) {
    let rest = p.rest();
    for (k, _) in rest.iter() {
        warn(&format!(
            "gallery: prop {k} is not a prop of this component"
        ));
    }
}

fn empty_state(p: Props) -> AnyView {
    EmptyState(EmptyStateProps {
        title: p.str("title"),
        description: p.str("description"),
        action: p.children("action"),
        variant: p.keyword("variant").unwrap_or_default(),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn inline_error(p: Props) -> AnyView {
    InlineError(InlineErrorProps {
        style: p.css("style"),
        children: p.children("children"),
        attrs: p.rest(),
    })
    .into_any()
}

fn spinner(p: Props) -> AnyView {
    let tone = match p.str("tone").as_deref() {
        Some("inverse") => SpinnerTone::Inverse,
        Some(t) => SpinnerTone::Tone(Tone::parse(t).unwrap_or_default()),
        None => SpinnerTone::default(),
    };
    let d = SpinnerProps::default();
    Spinner(SpinnerProps {
        size: p.num("size").unwrap_or(d.size),
        tone,
        label: p.str("label"),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn toast(p: Props) -> AnyView {
    Toast(ToastProps {
        tone: p.keyword("tone").unwrap_or_default(),
        message: p.str("message").unwrap_or_default(),
        action: handler(&p, "action"),
        action_label: p.str("actionLabel"),
        on_dismiss: handler(&p, "onDismiss"),
        dismiss_label: p.str("dismissLabel"),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn tooltip(p: Props) -> AnyView {
    let children = p
        .children("children")
        .unwrap_or_else(|| Box::new(|| ().into_any()));
    let props = TooltipProps {
        label: p.str("label").unwrap_or_default(),
        placement: p.keyword("placement").unwrap_or_default(),
        style: p.css("style"),
        children,
    };
    unread(&p);
    Tooltip(props).into_any()
}

// ---------------------------------------------------------------- forms

fn checkbox(p: Props) -> AnyView {
    Checkbox(CheckboxProps {
        checked: p.flag_signal("checked"),
        indeterminate: p.flag_signal("indeterminate"),
        disabled: p.bool("disabled"),
        label: p.str("label"),
        description: p.str("description"),
        on_change: handler(&p, "onChange"),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn drop_zone(p: Props) -> AnyView {
    DropZone(DropZoneProps {
        title: p.str("title").into(),
        hint: p.str("hint"),
        disabled: p.flag_signal("disabled"),
        on_click: handler(&p, "onClick"),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn filter_chip(p: Props) -> AnyView {
    FilterChip(FilterChipProps {
        selected: p.flag_signal("selected"),
        mark: p.str("mark"),
        tone: p.keyword("tone").unwrap_or_default(),
        on_click: handler(&p, "onClick"),
        style: p.css("style"),
        children: children_fn(&p, "children"),
        attrs: p.rest(),
    })
    .into_any()
}

fn option_card(p: Props) -> AnyView {
    OptionCard(OptionCardProps {
        selected: p.flag_signal("selected"),
        control: p.keyword("control").unwrap_or_default(),
        icon: p.content("icon"),
        title: p.content("title"),
        description: p.content("description"),
        meta: p.content("meta"),
        recommended: p.flag("recommended"),
        recommended_label: p.str("recommendedLabel"),
        trailing: p.content("trailing"),
        disabled: p.flag("disabled"),
        on_click: handler(&p, "onClick"),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn radio(p: Props) -> AnyView {
    Radio(RadioProps {
        checked: p.flag_signal("checked"),
        disabled: p.bool("disabled"),
        label: p.str("label"),
        description: p.str("description"),
        on_change: handler(&p, "onChange"),
        name: p.str("name"),
        value: p.str("value"),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn radio_group(p: Props) -> AnyView {
    let options = list(&p, "options")
        .iter()
        .filter_map(|o| match o {
            Value::String(s) => Some(RadioOption::from(s.as_str())),
            Value::Object(o) => Some(RadioOption {
                value: field_of(o, "value").unwrap_or_default(),
                label: field_of(o, "label").unwrap_or_default(),
                description: field_of(o, "description"),
            }),
            _ => None,
        })
        .collect();
    RadioGroup(RadioGroupProps {
        options,
        value: p.str_signal("value"),
        on_change: handler(&p, "onChange"),
        legend: p.str("legend"),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn search_field(p: Props) -> AnyView {
    SearchField(SearchFieldProps {
        value: controlled(&p, "value"),
        on_change: handler(&p, "onChange"),
        on_clear: handler(&p, "onClear"),
        placeholder: p.str("placeholder").map(Signal::from),
        clear_label: p.str("clearLabel").map(Signal::from),
        disabled: MaybeProp::from(p.bool("disabled")),
        style: p.css("style"),
        live_attrs: Vec::new(),
        attrs: p.rest(),
    })
    .into_any()
}

fn select(p: Props) -> AnyView {
    Select(SelectProps {
        label: p.str("label").map(Signal::from),
        options: select_options(&p, "options"),
        hint: p.str("hint").map(Signal::from),
        id: p.str("id"),
        value: p.str_signal("value"),
        on_change: handler(&p, "onChange"),
        // A screen turns a Select off while it is drawn (tC4 #552), so the prop follows its cell.
        disabled: p.flag_signal("disabled").into(),
        search_placeholder: p.str("searchPlaceholder").map(Signal::from),
        no_matches_label: p.str("noMatchesLabel").map(Signal::from),
        clear_label: p.str("clearLabel").map(Signal::from),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn suggest_field(p: Props) -> AnyView {
    SuggestField(SuggestFieldProps {
        label: p.str("label"),
        hint: p.str("hint"),
        id: p.str("id"),
        suggestions: p.list_signal("suggestions", select_option),
        value: controlled(&p, "value"),
        on_change: handler(&p, "onChange"),
        on_choose: handler(&p, "onChoose"),
        placeholder: p.str("placeholder"),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn text_area(p: Props) -> AnyView {
    let d = TextAreaProps::default();
    TextArea(TextAreaProps {
        label: p.str("label").map(Signal::from),
        variant: p.keyword("variant").unwrap_or_default(),
        dir: p.str("dir"),
        rows: p.num("rows").map_or(d.rows, |n| n as u32),
        id: p.str("id"),
        invalid: MaybeProp::from(p.bool("invalid")),
        disabled: MaybeProp::from(p.bool("disabled")),
        value: controlled(&p, "value"),
        default_value: p.str("defaultValue"),
        placeholder: p.str("placeholder"),
        on_change: handler(&p, "onChange"),
        on_blur: handler(&p, "onBlur"),
        input_style: p.css("inputStyle"),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn text_field(p: Props) -> AnyView {
    TextField(TextFieldProps {
        label: p.str("label").map(Signal::from),
        hint: p.str("hint").map(Signal::from),
        invalid: MaybeProp::from(p.bool("invalid")),
        id: p.str("id"),
        size: p.keyword("size"),
        disabled: MaybeProp::from(p.bool("disabled")),
        value: controlled(&p, "value"),
        default_value: p.str("defaultValue"),
        placeholder: p.str("placeholder"),
        on_change: handler(&p, "onChange"),
        on_key_down: handler(&p, "onKeyDown"),
        on_blur: handler(&p, "onBlur"),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn toggle(p: Props) -> AnyView {
    Toggle(ToggleProps {
        checked: p.flag_signal("checked"),
        on_change: handler(&p, "onChange"),
        label: p.str("label"),
        disabled: p.bool("disabled"),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

// ---------------------------------------------------------------- navigation

fn app_header(p: Props) -> AnyView {
    let props = AppHeaderProps {
        tone: p.keyword("tone").unwrap_or_default(),
        logo_src: asset(&p, "logoSrc"),
        project_initials: p.str("projectInitials"),
        project_name: p.str("projectName"),
        project_name_dir: p.str("projectNameDir"),
        project_meta: p.str("projectMeta"),
        switch_title: p.str("switchTitle"),
        center: p.children("center"),
        right: p.children("right"),
        on_brand_click: p.screen_handler("onBrandClick"),
        on_project_click: p.screen_handler("onProjectClick"),
        // tC4 takes the handlers away (`onBrandClick={editing ? undefined : exit}`); the port
        // keeps them and turns them off.
        exits_off: {
            let (brand, project) = (p.handler_on("onBrandClick"), p.handler_on("onProjectClick"));
            let conditional = p.is_if("onBrandClick") || p.is_if("onProjectClick");
            Signal::derive(move || conditional && !(brand.get() && project.get()))
        },
    };
    unread(&p);
    AppHeader(props).into_any()
}

fn breadcrumb(p: Props) -> AnyView {
    let items = list(&p, "items")
        .iter()
        .filter_map(Value::as_object)
        .map(|o| Crumb {
            label: field_of(o, "label").unwrap_or_default(),
            on_click: o
                .get("onClick")
                .filter(|v| v.get("$fn").is_some())
                .map(|_| Callback::new(|_| {})),
        })
        .collect();
    Breadcrumb(BreadcrumbProps {
        items,
        label: p.str("label"),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn menu(p: Props) -> AnyView {
    let items = list(&p, "items")
        .iter()
        .filter_map(Value::as_object)
        .map(|o| MenuItem {
            label: field_of(o, "label").unwrap_or_default(),
            meta: field_of(o, "meta"),
            on_click: o
                .get("onClick")
                .filter(|v| v.get("$fn").is_some())
                .map(|_| Callback::new(|_| {})),
            disabled: flag_of(o, "disabled"),
            destructive: flag_of(o, "destructive"),
            divider: flag_of(o, "divider"),
        })
        .collect();
    let trigger = p
        .children("trigger")
        .unwrap_or_else(|| Box::new(|| ().into_any()));
    let props = MenuProps {
        trigger,
        items,
        align: p.keyword("align").unwrap_or_default(),
        open: p.has("open").then(|| p.flag_signal("open")),
        on_open_change: handler(&p, "onOpenChange"),
        style: p.css("style"),
    };
    unread(&p);
    Menu(props).into_any()
}

// ---------------------------------------------------------------- surfaces

fn book_tile(p: Props) -> AnyView {
    BookTile(BookTileProps {
        name: p.str("name").unwrap_or_default().into(),
        percent: num_or(&p, "percent", 0.0),
        meta: p.str("meta").into(),
        meta_title: p.str("metaTitle"),
        layout: p.keyword("layout").unwrap_or_default(),
        active: p.flag_signal("active"),
        disabled: p.flag_signal("disabled"),
        on_click: handler(&p, "onClick"),
        on_pointer_down_capture: handler(&p, "onPointerDownCapture"),
        style: p.css_signal("style"),
        children: p.children("children"),
        attrs: p.rest(),
    })
    .into_any()
}

fn callout(p: Props) -> AnyView {
    Callout(CalloutProps {
        tone: p.keyword("tone").unwrap_or_default(),
        style: p.css("style"),
        children: p.children("children"),
        attrs: p.rest(),
    })
    .into_any()
}

fn card(p: Props) -> AnyView {
    Card(CardProps {
        variant: p.keyword("variant").unwrap_or_default(),
        interactive: p.flag("interactive"),
        padding: pad(&p, "padding"),
        on_click: handler(&p, "onClick"),
        style: p.css("style"),
        children: p.children("children"),
        attrs: p.rest(),
    })
    .into_any()
}

fn drawer(p: Props) -> AnyView {
    Drawer(DrawerProps {
        open: p.has("open").then(|| p.flag_signal("open")),
        eyebrow: p.content("eyebrow"),
        title: p.content("title"),
        title_dir: p.str("titleDir"),
        width: css_value(p.value("width")),
        on_close: handler(&p, "onClose"),
        close_label: p.str("closeLabel").map(Signal::from),
        children: children_fn(&p, "children"),
        attrs: p.rest(),
    })
    .into_any()
}

fn help_card(p: Props) -> AnyView {
    HelpCard(HelpCardProps {
        kind: p.keyword("kind").unwrap_or_default(),
        verse: p.str("verse"),
        title: p.str("title"),
        body: p.children("body"),
        active: p.flag_signal("active"),
        action_label: p.str("actionLabel"),
        on_action: handler(&p, "onAction"),
        on_click: handler(&p, "onClick"),
        on_mouse_enter: handler(&p, "onMouseEnter"),
        on_mouse_leave: handler(&p, "onMouseLeave"),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn modal(p: Props) -> AnyView {
    // `zIndex` is accepted for old call sites and ignored, as in tC4.
    let _ = p.value("zIndex");
    Modal(ModalProps {
        open: p.has("open").then(|| p.flag_signal("open")),
        title: p.content("title"),
        subtitle: p.content("subtitle"),
        width: css_value(p.value("width")),
        close_label: p.str("closeLabel").map(Signal::from),
        on_close: p.screen_handler("onClose"),
        closable: p.handler_on("onClose"),
        footer: children_fn(&p, "footer"),
        footer_shown: None,
        children: children_fn(&p, "children"),
        attrs: p.rest(),
        live_attrs: Vec::new(),
    })
    .into_any()
}

// ---------------------------------------------------------------- translation

fn scripture_passage(p: Props) -> AnyView {
    let verses = list(&p, "verses")
        .iter()
        .filter_map(Value::as_object)
        .map(|o| Verse {
            n: field_of(o, "n").unwrap_or_default(),
            text: field_of(o, "text").unwrap_or_default(),
        })
        .collect();
    let d = ScripturePassageProps::default();
    let props = ScripturePassageProps {
        verses,
        dir: p.str("dir"),
        script: p.keyword("script").unwrap_or_default(),
        step: p.keyword("step").unwrap_or(d.step),
        size: css_value(p.value("size")),
        highlight: strings(&p, "highlight"),
        style: p.css("style"),
    };
    unread(&p);
    ScripturePassage(props).into_any()
}

fn verse_marker(p: Props) -> AnyView {
    let d = VerseMarkerProps::new(p.num("n").map_or(0, |n| n as u32));
    VerseMarker(VerseMarkerProps {
        n: d.n,
        size: p.num("size").unwrap_or(d.size),
        state: p.keyword("state").unwrap_or_default(),
        on_pick_up: handler(&p, "onPickUp"),
        on_pointer_up: None,
        label: p.str("label"),
        style: p.css("style"),
        attrs: p.rest(),
    })
    .into_any()
}

fn word_chip(p: Props) -> AnyView {
    WordChip(WordChipProps {
        state: p.keyword("state").unwrap_or_default(),
        style: p.css("style"),
        children: p.children("children"),
        attrs: p.rest(),
    })
    .into_any()
}
