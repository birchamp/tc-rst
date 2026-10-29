//! What the page decides before it draws anything: the launch's token, and whether this
//! build and the server's are the same (APP-DESIGN 3.10, 5.2).

use std::fmt;

use tc_api::API_VERSION;
use tc_api::app::{BUILD_COMMIT, BUILD_VERSION, BuildInfo};

use super::client::CallError;

/// `sessionStorage` key of the launch's token, which a reload of the page keeps.
pub const TOKEN_KEY: &str = "tc4.token";
/// `sessionStorage` key of what the last reload for a build mismatch was for.
pub const RELOADED_FOR_KEY: &str = "tc4.reloadedFor";

/// The launch's token: 43 base64url characters, sent in `X-Tc-Token` with every request.
///
/// It is not logged and not shown; `Debug` hides it.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    /// `Some` for exactly 43 characters of the base64url alphabet.
    pub fn parse(text: &str) -> Option<Token> {
        let well_formed = text.len() == 43
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        well_formed.then(|| Token(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(..)")
    }
}

/// The token of this page: the address fragment's `t=` if it holds a valid one, else the
/// token the page kept from its launch. A new launch opens the page at a new fragment; a
/// reload may have lost it (the shell does not always keep it), but not the kept token.
pub fn token_from(fragment: &str, kept: Option<&str>) -> Option<Token> {
    fragment
        .strip_prefix('#')
        .unwrap_or(fragment)
        .split('&')
        .filter_map(|pair| pair.strip_prefix("t="))
        .find_map(Token::parse)
        .or_else(|| kept.and_then(Token::parse))
}

/// The line the page logs to the console at start, so a QA report can quote the running build
/// without a click (tC4 #278): this page's version and short commit, and the server's.
pub fn build_line(server: &BuildInfo) -> String {
    format!(
        "translationCore {BUILD_VERSION} ({}) \u{b7} server {} ({})",
        short_commit(BUILD_COMMIT),
        server.version,
        short_commit(&server.commit)
    )
}

/// The first 8 characters of a commit hash, as `git rev-parse --short` shows it; `dev` as it is.
fn short_commit(commit: &str) -> &str {
    commit.get(..8).unwrap_or(commit)
}

/// Whether this page and the server are one build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildCheck {
    Same,
    /// A different build: reload once. `marker` is what to keep in [`RELOADED_FOR_KEY`], so
    /// that a second look at the same mismatch does not reload again.
    ReloadOnce {
        marker: String,
    },
    /// A different build, and a reload already tried: a stale page, and the page stops with a
    /// message rather than reloading for ever.
    Stale {
        ui: String,
        server: String,
    },
}

fn describe(commit: &str, api: u32) -> String {
    format!("{commit} (API {api})")
}

/// Compares the server's build and API version (`app/hello`) with this build's. `reloaded_for`
/// is the marker a reload left in [`RELOADED_FOR_KEY`], if any.
pub fn check_build(server_commit: &str, server_api: u32, reloaded_for: Option<&str>) -> BuildCheck {
    if server_commit == BUILD_COMMIT && server_api == API_VERSION {
        return BuildCheck::Same;
    }
    let marker = format!("{server_commit}/{server_api}");
    if reloaded_for == Some(marker.as_str()) {
        BuildCheck::Stale {
            ui: describe(BUILD_COMMIT, API_VERSION),
            server: describe(server_commit, server_api),
        }
    } else {
        BuildCheck::ReloadOnce { marker }
    }
}

/// What the page does about a server of another build or API version (APP-DESIGN 3.10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildAction {
    /// The builds agree.
    Proceed,
    /// Reload the page, once: the marker that tells the page after the reload it has already
    /// tried was kept.
    Reload,
    /// Show the boot error naming both builds. The page already reloaded for this mismatch, or
    /// it could not keep the marker, so a reload would repeat for ever.
    Stop { ui: String, server: String },
}

/// [`check_build`], and the one rule about the marker that no caller may get wrong. `keep` writes
/// the marker of a reload and says whether it was written: when the page cannot keep it
/// (`sessionStorage` is blocked) the page after the reload would see none and reload again, for
/// ever, so the mismatch is a stale page at once.
///
/// The boot after `hello`, a call refused for the version and the event stream's open all decide
/// through this one function.
pub fn build_action(
    server_commit: &str,
    server_api: u32,
    reloaded_for: Option<&str>,
    keep: impl FnOnce(&str) -> bool,
) -> BuildAction {
    match check_build(server_commit, server_api, reloaded_for) {
        BuildCheck::Same => BuildAction::Proceed,
        BuildCheck::ReloadOnce { marker } if keep(&marker) => BuildAction::Reload,
        BuildCheck::ReloadOnce { .. } => stale(server_commit, server_api),
        BuildCheck::Stale { ui, server } => BuildAction::Stop { ui, server },
    }
}

/// What the page has decided about itself. Several requests can meet the same mismatch at once:
/// each one's [`build_action`] is right alone, but together they must not reload twice, show the
/// boot error while the page is leaving, or reload a page that has already stopped. The first
/// decision stands, and the event stream reads no more once there is one (APP-DESIGN 3.10).
#[derive(Debug, Default)]
pub struct Fate {
    decided: bool,
}

impl Fate {
    pub const fn new() -> Self {
        Self { decided: false }
    }

    /// Whether the page is reloading or has stopped with the boot error.
    pub fn is_settled(&self) -> bool {
        self.decided
    }

    /// `Some(action)` when the page is to carry `action` out: a reload or a stop, and only the
    /// first of them. `Proceed` decides nothing.
    pub fn take(&mut self, action: BuildAction) -> Option<BuildAction> {
        if self.decided || action == BuildAction::Proceed {
            return None;
        }
        self.decided = true;
        Some(action)
    }

    /// A page that cannot start or go on for another reason shows the boot error: true the first
    /// time only, and never for a page that is reloading.
    pub fn stop(&mut self) -> bool {
        !std::mem::replace(&mut self.decided, true)
    }
}

/// What the boot error says for a page that stops on a mismatch: both builds, so that the
/// person can tell which of the two is old.
pub fn stop_reason(ui: &str, server: &str) -> String {
    format!("this page is from build {ui}, but the app's server is build {server}")
}

fn stale(server_commit: &str, server_api: u32) -> BuildAction {
    BuildAction::Stop {
        ui: describe(BUILD_COMMIT, API_VERSION),
        server: describe(server_commit, server_api),
    }
}

/// The action for a call, or the event stream's open, that failed. `Some` only when the server
/// refused the API version: the server answers every `/api/` path with 409 first, the stream's
/// included. That error names the server's API version and not its commit, so the commit
/// compared is this page's own.
pub fn version_error_action(
    error: &CallError,
    reloaded_for: Option<&str>,
    keep: impl FnOnce(&str) -> bool,
) -> Option<BuildAction> {
    let server_api = error.version_mismatch()?;
    Some(build_action(BUILD_COMMIT, server_api, reloaded_for, keep))
}

/// Where a page that has met `hello` goes (APP-DESIGN 5.3, A5 D14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupAction {
    /// The resources are ready: the app starts.
    Proceed,
    /// The first launch is still preparing the resources, or it failed: the page goes to the
    /// first-launch window, `/prepare.html`, which shows the count or the failure and comes
    /// back at `Ready`.
    Prepare,
}

pub fn startup_action(state: &tc_api::event::StartupState) -> StartupAction {
    match state {
        tc_api::event::StartupState::Ready => StartupAction::Proceed,
        tc_api::event::StartupState::Preparing(_) | tc_api::event::StartupState::Failed { .. } => {
            StartupAction::Prepare
        }
    }
}
