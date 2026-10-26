//! A tool's session (tC4 `CheckSession`, `CheckRail`, `CheckEmpty`): the rail of the open tool
//! beside the item detail, which B5b draws. The rail is one Tab stop (#42); a rail longer than
//! [`VIRTUAL_FROM`] rows draws the rows in view (A-24).

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::check::{CheckEmpty, CheckUnit, RailRow, Tool};
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

pub(super) fn category_name(category: tc_api::check::Category) -> &'static str {
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
            data-rail-stop="true"
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

fn group_label(label: GroupLabel, terms: &[String], titles: &[String], unit: &str) -> String {
    match label {
        GroupLabel::Unit => unit.to_owned(),
        GroupLabel::Term(t) => terms.get(t as usize).cloned().unwrap_or_default(),
        // A figure is headed by its Translation Academy title; a note with no article is under
        // "Other notes" (tC4 #769).
        GroupLabel::Figure(figure) => match terms.get(figure as usize).map(String::as_str) {
            Some("") | None => t(keys::check::group::OTHER, &[]),
            Some(slug) => titles
                .get(figure as usize)
                .cloned()
                .unwrap_or_else(|| slug.to_owned()),
        },
    }
}

/// Left on a row (Right in a right-to-left rail) moves the focus to its group's heading, the
/// tree pattern, and selects nothing (tC4 #769 `rowToHeading`): the heading of the focused row's
/// own group, which `group_of` names by the row's check index.
fn row_to_heading(
    e: &web_sys::KeyboardEvent,
    list: NodeRef<leptos::html::Div>,
    group_of: impl Fn(u32) -> Option<String>,
) -> Option<web_sys::HtmlElement> {
    let list = list.get_untracked()?;
    let rtl = list
        .closest("[dir]")
        .ok()
        .flatten()
        .and_then(|el| el.get_attribute("dir"))
        .is_some_and(|dir| dir == "rtl");
    let target = e.target()?.dyn_into::<web_sys::Element>().ok()?;
    if e.key() != (if rtl { "ArrowRight" } else { "ArrowLeft" })
        || !target.has_attribute("data-rail-index")
    {
        return None;
    }
    let index = target
        .get_attribute("data-rail-index")?
        .parse::<u32>()
        .ok()?;
    let group = serde_json::to_string(&group_of(index)?).ok()?;
    list.query_selector(&format!("[data-testid=\"rail-group\"][data-group={group}]"))
        .ok()
        .flatten()?
        .dyn_into::<web_sys::HtmlElement>()
        .ok()
}

/// Up and Down on a fold heading go to the stop before or after it in screen order: a heading
/// takes the focus, a row is selected (tC4 #769 `headingKeyDown`). Enter and Space are the
/// button's own.
fn heading_key(e: &web_sys::KeyboardEvent, list: NodeRef<leptos::html::Div>, actions: &Actions) {
    let step: i64 = match e.key().as_str() {
        "ArrowDown" => 1,
        "ArrowUp" => -1,
        "Home" => i64::MIN,
        "End" => i64::MAX,
        _ => return,
    };
    e.prevent_default();
    // The list's own keys move the selection; a heading's are its own.
    e.stop_propagation();
    let (Some(list), Some(heading)) = (
        list.get_untracked(),
        e.current_target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok()),
    ) else {
        return;
    };
    let Ok(stops) = list.query_selector_all("[data-rail-stop]") else {
        return;
    };
    let n = i64::from(stops.length());
    let at = (0..stops.length())
        .find(|&i| stops.get(i).is_some_and(|s| s.is_same_node(Some(&heading))))
        .map_or(-1, i64::from);
    let to = match step {
        i64::MIN => 0,
        i64::MAX => n - 1,
        _ if at < 0 => 0,
        _ => (at + step + n) % n,
    };
    let Some(stop) = u32::try_from(to)
        .ok()
        .and_then(|i| stops.get(i))
        .and_then(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
    else {
        return;
    };
    let _ = stop.focus();
    if let Some(index) = stop
        .get_attribute("data-rail-index")
        .and_then(|i| i.parse::<u32>().ok())
    {
        actions.select_check(index);
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
    let titles = Arc::new(rail.view.group_titles.clone());
    let rows = Arc::new(rail.rows.clone());
    let term_counts = Arc::new(rail::term_progress(&rows, terms.len()));
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
                    .into_any()
                })
                .collect::<Vec<_>>()
        }
    };
    let by_index: Arc<std::collections::HashMap<u32, usize>> = Arc::new(
        rows.iter()
            .enumerate()
            .map(|(at, row)| (row.index, at))
            .collect(),
    );
    // The active check's group id: its group is open whatever was folded (tC4 #769).
    let active_group = {
        let (c, rows, terms, by_index) = (c.clone(), rows.clone(), terms.clone(), by_index.clone());
        Memo::new(move |_| {
            let at = *by_index.get(&c.active.get()?)?;
            let term = rows.get(at)?.term?;
            terms.get(usize::try_from(term).ok()?).cloned()
        })
    };
    // The groups, which are open, and the order the keys move in, from the filter, the sort and
    // the folds: the rows of a folded group leave the order (tC4 #769).
    let layout = {
        let (c, rows, terms) = (c.clone(), rows.clone(), terms.clone());
        Memo::new(move |_| {
            let mode = c.sort.get().unwrap_or(SortMode::default_for(tool));
            let groups = rail::groups(rows.iter(), terms.len(), c.filter.get(), mode);
            // Only a folding sort with a group folded depends on the selection, so a move under
            // By verse does not group the rail again.
            let open = c.folded.with(|folded| {
                if rail::folds(mode) && !folded.is_empty() {
                    active_group
                        .with(|a| rail::open_groups(&groups, &terms, folded, a.as_deref(), mode))
                } else {
                    vec![true; groups.len()]
                }
            });
            let order: Vec<u32> = groups
                .iter()
                .zip(&open)
                .filter(|(_, open)| **open)
                .flat_map(|(g, _)| {
                    g.rows
                        .iter()
                        .filter_map(|&at| rows.get(at).map(|r| r.index))
                })
                .collect();
            (groups, order, open, rail::folds(mode))
        })
    };
    let list_ref = NodeRef::<leptos::html::Div>::new();
    // The group id of check `index`, by its row's term.
    let group_of = {
        let (rows, terms, by_index) = (rows.clone(), terms.clone(), by_index.clone());
        move |index: u32| {
            let term = rows.get(*by_index.get(&index)?)?.term?;
            terms.get(usize::try_from(term).ok()?).cloned()
        }
    };
    let keydown = {
        let (c, actions) = (c.clone(), actions.clone());
        move |e: web_sys::KeyboardEvent| {
            // Left (Right in a right-to-left rail) on a row goes to its group's heading.
            if let Some(heading) = row_to_heading(&e, list_ref, &group_of) {
                e.prevent_default();
                let _ = heading.focus();
                return;
            }
            let order = layout.with_untracked(|(_, order, ..)| order.clone());
            if let Some(next) = rail_key(&order, c.active.get_untracked(), &e.key()) {
                e.prevent_default();
                actions.select_check(next);
                request_animation_frame(move || focus_row(next));
            }
        }
    };
    let scroll_top = RwSignal::new(0.0_f64);
    let active: Signal<Option<u32>> = {
        let c = c.clone();
        Signal::derive(move || c.active.get())
    };
    let stop: Signal<Option<u32>> = Memo::new(move |_| {
        let current = active.get();
        layout.with(|(_, order, ..)| tab_stop(order, current))
    })
    .into();
    // Fold all, or Unfold all once every other shown group is folded (tC4 #769), with the
    // sort chips when the sort folds and more than one group shows.
    let fold_all = {
        let (c, actions, terms) = (c.clone(), actions.clone(), terms.clone());
        move || {
            let (ids, fold, shown) = layout.with(|(groups, _, _, folds)| {
                let (ids, fold) = c.folded.with(|folded| {
                    active_group.with(|a| rail::fold_all(groups, &terms, folded, a.as_deref()))
                });
                (ids, fold, *folds && groups.len() > 1)
            });
            let actions = actions.clone();
            shown.then(|| {
                let key = if fold { keys::check::FOLD_ALL } else { keys::check::UNFOLD_ALL };
                view! {
                    <button
                        type="button"
                        data-trim="cap"
                        data-testid="rail-fold-all"
                        style=rail_chip(false).merge(&crate::css! { "margin-inline-start": "auto" }).to_style()
                        on:click=move |_| actions.set_rail_folded(tool, ids.clone(), fold)
                    >
                        {t(key, &[])}
                    </button>
                }
            })
        }
    };
    // With every shown group folded no row is drawn: the first heading is the Tab stop.
    let heading_stop = Memo::new(move |_| {
        layout.with(|(groups, order, _, folds)| {
            (*folds && order.is_empty()).then(|| groups.first().map(|g| g.label))?
        })
    });
    let heading = {
        let (terms, titles, unit, counts, actions) = (
            terms.clone(),
            titles.clone(),
            unit.clone(),
            term_counts.clone(),
            actions.clone(),
        );
        Arc::new(
            move |label: GroupLabel, open: bool, folds: bool, style: Css| {
                let text = group_label(label, &terms, &titles, &unit);
                let Some(id) = rail::group_id(label, &terms)
                    .filter(|_| folds)
                    .map(str::to_owned)
                else {
                    return view! { <Overline tag="p" style=style>{text}</Overline> }.into_any();
                };
                let count = match label {
                    GroupLabel::Term(t) | GroupLabel::Figure(t) => counts.get(t as usize).copied(),
                    GroupLabel::Unit => None,
                }
                .map(|(decided, total)| format!("{decided}/{total}"))
                .unwrap_or_default();
                let toggle = {
                    let (actions, id) = (actions.clone(), id.clone());
                    move |_| actions.set_rail_folded(tool, vec![id.clone()], open)
                };
                let keys = {
                    let actions = actions.clone();
                    move |e: web_sys::KeyboardEvent| heading_key(&e, list_ref, &actions)
                };
                let button = crate::css! {
                    "display": "flex",
                    "align-items": "center",
                    "gap": 8,
                    "width": "100%",
                    "padding": "4px 2px",
                    "border": 0,
                    "background": "transparent",
                    "cursor": "pointer",
                    "font-family": "inherit",
                    "text-align": "start",
                }
                .merge(&style);
                let chevron = crate::css! {
                    "color": "var(--text-tertiary)",
                    "transform": if open { "rotate(90deg)" } else { "none" },
                    "transition": "transform var(--dur-hover) var(--ease-standard)",
                };
                let count_style = crate::css! {
                    "font-size": "var(--fs-badge)",
                    "font-weight": "var(--fw-bold)",
                    "color": "var(--text-tertiary)",
                    "white-space": "nowrap",
                };
                view! {
                <button
                    type="button"
                    data-testid="rail-group"
                    data-group=id
                    data-rail-stop="true"
                    aria-expanded=if open { "true" } else { "false" }
                    tabindex=move || if heading_stop.get() == Some(label) { "0" } else { "-1" }
                    on:click=toggle
                    on:keydown=keys
                    style=button.to_style()
                >
                    <span aria-hidden="true" style=chevron.to_style()>"›"</span>
                    <Overline style=crate::css! { "flex": 1, "min-width": 0, "overflow-wrap": "anywhere" }>{text}</Overline>
                    <span data-testid="rail-group-count" style=count_style.to_style()>{count}</span>
                </button>
            }
            .into_any()
            },
        )
    };
    let list = if rows.len() <= VIRTUAL_FROM {
        // tC4's DOM: a group of rows under its heading. Drawn again only when the filter or the
        // sort changes.
        let (rows, terms, actions) = (rows.clone(), terms.clone(), actions.clone());
        let heading = heading.clone();
        (move || {
            layout.with(|(groups, _, open, folds)| {
                let overline = crate::css! { "margin": "0 0 6px" };
                let column =
                    crate::css! { "display": "flex", "flex-direction": "column", "gap": 6 };
                groups
                    .iter()
                    .zip(open)
                    .map(|(g, &open)| {
                        let head = heading(g.label, open, *folds, overline.clone());
                        let id = rail::group_id(g.label, &terms).unwrap_or_default().to_owned();
                        let buttons = open.then(|| {
                            g.rows
                                .iter()
                                .filter_map(|&at| rows.get(at))
                                .map(|row| rail_row(&actions, row, &terms, active, stop, None))
                                .collect::<Vec<AnyView>>()
                        });
                        view! {
                            <div>
                                {head}
                                {buttons.map(|b| view! { <div data-testid="rail-group-rows" data-group=id style=column.to_style()>{b}</div> })}
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
                layout.with(|(groups, _, open, folds)| {
                    // A folded group keeps its heading line and draws no row.
                    let indices: Vec<Vec<u32>> = groups
                        .iter()
                        .zip(open)
                        .map(|(g, &open)| {
                            if !open {
                                return Vec::new();
                            }
                            g.rows
                                .iter()
                                .filter_map(|&at| rows.get(at).map(|r| r.index))
                                .collect()
                        })
                        .collect();
                    let labels: Vec<(GroupLabel, bool)> = groups
                        .iter()
                        .map(|g| g.label)
                        .zip(open.iter().copied())
                        .collect();
                    (RailLayout::new(&indices), labels, *folds)
                })
            })
        };
        let lines = Memo::new(move |_| {
            let height = list_ref
                .get()
                .map_or(800.0, |el| f64::from(el.client_height()));
            let keep: Vec<u32> = stop.get().into_iter().chain(active.get()).collect();
            plan.with(|(plan, labels, folds)| {
                // A filter that shortens the list clamps the browser's scroll without a scroll
                // event: the window follows the clamp.
                let top = scroll_top.get().min((plan.height - height).max(0.0));
                let window = plan.window(top, height);
                // A heading line carries what it draws, so a fold, a sort or a filter that
                // leaves it in place still draws it again.
                plan.mounted(window, &keep)
                    .into_iter()
                    .map(|(line, y)| {
                        let head = match line {
                            Line::Header(n) => {
                                labels.get(n).map(|(label, open)| (*label, *open, *folds))
                            }
                            Line::Row(_) => None,
                        };
                        (line, y, head)
                    })
                    .collect::<Vec<_>>()
            })
        });
        let spacer = move || {
            plan.with(|(plan, ..)| {
                format!("position: relative; height: {}px; flex: none;", plan.height)
            })
        };
        let (rows, terms, actions, by_index, heading) = (
            rows.clone(),
            terms.clone(),
            actions.clone(),
            by_index.clone(),
            heading.clone(),
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
                key=|(line, y, head): &(Line, f64, Option<(GroupLabel, bool, bool)>)| (*line, y.to_bits(), *head)
                children=move |(line, y, head): (Line, f64, Option<(GroupLabel, bool, bool)>)| match line {
                    Line::Header(_) => {
                        match head {
                            Some((label, open, folds)) => heading(
                                label,
                                open,
                                folds,
                                place(y, crate::app::check::HEADER_HEIGHT).merge(&crate::css! { "margin": 0 }),
                            ),
                            None => ().into_any(),
                        }
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
                    {fold_all}
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

/// What a tool's session shows, apart from the rows themselves: a patch to a row leaves the
/// detail column as it is, so the focus in it survives a mark.
#[derive(Debug, Clone, PartialEq)]
enum Shown {
    Deriving,
    Failed(String),
    Empty(CheckEmpty),
    Rail,
}

/// tC4's `CheckSession`: deriving, an error, a designed empty state, or the rail and the detail.
pub fn check_session(
    s: &AppState,
    actions: &Actions,
    unit: CheckUnit,
    tool: Tool,
) -> impl IntoView + use<> {
    let session = s.c.session.clone();
    let shown = {
        let session = session.clone();
        Memo::new(move |_| {
            session.with(|session| match session {
                None | Some(Session::Loading(_)) => Shown::Deriving,
                Some(Session::Failed(_, error)) => Shown::Failed(error.clone()),
                Some(Session::Open(rail)) => match rail.view.empty {
                    Some(empty) => Shown::Empty(empty),
                    None => Shown::Rail,
                },
            })
        })
    };
    let (s, actions) = (s.clone(), actions.clone());
    move || match shown.get() {
        Shown::Deriving => {
            let p = crate::css! { "font-size": "var(--fs-ui)", "color": "var(--text-tertiary)" };
            centered(view! { <p style=p.to_style()>{t(keys::check::DERIVING, &[])}</p> }.into_any())
        }
        Shown::Failed(error) => {
            let p = crate::css! {
                "font-size": "var(--fs-ui)",
                "color": "var(--tc-invalid)",
                "line-height": "var(--lh-body)",
            };
            centered(
                view! { <p data-testid="check-error" style=p.to_style()>{error}</p> }.into_any(),
            )
        }
        Shown::Empty(empty) => {
            let (session, actions) = (session.clone(), actions.clone());
            centered(
                (move || {
                    session.with(|session| match session {
                        Some(Session::Open(rail)) => check_empty(&actions, rail, empty),
                        _ => ().into_any(),
                    })
                })
                .into_any(),
            )
        }
        Shown::Rail => {
            let root = crate::css! { "flex": 1, "display": "flex", "min-height": 0 };
            let main = crate::css! {
                "flex": 1,
                "overflow": "auto",
                "min-width": 0,
                "background": "var(--surface-app)",
            };
            let rail = {
                let (session, s, actions) = (session.clone(), s.clone(), actions.clone());
                move || {
                    session.with(|session| match session {
                        Some(Session::Open(rail)) => check_rail(&s, &actions, rail, tool, unit),
                        _ => ().into_any(),
                    })
                }
            };
            // Row patches update the rail without remounting the detail or its comment field.
            let detail = super::detail::check_detail(&s, &actions, unit, tool);
            let academy = super::detail::academy_drawer(&s, &actions);
            view! {
                <div data-testid="check-session" style=root.to_style()>
                    {rail}
                    <main style=main.to_style()>{detail}</main>
                    {academy}
                </div>
            }
            .into_any()
        }
    }
}
