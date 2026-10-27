//! Import (M5, tC4 #361, `tC4:src/views/modals/Import.jsx`): the kind of file, the files, the
//! review of what they hold, the run and its failure. This release imports the tC3 kind; the
//! USFM and Scripture Burrito cards stay on the kind step (rulings/2026-10-24-H3-H4-H7).
//!
//! Declared difference from tC4 4.0 (APP-DESIGN F1 row): the run shows its progress and has
//! Cancel.

use std::sync::Arc;

use leptos::prelude::*;
use tc_api::home::List;
use tc_api::import::{FoundVersion, ImportReview, VersionSlot};
use tc_api::refusal::ImportRefusal;

use crate::app::forms::{
    ImportEntry, ImportFailure, ImportForm, ImportKind, ImportState, ImportStep, ImportUpload,
    Reviewed, VersionLookup,
};
use crate::app::web::{Actions, AppCtx, use_app, use_runtime};
use crate::ds::{
    Attrs, Button, ButtonVariant, Callout, CalloutTone, Corner, DropZone, Edge, Field, Fill,
    IconButton, IconButtonVariant, Input, Inset, KeyValue, KeyValueGrid, OptionCard, Overline,
    ProgressBar, Select, SelectOption, Size, Spinner, Status, StatusDot, Surface, Text, TextRole,
    TextTone, Tone,
};
use crate::i18n::{Key, keys, t, t_dyn};

/// `importer.kind.<id>.<part>`.
fn kind_text(kind: ImportKind, part: &str) -> String {
    t_dyn(&format!("importer.kind.{}.{part}", kind.id()), &[], None)
}

/// tC4's `size`: "1.2 MB", else "40 KB".
#[expect(
    clippy::cast_precision_loss,
    reason = "a file's size is shown to one decimal place"
)]
fn size(bytes: u64) -> String {
    let bytes = bytes as f64;
    if bytes >= 1e6 {
        format!("{:.1} MB", bytes / 1e6)
    } else {
        format!("{} KB", (bytes / 1e3).round().max(1.0))
    }
}

/// A part of the form, read reactively.
fn read<T: Clone + PartialEq + Send + Sync + 'static>(
    im: &Signal<Option<ImportForm>>,
    get: impl Fn(&ImportForm) -> T + Send + Sync + 'static,
    none: T,
) -> Memo<T> {
    let im = *im;
    Memo::new(move |_| im.with(|im| im.as_ref().map_or_else(|| none.clone(), &get)))
}

/// The step overline, the title and the subtitle of each step (tC4 `STEP_TITLES`).
fn step_keys(step: ImportStep) -> (Key, Option<Key>, Key) {
    use keys::importer as k;
    match step {
        ImportStep::Kind => (k::step::KIND, Some(k::TITLE), k::subtitle::KIND),
        ImportStep::Files => (k::step::FILES, None, k::subtitle::FILES),
        ImportStep::Review => (k::step::REVIEW, Some(k::review::TITLE), k::subtitle::REVIEW),
        ImportStep::Working => (
            k::step::WORKING,
            Some(k::working::TITLE),
            k::subtitle::WORKING,
        ),
        ImportStep::Failed => (
            k::step::WORKING,
            Some(k::failed::HEADING),
            k::subtitle::FAILED,
        ),
    }
}

/// The dialog, while `s.modal` is Import.
#[component]
pub fn Import() -> impl IntoView {
    let (Some(AppCtx { s, actions }), Some(runtime)) = (use_app(), use_runtime()) else {
        return ().into_any();
    };
    let im = s.import_form();
    let list = runtime.cache().read(List {});
    // Every entry of the projects folder, for the name check (tC4 `nameClash`).
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
    let step = read(&im, ImportForm::step, ImportStep::Kind);
    // The Modal reads its title and width once: a step draws it again.
    (move || {
        modal(
            &actions,
            &im,
            listing,
            step,
            step.get() == ImportStep::Review,
        )
    })
    .into_any()
}

fn modal(
    actions: &Actions,
    im: &Signal<Option<ImportForm>>,
    listing: Memo<Vec<String>>,
    step: Memo<ImportStep>,
    wide: bool,
) -> AnyView {
    let kind = read(im, |im| im.kind, None);
    let damaged = read(
        im,
        |im| matches!(im.reviewed, Some(Reviewed::Damaged(_))),
        false,
    );
    let title = move || {
        let (overline, title, _) = step_keys(step.get());
        view! {
            <Overline tag="span" style=crate::css! { "display": "block", "margin-bottom": 8 }>
                {t(overline, &[])}
            </Overline>
            {match title {
                Some(key) => t(key, &[]),
                None => kind.get().map(|kind| kind_text(kind, "title")).unwrap_or_default(),
            }}
        }
    };
    let subtitle = move || {
        move || {
            if step.get() == ImportStep::Review && damaged.get() {
                t(keys::importer::subtitle::DAMAGED, &[])
            } else {
                t(step_keys(step.get()).2, &[])
            }
        }
    };
    let footer: ChildrenFn = {
        let (actions, im) = (actions.clone(), *im);
        Arc::new(move || {
            let (actions, im) = (actions.clone(), im);
            (move || footer(&actions, &im, listing, step.get())).into_any()
        })
    };
    let children: ChildrenFn = {
        let (actions, im) = (actions.clone(), *im);
        Arc::new(move || {
            let (actions, im) = (actions.clone(), im);
            let body = {
                move || match step.get() {
                    ImportStep::Kind => kind_step(&actions).into_any(),
                    ImportStep::Files => files_step(&actions, &im).into_any(),
                    ImportStep::Review => review_step(&actions, &im, listing).into_any(),
                    ImportStep::Working => working_step(&im).into_any(),
                    ImportStep::Failed => failed_step(&im).into_any(),
                }
            };
            let problem = read(
                &im,
                |im| match &im.state {
                    ImportState::Problem(problem) => Some(problem.clone()),
                    _ => None,
                },
                None,
            );
            view! {
                {body}
                {move || problem.get().map(|problem| view! {
                    <Callout
                        tone=CalloutTone::Warn
                        style=crate::css! { "overflow-wrap": "anywhere" }
                        attrs=Attrs::from([("role", "alert"), ("data-testid", "import-error")])
                    >
                        {problem}
                    </Callout>
                })}
            }
            .into_any()
        })
    };
    let close = actions.clone();
    crate::ds::Modal(crate::ds::ModalProps {
        title: Some(title.into()),
        subtitle: Some(subtitle.into()),
        width: Some((if wide { 760 } else { 560 }).into()),
        close_label: Some(Signal::derive(|| t(keys::common::CLOSE, &[]))),
        on_close: Some(Callback::new(move |()| close.close_modal())),
        closable: Signal::derive(move || step.get() != ImportStep::Working),
        footer: Some(footer),
        attrs: Attrs::from([("data-testid", "import-modal")]),
        children: Some(children),
        ..Default::default()
    })
    .into_any()
}

fn footer(
    actions: &Actions,
    im: &Signal<Option<ImportForm>>,
    listing: Memo<Vec<String>>,
    step: ImportStep,
) -> AnyView {
    let spacer = || view! { <div style="flex: 1;"></div> };
    let cancel = {
        let actions = actions.clone();
        move || {
            let actions = actions.clone();
            view! {
                <Button variant=ButtonVariant::Secondary on_click=Callback::new(move |_| actions.close_modal())>
                    {t(keys::new_bible::CANCEL, &[])}
                </Button>
            }
        }
    };
    let ghost = |label: Key, run: Callback<web_sys::MouseEvent>, id: &'static str| {
        view! {
            <Button variant=ButtonVariant::Ghost attrs=Attrs::from([("data-testid", id)]) on_click=run>
                {t(label, &[])}
            </Button>
        }
    };
    let actions = actions.clone();
    match step {
        ImportStep::Kind => {
            let back = Callback::new(move |_| actions.open_add_project());
            view! { {ghost(keys::importer::kind::BACK, back, "import-kind-back")} {spacer()} }
                .into_any()
        }
        ImportStep::Files => {
            let off = read(im, |im| !im.can_review(), true);
            let (back, review) = (actions.clone(), actions.clone());
            view! {
                {ghost(keys::importer::files::BACK, Callback::new(move |_| back.import_back_to_kind()), "import-files-back")}
                {spacer()}
                {cancel()}
                <Button
                    disabled=Signal::from(off)
                    attrs=Attrs::from([("data-testid", "import-to-review")])
                    on_click=Callback::new(move |_| review.import_review())
                >
                    {t(keys::importer::files::REVIEW, &[])}
                </Button>
            }
            .into_any()
        }
        ImportStep::Review => {
            let off = {
                let im = *im;
                Signal::derive(move || {
                    listing.with(|names| {
                        im.with(|im| {
                            !im.as_ref()
                                .is_some_and(|im| im.can_import(names.iter().map(String::as_str)))
                        })
                    })
                })
            };
            let (back, run) = (actions.clone(), actions.clone());
            view! {
                {ghost(keys::importer::review::BACK, Callback::new(move |_| back.import_back_to_files()), "import-review-back")}
                {spacer()}
                {cancel()}
                <Button
                    disabled=off
                    attrs=Attrs::from([("data-testid", "import-run")])
                    on_click=Callback::new(move |_| run.import_run(listing.get_untracked()))
                >
                    {t(keys::importer::review::RUN, &[])}
                </Button>
            }
            .into_any()
        }
        ImportStep::Working => view! {
            {spacer()}
            <Button
                variant=ButtonVariant::Secondary
                attrs=Attrs::from([("data-testid", "import-cancel")])
                on_click=Callback::new(move |_| actions.import_cancel())
            >
                {t(keys::new_bible::CANCEL, &[])}
            </Button>
        }
        .into_any(),
        ImportStep::Failed => {
            let (close, retry) = (actions.clone(), actions.clone());
            view! {
                <Button variant=ButtonVariant::Secondary on_click=Callback::new(move |_| close.close_modal())>
                    {t(keys::common::CLOSE, &[])}
                </Button>
                <Button
                    attrs=Attrs::from([("data-testid", "import-retry")])
                    on_click=Callback::new(move |_| retry.import_run(listing.get_untracked()))
                >
                    {t(keys::importer::failed::RETRY, &[])}
                </Button>
            }
            .into_any()
        }
    }
}

fn kind_step(actions: &Actions) -> impl IntoView + use<> {
    let kinds = ImportKind::ALL
        .into_iter()
        .map(|kind| {
            let actions = actions.clone();
            let live = kind.has_importer();
            view! {
                <OptionCard
                    icon=move || kind_text(kind, "icon")
                    title=move || kind_text(kind, "title")
                    description=move || {
                        if live {
                            kind_text(kind, "desc")
                        } else {
                            t(keys::importer::kind::LATER, &[])
                        }
                    }
                    trailing="→"
                    disabled=!live
                    style=if live {
                        crate::css! {}
                    } else {
                        crate::css! { "opacity": 0.5, "cursor": "default" }
                    }
                    attrs=Attrs::new().with("data-testid", format!("import-kind-{}", kind.id()))
                    on_click=Callback::new(move |_| actions.import_pick_kind(kind))
                />
            }
        })
        .collect_view();
    view! {
        <div data-testid="import-kind" style="display: flex; flex-direction: column; gap: 10px;">
            {kinds}
            <Text role=TextRole::Caption tone=TextTone::Muted style=crate::css! { "padding-top": 6 }>
                {move || t(keys::importer::kind::NOTE, &[])}
            </Text>
        </div>
    }
}

/// The files step (tC4 `FilesStep`): the drop zone, then the chosen files.
fn files_step(actions: &Actions, im: &Signal<Option<ImportForm>>) -> impl IntoView + use<> {
    let kind = im
        .with_untracked(|im| im.as_ref().and_then(|im| im.kind))
        .unwrap_or(ImportKind::Tc3);
    let input = NodeRef::<leptos::html::Input>::new();
    let add = {
        let actions = actions.clone();
        move |list: Option<web_sys::FileList>| {
            if let Some(list) = list.filter(|list| list.length() > 0) {
                actions.import_add_files(list);
            }
        }
    };
    let on_change = {
        let add = add.clone();
        move |_| {
            if let Some(input) = input.get() {
                add(input.files());
                input.set_value("");
            }
        }
    };
    let on_drop = move |event: web_sys::DragEvent| {
        event.prevent_default();
        add(event.data_transfer().and_then(|transfer| transfer.files()));
    };
    let click = Callback::new(move |_: web_sys::Event| {
        if let Some(input) = input.get() {
            input.click();
        }
    });
    let files = read(im, |im| im.files.clone(), Vec::new());
    let list = {
        let actions = actions.clone();
        move || {
            let files = files.get();
            (!files.is_empty()).then(|| {
                let count = if files.len() == 1 {
                    t(keys::importer::files::ONE, &[])
                } else {
                    t(keys::importer::files::MANY, &[("n", &files.len())])
                };
                let rows = files
                    .into_iter()
                    .map(|entry| file_row(&actions, entry))
                    .collect_view();
                view! {
                    <div style="display: flex; flex-direction: column; gap: 8px;">
                        <Overline tag="span">{count}</Overline>
                        {rows}
                    </div>
                }
            })
        }
    };
    view! {
        <div
            data-testid="import-files"
            style="display: flex; flex-direction: column; gap: 14px;"
            on:dragover=move |event: web_sys::DragEvent| event.prevent_default()
            on:drop=on_drop
        >
            <input node_ref=input type="file" multiple data-testid="import-file-input" hidden on:change=on_change />
            <DropZone
                title=kind_text(kind, "drop")
                hint=kind_text(kind, "hint")
                attrs=Attrs::from([("data-testid", "import-drop")])
                on_click=click
            />
            {list}
        </div>
    }
}

fn file_row(actions: &Actions, entry: ImportEntry) -> impl IntoView + use<> {
    let actions = actions.clone();
    let id = entry.id;
    let note = match &entry.upload {
        ImportUpload::Sent(_) => None,
        ImportUpload::Sending => Some((t(keys::add_book::USFM_READING, &[]), TextTone::Muted)),
        ImportUpload::Failed(why) => Some((why.clone(), TextTone::Default)),
    };
    view! {
        <Surface fill=Fill::Card border=Edge::Line radius=Corner::Md pad="10px 10px 10px 14px" attrs=Attrs::from([("data-testid", "import-file")])>
            <div style="display: flex; align-items: center; gap: 12px;">
                <div style="flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;">
                    <div style="display: flex; align-items: baseline; gap: 10px; min-width: 0;">
                        <Text role=TextRole::Strong truncate=true>{entry.name.clone()}</Text>
                        <Text role=TextRole::Meta tone=TextTone::Muted>{size(entry.bytes)}</Text>
                    </div>
                    {note.map(|(words, tone)| view! { <Text role=TextRole::Caption tone=tone>{words}</Text> })}
                </div>
                <IconButton
                    variant=IconButtonVariant::Plain
                    title=t(keys::importer::files::REMOVE, &[])
                    on_click=Callback::new(move |_| actions.import_remove_file(id))
                >
                    "✕"
                </IconButton>
            </div>
        </Surface>
    }
}

/// tC4's damaged panel and failed step: the sentence, then the code and the detail.
fn refusal_panel(
    failure: &ImportFailure,
    test_id: &'static str,
    title: Key,
    below: String,
) -> impl IntoView + use<> {
    let code = failure.code.clone().unwrap_or_default();
    let sentence = failure.sentence.clone();
    view! {
        <Surface
            tone=Tone::Invalid
            fill=Fill::Soft
            border=Edge::Tone
            radius=Corner::Lg
            pad=Inset::Md
            attrs=Attrs::from([("role", "alert"), ("data-testid", test_id), ("data-code", code.as_str())])
        >
            <div style="display: flex; flex-direction: column; gap: 6px;">
                <Text role=TextRole::Strong tone=TextTone::Tone>{t(title, &[])}</Text>
                <Text role=TextRole::Ui>{sentence}</Text>
                <Text role=TextRole::Meta tone=TextTone::Muted style=crate::css! { "overflow-wrap": "anywhere" }>{below}</Text>
            </div>
        </Surface>
    }
}

fn review_step(
    actions: &Actions,
    im: &Signal<Option<ImportForm>>,
    listing: Memo<Vec<String>>,
) -> AnyView {
    let reviewed = im.with_untracked(|im| im.as_ref().and_then(|im| im.reviewed.clone()));
    match reviewed {
        Some(Reviewed::Found(review)) => found(actions, im, listing, *review).into_any(),
        Some(Reviewed::Damaged(failure)) => {
            let code = failure
                .code
                .clone()
                .unwrap_or_else(|| "import.damaged".to_owned());
            let below = t(keys::importer::review::DAMAGED_CODE, &[("code", &code)]);
            view! {
                <div data-testid="import-review" style="display: flex; flex-direction: column; gap: 16px;">
                    {refusal_panel(&failure, "import-damaged", keys::importer::review::DAMAGED_TITLE, below)}
                </div>
            }
            .into_any()
        }
        None => ().into_any(),
    }
}

/// A row of the checks column.
fn check(id: &'static str, label: Key, status: Status, text: String) -> impl IntoView + use<> {
    let status_name = if status == Status::Warn {
        "warn"
    } else {
        "valid"
    };
    view! {
        <div
            data-testid=format!("import-check-{id}")
            data-status=status_name
            style="display: flex; gap: 10px; align-items: baseline;"
        >
            <StatusDot status=status size=8.0 />
            <div style="flex: 1; display: flex; flex-direction: column; gap: 3px;">
                <Text role=TextRole::Strong>{t(label, &[])}</Text>
                <Text role=TextRole::Caption>{text}</Text>
            </div>
        </div>
    }
}

/// tC4 `slotName`.
fn slot_name(slot: VersionSlot) -> String {
    t(
        match slot {
            VersionSlot::TranslationNotes => keys::importer::slot::TRANSLATION_NOTES,
            VersionSlot::TranslationWords | VersionSlot::TranslationWordsLinks => {
                keys::importer::slot::TRANSLATION_WORDS
            }
            VersionSlot::OriginalNewTestament | VersionSlot::OriginalOldTestament => {
                keys::importer::slot::ORIGINAL_LANGUAGE
            }
        },
        &[],
    )
}

fn slot_list(slots: &[VersionSlot]) -> String {
    let mut names: Vec<String> = Vec::new();
    for name in slots.iter().map(|s| slot_name(*s)) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names.join(", ")
}

/// "unfoldingWord/en_tn v87": owner, repository and tag, since two owners can publish a
/// repository of the same name.
fn found_list(found: &[FoundVersion]) -> String {
    found
        .iter()
        .map(|v| {
            let repo = v.repo_path.split('/').skip(1).collect::<Vec<_>>().join("/");
            format!("{repo} {}", v.tag)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The Resource versions row (tC4 `ResourcesCheck`, D82).
fn resources_check(
    actions: &Actions,
    im: &Signal<Option<ImportForm>>,
    slots: Vec<VersionSlot>,
) -> impl IntoView + use<> {
    use keys::importer::review as k;
    let versions = read(im, |im| im.versions.clone(), VersionLookup::NotLookedUp);
    let text = move || {
        let found_words = |found: &[FoundVersion]| {
            (!found.is_empty()).then(|| t(k::RESOURCES_FOUND, &[("list", &found_list(found))]))
        };
        let parts: Vec<Option<String>> = match versions.get() {
            VersionLookup::NotLookedUp => {
                vec![Some(t(
                    k::RESOURCES_OFFLINE,
                    &[("list", &slot_list(&slots))],
                ))]
            }
            VersionLookup::Looking => vec![Some(t(k::RESOURCES_LOOKING, &[]))],
            VersionLookup::Found(view) => vec![
                found_words(&view.found),
                (!view.unresolved.is_empty()).then(|| {
                    t(
                        k::RESOURCES_MISSING,
                        &[("list", &slot_list(&view.unresolved))],
                    )
                }),
            ],
            VersionLookup::Installed(view, carry) => vec![
                found_words(&view.found),
                Some(t(
                    k::RESOURCES_INSTALLED,
                    &[("list", &slot_list(&view.unresolved))],
                )),
                // Import waits for the count, so the row says what it waits for.
                Some(carry.map_or_else(
                    || t(k::DECISIONS_COUNTING, &[]),
                    |c| {
                        let carried = if c.carried == 1 {
                            k::DECISIONS_CARRIED_ONE
                        } else {
                            k::DECISIONS_CARRIED
                        };
                        let invalidated = if c.invalidated == 1 {
                            k::DECISIONS_INVALIDATED_ONE
                        } else {
                            k::DECISIONS_INVALIDATED
                        };
                        format!(
                            "{} {}",
                            t(carried, &[("n", &c.carried)]),
                            t(invalidated, &[("n", &c.invalidated)])
                        )
                    },
                )),
            ],
        };
        parts.into_iter().flatten().collect::<Vec<_>>().join(" ")
    };
    let buttons = {
        let actions = actions.clone();
        move || {
            let shown = versions.get();
            let look = shown == VersionLookup::NotLookedUp;
            let install =
                look || matches!(&shown, VersionLookup::Found(v) if !v.unresolved.is_empty());
            let (online, installed) = (actions.clone(), actions.clone());
            install.then(|| {
                view! {
                    <div style="display: flex; gap: 8px;">
                        {look.then(|| view! {
                            <Button
                                size=Size::Sm
                                variant=ButtonVariant::Secondary
                                attrs=Attrs::from([("data-testid", "import-allow-internet")])
                                on_click=Callback::new(move |_| online.import_look_up())
                            >
                                {t(k::GO_ONLINE, &[])}
                            </Button>
                        })}
                        <Button
                            size=Size::Sm
                            variant=ButtonVariant::Secondary
                            attrs=Attrs::from([("data-testid", "import-use-installed")])
                            on_click=Callback::new(move |_| installed.import_use_installed())
                        >
                            {t(k::USE_INSTALLED, &[])}
                        </Button>
                    </div>
                }
            })
        }
    };
    view! {
        <div
            style="display: flex; gap: 10px; align-items: baseline;"
            data-testid="import-resources"
            data-state=move || versions.with(VersionLookup::data_state)
        >
            <StatusDot status=Signal::derive(move || if versions.with(VersionLookup::open) { Status::Warn } else { Status::Valid }) size=8.0 />
            <div style="flex: 1; display: flex; flex-direction: column; gap: 6px;">
                <Text role=TextRole::Strong>{t(k::RESOURCES, &[])}</Text>
                <Text role=TextRole::Caption>{text}</Text>
                {buttons}
            </div>
        </div>
    }
}

/// The license check: the one license the files name, or that they differ (tC4 `license`).
fn license_check(review: &ImportReview) -> impl IntoView + use<> {
    let licenses = &review.found.licenses;
    let (status, text) = match licenses.as_slice() {
        [] | [_] if licenses.iter().all(String::is_empty) => {
            (Status::Warn, t(keys::importer::review::LICENSE_NONE, &[]))
        }
        [one] => (Status::Valid, one.clone()),
        many => {
            let named: Vec<String> = many
                .iter()
                .map(|l| {
                    if l.is_empty() {
                        t(keys::importer::tc3::LICENSE_NONE, &[])
                    } else {
                        l.clone()
                    }
                })
                .collect();
            (
                Status::Warn,
                t(
                    keys::importer::tc3::LICENSES,
                    &[("licenses", &named.join(" · "))],
                ),
            )
        }
    };
    check("license", keys::importer::review::LICENSE, status, text)
}

/// The license picker where the files differ (tC4 `import-license`). A value is `l:` and the
/// license, so "No license" (`l:`) differs from "Choose a license" (``).
fn license_pick(
    actions: &Actions,
    im: &Signal<Option<ImportForm>>,
    licenses: &[String],
) -> Option<impl IntoView + use<>> {
    use keys::importer::review as k;
    if licenses.len() < 2 {
        return None;
    }
    let none = SelectOption {
        label: t(k::LICENSE_PICK_NONE, &[]),
        ..SelectOption::from("")
    };
    let options: Vec<SelectOption> = std::iter::once(none)
        .chain(licenses.iter().map(|l| SelectOption {
            label: if l.is_empty() {
                t(k::LICENSE_PICK_EMPTY, &[])
            } else {
                l.clone()
            },
            ..SelectOption::from(format!("l:{l}").as_str())
        }))
        .collect();
    let value = read(
        im,
        |im| {
            Some(
                im.license
                    .as_ref()
                    .map_or_else(String::new, |l| format!("l:{l}")),
            )
        },
        None,
    );
    let actions = actions.clone();
    let on_change = Callback::new(move |value: String| {
        actions.import_pick_license(value.strip_prefix("l:").map(str::to_owned));
    });
    Some(view! {
        <Select
            label=move || t(k::LICENSE_PICK, &[])
            value=value
            on_change=on_change
            options=options
            attrs=Attrs::from([("data-testid", "import-license")])
        />
    })
}

/// The versification the project gets (H13): the books' verse counts give the default, the
/// note says how, and the person can choose another.
fn versification_pick(
    actions: &Actions,
    im: &Signal<Option<ImportForm>>,
    guess: &tc_api::import::SchemeGuess,
) -> impl IntoView + use<> {
    use keys::importer::review as k;
    use tc_api::import::SchemeFit;
    let scheme = guess.scheme.as_str();
    let note = match &guess.fit {
        SchemeFit::Exact => t(k::SCHEME_EXACT, &[("scheme", &scheme)]),
        SchemeFit::Undecided => t(k::SCHEME_UNDECIDED, &[("scheme", &scheme)]),
        SchemeFit::Tied { schemes } => t(
            k::SCHEME_TIED,
            &[("schemes", &schemes.join(", ")), ("scheme", &scheme)],
        ),
        SchemeFit::Nearest => t(k::SCHEME_NEAREST, &[("scheme", &scheme)]),
    };
    let fit = match guess.fit {
        SchemeFit::Exact => "exact",
        SchemeFit::Undecided => "undecided",
        SchemeFit::Tied { .. } => "tied",
        SchemeFit::Nearest => "nearest",
    };
    let options: Vec<SelectOption> = guess
        .names
        .iter()
        .map(|name| SelectOption::from(name.as_str()))
        .collect();
    let value = read(im, |im| Some(im.versification.clone()), None);
    let actions = actions.clone();
    let on_change = Callback::new(move |name: String| actions.import_pick_versification(name));
    view! {
        <div style="display: flex; flex-direction: column; gap: 6px;">
            <Select
                label=move || t(keys::wizard::VERSIFICATION, &[])
                value=value
                on_change=on_change
                options=options
                attrs=Attrs::from([("data-testid", "import-versification")])
            />
            <Text role=TextRole::Caption tone=TextTone::Muted attrs=Attrs::from([("data-testid", "import-versification-note"), ("data-fit", fit)])>
                {note}
            </Text>
        </div>
    }
}

fn found(
    actions: &Actions,
    im: &Signal<Option<ImportForm>>,
    listing: Memo<Vec<String>>,
    review: ImportReview,
) -> impl IntoView + use<> {
    use keys::importer::review as k;
    let books = review
        .found
        .books
        .clone()
        .into_iter()
        .map(|b| {
            let line = t(
                if b.chapters == 1 { k::BOOK_LINE_ONE } else { k::BOOK_LINE },
                &[("chapters", &b.chapters), ("verses", &b.verses), ("drafted", &b.drafted)],
            );
            let not_kept = b
                .not_kept
                .into_iter()
                .map(|n| view! { <Text role=TextRole::Caption tone=TextTone::Muted>{n}</Text> })
                .collect_view();
            view! {
                <Surface fill=Fill::Muted radius=Corner::Md pad="10px 14px" attrs=Attrs::new().with("data-testid", format!("import-book-{}", b.book.code()))>
                    <div style="display: flex; flex-direction: column; gap: 4px;">
                        <Text role=TextRole::Strong>{b.book.english_name()}</Text>
                        <Text role=TextRole::Caption tone=TextTone::Muted>{line}</Text>
                        {not_kept}
                    </div>
                </Surface>
            }
        })
        .collect_view();
    let files = im.with_untracked(|im| im.as_ref().map_or(0, |im| im.files.len()));
    let facts = vec![
        KeyValue {
            k: t(k::BOOKS, &[]),
            v: review.found.books.len().to_string(),
        },
        KeyValue {
            k: t(k::FORMAT, &[]),
            v: kind_text(ImportKind::Tc3, "title"),
        },
        KeyValue {
            k: t(k::FILES, &[]),
            v: files.to_string(),
        },
        KeyValue {
            k: t(k::CREATES, &[]),
            v: t(k::NEW_BIBLE, &[]),
        },
    ];
    let sum =
        |f: fn(&tc_api::import::FoundBook) -> u32| review.found.books.iter().map(f).sum::<u32>();
    let aligned = sum(|b| b.aligned);
    let carried = vec![
        KeyValue {
            k: t(k::VERSES, &[]),
            v: sum(|b| b.verses).to_string(),
        },
        KeyValue {
            k: t(k::ALIGNMENTS, &[]),
            v: t(
                if aligned == 1 {
                    k::ALIGNED_VERSES_ONE
                } else {
                    k::ALIGNED_VERSES
                },
                &[("n", &aligned)],
            ),
        },
        KeyValue {
            k: t(k::DECISIONS, &[]),
            v: sum(|b| b.decisions).to_string(),
        },
        KeyValue {
            k: t(k::CONTRIBUTORS, &[]),
            v: review.found.contributors.len().to_string(),
        },
    ];

    let name = read(im, |im| im.name.clone(), String::new());
    let code = read(im, |im| im.code.clone(), String::new());
    let name_error: Signal<Option<String>> = {
        let im = *im;
        Signal::from(Memo::new(move |_| {
            listing.with(|names| {
                im.with(|im| {
                    im.as_ref()
                        .filter(|im| im.name_in_use(names.iter().map(String::as_str)))
                        .map(|_| crate::i18n::refusal_text(&ImportRefusal::NameExists))
                })
            })
        }))
    };
    let lang_error: Signal<Option<String>> = {
        let im = *im;
        Signal::from(Memo::new(move |_| {
            im.with(|im| {
                let im = im.as_ref()?;
                if im.code.trim().is_empty() {
                    return Some(t(k::LANG_ERROR, &[]));
                }
                im.details_refusal().map(|r| crate::i18n::refusal_text(r))
            })
        }))
    };
    let details_text = move || {
        t(
            if lang_error.with(Option::is_some) {
                k::DETAILS_NO_LANG
            } else {
                k::DETAILS_TEXT
            },
            &[],
        )
    };
    let details_status = Signal::derive(move || {
        if lang_error.with(Option::is_some) {
            Status::Warn
        } else {
            Status::Valid
        }
    });
    let lang_hint = Signal::derive(move || {
        let tag = code.get();
        let primary = if tag.starts_with("x-") {
            tag.clone()
        } else {
            tag.split('-').next().unwrap_or_default().to_owned()
        };
        if !primary.is_empty() && primary != tag {
            t(k::LANG_STORED, &[("primary", &primary), ("tag", &tag)])
        } else {
            t(k::LANG_HINT, &[])
        }
    });
    let (on_name, on_code) = {
        let (a, b) = (actions.clone(), actions.clone());
        (
            Callback::new(move |e: web_sys::Event| a.import_type_name(event_target_value(&e))),
            Callback::new(move |e: web_sys::Event| b.import_type_code(event_target_value(&e))),
        )
    };
    let pick = license_pick(actions, im, &review.found.licenses);
    let versification = versification_pick(actions, im, &review.found.versification);
    view! {
        <div data-testid="import-review" style="display: flex; flex-direction: column; gap: 16px;">
            <div style="display: grid; grid-template-columns: repeat(auto-fit,minmax(280px,1fr)); gap: 24px; align-items: start;">
                <div style="display: flex; flex-direction: column; gap: 14px;">
                    <Overline tag="span">{t(k::FOUND, &[])}</Overline>
                    <div style="display: flex; flex-direction: column; gap: 8px;">{books}</div>
                    <KeyValueGrid columns=2 items=facts />
                    <div style="display: flex; flex-direction: column; gap: 8px;" data-testid="import-carried">
                        <Overline tag="span">{t(k::CARRIED, &[])}</Overline>
                        <KeyValueGrid columns=2 items=carried />
                    </div>
                </div>
                <div style="display: flex; flex-direction: column; gap: 14px;">
                    <Overline tag="span">{t(k::CHECKS, &[])}</Overline>
                    {resources_check(actions, im, review.found.slots.clone())}
                    {license_check(&review)}
                    <div
                        data-testid="import-check-details"
                        data-status=move || if details_status.get() == Status::Warn { "warn" } else { "valid" }
                        style="display: flex; gap: 10px; align-items: baseline;"
                    >
                        <StatusDot status=details_status size=8.0 />
                        <div style="flex: 1; display: flex; flex-direction: column; gap: 3px;">
                            <Text role=TextRole::Strong>{t(k::DETAILS, &[])}</Text>
                            <Text role=TextRole::Caption>{details_text}</Text>
                        </div>
                    </div>
                    {check("missing", k::MISSING, Status::Valid, t(k::NONE_MISSING, &[]))}
                    <Surface fill=Fill::Paper border=Edge::Line radius=Corner::Lg pad=Inset::Md>
                        <div style="display: flex; flex-direction: column; gap: 12px;">
                            <Text role=TextRole::Caption tone=TextTone::Muted>{t(k::READ_FROM_FILE, &[])}</Text>
                            {pick}
                            <Field
                                label=Signal::derive(|| t(keys::importer::review::NAME, &[]))
                                live_error=name_error
                                attrs=Attrs::from([("data-testid", "import-name-field")])
                            >
                                <Input
                                    value=name
                                    invalid=Signal::derive(move || name_error.with(Option::is_some))
                                    attrs=Attrs::from([("data-testid", "import-name")])
                                    on_change=on_name
                                />
                            </Field>
                            <Field
                                label=Signal::derive(|| t(keys::importer::review::LANG, &[]))
                                hint=lang_hint
                                live_error=lang_error
                            >
                                <Input
                                    value=code
                                    invalid=Signal::derive(move || lang_error.with(Option::is_some))
                                    attrs=Attrs::from([("data-testid", "import-lang")])
                                    on_change=on_code
                                />
                            </Field>
                            {versification}
                        </div>
                    </Surface>
                </div>
            </div>
        </div>
    }
}

fn working_step(im: &Signal<Option<ImportForm>>) -> impl IntoView + use<> {
    let name = im.with_untracked(|im| {
        im.as_ref()
            .map(|im| im.name.trim().to_owned())
            .unwrap_or_default()
    });
    let label = if name.is_empty() {
        t(keys::importer::working::LABEL, &[])
    } else {
        t(keys::importer::working::NAMED, &[("name", &name)])
    };
    let fraction = read(
        im,
        |im| match &im.state {
            ImportState::Running {
                progress: Some(p), ..
            } => p.total.filter(|n| *n > 0).map(|total| {
                #[expect(clippy::cast_precision_loss, reason = "a step count far below 2^52")]
                let f = p.done as f64 / total as f64;
                f.clamp(0.0, 1.0)
            }),
            _ => None,
        },
        None,
    );
    view! {
        <div style="padding: 26px 0 30px; display: flex; flex-direction: column; align-items: center; gap: 16px;" data-testid="import-working">
            <Spinner label=label.clone() />
            {move || fraction.get().map(|f| view! {
                <ProgressBar value=Signal::derive(move || f) label=Some(label.clone()) style=crate::css! { "width": "100%" } attrs=Attrs::from([("data-testid", "import-progress")]) />
            })}
        </div>
    }
}

fn failed_step(im: &Signal<Option<ImportForm>>) -> AnyView {
    let failure = im.with_untracked(|im| match im.as_ref().map(|im| &im.state) {
        Some(ImportState::Failed(failure)) => Some(failure.clone()),
        _ => None,
    });
    match failure {
        Some(failure) => {
            let below = [failure.code.clone(), failure.detail.clone()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            refusal_panel(
                &failure,
                "import-failed",
                keys::importer::failed::TITLE,
                below,
            )
            .into_any()
        }
        None => ().into_any(),
    }
}
