//! The job follower's rule (UI-1, INV-ONE-JOB-FOLLOWER): every job ends once, with the state
//! the server gave, and a job the server no longer knows ends lost.

use proptest::prelude::*;
use tc_api::error::ApiError;
use tc_api::jobs::{JobOutcome, JobState, Phase, Progress};
use tc_api::resources::Download;
use tc_ui::app::jobs::{FollowState, JobEnd, Step, typed};

fn running(done: u64) -> JobState {
    JobState::Running(Progress {
        phase: Phase::Install {
            repo: "en_ult".into(),
        },
        done,
        total: None,
        item: None,
    })
}

/// Steps 50 ms apart over `seen`, answering every server question with `answer`; the end and
/// how many times the server was asked.
fn follow(
    seen: &[Option<JobState>],
    answer: &Option<JobState>,
) -> (Option<JobEnd<JobOutcome>>, usize) {
    let mut state = FollowState::default();
    let mut asked = 0;
    for s in seen {
        match state.step(s.as_ref(), 50) {
            Step::End(end) => return (Some(end), asked),
            Step::AskServer => {
                asked += 1;
                if let Some(Step::End(end)) = state.asked(answer.clone()) {
                    return (Some(end), asked);
                }
            }
            Step::Wait | Step::Progress(_) => {}
        }
    }
    (None, asked)
}

#[test]
fn a_job_ends_once_with_the_state_the_server_gave() {
    let queued = Some(JobState::Queued);
    let busy = Some(running(1));
    for (last, end) in [
        (
            JobState::Done(JobOutcome::Finished),
            JobEnd::Done(JobOutcome::Finished),
        ),
        (
            JobState::Failed(ApiError::NeedsInternet(tc_api::NetKind::Sources)),
            JobEnd::Failed(ApiError::NeedsInternet(tc_api::NetKind::Sources)),
        ),
        (JobState::Cancelled, JobEnd::Cancelled),
    ] {
        let seen = [queued.clone(), busy.clone(), Some(last)];
        assert_eq!(follow(&seen, &busy).0, Some(end));
    }
    // Progress is passed on while the job runs.
    let mut state = FollowState::default();
    assert_eq!(
        state.step(Some(&running(3)), 50),
        Step::Progress(match running(3) {
            JobState::Running(p) => p,
            _ => unreachable!(),
        })
    );
    // A row's outcome is typed; another row's outcome ends lost.
    assert_eq!(
        typed::<Download>(JobEnd::Done(JobOutcome::Finished)),
        JobEnd::Done(())
    );
    assert_eq!(
        typed::<Download>(JobEnd::Done(JobOutcome::Created {
            project: tc_api::ids::ProjectRef::parse("p").unwrap()
        })),
        JobEnd::Lost
    );
}

#[test]
fn a_job_the_server_no_longer_knows_ends_lost() {
    // Nothing on the event stream for a second: the follower asks, and the server does not
    // know the job.
    let seen = vec![None; 40];
    let (end, asked) = follow(&seen, &None);
    assert_eq!(end, Some(JobEnd::Lost));
    assert_eq!(asked, 1, "asked once a second, after 1,000 ms");
    // A job the server still runs is asked for again a second later, and never ends by itself.
    let (end, asked) = follow(&vec![None; 41], &Some(running(1)));
    assert_eq!(end, None);
    assert_eq!(asked, 2);
}

fn arb_state() -> impl Strategy<Value = Option<JobState>> {
    prop_oneof![
        Just(None),
        Just(Some(JobState::Queued)),
        (0u64..5).prop_map(|d| Some(running(d))),
        Just(Some(JobState::Cancelling)),
        Just(Some(JobState::Cancelled)),
        Just(Some(JobState::Done(JobOutcome::Finished))),
        Just(Some(JobState::Failed(ApiError::NeedsInternet(
            tc_api::NetKind::Sources
        )))),
    ]
}

proptest! {
    /// Over any sequence of states and server answers, the follow ends at most once, ends at
    /// the first final state the stream or the server shows, and never ends while the job
    /// runs and the server knows it.
    #[test]
    fn the_follow_of_a_job_ends_with_one_end_on_every_sequence(
        seen in proptest::collection::vec(arb_state(), 0..80),
        answer in arb_state(),
    ) {
        let mut state = FollowState::default();
        let mut ends = Vec::new();
        let mut first_final: Option<JobEnd<JobOutcome>> = None;
        for s in &seen {
            let shown_final = match s {
                Some(JobState::Done(o)) => Some(JobEnd::Done(o.clone())),
                Some(JobState::Failed(e)) => Some(JobEnd::Failed(e.clone())),
                Some(JobState::Cancelled) => Some(JobEnd::Cancelled),
                _ => None,
            };
            match state.step(s.as_ref(), 50) {
                Step::End(end) => {
                    prop_assert_eq!(Some(end.clone()), shown_final.clone());
                    ends.push(end);
                    break;
                }
                Step::AskServer => {
                    let expected = match &answer {
                        None => Some(JobEnd::Lost),
                        Some(JobState::Done(o)) => Some(JobEnd::Done(o.clone())),
                        Some(JobState::Failed(e)) => Some(JobEnd::Failed(e.clone())),
                        Some(JobState::Cancelled) => Some(JobEnd::Cancelled),
                        Some(_) => None,
                    };
                    match state.asked(answer.clone()) {
                        Some(Step::End(end)) => {
                            prop_assert_eq!(Some(end.clone()), expected);
                            ends.push(end);
                            break;
                        }
                        other => prop_assert!(other.is_none() && expected.is_none(), "{:?} for {:?}", other, answer),
                    }
                }
                Step::Wait | Step::Progress(_) => prop_assert!(shown_final.is_none()),
            }
            if first_final.is_none() { first_final = shown_final; }
        }
        prop_assert!(ends.len() <= 1);
    }
}
