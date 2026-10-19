//! Add a book (M4, `tC4:src/views/modals/AddBook.jsx`), its frame and its first step: a blank
//! book, or books from USFM files. C6 draws the pick and USFM steps and adds the books.

use std::sync::Arc;

use leptos::prelude::*;

use crate::app::forms::{AddBookForm, AddBookStep};
use crate::app::web::{AppCtx, use_app};
use crate::ds::{Attrs, OptionCard};
use crate::i18n::{keys, t};

/// The dialog, while `s.modal` is AddBook.
#[component]
pub fn AddBook() -> impl IntoView {
    let Some(AppCtx { s, actions }) = use_app() else {
        return ().into_any();
    };
    let ab = s.ab.clone();
    let step = {
        let ab = ab.clone();
        Memo::new(move |_| ab.with(|ab| ab.as_ref().map_or(AddBookStep::Method, |ab| ab.step)))
    };
    let children: ChildrenFn = {
        let actions = actions.clone();
        Arc::new(move || {
            let (blank, usfm) = (actions.clone(), actions.clone());
            view! {
                {move || (step.get() == AddBookStep::Method).then(|| {
                    let (blank, usfm) = (blank.clone(), usfm.clone());
                    view! {
                        <div style="display: flex; flex-direction: column; gap: 10px; padding-bottom: 18px;">
                            <OptionCard
                                icon="+"
                                title=move || t(keys::add_book::BLANK_TITLE, &[])
                                description=move || t(keys::add_book::BLANK_DESC, &[])
                                trailing="→"
                                on_click=Callback::new(move |_| blank.ab_choose(AddBookStep::Pick))
                            />
                            <OptionCard
                                icon=move || t(keys::importer::kind::usfm::ICON, &[])
                                title=move || t(keys::add_book::USFM_TITLE, &[])
                                description=move || t(keys::add_book::USFM_DESC, &[])
                                trailing="→"
                                attrs=Attrs::from([("data-testid", "ab-usfm-option")])
                                on_click=Callback::new(move |_| usfm.ab_choose(AddBookStep::Usfm))
                            />
                        </div>
                    }
                })}
            }
            .into_any()
        })
    };
    let name = {
        let ab = ab.clone();
        move || {
            ab.with(|ab| {
                ab.as_ref()
                    .map(|ab: &AddBookForm| ab.project_name.clone())
                    .unwrap_or_default()
            })
        }
    };
    let subtitle = move || {
        let name = name.clone();
        view! {
            {move || t(keys::add_book::TO, &[])}" "
            <strong style="color: var(--uw-ocean);">{name}</strong>
        }
    };
    let close = actions.clone();
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some((move || move || t(keys::add_book::TITLE, &[])).into()),
        subtitle: Some(subtitle.into()),
        width: Some(600.into()),
        close_label: Some(Signal::derive(|| t(keys::new_bible::CLOSE, &[]))),
        on_close: Some(Callback::new(move |()| close.close_modal())),
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}
