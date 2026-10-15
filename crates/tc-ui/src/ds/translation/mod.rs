//! tC4's `components/translation`: ScripturePassage, VerseMarker, WordChip.

use leptos::ev::{KeyboardEvent, PointerEvent};
use leptos::prelude::*;

use crate::css::{CssValue, js_number};
use crate::ds::{Attrs, ChipState, Css, MarkerState, Script, Size, spread};
use crate::text::runs;

/// One verse of a ScripturePassage.
#[derive(Debug, Clone, PartialEq)]
pub struct Verse {
    pub n: String,
    pub text: String,
}

fn family(script: Script) -> &'static str {
    match script {
        Script::Latin => "var(--font-scripture)",
        Script::Greek => "var(--font-greek)",
        Script::Hebrew => "var(--font-hebrew)",
        Script::Arabic => "var(--font-arabic)",
        Script::Nastaliq => "var(--font-nastaliq)",
    }
}

/// ScripturePassage.jsx: continuous scripture prose. Verses run on; a highlight marks each
/// contiguous run of a phrase's words. Size and leading travel together as a step; `size`
/// is an escape hatch that breaks the pairing.
#[component]
pub fn ScripturePassage(
    #[prop(optional)] verses: Vec<Verse>,
    #[prop(optional, into)] dir: Option<String>,
    #[prop(optional)] script: Script,
    #[prop(default = Size::Lg)] step: Size,
    #[prop(optional, into)] size: Option<CssValue>,
    /// Phrases to mark.
    #[prop(optional)]
    highlight: Vec<String>,
    #[prop(optional)] style: Css,
) -> impl IntoView {
    let nastaliq = script == Script::Nastaliq;
    let d = dir.unwrap_or_else(|| if script.is_rtl() { "rtl" } else { "ltr" }.to_string());
    let (step_size, step_leading) = match step {
        Size::Lg => ("var(--fs-verse-lg)", "var(--lh-verse-lg)"),
        Size::Md => ("var(--fs-verse-md)", "var(--lh-verse-md)"),
        Size::Sm => ("var(--fs-verse-sm)", "var(--lh-verse-sm)"),
    };
    let phrases: Vec<String> = highlight.into_iter().filter(|p| !p.is_empty()).collect();
    let mut s = css! { "text-align": "start", "font-family": family(script) };
    s.set_opt(
        "font-size-adjust",
        (!matches!(script, Script::Latin | Script::Nastaliq))
            .then_some("var(--fs-adjust-scripture)"),
    );
    s.set(
        "font-size",
        size.unwrap_or_else(|| {
            if nastaliq {
                "var(--fs-verse-nastaliq)"
            } else {
                step_size
            }
            .into()
        }),
    );
    s.extend(&css! {
        "line-height": if nastaliq { "var(--lh-nastaliq)" } else { step_leading },
        "color": "var(--text-scripture)",
        "margin": "0 0 10px",
    });
    s.extend(&style);
    let sup = css! {
        "font-size": "var(--fs-label)",
        "letter-spacing": "var(--track-11)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--text-tertiary)",
        "margin-inline-end": 3,
        "vertical-align": "super",
    }
    .to_style();
    let mark = css! {
        "border-radius": "var(--radius-xs)",
        "padding": "0 .08em",
        "background": "var(--tc-highlight-soft)",
        "color": "var(--uw-ocean)",
        "box-decoration-break": "clone",
        "-webkit-box-decoration-break": "clone",
        "transition": "background var(--dur-hover) var(--ease-standard)",
    }
    .to_style();
    let body = verses
        .into_iter()
        .map(|v| {
            let words: Vec<&str> = v.text.split_whitespace().collect();
            let segs = if phrases.is_empty() {
                vec![crate::text::Run {
                    on: false,
                    text: words.join(" "),
                }]
            } else {
                runs(&words, &phrases)
            };
            // Real spaces between segments, inline rather than inline-block, so the line breaker
            // sets and compresses the gaps itself; unmarked runs carry no box.
            let segs = segs
                .into_iter()
                .enumerate()
                .map(|(i, seg)| {
                    let space = (i > 0).then_some(" ");
                    let text = if seg.on {
                        view! { <span style=mark.clone()>{seg.text}</span> }.into_any()
                    } else {
                        seg.text.into_any()
                    };
                    view! { {space} {text} }
                })
                .collect_view();
            view! {
                <sup style=sup.clone()>{v.n}</sup>
                {segs}
                " "
            }
        })
        .collect_view();
    view! { <p dir=d style=s.to_style()>{body}</p> }
}

const PIN: &str = "M320 64C214 64 128 148.4 128 252.6C128 371.9 248.2 514.9 298.4 569.4C310.2 582.2 329.8 582.2 341.6 569.4C391.8 514.9 512 371.9 512 252.6C512 148.4 426 64 320 64z";

/// VerseMarker.jsx: the draggable verse-boundary handle, a Kindle pin with the verse number in
/// its head. 32 px gives a 24×32 target, WCAG 2.5.8 AA and no larger, because the pin sits
/// among words that are targets themselves.
#[component]
pub fn VerseMarker(
    n: u32,
    #[prop(default = 32.0)] size: f64,
    #[prop(optional)] state: MarkerState,
    #[prop(optional)] on_pick_up: Option<Callback<web_sys::Event>>,
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let dragging = state == MarkerState::Dragging;
    let w = (size * 0.741 + 0.5).floor();
    let digits = n.to_string().len();
    let label = label.unwrap_or_else(|| format!("Move where verse {n} begins"));
    let s = css! {
        "display": "inline-flex",
        "width": w,
        "height": size,
        "vertical-align": "text-bottom",
        "cursor": if dragging { "grabbing" } else { "grab" },
        "flex": "none",
        "filter": if dragging { "drop-shadow(0 6px 10px rgba(1,66,99,.28))" } else { "drop-shadow(0 1px 2px rgba(1,66,99,.18))" },
        "transition": "filter var(--dur-hover) var(--ease-standard)",
    }
    .merge(&style);
    let font_size = match digits {
        1 => 250,
        2 => 205,
        3 => 160,
        _ => 140,
    };
    view! {
        <span
            role="button"
            tabindex="0"
            aria-label=label
            on:pointerdown=move |e: PointerEvent| {
                e.prevent_default();
                if let Some(cb) = on_pick_up {
                    cb.run(e.into());
                }
            }
            on:keydown=move |e: KeyboardEvent| {
                if e.key() == "Enter" || e.key() == " " {
                    e.prevent_default();
                    if let Some(cb) = on_pick_up {
                        cb.run(e.into());
                    }
                }
            }
            style=s.to_style()
            {..spread(&attrs)}
        >
            <svg viewBox="128 64 384 518" width=js_number(w) height=js_number(size) aria-hidden="true" style="display: block; overflow: visible;">
                <path d=PIN fill="var(--uw-kindle)" stroke="var(--tc-warn-text)" stroke-width="1.5" vector-effect="non-scaling-stroke"></path>
                <text
                    x="320"
                    y="252.6"
                    text-anchor="middle"
                    dominant-baseline="central"
                    fill="var(--uw-ink)"
                    font-family="var(--font-ui)"
                    font-weight="900"
                    font-size=font_size.to_string()
                >
                    {n.to_string()}
                </text>
            </svg>
        </span>
    }
}

/// WordChip.jsx: a draggable word token in the alignment editor.
#[component]
pub fn WordChip(
    #[prop(optional)] state: ChipState,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let placed = state == ChipState::Placed;
    let look = match state {
        ChipState::Default | ChipState::Placed => css! {
            "background": "#fff",
            "color": "var(--text-scripture)",
            "border": "var(--stroke) solid var(--border-input)",
            "box-shadow": "var(--shadow-chip)",
        },
        ChipState::Selected => css! {
            "background": "var(--accent)",
            "color": "#fff",
            "border": "var(--stroke) solid var(--accent)",
            "box-shadow": "var(--shadow-focus)",
        },
        ChipState::Suggested => css! {
            "background": "var(--tc-suggest-bg)",
            "color": "var(--tc-suggest-fg)",
            "border": "var(--stroke-selected) dashed var(--accent)",
            "box-shadow": "none",
        },
    };
    let s = css! {
        "font-family": "var(--font-scripture)",
        "font-size": if placed { "var(--fs-ui-md)" } else { "var(--fs-body)" },
        "line-height": 1.2,
        "border-radius": if placed { "var(--radius-chip)" } else { "var(--radius-sm)" },
        "padding": if placed { "3px 9px" } else { "5px 11px" },
        "cursor": "pointer",
        "white-space": "nowrap",
        "display": "inline-flex",
        "align-items": "center",
        "gap": 5,
    }
    .merge(&look)
    .merge(&style);
    view! {
        <span draggable="true" style=s.to_style() {..spread(&attrs)}>
            {children.map(|c| c())}
        </span>
    }
}
