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
//!    the switch is off. This comes before the reply cache and is never cached, because the
//!    UI asks the question, turns the switch on and sends *the same request again*; a cached
//!    refusal would answer that resend forever;
//! 3. a query runs in the caller's task and changes nothing;
//! 4. a write goes through the [`ReplyCache`](crate::replies::ReplyCache): the first attempt
//!    runs on a task of its own, so a dropped connection never stops it half way, and a retry
//!    gets that attempt's reply. Every answer of a write is cached, refusals included: a
//!    retry after a lost reply must get the same refusal, not run the write again.

use std::future::Future;

use serde_json::json;
use tc_api::{ApiError, Endpoint, Reply, Write};

use crate::app::{Answer, App, Then};
use crate::jobs::stop_reason;
use crate::replies::{BodyDigest, CacheAnswer, Outcome};
use crate::reports::facts;
#[cfg(feature = "test-hooks")]
use tc_api::test::CrashPoint;

/// What the app does for one endpoint. Impls write `async fn handle(&self, req: X) ->
/// Result<…, ApiError>`, which satisfies this signature while the body holds nothing that is
/// not `Send` across an await.
pub trait Handle<E: Endpoint> {
    fn handle(&self, req: E) -> impl Future<Output = Result<E::Ok, ApiError>> + Send;
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
        if let Some(error) = self.needs_internet::<E>() {
            return self.error_answer(error);
        }
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

        match self.inner.replies.run(id, digest, work).await {
            CacheAnswer::Fresh(outcome) => self.write_answer(outcome, self.then_after(path)),
            CacheAnswer::Repeat(outcome) => self.write_answer(outcome, Then::Continue),
            CacheAnswer::Reused => self.error_answer(ApiError::RequestReused),
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

    fn write_answer(&self, outcome: Outcome, then: Then) -> Answer {
        match outcome {
            Outcome::Reply(bytes) => Answer::Reply {
                body: bytes.to_vec(),
                then,
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
            #[cfg(feature = "test-hooks")]
            test_hooks: false,
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
