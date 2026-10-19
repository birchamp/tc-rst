//! Background jobs and their progress (APP-DESIGN §3.6, §4.6), and the `jobs` service.
//!
//! The units that build a job add its outcome and, where a screen shows its steps, its
//! phase: an open's `Opened` session (B3), a sync's result (B10), a plan (C8), and the
//! phases of share, exchange, import and export.

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::error::ApiError;
use crate::ids::{DownloadId, JobId, ProjectRef, RequestId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct JobView {
    pub id: JobId,
    pub kind: JobKind,
    pub project: Option<ProjectRef>,
    pub state: JobState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum JobKind {
    Open,
    Create,
    Join,
    Sync,
    Share,
    ExchangeExport,
    ExchangeReceive,
    Import,
    ImportVersions,
    AddUsfmBooks,
    Download,
    UpgradePlan,
    GatewayPlan,
    FixFetch,
    Export,
    /// No screen shows its progress: reports go out once a second at most (A-29).
    HomeSummary,
    /// No screen shows its progress: reports go out once a second at most (A-29).
    Train,
    /// First-launch seeding of the bundled resources.
    Seed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum JobState {
    Queued,
    Running(Progress),
    Done(JobOutcome),
    Failed(ApiError),
    Cancelling,
    /// A cancelled job changed no project data.
    Cancelled,
}

/// Determinate progress. Counts never fall within a phase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: Phase,
    pub done: u64,
    /// `None` until the phase knows its total: the UI draws tC4's indeterminate bar.
    pub total: Option<u64>,
    /// What the phase works on now, such as a file or a book.
    pub item: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Phase {
    Open(OpenPhase),
    Sync(SyncPhase),
    /// Installing one resource repository.
    Install {
        repo: String,
    },
    /// Planning an upgrade or a gateway change, book by book.
    Plan {
        #[cfg_attr(
            feature = "proptest",
            proptest(strategy = "proptest::option::of(crate::arb::book())")
        )]
        book: Option<BookId>,
    },
    Train,
    Seed,
    Summary,
}

/// The phases of an open (STORAGE §6.1, "Progress"); tc-store reports them (AD8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum OpenPhase {
    /// O2 to O7.
    FinishingWork,
    /// O8.
    ReadingChanges,
    Folding,
    /// O9.
    CheckingFiles,
    /// O10.
    WritingFiles,
}

/// The steps of a sync (STORAGE §9.1, S1 to S9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum SyncPhase {
    Checkpoint,
    Fetch,
    Intake,
    Integrate,
    MergeCommit,
    Push,
    Confirm,
}

/// A finished job's result. It stays readable through `jobs/get` for 10 minutes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum JobOutcome {
    /// A job whose end is its whole result: a seed, a download, a summary, a training.
    Finished,
    /// A project opened for the screens (B3).
    Opened(Box<crate::session::OpenedProject>),
    Created {
        project: ProjectRef,
    },
    ExportReady {
        download: DownloadId,
        file_name: String,
    },
}

/// `jobs/get`: a job's state, or `None` when no job has this id or its outcome expired.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Get {
    pub job: JobId,
}

/// `jobs/cancel`: the job stops at its next check. Cancelling a finished job does nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Cancel {
    pub request: RequestId,
    pub job: JobId,
}
