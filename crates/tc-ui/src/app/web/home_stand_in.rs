//! Home's content until unit B2 ports `Home.jsx` (A5 D5): a sample line in the shown language,
//! the picker of the ten languages, and the internet switch's state. A4a's smoke and A6b's
//! environment tests read its test ids; B2 replaces it and moves them onto Home and the App
//! language dialog. The picker calls `show_locale`, as the language dialog's preview does.

use std::collections::{BTreeMap, HashMap};

use leptos::prelude::*;
use tc_api::ids::LocaleId;

use super::{ServerState, js, use_runtime};
use crate::i18n::{LOCALES, keys, show_locale, shown_locale, t};

/// A sample of the shown language, the language picker and the internet state.
#[component]
pub(super) fn HomeStandIn() -> impl IntoView {
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
    // How much of what only events carry the page holds, for the smoke: a restart forgets all of it.
    let state = use_runtime().map(|runtime| runtime.state().clone());
    let held = move |count: fn(&ServerState) -> usize| {
        let state = state.clone();
        move || state.as_ref().map_or(0, count).to_string()
    };
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
                data-testid="runtime-held"
                data-jobs=held(|state| state.jobs.with(BTreeMap::len))
                data-checkpoints=held(|state| state.checkpoints.with(HashMap::len))
                data-blocked=held(|state| state.blocked.with(HashMap::len))
                data-align-saves=held(|state| state.align_saves.with(HashMap::len))
            ></p>
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
