//! Project settings (M7, unit B9a; `tC4:src/views/modals/ProjectSettings.jsx`): a Home card's
//! Settings opens it. Name, language name and code are shown and not edited; the direction
//! chips and the font change the form, and Save changes is C6's. The Checking language card
//! names the project's checking language from its pins and offers the packages installed here
//! that cover its books; choosing one is C8's gateway change.

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::resources::{FromInstalls, GatewayPackage};
use tc_api::shown::Direction;

use crate::app::forms::SCRIPT_FONTS;
use crate::app::sources::{GatewayState, SettingsForm};
use crate::app::web::{Actions, AppCtx, use_app};
use crate::ds::{
    Attrs, Badge, BadgeSize, BadgeTone, Button, ButtonVariant, Callout, CalloutTone, FilterChip,
    OptionCard, Overline, Select, SelectOption, Size, Text, TextField, TextRole, TextTone,
};
use crate::i18n::{keys, t};

/// The dialog, while `s.modal` is Settings and its form is set.
#[component]
pub fn ProjectSettings() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let st = s.st.clone();
    let read = move |get: fn(&SettingsForm) -> String| {
        let st = st.clone();
        Signal::derive(move || st.with(|st| st.as_ref().map(get).unwrap_or_default()))
    };
    let save_disabled = {
        let st = s.st.clone();
        Signal::derive(move || st.with(|st| st.as_ref().is_none_or(SettingsForm::save_disabled)))
    };
    let footer: ChildrenFn = {
        let actions = actions.clone();
        Arc::new(move || {
            let (cancel, save) = (actions.clone(), actions.clone());
            view! {
                <Button variant=ButtonVariant::Secondary on_click=Callback::new(move |_| cancel.close_modal())>
                    {move || t(keys::new_bible::CANCEL, &[])}
                </Button>
                <Button disabled=save_disabled on_click=Callback::new(move |_| save.save_settings())>
                    {move || t(keys::settings::SAVE, &[])}
                </Button>
            }
            .into_any()
        })
    };
    let children: ChildrenFn = {
        let (actions, st) = (actions.clone(), s.st.clone());
        let name = read(|st| st.name.clone());
        let lang_name = read(|st| st.lang_name.clone());
        let code = read(|st| st.code.clone());
        let font = read(|st| st.font.clone());
        Arc::new(move || {
            let (actions, st) = (actions.clone(), st.clone());
            let on_font = {
                let actions = actions.clone();
                Callback::new(move |value: String| actions.patch_st(|st| st.font = value))
            };
            let fonts: Vec<SelectOption> = SCRIPT_FONTS
                .iter()
                .map(|f| SelectOption::from(*f))
                .collect();
            let error = {
                let st = st.clone();
                move || {
                    st.with(|st| st.as_ref().and_then(|st| st.error.clone())).map(|error| {
                        view! { <Callout tone=CalloutTone::Warn attrs=Attrs::from([("role", "alert")])>{error}</Callout> }
                    })
                }
            };
            view! {
                <TextField id="st-name" label=Signal::derive(|| t(keys::new_bible::NAME, &[])) value=name disabled=true />
                <div style="display: grid; grid-template-columns: 1fr 130px; gap: 12px;">
                    <TextField id="st-lang" label=Signal::derive(|| t(keys::new_bible::LANG_NAME, &[])) value=lang_name disabled=true />
                    <TextField id="st-code" label=Signal::derive(|| t(keys::new_bible::CODE, &[])) value=code disabled=true />
                </div>
                <p style="font-size: var(--fs-caption); letter-spacing: var(--track-12); color: var(--text-tertiary); margin: -8px 0px 0px;">
                    {move || t(keys::settings::RENAME_NOTE, &[])}
                </p>
                {direction(&actions, &st)}
                {gateway_card(&actions, &st)}
                <Select
                    id="st-font"
                    label=Signal::derive(|| t(keys::new_bible::FONT, &[]))
                    value=Signal::derive(move || Some(font.get()))
                    on_change=on_font
                    options=fonts
                />
                {error}
            }
            .into_any()
        })
    };
    let subtitle = {
        let st = s.st.clone();
        move || {
            st.with(|st| {
                st.as_ref().map(|st| {
                    let n = st.book_count;
                    if st.one_book() {
                        t(keys::settings::SUBTITLE_ONE, &[("name", &st.name)])
                    } else {
                        t(keys::settings::SUBTITLE, &[("name", &st.name), ("n", &n)])
                    }
                })
            })
            .unwrap_or_default()
        }
    };
    let close = actions.clone();
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some((move || move || t(keys::settings::TITLE, &[])).into()),
        subtitle: Some((move || subtitle.clone()).into()),
        close_label: Some(Signal::derive(|| t(keys::new_bible::CLOSE, &[]))),
        on_close: Some(Callback::new(move |()| close.close_modal())),
        footer: Some(footer),
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}

/// The direction chips.
fn direction(actions: &Actions, st: &ArcRwSignal<Option<SettingsForm>>) -> impl IntoView + use<> {
    let chip = |dir: Direction, label: crate::i18n::Key| {
        let (actions, st) = (actions.clone(), st.clone());
        let selected =
            Signal::derive(move || st.with(|st| st.as_ref().is_some_and(|st| st.dir == dir)));
        view! {
            <FilterChip
                selected=selected
                style=crate::css! { "flex": 1, "justify-content": "center", "border-radius": "var(--radius-md)" }
                on_click=Callback::new(move |_| actions.patch_st(|st| st.dir = dir))
            >
                {move || t(label, &[])}
            </FilterChip>
        }
    };
    view! {
        <div>
            <Overline tag="span" style=crate::css! { "display": "block", "margin-bottom": 6 }>
                {move || t(keys::new_bible::DIRECTION, &[])}
            </Overline>
            <div style="display: flex; gap: 8px;">
                {chip(Direction::Ltr, keys::wizard::LTR)}
                {chip(Direction::Rtl, keys::wizard::RTL)}
            </div>
        </div>
    }
}

fn dir_attr(dir: Direction) -> &'static str {
    match dir {
        Direction::Ltr => "ltr",
        Direction::Rtl => "rtl",
    }
}

/// The Checking language card (tC4 `GatewayCard`).
fn gateway_card(
    actions: &Actions,
    st: &ArcRwSignal<Option<SettingsForm>>,
) -> impl IntoView + use<> {
    let state = {
        let st = st.clone();
        Memo::new(move |_| st.with(|st| st.as_ref().map(|st| st.gateway.clone())))
    };
    let line = move || {
        let card = match state.get()? {
            GatewayState::Loading => return None,
            GatewayState::Failed => None,
            GatewayState::Read(card) => Some(card),
        };
        let current = card.as_ref().and_then(|c| c.current.clone());
        let failed = matches!(
            card.as_ref().map(|c| &c.options),
            Some(FromInstalls::Failed { .. })
        );
        if current.is_none() && failed {
            return None;
        }
        let text = match current {
            Some(current) => t(
                keys::settings::GATEWAY_CURRENT,
                &[("lang", &current.autonym), ("org", &current.gateway.owner)],
            ),
            None => t(keys::settings::GATEWAY_UNSET, &[]),
        };
        Some(view! {
            <Text
                role=TextRole::Caption
                attrs=Attrs::from([("data-testid", "settings-gateway-current")])
                style=crate::css! { "display": "block", "margin-bottom": 8 }
            >
                {text}
            </Text>
        })
    };
    let loading = move || {
        matches!(state.get(), Some(GatewayState::Loading)).then(|| {
            view! { <Text role=TextRole::Caption tone=TextTone::Muted>{t(keys::settings::GATEWAY_LOADING, &[])}</Text> }
        })
    };
    let error = move || match state.get() {
        Some(GatewayState::Read(card)) => match card.options {
            FromInstalls::Failed { reason } => Some(view! {
                <Callout
                    tone=CalloutTone::Warn
                    attrs=Attrs::from([("role", "alert"), ("data-testid", "settings-gateway-error")])
                    style=crate::css! { "overflow-wrap": "anywhere" }
                >
                    {reason}
                </Callout>
            }),
            FromInstalls::Listed(_) => None,
        },
        _ => None,
    };
    let options = {
        let actions = actions.clone();
        move || {
            let Some(GatewayState::Read(card)) = state.get() else {
                return Vec::new();
            };
            let FromInstalls::Listed(options) = card.options else {
                return Vec::new();
            };
            options
                .into_iter()
                .map(|g| option(&actions, g, card.current.as_ref()))
                .collect::<Vec<_>>()
        }
    };
    let manage = actions.clone();
    view! {
        <div data-testid="settings-gateway">
            <Overline tag="span" style=crate::css! { "display": "block", "margin-bottom": 6 }>
                {move || t(keys::new_bible::CHECKING_LANGUAGE, &[])}
            </Overline>
            {line}
            {loading}
            {error}
            <div style="display: flex; flex-direction: column; gap: 8px;">{options}</div>
            <Button
                variant=ButtonVariant::Ghost
                size=Size::Sm
                attrs=Attrs::from([("data-testid", "settings-manage-sources")])
                style=crate::css! { "margin-top": 8 }
                on_click=Callback::new(move |_| manage.manage_settings_sources())
            >
                {move || t(keys::settings::MANAGE_SOURCES, &[])}" →"
            </Button>
        </div>
    }
}

/// One package of the card (tC4's `OptionCard` per option).
fn option(
    actions: &Actions,
    g: GatewayPackage,
    current: Option<&tc_api::settings::CurrentGateway>,
) -> AnyView {
    let is_current = current.is_some_and(|c| {
        c.gateway.language == g.gateway.language
            && c.gateway.owner.eq_ignore_ascii_case(&g.gateway.owner)
    });
    let key = g.gateway.key();
    let attrs = Attrs::new()
        .with("data-testid", format!("settings-gateway-{key}"))
        .with("data-current", if is_current { "1" } else { "0" });
    let (dir, autonym) = (dir_attr(g.direction), g.autonym.clone());
    let trailing: crate::ds::Content = if is_current {
        (move || {
            view! {
                <Badge tone=BadgeTone::AccentSoft size=BadgeSize::Sm>
                    {t(keys::settings::GATEWAY_CURRENT_BADGE, &[])}
                </Badge>
            }
        })
        .into()
    } else {
        "→".into()
    };
    let on_click = (!is_current).then(|| {
        let (actions, package) = (actions.clone(), g.clone());
        Callback::new(move |_: web_sys::Event| actions.choose_settings_gateway(package.clone()))
    });
    let style = if is_current {
        crate::css! { "cursor": "default" }
    } else {
        crate::ds::Css::new()
    };
    let title = move || view! { <span dir=dir>{autonym.clone()}</span> };
    let (meta, description) = (g.name.clone(), g.gateway.owner.clone());
    // tC4 gives the current package no `onClick`, so it is no control.
    match on_click {
        Some(on_click) => view! {
            <OptionCard selected=is_current title=title meta=meta description=description
                trailing=trailing on_click=on_click style=style attrs=attrs />
        }
        .into_any(),
        None => view! {
            <OptionCard selected=is_current title=title meta=meta description=description
                trailing=trailing style=style attrs=attrs />
        }
        .into_any(),
    }
}
