//! The actions of Project settings and Get source texts (unit B9a; tC4 `openSettings`,
//! `manageSettingsSources`, `openSources`, `pickGateway`, `setSourceBook`, `loadPackage`,
//! `downloadPackage`). Saving the settings is in `create.rs`; the checking-language change and the
//! release upgrades are C8's.

use leptos::prelude::*;
use tc_api::ApiError;
use tc_api::event::{Aspect, ChangeParts, ChangeScope};
use tc_api::home::HomeCard;
use tc_api::ids::JobId;
use tc_api::jobs::{Cancel, JobState, Phase};
use tc_api::refusal::Message;
use tc_api::resources::{Catalog, Download, GatewayPackage, PackageRow, Sources};
use tc_api::settings::ViewFromHome;
use tc_core::BookId;
use wasm_bindgen_futures::spawn_local;

use super::app_ctx::{Actions, Dialog};
use super::js;
use crate::app::client::CallError;
use crate::app::internet::Pending;
use crate::app::sources::{Dl, SettingsForm, SourcesForm};
use crate::i18n::t_dyn;

/// What a refusal says, in tC4's words; any other error as the server gave it.
pub(super) fn said(error: &ApiError) -> String {
    match error {
        ApiError::Refused(refusal) => match refusal.detail() {
            Some(detail) => detail.to_owned(),
            None => {
                let owned = refusal.vars();
                let vars: Vec<(&str, &dyn std::fmt::Display)> = owned
                    .iter()
                    .map(|(k, v)| (*k, v as &dyn std::fmt::Display))
                    .collect();
                t_dyn(refusal.key(), &vars, None)
            }
        },
        other => other.to_string(),
    }
}

pub(super) fn api_error(error: CallError) -> ApiError {
    match error {
        CallError::Api(error) => error,
        other => ApiError::BadRequest {
            field: "call".into(),
            reason: other.to_string(),
        },
    }
}

impl Actions {
    /// A card's Settings (tC4 `openSettings`): the dialog at once with the card's facts, and
    /// one read of the project's files for the rest (AD14).
    pub fn open_settings(&self, card: &HomeCard) {
        let s = self.s();
        let form = SettingsForm::of(card);
        let project = form.project.clone();
        s.st.try_set(Some(form));
        s.layers.dialog.try_set(Some(Dialog::Settings));
        let this = self.clone();
        spawn_local(async move {
            let request = ViewFromHome {
                project: project.clone(),
            };
            let read = this.runtime().api().call(&request).await;
            this.s().st.try_update(|st| {
                let Some(st) = st else { return };
                match read {
                    Ok(view) => st.read(&project, view),
                    Err(error) => st.failed(&project, said(&api_error(error))),
                }
            });
        });
    }

    /// The direction chips and the font (tC4 `patchSt`): what Save sends is another request.
    pub fn patch_st(&self, patch: impl FnOnce(&mut SettingsForm)) {
        self.s().st.try_update(|st| {
            if let Some(st) = st {
                patch(st);
                st.edited();
            }
        });
    }

    /// A package of the Checking language card (tC4 `chooseSettingsGateway`): Settings closes,
    /// the project opens, and the change of checking language is asked over it.
    pub fn choose_settings_gateway(&self, package: GatewayPackage) {
        let Some(project) = self
            .s()
            .st
            .with_untracked(|st| st.as_ref().map(|st| st.project.clone()))
        else {
            return;
        };
        self.close_modal();
        self.open_then_gateway(project, package);
    }

    /// "Manage source texts" (tC4 `manageSettingsSources`): Settings opens only from Home, and
    /// Get source texts needs the open project, so the project opens first and the dialog
    /// shows over it.
    pub fn manage_settings_sources(&self) {
        let Some(project) = self
            .s()
            .st
            .with_untracked(|st| st.as_ref().map(|st| st.project.clone()))
        else {
            return;
        };
        self.close_modal();
        self.open_then_sources(project);
    }

    /// Get source texts over the open project (tC4 `openSources`): step 1's packages are read
    /// again at each opening.
    pub fn open_sources(&self) {
        let s = self.s();
        s.layers.dialog.try_set(Some(Dialog::Sources));
        let Some(at) = s
            .project
            .with_untracked(|p| p.as_ref().map(|p| p.at.clone()))
        else {
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            if let Ok(view) = this.runtime().api().call(&Sources { at }).await {
                this.s().src.try_update(|src| src.packages = Some(view));
            }
        });
    }

    pub fn patch_src(&self, patch: impl FnOnce(&mut SourcesForm)) {
        self.s().src.try_update(patch);
    }

    /// A package of step 1 (tC4 `pickGateway`).
    pub fn pick_gateway(&self, gateway: GatewayPackage) {
        self.patch_src(|src| src.pick(gateway));
        self.internet_task(Pending::Catalog);
    }

    /// "Change language" (tC4 `changeGateway`).
    pub fn change_gateway(&self) {
        self.patch_src(SourcesForm::change_gateway);
    }

    /// The Book select (tC4 `setSourceBook`).
    pub fn set_source_book(&self, book: BookId) {
        let chosen = self.s().src.try_update(|src| {
            src.set_book(book);
            src.gateway.is_some()
        });
        if chosen == Some(true) {
            self.internet_task(Pending::Catalog);
        }
    }

    pub fn toggle_source_row(&self, row: &PackageRow) {
        self.patch_src(|src| src.toggle(row));
    }

    /// "Read the list" after Not now (tC4 `allowSources`).
    pub fn allow_sources(&self) {
        let chosen = self.s().src.try_update(|src| {
            if src.gateway.is_some() {
                src.set_book(src.book);
            }
            src.gateway.is_some()
        });
        if chosen == Some(true) {
            self.internet_task(Pending::Catalog);
        }
    }

    /// The internet question was Not now: the catalog was not read.
    pub(super) fn catalog_dropped(&self) {
        self.patch_src(SourcesForm::not_read);
    }

    /// Reads the chosen package's rows for the chosen book.
    pub(super) fn catalog_now(&self) {
        let s = self.s();
        let Some(at) = s
            .project
            .with_untracked(|p| p.as_ref().map(|p| p.at.clone()))
        else {
            return;
        };
        let Some((gateway, book)) = s
            .src
            .with_untracked(|src| src.gateway.clone().map(|g| (g, src.book)))
        else {
            return;
        };
        self.patch_src(|src| {
            src.loading = true;
            src.needs_internet = false;
        });
        let this = self.clone();
        spawn_local(async move {
            let request = Catalog {
                at,
                gateway: gateway.gateway.clone(),
                book,
            };
            match this.runtime().api().call(&request).await.map_err(api_error) {
                Ok(view) => this.patch_src(|src| src.rows_read(&gateway, book, view.rows)),
                Err(ApiError::NeedsInternet(_)) => this.ask_again(Pending::Catalog),
                Err(error) => {
                    let reason = said(&error);
                    this.patch_src(|src| src.rows_failed(&gateway, book, reason));
                }
            }
        });
    }

    /// "Download package" (tC4 `downloadPackage`): asks first while the internet is off.
    pub fn download_package(&self) {
        let ready = self
            .s()
            .src
            .with_untracked(|src| src.can_download() && !src.chosen().is_empty());
        if ready {
            self.internet_task(Pending::Download);
        }
    }

    pub(super) fn download_now(&self) {
        let s = self.s();
        let Some(at) = s
            .project
            .with_untracked(|p| p.as_ref().map(|p| p.at.clone()))
        else {
            return;
        };
        let Some((gateway, book, repos)) = s.src.with_untracked(|src| {
            src.gateway
                .clone()
                .filter(|_| src.dl.is_none())
                .map(|g| (g, src.book, src.chosen()))
        }) else {
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            let request = Download {
                request: js::random_request_id(),
                at: at.clone(),
                gateway: gateway.gateway.clone(),
                book,
                repos,
            };
            let job = match this.runtime().api().call(&request).await.map_err(api_error) {
                Ok(job) => job,
                Err(ApiError::NeedsInternet(_)) => return this.ask_again(Pending::Download),
                Err(error) => {
                    let reason = said(&error);
                    return this.patch_src(|src| src.error = Some(reason));
                }
            };
            this.patch_src(|src| {
                src.error = None;
                src.dl = Some(Dl::Run { job, repo: None });
            });
            let ended = this.follow_download(job).await;
            // Whatever the end, a repository may have installed before it: what a pane or a
            // help lacked may be here now, so Understand reads again (tC4 bumps
            // `installEpoch`), and step 1 counts the package.
            let installed = ChangeScope::Parts(ChangeParts {
                aspects: vec![Aspect::Pins],
                ..ChangeParts::default()
            });
            this.runtime().cache().changed(&at, &installed);
            if let Ok(view) = this.runtime().api().call(&Sources { at }).await {
                this.patch_src(|src| src.packages = Some(view));
            }
            match ended {
                Ok(()) => this.patch_src(|src| src.dl = Some(Dl::Done)),
                Err(None) => this.patch_src(|src| src.dl = None),
                Err(Some(ApiError::NeedsInternet(_))) => {
                    this.patch_src(|src| src.dl = None);
                    this.ask_again(Pending::Download);
                }
                Err(Some(error)) => {
                    let reason = said(&error);
                    this.patch_src(|src| {
                        src.dl = None;
                        src.error = Some(reason);
                    });
                }
            }
        });
    }

    /// Follows the download until it ends, showing the repository it fetches. `Err(None)`:
    /// cancelled, or ended without an outcome.
    async fn follow_download(&self, job: JobId) -> Result<(), Option<ApiError>> {
        let mut waited = 0;
        loop {
            let state = self
                .s()
                .server
                .jobs
                .with_untracked(|jobs| jobs.get(&job).map(|view| view.state.clone()));
            match state {
                Some(JobState::Done(_)) => return Ok(()),
                Some(JobState::Failed(error)) => return Err(Some(error)),
                Some(JobState::Cancelled) => return Err(None),
                Some(JobState::Running(progress)) => {
                    if let Phase::Install { repo } = progress.phase {
                        self.patch_src(|src| {
                            if let Some(Dl::Run {
                                job: running,
                                repo: shown,
                            }) = &mut src.dl
                                && *running == job
                            {
                                *shown = Some(repo);
                            }
                        });
                    }
                }
                Some(JobState::Queued | JobState::Cancelling) | None => {}
            }
            js::sleep(50).await;
            waited += 50;
            if waited >= 1000 {
                waited = 0;
                match self.runtime().api().call(&tc_api::jobs::Get { job }).await {
                    Ok(Some(view)) => {
                        self.s()
                            .server
                            .jobs
                            .try_update(|jobs| crate::app::effects::merge_job(jobs, view));
                    }
                    Ok(None) => return Err(None),
                    Err(_) => {}
                }
            }
        }
    }

    /// Cancel while a download runs: a declared difference, since tC4 has none (B9a). What was
    /// not finished is not installed.
    pub fn cancel_download(&self) {
        let Some(job) = self.s().src.with_untracked(|src| match src.dl {
            Some(Dl::Run { job, .. }) => Some(job),
            _ => None,
        }) else {
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            let cancel = Cancel {
                request: js::random_request_id(),
                job,
            };
            let _ = this.runtime().api().call(&cancel).await;
        });
    }
}
