//! The Feedback dialog (M14, tC4 #378, `tC4:src/views/modals/Feedback.jsx`): a message to the
//! unfoldingWord help desk. "Ask for help" on a refusal banner opens it as a Bug Report with the
//! refusal filled in. The attachment shows here exactly as it is sent. Opening the dialog and
//! typing in it send nothing but the request for the attachment; Send asks "Turn on the
//! internet?" while the internet is off, and the desktop shell sends it. The report lives in
//! `s.fb` until the page goes; nothing about it is stored (A5 D11).

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::feedback::FeedbackCategory;
use tc_api::refusal::{FeedbackRefusal, Message};

use crate::app::feedback::{Attachment, FeedbackForm, FeedbackResult, can_send};
use crate::app::web::{AppCtx, use_app};
use crate::ds::{
    Attrs, Button, ButtonVariant, Callout, CalloutTone, Select, SelectOption, Text, TextArea,
    TextField, TextRole, TextTone,
};
use crate::i18n::{keys, t, t_dyn};

const CATEGORIES: [FeedbackCategory; 3] = [
    FeedbackCategory::General,
    FeedbackCategory::Content,
    FeedbackCategory::Bug,
];

fn category_value(category: FeedbackCategory) -> &'static str {
    category.subject_name()
}

fn note() -> String {
    crate::css! {
        "font-size": "var(--fs-caption-lg)",
        "color": "var(--text-secondary)",
        "line-height": "var(--lh-body)",
        "margin": 0,
    }
    .to_style()
}

fn reason_name(why: FeedbackRefusal) -> &'static str {
    match why {
        FeedbackRefusal::NoDesktop => "no-desktop",
        FeedbackRefusal::NotConfigured => "not-configured",
        FeedbackRefusal::Offline => "offline",
        FeedbackRefusal::Timeout => "timeout",
        FeedbackRefusal::Refused => "refused",
    }
}

/// The dialog. The app mounts it while `s.modal` is Feedback and a report is kept.
#[component]
pub fn Feedback() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let fb = s.fb.clone();
    let read = move |f: fn(&FeedbackForm) -> bool| {
        let fb = fb.clone();
        Signal::derive(move || fb.with(|fb| fb.as_ref().is_some_and(f)))
    };
    let busy = read(|fb| fb.sending);
    let sent = Memo::new({
        let fb = s.fb.clone();
        move |_| fb.with(|fb| fb.as_ref().map(|f| f.result) == Some(Some(FeedbackResult::Sent)))
    });
    let send_off = read(|fb| !can_send(fb));

    let footer: ChildrenFn = {
        let actions = actions.clone();
        Arc::new(move || {
            let actions = actions.clone();
            (move || {
                let (close, cancel, send) = (actions.clone(), actions.clone(), actions.clone());
                if sent.get() {
                    view! {
                        <Button
                            attrs=Attrs::from([("data-testid", "feedback-close")])
                            on_click=Callback::new(move |_| close.close_feedback())
                        >
                            {move || t(keys::feedback::CLOSE, &[])}
                        </Button>
                    }
                    .into_any()
                } else {
                    view! {
                        <Button
                            variant=ButtonVariant::Secondary
                            disabled=busy
                            attrs=Attrs::from([("data-testid", "feedback-cancel")])
                            on_click=Callback::new(move |_| cancel.close_feedback())
                        >
                            {move || t(keys::feedback::CANCEL, &[])}
                        </Button>
                        <Button
                            disabled=send_off
                            attrs=Attrs::from([("data-testid", "feedback-send")])
                            on_click=Callback::new(move |_| send.send_feedback())
                        >
                            {move || {
                                if busy.get() {
                                    t(keys::feedback::SENDING, &[])
                                } else {
                                    t(keys::feedback::SEND, &[])
                                }
                            }}
                        </Button>
                    }
                    .into_any()
                }
            })
            .into_any()
        })
    };

    let children: ChildrenFn = {
        let (actions, fb) = (actions.clone(), s.fb.clone());
        Arc::new(move || {
            let (actions, fb) = (actions.clone(), fb.clone());
            let column = "display: flex; flex-direction: column; gap: 16px;";
            view! {
                <div style=column>
                    {move || {
                        if sent.get() {
                            view! {
                                <Callout
                                    tone=CalloutTone::Success
                                    attrs=Attrs::from([("role", "status"), ("data-testid", "feedback-result"), ("data-result", "sent")])
                                >
                                    {move || t(keys::feedback::SENT, &[])}
                                </Callout>
                            }
                            .into_any()
                        } else {
                            form(&actions, &fb, busy).into_any()
                        }
                    }}
                </div>
            }
            .into_any()
        })
    };

    let close = actions.clone();
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some((move || move || t(keys::feedback::TITLE, &[])).into()),
        subtitle: Some((move || move || t(keys::feedback::SUBTITLE, &[])).into()),
        width: Some(600.into()),
        close_label: Some(Signal::derive(|| t(keys::feedback::CLOSE, &[]))),
        on_close: Some(Callback::new(move |()| close.close_feedback())),
        footer: Some(footer),
        attrs: Attrs::from([("data-testid", "feedback")]),
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}

/// The fields, the attachment and the "not sent" callout. Each field has its id, so its label
/// names it (tC4 #634).
fn form(
    actions: &crate::app::web::Actions,
    fb: &ArcRwSignal<Option<FeedbackForm>>,
    busy: Signal<bool>,
) -> impl IntoView + use<> {
    let text = |get: fn(&FeedbackForm) -> String| {
        let fb = fb.clone();
        Signal::derive(move || fb.with(|fb| fb.as_ref().map(get).unwrap_or_default()))
    };
    let patch = |set: fn(&mut FeedbackForm, String)| {
        let actions = actions.clone();
        Callback::new(move |e: web_sys::Event| {
            let value = event_target_value(&e);
            actions.patch_fb(|fb| set(fb, value));
        })
    };
    let category = {
        let fb = fb.clone();
        Signal::derive(move || {
            fb.with(|fb| fb.as_ref().map(|f| category_value(f.category).to_owned()))
        })
    };
    let on_category = {
        let actions = actions.clone();
        Callback::new(move |value: String| {
            if let Some(chosen) = CATEGORIES.into_iter().find(|c| category_value(*c) == value) {
                actions.patch_fb(|fb| fb.category = chosen);
            }
        })
    };
    // Built at mount from the shown language, as tC4 builds them at each render.
    let options: Vec<SelectOption> = CATEGORIES
        .iter()
        .map(|c| SelectOption {
            value: category_value(*c).to_owned(),
            label: untrack(|| t_dyn(c.key(), &[], None)),
            ..SelectOption::from(category_value(*c))
        })
        .collect();
    let attachment = {
        let fb = fb.clone();
        move || {
            fb.with(|fb| match fb.as_ref().map(|f| &f.attachment) {
                Some(Attachment::Ready(text)) => text.clone(),
                Some(Attachment::Failed(reason)) => {
                    t(keys::feedback::ATTACHMENT_FAILED, &[("reason", reason)])
                }
                Some(Attachment::Loading) | None => String::new(),
            })
        }
    };
    // A memo, so a keystroke, which changes the report but not its result, keeps the callout's
    // node as React keeps it.
    let not_sent = {
        let fb = fb.clone();
        Memo::new(move |_| {
            fb.with(|fb| match fb.as_ref().and_then(|f| f.result) {
                Some(FeedbackResult::NotSent(why)) => Some(why),
                Some(FeedbackResult::Sent) | None => None,
            })
        })
    };
    let result = move || {
        let why = not_sent.get()?;
        Some(view! {
            <Callout
                tone=CalloutTone::Warn
                attrs=Attrs::from([("role", "alert"), ("data-testid", "feedback-result"), ("data-result", reason_name(why))])
            >
                {move || format!("{} {}", t(keys::feedback::NOT_SENT, &[]), t_dyn(why.key(), &[], None))}
            </Callout>
        })
    };
    let pre = crate::css! {
        "margin": "6px 0 0",
        "max-height": 220,
        "overflow": "auto",
        "white-space": "pre-wrap",
        "overflow-wrap": "anywhere",
        "font-size": "var(--fs-caption)",
        "padding": 10,
        "background": "var(--surface-muted)",
        "border-radius": 6,
    };
    view! {
        <Select
            id="fb-category"
            label=move || t(keys::feedback::CATEGORY, &[])
            value=category
            disabled=busy
            on_change=on_category
            options=options
        />
        <TextArea
            id="fb-message"
            label=move || t(keys::feedback::MESSAGE, &[])
            rows=5
            value=text(|f| f.message.clone())
            disabled=busy
            attrs=Attrs::from([("data-testid", "feedback-message")])
            on_change=patch(|f, v| f.message = v)
        />
        <TextField
            id="fb-name"
            label=move || t(keys::feedback::NAME, &[])
            value=text(|f| f.name.clone())
            disabled=busy
            attrs=Attrs::from([("autocomplete", "name")])
            on_change=patch(|f, v| f.name = v)
        />
        <TextField
            id="fb-email"
            label=move || t(keys::feedback::EMAIL, &[])
            hint=move || t(keys::feedback::EMAIL_HINT, &[])
            value=text(|f| f.email.clone())
            disabled=busy
            attrs=Attrs::from([("type", "email"), ("autocomplete", "email")])
            on_change=patch(|f, v| f.email = v)
        />
        <div>
            <p style=note()>{move || t(keys::feedback::ATTACHMENT, &[])}</p>
            <pre data-testid="feedback-attachment" style=pre.to_style()>{attachment}</pre>
            // The attachment stays English for the help desk (A5 D8); this says so in the app's
            // language, so the box is not an unexplained English line (tC4 #731).
            <Text
                role=TextRole::Caption
                tone=TextTone::Muted
                style=crate::css! { "margin-top": 6 }
                attrs=Attrs::from([("data-testid", "feedback-attachment-english")])
            >
                {move || t(keys::feedback::ATTACHMENT_ENGLISH, &[])}
            </Text>
        </div>
        {result}
    }
}
