//! History with *Restore this version* for the shown check's decision (owner ruling H5): a
//! teammate's newer decision or comment wins with no question, and the person's own stays here.

use leptos::prelude::*;
use tc_api::conflicts::Versions;

use crate::app::web::check::Detail;
use crate::app::web::{Actions, AppState, use_runtime};
use crate::attrs::Attrs;
use crate::ds::{Button, ButtonVariant, Size};
use crate::i18n::{keys, t};
use crate::views::review::history_section;

/// The History toggle under the check's marks and, while it is open, every version of its
/// decision. A decision in conflict has its review instead.
pub(super) fn check_history(s: &AppState, actions: &Actions, reference: String) -> AnyView {
    let c = s.c.clone();
    let at = s
        .project
        .with_untracked(|p| p.as_ref().map(|p| p.at.clone()));
    let Some(at) = at else {
        return ().into_any();
    };
    let shown = {
        let detail = c.detail.clone();
        Memo::new(move |_| {
            detail.with(|d| match d {
                Some(Detail::Ready(view)) if view.decision.marker.is_none() => {
                    Some(view.decision.token.clone())
                }
                _ => None,
            })
        })
    };
    let open = c.history_open.clone();
    let toggle = {
        let open = open.clone();
        Callback::new(move |_| open.update(|o| *o = !*o))
    };
    let busy = Signal::derive({
        let restoring = c.restoring.clone();
        move || restoring.get()
    });
    let restore = {
        let actions = actions.clone();
        Callback::new(move |from| actions.restore_check(from))
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
            <div data-testid="check-history" style="display: flex; flex-direction: column; gap: 8px; margin-top: 14px;">
                <Button variant=ButtonVariant::Ghost size=Size::Sm on_click=toggle
                    style=crate::css! { "align-self": "flex-start", "padding": 0, "height": "auto" }
                    attrs=Attrs::new().with("data-testid", "check-history-toggle")>
                    {label}
                </Button>
                {list}
            </div>
        })
    };
    view! { {history} }.into_any()
}
