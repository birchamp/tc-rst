//! Routing and running a request (APP-DESIGN §3.3, §4.4, AD4, AD9, AD25).
//!
//! `tc_api::endpoint_table!` lists every endpoint once. [`router!`] turns the list into a
//! route, and a row without `impl Handle<Row> for App` does not compile, so no endpoint
//! can lack a handler.
//!
//! A request goes through, in this order:
//!
//! 1. the body is parsed into the endpoint's request type, which refuses unknown keys: a
//!    body that does not parse is a `BadRequest` and is never cached, because it carries no
//!    trusted `RequestId`;
//! 2. the internet check: an endpoint that needs the internet answers `NeedsInternet` while
//!    the switch is off, and that answer says nothing was sent (§3.5). It is never kept under
//!    the request's id, because the UI asks the question, turns the switch on and sends *the
//!    same request again*; a kept refusal would answer that resend forever. A write hands the
//!    refusal to the reply cache instead of answering it first, because the cache alone knows
//!    whether the id was sent: a retry of a write that ran, after the switch went off, gets
//!    that write's reply and not a refusal that says it never happened;
//! 3. a query runs in the caller's task and changes nothing;
//! 4. a write that names a project (`Write::at`) is checked against the screen's open
//!    session before anything else (§4.3): a closed or replaced scope is
//!    `NotSaved(StaleScope)`, answered before the internet check and never kept under the
//!    request's id, because the UI resends the same id once it has reattached. The write
//!    then holds the session open until it ends, whoever waits for its reply;
//! 5. a write goes through the [`ReplyCache`](crate::replies::ReplyCache) of its session, or
//!    the app-wide one when it names no project: the first attempt runs on a task of its
//!    own, so a dropped connection never stops it half way, and a retry gets that attempt's
//!    reply. Every answer of a write is cached, refusals included: a retry after a lost reply
//!    must get the same refusal, not run the write again. The one exception is `test/reset`
//!    ([`kept_reply`]).

use std::future::Future;

use serde_json::json;
use tc_api::{ApiError, Endpoint, NotSaved, Reply, Write};

use crate::app::{Answer, App, Then};
use crate::jobs::stop_reason;
use crate::replies::{BodyDigest, CacheAnswer, Keep, Outcome};
use crate::reports::facts;
#[cfg(feature = "test-hooks")]
use tc_api::test::{CrashPoint, Reset};

/// What the app does for one endpoint. Impls write `async fn handle(&self, req: X) ->
/// Result<…, ApiError>`, which satisfies this signature while the body holds nothing that is
/// not `Send` across an await.
pub trait Handle<E: Endpoint> {
    fn handle(&self, req: E) -> impl Future<Output = Result<E::Ok, ApiError>> + Send;
}

/// What the reply cache keeps of the write at `path`. A reply is kept for every write but
/// `test/reset`: it sets the app back to a launch's state, so running it again changes
/// nothing, and the parity harness sends it first in every scenario with the id it used in
/// the one before. A kept reply would answer that reset without resetting anything.
fn kept_reply(path: &str) -> Keep {
    #[cfg(feature = "test-hooks")]
    if path == Reset::PATH {
        return Keep::Nothing;
    }
    let _ = path;
    Keep::Reply
}

/// The longest reason of a `BadRequest`: serde's message can repeat a key the body sent.
const MAX_REASON: usize = 300;

macro_rules! router {
    (fn $name:ident $( $kind:ident $([$net:ident])? $path:literal : $req:path => $ok:ty; )*) => {
        impl App {
            /// The answer to a product endpoint, or `None` when `path` is not one.
            pub(crate) async fn $name(&self, path: &str, body: &[u8]) -> Option<Answer> {
                match path {
                    $( $path => Some(router!(@run $kind self body $path => $req)), )*
                    _ => None,
                }
            }
        }
    };
    // `self` and `body` are passed in: a name written inside this rule would not be the
    // function's own.
    (@run query $app:ident $body:ident $path:literal => $req:path) => {
        $app.run_query::<$req>($body).await
    };
    (@run command $app:ident $body:ident $path:literal => $req:path) => {
        $app.run_write::<$req>($path, $body).await
    };
    (@run job $app:ident $body:ident $path:literal => $req:path) => {
        $app.run_write::<$req>($path, $body).await
    };
}

tc_api::endpoint_table!(router fn route_product);
#[cfg(feature = "test-hooks")]
tc_api::test_hook_table!(router fn route_test_hooks);

impl App {
    /// Parses a request body. The reason is cut, so a body cannot make a long reply.
    fn parse<E: Endpoint>(body: &[u8]) -> Result<E, ApiError> {
        serde_json::from_slice::<E>(body).map_err(|e| ApiError::BadRequest {
            field: "body".to_owned(),
            reason: e.to_string().chars().take(MAX_REASON).collect(),
        })
    }

    /// The internet check of AD25. `None` when the request may go on.
    fn needs_internet<E: Endpoint>(&self) -> Option<ApiError> {
        let kind = E::NET?;
        (!self.inner.internet.is_on()).then_some(ApiError::NeedsInternet(kind))
    }

    pub(crate) async fn run_query<E>(&self, body: &[u8]) -> Answer
    where
        E: Endpoint,
        App: Handle<E>,
    {
        let req = match Self::parse::<E>(body) {
            Ok(req) => req,
            Err(error) => return self.error_answer(error),
        };
        if let Some(error) = self.needs_internet::<E>() {
            return self.error_answer(error);
        }
        let reply = Reply::from(self.handle(req).await);
        #[cfg(feature = "test-hooks")]
        if self.is_dead() {
            return Answer::Ended;
        }
        Answer::Reply {
            body: self.reply_bytes(&reply, E::PATH),
            then: Then::Continue,
        }
    }

    pub(crate) async fn run_write<E>(&self, path: &'static str, body: &[u8]) -> Answer
    where
        E: Write + Send,
        E::Ok: Send + 'static,
        App: Handle<E>,
    {
        let req = match Self::parse::<E>(body) {
            Ok(req) => req,
            Err(error) => return self.error_answer(error),
        };
        // A project's write runs only in the screen's open session, and the lease keeps that
        // session open until the write ends. A refusal here is never cached: the same id comes
        // back once the UI has reattached to the current session.
        let lease = match req.at() {
            Some(at) => match self.inner.sessions.writer(at) {
                Some(lease) => Some(lease),
                None => return self.error_answer(ApiError::NotSaved(NotSaved::StaleScope)),
            },
            None => None,
        };
        let replies = lease.as_ref().map_or_else(
            || self.inner.replies.clone(),
            |lease| lease.session().replies().clone(),
        );
        // The cache applies this only to an id it has not seen.
        let refuse = self.needs_internet::<E>();
        // The digest is of the request as serde writes it again, not of the body's bytes:
        // a resend with other spacing or key order is the same request.
        let canonical = match serde_json::to_vec(&req) {
            Ok(canonical) => canonical,
            Err(e) => {
                let error = self.internal(
                    "a request could not be written again",
                    facts(json!({"path": path, "reason": e.to_string()})),
                );
                return self.error_answer(error);
            }
        };
        let digest = BodyDigest::of(path, &canonical);
        let id = req.request();

        let app = self.clone();
        let work = async move {
            let _lease = lease;
            #[cfg(feature = "test-hooks")]
            if let Some(ended) = app.crash_at(CrashPoint::BeforeStore, path) {
                return ended;
            }
            // The handler runs on a task of its own, so a panic in it becomes an `Internal`
            // reply that is cached like any other, never a write that vanished.
            let handled = tokio::spawn({
                let app = app.clone();
                async move { app.handle(req).await }
            })
            .await;
            let reply = match handled {
                Ok(result) => Reply::from(result),
                Err(stopped) => Reply::Err(app.internal(
                    "a request handler stopped",
                    facts(json!({"path": path, "stopped": stop_reason(stopped)})),
                )),
            };
            // A write that ran on past the end of a test app never answers: the process that
            // answered it is gone.
            #[cfg(feature = "test-hooks")]
            if app.is_dead() {
                return Outcome::Ended;
            }
            let bytes = app.reply_bytes(&reply, path);
            #[cfg(feature = "test-hooks")]
            if let Some(ended) = app.crash_at(CrashPoint::BeforeReply, path) {
                return ended;
            }
            Outcome::Reply(bytes.into())
        };

        match replies
            .run(kept_reply(path), id, digest, refuse, work)
            .await
        {
            CacheAnswer::Fresh(outcome) => self.write_answer(outcome, true, path),
            CacheAnswer::Repeat(outcome) => self.write_answer(outcome, false, path),
            CacheAnswer::Reused => self.error_answer(ApiError::RequestReused),
            CacheAnswer::Refused(error) => self.error_answer(error),
            CacheAnswer::Lost => {
                let error = self.internal(
                    "a write ended without a reply",
                    facts(json!({"path": path})),
                );
                self.error_answer(error)
            }
        }
    }

    /// What the server does once a first attempt's reply is sent. Only a first attempt can end
    /// the process after its reply: a retry's reply is a copy.
    fn then_after(&self, path: &str) -> Then {
        #[cfg(feature = "test-hooks")]
        if self.crash_after_reply(path) {
            return Then::EndProcess;
        }
        let _ = path;
        Then::Continue
    }

    fn write_answer(&self, outcome: Outcome, first_attempt: bool, path: &str) -> Answer {
        match outcome {
            Outcome::Reply(bytes) => Answer::Reply {
                body: bytes.to_vec(),
                then: if first_attempt {
                    self.then_after(path)
                } else {
                    Then::Continue
                },
            },
            #[cfg(feature = "test-hooks")]
            Outcome::Ended => Answer::Ended,
        }
    }
}

#[cfg(test)]
mod tests {
    //! The one rule no product handler can show: a handler that panics.

    use serde::{Deserialize, Serialize};
    use tc_api::endpoint::Kind;
    use tc_api::ids::RequestId;
    use tc_api::{ApiError, Endpoint, Reply, Write};

    use super::*;
    use crate::app::AppDeps;
    use crate::config::{AppConfig, ProjectsFolder};
    use crate::reports::{ReportOp, ReportOutcome};

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Boom {
        request: RequestId,
    }

    impl Endpoint for Boom {
        const PATH: &'static str = "test/boom";
        const KIND: Kind = Kind::Command;
        const NET: Option<tc_api::endpoint::NetKind> = None;
        type Ok = ();
    }

    impl Write for Boom {
        fn request(&self) -> RequestId {
            self.request
        }

        fn at(&self) -> Option<&tc_api::Scope> {
            None
        }
    }

    impl Handle<Boom> for App {
        async fn handle(&self, _req: Boom) -> Result<(), ApiError> {
            panic!("the handler broke")
        }
    }

    async fn app(dir: &std::path::Path) -> App {
        let projects = dir.join("projects");
        std::fs::create_dir(&projects).unwrap();
        let config = AppConfig {
            app_data: dir.join("app-data"),
            projects: ProjectsFolder::Given(projects),
            door43: "http://127.0.0.1:9".parse().unwrap(),
            build: tc_api::app::BuildInfo {
                version: "0".to_owned(),
                commit: "0".to_owned(),
            },
            bundle: None,
            #[cfg(feature = "test-hooks")]
            test_hooks: false,
            #[cfg(feature = "test-hooks")]
            seed_pace: None,
        };
        App::start(config, AppDeps::production()).await.unwrap()
    }

    #[tokio::test]
    async fn a_handler_that_panics_is_an_internal_reply_that_is_cached() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path()).await;
        let body = br#"{"request":"01010101010101010101010101010101"}"#;

        let first = app.run_write::<Boom>("test/boom", body).await;
        let Answer::Reply {
            body: first_body, ..
        } = &first
        else {
            panic!("{first:?}")
        };
        let reply: Reply<()> = serde_json::from_slice(first_body).unwrap();
        let Reply::Err(ApiError::Internal { report }) = reply else {
            panic!("{reply:?}")
        };
        let kept = app.reports().get(report).expect("the report is kept");
        assert_eq!(kept.op, ReportOp::Internal);
        assert!(
            matches!(&kept.outcome, ReportOutcome::Failed { reason } if reason == "a request handler stopped")
        );
        assert_eq!(kept.facts["path"], "test/boom");
        assert_eq!(kept.facts["stopped"], "the handler broke");

        // A retry gets the same reply: the write is not run again and is not "lost".
        let second = app.run_write::<Boom>("test/boom", body).await;
        assert_eq!(first, second);
        assert_eq!(app.reports().all().len(), 1, "it ran once");
        assert_eq!(app.inner.replies.running(), 0);
    }
}
