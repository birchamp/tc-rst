//! First-launch seeding (APP-DESIGN §4.6, PERFORMANCE.md "Start-up", unit A9).
//!
//! The machine's resource store lives under app data and is opened when the app starts. The
//! installer ships a bundled English suite as an expanded store (`--bundle <dir>`, read only);
//! a first launch copies it into the machine's store, a repeat launch copies nothing:
//!
//! - **Plan.** [`plan`] runs inline in `App::start`. It reads receipts and check records and
//!   never a content file, so `hello.startup` is exact from the first request: `Ready` when
//!   there is nothing to copy or check, `Preparing { done: 0, total }` with `total` in content
//!   files when there is, and `Failed` when the plan cannot be made.
//! - **Job.** The `Seed` job does the plan on the blocking pool. Its reports become `Startup`
//!   events through [`JobRegistry::watch`](crate::jobs::JobRegistry::watch). The job sets
//!   `Ready` or `Failed` itself before it returns, so `Ready` means every bundled install is
//!   here, checked, and none was refused.
//! - **Stop.** Close is a quit, and [`App::quit`] cancels the job and waits for it. The seed
//!   stops before its next file; every copy made before the stop is whole and checked, and the
//!   next start copies only what is missing.
//!
//! Declared differences from tC4 (APP-DESIGN §10, A9): the count is in files, not in tC4's
//! 16 resources, because one resource is up to 1,021 files each written with an fsync; and
//! a failure shows in the window instead of in a native error box.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use tc_api::error::ApiError;
use tc_api::event::{StartupProgress, StartupState};
use tc_api::ids::JobId;
use tc_api::jobs::{JobKind, JobOutcome, JobState, Phase};
use tc_resources::{
    ResourceStore, SeedPlan, SeedReport, SeedWatch, Seeding, StoreError, content_files_opened,
    content_files_written,
};

use crate::app::{App, lock};
use crate::jobs::{CancelToken, JobCtx, JobEnd, JobRegistry, JobSpec, ProgressSink};
use crate::reports::{ReportOp, ReportOutcome, facts};
use crate::startup::{StartupEnd, Update};

/// The machine's resource store, under app data (STORAGE §3.7).
pub const RESOURCES_FOLDER: &str = "resources";

/// What a stopped seed says. The window shows it under tC4's failure words.
const STOPPED: &str = "preparing the resources was stopped; it continues at the next start";

/// What `App::start` found to do.
pub(crate) enum SeedStart {
    /// No bundle, or every bundled install is here and checked: `Ready`, no job, no frame.
    Nothing,
    Work {
        bundle: PathBuf,
        plan: SeedPlan,
    },
    /// The plan could not be made, or only unreadable bundled folders were found: `Failed`.
    Failed {
        reason: String,
        facts: Map<String, Value>,
    },
}

/// Runs inline in `App::start`: reads receipts and check records, never a content file. The
/// counts of content files are per thread, so a plan on the blocking pool could not be
/// counted by a test, and it costs the receipts of 15 installs.
pub(crate) fn plan(store: &ResourceStore, bundle: Option<&Path>) -> SeedStart {
    let Some(bundle) = bundle else {
        return SeedStart::Nothing;
    };
    let failed = |reason: String| SeedStart::Failed {
        facts: facts(json!({"bundle": bundle.display().to_string(), "reason": reason})),
        reason,
    };
    let bundled = match ResourceStore::open_read_only(bundle) {
        Ok(bundled) => bundled,
        Err(e) => return failed(format!("the bundled resources cannot be read: {e}")),
    };
    let plan = match store.seed_plan(&bundled) {
        Ok(plan) => plan,
        Err(e) => return failed(e.to_string()),
    };
    if plan.is_empty() {
        return match end_of(plan.report()) {
            StartupEnd::Ready => SeedStart::Nothing,
            StartupEnd::Failed(reason) => SeedStart::Failed {
                facts: facts_of(bundle, 0, plan.report(), 0, 0, Duration::ZERO),
                reason,
            },
        };
    }
    SeedStart::Work {
        bundle: bundle.to_owned(),
        plan,
    }
}

/// The state `hello` starts with.
pub(crate) fn initial_state(start: &SeedStart) -> StartupState {
    match start {
        SeedStart::Nothing => StartupState::Ready,
        SeedStart::Work { plan, .. } => StartupState::Preparing(StartupProgress {
            done: 0,
            total: plan.files(),
        }),
        SeedStart::Failed { reason, .. } => StartupState::Failed {
            reason: reason.clone(),
        },
    }
}

/// `Ready`, or `Failed` when a bundled install was refused (D6): tC4 stops its start for a
/// damaged bundled resource, and a start without the English suite would open projects that
/// cannot find their helps. Pure.
pub(crate) fn end_of(report: &SeedReport) -> StartupEnd {
    if report.refused.is_empty() {
        return StartupEnd::Ready;
    }
    let reasons: Vec<String> = report
        .refused
        .iter()
        .map(|(install, error)| format!("{install}: {}", short(error)))
        .collect();
    StartupEnd::Failed(reasons.join("; "))
}

/// What the window shows of an error: a damaged file by its name and problem, where the full
/// message also holds the install's folder. The report keeps the full message.
fn short(error: &StoreError) -> String {
    match error {
        StoreError::Damaged { file, problem, .. } => format!("{file}: {problem}"),
        other => other.to_string(),
    }
}

/// The `seed` report's facts. Pure.
pub(crate) fn facts_of(
    bundle: &Path,
    files: u64,
    report: &SeedReport,
    opened: u64,
    written: u64,
    elapsed: Duration,
) -> Map<String, Value> {
    let refused: Vec<Value> = report
        .refused
        .iter()
        .map(|(install, error)| json!({"install": install, "reason": error.to_string()}))
        .collect();
    facts(json!({
        "bundle": bundle.display().to_string(),
        "files": files,
        "added": report.added,
        "checkedAgain": report.checked_again,
        "alreadyInstalled": report.already_installed.len(),
        "refused": refused,
        "contentFilesOpened": opened,
        "contentFilesWritten": written,
        "ms": u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
    }))
}

/// The job's watch: counts into the job's progress, and stops when the job is cancelled.
struct JobWatch<'a> {
    progress: &'a ProgressSink,
    cancel: &'a CancelToken,
    #[cfg(feature = "test-hooks")]
    pace: Option<Duration>,
}

impl SeedWatch for JobWatch<'_> {
    fn done(&self, files: u64) {
        self.progress.reach(files);
    }

    fn cancelled(&self) -> bool {
        #[cfg(feature = "test-hooks")]
        if let Some(pace) = self.pace {
            // Asked once per file, so this is the wait before each file. Slices let a
            // cancel end the wait at once.
            let slice = Duration::from_millis(10);
            let mut left = pace;
            while !left.is_zero() && !self.cancel.is_cancelled() {
                let wait = left.min(slice);
                std::thread::sleep(wait);
                left -= wait;
            }
        }
        self.cancel.is_cancelled()
    }
}

impl App {
    /// Acts on what the plan found, once the app is built: the state `hello` starts with was
    /// already taken from it.
    pub(crate) fn begin_seed(&self, start: SeedStart) {
        match start {
            SeedStart::Nothing => {}
            SeedStart::Failed { reason, facts } => {
                let now = self.inner.clock.now();
                self.inner.reports.add(
                    ReportOp::Seed,
                    ReportOutcome::Failed { reason },
                    facts,
                    now,
                    now,
                );
                // A page that subscribes from the start sees why.
                self.inner.startup.announce();
            }
            SeedStart::Work { bundle, plan } => {
                self.inner.startup.announce();
                self.start_seed_job(bundle, plan);
            }
        }
    }

    fn start_seed_job(&self, bundle: PathBuf, plan: SeedPlan) {
        let files = plan.files();
        let app = self.clone();
        let id = self
            .inner
            .jobs
            .start_blocking(JobSpec::new(JobKind::Seed, Phase::Seed), move |ctx| {
                app.seed_in_background(&bundle, plan, &ctx)
            });
        *lock(&self.inner.seed_job) = Some(id);
        self.mirror_seed_into_startup(id, files);
    }

    /// The work of the `Seed` job, on the blocking pool.
    fn seed_in_background(
        &self,
        bundle: &Path,
        plan: SeedPlan,
        ctx: &JobCtx,
    ) -> Result<JobOutcome, JobEnd> {
        let inner = &self.inner;
        let files = plan.files();
        ctx.progress.total(files);
        let watch = JobWatch {
            progress: &ctx.progress,
            cancel: &ctx.cancel,
            #[cfg(feature = "test-hooks")]
            pace: inner.config.seed_pace,
        };
        // The counts are per thread: take them on the thread that seeds.
        let (opened_before, written_before) = (content_files_opened(), content_files_written());
        let started = Instant::now();
        let at = inner.clock.now();
        let seeded = inner.resources.seed(plan, &watch);
        let (opened, written) = (
            content_files_opened() - opened_before,
            content_files_written() - written_before,
        );
        let elapsed = started.elapsed();

        let (report, end, job_end) = match seeded {
            Ok(Seeding::Finished(report)) => {
                let end = end_of(&report);
                (report, end, None)
            }
            Ok(Seeding::Cancelled(report)) => (
                report,
                StartupEnd::Failed(STOPPED.to_owned()),
                Some(JobEnd::Cancelled),
            ),
            Err(e) => (
                SeedReport::default(),
                StartupEnd::Failed(e.to_string()),
                None,
            ),
        };
        let outcome = match &end {
            StartupEnd::Ready => ReportOutcome::Ok,
            StartupEnd::Failed(reason) => ReportOutcome::Failed {
                reason: reason.clone(),
            },
        };
        let report_id = inner.reports.add(
            ReportOp::Seed,
            outcome,
            facts_of(bundle, files, &report, opened, written, elapsed),
            at,
            inner.clock.now(),
        );
        // Before the job returns, so `hello` says `Ready` only once every copy is whole and
        // checked, and a late progress report cannot undo it (`advance`).
        let ready = end == StartupEnd::Ready;
        inner.startup.update(Update::End(end));
        match (ready, job_end) {
            (true, _) => Ok(JobOutcome::Finished),
            (false, Some(end)) => Err(end),
            (false, None) => Err(JobEnd::Failed(ApiError::Internal { report: report_id })),
        }
    }

    /// Follows the job's reports into `Startup` events. The job sets the end itself; this
    /// covers a job that ended without it (a closure that panicked).
    fn mirror_seed_into_startup(&self, id: JobId, files: u64) {
        let Some(mut watch) = self.inner.jobs.watch(id) else {
            return;
        };
        let app = self.clone();
        tokio::spawn(async move {
            loop {
                let state = watch.borrow_and_update().clone();
                match state {
                    JobState::Running(progress) => {
                        app.inner.startup.update(Update::Progress {
                            done: progress.done,
                            total: progress.total.unwrap_or(files),
                        });
                    }
                    JobState::Done(_) => {
                        app.inner.startup.update(Update::End(StartupEnd::Ready));
                        break;
                    }
                    JobState::Failed(_) | JobState::Cancelled => {
                        app.inner
                            .startup
                            .update(Update::End(StartupEnd::Failed(STOPPED.to_owned())));
                        break;
                    }
                    JobState::Queued | JobState::Cancelling => {}
                }
                if watch.changed().await.is_err() {
                    break;
                }
            }
        });
    }

    /// Asks the seed to stop and waits until it has: the quit keeps `app.lock` until no seed
    /// writer runs (DELTA §3.15.10). The server bounds the whole quit by its close budget, so
    /// a seed stuck in a slow disk is cut at 5 s, which is safe because an install is staged
    /// and renamed whole.
    pub(crate) async fn stop_seed(&self) {
        let id = *lock(&self.inner.seed_job);
        let Some(id) = id else { return };
        self.inner.jobs.cancel(id);
        if let Some(mut watch) = self.inner.jobs.watch(id) {
            // Fails only when the job was pruned, which only an ended job is.
            let _ = watch.wait_for(JobRegistry::is_ended).await;
        }
    }

    /// The machine's resource store. A reader must not rely on it before
    /// [`App::startup`] is `Ready`.
    pub fn resources(&self) -> &ResourceStore {
        &self.inner.resources
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn damaged(file: &str) -> StoreError {
        StoreError::Damaged {
            path: PathBuf::from("/bundle/dcs/o/r/sha"),
            file: file.to_owned(),
            problem: "its SHA-256 is not the receipt's".to_owned(),
        }
    }

    #[test]
    fn a_refused_install_ends_the_start_and_names_each_install() {
        let mut report = SeedReport::default();
        assert_eq!(end_of(&report), StartupEnd::Ready);
        report
            .refused
            .push(("en_tw@v87".to_owned(), damaged("a.md")));
        report
            .refused
            .push(("en_tn@v91".to_owned(), damaged("b.tsv")));
        let StartupEnd::Failed(reason) = end_of(&report) else {
            panic!("a refused install did not fail the start")
        };
        assert!(reason.starts_with("en_tw@v87: a.md: "), "{reason}");
        assert!(reason.contains("; en_tn@v91: b.tsv: "), "{reason}");
        assert!(
            !reason.contains("/bundle/"),
            "the window does not show folders: {reason}"
        );
    }

    #[test]
    fn the_facts_hold_the_counts_and_the_names() {
        let mut report = SeedReport::default();
        report.added.push("en_obs@v9".to_owned());
        report.checked_again.push("en_tq@v91".to_owned());
        report.already_installed.push("en_tq@v91".to_owned());
        report
            .refused
            .push(("en_tw@v87".to_owned(), damaged("a.md")));
        let facts = facts_of(
            Path::new("/bundle"),
            61,
            &report,
            60,
            58,
            Duration::from_millis(1_234),
        );
        assert_eq!(facts["bundle"], "/bundle");
        assert_eq!(facts["files"], 61);
        assert_eq!(facts["added"], json!(["en_obs@v9"]));
        assert_eq!(facts["checkedAgain"], json!(["en_tq@v91"]));
        assert_eq!(facts["alreadyInstalled"], 1);
        assert_eq!(facts["refused"][0]["install"], "en_tw@v87");
        assert!(
            facts["refused"][0]["reason"]
                .as_str()
                .unwrap()
                .contains("a.md"),
            "{facts:?}"
        );
        assert_eq!(facts["contentFilesOpened"], 60);
        assert_eq!(facts["contentFilesWritten"], 58);
        assert_eq!(facts["ms"], 1_234);
    }
}
