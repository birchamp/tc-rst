//! Save work for a teammate and Receive work from a file (E2): the dialogs' actions, the
//! shell's pickers or a browser upload, and the `exchange/*` jobs followed to their end.

use leptos::prelude::*;
use leptos::task::spawn_local;
use tc_api::exchange::{ExchangeFile, Export, Identify, Receive};
use tc_api::home::HomeCard;
use tc_api::jobs::{Phase, Progress};
use tc_api::upload::UploadFlow;

use super::app_ctx::{Actions, Dialog};
use super::{bridge, js};
use crate::app::client::CallError;
use crate::app::exchange::{ExchangeProject, ExportForm, ReceiveForm, fits_upload};
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
        let s = self.s();
        s.ex.try_set(Some(ExportForm::open(
            Self::exchange_project(card),
            js::random_request_id(),
        )));
        s.layers.dialog.try_set(Some(Dialog::SaveForTeammate));
    }

    pub fn patch_ex(&self, patch: impl FnOnce(&mut ExportForm)) {
        self.s().ex.try_update(|ex| {
            if let Some(ex) = ex {
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
            .ex
            .try_update(|ex| {
                let ex = ex.as_mut()?;
                let project = ex.project.project.clone();
                ex.press()
                    .map(|(request, destination)| (project, request, destination))
            })
            .flatten();
        let Some((project, request, destination)) = pressed else {
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            let request = Export {
                request,
                project,
                destination,
            };
            let ended = match this.runtime().api().call(&request).await {
                Ok(job) => {
                    this.follow::<Export>(
                        job,
                        |progress| {
                            if let Some(phase) = phase_of(progress) {
                                this.patch_ex(|ex| ex.progress(phase));
                            }
                        },
                        || this.s().ex.with_untracked(Option::is_some),
                    )
                    .await
                }
                Err(error) => JobEnd::Failed(api_error(error)),
            };
            this.patch_ex(|ex| ex.ended(ended, js::random_request_id()));
        });
    }

    /// "Receive work from a file" on a Home card.
    pub fn open_receive_work(&self, card: &HomeCard) {
        let s = self.s();
        s.rx.try_set(Some(ReceiveForm::open(
            Self::exchange_project(card),
            js::random_request_id(),
        )));
        s.layers.dialog.try_set(Some(Dialog::ReceiveWork));
    }

    pub fn patch_rx(&self, patch: impl FnOnce(&mut ReceiveForm)) {
        self.s().rx.try_update(|rx| {
            if let Some(rx) = rx {
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
        let this = self.clone();
        spawn_local(async move {
            match this
                .runtime()
                .api()
                .upload(&file, UploadFlow::Exchange)
                .await
            {
                Ok(upload) => this.receive_identify(ExchangeFile::Upload(upload.id)),
                Err(error) => this.patch_rx(|rx| rx.identified(Err(api_error(error)))),
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
                .call(&Identify { file })
                .await
                .map_err(api_error);
            this.patch_rx(|rx| rx.identified(answer));
        });
    }

    /// "Receive": the receive job, followed to its end.
    pub fn receive_work(&self) {
        let pressed = self
            .s()
            .rx
            .try_update(|rx| {
                let rx = rx.as_mut()?;
                let project = rx.project.project.clone();
                rx.press().map(|(request, file)| (project, request, file))
            })
            .flatten();
        let Some((project, request, file)) = pressed else {
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            let request = Receive {
                request,
                project,
                file,
            };
            let ended = match this.runtime().api().call(&request).await {
                Ok(job) => {
                    this.follow::<Receive>(
                        job,
                        |progress| {
                            if let Some(phase) = phase_of(progress) {
                                this.patch_rx(|rx| rx.progress(phase));
                            }
                        },
                        || this.s().rx.with_untracked(Option::is_some),
                    )
                    .await
                }
                Err(error) => JobEnd::Failed(api_error(error)),
            };
            this.patch_rx(|rx| rx.ended(ended, js::random_request_id()));
        });
    }
}
