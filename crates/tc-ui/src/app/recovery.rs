//! What the panic hook leaves for `boot.js` and for the page after the reload (APP-DESIGN 6.8,
//! AD30, A-15).
//!
//! A panic in WebAssembly aborts the instance: every closure it registered traps on its next
//! call, and a reload it scheduled may never run. So the hook only writes the crash marker and
//! reports the panic, and `boot.js`, which is plain JavaScript, catches the first trap, counts
//! the page loads that ended in a panic and reloads, or after three within 10 s draws tC4's
//! view-error callout as static DOM and posts the buffered drafts.
//!
//! The keys here are the ones `app/recovery.js` reads; `tests/app/recovery.rs` keeps the two
//! files equal.

use serde::{Deserialize, Serialize};
use tc_api::ids::{ProjectRef, WallMs};

/// `sessionStorage`: the crash marker the panic hook writes.
pub const CRASH_KEY: &str = "tc4.crash.v1";
/// `sessionStorage`: the times of the page loads that ended in a panic. `boot.js` owns it.
pub const PANICS_KEY: &str = "tc4.panics.v1";
/// `sessionStorage`: the static callout's words in the shown language, which the page keeps
/// current, so `boot.js` can draw them when the WebAssembly cannot.
pub const VIEW_ERROR_TEXT_KEY: &str = "tc4.viewErrorText.v1";
/// `sessionStorage`: the navigation snapshot A5 writes and restores after a reload. "Back to
/// Home" in the static callout removes it, so the page that follows starts at Home.
pub const NAV_KEY: &str = "tc4.nav.v1";
/// `sessionStorage`: the server run the page last met (`hello.instance`). After three panics
/// `boot.js` posts the drafts of the entries of this run.
pub const INSTANCE_KEY: &str = "tc4.instance.v1";
/// The longest panic message the hook sends: the server keeps the first 8 KiB, and a request
/// sent with `keepalive` is capped at 64 KiB.
pub const MAX_CLIENT_MESSAGE: usize = 8 * 1024;

/// What the hook writes before the abort.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashMarker {
    /// The id `boot.js` gave the page load that panicked, so the next load can tell a marker
    /// a previous load left from its own.
    pub page: String,
    /// The view that was showing, by tC4's view name.
    pub view: Option<String>,
    pub project: Option<ProjectRef>,
    pub message: String,
    pub at: WallMs,
}

/// `message` cut to [`MAX_CLIENT_MESSAGE`] bytes at a character boundary, and whether it was
/// cut.
pub fn cut_message(message: &str) -> (String, bool) {
    if message.len() <= MAX_CLIENT_MESSAGE {
        return (message.to_owned(), false);
    }
    let mut end = MAX_CLIENT_MESSAGE;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    (message.get(..end).unwrap_or_default().to_owned(), true)
}

/// The crash marker a previous page load left, if the kept text is one and not this load's
/// own (`page` is this load's id). The page after a panic shows the view-error callout in
/// place of the screen that panicked (AD22).
pub fn follows_panic(marker: Option<&str>, page: &str) -> Option<CrashMarker> {
    let marker: CrashMarker = serde_json::from_str(marker?).ok()?;
    (marker.page != page).then_some(marker)
}

/// The words of the static callout, in the shown language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewErrorText {
    /// `<html lang>` and `dir` of the shown language, which a page that panicked at boot has
    /// not set yet.
    pub lang: String,
    pub dir: String,
    pub title: String,
    pub home: String,
}
