//! What the page decides before it draws anything: the launch's token, and whether this
//! build and the server's are the same (APP-DESIGN 3.10, 5.2).

use std::fmt;

use tc_api::API_VERSION;
use tc_api::app::BUILD_COMMIT;

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

/// The verdict when the page cannot keep the marker of a reload (`sessionStorage` is blocked):
/// the next load would see no marker and reload again, for ever, so the mismatch is a stale
/// page at once. Always [`BuildCheck::Stale`] for a mismatch, and [`BuildCheck::Same`] for none.
pub fn unkept_marker(server_commit: &str, server_api: u32) -> BuildCheck {
    let marker = format!("{server_commit}/{server_api}");
    check_build(server_commit, server_api, Some(&marker))
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
