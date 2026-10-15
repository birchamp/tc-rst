//! Switcher.jsx: an exclusive row of options. Tabs and a segmented control are the same
//! object; `indicator` is how the selection is marked: an underline, a filled pill, or colour
//! alone.

use leptos::ev::{self, KeyboardEvent};
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::ds::{
    Attrs, Css, Indicator, Size, Tag, Text, TextRole, TextTone, Tone, element, spread,
};

/// One option of a Switcher.
#[derive(Debug, Clone, PartialEq)]
pub struct SwitcherOption {
    pub value: String,
    pub label: String,
    /// A count after the label (a number of notes, …).
    pub count: Option<String>,
    pub disabled: bool,
}

impl SwitcherOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            count: None,
            disabled: false,
        }
    }
}

fn by_size(size: Size, sm: &'static str, md: &'static str, lg: &'static str) -> &'static str {
    match size {
        Size::Sm => sm,
        Size::Md => md,
        Size::Lg => lg,
    }
}

/// Arrow keys move the selection, as role="tablist" and a radio group both require; Tab
/// enters and leaves the whole control, because only the selected option is tabbable.
#[component]
pub fn Switcher(
    #[prop(optional)] options: Vec<SwitcherOption>,
    #[prop(optional, into)] value: Signal<Option<String>>,
    #[prop(optional)] on_change: Option<Callback<String>>,
    #[prop(optional)] indicator: Indicator,
    #[prop(optional)] tone: Tone,
    #[prop(optional)] size: Size,
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional)] grow: bool,
    #[prop(optional, into)] tag: Option<Tag>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let pill = indicator == Indicator::Pill;
    let options = StoredValue::new(options);
    let idx = move || {
        let v = value.get();
        options.with_value(|o| o.iter().position(|x| Some(&x.value) == v.as_ref()))
    };
    let on_key_down = move |e: KeyboardEvent| {
        let n = options.with_value(Vec::len);
        let step: i64 = match e.key().as_str() {
            "ArrowRight" | "ArrowDown" => 1,
            "ArrowLeft" | "ArrowUp" => -1,
            "Home" => i64::MIN,
            "End" => i64::MAX,
            _ => 0,
        };
        if step == 0 || n == 0 {
            return;
        }
        e.prevent_default();
        let n = n as i64;
        let at = idx().map_or(-1, |i| i as i64);
        let next = match step {
            i64::MIN => 0,
            i64::MAX => n - 1,
            s => (at + s + n).rem_euclid(n),
        } as usize;
        let Some(opt) = options.with_value(|o| o.get(next).cloned()) else {
            return;
        };
        if opt.disabled {
            return;
        }
        if let Some(cb) = on_change {
            cb.run(opt.value.clone());
        }
        let tabs = e
            .current_target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
        if let Some(button) = tabs.and_then(|t| t.children().item(next as u32))
            && let Ok(button) = button.dyn_into::<web_sys::HtmlElement>()
        {
            let _ = button.focus();
        }
    };

    let mut outer =
        css! { "display": if pill { "inline-flex" } else { "flex" }, "box-sizing": "border-box" };
    if pill {
        outer.extend(&css! { "gap": 3, "padding": 3, "background": "var(--fill-quiet-hover)", "border-radius": "var(--radius-pill)" });
    } else {
        outer.extend(&css! { "gap": 2, "padding-inline": 14, "border-bottom": "var(--stroke-hair) solid var(--border-hair)" });
    }
    outer.extend(&style);

    let buttons = options.with_value(|opts| {
        opts.iter()
            .enumerate()
            .map(|(i, o)| {
                let v = o.value.clone();
                let on = {
                    let v = v.clone();
                    move || value.get().as_ref() == Some(&v)
                };
                let disabled = o.disabled;
                let item_style = {
                    let on = on.clone();
                    move || {
                        let on = on();
                        let mut s = css! {
                            "box-sizing": "border-box",
                            "font": "inherit",
                            "font-family": "var(--font-ui)",
                            "font-size": by_size(size, "var(--fs-caption)", "var(--fs-caption-lg)", "var(--fs-ui)"),
                            "font-weight": "var(--fw-heavy)",
                            "letter-spacing": by_size(size, "var(--track-12)", "var(--track-12-5)", "var(--track-13-5)"),
                            "display": "inline-flex",
                            "align-items": "center",
                            "justify-content": "center",
                            "gap": "var(--control-gap)",
                            "white-space": "nowrap",
                            "line-height": 1.2,
                            "cursor": if disabled { "default" } else { "pointer" },
                            "flex": if grow { "1 1 0" } else { "none" },
                        };
                        if pill {
                            s.extend(&css! {
                                "height": by_size(size, "var(--control-h-sm)", "var(--control-h-md)", "var(--control-h-lg)"),
                                "padding-inline": "var(--action-px-sm)",
                                "border": 0,
                                "border-radius": "var(--radius-pill)",
                                "background": if on { "var(--tone-fill)" } else { "transparent" },
                                "color": if on { "var(--tone-on-fill)" } else { "var(--fg-muted)" },
                                "box-shadow": if on { "var(--shadow-raised)" } else { "none" },
                            });
                        } else {
                            s.extend(&css! {
                                "padding": "14px 9px",
                                "border": 0,
                                "border-radius": 0,
                                "background": "transparent",
                                "color": if on { "var(--tone-text)" } else { "var(--fg-muted)" },
                                "box-shadow": if indicator == Indicator::None || !on {
                                    "none"
                                } else {
                                    "inset 0 calc(-1 * var(--stroke-control)) 0 0 var(--tone)"
                                },
                            });
                        }
                        if disabled {
                            s.set("color", "var(--disabled-fg)");
                        }
                        s.to_style()
                    }
                };
                let count = o.count.clone().map(|c| {
                    let on = on.clone();
                    move || {
                        let c = c.clone();
                        view! { <Text role=TextRole::LabelNum tone=if on() { TextTone::Tone } else { TextTone::Faint }>{c}</Text> }
                    }
                });
                let (on_sel, on_tab, on_i) = (on.clone(), on.clone(), on.clone());
                view! {
                    <button
                        type="button"
                        role="tab"
                        aria-selected=move || if on() { "true" } else { "false" }
                        tabindex=move || if on_tab() || (idx().is_none() && i == 0) { "0" } else { "-1" }
                        disabled=disabled
                        data-i=move || {
                            if disabled {
                                None
                            } else if pill {
                                Some(if on_i() { "fill" } else { "quiet" })
                            } else {
                                Some("text")
                            }
                        }
                        data-selected=move || on_sel().then_some("true")
                        on:click=move |_| {
                            if !disabled
                                && let Some(cb) = on_change {
                                    cb.run(v.clone());
                                }
                        }
                        style=item_style
                    >
                        {o.label.clone()}
                        {count}
                    </button>
                }
            })
            .collect_view()
    });

    element(tag.unwrap_or("div".into()))
        .attr("role", "tablist")
        .attr("aria-label", label)
        .attr("aria-orientation", "horizontal")
        .attr("data-tone", tone.as_str())
        .attr("style", outer.to_style())
        .add_any_attr(spread(&attrs))
        .on(ev::keydown, on_key_down)
        .child(buttons)
}
