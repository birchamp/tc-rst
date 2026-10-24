//! The conflict review's actions (unit B11): tC4's `openConflictReview`, `openProjectConflicts`,
//! `showConflict`, `closeConflictReview` and `keepVersion`. The decisions are in
//! [`crate::app::review`].

use leptos::prelude::*;
use tc_api::conflicts::{BulkConflict, ConflictEntry, KeepChoice, Resolve, ResolveBulk, ReviewTab};
use tc_api::endpoint::{Endpoint, Write};
use tc_api::ids::{ChangeRef, ConflictRef, RecordRef, RequestId, Scope};
use tc_api::shown::PersonLabel;
use tc_api::write::Ack;
use wasm_bindgen_futures::spawn_local;

use super::app_ctx::Actions;
use super::js;
use crate::app::review::{KeepReply, Press, ReviewState, keep_reply, picks_of};
use crate::app::shell::Editing;

fn kept(ack: Ack) -> KeepReply {
    KeepReply::Kept {
        disk_warning: ack.disk_warning,
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
            .with_untracked(|r| r.as_ref().is_some_and(|r| r.keep.busy()))
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
            r.keep.dismiss();
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

    /// One path for every press of the review (INV-ONE-RESOLVE-PER-PRESS): the press goes
    /// through [`crate::app::review::KeepPhase`], which gives it a request only while none
    /// is in flight and the same request again after a lost reply, whose first reply the
    /// server gives again (A-27). `build` makes the request; `reply` reads its answer.
    fn send<E: Write + Endpoint + 'static>(
        &self,
        press: Press,
        position: Option<usize>,
        build: impl FnOnce(Scope, RequestId) -> E,
        reply: impl FnOnce(E::Ok) -> KeepReply + 'static,
    ) {
        let s = self.s();
        let Some(project) = s.project.get_untracked() else {
            return;
        };
        let mut request = None;
        self.update_review(|r| {
            request = r.keep.press(&press, js::random_request_id);
            if let (Some(_), Some(position)) = (request, position) {
                r.position = position;
            }
        });
        let Some(request) = request else {
            return;
        };
        let made = matches!(press, Press::Keep { .. });
        let req = build(project.at, request);
        let this = self.clone();
        spawn_local(async move {
            let answer = this.runtime().api().call(&req).await;
            let kept_it = answer.is_ok();
            let reply = keep_reply(answer.into(), reply);
            this.update_review(|r| {
                r.keep.reply(request, reply);
                if kept_it && made {
                    r.selected = None;
                }
            });
        });
    }

    /// A choice on `entry` (tC4 `keepVersion` for *Keep this version*): one resolution over
    /// every version the entry showed. The review stays at its place and shows the conflict
    /// now there.
    fn choose(&self, entry: &ConflictEntry, position: usize, choice: KeepChoice) {
        let (conflict, review) = (entry.conflict.clone(), entry.review.clone());
        self.send(
            Press::Keep {
                conflict: conflict.clone(),
                choice: choice.clone(),
            },
            Some(position),
            move |at, request| Resolve {
                at,
                request,
                conflict,
                review,
                choice,
            },
            kept,
        );
    }

    /// Keep this version: the version clicked.
    pub fn keep_version(&self, entry: ConflictEntry, position: usize) {
        let Some(change) = self
            .s()
            .review
            .with_untracked(|r| r.as_ref().and_then(|r| r.selected.clone()))
        else {
            return;
        };
        self.choose(&entry, position, KeepChoice::Candidate { change });
    }

    /// *Keep Maria's passage*: her side's versions, every record of the passage.
    pub fn keep_passage(&self, entry: ConflictEntry, position: usize, person: &PersonLabel) {
        let picks = picks_of(&entry, person);
        if picks.is_empty() {
            return;
        }
        self.choose(&entry, position, KeepChoice::Passage { picks });
    }

    /// *Keep the joined verses*, with the version clicked as the span picked, if any.
    pub fn keep_joined(&self, entry: ConflictEntry, position: usize) {
        let picks = self.picked();
        self.choose(&entry, position, KeepChoice::Joined { picks });
    }

    /// *Keep the verses separate*, with the version clicked as the text picked, if any.
    pub fn keep_separate(&self, entry: ConflictEntry, position: usize) {
        let picks = self.picked();
        self.choose(&entry, position, KeepChoice::Separate { picks });
    }

    fn picked(&self) -> Vec<ChangeRef> {
        self.s()
            .review
            .with_untracked(|r| r.as_ref().and_then(|r| r.selected.clone()))
            .into_iter()
            .collect()
    }

    /// *Restore this version* (#78): the old version kept over every version of its record;
    /// the others stay in History.
    pub fn restore_version(&self, entry: ConflictEntry, position: usize, from: ChangeRef) {
        self.choose(&entry, position, KeepChoice::Restore { from });
    }

    /// *Keep all of Maria's* over every conflict the tab lists: the reply says how many
    /// were left for a choice of their own.
    pub fn keep_all(&self, person: &PersonLabel, entries: Vec<ConflictEntry>) {
        let label = person.clone();
        let who = person.who.clone();
        let sent = u32::try_from(entries.len()).unwrap_or(u32::MAX);
        self.send(
            Press::KeepAll {
                person: who.clone(),
            },
            None,
            move |at, request| ResolveBulk {
                at,
                request,
                person: who,
                conflicts: entries
                    .into_iter()
                    .map(|e| BulkConflict {
                        conflict: e.conflict,
                        review: e.review,
                    })
                    .collect(),
            },
            move |kept| {
                let left = u32::try_from(kept.left.len()).unwrap_or(u32::MAX);
                KeepReply::KeptAll {
                    person: label,
                    kept: sent.saturating_sub(left),
                    sent,
                    disk_warning: kept.ack.disk_warning,
                }
            },
        );
    }
}
