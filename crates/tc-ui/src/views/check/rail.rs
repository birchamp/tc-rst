//! A tool's session (tC4 `CheckSession`, `CheckRail`, `CheckEmpty`): the rail of the open tool
//! beside the item detail, which B5b draws. The rail is one Tab stop (#42); a rail longer than
//! [`VIRTUAL_FROM`] rows draws the rows in view (A-24).

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::check::{CheckEmpty, CheckListRef, CheckUnit, RailRow, Tool};
use tc_api::model::rail::{self, Filter, GroupLabel, SortMode};
use wasm_bindgen::JsCast;

use super::picker::{dropped_note, rung_key, tool_name};
use crate::app::check::{
    Line, RailLayout, VIRTUAL_FROM, bar_value, min_fill, place_parts, rail_key, tab_stop,
};
use crate::app::shell::View;
use crate::app::web::check::{Rail, Session};
use crate::app::web::{Actions, AppState};
use crate::attrs::Attrs;
use crate::css::Css;
use crate::ds::{Button, ButtonVariant, Overline, ProgressBar, Size, Tone};
use crate::i18n::{keys, t, t_dyn};

fn centered(child: AnyView) -> AnyView {
    let main = crate::css! { "flex": 1, "overflow": "auto", "padding": "32px 40px 64px" };
    let inner = crate::css! { "max-width": 760, "margin": "0 auto" };
    view! {
        <main style=main.to_style()>
            <div style=inner.to_style()>{child}</div>
        </main>
    }
    .into_any()
}

fn empty_keys(empty: CheckEmpty) -> (&'static str, &'static str) {
    match empty {
        CheckEmpty::None => ("check.empty.none.title", "check.empty.none.body"),
        CheckEmpty::AllDropped => (
            "check.empty.all-dropped.title",
            "check.empty.all-dropped.body",
        ),
        CheckEmpty::Missing => ("check.empty.missing.title", "check.empty.missing.body"),
        CheckEmpty::VersificationUnavailable => (
            "check.empty.versification-unavailable.title",
            "check.empty.versification-unavailable.body",
        ),
        CheckEmpty::VersificationUnknown => (
            "check.empty.versification-unknown.title",
            "check.empty.versification-unknown.body",
        ),
    }
}

fn empty_id(empty: CheckEmpty) -> &'static str {
    match empty {
        CheckEmpty::None => "none",
        CheckEmpty::AllDropped => "all-dropped",
        CheckEmpty::Missing => "missing",
        CheckEmpty::VersificationUnavailable => "versification-unavailable",
        CheckEmpty::VersificationUnknown => "versification-unknown",
    }
}

/// tC4's `CheckEmpty`: the tool works, and this book has no check to show.
fn check_empty(actions: &Actions, rail: &Rail, empty: CheckEmpty) -> AnyView {
    let (title, body) = empty_keys(empty);
    let back = {
        let actions = actions.clone();
        Callback::new(move |_| actions.close_check_tool())
    };
    let boxed = crate::css! {
        "border": "var(--stroke-selected) dashed var(--border-dashed)",
        "border-radius": "var(--radius-xl)",
        "padding": "32px 26px",
        "text-align": "center",
    };
    let title_style = crate::css! {
        "font-size": "var(--fs-ui-md)",
        "font-weight": "var(--fw-heavy)",
        "color": "var(--uw-ocean)",
        "margin": "0 0 8px",
    };
    let body_style = crate::css! {
        "font-size": "var(--fs-ui-sm)",
        "letter-spacing": "var(--track-13)",
        "color": "var(--text-secondary)",
        "line-height": "var(--lh-body)",
        "margin": "0 auto",
        "max-width": 460,
    };
    let mono = crate::css! {
        "font-size": "var(--fs-meta)",
        "color": "var(--text-tertiary)",
        "font-family": "var(--font-mono)",
        "margin": "14px 0 0",
    };
    let citation = rail.view.citation.as_ref().map(|c| {
        (
            c.pin.repo_path.clone(),
            c.pin.version.clone().unwrap_or_default(),
        )
    });
    view! {
        <div data-testid="check-empty" data-empty=empty_id(empty)>
            <Button variant=ButtonVariant::Secondary size=Size::Sm on_click=back style=crate::css! { "margin-bottom": 16 }>
                {t(keys::check::BACK, &[])}
            </Button>
            <div style=boxed.to_style()>
                <p style=title_style.to_style()>{t_dyn(title, &[], None)}</p>
                <p style=body_style.to_style()>{t_dyn(body, &[], None)}</p>
                {dropped_note(rail.view.dropped.as_ref(), crate::css! { "margin": "14px auto 0", "max-width": 460 })}
                {citation.map(|(repo, version)| view! { <p style=mono.to_style()>{repo}" · "{version}</p> })}
            </div>
        </div>
    }
    .into_any()
}

/// tC4's `railChip`.
fn rail_chip(on: bool) -> Css {
    crate::css! {
        "border": "var(--stroke) solid",
        "cursor": "pointer",
        "font-family": "inherit",
        "font-weight": "var(--fw-bold)",
        "font-size": "var(--fs-label)",
        "letter-spacing": "var(--track-11)",
        "padding": "5px 10px",
        "border-radius": "var(--radius-pill)",
        "white-space": "nowrap",
        "background": if on { "var(--uw-ocean)" } else { "var(--surface-card)" },
        "color": if on { "var(--text-inverse)" } else { "var(--text-secondary)" },
        "border-color": if on { "var(--uw-ocean)" } else { "var(--border-input)" },
    }
}

fn filter_key(filter: Filter) -> String {
    format!("check.filter.{}", filter.id())
}

/// A sort's label; a story's By verse is By frame (tC4 `check.sort.byFrame`).
fn sort_key(mode: SortMode, story: bool) -> String {
    if story && mode == SortMode::ByVerse {
        return "check.sort.byFrame".to_owned();
    }
    format!("check.sort.{}", mode.id())
}

fn category_name(category: tc_api::check::Category) -> &'static str {
    use tc_api::check::{Category, NoteCategory, WordCategory};
    match category {
        Category::Note(n) => match n {
            NoteCategory::Discourse => "discourse",
            NoteCategory::Numbers => "numbers",
            NoteCategory::Figures => "figures",
            NoteCategory::Culture => "culture",
            NoteCategory::Grammar => "grammar",
            NoteCategory::Other => "other",
        },
        Category::Word(w) => match w {
            WordCategory::Kt => "kt",
            WordCategory::Names => "names",
            WordCategory::Other => "other",
        },
    }
}

/// One row of the rail (tC4's item button).
fn rail_row(
    actions: &Actions,
    row: &RailRow,
    terms: &[String],
    active: Signal<Option<u32>>,
    stop: Signal<Option<u32>>,
    style_extra: Option<Css>,
) -> AnyView {
    let index = row.index;
    let (chapter, verse) = place_parts(&row.place);
    let locator = format!("{chapter}:{verse}");
    let group = row
        .term
        .and_then(|t| terms.get(t as usize))
        .cloned()
        .unwrap_or_default();
    let flags = row.flags;
    let dot = rail::dot_id(flags.dot);
    // The selection moves without drawing the row again, so the row with the focus keeps it.
    let is_active = move || active.get() == Some(index);
    let style = move || {
        let on = is_active();
        let mut style = crate::css! {
            "border": format!("var(--stroke) solid {}", if on { "var(--accent)" } else { "var(--border)" }),
            "text-align": "start",
            "cursor": "pointer",
            "font-family": "inherit",
            "border-radius": "var(--radius-md)",
            "padding": "10px 12px",
            "width": "100%",
            "display": "flex",
            "align-items": "center",
            "gap": 10,
            "background": if on { "var(--surface-accent-soft)" } else { "var(--surface-card)" },
        };
        if let Some(extra) = &style_extra {
            style.extend(extra);
        }
        style.to_style()
    };
    let dot_style = crate::css! {
        "width": 9,
        "height": 9,
        "border-radius": "var(--radius-pill)",
        "flex": "none",
        "background": match dot {
            "valid" => "var(--tc-valid)",
            "invalid" => "var(--tc-invalid)",
            _ => "var(--text-tertiary)",
        },
    };
    let title_style = crate::css! {
        "flex": 1,
        "font-size": "var(--fs-ui-sm)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--text-body)",
        "overflow": "hidden",
        "text-overflow": "ellipsis",
        "white-space": "nowrap",
    };
    let glyph = |color: &'static str| {
        crate::css! { "font-size": "var(--fs-badge)", "color": color, "flex": "none" }
    };
    let ref_style = crate::css! {
        "font-size": "var(--fs-badge)",
        "letter-spacing": "var(--track-10)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--text-tertiary)",
        "white-space": "nowrap",
    };
    let actions = actions.clone();
    view! {
        <button
            type="button"
            data-i="choice"
            data-tone="accent"
            data-selected=move || is_active().then_some("true")
            data-rail-index=index.to_string()
            tabindex=move || if stop.get() == Some(index) { "0" } else { "-1" }
            aria-current=move || is_active().then_some("true")
            title=format!("{locator} · {group}")
            data-ref=locator.clone()
            data-check-id=row.check_id.clone()
            data-decided=if flags.decided { "1" } else { "0" }
            data-invalid=if flags.invalidated { "1" } else { "0" }
            data-status=dot
            data-bookmarked=if flags.bookmarked { "1" } else { "0" }
            data-commented=if flags.commented { "1" } else { "0" }
            data-save-error="0"
            style=style
            on:click=move |_| actions.select_check(index)
        >
            <span style=dot_style.to_style()></span>
            <span style=title_style.to_style()>"“"{row.title.clone()}"”"</span>
            {flags.bookmarked.then(|| view! {
                <span data-testid="row-bookmark" aria-label=t(keys::check::filter::BOOKMARKED, &[]) style=glyph("var(--uw-kindle, #E59D33)").to_style()>
                    {t(keys::check::glyph::BOOKMARK, &[])}
                </span>
            })}
            {flags.commented.then(|| view! {
                <span data-testid="row-comment" aria-label=t(keys::check::filter::COMMENTED, &[]) style=glyph("var(--text-tertiary)").to_style()>
                    {t(keys::check::glyph::COMMENT, &[])}
                </span>
            })}
            <span style=ref_style.to_style()>{t(keys::check::REF, &[("c", &chapter), ("v", &verse)])}</span>
        </button>
    }
    .into_any()
}

fn group_label(label: GroupLabel, terms: &[String], unit: &str) -> String {
    match label {
        GroupLabel::Unit => unit.to_owned(),
        GroupLabel::Term(t) => terms.get(t as usize).cloned().unwrap_or_default(),
        GroupLabel::Category(c) => category_name(c).to_owned(),
    }
}

/// Focuses the row of check `index`, if it is drawn.
fn focus_row(index: u32) {
    let selector = format!("[data-testid=\"check-list\"] [data-rail-index=\"{index}\"]");
    if let Some(row) = document()
        .query_selector(&selector)
        .ok()
        .flatten()
        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = row.focus();
    }
}

fn focus_is_unmoved() -> bool {
    let document = document();
    document
        .active_element()
        .is_none_or(|el| document.body().is_some_and(|b| b.is_same_node(Some(&el))))
}

/// tC4's `CheckRail`.
fn check_rail(
    s: &AppState,
    actions: &Actions,
    rail: &Rail,
    tool: Tool,
    kind: CheckUnit,
) -> AnyView {
    let c = s.c.clone();
    let story = matches!(kind, CheckUnit::Story(_));
    let unit = super::picker::unit_label(kind);
    let terms = Arc::new(rail.view.terms.clone());
    let rows = Arc::new(rail.rows.clone());
    let progress = rail.view.progress;
    let counts = rail::counts(&rows);
    let aside = crate::css! {
        "width": "var(--rail-width-wide)",
        "flex": "none",
        "background": "var(--surface-card)",
        "border-inline-end": "var(--stroke-hair) solid var(--border)",
        "display": "flex",
        "flex-direction": "column",
        "min-height": 0,
    };
    let head = crate::css! {
        "padding": "14px 18px",
        "border-bottom": "var(--stroke-hair) solid var(--border-hair)",
        "flex": "none",
    };
    let h2 = crate::css! {
        "font-size": "var(--fs-title-sm)",
        "letter-spacing": "var(--track-16)",
        "font-weight": "var(--fw-black)",
        "color": "var(--text-heading)",
        "margin": "0 0 2px",
    };
    let unit_style = crate::css! {
        "font-size": "var(--fs-caption)",
        "letter-spacing": "var(--track-12)",
        "color": "var(--text-tertiary)",
        "margin": "0 0 10px",
        "font-weight": "var(--fw-medium)",
    };
    let progress_style = crate::css! {
        "font-size": "var(--fs-caption)",
        "letter-spacing": "var(--track-12)",
        "color": "var(--text-secondary)",
        "margin": "8px 0 12px",
        "font-weight": "var(--fw-medium)",
    };
    // tC4's text nodes: the repository, the version and the rung, " · " between.
    let citation = rail.view.citation.as_ref().map(|c| {
        (
            c.pin.repo_path.clone(),
            c.pin.version.clone().unwrap_or_default(),
            t(rung_key(c.rung), &[]),
        )
    });
    let citation_style = crate::css! {
        "font-size": "var(--fs-micro)",
        "color": "var(--text-tertiary)",
        "font-family": "var(--font-mono)",
        "margin": "10px 0 0",
        "overflow-wrap": "anywhere",
    };
    let (decided, total) = (progress.decided.to_string(), progress.total.to_string());
    let progress_text = t(
        keys::check::PROGRESS,
        &[("decided", &decided), ("total", &total)],
    );
    let back = {
        let actions = actions.clone();
        Callback::new(move |_| actions.close_check_tool())
    };
    let to_draft = {
        let actions = actions.clone();
        Callback::new(move |_| {
            actions.close_check_tool();
            actions.go(View::Draft);
        })
    };
    let chips = {
        let (c, actions) = (c.clone(), actions.clone());
        move || {
            let on = c.filter.get();
            Filter::ORDER
                .iter()
                .zip(counts)
                .map(|(&filter, (_, n))| {
                    let actions = actions.clone();
                    view! {
                        <button
                            type="button"
                            data-trim="cap"
                            data-testid=format!("filter-{}", filter.id())
                            data-count=n.to_string()
                            style=rail_chip(on == filter).to_style()
                            on:click=move |_| actions.set_check_filter(filter)
                        >
                            {t_dyn(&filter_key(filter), &[], None)}" · "{n.to_string()}
                        </button>
                    }
                })
                .collect::<Vec<_>>()
        }
    };
    let sorts = {
        let (c, actions) = (c.clone(), actions.clone());
        move || {
            let shown = c.sort.get().unwrap_or(SortMode::default_for(tool));
            SortMode::modes(tool)
                .iter()
                .map(|&mode| {
                    let actions = actions.clone();
                    view! {
                        <button
                            type="button"
                            data-trim="cap"
                            data-testid=format!("sort-{}", mode.id())
                            style=rail_chip(shown == mode).to_style()
                            on:click=move |_| actions.set_check_sort(mode)
                        >
                            {t_dyn(&sort_key(mode, story), &[], None)}
                        </button>
                    }
                })
                .collect::<Vec<_>>()
        }
    };
    // The groups and the order the keys move in, from the filter and the sort.
    let layout = {
        let (c, rows, terms) = (c.clone(), rows.clone(), terms.clone());
        Memo::new(move |_| {
            let mode = c.sort.get().unwrap_or(SortMode::default_for(tool));
            let groups = rail::groups(rows.iter(), terms.len(), c.filter.get(), mode);
            let order: Vec<u32> = groups
                .iter()
                .flat_map(|g| {
                    g.rows
                        .iter()
                        .filter_map(|&at| rows.get(at).map(|r| r.index))
                })
                .collect();
            (groups, order)
        })
    };
    let keydown = {
        let (c, actions) = (c.clone(), actions.clone());
        move |e: web_sys::KeyboardEvent| {
            let order = layout.with_untracked(|(_, order)| order.clone());
            if let Some(next) = rail_key(&order, c.active.get_untracked(), &e.key()) {
                e.prevent_default();
                actions.select_check(next);
                request_animation_frame(move || focus_row(next));
            }
        }
    };
    let scroll_top = RwSignal::new(0.0_f64);
    let list_ref = NodeRef::<leptos::html::Div>::new();
    let active: Signal<Option<u32>> = {
        let c = c.clone();
        Signal::derive(move || c.active.get())
    };
    let stop: Signal<Option<u32>> = Memo::new(move |_| {
        let current = active.get();
        layout.with(|(_, order)| tab_stop(order, current))
    })
    .into();
    let by_index: Arc<std::collections::HashMap<u32, usize>> = Arc::new(
        rows.iter()
            .enumerate()
            .map(|(at, row)| (row.index, at))
            .collect(),
    );
    let list = if rows.len() <= VIRTUAL_FROM {
        // tC4's DOM: a group of rows under its heading. Drawn again only when the filter or the
        // sort changes.
        let (rows, terms, actions, unit) =
            (rows.clone(), terms.clone(), actions.clone(), unit.clone());
        (move || {
            layout.with(|(groups, _)| {
                let overline = crate::css! { "margin": "0 0 6px" };
                let column =
                    crate::css! { "display": "flex", "flex-direction": "column", "gap": 6 };
                groups
                    .iter()
                    .map(|g| {
                        let label = group_label(g.label, &terms, &unit);
                        let buttons: Vec<AnyView> = g
                            .rows
                            .iter()
                            .filter_map(|&at| rows.get(at))
                            .map(|row| rail_row(&actions, row, &terms, active, stop, None))
                            .collect();
                        view! {
                            <div>
                                <Overline tag="p" style=overline.clone()>{label}</Overline>
                                <div style=column.to_style()>{buttons}</div>
                            </div>
                        }
                        .into_any()
                    })
                    .collect::<Vec<_>>()
            })
        })
        .into_any()
    } else {
        // A-24: fixed heights, the rows in view and the tab stop mounted, each line keyed, so a
        // scroll or a move of the selection never draws the focused row again.
        let plan = {
            let rows = rows.clone();
            Memo::new(move |_| {
                layout.with(|(groups, _)| {
                    let indices: Vec<Vec<u32>> = groups
                        .iter()
                        .map(|g| {
                            g.rows
                                .iter()
                                .filter_map(|&at| rows.get(at).map(|r| r.index))
                                .collect()
                        })
                        .collect();
                    let labels: Vec<GroupLabel> = groups.iter().map(|g| g.label).collect();
                    (RailLayout::new(&indices), labels)
                })
            })
        };
        let lines = Memo::new(move |_| {
            let height = list_ref
                .get()
                .map_or(800.0, |el| f64::from(el.client_height()));
            let keep: Vec<u32> = stop.get().into_iter().chain(active.get()).collect();
            plan.with(|(plan, _)| {
                // A filter that shortens the list clamps the browser's scroll without a scroll
                // event: the window follows the clamp.
                let top = scroll_top.get().min((plan.height - height).max(0.0));
                let window = plan.window(top, height);
                plan.mounted(window, &keep)
            })
        });
        let spacer = move || {
            plan.with(|(plan, _)| {
                format!("position: relative; height: {}px; flex: none;", plan.height)
            })
        };
        let (rows, terms, actions, unit, by_index) = (
            rows.clone(),
            terms.clone(),
            actions.clone(),
            unit.clone(),
            by_index.clone(),
        );
        // Lines are placed from the list's padding edge; the list's 14 px padding comes first.
        let place = |y: f64, h: f64| {
            crate::css! {
                "position": "absolute",
                "top": y + 14.0,
                "left": 14,
                "right": 14,
                "height": h,
            }
        };
        view! {
            <div style=spacer></div>
            <For
                each=move || {
                    // For moves a kept row with insertBefore, which blurs it: the row that had the
                    // focus gets it back (ds::keyed_list does the same).
                    crate::ds::refocus_after_moves();
                    lines.get()
                }
                key=|(line, y): &(Line, f64)| (*line, y.to_bits())
                children=move |(line, y): (Line, f64)| match line {
                    Line::Header(n) => {
                        let label = plan.with_untracked(|(_, labels)| {
                            labels.get(n).map(|l| group_label(*l, &terms, &unit)).unwrap_or_default()
                        });
                        view! {
                            <Overline tag="p" style=place(y, crate::app::check::HEADER_HEIGHT).merge(&crate::css! { "margin": 0 })>{label}</Overline>
                        }
                        .into_any()
                    }
                    Line::Row(index) => match by_index.get(&index).and_then(|&at| rows.get(at)) {
                        Some(row) => rail_row(
                            &actions,
                            row,
                            &terms,
                            active,
                            stop,
                            Some(place(y, crate::app::check::ROW_HEIGHT - 6.0)),
                        ),
                        None => ().into_any(),
                    },
                }
            />
        }
        .into_any()
    };
    let list_style = crate::css! {
        "flex": 1,
        "overflow": "auto",
        "padding": 14,
        "min-height": 0,
        "display": "flex",
        "flex-direction": "column",
        "gap": 16,
        // A virtual rail places its lines from the list; a whole one is tC4's list as it is.
        "position": (rows.len() > VIRTUAL_FROM).then_some("relative"),
    };
    let nothing = {
        let c = c.clone();
        let rows = rows.clone();
        move || {
            let filter = c.filter.get();
            let none = !rows.iter().any(|r| filter.matches(&r.flags));
            let s = crate::css! {
                "font-size": "var(--fs-caption-lg)",
                "letter-spacing": "var(--track-12-5)",
                "color": "var(--text-tertiary)",
                "font-style": "italic",
                "margin": "6px 2px",
            };
            none.then(|| view! { <p data-testid="rail-empty" style=s.to_style()>{t(keys::check::RAIL_EMPTY, &[])}</p> })
        }
    };
    // #42: the session's current row takes the focus once, when nothing else holds it.
    {
        let c = c.clone();
        request_animation_frame(move || {
            if c.focused_for.get_untracked() == Some(tool) {
                return;
            }
            c.focused_for.try_set(Some(tool));
            if focus_is_unmoved()
                && let Some(row) = document()
                    .query_selector("[data-testid=\"check-list\"] [tabindex=\"0\"]")
                    .ok()
                    .flatten()
                    .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
            {
                let _ = row.focus();
            }
        });
    }
    view! {
        <aside data-testid="check-rail" data-resource=rail.view.citation.as_ref().map(|c| c.pin.repo_path.clone()) style=aside.to_style()>
            <div style=head.to_style()>
                <Button variant=ButtonVariant::Ghost size=Size::Sm attrs=Attrs::from([("data-testid", "check-back")]) on_click=back style=crate::css! { "padding": 0, "margin-bottom": 8 }>
                    {t(keys::check::BACK, &[])}
                </Button>
                <h2 style=h2.to_style()>{t(tool_name(tool), &[])}</h2>
                <p style=unit_style.to_style()>{unit.clone()}</p>
                <ProgressBar
                    tone=Tone::Valid
                    value=bar_value(progress.decided, progress.total)
                    height=7.0
                    min_fill=min_fill(progress.decided)
                    label=Some(progress_text.clone())
                />
                <p data-testid="check-progress" style=progress_style.to_style()>{progress_text.clone()}</p>
                <div style="display: flex; gap: 6px; flex-wrap: wrap;">{chips}</div>
                <div style="display: flex; align-items: center; gap: 6px; margin-top: 8px;">
                    <Overline>{t(keys::check::SORT, &[])}</Overline>
                    {sorts}
                </div>
                {dropped_note(rail.view.dropped.as_ref(), crate::css! { "margin-top": 10 })}
                {citation.map(|(repo, version, rung)| view! { <p style=citation_style.to_style()>{repo}" · "{version}" · "{rung}</p> })}
            </div>
            <div
                data-testid="check-list"
                node_ref=list_ref
                style=list_style.to_style()
                on:keydown=keydown
                on:scroll=move |_| {
                    if let Some(el) = list_ref.get_untracked() {
                        scroll_top.set(f64::from(el.scroll_top()));
                    }
                }
            >
                {list}
                {nothing}
            </div>
            <div style="padding: 12px 16px; border-top: var(--stroke-hair) solid var(--border-hair);">
                <Button variant=ButtonVariant::Ghost size=Size::Sm attrs=Attrs::from([("data-testid", "check-to-draft"), ("tabindex", "-1")]) on_click=to_draft style=crate::css! { "padding": 0 }>
                    {t(keys::check::BACK_TO_TRANSLATING, &[])}
                </Button>
            </div>
        </aside>
    }
    .into_any()
}

/// tC4's `CheckSession`: deriving, an error, a designed empty state, or the rail and the detail.
pub fn check_session(
    s: &AppState,
    actions: &Actions,
    at: tc_api::ids::Scope,
    unit: CheckUnit,
    tool: Tool,
) -> impl IntoView + use<> {
    let session = s.c.session.clone();
    let (s, actions) = (s.clone(), actions.clone());
    move || {
        session.with(|session| match session {
            None | Some(Session::Loading(_)) => {
                let p =
                    crate::css! { "font-size": "var(--fs-ui)", "color": "var(--text-tertiary)" };
                centered(
                    view! { <p style=p.to_style()>{t(keys::check::DERIVING, &[])}</p> }.into_any(),
                )
            }
            Some(Session::Failed(_, error)) => {
                let p = crate::css! {
                    "font-size": "var(--fs-ui)",
                    "color": "var(--tc-invalid)",
                    "line-height": "var(--lh-body)",
                };
                centered(
                    view! { <p data-testid="check-error" style=p.to_style()>{error.clone()}</p> }
                        .into_any(),
                )
            }
            Some(Session::Open(rail)) => match rail.view.empty {
                Some(empty) => centered(check_empty(&actions, rail, empty)),
                None => {
                    let root = crate::css! { "flex": 1, "display": "flex", "min-height": 0 };
                    let main = crate::css! {
                        "flex": 1,
                        "overflow": "auto",
                        "min-width": 0,
                        "background": "var(--surface-app)",
                    };
                    // A story's detail is the story card; a book's is B5b's.
                    let detail = match unit {
                        CheckUnit::Story(_) => {
                            let list = CheckListRef { tool, unit };
                            let order = Arc::new(rail_order(rail, tool));
                            super::story::story_detail(
                                &s,
                                &actions,
                                at.clone(),
                                list,
                                rail.rows.clone(),
                                order,
                            )
                        }
                        CheckUnit::Book(_) => ().into_any(),
                    };
                    view! {
                        <div data-testid="check-session" style=root.to_style()>
                            {check_rail(&s, &actions, rail, tool, unit)}
                            <main style=main.to_style()>{detail}</main>
                        </div>
                    }
                    .into_any()
                }
            },
        })
    }
}

/// The order the rail's rows are walked in under the filter and sort chosen when the session
/// opened: the order of Previous and Next. The detail holds it as the session opened, and the
/// arrow keys of the rail follow the live filter.
fn rail_order(rail: &Rail, tool: Tool) -> Vec<u32> {
    rail::groups(
        rail.rows.iter(),
        rail.view.terms.len(),
        Filter::All,
        SortMode::default_for(tool),
    )
    .iter()
    .flat_map(|g| {
        g.rows
            .iter()
            .filter_map(|&at| rail.rows.get(at).map(|r| r.index))
    })
    .collect()
}
