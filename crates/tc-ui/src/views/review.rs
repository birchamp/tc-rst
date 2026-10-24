//! The port of tC4's `ConflictReview.jsx` (unit B11, #679; owner mockup 2026-10-13, artboards
//! a and b): one conflict at a time, in Translate, every version with its owner and date,
//! newest first, coloured by owner. A click selects a version and the others fade; Keep this
//! version writes one resolution. The person can leave at any time; the open conflicts stay
//! listed. The screen reads the fold and starts no sync.
//!
//! The rebuild adds, each a declared difference: tabs for the kinds tC4 does not review, drawn
//! only while one of them holds something (STORAGE §8.5, Q-9); the clock warning on a version;
//! History (#78) with *Restore this version*; and the choices of the other kinds of the Text
//! tab (C7a): a passage's sides, the joined or separate verses, and *Keep all of Maria's*.

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::conflicts::{
    CheckPart, ConflictEntry, ConflictKind, DetachedEntry, DetachedKind, History, KeepChoice,
    List, NoticeEntry, PartPick, ReviewEntries, ReviewPlace, ReviewTab, TabCount, Version,
    VersionValue,
};
use tc_api::editor::{PaneText, SourceFrame, Sources};
use tc_api::ids::Scope;
use tc_api::shown::{Direction as TextDir, PersonLabel};
use wasm_bindgen::JsValue;

use super::draft_chrome::HAIR;
use crate::app::q9::{Q9Body, Q9Conflict, Q9Question, Q9Review, Q9Row, Q9Screen, Q9Version};
use crate::app::review::{
    BackTo, CheckButton, Choices, KeepOutcome, Owner, back_label, cause_label, check_buttons,
    choices, consequence, decision_lines, group_others, keep_all_label, keep_passage_label,
    kept_all_text, kept_name, made_by, moved_choices, notice_text, part_label, per_line_choice,
    persons, place_label, tab_label, tabs_shown,
};
use crate::app::web::{Actions, AppCtx, AppState, use_app, use_runtime};
use crate::attrs::Attrs;
use crate::ds::{Button, ButtonVariant, Callout, CalloutTone, Overline, OverlineTone, Size};
use crate::i18n::{keys, t};

/// tC4's `MEASURE`.
fn measure() -> crate::css::Css {
    crate::css! { "max-width": 860, "margin": "0 auto" }
}

/// tC4's `Icon`.
fn icon(paths: &[&'static str], size: u32, width: &'static str) -> AnyView {
    let paths = paths
        .iter()
        .map(|d| view! { <path d=*d></path> })
        .collect_view();
    view! {
        <svg aria-hidden="true" width=size height=size viewBox="0 0 24 24" fill="none" stroke="currentColor"
            stroke-width=width stroke-linecap="round" stroke-linejoin="round" style="flex: none; display: block;">
            {paths}
        </svg>
    }
    .into_any()
}

/// tC4's `versionDate`: the machine's locale, with the year and the time, as the Home card
/// writes a date: two versions can come from one day.
fn version_date(at: i64) -> String {
    #[expect(
        clippy::cast_precision_loss,
        reason = "milliseconds since 1970 stay below 2^53 until the year 287396"
    )]
    let date = js_sys::Date::new(&JsValue::from_f64(at as f64));
    let day = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&day, &"day".into(), &"numeric".into());
    let _ = js_sys::Reflect::set(&day, &"month".into(), &"short".into());
    let _ = js_sys::Reflect::set(&day, &"year".into(), &"numeric".into());
    let time = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&time, &"hour".into(), &"2-digit".into());
    let _ = js_sys::Reflect::set(&time, &"minute".into(), &"2-digit".into());
    let day: String = date.to_locale_date_string("default", &day).into();
    let time: String = date
        .to_locale_time_string_with_options("default", &time)
        .into();
    format!("{day} · {time}")
}

fn dir_of(s: &AppState) -> &'static str {
    match s.project.with_untracked(|p| p.as_ref().map(|p| p.dir)) {
        Some(TextDir::Rtl) => "rtl",
        _ => "ltr",
    }
}

/// The review over the Check screen, opened from an aligner's or a check's marker: a layer
/// below the top bar, so the screen under it, and any aligner edit it holds, stays as it is.
#[component]
pub fn ReviewOverOther() -> impl IntoView {
    let Some(AppCtx { s, .. }) = use_app() else {
        return ().into_any();
    };
    let review = s.review.clone();
    let open = Memo::new(move |_| review.with(Option::is_some));
    let layer = crate::css! {
        "position": "fixed",
        "top": "var(--header-height)",
        "left": 0,
        "right": 0,
        "bottom": 0,
        "z-index": "calc(var(--z-raised) + 1)",
        "display": "flex",
        "flex-direction": "column",
        "background": "var(--surface-app)",
    };
    (move || {
        open.get().then(|| {
            view! { <div data-testid="conflict-review-layer" style=layer.to_style()><ConflictReview /></div> }
        })
    })
    .into_any()
}

/// The review over the open project: tC4's `ConflictReview`.
#[component]
pub fn ConflictReview() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let read = {
        let (project, review) = (s.project.clone(), s.review.clone());
        Memo::new(move |_| {
            let at = project.with(|p| p.as_ref().map(|p| p.at.clone()))?;
            let tab = review.with(|r| r.as_ref().map(|r| r.tab))?;
            Some((at, tab))
        })
    };
    (move || match read.get() {
        Some((at, tab)) => review_tab(&s, &actions, at, tab).into_any(),
        None => ().into_any(),
    })
    .into_any()
}

fn review_tab(s: &AppState, actions: &Actions, at: Scope, tab: ReviewTab) -> AnyView {
    let Some(runtime) = use_runtime() else {
        return ().into_any();
    };
    let list = runtime.cache().read(List {
        at: at.clone(),
        tab,
    });
    let view = Memo::new(move |_| list.with(|l| l.value().cloned()));
    let review = s.review.clone();
    let busy = {
        let review = review.clone();
        Signal::derive(move || review.with(|r| r.as_ref().is_some_and(|r| r.keep.busy())))
    };
    let shown: Memo<Option<(usize, ConflictEntry, usize)>> = {
        let review = review.clone();
        Memo::new(move |_| {
            view.with(|v| {
                let v = v.as_ref()?;
                let ReviewEntries::Conflicts(entries) = &v.entries else {
                    return None;
                };
                let i = review.with(|r| r.as_ref().and_then(|r| r.shown(entries)))?;
                Some((i, entries.get(i)?.clone(), entries.len()))
            })
        })
    };
    let together = Memo::new(move |_| {
        view.with(|v| match (v.as_ref().map(|v| &v.entries), shown.get()) {
            (Some(ReviewEntries::Conflicts(entries)), Some((_, entry, _))) => {
                group_others(entries, &entry)
            }
            _ => 0,
        })
    });
    let root = crate::css! {
        "flex": 1,
        "min-height": 0,
        "display": "flex",
        "flex-direction": "column",
        "background": "var(--surface-app)",
    };
    let bar = crate::css! {
        "height": 53,
        "flex": "none",
        "box-sizing": "border-box",
        "display": "flex",
        "align-items": "center",
        "gap": 14,
        "padding": "0 24px",
        "background": "var(--surface-card)",
        "border-bottom": "var(--stroke-hair) solid var(--border)",
    };
    let h1 = crate::css! {
        "margin": 0,
        "font-size": "var(--fs-title)",
        "letter-spacing": "var(--track-17)",
        "font-weight": "var(--fw-heavy)",
        "color": "var(--text-heading)",
    };
    let back = {
        let (book, chapter, screen) = (s.u.book.clone(), s.u.chapter.clone(), s.view.clone());
        move || {
            back_label(if screen.get() == crate::app::shell::View::Check {
                BackTo::Check
            } else {
                BackTo::Translate(book.get(), chapter.get())
            })
        }
    };
    let on_back = {
        let actions = actions.clone();
        Callback::new(move |_| actions.close_review())
    };
    let navigator = {
        let actions = actions.clone();
        move || {
            shown.get().map(|(i, _, m)| {
                view.with(|v| {
                    let entries = match v.as_ref().map(|v| &v.entries) {
                        Some(ReviewEntries::Conflicts(entries)) => entries.clone(),
                        _ => Vec::new(),
                    };
                    navigator(&actions, entries, i, m, busy)
                })
            })
        }
    };
    let tabs = {
        let actions = actions.clone();
        move || {
            view.with(|v| {
                let on_tab = {
                    let actions = actions.clone();
                    Callback::new(move |tab| actions.choose_review_tab(tab))
                };
                v.as_ref()
                    .map(|v| tab_strip(on_tab, &tabs_shown(&v.tabs, tab), tab))
            })
        }
    };
    let body = {
        let (s, actions, at) = (s.clone(), actions.clone(), at.clone());
        move || {
            let outcome = review.with(|r| r.as_ref().and_then(|r| r.keep.outcome()));
            let error = outcome.map(outcome_callout);
            let content = view.with(|v| match v.as_ref().map(|v| &v.entries) {
                Some(ReviewEntries::Conflicts(entries)) => match shown.get() {
                    Some((i, entry, _)) => conflict_versions(
                        &s,
                        &actions,
                        &at,
                        tab,
                        &entry,
                        i,
                        entries,
                        together.get(),
                        busy,
                    )
                    .into_any(),
                    None => none(t(keys::conflicts::NONE, &[])).into_any(),
                },
                Some(ReviewEntries::Detached(entries)) => detached(entries).into_any(),
                Some(ReviewEntries::Notices(entries)) => notices(entries).into_any(),
                None => ().into_any(),
            });
            (error, content)
        }
    };
    let scroll = crate::css! {
        "flex": 1,
        "min-height": 0,
        "overflow": "auto",
        "padding": "22px 24px",
    };
    let footer = {
        let actions = actions.clone();
        move || {
            shown.get().map(|(i, entry, _)| match offered(&entry) {
                Choices::Candidate => keep_bar(&actions, &entry, i, busy).into_any(),
                Choices::PassageSides => passage_bar(&actions, &entry, i, busy).into_any(),
                Choices::JoinedOrSeparate => structure_bar(&actions, &entry, i, busy).into_any(),
                Choices::CheckVersions => check_bar(&actions, &entry, i, busy),
                Choices::MovedCheck => moved_bar(&actions, &entry, i, busy),
                Choices::Later => later_bar().into_any(),
            })
        }
    };
    let ghost = crate::css! { "padding": 0, "height": "auto", "font-size": "var(--fs-ui-sm)" };
    view! {
        <main data-testid="conflict-review" style=root.to_style()>
            <div style=bar.to_style()>
                <Button variant=ButtonVariant::Ghost disabled=busy on_click=on_back style=ghost
                    attrs=Attrs::new().with("data-testid", "conflict-back")>
                    {icon(&["M19 12H5", "M12 19l-7-7 7-7"], 16, "2")}{back}
                </Button>
                <span aria-hidden="true" style="width: 1px; height: 22px; background: var(--border);"></span>
                <h1 style=h1.to_style()>{t(keys::conflicts::TITLE, &[])}</h1>
                <span style="flex: 1;"></span>
                {navigator}
            </div>
            {tabs}
            <div style=scroll.to_style()>{body}</div>
            {footer}
        </main>
    }
    .into_any()
}

/// What the review offers for `entry` now: its kind's choices, with every version readable
/// (no choice replaces a version this computer cannot read yet).
fn offered(entry: &ConflictEntry) -> Choices {
    if entry.unreadable == 0 {
        choices(entry.kind)
    } else {
        Choices::Later
    }
}

/// What the last press left to say: a warning when nothing was kept or the disk complained,
/// else what a bulk keep did.
fn outcome_callout(outcome: KeepOutcome) -> AnyView {
    let style = measure().merge(&crate::css! {
        "margin-bottom": 12,
        "overflow-wrap": "anywhere",
    });
    let (tone, test_id, lead, text, more) = match outcome {
        KeepOutcome::NotKept(why) => (
            CalloutTone::Warn,
            "conflict-error",
            Some(t(keys::conflicts::KEEP_ERROR, &[])),
            why,
            None,
        ),
        KeepOutcome::DiskWarning(warning) => {
            (CalloutTone::Warn, "conflict-error", None, warning, None)
        }
        KeepOutcome::KeptAll {
            person,
            kept,
            sent,
            disk_warning,
        } => (
            CalloutTone::Info,
            "conflict-kept-all",
            None,
            kept_all_text(&person, kept, sent),
            disk_warning,
        ),
    };
    let lead = lead.map(|lead| view! { <strong>{lead}</strong>" " });
    let more = more.map(|more| view! { <p style="margin: 6px 0 0;">{more}</p> });
    view! {
        <Callout tone=tone style=style
            attrs=Attrs::new().with("role", "alert").with("data-testid", test_id)>
            {lead}{text}{more}
        </Callout>
    }
    .into_any()
}

fn none(text: String) -> impl IntoView {
    let style = measure().merge(&crate::css! {
        "font-size": "var(--fs-ui)",
        "color": "var(--text-tertiary)",
    });
    view! { <p data-testid="conflict-none" style=style.to_style()>{text}</p> }
}

/// tC4's `Navigator`: "Conflict n of m", Previous and Next. A step off either end of the
/// list is off, and so is every step while a keep is in flight.
fn navigator(
    actions: &Actions,
    entries: Vec<ConflictEntry>,
    index: usize,
    m: usize,
    busy: Signal<bool>,
) -> AnyView {
    let position = crate::css! {
        "font-size": "var(--fs-ui-sm)",
        "font-weight": "var(--fw-heavy)",
        "color": "var(--uw-slate)",
    };
    let (n, total) = ((index + 1).to_string(), m.to_string());
    let label = t(keys::conflicts::POSITION, &[("n", &(index + 1)), ("m", &m)]);
    let step = |to: Option<usize>| {
        let actions = actions.clone();
        let target = to.and_then(|i| entries.get(i).map(|e| (i, e.conflict.clone())));
        let off = target.is_none();
        let disabled = Signal::derive(move || busy.get() || off);
        let on_click = Callback::new(move |_| {
            if let Some((i, conflict)) = target.clone() {
                actions.show_conflict(conflict, i);
            }
        });
        (disabled, on_click)
    };
    let (prev_off, prev) = step(index.checked_sub(1));
    let (next_off, next) = step(Some(index + 1).filter(|i| *i < m));
    view! {
        <span aria-live="polite" data-testid="conflict-position" data-n=n data-m=total style=position.to_style()>{label}</span>
        <Button variant=ButtonVariant::Secondary size=Size::Sm disabled=prev_off on_click=prev
            style=crate::css! { "padding": "0 16px 0 12px" }
            attrs=Attrs::new().with("data-testid", "conflict-previous")>
            {icon(&["M15 18l-6-6 6-6"], 14, "2.2")}{t(keys::conflicts::PREVIOUS, &[])}
        </Button>
        <Button variant=ButtonVariant::Secondary size=Size::Sm disabled=next_off on_click=next
            style=crate::css! { "padding": "0 12px 0 16px" }
            attrs=Attrs::new().with("data-testid", "conflict-next")>
            {t(keys::conflicts::NEXT, &[])}{icon(&["M9 18l6-6-6-6"], 14, "2.2")}
        </Button>
    }
    .into_any()
}

/// The tabs, drawn only while a kind tC4 does not review holds something (Q-9).
fn tab_strip(on_tab: Callback<ReviewTab>, tabs: &[TabCount], shown: ReviewTab) -> Option<AnyView> {
    if tabs.is_empty() {
        return None;
    }
    let strip = crate::css! {
        "flex": "none",
        "display": "flex",
        "gap": 6,
        "padding": "10px 24px",
        "background": "var(--surface-card)",
        "border-bottom": HAIR,
    };
    let buttons = tabs
        .iter()
        .map(|count| {
            let (tab, n) = (count.tab, count.count);
            let selected = tab == shown;
            let style = crate::css! {
                "display": "inline-flex",
                "align-items": "center",
                "gap": 6,
                "height": 30,
                "padding": "0 12px",
                "border-radius": "var(--radius-pill)",
                "border": "var(--stroke-hair) solid var(--border-input)",
                "background": if selected { "var(--uw-ocean)" } else { "var(--surface-card)" },
                "color": if selected { "var(--text-inverse)" } else { "var(--uw-ink)" },
                "font-family": "var(--font-ui)",
                "font-size": "var(--fs-ui-sm)",
                "font-weight": "var(--fw-heavy)",
                "cursor": "pointer",
            };
            view! {
                <button type="button" role="tab" aria-selected=selected.to_string()
                    data-testid=format!("review-tab-{}", tab_id(tab)) data-count=n.to_string()
                    style=style.to_style() on:click=move |_| on_tab.run(tab)>
                    {tab_label(tab)}" · "{n.to_string()}
                </button>
            }
        })
        .collect_view();
    Some(
        view! {
            <div role="tablist" aria-label=t(keys::review::TABS, &[]) style=strip.to_style()>{buttons}</div>
        }
        .into_any(),
    )
}

fn tab_id(tab: ReviewTab) -> &'static str {
    match tab {
        ReviewTab::Text => "text",
        ReviewTab::Alignments => "alignments",
        ReviewTab::Checks => "checks",
        ReviewTab::Settings => "settings",
        ReviewTab::Detached => "detached",
        ReviewTab::Notices => "notices",
    }
}

/// The words over a conflict's versions: tC4's count of a verse's versions, or the rebuild's
/// kind of a record tC4 does not review.
fn kind_line(entry: &ConflictEntry) -> String {
    use keys::review::kind as k;
    let n = entry.versions.len();
    let key = match entry.kind {
        ConflictKind::VerseText => return t(keys::conflicts::VERSION_COUNT, &[("n", &n)]),
        ConflictKind::VerseStructure => k::VERSE_STRUCTURE,
        ConflictKind::Passage => k::PASSAGE,
        ConflictKind::StructureIssue => k::STRUCTURE_ISSUE,
        ConflictKind::Alignment => k::ALIGNMENT,
        ConflictKind::Check => k::CHECK,
        ConflictKind::CheckMoved => k::CHECK_MOVED,
        ConflictKind::Comment => k::COMMENT,
        ConflictKind::Pins => k::PINS,
        ConflictKind::Setting => k::SETTING,
        ConflictKind::ProjectMeta => k::PROJECT_META,
        ConflictKind::Versification => k::VERSIFICATION,
        ConflictKind::BookPresence => k::BOOK_PRESENCE,
        ConflictKind::BookRemovedWhileEdited => k::BOOK_REMOVED_WHILE_EDITED,
        ConflictKind::LastBookRemoved => k::LAST_BOOK_REMOVED,
        ConflictKind::BookLayout => k::BOOK_LAYOUT,
        ConflictKind::CarriedFile => k::CARRIED_FILE,
        ConflictKind::PathCase => k::PATH_CASE,
        ConflictKind::DeviceProfile => k::DEVICE_PROFILE,
        ConflictKind::Unknown => k::UNKNOWN,
        ConflictKind::BookHeader
        | ConflictKind::ChapterFront
        | ConflictKind::StoryBase
        | ConflictKind::Frame
        | ConflictKind::StoryRef => {
            return t(keys::review::VERSION_COUNT, &[("n", &n)]);
        }
    };
    format!(
        "{} · {}",
        t(key, &[]),
        t(keys::review::VERSION_COUNT, &[("n", &n)])
    )
}

/// One conflict: tC4's `ConflictVersions`, the place and its versions, newest first.
#[expect(
    clippy::too_many_arguments,
    reason = "one conflict's drawing takes the review's state, place and list as they are"
)]
fn conflict_versions(
    s: &AppState,
    actions: &Actions,
    at: &Scope,
    tab: ReviewTab,
    entry: &ConflictEntry,
    position: usize,
    all: &[ConflictEntry],
    together: usize,
    busy: Signal<bool>,
) -> AnyView {
    let reference = place_label(&entry.place);
    let group = crate::css! {
        "display": "flex",
        "flex-direction": "column",
        "gap": 12,
    };
    let group = measure().merge(&group);
    let head = crate::css! { "display": "flex", "align-items": "baseline", "gap": 12 };
    let h2 = crate::css! {
        "margin": 0,
        "font-size": "var(--fs-h2)",
        "letter-spacing": "var(--track-22)",
        "font-weight": "var(--fw-black)",
        "color": "var(--text-heading)",
    };
    let count = crate::css! {
        "font-size": "var(--fs-ui-sm)",
        "font-weight": "var(--fw-medium)",
        "color": "var(--text-tertiary)",
    };
    let legend = crate::css! {
        "display": "flex",
        "align-items": "center",
        "gap": 14,
        "font-size": "var(--fs-caption)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--uw-slate)",
    };
    let swatches = [Owner::Mine, Owner::Teammate]
        .into_iter()
        .map(|owner| {
            let swatch = crate::css! {
                "width": 14,
                "height": 14,
                "border-radius": "var(--radius-xs)",
                "background": owner.fill(),
                "border": "var(--stroke-hair) solid var(--border-input)",
            };
            let label = t(
                match owner {
                    Owner::Mine => keys::conflicts::MINE,
                    Owner::Teammate => keys::conflicts::TEAMMATES,
                },
                &[],
            );
            view! {
                <span style="display: inline-flex; align-items: center; gap: 6px;">
                    <span style=swatch.to_style()></span>
                    {label}
                </span>
            }
        })
        .collect_view();
    let ult = (tab == ReviewTab::Text)
        .then(|| ult_reference(s, at, &entry.place))
        .flatten();
    let notes = entry_notes(entry, together);
    let selected = {
        let review = s.review.clone();
        Memo::new(move |_| review.with(|r| r.as_ref().and_then(|r| r.selected.clone())))
    };
    let choosable = matches!(
        offered(entry),
        Choices::Candidate | Choices::JoinedOrSeparate
    );
    // A bulk keep is the Text tab's (STORAGE §5.9 step 3); the packet drew none elsewhere.
    let bulk = (tab == ReviewTab::Text && all.len() > 1 && offered(entry) != Choices::Later)
        .then(|| bulk_row(actions, entry, all, busy));
    let dir = dir_of(s);
    let cards = entry
        .versions
        .iter()
        .map(|v| {
            let place = (v.place != entry.place).then(|| place_label(&v.place));
            version_card(actions, v.clone(), place, selected, choosable, dir)
        })
        .collect_view();
    let history = {
        let (s, actions, at) = (s.clone(), actions.clone(), at.clone());
        let entry = entry.clone();
        let reference = reference.clone();
        move || history_panel(&s, &actions, &at, &entry, position, &reference, busy)
    };
    let label = t(keys::conflicts::VERSIONS_OF, &[("ref", &reference)]);
    view! {
        <div role="group" aria-label=label style=group.to_style()>
            <div style=head.to_style()>
                <h2 data-testid="conflict-ref" style=h2.to_style()>{reference.clone()}</h2>
                <span style=count.to_style()>{kind_line(entry)}</span>
                <span style="flex: 1;"></span>
                <span style=legend.to_style()>{swatches}</span>
            </div>
            {notes}
            {bulk}
            {ult}
            <div style="display: flex; flex-direction: column; gap: 10px; margin-top: 4px;">{cards}</div>
            {history}
        </div>
    }
    .into_any()
}

/// What a conflict says beside its versions: conflicts made by the same changes, an
/// incomplete history, versions this computer cannot read.
fn entry_notes(entry: &ConflictEntry, together: usize) -> Option<AnyView> {
    let mut lines = Vec::new();
    match together {
        0 => {}
        1 => lines.push(t(keys::review::GROUP_ONE, &[])),
        n => lines.push(t(keys::review::GROUP, &[("n", &n)])),
    }
    if entry.incomplete_history {
        lines.push(t(keys::review::INCOMPLETE, &[]));
    }
    match entry.unreadable {
        0 => {}
        1 => lines.push(t(keys::review::UNREADABLE_ONE, &[])),
        n => lines.push(t(keys::review::UNREADABLE, &[("n", &n)])),
    }
    if lines.is_empty() {
        return None;
    }
    let style = crate::css! {
        "font-size": "var(--fs-ui-sm)",
        "font-weight": "var(--fw-medium)",
        "color": "var(--tc-warn-text)",
    };
    Some(
        view! {
            <div data-testid="conflict-notes" style=style.to_style()>
                {lines.into_iter().map(|l| view! { <p style="margin: 0;">{l}</p> }).collect_view()}
            </div>
        }
        .into_any(),
    )
}

/// tC4's `UltReference`: the ULT verse above the versions, when Translate has it: the
/// conflict's book is the open book, the ULT is loaded, and the pane uses the project's own
/// numbering. Else nothing.
fn ult_reference(s: &AppState, at: &Scope, place: &ReviewPlace) -> Option<AnyView> {
    let ReviewPlace::Verse {
        book,
        chapter,
        verse,
    } = place.clone()
    else {
        return None;
    };
    // Tracked: Translate moves to the conflict's book once the review is open.
    if s.u.book.get() != Some(book) {
        return None;
    }
    let runtime = use_runtime()?;
    let sources = runtime.cache().read(Sources {
        at: at.clone(),
        book,
        chapter,
    });
    let text = Memo::new(move |_| {
        sources.with(|l| {
            let view = l.value()?;
            if !matches!(view.frame, SourceFrame::Same) {
                return None;
            }
            let pane = view.panes.iter().find(|p| p.id == "ult")?;
            let PaneText::Ready(chapter) = &pane.text else {
                return None;
            };
            let key = verse.to_string();
            chapter
                .verses
                .iter()
                .find(|v| v.key.as_str() == key)
                .map(|v| v.text.clone())
                .filter(|t| !t.is_empty())
        })
    });
    let card = crate::css! {
        "padding": "12px 16px",
        "border-radius": "var(--radius-lg)",
        "background": "var(--surface-card)",
        "border": "var(--stroke-hair) solid var(--border)",
        "display": "flex",
        "flex-direction": "column",
        "gap": 4,
    };
    let verse_style = crate::css! {
        "font-family": "var(--font-scripture)",
        "font-size": "var(--fs-verse-sm)",
        "line-height": "var(--lh-verse-sm)",
        "color": "var(--text-scripture)",
    };
    Some(
        (move || {
            text.get().map(|text| {
                let source = format!(
                    "{} · {}",
                    t(keys::source::ULT, &[]),
                    t(keys::source::ult::NAME, &[])
                );
                view! {
                    <div data-testid="conflict-ult" style=card.to_style()>
                        <Overline tone=OverlineTone::Muted>{source}</Overline>
                        <span lang="en" style=verse_style.to_style()>{text}</span>
                    </div>
                }
            })
        })
        .into_any(),
    )
}

/// tC4's `VersionCard`. Only the Text tab's verse versions are choices; the others show who
/// made what when, and their choices come with C7b.
fn version_card(
    actions: &Actions,
    version: Version,
    place: Option<String>,
    selected: Memo<Option<tc_api::ids::ChangeRef>>,
    choosable: bool,
    dir: &'static str,
) -> AnyView {
    let owner = Owner::of(&version.by);
    let label = made_by(&version.by);
    let date = version_date(version.at.0);
    let change = version.change.clone();
    let is_selected = {
        let change = change.clone();
        Memo::new(move |_| selected.with(|s| s.as_ref() == Some(&change)))
    };
    let faded = Memo::new(move |_| selected.with(Option::is_some) && !is_selected.get());
    let style = move || {
        let on = is_selected.get();
        crate::css! {
            "box-sizing": "border-box",
            "width": "100%",
            "text-align": "start",
            "cursor": if choosable { "pointer" } else { "default" },
            "font-family": "var(--font-ui)",
            "background": owner.fill(),
            "border": format!("2px solid {}", if on { "var(--uw-ocean)" } else { "transparent" }),
            "border-radius": "var(--radius-lg)",
            "padding": "15px 19px",
            "box-shadow": if on { "var(--shadow-hover)" } else { "none" },
            "display": "flex",
            "gap": 14,
            "align-items": "flex-start",
        }
        .to_style()
    };
    let mark = move || {
        if is_selected.get() {
            let ring = crate::css! {
                "width": 22,
                "height": 22,
                "border-radius": "var(--radius-pill)",
                "background": "var(--uw-ocean)",
                "color": "var(--text-inverse)",
                "display": "inline-flex",
                "align-items": "center",
                "justify-content": "center",
            };
            view! { <span style=ring.to_style()>{icon(&["M5 12.5l4.5 4.5L19 7.5"], 14, "2.6")}</span> }
                .into_any()
        } else {
            let edge = if faded.get() {
                "var(--uw-slate)"
            } else {
                "var(--uw-steel)"
            };
            let ring = crate::css! {
                "display": "block",
                "width": 22,
                "height": 22,
                "box-sizing": "border-box",
                "border-radius": "var(--radius-pill)",
                "border": format!("var(--stroke-selected) solid {edge}"),
                "background": "var(--surface-card)",
            };
            view! { <span style=ring.to_style()></span> }.into_any()
        }
    };
    let mut pill = crate::css! {
        "display": "inline-flex",
        "align-items": "center",
        "padding": "3px 10px",
        "border-radius": "var(--radius-pill)",
        "font-size": "var(--fs-label)",
        "font-weight": "var(--fw-heavy)",
        "letter-spacing": ".02em",
    };
    pill.extend(&match owner {
        Owner::Mine => crate::css! {
            "background": "var(--uw-ocean)",
            "color": "var(--text-inverse)",
        },
        Owner::Teammate => crate::css! {
            "background": "var(--surface-card)",
            "color": "var(--uw-ink)",
            "border": "var(--stroke-hair) solid var(--border-input)",
        },
    });
    let date_style = crate::css! {
        "font-size": "var(--fs-caption)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--uw-ink)",
    };
    let text_style = move || {
        let faded = faded.get();
        crate::css! {
            "font-family": "var(--font-scripture)",
            "font-size": if faded { "var(--fs-verse)" } else { "var(--fs-verse-md)" },
            "line-height": if faded { "var(--lh-verse)" } else { "var(--lh-verse-md)" },
            "color": if faded { "var(--uw-slate)" } else { "var(--text-scripture)" },
            "white-space": "pre-wrap",
        }
        .to_style()
    };
    let text = match &version.value {
        VersionValue::Text(text) => Some(text.trim().to_owned()),
        VersionValue::Joined { by } => Some(t(keys::review::value::JOINED, &[("by", by)])),
        VersionValue::Absent => Some(t(keys::review::value::ABSENT, &[])),
        VersionValue::Decision(_) | VersionValue::MovedTo { .. } => {
            Some(decision_lines(&version.value).join("\n"))
        }
        VersionValue::NotShown => None,
    };
    let text = text.map(|text| {
        view! { <span data-testid="conflict-text" dir=dir style=text_style>{text}</span> }
    });
    let clock = version.clock_ahead.then(|| {
        let style = crate::css! {
            "font-size": "var(--fs-caption)",
            "font-weight": "var(--fw-bold)",
            "color": "var(--tc-warn-text)",
        };
        view! { <span data-testid="conflict-clock" style=style.to_style()>{t(keys::review::CLOCK_AHEAD, &[])}</span> }
    });
    let aria = t(
        keys::conflicts::VERSION_LABEL,
        &[("version", &label), ("date", &date)],
    );
    // A passage's versions are of several verses: each names its own.
    let place = place.map(|place| {
        view! { <span data-testid="conflict-version-place" style=date_style.to_style()>{place}</span> }
    });
    let inner = view! {
        <span style="padding-top: 1px;">{mark}</span>
        <span style="flex: 1; display: flex; flex-direction: column; gap: 8px;">
            <span style="display: flex; align-items: center; gap: 10px;">
                <span data-testid="conflict-owner" style=pill.to_style()>{label}</span>
                <span data-testid="conflict-date" style=date_style.to_style()>{date}</span>
                {place}
            </span>
            {clock}
            {text}
        </span>
    };
    let on_click = {
        let actions = actions.clone();
        move |_| {
            if choosable {
                actions.select_version(change.clone());
            }
        }
    };
    leptos::html::button()
        .attr("type", "button")
        .attr("aria-pressed", move || is_selected.get().to_string())
        .attr("aria-label", aria)
        .attr("data-testid", "conflict-version")
        .attr("data-owner", owner.attr())
        .attr("data-ts", version.change.as_str().to_owned())
        .attr("disabled", (!choosable).then_some(""))
        .attr("style", style)
        .on(leptos::ev::click, on_click)
        .child(inner)
        .into_any()
}

/// tC4's `KeepBar`: the hint, or what Keep this version does with the selected version.
fn keep_bar(
    actions: &Actions,
    entry: &ConflictEntry,
    position: usize,
    busy: Signal<bool>,
) -> AnyView {
    let s = actions.s().clone();
    let versions = entry.versions.clone();
    let kept = {
        let review = s.review.clone();
        Memo::new(move |_| {
            let selected = review.with(|r| r.as_ref().and_then(|r| r.selected.clone()))?;
            versions.iter().find(|v| v.change == selected).cloned()
        })
    };
    let n = entry.versions.len();
    let bar = crate::css! {
        "flex": "none",
        "box-sizing": "border-box",
        "height": 68,
        "display": "flex",
        "align-items": "center",
        "padding": "0 24px",
        "background": "var(--surface-card)",
        "border-top": HAIR,
    };
    let inner = measure().merge(&crate::css! {
        "width": "100%",
        "display": "flex",
        "align-items": "center",
        "gap": 12,
    });
    let line = move || match kept.get() {
        Some(version) => {
            let (before, after) = consequence(n);
            let style = crate::css! {
                "font-size": "var(--fs-ui-sm)",
                "line-height": 1.45,
                "font-weight": "var(--fw-medium)",
                "color": "var(--uw-ink)",
            };
            view! {
                <span data-testid="conflict-consequence" style=style.to_style()>
                    {before}<strong style="font-weight: var(--fw-heavy);">{kept_name(&version)}</strong>{after}
                </span>
            }
            .into_any()
        }
        None => {
            let style = crate::css! {
                "font-size": "var(--fs-ui-sm)",
                "font-weight": "var(--fw-bold)",
                "color": "var(--uw-slate)",
            };
            view! { <span style=style.to_style()>{t(keys::conflicts::HINT, &[])}</span> }.into_any()
        }
    };
    let cancel = {
        let actions = actions.clone();
        move || {
            kept.get().map(|_| {
                let actions = actions.clone();
                view! {
                    <Button variant=ButtonVariant::Secondary on_click=Callback::new(move |_| actions.cancel_selection())
                        attrs=Attrs::new().with("data-testid", "conflict-cancel")>
                        {t(keys::conflicts::CANCEL, &[])}
                    </Button>
                }
            })
        }
    };
    let off = Signal::derive(move || kept.with(Option::is_none) || busy.get());
    let keep = {
        let (actions, entry) = (actions.clone(), entry.clone());
        Callback::new(move |_| actions.keep_version(entry.clone(), position))
    };
    view! {
        <div style=bar.to_style()>
            <div style=inner.to_style()>
                {line}
                <span style="flex: 1;"></span>
                {cancel}
                <Button disabled=off on_click=keep attrs=Attrs::new().with("data-testid", "conflict-keep")>
                    {t(keys::conflicts::KEEP, &[])}
                </Button>
            </div>
        </div>
    }
    .into_any()
}

/// The footer's frame: the keep bar's, with its contents in a row.
fn footer_frame(hint: String, buttons: AnyView, test_id: &'static str) -> AnyView {
    let bar = crate::css! {
        "flex": "none",
        "box-sizing": "border-box",
        "min-height": 68,
        "display": "flex",
        "align-items": "center",
        "padding": "10px 24px",
        "background": "var(--surface-card)",
        "border-top": HAIR,
    };
    let inner = measure().merge(&crate::css! {
        "width": "100%",
        "display": "flex",
        "align-items": "center",
        "flex-wrap": "wrap",
        "gap": 12,
    });
    let hint_style = crate::css! {
        "font-size": "var(--fs-ui-sm)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--uw-slate)",
    };
    view! {
        <div data-testid=test_id style=bar.to_style()>
            <div style=inner.to_style()>
                <span style=hint_style.to_style()>{hint}</span>
                <span style="flex: 1;"></span>
                {buttons}
            </div>
        </div>
    }
    .into_any()
}

/// A passage's footer (STORAGE §8.3): *Keep Maria's passage* for each person with a version,
/// every verse of the passage as that person left it.
fn passage_bar(
    actions: &Actions,
    entry: &ConflictEntry,
    position: usize,
    busy: Signal<bool>,
) -> AnyView {
    let buttons = persons(entry)
        .into_iter()
        .map(|person| {
            let (actions, entry) = (actions.clone(), entry.clone());
            let label = keep_passage_label(&person);
            let who = person.who.clone();
            let on_click = Callback::new(move |_| actions.keep_passage(entry.clone(), position, &person));
            view! {
                <Button disabled=busy on_click=on_click
                    attrs=Attrs::new().with("data-testid", "conflict-keep-passage").with("data-who", format!("{who:?}"))>
                    {label}
                </Button>
            }
        })
        .collect_view()
        .into_any();
    footer_frame(
        t(keys::review::PASSAGE_HINT, &[]),
        buttons,
        "conflict-passage-bar",
    )
}

/// A verse-structure footer (STORAGE §8.3): *Keep the joined verses* or *Keep the verses
/// separate*, with the version clicked as the pick where a verse holds several.
fn structure_bar(
    actions: &Actions,
    entry: &ConflictEntry,
    position: usize,
    busy: Signal<bool>,
) -> AnyView {
    // Off until a version is clicked: the fold needs the pick where a verse holds several,
    // and a press without one would come back refused.
    let picked = {
        let review = actions.s().review.clone();
        Memo::new(move |_| review.with(|r| r.as_ref().is_some_and(|r| r.selected.is_some())))
    };
    let off = Signal::derive(move || busy.get() || !picked.get());
    let joined = {
        let (actions, entry) = (actions.clone(), entry.clone());
        Callback::new(move |_| actions.keep_joined(entry.clone(), position))
    };
    let separate = {
        let (actions, entry) = (actions.clone(), entry.clone());
        Callback::new(move |_| actions.keep_separate(entry.clone(), position))
    };
    let buttons = view! {
        <Button variant=ButtonVariant::Secondary disabled=off on_click=separate
            attrs=Attrs::new().with("data-testid", "conflict-keep-separate")>
            {t(keys::review::KEEP_SEPARATE, &[])}
        </Button>
        <Button variant=ButtonVariant::Secondary disabled=off on_click=joined attrs=Attrs::new().with("data-testid", "conflict-keep-joined")>
            {t(keys::review::KEEP_JOINED, &[])}
        </Button>
    }
    .into_any();
    footer_frame(
        t(keys::review::STRUCTURE_HINT, &[]),
        buttons,
        "conflict-structure-bar",
    )
}

/// A check's footer (STORAGE §8.3): *Keep mine* and *Keep Ana's* on each version, *Choose
/// per line* while parts differ, which opens a row of versions for each part, and *Keep both*
/// while the comments differ.
fn check_bar(
    actions: &Actions,
    entry: &ConflictEntry,
    position: usize,
    busy: Signal<bool>,
) -> AnyView {
    let picks: RwSignal<Option<Vec<PartPick>>> = RwSignal::new(None);
    let send = {
        let (actions, entry) = (actions.clone(), entry.clone());
        move |choice: KeepChoice| actions.keep_choice(entry.clone(), position, choice)
    };
    let buttons = check_buttons(entry)
        .into_iter()
        .map(|button| match button {
            CheckButton::Keep { label, change } => {
                let send = send.clone();
                let ts = change.as_str().to_owned();
                let on_click =
                    Callback::new(move |_| send(KeepChoice::Candidate { change: change.clone() }));
                view! {
                    <Button variant=ButtonVariant::Secondary disabled=busy on_click=on_click
                        attrs=Attrs::new().with("data-testid", "conflict-keep-check").with("data-ts", ts)>
                        {label}
                    </Button>
                }
                .into_any()
            }
            CheckButton::PerLine => {
                let on_click = Callback::new(move |_| {
                    picks.update(|p| *p = if p.is_some() { None } else { Some(Vec::new()) });
                });
                view! {
                    <Button variant=ButtonVariant::Secondary disabled=busy on_click=on_click
                        attrs=Attrs::new().with("data-testid", "conflict-per-line")>
                        {t(keys::review::PER_LINE, &[])}
                    </Button>
                }
                .into_any()
            }
            CheckButton::KeepBoth => {
                let send = send.clone();
                let on_click = Callback::new(move |_| send(KeepChoice::KeepBothComments));
                view! {
                    <Button variant=ButtonVariant::Secondary disabled=busy on_click=on_click
                        attrs=Attrs::new().with("data-testid", "conflict-keep-both")>
                        {t(keys::review::KEEP_BOTH, &[])}
                    </Button>
                }
                .into_any()
            }
        })
        .collect_view()
        .into_any();
    let hint = if entry.disputed.contains(&CheckPart::Comment) {
        t(keys::review::KEEP_BOTH_HINT, &[])
    } else {
        t(keys::review::CHECK_HINT, &[])
    };
    let panel = {
        let entry = entry.clone();
        move || {
            picks.get().map(|current| per_line_panel(&entry, &current, picks, busy, send.clone()))
        }
    };
    view! {
        <div style="flex: none;">
            {panel}
            {footer_frame(hint, buttons, "conflict-check-bar")}
        </div>
    }
    .into_any()
}

/// *Choose per line*: for each part the versions set differently, a button per version, and
/// *Keep these* once every part has one.
fn per_line_panel(
    entry: &ConflictEntry,
    current: &[PartPick],
    picks: RwSignal<Option<Vec<PartPick>>>,
    busy: Signal<bool>,
    send: impl Fn(KeepChoice) + Clone + Send + Sync + 'static,
) -> AnyView {
    let style = crate::css! {
        "box-sizing": "border-box",
        "padding": "12px 24px",
        "background": "var(--surface-card)",
        "border-top": HAIR,
    };
    let inner = measure().merge(&crate::css! {
        "display": "flex",
        "flex-direction": "column",
        "gap": 8,
    });
    let rows = entry
        .disputed
        .iter()
        .map(|part| {
            let versions = entry
                .versions
                .iter()
                .map(|v| {
                    let chosen = current
                        .iter()
                        .any(|p| &p.part == part && p.change == v.change);
                    let label = match Owner::of(&v.by) {
                        Owner::Mine => t(keys::review::PER_LINE_PICK_MINE, &[]),
                        Owner::Teammate => t(
                            keys::review::PER_LINE_PICK,
                            &[("person", &made_by(&v.by))],
                        ),
                    };
                    let pick = PartPick {
                        part: part.clone(),
                        change: v.change.clone(),
                    };
                    let on_click = Callback::new(move |_| {
                        let pick = pick.clone();
                        picks.update(|p| {
                            if let Some(p) = p {
                                p.retain(|old| old.part != pick.part);
                                p.push(pick);
                            }
                        });
                    });
                    let variant = if chosen {
                        ButtonVariant::Ocean
                    } else {
                        ButtonVariant::Secondary
                    };
                    view! {
                        <Button variant=variant size=Size::Sm disabled=busy on_click=on_click
                            attrs=Attrs::new().with("data-testid", "conflict-part-pick")
                                .with("aria-pressed", chosen.to_string())
                                .with("data-ts", v.change.as_str().to_owned())>
                            {label}
                        </Button>
                    }
                })
                .collect_view();
            view! {
                <div data-testid="conflict-part" style="display: flex; align-items: center; flex-wrap: wrap; gap: 8px;">
                    <strong style="min-width: 120px; font-size: var(--fs-ui-sm); color: var(--text-heading);">{part_label(part)}</strong>
                    {versions}
                </div>
            }
        })
        .collect_view();
    let choice = per_line_choice(entry, current);
    let off = Signal::derive({
        let ready = choice.is_some();
        move || busy.get() || !ready
    });
    let on_keep = Callback::new(move |_| {
        if let Some(choice) = choice.clone() {
            send(choice);
        }
    });
    view! {
        <div data-testid="conflict-per-line-panel" style=style.to_style()>
            <div style=inner.to_style()>
                <span style="font-size: var(--fs-ui-sm); color: var(--uw-slate);">{t(keys::review::PER_LINE_HINT, &[])}</span>
                {rows}
                <span style="display: flex; justify-content: flex-end;">
                    <Button disabled=off on_click=on_keep attrs=Attrs::new().with("data-testid", "conflict-per-line-keep")>
                        {t(keys::review::PER_LINE_KEEP, &[])}
                    </Button>
                </span>
            </div>
        </div>
    }
    .into_any()
}

/// A moved check's footer (STORAGE §8.3): *Keep Pedro's check where it was* and *Apply
/// Pedro's change to the moved check*.
fn moved_bar(
    actions: &Actions,
    entry: &ConflictEntry,
    position: usize,
    busy: Signal<bool>,
) -> AnyView {
    let Some((apply, keep, change)) = moved_choices(entry) else {
        return later_bar();
    };
    let person = entry
        .versions
        .iter()
        .find(|v| v.change == change)
        .map(|v| made_by(&v.by))
        .unwrap_or_default();
    let on_keep = {
        let (actions, entry) = (actions.clone(), entry.clone());
        Callback::new(move |_| {
            actions.keep_choice(
                entry.clone(),
                position,
                KeepChoice::Candidate {
                    change: change.clone(),
                },
            );
        })
    };
    let on_apply = {
        let (actions, entry) = (actions.clone(), entry.clone());
        Callback::new(move |_| {
            actions.keep_choice(entry.clone(), position, KeepChoice::ApplyToMovedCheck);
        })
    };
    let buttons = view! {
        <Button variant=ButtonVariant::Secondary disabled=busy on_click=on_keep
            attrs=Attrs::new().with("data-testid", "conflict-keep-where-it-was")>
            {keep}
        </Button>
        <Button disabled=busy on_click=on_apply attrs=Attrs::new().with("data-testid", "conflict-apply-to-moved")>
            {apply}
        </Button>
    }
    .into_any();
    footer_frame(
        t(keys::review::MOVED_HINT, &[("person", &person)]),
        buttons,
        "conflict-moved-bar",
    )
}

/// *Keep all of Maria's* for each person with a version here, over every conflict the tab
/// lists (STORAGE §5.9 step 3); the reply says how many were left for a choice of their own.
fn bulk_row(
    actions: &Actions,
    entry: &ConflictEntry,
    all: &[ConflictEntry],
    busy: Signal<bool>,
) -> AnyView {
    let row = crate::css! {
        "display": "flex",
        "align-items": "center",
        "flex-wrap": "wrap",
        "gap": 8,
    };
    let buttons = persons(entry)
        .into_iter()
        .map(|person: PersonLabel| {
            let (actions, all) = (actions.clone(), all.to_vec());
            let label = keep_all_label(&person);
            let who = person.who.clone();
            let on_click = Callback::new(move |_| actions.keep_all(&person, all.clone()));
            view! {
                <Button variant=ButtonVariant::Secondary size=Size::Sm disabled=busy on_click=on_click
                    attrs=Attrs::new().with("data-testid", "conflict-keep-all").with("data-who", format!("{who:?}"))>
                    {label}
                </Button>
            }
        })
        .collect_view();
    view! { <div data-testid="conflict-bulk" style=row.to_style()>{buttons}</div> }.into_any()
}

/// The footer of a conflict this build has no choice for: every version is kept (C7b).
fn later_bar() -> AnyView {
    let bar = crate::css! {
        "flex": "none",
        "box-sizing": "border-box",
        "min-height": 68,
        "display": "flex",
        "align-items": "center",
        "padding": "0 24px",
        "background": "var(--surface-card)",
        "border-top": HAIR,
    };
    let text = measure().merge(&crate::css! {
        "width": "100%",
        "font-size": "var(--fs-ui-sm)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--uw-slate)",
    });
    view! {
        <div style=bar.to_style()>
            <span data-testid="conflict-later" style=text.to_style()>{t(keys::review::LATER, &[])}</span>
        </div>
    }
    .into_any()
}

/// A History toggle and, while it is open, every version of the record, newest first (#78).
fn history_panel(
    s: &AppState,
    actions: &Actions,
    at: &Scope,
    entry: &ConflictEntry,
    position: usize,
    reference: &str,
    busy: Signal<bool>,
) -> AnyView {
    let record = &entry.record;
    let open = {
        let (review, record) = (s.review.clone(), record.clone());
        Memo::new(move |_| {
            review.with(|r| r.as_ref().and_then(|r| r.history.as_ref()) == Some(&record))
        })
    };
    let toggle = {
        let (actions, record) = (actions.clone(), record.clone());
        Callback::new(move |_| actions.toggle_history(record.clone()))
    };
    let button = move || {
        let label = if open.get() {
            t(keys::review::HISTORY_HIDE, &[])
        } else {
            t(keys::review::HISTORY, &[])
        };
        view! {
            <Button variant=ButtonVariant::Ghost size=Size::Sm on_click=toggle
                style=crate::css! { "align-self": "flex-start", "padding": 0, "height": "auto" }
                attrs=Attrs::new().with("data-testid", "conflict-history")>
                {label}
            </Button>
        }
    };
    let list = {
        let (actions, at, entry) = (actions.clone(), at.clone(), entry.clone());
        let reference = reference.to_owned();
        move || {
            open.get()
                .then(|| history_list(&actions, &at, &entry, position, &reference, busy))
        }
    };
    view! { {button}{list} }.into_any()
}

/// Every version of the record, newest first, each with *Restore this version* but the one
/// shown: that version kept over every version of the record, as a choice on the conflict.
fn history_list(
    actions: &Actions,
    at: &Scope,
    entry: &ConflictEntry,
    position: usize,
    reference: &str,
    busy: Signal<bool>,
) -> Option<AnyView> {
    let runtime = use_runtime()?;
    let read = runtime.cache().read(History {
        at: at.clone(),
        record: entry.record.clone(),
    });
    let title = t(keys::review::HISTORY_OF, &[("ref", &reference)]);
    let label = title.clone();
    let (actions, entry) = (actions.clone(), entry.clone());
    let rows = move || {
        read.with(|l| {
            l.value().map(|view: &Arc<tc_api::conflicts::HistoryView>| {
                view.versions
                    .iter()
                    .map(|v| {
                        let restore = (!v.version.shown).then(|| {
                            let (actions, entry) = (actions.clone(), entry.clone());
                            let from = v.version.change.clone();
                            let on_click = Callback::new(move |_| {
                                actions.restore_version(entry.clone(), position, from.clone());
                            });
                            view! {
                                <Button variant=ButtonVariant::Ghost size=Size::Sm disabled=busy on_click=on_click
                                    style=crate::css! { "align-self": "flex-start", "padding": 0, "height": "auto" }
                                    attrs=Attrs::new().with("data-testid", "history-restore").with("data-ts", v.version.change.as_str().to_owned())>
                                    {t(keys::review::RESTORE, &[])}
                                </Button>
                            }
                        });
                        let text = match &v.version.value {
                            VersionValue::Text(text) => text.trim().to_owned(),
                            VersionValue::Joined { by } => {
                                t(keys::review::value::JOINED, &[("by", by)])
                            }
                            VersionValue::Absent => t(keys::review::value::ABSENT, &[]),
                            VersionValue::Decision(_) | VersionValue::MovedTo { .. } => {
                                decision_lines(&v.version.value).join("\n")
                            }
                            VersionValue::NotShown => String::new(),
                        };
                        let meta = format!(
                            "{} · {} · {}",
                            made_by(&v.version.by),
                            version_date(v.version.at.0),
                            cause_label(v.cause)
                        );
                        view! {
                            <li data-testid="history-version" data-shown=v.version.shown.to_string()
                                style="display: flex; flex-direction: column; gap: 2px; padding: 8px 0; border-top: var(--stroke-hair) solid var(--border-hair);">
                                <span style="font-size: var(--fs-caption); font-weight: var(--fw-bold); color: var(--uw-slate);">{meta}</span>
                                <span style="font-family: var(--font-scripture); font-size: var(--fs-verse); line-height: var(--lh-verse); color: var(--text-scripture); white-space: pre-wrap;">{text}</span>
                                {restore}
                            </li>
                        }
                    })
                    .collect_view()
            })
        })
    };
    let style = crate::css! {
        "padding": "12px 16px",
        "border-radius": "var(--radius-lg)",
        "background": "var(--surface-card)",
        "border": "var(--stroke-hair) solid var(--border)",
    };
    Some(
        view! {
            <section data-testid="conflict-history-list" aria-label=label style=style.to_style()>
                <Overline tone=OverlineTone::Muted>{title}</Overline>
                <ul style="list-style: none; margin: 6px 0 0; padding: 0;">{rows}</ul>
            </section>
        }
        .into_any(),
    )
}

fn list_box(rows: Vec<(String, String)>, test_id: &'static str, about: Option<String>) -> AnyView {
    let style = measure().merge(&crate::css! {
        "display": "flex",
        "flex-direction": "column",
        "gap": 10,
    });
    let about = about.map(|about| {
        view! { <p style="margin: 0; font-size: var(--fs-ui-sm); color: var(--text-tertiary);">{about}</p> }
    });
    let rows = rows
        .into_iter()
        .map(|(head, line)| {
            view! {
                <div data-testid=test_id style="padding: 12px 16px; border-radius: var(--radius-lg); background: var(--surface-card); border: var(--stroke-hair) solid var(--border); display: flex; flex-direction: column; gap: 4px;">
                    <strong style="font-size: var(--fs-ui-sm); color: var(--text-heading);">{head}</strong>
                    <span style="font-size: var(--fs-ui-sm); color: var(--uw-ink);">{line}</span>
                </div>
            }
        })
        .collect_view();
    view! { <div style=style.to_style()>{about}{rows}</div> }.into_any()
}

/// The records left with no place (STORAGE §7.7), each with where it was made for.
fn detached(entries: &[DetachedEntry]) -> AnyView {
    if entries.is_empty() {
        return none(t(keys::review::NONE, &[])).into_any();
    }
    let rows = entries
        .iter()
        .map(|e| {
            let what = t(
                match e.what {
                    DetachedKind::Decision => keys::review::detached::DECISION,
                    DetachedKind::Alignment => keys::review::detached::ALIGNMENT,
                    DetachedKind::Comment => keys::review::detached::COMMENT,
                    DetachedKind::Other => keys::review::detached::OTHER,
                },
                &[],
            );
            (place_label(&e.place), what)
        })
        .collect();
    list_box(
        rows,
        "review-detached",
        Some(t(keys::review::detached::ABOUT, &[])),
    )
}

/// The notices, one sentence each (STORAGE §8.5).
fn notices(entries: &[NoticeEntry]) -> AnyView {
    if entries.is_empty() {
        return none(t(keys::review::NONE, &[])).into_any();
    }
    let rows = entries
        .iter()
        .map(|e| (String::new(), notice_text(&e.notice)))
        .collect();
    list_box(rows, "review-notice", None)
}

/// One screen of the H5 packet (C7b step 2), drawn from its fixed data in the review's own
/// style. Its buttons do nothing: the owner signs the screens off before any is wired. The
/// boot error is the app's own component, which the probe page draws itself.
pub fn q9_screen(screen: Q9Screen) -> AnyView {
    match screen {
        Q9Screen::Review(review) => q9_review(review),
        Q9Screen::Received(sheet) => q9_sheet(sheet, "q9-received", true),
        Q9Screen::DraftOffer(offer) => q9_sheet(offer, "q9-draft-offer", true),
        Q9Screen::ReadOnly(banner) => q9_sheet(banner, "q9-read-only", false),
        Q9Screen::BootError(_) => ().into_any(),
    }
}

/// A packet button: `data-choice` carries its words, so the packet's JSON lists every choice.
fn q9_choice(label: String, primary: bool, size: Size) -> AnyView {
    let variant = if primary {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Secondary
    };
    view! {
        <Button variant=variant size=size
            attrs=Attrs::new().with("data-testid", "q9-choice").with("data-choice", label.clone())>
            {label}
        </Button>
    }
    .into_any()
}

/// A row of buttons, the last one primary when `primary_last`.
fn q9_choices(labels: Vec<String>, primary_last: bool, size: Size) -> AnyView {
    let last = labels.len().saturating_sub(1);
    labels
        .into_iter()
        .enumerate()
        .map(|(i, label)| q9_choice(label, primary_last && i == last, size))
        .collect_view()
        .into_any()
}

fn q9_review(review: Q9Review) -> AnyView {
    let root = crate::css! {
        "flex": 1,
        "min-height": "100vh",
        "display": "flex",
        "flex-direction": "column",
        "background": "var(--surface-app)",
    };
    let bar = crate::css! {
        "height": 53,
        "flex": "none",
        "box-sizing": "border-box",
        "display": "flex",
        "align-items": "center",
        "gap": 14,
        "padding": "0 24px",
        "background": "var(--surface-card)",
        "border-bottom": "var(--stroke-hair) solid var(--border)",
    };
    let h1 = crate::css! {
        "margin": 0,
        "font-size": "var(--fs-title)",
        "letter-spacing": "var(--track-17)",
        "font-weight": "var(--fw-heavy)",
        "color": "var(--text-heading)",
    };
    let position = crate::css! {
        "font-size": "var(--fs-ui-sm)",
        "font-weight": "var(--fw-heavy)",
        "color": "var(--uw-slate)",
    };
    let scroll = crate::css! {
        "flex": 1,
        "min-height": 0,
        "overflow": "auto",
        "padding": "22px 24px",
    };
    let count = review
        .tabs
        .iter()
        .find(|c| c.tab == review.tab)
        .map_or(0, |c| c.count);
    let navigator = matches!(review.body, Q9Body::Conflict(_)).then(|| {
        let label = t(keys::conflicts::POSITION, &[("n", &1), ("m", &count)]);
        view! {
            <span style=position.to_style()>{label}</span>
            <Button variant=ButtonVariant::Secondary size=Size::Sm disabled=true>
                {icon(&["M15 18l-6-6 6-6"], 14, "2.2")}{t(keys::conflicts::PREVIOUS, &[])}
            </Button>
            <Button variant=ButtonVariant::Secondary size=Size::Sm disabled=count < 2>
                {t(keys::conflicts::NEXT, &[])}{icon(&["M9 18l6-6-6-6"], 14, "2.2")}
            </Button>
        }
    });
    let tabs = tab_strip(
        Callback::new(|_| ()),
        &tabs_shown(&review.tabs, review.tab),
        review.tab,
    );
    let (body, footer) = match review.body {
        Q9Body::Conflict(conflict) => {
            let footer = (!conflict.footer.is_empty() || !conflict.hint.is_empty()).then(|| {
                footer_frame(
                    conflict.hint.clone(),
                    q9_choices(conflict.footer.clone(), true, Size::default()),
                    "q9-footer",
                )
            });
            (q9_conflict(conflict), footer)
        }
        Q9Body::Rows { about, rows } => (q9_rows(about, rows), None),
    };
    let ghost = crate::css! { "padding": 0, "height": "auto", "font-size": "var(--fs-ui-sm)" };
    let back = back_label(BackTo::Translate(Some(tc_core::BookId::Titus), 1));
    view! {
        <main data-testid="q9-review" data-tab=tab_id(review.tab) style=root.to_style()>
            <div style=bar.to_style()>
                <Button variant=ButtonVariant::Ghost style=ghost>
                    {icon(&["M19 12H5", "M12 19l-7-7 7-7"], 16, "2")}{back}
                </Button>
                <span aria-hidden="true" style="width: 1px; height: 22px; background: var(--border);"></span>
                <h1 style=h1.to_style()>{t(keys::conflicts::TITLE, &[])}</h1>
                <span style="flex: 1;"></span>
                {navigator}
            </div>
            {tabs}
            <div style=scroll.to_style()>{body}</div>
            {footer}
        </main>
    }
    .into_any()
}

fn q9_conflict(conflict: Q9Conflict) -> AnyView {
    let group = measure().merge(&crate::css! {
        "display": "flex",
        "flex-direction": "column",
        "gap": 12,
    });
    let head = crate::css! { "display": "flex", "align-items": "baseline", "gap": 12, "flex-wrap": "wrap" };
    let h2 = crate::css! {
        "margin": 0,
        "font-size": "var(--fs-h2)",
        "letter-spacing": "var(--track-22)",
        "font-weight": "var(--fw-black)",
        "color": "var(--text-heading)",
    };
    let kind = crate::css! {
        "font-size": "var(--fs-ui-sm)",
        "font-weight": "var(--fw-medium)",
        "color": "var(--text-tertiary)",
    };
    let about_style = crate::css! {
        "margin": 0,
        "font-size": "var(--fs-ui)",
        "line-height": 1.45,
        "color": "var(--uw-ink)",
    };
    let about = conflict
        .about
        .map(|about| view! { <p data-testid="q9-about" style=about_style.to_style()>{about}</p> });
    let bulk = (!conflict.bulk.is_empty()).then(|| {
        view! {
            <div data-testid="q9-bulk" style="display: flex; flex-wrap: wrap; gap: 8px;">
                {q9_choices(conflict.bulk, false, Size::Sm)}
            </div>
        }
    });
    let cards = conflict.versions.into_iter().map(q9_version).collect_view();
    view! {
        <div role="group" style=group.to_style()>
            <div style=head.to_style()>
                <h2 data-testid="q9-place" style=h2.to_style()>{conflict.place}</h2>
                <span style=kind.to_style()>{conflict.kind}</span>
            </div>
            {about}
            {bulk}
            <div style="display: flex; flex-direction: column; gap: 10px; margin-top: 4px;">{cards}</div>
            <Button variant=ButtonVariant::Ghost size=Size::Sm
                style=crate::css! { "align-self": "flex-start", "padding": 0, "height": "auto" }>
                {t(keys::review::HISTORY, &[])}
            </Button>
        </div>
    }
    .into_any()
}

/// A version as the Text tab's cards draw one, with the version's own buttons under it.
fn q9_version(version: Q9Version) -> AnyView {
    let card = crate::css! {
        "box-sizing": "border-box",
        "background": version.owner.fill(),
        "border-radius": "var(--radius-lg)",
        "padding": "15px 19px",
        "display": "flex",
        "flex-direction": "column",
        "gap": 8,
    };
    let mut pill = crate::css! {
        "display": "inline-flex",
        "align-items": "center",
        "padding": "3px 10px",
        "border-radius": "var(--radius-pill)",
        "font-size": "var(--fs-label)",
        "font-weight": "var(--fw-heavy)",
        "letter-spacing": ".02em",
    };
    pill.extend(&match version.owner {
        Owner::Mine => crate::css! {
            "background": "var(--uw-ocean)",
            "color": "var(--text-inverse)",
        },
        Owner::Teammate => crate::css! {
            "background": "var(--surface-card)",
            "color": "var(--uw-ink)",
            "border": "var(--stroke-hair) solid var(--border-input)",
        },
    });
    let date = crate::css! {
        "font-size": "var(--fs-caption)",
        "font-weight": "var(--fw-bold)",
        "color": "var(--uw-ink)",
    };
    let line = crate::css! {
        "font-size": "var(--fs-ui)",
        "line-height": 1.45,
        "color": "var(--text-scripture)",
    };
    let lines = version
        .lines
        .into_iter()
        .map(|l| view! { <span style=line.to_style()>{l}</span> })
        .collect_view();
    let choices = (!version.choices.is_empty()).then(|| {
        view! {
            <span style="display: flex; flex-wrap: wrap; gap: 8px; margin-top: 4px;">
                {q9_choices(version.choices, false, Size::Sm)}
            </span>
        }
    });
    view! {
        <div data-testid="q9-version" data-owner=version.owner.attr() style=card.to_style()>
            <span style="display: flex; align-items: center; gap: 10px;">
                <span style=pill.to_style()>{version.who}</span>
                <span style=date.to_style()>{version_date(version.at)}</span>
            </span>
            {lines}
            {choices}
        </div>
    }
    .into_any()
}

/// The Detached and Notices tabs: `list_box`'s rows, each with its buttons.
fn q9_rows(about: Option<String>, rows: Vec<Q9Row>) -> AnyView {
    let style = measure().merge(&crate::css! {
        "display": "flex",
        "flex-direction": "column",
        "gap": 10,
    });
    let about = about.map(|about| {
        view! { <p style="margin: 0; font-size: var(--fs-ui-sm); color: var(--text-tertiary);">{about}</p> }
    });
    let rows = rows
        .into_iter()
        .map(|row| {
            let head = (!row.head.is_empty()).then(|| {
                view! { <strong style="font-size: var(--fs-ui-sm); color: var(--text-heading);">{row.head}</strong> }
            });
            view! {
                <div data-testid="q9-row" style="padding: 12px 16px; border-radius: var(--radius-lg); background: var(--surface-card); border: var(--stroke-hair) solid var(--border); display: flex; flex-direction: column; gap: 6px;">
                    {head}
                    <span style="font-size: var(--fs-ui-sm); color: var(--uw-ink);">{row.line}</span>
                    <span style="display: flex; flex-wrap: wrap; gap: 8px;">{q9_choices(row.choices, false, Size::Sm)}</span>
                </div>
            }
        })
        .collect_view();
    view! { <div style=style.to_style()>{about}{rows}</div> }.into_any()
}

/// A sentence with its buttons: the Received sheet and the draft offer as a card over the page,
/// the read-only banner as a strip across its top.
fn q9_sheet(question: Q9Question, test_id: &'static str, card: bool) -> AnyView {
    let frame = if card {
        crate::css! {
            "box-sizing": "border-box",
            "max-width": 560,
            "margin": "48px auto",
            "padding": "20px 24px",
            "border-radius": "var(--radius-lg)",
            "background": "var(--surface-card)",
            "box-shadow": "var(--shadow-hover)",
            "display": "flex",
            "flex-direction": "column",
            "gap": 12,
        }
    } else {
        crate::css! {
            "box-sizing": "border-box",
            "padding": "12px 24px",
            "background": "var(--uw-cream)",
            "border-bottom": HAIR,
            "display": "flex",
            "align-items": "center",
            "flex-wrap": "wrap",
            "gap": 12,
        }
    };
    let sentence = crate::css! {
        "margin": 0,
        "font-size": "var(--fs-ui)",
        "line-height": 1.45,
        "font-weight": "var(--fw-bold)",
        "color": "var(--uw-ink)",
        "flex": "1 1 320px",
    };
    let lines = question
        .lines
        .into_iter()
        .map(|l| {
            view! { <p style="margin: 0; font-size: var(--fs-ui-sm); color: var(--uw-slate);">{l}</p> }
        })
        .collect_view();
    view! {
        <div style="min-height: 100vh; background: var(--surface-app);">
            <section data-testid=test_id role="region" style=frame.to_style()>
                <p style=sentence.to_style()>{question.sentence}</p>
                {lines}
                <span style="display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px;">
                    {q9_choices(question.choices, true, Size::default())}
                </span>
            </section>
        </div>
    }
    .into_any()
}
