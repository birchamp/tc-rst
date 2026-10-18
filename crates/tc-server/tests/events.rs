//! The event stream (APP-DESIGN §5.4): frames as server-sent events, in order, with a
//! heartbeat. The ring, replay and `Resync` are the backend's; a scripted source stands in
//! for it here, so these tests prove what the server does with what the bus yields.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{Got, Sse, SseReader, TestServer};
use tc_api::event::{Event, EventFrame};
use tc_api::ids::ServerInstance;
use tc_server::{Answer, Backend, Bytes, EventSource, Exit, Next};
use tokio::sync::mpsc;

const WAIT: Duration = Duration::from_secs(5);

fn frame(seq: u64, on: bool) -> EventFrame {
    EventFrame {
        seq,
        instance: ServerInstance::from_bytes([7; 16]),
        event: Event::Internet { on },
    }
}

struct Source {
    replay: VecDeque<Next>,
    live: mpsc::UnboundedReceiver<Next>,
    dropped: Arc<AtomicUsize>,
}

impl EventSource for Source {
    async fn next(&mut self) -> Next {
        if let Some(next) = self.replay.pop_front() {
            return next;
        }
        self.live.recv().await.unwrap_or(Next::Closed)
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
    }
}

/// Hands out the prepared sources in order and remembers what each subscription asked for.
#[derive(Default)]
struct Scripted {
    afters: Mutex<Vec<u64>>,
    sources: Mutex<VecDeque<Source>>,
    dropped: Arc<AtomicUsize>,
}

impl Scripted {
    /// A source that yields `replay` and then whatever is sent on the returned sender.
    fn prepare(&self, replay: Vec<Next>) -> mpsc::UnboundedSender<Next> {
        let (sender, live) = mpsc::unbounded_channel();
        self.sources.lock().unwrap().push_back(Source {
            replay: replay.into(),
            live,
            dropped: Arc::clone(&self.dropped),
        });
        sender
    }

    fn afters(&self) -> Vec<u64> {
        self.afters.lock().unwrap().clone()
    }
}

impl Backend for Scripted {
    type Events = Source;

    async fn call(&self, _: &'static str, _: Bytes) -> Answer {
        Answer::NotFound
    }
    fn subscribe(&self, after: u64) -> Source {
        self.afters.lock().unwrap().push(after);
        self.sources.lock().unwrap().pop_front().unwrap_or_else(|| {
            let (_, live) = mpsc::unbounded_channel();
            Source {
                replay: VecDeque::new(),
                live,
                dropped: Arc::clone(&self.dropped),
            }
        })
    }
    async fn quit(&self) {}
    async fn quitting(&self) {
        std::future::pending::<()>().await;
    }
}

fn seqs(reader: &mut SseReader, count: usize) -> Vec<u64> {
    (0..count).map(|_| reader.next_frame(WAIT).0).collect()
}

#[test]
fn frames_are_written_as_server_sent_events() {
    let backend = Arc::new(Scripted::default());
    let live = backend.prepare(vec![]);
    let server = TestServer::start(Arc::clone(&backend));
    let mut reader = SseReader::open(&server, "");
    assert_eq!(reader.status, 200);
    assert_eq!(reader.header("content-type"), Some("text/event-stream"));
    assert_eq!(reader.header("cache-control"), Some("no-store"));
    assert_eq!(reader.next(WAIT), Sse::Comment("tc4".to_owned()));
    for seq in 1..=3 {
        live.send(Next::Frame(frame(seq, seq % 2 == 1))).unwrap();
    }
    for seq in 1..=3 {
        let (id, got) = reader.next_frame(WAIT);
        assert_eq!(id, seq, "the id: line is the frame's sequence");
        assert_eq!(got, frame(seq, seq % 2 == 1));
    }
}

#[test]
fn after_is_handed_to_the_backend() {
    let backend = Arc::new(Scripted::default());
    let server = TestServer::start(Arc::clone(&backend));
    let _a = SseReader::open(&server, "?after=7");
    let _b = SseReader::open(&server, "");
    let _c = SseReader::open(&server, "?after=0");
    let _d = SseReader::open(&server, "?after=18446744073709551615");
    assert_eq!(backend.afters(), vec![7, 0, 0, u64::MAX]);
}

#[test]
fn a_bad_after_is_a_400_and_reaches_no_backend() {
    let backend = Arc::new(Scripted::default());
    let server = TestServer::start(Arc::clone(&backend));
    for query in [
        "?after=x",
        "?after=-1",
        "?after=1.5",
        "?after=18446744073709551616",
    ] {
        let response = server.send(
            &server
                .authed("GET", &format!("/api/v1/events{query}"))
                .with("X-Tc-Api", "1"),
        );
        assert_eq!(response.status, 400, "{query}");
    }
    assert!(backend.afters().is_empty());
}

#[test]
fn replay_then_live_reaches_the_reader_in_order() {
    let backend = Arc::new(Scripted::default());
    let live = backend.prepare(vec![
        Next::Frame(frame(5, true)),
        Next::Frame(frame(6, false)),
        Next::Frame(frame(7, true)),
    ]);
    let server = TestServer::start(Arc::clone(&backend));
    let mut reader = SseReader::open(&server, "?after=4");
    assert_eq!(seqs(&mut reader, 3), [5, 6, 7], "the replay");
    live.send(Next::Frame(frame(8, false))).unwrap();
    live.send(Next::Frame(frame(9, true))).unwrap();
    assert_eq!(seqs(&mut reader, 2), [8, 9], "then what is live");
    assert_eq!(backend.afters(), vec![4]);
}

#[test]
fn a_resync_frame_reaches_the_reader() {
    let backend = Arc::new(Scripted::default());
    let resync = EventFrame {
        seq: 4096,
        instance: ServerInstance::from_bytes([7; 16]),
        event: Event::Resync,
    };
    let live = backend.prepare(vec![Next::Frame(resync.clone())]);
    let server = TestServer::start(Arc::clone(&backend));
    let mut reader = SseReader::open(&server, "?after=1");
    let (id, first) = reader.next_frame(WAIT);
    assert_eq!((id, first), (4096, resync), "Resync comes first");
    live.send(Next::Frame(frame(4097, true))).unwrap();
    assert_eq!(reader.next_frame(WAIT).0, 4097);
}

#[test]
fn a_lagging_subscription_ends_the_stream_and_a_reconnect_resumes() {
    let backend = Arc::new(Scripted::default());
    let first = backend.prepare(vec![]);
    let second = backend.prepare(vec![Next::Frame(frame(3, true))]);
    let server = TestServer::start(Arc::clone(&backend));
    let mut reader = SseReader::open(&server, "");
    first.send(Next::Frame(frame(1, true))).unwrap();
    first.send(Next::Frame(frame(2, false))).unwrap();
    first.send(Next::Lagged).unwrap();
    // A frame that follows the lag is never written to this connection. The server may have
    // dropped the source by now, so the send may fail.
    let _ = first.send(Next::Frame(frame(9, true)));
    assert_eq!(seqs(&mut reader, 2), [1, 2]);
    loop {
        match reader.read(WAIT) {
            Got::End => break,
            Got::Item(Sse::Comment(_)) => {}
            Got::Item(other) => panic!("a frame after the lag: {other:?}"),
            Got::TimedOut => panic!("the stream did not end"),
        }
    }
    // The UI reconnects with the last sequence it read.
    let mut again = SseReader::open(&server, "?after=2");
    assert_eq!(again.next_frame(WAIT).0, 3);
    drop(second);
    assert_eq!(backend.afters(), vec![0, 2]);
}

#[test]
fn a_closed_source_ends_the_stream() {
    let backend = Arc::new(Scripted::default());
    let live = backend.prepare(vec![]);
    let server = TestServer::start(Arc::clone(&backend));
    let mut reader = SseReader::open(&server, "");
    live.send(Next::Closed).unwrap();
    loop {
        match reader.read(WAIT) {
            Got::End => break,
            Got::Item(_) => {}
            Got::TimedOut => panic!("the stream did not end"),
        }
    }
}

#[test]
fn the_stream_starts_with_a_comment_and_heartbeats_while_idle() {
    let backend = Arc::new(Scripted::default());
    let _live = backend.prepare(vec![]);
    let server = TestServer::start_with(
        Arc::clone(&backend),
        |config| config.limits.heartbeat = Duration::from_millis(100),
        true,
    );
    let mut reader = SseReader::open(&server, "");
    assert_eq!(reader.next(WAIT), Sse::Comment("tc4".to_owned()));
    let until = std::time::Instant::now() + Duration::from_millis(450);
    let mut beats = 0;
    while let Got::Item(item) =
        reader.read(until.saturating_duration_since(std::time::Instant::now()))
    {
        match item {
            Sse::Comment(text) if text.is_empty() => beats += 1,
            other => panic!("expected only heartbeats, got {other:?}"),
        }
    }
    assert!(
        (3..=5).contains(&beats),
        "{beats} heartbeats in 450 ms at 100 ms"
    );
}

#[test]
fn a_frame_resets_nothing_but_the_stream_keeps_beating_after_it() {
    let backend = Arc::new(Scripted::default());
    let live = backend.prepare(vec![]);
    let server = TestServer::start_with(
        Arc::clone(&backend),
        |config| config.limits.heartbeat = Duration::from_millis(100),
        true,
    );
    let mut reader = SseReader::open(&server, "");
    reader.next(WAIT);
    live.send(Next::Frame(frame(1, true))).unwrap();
    assert_eq!(reader.next_frame(WAIT).0, 1);
    assert_eq!(reader.next(WAIT), Sse::Comment(String::new()));
}

#[test]
fn a_stream_the_client_closed_drops_its_subscription() {
    let backend = Arc::new(Scripted::default());
    let _live = backend.prepare(vec![]);
    let server = TestServer::start_with(
        Arc::clone(&backend),
        |config| config.limits.heartbeat = Duration::from_millis(50),
        true,
    );
    let mut reader = SseReader::open(&server, "");
    reader.next(WAIT);
    drop(reader);
    let until = std::time::Instant::now() + WAIT;
    while backend.dropped.load(Ordering::SeqCst) == 0 {
        assert!(
            std::time::Instant::now() < until,
            "the subscription was kept"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn streams_end_when_the_server_quits() {
    let backend = Arc::new(Scripted::default());
    let _a = backend.prepare(vec![]);
    let _b = backend.prepare(vec![]);
    let mut server = TestServer::start(Arc::clone(&backend));
    let mut one = SseReader::open(&server, "");
    let mut two = SseReader::open(&server, "?after=3");
    one.next(WAIT);
    two.next(WAIT);
    server.quit_message();
    for reader in [&mut one, &mut two] {
        loop {
            match reader.read(WAIT) {
                Got::End => break,
                Got::Item(_) => {}
                Got::TimedOut => panic!("a stream outlived the quit"),
            }
        }
    }
    assert_eq!(server.join(), Exit::QuitMessage);
    assert_eq!(backend.dropped.load(Ordering::SeqCst), 2);
}
