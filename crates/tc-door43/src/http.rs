//! The one way the network clients build their HTTP client: [`crate::Door43Client`] and
//! `AquiferClient` in `tc-resources` both call [`Http::with_finder`], so the rules below
//! hold for both and are written once.
//!
//! - A client for a server on this computer (the test fake) is never proxied and may use
//!   plain http. Every other client is https only.
//! - Which proxy serves a server is asked of a [`ProxyFinder`], never when the client is
//!   built. Finding one can use the network (Windows looks for a WPAD host and downloads a
//!   PAC script), so [`Http::client`] asks only while the internet switch is on. It asks
//!   once for each origin the client may reach, and again each time the switch goes from
//!   off to on, because the computer may have joined another network meanwhile.
//! - A proxy the finder names is used for that origin. For every other origin the system's
//!   own settings apply: the `*_PROXY` variables, and on Windows the manual proxy setting.
//!
//! reqwest's system proxy on Windows reads only the manual proxy setting. A school or
//! office network that hands out a PAC script or a WPAD host (hazard 6 of
//! `docs/evidence/windows-build-2026-10-17.md`) therefore needs a finder that asks WinHTTP,
//! and that finder does not exist yet. It must call WinHTTP, which this workspace cannot do
//! while `unsafe` is forbidden (`AGENTS.md`): the owner chooses between an exception for
//! one small crate and a vetted dependency. [`system_proxy_finder`] is where it goes.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use url::Url;

use crate::internet::{InternetSwitch, Offline};

/// Asks the system which proxy serves a server.
pub trait ProxyFinder: Send + Sync + 'static {
    /// The proxy that serves `origin` (a scheme, host and port), or `None` when the system
    /// names none. This blocks, may take seconds and may use the network, so it runs on the
    /// blocking pool, and only while the internet switch is on.
    fn proxy_for(&self, origin: &Url) -> Option<Url>;
}

/// What the platform's own proxy settings say, for the clients that do not choose a finder.
/// `None` means that reqwest's system proxy is all there is: the `*_PROXY` variables, and
/// on Windows the manual proxy setting. No platform has a finder yet; see the module
/// documentation.
pub fn system_proxy_finder() -> Option<Arc<dyn ProxyFinder>> {
    None
}

/// How one client reaches the network.
pub struct HttpSpec {
    pub user_agent: &'static str,
    pub connect_timeout: Duration,
    /// The longest wait for the next bytes of an answer.
    pub read_timeout: Duration,
    pub redirects: reqwest::redirect::Policy,
    /// Every origin the client sends to, or may be redirected to.
    pub origins: Vec<Url>,
    /// The origins are a test server on this computer: plain http is allowed, and no proxy
    /// sees the traffic. Otherwise the client is https only.
    pub loopback: bool,
}

/// A reqwest client behind the internet switch. Clones share their proxy answers.
#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
    internet: InternetSwitch,
    routes: Option<Arc<Routes>>,
}

impl fmt::Debug for Http {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The reqwest client is left out: its Debug lists proxies, whose URLs can carry
        // credentials.
        f.debug_struct("Http")
            .field("proxied", &self.routes.is_some())
            .finish_non_exhaustive()
    }
}

impl Http {
    /// A client whose proxies `finder` names. Nothing is asked of it here.
    pub fn with_finder(
        spec: HttpSpec,
        internet: InternetSwitch,
        finder: Option<Arc<dyn ProxyFinder>>,
    ) -> Result<Self, reqwest::Error> {
        let mut builder = reqwest::Client::builder()
            .user_agent(spec.user_agent)
            .connect_timeout(spec.connect_timeout)
            .read_timeout(spec.read_timeout)
            .redirect(spec.redirects)
            .referer(false);
        let mut routes = None;
        if spec.loopback {
            builder = builder.no_proxy();
        } else {
            builder = builder.https_only(true);
            if let Some(finder) = finder {
                let found = Arc::new(RwLock::new(Found::default()));
                let lookup = found.clone();
                // Ahead of the system's own proxy, which reqwest adds last.
                builder = builder.proxy(reqwest::Proxy::custom(move |url| {
                    let found = lookup.read().unwrap_or_else(PoisonError::into_inner);
                    found.proxies.get(&origin_key(url)).cloned()
                }));
                routes = Some(Arc::new(Routes {
                    finder,
                    origins: spec.origins,
                    found,
                }));
            }
        }
        Ok(Self {
            client: builder.build()?,
            internet,
            routes,
        })
    }

    /// The client to send with, or [`Offline`] while the internet switch is off. The
    /// proxies are found here, so nothing looks for one while the switch is off.
    pub async fn client(&self) -> Result<&reqwest::Client, Offline> {
        self.internet.check()?;
        if let Some(routes) = &self.routes {
            routes.find(self.internet.times_turned_on()).await;
        }
        Ok(&self.client)
    }
}

struct Routes {
    finder: Arc<dyn ProxyFinder>,
    origins: Vec<Url>,
    found: Arc<RwLock<Found>>,
}

/// The proxies found, and the turn-on count they were found for.
#[derive(Default)]
struct Found {
    for_turn_on: Option<u64>,
    proxies: BTreeMap<String, Url>,
}

impl Routes {
    /// Asks the finder about every origin, unless it was asked since the switch was last
    /// turned on. Two requests that start together may both ask; the answers are the same.
    async fn find(&self, turn_on: u64) {
        let asked = self
            .found
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .for_turn_on;
        if asked == Some(turn_on) {
            return;
        }
        let (finder, origins) = (self.finder.clone(), self.origins.clone());
        let task = tokio::task::spawn_blocking(move || {
            origins
                .iter()
                .filter_map(|origin| Some((origin_key(origin), finder.proxy_for(origin)?)))
                .collect::<BTreeMap<_, _>>()
        });
        let proxies = match task.await {
            Ok(proxies) => proxies,
            // A panic in the finder is raised here, as if it had run on this task.
            Err(e) if e.is_panic() => std::panic::resume_unwind(e.into_panic()),
            // The runtime is shutting down.
            Err(_) => return,
        };
        *self.found.write().unwrap_or_else(PoisonError::into_inner) = Found {
            for_turn_on: Some(turn_on),
            proxies,
        };
    }
}

/// The scheme, host and port of `url`, which is what a proxy serves.
fn origin_key(url: &Url) -> String {
    url.origin().ascii_serialization()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A finder that names `proxy` for the origins in `proxied`, and counts its calls.
    struct Named {
        proxied: Vec<Url>,
        proxy: Url,
        asked: Mutex<Vec<String>>,
        calls: AtomicUsize,
    }

    impl Named {
        fn for_origins(proxied: &[&str], proxy: &str) -> Arc<Self> {
            Arc::new(Self {
                proxied: proxied.iter().map(|o| o.parse().unwrap()).collect(),
                proxy: proxy.parse().unwrap(),
                asked: Mutex::default(),
                calls: AtomicUsize::new(0),
            })
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }

        fn asked(&self) -> Vec<String> {
            self.asked.lock().unwrap().clone()
        }
    }

    impl ProxyFinder for Named {
        fn proxy_for(&self, origin: &Url) -> Option<Url> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.asked.lock().unwrap().push(origin_key(origin));
            self.proxied
                .iter()
                .any(|p| origin_key(p) == origin_key(origin))
                .then(|| self.proxy.clone())
        }
    }

    fn spec(origins: &[&str], loopback: bool) -> HttpSpec {
        HttpSpec {
            user_agent: "test",
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(5),
            redirects: reqwest::redirect::Policy::none(),
            origins: origins.iter().map(|o| o.parse().unwrap()).collect(),
            loopback,
        }
    }

    fn found_for(http: &Http, url: &str) -> Option<Url> {
        let routes = http.routes.as_ref().unwrap();
        let found = routes.found.read().unwrap();
        found
            .proxies
            .get(&origin_key(&url.parse().unwrap()))
            .cloned()
    }

    #[tokio::test]
    async fn nothing_is_asked_until_a_request_is_made_with_the_internet_on() {
        let finder = Named::for_origins(&["https://a.test"], "http://proxy.test:8080");
        let switch = InternetSwitch::new();
        let http = Http::with_finder(
            spec(&["https://a.test", "https://b.test"], false),
            switch.clone(),
            Some(finder.clone()),
        )
        .unwrap();
        assert_eq!(finder.calls(), 0, "building the client asks nothing");
        assert!(http.client().await.is_err());
        assert_eq!(finder.calls(), 0, "a refused request asks nothing");
        switch.turn_on();
        http.client().await.unwrap();
        assert_eq!(finder.asked(), ["https://a.test", "https://b.test"]);
        assert_eq!(
            found_for(&http, "https://a.test/x"),
            Some("http://proxy.test:8080".parse().unwrap())
        );
        assert_eq!(found_for(&http, "https://b.test/x"), None);
    }

    #[tokio::test]
    async fn the_answers_are_kept_until_the_internet_is_turned_off_and_on() {
        let finder = Named::for_origins(&["https://a.test"], "http://proxy.test:8080");
        let switch = InternetSwitch::new();
        let http = Http::with_finder(
            spec(&["https://a.test"], false),
            switch.clone(),
            Some(finder.clone()),
        )
        .unwrap();
        switch.turn_on();
        http.client().await.unwrap();
        http.client().await.unwrap();
        assert_eq!(finder.calls(), 1, "the second request reuses the answer");
        switch.turn_off();
        assert!(http.client().await.is_err());
        switch.turn_on();
        http.client().await.unwrap();
        assert_eq!(finder.calls(), 2, "another network may serve another proxy");
    }

    #[tokio::test]
    async fn a_server_on_this_computer_is_never_asked_about() {
        let finder = Named::for_origins(&["http://127.0.0.1:1"], "http://proxy.test:8080");
        let switch = InternetSwitch::new();
        switch.turn_on();
        let http = Http::with_finder(
            spec(&["http://127.0.0.1:1"], true),
            switch,
            Some(finder.clone()),
        )
        .unwrap();
        http.client().await.unwrap();
        assert_eq!(finder.calls(), 0);
        assert!(http.routes.is_none());
    }

    #[tokio::test]
    async fn a_panic_in_the_finder_is_not_hidden() {
        struct Broken;
        impl ProxyFinder for Broken {
            fn proxy_for(&self, _: &Url) -> Option<Url> {
                panic!("the finder broke")
            }
        }
        let switch = InternetSwitch::new();
        switch.turn_on();
        let http = Http::with_finder(
            spec(&["https://a.test"], false),
            switch,
            Some(Arc::new(Broken)),
        )
        .unwrap();
        let task = tokio::spawn(async move { http.client().await.map(|_| ()) });
        assert!(task.await.unwrap_err().is_panic());
    }
}
