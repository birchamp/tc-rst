//! A backend that answers the minimum, honestly, until `tc-app` is joined to the server.
//!
//! `tc4 serve` runs the application's `App`; this stand-in stays as the backend of this
//! crate's own tests, because the server's behaviour (routing, security, streams, quit) must
//! be provable without an application. It answers `hello`, `quit`, the UI's error report and
//! the test hooks it can, and every other row with `Unavailable(Writes)`, so nothing it says
//! pretends to save. Every request body is parsed into its tc-api request type first, so it
//! never accepts what the application would refuse.
//!
//! With the `test-hooks` feature it also holds the crash points of AD35 until `tc-app`'s write
//! path replaces them: `test/crash` arms one, and the stand-in's one write that succeeds,
//! `app/clientError`, ends the process there. `app/quit` carries no crash point here; the
//! application's write path covers every write.

#[cfg(feature = "test-hooks")]
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(feature = "test-hooks")]
use std::sync::{Mutex, MutexGuard, PoisonError};

use axum::body::Bytes;
use tc_api::account::{AccountView, KeepState, KeychainState};
use tc_api::app::{
    AppPrefsView, BUILD_COMMIT, BUILD_VERSION, BuildInfo, ClientError, Hello, HelloView, Quit,
};
#[cfg(feature = "test-hooks")]
use tc_api::endpoint::Kind;
use tc_api::error::{ApiError, Feature};
use tc_api::event::StartupState;
use tc_api::ids::{LocaleId, ServerInstance};
#[cfg(feature = "test-hooks")]
use tc_api::test::{Crash, CrashPoint, Echo, EchoView, MAX_ECHO, Quiet, QuietView};
use tc_api::{API_VERSION, Endpoint, Reply};
use tokio::sync::watch;

use crate::ServerError;
use crate::backend::{Answer, Backend, EventSource, Next, Then};
#[cfg(feature = "test-hooks")]
use crate::hooks::{Ender, ExitProcess};
use crate::log::warn;

pub struct Stub {
    instance: ServerInstance,
    quit: watch::Sender<bool>,
    /// Requests being answered, `test/quiet` excluded: it asks whether any are.
    busy: AtomicUsize,
    #[cfg(feature = "test-hooks")]
    ender: Arc<dyn Ender>,
    /// The crash `test/crash` armed, if it has not fired yet.
    #[cfg(feature = "test-hooks")]
    armed: Mutex<Option<Armed>>,
}

#[cfg(feature = "test-hooks")]
struct Armed {
    at: CrashPoint,
    /// Only a write to this path; any write when `None`.
    path: Option<String>,
}

/// The one write the stand-in accepts.
#[cfg(feature = "test-hooks")]
const WRITE_PATH: &str = "app/clientError";

impl Stub {
    pub fn new() -> Result<Stub, ServerError> {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| ServerError::Random)?;
        Ok(Stub {
            instance: ServerInstance::from_bytes(bytes),
            quit: watch::channel(false).0,
            busy: AtomicUsize::new(0),
            #[cfg(feature = "test-hooks")]
            ender: Arc::new(ExitProcess),
            #[cfg(feature = "test-hooks")]
            armed: Mutex::new(None),
        })
    }

    /// Ends the process through `ender` at a crash point instead of with [`ExitProcess`].
    #[cfg(feature = "test-hooks")]
    #[must_use]
    pub fn with_ender(mut self, ender: Arc<dyn Ender>) -> Self {
        self.ender = ender;
        self
    }

    #[cfg(feature = "test-hooks")]
    fn armed(&self) -> MutexGuard<'_, Option<Armed>> {
        // An `Option` is replaced or taken whole, so a panic elsewhere cannot leave it half
        // changed.
        self.armed.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `test/crash`: arms one crash, replacing any armed one. Only a command or a job can be
    /// cut off, so only one of those names a path.
    #[cfg(feature = "test-hooks")]
    fn arm(&self, crash: Crash) -> Result<(), ApiError> {
        if let Some(path) = &crash.path {
            let writes = tc_api::ALL
                .iter()
                .any(|row| row.path == path && matches!(row.kind, Kind::Command | Kind::StartJob));
            if !writes {
                return Err(ApiError::BadRequest {
                    field: "path".to_owned(),
                    reason: "not a command or job endpoint".to_owned(),
                });
            }
        }
        *self.armed() = Some(Armed {
            at: crash.at,
            path: crash.path,
        });
        Ok(())
    }

    /// The crash that fires on this write, if one is armed for it: it fires once.
    /// `DuringDraftWrite` is accepted and never reached, because the stand-in writes no draft.
    #[cfg(feature = "test-hooks")]
    fn crash_for(&self, path: &str) -> Option<CrashPoint> {
        if path != WRITE_PATH {
            return None;
        }
        let mut armed = self.armed();
        let hit = armed.as_ref().is_some_and(|armed| {
            armed.at != CrashPoint::DuringDraftWrite
                && armed.path.as_deref().is_none_or(|only| only == path)
        });
        if hit {
            armed.take().map(|armed| armed.at)
        } else {
            None
        }
    }

    fn hello(&self) -> HelloView {
        HelloView {
            api: API_VERSION,
            build: BuildInfo {
                version: BUILD_VERSION.to_owned(),
                commit: BUILD_COMMIT.to_owned(),
            },
            instance: self.instance,
            internet: false,
            account: AccountView {
                user: None,
                kept: KeepState::NotKept,
                notice: None,
                server: "git.door43.org".to_owned(),
                keychain: KeychainState::Missing,
            },
            sessions: Vec::new(),
            prefs: AppPrefsView {
                app_locale: LocaleId::ENGLISH,
            },
            startup: StartupState::Ready,
        }
    }

    fn handle(&self, path: &'static str, body: &[u8]) -> Option<Vec<u8>> {
        Some(match path {
            "app/hello" => reply::<Hello>(body, |_| Ok(self.hello())),
            "app/quit" => reply::<Quit>(body, |_| {
                self.quit.send_replace(true);
                Ok(())
            }),
            "app/clientError" => reply::<ClientError>(body, |report| {
                // The view and the message only: the report may hold a project's text.
                warn(format_args!(
                    "the UI reported an error in {}: {}",
                    report.view.as_deref().unwrap_or("an unknown view"),
                    report.message
                ));
                Ok(())
            }),
            other => {
                return self
                    .test_rows(other, body)
                    .or_else(|| unavailable(other, body));
            }
        })
    }

    #[cfg(feature = "test-hooks")]
    fn test_rows(&self, path: &str, body: &[u8]) -> Option<Vec<u8>> {
        Some(match path {
            "test/echo" => reply::<Echo>(body, |echo| {
                if echo.size > MAX_ECHO {
                    return Err(ApiError::BadRequest {
                        field: "size".to_owned(),
                        reason: format!("at most {MAX_ECHO}"),
                    });
                }
                Ok(EchoView::of_size(echo.size))
            }),
            "test/crash" => reply::<Crash>(body, |crash| self.arm(crash)),
            "test/quiet" => reply::<Quiet>(body, |_| {
                Ok(QuietView {
                    busy: self.busy.load(Ordering::Acquire) > 0,
                })
            }),
            _ => return None,
        })
    }

    #[cfg(not(feature = "test-hooks"))]
    #[allow(clippy::unused_self)]
    fn test_rows(&self, _path: &str, _body: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

/// Parses `body` as the request type `E`, answers with `answer`'s result, and writes the
/// reply as the wire carries it.
fn reply<E: Endpoint>(body: &[u8], answer: impl FnOnce(E) -> Result<E::Ok, ApiError>) -> Vec<u8> {
    let reply: Reply<E::Ok> = match serde_json::from_slice::<E>(body) {
        Ok(request) => answer(request).into(),
        Err(error) => Reply::Err(ApiError::BadRequest {
            field: "body".to_owned(),
            reason: error.to_string(),
        }),
    };
    serde_json::to_vec(&reply).unwrap_or_default()
}

/// The rows the stand-in does not build, parsed like the application would and refused.
fn refuse<E: Endpoint>(body: &[u8]) -> Vec<u8> {
    reply::<E>(body, |_| Err(ApiError::Unavailable(Feature::Writes)))
}

macro_rules! unavailable_rows {
    ($name:ident $( $kind:ident $([$net:ident])? $path:literal : $req:path => $ok:ty; )*) => {
        fn $name(path: &str, body: &[u8]) -> Option<Vec<u8>> {
            match path {
                $( $path => Some(refuse::<$req>(body)), )*
                _ => None,
            }
        }
    };
}

tc_api::endpoint_table!(unavailable_rows unavailable_product);
#[cfg(feature = "test-hooks")]
tc_api::test_hook_table!(unavailable_rows unavailable_test);

fn unavailable(path: &str, body: &[u8]) -> Option<Vec<u8>> {
    #[cfg(feature = "test-hooks")]
    if let Some(answer) = unavailable_test(path, body) {
        return Some(answer);
    }
    unavailable_product(path, body)
}

struct Busy<'a>(&'a AtomicUsize);

impl<'a> Busy<'a> {
    fn enter(counter: &'a AtomicUsize) -> Self {
        counter.fetch_add(1, Ordering::AcqRel);
        Self(counter)
    }
}

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Backend for Stub {
    type Events = StubEvents;

    async fn call(&self, path: &'static str, body: Bytes) -> Answer {
        // `test/quiet` asks whether anything else is being answered, so it does not count.
        // Without the hooks there is no such row, and no such text in the binary.
        #[cfg(feature = "test-hooks")]
        let counted = path != "test/quiet";
        #[cfg(not(feature = "test-hooks"))]
        let counted = true;
        let _busy = counted.then(|| Busy::enter(&self.busy));
        #[cfg(feature = "test-hooks")]
        let crash = self.crash_for(path);
        #[cfg(feature = "test-hooks")]
        if crash == Some(CrashPoint::BeforeStore) {
            self.ender.end_now(CrashPoint::BeforeStore);
            return Answer::Ended;
        }
        let Some(body) = self.handle(path, &body) else {
            return Answer::NotFound;
        };
        #[cfg(feature = "test-hooks")]
        let then = match crash {
            Some(CrashPoint::BeforeReply) => {
                self.ender.end_now(CrashPoint::BeforeReply);
                return Answer::Ended;
            }
            Some(CrashPoint::AfterReply) => Then::EndProcess,
            _ => Then::Continue,
        };
        #[cfg(not(feature = "test-hooks"))]
        let then = Then::Continue;
        Answer::Reply { body, then }
    }

    fn subscribe(&self, _after: u64) -> StubEvents {
        StubEvents
    }

    async fn quit(&self) {
        self.quit.send_replace(true);
    }

    async fn quitting(&self) {
        let mut asked = self.quit.subscribe();
        // The sender lives as long as `self`, so the wait can only end by the value.
        let _ = asked.wait_for(|asked| *asked).await;
    }
}

/// The stand-in's event source: no frame ever comes, so the stream shows only its
/// heartbeat.
pub struct StubEvents;

impl EventSource for StubEvents {
    async fn next(&mut self) -> Next {
        std::future::pending().await
    }
}
