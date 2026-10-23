//! The guided fix's three ways (M11; tC4 `fixFetch`, `fixSideload`, `fixRepin`, `finishFix`),
//! wired to C8's endpoints by B5b. The decisions are `crate::app::fix`'s; this sends the
//! requests and follows the download.
//!
//! The download and the file install change no pin: what was missing is installed, the
//! Check picker reads again and the tool's card turns ready. The re-pin plans the move and opens
//! the confirmation (M9b), which takes over from this screen; the pins move only when the person
//! confirms it there.

use std::cell::Cell;

use leptos::prelude::*;
use tc_api::ApiError;
use tc_api::check::CheckUnit;
use tc_api::event::{Aspect, ChangeParts, ChangeScope};
use tc_api::refusal::{Refusal, ResourceRefusal};
use tc_api::resources::{FixFetch, FixRepin, FixSideload};
use tc_api::upload::UploadFlow;
use wasm_bindgen_futures::spawn_local;

use super::app_ctx::Actions;
use super::js;
use super::sources::{api_error, said};
use crate::app::fix::{self, Fix, FixScreen};
use crate::app::internet::Pending;
use crate::i18n::{keys, t};

thread_local! {
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
}

/// A new screen's identity.
pub(super) fn next_id() -> u64 {
    NEXT_ID.with(|id| {
        id.set(id.get() + 1);
        id.get()
    })
}

/// Why a way failed, for the screen's own sentence: a refusal of the fix carries its reason, which
/// the server would otherwise word as a failed install, wrong for a re-pin.
fn why_failed(error: &ApiError) -> String {
    match error {
        ApiError::Refused(Refusal::Resources(ResourceRefusal::FixFailed { reason })) => {
            reason.clone()
        }
        other => said(other),
    }
}

/// What a request of the fix names: the project, the tool and its book.
struct Asked {
    at: tc_api::Scope,
    tool: tc_api::check::Tool,
    unit: CheckUnit,
}

impl Actions {
    fn patch_fix<R>(&self, id: u64, change: impl FnOnce(&mut FixScreen) -> R) -> Option<R> {
        self.s()
            .c
            .fix
            .try_update(|f| fix::patch(f, id, change))
            .flatten()
    }

    /// The screen shown, when it can start a way: its id and what its requests name.
    fn fix_screen(&self) -> Option<(u64, Asked)> {
        let s = self.s();
        let (id, tool) = s.c.fix.with_untracked(|f| match f {
            Some(Fix::Ready(screen)) if screen.busy.is_none() => Some((screen.id, screen.tool)),
            _ => None,
        })?;
        let at = s.project.get_untracked()?.at;
        let book = s.u.book.get_untracked()?;
        Some((
            id,
            Asked {
                at,
                tool,
                unit: CheckUnit::Book(book),
            },
        ))
    }

    /// "Download it" (tC4 `fixFetch`): asks first while the internet is off.
    pub fn fix_fetch(&self) {
        if let Some((id, _)) = self.fix_screen() {
            self.internet_task(Pending::Fix(id));
        }
    }

    pub(super) fn fix_fetch_now(&self, id: u64) {
        let Some((shown, asked)) = self.fix_screen() else {
            return;
        };
        if shown != id || self.patch_fix(id, FixScreen::start_fetch) != Some(true) {
            return;
        }
        let this = self.clone();
        spawn_local(async move {
            let request = FixFetch {
                request: js::random_request_id(),
                at: asked.at.clone(),
                tool: asked.tool,
                unit: asked.unit,
            };
            let ended = match this.runtime().api().call(&request).await.map_err(api_error) {
                Ok(job) => {
                    this.follow_job(job, |repo| {
                        this.patch_fix(id, |screen| screen.fetching(&repo));
                    })
                    .await
                }
                Err(error) => Err(Some(error)),
            };
            match ended {
                Ok(_) => this.finish_fix(id, &asked.at),
                Err(None) => {
                    this.patch_fix(id, FixScreen::stopped);
                }
                // The switch went off after the question: ask once more.
                Err(Some(ApiError::NeedsInternet(_))) => {
                    this.patch_fix(id, FixScreen::stopped);
                    this.ask_again(Pending::Fix(id));
                }
                Err(Some(error)) => {
                    let reason = why_failed(&error);
                    this.patch_fix(id, |screen| screen.install_failed(&reason));
                }
            }
        });
    }

    /// A file chosen or dropped on the third way (tC4 `fixSideload`): installed only when it
    /// declares exactly the pinned commit, which the server checks before anything is written.
    pub fn fix_sideload(&self, file: web_sys::File) {
        let Some((id, asked)) = self.fix_screen() else {
            return;
        };
        let name = file.name();
        if self.patch_fix(id, |screen| screen.start_sideload(&name)) != Some(true) {
            return;
        }
        let this = self.clone();
        spawn_local(async move {
            let api = this.runtime().api();
            // The server refuses a file over the flow's cap before it stores it.
            let sent = match api.upload(&file, UploadFlow::GuidedFix).await {
                Ok(upload) => {
                    let request = FixSideload {
                        request: js::random_request_id(),
                        at: asked.at.clone(),
                        tool: asked.tool,
                        unit: asked.unit,
                        upload: upload.id,
                    };
                    api.call(&request).await
                }
                Err(error) => Err(error),
            }
            .map_err(|error| why_failed(&api_error(error)));
            match sent {
                Ok(()) => this.finish_fix(id, &asked.at),
                Err(reason) => {
                    this.patch_fix(id, |screen| screen.install_failed(&reason));
                }
            }
        });
    }

    /// An installed version (tC4 `fixRepin`): the move is planned from the project as it is
    /// and confirmed in the re-pin confirmation, which replaces this screen. A plan that cannot
    /// be made is said here, with the other two ways still at hand.
    pub fn fix_repin(&self, sha: String) {
        let Some((id, asked)) = self.fix_screen() else {
            return;
        };
        if self.patch_fix(id, FixScreen::start_repin) != Some(true) {
            return;
        }
        let this = self.clone();
        spawn_local(async move {
            // Saved first: the plan is read over work that is safe (A-26), as an upgrade's is.
            if !this.runtime().saves().gate().await {
                let reason = t(keys::upgrade::ALIGN_UNSAVED, &[]);
                this.patch_fix(id, |screen| screen.repin_failed(&reason));
                return;
            }
            let request = FixRepin {
                request: js::random_request_id(),
                at: asked.at.clone(),
                tool: asked.tool,
                unit: asked.unit,
                sha,
            };
            let planned = this.runtime().api().call(&request).await.map_err(api_error);
            // A plan for a project the person has left would describe one project over another.
            let same_project = this
                .s()
                .project
                .with_untracked(|p| p.as_ref().is_some_and(|p| p.at == asked.at));
            match planned {
                Ok(plan) if same_project => {
                    this.s().up.try_update(|up| up.plan_ready(plan));
                    // The confirmation takes over: one dialog at a time.
                    if this.patch_fix(id, FixScreen::stopped).is_some() {
                        this.close_modal();
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    let reason = why_failed(&error);
                    this.patch_fix(id, |screen| screen.repin_failed(&reason));
                }
            }
        });
    }

    /// What was missing is installed (tC4 `finishFix`): the picker and the panes read again,
    /// and the screen closes if it is still the one this way started from.
    fn finish_fix(&self, id: u64, at: &tc_api::Scope) {
        let installed = ChangeScope::Parts(ChangeParts {
            aspects: vec![Aspect::Pins],
            ..ChangeParts::default()
        });
        self.runtime().cache().changed(at, &installed);
        if self.patch_fix(id, FixScreen::stopped).is_some() {
            self.close_modal();
        }
    }
}
