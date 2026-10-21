//! The conflict review's actions (unit B11): tC4's `openConflictReview`, `openProjectConflicts`,
//! `showConflict`, `closeConflictReview` and `keepVersion`. The decisions are in
//! [`crate::app::review`].

use leptos::prelude::*;
use tc_api::conflicts::{ConflictEntry, Resolve, ReviewTab};
use tc_api::ids::{ChangeRef, ConflictRef, RecordRef};
use tc_api::{ApiError, NotSaved};
use wasm_bindgen_futures::spawn_local;

use super::app_ctx::Actions;
use super::js;
use crate::app::client::CallError;
use crate::app::review::{KeepOutcome, ReviewState};
use crate::app::shell::Editing;
use crate::i18n::{keys, t};

/// What a keep that did not happen says after "The version was not kept.".
fn failure_text(error: &CallError) -> String {
    match error {
        CallError::Api(ApiError::NotSaved(NotSaved::LookAgain)) => {
            t(keys::conflicts::OTHER_KEPT, &[])
        }
        other => other.to_string(),
    }
}

impl Actions {
    fn update_review(&self, patch: impl FnOnce(&mut ReviewState)) {
        self.s().review.try_update(|r| {
            if let Some(r) = r {
                patch(r);
            }
        });
    }

    fn review_busy(&self) -> bool {
        self.s()
            .review
            .with_untracked(|r| r.as_ref().is_some_and(|r| r.busy))
    }

    /// tC4 `openConflictReview`: the review of `tab` at `conflict`, or at its first. Never over
    /// an open editing card: its text lives in the card (tC4 bench P1); the entries are off
    /// while one is open.
    pub fn open_review(&self, tab: ReviewTab, conflict: Option<ConflictRef>) {
        let s = self.s();
        if s.editing.with_untracked(Editing::is_open) || s.project.with_untracked(Option::is_none) {
            return;
        }
        s.review.try_set(Some(ReviewState::at(tab, conflict)));
    }

    /// A Home card's chip and the sync toast's Review (tC4 `openProjectConflicts`): the project
    /// opens in Translate at `book`, the book of its first conflict, then the review. The toast
    /// closes once the review is open, so a failed open keeps its links (#724).
    pub fn open_project_conflicts(
        &self,
        project: tc_api::ids::ProjectRef,
        book: Option<tc_core::BookId>,
        tab: ReviewTab,
    ) {
        self.open_for_review(project, book, tab);
    }

    /// Back (tC4 `closeConflictReview`): the conflicts that are open stay listed, and nothing
    /// is written.
    pub fn close_review(&self) {
        if self.review_busy() {
            return;
        }
        self.s().review.try_set(None);
    }

    /// Previous, Next (tC4 `showConflict`).
    pub fn show_conflict(&self, conflict: ConflictRef, position: usize) {
        if self.review_busy() {
            return;
        }
        self.update_review(|r| {
            r.show(conflict, position);
            r.outcome = None;
        });
    }

    pub fn choose_review_tab(&self, tab: ReviewTab) {
        if self.review_busy() {
            return;
        }
        self.update_review(|r| r.choose_tab(tab));
    }

    /// A click on a version: the others fade.
    pub fn select_version(&self, change: ChangeRef) {
        if self.review_busy() {
            return;
        }
        self.update_review(|r| r.selected = Some(change));
    }

    pub fn cancel_selection(&self) {
        self.update_review(|r| r.selected = None);
    }

    pub fn toggle_history(&self, record: RecordRef) {
        self.update_review(|r| {
            r.history = if r.history.as_ref() == Some(&record) {
                None
            } else {
                Some(record)
            };
        });
    }

    /// Keep this version (tC4 `keepVersion`): one resolution over every version the entry
    /// showed. Off while it runs; a second press of the same choice sends the same request,
    /// whose first reply the server gives again (A-27). The review stays at its place and
    /// shows the conflict now there.
    pub fn keep_version(&self, entry: ConflictEntry, position: usize) {
        let s = self.s();
        let Some(project) = s.project.get_untracked() else {
            return;
        };
        let Some(change) = s.review.with_untracked(|r| {
            r.as_ref()
                .filter(|r| !r.busy)
                .and_then(|r| r.selected.clone())
        }) else {
            return;
        };
        let mut request = None;
        self.update_review(|r| {
            r.busy = true;
            r.outcome = None;
            r.position = position;
            request = Some(r.request_for(&entry.conflict, &change, js::random_request_id));
        });
        let Some(request) = request else {
            return;
        };
        let resolve = Resolve {
            at: project.at,
            request,
            conflict: entry.conflict.clone(),
            review: entry.review.clone(),
            keep: change,
        };
        let this = self.clone();
        spawn_local(async move {
            let reply = this.runtime().api().call(&resolve).await;
            this.update_review(|r| {
                r.busy = false;
                if matches!(reply, Ok(_) | Err(CallError::Api(_))) {
                    r.answered();
                }
                match reply {
                    Ok(ack) => {
                        r.selected = None;
                        r.outcome = ack
                            .disk_warning
                            .map(|error| KeepOutcome { kept: true, error });
                    }
                    Err(error) => {
                        r.outcome = Some(KeepOutcome {
                            kept: false,
                            error: failure_text(&error),
                        });
                    }
                }
            });
        });
    }
}
