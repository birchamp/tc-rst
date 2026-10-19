//! The "Language name" field of New Bible and New Open Bible Stories (tC4 #492,
//! `tC4:src/views/modals/LanguageNameField.jsx`): it suggests languages from Door43's list as
//! the person types. The list stays on the server (AD27) and nothing here uses the network: each
//! change asks `home/languages`, and a reply for an older value is dropped. A choice fills the
//! name, the Code box and the direction; all three stay editable.

use std::cell::Cell;
use std::rc::Rc;

use leptos::prelude::*;
use send_wrapper::SendWrapper;
use tc_api::home::LanguageSuggestion;
use wasm_bindgen_futures::spawn_local;

use crate::app::shell::js_trim;
use crate::app::web::{AppCtx, use_app};
use crate::ds::{SelectOption, SuggestField};
use crate::i18n::{keys, t};

/// The field, with input id `id` (`nb-lang` or `no-lang`).
#[component]
pub fn LanguageNameField(id: &'static str) -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let found: RwSignal<Vec<LanguageSuggestion>> = RwSignal::new(Vec::new());
    let asked = SendWrapper::new(Rc::new(Cell::new(0u64)));
    // The list is read once per process on the server; asking with nothing typed reads it while
    // the dialog opens, as tC4 loads it when the field mounts, so the first keystroke is quick.
    {
        let actions = actions.clone();
        spawn_local(async move {
            let _ = actions.languages(String::new()).await;
        });
    }
    let value = {
        let np = s.np.clone();
        Signal::derive(move || {
            np.with(|np| {
                np.as_ref()
                    .map(|np| np.lang_name.clone())
                    .unwrap_or_default()
            })
        })
    };
    let suggestions = Signal::derive(move || {
        found.with(|found| {
            found
                .iter()
                .map(|s| {
                    let door43 = s
                        .door43
                        .clone()
                        .or_else(|| s.code.clone())
                        .unwrap_or_default();
                    SelectOption {
                        value: door43.clone(),
                        label: s.name.clone(),
                        meta: s.own.clone(),
                        code: Some(door43),
                        ..SelectOption::from(s.name.as_str())
                    }
                })
                .collect()
        })
    });
    let on_change = {
        let actions = actions.clone();
        Callback::new(move |e: web_sys::Event| {
            let query = event_target_value(&e);
            actions.patch_np(|np| np.lang_name = query.clone());
            let seq = asked.get() + 1;
            asked.set(seq);
            if js_trim(&query).is_empty() {
                found.set(Vec::new());
                return;
            }
            let (actions, asked) = (actions.clone(), asked.clone());
            spawn_local(async move {
                let answer = actions.languages(query).await;
                // Latest wins: a reply for an older value is dropped.
                if asked.get() != seq {
                    return;
                }
                found.try_set(answer.unwrap_or_default());
            });
        })
    };
    let on_choose = {
        let actions = actions.clone();
        Callback::new(move |option: SelectOption| {
            let chosen = found.with_untracked(|found| {
                found
                    .iter()
                    .find(|s| s.door43.as_ref().or(s.code.as_ref()) == Some(&option.value))
                    .cloned()
            });
            if let Some(chosen) = chosen {
                actions.patch_np(|np| np.choose(&chosen));
            }
        })
    };
    view! {
        <SuggestField
            id=id
            label=untrack(|| t(keys::new_bible::LANG_NAME, &[]))
            value=value
            placeholder=untrack(|| t(keys::new_bible::LANG_PLACEHOLDER, &[]))
            suggestions=suggestions
            on_change=on_change
            on_choose=on_choose
        />
    }
    .into_any()
}
