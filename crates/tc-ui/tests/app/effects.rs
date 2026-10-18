//! What the runtime does for each delivery (APP-DESIGN 6.3): every frame has an effect, and
//! the job map and the busy count follow their rules.

use std::collections::BTreeMap;

use proptest::prelude::*;
use proptest::test_runner::{Config, TestCaseError, TestRunner};
use tc_api::account::{AccountView, KeepState};
use tc_api::event::{
    ChangeCause, ChangeParts, ChangeScope, CheckpointOutcome, Event, EventFrame, SessionEvent,
    SessionMode, StartupState,
};
use tc_api::ids::{AlignerId, Epoch, JobId, ProjectRef, Scope, ServerInstance, ViewSeq};
use tc_api::jobs::{JobKind, JobState, JobView, Progress};
use tc_ui::app::cache::AppTopic;
use tc_ui::app::effects::{
    BusyCount, Effect, FINISHED_JOBS_KEPT, active_jobs, is_active, merge_job, react,
};
use tc_ui::app::events::Incoming;

fn instance(n: u8) -> ServerInstance {
    ServerInstance::from_bytes([n; 16])
}

fn at() -> Scope {
    Scope {
        project: ProjectRef::parse("TIT").unwrap(),
        epoch: Epoch::new(2),
    }
}

fn frame(event: Event) -> Incoming {
    Incoming::Frame(EventFrame {
        seq: 5,
        instance: instance(1),
        event,
    })
}

fn session(event: SessionEvent) -> Incoming {
    frame(Event::Session { at: at(), event })
}

fn job(id: u64, state: JobState) -> JobView {
    JobView {
        id: JobId::new(id),
        kind: JobKind::Sync,
        project: None,
        state,
    }
}

#[test]
fn a_resync_and_an_unreadable_frame_refresh_everything_and_read_hello() {
    let both = vec![Effect::ReadEverything, Effect::ReadHello];
    assert_eq!(react(&frame(Event::Resync)), both);
    let unreadable = Incoming::Unreadable {
        seq: Some(3),
        error: "unknown variant".into(),
    };
    assert_eq!(react(&unreadable), both);
}

#[test]
fn a_restart_forgets_the_scopes_and_reads_hello() {
    assert_eq!(
        react(&Incoming::Restart(instance(2))),
        vec![Effect::Restarted(instance(2)), Effect::ReadHello]
    );
}

#[test]
fn values_that_events_carry_whole_become_effects() {
    assert_eq!(
        react(&frame(Event::Internet { on: true })),
        vec![Effect::Internet(true)]
    );
    assert_eq!(
        react(&frame(Event::Startup(StartupState::Ready))),
        vec![Effect::Startup(StartupState::Ready)]
    );
    let finished = job(4, JobState::Cancelled);
    assert_eq!(
        react(&frame(Event::Job(finished.clone()))),
        vec![Effect::Job(finished)]
    );
    let account = AccountView {
        user: Some("ann".into()),
        kept: KeepState::Kept,
        notice: None,
    };
    assert_eq!(
        react(&frame(Event::Account(account.clone()))),
        vec![Effect::Account(account)]
    );
}

#[test]
fn topics_name_what_changed_in_the_app() {
    assert_eq!(
        react(&frame(Event::ProjectsChanged)),
        vec![Effect::Topic(AppTopic::Projects)]
    );
    let project = ProjectRef::parse("PSA").unwrap();
    assert_eq!(
        react(&frame(Event::HomeCard {
            project: project.clone()
        })),
        vec![Effect::Topic(AppTopic::HomeCard(project))]
    );
}

#[test]
fn a_session_event_acts_on_its_open() {
    assert_eq!(
        react(&session(SessionEvent::Opened)),
        vec![Effect::SessionOpened(at()), Effect::ReadHello],
        "an open names no project id or role, which hello has"
    );
    assert_eq!(
        react(&session(SessionEvent::Closed)),
        vec![Effect::SessionClosed(at())]
    );
    assert_eq!(
        react(&session(SessionEvent::Mode(SessionMode::Writable))),
        vec![Effect::SessionMode {
            at: at(),
            mode: SessionMode::Writable
        }]
    );
    let scope = ChangeScope::Parts(ChangeParts::default());
    assert_eq!(
        react(&session(SessionEvent::Changed {
            view: ViewSeq::new(3),
            scope: scope.clone(),
            cause: ChangeCause::System,
        })),
        vec![Effect::Changed { at: at(), scope }]
    );
    assert_eq!(
        react(&session(SessionEvent::Checkpoint(
            CheckpointOutcome::Committed
        ))),
        vec![Effect::Checkpoint {
            at: at(),
            outcome: CheckpointOutcome::Committed
        }]
    );
    assert_eq!(
        react(&session(SessionEvent::MaterializeBlocked {
            paths: vec!["TIT.usfm".into()]
        })),
        vec![Effect::MaterializeBlocked {
            at: at(),
            paths: vec!["TIT.usfm".into()]
        }]
    );
    let aligner = AlignerId::new(9);
    let state = tc_api::align::AlignSave::Saved;
    assert_eq!(
        react(&session(SessionEvent::AlignSave {
            aligner,
            state: state.clone()
        })),
        vec![Effect::AlignSave { aligner, state }]
    );
}

#[test]
fn no_frame_is_ignored() {
    // Every event tc-api can generate has an effect, whatever it holds.
    let outcome = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(|| {
            let mut runner = TestRunner::new(Config {
                cases: 512,
                failure_persistence: None,
                ..Config::default()
            });
            runner
                .run(&any::<EventFrame>(), |frame| {
                    let effects = react(&Incoming::Frame(frame));
                    if effects.is_empty() {
                        return Err(TestCaseError::fail("a frame with no effect"));
                    }
                    Ok(())
                })
                .map_err(|e| e.to_string())
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(outcome, Ok(()));
}

fn running() -> JobState {
    JobState::Running(Progress {
        phase: tc_api::jobs::Phase::Seed,
        done: 1,
        total: Some(4),
        item: None,
    })
}

#[test]
fn a_job_is_active_until_it_finishes() {
    assert!(is_active(&JobState::Queued));
    assert!(is_active(&running()));
    assert!(is_active(&JobState::Cancelling));
    assert!(!is_active(&JobState::Cancelled));
    assert!(!is_active(&JobState::Failed(
        tc_api::ApiError::StaleSession
    )));
}

#[test]
fn the_newest_view_of_a_job_replaces_the_older() {
    let mut jobs = BTreeMap::new();
    merge_job(&mut jobs, job(1, JobState::Queued));
    merge_job(&mut jobs, job(1, running()));
    assert_eq!(jobs.len(), 1);
    assert_eq!(
        jobs.get(&JobId::new(1)).map(|j| is_active(&j.state)),
        Some(true)
    );
    assert_eq!(active_jobs(&jobs), 1);
    merge_job(&mut jobs, job(1, JobState::Cancelled));
    assert_eq!(active_jobs(&jobs), 0);
    assert_eq!(jobs.len(), 1, "a finished job stays readable");
}

#[test]
fn finished_jobs_are_bounded_and_active_jobs_never_dropped() {
    let mut jobs = BTreeMap::new();
    // An old job that is still running, then a long run of short jobs.
    merge_job(&mut jobs, job(1, running()));
    let total = FINISHED_JOBS_KEPT as u64 * 3;
    for id in 2..2 + total {
        merge_job(&mut jobs, job(id, JobState::Queued));
        merge_job(&mut jobs, job(id, JobState::Cancelled));
    }
    assert_eq!(active_jobs(&jobs), 1);
    assert!(jobs.contains_key(&JobId::new(1)), "the running job stays");
    assert_eq!(jobs.len(), FINISHED_JOBS_KEPT + 1);
    // The ones kept are the newest.
    assert!(jobs.contains_key(&JobId::new(1 + total)));
    assert!(!jobs.contains_key(&JobId::new(2)));
}

#[test]
fn the_page_is_idle_only_when_nothing_is_in_flight() {
    assert!(BusyCount::default().is_idle());
    assert!(
        !BusyCount {
            fetches: 1,
            jobs: 0
        }
        .is_idle()
    );
    assert!(
        !BusyCount {
            fetches: 0,
            jobs: 2
        }
        .is_idle()
    );
}
