//! What the runtime does for each thing the event stream delivers, decided without touching
//! anything (APP-DESIGN 6.3).
//!
//! [`react`] is one exhaustive `match` with no wildcard arm: a variant that tc-api adds to
//! `Event` or `SessionEvent` does not compile here until it is handled, so a new kind of
//! frame cannot be silently ignored. The browser glue only executes the [`Effect`]s.

use std::collections::{BTreeMap, HashMap};

use tc_api::account::AccountView;
use tc_api::align::AlignSave;
use tc_api::app::OpenSession;
use tc_api::event::{
    ChangeScope, CheckpointOutcome, Event, SessionEvent, SessionMode, StartupState,
};
use tc_api::ids::{AlignerId, JobId, Scope, ServerInstance};
use tc_api::jobs::{JobState, JobView};

use super::cache::AppTopic;
use super::events::Incoming;

/// One thing the runtime does.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Read `app/hello` again and replace what it holds: the internet switch, the account,
    /// the startup state, the open sessions and the saved language.
    ReadHello,
    /// Read each job held as active with `jobs/get`. `hello` carries no jobs, and the frame
    /// that ended one may be among those a gap lost; a job left as running would leave its
    /// progress standing for ever (PERFORMANCE rule 4).
    ReadJobs,
    /// Every live read fetches again.
    ReadEverything,
    /// The server started again: every open is gone (APP-DESIGN 3.10).
    Restarted(ServerInstance),
    Topic(AppTopic),
    /// A project's data changed: the reads that depend on it fetch again.
    Changed {
        at: Scope,
        scope: ChangeScope,
    },
    SessionOpened(Scope),
    SessionClosed(Scope),
    SessionMode {
        at: Scope,
        mode: SessionMode,
    },
    Internet(bool),
    Account(AccountView),
    Startup(StartupState),
    Job(JobView),
    Checkpoint {
        at: Scope,
        outcome: CheckpointOutcome,
    },
    MaterializeBlocked {
        at: Scope,
        paths: Vec<String>,
    },
    AlignSave {
        aligner: AlignerId,
        state: AlignSave,
    },
    /// The suggestion engine's state changed: the open aligner's view is read again, because
    /// the view carries the state (a `Resync` does the same, APP-DESIGN 5.4).
    Suggest {
        at: Scope,
    },
}

/// The effects of one delivery, in the order to run them. Every delivery has at least one.
pub fn react(incoming: &Incoming) -> Vec<Effect> {
    match incoming {
        // A change the build cannot read may be a teammate's: refresh everything, and read the
        // values that only events carry.
        Incoming::Unreadable { .. } => {
            vec![Effect::ReadEverything, Effect::ReadHello, Effect::ReadJobs]
        }
        Incoming::Restart(instance) => vec![Effect::Restarted(*instance), Effect::ReadHello],
        Incoming::Frame(frame) => match &frame.event {
            Event::Resync => vec![Effect::ReadEverything, Effect::ReadHello, Effect::ReadJobs],
            Event::Internet { on } => vec![Effect::Internet(*on)],
            Event::Account(account) => vec![Effect::Account(account.clone())],
            Event::Job(job) => vec![Effect::Job(job.clone())],
            Event::ProjectsChanged => vec![Effect::Topic(AppTopic::Projects)],
            Event::HomeCard { project } => {
                vec![Effect::Topic(AppTopic::HomeCard(project.clone()))]
            }
            Event::Startup(state) => vec![Effect::Startup(state.clone())],
            Event::Session { at, event } => match event {
                // The event names the open and nothing else; the list of sessions needs the
                // project's id and role, which `hello` has.
                SessionEvent::Opened => vec![Effect::SessionOpened(at.clone()), Effect::ReadHello],
                SessionEvent::Closed => vec![Effect::SessionClosed(at.clone())],
                SessionEvent::Mode(mode) => vec![Effect::SessionMode {
                    at: at.clone(),
                    mode: *mode,
                }],
                SessionEvent::Changed { scope, .. } => vec![Effect::Changed {
                    at: at.clone(),
                    scope: scope.clone(),
                }],
                SessionEvent::Checkpoint(outcome) => vec![Effect::Checkpoint {
                    at: at.clone(),
                    outcome: outcome.clone(),
                }],
                SessionEvent::MaterializeBlocked { paths } => vec![Effect::MaterializeBlocked {
                    at: at.clone(),
                    paths: paths.clone(),
                }],
                SessionEvent::AlignSave { aligner, state } => vec![Effect::AlignSave {
                    aligner: *aligner,
                    state: state.clone(),
                }],
                SessionEvent::Suggest { .. } => vec![Effect::Suggest { at: at.clone() }],
            },
        },
    }
}

/// How many finished jobs the UI keeps. The server keeps a final outcome readable for ten
/// minutes (`jobs/get`); a screen that waits for one reads it from here.
pub const FINISHED_JOBS_KEPT: usize = 64;

/// Queued, running or being cancelled: the job still counts as work in flight.
pub fn is_active(state: &JobState) -> bool {
    match state {
        JobState::Queued | JobState::Running(_) | JobState::Cancelling => true,
        JobState::Done(_) | JobState::Failed(_) | JobState::Cancelled => false,
    }
}

/// Records a job's newest view. The map keeps every job that is active and the newest
/// [`FINISHED_JOBS_KEPT`] that are not, so a long session does not grow it without end.
pub fn merge_job(jobs: &mut BTreeMap<JobId, JobView>, view: JobView) {
    jobs.insert(view.id, view);
    let finished: Vec<JobId> = jobs
        .iter()
        .filter(|(_, job)| !is_active(&job.state))
        .map(|(id, _)| *id)
        .collect();
    let excess = finished.len().saturating_sub(FINISHED_JOBS_KEPT);
    for id in finished.into_iter().take(excess) {
        jobs.remove(&id);
    }
}

/// The jobs to read again after frames may have been lost: each one held as active, as it is
/// held now, so the reply can be told from a frame that came after the call was sent.
pub fn jobs_to_reread(jobs: &BTreeMap<JobId, JobView>) -> Vec<JobView> {
    jobs.values()
        .filter(|job| is_active(&job.state))
        .cloned()
        .collect()
}

/// Applies the reply of `jobs/get` for `held`, the view the job had when the call was sent.
///
/// - A frame came meanwhile (the job is no longer as `held`): the frame is newer than a reply
///   that may have been computed before it, so the reply is dropped.
/// - The server knows the job: its view replaces the held one.
/// - The server does not: the job ended and its outcome is older than the server keeps. Nothing
///   can show it, and it is not left active.
pub fn apply_reread(jobs: &mut BTreeMap<JobId, JobView>, held: &JobView, reply: Option<JobView>) {
    if jobs.get(&held.id) != Some(held) {
        return;
    }
    match reply {
        Some(view) => merge_job(jobs, view),
        None => {
            jobs.remove(&held.id);
        }
    }
}

/// Forgets what is held for an open that `hello` no longer lists. `hello` holds no checkpoint
/// outcome and no blocked file (only events carry them, APP-DESIGN 6.1), so a `Resync` cannot
/// restore them. It can say which opens are gone: a `Closed` frame lost in the gap would leave
/// the outcome and the files of an open that no longer exists, and nothing else removes them.
/// What is held for an open that is still listed stays: a failure is never cleared by a gap.
pub fn retain_open<V>(held: &mut HashMap<Scope, V>, sessions: &[OpenSession]) {
    held.retain(|at, _| sessions.iter().any(|session| session.at == *at));
}

/// What a new server run makes meaningless (APP-DESIGN 3.10, 5.4). The new run numbers its jobs
/// and its aligners from the start, and its opens from epoch 1, so every map keyed by one of
/// those would name another thing than the old run's entry does: a job id that the new run gives
/// to another job, an aligner whose first save state the new run has not sent yet, a failed
/// checkpoint or a blocked file of a `TIT` at epoch 1 that is not the open that failed.
/// `hello` follows a restart and lists the new run's opens; these are not in it.
///
/// Every map of `ServerState` that an event fills and that a number of the run keys goes here.
pub fn forget_run(
    jobs: &mut BTreeMap<JobId, JobView>,
    checkpoints: &mut HashMap<Scope, CheckpointOutcome>,
    blocked: &mut HashMap<Scope, Vec<String>>,
    align_saves: &mut HashMap<AlignerId, AlignSave>,
) {
    jobs.clear();
    checkpoints.clear();
    blocked.clear();
    align_saves.clear();
}

pub fn active_jobs(jobs: &BTreeMap<JobId, JobView>) -> usize {
    jobs.values().filter(|job| is_active(&job.state)).count()
}

/// The work in flight, for `window.__tcIdle` (APP-DESIGN 6.2): API calls and catalog fetches
/// (not the event stream), jobs that are active, and saves in flight or armed (the save
/// machine's `busy`, so a test that waits for idle sees a save that is due). The page is idle
/// when there is none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BusyCount {
    pub fetches: usize,
    pub jobs: usize,
    pub saves: usize,
}

impl BusyCount {
    pub fn is_idle(self) -> bool {
        self.fetches == 0 && self.jobs == 0 && self.saves == 0
    }
}
