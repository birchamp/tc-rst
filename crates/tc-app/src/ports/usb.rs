//! The release exchange port (E2): tc-sync's export and receive (STORAGE §10) on the
//! project's own tc-store handle, held open through a side session for the job's length.

use std::path::{Path, PathBuf};

use tc_api::door43::ProjectName;
use tc_api::error::{ApiError, Feature};
use tc_api::exchange::{
    ExchangeFile, ExportedView, HandEditOutcome, HandEdited, IdentifyView, ReceivedView,
};
use tc_api::ids::ProjectRef;
use tc_api::jobs::{ExchangePhase, Phase};
use tc_api::refusal::{ExchangeRefusal, Refusal};
use tc_sync::exchange::ExchangeError;

use super::ExchangePort;
use crate::app::App;
use crate::backend::BoxFuture;
use crate::jobs::{JobEnd, ProgressSink};
use crate::uploads::HeldUpload;

pub struct UsbExchange;

/// tc-sync's refusals as the screens name them (`tc_api::refusal::ExchangeRefusal`). An
/// export's I/O and store errors say the file was not saved; a receive's say it was not read.
fn refusal(error: ExchangeError, export: bool) -> ExchangeRefusal {
    let reason = error.to_string();
    match error {
        ExchangeError::Damaged(_) => ExchangeRefusal::Damaged,
        ExchangeError::OtherProject => ExchangeRefusal::OtherProject {
            name: String::new(),
        },
        ExchangeError::NeedsNewerApp { major } => ExchangeRefusal::NewerFormat {
            format: major.parse().unwrap_or(u16::MAX),
        },
        ExchangeError::NotVerified(_) => ExchangeRefusal::VerifyFailed,
        ExchangeError::PartlyReceived(reason) => ExchangeRefusal::PartlyReceived { reason },
        ExchangeError::NoHistory
        | ExchangeError::NotCommitted(_)
        | ExchangeError::Io { .. }
        | ExchangeError::Zip { .. }
        | ExchangeError::Git(_)
        | ExchangeError::Stopped(_)
        | ExchangeError::Create(_)
        | ExchangeError::Store(_) => {
            if export {
                ExchangeRefusal::NotWritten { reason }
            } else {
                ExchangeRefusal::Unreadable { reason }
            }
        }
    }
}

fn refused(error: ExchangeError, export: bool) -> JobEnd {
    JobEnd::Failed(ApiError::Refused(Refusal::Exchange(refusal(error, export))))
}

fn unavailable() -> JobEnd {
    JobEnd::Failed(ApiError::Unavailable(Feature::Exchange))
}

impl App {
    /// The file on this computer an [`ExchangeFile`] names, and the upload held while it is
    /// read. A file the shell chose stays where it is.
    fn exchange_file(
        &self,
        file: &ExchangeFile,
    ) -> Result<(PathBuf, Option<HeldUpload>), ApiError> {
        match file {
            ExchangeFile::Path(path) => Ok((PathBuf::from(path), None)),
            ExchangeFile::Upload(id) => {
                let held = self.hold_upload(id)?;
                Ok((held.path().to_owned(), Some(held)))
            }
        }
    }

    /// The name the exchange file carries for its sender: the Door43 username when signed in.
    fn exchange_person(&self) -> String {
        self.held_session()
            .map(|session| session.username.as_str().to_owned())
            .unwrap_or_else(|| "translationCore".to_owned())
    }
}

impl ExchangePort for UsbExchange {
    fn export(
        &self,
        app: App,
        project: ProjectRef,
        destination: PathBuf,
    ) -> BoxFuture<Result<ExportedView, JobEnd>> {
        Box::pin(async move {
            let progress = ProgressSink::new(Phase::Exchange(ExchangePhase::Checkpoint));
            let lease = app
                .sessions()
                .open_side(&project, progress)
                .await
                .map_err(|e| JobEnd::Failed(crate::wire::error::open_failure(e)))?;
            let handle = lease
                .session()
                .handle()
                .store_handle()
                .ok_or_else(unavailable)?;
            let person = app.exchange_person();
            let written = tc_sync::exchange::export(&handle, &destination, &person)
                .await
                .map_err(|e| refused(e, true));
            // tc-store's drop may block (the last owner closes the project): never on a worker.
            let _ = tokio::task::spawn_blocking(move || drop(handle)).await;
            drop(lease);
            app.sessions().settled(&project).await;
            Ok(ExportedView {
                path: written?.to_string_lossy().into_owned(),
            })
        })
    }

    fn identify(&self, app: App, file: ExchangeFile) -> BoxFuture<Result<IdentifyView, ApiError>> {
        Box::pin(async move {
            let (path, _held) = app.exchange_file(&file)?;
            let found = tokio::task::spawn_blocking(move || tc_sync::exchange::identify(&path))
                .await
                .map_err(|e| {
                    ApiError::Refused(Refusal::Exchange(ExchangeRefusal::Unreadable {
                        reason: e.to_string(),
                    }))
                })?
                .map_err(|e| ApiError::Refused(Refusal::Exchange(refusal(e, false))))?;
            let listing = app.with_store(|store| store.list()).await?;
            let project = listing
                .map_err(|e| {
                    ApiError::Refused(Refusal::Exchange(ExchangeRefusal::Unreadable {
                        reason: e.to_string(),
                    }))
                })?
                .projects
                .into_iter()
                .find(|entry| entry.id == found.id)
                .map(|entry| entry.folder);
            Ok(IdentifyView {
                name: found.name,
                project,
            })
        })
    }

    fn receive(
        &self,
        app: App,
        project: ProjectRef,
        file: ExchangeFile,
    ) -> BoxFuture<Result<ReceivedView, JobEnd>> {
        Box::pin(async move {
            let (path, held) = app.exchange_file(&file).map_err(JobEnd::Failed)?;
            let result = receive_into(&app, &project, &path).await;
            // The upload is read once, whatever came of it.
            if let Some(held) = held {
                held.used_up();
            }
            result
        })
    }

    fn receive_new(
        &self,
        app: App,
        file: ExchangeFile,
        name: ProjectName,
    ) -> BoxFuture<Result<ProjectRef, JobEnd>> {
        Box::pin(async move {
            let (path, held) = app.exchange_file(&file).map_err(JobEnd::Failed)?;
            let folder = name.as_str().to_owned();
            let made = app
                .with_store(move |store| store.receive_new(&path, &folder))
                .await
                .map_err(JobEnd::Failed);
            if let Some(held) = held {
                held.used_up();
            }
            match made? {
                // No project.json in the file: nothing for this computer to become.
                Err(ExchangeError::OtherProject) => Err(JobEnd::Failed(ApiError::Refused(
                    Refusal::Exchange(ExchangeRefusal::NotTranslationCore),
                ))),
                Err(e) => Err(refused(e, false)),
                Ok(not_written) => {
                    app.report_not_written(
                        crate::reports::ReportOp::Import,
                        Path::new(name.as_str()),
                        &not_written,
                    );
                    let project = ProjectRef::parse(name.as_str()).map_err(|_| {
                        JobEnd::Failed(ApiError::BadRequest {
                            field: "name".to_owned(),
                            reason: "not a folder name".to_owned(),
                        })
                    })?;
                    app.inner.bus.publish(tc_api::event::Event::ProjectsChanged);
                    Ok(project)
                }
            }
        })
    }
}

async fn receive_into(
    app: &App,
    project: &ProjectRef,
    path: &Path,
) -> Result<ReceivedView, JobEnd> {
    let from = file_stem(path);
    let progress = ProgressSink::new(Phase::Exchange(ExchangePhase::Read));
    let lease = app
        .sessions()
        .open_side(project, progress)
        .await
        .map_err(|e| JobEnd::Failed(crate::wire::error::open_failure(e)))?;
    let handle = lease
        .session()
        .handle()
        .store_handle()
        .ok_or_else(unavailable)?;
    let report = tc_sync::exchange::receive(&handle, path, &from).await;
    // tc-store's drop may block (the last owner closes the project): never on a worker.
    tokio::task::spawn_blocking(move || drop(handle))
        .await
        .map_err(|e| {
            JobEnd::Failed(app.internal(
                "the exchange handle was not released",
                crate::reports::facts(serde_json::json!({ "reason": e.to_string() })),
            ))
        })?;
    let view = match report {
        Ok(report) => {
            let session = lease.session();
            let me = session.handle().facts().device;
            let resources = &app.inner.resources;
            let mut view =
                crate::sync::synced_view(&session.read(), me, &report.received_ids, 0, &|pin| {
                    resources.has_dcs(pin)
                });
            view.hand_edited = report
                .hand_edited
                .into_iter()
                .map(|path| HandEdited {
                    path,
                    outcome: HandEditOutcome::NotTakenIn,
                })
                .collect();
            Ok(view)
        }
        // The refusal names the file's project, as the dialog does before a receive.
        Err(ExchangeError::OtherProject) => {
            let name = tokio::task::spawn_blocking({
                let path = path.to_owned();
                move || tc_sync::exchange::identify(&path).map(|i| i.name)
            })
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
            Err(JobEnd::Failed(ApiError::Refused(Refusal::Exchange(
                ExchangeRefusal::OtherProject { name },
            ))))
        }
        Err(e) => Err(refused(e, false)),
    };
    // The side session ends and the card reads the project again, whatever came of it.
    drop(lease);
    app.sessions().settled(project).await;
    app.inner.bus.publish(tc_api::event::Event::HomeCard {
        project: project.clone(),
    });
    view
}

/// The file's name without its extension: what the checkpoint names as the sender when the
/// person chose a file by hand.
fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}
