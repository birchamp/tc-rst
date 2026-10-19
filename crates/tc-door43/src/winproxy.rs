//! The proxy Windows itself picks for a server, as Edge does: the user's Internet Options for
//! the active connection (so a VPN with its own proxy counts), then WPAD (DHCP, then
//! `wpad.<domain>`) and the PAC script, run by WinHTTP's own auto-proxy service.
//!
//! `proxy-watch` makes the WinHTTP calls (`WinHttpGetIEProxyConfigForCurrentUser`, then
//! `WinHttpGetProxyForUrlEx`). This workspace forbids `unsafe` (AGENTS.md), and that crate is
//! the one published crate that asks WinHTTP for the whole lookup through a safe API, with a
//! time limit on each call. It is pinned to one version (the workspace Cargo.toml says how to
//! upgrade it). The survey of the alternatives, the audit of its Windows code and the
//! conditions this module meets are in `docs/evidence/proxy-watch-audit-2026-10-19.md`.

use std::time::Duration;

use proxy_watch::pac::WinHttpPacResolver;
use proxy_watch::{Error, ProxyConfig, ProxyConfigSource, ProxyMode, ProxyStep};
use url::Url;

use crate::http::{ProxyAnswer, ProxyFinder};

/// The longest one lookup waits, a WPAD search on a network without a WPAD host or a PAC
/// script on a slow server among them. WinHTTP is given the same limit, and the call is
/// cancelled after it.
const LOOKUP_LIMIT: Duration = Duration::from_secs(10);

/// The variables that name a proxy for https requests. When one is set it decides, as it
/// does for reqwest and curl, so this finder then has no answer.
const PROXY_VARIABLES: [&str; 4] = ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"];

/// Asks WinHTTP which proxy serves a server.
pub struct WinHttpFinder {
    settings: Settings,
}

enum Settings {
    /// The signed-in user's Internet Options, read again for every lookup.
    CurrentUser,
    /// A PAC script at a fixed address, for a test on this computer.
    PacScript(Url),
}

impl WinHttpFinder {
    /// The finder the clients use on Windows: the signed-in user's own settings.
    pub fn current_user() -> Self {
        Self {
            settings: Settings::CurrentUser,
        }
    }

    /// A finder that runs the PAC script at `script` instead of reading the user's settings,
    /// so a test can serve a script on this computer. The `*_PROXY` variables still come
    /// first. WinHTTP refuses a `file:` URL; serve the script over http.
    pub fn with_pac_script(script: Url) -> Self {
        Self {
            settings: Settings::PacScript(script),
        }
    }

    fn lookup(&self, origin: &Url) -> Result<Vec<ProxyStep>, Error> {
        let settings = match &self.settings {
            Settings::CurrentUser => proxy_watch::read()?,
            Settings::PacScript(url) => {
                ProxyConfig::from_source(ProxyConfigSource::Registry, ProxyMode::pac(url.clone()))
            }
        };
        // A resolver of its own for every lookup: WinHTTP remembers a failed WPAD search for
        // the life of a session, and the computer may have joined another network since.
        WinHttpPacResolver::with_timeout(LOOKUP_LIMIT)?
            .with_wpad(true)
            .resolve_config(&settings, origin)
    }
}

impl ProxyFinder for WinHttpFinder {
    fn proxy_for(&self, origin: &Url) -> Option<ProxyAnswer> {
        if PROXY_VARIABLES
            .iter()
            .any(|name| std::env::var_os(name).is_some_and(|v| !v.is_empty()))
        {
            return None;
        }
        match self.lookup(origin) {
            Ok(steps) => answer(&steps),
            // No script could be fetched or run, or the limit passed: no answer, so the
            // system's manual proxy setting decides, or the request goes directly, as in
            // Edge. Going directly here would pass over that manual proxy.
            Err(error) => {
                log_no_answer(&error);
                None
            }
        }
    }
}

/// The first step the clients can take: going directly, or a proxy whose URL can be written.
/// A step whose URL cannot be is passed over for the next one, as a browser passes over a
/// proxy it cannot use.
fn answer(steps: &[ProxyStep]) -> Option<ProxyAnswer> {
    steps.iter().find_map(|step| match step {
        ProxyStep::Direct => Some(ProxyAnswer::Direct),
        _ => step.to_url().map(ProxyAnswer::Through),
    })
}

/// Logs the kind of a failed lookup and the system's error code, and nothing else: the
/// error's message can carry the PAC script's URL, and with it a user name and password.
fn log_no_answer(error: &Error) {
    let kind = match error {
        Error::Io { .. } => "io",
        Error::PacTimeout { .. } => "pac-timeout",
        Error::PacEvaluation { .. } => "pac-evaluation",
        Error::PacInvalidResult { .. } => "pac-invalid-result",
        Error::PacNotSupported { .. } => "pac-not-supported",
        Error::InvalidProxyUrl { .. } => "invalid-proxy-url",
        _ => "other",
    };
    let os_error = match error {
        Error::Io { source, .. } => source.raw_os_error(),
        _ => None,
    };
    tracing::info!(
        kind,
        os_error,
        "windows proxy lookup failed: the manual proxy setting decides"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use proxy_watch::ProxyEndpoint;

    fn endpoint(text: &str) -> ProxyEndpoint {
        ProxyEndpoint::parse(text, 80).unwrap()
    }

    #[test]
    fn the_first_usable_step_is_the_answer() {
        let steps = [
            ProxyStep::Http(endpoint("http://a.test:8080")),
            ProxyStep::Direct,
        ];
        assert_eq!(
            answer(&steps),
            Some(ProxyAnswer::Through("http://a.test:8080".parse().unwrap()))
        );
        assert_eq!(answer(&[ProxyStep::Direct]), Some(ProxyAnswer::Direct));
        assert_eq!(answer(&[]), None);
    }

    #[test]
    fn a_socks_proxy_is_an_answer() {
        let steps = [ProxyStep::Socks4(endpoint("socks4://s.test:1080"))];
        assert_eq!(
            answer(&steps),
            Some(ProxyAnswer::Through(
                "socks4://s.test:1080".parse().unwrap()
            ))
        );
    }
}
