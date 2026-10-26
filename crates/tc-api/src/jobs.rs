//! Background jobs and their progress (APP-DESIGN §3.6, §4.6), and the `jobs` service.
//!
//! The units that build a job add its outcome and, where a screen shows its steps, its
//! phase: an open's `Opened` session (B3), a sync's result (B10), a plan (C8), and the
//! phases of share, exchange, import and export.

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::error::ApiError;
use crate::ids::{JobId, ProjectRef, RequestId};
use crate::shown::PersonLabel;

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
    /// Update a book from USFM: its two batches.
    UpdateBook,
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
    Exchange(ExchangePhase),
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
    Export,
    /// Reading the files of an import, or writing what they hold.
    Import,
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

/// The phases of a USB export (STORAGE §10.1: checkpoint, write, verify from the device) and of
/// a receive (§10.2: read the file, take its packs in).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum ExchangePhase {
    Checkpoint,
    Write,
    Verify,
    Read,
    TakeIn,
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
    /// An export's file, ready to download (D2).
    ExportReady(crate::export::Exported),
    /// A sync that completed (B10): what its toast shows.
    Synced(SyncResultView),
    /// An exchange file written and verified (E2): "You can remove the stick".
    Exported(crate::exchange::ExportedView),
    /// A receive from an exchange file that completed (E2): what the Received sheet shows.
    Received(crate::exchange::ReceivedView),
    /// An upgrade, a gateway change or a re-pin planned (C8): the plan to read and confirm.
    PlanReady(Box<crate::resources::PlanView>),
    /// The resource versions an import's projects name, looked up (F1).
    VersionsFound(crate::import::VersionsView),
    /// What Update a book did (F1).
    BookUpdated(crate::import::UpdateReport),
}

/// What a completed sync brought and left (tC4's `report.facts` of an ok sync, which its toast
/// reads, `tC4:src/views/SyncResult.jsx`). The conflict counts are the project's open
/// conflicts after the sync, as tC4's are, so a second sync still names a conflict the first
/// one brought.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct SyncResultView {
    /// The teammates' changes this sync received.
    pub received: u32,
    /// Who sent them, each person once, named as the conflict review names them (#680).
    pub from: Vec<PersonLabel>,
    /// Records that held two versions and kept the newest with no question (owner ruling H5),
    /// one per record as tC4 counts them: tC4's "{n} conflicts kept the newest version".
    pub kept_newest: u32,
    /// Open verse conflicts.
    pub verse_conflicts: u32,
    /// Verses with two alignments to review (#791).
    pub alignments: u32,
    /// Changes that Door43 had lost and this sync sent again.
    pub restored: u32,
    /// The project's pinned resource versions that this computer does not have (D88 point 6).
    pub missing_pins: u32,
    /// What a teammate's newer version replaced, with no question (owner ruling H5): "Juan's
    /// alignment of Titus 1:1 replaced yours. Your version is in History."
    #[serde(default)]
    pub replaced: Vec<ReplacedView>,
    /// Files inside an exchange file that were edited by hand and not taken in (E2; a sync
    /// has none): the Received sheet says so once.
    #[serde(default)]
    pub hand_edited: Vec<crate::exchange::HandEdited>,
}

/// The versions of one kind of `whose` that the newer changes of `by` replaced: `place` is
/// the first of them, `count` how many.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplacedView {
    pub by: crate::shown::PersonLabel,
    pub whose: crate::shown::PersonLabel,
    pub kind: ReplacedKind,
    pub place: crate::conflicts::ReviewPlace,
    pub count: u32,
    /// A setting's or the pins' older value, which the person can set again where it is set
    /// (owner ruling H5 for settings and pins); a row of one record names it.
    #[serde(default)]
    pub was: Option<ReplacedValue>,
}

/// The older value a settings or pins row names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ReplacedValue {
    /// The project's text font.
    Font(String),
    /// The project's text direction.
    Direction(crate::shown::Direction),
    /// The pinned resources that differ: each by its repository's name and version, as
    /// `en_tn v90`, or a gateway language by its code.
    Resources(
        #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))] Vec<String>,
    ),
}

/// What a newer version replaced, as the Received sheet names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum ReplacedKind {
    Alignment,
    Check,
    Comment,
    Setting,
    Resources,
    Book,
    File,
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

/// Each job row's outcome: what its handler ends with (`JobOutcome::*` in tc-app).
macro_rules! starts_job {
    ($($req:path => $out:ty { $from:pat => $to:expr, $value:ident => $wrap:expr })*) => {
        $(
            impl crate::endpoint::StartsJob for $req {
                type Outcome = $out;

                fn outcome(outcome: JobOutcome) -> Result<$out, JobOutcome> {
                    match outcome {
                        $from => Ok($to),
                        other => Err(other),
                    }
                }

                fn wrap($value: $out) -> JobOutcome {
                    $wrap
                }
            }
        )*
    };
}

starts_job! {
    crate::door43::OpenDoor43 => ProjectRef {
        JobOutcome::Created { project } => project, project => JobOutcome::Created { project }
    }
    crate::door43::SyncProject => SyncResultView {
        JobOutcome::Synced(view) => view, view => JobOutcome::Synced(view)
    }
    crate::exchange::Export => crate::exchange::ExportedView {
        JobOutcome::Exported(view) => view, view => JobOutcome::Exported(view)
    }
    crate::exchange::Receive => crate::exchange::ReceivedView {
        JobOutcome::Received(view) => view, view => JobOutcome::Received(view)
    }
    crate::exchange::ReceiveNew => ProjectRef {
        JobOutcome::Created { project } => project, project => JobOutcome::Created { project }
    }
    crate::session::Open => Box<crate::session::OpenedProject> {
        JobOutcome::Opened(opened) => opened, opened => JobOutcome::Opened(opened)
    }
    crate::resources::Download => () {
        JobOutcome::Finished => (), _done => JobOutcome::Finished
    }
    crate::resources::PlanUpgrade => Box<crate::resources::PlanView> {
        JobOutcome::PlanReady(plan) => plan, plan => JobOutcome::PlanReady(plan)
    }
    crate::resources::PlanGateway => Box<crate::resources::PlanView> {
        JobOutcome::PlanReady(plan) => plan, plan => JobOutcome::PlanReady(plan)
    }
    crate::import::Versions => crate::import::VersionsView {
        JobOutcome::VersionsFound(view) => view, view => JobOutcome::VersionsFound(view)
    }
    crate::import::Run => ProjectRef {
        JobOutcome::Created { project } => project, project => JobOutcome::Created { project }
    }
    crate::import::UpdateBook => crate::import::UpdateReport {
        JobOutcome::BookUpdated(report) => report, report => JobOutcome::BookUpdated(report)
    }
    crate::resources::FixFetch => () {
        JobOutcome::Finished => (), _done => JobOutcome::Finished
    }
    crate::export::RunExport => crate::export::Exported {
        JobOutcome::ExportReady(done) => done, done => JobOutcome::ExportReady(done)
    }
}
