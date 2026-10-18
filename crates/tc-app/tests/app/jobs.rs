//! The job registry and its reporter: 300 ms of silence, then a report each 100 ms (a second
//! for jobs no screen shows), counts that only grow, a cancel that stops at the next check.
//!
//! Timing tests run on a paused clock and move it by hand in 10 ms steps, so each report's
//! time is exact. They never await a channel: with nothing but timers pending, a paused
//! clock jumps to the next timer by itself and would hide the 300 ms.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use proptest::prelude::*;
use tc_api::error::ApiError;
use tc_api::ids::{JobId, ProjectRef};
use tc_api::jobs::{JobKind, JobOutcome, JobState, OpenPhase, Phase};
use tc_app::events::Next;
use tc_app::flows::{FlowGate, FlowRequest};
use tc_app::jobs::{JobEnd, JobSpec};

use super::common::{ManualJob, done_of, drain_jobs, jobs, settle, step};

fn download() -> JobSpec {
    JobSpec::new(
        JobKind::Download,
        Phase::Install {
            repo: "en_ult".to_owned(),
        },
    )
}

#[tokio::test(start_paused = true)]
async fn job_reports_nothing_before_300ms_then_every_100ms() {
    let t = jobs();
    let mut reader = t.bus.subscribe(0);
    let job = ManualJob::start(&t.jobs, download()).await;
    let mut seen = Vec::new();
    // One unit of work every 10 ms for 600 ms, then none.
    for ms in (10..=1000).step_by(10) {
        if ms <= 600 {
            job.ctx.progress.add(1);
        }
        step(10).await;
        drain_jobs(&mut reader, ms, &mut seen);
    }
    let reports: Vec<(u64, u64)> = seen.iter().map(|(ms, s)| (*ms, done_of(s))).collect();
    // Nothing in [0, 300); the first at 300 ms with the 30 units done by then; then one per
    // 100 ms while the count moves; none once it stopped at 60.
    assert_eq!(reports, [(300, 30), (400, 40), (500, 50), (600, 60)]);

    // The end comes at once, and is the last frame.
    job.finish(Ok(JobOutcome::Finished));
    settle().await;
    drain_jobs(&mut reader, 1000, &mut seen);
    assert_eq!(seen.len(), 5);
    assert_eq!(seen[4].0, 1000);
    assert!(matches!(seen[4].1, JobState::Done(JobOutcome::Finished)));
    step(2000).await;
    drain_jobs(&mut reader, 3000, &mut seen);
    assert_eq!(seen.len(), 5);
}

#[tokio::test(start_paused = true)]
async fn unseen_jobs_report_once_a_second() {
    for kind in [JobKind::Train, JobKind::HomeSummary] {
        let t = jobs();
        let mut reader = t.bus.subscribe(0);
        let job = ManualJob::start(&t.jobs, JobSpec::new(kind, Phase::Summary)).await;
        let mut seen = Vec::new();
        for ms in (10..=2500).step_by(10) {
            job.ctx.progress.add(1);
            step(10).await;
            drain_jobs(&mut reader, ms, &mut seen);
        }
        let reports: Vec<(u64, u64)> = seen.iter().map(|(ms, s)| (*ms, done_of(s))).collect();
        assert_eq!(reports, [(300, 30), (1300, 130), (2300, 230)], "{kind:?}");
        job.finish(Ok(JobOutcome::Finished));
        settle().await;
        drain_jobs(&mut reader, 2500, &mut seen);
        assert_eq!(seen.len(), 4, "{kind:?}");
        assert_eq!(seen[3].0, 2500, "the end comes at once");
        assert!(matches!(seen[3].1, JobState::Done(_)));
    }
}

#[tokio::test(start_paused = true)]
async fn a_job_that_ends_before_300ms_sends_only_its_outcome() {
    let t = jobs();
    let mut reader = t.bus.subscribe(0);
    let job = ManualJob::start(&t.jobs, download()).await;
    job.ctx.progress.add(5);
    step(200).await;
    job.finish(Ok(JobOutcome::Finished));
    settle().await;
    let mut seen = Vec::new();
    drain_jobs(&mut reader, 200, &mut seen);
    assert_eq!(seen.len(), 1);
    assert!(matches!(seen[0].1, JobState::Done(JobOutcome::Finished)));
    // The 300 ms mark passes with nothing more.
    step(500).await;
    drain_jobs(&mut reader, 700, &mut seen);
    assert_eq!(seen.len(), 1);
}

#[tokio::test(start_paused = true)]
async fn the_last_count_comes_before_the_end() {
    let t = jobs();
    let mut reader = t.bus.subscribe(0);
    let job = ManualJob::start(&t.jobs, download()).await;
    job.ctx.progress.add(1);
    let mut seen = Vec::new();
    for ms in (10..=450).step_by(10) {
        step(10).await;
        drain_jobs(&mut reader, ms, &mut seen);
    }
    // The count moves at 450 ms, and the job ends at 460 ms, before the 500 ms report.
    job.ctx.progress.add(5);
    step(10).await;
    job.finish(Ok(JobOutcome::Finished));
    settle().await;
    drain_jobs(&mut reader, 460, &mut seen);
    let shape: Vec<(u64, Option<u64>)> = seen
        .iter()
        .map(|(ms, s)| (*ms, matches!(s, JobState::Running(_)).then(|| done_of(s))))
        .collect();
    assert_eq!(shape, [(300, Some(1)), (460, Some(6)), (460, None)]);
    assert!(matches!(seen[2].1, JobState::Done(_)));
}

#[derive(Debug, Clone)]
enum SinkOp {
    /// The next phase of the job, or the same one again.
    NextPhase(Option<u64>),
    SamePhase(Option<u64>),
    Add(u64),
    Reach(u64),
    Total(u64),
    Item(bool),
    Wait(u64),
}

fn sink_ops() -> impl Strategy<Value = Vec<SinkOp>> {
    proptest::collection::vec(
        prop_oneof![
            1 => proptest::option::of(0u64..500).prop_map(SinkOp::NextPhase),
            1 => proptest::option::of(0u64..500).prop_map(SinkOp::SamePhase),
            4 => (0u64..50).prop_map(SinkOp::Add),
            3 => (0u64..500).prop_map(SinkOp::Reach),
            1 => (0u64..500).prop_map(SinkOp::Total),
            1 => any::<bool>().prop_map(SinkOp::Item),
            4 => (1u64..40).prop_map(|n| SinkOp::Wait(n * 10)),
        ],
        1..80,
    )
}

/// The phases of an open, in order. A job moves through its phases and does not go back.
fn phase_of(n: usize) -> Phase {
    Phase::Open(match n {
        0 => OpenPhase::FinishingWork,
        1 => OpenPhase::ReadingChanges,
        2 => OpenPhase::Folding,
        3 => OpenPhase::CheckingFiles,
        _ => OpenPhase::WritingFiles,
    })
}

proptest! {
    /// In the `Running` frames of a job, the count never falls between two frames of one phase,
    /// whatever the job does to its sink, and every frame is a state the sink really had.
    #[test]
    fn counts_only_grow(ops in sink_ops()) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .unwrap();
        let frames = runtime.block_on(async {
            let t = jobs();
            let mut reader = t.bus.subscribe(0);
            let spec = JobSpec::new(JobKind::Open, phase_of(0));
            let job = ManualJob::start(&t.jobs, spec).await;
            let mut seen = Vec::new();
            let mut now = 0;
            let mut phase = 0;
            for op in ops {
                match op {
                    SinkOp::NextPhase(total) => {
                        phase += 1;
                        job.ctx.progress.phase(phase_of(phase), total);
                    }
                    SinkOp::SamePhase(total) => job.ctx.progress.phase(phase_of(phase), total),
                    SinkOp::Add(n) => job.ctx.progress.add(n),
                    SinkOp::Reach(n) => job.ctx.progress.reach(n),
                    SinkOp::Total(n) => job.ctx.progress.total(n),
                    SinkOp::Item(on) => job.ctx.progress.item(on.then(|| "a.usfm".to_owned())),
                    SinkOp::Wait(ms) => {
                        for _ in 0..ms / 10 {
                            step(10).await;
                            now += 10;
                            drain_jobs(&mut reader, now, &mut seen);
                        }
                    }
                }
            }
            job.finish(Ok(JobOutcome::Finished));
            settle().await;
            drain_jobs(&mut reader, now, &mut seen);
            seen
        });
        let mut last: Option<(Phase, u64)> = None;
        for (_, state) in &frames {
            if let JobState::Running(progress) = state {
                if let Some((phase, done)) = &last
                    && *phase == progress.phase
                {
                    prop_assert!(progress.done >= *done, "fell from {done} to {}", progress.done);
                }
                last = Some((progress.phase.clone(), progress.done));
            }
        }
        prop_assert!(matches!(frames.last(), Some((_, JobState::Done(_)))));
    }
}

#[tokio::test(start_paused = true)]
async fn the_reporter_reports_while_the_job_thread_is_blocked() {
    // PERF rule 4: a slow read never holds the first report back.
    let t = jobs();
    let mut reader = t.bus.subscribe(0);
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
    let spec = JobSpec::new(JobKind::Open, Phase::Open(OpenPhase::ReadingChanges));
    t.jobs.start_blocking(spec, move |ctx| {
        ctx.progress.total(10);
        ctx.progress.reach(5);
        ready_tx.send(()).unwrap();
        // The slow read: the thread is busy until the test lets it go.
        go_rx.recv().unwrap();
        Ok(JobOutcome::Finished)
    });
    while ready_rx.try_recv().is_err() {
        std::thread::sleep(Duration::from_millis(1));
        tokio::task::yield_now().await;
    }
    let mut seen = Vec::new();
    for ms in (10..=300).step_by(10) {
        step(10).await;
        drain_jobs(&mut reader, ms, &mut seen);
    }
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, 300);
    match &seen[0].1 {
        JobState::Running(progress) => {
            assert_eq!((progress.done, progress.total), (5, Some(10)));
        }
        other => panic!("{other:?}"),
    }
    go_tx.send(()).unwrap();
    for _ in 0..5000 {
        std::thread::sleep(Duration::from_millis(1));
        settle().await;
        drain_jobs(&mut reader, 300, &mut seen);
        if seen.len() == 2 {
            break;
        }
    }
    assert!(matches!(seen[1].1, JobState::Done(_)), "{seen:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_stops_at_next_chunk() {
    let t = jobs();
    let mut reader = t.bus.subscribe(0);
    let worked = Arc::new(AtomicU64::new(0));
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
    let counter = Arc::clone(&worked);
    let id = t.jobs.start_blocking(download(), move |ctx| {
        for _ in 0..100 {
            ctx.cancel.check()?;
            counter.fetch_add(1, Ordering::SeqCst);
            ctx.progress.add(1);
            // The next chunk waits for the test.
            if go_rx.recv().is_err() {
                return Err(JobEnd::Cancelled);
            }
        }
        Ok(JobOutcome::Finished)
    });
    for chunk in 1..=3 {
        wait_until(|| worked.load(Ordering::SeqCst) == chunk).await;
        if chunk < 3 {
            go_tx.send(()).unwrap();
        }
    }
    // Three chunks are done, and the job waits to start the fourth.
    t.jobs.cancel(id);
    // `Cancelling` is published at once, before the job has noticed.
    let mut states = Vec::new();
    while let Some(Next::Frame(frame)) = reader.try_next() {
        if let tc_api::event::Event::Job(view) = frame.event {
            states.push(view.state);
        }
    }
    assert_eq!(states.last(), Some(&JobState::Cancelling), "{states:?}");
    go_tx.send(()).unwrap();
    loop {
        match tokio::time::timeout(Duration::from_secs(5), reader.next())
            .await
            .unwrap()
        {
            Next::Frame(frame) => {
                if let tc_api::event::Event::Job(view) = frame.event {
                    let end = view.state == JobState::Cancelled;
                    states.push(view.state);
                    if end {
                        break;
                    }
                }
            }
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(
        worked.load(Ordering::SeqCst),
        3,
        "no chunk ran after the cancel"
    );
    let cancelling = states
        .iter()
        .position(|s| *s == JobState::Cancelling)
        .unwrap();
    assert!(
        states[cancelling..]
            .iter()
            .all(|s| matches!(s, JobState::Cancelling | JobState::Cancelled)),
        "a frame after Cancelling: {states:?}"
    );
    assert_eq!(states.last(), Some(&JobState::Cancelled));
    assert_eq!(t.jobs.view(id).unwrap().state, JobState::Cancelled);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(worked.load(Ordering::SeqCst), 3);
}

async fn wait_until(mut condition: impl FnMut() -> bool) {
    for _ in 0..5000 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!("the condition never held");
}

#[tokio::test(start_paused = true)]
async fn no_running_frame_follows_cancelling() {
    let t = jobs();
    let mut reader = t.bus.subscribe(0);
    let job = ManualJob::start(&t.jobs, download()).await;
    job.ctx.progress.add(1);
    let mut seen = Vec::new();
    for ms in (10..=350).step_by(10) {
        step(10).await;
        drain_jobs(&mut reader, ms, &mut seen);
    }
    t.jobs.cancel(job.id);
    assert!(job.ctx.cancel.is_cancelled());
    // The job keeps counting for a while before it notices.
    for ms in (360..=700).step_by(10) {
        job.ctx.progress.add(1);
        step(10).await;
        drain_jobs(&mut reader, ms, &mut seen);
    }
    job.finish(Err(JobEnd::Cancelled));
    settle().await;
    drain_jobs(&mut reader, 700, &mut seen);
    let states: Vec<&JobState> = seen.iter().map(|(_, s)| s).collect();
    assert!(matches!(states[0], JobState::Running(_)));
    assert_eq!(states[1..], [&JobState::Cancelling, &JobState::Cancelled]);
}

#[tokio::test(start_paused = true)]
async fn a_cancelled_job_that_finishes_anyway_is_done() {
    // Its last check had passed, so the work is complete (§4.6, Download).
    let t = jobs();
    let job = ManualJob::start(&t.jobs, download()).await;
    let id = job.id;
    t.jobs.cancel(id);
    job.finish(Ok(JobOutcome::Finished));
    settle().await;
    assert_eq!(
        t.jobs.view(id).unwrap().state,
        JobState::Done(JobOutcome::Finished)
    );
}

#[tokio::test(start_paused = true)]
async fn a_failed_job_reports_its_error() {
    let t = jobs();
    let mut reader = t.bus.subscribe(0);
    let job = ManualJob::start(&t.jobs, download()).await;
    let id = job.id;
    job.finish(Err(JobEnd::Failed(ApiError::StaleSession)));
    settle().await;
    let mut seen = Vec::new();
    drain_jobs(&mut reader, 0, &mut seen);
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].1, JobState::Failed(ApiError::StaleSession));
    assert_eq!(
        t.jobs.view(id).unwrap().state,
        JobState::Failed(ApiError::StaleSession)
    );
}

#[tokio::test(start_paused = true)]
async fn a_job_that_panics_fails_with_an_internal_report() {
    let t = jobs();
    let mut reader = t.bus.subscribe(0);
    let gate = FlowGate::default();
    let pass = gate
        .enter(FlowRequest::Sync(ProjectRef::parse("titus").unwrap()))
        .unwrap();
    let spec = JobSpec::new(JobKind::Sync, Phase::Train).pass(pass);
    t.jobs
        .start_blocking(spec, |_ctx| -> Result<JobOutcome, JobEnd> {
            panic!("the disk fell out");
        });
    let mut seen = Vec::new();
    for _ in 0..5000 {
        std::thread::sleep(Duration::from_millis(1));
        settle().await;
        drain_jobs(&mut reader, 0, &mut seen);
        if !seen.is_empty() {
            break;
        }
    }
    let JobState::Failed(ApiError::Internal { report }) = &seen[0].1 else {
        panic!("{seen:?}");
    };
    let report = t.reports.get(*report).unwrap();
    assert_eq!(report.facts["stopped"], "the disk fell out");
    // The flow is free again, as the job is over.
    assert!(gate.running().is_empty());
    let _free = gate
        .enter(FlowRequest::Sync(ProjectRef::parse("jonah").unwrap()))
        .unwrap();
}

#[tokio::test(start_paused = true)]
async fn a_pass_in_the_spec_is_held_until_the_job_ends() {
    let t = jobs();
    let gate = FlowGate::default();
    let project = ProjectRef::parse("titus").unwrap();
    let pass = gate.enter(FlowRequest::Sync(project.clone())).unwrap();
    let job = ManualJob::start(
        &t.jobs,
        JobSpec::new(JobKind::Sync, Phase::Train).pass(pass),
    )
    .await;
    assert!(gate.enter(FlowRequest::Sync(project.clone())).is_err());
    assert_eq!(t.jobs.live(), 1);
    job.finish(Ok(JobOutcome::Finished));
    settle().await;
    assert!(gate.enter(FlowRequest::Sync(project)).is_ok());
    assert_eq!(t.jobs.live(), 0);
}

#[tokio::test(start_paused = true)]
async fn a_job_is_queued_until_its_first_report_and_has_a_view() {
    let t = jobs();
    let job = ManualJob::start(
        &t.jobs,
        download().project(ProjectRef::parse("titus").unwrap()),
    )
    .await;
    let view = t.jobs.view(job.id).unwrap();
    assert_eq!(view.state, JobState::Queued);
    assert_eq!(view.kind, JobKind::Download);
    assert_eq!(view.project, Some(ProjectRef::parse("titus").unwrap()));
    job.ctx.progress.add(3);
    step(300).await;
    assert_eq!(done_of(&t.jobs.view(job.id).unwrap().state), 3);
}

#[tokio::test(start_paused = true)]
async fn jobs_get_keeps_an_outcome_10_minutes() {
    let t = jobs();
    let job = ManualJob::start(&t.jobs, download()).await;
    let id = job.id;
    job.finish(Ok(JobOutcome::Finished));
    settle().await;
    tokio::time::advance(Duration::from_secs(9 * 60 + 59)).await;
    assert!(matches!(t.jobs.view(id).unwrap().state, JobState::Done(_)));
    tokio::time::advance(Duration::from_secs(2)).await;
    assert_eq!(t.jobs.view(id), None);
}

#[tokio::test(start_paused = true)]
async fn cancel_of_a_finished_or_unknown_job_does_nothing() {
    let t = jobs();
    let job = ManualJob::start(&t.jobs, download()).await;
    let id = job.id;
    job.finish(Ok(JobOutcome::Finished));
    settle().await;
    let mut reader = t.bus.subscribe(t.bus.head());
    t.jobs.cancel(id);
    t.jobs.cancel(JobId::new(9999));
    settle().await;
    assert!(reader.try_next().is_none());
    assert!(matches!(t.jobs.view(id).unwrap().state, JobState::Done(_)));
    assert_eq!(t.jobs.view(JobId::new(9999)), None);
}

#[tokio::test(start_paused = true)]
async fn job_ids_are_not_reused() {
    let t = jobs();
    let first = ManualJob::start(&t.jobs, download()).await;
    let second = ManualJob::start(&t.jobs, download()).await;
    assert_ne!(first.id, second.id);
}

#[tokio::test(start_paused = true)]
async fn waiting_for_the_cancel_token_resolves_when_asked() {
    let t = jobs();
    let job = ManualJob::start(&t.jobs, download()).await;
    let token = job.ctx.cancel.clone();
    let waiter = tokio::spawn(async move { token.cancelled().await });
    settle().await;
    assert!(!waiter.is_finished());
    t.jobs.cancel(job.id);
    settle().await;
    assert!(waiter.is_finished());
}
