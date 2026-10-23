//! The operation report log.

use serde_json::Map;
use tc_api::ids::{ReportId, WallMs};
use tc_app::reports::{ReportLog, ReportOp, ReportOutcome, facts};

fn add(log: &ReportLog, n: i64) -> ReportId {
    log.add(
        ReportOp::Sync,
        ReportOutcome::Ok,
        facts(serde_json::json!({ "n": n })),
        WallMs(n),
        WallMs(n + 1),
    )
}

#[test]
fn the_report_log_keeps_the_last_50() {
    let log = ReportLog::new();
    for n in 1..=60 {
        assert_eq!(add(&log, n).get(), n as u64);
    }
    let kept = log.all();
    assert_eq!(kept.len(), ReportLog::KEPT);
    assert_eq!(kept[0].id.get(), 11);
    assert_eq!(kept[49].id.get(), 60);
    assert!(log.get(ReportId::new(5)).is_none());
    assert_eq!(log.get(ReportId::new(11)).unwrap().facts["n"], 11);
}

#[test]
fn to_attach_prefers_the_refused_operation() {
    let log = ReportLog::new();
    let first = add(&log, 1);
    let _second = add(&log, 2);
    let newest = add(&log, 3);
    assert_eq!(log.to_attach(Some(first)).unwrap().id, first);
    assert_eq!(log.to_attach(None).unwrap().id, newest);
}

#[test]
fn to_attach_falls_back_to_the_newest_when_the_named_one_was_dropped() {
    let log = ReportLog::new();
    let first = add(&log, 1);
    for n in 2..=60 {
        add(&log, n);
    }
    assert!(log.get(first).is_none());
    assert_eq!(log.to_attach(Some(first)).unwrap().id.get(), 60);
}

#[test]
fn no_reports_attach_nothing() {
    assert!(ReportLog::new().to_attach(None).is_none());
    assert!(ReportLog::new().to_attach(Some(ReportId::new(1))).is_none());
}

#[test]
fn ids_are_never_reused_after_a_clear() {
    let log = ReportLog::new();
    add(&log, 1);
    add(&log, 2);
    log.clear();
    assert!(log.all().is_empty());
    assert_eq!(add(&log, 3).get(), 3);
}

#[test]
fn an_operation_is_named_as_tc4_names_it() {
    let names: Vec<String> = [ReportOp::PinComplete, ReportOp::OpenDoor43, ReportOp::Sync]
        .iter()
        .map(|op| {
            serde_json::to_value(op)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(names, ["pin-complete", "open-door43", "sync"]);
}

#[test]
fn facts_of_something_that_is_not_an_object_are_empty() {
    assert_eq!(facts(serde_json::json!([1, 2])), Map::new());
}

// ---- reports.json (unit C1a) ----------------------------------------------------------------

mod kept {
    use std::sync::Arc;

    use serde_json::Value;
    use tc_api::app::ClientError;
    use tc_api::feedback::Compose;
    use tc_api::ids::WallMs;
    use tc_api::test::Reset;
    use tc_app::FilesChoice;
    use tc_app::clock::ManualClock;
    use tc_app::prefs::{AppDataPath, MemoryFiles};
    use tc_app::reports::{ReportOp, ReportOutcome};

    use crate::app::common::{Parts, TestApp, post, prefs_over, rid, settle, step};

    fn client_error(n: u8) -> ClientError {
        ClientError {
            request: rid(n),
            view: Some("home".to_owned()),
            project: None,
            message: format!("panic {n}"),
            at: WallMs(i64::from(n)),
        }
    }

    fn reports_file(bytes: &[u8]) -> Vec<Value> {
        let file: Value = serde_json::from_slice(bytes).unwrap();
        assert_eq!(file["schemaVersion"], 1);
        file["reports"].as_array().unwrap().clone()
    }

    /// Declared difference (APP-DESIGN §4.9): tC4 keeps its reports in memory only, so after a
    /// crash and a relaunch its "Ask for help" attaches nothing of what went wrong.
    #[tokio::test]
    async fn reports_survive_a_relaunch_and_ids_continue() {
        let dir = tempfile::tempdir().unwrap();
        let first = TestApp::start_in(dir.path(), |_, deps| deps.files = FilesChoice::OnDisk).await;
        post(&first.app, &client_error(1)).await.unwrap();
        let kept = first.app.reports().all();
        let report = kept.last().unwrap().clone();
        assert_eq!(report.op, ReportOp::ClientError);
        first.app.quit().await;
        drop(first);

        let mut parts = Parts::new();
        parts.clock = Arc::new(ManualClock::new(5_000_000));
        let second = TestApp::start_with(dir.path(), &parts, |_, deps| {
            deps.files = FilesChoice::OnDisk;
        })
        .await;
        let attachment = post(&second.app, &Compose { report: None })
            .await
            .unwrap()
            .attachment;
        assert!(
            attachment.contains("Report: client-error, 1970-01-01T00:16:40.000Z"),
            "the first run's report, at the first run's time: {attachment}"
        );
        assert!(attachment.contains("panic 1"), "{attachment}");
        assert_eq!(second.app.reports().get(report.id), Some(report.clone()));

        post(&second.app, &client_error(2)).await.unwrap();
        let newest = second.app.reports().all().last().unwrap().id;
        assert!(newest > report.id, "{newest:?} after {:?}", report.id);
    }

    #[tokio::test(start_paused = true)]
    async fn reports_are_written_a_second_after_the_first_change_and_at_quit() {
        let env = prefs_over(Arc::new(MemoryFiles::new()));
        let path = AppDataPath::reports();
        let writes = |env: &crate::app::common::Prefs| {
            env.files
                .writes()
                .iter()
                .filter(|(p, _)| *p == path)
                .count()
        };
        let add = |n: i64| {
            env.reports.add(
                ReportOp::Sync,
                ReportOutcome::Ok,
                serde_json::Map::new(),
                WallMs(n),
                WallMs(n),
            )
        };
        add(1);
        settle().await;
        step(999).await;
        assert_eq!(writes(&env), 0, "nothing before a second");
        add(2);
        settle().await;
        step(1).await;
        assert_eq!(writes(&env), 1, "one write a second after the first change");
        assert_eq!(reports_file(&env.files.get(&path).unwrap()).len(), 2);
        add(3);
        env.prefs.flush().await;
        assert_eq!(writes(&env), 2, "a quit writes what is left");
        let kept = reports_file(&env.files.get(&path).unwrap());
        assert_eq!(kept.len(), 3);
        assert_eq!(kept[2]["op"], "sync");
        assert_eq!(kept[2]["outcome"], serde_json::json!({"type": "ok"}));
        step(5_000).await;
        assert_eq!(writes(&env), 2, "nothing more to write");
    }

    #[tokio::test(start_paused = true)]
    async fn a_disk_that_refuses_reports_json_costs_one_report_per_change() {
        let env = prefs_over(Arc::new(MemoryFiles::new()));
        env.files.fail_writes(true);
        env.reports.add(
            ReportOp::Sync,
            ReportOutcome::Ok,
            serde_json::Map::new(),
            WallMs(1),
            WallMs(1),
        );
        settle().await;
        step(60_000).await;
        let kept = env.reports.all();
        assert_eq!(
            kept.len(),
            2,
            "the change and the failure to write it: {kept:?}"
        );
        assert_eq!(kept[1].op, ReportOp::PrefsWrite);
    }

    #[tokio::test]
    async fn an_unreadable_reports_file_is_kept_aside_and_reported() {
        let dir = tempfile::tempdir().unwrap();
        let parts = Parts::new();
        parts
            .files
            .seed(AppDataPath::reports(), b"[1, 2, 3]".to_vec());
        let t = TestApp::start_with(dir.path(), &parts, |_, _| {}).await;
        assert_eq!(
            parts
                .files
                .get(&AppDataPath::reports().unreadable_copy())
                .as_deref(),
            Some(&b"[1, 2, 3]"[..])
        );
        let kept: Vec<_> = t
            .app
            .reports()
            .all()
            .into_iter()
            .filter(|r| r.op == ReportOp::PrefsWrite)
            .collect();
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert_eq!(kept[0].facts["file"], "reports.json");
    }

    #[tokio::test]
    async fn an_entry_a_newer_build_wrote_is_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let parts = Parts::new();
        parts.files.seed(
            AppDataPath::reports(),
            br#"{"schemaVersion":1,"reports":[
                {"id":3,"op":"sync","outcome":{"type":"ok"},"facts":{},"startedAt":1,"endedAt":2},
                {"id":4,"op":"teleport","outcome":{"type":"ok"},"facts":{},"startedAt":1,"endedAt":2}
            ]}"#
            .to_vec(),
        );
        let t = TestApp::start_with(dir.path(), &parts, |_, _| {}).await;
        let kept: Vec<u64> = t.app.reports().all().iter().map(|r| r.id.get()).collect();
        assert_eq!(kept[0], 3, "{kept:?}");
        assert!(
            kept[1..].iter().all(|id| *id > 4),
            "the left-out entry's id is not given again: {kept:?}"
        );
    }

    #[tokio::test]
    async fn a_reset_empties_the_reports_file() {
        let dir = tempfile::tempdir().unwrap();
        let t = TestApp::start_in(dir.path(), |_, _| {}).await;
        post(&t.app, &client_error(1)).await.unwrap();
        t.app.prefs().flush().await;
        assert!(!reports_file(&t.files.get(&AppDataPath::reports()).unwrap()).is_empty());
        post(&t.app, &Reset { request: rid(9) }).await.unwrap();
        assert!(
            reports_file(&t.files.get(&AppDataPath::reports()).unwrap()).is_empty(),
            "written by the reset itself"
        );
    }
}
