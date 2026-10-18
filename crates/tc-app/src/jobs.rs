//! Background jobs and their progress (APP-DESIGN §3.6, §4.6, PERF rule 4, A-29).
//!
//! A job runs on a thread of the blocking pool or a task, and counts what it has done into a
//! [`ProgressSink`]. A separate task per job, the reporter, reads that sink and tells the
//! event bus, so a slow read can never hold the first report back and the working threads
//! never wait for the bus:
//!
//! - nothing is sent for the first 300 ms, so an action that ends sooner shows no indicator;
//! - from 300 ms a report goes out when the counts have moved, at most one per 100 ms, or one
//!   per second for the jobs no screen shows (`Train`, `HomeSummary`);
//! - counts never fall within a phase;
//! - the job's last count goes out before its end, and its end goes out at once (a job no
//!   screen shows sends its end alone, so it keeps to one frame a second);
//! - after `Cancelling` no `Running` frame is sent, so the UI never shows a cancelled job
//!   running again.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tc_api::error::ApiError;
use tc_api::event::Event;
use tc_api::ids::{JobId, ProjectRef};
use tc_api::jobs::{JobKind, JobOutcome, JobState, JobView, Phase, Progress};
use tokio::sync::watch;
use tokio::task::JoinError;
use tokio::time::Instant;

use crate::clock::Clock;
use crate::events::EventBus;
use crate::flows::FlowPass;
use crate::reports::{ReportLog, ReportOp, ReportOutcome, facts};

/// A job that runs longer than this shows its progress from then on (PERF rule 4).
pub const FIRST_REPORT_AFTER: Duration = Duration::from_millis(300);

/// How long a finished job's outcome stays readable through `jobs/get` (§3.6).
pub const OUTCOME_KEPT: Duration = Duration::from_secs(10 * 60);

/// Whether no screen shows the job's progress (A-29). Each frame wakes the UI thread while
/// the person types, so these jobs report once a second at most, and send only their end at
/// once.
fn is_unseen(kind: JobKind) -> bool {
    matches!(kind, JobKind::Train | JobKind::HomeSummary)
}

/// The wait between two reports of a job whose counts keep moving: 100 ms for a job a screen
/// shows, a second for one no screen shows.
pub fn report_every(kind: JobKind) -> Duration {
    if is_unseen(kind) {
        Duration::from_secs(1)
    } else {
        Duration::from_millis(100)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // No code that holds one of these locks can leave its state half changed.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

struct ProgressState {
    phase: Phase,
    done: u64,
    total: Option<u64>,
    item: Option<String>,
}

/// The counts of one job, shared between the working threads and the reporter. One lock
/// holds phase, count and total together, so a report never pairs one phase's count with
/// another phase's name.
#[derive(Clone)]
pub struct ProgressSink {
    state: Arc<Mutex<ProgressState>>,
}

impl ProgressSink {
    pub fn new(phase: Phase) -> Self {
        Self {
            state: Arc::new(Mutex::new(ProgressState {
                phase,
                done: 0,
                total: None,
                item: None,
            })),
        }
    }

    /// A new phase starts its counts again, with the total it knows (or none). The phase the
    /// job is already in keeps its count and item, and takes the total if one is given.
    /// Phases go forward: a job that returned to a phase it had left would show that phase's
    /// count falling.
    pub fn phase(&self, phase: Phase, total: Option<u64>) {
        let mut state = lock(&self.state);
        if state.phase == phase {
            if total.is_some() {
                state.total = total;
            }
        } else {
            *state = ProgressState {
                phase,
                done: 0,
                total,
                item: None,
            };
        }
    }

    pub fn total(&self, total: u64) {
        lock(&self.state).total = Some(total);
    }

    pub fn add(&self, n: u64) {
        let mut state = lock(&self.state);
        state.done = state.done.saturating_add(n);
    }

    /// The count is at least `done`. A lower value changes nothing: counts never fall within
    /// a phase.
    pub fn reach(&self, done: u64) {
        let mut state = lock(&self.state);
        state.done = state.done.max(done);
    }

    pub fn item(&self, item: Option<String>) {
        lock(&self.state).item = item;
    }

    pub fn snapshot(&self) -> Progress {
        let state = lock(&self.state);
        Progress {
            phase: state.phase.clone(),
            done: state.done,
            total: state.total,
            item: state.item.clone(),
        }
    }
}

/// Asks a job to stop at its next check. Clones share one request.
#[derive(Clone)]
pub struct CancelToken {
    asked: Arc<watch::Sender<bool>>,
}

impl Default for CancelToken {
    fn default() -> Self {
        Self {
            asked: Arc::new(watch::channel(false).0),
        }
    }
}

/// The error of [`CancelToken::check`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("cancelled")]
pub struct Cancelled;

impl CancelToken {
    pub fn cancel(&self) {
        self.asked.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.asked.borrow()
    }

    /// A job calls it between files, books and chunks: `ctx.cancel.check()?`.
    pub fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }

    /// Resolves when the job is asked to stop, for a job that waits on something else (a
    /// download waiting for Door43, A11).
    pub async fn cancelled(&self) {
        let mut asked = self.asked.subscribe();
        // The sender lives as long as this token does, so the wait cannot fail.
        let _ = asked.wait_for(|asked| *asked).await;
    }
}

/// What a job gets to work with.
pub struct JobCtx {
    pub progress: ProgressSink,
    pub cancel: CancelToken,
}

/// How a job ended when it did not finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobEnd {
    Cancelled,
    Failed(ApiError),
}

impl From<Cancelled> for JobEnd {
    fn from(Cancelled: Cancelled) -> Self {
        Self::Cancelled
    }
}

impl From<ApiError> for JobEnd {
    fn from(error: ApiError) -> Self {
        Self::Failed(error)
    }
}

pub struct JobSpec {
    pub kind: JobKind,
    pub project: Option<ProjectRef>,
    /// The phase the job starts in.
    pub phase: Phase,
    /// The one-at-a-time flow this job holds. It is freed when the job ends, however it ends.
    pub pass: Option<FlowPass>,
}

impl JobSpec {
    pub fn new(kind: JobKind, phase: Phase) -> Self {
        Self {
            kind,
            project: None,
            phase,
            pass: None,
        }
    }

    pub fn project(mut self, project: ProjectRef) -> Self {
        self.project = Some(project);
        self
    }

    pub fn pass(mut self, pass: FlowPass) -> Self {
        self.pass = Some(pass);
        self
    }
}

struct Job {
    kind: JobKind,
    project: Option<ProjectRef>,
    state: JobState,
    cancel: CancelToken,
    ended: Option<Instant>,
}

impl Job {
    fn view(&self, id: JobId) -> JobView {
        JobView {
            id,
            kind: self.kind,
            project: self.project.clone(),
            state: self.state.clone(),
        }
    }
}

struct Registry {
    jobs: HashMap<JobId, Job>,
}

struct JobsInner {
    bus: EventBus,
    reports: ReportLog,
    clock: Arc<dyn Clock>,
    next_id: AtomicU64,
    // Lock order: this one, then the bus's. The bus never calls back into the registry.
    registry: Mutex<Registry>,
}

/// The jobs of this server run.
#[derive(Clone)]
pub struct JobRegistry {
    inner: Arc<JobsInner>,
}

/// Whether a job in state `from` may go to state `to`. A terminal state is final, a job does
/// not go back to `Queued`, and nothing shows it running after it was asked to stop.
fn allowed(from: &JobState, to: &JobState) -> bool {
    !matches!(
        (from, to),
        (
            JobState::Done(_) | JobState::Failed(_) | JobState::Cancelled,
            _
        ) | (_, JobState::Queued)
            | (
                JobState::Cancelling,
                JobState::Running(_) | JobState::Cancelling
            )
    )
}

fn is_terminal(state: &JobState) -> bool {
    matches!(
        state,
        JobState::Done(_) | JobState::Failed(_) | JobState::Cancelled
    )
}

type Work = tokio::task::JoinHandle<Result<JobOutcome, JobEnd>>;

impl JobRegistry {
    pub fn new(bus: EventBus, reports: ReportLog, clock: Arc<dyn Clock>) -> Self {
        Self {
            inner: Arc::new(JobsInner {
                bus,
                reports,
                clock,
                next_id: AtomicU64::new(1),
                registry: Mutex::new(Registry {
                    jobs: HashMap::new(),
                }),
            }),
        }
    }

    /// Starts a job that blocks: reads, writes, hashing. It runs on the blocking pool, and
    /// checks `ctx.cancel` between its chunks. Must run inside a tokio runtime.
    pub fn start_blocking<F>(&self, spec: JobSpec, f: F) -> JobId
    where
        F: FnOnce(JobCtx) -> Result<JobOutcome, JobEnd> + Send + 'static,
    {
        let JobSpec {
            kind,
            project,
            phase,
            pass,
        } = spec;
        let (id, ctx) = self.register(kind, project, phase);
        let progress = ctx.progress.clone();
        let work = tokio::task::spawn_blocking(move || {
            let _pass = pass;
            f(ctx)
        });
        self.spawn_reporter(id, kind, progress, work);
        id
    }

    /// Starts a job that waits on the network or other tasks. Must run inside a tokio
    /// runtime.
    pub fn start_async<F, Fut>(&self, spec: JobSpec, f: F) -> JobId
    where
        F: FnOnce(JobCtx) -> Fut + Send + 'static,
        Fut: Future<Output = Result<JobOutcome, JobEnd>> + Send + 'static,
    {
        let JobSpec {
            kind,
            project,
            phase,
            pass,
        } = spec;
        let (id, ctx) = self.register(kind, project, phase);
        let progress = ctx.progress.clone();
        let work = tokio::spawn(async move {
            let _pass = pass;
            f(ctx).await
        });
        self.spawn_reporter(id, kind, progress, work);
        id
    }

    /// The job's state, or `None` when no job has this id or its outcome is older than ten
    /// minutes.
    pub fn view(&self, id: JobId) -> Option<JobView> {
        let mut registry = lock(&self.inner.registry);
        Self::prune(&mut registry);
        registry.jobs.get(&id).map(|job| job.view(id))
    }

    /// Asks the job to stop at its next check and shows `Cancelling` at once. A job that is
    /// finished or unknown is left as it is.
    pub fn cancel(&self, id: JobId) {
        let mut registry = lock(&self.inner.registry);
        Self::prune(&mut registry);
        let Some(job) = registry.jobs.get(&id) else {
            return;
        };
        if is_terminal(&job.state) {
            return;
        }
        job.cancel.cancel();
        self.transition(&mut registry, id, JobState::Cancelling);
    }

    /// The jobs that have not ended.
    pub fn live(&self) -> usize {
        lock(&self.inner.registry)
            .jobs
            .values()
            .filter(|job| !is_terminal(&job.state))
            .count()
    }

    fn register(
        &self,
        kind: JobKind,
        project: Option<ProjectRef>,
        phase: Phase,
    ) -> (JobId, JobCtx) {
        let id = JobId::new(self.inner.next_id.fetch_add(1, Ordering::SeqCst));
        let ctx = JobCtx {
            progress: ProgressSink::new(phase),
            cancel: CancelToken::default(),
        };
        let mut registry = lock(&self.inner.registry);
        Self::prune(&mut registry);
        // No frame announces the job: one that ends within 300 ms shows nothing (PERF rule 4).
        registry.jobs.insert(
            id,
            Job {
                kind,
                project,
                state: JobState::Queued,
                cancel: ctx.cancel.clone(),
                ended: None,
            },
        );
        (id, ctx)
    }

    fn prune(registry: &mut Registry) {
        let now = Instant::now();
        registry.jobs.retain(|_, job| {
            job.ended
                .is_none_or(|ended| now.saturating_duration_since(ended) < OUTCOME_KEPT)
        });
    }

    /// The one place a job's state changes and a frame is published, under the registry's
    /// lock so the frames of a job come in the order its states did.
    fn transition(&self, registry: &mut Registry, id: JobId, to: JobState) -> bool {
        let Some(job) = registry.jobs.get_mut(&id) else {
            return false;
        };
        if !allowed(&job.state, &to) {
            return false;
        }
        if is_terminal(&to) {
            job.ended = Some(Instant::now());
        }
        job.state = to;
        self.inner.bus.publish(Event::Job(job.view(id)));
        true
    }

    fn change(&self, id: JobId, to: JobState) -> bool {
        let mut registry = lock(&self.inner.registry);
        self.transition(&mut registry, id, to)
    }

    fn spawn_reporter(&self, id: JobId, kind: JobKind, progress: ProgressSink, work: Work) {
        let registry = self.clone();
        tokio::spawn(async move { registry.report(id, kind, progress, work).await });
    }

    async fn report(&self, id: JobId, kind: JobKind, progress: ProgressSink, mut work: Work) {
        let first = Instant::now() + FIRST_REPORT_AFTER;
        tokio::select! {
            result = &mut work => {
                self.finish(id, kind, result);
                return;
            }
            () = tokio::time::sleep_until(first) => {}
        }
        let every = report_every(kind);
        let mut last: Option<Progress> = None;
        loop {
            let now = progress.snapshot();
            if last.as_ref() != Some(&now) && self.change(id, JobState::Running(now.clone())) {
                last = Some(now);
            }
            tokio::select! {
                result = &mut work => {
                    // The last count comes before the end, for a job a screen shows.
                    let now = progress.snapshot();
                    if !is_unseen(kind) && last.as_ref() != Some(&now) {
                        self.change(id, JobState::Running(now));
                    }
                    self.finish(id, kind, result);
                    return;
                }
                () = tokio::time::sleep(every) => {}
            }
        }
    }

    /// Publishes the end. A job that returns `Ok` after it was asked to stop is `Done`: its
    /// last check had passed (§4.6, the Download row).
    fn finish(
        &self,
        id: JobId,
        kind: JobKind,
        result: Result<Result<JobOutcome, JobEnd>, JoinError>,
    ) {
        let to = match result {
            Ok(Ok(outcome)) => JobState::Done(outcome),
            Ok(Err(JobEnd::Cancelled)) => JobState::Cancelled,
            Ok(Err(JobEnd::Failed(error))) => JobState::Failed(error),
            Err(stopped) => {
                let report = self.inner.reports.add(
                    ReportOp::Internal,
                    ReportOutcome::Failed {
                        reason: "a background job stopped".to_owned(),
                    },
                    facts(serde_json::json!({
                        "job": id.get(),
                        "kind": format!("{kind:?}"),
                        "stopped": stop_reason(stopped),
                    })),
                    self.inner.clock.now(),
                    self.inner.clock.now(),
                );
                JobState::Failed(ApiError::Internal { report })
            }
        };
        self.change(id, to);
    }
}

fn stop_reason(stopped: JoinError) -> String {
    match stopped.try_into_panic() {
        Ok(payload) => payload
            .downcast_ref::<&str>()
            .map(|message| (*message).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "it panicked".to_owned()),
        Err(other) => other.to_string(),
    }
}
