//! Determinate progress within 300 ms of the open (`docs/STORAGE.md` §6.1 "Progress",
//! tC4's J15 `e2e/j15-slow-open.spec.ts`).

use crate::common::*;
use crate::corpus::psalms;
use std::sync::{Arc, Mutex};
use std::thread::ThreadId;
use std::time::{Duration, Instant};
use tc_store::{OpenOptions, Phase, Progress, ProgressSink};

type Seen = Arc<Mutex<Vec<(Progress, ThreadId, Instant)>>>;

fn collecting() -> (ProgressSink, Seen) {
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let into = seen.clone();
    let sink: ProgressSink = Arc::new(move |p| {
        into.lock()
            .unwrap()
            .push((p, std::thread::current().id(), Instant::now()));
    });
    (sink, seen)
}

/// Counts never fall within a phase, phases come in order, and each phase reported ends at
/// its total.
fn assert_well_formed(reports: &[Progress]) {
    for pair in reports.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        assert!(
            a.phase < b.phase
                || (a.phase == b.phase && a.done <= b.done && a.bytes_done <= b.bytes_done),
            "{a:?} then {b:?}"
        );
        if a.phase != b.phase {
            assert_eq!(a.done, a.total, "phase {:?} ended below its total", a.phase);
        }
    }
    let last = reports.last().unwrap();
    assert_eq!(last.done, last.total);
    assert!(reports.iter().all(|r| r.total > 0));
}

#[test]
fn open_shows_determinate_progress_by_300_ms() {
    let corpus = psalms();
    let fs = corpus.fs.copy();
    let rig = rig_as(fs.clone(), APP, "host-1");
    // Every read waits, so the open lasts several seconds.
    fs.set_read_delay(Some(Duration::from_millis(5)));
    let (sink, seen) = collecting();
    let caller = std::thread::current().id();
    let started = Instant::now();
    let handle = rig
        .store
        .open(
            &corpus.folder,
            &OpenOptions {
                progress: sink,
                ..OpenOptions::default()
            },
        )
        .unwrap();
    let usable = Instant::now();
    fs.set_read_delay(None);
    let seen = seen.lock().unwrap().clone();
    assert!(
        usable - started > Duration::from_millis(600),
        "the open was not slow"
    );
    let (first, _, first_at) = &seen[0];
    let after = *first_at - started;
    assert!(
        after >= Duration::from_millis(300) && after <= Duration::from_millis(400),
        "first report after {after:?}"
    );
    assert!(first.total > 0);
    assert!(
        seen.iter().all(|(_, thread, _)| *thread != caller),
        "a report from the caller's thread"
    );
    assert!(
        seen.iter().all(|(_, _, at)| *at <= usable),
        "a report after usable"
    );
    let reports: Vec<Progress> = seen.iter().map(|(p, _, _)| p.clone()).collect();
    assert_well_formed(&reports);
    assert!(
        reports
            .iter()
            .any(|r| r.phase == Phase::Reading && r.bytes_total > 0)
    );
    handle.close().unwrap();
}

#[test]
fn a_quick_open_shows_nothing() {
    let (rig, folder) = titus();
    let (sink, seen) = collecting();
    let started = Instant::now();
    let handle = rig
        .store
        .open(
            &folder,
            &OpenOptions {
                progress: sink,
                ..OpenOptions::default()
            },
        )
        .unwrap();
    let took = started.elapsed();
    // On a slow runner the open may pass 300 ms and report; it must never report sooner.
    let seen = seen.lock().unwrap().clone();
    assert!(
        seen.iter()
            .all(|(_, _, at)| *at - started >= tc_store::progress::FIRST_REPORT),
        "a report before 300 ms"
    );
    if took < tc_store::progress::FIRST_REPORT {
        assert!(seen.is_empty(), "a quick open flashed an indicator");
    }
    handle.close().unwrap();
}

#[test]
fn an_open_that_fails_ends_with_its_error_after_its_last_report() {
    let corpus = psalms();
    let fs = corpus.fs.copy();
    let rig = rig_as(fs.clone(), APP, "host-1");
    // Every read waits, so the open is still reading the folder 400 ms after it began.
    fs.set_read_delay(Some(Duration::from_millis(15)));
    let started = Instant::now();
    fs.fail_reads_from(Some(started + Duration::from_millis(400)));
    let (sink, seen) = collecting();
    let result = rig.store.open(
        &corpus.folder,
        &OpenOptions {
            progress: sink,
            ..OpenOptions::default()
        },
    );
    let ended = Instant::now();
    fs.fail_reads_from(None);
    fs.set_read_delay(None);
    assert!(
        result.is_err(),
        "the open must fail when the folder stops being readable"
    );
    let seen = seen.lock().unwrap().clone();
    assert!(!seen.is_empty());
    assert!(seen.iter().all(|(_, _, at)| *at <= ended));
    let reports: Vec<Progress> = seen.iter().map(|(p, _, _)| p.clone()).collect();
    assert_well_formed(&reports);
}
