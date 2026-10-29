//! tC4's `src/views/AccountMenu.jsx` (D88, D95, #519 to #522), read top to bottom: the Door43
//! account menu at the right of the top bar (X6), the internet switch in it, and the dialogs it
//! opens: Help and guides (S13, `help_panel`), App language (M16), About and License (M17); and
//! the "Turn on the internet?" question (M15), which the app mounts over every other dialog.
//!
//! Opening the menu sends nothing: its account row shows only what this computer knows. Every
//! text reads the shown language, so the App language preview rewrites it in place.

use std::sync::Arc;

use leptos::html;
use leptos::prelude::*;
use tc_api::app::{BUILD_COMMIT, BUILD_VERSION};
use tc_api::endpoint::NetKind;
use tc_api::ids::LocaleId;
use wasm_bindgen::JsCast;

use super::help_panel::HelpPanel;
use crate::app::internet::InternetFlow;
use crate::app::language::LanguageDialog;
use crate::app::shell::{AccountState, account_state, js_trim, user_initials};
use crate::app::web::{Actions, AppCtx, use_app};
use crate::css::Css;
use crate::ds::{
    Anchor, Attrs, Button, ButtonVariant, Callout, CalloutTone, Corner, Dismiss, Edge, Fill,
    LayerAlign, LayerPlacement, Level, Rule, Select, SelectOption, Shadow, Surface, Weight, fixed,
};
use crate::i18n::{LOCALES, keys, locale_of, t, t_dyn};
use crate::roving::{Axis, Move, roving};

/// tC4's LICENSE as tC4 ships it (vendor-tc4.sh); its line that starts with "Copyright" is
/// About's copyright line (`tC4:src/data/about.ts`).
const LICENSE_TEXT: &str = include_str!("../../assets/about/LICENSE");

fn trigger_base() -> Css {
    crate::css! {
        "width": 34,
        "height": 34,
        "padding": 0,
        "border-radius": "var(--radius-pill)",
        "display": "flex",
        "align-items": "center",
        "justify-content": "center",
        "cursor": "pointer",
        "color": "#fff",
        "font": "inherit",
    }
}

fn sub() -> Css {
    crate::css! {
        "font-size": "var(--fs-caption)",
        "font-weight": 600,
        "color": "var(--text-secondary)",
        "white-space": "normal",
    }
}

fn body() -> Css {
    crate::css! {
        "margin": 0,
        "font-size": "var(--fs-ui-sm)",
        "color": "var(--text-secondary)",
        "line-height": "var(--lh-body)",
    }
}

/// tC4's `LICENSE_PRE`: `font: inherit` comes after the font size, as in tC4's object, so the
/// text takes the dialog's size.
fn license_pre() -> Css {
    body()
        .with("font", "inherit")
        .with("font-size", "var(--fs-ui-sm)")
        .with("white-space", "pre-wrap")
}

/// The person icon of a button with nobody signed in.
fn person() -> AnyView {
    use leptos::svg;
    svg::svg()
        .add_any_attr(fixed(&[
            ("width", "18"),
            ("height", "18"),
            ("viewBox", "0 0 24 24"),
            ("fill", "none"),
            ("stroke", "currentColor"),
            ("stroke-width", "2"),
            ("aria-hidden", "true"),
        ]))
        .child((
            svg::circle().add_any_attr(fixed(&[("cx", "12"), ("cy", "8"), ("r", "4")])),
            svg::path().add_any_attr(fixed(&[("d", "M4 21c0-4.4 3.6-7 8-7s8 2.6 8 7")])),
        ))
        .into_any()
}

/// A two-line menu row: the action, and one line that says what it does.
struct Row {
    test_id: &'static str,
    title: Box<dyn Fn() -> String + Send + Sync>,
    sub: Option<Box<dyn Fn() -> String + Send + Sync>>,
    role: &'static str,
    checked: Option<Signal<Option<String>>>,
    end: Option<Box<dyn FnOnce() -> AnyView + Send>>,
    on_click: Callback<web_sys::MouseEvent>,
}

impl Row {
    fn new(
        test_id: &'static str,
        title: impl Fn() -> String + Send + Sync + 'static,
        on_click: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            test_id,
            title: Box::new(title),
            sub: None,
            role: "menuitem",
            checked: None,
            end: None,
            on_click: Callback::new(move |_| on_click()),
        }
    }

    fn sub(mut self, sub: impl Fn() -> String + Send + Sync + 'static) -> Self {
        self.sub = Some(Box::new(sub));
        self
    }

    fn view(self) -> AnyView {
        let Row {
            test_id,
            title,
            sub: line,
            role,
            checked,
            end,
            on_click,
        } = self;
        let live_attrs = checked
            .map(|c| vec![("aria-checked", c)])
            .unwrap_or_default();
        crate::ds::Action(crate::ds::ActionProps {
            weight: Weight::Row,
            on_click: Some(on_click),
            style: Signal::from(crate::css! {
                "padding": "8px 10px",
                "border-radius": 8,
                "width": "100%",
            }),
            attrs: Attrs::from([("role", role), ("data-testid", test_id)]),
            live_attrs,
            children: Some(Box::new(move || {
                let column = "flex: 1; display: flex; flex-direction: column; gap: 2px; min-width: 230px;";
                view! {
                    <span style=column>
                        <span>{move || title()}</span>
                        {line.map(|line| view! { <span style=sub().to_style()>{move || line()}</span> })}
                    </span>
                    {end.map(|end| end())}
                }
                .into_any()
            })),
            ..Default::default()
        })
        .into_any()
    }
}

/// The switch at the end of the internet row.
fn switch(on: Signal<bool>) -> AnyView {
    let track = move || {
        let on = on.get();
        crate::css! {
            "width": 32,
            "height": 18,
            "padding": 2,
            "border-radius": "var(--radius-pill)",
            "flex": "none",
            "box-sizing": "border-box",
            "display": "flex",
            "justify-content": if on { "flex-end" } else { "flex-start" },
            "background": if on { "var(--uw-inspire)" } else { "var(--border-strong)" },
        }
        .to_style()
    };
    let knob = crate::css! {
        "width": 14,
        "height": 14,
        "border-radius": "var(--radius-pill)",
        "background": "#fff",
    };
    view! {
        <span aria-hidden="true" style=track>
            <span style=knob.to_style()></span>
        </span>
    }
    .into_any()
}

/// What About shows after License closes: the focus goes back to "Read the license".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AboutStep {
    About,
    License,
    Back,
}

/// The account button and its menu (`tC4:AccountMenu.jsx:73-180`).
#[component]
pub fn AccountMenu() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let open = RwSignal::new(false);
    let about = RwSignal::new(None::<AboutStep>);
    let language = RwSignal::new(false);
    let help = RwSignal::new(false);
    let wrapper = NodeRef::<html::Span>::new();
    let panel = NodeRef::<html::Div>::new();

    let account = s.server.account.clone();
    let internet = s.server.internet.clone();
    // A memo: every re-read of `hello` sets the switch again, which must not draw the dot again.
    let online = Signal::from(Memo::new(move |_| internet.get()));
    let user = {
        let account = account.clone();
        Memo::new(move |_| account.with(|a| a.user.clone()))
    };
    let state = {
        let account = account.clone();
        Memo::new(move |_| account.with(|a| account_state(a).data_state()))
    };
    let label = {
        let account = account.clone();
        move || account.with(|a| account_state(a).label())
    };

    hold_first_row(open, panel);

    let focus_trigger = move || {
        let button = wrapper
            .get_untracked()
            .and_then(|w| w.query_selector("button").ok().flatten())
            .and_then(|b| b.dyn_into::<web_sys::HtmlElement>().ok());
        if let Some(button) = button {
            let _ = button.focus();
        }
    };

    let trigger = {
        let label = label.clone();
        let title = {
            let label = label.clone();
            move || {
                if online.get() {
                    t(keys::net::on::TOOLTIP, &[])
                } else {
                    label()
                }
            }
        };
        let aria_label = {
            let (label, title) = (label.clone(), title.clone());
            move || {
                if online.get() {
                    format!("{}. {}", label(), title())
                } else {
                    label()
                }
            }
        };
        let style = move || {
            let mut style = trigger_base();
            if user.with(Option::is_some) {
                style.extend(&crate::css! {
                    "background": "var(--uw-inspire)",
                    "border": "2px solid rgba(255,255,255,.9)",
                    "font-weight": 900,
                    "font-size": 12.5,
                });
            } else {
                style.extend(&crate::css! {
                    "background": "rgba(255,255,255,.12)",
                    "border": "1.5px solid rgba(255,255,255,.55)",
                });
            }
            style.set(
                "box-shadow",
                if open.get() {
                    "0 0 0 3px rgba(49,173,227,.45)"
                } else {
                    "none"
                },
            );
            style.to_style()
        };
        let dot = crate::css! {
            "position": "absolute",
            "inset-inline-end": -2,
            "top": -2,
            "width": 11,
            "height": 11,
            "border-radius": "var(--radius-pill)",
            "background": "var(--uw-inspire)",
            "border": "2px solid #fff",
            "box-sizing": "border-box",
            "pointer-events": "none",
        };
        view! {
            <span node_ref=wrapper style="display: inline-flex; position: relative;">
                <button
                    type="button"
                    data-testid="account-menu"
                    data-state=move || state.get()
                    data-internet=move || if online.get() { "on" } else { "off" }
                    aria-haspopup="menu"
                    aria-expanded=move || open.get().to_string()
                    aria-label=aria_label
                    title=title
                    on:click=move |_| open.update(|o| *o = !*o)
                    style=style
                >
                    {move || match user.get() {
                        Some(user) => user_initials(&user).into_any(),
                        None => person(),
                    }}
                </button>
                {move || {
                    online
                        .get()
                        .then(|| view! { <span data-testid="internet-indicator" aria-hidden="true" style=dot.to_style()></span> })
                }}
            </span>
        }
    };

    let menu = menu_layer(MenuParts {
        open,
        panel,
        wrapper,
        label: label.clone(),
        online,
        ctx: AppCtx {
            s: s.clone(),
            actions: actions.clone(),
        },
        help,
        language,
        about,
    });

    let close_language = move || {
        language.set(false);
        focus_trigger();
    };
    let close_help = move || {
        help.set(false);
        focus_trigger();
    };
    let close_about = move || {
        about.set(None);
        focus_trigger();
    };
    let dialogs = {
        let actions = actions.clone();
        let applied = s.server.saved_locale.clone();
        view! {
            {move || {
                language.get().then(|| {
                    let actions = actions.clone();
                    view! {
                        <span data-on="light" style="display: contents;">
                            <AppLanguageDialog
                                applied=applied.get_untracked()
                                actions=actions
                                on_close=Callback::new(move |()| close_language())
                            />
                        </span>
                    }
                })
            }}
            {move || {
                help.get().then(|| {
                    view! {
                        <span data-on="light" style="display: contents;">
                            <HelpPanel on_close=Callback::new(move |()| close_help()) />
                        </span>
                    }
                })
            }}
            {move || {
                about.get().map(|step| {
                    let dialog = match step {
                        AboutStep::License => view! {
                            <LicenseDialog on_close=Callback::new(move |()| about.set(Some(AboutStep::Back))) />
                        }
                        .into_any(),
                        AboutStep::About | AboutStep::Back => view! {
                            <AboutDialog
                                focus_read=step == AboutStep::Back
                                on_read=Callback::new(move |()| about.set(Some(AboutStep::License)))
                                on_close=Callback::new(move |()| close_about())
                            />
                        }
                        .into_any(),
                    };
                    view! { <span data-on="light" style="display: contents;">{dialog}</span> }
                })
            }}
        }
    };

    view! {
        {trigger}
        {menu}
        {dialogs}
    }
    .into_any()
}

/// The menu keeps its first row focused for its first frames: the Layer settles over them and
/// can hand the focus back to the trigger (`tC4:AccountMenu.jsx:97-108`). The effect's cleanup
/// cancels the frames, as React cancels tC4's when `open` changes or the menu unmounts.
///
/// The first try waits for the next frame. React runs the Layer's effect, a child's, before
/// this one, so the Layer notes the trigger as the element to give the focus back to; here this
/// effect would run first, and on a menu opened again while the last one still fades out, it
/// would focus that panel's first row before the Layer noted it, and the Layer would give the
/// focus back to that row when the menu closes.
///
/// Each frame also stops the hold as soon as the menu closes: the effect whose cleanup cancels
/// the frames runs later than the next frame, and a row focused then would take the focus from
/// the dialog the row opened.
fn hold_first_row(open: RwSignal<bool>, panel: NodeRef<html::Div>) {
    Effect::new(move |_| {
        if open.get() {
            super::each_frame(11, move || {
                if !open.try_get_untracked().unwrap_or(false) {
                    return false;
                }
                if let Some(panel) = panel.try_get_untracked().flatten() {
                    let active = document().active_element();
                    let inside = active.as_ref().is_some_and(|a| panel.contains(Some(a)));
                    if !inside && let Some(first) = menu_rows(&panel).into_iter().next() {
                        let _ = first.focus();
                    }
                }
                true
            });
        }
    });
}

/// The rows the keyboard moves between: `[role^="menuitem"]`.
fn menu_rows(panel: &web_sys::HtmlDivElement) -> Vec<web_sys::HtmlElement> {
    let Ok(list) = panel.query_selector_all("[role^=\"menuitem\"]") else {
        return Vec::new();
    };
    (0..list.length())
        .filter_map(|i| list.item(i))
        .filter_map(|n| n.dyn_into::<web_sys::HtmlElement>().ok())
        .collect()
}

struct MenuParts<L> {
    open: RwSignal<bool>,
    panel: NodeRef<html::Div>,
    wrapper: NodeRef<html::Span>,
    label: L,
    online: Signal<bool>,
    ctx: AppCtx,
    help: RwSignal<bool>,
    language: RwSignal<bool>,
    about: RwSignal<Option<AboutStep>>,
}

fn menu_layer<L: Fn() -> String + Clone + Send + Sync + 'static>(parts: MenuParts<L>) -> AnyView {
    let MenuParts {
        open,
        panel,
        wrapper,
        label,
        online,
        ctx,
        help,
        language,
        about,
    } = parts;
    let anchor: Anchor = Callback::new(move |_| wrapper.get_untracked().map(Into::into));
    // The menu keyboard contract (AC 1): the arrows, Home and End move between rows, and Tab
    // leaves the menu. Escape and the focus return to the trigger are the Layer's.
    let on_key_down = move |e: web_sys::KeyboardEvent| {
        let Some(panel) = panel.get_untracked() else {
            return;
        };
        let list = menu_rows(&panel);
        let active = document().active_element();
        let at = list.iter().position(|row| {
            active
                .as_ref()
                .is_some_and(|a| a == row.unchecked_ref::<web_sys::Element>())
        });
        let key = e.key();
        if key == "Tab" {
            open.set(false);
        }
        let Some(target) =
            Move::of_key(&key, Axis::Vertical).and_then(|step| roving(list.len(), at, step))
        else {
            return;
        };
        e.prevent_default();
        if let Some(row) = list.get(target) {
            let _ = row.focus();
        }
    };
    let content: ChildrenFn = Arc::new(move || {
        let ctx = ctx.clone();
        let panel_style = crate::css! {
            "display": "flex",
            "flex-direction": "column",
            "gap": 1,
            "color": "var(--text-body)",
            "--fg": "var(--text-body)",
            "--fg-muted": "var(--text-secondary)",
            "--line": "var(--border)",
        };
        view! {
            <Surface
                fill=Fill::Card
                border=Edge::Line
                radius=Corner::Lg
                elevation=Shadow::Hover
                pad=6.0
                attrs=Attrs::from([("data-testid", "account-menu-panel")])
            >
                <div node_ref=panel on:keydown=on_key_down style=panel_style.to_style()>
                    {menu_rows_view(&ctx, open, online, help, language, about)}
                </div>
            </Surface>
        }
        .into_any()
    });
    crate::ds::Layer(crate::ds::LayerProps {
        open: Some(open.into()),
        level: Level::Popover,
        placement: LayerPlacement::Anchor,
        anchor_to: Some(anchor),
        align: LayerAlign::End,
        role: Some("menu".to_owned()),
        label: Some(Signal::derive(label)),
        dismiss: Dismiss::parse("outside escape"),
        on_dismiss: Signal::from(Some(Callback::new(move |_| open.set(false)))),
        ..crate::ds::LayerProps::new(content)
    })
    .into_any()
}

/// The rows by account state, the internet switch, and the rows every state has
/// (`tC4:AccountMenu.jsx:130-153`).
fn menu_rows_view(
    ctx: &AppCtx,
    open: RwSignal<bool>,
    online: Signal<bool>,
    help: RwSignal<bool>,
    language: RwSignal<bool>,
    about: RwSignal<Option<AboutStep>>,
) -> impl IntoView + use<> {
    let AppCtx { s, actions } = ctx.clone();
    // A row's action closes the menu, then runs.
    let choose = move |run: Box<dyn Fn() + Send + Sync>| {
        move || {
            open.set(false);
            run();
        }
    };
    let account = s.server.account.clone();
    let account_rows = {
        let actions = actions.clone();
        let account = account.clone();
        move || {
            let actions = actions.clone();
            account.with(|a| match account_state(a) {
                AccountState::In(user) => {
                    let title = format!("@{user}");
                    Row::new(
                        "account-page",
                        move || title.clone(),
                        choose(Box::new(move || actions.open_profile())),
                    )
                    .sub(|| t(keys::account::OPEN_PAGE, &[]))
                    .view()
                }
                AccountState::Saved => Row::new(
                    "account-check",
                    || t(keys::account::SAVED, &[]),
                    choose(Box::new(move || actions.check_saved_sign_in())),
                )
                .sub(|| t(keys::account::SAVED_CHECK, &[]))
                .view(),
                AccountState::Out => Row::new(
                    "account-sign-in",
                    || t(keys::account::SIGN_IN, &[]),
                    choose(Box::new(move || actions.open_sign_in())),
                )
                .sub(|| t(keys::account::SIGN_IN_HINT, &[]))
                .view(),
            })
        }
    };
    let notice = {
        let account = account.clone();
        move || {
            account.with(|a| a.notice).map(|notice| {
                let style = sub().with("margin", "2px 10px 6px").with("max-width", 260);
                view! {
                    <p role="alert" data-testid="account-error" style=style.to_style()>
                        {move || t_dyn(notice.key(), &[], None)}
                    </p>
                }
            })
        }
    };
    let internet_row = {
        let actions = actions.clone();
        let mut row = Row::new(
            "account-internet",
            || t(keys::account::INTERNET, &[]),
            move || actions.toggle_internet(),
        )
        .sub(move || {
            if online.get() {
                t(keys::account::INTERNET_ON, &[])
            } else {
                t(keys::account::INTERNET_OFF, &[])
            }
        });
        row.role = "menuitemcheckbox";
        row.checked = Some(Signal::derive(move || Some(online.get().to_string())));
        row.end = Some(Box::new(move || switch(online)));
        row.view()
    };
    let saved_locale = s.server.saved_locale.clone();
    let language_label = move || saved_locale.with(|id: &LocaleId| locale_of(*id).label.to_owned());
    let sign_out = {
        let actions = actions.clone();
        move || {
            let actions = actions.clone();
            account
                .with(|a| !matches!(account_state(a), AccountState::Out))
                .then(|| {
                    view! {
                        <Rule style=crate::css! { "margin": "5px 4px" } />
                        {Row::new(
                            "account-sign-out",
                            || t(keys::account::SIGN_OUT, &[]),
                            choose(Box::new(move || actions.sign_out())),
                        )
                        .view()}
                    }
                })
        }
    };
    // F1: a Bible open on screen can take a book edited elsewhere (STORAGE §13.6). Stories have
    // no update yet: tc-api has no story update.
    let update_book = {
        let (actions, project) = (actions.clone(), s.project.clone());
        move || {
            let actions = actions.clone();
            project
                .with(|p| {
                    p.as_ref().is_some_and(|p| {
                        p.flavor == crate::app::shell::ProjectFlavor::TextTranslation
                    })
                })
                .then(|| {
                    view! {
                        <Rule style=crate::css! { "margin": "5px 4px" } />
                        {Row::new(
                            "account-update-book",
                            || t(keys::account::UPDATE_BOOK, &[]),
                            choose(Box::new(move || actions.open_update_book())),
                        )
                        .sub(|| t(keys::account::UPDATE_BOOK_HINT, &[]))
                        .view()}
                    }
                })
        }
    };
    let report = actions.clone();
    view! {
        {account_rows}
        {notice}
        <Rule style=crate::css! { "margin": "5px 4px" } />
        {internet_row}
        {Row::new("account-help", || t(keys::account::HELP, &[]), choose(Box::new(move || help.set(true)))).view()}
        {Row::new("account-language", || t(keys::account::LANGUAGE, &[]), choose(Box::new(move || language.set(true))))
            .sub(language_label)
            .view()}
        {Row::new("account-report", || t(keys::account::REPORT, &[]), choose(Box::new(move || report.report_problem()))).view()}
        {Row::new("account-about", || t(keys::account::ABOUT, &[]), choose(Box::new(move || about.set(Some(AboutStep::About))))).view()}
        {update_book}
        {sign_out}
    }
}

/// The ✕'s title in every dialog of this file.
fn close_label() -> Signal<String> {
    Signal::derive(|| t(keys::common::CLOSE, &[]))
}

/// About translationCore (#520): the build's version and commit, the copyright line and the
/// license name. Every value is compiled in, so opening it sends no request.
#[component]
fn AboutDialog(focus_read: bool, on_read: Callback<()>, on_close: Callback<()>) -> impl IntoView {
    let read = NodeRef::<html::Div>::new();
    if focus_read {
        // Back from License: the focus returns to "Read the license", held while the Layer
        // puts it on its first control over its first frames. Closing About inside them ends
        // the hold (`tC4:AccountMenu.jsx:298-309`).
        let hold = move || {
            let button = read
                .try_get_untracked()
                .flatten()
                .and_then(|div| div.query_selector("button").ok().flatten())
                .and_then(|b| b.dyn_into::<web_sys::HtmlElement>().ok());
            if let Some(button) = button {
                let _ = button.focus();
            }
            true
        };
        hold();
        super::each_frame(10, hold);
    }
    let copyright = LICENSE_TEXT
        .split('\n')
        .find(|line| line.starts_with("Copyright"))
        .map(|line| js_trim(line).to_owned())
        .unwrap_or_default();
    let body_style = body().with("color", "var(--text-body)").to_style();
    let plain = body().to_style();
    let footer: ChildrenFn = Arc::new(move || {
        view! {
            <Button attrs=Attrs::from([("data-testid", "about-close")]) on_click=Callback::new(move |_| on_close.run(()))>
                {move || t(keys::common::CLOSE, &[])}
            </Button>
        }
        .into_any()
    });
    let children: ChildrenFn = Arc::new(move || {
        let (body_style, plain, copyright) = (body_style.clone(), plain.clone(), copyright.clone());
        view! {
            <p style=body_style>
                {move || t(keys::about::VERSION, &[])}
                " "
                <strong data-testid="about-version">{format!("{BUILD_VERSION} ({BUILD_COMMIT})")}</strong>
            </p>
            <p style=plain.clone() data-testid="about-copyright">{copyright}</p>
            <p style=plain data-testid="about-license-name">{move || t(keys::about::LICENSE_NAME, &[])}</p>
            <div node_ref=read>
                <Button
                    variant=ButtonVariant::Secondary
                    attrs=Attrs::from([("data-testid", "about-read-license")])
                    on_click=Callback::new(move |_| on_read.run(()))
                >
                    {move || t(keys::about::READ_LICENSE, &[])}
                </Button>
            </div>
        }
        .into_any()
    });
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some((move || move || t(keys::account::ABOUT, &[])).into()),
        width: Some(460.into()),
        close_label: Some(close_label()),
        on_close: Some(on_close),
        footer: Some(footer),
        attrs: Attrs::from([("data-testid", "about-dialog")]),
        children: Some(children),
        ..Default::default()
    })
}

/// The LICENSE text (the MIT license since tC4 #838), in one region the keyboard can reach and
/// scroll (#520). Closing it returns to About.
#[component]
fn LicenseDialog(on_close: Callback<()>) -> impl IntoView {
    let footer: ChildrenFn = Arc::new(move || {
        view! {
            <Button attrs=Attrs::from([("data-testid", "license-close")]) on_click=Callback::new(move |_| on_close.run(()))>
                {move || t(keys::common::CLOSE, &[])}
            </Button>
        }
        .into_any()
    });
    let children: ChildrenFn = Arc::new(move || {
        let region = crate::css! {
            "max-height": "52vh",
            "overflow": "auto",
            "padding": "10px 12px",
            "border": "1px solid var(--border)",
            "border-radius": 8,
        };
        let notice = license_pre().with("margin", 0).to_style();
        view! {
            <div
                role="region"
                aria-label=move || t(keys::about::LICENSE_TEXT, &[])
                tabindex="0"
                data-testid="license-text"
                style=region.to_style()
            >
                <pre data-testid="license-notice" style=notice>{LICENSE_TEXT}</pre>
            </div>
        }
        .into_any()
    });
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some((move || move || t(keys::about::LICENSE, &[])).into()),
        width: Some(720.into()),
        close_label: Some(close_label()),
        on_close: Some(on_close),
        footer: Some(footer),
        attrs: Attrs::from([("data-testid", "license-dialog")]),
        children: Some(children),
        ..Default::default()
    })
}

/// App language (#522, M16): one select of the installed catalogs, Apply and Cancel. A
/// selection previews the language across the app, this dialog included; Apply saves it and
/// closes once the save succeeded; Cancel, Escape, the ✕ and the scrim show the applied
/// language again and save nothing. While the save runs the select, Apply and Cancel are off
/// and the ✕, Escape and the scrim are withdrawn. A failed save keeps the preview and the
/// dialog, with an error.
#[component]
pub fn AppLanguageDialog(
    applied: LocaleId,
    actions: Actions,
    on_close: Callback<()>,
) -> impl IntoView {
    let (dialog, show) = LanguageDialog::open(applied);
    actions.preview_app_locale(show);
    let dialog = ArcRwSignal::new(dialog);
    // A memo, so a preview, which changes the dialog's state but not the lock, does not draw
    // the ✕ again: the Modal draws it from `closable`.
    let locked = {
        let dialog = dialog.clone();
        Signal::from(Memo::new(move |_| dialog.with(LanguageDialog::locked)))
    };
    let cancel = {
        let (dialog, actions) = (dialog.clone(), actions.clone());
        Callback::new(move |()| {
            if let Some(id) = dialog.with_untracked(LanguageDialog::cancel) {
                actions.preview_app_locale(id);
                on_close.run(());
            }
        })
    };
    let apply = {
        let (dialog, actions) = (dialog.clone(), actions.clone());
        Callback::new(move |()| {
            let Some(id) = dialog.try_update(LanguageDialog::start_apply).flatten() else {
                return;
            };
            let (dialog, actions) = (dialog.clone(), actions.clone());
            wasm_bindgen_futures::spawn_local(async move {
                match actions.apply_app_locale(id).await {
                    Ok(()) => on_close.run(()),
                    Err(_) => {
                        dialog.try_update(LanguageDialog::apply_failed);
                    }
                }
            });
        })
    };
    let select = {
        let (dialog, actions) = (dialog.clone(), actions.clone());
        Callback::new(move |value: String| {
            let Ok(id) = LocaleId::parse(&value) else {
                return;
            };
            if let Some(id) = dialog.try_update(|d| d.select(id)).flatten() {
                actions.preview_app_locale(id);
            }
        })
    };
    let footer: ChildrenFn = {
        let dialog = dialog.clone();
        Arc::new(move || {
            let apply_off = {
                let dialog = dialog.clone();
                Signal::derive(move || !dialog.with(LanguageDialog::apply_enabled))
            };
            view! {
                <Button
                    variant=ButtonVariant::Secondary
                    disabled=locked
                    attrs=Attrs::from([("data-testid", "language-cancel")])
                    on_click=Callback::new(move |_| cancel.run(()))
                >
                    {move || t(keys::language::CANCEL, &[])}
                </Button>
                <Button
                    disabled=apply_off
                    attrs=Attrs::from([("data-testid", "language-apply")])
                    on_click=Callback::new(move |_| apply.run(()))
                >
                    {move || t(keys::language::APPLY, &[])}
                </Button>
            }
            .into_any()
        })
    };
    let children: ChildrenFn = {
        let dialog = dialog.clone();
        Arc::new(move || {
            let preview = {
                let dialog = dialog.clone();
                Signal::derive(move || Some(dialog.with(|d| d.preview().as_str().to_owned())))
            };
            let error = {
                let dialog = dialog.clone();
                Memo::new(move |_| dialog.with(LanguageDialog::error))
            };
            let options: Vec<SelectOption> = LOCALES
                .iter()
                .map(|l| SelectOption {
                    value: l.id.to_owned(),
                    label: l.label.to_owned(),
                    ..SelectOption::from(l.id)
                })
                .collect();
            let hint = body().with("color", "var(--text-body)").to_style();
            view! {
                <p style=hint>{move || t(keys::language::HINT, &[])}</p>
                <Select
                    id="app-language"
                    label=move || t(keys::language::FIELD, &[])
                    value=preview
                    disabled=locked
                    attrs=Attrs::from([("data-testid", "language-select")])
                    on_change=select
                    options=options
                    search_placeholder=move || t(keys::language::FIND, &[])
                    no_matches_label=move || t(keys::add_book::NO_MATCHES, &[])
                    clear_label=move || t(keys::common::CLEAR, &[])
                />
                {move || {
                    error.get().then(|| view! {
                        <Callout tone=CalloutTone::Warn attrs=Attrs::from([("role", "alert"), ("data-testid", "language-error")])>
                            {move || t(keys::language::SAVE_FAILED, &[])}
                        </Callout>
                    })
                }}
            }
            .into_any()
        })
    };
    let saving = {
        let dialog = dialog.clone();
        Signal::derive(move || {
            Some(
                if dialog.with(LanguageDialog::locked) {
                    "true"
                } else {
                    "false"
                }
                .to_owned(),
            )
        })
    };
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some((move || move || t(keys::language::TITLE, &[])).into()),
        width: Some(420.into()),
        close_label: Some(close_label()),
        on_close: Some(cancel),
        closable: Signal::derive(move || !locked.get()),
        footer: Some(footer),
        attrs: Attrs::from([("data-testid", "language-dialog")]),
        live_attrs: vec![("data-saving", saving)],
        children: Some(children),
        ..Default::default()
    })
}

/// What the internet dialog draws: the question of one kind, or the failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NetDialog {
    None,
    Ask(NetKind),
    Failed,
}

/// The kind's name in `data-kind`: its wire name, which is its key's last part.
fn kind_name(kind: NetKind) -> &'static str {
    kind.ask_key().trim_start_matches("net.ask.")
}

/// "Turn on the internet?" (D95, M15): what the step does, and that the internet stays on
/// until the person turns it off or closes tC4; and "tC4 could not use the internet" (D88).
/// A question of another kind is a new dialog (tC4's `key={kind}`). "Turn on internet" closes
/// the question at once, as tC4's does, while the switch turns on.
#[component]
pub fn InternetDialog() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let flow = s.layers.internet.clone();
    let shown = Memo::new(move |_| {
        flow.with(|flow| match (flow, flow.asking()) {
            (InternetFlow::Failed, _) => NetDialog::Failed,
            (_, Some(kind)) => NetDialog::Ask(kind),
            (_, None) => NetDialog::None,
        })
    });
    (move || match shown.get() {
        NetDialog::None => ().into_any(),
        NetDialog::Failed => failed_dialog(&actions).into_any(),
        NetDialog::Ask(kind) => ask_dialog(&actions, kind).into_any(),
    })
    .into_any()
}

fn failed_dialog(actions: &Actions) -> impl IntoView + use<> {
    let close = {
        let actions = actions.clone();
        Callback::new(move |()| actions.close_net_failed())
    };
    let footer: ChildrenFn = Arc::new(move || {
        view! {
            <Button attrs=Attrs::from([("data-testid", "net-failed-close")]) on_click=Callback::new(move |_| close.run(()))>
                {move || t(keys::net::failed::CLOSE, &[])}
            </Button>
        }
        .into_any()
    });
    let children: ChildrenFn = Arc::new(move || {
        view! { <p role="alert" style=body().to_style()>{move || t(keys::net::failed::BODY, &[])}</p> }
            .into_any()
    });
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some((move || move || t(keys::net::failed::TITLE, &[])).into()),
        width: Some(460.into()),
        close_label: Some(close_label()),
        on_close: Some(close),
        footer: Some(footer),
        attrs: Attrs::from([("data-testid", "net-failed")]),
        children: Some(children),
        ..Default::default()
    })
}

fn ask_dialog(actions: &Actions, kind: NetKind) -> impl IntoView + use<> {
    let cancel = {
        let actions = actions.clone();
        Callback::new(move |()| actions.cancel_internet())
    };
    let confirm = {
        let actions = actions.clone();
        Callback::new(move |()| actions.turn_on_internet())
    };
    let footer: ChildrenFn = Arc::new(move || {
        view! {
            <Button
                variant=ButtonVariant::Secondary
                attrs=Attrs::from([("data-testid", "net-cancel")])
                on_click=Callback::new(move |_| cancel.run(()))
            >
                {move || t(keys::net::ask::NOT_NOW, &[])}
            </Button>
            <Button attrs=Attrs::from([("data-testid", "net-confirm")]) on_click=Callback::new(move |_| confirm.run(()))>
                {move || t(keys::net::ask::TURN_ON, &[])}
            </Button>
        }
        .into_any()
    });
    let children: ChildrenFn = Arc::new(move || {
        let reason = body().with("color", "var(--text-body)").to_style();
        let until = body().with("margin-top", 14).to_style();
        view! {
            <p style=reason data-testid="net-ask-reason">
                {move || {
                    let what = t_dyn(kind.ask_key(), &[], None);
                    t(keys::net::ask::STEP, &[("what", &what)])
                }}
            </p>
            <p style=until>{move || t(keys::net::ask::UNTIL, &[])}</p>
        }
        .into_any()
    });
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some((move || move || t(keys::net::ask::TITLE, &[])).into()),
        width: Some(520.into()),
        close_label: Some(close_label()),
        on_close: Some(cancel),
        footer: Some(footer),
        attrs: Attrs::from([("data-testid", "net-ask"), ("data-kind", kind_name(kind))]),
        children: Some(children),
        ..Default::default()
    })
}
