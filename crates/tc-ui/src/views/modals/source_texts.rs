//! Get source texts (M8 steps 1 and 2, unit B9a; `tC4:src/views/modals/SourceTexts.jsx`), over
//! the open project. Step 1: the project's pinned resources with Check for updates (C8 reads
//! the offers), and the gateway-language packages with how many this session downloaded. Step
//! 2: the package for one book from Door43's catalog, its rows to keep or leave out, and
//! Download. While a download runs, Cancel stops it: tC4 has none, and a slow connection can
//! hold a package for minutes (a declared difference, AGENTS.md "Matching tC4").

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::resources::{
    GatewayPackage, PackageRole, PackageRow, RepoMove, SetOffer, SetRung, SourceGateway, TextKind,
    TextOffer,
};
use tc_api::shown::Direction;
use tc_core::BookId;

use crate::app::forms::chapters;
use crate::app::shell::CurrentProject;
use crate::app::sources::{Dl, SourcesForm};
use crate::app::upgrades::{Installing, UpgradeState};
use crate::app::web::{Actions, AppCtx, use_app};
use crate::ds::{
    Attrs, Badge, BadgeTone, Button, ButtonVariant, Callout, CalloutTone, OptionCard, Overline,
    Select, SelectOption, Size,
};
use crate::i18n::{Key, keys, t};

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
    let (up, project) = (s.up.clone(), s.project.clone());
    let gateway = {
        let src = src.clone();
        Memo::new(move |_| src.with(|src| src.gateway.clone()))
    };
    // tC4 gives step 1 no footer at all (`footer={g ? … : null}`), so a change of step draws
    // the dialog again.
    let step2 = Memo::new(move |_| gateway.with(Option::is_some));
    (move || sources_modal(&actions, &src, &up, &project, gateway, step2.get())).into_any()
}

fn sources_modal(
    actions: &Actions,
    src: &ArcRwSignal<SourcesForm>,
    up: &ArcRwSignal<UpgradeState>,
    project: &ArcRwSignal<Option<CurrentProject>>,
    gateway: Memo<Option<GatewayPackage>>,
    step2: bool,
) -> AnyView {
    let footer: Option<ChildrenFn> = step2.then(|| {
        let (actions, src) = (actions.clone(), src.clone());
        Arc::new(move || {
            gateway.with_untracked(|g| {
                g.as_ref()
                    .map_or_else(|| ().into_any(), |g| footer(&actions, &src, g).into_any())
            })
        }) as ChildrenFn
    });
    let children: ChildrenFn = {
        let (actions, src) = (actions.clone(), src.clone());
        let (up, project) = (up.clone(), project.clone());
        Arc::new(move || {
            let (actions, src) = (actions.clone(), src.clone());
            let (up, project) = (up.clone(), project.clone());
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
                        {resource_updates(&actions, &up, &project, pinned)}
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
/// language sets: on demand and online only, the offers per language set and per scripture
/// text. Accepting installs the release and opens the confirmation, where the pins move.
fn resource_updates(
    actions: &Actions,
    up: &ArcRwSignal<UpgradeState>,
    project: &ArcRwSignal<Option<CurrentProject>>,
    pinned: bool,
) -> impl IntoView + use<> {
    let (actions, up, project) = (actions.clone(), up.clone(), project.clone());
    pinned.then(|| {
        let check = actions.clone();
        let checking = {
            let up = up.clone();
            Signal::derive(move || up.with(|up| up.checking))
        };
        let progress = {
            let up = up.clone();
            move || {
                up.with(|up| up.progress.clone()).map(|repo| {
                    view! {
                        <p data-testid="upgrade-progress" style="font-size: var(--fs-caption-lg); color: var(--text-secondary); font-weight: var(--fw-bold); margin: 0px;">
                            {t(keys::sources::PROGRESS, &[("repo", &repo)])}
                        </p>
                    }
                })
            }
        };
        let error = {
            let up = up.clone();
            move || {
                up.with(|up| up.error.clone()).map(|error| {
                    view! {
                        <Callout
                            tone=CalloutTone::Warn
                            attrs=Attrs::from([("role", "alert"), ("data-testid", "upgrade-error")])
                            style=crate::css! { "overflow-wrap": "anywhere" }
                        >
                            {error}
                        </Callout>
                    }
                })
            }
        };
        // Offers show only for the project they were asked for.
        let offers = {
            let (actions, up, project) = (actions.clone(), up.clone(), project.clone());
            move || {
                let here = project.with(|p| p.as_ref().map(|p| p.at.project.clone()));
                let (offers, installing) = up.with(|up| (up.offers.clone(), up.installing.clone()));
                let offers = offers.filter(|o| Some(&o.project) == here.as_ref())?;
                let sets = offers
                    .view
                    .sets
                    .iter()
                    .map(|offer| offer_card(&actions, offer, &installing))
                    .collect_view();
                Some(view! {
                    {sets}
                    {text_offers(&actions, &offers.view.texts, &installing)}
                })
            }
        };
        view! {
            <div data-testid="resource-updates" style="display: flex; flex-direction: column; gap: 8px; padding-bottom: 6px;">
                <div style="display: flex; align-items: center; gap: 10px;">
                    <Overline tag="span">{t(keys::upgrade::TITLE, &[])}</Overline>
                    <div style="flex: 1 1 0%;"></div>
                    <Button
                        size=Size::Sm
                        variant=ButtonVariant::Secondary
                        disabled=checking
                        attrs=Attrs::from([("data-testid", "check-updates")])
                        on_click=Callback::new(move |_| check.check_for_updates())
                    >
                        {move || if checking.get() { t(keys::upgrade::CHECKING, &[]) } else { t(keys::upgrade::CHECK, &[]) }}
                    </Button>
                </div>
                {progress}
                {error}
                {offers}
            </div>
        }
    })
}

const OFFER_BOX: &str = "border: var(--stroke) solid var(--border); border-radius: var(--radius-lg); padding: 10px 14px; display: flex; flex-direction: column; gap: 6px;";
const OFFER_TITLE: &str =
    "font-weight: var(--fw-heavy); color: var(--uw-ocean); font-size: var(--fs-ui-sm);";
const OFFER_ROW: &str =
    "font-size: var(--fs-caption-lg); color: var(--text-secondary); font-family: var(--font-mono);";
const CURRENT: &str =
    "font-size: var(--fs-caption); color: var(--tc-valid-strong); font-weight: var(--fw-bold);";

fn date_label(published: &Option<String>) -> String {
    published
        .as_deref()
        .map(|p| p.chars().take(10).collect())
        .unwrap_or_default()
}

fn row_text(change: &RepoMove) -> String {
    t(
        keys::upgrade::ROW,
        &[
            ("repo", &change.repo),
            ("from", &change.from),
            ("to", &change.to),
            ("date", &date_label(&change.published)),
        ],
    )
}

/// One language set's offer (tC4 `OfferCard`).
fn offer_card(actions: &Actions, offer: &SetOffer, installing: &Option<Installing>) -> AnyView {
    let rung = match offer.rung {
        SetRung::Primary => "primary",
        SetRung::Fallback => "fallback",
    };
    let key = match offer.rung {
        SetRung::Primary => keys::upgrade::set::PRIMARY,
        SetRung::Fallback => keys::upgrade::set::FALLBACK,
    };
    let title = t(key, &[("lang", &offer.language)]);
    let busy = *installing == Some(Installing::Set(offer.rung));
    let any = installing.is_some();
    let n = offer.moves.len();
    let action = if n > 0 {
        let (actions, which) = (actions.clone(), offer.rung);
        view! {
            <Button
                size=Size::Sm
                disabled=any
                attrs=Attrs::new().with("data-testid", format!("upgrade-set-{rung}"))
                on_click=Callback::new(move |_| actions.upgrade_set(which))
            >
                {if busy { t(keys::upgrade::INSTALLING, &[]) } else { t(keys::upgrade::UPGRADE_SET, &[("n", &n)]) }}
            </Button>
        }
        .into_any()
    } else {
        view! {
            <span data-testid=format!("upgrade-current-{rung}") style=CURRENT>{t(keys::upgrade::UP_TO_DATE, &[])}</span>
        }
        .into_any()
    };
    let rows = offer
        .moves
        .iter()
        .map(|m| view! { <div data-testid="upgrade-row" style=OFFER_ROW>{row_text(m)}</div> })
        .collect_view();
    view! {
        <div data-testid=format!("upgrade-offer-{rung}") data-upgrades=n.to_string() style=OFFER_BOX>
            <div style="display: flex; align-items: center; gap: 10px;">
                <span style=OFFER_TITLE>{title}</span>
                <div style="flex: 1 1 0%;"></div>
                {action}
            </div>
            {rows}
        </div>
    }
    .into_any()
}

/// One row for each scripture text with a newer release (tC4 `TextOffers`, #258).
fn text_offers(
    actions: &Actions,
    offers: &[TextOffer],
    installing: &Option<Installing>,
) -> impl IntoView + use<> {
    let any = installing.is_some();
    let current = offers.is_empty().then(|| {
        view! { <span data-testid="upgrade-texts-current" style=CURRENT>{t(keys::upgrade::TEXTS_CURRENT, &[])}</span> }
    });
    let rows = offers
        .iter()
        .map(|offer| {
            let repo = offer.change.repo.clone();
            let kind = match offer.kind {
                TextKind::Original => ("original", keys::upgrade::text::ORIGINAL),
                TextKind::Gateway => ("gateway", keys::upgrade::text::GATEWAY),
            };
            let busy = *installing == Some(Installing::Text(repo.clone()));
            let (actions, which) = (actions.clone(), repo.clone());
            view! {
                <div data-testid=format!("upgrade-text-{repo}") data-kind=kind.0 style="display: flex; align-items: center; gap: 10px;">
                    <span style="font-size: var(--fs-caption-lg); color: var(--text-secondary);">{t(kind.1, &[])}</span>
                    <span style=OFFER_ROW>{row_text(&offer.change)}</span>
                    <div style="flex: 1 1 0%;"></div>
                    <Button
                        size=Size::Sm
                        disabled=any
                        attrs=Attrs::new().with("data-testid", format!("upgrade-text-apply-{repo}"))
                        on_click=Callback::new(move |_| actions.upgrade_text(which.clone()))
                    >
                        {if busy { t(keys::upgrade::INSTALLING, &[]) } else { t(keys::upgrade::UPGRADE_TEXT, &[]) }}
                    </Button>
                </div>
            }
        })
        .collect_view();
    view! {
        <div data-testid="upgrade-texts" style=OFFER_BOX>
            <span style=OFFER_TITLE>{t(keys::upgrade::TEXTS, &[])}</span>
            {current}
            {rows}
        </div>
    }
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

/// The books in canonical order with their chapters and testament (tC4's `ALL_CODES` options).
/// A book in `held` is greyed out and badged, as Add a book shows the books the project has.
pub(super) fn book_options(held: &[BookId]) -> Vec<SelectOption> {
    BookId::ALL
        .iter()
        .enumerate()
        .map(|(i, book)| {
            let already = held.contains(book);
            SelectOption {
                value: book.code().to_owned(),
                label: book.english_name().to_owned(),
                code: Some(book.code().to_owned()),
                meta: Some(t(
                    keys::add_book::CHAPTERS_SHORT,
                    &[("n", &chapters(*book))],
                )),
                group: Some(t(
                    if i >= 39 {
                        keys::add_book::NT
                    } else {
                        keys::add_book::OT
                    },
                    &[],
                )),
                badge: already.then(|| t(keys::add_book::IN_THIS_BIBLE, &[])),
                disabled: already,
            }
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
        let (actions, package, autonym) = (actions.clone(), g.clone(), g.autonym.clone());
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
                    on_click=Callback::new(move |_| actions.ask_gateway_change(package.clone()))
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
            options=book_options(&[])
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
