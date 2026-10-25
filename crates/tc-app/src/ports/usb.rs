//! The release exchange port (E2): tc-sync's export and receive (STORAGE §10) on the
//! project's own tc-store handle, held open through a side session for the job's length.

use std::path::{Path, PathBuf};

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
use crate::sync::ConflictTally;

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
        | ExchangeError::Io(_)
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
    /// The file on this computer an [`ExchangeFile`] names.
    fn exchange_path(&self, file: &ExchangeFile) -> PathBuf {
        match file {
            ExchangeFile::Path(path) => PathBuf::from(path),
            ExchangeFile::Upload(id) => self.upload_path(id),
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
            drop(handle);
            drop(lease);
            app.sessions().settled(&project).await;
            Ok(ExportedView {
                path: written?.to_string_lossy().into_owned(),
            })
        })
    }

    fn identify(&self, app: App, file: ExchangeFile) -> BoxFuture<Result<IdentifyView, ApiError>> {
        Box::pin(async move {
            let path = app.exchange_path(&file);
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
            let path = app.exchange_path(&file);
            let from = file_stem(&path);
            let progress = ProgressSink::new(Phase::Exchange(ExchangePhase::Read));
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
            let report = tc_sync::exchange::receive(&handle, &path, &from)
                .await
                .map_err(|e| refused(e, false));
            drop(handle);
            let view = report.map(|report| {
                let session = lease.session();
                let snapshot = session.read();
                let tally = ConflictTally::of(snapshot.conflict_counts());
                ReceivedView {
                    received: report.received_by.values().sum(),
                    from: report
                        .received_by
                        .keys()
                        .map(crate::wire::scope::person_label)
                        .collect(),
                    kept_newest: tally.others,
                    verse_conflicts: tally.verse,
                    alignments: tally.alignments,
                    hand_edited: report
                        .hand_edited
                        .into_iter()
                        .map(|path| HandEdited {
                            path,
                            outcome: HandEditOutcome::NotTakenIn,
                        })
                        .collect(),
                }
            });
            drop(lease);
            app.sessions().settled(&project).await;
            app.inner
                .bus
                .publish(tc_api::event::Event::HomeCard { project });
            if let ExchangeFile::Upload(id) = &file {
                app.remove_upload(id);
            }
            view
        })
    }
}

/// The file's name without its extension: what the checkpoint names as the sender when the
/// person chose a file by hand.
fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}
