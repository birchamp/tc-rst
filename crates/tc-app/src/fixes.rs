//! The guided fix (unit C8; tC4 `openFix`, `fixFetch`, `fixSideload`, `fixRepin`, D72 point 6).
//!
//! A check tool whose pinned resource is not on this computer never blocks other work. The fix
//! names the pin and gives three ways to get it: download exactly that commit, move the pin to
//! another commit of the same repository that is installed, or install it from a file that
//! declares exactly that commit. Asking writes nothing. Downloading and the file install the
//! resource and change no pin. Moving the pin is planned and confirmed like a release upgrade,
//! with the counts of the decisions it carries over.

use std::path::PathBuf;
use std::sync::Arc;

use tc_api::ApiError;
use tc_api::endpoint::NetKind;
use tc_api::ids::{JobId, UploadId};
use tc_api::jobs::{JobKind, JobOutcome, Phase};
use tc_api::refusal::{Refusal, ResourceRefusal};
use tc_api::resources::{FixFetch, FixRepin, FixSideload, FixState, FixView, PinLabel, PlanView};
use tc_check::fix::{self, GuidedFix, Installed, Machine};
use tc_check::{Coverage, Ladder, ResourceRef};
use tc_door43::Door43Error;
use tc_resources::updates::{RepoUpgrade, SetOffer};
use tc_resources::{
    InstallError, Packaging, ProjectPins, ResourcePin, install_dcs_watched, install_dcs_zip,
};

use crate::app::App;
use crate::dispatch::Handle;
use crate::jobs::{JobEnd, JobSpec};
use crate::plans::{core_tool, core_unit};
use crate::resources::JobWatch;
use crate::session::ProjectSession;
use crate::upgrades::{Recipe, api_rung};

/// A file the person chose, stored by the server. The registry of uploads is the server's; the
/// app reads a file through this seam and owns it, and deletes it, once it has taken it.
pub trait UploadedFiles: Send + Sync {
    fn take(&self, id: &UploadId) -> Option<UploadedFile>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadedFile {
    /// The name the person's file had: a label, never a path.
    pub name: String,
    pub path: PathBuf,
}

/// The most a sideloaded export may unpack to (the bundled store's cap).
const MAX_UNPACKED: u64 = 1 << 30;

fn fix_failed(reason: impl ToString) -> ApiError {
    ApiError::Refused(Refusal::Resources(ResourceRefusal::FixFailed {
        reason: reason.to_string(),
    }))
}

fn installed_of(pin: &ResourcePin) -> Installed {
    Installed {
        resource: pin.resource(),
        version: pin.version.as_ref().map(ToString::to_string),
        coverage: pin
            .books
            .as_ref()
            .map_or(Coverage::Unrecorded, tc_resources::BookCoverage::coverage),
    }
}

fn label(pin: &tc_check::Pin) -> PinLabel {
    PinLabel {
        repo_path: pin.resource.repo_path.to_string(),
        version: pin.version.clone(),
        sha: pin.resource.sha.to_hex(),
    }
}

/// What a fix is about: the pin to get and the rung that pins it, with what is installed.
struct Found {
    pins: ProjectPins,
    fix: GuidedFix,
    installed: Vec<ResourcePin>,
    /// Why the installed copies could not be read.
    installed_error: Option<String>,
}

impl App {
    async fn fix_found(
        &self,
        session: &ProjectSession,
        tool: tc_api::check::Tool,
        unit: tc_api::check::CheckUnit,
    ) -> Result<Option<Found>, ApiError> {
        let Some(pins) = crate::resources::project_pins(&session.read())? else {
            return Ok(None);
        };
        let (installed, installed_error) = match self.installed_pins().await {
            Ok(installed) => (installed, None),
            Err(reason) => (Vec::new(), Some(reason)),
        };
        let on_machine: Vec<Installed> = installed.iter().map(installed_of).collect();
        let machine = Machine {
            installed: &on_machine,
            online: self.inner.internet.is_on(),
        };
        let ladder: &dyn Ladder = &pins;
        let preflight = fix::preflight(Some(ladder), core_tool(tool), core_unit(unit), &machine);
        // A tool that can open has nothing to fix.
        if preflight.state == fix::PreflightState::Ready {
            return Ok(None);
        }
        let Some(fix) = fix::guided_fix(ladder, &preflight, &machine) else {
            return Ok(None);
        };
        Ok(Some(Found {
            pins,
            fix,
            installed,
            installed_error,
        }))
    }
}

/// The pin to get as the project holds it, with its flavor; the journal's pin when no pin of
/// the project is the same commit.
fn pin_to_get(found: &Found) -> Option<ResourcePin> {
    found
        .pins
        .all_pins()
        .into_iter()
        .find(|p| p.resource().same_resource(&found.fix.pin.resource))
        .cloned()
        .or_else(|| {
            let resource: &ResourceRef = &found.fix.pin.resource;
            let mut pin = ResourcePin::new(resource.repo_path.parse().ok()?, resource.sha);
            if let Some(version) = &found.fix.pin.version {
                pin = pin.with_version(version.parse().ok()?);
            }
            Some(pin)
        })
}

impl Handle<FixState> for App {
    async fn handle(&self, req: FixState) -> Result<Option<FixView>, ApiError> {
        let session = self.read(&req.at)?;
        let Some(found) = self.fix_found(&session, req.tool, req.unit).await? else {
            return Ok(None);
        };
        Ok(Some(FixView {
            pin: label(&found.fix.pin),
            rung: api_rung(found.fix.rung),
            candidates: found
                .fix
                .candidates
                .iter()
                .map(|c| {
                    label(&tc_check::Pin {
                        resource: c.resource.clone(),
                        version: c.version.clone(),
                        coverage: c.coverage.clone(),
                    })
                })
                .collect(),
            candidates_error: found.installed_error,
        }))
    }
}

impl Handle<FixFetch> for App {
    async fn handle(&self, req: FixFetch) -> Result<JobId, ApiError> {
        let session = self.read(&req.at)?;
        let client = self.resources_client()?;
        let found = self
            .fix_found(&session, req.tool, req.unit)
            .await?
            .ok_or_else(|| fix_failed("there is nothing to get for this tool"))?;
        let pin = pin_to_get(&found).ok_or_else(|| fix_failed("the pin names no repository"))?;
        let spec = JobSpec::new(
            JobKind::FixFetch,
            Phase::Install {
                repo: pin.repo.repo().name.to_string(),
            },
        )
        .project(session.scope().project.clone());
        let app = self.clone();
        Ok(self.inner.jobs.start_async(spec, move |ctx| async move {
            let phase = Phase::Install {
                repo: pin.repo.repo().name.to_string(),
            };
            let watch = JobWatch {
                progress: ctx.progress.clone(),
                cancel: ctx.cancel.clone(),
                phase,
            };
            // The download is checked against the pinned commit before it is installed.
            match install_dcs_watched(
                &app.inner.resources,
                &client,
                &pin,
                Packaging::for_pin(&pin),
                &watch,
            )
            .await
            {
                Ok(_) => {}
                Err(InstallError::Door43(Door43Error::Cancelled { .. })) => {
                    return Err(JobEnd::Cancelled);
                }
                Err(InstallError::Door43(Door43Error::Offline { .. })) => {
                    return Err(JobEnd::Failed(ApiError::NeedsInternet(NetKind::Fix)));
                }
                Err(e) => return Err(JobEnd::Failed(fix_failed(e))),
            }
            // What was missing is here now: the book's panes and helps read again.
            session.books().forget().await;
            Ok(JobOutcome::Finished)
        }))
    }
}

impl Handle<FixSideload> for App {
    async fn handle(&self, req: FixSideload) -> Result<(), ApiError> {
        let session = self.read(&req.at)?;
        let found = self
            .fix_found(&session, req.tool, req.unit)
            .await?
            .ok_or_else(|| fix_failed("there is nothing to get for this tool"))?;
        let pin = pin_to_get(&found).ok_or_else(|| fix_failed("the pin names no repository"))?;
        let file = self
            .inner
            .uploads
            .get()
            .and_then(|uploads| uploads.take(&req.upload))
            .ok_or_else(|| fix_failed("the file is not here any more; choose it again"))?;
        let store = self.inner.resources.clone();
        let path = file.path.clone();
        let installed = tokio::task::spawn_blocking(move || {
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            // Installed only when the file declares exactly the pinned commit (tC4
            // `verifySideload`, D58); anything else installs nothing.
            let result =
                install_dcs_zip(&store, &pin, &bytes, MAX_UNPACKED).map_err(|e| e.to_string());
            // The file is the app's to delete once taken.
            let _ = std::fs::remove_file(&path);
            result.map(|_| ())
        })
        .await
        .map_err(|stopped| fix_failed(crate::jobs::stop_reason(stopped)))?;
        installed.map_err(fix_failed)?;
        session.books().forget().await;
        Ok(())
    }
}

impl Handle<FixRepin> for App {
    async fn handle(&self, req: FixRepin) -> Result<PlanView, ApiError> {
        let session = self.read(&req.at)?;
        let found = self
            .fix_found(&session, req.tool, req.unit)
            .await?
            .ok_or_else(|| fix_failed("there is nothing to move"))?;
        let rung = found.fix.rung;
        let missing =
            pin_to_get(&found).ok_or_else(|| fix_failed("the pin names no repository"))?;
        let candidate = found
            .installed
            .iter()
            .find(|p| {
                p.resource().sha.to_hex() == req.sha
                    && p.resource()
                        .repo_path
                        .eq_ignore_ascii_case(&found.fix.pin.resource.repo_path)
            })
            .cloned()
            .ok_or_else(|| fix_failed("that version is not installed here"))?;
        let set = found
            .pins
            .set(rung)
            .ok_or_else(|| fix_failed("the project pins no such set"))?;
        // Every slot of the rung that pins the missing commit moves with it.
        let (from, slots) = set
            .help_repos()
            .into_iter()
            .find(|(pin, _)| pin.same_identity(&missing))
            .ok_or_else(|| fix_failed("the project no longer pins that version"))?;
        let offer = SetOffer {
            rung,
            upgrades: vec![RepoUpgrade {
                slots,
                from,
                to: candidate,
                published: None,
            }],
            current: Vec::new(),
            unanswered: Vec::new(),
        };
        self.plan_recipe(&session, Recipe::Repin(offer), None)
            .await?
            .ok_or_else(|| fix_failed("the plan was stopped"))
    }
}

/// What the app keeps of the server's uploads: nothing until the server gives them.
pub type Uploads = Arc<dyn UploadedFiles>;
