//! Following a job to its end (UI-1): the one rule the job follower in `web/jobs.rs` applies,
//! kept pure so it is tested on the host.

use tc_api::StartsJob;
use tc_api::error::ApiError;
use tc_api::jobs::{JobOutcome, JobState, Progress};

/// How a followed job ended for the page.
#[derive(Debug, Clone, PartialEq)]
pub enum JobEnd<O> {
    Done(O),
    Failed(ApiError),
    Cancelled,
    /// The server no longer knows the job (a new server run), or it ended with another
    /// row's outcome: it ended, and the page cannot say how.
    Lost,
    /// The page stopped wanting the answer (a newer attempt replaced it).
    Left,
}

/// What the follower does next.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Wait,
    AskServer,
    Progress(Progress),
    End(JobEnd<JobOutcome>),
}

/// The time since the server was last asked; a job is asked for once a second.
#[derive(Debug, Default)]
pub struct FollowState {
    quiet_ms: u32,
}

impl FollowState {
    /// One look at the job's state as the event stream has it, `elapsed_ms` after the last.
    pub fn step(&mut self, seen: Option<&JobState>, elapsed_ms: u32) -> Step {
        if let Some(end) = seen.and_then(final_end) {
            return Step::End(end);
        }
        self.quiet_ms = self.quiet_ms.saturating_add(elapsed_ms);
        // The stream can miss a job's end (a gap, a new server run): ask the server itself.
        if self.quiet_ms >= ASK_EVERY_MS {
            self.quiet_ms = 0;
            return Step::AskServer;
        }
        match seen {
            Some(JobState::Running(progress)) => Step::Progress(progress.clone()),
            _ => Step::Wait,
        }
    }

    /// The server's answer to `jobs/get`: `None` when it does not know the job. `Some` step
    /// when the answer ends the follow.
    pub fn asked(&mut self, answer: Option<JobState>) -> Option<Step> {
        match answer {
            None => Some(Step::End(JobEnd::Lost)),
            Some(state) => final_end(&state).map(Step::End),
        }
    }
}

const ASK_EVERY_MS: u32 = 1_000;

fn final_end(state: &JobState) -> Option<JobEnd<JobOutcome>> {
    match state {
        JobState::Done(outcome) => Some(JobEnd::Done(outcome.clone())),
        JobState::Failed(error) => Some(JobEnd::Failed(error.clone())),
        JobState::Cancelled => Some(JobEnd::Cancelled),
        JobState::Queued | JobState::Running(_) | JobState::Cancelling => None,
    }
}

/// An end with the outcome of row `E`; an outcome of another kind can only come from a server
/// of another build, which the version check reloads, so it ends `Lost`.
pub fn typed<E: StartsJob>(end: JobEnd<JobOutcome>) -> JobEnd<E::Outcome> {
    match end {
        JobEnd::Done(outcome) => E::outcome(outcome).map_or(JobEnd::Lost, JobEnd::Done),
        JobEnd::Failed(error) => JobEnd::Failed(error),
        JobEnd::Cancelled => JobEnd::Cancelled,
        JobEnd::Lost => JobEnd::Lost,
        JobEnd::Left => JobEnd::Left,
    }
}
