//! The one job follower (UI-1, INV-ONE-JOB-FOLLOWER): the only code that reads a job's state
//! from `server.jobs` or `jobs/get` to see it end.

use leptos::prelude::*;
use tc_api::StartsJob;
use tc_api::ids::JobId;
use tc_api::jobs::{Get as JobsGet, Progress};

use super::app_ctx::Actions;
use super::js;
use crate::app::jobs::{FollowState, JobEnd, Step, typed};

impl Actions {
    /// Follows `job`, a job of row `E`, until it ends: its frames as they come, and `jobs/get`
    /// once a second in case a frame was lost. Each report goes to `on_progress`; once
    /// `wanted` answers false the follow ends `Left`.
    pub(super) async fn follow<E: StartsJob>(
        &self,
        job: JobId,
        mut on_progress: impl FnMut(&Progress),
        wanted: impl Fn() -> bool,
    ) -> JobEnd<E::Outcome> {
        let mut state = FollowState::default();
        loop {
            if !wanted() {
                return JobEnd::Left;
            }
            let seen = self
                .s()
                .server
                .jobs
                .with_untracked(|jobs| jobs.get(&job).map(|view| view.state.clone()));
            match state.step(seen.as_ref(), 50) {
                Step::End(end) => return typed::<E>(end),
                Step::Progress(progress) => on_progress(&progress),
                Step::AskServer => {
                    // A failed call says nothing about the job: the next second asks again.
                    if let Ok(answer) = self.runtime().api().call(&JobsGet { job }).await {
                        let state_now = answer.as_ref().map(|view| view.state.clone());
                        if let Some(view) = answer {
                            self.s()
                                .server
                                .jobs
                                .try_update(|jobs| crate::app::effects::merge_job(jobs, view));
                        }
                        if let Some(Step::End(end)) = state.asked(state_now) {
                            return typed::<E>(end);
                        }
                    }
                }
                Step::Wait => {}
            }
            js::sleep(50).await;
        }
    }
}
