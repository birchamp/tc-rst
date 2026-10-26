//! The shared user-comment field of Bible and OBS Understand. Its target and displayed
//! token stay together; the save machine owns typing, blur saves and recovery.

use leptos::prelude::*;
use send_wrapper::SendWrapper;
use tc_api::editor::CommentTarget;
use tc_api::ids::Scope;
use wasm_bindgen::JsCast;

use crate::app::save::{FieldScope, Pair, Source};
use crate::app::shell::js_trim;
use crate::app::translate::comment_key;
use crate::app::web::{use_app, use_runtime};
use crate::i18n::{keys, t};

#[component]
pub fn CommentBox(
    at: Scope,
    target: CommentTarget,
    current: Memo<Option<Pair>>,
    dir: &'static str,
    placeholder: String,
) -> impl IntoView {
    let (Some(app), Some(runtime)) = (use_app(), use_runtime()) else {
        return ().into_any();
    };
    let Some(project) = app.s.project.get_untracked() else {
        return ().into_any();
    };
    let Some(key) = comment_key(&at.project, &project.id, &target) else {
        return ().into_any();
    };
    let saves = runtime.saves().clone();
    let shown = saves.open_comment(
        &key,
        FieldScope::Project(Some(at.clone())),
        current.get_untracked(),
    );
    let sessions = runtime.state().sessions.clone();
    let writable = {
        let (saves, at) = (SendWrapper::new(saves.clone()), at.clone());
        Memo::new(move |_| {
            sessions.track();
            saves.write_cap(&at).is_some()
        })
    };
    {
        let (saves, key) = (SendWrapper::new(saves.clone()), key.clone());
        Effect::new(move |_| {
            if let Some(pair) = current.get() {
                saves.refresh(&key, pair, None);
            }
        });
    }
    {
        let (saves, key) = (SendWrapper::new(saves.clone()), key.clone());
        on_cleanup(move || saves.close(&key));
    }
    let emptied = RwSignal::new(false);
    let clear_refused = RwSignal::new(false);
    let input = {
        let (saves, key) = (SendWrapper::new(saves.clone()), key.clone());
        move |event: leptos::ev::Event| {
            let text = event_target_value(&event);
            clear_refused.set(false);
            emptied.set(js_trim(&text).is_empty());
            if emptied.get_untracked() {
                saves.revert_comment(&key);
            } else {
                saves.typed(&key, &text, Source::Input);
            }
        }
    };
    let focus = {
        let (saves, key) = (SendWrapper::new(saves.clone()), key.clone());
        move |event: leptos::ev::FocusEvent| {
            if let Some(element) = event
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
            {
                saves.focus(&key, &element);
            }
        }
    };
    let blur = {
        let (saves, key, shown) = (SendWrapper::new(saves.clone()), key.clone(), shown.clone());
        move |_: leptos::ev::FocusEvent| {
            if emptied.get_untracked() {
                let text = saves.revert_comment(&key);
                clear_refused.set(!js_trim(&text).is_empty());
                shown.try_set(text);
                emptied.set(false);
            }
            saves.blur(&key);
        }
    };
    let composition = {
        let (saves, key) = (SendWrapper::new(saves.clone()), key.clone());
        move |_: leptos::ev::CompositionEvent| saves.composition(&key)
    };
    let history = comment_history(&at, &target, &key, current, &saves, &runtime);
    let style = crate::css! {
        "width": "100%", "box-sizing": "border-box", "margin-top": 4,
        "border": "var(--stroke) solid var(--border-input)",
        "border-radius": "var(--radius-md)", "padding": "11px 13px", "outline": "none",
        "resize": "vertical", "font-family": "var(--font-scripture)",
        "font-size": "var(--fs-verse)", "line-height": "var(--lh-verse)",
        "color": "var(--text-scripture)", "background": "var(--surface-app)",
    };
    view! {
        <textarea data-i="field" rows="3" dir=dir disabled=move || !writable.get()
            prop:value=move || shown.get() style=style.to_style() placeholder=placeholder
            on:input=input on:focus=focus on:blur=blur on:compositionstart=composition
        ></textarea>
        {move || clear_refused.get().then(|| view! {
            <p data-testid="understand-clear-refused" style="font-size: var(--fs-caption); letter-spacing: var(--track-12); color: var(--tc-warn-text); margin: 6px 0 0;">
                {t(keys::understand::CANNOT_CLEAR, &[])}
            </p>
        })}
        {history}
    }.into_any()
}

/// The comment's History (owner ruling H5): a teammate's newer comment wins with no question,
/// and *Restore this version* brings the person's own back. The text being typed is saved
/// first, so its save never lands over the restore.
fn comment_history(
    at: &Scope,
    target: &CommentTarget,
    key: &tc_api::buffer::BufferKey,
    current: Memo<Option<Pair>>,
    saves: &crate::app::web::Saves,
    runtime: &crate::app::web::Runtime,
) -> AnyView {
    let open = ArcRwSignal::new(false);
    let restoring = ArcRwSignal::new(false);
    let failed = ArcRwSignal::new(false);
    let restore = {
        let (at, key) = (at.clone(), key.clone());
        let (saves, runtime) = (
            SendWrapper::new(saves.clone()),
            SendWrapper::new(runtime.clone()),
        );
        let (restoring, failed) = (restoring.clone(), failed.clone());
        Callback::new(move |from: tc_api::ids::ChangeRef| {
            let (at, key, saves, runtime) =
                (at.clone(), key.clone(), saves.clone(), runtime.clone());
            let (restoring, failed) = (restoring.clone(), failed.clone());
            restoring.set(true);
            failed.set(false);
            wasm_bindgen_futures::spawn_local(async move {
                let shown = if saves.gate().await {
                    saves.base(&key).and_then(|base| base.into_iter().next())
                } else {
                    None
                };
                let restored = match shown {
                    Some(shown) => {
                        let request = tc_api::conflicts::Restore {
                            at,
                            request: crate::app::web::js::random_request_id(),
                            shown,
                            from,
                        };
                        runtime.api().call(&request).await.is_ok()
                    }
                    None => false,
                };
                restoring.set(false);
                failed.set(!restored);
            });
        })
    };
    crate::views::history::record_history(crate::views::history::RecordHistory {
        test_id: "comment-history",
        at: at.clone(),
        // A verse with no comment shows no History: one line per verse would crowd the list.
        shown: Signal::derive(move || {
            current
                .get()
                .filter(|pair| !pair.value.trim().is_empty())
                .map(|pair| pair.token)
        }),
        open,
        busy: Signal::derive(move || restoring.get()),
        failed: Signal::derive(move || failed.get()),
        reference: match target {
            CommentTarget::Verse {
                book,
                chapter,
                verse,
            } => format!("{} {chapter}:{verse}", book.english_name()),
            CommentTarget::Frame { story, frame } => t(
                keys::review::place::FRAME,
                &[("story", &story.get()), ("frame", frame)],
            ),
        },
        restore,
    })
}
