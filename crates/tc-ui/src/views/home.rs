//! The port of tC4's `Home.jsx` (unit B2): the project picker, with a card per project, its
//! book or story tiles, the Share and Settings actions, the conflict chip, the Resume card, the
//! banners of a refused open and a failed checkpoint, and the cards of the folders that cannot
//! be opened (a declared difference from tC4, which leaves them off, §4.5).
//!
//! Home holds `home/list` (read again when the projects change); each card holds its own
//! `home/card` (read again when its summary is ready) and draws from it once it is in, else
//! from the list's copy: a redraw reads nothing, a new Home visit reads again (tC4's
//! `homeProjectCard`). Every text is a closure over `t()`, so a language preview changes it in
//! place, and the cards are keyed by folder.

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::home::{
    Card as CardRead, Door43Card, HomeCard, HomeView, LastEditView, List, ProjectKind, StartNotice,
    StoryTileView, UnusableCard,
};
use tc_api::ids::ProjectRef;
use tc_api::refusal::OpenDoor43Refusal;
use tc_api::shown::{Direction, Known};
use wasm_bindgen::JsValue;

use crate::app::home::{
    collapsed_stories, location, obs_line, resume_line, resume_place, share_button, shown_books,
    shows_book_toggle, story_tile_meta, story_tile_name, summary_line,
};
use crate::app::load::Load;
use crate::app::shell::project_initials;
use crate::app::web::app_ctx::OpenTarget;
use crate::app::web::{Actions, AppCtx, AppState, use_app, use_runtime};
use crate::attrs::Attrs;
use crate::css::Css;
use crate::ds::{
    Badge, BadgeSize, BadgeTone, BookTile, Button, ButtonVariant, Callout, CalloutTone, Card, Menu,
    MenuItem, Overline, Size, Toast, ToastTone,
};
use crate::i18n::{keys, refusal_text, t};
use crate::views::sync_result::SyncResult;

fn dir_str(dir: Direction) -> &'static str {
    match dir {
        Direction::Ltr => "ltr",
        Direction::Rtl => "rtl",
    }
}

/// `new Date(at).toLocaleDateString(undefined, { day: 'numeric', month: 'short' })`: the
/// browser's locale, as tC4 (never the app language).
fn edited_on(at: i64) -> String {
    let date = js_sys::Date::new(&JsValue::from_f64(at as f64));
    let options = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&options, &"day".into(), &"numeric".into());
    let _ = js_sys::Reflect::set(&options, &"month".into(), &"short".into());
    date.to_locale_date_string("default", &options).into()
}

/// tC4's `Home`.
#[component]
pub fn Home() -> impl IntoView {
    let (Some(AppCtx { s, actions }), Some(runtime)) = (use_app(), use_runtime()) else {
        return ().into_any();
    };
    let list = runtime.cache().read(List {});
    let main = crate::css! {
        "flex": 1,
        "overflow": "auto",
        "padding": "44px 40px 64px",
        "background": "var(--surface-app)",
    };
    let h1 = crate::css! {
        "font-size": "var(--fs-display)",
        "letter-spacing": "var(--track-38)",
        "margin": "0 0 24px",
    };
    let add_project = {
        let actions = actions.clone();
        Callback::new(move |_: web_sys::MouseEvent| actions.open_add_project())
    };
    let ready = Memo::new(move |_| list.with(|l| l.value().cloned()));
    provide_context(SharedNames(Memo::new(move |_| {
        ready.with(|view| {
            view.as_ref()
                .map(|v| crate::app::home::shared_names(&v.cards))
                .unwrap_or_default()
        })
    })));
    let resume = {
        let actions = actions.clone();
        move || {
            let view = ready.get()?;
            let edit = view.resume.clone()?;
            let card = view
                .cards
                .iter()
                .find(|c| c.project == edit.project)?
                .clone();
            Some(resume_card(&actions, edit, card))
        }
    };
    let folders = Memo::new(move |_| {
        ready.with(|view| {
            view.as_ref()
                .map(|v| {
                    v.cards
                        .iter()
                        .map(|c| (c.project.clone(), c.kind))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
    });
    let from_list = move |project: ProjectRef| {
        Signal::derive(move || {
            ready.with(|view| {
                view.as_ref()
                    .and_then(|v| v.cards.iter().find(|c| c.project == project).cloned())
            })
        })
    };
    let cards = {
        let actions = actions.clone();
        let s = s.clone();
        view! {
            <For each={move || folders.get()} key={|(project, kind)| (project.clone(), *kind)} let:entry>
                {
                    let (project, kind) = entry;
                    let listed = from_list(project.clone());
                    match kind {
                        ProjectKind::Bible => project_card(&s, &actions, project, listed).into_any(),
                        ProjectKind::Stories => obs_project_card(&s, &actions, project, listed).into_any(),
                    }
                }
            </For>
        }
    };
    let unusable = move || {
        ready
            .get()
            .map(|view| view.unusable.iter().map(unusable_card).collect_view())
    };
    let empty = move || {
        ready.get().filter(|view| view.cards.is_empty() && view.unusable.is_empty()).map(|_| {
            let style = crate::css! {
                "border": "var(--stroke-selected) dashed var(--border-dashed)",
                "border-radius": "var(--radius-xl)",
                "padding": "40px 24px",
                "text-align": "center",
                "color": "var(--text-tertiary)",
                "font-size": "var(--fs-ui-md)",
            };
            view! { <div style=style.to_style()>{move || t(keys::home::NO_PROJECTS, &[])}</div> }
        })
    };
    let loading = move || {
        matches!(list.get(), Load::Loading).then(|| {
            view! {
                <p style="font-size: var(--fs-ui); color: var(--text-tertiary);">
                    {move || t(keys::home::LOADING, &[])}
                </p>
            }
        })
    };
    view! {
        <main style=main.to_style()>
            <div style="max-width: var(--measure-page); margin: 0 auto;">
                <h1 style=h1.to_style()>{move || t(keys::home::YOUR_PROJECTS, &[])}</h1>
                {resume}
                <div style="display: flex; align-items: center; gap: 12px; margin: 0 0 14px;">
                    <Overline tag="h2" style=crate::css! { "letter-spacing": "var(--tracking-section)", "margin": 0 }>
                        {move || t(keys::home::PROJECTS, &[])}
                    </Overline>
                    <div style="flex: 1;"></div>
                    <Button on_click=add_project attrs=Attrs::from([("data-testid", "add-project")])>
                        "+ "{move || t(keys::home::ADD_PROJECT, &[])}
                    </Button>
                </div>
                {banners(&s, &actions, list)}
                {loading}
                {empty}
                <div style="display: flex; flex-direction: column; gap: 18px;">
                    {cards}
                    {unusable}
                </div>
            </div>
            {sync_toast(&s, &actions)}
            {import_toast(&s, &actions)}
        </main>
    }
    .into_any()
}

/// The banners: the failed checkpoint of the project just left (D1), a refused open (B3), and a
/// listing that failed, with its Retry (never "No projects yet" over a failed listing).
fn banners(
    s: &AppState,
    actions: &Actions,
    list: Signal<Load<Arc<HomeView>>>,
) -> impl IntoView + use<> {
    let banner_style = crate::css! { "margin": "0 0 16px", "overflow-wrap": "anywhere" };
    let commit = {
        let (failed, actions, style) = (
            s.server.failed.clone(),
            actions.clone(),
            banner_style.clone(),
        );
        move || {
            failed.with(|failed| failed.first().cloned()).map(|failed| {
                let (text, help) = crate::app::home::checkpoint_banner(&failed);
                let help = ask_for_help(&actions, help);
                view! {
                    <Callout tone=CalloutTone::Warn style=style.clone() attrs=Attrs::from([("role", "alert"), ("data-testid", "home-checkpoint-error")])>
                        {text}" "{move || t(keys::home::COMMIT_ERROR_HINT, &[])}
                        {help}
                    </Callout>
                }
            })
        }
    };
    let removed = {
        let (closed, style) = (s.start_notice_closed.clone(), banner_style.clone());
        move || {
            let shown = list.with(|l| {
                l.value()
                    .is_some_and(|v| v.notices.contains(&StartNotice::OpenDoor43Removed))
            });
            (shown && !closed.get()).then(|| {
                let closed = closed.clone();
                view! {
                    <Callout tone=CalloutTone::Warn style=style.clone() attrs=Attrs::from([("role", "status"), ("data-testid", "home-open-removed")])>
                        {move || refusal_text(&OpenDoor43Refusal::Removed)}" "
                        <Button
                            size=Size::Sm
                            variant=ButtonVariant::Outline
                            attrs=Attrs::from([("data-testid", "home-open-removed-close")])
                            on_click=Callback::new(move |_| closed.set(true))
                        >
                            {move || t(keys::common::CLOSE, &[])}
                        </Button>
                    </Callout>
                }
            })
        }
    };
    let open = {
        let (error, actions, style) = (s.book_error.clone(), actions.clone(), banner_style);
        move || {
            let failed = list.with(|l| l.error().map(ToString::to_string));
            let (text, help, retry) = match (error.get(), failed) {
                (Some(banner), failed) => (banner.text, banner.help, failed.is_some()),
                (None, Some(text)) => (text, None, true),
                (None, None) => return None,
            };
            let retry = retry.then(|| {
                let actions = actions.clone();
                view! {
                    <Button
                        size=Size::Sm
                        variant=ButtonVariant::Outline
                        attrs=Attrs::from([("data-testid", "projects-retry")])
                        on_click=Callback::new(move |_| actions.refresh_projects())
                    >
                        {move || t(keys::app::RETRY, &[])}
                    </Button>
                }
            });
            let help = ask_for_help(&actions, help);
            Some(view! {
                <Callout tone=CalloutTone::Warn style=style.clone() attrs=Attrs::from([("role", "alert"), ("data-testid", "home-open-error")])>
                    <strong>{move || t(keys::home::OPEN_ERROR, &[])}</strong>" "{text}" "
                    {retry}
                    {help}
                </Callout>
            })
        }
    };
    // The server says it in one listing only (once per launch), so the notice is held here.
    let import_held = s.import_removed.clone();
    Effect::new({
        let import_held = import_held.clone();
        move |_| {
            if list.with(|l| l.value().is_some_and(|v| v.import_removed)) {
                import_held.set(true);
            }
        }
    });
    let import_removed = move || {
        import_held.get().then(|| {
            view! {
                <Callout tone=CalloutTone::Warn style=crate::css! { "margin": "0 0 16px" } attrs=Attrs::from([("role", "status"), ("data-testid", "home-import-removed")])>
                    {move || t(keys::home::IMPORT_REMOVED, &[])}
                </Callout>
            }
        })
    };
    view! { {commit} {removed} {open} {import_removed} }
}

/// "Ask for help" on a banner (tC4 #378).
fn ask_for_help(
    actions: &Actions,
    help: Option<crate::app::feedback::HelpRequest>,
) -> Option<impl IntoView + use<>> {
    let help = help?;
    let actions = actions.clone();
    let code = help.code.clone();
    Some(view! {
        <Button
            size=Size::Sm
            variant=ButtonVariant::Outline
            attrs=Attrs::from([("data-testid", "ask-for-help"), ("data-code", code.as_str())])
            style=crate::css! { "margin-inline-start": 8 }
            on_click=Callback::new(move |_| actions.ask_for_help(help.clone()))
        >
            {move || t(keys::feedback::ASK, &[])}
        </Button>
    })
}

/// The result of the last sync (X10), above the import toast when both show.
fn sync_toast(s: &AppState, actions: &Actions) -> impl IntoView + use<> {
    let (result, import, actions) = (
        s.sync_result.clone(),
        s.import_toast.clone(),
        actions.clone(),
    );
    move || {
        result.get().map(|toast| {
            let wrap = crate::css! {
                "position": "fixed",
                "inset-block-end": if import.get().is_some() { 88 } else { 24 },
                "inset-inline-start": "50%",
                "transform": "translateX(-50%)",
                "z-index": 90,
                "width": "max-content",
                "max-width": "calc(100% - 48px)",
            };
            let (review, download, dismiss) = (actions.clone(), actions.clone(), actions.clone());
            view! {
                <div style=wrap.to_style()>
                    <SyncResult
                        toast=toast
                        on_review=Callback::new(move |()| review.sync_review())
                        on_download=Callback::new(move |()| download.sync_download())
                        on_dismiss=Callback::new(move |()| dismiss.dismiss_sync_result())
                    />
                </div>
            }
        })
    }
}

fn import_toast(s: &AppState, actions: &Actions) -> impl IntoView + use<> {
    let (toast, actions) = (s.import_toast.clone(), actions.clone());
    move || {
        toast.get().map(|toast| {
            let actions = actions.clone();
            let wrap = crate::css! {
                "position": "fixed",
                "inset-block-end": 24,
                "inset-inline-start": "50%",
                "transform": "translateX(-50%)",
                "z-index": 90,
                "width": "max-content",
                "max-width": "calc(100% - 48px)",
            };
            view! {
                <div style=wrap.to_style()>
                    <Toast
                        tone=ToastTone::Success
                        message=toast.message
                        on_dismiss=Callback::new(move |_| actions.dismiss_import_toast())
                        dismiss_label=t(keys::common::DISMISS, &[])
                        attrs=Attrs::from([("data-testid", "import-toast")])
                    />
                </div>
            }
        })
    }
}

/// The card's square of initials.
fn initials(name: Signal<String>) -> impl IntoView {
    let style = crate::css! {
        "width": 44,
        "height": 44,
        "border-radius": "var(--radius-lg)",
        "background": "var(--uw-ocean)",
        "color": "var(--text-inverse)",
        "display": "flex",
        "align-items": "center",
        "justify-content": "center",
        "font-weight": "var(--fw-black)",
        "font-size": "var(--fs-title)",
        "letter-spacing": "var(--track-17)",
        "flex": "none",
    };
    view! { <span style=style.to_style()>{move || project_initials(&name.get())}</span> }
}

fn name_style() -> String {
    crate::css! {
        "font-size": "var(--fs-h3)",
        "letter-spacing": "var(--track-20)",
        "font-weight": "var(--fw-black)",
        "color": "var(--text-heading)",
    }
    .to_style()
}

const META_STYLE: &str =
    "font-size: var(--fs-ui-sm); color: var(--text-tertiary); font-weight: var(--fw-medium);";

/// The direction badge: RTL in the warn tone, LTR neutral in the secondary text colour.
fn dir_badge(dir: Direction) -> impl IntoView {
    let (tone, style) = match dir {
        Direction::Rtl => (BadgeTone::Warn, Css::new()),
        Direction::Ltr => (
            BadgeTone::Neutral,
            crate::css! { "color": "var(--text-secondary)" },
        ),
    };
    view! { <Badge size=BadgeSize::Sm tone=tone style=style>{dir_str(dir).to_uppercase()}</Badge> }
}

/// "On Door43" beside the name of a shared project.
fn shared_badge(card: Signal<Option<HomeCard>>, project: &ProjectRef) -> impl IntoView + use<> {
    let id = format!("share-badge-{project}");
    move || {
        let shared = card.with(|c| {
            c.as_ref()
                .is_some_and(|c| matches!(c.door43, Door43Card::Shared { .. }))
        });
        shared.then(|| {
            view! {
                <Badge size=BadgeSize::Sm tone=BadgeTone::Valid attrs=Attrs::from([("data-testid", id.as_str())])>
                    {move || t(keys::home::ON_DOOR43, &[])}
                </Badge>
            }
        })
    }
}

/// The end of the card's line: "· Only on this computer" or "· Shared at owner/repository".
fn share_meta(card: Signal<Option<HomeCard>>, project: &ProjectRef) -> impl IntoView + use<> {
    let id = format!("share-state-{project}");
    view! {
        " · "
        <span data-testid=id>
            {move || {
                card.with(|c| match c.as_ref().map(|c| &c.door43) {
                    Some(Door43Card::Shared { place }) => {
                        t(keys::home::SHARED_AT, &[("location", &location(place))])
                    }
                    _ => t(keys::home::ONLY_HERE, &[]),
                })
            }}
        </span>
    }
}

/// The Share action: "Share on Door43", or "Sync with Door43" for a shared project (PLAN Q2).
fn share_action(
    s: &AppState,
    actions: &Actions,
    card: Signal<Option<HomeCard>>,
    project: &ProjectRef,
) -> impl IntoView + use<> {
    let syncing = s.syncing.clone();
    let button = {
        let syncing = syncing.clone();
        Memo::new(move |_| {
            let now = syncing.get();
            card.with(|c| c.as_ref().map(|c| share_button(c, now.as_ref())))
        })
    };
    let label = Signal::derive(move || button.get().map(|b| t(b.label, &[])).unwrap_or_default());
    let actions = actions.clone();
    let press = Callback::new(move |_: web_sys::MouseEvent| {
        if let Some(card) = card.get_untracked() {
            actions.share_project(&card);
        }
    });
    view! {
        <div
            data-testid=format!("share-card-{project}")
            data-shared=move || if button.get().is_some_and(|b| b.shared) { "1" } else { "0" }
            data-action=move || button.get().map_or("share", |b| b.action)
        >
            <Button
                variant=ButtonVariant::Outline
                size=Size::Sm
                attrs=Attrs::new().with("data-testid", format!("share-{project}"))
                title=Signal::derive(move || Some(label.get()))
                aria_busy=Signal::derive(move || button.get().is_some_and(|b| b.busy))
                disabled=Signal::derive(move || button.get().is_some_and(|b| b.disabled))
                on_click=press
            >
                {move || label.get()}
            </Button>
        </div>
    }
}

/// "Save or receive work by USB": one menu beside Share on Door43, so the card's row stays as
/// tC4's (owner ruling 2026-10-26, E2 packet question 1). "Save work for a teammate" is listed
/// only where the shell can pick a folder.
fn exchange_menu(
    actions: &Actions,
    card: Signal<Option<HomeCard>>,
    project: &ProjectRef,
) -> impl IntoView + use<> {
    let mut items = Vec::new();
    if actions.can_save_for_teammate() {
        let actions = actions.clone();
        items.push(MenuItem {
            label: t(keys::home::SAVE_FOR_TEAMMATE, &[]),
            on_click: Some(Callback::new(move |()| {
                if let Some(card) = card.get_untracked() {
                    actions.open_save_for_teammate(&card);
                }
            })),
            ..MenuItem::default()
        });
    }
    {
        let actions = actions.clone();
        items.push(MenuItem {
            label: t(keys::home::RECEIVE_WORK, &[]),
            on_click: Some(Callback::new(move |()| {
                if let Some(card) = card.get_untracked() {
                    actions.open_receive_work(&card);
                }
            })),
            ..MenuItem::default()
        });
    }
    let testid = format!("usb-menu-{project}");
    let trigger: crate::ds::Slot = Box::new(move || {
        view! {
            <Button
                variant=ButtonVariant::Secondary
                size=Size::Sm
                attrs=Attrs::new().with("data-testid", testid.clone())
                title=Signal::derive(|| Some(t(keys::home::USB_MENU, &[])))
            >
                {move || t(keys::home::USB_MENU, &[])}
            </Button>
        }
        .into_any()
    });
    view! { <Menu trigger=trigger items=items /> }
}

fn settings_button(actions: &Actions, card: Signal<Option<HomeCard>>) -> impl IntoView + use<> {
    let actions = actions.clone();
    view! {
        <Button
            variant=ButtonVariant::Secondary
            size=Size::Sm
            title=Signal::derive(|| Some(t(keys::home::SETTINGS, &[])))
            on_click=Callback::new(move |_| {
                if let Some(card) = card.get_untracked() {
                    actions.open_settings(&card);
                }
            })
        >
            {move || t(keys::home::SETTINGS, &[])}
        </Button>
    }
}

/// The darker warn brown and Kindle tint of the owner's mockup (#679); no token holds them.
const CONFLICT_WARN: &str =
    "color: #7A4A06; font-weight: var(--fw-heavy); font-family: var(--font-ui);";

/// The book of a card's first verse conflict, where its review opens (tC4
/// `openProjectConflicts`).
fn first_book(card: Signal<Option<HomeCard>>) -> Option<tc_core::BookId> {
    card.with_untracked(|c| match c.as_ref().map(|c| &c.books) {
        Some(Known::Known(books)) => books.iter().find(|b| b.conflicts > 0).map(|b| b.book),
        _ => None,
    })
}

/// What the project's last sync replaced, readable after its toast (owner ruling H5): the link
/// shows the toast again. Nothing before a sync that replaced something.
fn last_sync_link(actions: &Actions, project: &ProjectRef) -> impl IntoView + use<> {
    let read = crate::app::web::use_runtime().map(|runtime| {
        runtime.cache().read(tc_api::door43::LastSync {
            project: project.clone(),
        })
    });
    let (actions, project) = (actions.clone(), project.clone());
    move || {
        let view = read?.with(|l| l.value().and_then(|v| v.view.clone()))?;
        let n: u32 = view.replaced.iter().map(|row| row.count).sum();
        (n > 0).then(|| {
            let (actions, project) = (actions.clone(), project.clone());
            view! {
                <button type="button" data-testid=format!("last-sync-{project}") data-count=n.to_string()
                    on:click=move |_| actions.show_last_sync(project.clone(), view.clone())
                    style="border: 0; background: transparent; padding: 0; margin-inline-start: 8px; font: inherit; white-space: nowrap; color: var(--uw-ocean); font-weight: var(--fw-bold); cursor: pointer;">
                    {t(if n == 1 { keys::home::LAST_SYNC_REPLACED_ONE } else { keys::home::LAST_SYNC_REPLACED_MANY }, &[("n", &n.to_string())])}
                </button>
            }
        })
    }
}

/// The card's conflict count (#679): nothing while unknown or none.
fn conflicts_chip(
    actions: &Actions,
    card: Signal<Option<HomeCard>>,
    project: &ProjectRef,
) -> impl IntoView + use<> {
    let total = Memo::new(move |_| {
        card.with(|c| match c.as_ref().map(|c| &c.conflicts) {
            Some(Known::Known(n)) if *n > 0 => Some(*n),
            _ => None,
        })
    });
    let (actions, project) = (actions.clone(), project.clone());
    move || {
        total.get().map(|n| {
            let (actions, project_open) = (actions.clone(), project.clone());
            let style = format!(
                "{CONFLICT_WARN} box-sizing: border-box; display: inline-flex; align-items: center; gap: 6px; height: 24px; padding: 0 10px 0 8px; border-radius: var(--radius-pill); border: var(--stroke-hair) solid var(--uw-kindle); background: #FDF1DE; font-size: var(--fs-meta); line-height: 1; cursor: pointer;"
            );
            let count = n.to_string();
            view! {
                <button
                    type="button"
                    data-testid=format!("conflicts-{project}")
                    data-count=count.clone()
                    aria-label=move || t(keys::home::CONFLICTS_LABEL, &[("n", &n.to_string())])
                    on:click=move |_| actions.open_project_conflicts(project_open.clone(), first_book(card), tc_api::conflicts::ReviewTab::Text)
                    style=style
                >
                    {branch_icon()}
                    {move || t(if n == 1 { keys::home::CONFLICTS_ONE } else { keys::home::CONFLICTS_MANY }, &[("n", &n.to_string())])}
                </button>
            }
        })
    }
}

/// The chip's branch icon. Built with the SVG builders: `view!` would wrap each shape in a
/// group.
fn branch_icon() -> AnyView {
    use crate::ds::fixed;
    use leptos::svg;
    svg::svg()
        .add_any_attr(fixed(&[
            ("aria-hidden", "true"),
            ("width", "13"),
            ("height", "13"),
            ("viewBox", "0 0 24 24"),
            ("fill", "none"),
            ("stroke", "currentColor"),
            ("stroke-width", "2.2"),
            ("stroke-linecap", "round"),
            ("stroke-linejoin", "round"),
            ("style", "flex: none; display: block;"),
        ]))
        .child((
            svg::path().add_any_attr(fixed(&[("d", "M6 3v12")])),
            svg::circle().add_any_attr(fixed(&[("cx", "6"), ("cy", "18"), ("r", "3")])),
            svg::circle().add_any_attr(fixed(&[("cx", "18"), ("cy", "6"), ("r", "3")])),
            svg::path().add_any_attr(fixed(&[("d", "M18 9a9 9 0 0 1-9 9")])),
        ))
        .into_any()
}

/// A book tile's conflict line.
fn book_conflicts(code: &'static str, n: Signal<u32>) -> impl IntoView {
    move || {
        let count = n.get();
        (count > 0).then(|| {
            view! {
                <span
                    data-testid=format!("book-conflicts-{code}")
                    data-count=count.to_string()
                    style=format!("{CONFLICT_WARN} display: inline-flex; align-items: center; gap: 5px; margin-top: 8px; font-size: 10.5px;")
                >
                    <span aria-hidden="true" style="width: 7px; height: 7px; border-radius: var(--radius-pill); background: var(--uw-kindle); flex: none;"></span>
                    {move || t(if count == 1 { keys::home::BOOK_CONFLICTS_ONE } else { keys::home::BOOK_CONFLICTS_MANY }, &[("n", &count.to_string())])}
                </span>
            }
        })
    }
}

/// The card's own read, or the list's copy until it is in.
fn card_signal(project: &ProjectRef, listed: Signal<Option<HomeCard>>) -> Signal<Option<HomeCard>> {
    let own = use_runtime().map(|runtime| {
        runtime.cache().read(CardRead {
            project: project.clone(),
        })
    });
    let memo = Memo::new(move |_| {
        own.and_then(|own| own.with(|l| l.value().map(|c| HomeCard::clone(c))))
            .or_else(|| listed.get())
    });
    memo.into()
}

/// The names more than one Home card holds.
#[derive(Clone, Copy)]
struct SharedNames(Memo<Vec<String>>);

/// The card's header: initials, name and badges, where it lives when another card has its name,
/// the line, Share and Settings.
fn header(
    s: &AppState,
    actions: &Actions,
    card: Signal<Option<HomeCard>>,
    project: &ProjectRef,
    stories: bool,
) -> impl IntoView + use<> {
    let name = Signal::derive(move || {
        card.with(|c| c.as_ref().map(|c| c.name.clone()).unwrap_or_default())
    });
    let dir = Memo::new(move |_| card.with(|c| c.as_ref().map_or(Direction::Ltr, |c| c.dir)));
    let imported = {
        let (imported, project) = (s.imported_repo.clone(), project.clone());
        move || {
            (!stories && imported.get().as_ref() == Some(&project)).then(|| {
                view! {
                    <Badge size=BadgeSize::Sm tone=BadgeTone::AccentSoft attrs=Attrs::from([("data-testid", "imported-badge")])>
                        {move || t(keys::importer::BADGE, &[])}
                    </Badge>
                }
            })
        }
    };
    let line = move || {
        card.with(|c| {
            c.as_ref()
                .map(|c| match &c.stories {
                    Some(stories) => obs_line(c, stories),
                    None => summary_line(c),
                })
                .unwrap_or_default()
        })
    };
    let obs_marker = stories.then(|| {
        view! {
            <Badge size=BadgeSize::Sm tone=BadgeTone::Accent attrs=Attrs::from([("data-testid", "obs-marker")])>
                {move || t(keys::home::OBS_MARKER, &[])}
            </Badge>
        }
    });
    let chip = (!stories).then(|| conflicts_chip(actions, card, project));
    let shared = use_context::<SharedNames>();
    let where_id = format!("card-where-{project}");
    let where_line = move || {
        let SharedNames(names) = shared?;
        let line = names.with(|names| {
            card.with(|c| {
                c.as_ref()
                    .and_then(|c| crate::app::home::where_line(c, names))
            })
        })?;
        Some(view! { <span data-testid=where_id.clone() style=META_STYLE>{line}</span> })
    };
    view! {
        <div style="display: flex; align-items: center; gap: 14px; margin-bottom: 18px;">
            {initials(name)}
            <div style="flex: 1; display: flex; flex-direction: column; gap: 4px;">
                <div style="display: flex; align-items: center; gap: 10px;">
                    <span dir=move || dir_str(dir.get()) style=name_style()>{move || name.get()}</span>
                    {obs_marker}
                    {move || dir_badge(dir.get())}
                    {imported}
                    {shared_badge(card, project)}
                    {chip}
                </div>
                {where_line}
                <span style=META_STYLE>
                    <span data-testid=if stories { "obs-progress" } else { "project-summary" }>{line}</span>
                    {share_meta(card, project)}
                    {last_sync_link(actions, project)}
                </span>
            </div>
            {share_action(s, actions, card, project)}
            {exchange_menu(actions, card, project)}
            {settings_button(actions, card)}
        </div>
    }
}

/// tC4's `ProjectCard`: a Bible project.
fn project_card(
    s: &AppState,
    actions: &Actions,
    project: ProjectRef,
    listed: Signal<Option<HomeCard>>,
) -> impl IntoView + use<> {
    let card = card_signal(&project, listed);
    let expanded = RwSignal::new(false);
    let books = Memo::new(move |_| {
        card.with(|c| match c.as_ref().map(|c| &c.books) {
            Some(Known::Known(books)) => books.clone(),
            _ => Vec::new(),
        })
    });
    let shown = Memo::new(move |_| books.with(|b| shown_books(b, expanded.get())));
    let tiles = {
        let (actions, project) = (actions.clone(), project.clone());
        view! {
            <For each={move || shown.get().into_iter().map(|b| b.book).collect::<Vec<_>>()} key={|book| *book} let:book>
                {
                    let tile = Memo::new(move |_| books.with(|b| b.iter().find(|t| t.book == book).cloned()));
                    let percent = Signal::derive(move || tile.with(|t| match t.as_ref().map(|t| &t.drafted) {
                        Some(Known::Known(n)) => f64::from(*n),
                        _ => 0.0,
                    }));
                    let meta = Signal::derive(move || tile.with(|t| match t.as_ref().map(|t| &t.drafted) {
                        Some(Known::Known(_)) => None,
                        _ => Some(crate::app::home::DASH.to_owned()),
                    }));
                    let conflicts = Signal::derive(move || tile.with(|t| t.as_ref().map_or(0, |t| t.conflicts)));
                    let (actions, project) = (actions.clone(), project.clone());
                    view! {
                        <BookTile
                            name=book.english_name().to_owned()
                            percent=percent
                            meta=meta
                            attrs=Attrs::new().with("data-testid", format!("book-tile-{}", book.code()))
                            on_click=Callback::new(move |_| actions.open_project_at(project.clone(), OpenTarget::Book(book)))
                        >
                            {book_conflicts(book.code(), conflicts)}
                        </BookTile>
                    }
                }
            </For>
        }
    };
    let add_book = {
        let actions = actions.clone();
        let style = crate::css! {
            "border": "var(--stroke-selected) dashed var(--border-strong)",
            "background": "transparent",
            "cursor": "pointer",
            "border-radius": "var(--radius-md)",
            "padding": "10px 12px",
            "min-height": 62,
            "font-family": "var(--font-ui)",
            "display": "flex",
            "flex-direction": "column",
            "align-items": "center",
            "justify-content": "center",
            "gap": 2,
            "color": "var(--text-tertiary)",
        };
        view! {
            <button
                type="button"
                data-i="choice"
                data-tone="accent"
                style=style.to_style()
                on:click=move |_| {
                    if let Some(card) = card.get_untracked() {
                        actions.open_add_book(&card);
                    }
                }
            >
                <span style="font-size: var(--fs-title-sm); letter-spacing: var(--track-16); font-weight: var(--fw-heavy); color: var(--text-accent); line-height: 1;">"+"</span>
                <span style="font-size: var(--fs-meta); letter-spacing: var(--track-11-5); font-weight: var(--fw-heavy);">{move || t(keys::home::ADD_BOOK_TILE, &[])}</span>
            </button>
        }
    };
    let toggle = {
        let id = format!("toggle-books-{project}");
        move || {
            books.with(|b| shows_book_toggle(b)).then(|| {
                let n = books.with(Vec::len).to_string();
                view! {
                    <Button
                        variant=ButtonVariant::Ghost
                        attrs=Attrs::new().with("data-testid", id.clone())
                        style=crate::css! { "margin-top": 12 }
                        on_click=Callback::new(move |_| expanded.update(|e| *e = !*e))
                    >
                        {move || if expanded.get() { t(keys::home::SHOW_IN_PROGRESS, &[]) } else { t(keys::home::SHOW_ALL, &[("n", &n)]) }}
                        " →"
                    </Button>
                }
            })
        }
    };
    let head = header(s, actions, card, &project, false);
    view! {
        <Card padding="22px 24px" attrs=Attrs::new().with("data-testid", format!("project-{project}"))>
            {head}
            <div style="display: grid; grid-template-columns: repeat(auto-fill,minmax(148px,1fr)); gap: 8px;">
                {tiles}
                {add_book}
            </div>
            {toggle}
        </Card>
    }
}

/// tC4's `ObsProjectCard`: a story project, a tile per story.
fn obs_project_card(
    s: &AppState,
    actions: &Actions,
    project: ProjectRef,
    listed: Signal<Option<HomeCard>>,
) -> impl IntoView + use<> {
    let card = card_signal(&project, listed);
    let expanded = RwSignal::new(false);
    let stories = Memo::new(move |_| card.with(|c| c.as_ref().and_then(|c| c.stories.clone())));
    let dir = Memo::new(move |_| card.with(|c| c.as_ref().map_or(Direction::Ltr, |c| c.dir)));
    let shown = Memo::new(move |_| {
        stories.with(|s| {
            s.as_ref()
                .map(|s| {
                    if expanded.get() {
                        s.stories.iter().map(|t| t.story).collect::<Vec<_>>()
                    } else {
                        collapsed_stories(&s.stories, &s.recent)
                            .into_iter()
                            .map(|t| t.story)
                            .collect()
                    }
                })
                .unwrap_or_default()
        })
    });
    let tiles = {
        let (actions, project) = (actions.clone(), project.clone());
        view! {
            <For each={move || shown.get()} key={|story| *story} let:story>
                {
                    let tile = Memo::new(move |_| stories.with(|s| s.as_ref().and_then(|s| s.stories.iter().find(|t| t.story == story).cloned())));
                    let edited = Memo::new(move |_| stories.with(|s| s.as_ref().and_then(|s| s.recent.iter().find(|r| r.story == story).map(|r| r.at.0))));
                    let name = Signal::derive(move || tile.with(|t| t.as_ref().map(story_tile_name).unwrap_or_default()));
                    let meta = Signal::derive(move || tile.with(|t: &Option<StoryTileView>| t.as_ref().and_then(|t| story_tile_meta(t, edited.get().map(edited_on)))));
                    let percent = Signal::derive(move || tile.with(|t| match t.as_ref().map(|t| &t.drafted) {
                        Some(Known::Known(n)) => f64::from(*n),
                        _ => 0.0,
                    }));
                    let (actions, project) = (actions.clone(), project.clone());
                    view! {
                        <BookTile
                            name=name
                            percent=percent
                            meta=meta
                            attrs=Attrs::new().with("data-testid", format!("story-tile-{}", story.get())).with("dir", dir_str(dir.get_untracked()))
                            on_click=Callback::new(move |_| actions.open_project_at(project.clone(), OpenTarget::Story(story)))
                        />
                    }
                }
            </For>
        }
    };
    let toggle = {
        let id = format!("toggle-stories-{project}");
        move || {
            let count = stories.with(|s| s.as_ref().map_or(0, |s| s.stories.len()));
            (count > crate::app::home::RECENT_TILES).then(|| {
                let n = count.to_string();
                view! {
                    <Button
                        variant=ButtonVariant::Ghost
                        attrs=Attrs::new().with("data-testid", id.clone())
                        style=crate::css! { "margin-top": 12 }
                        on_click=Callback::new(move |_| expanded.update(|e| *e = !*e))
                    >
                        {move || if expanded.get() { t(keys::home::SHOW_RECENT_STORIES, &[]) } else { t(keys::home::SHOW_ALL_STORIES, &[("n", &n)]) }}
                        " →"
                    </Button>
                }
            })
        }
    };
    let head = header(s, actions, card, &project, true);
    view! {
        <Card padding="22px 24px" attrs=Attrs::new().with("data-testid", format!("project-{project}"))>
            {head}
            <div style="display: grid; grid-template-columns: repeat(auto-fill,minmax(236px,1fr)); gap: 8px;" data-testid="story-tiles">
                {tiles}
            </div>
            {toggle}
        </Card>
    }
}

/// The Ocean banner: the most recent change on this computer, one press back to it.
fn resume_card(actions: &Actions, edit: LastEditView, card: HomeCard) -> impl IntoView + use<> {
    let dir = dir_str(card.dir);
    let press = {
        let (actions, edit) = (actions.clone(), edit.clone());
        move || actions.resume(&edit)
    };
    let key_press = press.clone();
    let style = crate::css! {
        "display": "flex",
        "align-items": "center",
        "gap": 20,
        "padding": "22px 24px",
        "background": "var(--uw-ocean)",
        "border-radius": "var(--radius-xl)",
        "cursor": "pointer",
        "box-shadow": "var(--shadow-hero)",
        "margin-bottom": 34,
    };
    let title_style = crate::css! {
        "font-size": "var(--fs-h2)",
        "letter-spacing": "var(--track-22)",
        "font-weight": "var(--fw-black)",
        "color": "var(--text-inverse)",
        "margin": "0 0 2px",
    };
    let line_style = crate::css! {
        "font-size": "var(--fs-ui-sm)",
        "color": "rgba(255,255,255,.72)",
        "margin": 0,
        "overflow": "hidden",
        "text-overflow": "ellipsis",
        "white-space": "nowrap",
    };
    let pill = crate::css! {
        "display": "inline-flex",
        "align-items": "center",
        "gap": 8,
        "padding": "11px 20px",
        "background": "var(--accent)",
        "color": "var(--text-inverse)",
        "border-radius": "var(--radius-pill)",
        "font-weight": "var(--fw-heavy)",
        "font-size": "var(--fs-ui-md)",
        "white-space": "nowrap",
    };
    let line_dir = (!edit.snippet.is_empty()).then_some(dir);
    let (place, line) = (edit.clone(), edit);
    view! {
        <div
            role="button"
            tabindex="0"
            data-testid="resume-card"
            style=style.to_style()
            on:click=move |_| press()
            on:keydown=move |e: web_sys::KeyboardEvent| {
                if e.key() == "Enter" || e.key() == " " {
                    e.prevent_default();
                    key_press();
                }
            }
        >
            <div style="flex: 1; min-width: 0;">
                <p style=title_style.to_style()>
                    <span dir=dir>{card.name.clone()}</span>" · "{move || resume_place(&place)}
                </p>
                <p dir=line_dir style=line_style.to_style()>{move || resume_line(&line)}</p>
            </div>
            <span data-i="fill" data-tone="accent" style=pill.to_style()>
                {move || t(keys::home::RESUME, &[])}" →"
            </span>
        </div>
    }
}

/// A folder that holds someone's work but cannot be opened: its name and why, and no button
/// (B2 D9; a declared difference from tC4, whose list leaves it off).
fn unusable_card(folder: &UnusableCard) -> impl IntoView + use<> {
    let reason = folder.reason.clone();
    let text = move || crate::i18n::refusal_text(&reason);
    let name = folder.name.clone();
    view! {
        <Card padding="22px 24px" attrs=Attrs::new().with("data-testid", format!("unusable-{name}"))>
            <div style="display: flex; align-items: center; gap: 10px; margin-bottom: 6px;">
                <span style=name_style()>{name.clone()}</span>
                <Badge size=BadgeSize::Sm tone=BadgeTone::Warn>{move || t(keys::home::unusable::BADGE, &[])}</Badge>
            </div>
            <p style="margin: 0; font-size: var(--fs-ui-sm); color: var(--text-secondary); line-height: var(--lh-body);">{text}</p>
        </Card>
    }
}
