//! Import (M5, tC4 #361, `tC4:src/views/modals/Import.jsx`), its frame and its first step: the
//! kind of file. F1 draws the files, review, working and failed steps and runs the import.

use std::sync::Arc;

use leptos::prelude::*;

use crate::app::forms::{ImportForm, ImportKind, ImportStep};
use crate::app::web::{AppCtx, use_app};
use crate::ds::{
    Attrs, Button, ButtonVariant, Callout, CalloutTone, OptionCard, Overline, Text, TextRole,
    TextTone,
};
use crate::i18n::{keys, t, t_dyn};

/// `importer.kind.<id>.<part>`.
fn kind_text(kind: ImportKind, part: &str) -> String {
    t_dyn(&format!("importer.kind.{}.{part}", kind.id()), &[], None)
}

/// The dialog, while `s.modal` is Import.
#[component]
pub fn Import() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let step = {
        let im = s.im.clone();
        Memo::new(move |_| im.with(|im| im.as_ref().map_or(ImportStep::Kind, |im| im.step)))
    };
    let footer: ChildrenFn = {
        let actions = actions.clone();
        Arc::new(move || {
            let actions = actions.clone();
            (move || match step.get() {
                ImportStep::Kind => {
                    let actions = actions.clone();
                    Some(view! {
                        <Button
                            variant=ButtonVariant::Ghost
                            attrs=Attrs::from([("data-testid", "import-kind-back")])
                            on_click=Callback::new(move |_| actions.open_add_project())
                        >
                            {move || t(keys::importer::kind::BACK, &[])}
                        </Button>
                        <div style="flex: 1;"></div>
                    })
                }
                // F1's steps draw their own footers.
                ImportStep::Files
                | ImportStep::Review
                | ImportStep::Working
                | ImportStep::Failed => None,
            })
            .into_any()
        })
    };
    let children: ChildrenFn = {
        let (actions, im) = (actions.clone(), s.im.clone());
        Arc::new(move || {
            let kinds = move |actions: &crate::app::web::Actions| {
                ImportKind::ALL
                    .into_iter()
                    .map(|kind| {
                        let actions = actions.clone();
                        view! {
                            <OptionCard
                                icon=move || kind_text(kind, "icon")
                                title=move || kind_text(kind, "title")
                                description=move || kind_text(kind, "desc")
                                trailing="→"
                                attrs=Attrs::new().with("data-testid", format!("import-kind-{}", kind.id()))
                                on_click=Callback::new(move |_| actions.import_pick_kind(kind))
                            />
                        }
                    })
                    .collect_view()
            };
            let error = {
                let im = im.clone();
                move || {
                    im.with(|im| im.as_ref().and_then(|im: &ImportForm| im.error.clone())).map(|error| {
                        view! {
                            <Callout
                                tone=CalloutTone::Warn
                                style=crate::css! { "overflow-wrap": "anywhere" }
                                attrs=Attrs::from([("role", "alert"), ("data-testid", "import-error")])
                            >
                                {error}
                            </Callout>
                        }
                    })
                }
            };
            let actions = actions.clone();
            view! {
                {move || (step.get() == ImportStep::Kind).then(|| view! {
                    <div data-testid="import-kind" style="display: flex; flex-direction: column; gap: 10px;">
                        {kinds(&actions)}
                        <Text role=TextRole::Caption tone=TextTone::Muted style=crate::css! { "padding-top": 6 }>
                            {move || t(keys::importer::kind::NOTE, &[])}
                        </Text>
                    </div>
                })}
                {error}
            }
            .into_any()
        })
    };
    let title = move || {
        view! {
            <Overline tag="span" style=crate::css! { "display": "block", "margin-bottom": 8 }>
                {move || t(keys::importer::step::KIND, &[])}
            </Overline>
            {move || t(keys::importer::TITLE, &[])}
        }
    };
    let close = actions.clone();
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some(title.into()),
        subtitle: Some((move || move || t(keys::importer::subtitle::KIND, &[])).into()),
        width: Some(560.into()),
        close_label: Some(Signal::derive(|| t(keys::common::CLOSE, &[]))),
        on_close: Some(Callback::new(move |()| close.close_modal())),
        footer: Some(footer),
        attrs: Attrs::from([("data-testid", "import-modal")]),
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}
