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
//! - What the finder answers for an origin decides: a proxy, or going directly. For an origin
//!   it has no answer for, the system's own settings apply: the `*_PROXY` variables, and on
//!   Windows the manual proxy setting. reqwest drops its system proxy as soon as the client
//!   has a proxy of its own, so [`Http`] reads those settings itself, once when the client is
//!   built, with the matcher reqwest uses (`SystemProxy`), and falls back to them.
//!
//! reqwest's system proxy on Windows reads only the manual proxy setting. A school or
//! office network that hands out a PAC script or a WPAD host (hazard 6 of
//! `docs/evidence/windows-build-2026-10-17.md`) needs WinHTTP's answer, so on Windows
//! [`system_proxy_finder`] is [`crate::WinHttpFinder`].

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use base64::Engine as _;
use base64::prelude::BASE64_STANDARD;
use hyper_util::client::proxy::matcher::Matcher;
use url::Url;

use crate::internet::{InternetSwitch, Offline};

/// What a [`ProxyFinder`] says about one origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProxyAnswer {
    /// Requests to the origin go through this proxy. The URL can carry the proxy's user name
    /// and password, so it is never logged.
    Through(Url),
    /// Requests to the origin go directly, whatever the system's manual proxy setting says: a
    /// PAC script that answers `DIRECT` overrides it, as it does in a browser.
    Direct,
}

/// Asks the system which proxy serves a server.
pub trait ProxyFinder: Send + Sync + 'static {
    /// The proxy that serves `origin` (a scheme, host and port), going directly, or `None`
    /// when this finder has no answer. Then the system's own settings decide (the `*_PROXY`
    /// variables, and on Windows and macOS the manual proxy setting), so a finder need not
    /// repeat them. This blocks, may take seconds and may use the network, so it runs on the
    /// blocking pool, and only while the internet switch is on.
    fn proxy_for(&self, origin: &Url) -> Option<ProxyAnswer>;
}

/// What the platform's own proxy settings say, for the clients that do not choose a finder.
/// On Windows that is [`crate::WinHttpFinder`], which asks WinHTTP (WPAD and PAC included).
/// Elsewhere it is `None`: reqwest's system proxy is all there is, the `*_PROXY` variables
/// and, on macOS, the manual proxy setting.
pub fn system_proxy_finder() -> Option<Arc<dyn ProxyFinder>> {
    #[cfg(windows)]
    {
        Some(Arc::new(crate::WinHttpFinder::current_user()))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// How one client reaches the network.
pub struct HttpSpec {
    pub user_agent: &'static str,
    pub connect_timeout: Duration,
    /// The longest wait for the next bytes of an answer.
    pub read_timeout: Duration,
    pub redirects: RedirectRule,
    /// Every origin the client sends to, or may be redirected to.
    pub origins: Vec<Url>,
    /// The origins are a test server on this computer: plain http is allowed, and no proxy
    /// sees the traffic. Otherwise the client is https only.
    pub loopback: bool,
}

/// Which redirects a client follows: to one of `origins`, or over https to one of
/// `https_hosts` (any port), at most `max` in a row. A redirect that carries a user name or
/// a password is never followed.
#[derive(Clone, Debug)]
pub struct RedirectRule {
    pub origins: Vec<Url>,
    pub https_hosts: Vec<String>,
    pub max: usize,
}

impl RedirectRule {
    fn allows(&self, url: &Url) -> bool {
        let named = self.origins.iter().any(|o| o.origin() == url.origin())
            || (url.scheme() == "https"
                && url
                    .host_str()
                    .is_some_and(|h| self.https_hosts.iter().any(|a| h.eq_ignore_ascii_case(a))));
        named && url.username().is_empty() && url.password().is_none()
    }

    fn policy(self) -> reqwest::redirect::Policy {
        reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= self.max {
                attempt.error("too many redirects")
            } else if self.allows(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("a redirect this client does not follow")
            }
        })
    }
}

/// Why a request did not get an answer, or its body could not be read.
#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("the internet is off")]
    Offline,
    #[error("the server did not answer in time")]
    Timeout,
    #[error("the server could not be reached")]
    Connect,
    #[error("the answer broke off")]
    Interrupted,
    #[error("the server redirected the request where this client does not follow")]
    Redirect,
    #[error("the answer is larger than {limit} bytes")]
    TooLarge { limit: u64 },
    #[error("{0}")]
    Other(String),
}

impl From<Offline> for HttpError {
    fn from(_: Offline) -> Self {
        Self::Offline
    }
}

impl From<reqwest::Error> for HttpError {
    fn from(error: reqwest::Error) -> Self {
        if error.is_redirect() {
            Self::Redirect
        } else if error.is_timeout() {
            Self::Timeout
        } else if error.is_connect() {
            Self::Connect
        } else if error.is_body() || error.is_decode() {
            Self::Interrupted
        } else {
            Self::Other(error.without_url().to_string())
        }
    }
}

/// An answer whose status and headers have arrived; its body is read with
/// [`HttpResponse::body`], which stops when the internet switch goes off.
pub struct HttpResponse {
    response: reqwest::Response,
    internet: InternetSwitch,
}

impl fmt::Debug for HttpResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpResponse")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

impl HttpResponse {
    pub fn status(&self) -> u16 {
        self.response.status().as_u16()
    }

    /// The header's value, when it is present and readable as text.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.response.headers().get(name)?.to_str().ok()
    }

    /// The whole body, refused as soon as it is known to exceed `limit` bytes.
    pub async fn body(mut self, limit: u64) -> Result<Vec<u8>, HttpError> {
        if self
            .response
            .content_length()
            .is_some_and(|len| len > limit)
        {
            return Err(HttpError::TooLarge { limit });
        }
        let mut body = Vec::new();
        while let Some(chunk) = self.response.chunk().await? {
            self.internet.check()?;
            if (body.len() + chunk.len()) as u64 > limit {
                return Err(HttpError::TooLarge { limit });
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }
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
    /// A client whose proxies `finder` names, and the system's own settings for any origin
    /// it names none for. Nothing is asked of the finder here.
    pub fn with_finder(
        spec: HttpSpec,
        internet: InternetSwitch,
        finder: Option<Arc<dyn ProxyFinder>>,
    ) -> Result<Self, HttpError> {
        let mut builder = reqwest::Client::builder()
            .user_agent(spec.user_agent)
            .connect_timeout(spec.connect_timeout)
            .read_timeout(spec.read_timeout)
            .redirect(spec.redirects.policy())
            .referer(false);
        let mut routes = None;
        if spec.loopback {
            builder = builder.no_proxy();
        } else {
            builder = builder.https_only(true);
            if let Some(finder) = finder {
                let found = Arc::new(RwLock::new(Found::default()));
                let lookup = found.clone();
                // Any proxy of its own makes reqwest leave out the system's, so the
                // closure falls back to it.
                let system = SystemProxy::from_system();
                builder = builder.proxy(reqwest::Proxy::custom(move |url| {
                    choose(&lookup, &system, url)
                }));
                routes = Some(Arc::new(Routes {
                    finder,
                    origins: spec.origins,
                    found,
                    limit_per_origin: FIND_LIMIT_PER_ORIGIN,
                }));
            }
        }
        Ok(Self {
            client: builder.build()?,
            internet,
            routes,
        })
    }

    /// Sends a GET with `accept` and `headers`. Only a success status (2xx) comes back as
    /// an answer the caller reads the body of; any other status comes back too, for the
    /// caller to read its headers.
    pub async fn get(
        &self,
        url: Url,
        accept: &str,
        headers: &[(&str, &str)],
    ) -> Result<HttpResponse, HttpError> {
        let mut request = self
            .client()
            .await?
            .get(url)
            .header(reqwest::header::ACCEPT, accept);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        Ok(HttpResponse {
            response: request.send().await?,
            internet: self.internet.clone(),
        })
    }

    /// The client to send with, or [`Offline`] while the internet switch is off. The
    /// proxies are found here, so nothing looks for one while the switch is off.
    pub(crate) async fn client(&self) -> Result<&reqwest::Client, Offline> {
        let Some(routes) = &self.routes else {
            self.internet.check()?;
            return Ok(&self.client);
        };
        loop {
            let turn_on = self.internet.turn_on_count()?;
            routes.find(turn_on).await;
            // The lookup can take seconds. A switch turned off meanwhile refuses this
            // request, and one turned off and on has moved the computer to a network the
            // answers may not fit.
            if self.internet.turn_on_count()? == turn_on {
                return Ok(&self.client);
            }
        }
    }
}

/// The proxy for `url`: what the finder answered for its origin, else the system's.
fn choose(found: &RwLock<Found>, system: &SystemProxy, url: &Url) -> Option<Url> {
    let answer = found
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .proxies
        .get(&origin_key(url))
        .cloned();
    match answer {
        Some(ProxyAnswer::Through(proxy)) => Some(proxy),
        Some(ProxyAnswer::Direct) => None,
        None => system.proxy_for(url),
    }
}

/// The system's own proxy settings: what reqwest's system proxy would apply, were it not
/// dropped for a client that has a proxy of its own. They are read when this is made.
struct SystemProxy(Matcher);

impl SystemProxy {
    /// The `*_PROXY` and `NO_PROXY` variables and, where hyper-util reads it, the system's
    /// manual proxy setting (the variables win).
    fn from_system() -> Self {
        Self(Matcher::from_system())
    }

    /// The proxy that serves `url`, with its user name and password in the URL, which is
    /// how a proxy is handed to reqwest.
    fn proxy_for(&self, url: &Url) -> Option<Url> {
        let served = self.0.intercept(&url.as_str().parse().ok()?)?;
        let mut proxy: Url = served.uri().to_string().parse().ok()?;
        let credentials = match (served.basic_auth(), served.raw_auth()) {
            (Some(basic), _) => {
                // hyper-util wrote this header itself, from a user name and a password.
                let basic = basic.to_str().ok()?.strip_prefix("Basic ")?;
                let text = String::from_utf8(BASE64_STANDARD.decode(basic).ok()?).ok()?;
                let (user, password) = text.split_once(':')?;
                Some((user.to_owned(), password.to_owned()))
            }
            (None, Some((user, password))) => Some((user.to_owned(), password.to_owned())),
            (None, None) => None,
        };
        if let Some((user, password)) = credentials {
            proxy.set_username(&user).ok()?;
            proxy
                .set_password((!password.is_empty()).then_some(&password[..]))
                .ok()?;
        }
        Some(proxy)
    }
}

struct Routes {
    finder: Arc<dyn ProxyFinder>,
    origins: Vec<Url>,
    found: Arc<RwLock<Found>>,
    limit_per_origin: Duration,
}

/// The finder's answers, and the turn-on count they were found for.
#[derive(Default)]
struct Found {
    for_turn_on: Option<u64>,
    proxies: BTreeMap<String, ProxyAnswer>,
}

/// The longest wait for the finder's answer about one origin. A finder bounds its own wait
/// (the Windows one tells WinHTTP 10 s), but not every call it makes; past this, the origin
/// has no answer until the switch is next turned on.
const FIND_LIMIT_PER_ORIGIN: Duration = Duration::from_secs(15);

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
        let limit = self
            .limit_per_origin
            .saturating_mul(u32::try_from(origins.len()).unwrap_or(u32::MAX));
        let task = tokio::task::spawn_blocking(move || {
            origins
                .iter()
                .filter_map(|origin| Some((origin_key(origin), finder.proxy_for(origin)?)))
                .collect::<BTreeMap<_, _>>()
        });
        let proxies = match tokio::time::timeout(limit, task).await {
            Ok(Ok(proxies)) => proxies,
            // A panic in the finder is raised here, as if it had run on this task.
            Ok(Err(e)) if e.is_panic() => std::panic::resume_unwind(e.into_panic()),
            // The runtime is shutting down.
            Ok(Err(_)) => return,
            // A lookup that never ends: the system's own settings serve every origin, and it is
            // not started again for each request. Its thread finishes on its own.
            Err(_) => BTreeMap::new(),
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
        fn proxy_for(&self, origin: &Url) -> Option<ProxyAnswer> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.asked.lock().unwrap().push(origin_key(origin));
            self.proxied
                .iter()
                .any(|p| origin_key(p) == origin_key(origin))
                .then(|| ProxyAnswer::Through(self.proxy.clone()))
        }
    }

    fn spec(origins: &[&str], loopback: bool) -> HttpSpec {
        HttpSpec {
            user_agent: "test",
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(5),
            redirects: RedirectRule {
                origins: Vec::new(),
                https_hosts: Vec::new(),
                max: 0,
            },
            origins: origins.iter().map(|o| o.parse().unwrap()).collect(),
            loopback,
        }
    }

    /// The client with a shorter wait for its finder, for a test of that wait.
    fn with_limit(mut http: Http, limit: Duration) -> Http {
        let routes = http.routes.as_mut().and_then(Arc::get_mut).unwrap();
        routes.limit_per_origin = limit;
        http
    }

    fn found_for(http: &Http, url: &str) -> Option<ProxyAnswer> {
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
            Some(ProxyAnswer::Through(
                "http://proxy.test:8080".parse().unwrap()
            ))
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

    /// A finder that changes the switch during its first lookup, as a person can while a
    /// slow PAC lookup runs, and names no proxy.
    struct Interrupted {
        switch: InternetSwitch,
        on_again: bool,
        calls: AtomicUsize,
    }

    impl ProxyFinder for Interrupted {
        fn proxy_for(&self, _: &Url) -> Option<ProxyAnswer> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                self.switch.turn_off();
                if self.on_again {
                    self.switch.turn_on();
                }
            }
            None
        }
    }

    fn interrupted(on_again: bool) -> (Http, Arc<Interrupted>) {
        let switch = InternetSwitch::new();
        switch.turn_on();
        let finder = Arc::new(Interrupted {
            switch: switch.clone(),
            on_again,
            calls: AtomicUsize::new(0),
        });
        let http = Http::with_finder(
            spec(&["https://a.test"], false),
            switch,
            Some(finder.clone()),
        )
        .unwrap();
        (http, finder)
    }

    #[tokio::test]
    async fn a_request_is_refused_when_the_internet_goes_off_during_the_lookup() {
        let (http, finder) = interrupted(false);
        assert!(http.client().await.is_err());
        assert_eq!(finder.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn the_lookup_is_repeated_when_the_internet_goes_off_and_on_during_it() {
        let (http, finder) = interrupted(true);
        http.client().await.unwrap();
        assert_eq!(
            finder.calls.load(Ordering::SeqCst),
            2,
            "the first answers were found for the network before"
        );
        http.client().await.unwrap();
        assert_eq!(finder.calls.load(Ordering::SeqCst), 2);
    }

    /// A finder whose lookup never returns until the test ends, as a WinHTTP call that hangs.
    struct Stuck {
        release: Mutex<std::sync::mpsc::Receiver<()>>,
        calls: AtomicUsize,
    }

    impl ProxyFinder for Stuck {
        fn proxy_for(&self, _: &Url) -> Option<ProxyAnswer> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let _ = self.release.lock().unwrap().recv();
            Some(ProxyAnswer::Through(
                "http://late.test:8080".parse().unwrap(),
            ))
        }
    }

    #[tokio::test]
    async fn a_lookup_that_never_ends_is_given_up_and_not_started_again() {
        let (_hold, release) = std::sync::mpsc::channel::<()>();
        let finder = Arc::new(Stuck {
            release: Mutex::new(release),
            calls: AtomicUsize::new(0),
        });
        let switch = InternetSwitch::new();
        switch.turn_on();
        let http = Http::with_finder(
            spec(&["https://a.test"], false),
            switch,
            Some(finder.clone()),
        )
        .unwrap();
        let http = with_limit(http, Duration::from_millis(200));
        let started = std::time::Instant::now();
        http.client().await.unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(
            found_for(&http, "https://a.test/x"),
            None,
            "the system's settings serve it"
        );
        http.client().await.unwrap();
        assert_eq!(finder.calls.load(Ordering::SeqCst), 1, "not asked again");
    }

    fn system(https: &str, no: &str) -> SystemProxy {
        SystemProxy(Matcher::builder().https(https).no(no).build())
    }

    fn found_with(origin: &str, answer: ProxyAnswer) -> RwLock<Found> {
        RwLock::new(Found {
            for_turn_on: Some(1),
            proxies: BTreeMap::from([(origin.to_owned(), answer)]),
        })
    }

    fn through(proxy: &str) -> ProxyAnswer {
        ProxyAnswer::Through(proxy.parse().unwrap())
    }

    fn url(text: &str) -> Url {
        text.parse().unwrap()
    }

    #[test]
    fn the_proxy_the_finder_names_comes_before_the_systems() {
        let found = found_with("https://a.test", through("http://finder.test:8080"));
        let system = system("http://system.test:3128", "");
        assert_eq!(
            choose(&found, &system, &url("https://a.test/x")),
            Some(url("http://finder.test:8080"))
        );
    }

    #[test]
    fn the_systems_proxy_serves_an_origin_the_finder_names_none_for() {
        let found = found_with("https://a.test", through("http://finder.test:8080"));
        let system = system("http://system.test:3128", "");
        assert_eq!(
            choose(&found, &system, &url("https://b.test/x")),
            Some(url("http://system.test:3128"))
        );
    }

    #[test]
    fn going_directly_is_not_overridden_by_the_systems_proxy() {
        // A PAC script that answers DIRECT for a server: the manual proxy beneath it is not used.
        let found = found_with("https://a.test", ProxyAnswer::Direct);
        let system = system("http://system.test:3128", "");
        assert_eq!(choose(&found, &system, &url("https://a.test/x")), None);
        assert_eq!(
            choose(&found, &system, &url("https://b.test/x")),
            Some(url("http://system.test:3128")),
            "an origin the finder has no answer for still gets the system's proxy"
        );
    }

    #[test]
    fn no_proxy_still_excludes_a_host_from_the_systems_proxy() {
        let found = RwLock::new(Found::default());
        let system = system("http://system.test:3128", "b.test");
        assert_eq!(choose(&found, &system, &url("https://b.test/x")), None);
        assert_eq!(
            choose(&found, &system, &url("https://a.test/x")),
            Some(url("http://system.test:3128"))
        );
    }

    #[test]
    fn the_systems_proxy_keeps_its_user_name_and_password() {
        let found = RwLock::new(Found::default());
        let system = system("http://us%40er:p%3Ass%25@system.test:3128", "");
        let proxy = choose(&found, &system, &url("https://a.test/x")).unwrap();
        // reqwest hands this URL to the same matcher, which must read the same header.
        let rebuilt = Matcher::builder().all(proxy.as_str()).build();
        let asked = rebuilt
            .intercept(&"https://a.test/x".parse().unwrap())
            .unwrap();
        let original = system
            .0
            .intercept(&"https://a.test/x".parse().unwrap())
            .unwrap();
        assert_eq!(asked.uri(), original.uri());
        assert!(original.basic_auth().is_some());
        assert_eq!(asked.basic_auth(), original.basic_auth());
    }

    #[test]
    fn a_proxy_user_without_a_password_is_kept() {
        let found = RwLock::new(Found::default());
        let system = system("http://user@system.test:3128", "");
        let proxy = choose(&found, &system, &url("https://a.test/x")).unwrap();
        let asked = Matcher::builder()
            .all(proxy.as_str())
            .build()
            .intercept(&"https://a.test/x".parse().unwrap())
            .unwrap();
        // "user:" in base64.
        assert_eq!(asked.basic_auth().unwrap(), "Basic dXNlcjo=");
    }

    #[tokio::test]
    async fn a_panic_in_the_finder_is_not_hidden() {
        struct Broken;
        impl ProxyFinder for Broken {
            fn proxy_for(&self, _: &Url) -> Option<ProxyAnswer> {
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
