//! What the test modules share.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tc_api::event::Event;
use tc_api::ids::{JobId, ServerInstance};
use tc_api::jobs::{JobKind, JobOutcome, JobState, JobView, Phase, Progress};
use tc_app::clock::ManualClock;
use tc_app::config::{HostEnv, HostOs};
use tc_app::events::{EventBus, Next, Subscription};
use tc_app::folders::FolderCheck;
use tc_app::jobs::{JobCtx, JobEnd, JobRegistry, JobSpec};
use tc_app::reports::ReportLog;

/// A computer that says which folders exist and which a cloud client copies.
#[derive(Default)]
pub struct FakeFolders {
    pub folders: BTreeSet<PathBuf>,
    /// A path is copied when it, or an ancestor, is in this map.
    pub synced: BTreeMap<PathBuf, String>,
}

impl FolderCheck for FakeFolders {
    fn is_folder(&self, path: &Path) -> bool {
        self.folders.contains(path)
    }

    fn synced_by(&self, path: &Path) -> Option<String> {
        path.ancestors()
            .find_map(|ancestor| self.synced.get(ancestor).cloned())
    }
}

pub fn env(os: HostOs, vars: &[(&str, &str)]) -> HostEnv {
    HostEnv::new(
        os,
        vars.iter()
            .map(|(name, value)| ((*name).to_owned(), OsString::from(*value))),
    )
}

pub fn instance() -> ServerInstance {
    ServerInstance::from_bytes([7; 16])
}

pub fn progress(done: u64) -> Progress {
    Progress {
        phase: Phase::Seed,
        done,
        total: Some(1000),
        item: None,
    }
}

pub fn job_event(job: u64, state: JobState) -> Event {
    Event::Job(JobView {
        id: JobId::new(job),
        kind: JobKind::Download,
        project: None,
        state,
    })
}

/// A progress frame of a job.
pub fn running(job: u64, done: u64) -> Event {
    job_event(job, JobState::Running(progress(done)))
}

/// A job registry with its bus and report log.
pub struct Jobs {
    pub jobs: JobRegistry,
    pub bus: EventBus,
    pub reports: ReportLog,
}

pub fn jobs() -> Jobs {
    let bus = EventBus::new(instance());
    let reports = ReportLog::new();
    let jobs = JobRegistry::new(
        bus.clone(),
        reports.clone(),
        Arc::new(ManualClock::new(1_000)),
    );
    Jobs { jobs, bus, reports }
}

/// A job a test drives by hand: it hands its `JobCtx` out, and ends when the test says so.
pub struct ManualJob {
    pub id: JobId,
    pub ctx: JobCtx,
    end: tokio::sync::oneshot::Sender<Result<JobOutcome, JobEnd>>,
}

impl ManualJob {
    /// Starts the job and lets it run until it has given its context out. Never awaits a
    /// channel, which would let a paused clock jump ahead.
    pub async fn start(jobs: &JobRegistry, spec: JobSpec) -> ManualJob {
        let (ctx_tx, ctx_rx) = std::sync::mpsc::channel();
        let (end, end_rx) = tokio::sync::oneshot::channel();
        let id = jobs.start_async(spec, move |ctx| async move {
            let given = JobCtx {
                progress: ctx.progress.clone(),
                cancel: ctx.cancel.clone(),
            };
            let _ = ctx_tx.send(given);
            end_rx.await.unwrap_or(Err(JobEnd::Cancelled))
        });
        for _ in 0..100 {
            tokio::task::yield_now().await;
            if let Ok(ctx) = ctx_rx.try_recv() {
                return ManualJob { id, ctx, end };
            }
        }
        panic!("the job never started");
    }

    pub fn finish(self, result: Result<JobOutcome, JobEnd>) {
        let _ = self.end.send(result);
    }
}

/// Lets every ready task run, without moving a paused clock.
pub async fn settle() {
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
}

/// Moves a paused clock forward and lets the tasks it woke run.
pub async fn step(ms: u64) {
    tokio::time::advance(Duration::from_millis(ms)).await;
    settle().await;
}

/// The job frames a reader has, each with the time the test says it was read at.
pub fn drain_jobs(subscription: &mut Subscription, at_ms: u64, into: &mut Vec<(u64, JobState)>) {
    while let Some(next) = subscription.try_next() {
        match next {
            Next::Frame(frame) => {
                if let Event::Job(view) = frame.event {
                    into.push((at_ms, view.state));
                }
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

/// The count of a `Running` state.
pub fn done_of(state: &JobState) -> u64 {
    match state {
        JobState::Running(progress) => progress.done,
        other => panic!("not running: {other:?}"),
    }
}
