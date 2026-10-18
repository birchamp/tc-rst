//! The page's root until unit A5 replaces it with the port of tC4's `App.jsx`: a sample line
//! in the shown language, the picker of the ten languages, and the internet switch's state.
//!
//! It is the only way the smoke can change the language before A5 exists, and it uses no
//! path of its own: the picker calls `show_locale`, as the language dialog will.

use leptos::prelude::*;
use tc_api::ids::LocaleId;

use super::{js, use_runtime};
use crate::i18n::{LOCALES, keys, show_locale, shown_locale, t};

/// A sample of the shown language, the language picker and the internet state.
#[component]
pub(super) fn StandIn() -> impl IntoView {
    let internet = use_runtime().map(|runtime| runtime.state().internet.clone());
    let on_change = move |event: leptos::ev::Event| {
        if let Ok(locale) = LocaleId::parse(&event_target_value(&event)) {
            js::mark("tc4:locale");
            show_locale(locale);
        }
    };
    let options = LOCALES
        .iter()
        .map(|locale| {
            let id = locale.id;
            view! {
                <option value=id prop:selected=move || shown_locale().as_str() == id>
                    {locale.label}
                </option>
            }
        })
        .collect_view();
    let internet_on = move || internet.as_ref().is_some_and(|on| on.get());
    let internet_for_attr = internet_on.clone();
    view! {
        <div
            data-testid="runtime-ready"
            style="padding:24px;display:flex;flex-direction:column;gap:12px"
        >
            <p data-testid="runtime-sample">{move || t(keys::account::LANGUAGE, &[])}</p>
            <select
                data-testid="runtime-locale"
                aria-label=move || t(keys::account::LANGUAGE, &[])
                on:change=on_change
            >
                {options}
            </select>
            <p
                data-testid="runtime-internet"
                data-on=move || internet_for_attr().to_string()
            >
                {move || {
                    if internet_on() {
                        t(keys::account::INTERNET_ON, &[])
                    } else {
                        t(keys::account::INTERNET_OFF, &[])
                    }
                }}
            </p>
        </div>
    }
}
