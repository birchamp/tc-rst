//! The `test` service as a whole: it answers only when switched on, every row answers, an
//! arm that could never fire is refused, `test/quiet` sees what is in flight, and
//! `test/reset` starts the next scenario from a launch's state.

use std::time::Duration;

use serde_json::Value;
use tc_api::ALL;
use tc_api::event::Event;
use tc_api::ids::{LocaleId, WallMs};
use tc_api::jobs::{JobKind, JobOutcome, Phase};
use tc_api::net::Set;
use tc_api::prefs::SetLocale;
use tc_api::test::{
    Crash, CrashPoint, Echo, EchoView, Hold, HoldPoint, MAX_ECHO, Release, Reset, Validate,
    ValidateView,
};
use tc_api::{ApiError, Reply};
use tc_app::Answer;
use tc_app::events::Next;
use tc_app::hooks::CRASH_EXIT_CODE;
use tc_app::jobs::JobSpec;
use tc_app::reports::{ReportOp, ReportOutcome, facts};

use super::common::{
    ManualJob, Parts, TestApp, answer, field_log, field_token, hello_value, is_busy, post, rid,
    store, store_req, try_start, until_held_count, until_quiet,
};

fn crash(id: u8, at: CrashPoint, path: Option<&str>) -> Crash {
    Crash {
        request: rid(id),
        at,
        path: path.map(str::to_owned),
    }
}

fn hold(id: u8, at: HoldPoint) -> Hold {
    Hold {
        request: rid(id),
        at,
    }
}

fn release(id: u8, at: HoldPoint) -> Release {
    Release {
        request: rid(id),
        at,
    }
}

fn net(id: u8, on: bool) -> Set {
    Set {
        request: rid(id),
        on,
    }
}

async fn started() -> (tempfile::TempDir, TestApp) {
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_in(dir.path(), |_, _| {}).await;
    (dir, t)
}

#[test]
fn a_test_crash_exits_with_the_code_the_server_uses() {
    assert_eq!(CRASH_EXIT_CODE, 86);
}

#[tokio::test]
async fn test_hooks_are_not_found_when_switched_off() {
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_in(dir.path(), |config, _| config.test_hooks = false).await;
    for info in ALL.iter().filter(|info| info.path.starts_with("test/")) {
        assert_eq!(
            t.app.call(info.path, b"{}").await,
            Answer::NotFound,
            "{}",
            info.path
        );
    }
    assert!(
        !t.app_data.join("test-hooks").exists(),
        "no test-hooks folder without the flag"
    );
    // The product endpoints still answer.
    assert!(matches!(
        t.app.call("app/hello", b"{}").await,
        Answer::Reply { .. }
    ));
}

#[tokio::test]
async fn test_hooks_make_their_folder_when_switched_on() {
    let (_dir, t) = started().await;
    assert!(t.app_data.join("test-hooks").join("fields.json").is_file());
}

#[tokio::test]
async fn an_unreadable_fields_file_stops_the_start_and_is_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_in(dir.path(), |_, _| {}).await;
    let file = t.app_data.join("test-hooks").join("fields.json");
    std::fs::write(&file, b"{ not json").unwrap();
    let again = try_start(dir.path(), &Parts::new(), |_, _| {}).await;
    assert!(
        matches!(again, Err(tc_app::StartError::TestFields(_))),
        "{:?}",
        again.map(|_| ())
    );
    assert_eq!(std::fs::read(&file).unwrap(), b"{ not json");
}

/// Every row answers its golden request with a well-formed reply, the test hooks too.
#[tokio::test]
async fn every_endpoint_answers_its_golden_request() {
    let golden = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../tc-api/tests/golden/endpoints/"
    );
    for info in ALL {
        let text = std::fs::read_to_string(format!("{golden}{}.json", info.path)).unwrap();
        let file: Value = serde_json::from_str(&text).unwrap();
        // A fresh app for each: `app/quit`, `net/set` and `test/crash` change state.
        let (_dir, t) = started().await;
        if info.net.is_some() {
            post(
                &t.app,
                &Set {
                    request: rid(200),
                    on: true,
                },
            )
            .await
            .unwrap();
        }
        let body = serde_json::to_vec(&file["request"]).unwrap();
        let answered = t.app.call(info.path, &body).await;
        let Answer::Reply { body, .. } = answered else {
            panic!("{}: {answered:?}", info.path)
        };
        let reply: Reply<Value> = serde_json::from_slice(&body)
            .unwrap_or_else(|e| panic!("{}: {e}: {}", info.path, String::from_utf8_lossy(&body)));
        if [
            "app/hello",
            "net/state",
            "account/state",
            "account/helpLink",
            "test/quiet",
            "test/echo",
            "test/hold",
            "test/release",
            "test/reset",
            "test/crash",
            "test/fieldLog",
            "test/validate",
        ]
        .contains(&info.path)
        {
            assert!(matches!(reply, Reply::Ok(_)), "{}: {reply:?}", info.path);
        }
    }
}

/// The loopback spike measures the reply's size, so the app must answer the size asked for
/// with the words tc-api builds, and refuse one it would not build.
#[tokio::test]
async fn echo_answers_the_size_asked_for_and_refuses_a_larger_one() {
    let (_dir, t) = started().await;
    for size in [0, 1, 20_480, MAX_ECHO] {
        let view = post(&t.app, &Echo { size }).await.unwrap();
        assert_eq!(view, EchoView::of_size(size), "size {size}");
        let sent = serde_json::to_vec(&Reply::Ok(view)).unwrap();
        assert!(sent.len() >= size as usize, "size {size}: {}", sent.len());
    }
    let refused = post(&t.app, &Echo { size: MAX_ECHO + 1 }).await;
    assert!(
        matches!(&refused, Err(ApiError::BadRequest { field, .. }) if field == "size"),
        "{refused:?}"
    );
}

#[tokio::test]
async fn arming_a_crash_that_can_never_fire_is_a_bad_request() {
    let (_dir, t) = started().await;
    let paths = [
        "app/hello",
        "test/crash",
        "test/hold",
        "test/release",
        "test/reset",
        "nope/nope",
        "net/state",
        "test/fieldLog",
        "",
    ];
    for (n, path) in paths.into_iter().enumerate() {
        let id = 10 + u8::try_from(n).unwrap();
        let armed = post(&t.app, &crash(id, CrashPoint::BeforeStore, Some(path))).await;
        assert!(
            matches!(&armed, Err(ApiError::BadRequest { field, .. }) if field == "path"),
            "{path:?}: {armed:?}"
        );
    }
    // Nothing was armed: the next write runs.
    post(&t.app, &net(2, true)).await.unwrap();
    assert!(t.app.internet().is_on());
    assert!(t.ender.ended().is_empty());
}

/// A crash armed at a point no write reaches would let a crash test pass by doing nothing,
/// as one armed for a query does (APP-DESIGN §5.8). No phase-0 write is a draft write.
#[tokio::test]
async fn arming_a_crash_at_a_point_no_write_reaches_is_a_bad_request() {
    let (_dir, t) = started().await;
    for (n, path) in [None, Some("net/set"), Some("test/fieldStore")]
        .into_iter()
        .enumerate()
    {
        let id = 10 + u8::try_from(n).unwrap();
        let armed = post(&t.app, &crash(id, CrashPoint::DuringDraftWrite, path)).await;
        assert!(
            matches!(&armed, Err(ApiError::BadRequest { field, .. }) if field == "at"),
            "{path:?}: {armed:?}"
        );
    }
    // Nothing was armed: writes run, and the three points they do reach are accepted.
    post(&t.app, &net(2, true)).await.unwrap();
    assert!(t.ender.ended().is_empty());
    for (n, at) in [
        CrashPoint::BeforeStore,
        CrashPoint::BeforeReply,
        CrashPoint::AfterReply,
    ]
    .into_iter()
    .enumerate()
    {
        let id = 20 + u8::try_from(n).unwrap();
        post(&t.app, &crash(id, at, None)).await.unwrap();
    }
}

#[tokio::test]
async fn a_crash_for_any_write_is_taken_by_the_first_write_and_not_by_the_arming_hooks() {
    let (_dir, t) = started().await;
    // Hold, release and a second arm do not take the crash.
    post(&t.app, &crash(1, CrashPoint::BeforeStore, None))
        .await
        .unwrap();
    post(&t.app, &hold(2, HoldPoint::Checkpoint)).await.unwrap();
    post(&t.app, &release(3, HoldPoint::Checkpoint))
        .await
        .unwrap();
    post(&t.app, &crash(4, CrashPoint::BeforeStore, None))
        .await
        .unwrap();
    assert!(t.ender.ended().is_empty());
    assert_eq!(answer(&t.app, &net(5, true)).await, Answer::Ended);
    assert_eq!(t.ender.ended(), vec![CrashPoint::BeforeStore]);
}

#[tokio::test]
async fn quiet_follows_jobs() {
    let (_dir, t) = started().await;
    assert!(!is_busy(&t.app).await, "idle");
    let job = ManualJob::start(t.app.jobs(), JobSpec::new(JobKind::Download, Phase::Seed)).await;
    assert!(is_busy(&t.app).await, "a job is live");
    job.finish(Ok(JobOutcome::Finished));
    until_quiet(&t.app).await;
    // `test/quiet` does not count itself.
    assert_eq!(
        t.app.call("test/quiet", b"{}").await,
        t.app.call("test/quiet", b"{}").await
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quiet_follows_writes_and_holds() {
    let (_dir, t) = started().await;
    let token = field_token(&t.app, "title").await;
    post(&t.app, &hold(1, HoldPoint::Save)).await.unwrap();
    let held = {
        let app = t.app.clone();
        let body = serde_json::to_vec(&store_req(2, "title", &token, "x")).unwrap();
        tokio::spawn(async move { app.call("test/fieldStore", &body).await })
    };
    until_held_count(&t.app, 1).await;
    assert!(is_busy(&t.app).await, "a held store is in flight");
    post(&t.app, &release(3, HoldPoint::Save)).await.unwrap();
    held.await.unwrap();
    until_quiet(&t.app).await;
    assert!(!is_busy(&t.app).await);
}

/// A write whose task is gone is not in flight, or every capture would wait for it for ever.
#[tokio::test]
async fn quiet_ignores_a_request_that_has_ended() {
    let (_dir, t) = started().await;
    for n in 0..3 {
        post(&t.app, &net(n + 1, n % 2 == 0)).await.unwrap();
        assert!(!is_busy(&t.app).await, "after write {n}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reset_restores_default_prefs_clears_reports_releases_holds_and_turns_the_internet_off() {
    let (_dir, t) = started().await;
    let mut events = t.app.events().subscribe(t.app.events().head());

    let french = SetLocale {
        request: rid(1),
        locale: LocaleId::parse("fr").unwrap(),
    };
    post(&t.app, &french).await.unwrap();
    t.app.reports().add(
        ReportOp::Open,
        ReportOutcome::Ok,
        facts(serde_json::json!({})),
        WallMs(1),
        WallMs(2),
    );
    post(&t.app, &net(2, true)).await.unwrap();
    // A store waits at its hold when the reset comes.
    let token = field_token(&t.app, "title").await;
    post(&t.app, &hold(4, HoldPoint::Save)).await.unwrap();
    let waiting = {
        let app = t.app.clone();
        let body = serde_json::to_vec(&store_req(5, "title", &token, "Waits")).unwrap();
        tokio::spawn(async move { app.call("test/fieldStore", &body).await })
    };
    until_held_count(&t.app, 1).await;
    // Armed after the store passed its crash point, so it waits for the next write.
    post(&t.app, &crash(3, CrashPoint::BeforeStore, None))
        .await
        .unwrap();
    while events.try_next().is_some() {}

    post(&t.app, &Reset { request: rid(6) }).await.unwrap();

    let hello = hello_value(&t.app).await;
    assert_eq!(hello["prefs"]["appLocale"], "en");
    assert_eq!(hello["internet"], false);
    assert!(t.app.reports().all().is_empty());
    // The held store finished.
    let stored = tokio::time::timeout(Duration::from_secs(60), waiting)
        .await
        .expect("hung: the held store was released")
        .unwrap();
    assert!(matches!(stored, Answer::Reply { .. }));
    // The crash is disarmed: a write runs.
    let current = field_token(&t.app, "title").await;
    store(&t.app, 7, "title", &current, "After").await.unwrap();
    assert!(t.ender.ended().is_empty());
    // The switch going off was announced; a second reset says nothing new.
    let mut seen = Vec::new();
    while let Some(Next::Frame(frame)) = events.try_next() {
        if let Event::Internet { on } = frame.event {
            seen.push(on);
        }
    }
    assert_eq!(seen, vec![false]);
    post(&t.app, &Reset { request: rid(8) }).await.unwrap();
    while let Some(Next::Frame(frame)) = events.try_next() {
        assert!(!matches!(frame.event, Event::Internet { .. }), "{frame:?}");
    }
    // The test fields are kept: tests name their own.
    assert_eq!(field_log(&t.app, "title").await.stores.len(), 2);
    until_quiet(&t.app).await;
}

/// A launch has an empty reply cache, so a request id the harness used in one scenario is
/// free in the next: it must run, and must not be answered with the old scenario's reply.
#[tokio::test]
async fn reset_forgets_the_finished_replies_so_a_request_id_can_be_used_again() {
    let (_dir, t) = started().await;
    post(&t.app, &net(1, true)).await.unwrap();
    assert!(t.app.internet().is_on());

    post(&t.app, &Reset { request: rid(2) }).await.unwrap();
    assert!(!t.app.internet().is_on());

    // The same request again runs. The cache answered `on: true` and left the switch off.
    let again = post(&t.app, &net(1, true)).await.unwrap();
    assert!(again.on);
    assert!(t.app.internet().is_on(), "the write ran");

    // Another body under the id is not `RequestReused`: the id was forgotten.
    post(&t.app, &Reset { request: rid(3) }).await.unwrap();
    let other = post(&t.app, &net(1, false)).await;
    assert_eq!(other.map(|view| view.on), Ok(false));
}

/// The parity harness numbers its request ids from 1 in every scenario and sends the reset
/// first, so the second scenario's reset carries the id of the first one's. It must run: a
/// reply kept for the reset would answer it without resetting anything, and every scenario
/// from the second on would start with what the one before it left.
#[tokio::test]
async fn a_reset_with_the_id_of_an_earlier_reset_resets_again() {
    let (_dir, t) = started().await;
    let french = SetLocale {
        request: rid(3),
        locale: LocaleId::parse("fr").unwrap(),
    };

    // Scenario one.
    post(&t.app, &Reset { request: rid(1) }).await.unwrap();
    post(&t.app, &net(2, true)).await.unwrap();
    post(&t.app, &french).await.unwrap();
    assert!(t.app.internet().is_on());

    // Scenario two starts with the same reset.
    post(&t.app, &Reset { request: rid(1) }).await.unwrap();
    assert!(!t.app.internet().is_on(), "the switch was not turned off");
    assert_eq!(hello_value(&t.app).await["prefs"]["appLocale"], "en");

    // The ids of scenario one are free in scenario two.
    post(&t.app, &net(2, true)).await.unwrap();
    assert!(
        t.app.internet().is_on(),
        "the write was served from a cache"
    );
    post(&t.app, &Reset { request: rid(1) }).await.unwrap();
    assert!(!t.app.internet().is_on(), "a third reset did not run");
    until_quiet(&t.app).await;
}

/// A write still running is not a finished reply: dropping it would let its retry run the
/// write a second time.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reset_keeps_a_write_in_flight_so_its_retry_still_waits_for_it() {
    let (_dir, t) = started().await;
    let token = field_token(&t.app, "title").await;
    post(&t.app, &hold(1, HoldPoint::Save)).await.unwrap();
    let body = serde_json::to_vec(&store_req(5, "title", &token, "Waits")).unwrap();
    let first = {
        let app = t.app.clone();
        let body = body.clone();
        tokio::spawn(async move { app.call("test/fieldStore", &body).await })
    };
    until_held_count(&t.app, 1).await;

    // The reset releases the hold: the store finishes after the reset has run.
    post(&t.app, &Reset { request: rid(2) }).await.unwrap();
    let first = first.await.unwrap();

    let retry = t.app.call("test/fieldStore", &body).await;
    assert_eq!(retry, first, "the retry got the first attempt's reply");
    assert_eq!(
        field_log(&t.app, "title").await.stores.len(),
        1,
        "stored once"
    );
}

#[tokio::test]
async fn holds_at_the_other_points_are_accepted_and_nothing_reaches_them() {
    let (_dir, t) = started().await;
    let points = [
        HoldPoint::Train,
        HoldPoint::BookRead,
        HoldPoint::Checkpoint,
        HoldPoint::Materialize,
        HoldPoint::DraftWrite,
    ];
    for (n, at) in points.into_iter().enumerate() {
        post(&t.app, &hold(10 + u8::try_from(n).unwrap(), at))
            .await
            .unwrap();
    }
    // No phase-0 write reaches any of them.
    post(&t.app, &net(3, true)).await.unwrap();
    assert!(!is_busy(&t.app).await);
    assert_eq!(t.app.holding(), 0);
    assert!(t.ender.ended().is_empty());
}

#[tokio::test]
async fn a_release_with_nothing_held_does_nothing() {
    let (_dir, t) = started().await;
    post(&t.app, &release(1, HoldPoint::Save)).await.unwrap();
    // And an armed hold is removed by a release before anything reached it.
    post(&t.app, &hold(2, HoldPoint::Save)).await.unwrap();
    post(&t.app, &release(3, HoldPoint::Save)).await.unwrap();
    let token = field_token(&t.app, "title").await;
    let stored = tokio::time::timeout(
        Duration::from_secs(60),
        store(&t.app, 4, "title", &token, "x"),
    )
    .await
    .expect("hung: the call did not come back while the store was not held");
    assert!(stored.is_ok());
}

/// P5/INV-VALID-BURRITO: `test/validate` checks every folder on disk once the app has
/// settled, and names a project whose metadata lists a wrong md5 for an ingredient.
#[tokio::test]
async fn test_validate_names_a_project_whose_ingredient_md5_is_wrong() {
    let disk = super::store::OnDisk::new();
    let folder = super::store::make_project(&disk.app_data(), &disk.projects, "titus_on_disk");
    let t = disk.start().await;
    let clean = post(&t.app, &Validate {}).await.unwrap();
    assert_eq!(
        clean,
        ValidateView {
            checked: 1,
            invalid: Vec::new()
        }
    );
    let metadata = folder.join("metadata.json");
    let text = std::fs::read_to_string(&metadata).unwrap();
    let at = text.find("\"md5\": \"").unwrap() + "\"md5\": \"".len();
    let planted = format!("{}{}{}", &text[..at], "0".repeat(32), &text[at + 32..]);
    std::fs::write(&metadata, planted).unwrap();
    let view = post(&t.app, &Validate {}).await.unwrap();
    assert_eq!(view.checked, 1);
    assert_eq!(view.invalid.len(), 1, "{view:?}");
    assert_eq!(view.invalid[0].folder, "titus_on_disk");
    assert!(
        view.invalid[0]
            .problems
            .iter()
            .any(|p| p.contains("ChecksumMismatch")),
        "{view:?}"
    );
}
