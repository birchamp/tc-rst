//! Get source texts (M8 steps 1 and 2, unit B9a; `tC4:src/views/modals/SourceTexts.jsx`), over
//! the open project. Step 1: the project's pinned resources with Check for updates (C8 reads
//! the offers), and the gateway-language packages with how many this session downloaded. Step
//! 2: the package for one book from Door43's catalog, its rows to keep or leave out, and
//! Download. While a download runs, Cancel stops it: tC4 has none, and a slow connection can
//! hold a package for minutes (a declared difference, AGENTS.md "Matching tC4").

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::resources::{GatewayPackage, PackageRole, PackageRow, SourceGateway};
use tc_api::shown::Direction;
use tc_core::BookId;

use crate::app::sources::{Dl, SourcesForm};
use crate::app::web::{Actions, AppCtx, use_app};
use crate::ds::{
    Attrs, Badge, BadgeTone, Button, ButtonVariant, Callout, CalloutTone, OptionCard, Overline,
    Select, SelectOption, Size,
};
use crate::i18n::{Key, keys, t};

/// The chapters of each book in canonical order (tC4 `BOOK_CHAPTERS`).
const CHAPTERS: [u16; 66] = [
    50, 40, 27, 36, 34, 24, 21, 4, 31, 24, 22, 25, 29, 36, 10, 13, 10, 42, 150, 31, 12, 8, 66, 52,
    5, 48, 12, 14, 3, 9, 1, 4, 7, 3, 3, 3, 2, 14, 4, 28, 16, 24, 21, 28, 16, 16, 13, 6, 6, 4, 4, 5,
    3, 6, 4, 3, 1, 13, 5, 5, 3, 5, 1, 1, 1, 22,
];

fn dir_attr(dir: Direction) -> &'static str {
    match dir {
        Direction::Ltr => "ltr",
        Direction::Rtl => "rtl",
    }
}

fn role_name(role: PackageRole) -> Key {
    match role {
        PackageRole::Text => keys::sources::ROLE_TEXT,
        PackageRole::Notes | PackageRole::ObsNotes => keys::sources::ROLE_NOTES,
        PackageRole::Questions => keys::sources::ROLE_QUESTIONS,
        PackageRole::WordLinks | PackageRole::ObsWords => keys::sources::ROLE_WORD_LINKS,
        PackageRole::Words => keys::sources::ROLE_WORDS,
        PackageRole::Academy => keys::sources::ROLE_ACADEMY,
        PackageRole::ObsText => keys::sources::ROLE_OBS_TEXT,
        PackageRole::ObsImages => keys::sources::ROLE_OBS_IMAGES,
    }
}

/// The dialog, while `s.modal` is Sources.
#[component]
pub fn SourceTexts() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let src = s.src.clone();
    let gateway = {
        let src = src.clone();
        Memo::new(move |_| src.with(|src| src.gateway.clone()))
    };
    let footer: Option<ChildrenFn> = {
        let (actions, src) = (actions.clone(), src.clone());
        Some(Arc::new(move || {
            gateway
                .get()
                .map_or_else(|| ().into_any(), |g| footer(&actions, &src, &g).into_any())
        }))
    };
    let children: ChildrenFn = {
        let (actions, src) = (actions.clone(), src.clone());
        Arc::new(move || {
            let (actions, src) = (actions.clone(), src.clone());
            let offline = {
                let (actions, src) = (actions.clone(), src.clone());
                move || {
                    src.with(|src| src.needs_internet && src.gateway.is_some()).then(|| {
                        let actions = actions.clone();
                        view! {
                            <Callout tone=CalloutTone::Kindle style=crate::css! { "display": "flex", "align-items": "center", "gap": 10 }>
                                <span style="flex: 1 1 0%;">{t(keys::sources::OFFLINE, &[])}</span>
                                <Button
                                    size=Size::Sm
                                    attrs=Attrs::from([("data-testid", "sources-allow-internet")])
                                    style=crate::css! { "background": "var(--uw-kindle)", "flex": "none" }
                                    on_click=Callback::new(move |_| actions.allow_sources())
                                >
                                    {t(keys::sources::GO_ONLINE, &[])}
                                </Button>
                            </Callout>
                        }
                    })
                }
            };
            let packages = {
                let src = src.clone();
                Memo::new(move |_| src.with(|src| src.packages.clone()))
            };
            let body = move || match gateway.get() {
                None => {
                    let packages = packages.get();
                    let pinned = packages.as_ref().is_some_and(|p| p.pinned);
                    let gateways = packages.map(|p| p.gateways).unwrap_or_default();
                    view! {
                        {resource_updates(&actions, pinned)}
                        {language_step(&actions, gateways)}
                    }
                    .into_any()
                }
                Some(g) => gateway_step(&actions, &src, g).into_any(),
            };
            view! { {offline} {body} }.into_any()
        })
    };
    let close = actions.clone();
    crate::ds::Modal(crate::ds::ModalProps {
        width: Some(640.into()),
        attrs: Attrs::from([("data-testid", "sources-modal")]),
        title: Some((move || move || t(keys::sources::TITLE, &[])).into()),
        subtitle: Some((move || move || t(keys::sources::SUBTITLE, &[])).into()),
        close_label: Some(Signal::derive(|| t(keys::common::CLOSE, &[]))),
        on_close: Some(Callback::new(move |()| close.close_modal())),
        footer,
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}

/// J12's "Pinned resources" with Check for updates (tC4 `ResourceUpdates`), for a project with
/// language sets. The offers are C8's.
fn resource_updates(actions: &Actions, pinned: bool) -> impl IntoView + use<> {
    let actions = actions.clone();
    pinned.then(|| {
        view! {
            <div data-testid="resource-updates" style="display: flex; flex-direction: column; gap: 8px; padding-bottom: 6px;">
                <div style="display: flex; align-items: center; gap: 10px;">
                    <Overline tag="span">{t(keys::upgrade::TITLE, &[])}</Overline>
                    <div style="flex: 1 1 0%;"></div>
                    <Button
                        size=Size::Sm
                        variant=ButtonVariant::Secondary
                        attrs=Attrs::from([("data-testid", "check-updates")])
                        on_click=Callback::new(move |_| actions.check_for_updates())
                    >
                        {t(keys::upgrade::CHECK, &[])}
                    </Button>
                </div>
            </div>
        }
    })
}

/// Step 1 (tC4 `LanguageStep`).
fn language_step(actions: &Actions, gateways: Vec<SourceGateway>) -> impl IntoView + use<> {
    let cards = gateways
        .into_iter()
        .map(|g| package_card(actions, g))
        .collect::<Vec<_>>();
    view! {
        <div style="padding-bottom: 18px;">
            <Overline tag="span" style=crate::css! { "display": "block", "margin-bottom": 8 }>
                {t(keys::sources::CHOOSE_GATEWAY, &[])}
            </Overline>
            <div style="display: flex; flex-direction: column; gap: 8px;">{cards}</div>
        </div>
    }
}

fn package_card(actions: &Actions, g: SourceGateway) -> AnyView {
    let SourceGateway { package, installed } = g;
    let (dir, autonym) = (dir_attr(package.direction), package.autonym.clone());
    let seen = t(
        keys::sources::SUITE_SEEN,
        &[
            ("tn", &package.seen.tn),
            ("tw", &package.seen.tw),
            ("ta", &package.seen.ta),
        ],
    );
    let owner = package.gateway.owner.clone();
    let trailing = (installed > 0).then(|| {
        crate::ds::Content::from(move || {
            view! {
                <span style="font-size: var(--fs-label); letter-spacing: var(--track-11); font-weight: var(--fw-heavy); color: var(--tc-valid-strong); white-space: nowrap;">
                    {t(keys::sources::N_INSTALLED, &[("n", &installed)])}
                </span>
            }
        })
    });
    let pick = {
        let (actions, package) = (actions.clone(), package.clone());
        Callback::new(move |_: web_sys::Event| actions.pick_gateway(package.clone()))
    };
    let title = move || view! { <span dir=dir>{autonym.clone()}</span> };
    let description = move || view! { <span style="font-family: var(--font-mono);">{owner.clone()}" · "{seen.clone()}</span> };
    match trailing {
        Some(trailing) => view! {
            <OptionCard on_click=pick title=title meta=package.name.clone() description=description trailing=trailing />
        }
        .into_any(),
        None => view! {
            <OptionCard on_click=pick title=title meta=package.name.clone() description=description />
        }
        .into_any(),
    }
}

fn book_options() -> Vec<SelectOption> {
    BookId::ALL
        .iter()
        .zip(CHAPTERS)
        .enumerate()
        .map(|(i, (book, chapters))| SelectOption {
            value: book.code().to_owned(),
            label: book.english_name().to_owned(),
            code: Some(book.code().to_owned()),
            meta: Some(t(keys::add_book::CHAPTERS_SHORT, &[("n", &chapters)])),
            group: Some(t(
                if i >= 39 {
                    keys::add_book::NT
                } else {
                    keys::add_book::OT
                },
                &[],
            )),
            badge: None,
            disabled: false,
        })
        .collect()
}

/// Step 2 (tC4 `GatewayStep`).
fn gateway_step(
    actions: &Actions,
    src: &ArcRwSignal<SourcesForm>,
    g: GatewayPackage,
) -> impl IntoView + use<> {
    let (is_current, is_checkable) =
        src.with_untracked(|src| (src.is_current(&g), src.is_checkable(&g)));
    let change = actions.clone();
    let check_in = (is_checkable && !is_current).then(|| {
        let (actions, gateway, autonym) = (actions.clone(), g.gateway.clone(), g.autonym.clone());
        view! {
            <Callout
                tone=CalloutTone::Info
                style=crate::css! {
                    "display": "flex", "align-items": "center", "gap": 10,
                    "background": "var(--surface-accent-soft)", "border-color": "rgba(49,173,227,.35)",
                    "color": "var(--uw-ocean)",
                }
            >
                <span style="flex: 1 1 0%;">{t(keys::sources::CHECK_IN_PROMPT, &[("lang", &autonym)])}</span>
                <Button
                    size=Size::Sm
                    attrs=Attrs::from([("data-testid", "use-for-checking")])
                    style=crate::css! { "flex": "none" }
                    on_click=Callback::new(move |_| actions.ask_gateway_change(gateway.clone()))
                >
                    {t(keys::sources::CHECK_IN, &[])}
                </Button>
            </Callout>
        }
    });
    let already = is_current.then(|| {
        view! {
            <p data-testid="already-checking-in" style="font-size: var(--fs-caption); letter-spacing: var(--track-12); color: var(--tc-valid-strong); font-weight: var(--fw-bold); margin: 0px;">
                {t(keys::sources::ALREADY_CHECKING_IN, &[("lang", &g.autonym)])}
            </p>
        }
    });
    let book = {
        let src = src.clone();
        Signal::derive(move || Some(src.with(|src| src.book.code().to_owned())))
    };
    let on_book = {
        let actions = actions.clone();
        Callback::new(move |code: String| {
            if let Ok(book) = code.parse::<BookId>() {
                actions.set_source_book(book);
            }
        })
    };
    let contents = {
        let (actions, src, g) = (actions.clone(), src.clone(), g.clone());
        move || package_contents(&actions, &src, &g)
    };
    view! {
        <Callout tone=CalloutTone::Info style=crate::css! { "display": "flex", "align-items": "center", "gap": 8 }>
            <span style="font-weight: var(--fw-heavy); color: var(--uw-ocean);">{g.autonym.clone()}" · "{g.gateway.owner.clone()}</span>
            <div style="flex: 1 1 0%;"></div>
            <Button
                variant=ButtonVariant::Ghost
                style=crate::css! { "font-size": "var(--fs-caption)", "letter-spacing": "var(--track-12)" }
                on_click=Callback::new(move |_| change.change_gateway())
            >
                {t(keys::sources::CHANGE_LANGUAGE, &[])}
            </Button>
        </Callout>
        {check_in}
        {already}
        <Select
            id="src-book"
            label=Signal::derive(|| t(keys::sources::BOOK, &[]))
            value=book
            on_change=on_book
            search_placeholder=Signal::derive(|| t(keys::add_book::FIND_BOOK, &[]))
            no_matches_label=Signal::derive(|| t(keys::add_book::NO_MATCHES, &[]))
            clear_label=Signal::derive(|| t(keys::common::CLEAR, &[]))
            options=book_options()
        />
        <div style="padding-bottom: 14px;">
            <Overline tag="span" style=crate::css! { "display": "block", "margin-bottom": 8 }>
                {t(keys::sources::PACKAGE_CONTENTS, &[])}
            </Overline>
            {contents}
        </div>
    }
}

fn package_contents(
    actions: &Actions,
    src: &ArcRwSignal<SourcesForm>,
    g: &GatewayPackage,
) -> AnyView {
    let caption = "font-size: var(--fs-caption-lg); margin: 0px; line-height: var(--lh-body);";
    let (loading, error, rows, book) =
        src.with(|src| (src.loading, src.error.clone(), src.rows.clone(), src.book));
    if loading {
        return view! {
            <p style="font-size: var(--fs-caption-lg); color: var(--text-tertiary); margin: 0px;">
                {t(keys::sources::LOADING_CATALOG, &[])}
            </p>
        }
        .into_any();
    }
    if let Some(error) = error {
        return view! {
            <p data-testid="sources-error" style=format!("{caption} color: var(--tc-invalid);")>{error}</p>
        }
        .into_any();
    }
    let none = rows.is_empty().then(|| {
        view! {
            <p style=format!("{caption} color: var(--text-tertiary);")>
                {t(keys::sources::NONE_FOR_BOOK, &[("book", &book.english_name()), ("lang", &g.autonym)])}
            </p>
        }
    });
    let cards = rows
        .into_iter()
        .map(|row| package_row(actions, src, row))
        .collect::<Vec<_>>();
    view! {
        <div style="display: flex; flex-direction: column; gap: 7px;">
            {none}
            {cards}
        </div>
    }
    .into_any()
}

/// One row (tC4 `PackageRow`): a fixed row is always included; another is a choice. The rows
/// are drawn again at each change of the form.
fn package_row(actions: &Actions, src: &ArcRwSignal<SourcesForm>, row: PackageRow) -> AnyView {
    let fixed = row.role.fixed();
    let on = src.with_untracked(|src| src.on(&row));
    let style = if fixed {
        crate::css! { "cursor": "default" }
    } else if on {
        crate::ds::Css::new()
    } else {
        crate::css! { "opacity": 0.72 }
    };
    let on_click = (!fixed).then(|| {
        let (actions, row) = (actions.clone(), row.clone());
        Callback::new(move |_: web_sys::Event| actions.toggle_source_row(&row))
    });
    let trailing = fixed.then(|| {
        crate::ds::Content::from(|| {
            let label = t(keys::sources::ALWAYS_INCLUDED, &[]);
            view! {
                <Badge tone=BadgeTone::Neutral attrs=Attrs::new().with("title", label.clone())>{label}</Badge>
            }
        })
    });
    let (title, meta, description) = (
        t(role_name(row.role), &[]),
        row.repo.clone(),
        row.desc.clone(),
    );
    match (on_click, trailing) {
        (Some(on_click), _) => view! {
            <OptionCard selected=on title=title meta=meta description=description on_click=on_click style=style />
        }
        .into_any(),
        (None, Some(trailing)) => view! {
            <OptionCard selected=on title=title meta=meta description=description trailing=trailing style=style />
        }
        .into_any(),
        (None, None) => view! {
            <OptionCard selected=on title=title meta=meta description=description style=style />
        }
        .into_any(),
    }
}

/// tC4's `SourcesFooter`, with Cancel while a download runs.
fn footer(
    actions: &Actions,
    src: &ArcRwSignal<SourcesForm>,
    g: &GatewayPackage,
) -> impl IntoView + use<> {
    let state = {
        let src = src.clone();
        Memo::new(move |_| src.with(|src| (src.dl.clone(), src.can_download(), src.book)))
    };
    let lang = g.autonym.clone();
    let status = move || {
        match state.get() {
        (Some(Dl::Done), _, book) => Some(view! {
            <span data-testid="sources-done" style="font-size: var(--fs-caption-lg); letter-spacing: var(--track-12-5); font-weight: var(--fw-heavy); color: var(--tc-valid-strong);">
                {t(keys::sources::READY, &[("book", &book.english_name()), ("lang", &lang)])}
            </span>
        }
        .into_any()),
        (Some(Dl::Run { repo, .. }), _, _) => Some(view! {
            <span data-testid="sources-progress" style="font-size: var(--fs-caption-lg); letter-spacing: var(--track-12-5); font-weight: var(--fw-bold); color: var(--text-secondary);">
                {match repo {
                    Some(repo) => t(keys::sources::PROGRESS, &[("repo", &repo)]),
                    None => t(keys::sources::DOWNLOADING, &[]),
                }}
            </span>
        }
        .into_any()),
        (None, _, _) => None,
    }
    };
    let (close, download, cancel) = (actions.clone(), actions.clone(), actions.clone());
    let run = move || matches!(state.get().0, Some(Dl::Run { .. }));
    let can = move || state.get().1;
    view! {
        {status}
        <div style="flex: 1 1 0%;"></div>
        {move || run().then(|| {
            let cancel = cancel.clone();
            view! {
                <Button
                    variant=ButtonVariant::Secondary
                    attrs=Attrs::from([("data-testid", "sources-cancel")])
                    on_click=Callback::new(move |_| cancel.cancel_download())
                >
                    {t(keys::wizard::CANCEL, &[])}
                </Button>
            }
        })}
        <Button variant=ButtonVariant::Secondary on_click=Callback::new(move |_| close.close_modal())>
            {t(keys::common::CLOSE, &[])}
        </Button>
        {move || can().then(|| {
            let download = download.clone();
            view! {
                <Button attrs=Attrs::from([("data-testid", "sources-download")]) on_click=Callback::new(move |_| download.download_package())>
                    {t(keys::sources::DOWNLOAD, &[])}
                </Button>
            }
        })}
    }
}
