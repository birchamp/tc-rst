//! Open a project from Door43 (M6, unit B9b; `tC4:src/views/modals/OpenFromDoor43.jsx`, #677):
//! the fourth card of "Add a project" opens it. The repository is `owner/repository` or a link;
//! the project name is the repository name until the person types one. A name in use shows
//! under the name; any other refusal shows in a warn callout with its code, and no project was
//! added. The note names the server without tC4's "Only public repositories can be opened": a
//! private repository opens after sign-in here (Q3).

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::home::List;

use crate::app::open_door43::{NameError, OpenForm, OpenRefusal, RepoError, refusal_key};
use crate::app::web::{AppCtx, use_app, use_runtime};
use crate::ds::{
    Attrs, Button, ButtonVariant, Callout, CalloutTone, Field, Input, Size, Text, TextRole,
    TextTone,
};
use crate::i18n::{keys, t};

/// The dialog, while `s.modal` is Open from Door43 and its form is set.
#[component]
pub fn OpenFromDoor43() -> impl IntoView {
    let (Some(AppCtx { s, actions }), Some(runtime)) = (use_app(), use_runtime()) else {
        return ().into_any();
    };
    let od = s.od.clone();
    let list = runtime.cache().read(List {});
    // Every entry of the projects folder, for the name check: the cards', the unusable
    // folders' and the others'.
    let listing = Memo::new(move |_| {
        list.with(|l| {
            l.value()
                .map(|view| {
                    view.cards
                        .iter()
                        .map(|c| c.project.as_str().to_owned())
                        .chain(view.unusable.iter().map(|u| u.name.clone()))
                        .chain(view.others.iter().cloned())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
    });
    let account = s.server.account.clone();
    let host = Memo::new(move |_| account.with(|a| a.server.clone()));
    let busy = {
        let od = od.clone();
        Signal::derive(move || od.with(|od| od.as_ref().is_some_and(OpenForm::busy)))
    };
    let cant_open = {
        let od = od.clone();
        Signal::derive(move || {
            let host = host.get();
            listing.with(|names| {
                od.with(|od| {
                    od.as_ref()
                        .and_then(|od| od.ready(&host, names.iter().map(String::as_str)))
                        .is_none()
                })
            })
        })
    };

    let footer: ChildrenFn = {
        let actions = actions.clone();
        Arc::new(move || {
            let (cancel, open) = (actions.clone(), actions.clone());
            view! {
                <Button
                    variant=ButtonVariant::Secondary
                    disabled=busy
                    attrs=Attrs::from([("data-testid", "open-door43-cancel")])
                    on_click=Callback::new(move |_| cancel.close_modal())
                >
                    {move || t(keys::open_door43::CANCEL, &[])}
                </Button>
                <Button
                    disabled=cant_open
                    aria_busy=busy
                    attrs=Attrs::from([("data-testid", "open-door43-submit")])
                    on_click=Callback::new(move |_| open.open_door43_run())
                >
                    {move || t(if busy.get() { keys::open_door43::OPENING } else { keys::open_door43::OPEN }, &[])}
                </Button>
            }
            .into_any()
        })
    };

    let children: ChildrenFn = {
        let (actions, od) = (actions.clone(), od.clone());
        Arc::new(move || {
            let (actions, od) = (actions.clone(), od.clone());
            let text = |get: fn(&OpenForm) -> String| -> Signal<String> {
                let od = od.clone();
                Signal::from(Memo::new(move |_| {
                    od.with(|od| od.as_ref().map(get).unwrap_or_default())
                }))
            };
            let repo = text(|od| od.repo.clone());
            let name = text(|od| od.name.clone());
            let repo_error: Signal<Option<String>> = {
                let od = od.clone();
                Signal::from(Memo::new(move |_| {
                    let host = host.get();
                    od.with(|od| od.as_ref().and_then(|od| od.repo_error(&host)))
                        .map(|RepoError::Bad| t(keys::open_door43::REPO_BAD, &[("host", &host)]))
                }))
            };
            let name_error: Signal<Option<String>> = {
                let od = od.clone();
                Signal::from(Memo::new(move |_| {
                    listing.with(|names| {
                        od.with(|od| {
                            let od = od.as_ref()?;
                            let error = od.name_error(names.iter().map(String::as_str))?;
                            Some(match error {
                                NameError::Bad => t(keys::open_door43::NAME_BAD, &[]),
                                NameError::InUse => {
                                    t(keys::open_door43::NAME_IN_USE, &[("name", &od.name.trim())])
                                }
                            })
                        })
                    })
                }))
            };
            let on_repo = {
                let actions = actions.clone();
                Callback::new(move |e: web_sys::Event| {
                    let value = event_target_value(&e);
                    let host = host.get_untracked();
                    actions.edit_od(|od| od.type_repo(value, &host));
                })
            };
            let on_name = {
                let actions = actions.clone();
                Callback::new(move |e: web_sys::Event| {
                    let value = event_target_value(&e);
                    actions.edit_od(|od| od.type_name(value));
                })
            };
            let refusal = {
                let od = od.clone();
                Memo::new(move |_| od.with(|od| od.as_ref().and_then(OpenForm::refusal).cloned()))
            };
            let below = {
                let actions = actions.clone();
                move || {
                    match refusal.get() {
                    Some(refusal) => refused(&actions, refusal).into_any(),
                    None => view! {
                        <Text role=TextRole::Caption tone=TextTone::Muted>
                            {move || t(keys::open_door43::CHECKS, &[("sync", &t(keys::home::SYNC, &[]))])}
                        </Text>
                    }
                    .into_any(),
                }
                }
            };
            view! {
                <div style="display: flex; flex-direction: column; gap: 16px; padding-bottom: 6px;">
                    <Callout tone=CalloutTone::Info>
                        {move || t(keys::open_door43::WHERE_ANY, &[("host", &host.get())])}
                    </Callout>
                    <Field
                        label=Signal::derive(|| t(keys::open_door43::REPO, &[]))
                        hint=Signal::derive(|| t(keys::open_door43::REPO_HINT, &[]))
                        live_error=repo_error
                    >
                        <Input
                            value=repo
                            invalid=Signal::derive(move || repo_error.with(Option::is_some))
                            disabled=busy
                            attrs=Attrs::from([("data-testid", "open-door43-repo")])
                            on_change=on_repo
                        />
                    </Field>
                    <Field
                        label=Signal::derive(|| t(keys::open_door43::NAME, &[]))
                        hint=Signal::derive(|| t(keys::open_door43::NAME_HINT, &[]))
                        live_error=name_error
                        attrs=Attrs::from([("data-testid", "open-door43-name-field")])
                    >
                        <Input
                            value=name
                            invalid=Signal::derive(move || name_error.with(Option::is_some))
                            disabled=busy
                            attrs=Attrs::from([("data-testid", "open-door43-name")])
                            on_change=on_name
                        />
                    </Field>
                    {below}
                </div>
            }
            .into_any()
        })
    };

    let close = actions.clone();
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some((move || move || t(keys::open_door43::TITLE, &[])).into()),
        subtitle: Some((move || move || t(keys::open_door43::SUBTITLE, &[])).into()),
        width: Some(520.into()),
        close_label: Some(Signal::derive(|| t(keys::common::CLOSE, &[]))),
        on_close: Some(Callback::new(move |()| close.close_modal())),
        // Cancel and the close are off while the open runs, so a refusal always has its dialog.
        closable: Signal::derive(move || !busy.get()),
        footer: Some(footer),
        attrs: Attrs::from([("data-testid", "open-door43-modal")]),
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}

/// tC4's artboard g: the refusal, its code, "No project was added", and Ask for help.
fn refused(actions: &crate::app::web::Actions, refusal: OpenRefusal) -> impl IntoView + use<> {
    let key = refusal_key(refusal.code.as_deref());
    let code = refusal.code.clone().unwrap_or_default();
    let help = actions.clone();
    let on_help = {
        let refusal = refusal.clone();
        Callback::new(move |_| {
            let text = t(key, &[]);
            let text = format!("{text}\n\n{}", refusal.reason).trim().to_owned();
            help.ask_for_help(crate::app::feedback::HelpRequest {
                code: refusal.code.clone().unwrap_or_default(),
                text,
                report: None,
            });
        })
    };
    let shown_code = refusal.code.clone();
    view! {
        <Callout
            tone=CalloutTone::Warn
            attrs=Attrs::from([("data-testid", "open-door43-refused"), ("data-code", code.as_str()), ("role", "alert")])
        >
            <div style="display: flex; flex-direction: column; gap: 6px;">
                <Text role=TextRole::Strong>{move || t(keys::open_door43::REFUSED_TITLE, &[])}</Text>
                <span>{move || t(key, &[])}</span>
                <span>
                    {shown_code.map(|code| view! { <code>{code}</code> " · " })}
                    {move || t(keys::open_door43::NOTHING_WRITTEN, &[])}
                </span>
                <div>
                    <Button
                        size=Size::Sm
                        variant=ButtonVariant::Outline
                        attrs=Attrs::from([("data-testid", "open-door43-help")])
                        on_click=on_help
                    >
                        {move || t(keys::feedback::ASK, &[])}
                    </Button>
                </div>
            </div>
        </Callout>
    }
}
