//! The event bus: numbering, the ring, replay on reconnect and `Resync`.

use std::collections::BTreeSet;

use proptest::prelude::*;
use tc_api::event::{Event, EventFrame};
use tc_api::jobs::JobState;
use tc_app::events::{EventBus, Next, RING_FRAMES, Subscription};

use super::common::{instance, job_event, progress, running};

fn bus() -> EventBus {
    EventBus::new(instance())
}

fn drain(subscription: &mut Subscription) -> Vec<EventFrame> {
    let mut frames = Vec::new();
    while let Some(next) = subscription.try_next() {
        match next {
            Next::Frame(frame) => frames.push(frame),
            other => panic!("unexpected {other:?}"),
        }
    }
    frames
}

fn seqs(frames: &[EventFrame]) -> Vec<u64> {
    frames.iter().map(|frame| frame.seq).collect()
}

#[test]
fn frames_are_numbered_from_one_and_carry_the_instance() {
    let bus = bus();
    assert_eq!(bus.head(), 0);
    let mut reader = bus.subscribe(0);
    assert_eq!(bus.publish(Event::Internet { on: true }), 1);
    assert_eq!(bus.publish(Event::Internet { on: false }), 2);
    assert_eq!(bus.head(), 2);
    let frames = drain(&mut reader);
    assert_eq!(seqs(&frames), [1, 2]);
    assert!(frames.iter().all(|frame| frame.instance == instance()));
    assert_eq!(frames[0].event, Event::Internet { on: true });
    assert_eq!(bus.instance(), instance());
}

#[test]
fn reconnect_replays_the_frames_after_its_last_seq() {
    let bus = bus();
    for _ in 0..5 {
        bus.publish(Event::ProjectsChanged);
    }
    let mut reader = bus.subscribe(2);
    assert_eq!(seqs(&drain(&mut reader)), [3, 4, 5]);
    bus.publish(Event::ProjectsChanged);
    assert_eq!(seqs(&drain(&mut reader)), [6]);
}

#[test]
fn a_reader_that_holds_the_newest_frame_gets_only_later_ones() {
    let bus = bus();
    bus.publish(Event::ProjectsChanged);
    let mut reader = bus.subscribe(1);
    assert!(drain(&mut reader).is_empty());
    bus.publish(Event::ProjectsChanged);
    assert_eq!(seqs(&drain(&mut reader)), [2]);
}

#[test]
fn reconnect_older_than_the_ring_gets_resync() {
    let bus = bus();
    let total = RING_FRAMES as u64 + 10;
    for _ in 0..total {
        bus.publish(Event::ProjectsChanged);
    }
    let mut reader = bus.subscribe(1);
    let frames = drain(&mut reader);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].event, Event::Resync);
    assert_eq!(frames[0].seq, total);
    assert_eq!(frames[0].instance, instance());
    // Then live frames only.
    bus.publish(Event::Internet { on: true });
    assert_eq!(seqs(&drain(&mut reader)), [total + 1]);
}

#[test]
fn a_reader_exactly_at_the_ring_edge_is_served_without_resync() {
    let bus = bus();
    let total = RING_FRAMES as u64 + 10;
    for _ in 0..total {
        bus.publish(Event::ProjectsChanged);
    }
    // Frames 1..=10 were dropped; a reader that holds frame 10 needs 11.. and has them all.
    let mut at_edge = bus.subscribe(10);
    let frames = drain(&mut at_edge);
    assert_eq!(frames.len(), RING_FRAMES);
    assert_eq!(frames[0].seq, 11);
    assert!(frames.iter().all(|f| f.event == Event::ProjectsChanged));
    // One behind the edge misses frame 10.
    let mut behind = bus.subscribe(9);
    assert_eq!(drain(&mut behind)[0].event, Event::Resync);
}

#[test]
fn reconnect_from_another_run_gets_resync() {
    let bus = bus();
    for _ in 0..3 {
        bus.publish(Event::ProjectsChanged);
    }
    let mut reader = bus.subscribe(bus.head() + 50);
    let frames = drain(&mut reader);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].event, Event::Resync);
    assert_eq!(frames[0].seq, 3);
    // The numbering goes on from this run's head.
    bus.publish(Event::ProjectsChanged);
    assert_eq!(seqs(&drain(&mut reader)), [4]);
}

#[test]
fn progress_frames_are_coalesced_in_the_ring() {
    let bus = bus();
    let mut early = bus.subscribe(0);
    bus.publish(running(1, 10));
    bus.publish(running(1, 20));
    bus.publish(running(2, 5));
    bus.publish(running(1, 30));
    // A live reader gets all four.
    assert_eq!(seqs(&drain(&mut early)), [1, 2, 3, 4]);
    // A reconnect gets the newest of each job.
    let mut late = bus.subscribe(0);
    let frames = drain(&mut late);
    assert_eq!(seqs(&frames), [3, 4]);
    assert_eq!(frames[0].event, running(2, 5));
    assert_eq!(frames[1].event, running(1, 30));
}

#[test]
fn a_finished_job_replays_as_its_end_only() {
    let bus = bus();
    bus.publish(running(1, 10));
    bus.publish(running(1, 20));
    bus.publish(job_event(
        1,
        JobState::Done(tc_api::jobs::JobOutcome::Finished),
    ));
    let mut late = bus.subscribe(0);
    let frames = drain(&mut late);
    assert_eq!(seqs(&frames), [3]);
    assert!(matches!(
        &frames[0].event,
        Event::Job(view) if matches!(view.state, JobState::Done(_))
    ));
}

#[test]
fn state_frames_other_than_progress_are_never_coalesced() {
    let bus = bus();
    bus.publish(job_event(1, JobState::Queued));
    bus.publish(running(1, 10));
    bus.publish(job_event(1, JobState::Cancelling));
    bus.publish(job_event(1, JobState::Cancelled));
    let mut late = bus.subscribe(0);
    // Queued is a state of the job and stays; the progress frame went when Cancelling came.
    assert_eq!(seqs(&drain(&mut late)), [1, 3, 4]);
}

#[test]
fn a_slow_reader_is_told_it_lagged() {
    let bus = bus();
    let mut reader = bus.subscribe(0);
    for _ in 0..=RING_FRAMES {
        bus.publish(Event::ProjectsChanged);
    }
    assert_eq!(reader.try_next(), Some(Next::Lagged));
}

#[test]
fn many_progress_frames_cost_the_ring_one_slot_per_job() {
    let bus = bus();
    // 3 jobs reporting 10,000 times each: tombstones count against the ring, but a
    // reconnect still gets only the newest of each.
    for round in 0..10_000u64 {
        for job in 1..=3 {
            bus.publish(running(job, round));
        }
    }
    let mut late = bus.subscribe(bus.head() - 100);
    let frames = drain(&mut late);
    assert_eq!(frames.len(), 3);
    for frame in &frames {
        assert!(matches!(
            &frame.event,
            Event::Job(view) if view.state == JobState::Running(progress(9_999))
        ));
    }
}

#[tokio::test]
async fn a_waiting_reader_gets_the_next_published_frame() {
    let bus = bus();
    let mut reader = bus.subscribe(0);
    let publisher = bus.clone();
    let handle = tokio::spawn(async move {
        publisher.publish(Event::Internet { on: true });
    });
    match reader.next().await {
        Next::Frame(frame) => assert_eq!(frame.event, Event::Internet { on: true }),
        other => panic!("{other:?}"),
    }
    handle.await.unwrap();
}

#[derive(Debug, Clone)]
enum Op {
    Plain,
    Running(u8),
    Finished(u8),
    Subscribe(u16),
}

fn ops() -> impl Strategy<Value = Vec<Op>> {
    proptest::collection::vec(
        prop_oneof![
            3 => Just(Op::Plain),
            5 => (0u8..3).prop_map(Op::Running),
            2 => (0u8..3).prop_map(Op::Finished),
            3 => any::<u16>().prop_map(Op::Subscribe),
        ],
        1..80,
    )
}

proptest! {
    /// For every reader, the numbers strictly increase, and every frame after the number it
    /// asked for arrives, except a progress frame that a newer frame of its job had already
    /// replaced when it subscribed.
    #[test]
    fn replay_then_live_never_skips_or_repeats(ops in ops()) {
        let bus = bus();
        // (seq, job, is progress) of every frame published so far.
        let mut published: Vec<(u64, Option<u8>, bool)> = Vec::new();
        let mut readers: Vec<(u64, Subscription, BTreeSet<u64>)> = Vec::new();
        for op in ops {
            let (event, job, progress_frame) = match op {
                Op::Plain => (Event::ProjectsChanged, None, false),
                Op::Running(job) => (running(u64::from(job), 1), Some(job), true),
                Op::Finished(job) => (
                    job_event(u64::from(job), JobState::Done(tc_api::jobs::JobOutcome::Finished)),
                    Some(job),
                    false,
                ),
                Op::Subscribe(pick) => {
                    let head = published.len() as u64;
                    let after = u64::from(pick) % (head + 1);
                    // Progress frames already replaced by a newer frame of their job.
                    let replaced: BTreeSet<u64> = published
                        .iter()
                        .enumerate()
                        .filter(|(_, (_, job, progress))| job.is_some() && *progress)
                        .filter(|(index, (_, job, _))| {
                            published[index + 1..].iter().any(|(_, later, _)| later == job)
                        })
                        .map(|(_, (seq, _, _))| *seq)
                        .collect();
                    readers.push((after, bus.subscribe(after), replaced));
                    continue;
                }
            };
            let seq = bus.publish(event);
            prop_assert_eq!(seq, published.len() as u64 + 1);
            published.push((seq, job, progress_frame));
        }
        let head = published.len() as u64;
        for (after, mut reader, replaced) in readers {
            let got = seqs(&drain(&mut reader));
            prop_assert!(got.windows(2).all(|w| w[0] < w[1]), "not increasing: {:?}", got);
            let expected: Vec<u64> = ((after + 1)..=head)
                .filter(|seq| !replaced.contains(seq))
                .collect();
            prop_assert_eq!(got, expected);
        }
    }
}
