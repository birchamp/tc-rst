//! `feedback/compose`: the attachment the Feedback dialog shows and the shell sends, as tC4's
//! `attachmentText` builds it (APP-DESIGN §4.7, A5 D8).

use serde_json::json;
use tc_api::feedback::{Compose, FeedbackDraft};
use tc_api::ids::{LocaleId, ReportId, WallMs};
use tc_api::prefs::SetLocale;
use tc_app::feedback::{attachment_text, iso_utc_millis, platform, redact, version_line};
use tc_app::reports::{ReportLog, ReportOp, ReportOutcome, facts};

use super::common::{TestApp, build, post, rid};

async fn started() -> (tempfile::TempDir, TestApp) {
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_in(dir.path(), |_, _| {}).await;
    // The start records which projects folder it chose; these tests choose their reports.
    t.app.reports().clear();
    (dir, t)
}

fn line() -> String {
    version_line(&build(), &platform())
}

async fn compose(t: &TestApp, report: Option<ReportId>) -> String {
    post(&t.app, &Compose { report }).await.unwrap().attachment
}

fn sync_report(log: &ReportLog, n: i64) -> ReportId {
    log.add(
        ReportOp::Sync,
        ReportOutcome::Ok,
        facts(json!({ "n": n })),
        WallMs(n),
        WallMs(n + 1),
    )
}

#[tokio::test]
async fn attachment_without_a_report_says_no_report() {
    let (_dir, t) = started().await;
    let text = compose(&t, None).await;
    assert_eq!(
        text,
        format!(
            "translationCore 4.0.0-test (0123abc) · {}\nno Report\n",
            platform()
        )
    );
}

#[test]
fn attachment_holds_the_report_in_tc4s_shape() {
    let log = ReportLog::new();
    let id = log.add(
        ReportOp::Open,
        ReportOutcome::Refused {
            code: Some("open.unexplained-divergence".to_owned()),
            key: "refusal.open.unexplainedDivergence".to_owned(),
        },
        facts(json!({ "path": "x" })),
        WallMs(1_791_559_266_123),
        WallMs(1_791_559_267_000),
    );
    let report = log.get(id).unwrap();
    let text = attachment_text(
        "translationCore 4.0.0 (abc1234) · Linux x86_64",
        Some(&report),
    );
    let expected = "translationCore 4.0.0 (abc1234) · Linux x86_64\n\
        Report: open, 2026-10-09T15:21:07.000Z\n\
        {\n  \"op\": \"open\",\n  \"ok\": false,\n  \"code\": \"open.unexplained-divergence\",\n  \
        \"key\": \"refusal.open.unexplainedDivergence\",\n  \"facts\": {\n    \"path\": \"x\"\n  },\n  \
        \"startedAt\": \"2026-10-09T15:21:06.123Z\",\n  \"endedAt\": \"2026-10-09T15:21:07.000Z\"\n}\n";
    assert_eq!(text, expected);
}

#[test]
fn a_failure_without_a_refusal_carries_its_reason() {
    let log = ReportLog::new();
    let id = log.add(
        ReportOp::ClientError,
        ReportOutcome::Failed {
            reason: "boom".to_owned(),
        },
        facts(json!({})),
        WallMs(0),
        WallMs(0),
    );
    let text = attachment_text("v", log.get(id).as_ref());
    assert_eq!(
        text,
        "v\nReport: client-error, 1970-01-01T00:00:00.000Z\n{\n  \"op\": \"client-error\",\n  \
         \"ok\": false,\n  \"reason\": \"boom\",\n  \"facts\": {},\n  \
         \"startedAt\": \"1970-01-01T00:00:00.000Z\",\n  \"endedAt\": \"1970-01-01T00:00:00.000Z\"\n}\n"
    );
}

#[tokio::test]
async fn compose_attaches_the_named_report_while_it_is_kept() {
    let (_dir, t) = started().await;
    let named = sync_report(t.app.reports(), 1);
    sync_report(t.app.reports(), 2);
    let text = compose(&t, Some(named)).await;
    assert!(text.contains("\"n\": 1"), "{text}");
    assert!(text.starts_with(&format!("{}\nReport: sync, ", line())));
}

#[tokio::test]
async fn compose_attaches_the_newest_when_the_named_one_is_gone() {
    let (_dir, t) = started().await;
    let named = sync_report(t.app.reports(), 1);
    for n in 2..=(ReportLog::KEPT as i64 + 1) {
        sync_report(t.app.reports(), n);
    }
    assert!(t.app.reports().get(named).is_none());
    let text = compose(&t, Some(named)).await;
    assert!(
        text.contains(&format!("\"n\": {}", ReportLog::KEPT + 1)),
        "{text}"
    );
}

#[tokio::test]
async fn compose_with_none_named_attaches_the_newest() {
    let (_dir, t) = started().await;
    sync_report(t.app.reports(), 1);
    sync_report(t.app.reports(), 2);
    let text = compose(&t, None).await;
    assert!(text.contains("\"n\": 2"), "{text}");
    assert!(!text.contains("\"n\": 1"), "{text}");
}

#[tokio::test]
async fn compose_answers_over_app_call() {
    let (_dir, t) = started().await;
    let tc_app::Answer::Reply { body, .. } =
        t.app.call("feedback/compose", br#"{"report":null}"#).await
    else {
        panic!("no reply");
    };
    let reply: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(reply["type"], "ok");
    let draft: FeedbackDraft = serde_json::from_value(reply["value"].clone()).unwrap();
    assert!(draft.attachment.starts_with("translationCore "));
    assert!(draft.attachment.ends_with("\nno Report\n"));
}

#[test]
fn platform_line_names_the_os_and_the_architecture() {
    let os = if cfg!(target_os = "linux") {
        "Linux"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else {
        std::env::consts::OS
    };
    assert_eq!(platform(), format!("{os} {}", std::env::consts::ARCH));
    assert!(platform().ends_with(std::env::consts::ARCH));
}

/// Made once with node: `for (const t of [0,-1,1791559266123,951782400000,4102444799999])
/// console.log(t, new Date(t).toISOString())`.
#[test]
fn iso_utc_millis_is_javascripts_to_iso_string() {
    for (at, iso) in [
        (0, "1970-01-01T00:00:00.000Z"),
        (-1, "1969-12-31T23:59:59.999Z"),
        (1_791_559_266_123, "2026-10-09T15:21:06.123Z"),
        (951_782_400_000, "2000-02-29T00:00:00.000Z"),
        (4_102_444_799_999, "2099-12-31T23:59:59.999Z"),
    ] {
        assert_eq!(iso_utc_millis(WallMs(at)), iso, "{at}");
    }
}

#[tokio::test]
async fn the_attachment_is_english_in_every_app_language() {
    let (_dir, t) = started().await;
    post(
        &t.app,
        &SetLocale {
            request: rid(1),
            locale: LocaleId::parse("ar").unwrap(),
        },
    )
    .await
    .unwrap();
    let text = compose(&t, None).await;
    assert!(text.starts_with("translationCore "), "{text}");
    assert!(text.ends_with("\nno Report\n"), "{text}");
}

/// The home folder and the account name never leave the computer (4.1.0-preview tester):
/// the first launch's report names the projects folder under the home folder, and a failure's
/// reason or facts can hold any path. A declared difference from tC4, which sends them whole.
#[tokio::test]
async fn the_attachment_carries_no_home_folder_and_no_account_name() {
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_in(dir.path(), |_, _| {}).await;
    let home = dir.path().join("home");
    let home = home.to_str().unwrap();
    let first = compose(&t, None).await;
    assert!(!first.contains(home), "INV-FEEDBACK-NO-HOME: {first}");
    assert!(first.contains("~/translationCore4/projects"), "{first}");
    t.app.reports().add(
        ReportOp::Sync,
        ReportOutcome::Failed {
            reason: format!("{home}/x is held by another program"),
        },
        facts(json!({ "folder": format!("{home}/AppData") })),
        WallMs(1),
        WallMs(2),
    );
    let text = compose(&t, None).await;
    assert!(!text.contains(home), "{text}");
    assert!(text.contains("~/x is held by another program"), "{text}");
}

#[test]
fn redact_takes_out_a_home_folder_on_each_system() {
    let mac = std::path::Path::new("/Users/ana");
    assert_eq!(
        redact(
            "/Users/ana/Documents and /Volumes/USB/ana/x and /Users/anabel",
            Some(mac)
        ),
        "~/Documents and /Volumes/USB/<user>/x and /Users/anabel"
    );
    let windows = std::path::Path::new(r"C:\Users\Ana");
    assert_eq!(
        redact(
            r#"c:\users\ANA\AppData "C:\\Users\\Ana\\x" D:\Ana\y C:/Users/Ana/z"#,
            Some(windows)
        ),
        r#"~\AppData "~\\x" D:\<user>\y ~/z"#
    );
    let linux = std::path::Path::new("/home/ana/");
    assert_eq!(redact("/home/ana", Some(linux)), "~");
    assert_eq!(redact("/home/ana", None), "/home/ana");
}
