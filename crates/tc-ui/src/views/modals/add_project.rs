//! "+ Add a project" (M1, tC4 #431, `tC4:src/views/modals/AddProject.jsx`): one chooser for the
//! ways to start. Each card opens its dialog in place of this one: New Bible, New Open Bible
//! Stories, Import's first step, or Open a project from Door43 (B9b).

use std::sync::Arc;

use leptos::prelude::*;

use crate::app::web::{Actions, AppCtx, use_app};
use crate::ds::{Attrs, Content, OptionCard};
use crate::i18n::{Key, keys, t};

/// The arrow down to a line of the Door43 card (the owner's mockup, artboard d). Built with the
/// SVG builders: `view!` would wrap each path in a group.
fn download_icon() -> AnyView {
    use leptos::svg;
    svg::svg()
        .add_any_attr(crate::ds::fixed(&[
            ("aria-hidden", "true"),
            ("width", "18"),
            ("height", "18"),
            ("viewBox", "0 0 24 24"),
            ("fill", "none"),
            ("stroke", "currentColor"),
            ("stroke-width", "2"),
            ("stroke-linecap", "round"),
            ("stroke-linejoin", "round"),
        ]))
        .child((
            svg::path().add_any_attr(crate::ds::fixed(&[("d", "M12 4v11")])),
            svg::path().add_any_attr(crate::ds::fixed(&[("d", "M7 10l5 5 5-5")])),
            svg::path().add_any_attr(crate::ds::fixed(&[("d", "M5 20h14")])),
        ))
        .into_any()
}

/// The dialog, while `s.modal` is AddProject.
#[component]
pub fn AddProject() -> impl IntoView {
    let Some(AppCtx { actions, .. }) = use_app() else {
        return ().into_any();
    };
    let children: ChildrenFn = {
        let actions = actions.clone();
        Arc::new(move || {
            type Pick = fn(&Actions);
            let cards: [(&str, Content, Key, Key, Pick); 5] = [
                (
                    "bible",
                    "+".into(),
                    keys::home::NEW_BIBLE,
                    keys::add_project::BIBLE_DESC,
                    Actions::open_new_project,
                ),
                (
                    "obs",
                    (move || t(keys::home::OBS_MARKER, &[])).into(),
                    keys::home::NEW_OBS,
                    keys::add_project::OBS_DESC,
                    Actions::open_new_obs,
                ),
                (
                    "import",
                    "↪".into(),
                    keys::importer::TITLE,
                    keys::add_project::IMPORT_DESC,
                    Actions::open_import,
                ),
                (
                    "door43",
                    (|| download_icon()).into(),
                    keys::add_project::DOOR43_TITLE,
                    keys::add_project::DOOR43_DESC,
                    Actions::open_door43_dialog,
                ),
                (
                    "receive",
                    "⇥".into(),
                    keys::add_project::RECEIVE_TITLE,
                    keys::add_project::RECEIVE_DESC,
                    Actions::open_receive_work_new,
                ),
            ];
            let cards = cards
                .into_iter()
                .map(|(id, icon, title, desc, pick)| {
                    let actions = actions.clone();
                    view! {
                        <OptionCard
                            icon=icon
                            title=move || t(title, &[])
                            description=move || t(desc, &[])
                            trailing="→"
                            attrs=Attrs::new().with("data-testid", format!("add-project-{id}"))
                            on_click=Callback::new(move |_| pick(&actions))
                        />
                    }
                })
                .collect_view();
            view! {
                <div style="display: flex; flex-direction: column; gap: 10px; padding-bottom: 18px;">
                    {cards}
                </div>
            }
            .into_any()
        })
    };
    let close = actions.clone();
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some((move || move || t(keys::add_project::TITLE, &[])).into()),
        subtitle: Some((move || move || t(keys::add_project::SUBTITLE, &[])).into()),
        close_label: Some(Signal::derive(|| t(keys::common::CLOSE, &[]))),
        on_close: Some(Callback::new(move |()| close.close_modal())),
        attrs: Attrs::from([("data-testid", "add-project-modal")]),
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}
