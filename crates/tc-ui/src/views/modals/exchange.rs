//! Save work for a teammate and Receive work from a file (E2, STORAGE §10): rebuild-only
//! dialogs, drawn as the other Home dialogs are. The save says "You can remove the stick"
//! only once the file read back from the device.

use std::sync::Arc;

use leptos::prelude::*;

use crate::app::exchange::{ExportForm, ExportStep, ReceiveForm, ReceiveStep};
use crate::app::web::{AppCtx, use_app};
use crate::ds::{
    Attrs, Button, ButtonVariant, Callout, CalloutTone, Spinner, Text, TextField, TextRole,
};
use crate::i18n::{keys, t};

fn callout(testid: &'static str, tone: CalloutTone, text: String) -> impl IntoView {
    let role = match tone {
        CalloutTone::Warn => "alert",
        _ => "status",
    };
    view! {
        <Callout
            tone=tone
            style=crate::css! { "overflow-wrap": "anywhere" }
            attrs=Attrs::from([("role", role), ("data-testid", testid)])
        >
            {text}
        </Callout>
    }
}

fn working(testid: &'static str, line: String) -> impl IntoView {
    view! {
        <div data-testid=testid style="display: flex; align-items: center; gap: 10px;">
            <Spinner label=line.clone() />
            <Text role=TextRole::Body>{line}</Text>
        </div>
    }
}

/// Cancel while nothing runs, Close once the run ended; nothing while it runs (the dialog
/// cannot close meanwhile either).
fn leave_button(
    actions: crate::app::web::Actions,
    busy: Signal<bool>,
    ended: Signal<bool>,
    testid: &'static str,
) -> impl IntoView {
    view! {
        {move || (!busy.get()).then(|| {
            let actions = actions.clone();
            let key = if ended.get() { keys::common::CLOSE } else { keys::exchange::save::CANCEL };
            view! {
                <Button
                    variant=ButtonVariant::Secondary
                    attrs=Attrs::from([("data-testid", testid)])
                    on_click=Callback::new(move |_| actions.close_modal())
                >
                    {move || t(key, &[])}
                </Button>
            }
        })}
    }
}

/// The dialog, while `s.layers.dialog` is SaveForTeammate with its form.
#[component]
pub fn SaveForTeammate() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let ex = s.export_form();
    let form = Memo::new(move |_| ex.get());
    let busy = Signal::derive(move || form.get().is_some_and(|f| f.busy()));
    let done = Signal::derive(move || {
        form.get()
            .is_some_and(|f| matches!(f.step, ExportStep::Done(_)))
    });
    let footer: ChildrenFn = {
        let actions = actions.clone();
        Arc::new(move || {
            let actions = actions.clone();
            let can_save = Signal::derive(move || form.get().is_some_and(|f| f.can_save()));
            view! {
                {leave_button(actions.clone(), busy, done, "save-for-teammate-leave")}
                <div style="flex: 1;"></div>
                {move || (!done.get()).then(|| {
                    let actions = actions.clone();
                    view! {
                        <Button
                            variant=ButtonVariant::Primary
                            attrs=Attrs::from([("data-testid", "save-for-teammate-save")])
                            disabled=Signal::derive(move || !can_save.get())
                            on_click=Callback::new(move |_| actions.save_for_teammate())
                        >
                            {move || t(keys::exchange::save::BUTTON, &[])}
                        </Button>
                    }
                })}
            }
            .into_any()
        })
    };
    let children: ChildrenFn = {
        let actions = actions.clone();
        Arc::new(move || {
            let actions = actions.clone();
            view! {
                {move || form.get().map(|f: ExportForm| {
                    let actions = actions.clone();
                    let choosing = f.choosing();
                    let folder = f
                        .destination
                        .as_ref()
                        .map(|path| t(keys::exchange::save::FOLDER, &[("path", path)]))
                        .unwrap_or_default();
                    let working_line = f.working_line();
                    let stick = f.stick_sentence();
                    let failed = match &f.step {
                        ExportStep::Failed(text) => Some(text.clone()),
                        _ => None,
                    };
                    view! {
                        {choosing.then(|| view! {
                            <div style="display: flex; flex-direction: column; gap: 10px;">
                                <Text role=TextRole::Body attrs=Attrs::from([("data-testid", "save-for-teammate-folder")])>
                                    {folder}
                                </Text>
                                <Button
                                    variant=ButtonVariant::Outline
                                    attrs=Attrs::from([("data-testid", "save-for-teammate-choose")])
                                    on_click=Callback::new(move |_| actions.save_choose_folder())
                                >
                                    {move || t(keys::exchange::save::CHOOSE_FOLDER, &[])}
                                </Button>
                            </div>
                        })}
                        {working_line.map(|line| working("save-for-teammate-working", line))}
                        {stick.map(|text| callout("save-for-teammate-done", CalloutTone::Info, text))}
                        {failed.map(|text| callout("save-for-teammate-error", CalloutTone::Warn, text))}
                    }
                })}
            }
            .into_any()
        })
    };
    let close = actions.clone();
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some((move || move || t(keys::exchange::save::TITLE, &[])).into()),
        subtitle: Some(
            (move || {
                move || {
                    let name = form.get().map(|f| f.project.name).unwrap_or_default();
                    t(keys::exchange::save::SUBTITLE, &[("name", &name)])
                }
            })
            .into(),
        ),
        width: Some(560.into()),
        close_label: Some(Signal::derive(|| t(keys::common::CLOSE, &[]))),
        on_close: Some(Callback::new(move |()| close.close_modal())),
        closable: Signal::derive(move || !busy.get()),
        footer: Some(footer),
        attrs: Attrs::from([("data-testid", "save-for-teammate-modal")]),
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}

/// The dialog, while `s.layers.dialog` is ReceiveWork with its form.
#[component]
pub fn ReceiveWork() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let rx = s.receive_form();
    let form = Memo::new(move |_| rx.get());
    let input: NodeRef<leptos::html::Input> = NodeRef::new();
    let busy = Signal::derive(move || form.get().is_some_and(|f| f.busy()));
    let received = Signal::derive(move || {
        form.get()
            .is_some_and(|f| matches!(f.step, ReceiveStep::Received(_) | ReceiveStep::Added(_)))
    });
    let footer: ChildrenFn = {
        let actions = actions.clone();
        Arc::new(move || {
            let actions = actions.clone();
            let can_receive = Signal::derive(move || form.get().is_some_and(|f| f.can_receive()));
            view! {
                {leave_button(actions.clone(), busy, received, "receive-work-leave")}
                <div style="flex: 1;"></div>
                {move || (!received.get()).then(|| {
                    let actions = actions.clone();
                    view! {
                        <Button
                            variant=ButtonVariant::Primary
                            attrs=Attrs::from([("data-testid", "receive-work-receive")])
                            disabled=Signal::derive(move || !can_receive.get())
                            on_click=Callback::new(move |_| actions.receive_work())
                        >
                            {move || {
                                let adding = form.get().is_some_and(|f| matches!(f.step, ReceiveStep::ConfirmNew { .. }));
                                t(if adding { keys::exchange::receive::ADD_BUTTON } else { keys::exchange::receive::BUTTON }, &[])
                            }}
                        </Button>
                    }
                })}
            }
            .into_any()
        })
    };
    let children: ChildrenFn = {
        let actions = actions.clone();
        Arc::new(move || {
            let actions = actions.clone();
            let shell = actions.receive_picks_with_shell();
            let choose = {
                let actions = actions.clone();
                Callback::new(move |_: web_sys::MouseEvent| {
                    if shell {
                        actions.receive_choose_file();
                    } else if let Some(input) = input.get() {
                        input.click();
                    }
                })
            };
            let on_change = {
                let actions = actions.clone();
                move |_: web_sys::Event| {
                    if let Some(input) = input.get()
                        && let Some(file) = input.files().and_then(|files| files.get(0))
                    {
                        actions.receive_browser_file(file);
                        input.set_value("");
                    }
                }
            };
            view! {
                <input
                    node_ref=input
                    type="file"
                    accept=".zip"
                    data-testid="receive-work-input"
                    hidden
                    on:change=on_change
                />
                {move || form.get().map(|f: ReceiveForm| {
                    let project = f.project.as_ref().map(|p| p.name.clone()).unwrap_or_default();
                    let choosing = f.choosing();
                    let uploading = f.step == ReceiveStep::Uploading;
                    let identifying = f.step == ReceiveStep::Identifying;
                    let confirm = match &f.step {
                        ReceiveStep::Confirm(view) => Some(t(
                            keys::exchange::receive::CONFIRM,
                            &[("name", &view.name), ("project", &project)],
                        )),
                        ReceiveStep::ConfirmNew { view, .. } => Some(t(
                            keys::exchange::receive::NEW_PROJECT,
                            &[("name", &view.name)],
                        )),
                        _ => None,
                    };
                    let new_name = match &f.step {
                        ReceiveStep::ConfirmNew { name, .. } => Some((name.clone(), f.new_name_bad())),
                        _ => None,
                    };
                    let added = match &f.step {
                        ReceiveStep::Added(project) => Some(t(
                            keys::exchange::receive::ADDED,
                            &[("name", &project.as_str())],
                        )),
                        _ => None,
                    };
                    let on_name = {
                        let actions = actions.clone();
                        Callback::new(move |e: web_sys::Event| {
                            actions.type_new_project_name(event_target_value(&e));
                        })
                    };
                    let working_line = match &f.step {
                        ReceiveStep::Working(phase) => Some(t(
                            match phase {
                                Some(tc_api::jobs::ExchangePhase::TakeIn) => keys::exchange::receive::working::TAKE_IN,
                                _ => keys::exchange::receive::working::READ,
                            },
                            &[],
                        )),
                        _ => None,
                    };
                    let lines = f.received_lines();
                    let failed = match &f.step {
                        ReceiveStep::Failed(text) => Some(text.clone()),
                        _ => None,
                    };
                    view! {
                        {choosing.then(|| view! {
                            <Button
                                variant=ButtonVariant::Outline
                                attrs=Attrs::from([("data-testid", "receive-work-choose")])
                                on_click=choose
                            >
                                {move || t(keys::exchange::receive::CHOOSE_FILE, &[])}
                            </Button>
                        })}
                        {uploading.then(|| working("receive-work-uploading", t(keys::exchange::receive::UPLOADING, &[])))}
                        {identifying.then(|| working("receive-work-identifying", t(keys::exchange::receive::IDENTIFYING, &[])))}
                        {confirm.map(|text| view! {
                            <Text role=TextRole::Body attrs=Attrs::from([("data-testid", "receive-work-confirm")])>
                                {text}
                            </Text>
                        })}
                        {new_name.map(|(name, bad)| view! {
                            <TextField
                                id="receive-work-name"
                                label=Signal::derive(|| t(keys::exchange::receive::NEW_NAME, &[]))
                                hint=Signal::derive(|| t(keys::open_door43::NAME_BAD, &[]))
                                value=Signal::derive(move || name.clone())
                                invalid=bad
                                attrs=Attrs::from([("data-testid", "receive-work-name")])
                                on_change=on_name
                            />
                        })}
                        {added.map(|text| callout("receive-work-added", CalloutTone::Info, text))}
                        {working_line.map(|line| working("receive-work-working", line))}
                        {(!lines.is_empty()).then(|| view! {
                            <div data-testid="receive-work-received" style="display: flex; flex-direction: column; gap: 8px;">
                                {lines.into_iter().map(|line| view! {
                                    <Text role=TextRole::Body>{line}</Text>
                                }).collect_view()}
                            </div>
                        })}
                        {failed.map(|text| callout("receive-work-error", CalloutTone::Warn, text))}
                    }
                })}
            }
            .into_any()
        })
    };
    let close = actions.clone();
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some((move || move || t(keys::exchange::receive::TITLE, &[])).into()),
        subtitle: Some(
            (move || {
                move || match form.get().and_then(|f| f.project.map(|p| p.name)) {
                    Some(name) => t(keys::exchange::receive::SUBTITLE, &[("name", &name)]),
                    None => t(keys::add_project::RECEIVE_DESC, &[]),
                }
            })
            .into(),
        ),
        width: Some(560.into()),
        close_label: Some(Signal::derive(|| t(keys::common::CLOSE, &[]))),
        on_close: Some(Callback::new(move |()| close.close_modal())),
        closable: Signal::derive(move || !busy.get()),
        footer: Some(footer),
        attrs: Attrs::from([("data-testid", "receive-work-modal")]),
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}
