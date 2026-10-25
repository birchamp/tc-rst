//! The `app` service: the UI's first request, quitting, and reports of UI errors.

use serde::{Deserialize, Serialize};

use crate::account::AccountView;
use crate::event::{SessionMode, StartupState};
use crate::ids::{LocaleId, ProjectId, ProjectRef, RequestId, Scope, ServerInstance, WallMs};

/// The version the About dialog shows: the workspace's package version.
pub const BUILD_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The commit this build was made from: the release pipeline sets `TC4_BUILD_COMMIT` for the
/// server and the wasm build alike, and `dev` stands when it is not set. The UI compares
/// `hello.build.commit` with its own copy of this constant and reloads once on a difference
/// (§3.10), so both sides read this one definition. Cargo builds again when the variable
/// changes.
pub const BUILD_COMMIT: &str = match option_env!("TC4_BUILD_COMMIT") {
    Some(commit) => commit,
    None => "dev",
};

/// `app/hello`: everything the UI needs before its first render, and after a reload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Hello {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct HelloView {
    /// The server's [`API_VERSION`](crate::API_VERSION).
    pub api: u32,
    pub build: BuildInfo,
    pub instance: ServerInstance,
    /// The internet switch, off at every launch.
    pub internet: bool,
    pub account: AccountView,
    /// The sessions open in this server run, so a reload continues without opening the
    /// project again (AD22).
    pub sessions: Vec<OpenSession>,
    pub prefs: AppPrefsView,
    pub startup: StartupState,
    /// INV-A-FAILED-CHECKPOINT-IS-SHOWN-UNTIL-RETRIED (D1): each project of this server run whose
    /// last checkpoint failed, so a reload shows "Checkpoint not saved" again.
    pub checkpoints: Vec<FailedCheckpoint>,
}

/// A project whose last checkpoint failed, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct FailedCheckpoint {
    pub project: ProjectRef,
    pub reason: String,
}

/// The build the server runs. A UI built from another commit reloads once (§3.10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct BuildInfo {
    /// The version the About dialog shows.
    pub version: String,
    pub commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct OpenSession {
    pub at: Scope,
    /// With the folder, the key of the UI's buffered drafts (§6.6).
    pub id: ProjectId,
    pub mode: SessionMode,
    pub role: SessionRole,
    /// INV-A-CHANGED-BOOK-IS-ASKED-NOT-TAKEN, INV-A-KEPT-COPY-STAYS-ON-A-SCREEN (D1): the books
    /// the question asks about and the kept copies, so a reload asks and lists them again.
    pub outside: crate::session::OutsideEdits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum SessionRole {
    /// The project the screens show. There is at most one.
    Screen,
    /// A project a sync or an export from Home holds open.
    Side,
}

/// The app's own preferences (`prefs/app.json`, §4.9) that the UI needs before its first
/// render.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct AppPrefsView {
    /// The saved app language, English until one is chosen; it sets `<html lang dir>`
    /// before the first paint.
    pub app_locale: LocaleId,
}

/// `app/quit`: saves aligner links, checkpoints every session and exits, as a closed
/// stdin does (§5.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Quit {
    pub request: RequestId,
}

/// `app/clientError`: the UI's panic hook reports the panic before the page reloads
/// (§6.8). The server adds it to the report log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientError {
    pub request: RequestId,
    /// The view that was showing, by tC4's view name.
    pub view: Option<String>,
    pub project: Option<ProjectRef>,
    pub message: String,
    /// The browser's clock when the panic happened.
    pub at: WallMs,
}
