//! Layer.jsx: everything that renders above the page: modals, drawers, menus, tooltips, toast
//! stacks. Layer owns what those share and nothing else: stacking depth, a scrim, dismissal,
//! focus trapping and restoration, scroll locking, anchored placement with a flip, an
//! enter/exit transition and optional roving keyboard navigation.
//!
//! It renders in place. tC4's Layer portals only when `window.ReactDOM` is set, which tC4
//! never does, so its layers render where they are declared too; fixed positioning still
//! takes them out of flow.

use std::cell::{Cell, RefCell};

use leptos::ev::MouseEvent;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;

use crate::css::js_number;
use crate::ds::{
    Attrs, Css, LayerAlign, LayerPlacement, Level, Navigate, Scrim, ScrimToken, Side, live,
    provide, spread,
};
use crate::geometry::{Rect, place};

/// How deep a layer is nested in other layers. A menu opened inside a drawer resolves its
/// z-index as popover + 10, so it sits above that drawer without either knowing the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LayerDepth(usize);

/// Why a layer asked to close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DismissReason {
    Escape,
    Outside,
    Scrim,
}

/// What dismisses a layer (React's `dismiss="scrim escape"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dismiss {
    pub scrim: bool,
    pub escape: bool,
    pub outside: bool,
}

impl Dismiss {
    /// None of the three: a tooltip.
    pub const NONE: Dismiss = Dismiss {
        scrim: false,
        escape: false,
        outside: false,
    };

    /// The words of tC4's `dismiss` string.
    pub fn parse(s: &str) -> Self {
        Self {
            scrim: s.contains("scrim"),
            escape: s.contains("escape"),
            outside: s.contains("outside"),
        }
    }
}

impl Default for Dismiss {
    fn default() -> Self {
        Self {
            scrim: true,
            escape: true,
            outside: false,
        }
    }
}

/// The anchor of an anchored layer: the element it opens beside.
pub type Anchor = Callback<(), Option<web_sys::Element>>;

const FOCUSABLE: &str = "a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex=\"-1\"])";

thread_local! {
    /// Open layers that close on Escape, by token and depth. Escape belongs to the innermost
    /// layer only; ordering is by nesting depth, not registration time.
    static ESCAPE: RefCell<Vec<(u64, usize)>> = const { RefCell::new(Vec::new()) };
    /// Open layers that close on an outside press.
    static OUTSIDE: RefCell<Vec<(u64, usize)>> = const { RefCell::new(Vec::new()) };
    static TOKENS: Cell<u64> = const { Cell::new(0) };
    static PANEL_MS: Cell<Option<i32>> = const { Cell::new(None) };
}

fn token() -> u64 {
    TOKENS.with(|t| {
        t.set(t.get() + 1);
        t.get()
    })
}

/// Whether `token` is the innermost open escape layer: the deepest, and among equal depths the
/// most recently opened.
fn is_innermost(token: u64) -> bool {
    ESCAPE.with(|s| {
        let s = s.borrow();
        let Some(max) = s.iter().map(|(_, d)| *d).max() else {
            return false;
        };
        s.iter()
            .rev()
            .find(|(_, d)| *d == max)
            .is_some_and(|(t, _)| *t == token)
    })
}

/// `--dur-panel` in ms, read once from the stylesheet, which stays the source of truth.
fn exit_duration() -> i32 {
    if let Some(ms) = PANEL_MS.with(Cell::get) {
        return ms;
    }
    let v = document()
        .document_element()
        .and_then(|el| window().get_computed_style(&el).ok().flatten())
        .and_then(|s| s.get_property_value("--dur-panel").ok())
        .unwrap_or_default();
    let v = v.trim();
    let ms = if let Some(n) = v.strip_suffix("ms") {
        n.trim().parse::<f64>().ok()
    } else if let Some(n) = v.strip_suffix('s') {
        n.trim().parse::<f64>().ok().map(|s| s * 1000.0)
    } else {
        None
    }
    .filter(|ms| *ms > 0.0)
    .unwrap_or(240.0) as i32;
    PANEL_MS.with(|c| c.set(Some(ms)));
    ms
}

fn reduced_motion() -> bool {
    window()
        .match_media("(prefers-reduced-motion: reduce)")
        .ok()
        .flatten()
        .is_some_and(|m| m.matches())
}

/// React's state setter: a value equal to the current one changes nothing, so the panel is not
/// drawn again (which would drop the focus a dialog just gave its first control).
fn settle<T: PartialEq + Send + Sync + 'static>(signal: RwSignal<T>, value: T) {
    if signal.with_untracked(|v| *v != value) {
        signal.set(value);
    }
}

/// Whether two DOM objects are the same node (`===`).
fn same(a: &impl AsRef<wasm_bindgen::JsValue>, b: &impl AsRef<wasm_bindgen::JsValue>) -> bool {
    a.as_ref() == b.as_ref()
}

fn focusables(panel: &web_sys::Element) -> Vec<web_sys::HtmlElement> {
    let Ok(list) = panel.query_selector_all(FOCUSABLE) else {
        return Vec::new();
    };
    (0..list.length())
        .filter_map(|i| list.item(i))
        .filter_map(|n| n.dyn_into::<web_sys::HtmlElement>().ok())
        .filter(|el| el.offset_parent().is_some())
        .collect()
}

/// A document listener removed when the effect that added it is cleaned up.
fn listen(kind: &'static str, capture: bool, f: impl FnMut(web_sys::Event) + 'static) {
    let closure = Closure::<dyn FnMut(web_sys::Event)>::new(f);
    let doc = document();
    let _ = doc.add_event_listener_with_callback_and_bool(
        kind,
        closure.as_ref().unchecked_ref(),
        capture,
    );
    let closure = send_wrapper::SendWrapper::new(closure);
    on_cleanup(move || {
        let closure = closure.take();
        let _ = document().remove_event_listener_with_callback_and_bool(
            kind,
            closure.as_ref().unchecked_ref(),
            capture,
        );
    });
}

#[component]
pub fn Layer(
    /// Shown while true; tC4's default is open.
    #[prop(optional, into)]
    open: Option<Signal<bool>>,
    #[prop(optional)] level: Level,
    #[prop(optional)] scrim: Option<Scrim>,
    #[prop(optional)] placement: LayerPlacement,
    #[prop(optional)] anchor_to: Option<Anchor>,
    #[prop(default = 6.0)] offset: f64,
    #[prop(optional)] align: LayerAlign,
    #[prop(optional)] side: Side,
    #[prop(optional)] dismiss: Dismiss,
    #[prop(optional)] trap_focus: bool,
    #[prop(optional)] lock_scroll: bool,
    #[prop(default = true)] restore_focus: bool,
    /// The handler in force now. tC4's dialogs pass none while an action runs
    /// (`onClose={busy ? undefined : close}`): Escape then passes through and a press on the
    /// scrim does nothing.
    #[prop(optional, into)]
    on_dismiss: Signal<Option<Callback<DismissReason>>>,
    #[prop(optional)] navigate: Option<Navigate>,
    #[prop(default = true)] animate: bool,
    /// Extra attributes of the scrim (React's `scrimProps`).
    #[prop(optional)]
    scrim_attrs: Attrs,
    /// Attributes of the scrim whose values follow signals; `None` removes one.
    #[prop(optional)]
    scrim_live: Vec<(&'static str, Signal<Option<String>>)>,
    #[prop(optional, into)] role: Option<String>,
    /// The panel's accessible name, which follows the shown language while the layer is on
    /// screen (React draws `label` again on every render).
    #[prop(optional, into)]
    label: Option<Signal<String>>,
    #[prop(optional, into)] labelled_by: Option<String>,
    /// A press on the panel (React passes `onMouseDown` through `...rest`).
    #[prop(optional)]
    on_mouse_down: Option<Callback<MouseEvent>>,
    #[prop(optional)] style: Css,
    /// Extra attributes of the panel.
    #[prop(optional)]
    attrs: Attrs,
    children: ChildrenFn,
) -> impl IntoView {
    let open = open.unwrap_or_else(|| Signal::from(true));
    let depth = use_context::<LayerDepth>().map_or(0, |d| d.0);
    let anchored = placement == LayerPlacement::Anchor;
    let panel = NodeRef::<leptos::html::Div>::new();
    let mounted = RwSignal::new(open.get_untracked());
    let shown = RwSignal::new(false);
    let rect = RwSignal::new(None::<Rect>);
    let panel_size = RwSignal::new(None::<(f64, f64)>);
    let rtl = RwSignal::new(false);
    let scrim_press = StoredValue::new(true);

    // Two flags: a layer asked to close stops behaving like an open one at once (no Escape, no
    // focus trap) while its node stays mounted long enough to animate out.
    Effect::new(move |_| {
        if open.get() {
            settle(mounted, true);
            if let Ok(h) = request_animation_frame_with_handle(move || settle(shown, true)) {
                on_cleanup(move || h.cancel());
            }
        } else {
            settle(shown, false);
            if !animate || reduced_motion() {
                settle(mounted, false);
            } else {
                let ms = exit_duration();
                let h = set_timeout_with_handle(
                    move || settle(mounted, false),
                    std::time::Duration::from_millis(ms as u64),
                );
                if let Ok(h) = h {
                    on_cleanup(move || h.clear());
                }
            }
        }
    });

    // Focus, Escape, outside presses, scroll lock and the focus trap, while open.
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        let returns = (restore_focus || trap_focus)
            .then(|| document().active_element())
            .flatten();
        if trap_focus && let Some(p) = panel.get_untracked() {
            let first = focusables(&p).into_iter().next();
            match first {
                Some(el) => {
                    let _ = el.focus();
                }
                None => {
                    let _ = p.focus();
                }
            }
        }
        if dismiss.escape {
            let t = token();
            ESCAPE.with(|s| s.borrow_mut().push((t, depth)));
            listen("keydown", true, move |e| {
                let Some(e) = e.dyn_ref::<web_sys::KeyboardEvent>() else {
                    return;
                };
                // No handler means nothing to dismiss: never swallow the key.
                let Some(cb) = on_dismiss.get_untracked() else {
                    return;
                };
                if e.key() != "Escape" || !is_innermost(t) {
                    return;
                }
                e.stop_immediate_propagation();
                e.prevent_default();
                cb.run(DismissReason::Escape);
            });
            on_cleanup(move || ESCAPE.with(|s| s.borrow_mut().retain(|(x, _)| *x != t)));
        }
        if dismiss.outside {
            let t = token();
            OUTSIDE.with(|s| s.borrow_mut().push((t, depth)));
            on_cleanup(move || OUTSIDE.with(|s| s.borrow_mut().retain(|(x, _)| *x != t)));
            listen("mousedown", false, move |e| {
                let target = e.target().and_then(|t| t.dyn_into::<web_sys::Node>().ok());
                if let Some(p) = panel.get_untracked()
                    && !p.contains(target.as_ref())
                    && let Some(cb) = on_dismiss.get_untracked()
                {
                    cb.run(DismissReason::Outside);
                }
            });
        }
        if lock_scroll && let Some(body) = document().body() {
            let prev = body
                .style()
                .get_property_value("overflow")
                .unwrap_or_default();
            let _ = body.style().set_property("overflow", "hidden");
            on_cleanup(move || {
                if let Some(body) = document().body() {
                    let _ = body.style().set_property("overflow", &prev);
                }
            });
        }
        if trap_focus {
            listen("keydown", true, move |e| {
                let Some(e) = e.dyn_ref::<web_sys::KeyboardEvent>() else {
                    return;
                };
                let Some(p) = panel.get_untracked() else {
                    return;
                };
                if e.key() != "Tab" {
                    return;
                }
                let items = focusables(&p);
                let (Some(first), Some(last)) = (items.first(), items.last()) else {
                    return;
                };
                let active = document().active_element();
                let is = |el: &web_sys::HtmlElement| active.as_ref().is_some_and(|a| same(a, el));
                if e.shift_key() && is(first) {
                    e.prevent_default();
                    let _ = last.focus();
                } else if !e.shift_key() && is(last) {
                    e.prevent_default();
                    let _ = first.focus();
                }
            });
        }
        let returns = send_wrapper::SendWrapper::new(returns);
        on_cleanup(move || {
            if let Some(el) = returns.take()
                && let Ok(el) = el.dyn_into::<web_sys::HtmlElement>()
            {
                let _ = el.focus();
            }
        });
    });

    // Roving focus inside the panel: arrows, Home/End and typeahead, as role="menu" requires.
    Effect::new(move |_| {
        let (Some(nav), true) = (navigate, open.get()) else {
            return;
        };
        let Some(p) = panel.get() else { return };
        let node: web_sys::Element = p.clone().into();
        let vertical = nav == Navigate::Vertical;
        let handler =
            Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
                let list = focusables(&node);
                if list.is_empty() {
                    return;
                }
                let active = document().active_element();
                let i = list
                    .iter()
                    .position(|el| active.as_ref().is_some_and(|a| same(a, el)))
                    .map_or(-1, |i| i as i64);
                let n = list.len() as i64;
                let key = e.key();
                let target = if key == if vertical { "ArrowDown" } else { "ArrowRight" } {
                    if i < 0 { 0 } else { (i + 1) % n }
                } else if key == if vertical { "ArrowUp" } else { "ArrowLeft" } {
                    if i <= 0 { n - 1 } else { i - 1 }
                } else if key == "Home" {
                    0
                } else if key == "End" {
                    n - 1
                } else if key.chars().count() == 1
                    && !key.trim().is_empty()
                    && !e.meta_key()
                    && !e.ctrl_key()
                    && !e.alt_key()
                {
                    let c = key.to_lowercase();
                    let from = i.max(0);
                    let hit = (1..=n).map(|k| (from + k) % n).find(|&j| {
                        list[j as usize]
                            .text_content()
                            .unwrap_or_default()
                            .trim()
                            .to_lowercase()
                            .starts_with(&c)
                    });
                    match hit {
                        Some(j) => j,
                        None => return,
                    }
                } else {
                    return;
                };
                e.prevent_default();
                let _ = list[target as usize].focus();
            });
        let _ = p.add_event_listener_with_callback("keydown", handler.as_ref().unchecked_ref());
        let cleanup = send_wrapper::SendWrapper::new((p, handler));
        on_cleanup(move || {
            let (p, handler) = cleanup.take();
            let _ =
                p.remove_event_listener_with_callback("keydown", handler.as_ref().unchecked_ref());
        });
    });

    // Anchored layers measure twice: the trigger, for where to go, and the panel itself,
    // because a flip cannot be decided without knowing how tall it is.
    Effect::new(move |_| {
        if !mounted.get() || !anchored {
            return;
        }
        let Some(anchor) = anchor_to else { return };
        let measure = move || {
            let Some(el) = anchor.run(()) else { return };
            let r = el.get_bounding_client_rect();
            rect.set(Some(Rect {
                left: r.left(),
                top: r.top(),
                width: r.width(),
                height: r.height(),
            }));
            let dir = window()
                .get_computed_style(&el)
                .ok()
                .flatten()
                .and_then(|s| s.get_property_value("direction").ok())
                .unwrap_or_default();
            rtl.set(dir == "rtl");
            if let Some(p) = panel.get_untracked() {
                let b = p.get_bounding_client_rect();
                let (w, h) = (b.width(), b.height());
                let same = panel_size.with_untracked(|prev| {
                    prev.is_some_and(|(pw, ph)| (pw - w).abs() < 1.0 && (ph - h).abs() < 1.0)
                });
                if !same {
                    panel_size.set(Some((w, h)));
                }
            }
        };
        measure();
        let handler = Closure::<dyn FnMut()>::new(measure);
        let w = window();
        let _ = w.add_event_listener_with_callback("resize", handler.as_ref().unchecked_ref());
        let _ = w.add_event_listener_with_callback_and_bool(
            "scroll",
            handler.as_ref().unchecked_ref(),
            true,
        );
        // tC4 measures again whenever the layer's content is drawn anew (its effect depends on
        // `children`); here that is any change inside the panel. The panel's own attributes are
        // its position, which must not feed back into the measurement.
        let observer = panel.get_untracked().and_then(|p| {
            let p: web_sys::Node = p.into();
            let watched = p.clone();
            let on_change =
                Closure::<dyn FnMut(js_sys::Array)>::new(move |records: js_sys::Array| {
                    let inside = records.iter().any(|r| {
                        let r: web_sys::MutationRecord = r.unchecked_into();
                        r.target().is_some_and(|t| t != watched)
                    });
                    if inside {
                        measure();
                    }
                });
            let mo = web_sys::MutationObserver::new(on_change.as_ref().unchecked_ref()).ok()?;
            let opts = web_sys::MutationObserverInit::new();
            opts.set_child_list(true);
            opts.set_subtree(true);
            opts.set_character_data(true);
            opts.set_attributes(true);
            mo.observe_with_options(&p, &opts).ok()?;
            Some((mo, on_change))
        });
        let handler = send_wrapper::SendWrapper::new((handler, observer));
        on_cleanup(move || {
            let (handler, observer) = handler.take();
            let w = window();
            let _ =
                w.remove_event_listener_with_callback("resize", handler.as_ref().unchecked_ref());
            let _ = w.remove_event_listener_with_callback_and_bool(
                "scroll",
                handler.as_ref().unchecked_ref(),
                true,
            );
            if let Some((mo, _)) = observer {
                mo.disconnect();
            }
        });
    });

    let z = format!("calc(var(--z-{}) + {})", level.as_str(), depth * 10);
    let still = !animate || reduced_motion();
    let pos = move || {
        if !anchored {
            return None;
        }
        let (r, size) = (rect.get()?, panel_size.get()?);
        let vw = window()
            .inner_width()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let vh = window()
            .inner_height()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        Some(place(r, size, side, align, offset, rtl.get(), (vw, vh)))
    };
    let modal = matches!(role.as_deref(), Some("dialog" | "alertdialog"));
    let style = StoredValue::new(style);
    let panel_z = z.clone();
    let panel_style = move || {
        let pos = pos();
        let mut s = css! { "outline": "none", "box-sizing": "border-box", "max-width": "100%" };
        if anchored {
            s.extend(&css! {
                "position": "fixed",
                "z-index": panel_z.clone(),
                "top": pos.map_or(-9999.0, |p| p.top),
                "left": pos.map_or(0.0, |p| p.left),
                "visibility": pos.is_none().then_some("hidden"),
            });
        }
        if matches!(placement, LayerPlacement::Start | LayerPlacement::End) {
            s.set("height", "100%");
        }
        if !still {
            let dir = if document()
                .document_element()
                .map(|e| e.get_attribute("dir"))
                == Some(Some("rtl".into()))
            {
                -1.0
            } else {
                1.0
            };
            let away = if anchored {
                format!(
                    "translateY({}px)",
                    if pos.is_some_and(|p| p.side == Side::Top) {
                        4
                    } else {
                        -4
                    }
                )
            } else {
                match placement {
                    LayerPlacement::End => format!("translateX({}%)", js_number(100.0 * dir)),
                    LayerPlacement::Start => format!("translateX({}%)", js_number(-100.0 * dir)),
                    LayerPlacement::BottomStart | LayerPlacement::BottomEnd => {
                        "translateY(12px)".to_string()
                    }
                    _ => "scale(.97)".to_string(),
                }
            };
            let on = shown.get();
            s.extend(&css! {
                "opacity": if on { 1 } else { 0 },
                "transform": if on { "none".to_string() } else { away },
                "transition": "opacity var(--dur-panel) var(--ease-standard), transform var(--dur-panel) var(--ease-standard)",
            });
        }
        style.with_value(|st| s.extend(st));
        s.to_style()
    };
    let attrs = StoredValue::new(attrs);
    let panel_view = move || {
        let children = children.clone();
        let body = provide(LayerDepth(depth + 1), move || children());
        view! {
            <div
                node_ref=panel
                role=role.clone()
                tabindex="-1"
                aria-modal=modal.then_some("true")
                aria-label=move || label.map(|l| l.get())
                aria-labelledby=labelled_by.clone()
                data-layer-placement=anchored.then_some(placement.as_str())
                data-side=move || if anchored { pos().map(|p| p.side.as_str()) } else { None }
                on:click=|e: MouseEvent| e.stop_propagation()
                on:mousedown=move |e: MouseEvent| {
                    if let Some(cb) = on_mouse_down {
                        cb.run(e);
                    }
                }
                style=panel_style.clone()
                {..attrs.with_value(spread)}
            >
                {body}
            </div>
        }
    };

    if anchored {
        return (move || mounted.get().then(panel_view.clone())).into_any();
    }

    let bottom = matches!(
        placement,
        LayerPlacement::BottomStart | LayerPlacement::BottomEnd
    );
    let justify = match placement {
        LayerPlacement::Start | LayerPlacement::BottomStart => "flex-start",
        LayerPlacement::End | LayerPlacement::BottomEnd => "flex-end",
        _ => "center",
    };
    let scrim_bg = scrim.as_ref().map(|s| match s {
        Scrim::Token(ScrimToken::Modal) => "var(--scrim-modal)".to_string(),
        Scrim::Token(ScrimToken::Drawer) => "var(--scrim-drawer)".to_string(),
        Scrim::Css(c) => c.clone(),
    });
    let has_scrim = scrim_bg.is_some();
    let scrim_style = move || {
        let mut s = css! {
            "position": "fixed",
            "inset": 0,
            "z-index": z.clone(),
            "display": "flex",
            "justify-content": justify,
            "align-items": if bottom { "flex-end" } else if placement == LayerPlacement::Center { "center" } else { "stretch" },
            "padding": if placement == LayerPlacement::Center { 28 } else if bottom { 24 } else { 0 },
            "background": scrim_bg.clone(),
            "pointer-events": if has_scrim { "auto" } else { "none" },
        };
        if has_scrim && !still {
            s.extend(&css! {
                "opacity": if shown.get() { 1 } else { 0 },
                "transition": "opacity var(--dur-panel) var(--ease-standard)",
            });
        }
        s.to_style()
    };
    let scrim_attrs = StoredValue::new(scrim_attrs);
    let scrim_live = StoredValue::new(scrim_live);
    (move || {
        mounted.get().then(|| {
            view! {
                <div
                    {..scrim_attrs.with_value(spread)}
                    {..scrim_live.with_value(|l| live(l.clone()))}
                    data-layer-placement=placement.as_str()
                    on:mousedown=move |e: MouseEvent| {
                        // A press inside the panel is not a press on the scrim.
                        if e.target() != e.current_target() {
                            return;
                        }
                        // A press that will not close this layer (an inner layer takes it)
                        // keeps the focus where it is.
                        let deeper = OUTSIDE.with(|s| s.borrow().iter().any(|(_, d)| *d > depth));
                        scrim_press.set_value(!deeper);
                        if deeper {
                            e.prevent_default();
                        }
                    }
                    on:click=move |_| {
                        if dismiss.scrim
                            && let Some(cb) = on_dismiss.get_untracked()
                        {
                            let press = scrim_press.get_value();
                            scrim_press.set_value(true);
                            if press {
                                cb.run(DismissReason::Scrim);
                            }
                        }
                    }
                    style=scrim_style.clone()
                >
                    <div style="pointer-events: auto; display: flex; max-height: 100%; max-width: 100%;">
                        {panel_view()}
                    </div>
                </div>
            }
        })
    })
    .into_any()
}
