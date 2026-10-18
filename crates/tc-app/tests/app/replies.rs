//! The reply cache: a retry gets the first reply, a retry of a request in flight waits for it,
//! another body is refused, and a write finishes when its caller goes away.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use proptest::prelude::*;
use tc_api::ApiError;
use tc_api::endpoint::NetKind;
use tc_api::ids::RequestId;
use tc_app::replies::{BodyDigest, CacheAnswer, Outcome, ReplyCache};
use tokio::sync::Notify;

fn id(n: u8) -> RequestId {
    RequestId::from_bytes([n; 16])
}

fn digest(body: &str) -> BodyDigest {
    BodyDigest::of("editor/saveVerse", body.as_bytes())
}

fn bytes(outcome: &Outcome) -> Vec<u8> {
    match outcome {
        Outcome::Reply(bytes) => bytes.to_vec(),
        #[allow(unreachable_patterns)]
        other => panic!("{other:?}"),
    }
}

fn reply(text: &'static str) -> Outcome {
    Outcome::Reply(Arc::from(text.as_bytes()))
}

/// The answer's outcome and whether it came from a first attempt.
fn answered(answer: CacheAnswer) -> (Vec<u8>, bool) {
    match answer {
        CacheAnswer::Fresh(outcome) => (bytes(&outcome), true),
        CacheAnswer::Repeat(outcome) => (bytes(&outcome), false),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_digest_covers_the_path_and_the_body_apart() {
    assert_eq!(digest("x"), digest("x"));
    assert_ne!(digest("x"), digest("y"));
    assert_ne!(
        BodyDigest::of("a/b", b"x"),
        BodyDigest::of("a/c", b"x"),
        "an id reused for another endpoint differs"
    );
    assert_ne!(BodyDigest::of("a", b"bc"), BodyDigest::of("ab", b"c"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retry_of_a_finished_request_gets_the_first_reply() {
    let cache = ReplyCache::new(ReplyCache::FINISHED);
    let runs = Arc::new(AtomicUsize::new(0));
    let first = {
        let runs = Arc::clone(&runs);
        cache
            .run(id(1), digest("a"), None, async move {
                runs.fetch_add(1, Ordering::SeqCst);
                reply("A")
            })
            .await
    };
    assert_eq!(answered(first), (b"A".to_vec(), true));
    let second = {
        let runs = Arc::clone(&runs);
        cache
            .run(id(1), digest("a"), None, async move {
                runs.fetch_add(1, Ordering::SeqCst);
                reply("B")
            })
            .await
    };
    assert_eq!(answered(second), (b"A".to_vec(), false));
    assert_eq!(runs.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retry_while_the_first_attempt_runs_waits_and_runs_once() {
    let cache = ReplyCache::new(ReplyCache::FINISHED);
    let runs = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(Notify::new());
    let started = Arc::new(Notify::new());
    let first = {
        let (cache, runs, release, started) = (
            Arc::clone(&cache),
            Arc::clone(&runs),
            Arc::clone(&release),
            Arc::clone(&started),
        );
        tokio::spawn(async move {
            cache
                .run(id(1), digest("a"), None, async move {
                    runs.fetch_add(1, Ordering::SeqCst);
                    started.notify_one();
                    release.notified().await;
                    reply("A")
                })
                .await
        })
    };
    started.notified().await;
    let retry = {
        let (cache, runs) = (Arc::clone(&cache), Arc::clone(&runs));
        tokio::spawn(async move {
            cache
                .run(id(1), digest("a"), None, async move {
                    runs.fetch_add(1, Ordering::SeqCst);
                    reply("B")
                })
                .await
        })
    };
    // The retry does not return while the first attempt runs.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!retry.is_finished());
    assert_eq!(cache.running(), 1);
    release.notify_one();
    assert_eq!(answered(first.await.unwrap()), (b"A".to_vec(), true));
    assert_eq!(answered(retry.await.unwrap()), (b"A".to_vec(), false));
    assert_eq!(runs.load(Ordering::SeqCst), 1);
    assert_eq!(cache.running(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retry_with_another_body_is_request_reused() {
    let cache = ReplyCache::new(ReplyCache::FINISHED);
    let runs = Arc::new(AtomicUsize::new(0));

    // Finished.
    let done = cache
        .run(id(1), digest("a"), None, async { reply("A") })
        .await;
    answered(done);
    let counted = Arc::clone(&runs);
    let reused = cache
        .run(id(1), digest("other"), None, async move {
            counted.fetch_add(1, Ordering::SeqCst);
            reply("B")
        })
        .await;
    assert!(matches!(reused, CacheAnswer::Reused), "{reused:?}");

    // Running.
    let release = Arc::new(Notify::new());
    let started = Arc::new(Notify::new());
    let first = {
        let (cache, release, started) = (
            Arc::clone(&cache),
            Arc::clone(&release),
            Arc::clone(&started),
        );
        tokio::spawn(async move {
            cache
                .run(id(2), digest("a"), None, async move {
                    started.notify_one();
                    release.notified().await;
                    reply("A2")
                })
                .await
        })
    };
    started.notified().await;
    let counted = Arc::clone(&runs);
    let reused = cache
        .run(id(2), digest("other"), None, async move {
            counted.fetch_add(1, Ordering::SeqCst);
            reply("B")
        })
        .await;
    assert!(matches!(reused, CacheAnswer::Reused), "{reused:?}");
    release.notify_one();
    assert_eq!(answered(first.await.unwrap()), (b"A2".to_vec(), true));
    assert_eq!(runs.load(Ordering::SeqCst), 0, "the second work never ran");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_id_reused_for_another_endpoint_is_request_reused() {
    let cache = ReplyCache::new(ReplyCache::FINISHED);
    let first = cache
        .run(id(1), BodyDigest::of("net/set", b"{}"), None, async {
            reply("A")
        })
        .await;
    answered(first);
    let other = cache
        .run(
            id(1),
            BodyDigest::of("prefs/setLocale", b"{}"),
            None,
            async { reply("B") },
        )
        .await;
    assert!(matches!(other, CacheAnswer::Reused));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_write_finishes_when_its_caller_goes_away() {
    let cache = ReplyCache::new(ReplyCache::FINISHED);
    let release = Arc::new(Notify::new());
    let started = Arc::new(Notify::new());
    let stored = Arc::new(AtomicUsize::new(0));
    let caller = {
        let (cache, release, started, stored) = (
            Arc::clone(&cache),
            Arc::clone(&release),
            Arc::clone(&started),
            Arc::clone(&stored),
        );
        tokio::spawn(async move {
            cache
                .run(id(1), digest("a"), None, async move {
                    started.notify_one();
                    release.notified().await;
                    stored.fetch_add(1, Ordering::SeqCst);
                    reply("A")
                })
                .await
        })
    };
    started.notified().await;
    // The page reloads: the caller's future is dropped while the write waits.
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    assert_eq!(stored.load(Ordering::SeqCst), 0);
    release.notify_one();
    for _ in 0..500 {
        if stored.load(Ordering::SeqCst) == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert_eq!(stored.load(Ordering::SeqCst), 1, "the write still finished");
    // The retry gets the reply of the write that finished.
    let retry = cache
        .run(id(1), digest("a"), None, async { reply("B") })
        .await;
    assert_eq!(answered(retry), (b"A".to_vec(), false));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn finished_replies_are_bounded_and_running_ones_never_evicted() {
    let cache = ReplyCache::new(4);
    let release = Arc::new(Notify::new());
    let started = Arc::new(Notify::new());
    let held = {
        let (cache, release, started) = (
            Arc::clone(&cache),
            Arc::clone(&release),
            Arc::clone(&started),
        );
        tokio::spawn(async move {
            cache
                .run(id(200), digest("held"), None, async move {
                    started.notify_one();
                    release.notified().await;
                    reply("HELD")
                })
                .await
        })
    };
    started.notified().await;
    for n in 1..=10 {
        let done = cache
            .run(id(n), digest("a"), None, async { reply("done") })
            .await;
        answered(done);
    }
    // Only the last four finished replies are kept; an older id runs again.
    for n in 1..=6 {
        let again = cache
            .run(id(n), digest("a"), None, async { reply("rerun") })
            .await;
        assert_eq!(answered(again), (b"rerun".to_vec(), true), "id {n}");
    }
    // That pushed the older ones out, but the running one is still there.
    let retry = {
        let cache = Arc::clone(&cache);
        tokio::spawn(async move {
            cache
                .run(id(200), digest("held"), None, async { reply("X") })
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!retry.is_finished());
    release.notify_one();
    assert_eq!(answered(held.await.unwrap()), (b"HELD".to_vec(), true));
    assert_eq!(answered(retry.await.unwrap()), (b"HELD".to_vec(), false));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_last_four_finished_ids_stay_repeatable() {
    let cache = ReplyCache::new(4);
    for n in 1..=10 {
        let done = cache
            .run(id(n), digest("a"), None, async { reply("done") })
            .await;
        answered(done);
    }
    for n in 7..=10 {
        let again = cache
            .run(id(n), digest("a"), None, async { reply("rerun") })
            .await;
        assert_eq!(answered(again), (b"done".to_vec(), false), "id {n}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_work_that_vanishes_is_lost_and_never_runs_again() {
    let cache = ReplyCache::new(ReplyCache::FINISHED);
    let runs = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&runs);
    let first = cache
        .run(id(1), digest("a"), None, async move {
            counted.fetch_add(1, Ordering::SeqCst);
            panic!("the work stopped");
        })
        .await;
    assert!(matches!(first, CacheAnswer::Lost), "{first:?}");
    let counted = Arc::clone(&runs);
    let again = cache
        .run(id(1), digest("a"), None, async move {
            counted.fetch_add(1, Ordering::SeqCst);
            reply("B")
        })
        .await;
    assert!(matches!(again, CacheAnswer::Lost), "{again:?}");
    assert_eq!(runs.load(Ordering::SeqCst), 1);
}

/// `test/quiet` counts the writes still running. A write whose task ended without an outcome
/// is not running, or every capture's settle would wait for it until the process ends.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_write_is_not_counted_as_running() {
    let cache = ReplyCache::new(ReplyCache::FINISHED);
    let runs = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&runs);
    let first = cache
        .run(id(1), digest("a"), None, async move {
            counted.fetch_add(1, Ordering::SeqCst);
            panic!("the work stopped");
        })
        .await;
    assert!(matches!(first, CacheAnswer::Lost), "{first:?}");
    assert_eq!(cache.running(), 0);

    let counted = Arc::clone(&runs);
    let again = cache
        .run(id(1), digest("a"), None, async move {
            counted.fetch_add(1, Ordering::SeqCst);
            reply("B")
        })
        .await;
    assert!(
        matches!(again, CacheAnswer::Lost),
        "a retry still gets Lost: {again:?}"
    );
    assert_eq!(runs.load(Ordering::SeqCst), 1, "the work did not run again");
    assert_eq!(cache.running(), 0);
}

/// The other side of the same rule: a write that is really running still counts, and stops
/// counting when it ends.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_running_write_is_counted_until_it_ends() {
    let cache = ReplyCache::new(ReplyCache::FINISHED);
    let gate = Arc::new(Notify::new());
    let held = Arc::clone(&gate);
    let waiting = {
        let cache = Arc::clone(&cache);
        tokio::spawn(async move {
            cache
                .run(id(1), digest("a"), None, async move {
                    held.notified().await;
                    reply("done")
                })
                .await
        })
    };
    for _ in 0..2000 {
        if cache.running() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert_eq!(cache.running(), 1);
    gate.notify_one();
    let answer = waiting.await.unwrap();
    assert_eq!(answered(answer), (b"done".to_vec(), true));
    assert_eq!(cache.running(), 0);
}

fn offline() -> Option<ApiError> {
    Some(ApiError::NeedsInternet(NetKind::SignIn))
}

/// `NeedsInternet` says nothing was sent. For an id the cache has not seen that is true, and
/// nothing is kept: the UI turns the switch on and sends the same request again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refusal_of_a_new_id_runs_nothing_and_keeps_nothing() {
    let cache = ReplyCache::new(ReplyCache::FINISHED);
    let runs = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&runs);
    let refused = cache
        .run(id(1), digest("a"), offline(), async move {
            counted.fetch_add(1, Ordering::SeqCst);
            reply("never")
        })
        .await;
    assert!(
        matches!(
            &refused,
            CacheAnswer::Refused(ApiError::NeedsInternet(NetKind::SignIn))
        ),
        "{refused:?}"
    );
    assert_eq!(runs.load(Ordering::SeqCst), 0);
    assert_eq!(cache.running(), 0);

    // The same id and body again, now allowed: it runs, as the first attempt.
    let counted = Arc::clone(&runs);
    let again = cache
        .run(id(1), digest("a"), None, async move {
            counted.fetch_add(1, Ordering::SeqCst);
            reply("A")
        })
        .await;
    assert_eq!(answered(again), (b"A".to_vec(), true));
    assert_eq!(runs.load(Ordering::SeqCst), 1);
}

/// An id the cache knows was sent, so its retry gets its own answer whatever the refusal
/// says now: finished, still running, or used for another request.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refusal_never_applies_to_an_id_that_was_sent() {
    let cache = ReplyCache::new(ReplyCache::FINISHED);
    let done = cache
        .run(id(1), digest("a"), None, async { reply("A") })
        .await;
    assert_eq!(answered(done), (b"A".to_vec(), true));
    let retry = cache
        .run(id(1), digest("a"), offline(), async { reply("rerun") })
        .await;
    assert_eq!(answered(retry), (b"A".to_vec(), false), "a finished write");

    let other = cache
        .run(id(1), digest("other"), offline(), async { reply("rerun") })
        .await;
    assert!(matches!(other, CacheAnswer::Reused), "{other:?}");

    let gate = Arc::new(Notify::new());
    let held = Arc::clone(&gate);
    let first = {
        let cache = Arc::clone(&cache);
        tokio::spawn(async move {
            cache
                .run(id(2), digest("b"), None, async move {
                    held.notified().await;
                    reply("B")
                })
                .await
        })
    };
    for _ in 0..2000 {
        if cache.running() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    let waiting = {
        let cache = Arc::clone(&cache);
        tokio::spawn(async move {
            cache
                .run(id(2), digest("b"), offline(), async { reply("rerun") })
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(20)).await;
    gate.notify_one();
    assert_eq!(answered(first.await.unwrap()), (b"B".to_vec(), true));
    assert_eq!(
        answered(waiting.await.unwrap()),
        (b"B".to_vec(), false),
        "a write in flight"
    );
}

/// `test/reset` gives the next scenario the empty cache of a new launch, but a write that is
/// still running stays, or its retry would run the write a second time.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn forget_settled_forgets_finished_and_lost_writes_and_keeps_running_ones() {
    let cache = ReplyCache::new(ReplyCache::FINISHED);
    let done = cache
        .run(id(1), digest("a"), None, async { reply("A") })
        .await;
    assert_eq!(answered(done), (b"A".to_vec(), true));
    let lost = cache
        .run(id(2), digest("a"), None, async {
            panic!("the work stopped")
        })
        .await;
    assert!(matches!(lost, CacheAnswer::Lost), "{lost:?}");

    let runs = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(Notify::new());
    let first = {
        let cache = Arc::clone(&cache);
        let held = Arc::clone(&gate);
        let counted = Arc::clone(&runs);
        tokio::spawn(async move {
            cache
                .run(id(3), digest("c"), None, async move {
                    counted.fetch_add(1, Ordering::SeqCst);
                    held.notified().await;
                    reply("C")
                })
                .await
        })
    };
    for _ in 0..2000 {
        if cache.running() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    cache.forget_settled();

    // The ids of the finished and the lost write are free, and another body is not "reused".
    let again = cache
        .run(id(1), digest("other"), None, async { reply("A2") })
        .await;
    assert_eq!(answered(again), (b"A2".to_vec(), true));
    let again = cache
        .run(id(2), digest("a"), None, async { reply("L2") })
        .await;
    assert_eq!(answered(again), (b"L2".to_vec(), true));

    // The running write is still the one answer for its id.
    let counted = Arc::clone(&runs);
    let retry = {
        let cache = Arc::clone(&cache);
        tokio::spawn(async move {
            cache
                .run(id(3), digest("c"), None, async move {
                    counted.fetch_add(1, Ordering::SeqCst);
                    reply("again")
                })
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(20)).await;
    gate.notify_one();
    assert_eq!(answered(first.await.unwrap()), (b"C".to_vec(), true));
    assert_eq!(answered(retry.await.unwrap()), (b"C".to_vec(), false));
    assert_eq!(runs.load(Ordering::SeqCst), 1, "the running write ran once");
}

#[derive(Debug, Clone)]
struct Call {
    id: u8,
    body: u8,
    yields: u8,
}

fn calls() -> impl Strategy<Value = Vec<Call>> {
    proptest::collection::vec(
        (0u8..4, 0u8..3, 0u8..4).prop_map(|(id, body, yields)| Call { id, body, yields }),
        1..40,
    )
}

proptest! {
    /// However the calls interleave, one body wins each id; the work runs exactly once for it,
    /// every call with that body gets the same bytes, and every call with another body is
    /// refused.
    #[test]
    fn each_request_runs_once(calls in calls()) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_time()
            .build()
            .unwrap();
        let results = runtime.block_on(async {
            let cache = ReplyCache::new(ReplyCache::FINISHED);
            let runs: Arc<std::sync::Mutex<BTreeMap<u8, usize>>> = Arc::default();
            let mut handles = Vec::new();
            for call in calls.clone() {
                let cache = Arc::clone(&cache);
                let runs = Arc::clone(&runs);
                handles.push(tokio::spawn(async move {
                    for _ in 0..call.yields {
                        tokio::task::yield_now().await;
                    }
                    let answer = cache
                        .run(id(call.id), digest(&call.body.to_string()), None, async move {
                            *runs.lock().unwrap().entry(call.id).or_default() += 1;
                            tokio::task::yield_now().await;
                            Outcome::Reply(Arc::from(vec![call.id, call.body]))
                        })
                        .await;
                    (call, answer)
                }));
            }
            let mut answers = Vec::new();
            for handle in handles {
                answers.push(handle.await.unwrap());
            }
            let runs = runs.lock().unwrap().clone();
            (answers, runs)
        });
        let (answers, runs) = results;
        let mut winners: BTreeMap<u8, BTreeSet<u8>> = BTreeMap::new();
        let mut fresh: BTreeMap<u8, usize> = BTreeMap::new();
        for (call, answer) in &answers {
            match answer {
                CacheAnswer::Fresh(outcome) | CacheAnswer::Repeat(outcome) => {
                    prop_assert_eq!(bytes(outcome), vec![call.id, call.body]);
                    winners.entry(call.id).or_default().insert(call.body);
                    if matches!(answer, CacheAnswer::Fresh(_)) {
                        *fresh.entry(call.id).or_default() += 1;
                    }
                }
                CacheAnswer::Reused => {}
                CacheAnswer::Lost => prop_assert!(false, "a work was lost"),
                CacheAnswer::Refused(_) => prop_assert!(false, "no call asked to be refused"),
            }
        }
        for (call, answer) in &answers {
            if matches!(answer, CacheAnswer::Reused) {
                // It was refused because another body won the id.
                let winner = winners.get(&call.id).unwrap();
                prop_assert!(!winner.contains(&call.body));
            }
        }
        for (call_id, bodies) in &winners {
            prop_assert_eq!(bodies.len(), 1, "two bodies won id {}", call_id);
            prop_assert_eq!(fresh.get(call_id), Some(&1));
            prop_assert_eq!(runs.get(call_id), Some(&1));
        }
        prop_assert_eq!(runs.len(), winners.len());
    }
}
