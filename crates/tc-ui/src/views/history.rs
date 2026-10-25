//! History with *Restore this version* for the record a screen shows (owner ruling H5): outside
//! verse text a teammate's newer version wins with no question, and the person's own stays here.
//! The Check screen places it under a check's marks, the aligner under a verse's reference.

use leptos::prelude::*;
use tc_api::ShownToken;
use tc_api::conflicts::Versions;
use tc_api::ids::{ChangeRef, Scope};

use crate::app::web::use_runtime;
use crate::attrs::Attrs;
use crate::ds::{Button, ButtonVariant, Size};
use crate::i18n::{keys, t};
use crate::views::review::history_section;

/// What a screen gives its History: the record's token as shown (none hides History, as for a
/// record in conflict, which has its review instead), whether it is open, and what Restore does.
pub struct RecordHistory {
    pub test_id: &'static str,
    pub at: Scope,
    pub shown: Signal<Option<ShownToken>>,
    pub open: ArcRwSignal<bool>,
    pub busy: Signal<bool>,
    /// The last Restore was not saved.
    pub failed: Signal<bool>,
    /// "History of {ref}"'s place.
    pub reference: String,
    pub restore: Callback<ChangeRef>,
}

/// The History toggle and, while it is open, every version of the record, newest first.
pub fn record_history(h: RecordHistory) -> AnyView {
    let RecordHistory {
        test_id,
        at,
        shown,
        open,
        busy,
        failed,
        reference,
        restore,
    } = h;
    let toggle = {
        let open = open.clone();
        Callback::new(move |_| open.update(|o| *o = !*o))
    };
    let title = t(keys::review::HISTORY_OF, &[("ref", &reference)]);
    let history = move || {
        let shown = shown.get()?;
        let list = open
            .get()
            .then(|| {
                let read = use_runtime()?.cache().read(Versions {
                    at: at.clone(),
                    shown,
                });
                Some(history_section(read, title.clone(), restore, busy))
            })
            .flatten();
        let label = if open.get() {
            t(keys::review::HISTORY_HIDE, &[])
        } else {
            t(keys::review::HISTORY, &[])
        };
        Some(view! {
            <div data-testid=test_id style="display: flex; flex-direction: column; gap: 8px; margin: 14px 0;">
                <Button variant=ButtonVariant::Ghost size=Size::Sm on_click=toggle
                    style=crate::css! { "align-self": "flex-start", "padding": 0, "height": "auto" }
                    attrs=Attrs::new().with("data-testid", "history-toggle").with("aria-expanded", open.get().to_string())>
                    {label}
                </Button>
                {move || failed.get().then(|| view! {
                    <p data-testid="history-not-restored" role="alert" style="margin: 0; font-size: var(--fs-meta); color: var(--tc-invalid);">
                        {t(keys::review::RESTORE_NOT_SAVED, &[])}
                    </p>
                })}
                {list}
            </div>
        })
    };
    view! { {history} }.into_any()
}
