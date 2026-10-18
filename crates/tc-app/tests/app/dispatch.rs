//! Routing, the reply cache in front of every write, and what each product handler answers.

use std::time::Duration;

use serde_json::{Value, json};
use tc_api::account::{AccountView, HelpLink, KeepState, LinkView, SignIn};
use tc_api::app::{ClientError, Quit};
use tc_api::error::Feature;
use tc_api::ids::{JobId, LocaleId, ProjectId, ProjectRef, WallMs};
use tc_api::jobs::{Cancel, Get, JobKind, JobOutcome, JobState, Phase};
use tc_api::net::Set;
use tc_api::prefs::{
    DraftUnit, PlaceChange, PlaceMode, PlaceUpdate, RecordPlace, SetAlignSuggestions, SetDraftUnit,
    SetLocale,
};
use tc_api::refusal::PrefsRefusal;
use tc_api::{ALL, ApiError, NotSaved, Refusal, Reply};
use tc_app::events::Next;
use tc_app::jobs::JobSpec;
use tc_app::prefs::AppDataPath;
use tc_app::reports::{ReportOp, ReportOutcome};
use tc_app::{Answer, App, Then};

use super::common::{
    ManualJob, PROJECT_ID, Parts, TestApp, hello_value, last_edit, open_session, rid, scope,
    settle, step,
};

async fn started() -> (tempfile::TempDir, TestApp) {
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_in(dir.path(), |_, _| {}).await;
    (dir, t)
}

async fn raw(app: &App, path: &str, body: &[u8]) -> Reply<Value> {
    match app.call(path, body).await {
        Answer::Reply { body, .. } => serde_json::from_slice(&body).unwrap(),
        other => panic!("{other:?}"),
    }
}

fn error_of(reply: Reply<Value>) -> ApiError {
    match reply {
        Reply::Err(error) => error,
        Reply::Ok(value) => panic!("expected an error, got {value}"),
    }
}

fn client_error(id: u8, message: &str) -> ClientError {
    ClientError {
        request: rid(id),
        view: Some("Translate".to_owned()),
        project: Some(ProjectRef::parse("kr_ulb").unwrap()),
        message: message.to_owned(),
        at: WallMs(42),
    }
}

#[tokio::test]
async fn an_unknown_path_is_not_found() {
    let (_dir, t) = started().await;
    for path in [
        "",
        "app",
        "app/nope",
        "nope/nope",
        "/app/hello",
        "APP/HELLO",
    ] {
        assert_eq!(t.app.call(path, b"{}").await, Answer::NotFound, "{path:?}");
    }
}

#[tokio::test]
async fn a_body_that_does_not_parse_is_a_bad_request() {
    let (_dir, t) = started().await;
    for (path, body) in [
        ("app/hello", &b"{"[..]),
        ("app/hello", &b""[..]),
        ("app/hello", &b"null"[..]),
        ("net/set", &b"{}"[..]),
    ] {
        let error = error_of(raw(&t.app, path, body).await);
        assert!(
            matches!(&error, ApiError::BadRequest { field, .. } if field == "body"),
            "{path} {body:?}: {error:?}"
        );
    }
}

#[tokio::test]
async fn an_unknown_key_is_a_bad_request() {
    let (_dir, t) = started().await;
    let body = json!({"request": rid(1), "on": true, "extra": 1}).to_string();
    let error = error_of(raw(&t.app, "net/set", body.as_bytes()).await);
    assert!(
        matches!(&error, ApiError::BadRequest { field, reason } if field == "body" && reason.contains("extra")),
        "{error:?}"
    );
    assert!(!t.app.internet().is_on(), "nothing ran");
}

#[tokio::test]
async fn a_bad_request_reason_is_bounded() {
    let (_dir, t) = started().await;
    let key = "k".repeat(100_000);
    let body = format!(r#"{{"{key}":1}}"#);
    let error = error_of(raw(&t.app, "app/hello", body.as_bytes()).await);
    let ApiError::BadRequest { reason, .. } = error else {
        panic!("{error:?}")
    };
    assert!(reason.chars().count() <= 300, "{}", reason.len());
}

#[tokio::test]
async fn a_bad_request_is_not_cached() {
    let (_dir, t) = started().await;
    let bad = json!({"request": rid(1), "on": "yes"}).to_string();
    let error = error_of(raw(&t.app, "net/set", bad.as_bytes()).await);
    assert!(matches!(error, ApiError::BadRequest { .. }));
    // The same id with a good body afterwards runs.
    let good = json!({"request": rid(1), "on": true}).to_string();
    let reply = raw(&t.app, "net/set", good.as_bytes()).await;
    assert_eq!(reply, Reply::Ok(json!({"on": true})));
    assert!(t.app.internet().is_on());
}

#[tokio::test]
async fn a_retried_write_gets_the_first_reply() {
    let (_dir, t) = started().await;
    let request = client_error(1, "panicked at src/lib.rs");
    let first = t
        .app
        .call("app/clientError", &serde_json::to_vec(&request).unwrap())
        .await;
    let again = t
        .app
        .call("app/clientError", &serde_json::to_vec(&request).unwrap())
        .await;
    assert_eq!(first, again, "identical bytes");
    let reports: Vec<_> = t
        .app
        .reports()
        .all()
        .into_iter()
        .filter(|r| r.op == ReportOp::ClientError)
        .collect();
    assert_eq!(reports.len(), 1, "the retry did not add a report");
    assert_eq!(
        reports[0].outcome,
        ReportOutcome::Failed {
            reason: "panicked at src/lib.rs".to_owned()
        }
    );
    assert_eq!(reports[0].facts["view"], "Translate");
    assert_eq!(reports[0].facts["project"], "kr_ulb");
    assert_eq!(reports[0].facts["at"], 42);

    // The same id with another message is refused and runs nothing.
    let other = client_error(1, "something else");
    let reply = t
        .app
        .request(&other)
        .await
        .unwrap()
        .into_result()
        .unwrap_err();
    assert_eq!(reply, ApiError::RequestReused);
    let count = t
        .app
        .reports()
        .all()
        .into_iter()
        .filter(|r| r.op == ReportOp::ClientError)
        .count();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn a_retry_with_other_spacing_is_the_same_request() {
    let (_dir, t) = started().await;
    let compact = json!({"request": rid(5), "on": true}).to_string();
    let spaced = format!(r#"{{ "on" : true , "request" : "{}" }}"#, rid(5));
    let first = t.app.call("net/set", compact.as_bytes()).await;
    let second = t.app.call("net/set", spaced.as_bytes()).await;
    assert_eq!(first, second);
}

#[tokio::test]
async fn a_long_client_error_is_cut_and_marked() {
    let (_dir, t) = started().await;
    let message = "é".repeat(100 * 1024 / 2);
    t.app.request(&client_error(1, &message)).await.unwrap();
    let report = t
        .app
        .reports()
        .all()
        .into_iter()
        .find(|r| r.op == ReportOp::ClientError)
        .unwrap();
    let ReportOutcome::Failed { reason } = report.outcome else {
        panic!()
    };
    assert!(reason.len() <= 8 * 1024, "{}", reason.len());
    assert!(message.starts_with(&reason), "a prefix of the message");
    assert_eq!(report.facts["truncated"], true);

    t.app.request(&client_error(2, "short")).await.unwrap();
    let short = t.app.reports().all().pop().unwrap();
    assert!(!short.facts.contains_key("truncated"));
}

fn place() -> PlaceUpdate {
    PlaceUpdate::Book {
        book: "TIT".parse().unwrap(),
        mode: Some(PlaceMode::Draft),
        chapter: Some(2),
        verse: PlaceChange::Keep,
        tool: None,
    }
}

#[tokio::test]
async fn scoped_prefs_need_an_open_session() {
    let (_dir, t) = started().await;
    let at = scope(1);
    let stale = Err(ApiError::NotSaved(NotSaved::StaleScope));
    let unit = SetDraftUnit {
        at: at.clone(),
        request: rid(1),
        unit: DraftUnit::Verse,
    };
    let align = SetAlignSuggestions {
        at: at.clone(),
        request: rid(2),
        on: true,
    };
    let record = RecordPlace {
        at,
        request: rid(3),
        place: place(),
    };
    assert_eq!(t.app.request(&unit).await.unwrap().into_result(), stale);
    assert_eq!(t.app.request(&align).await.unwrap().into_result(), stale);
    assert_eq!(t.app.request(&record).await.unwrap().into_result(), stale);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(t.files.writes().is_empty(), "nothing was written");
}

#[tokio::test]
async fn scoped_prefs_land_where_the_session_says() {
    let dir = tempfile::tempdir().unwrap();
    let parts = Parts::new();
    let (at, info, open) = open_session(3);
    let (id, folder) = (info.id.clone(), info.project.clone());
    parts.sessions.add(at, info, open);
    let t = TestApp::start_with(dir.path(), &parts, |_, _| {}).await;
    let at = scope(3);
    t.app
        .request(&SetDraftUnit {
            at: at.clone(),
            request: rid(1),
            unit: DraftUnit::Verse,
        })
        .await
        .unwrap()
        .into_result()
        .unwrap();
    t.app
        .request(&SetAlignSuggestions {
            at: at.clone(),
            request: rid(2),
            on: true,
        })
        .await
        .unwrap()
        .into_result()
        .unwrap();
    t.app
        .request(&RecordPlace {
            at,
            request: rid(3),
            place: place(),
        })
        .await
        .unwrap()
        .into_result()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(t.app.prefs().draft_unit(&id), Some(DraftUnit::Verse));
    assert_eq!(t.app.prefs().align_suggestions(&folder), Some(true));
    let record = t.app.prefs().place(&id, "TIT").unwrap();
    assert_eq!(record.chapter, 2);
    // The scope of another open is stale.
    let stale = t
        .app
        .request(&SetDraftUnit {
            at: scope(4),
            request: rid(4),
            unit: DraftUnit::Section,
        })
        .await
        .unwrap()
        .into_result();
    assert_eq!(stale, Err(ApiError::NotSaved(NotSaved::StaleScope)));
}

#[tokio::test]
async fn a_failed_locale_save_is_refused_through_the_endpoint() {
    let (_dir, t) = started().await;
    t.files.fail_writes(true);
    let reply = t
        .app
        .request(&SetLocale {
            request: rid(1),
            locale: LocaleId::parse("fr").unwrap(),
        })
        .await
        .unwrap();
    assert_eq!(
        reply.into_result(),
        Err(ApiError::Refused(Refusal::Prefs(
            PrefsRefusal::LocaleNotSaved
        )))
    );
    let hello = hello_value(&t.app).await;
    assert_eq!(hello["prefs"]["appLocale"], "en", "nothing changed");
    let report = t
        .app
        .reports()
        .all()
        .into_iter()
        .find(|r| r.op == ReportOp::PrefsWrite)
        .expect("a report names the file");
    assert!(
        report.facts["file"]
            .as_str()
            .unwrap()
            .contains(AppDataPath::app_prefs().as_str()),
        "{:?}",
        report.facts
    );
}

#[tokio::test]
async fn jobs_endpoints_follow_the_registry() {
    let (_dir, t) = started().await;
    let job = ManualJob::start(t.app.jobs(), JobSpec::new(JobKind::Download, Phase::Seed)).await;
    let id = job.id;
    let mut events = t.app.events().subscribe(t.app.events().head());

    let view = t
        .app
        .request(&Get { job: id })
        .await
        .unwrap()
        .into_result()
        .unwrap();
    assert!(matches!(
        view.unwrap().state,
        JobState::Running(_) | JobState::Queued
    ));

    t.app
        .request(&Cancel {
            request: rid(1),
            job: id,
        })
        .await
        .unwrap()
        .into_result()
        .unwrap();
    let view = t
        .app
        .request(&Get { job: id })
        .await
        .unwrap()
        .into_result()
        .unwrap();
    assert_eq!(view.unwrap().state, JobState::Cancelling);
    // The frame was on the bus before the reply.
    let mut states = Vec::new();
    while let Some(Next::Frame(frame)) = events.try_next() {
        if let tc_api::event::Event::Job(view) = frame.event {
            states.push(view.state);
        }
    }
    assert!(states.contains(&JobState::Cancelling), "{states:?}");

    let unknown = JobId::new(9_999);
    assert_eq!(
        t.app
            .request(&Get { job: unknown })
            .await
            .unwrap()
            .into_result(),
        Ok(None)
    );
    assert_eq!(
        t.app
            .request(&Cancel {
                request: rid(2),
                job: unknown
            })
            .await
            .unwrap()
            .into_result(),
        Ok(())
    );
    job.finish(Ok(JobOutcome::Finished));
}

#[tokio::test(start_paused = true)]
async fn quit_writes_pending_prefs_and_resolves_quitting() {
    let (_dir, t) = started().await;
    let id = ProjectId::parse(PROJECT_ID).unwrap();
    t.app.prefs().record_last_edit(id, last_edit());
    step(1_000).await;
    assert!(
        t.files.get(&AppDataPath::app_prefs()).is_none(),
        "the Resume record waits 5 s"
    );
    let quitting = {
        let app = t.app.clone();
        tokio::spawn(async move { app.quitting().await })
    };
    settle().await;
    assert!(!quitting.is_finished());

    let answer = t
        .app
        .call(
            "app/quit",
            &serde_json::to_vec(&Quit { request: rid(1) }).unwrap(),
        )
        .await;
    assert!(matches!(
        answer,
        Answer::Reply {
            then: Then::Continue,
            ..
        }
    ));
    let written = t
        .files
        .get(&AppDataPath::app_prefs())
        .expect("written at quit");
    assert!(String::from_utf8(written).unwrap().contains("lastEdit"));
    settle().await;
    assert!(quitting.is_finished());
}

#[tokio::test]
async fn quit_is_safe_twice() {
    let (_dir, t) = started().await;
    t.app.quit().await;
    t.app.quit().await;
    tokio::time::timeout(Duration::from_secs(5), t.app.quitting())
        .await
        .expect("quitting resolves");
    let answer = t
        .app
        .call(
            "app/quit",
            &serde_json::to_vec(&Quit { request: rid(1) }).unwrap(),
        )
        .await;
    assert!(matches!(answer, Answer::Reply { .. }));
}

#[tokio::test]
async fn account_state_answers_from_memory() {
    let (_dir, t) = started().await;
    let view = t
        .app
        .request(&tc_api::account::State {})
        .await
        .unwrap()
        .into_result()
        .unwrap();
    assert_eq!(
        view,
        AccountView {
            user: None,
            kept: KeepState::NotKept,
            notice: None
        }
    );
}

#[tokio::test]
async fn the_account_rows_a_later_unit_builds_say_so() {
    let (_dir, t) = started().await;
    t.app
        .request(&Set {
            request: rid(9),
            on: true,
        })
        .await
        .unwrap();
    let sign_in = SignIn {
        request: rid(1),
        login: "jdoe".to_owned(),
        password: "secret".to_owned(),
        stay: false,
    };
    assert_eq!(
        t.app.request(&sign_in).await.unwrap().into_result(),
        Err(ApiError::Unavailable(Feature::Account))
    );
    let compose = tc_api::feedback::Compose { report: None };
    assert_eq!(
        t.app.request(&compose).await.unwrap().into_result(),
        Err(ApiError::Unavailable(Feature::Feedback))
    );
}

/// Every product row answers its golden request with a well-formed reply.
#[tokio::test]
async fn every_product_endpoint_answers_its_golden_request() {
    let golden = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../tc-api/tests/golden/endpoints/"
    );
    let mut seen = 0;
    for info in ALL.iter().filter(|info| !info.path.starts_with("test/")) {
        let text = std::fs::read_to_string(format!("{golden}{}.json", info.path)).unwrap();
        let file: Value = serde_json::from_str(&text).unwrap();
        // A fresh app for each: `app/quit` and `net/set` change state.
        let (_dir, t) = started().await;
        if info.net.is_some() {
            t.app
                .request(&Set {
                    request: rid(200),
                    on: true,
                })
                .await
                .unwrap();
        }
        let body = serde_json::to_vec(&file["request"]).unwrap();
        let reply = raw(&t.app, info.path, &body).await;
        if [
            "app/hello",
            "net/state",
            "account/state",
            "account/helpLink",
        ]
        .contains(&info.path)
        {
            assert!(matches!(reply, Reply::Ok(_)), "{}: {reply:?}", info.path);
        }
        seen += 1;
    }
    assert!(seen >= 19, "{seen}");
}

#[tokio::test]
async fn help_link_echoes_the_address_once_online() {
    let (_dir, t) = started().await;
    let url = tc_api::account::WebUrl::parse("https://git.door43.org/help").unwrap();
    t.app
        .request(&Set {
            request: rid(1),
            on: true,
        })
        .await
        .unwrap();
    let reply = t
        .app
        .request(&HelpLink { url: url.clone() })
        .await
        .unwrap()
        .into_result();
    assert_eq!(reply, Ok(LinkView { url }));
}

#[tokio::test]
async fn the_dispatch_keeps_the_endpoint_paths_of_the_table() {
    // `ALL` and the router are built from one table; a path in one is a path in the other.
    let (_dir, t) = started().await;
    for info in ALL.iter().filter(|info| !info.path.starts_with("test/")) {
        let answer = t.app.call(info.path, b"not json").await;
        assert!(
            matches!(answer, Answer::Reply { .. }),
            "{} is routed",
            info.path
        );
    }
}
