use super::*;

use crate::clock::ManualClock;
use crate::events::EventBus;
use crate::jobs::JobRegistry;
use crate::reports::ReportLog;
use tc_api::ids::ServerInstance;
use tc_api::jobs::JobState;

fn verse(reference: &str) -> TrainingVerse {
    let usfm = include_str!("../../../../fixtures/tc4/test-fixtures/en_ult/TIT.usfm");
    tc_usfm::Book::parse(usfm)
        .verses()
        .find_map(|v| TrainingVerse::from_aligned_usfm(reference, v.body()).unwrap())
        .unwrap()
}

fn memory(testament: Testament, verses: &[TrainingVerse]) -> Memory {
    Memory {
        model: Arc::new(Model::train(engine(testament), verses, 0)),
        references: verses.iter().map(|v| v.reference().to_owned()).collect(),
    }
}

fn drain(hub: &SuggestHub, testament: Testament, generation: u64) -> usize {
    let mut retrains = 0;
    while let Some(retrain) = hub.apply_update(testament, generation, &|_| {}) {
        retrains += usize::from(retrain);
    }
    retrains
}

fn seed(hub: &SuggestHub, testament: Testament, verses: &[TrainingVerse]) -> Ticket {
    let (ticket, _) = hub.begin(testament, false, None, || true).unwrap();
    let generation = hub
        .trained(
            testament,
            ticket,
            Ok(memory(testament, verses)),
            CancelToken::default(),
        )
        .unwrap();
    drain(hub, testament, generation);
    ticket
}

#[test]
fn saves_queued_during_initial_training_join_its_snapshot_without_counting_a_resave_twice() {
    let hub = SuggestHub::default();
    let first = verse("TIT 1:1");
    let second = first.relabeled("TIT 1:2");
    let (ticket, _) = hub.begin(Testament::Nt, false, None, || true).unwrap();
    assert!(hub.saved(Testament::Nt, second.clone()).is_none());
    assert!(hub.saved(Testament::Nt, second).is_none());
    assert_eq!(hub.status(Testament::Nt), Some(SuggestStatus::Reading));
    let generation = hub
        .trained(
            Testament::Nt,
            ticket,
            Ok(memory(Testament::Nt, &[first])),
            CancelToken::default(),
        )
        .unwrap();
    drain(&hub, Testament::Nt, generation);
    assert_eq!(hub.model(Testament::Nt).unwrap().verses(), 2);
}

#[test]
fn a_retrain_keeps_saves_that_arrived_after_its_snapshot_and_testaments_stay_separate() {
    let hub = SuggestHub::default();
    let first = verse("TIT 1:1");
    seed(&hub, Testament::Nt, std::slice::from_ref(&first));
    seed(&hub, Testament::Ot, &[]);
    let (ticket, _) = hub.begin(Testament::Nt, true, None, || true).unwrap();
    let generation = hub
        .saved(Testament::Nt, first.relabeled("TIT 1:2"))
        .unwrap();
    drain(&hub, Testament::Nt, generation);
    assert_eq!(
        hub.model(Testament::Nt).unwrap().verses(),
        2,
        "memory updates during the train"
    );
    let generation = hub
        .trained(
            Testament::Nt,
            ticket,
            Ok(memory(Testament::Nt, &[first])),
            CancelToken::default(),
        )
        .unwrap();
    drain(&hub, Testament::Nt, generation);
    assert_eq!(
        hub.model(Testament::Nt).unwrap().verses(),
        2,
        "the old snapshot cannot replace memory"
    );
    assert_eq!(hub.model(Testament::Ot).unwrap().verses(), 0);
}

#[test]
fn only_the_first_crossing_of_a_growth_threshold_requests_a_retrain() {
    let hub = SuggestHub::default();
    let first = verse("TIT 1:1");
    seed(&hub, Testament::Nt, &[]);
    let mut retrains = 0;
    for n in 1..=10 {
        let generation = hub
            .saved(Testament::Nt, first.relabeled(format!("TIT 1:{n}")))
            .unwrap();
        retrains += drain(&hub, Testament::Nt, generation);
    }
    assert_eq!(retrains, 1);
    let generation = hub
        .saved(Testament::Nt, first.relabeled("TIT 1:10"))
        .unwrap();
    assert_eq!(drain(&hub, Testament::Nt, generation), 0);
    assert_eq!(hub.model(Testament::Nt).unwrap().verses(), 10);
}

#[test]
fn superseded_initial_training_cannot_publish_or_discard_queued_saves() {
    let hub = SuggestHub::default();
    let first = verse("TIT 1:1");
    let (old, _) = hub.begin(Testament::Nt, false, None, || true).unwrap();
    hub.saved(Testament::Nt, first.relabeled("TIT 1:3"));
    let (current, _) = hub.begin(Testament::Nt, true, None, || true).unwrap();
    assert!(
        hub.trained(
            Testament::Nt,
            old,
            Ok(memory(Testament::Nt, &[])),
            CancelToken::default(),
        )
        .is_none()
    );
    let generation = hub
        .trained(
            Testament::Nt,
            current,
            Ok(memory(Testament::Nt, &[first])),
            CancelToken::default(),
        )
        .unwrap();
    drain(&hub, Testament::Nt, generation);
    assert_eq!(hub.model(Testament::Nt).unwrap().verses(), 2);
}

#[test]
fn turning_off_after_a_completion_was_queued_prevents_the_worker_from_restoring_it() {
    let hub = SuggestHub::default();
    let (ticket, _) = hub.begin(Testament::Nt, false, None, || true).unwrap();
    let cancel = CancelToken::default();
    assert!(hub.attach_cancel(Testament::Nt, ticket, &cancel));
    let generation = hub
        .trained(
            Testament::Nt,
            ticket,
            Ok(memory(Testament::Nt, &[verse("TIT 1:1")])),
            cancel.clone(),
        )
        .unwrap();
    hub.forget();
    assert!(cancel.is_cancelled());
    let published = std::cell::Cell::new(false);
    assert_eq!(
        hub.apply_update(Testament::Nt, generation, &|_| published.set(true)),
        None
    );
    assert!(!published.get());
    assert_eq!(hub.status(Testament::Nt), None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_a_background_train_after_computation_never_installs_its_model() {
    let hub = Arc::new(SuggestHub::default());
    let jobs = JobRegistry::new(
        EventBus::new(ServerInstance::from_bytes([7; 16])),
        ReportLog::new(),
        Arc::new(ManualClock::new(0)),
    );
    let (ticket, _) = hub.begin(Testament::Nt, false, None, || true).unwrap();
    let (computed, ready) = tokio::sync::oneshot::channel();
    let (release, held) = std::sync::mpsc::channel();
    let done = hub.clone();
    let job = jobs.start_blocking(JobSpec::new(JobKind::Train, Phase::Train), move |ctx| {
        assert!(done.attach_cancel(Testament::Nt, ticket, &ctx.cancel));
        let trained = memory(Testament::Nt, &[verse("TIT 1:1")]);
        computed.send(()).unwrap();
        held.recv().unwrap();
        if let Some(generation) =
            done.trained(Testament::Nt, ticket, Ok(trained), ctx.cancel.clone())
        {
            drain(&done, Testament::Nt, generation);
        }
        ctx.cancel.check()?;
        Ok(JobOutcome::Finished)
    });
    hub.attach(Testament::Nt, ticket, job);
    ready.await.unwrap();
    jobs.cancel(job);
    release.send(()).unwrap();
    let mut state = jobs.watch(job).unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        state.wait_for(JobRegistry::is_ended),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(*state.borrow(), JobState::Cancelled);
    assert!(hub.model(Testament::Nt).is_none());
    assert!(matches!(
        hub.status(Testament::Nt),
        Some(SuggestStatus::Error { .. })
    ));
}

#[test]
fn a_retrain_requested_before_off_cannot_recreate_the_forgotten_slot() {
    let hub = SuggestHub::default();
    let ticket = seed(&hub, Testament::Nt, &[]);
    hub.forget();
    assert!(
        hub.begin(Testament::Nt, true, Some(ticket.slot), || true)
            .is_none()
    );
    assert!(hub.begin(Testament::Nt, false, None, || false).is_none());
    assert_eq!(hub.status(Testament::Nt), None);
}
