//! `tokio_never_blocked` (APP-DESIGN §8.3): under a paced load of API calls, event streams, an
//! upload, static files and a download, no wake of the runtime is more than 50 ms late. Its
//! own test binary, because it reads timings that parallel tests would spoil.
//!
//! The server runs on one worker thread, where any blocking call shows: with several, work
//! stealing hides a blocked worker. The load is paced, not unbounded: a saturated single
//! worker polls its timers only every 61 tasks and looks blocked with no blocking call in it,
//! and this test is after blocking calls.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use std::io::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::{Got, SseReader, TestServer, read_response};
use tc_api::event::{Event, EventFrame};
use tc_api::ids::ServerInstance;
use tc_api::upload::UploadFlow;
use tc_server::stub::Stub;
use tc_server::watchdog::Watchdog;
use tc_server::{Answer, Backend, Bytes, EventSource, Next};

const LOAD: Duration = Duration::from_secs(3);
const MIB: usize = 1024 * 1024;

/// Both tests read the lateness of their own runtime, and the machine's, so they take turns.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// The stand-in's answers, a frame every 5 ms on the event stream, and optionally one call
/// that blocks its worker thread, as a synchronous disk read would.
struct Loaded {
    inner: Stub,
    block_once: Option<Duration>,
    blocked: AtomicBool,
}

impl Loaded {
    fn new(block_once: Option<Duration>) -> Self {
        Self {
            inner: Stub::new().unwrap(),
            block_once,
            blocked: AtomicBool::new(false),
        }
    }
}

struct Ticker {
    seq: u64,
}

impl EventSource for Ticker {
    async fn next(&mut self) -> Next {
        tokio::time::sleep(Duration::from_millis(5)).await;
        self.seq += 1;
        Next::Frame(EventFrame {
            seq: self.seq,
            instance: ServerInstance::from_bytes([7; 16]),
            event: Event::Internet {
                on: self.seq.is_multiple_of(2),
            },
        })
    }
}

impl Backend for Loaded {
    type Events = Ticker;

    async fn call(&self, path: &'static str, body: Bytes) -> Answer {
        if let Some(block) = self.block_once
            && !self.blocked.swap(true, Ordering::SeqCst)
        {
            // The mistake the test exists to catch.
            std::thread::sleep(block);
        }
        self.inner.call(path, body).await
    }
    fn subscribe(&self, _after: u64) -> Ticker {
        Ticker { seq: 0 }
    }
    async fn quit(&self) {
        self.inner.quit().await;
    }
    async fn quitting(&self) {
        self.inner.quitting().await;
    }
}

/// What the load did, for the evidence file.
#[derive(Debug, Default)]
struct Counts {
    api_calls: AtomicU64,
    stream_frames: AtomicU64,
    upload_bytes: AtomicU64,
    static_gets: AtomicU64,
    downloads: AtomicU64,
}

struct Outcome {
    worst_runtime: Duration,
    worst_os: Duration,
    counts: Counts,
}

/// Runs the load for [`LOAD`] against a one-worker server and reports the worst lateness.
fn run(backend: Loaded) -> Outcome {
    let (server, runtime) = TestServer::start_on(Arc::new(backend), |_| {}, true, 1);
    let server = Arc::new(server);
    // The files the load fetches.
    let big = vec![b'u'; 5 * MIB];
    std::fs::write(server.ui_root.path().join("site/ui/big.bin"), &big).unwrap();
    let written = server.handle.downloads().dir().join("work.tmp");
    std::fs::write(&written, &big).unwrap();
    let download = server
        .handle
        .downloads()
        .add(&written, "big.html", "text/html; charset=utf-8")
        .unwrap();

    let counts = Arc::new(Counts::default());
    let stop = Arc::new(AtomicBool::new(false));
    let dog = Watchdog::start(&runtime, Duration::from_millis(10));
    let began = Instant::now();
    let mut threads = Vec::new();

    // Four callers of the API, a millisecond apart.
    for _ in 0..4 {
        let (server, counts, stop) = (Arc::clone(&server), Arc::clone(&counts), Arc::clone(&stop));
        threads.push(std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let response = server.api("app/hello", "{}");
                assert_eq!(response.status, 200);
                counts.api_calls.fetch_add(1, Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(1));
            }
        }));
    }
    // Two event streams.
    for _ in 0..2 {
        let (server, counts, stop) = (Arc::clone(&server), Arc::clone(&counts), Arc::clone(&stop));
        threads.push(std::thread::spawn(move || {
            let mut reader = SseReader::open(&server, "");
            while !stop.load(Ordering::Relaxed) {
                match reader.read(Duration::from_millis(200)) {
                    Got::Item(_) => {
                        counts.stream_frames.fetch_add(1, Ordering::Relaxed);
                    }
                    Got::TimedOut => {}
                    Got::End => break,
                }
            }
        }));
    }
    // An exchange upload of 256 MiB, sent at about 60 MB/s until the load ends.
    {
        let (server, counts, stop) = (Arc::clone(&server), Arc::clone(&counts), Arc::clone(&stop));
        threads.push(std::thread::spawn(move || {
            let declared = 256 * MIB;
            let request = server
                .upload_req(UploadFlow::Exchange, "work.tcx", b"")
                .with("Content-Length", &declared.to_string());
            let mut stream = server.connect();
            stream.write_all(&request.head_bytes()).unwrap();
            let piece = vec![7_u8; 64 * 1024];
            let mut sent = 0;
            while sent < declared && !stop.load(Ordering::Relaxed) {
                if stream.write_all(&piece).is_err() {
                    break;
                }
                sent += piece.len();
                counts
                    .upload_bytes
                    .fetch_add(piece.len() as u64, Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(1));
            }
            if sent == declared {
                assert_eq!(read_response(&mut stream).status, 200);
            }
            // Otherwise the load ended first and the connection is cut: the server deletes the
            // partial file, which `uploads_are_gone` below checks.
        }));
    }
    // Two readers of a 5 MiB static file, and one of a 5 MiB download.
    for _ in 0..2 {
        let (server, counts, stop) = (Arc::clone(&server), Arc::clone(&counts), Arc::clone(&stop));
        threads.push(std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let response = server.get("/ui/big.bin");
                assert_eq!(response.body.len(), 5 * MIB);
                counts.static_gets.fetch_add(1, Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(40));
            }
        }));
    }
    {
        let (server, counts, stop) = (Arc::clone(&server), Arc::clone(&counts), Arc::clone(&stop));
        let path = format!("/download/{}/big.html", download.as_str());
        threads.push(std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let response = server.get(&path);
                assert_eq!(response.body.len(), 5 * MIB);
                counts.downloads.fetch_add(1, Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(40));
            }
        }));
    }

    std::thread::sleep(LOAD.saturating_sub(began.elapsed()));
    stop.store(true, Ordering::Relaxed);
    for thread in threads {
        thread.join().unwrap();
    }
    let outcome = Outcome {
        worst_runtime: dog.worst_runtime(),
        worst_os: dog.worst_os(),
        counts: Arc::try_unwrap(counts).unwrap(),
    };
    // A cut upload leaves nothing behind.
    let uploads = server.uploads_dir();
    let until = Instant::now() + Duration::from_secs(5);
    while std::fs::read_dir(&uploads).unwrap().next().is_some() {
        assert!(Instant::now() < until, "an upload was left behind");
        std::thread::sleep(Duration::from_millis(20));
    }
    outcome
}

fn report(what: &str, outcome: &Outcome) {
    let c = &outcome.counts;
    println!(
        "{what}: worst runtime lateness {:?}, worst OS lateness {:?}; load: {} API calls, {} \
         stream frames, {:.0} MiB uploaded, {} static GETs of 5 MiB, {} downloads of 5 MiB",
        outcome.worst_runtime,
        outcome.worst_os,
        c.api_calls.load(Ordering::Relaxed),
        c.stream_frames.load(Ordering::Relaxed),
        c.upload_bytes.load(Ordering::Relaxed) as f64 / MIB as f64,
        c.static_gets.load(Ordering::Relaxed),
        c.downloads.load(Ordering::Relaxed),
    );
}

#[test]
fn tokio_never_blocked() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    // A blocking call shows in every run; a busy machine's noise does not repeat. So a run over the
    // bound is measured again, up to three times, before it counts.
    let mut outcome = run(Loaded::new(None));
    for _ in 0..2 {
        if outcome.worst_runtime <= Duration::from_millis(50) + outcome.worst_os {
            break;
        }
        outcome = run(Loaded::new(None));
    }
    report("tokio_never_blocked", &outcome);
    // The load really ran.
    let c = &outcome.counts;
    assert!(c.api_calls.load(Ordering::Relaxed) > 100);
    assert!(c.stream_frames.load(Ordering::Relaxed) > 100);
    assert!(c.upload_bytes.load(Ordering::Relaxed) > 16 * MIB as u64);
    assert!(c.static_gets.load(Ordering::Relaxed) > 4);
    assert!(c.downloads.load(Ordering::Relaxed) > 2);
    assert!(
        outcome.worst_runtime <= Duration::from_millis(50) + outcome.worst_os,
        "the runtime woke {:?} late (the machine alone: {:?})",
        outcome.worst_runtime,
        outcome.worst_os
    );
}

/// The proof that the test above can fail: the same load and server with one call that blocks
/// its worker thread for 120 ms, as a synchronous read of a big file would.
#[test]
fn a_blocking_call_in_the_server_is_seen() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let outcome = run(Loaded::new(Some(Duration::from_millis(120))));
    report("a_blocking_call_in_the_server_is_seen", &outcome);
    assert!(
        outcome.worst_runtime >= Duration::from_millis(100),
        "a 120 ms block showed as only {:?}",
        outcome.worst_runtime
    );
    // When the machine alone stalled longer than the block, no bound can tell them apart, and
    // that is the machine's speed, not a fault (rulings/2026-10-23-tests-prove-behavior-not-speed.md).
    if outcome.worst_os + Duration::from_millis(50) < Duration::from_millis(120) {
        assert!(
            outcome.worst_runtime > Duration::from_millis(50) + outcome.worst_os,
            "the bound of the main test would have passed: {:?} against {:?}",
            outcome.worst_runtime,
            outcome.worst_os
        );
    }
}

#[test]
fn the_watchdog_sees_a_blocked_worker() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let dog = Watchdog::start(runtime.handle(), Duration::from_millis(10));
    runtime.block_on(async {
        tokio::time::sleep(Duration::from_millis(60)).await;
        tokio::spawn(async { std::thread::sleep(Duration::from_millis(120)) })
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(60)).await;
    });
    assert!(
        dog.worst_runtime() >= Duration::from_millis(100),
        "{:?}",
        dog.worst_runtime()
    );
}
