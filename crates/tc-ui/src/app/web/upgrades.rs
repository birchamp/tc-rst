//! The actions of release upgrades and the gateway change (unit C8; tC4 `checkForUpdates`,
//! `upgradeSet`, `upgradeText`, `confirmUpgrade`, `cancelUpgrade`, `askGatewayChange`,
//! `confirmGatewayChange`, `cancelGatewayChange`). The decisions are `crate::app::upgrades`'s;
//! this sends the requests and follows the jobs.
//!
//! Opening a preview first runs the navigation gate (APP-DESIGN 6.5, A-26): every field is
//! saved, and a standing failure keeps the person where they are, so a plan is never read over
//! work that is not safe. The server plans from one snapshot, so a preview is read once; a
//! confirm that finds the project moved comes back with the plan made again, shown to be read.

use leptos::prelude::*;
use tc_api::ApiError;
use tc_api::NotSaved;
use tc_api::ids::JobId;
use tc_api::jobs::{JobOutcome, JobState, Phase};
use tc_api::resources::{
    ApplyPlan, CheckUpdates, GatewayPackage, OfferRef, PlanApplied, PlanGateway, PlanUpgrade,
    PlanView, SetRung,
};
use wasm_bindgen_futures::spawn_local;

use super::app_ctx::Actions;
use super::js;
use super::sources::{api_error, said};
use crate::app::internet::Pending;
use crate::app::upgrades::Installing;
use crate::i18n::{keys, t};

/// How a plan job ended.
enum Planned {
    Ready(Box<PlanView>),
    /// Cancelled, or gone.
    Stopped,
    Failed(ApiError),
}

/// What a failed confirm says: tC4's `upgrade.failed` wraps the stale words, and a refused disk
/// says its own reason.
fn apply_error(error: &ApiError) -> String {
    match error {
        ApiError::NotSaved(NotSaved::PlanStale) => t(
            keys::upgrade::FAILED,
            &[("reason", &t(keys::upgrade::STALE, &[]))],
        ),
        ApiError::NotSaved(NotSaved::Disk(reason)) => reason.clone(),
        other => said(other),
    }
}

impl Actions {
    fn project_scope(&self) -> Option<tc_api::Scope> {
        self.s()
            .project
            .with_untracked(|p| p.as_ref().map(|p| p.at.clone()))
    }

    // ---- J12: the release upgrades of Get source texts ---------------------------------------

    /// "Check for updates" (tC4 `checkForUpdates`): asks first while the internet is off.
    pub fn check_for_updates(&self) {
        self.internet_task(Pending::Updates);
    }

    pub(super) fn check_updates_now(&self) {
        let Some(at) = self.project_scope() else {
            return;
        };
        self.s().up.try_update(|up| up.check_started());
        let this = self.clone();
        spawn_local(async move {
            let project = at.project.clone();
            let answer = this
                .runtime()
                .api()
                .call(&CheckUpdates { at })
                .await
                .map_err(api_error);
            let current = this
                .s()
                .project
                .with_untracked(|p| p.as_ref().map(|p| p.at.project.clone()));
            match answer {
                Ok(view) => {
                    if let Some(current) = current {
                        this.s()
                            .up
                            .try_update(|up| up.offers_read(&project, &current, view));
                    }
                }
                Err(ApiError::NeedsInternet(_)) => this.ask_again(Pending::Updates),
                Err(error) => {
                    let reason = t(keys::upgrade::CHECK_FAILED, &[("reason", &said(&error))]);
                    this.s().up.try_update(|up| up.check_failed(reason));
                }
            }
        });
    }

    /// A language set's Upgrade (tC4 `upgradeSet`).
    pub fn upgrade_set(&self, rung: SetRung) {
        if self.s().up.with_untracked(|up| up.installing.is_some()) {
            return;
        }
        self.internet_task(Pending::UpgradeSet(rung));
    }

    /// A scripture text's Upgrade (tC4 `upgradeText`).
    pub fn upgrade_text(&self, repo: String) {
        if self.s().up.with_untracked(|up| up.installing.is_some()) {
            return;
        }
        self.internet_task(Pending::UpgradeText(repo));
    }

    pub(super) fn upgrade_set_now(&self, rung: SetRung) {
        self.plan_upgrade(Installing::Set(rung), OfferRef::Set { rung });
    }

    pub(super) fn upgrade_text_now(&self, repo: String) {
        self.plan_upgrade(Installing::Text(repo.clone()), OfferRef::Text { repo });
    }

    fn plan_upgrade(&self, what: Installing, offer: OfferRef) {
        let Some(at) = self.project_scope() else {
            return;
        };
        let pending = match &what {
            Installing::Set(rung) => Pending::UpgradeSet(*rung),
            Installing::Text(repo) => Pending::UpgradeText(repo.clone()),
        };
        self.s().up.try_update(|up| up.install_started(what));
        let this = self.clone();
        spawn_local(async move {
            // Saved first: the plan is read over work that is safe (A-26).
            if !this.runtime().saves().gate().await {
                // tC4's own sentence for a text upgrade that finds work unsaved.
                let reason = t(
                    keys::upgrade::FAILED,
                    &[("reason", &t(keys::upgrade::ALIGN_UNSAVED, &[]))],
                );
                this.s().up.try_update(|up| up.install_failed(Some(reason)));
                return;
            }
            let request = PlanUpgrade {
                request: js::random_request_id(),
                at: at.clone(),
                offer,
            };
            let job = match this.runtime().api().call(&request).await.map_err(api_error) {
                Ok(job) => job,
                Err(ApiError::NeedsInternet(_)) => {
                    this.s().up.try_update(|up| up.install_failed(None));
                    return this.ask_again(pending);
                }
                Err(error) => {
                    let reason = said(&error);
                    {
                        this.s().up.try_update(|up| up.install_failed(Some(reason)));
                        return;
                    }
                }
            };
            match this
                .follow_plan(job, |repo| {
                    this.s().up.try_update(|up| up.install_progress(repo));
                })
                .await
            {
                Planned::Ready(plan) => {
                    this.s().up.try_update(|up| up.plan_ready(*plan));
                }
                Planned::Stopped => {
                    this.s().up.try_update(|up| up.install_failed(None));
                }
                Planned::Failed(ApiError::NeedsInternet(_)) => {
                    this.s().up.try_update(|up| up.install_failed(None));
                    this.ask_again(pending);
                }
                Planned::Failed(error) => {
                    let reason = said(&error);
                    this.s().up.try_update(|up| up.install_failed(Some(reason)));
                }
            }
        });
    }

    /// "Keep the current release" and the ✕: nothing changes.
    pub fn cancel_upgrade(&self) {
        self.s().up.try_update(|up| up.cancel());
    }

    /// "Upgrade" (tC4 `confirmUpgrade`): the plan the person read is confirmed by its id.
    pub fn confirm_upgrade(&self) {
        let Some(at) = self.project_scope() else {
            return;
        };
        let Some(plan) = self
            .s()
            .up
            .try_update(|up| {
                if up.confirm_started() {
                    up.preview.as_ref().map(|p| p.plan.plan)
                } else {
                    None
                }
            })
            .flatten()
        else {
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            let request = ApplyPlan {
                request: js::random_request_id(),
                at: at.clone(),
                plan,
            };
            match this.runtime().api().call(&request).await.map_err(api_error) {
                Ok(PlanApplied::Applied) => {
                    this.s().up.try_update(|up| up.applied());
                    this.pins_changed(&at);
                }
                Ok(PlanApplied::Replanned(view)) => {
                    this.s().up.try_update(|up| up.replanned(*view));
                }
                Err(error) => {
                    let reason = apply_error(&error);
                    this.s().up.try_update(|up| up.apply_failed(reason));
                }
            }
        });
    }

    /// What the pins feed reads again: Understand's panes and helps, and Get source texts' step 1.
    fn pins_changed(&self, at: &tc_api::Scope) {
        let scope = tc_api::event::ChangeScope::Parts(tc_api::event::ChangeParts {
            aspects: vec![tc_api::event::Aspect::Pins],
            ..tc_api::event::ChangeParts::default()
        });
        self.runtime().cache().changed(at, &scope);
        let this = self.clone();
        let at = at.clone();
        spawn_local(async move {
            if let Ok(view) = this
                .runtime()
                .api()
                .call(&tc_api::resources::Sources { at })
                .await
            {
                this.patch_src(|src| src.packages = Some(view));
            }
        });
    }

    // ---- J13: a new checking language ------------------------------------------------------------

    /// A package was chosen as the project's checking language (tC4 `askGatewayChange`): one
    /// change at a time, saved first, planned once from one snapshot.
    pub fn ask_gateway_change(&self, package: GatewayPackage) {
        let Some(at) = self.project_scope() else {
            return;
        };
        if !self.s().gw.try_update(|gw| gw.asking()).unwrap_or(false) {
            return;
        }
        let this = self.clone();
        spawn_local(async move {
            if !this.runtime().saves().gate().await {
                let reason = t(keys::gateway::UNSAVED, &[]);
                {
                    this.s().gw.try_update(|gw| gw.plan_failed(package, reason));
                    return;
                }
            }
            let request = PlanGateway {
                request: js::random_request_id(),
                at: at.clone(),
                gateway: package.gateway.clone(),
            };
            let job = match this.runtime().api().call(&request).await.map_err(api_error) {
                Ok(job) => job,
                Err(error) => {
                    let reason = said(&error);
                    {
                        this.s().gw.try_update(|gw| gw.plan_failed(package, reason));
                        return;
                    }
                }
            };
            // The person left the project while the plan was made: it would describe one project
            // over another, so nothing is shown (tC4 #580, interruption 4).
            let same_project = |this: &Actions| this.project_scope().is_some_and(|now| now == at);
            let planned = this.follow_plan(job, |_| {}).await;
            if !same_project(&this) {
                this.s().gw.try_update(|gw| {
                    gw.busy = false;
                });
                return;
            }
            match planned {
                Planned::Ready(plan) => {
                    this.s().gw.try_update(|gw| gw.planned(package, *plan));
                }
                Planned::Stopped => {
                    this.s().gw.try_update(|gw| gw.busy = false);
                }
                Planned::Failed(error) => {
                    let reason = said(&error);
                    this.s().gw.try_update(|gw| gw.plan_failed(package, reason));
                }
            };
        });
    }

    /// "Keep …" and the ✕ (tC4 `cancelGatewayChange`): nothing changes, and nothing cancels a
    /// change that was confirmed.
    pub fn cancel_gateway_change(&self) {
        self.s().gw.try_update(|gw| gw.cancel());
    }

    /// "Change to …" (tC4 `confirmGatewayChange`).
    pub fn confirm_gateway_change(&self) {
        let Some(at) = self.project_scope() else {
            return;
        };
        let Some(plan) = self
            .s()
            .gw
            .try_update(|gw| {
                if !gw.confirm_started() {
                    return None;
                }
                match &gw.preview {
                    Some(crate::app::upgrades::GatewayPreview::Plan { plan, .. }) => {
                        Some(plan.plan)
                    }
                    _ => None,
                }
            })
            .flatten()
        else {
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            let request = ApplyPlan {
                request: js::random_request_id(),
                at: at.clone(),
                plan,
            };
            match this.runtime().api().call(&request).await.map_err(api_error) {
                Ok(PlanApplied::Applied) => {
                    this.s().gw.try_update(|gw| gw.applied());
                    this.pins_changed(&at);
                }
                Ok(PlanApplied::Replanned(view)) => {
                    this.s().gw.try_update(|gw| gw.replanned(*view));
                }
                Err(error) => {
                    let reason = apply_error(&error);
                    this.s().gw.try_update(|gw| gw.apply_failed(reason));
                }
            }
        });
    }

    // ---- the jobs ------------------------------------------------------------------------------

    /// Follows a plan job until it ends, telling `progress` which repository it downloads.
    async fn follow_plan(&self, job: JobId, progress: impl Fn(String)) -> Planned {
        let mut waited = 0;
        loop {
            let state = self
                .s()
                .server
                .jobs
                .with_untracked(|jobs| jobs.get(&job).map(|view| view.state.clone()));
            match state {
                Some(JobState::Done(JobOutcome::PlanReady(plan))) => return Planned::Ready(plan),
                Some(JobState::Done(_)) | Some(JobState::Cancelled) => return Planned::Stopped,
                Some(JobState::Failed(error)) => return Planned::Failed(error),
                Some(JobState::Running(running)) => {
                    if let Phase::Install { repo } = running.phase {
                        progress(repo);
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
                    Ok(None) => return Planned::Stopped,
                    Err(_) => {}
                }
            }
        }
    }
}
