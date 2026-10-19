//! The one error type of every reply (APP-DESIGN §3.5, AD9). HTTP 200 carries
//! [`Reply`](crate::Reply), the reply or an `ApiError`, so the UI handles every outcome in
//! one exhaustive match, and a proxy or token failure, which never reaches this type, cannot
//! be read as a refusal.

use serde::{Deserialize, Serialize};

use crate::endpoint::NetKind;
use crate::ids::{Epoch, ReportId};
use crate::refusal::Refusal;
use crate::write::NotSaved;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ApiError {
    /// The UI was built for another API version: a stale UI after an update. It reloads.
    #[error("the server speaks API version {server}")]
    Version { server: u32 },
    /// A request the boundary could not parse: a bug, logged.
    #[error("bad request: {field}: {reason}")]
    BadRequest { field: String, reason: String },
    /// A query's project was closed or opened again; the UI drops the reply. A write never
    /// gets this: it gets `NotSaved::StaleScope` (A-19).
    #[error("the project was closed or opened again")]
    StaleScope { current: Option<Epoch> },
    /// A check or align session was replaced.
    #[error("the session was replaced")]
    StaleSession,
    /// A `RequestId` came back with another body: a bug, reported. The field stays dirty
    /// and saves again with a new id (A-2).
    #[error("a request id was used for another request")]
    RequestReused,
    /// Writes only.
    #[error("not saved: {0:?}")]
    NotSaved(NotSaved),
    /// A domain refusal, in tC4's words.
    #[error("refused: {0:?}")]
    Refused(Refusal),
    /// The internet switch is off; nothing was sent. The UI asks the question again.
    #[error("needs the internet")]
    NeedsInternet(NetKind),
    #[error("needs a Door43 sign-in")]
    NeedsSignIn(SignInReason),
    /// A flow that runs one at a time is running (AD31).
    #[error("busy: {0:?}")]
    Busy(Flow),
    /// The backend cannot do this yet: before the storage, sync or import units land.
    #[error("not available yet: {0:?}")]
    Unavailable(Feature),
    /// A bug; "Ask for help" attaches the report.
    #[error("internal error")]
    Internal { report: ReportId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum SignInReason {
    NotSignedIn,
    /// Door43 refused the held token; it was forgotten.
    TokenRejected,
    /// A repository that answers 401 or 404 without a sign-in may be private (PLAN Q3).
    PrivateRepository,
}

/// The flows that run one at a time (AD31). The first five run one at a time across the
/// app; the exchange flows one at a time per project, and never beside its sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Flow {
    Sync,
    GatewayChange,
    Share,
    SignIn,
    Import,
    ExchangeExport,
    ExchangeReceive,
}

/// What the backend cannot do yet, by the unit that brings it (APP-DESIGN AD6). The units that
/// bring a feature delete its variant when nothing answers `Unavailable` with it any more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Feature {
    /// Saves, drafts, creates and checkpoints: storage unit S1.
    Writes,
    /// Materialize and outside edits: S2.
    Materialize,
    /// Door43 sync, share and open: X.
    Sync,
    /// Save work for a teammate, receive work from a file: X.
    Exchange,
    /// Import of tC4, tC3, USFM and burrito projects: I.
    Import,
    /// Export files: D2.
    Export,
    /// The Feedback dialog's attachment: unit A5.
    Feedback,
}
