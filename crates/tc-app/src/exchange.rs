//! USB exchange from Home (unit E2; STORAGE §10): `exchange/identify`, the `exchange/export`
//! and `exchange/receive` jobs. The exchange's own steps are the port's (`crate::ports`);
//! the app holds the project's exchange flow around each job, so one exchange of a project
//! runs at a time and never beside its sync (A-31, INV-NEVER-BESIDE-A-SYNC).

use std::path::PathBuf;

use tc_api::ApiError;
#[cfg(feature = "test-hooks")]
use tc_api::exchange::ReceivedView;
use tc_api::exchange::{Export, Identify, IdentifyView, Receive, ReceiveNew};
use tc_api::ids::JobId;
use tc_api::ids::ProjectRef;
use tc_api::jobs::{ExchangePhase, JobKind, Phase};
use tc_api::refusal::{ExchangeRefusal, Refusal};

use crate::app::App;
use crate::dispatch::Handle;
use crate::flows::FlowRequest;
#[cfg(feature = "test-hooks")]
use crate::jobs::JobEnd;
use crate::jobs::JobSpec;

impl Handle<Identify> for App {
    async fn handle(&self, req: Identify) -> Result<IdentifyView, ApiError> {
        self.inner.exchange.identify(self.clone(), req.file).await
    }
}

impl Handle<Export> for App {
    /// "Save work for a teammate": the project's exchange flow, then the job.
    async fn handle(&self, req: Export) -> Result<JobId, ApiError> {
        let pass = self
            .inner
            .flows
            .enter(FlowRequest::ExchangeExport(req.project.clone()))?;
        let spec = JobSpec::new(
            JobKind::ExchangeExport,
            Phase::Exchange(ExchangePhase::Checkpoint),
        )
        .project(req.project.clone())
        .pass(pass);
        let app = self.clone();
        Ok(self
            .inner
            .jobs
            .start_typed::<Export, _, _>(spec, move |_ctx| async move {
                app.inner.sessions.settled(&req.project).await;
                app.inner
                    .exchange
                    .export(app.clone(), req.project, PathBuf::from(req.destination))
                    .await
            }))
    }
}

impl Handle<Receive> for App {
    /// "Receive work from a file", after the person confirmed the file's project.
    async fn handle(&self, req: Receive) -> Result<JobId, ApiError> {
        let pass = self
            .inner
            .flows
            .enter(FlowRequest::ExchangeReceive(req.project.clone()))?;
        let spec = JobSpec::new(
            JobKind::ExchangeReceive,
            Phase::Exchange(ExchangePhase::Read),
        )
        .project(req.project.clone())
        .pass(pass);
        let app = self.clone();
        Ok(self
            .inner
            .jobs
            .start_typed::<Receive, _, _>(spec, move |_ctx| async move {
                app.inner.sessions.settled(&req.project).await;
                let view = app
                    .inner
                    .exchange
                    .receive(app.clone(), req.project.clone(), req.file)
                    .await?;
                // The Received sheet reads it again after a gap, as the sync's toast does
                // (`door43/lastSync`, HIST-1).
                crate::app::lock(&app.inner.last_sync).insert(req.project, view.clone());
                Ok(view)
            }))
    }
}

impl Handle<ReceiveNew> for App {
    /// "Receive work from a file" for a project this computer does not have yet: the name is
    /// checked first, as Open from Door43 checks its own, then the job, one import at a time.
    async fn handle(&self, req: ReceiveNew) -> Result<JobId, ApiError> {
        if self.project_name_in_use(&req.name).await? {
            return Err(ApiError::Refused(Refusal::Exchange(
                ExchangeRefusal::NameExists,
            )));
        }
        let pass = self.inner.flows.enter(FlowRequest::Import)?;
        let spec = JobSpec::new(
            JobKind::ExchangeReceive,
            Phase::Exchange(ExchangePhase::Read),
        )
        .project(
            ProjectRef::parse(req.name.as_str()).map_err(|_| ApiError::BadRequest {
                field: "name".to_owned(),
                reason: "not a folder name".to_owned(),
            })?,
        )
        .pass(pass);
        let app = self.clone();
        Ok(self
            .inner
            .jobs
            .start_typed::<ReceiveNew, _, _>(spec, move |_ctx| async move {
                app.inner
                    .exchange
                    .receive_new(app.clone(), req.file, req.name)
                    .await
            }))
    }
}

#[cfg(feature = "test-hooks")]
impl App {
    /// The scripted exchange port, when the app runs on one (tests and journeys).
    pub fn scripted_exchange_port(&self) -> Option<&crate::ports::scripted::ScriptedExchangePort> {
        self.inner.exchange.scripted()
    }

    /// What the scripted exchange port does for a receive a test scripted: the file's changes
    /// written through the store as the scripted sync writes a teammate's, then the project
    /// read again, so the sheet's counts come from the fold's own conflicts.
    pub(crate) async fn scripted_receive(
        &self,
        folder: tc_api::ids::ProjectRef,
        changes: tc_api::test::ScriptedSync,
        hand_edited: Vec<tc_api::exchange::HandEdited>,
    ) -> Result<ReceivedView, JobEnd> {
        let mut view = self.scripted_sync(folder, changes).await?;
        view.hand_edited = hand_edited;
        Ok(view)
    }
}
