//! What the runtime does for each delivery (APP-DESIGN 6.3): every frame has an effect, and
//! the job map and the busy count follow their rules.

use std::collections::{BTreeMap, HashMap};

use proptest::prelude::*;
use proptest::test_runner::{Config, TestCaseError, TestRunner};
use tc_api::account::{AccountView, KeepState, KeychainState};
use tc_api::align::AlignSave;
use tc_api::app::{OpenSession, SessionRole};
use tc_api::event::{
    ChangeCause, ChangeParts, ChangeScope, CheckpointOutcome, Event, EventFrame, SessionEvent,
    SessionMode, StartupState,
};
use tc_api::ids::{AlignerId, Epoch, JobId, ProjectId, ProjectRef, Scope, ServerInstance, ViewSeq};
use tc_api::jobs::{JobKind, JobState, JobView, Progress};
use tc_ui::app::cache::AppTopic;
use tc_ui::app::effects::{
    BusyCount, Effect, FINISHED_JOBS_KEPT, active_jobs, apply_reread, forget_run, is_active,
    jobs_to_reread, merge_job, react, retain_open,
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
fn a_resync_and_an_unreadable_frame_refresh_everything_and_read_hello_and_the_jobs() {
    let both = vec![Effect::ReadEverything, Effect::ReadHello, Effect::ReadJobs];
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
        server: "git.door43.org".into(),
        keychain: KeychainState::Works,
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

/// The events the smoke writes into the page's stream (`crates/tc-ui/app/smoke-events.json`).
const SMOKE_EVENTS: &str = include_str!("../../app/smoke-events.json");

#[test]
fn the_events_the_smoke_injects_are_events_this_build_reads_and_do_what_the_smoke_expects() {
    // A frame the page cannot read is handled as a `Resync`, which would make the smoke's job
    // cases pass or fail for the wrong reason: so each one is parsed here, and its effect named.
    let events: BTreeMap<String, Event> = serde_json::from_str(SMOKE_EVENTS).unwrap();
    assert_eq!(
        events.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "alignSaving",
            "internetOn",
            "jobDone",
            "jobRunning",
            "resync"
        ]
    );
    let effects = |name: &str| react(&frame(events[name].clone()));
    assert_eq!(
        effects("resync"),
        vec![Effect::ReadEverything, Effect::ReadHello, Effect::ReadJobs]
    );
    assert_eq!(effects("internetOn"), vec![Effect::Internet(true)]);
    assert_eq!(
        effects("alignSaving"),
        vec![Effect::AlignSave {
            aligner: AlignerId::new(1),
            state: AlignSave::Saving
        }],
        "the aligner whose number the new run gives again"
    );
    let running = effects("jobRunning");
    let [Effect::Job(running)] = running.as_slice() else {
        panic!("a job frame is one job effect");
    };
    assert!(is_active(&running.state));
    let done = effects("jobDone");
    let [Effect::Job(done)] = done.as_slice() else {
        panic!("a job frame is one job effect");
    };
    assert!(!is_active(&done.state));
    assert_eq!(running.id, done.id, "the end frame ends the running job");
    // The reply the smoke gives `jobs/get` is the same view.
    let Event::Job(view) = &events["jobRunning"] else {
        panic!("jobRunning is a job");
    };
    let reply = serde_json::to_value(view).unwrap();
    assert_eq!(
        reply,
        serde_json::to_value(running).unwrap(),
        "jobs/get answers the view that a frame carries"
    );
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
fn only_the_jobs_held_as_active_are_read_again() {
    let mut jobs = BTreeMap::new();
    merge_job(&mut jobs, job(1, running()));
    merge_job(&mut jobs, job(2, JobState::Cancelled));
    merge_job(&mut jobs, job(3, JobState::Queued));
    let ids: Vec<JobId> = jobs_to_reread(&jobs).iter().map(|j| j.id).collect();
    assert_eq!(ids, vec![JobId::new(1), JobId::new(3)]);
}

#[test]
fn a_job_whose_end_frame_was_lost_is_not_left_running() {
    // The page saw the job run; the frame that ended it was in the gap a Resync names.
    let mut jobs = BTreeMap::new();
    merge_job(&mut jobs, job(1, running()));
    let held = job(1, running());
    assert_eq!(active_jobs(&jobs), 1);
    // The server still has the job, and it is done.
    apply_reread(&mut jobs, &held, Some(job(1, JobState::Cancelled)));
    assert_eq!(active_jobs(&jobs), 0);
    assert_eq!(
        jobs.get(&JobId::new(1)).map(|j| j.state.clone()),
        Some(JobState::Cancelled),
        "a finished job stays readable"
    );
}

#[test]
fn a_job_the_server_no_longer_knows_is_forgotten() {
    let mut jobs = BTreeMap::new();
    merge_job(&mut jobs, job(1, running()));
    merge_job(&mut jobs, job(2, running()));
    apply_reread(&mut jobs, &job(1, running()), None);
    assert!(!jobs.contains_key(&JobId::new(1)));
    assert!(jobs.contains_key(&JobId::new(2)), "only that job");
    assert_eq!(active_jobs(&jobs), 1);
}

#[test]
fn a_frame_that_came_during_the_call_is_newer_than_its_reply() {
    // The call was sent while the job ran; a frame then said it was cancelling; the reply
    // (computed before that frame) still says running. The frame stands.
    let mut jobs = BTreeMap::new();
    merge_job(&mut jobs, job(1, running()));
    let held = job(1, running());
    merge_job(&mut jobs, job(1, JobState::Cancelling));
    apply_reread(&mut jobs, &held, Some(job(1, running())));
    assert_eq!(
        jobs.get(&JobId::new(1)).map(|j| j.state.clone()),
        Some(JobState::Cancelling)
    );
    // The same holds for "unknown": a job a frame has since changed is not forgotten by a
    // reply made before it.
    apply_reread(&mut jobs, &held, None);
    assert!(jobs.contains_key(&JobId::new(1)));
    // And a job a frame removed from the map is not put back.
    let mut gone = BTreeMap::new();
    apply_reread(&mut gone, &held, Some(job(1, running())));
    assert!(gone.is_empty());
}

#[test]
fn the_page_is_idle_only_when_nothing_is_in_flight() {
    assert!(BusyCount::default().is_idle());
    for busy in [
        BusyCount {
            fetches: 1,
            ..BusyCount::default()
        },
        BusyCount {
            jobs: 2,
            ..BusyCount::default()
        },
        BusyCount {
            saves: 1,
            ..BusyCount::default()
        },
    ] {
        assert!(!busy.is_idle(), "{busy:?}");
    }
}

fn open_session(at: Scope) -> OpenSession {
    OpenSession {
        at,
        id: ProjectId::parse("6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50").unwrap(),
        mode: SessionMode::Writable,
        role: SessionRole::Screen,
        outside: Default::default(),
    }
}

#[test]
fn a_resync_forgets_what_was_held_for_an_open_that_hello_no_longer_lists() {
    // `hello` holds no checkpoint outcome and no blocked file, so a `Resync` cannot restore
    // them; it can say which opens are gone. A `Closed` frame lost in the gap must not leave
    // a failed checkpoint standing for a project that is closed.
    let still_open = at();
    let closed = Scope {
        project: ProjectRef::parse("PSA").unwrap(),
        epoch: Epoch::new(1),
    };
    let reopened = Scope {
        epoch: Epoch::new(3),
        ..still_open.clone()
    };
    let failed = CheckpointOutcome::Failed {
        reason: "disk full".into(),
        code: None,
    };
    let mut held: HashMap<Scope, CheckpointOutcome> = HashMap::from([
        (still_open.clone(), failed.clone()),
        (closed, failed.clone()),
        (reopened, failed.clone()),
    ]);
    retain_open(&mut held, &[open_session(still_open.clone())]);
    assert_eq!(
        held,
        HashMap::from([(still_open.clone(), failed)]),
        "the open that is listed keeps its failure; another epoch of its folder is another open"
    );

    // No open at all: nothing is held.
    retain_open(&mut held, &[]);
    assert!(held.is_empty());
}

#[test]
fn a_restart_forgets_everything_an_event_filled_that_the_old_runs_numbers_key() {
    // The new run numbers its jobs and aligners from the start and its opens from epoch 1: an
    // entry of the old run under the same number would be read as the new run's.
    let open = Scope {
        epoch: Epoch::new(1),
        ..at()
    };
    let mut jobs = BTreeMap::from([(JobId::new(1), job(1, running()))]);
    let mut checkpoints = HashMap::from([(
        open.clone(),
        CheckpointOutcome::Failed {
            reason: "disk full".into(),
            code: None,
        },
    )]);
    let mut blocked = HashMap::from([(open, vec!["TIT.usfm".to_owned()])]);
    let mut align_saves = HashMap::from([
        (AlignerId::new(1), AlignSave::Saving),
        (AlignerId::new(2), AlignSave::Unsaved),
    ]);

    forget_run(&mut jobs, &mut checkpoints, &mut blocked, &mut align_saves);

    assert!(jobs.is_empty(), "job 1 of the new run is another job");
    assert!(checkpoints.is_empty(), "TIT at epoch 1 is another open");
    assert!(blocked.is_empty());
    assert!(
        align_saves.is_empty(),
        "aligner 1 of the new run would show the old run's Saving before its first frame"
    );
}
