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
