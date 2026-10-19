//! The proxy Windows itself picks for a server, as Edge does: the user's Internet Options for
//! the active connection (so a VPN with its own proxy counts), then WPAD (DHCP, then
//! `wpad.<domain>`) and the PAC script, run by WinHTTP's own auto-proxy service.
//!
//! `proxy-watch` makes the WinHTTP calls (`WinHttpGetIEProxyConfigForCurrentUser`, then
//! `WinHttpGetProxyForUrlEx`). This workspace forbids `unsafe` (AGENTS.md), and that crate is
//! the one published crate that asks WinHTTP for the whole lookup through a safe API, with a
//! time limit on each call. It is pinned to one version (Cargo.toml says how to upgrade it).
//! The survey of the alternatives and the audit of its Windows code are in
//! `docs/evidence/windows-build-2026-10-17.md`, hazard 6.

use std::time::Duration;

use proxy_watch::pac::WinHttpPacResolver;
use proxy_watch::{ProxyConfig, ProxyConfigSource, ProxyMode, ProxyStep};
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

    /// A finder that runs the PAC script at `script` and reads neither the user's settings nor
    /// the environment, so a test can serve a script on this computer. WinHTTP refuses a
    /// `file:` URL; serve the script over http.
    pub fn with_pac_script(script: Url) -> Self {
        Self {
            settings: Settings::PacScript(script),
        }
    }

    fn settings(&self) -> Option<ProxyConfig> {
        match &self.settings {
            Settings::CurrentUser => {
                if PROXY_VARIABLES
                    .iter()
                    .any(|name| std::env::var_os(name).is_some_and(|v| !v.is_empty()))
                {
                    return None;
                }
                proxy_watch::read().ok()
            }
            Settings::PacScript(url) => Some(ProxyConfig::from_source(
                ProxyConfigSource::Registry,
                ProxyMode::pac(url.clone()),
            )),
        }
    }
}

impl ProxyFinder for WinHttpFinder {
    fn proxy_for(&self, origin: &Url) -> Option<ProxyAnswer> {
        let settings = self.settings()?;
        // A resolver of its own for every lookup: WinHTTP remembers a failed WPAD search for
        // the life of a session, and the computer may have joined another network since.
        let resolver = WinHttpPacResolver::with_timeout(LOOKUP_LIMIT)
            .ok()?
            .with_wpad(true);
        // Any failure (no PAC script could be fetched or run, the limit passed) is no answer,
        // so the system's manual proxy setting decides, or the request goes directly, as in
        // Edge. The error is not logged: its PAC URL can carry a user name and password.
        let steps = resolver.resolve_config(&settings, origin).ok()?;
        answer(&steps)
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
            Some(ProxyAnswer::Through("socks4://s.test:1080".parse().unwrap()))
        );
    }
}
