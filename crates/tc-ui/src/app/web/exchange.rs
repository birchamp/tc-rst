//! Save work for a teammate and Receive work from a file (E2): the dialogs' actions, the
//! shell's pickers or a browser upload, and the `exchange/*` jobs followed to their end.

use leptos::prelude::*;
use leptos::task::spawn_local;
use tc_api::exchange::{ExchangeFile, Export, Identify, Receive, ReceiveNew};
use tc_api::home::HomeCard;
use tc_api::ids::JobId;
use tc_api::jobs::{Phase, Progress};
use tc_api::upload::UploadFlow;

use super::app_ctx::{Actions, Dialog};
use super::{bridge, js};
use crate::app::client::CallError;
use crate::app::exchange::{
    ExchangeProject, ExportForm, ReceiveForm, ReceivePress, ReceiveStep, fits_upload,
};
use crate::app::jobs::JobEnd;

fn phase_of(progress: &Progress) -> Option<tc_api::jobs::ExchangePhase> {
    match progress.phase {
        Phase::Exchange(phase) => Some(phase),
        _ => None,
    }
}

/// A call's failure as the dialog says it: the server's error; no usable answer reads as
/// unexpected, and the next press is a new attempt.
fn api_error(error: CallError) -> tc_api::ApiError {
    match error {
        CallError::Api(error) => error,
        CallError::Dropped | CallError::Transport(_) => tc_api::ApiError::Internal {
            report: tc_api::ids::ReportId::new(0),
        },
    }
}

impl Actions {
    /// Whether this computer can save work for a teammate: the shell's folder picker.
    pub fn can_save_for_teammate(&self) -> bool {
        bridge::has_choose_folder()
    }

    fn exchange_project(card: &HomeCard) -> ExchangeProject {
        ExchangeProject {
            project: card.project.clone(),
            name: card.name.clone(),
        }
    }

    /// "Save work for a teammate" on a Home card.
    pub fn open_save_for_teammate(&self, card: &HomeCard) {
        let form = ExportForm::open(Self::exchange_project(card), js::random_request_id());
        self.s()
            .layers
            .dialog
            .try_set(Some(Dialog::SaveForTeammate(form)));
    }

    pub fn patch_ex(&self, patch: impl FnOnce(&mut ExportForm)) {
        self.s().layers.dialog.try_update(|dialog| {
            if let Some(Dialog::SaveForTeammate(ex)) = dialog {
                patch(ex);
            }
        });
    }

    /// "Choose a folder…": the shell's picker.
    pub fn save_choose_folder(&self) {
        let this = self.clone();
        spawn_local(async move {
            let chosen = bridge::choose_folder().await;
            this.patch_ex(|ex| ex.chose_folder(chosen, js::random_request_id()));
        });
    }

    /// "Save the file": the export job, followed to its end.
    pub fn save_for_teammate(&self) {
        let pressed = self
            .s()
            .layers
            .dialog
            .try_update(|dialog| {
                let Some(Dialog::SaveForTeammate(ex)) = dialog else {
                    return None;
                };
                let project = ex.project.project.clone();
                ex.press()
                    .map(|(request, destination)| (project, request, destination))
            })
            .flatten();
        let Some((project, attempt, destination)) = pressed else {
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            let request = Export {
                request: attempt,
                project,
                destination,
            };
            let ended = match this.runtime().api().call(&request).await {
                Err(CallError::Dropped | CallError::Transport(_)) => {
                    this.patch_ex(|ex| ex.reply_lost(attempt));
                    return;
                }
                Ok(job) => {
                    this.follow::<Export>(
                        job,
                        |progress| {
                            if let Some(phase) = phase_of(progress) {
                                this.patch_ex(|ex| ex.progress(attempt, phase));
                            }
                        },
                        || this.s().export_form().with_untracked(Option::is_some),
                    )
                    .await
                }
                Err(error) => JobEnd::Failed(api_error(error)),
            };
            this.patch_ex(|ex| ex.ended(attempt, ended, js::random_request_id()));
        });
    }

    /// "Receive work from a file" on a Home card.
    pub fn open_receive_work(&self, card: &HomeCard) {
        self.open_receive(Some(Self::exchange_project(card)));
    }

    /// "Receive work from a file" from Add a project: the file says which project it holds,
    /// and one this computer lacks is added (E2 step 4b).
    pub fn open_receive_work_new(&self) {
        self.open_receive(None);
    }

    /// A receive whose dialog was closed while it ran shows its end when a Receive dialog
    /// opens next; else a fresh form.
    fn open_receive(&self, project: Option<ExchangeProject>) {
        let s = self.s();
        let kept = s.rx_last.try_update(Option::take).flatten();
        let form = match kept {
            Some(last)
                if last.project.as_ref().map(|p| &p.project)
                    == project.as_ref().map(|p| &p.project) =>
            {
                last
            }
            _ => ReceiveForm::open(project, js::random_request_id()),
        };
        s.layers.dialog.try_set(Some(Dialog::ReceiveWork(form)));
    }

    pub fn type_new_project_name(&self, name: String) {
        self.patch_rx(|rx| rx.type_new_name(name, js::random_request_id()));
    }

    /// A report for the receive form, or for the kept form once the dialog closed: the end
    /// of a receive is never lost to the person (E2 step 4b).
    fn patch_rx_or_last(&self, patch: impl FnOnce(&mut ReceiveForm)) {
        let s = self.s();
        if s.receive_form().with_untracked(Option::is_some) {
            self.patch_rx(patch);
        } else {
            s.rx_last.try_update(|last| {
                if let Some(last) = last {
                    patch(last);
                }
            });
        }
    }

    pub fn patch_rx(&self, patch: impl FnOnce(&mut ReceiveForm)) {
        self.s().layers.dialog.try_update(|dialog| {
            if let Some(Dialog::ReceiveWork(rx)) = dialog {
                patch(rx);
            }
        });
    }

    /// Whether the shell picks the file; else the dialog shows a file input.
    pub fn receive_picks_with_shell(&self) -> bool {
        bridge::has_choose_file()
    }

    /// "Choose a file…" in the shell.
    pub fn receive_choose_file(&self) {
        let this = self.clone();
        spawn_local(async move {
            if let Some(path) = bridge::choose_file().await {
                this.receive_identify(ExchangeFile::Path(path));
            }
        });
    }

    /// A browser's file: sent as an upload, after the size check (A-28).
    pub fn receive_browser_file(&self, file: web_sys::File) {
        if !fits_upload(file.size()) {
            self.patch_rx(ReceiveForm::too_large);
            return;
        }
        self.patch_rx(ReceiveForm::uploading);
        let this = self.clone();
        spawn_local(async move {
            match this
                .runtime()
                .api()
                .upload(&file, UploadFlow::Exchange)
                .await
            {
                Ok(upload) => this.receive_identify(ExchangeFile::Upload(upload.id)),
                Err(error) => this.patch_rx(|rx| rx.too_large_or_failed(api_error(error))),
            }
        });
    }

    fn receive_identify(&self, file: ExchangeFile) {
        self.patch_rx(|rx| rx.chose_file(file.clone(), js::random_request_id()));
        let this = self.clone();
        spawn_local(async move {
            let answer = this
                .runtime()
                .api()
                .call(&Identify { file: file.clone() })
                .await
                .map_err(api_error);
            this.patch_rx(|rx| rx.identified(&file, answer));
        });
    }

    /// "Receive": the receive job, followed to its end, also after the dialog closed: the
    /// form is kept aside then, and the next Receive dialog shows what came of it.
    pub fn receive_work(&self) {
        let pressed = self
            .s()
            .layers
            .dialog
            .try_update(|dialog| match dialog {
                Some(Dialog::ReceiveWork(rx)) => rx.press(),
                _ => None,
            })
            .flatten();
        let Some(press) = pressed else {
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            match press {
                ReceivePress::Into(attempt, project, file) => {
                    let request = Receive {
                        request: attempt,
                        project,
                        file,
                    };
                    let ended = match this.runtime().api().call(&request).await {
                        Err(CallError::Dropped | CallError::Transport(_)) => {
                            this.patch_rx_or_last(|rx| rx.reply_lost(attempt));
                            return;
                        }
                        Ok(job) => {
                            if let Some(form) = this.s().receive_form().get_untracked() {
                                this.remember_receive_job(job, &form, None);
                            }
                            this.follow::<Receive>(
                                job,
                                |progress| {
                                    if let Some(phase) = phase_of(progress) {
                                        this.patch_rx_or_last(|rx| rx.progress(attempt, phase));
                                    }
                                },
                                || true,
                            )
                            .await
                        }
                        Err(error) => JobEnd::Failed(api_error(error)),
                    };
                    this.patch_rx_or_last(|rx| rx.ended(attempt, ended, js::random_request_id()));
                    js::session_remove(RECEIVE_JOB_KEY);
                }
                ReceivePress::New(attempt, file, name) => {
                    let request = ReceiveNew {
                        request: attempt,
                        file,
                        name,
                    };
                    let ended = match this.runtime().api().call(&request).await {
                        Err(CallError::Dropped | CallError::Transport(_)) => {
                            this.patch_rx_or_last(|rx| rx.reply_lost(attempt));
                            return;
                        }
                        Ok(job) => {
                            if let Some(form) = this.s().receive_form().get_untracked() {
                                this.remember_receive_job(job, &form, Some(request.name.as_str()));
                            }
                            this.follow::<ReceiveNew>(
                                job,
                                |progress| {
                                    if let Some(phase) = phase_of(progress) {
                                        this.patch_rx_or_last(|rx| rx.progress(attempt, phase));
                                    }
                                },
                                || true,
                            )
                            .await
                        }
                        Err(error) => JobEnd::Failed(api_error(error)),
                    };
                    this.patch_rx_or_last(|rx| rx.added(attempt, ended, js::random_request_id()));
                    js::session_remove(RECEIVE_JOB_KEY);
                }
            }
        });
    }

    /// The running receive, remembered across a reload: the job's outcome stays readable on
    /// the server for ten minutes, so the page follows it again and shows what came of it.
    fn remember_receive_job(&self, job: JobId, form: &ReceiveForm, new_name: Option<&str>) {
        let kept = serde_json::json!({
            "job": job,
            "request": form.submit.request(),
            "project": form.project,
            "file": form.file,
            "identified": form.identified,
            "newName": new_name,
        });
        js::session_set(RECEIVE_JOB_KEY, &kept.to_string());
    }

    /// After a reload: a receive that was running is followed to its end, and its form waits
    /// for the next Receive dialog (E2 step 4b).
    pub fn resume_receive_job(&self) {
        let Some(text) = js::session_get(RECEIVE_JOB_KEY) else {
            return;
        };
        let Ok(kept) = serde_json::from_str::<RememberedReceive>(&text) else {
            js::session_remove(RECEIVE_JOB_KEY);
            return;
        };
        let mut form = ReceiveForm::open(kept.project, kept.request);
        form.file = kept.file;
        form.identified = kept.identified;
        form.step = ReceiveStep::Working(None);
        self.s().rx_last.try_set(Some(form));
        let this = self.clone();
        let attempt = kept.request;
        spawn_local(async move {
            if kept.new_name.is_some() {
                let ended = this.follow::<ReceiveNew>(kept.job, |_| {}, || true).await;
                this.patch_rx_or_last(|rx| rx.added(attempt, ended, js::random_request_id()));
            } else {
                let ended = this.follow::<Receive>(kept.job, |_| {}, || true).await;
                this.patch_rx_or_last(|rx| rx.ended(attempt, ended, js::random_request_id()));
            }
            js::session_remove(RECEIVE_JOB_KEY);
        });
    }
}

const RECEIVE_JOB_KEY: &str = "tc4.exchange.receive";

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RememberedReceive {
    job: JobId,
    request: tc_api::ids::RequestId,
    project: Option<ExchangeProject>,
    file: Option<ExchangeFile>,
    identified: Option<tc_api::exchange::IdentifyView>,
    new_name: Option<String>,
}
