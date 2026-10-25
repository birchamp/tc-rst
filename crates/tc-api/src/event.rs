//! The event stream (APP-DESIGN §3.6, §5.4, AD10): `GET /api/v1/events` sends one
//! [`EventFrame`] per server-sent event.
//!
//! Units add the session events of their screens: the notices (`Notice`), the received-work
//! sheet (`Received`) and the suggestion status (`Suggest`) come with the units that show
//! them.

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::account::AccountView;
use crate::align::{AlignSave, SuggestStatus, Testament};
use crate::check::CheckListRef;
use crate::ids::{
    AlignerId, ProjectRef, RecordRef, RequestId, Scope, ServerInstance, StoryNo, ViewSeq,
};
use crate::jobs::JobView;
use crate::shown::PersonLabel;
use crate::write::ReadOnlyReason;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct EventFrame {
    /// The frame's place in the stream. A reconnect asks for the frames after the last one
    /// it read; one the ring no longer holds gets `Resync`.
    pub seq: u64,
    pub instance: ServerInstance,
    pub event: Event,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Event {
    /// Frames were lost: the UI reads every live query again.
    Resync,
    Internet {
        on: bool,
    },
    Account(AccountView),
    Job(JobView),
    /// A project was created, imported or joined.
    ProjectsChanged,
    /// A Home card's summary was computed or changed.
    HomeCard {
        project: ProjectRef,
    },
    Session {
        at: Scope,
        event: SessionEvent,
    },
    /// First-launch seeding (unit A9).
    Startup(StartupState),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SessionEvent {
    Opened,
    Closed,
    Mode(SessionMode),
    /// The project's data changed. The UI fetches again only the queries that
    /// [`QueryKey::depends_on`](crate::query::QueryKey::depends_on) this scope.
    Changed {
        view: ViewSeq,
        scope: ChangeScope,
        cause: ChangeCause,
    },
    Checkpoint(CheckpointOutcome),
    /// A file the app must rewrite is held by another program (STORAGE §5.10).
    MaterializeBlocked {
        paths: Vec<String>,
    },
    /// The project's outside edits changed: a file was held, or a hold or a kept copy ended
    /// (D1). The lists replace the session's.
    OutsideEdits(crate::session::OutsideEdits),
    /// The server's alignment save for an open aligner (AD18).
    AlignSave {
        aligner: AlignerId,
        state: AlignSave,
    },
    /// Where the suggestion engine stands for a testament of this project (B6). Every aligner
    /// view carries it again, so a gap cannot leave a stale row standing.
    Suggest {
        testament: Testament,
        status: SuggestStatus,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SessionMode {
    Writable,
    ReadOnly(ReadOnlyReason),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChangeCause {
    /// The UI's own write. A field whose request this is shows no notice (AD37).
    LocalSave {
        request: RequestId,
    },
    /// Teammates' work received by a sync or from a file.
    Intake {
        from: Vec<PersonLabel>,
    },
    /// A file edited by another program, recorded as changes (STORAGE §11).
    OutsideEdit {
        path: String,
    },
    Upgrade,
    Resolve,
    Import,
    System,
}

/// What a change touched. Past its caps a scope is `Everything`, which costs a refresh of
/// every live query but is never wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChangeScope {
    Parts(ChangeParts),
    Everything,
}

impl ChangeScope {
    /// The scope of `parts`, or `Everything` when it names more records than a frame
    /// carries.
    pub fn bounded(parts: ChangeParts) -> Self {
        if parts.records.len() > ChangeParts::RECORDS_CAP {
            Self::Everything
        } else {
            Self::Parts(parts)
        }
    }
}

/// tc-app's form of the fold's `Delta` (STORAGE §7.8).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ChangeParts {
    /// Chapters whose verses, fronts or sections changed.
    pub chapters: Vec<ChapterRef>,
    /// Books that changed as a whole: added, removed, restored, or their layout.
    #[cfg_attr(
        feature = "proptest",
        proptest(strategy = "proptest::collection::vec(crate::arb::book(), 0..4)")
    )]
    pub books: Vec<BookId>,
    /// OBS stories.
    pub stories: Vec<StoryNo>,
    /// Check lists with rows that changed. An open rail asks `check/rows { since }` for
    /// those rows, never for the whole rail (A-12).
    pub checks: Vec<CheckListRef>,
    /// Records that changed, with who wrote each; at most [`ChangeParts::RECORDS_CAP`].
    pub records: Vec<RecordChange>,
    pub aspects: Vec<Aspect>,
}

impl ChangeParts {
    pub const RECORDS_CAP: usize = 256;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ChapterRef {
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: BookId,
    pub chapter: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct RecordChange {
    pub record: RecordRef,
    /// Who wrote it, for "Juan changed this verse while you were typing".
    pub by: Option<PersonLabel>,
}

/// Project-wide data a change can touch besides text and checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Aspect {
    /// The resources the project is pinned to.
    Pins,
    Settings,
    /// The project's name, language and other details.
    Meta,
    Conflicts,
}

/// A checkpoint's result. A failed one shows "Checkpoint not saved" with Retry
/// (`tC4:src/App.jsx:63-67`), the reason as its diagnosis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CheckpointOutcome {
    Committed,
    /// Nothing changed since the last checkpoint, so no commit was made.
    Unchanged,
    Failed {
        reason: String,
    },
}

/// First-launch seeding of the bundled resources: "Preparing resources: n of total".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StartupState {
    Ready,
    Preparing(StartupProgress),
    /// Seeding stopped; it resumes at the next launch.
    Failed {
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct StartupProgress {
    pub done: u64,
    pub total: u64,
}
