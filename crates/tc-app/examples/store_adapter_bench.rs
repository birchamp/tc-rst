//! What the store adapter adds to tc-store (unit C0, docs/PERFORMANCE.md rule 5), on the
//! whole-Bible project tc-store's own bench writes. Each row times the same work through
//! tc-store directly and through the seam, alternating the two so disk noise falls on both:
//!
//! - M1 `snapshot()` per call, 100,000 calls a run;
//! - M2 a Psalms verse saved until its reply;
//! - M3 from a save's submit to its `Committed` at a subscriber (the seam's events thread and
//!   the join with the command's origin);
//! - M4 `editor/saveVerse` through `App::call` until its `Ack` (dispatch, the reply cache, the
//!   session, `wire::write::ack`, the store);
//! - M5 an open of the whole Bible (warm cache), and when the first progress report reaches
//!   the open's sink;
//! - M6 a close after 10 saves;
//! - M7 a save on tc-store's in-memory file system, where no disk adds its noise: blocking on
//!   tc-store's reply, awaiting it on the runtime, and through the seam, 2,000 each.
//!
//! ```text
//! cargo run --release -p tc-store --example storebench -j 6 -- setup target/store-adapter-bench
//! cargo run --release -p tc-app --features test-hooks --example store_adapter_bench -j 6 -- target/store-adapter-bench
//! # Only the production verse-save clock probe (200 samples; skips M1–M3 and M5–M7):
//! cargo run --release -p tc-app --features test-hooks --example store_adapter_bench -j 6 -- target/store-adapter-bench --m4-only
//! ```
//!
//! The direct runs use `DIR/projects/bible`, the seam's a copy of it, `DIR/projects/bible-seam`,
//! so both can be open at once; with `STORE_ADAPTER_BENCH_SWAP=1` the two swap folders, so a
//! difference that follows the folder is the disk's, not the adapter's. Prints one JSON line
//! per measurement. M4 measures 200 changed Psalms verses through the production route with
//! runtime hooks disabled, including the store's fsync before Ack. These local measurements
//! are compared with the 25 ms budget; the Windows reference laptop requires its own run.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "a benchmark stops at its first failure, with the reason"
)]
#![allow(
    clippy::disallowed_methods,
    reason = "the direct runs time tc-store's own handle, minting as its bench does"
)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;
use tc_api::editor::SaveVerse;
use tc_api::jobs::{OpenPhase, Phase};
use tc_api::{Endpoint, Reply};
use tc_app::backend::store::StoreBackend;
use tc_app::backend::{ProjectStore, StoreCommand, StoreEvent};
use tc_app::config::{AppConfig, ProjectsFolder};
use tc_app::jobs::ProgressSink;
use tc_app::{Answer, App, AppDeps, StoreChoice};
use tc_journal::command::Command;
use tc_journal::{BookId, RecordKey};

const SEAM: &str = "bible-seam";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    All,
    M4,
}

fn millis(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// The `q`-quantile of samples, by the nearest rank.
fn quantile(samples: &[f64], q: f64) -> f64 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    if sorted.is_empty() {
        return f64::NAN;
    }
    let rank = (q * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

fn report(name: &str, unit: &str, samples: &[f64]) {
    println!(
        "{}",
        json!({
            "name": name,
            "unit": unit,
            "runs": samples.len(),
            "median": quantile(samples, 0.5),
            "p95": quantile(samples, 0.95),
        })
    );
}

fn config(dir: &Path) -> tc_store::StoreConfig {
    tc_store::StoreConfig {
        app_data: dir.join("app"),
        projects: dir.join("projects"),
        vfs: Arc::new(tc_store::StdFs),
        platform: Arc::new(tc_store::StdPlatform),
        clock: Arc::new(tc_store::SystemClock),
        threads: std::thread::available_parallelism().map_or(2, |n| n.get()),
        watch_debounce: std::time::Duration::from_millis(20),
    }
}

fn progress() -> ProgressSink {
    ProgressSink::new(Phase::Open(OpenPhase::ReadingChanges))
}

/// Copies the project folder for the seam's runs, once; the lock and the device file stay
/// behind, so the copy is a project of its own device.
fn copy_project(from: &Path, to: &Path) {
    if to.exists() {
        return;
    }
    let mut stack = vec![(from.to_owned(), to.to_owned())];
    while let Some((src, dst)) = stack.pop() {
        std::fs::create_dir_all(&dst).unwrap();
        for entry in std::fs::read_dir(&src).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            let (s, d) = (src.join(&name), dst.join(&name));
            if entry.file_type().unwrap().is_dir() {
                stack.push((s, d));
            } else if !(src.ends_with(".tc") && (name == "lock" || name == "device.json")) {
                std::fs::copy(&s, &d).unwrap();
            }
        }
    }
}

/// Psalms verses, from a snapshot.
fn psalms(fold: &tc_journal::fold::FoldState, n: usize) -> Vec<RecordKey> {
    let mut verses = Vec::new();
    for chapter in fold.chapters_of(BookId::Psalms) {
        verses.extend(fold.chapter_keys(BookId::Psalms, chapter).verses);
        if verses.len() >= n {
            break;
        }
    }
    verses.truncate(n);
    verses
}

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("DIR"));
    let mode = if std::env::args().skip(2).any(|arg| arg == "--m4-only") {
        Mode::M4
    } else {
        Mode::All
    };
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    if mode == Mode::All {
        rt.block_on(in_memory());
    }
    rt.block_on(bench(&dir, mode));
}

/// The sample's Titus as records, as tc-app's store tests make it.
fn titus_records() -> Vec<tc_journal::Record> {
    use tc_journal::record::{BookPresence, PresentHow, Scope, VersificationValue};
    use tc_journal::{MetaField, Record};
    let usfm = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/tc4/sample-burrito/ingredients/TIT.usfm"),
    )
    .unwrap();
    let parts = tc_usfm::codec::decompose_bytes(&usfm).unwrap();
    let mut records = vec![
        Record::ProjectMeta {
            field: MetaField::Name,
            value: Some(tc_journal::json::parse(br#"{"en":"Titus"}"#).unwrap()),
        },
        Record::Versification {
            value: VersificationValue {
                name: Some("eng".into()),
                bytes: tc_versification::StandardScheme::Eng.json().to_owned(),
            },
        },
        Record::BookPresence {
            book: BookId::Titus,
            value: BookPresence::Present {
                scope: Scope::Whole,
                how: PresentHow::Added,
            },
        },
    ];
    records.extend(tc_journal::fold::records_from_usfm(BookId::Titus, &parts));
    records
}

/// M7: what the seam adds to a save when no disk adds its noise.
async fn in_memory() {
    let fs = tc_store::fault::FaultFs::new();
    let backend = Arc::new(
        StoreBackend::start(
            tc_store::StoreConfig {
                app_data: "/app".into(),
                projects: "/projects".into(),
                vfs: Arc::new(fs.clone()),
                platform: Arc::new(tc_store::FakePlatform::default()),
                clock: Arc::new(tc_store::SystemClock),
                threads: 2,
                watch_debounce: std::time::Duration::from_millis(20),
            },
            Arc::new(tc_store::Hooks::default()),
        )
        .unwrap(),
    );
    backend.store().wait_cleanup();
    // One project for each path, so each gets the same writes.
    for name in ["blocking", "awaited", "seam"] {
        backend
            .store()
            .create(&tc_store::NewProject {
                name: name.to_owned(),
                kind: tc_journal::fold::ProjectKind::Bible,
                records: titus_records(),
                books: Vec::new(),
            })
            .unwrap();
    }
    let open = |name: &str| {
        let handle = backend
            .store()
            .open(
                &Path::new("/projects").join(name),
                &tc_store::OpenOptions::default(),
            )
            .unwrap();
        handle.wait_idle();
        handle
    };
    let (direct, other) = (open("blocking"), open("awaited"));
    let seam = backend
        .open(&tc_api::ids::ProjectRef::parse("seam").unwrap(), progress())
        .unwrap();
    seam.wait_idle();
    let verses: Vec<RecordKey> = {
        let snapshot = direct.snapshot();
        let fold = snapshot.fold();
        fold.chapters_of(BookId::Titus)
            .into_iter()
            .flat_map(|c| fold.chapter_keys(BookId::Titus, c).verses)
            .collect()
    };
    let (mut blocking, mut awaited, mut through) = (Vec::new(), Vec::new(), Vec::new());
    for i in 0..2_000 {
        let key = &verses[i % verses.len()];
        let shown = direct.mint(key, None).unwrap();
        let started = Instant::now();
        direct
            .submit(Command::SaveVerse {
                shown,
                words: format!("blocking {i}"),
            })
            .wait()
            .unwrap();
        blocking.push(millis(started.elapsed()));
        let shown = other.mint(key, None).unwrap();
        let started = Instant::now();
        other
            .submit(Command::SaveVerse {
                shown,
                words: format!("awaited {i}"),
            })
            .await
            .unwrap();
        awaited.push(millis(started.elapsed()));
        let token = seam.snapshot().mint(key, None).unwrap();
        let started = Instant::now();
        seam.submit(StoreCommand::save_verse(token, format!("seam {i}")))
            .await
            .unwrap();
        through.push(millis(started.elapsed()));
    }
    report(
        "M7 in memory: tc-store, blocking on the reply",
        "ms",
        &blocking,
    );
    report("M7 in memory: tc-store, awaiting the reply", "ms", &awaited);
    report("M7 in memory: through the seam", "ms", &through);
    direct.close().unwrap();
    other.close().unwrap();
    seam.close().await.unwrap();
}

async fn bench(dir: &Path, mode: Mode) {
    copy_project(
        &dir.join("projects/bible"),
        &dir.join("projects").join(SEAM),
    );
    let swap = std::env::var_os("STORE_ADAPTER_BENCH_SWAP").is_some();
    let (direct_name, seam_name) = if swap {
        (SEAM, "bible")
    } else {
        ("bible", SEAM)
    };
    let direct_store = tc_store::Store::start(config(dir)).unwrap();
    direct_store.wait_cleanup();
    let seam_store =
        Arc::new(StoreBackend::start(config(dir), Arc::new(tc_store::Hooks::default())).unwrap());
    let direct_path = dir.join("projects").join(direct_name);
    let seam_folder = tc_api::ids::ProjectRef::parse(seam_name).unwrap();
    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());

    if mode == Mode::All {
        // ---- M5: opens, warm (a first untimed open of each writes its fold cache).
        for _ in 0..1 {
            direct_store
                .open(&direct_path, &tc_store::OpenOptions::default())
                .unwrap()
                .close()
                .unwrap();
            let h = seam_store.open(&seam_folder, progress()).unwrap();
            h.close().await.unwrap();
        }
        let (mut open_direct, mut open_seam, mut first_report) =
            (Vec::new(), Vec::new(), Vec::new());
        for _ in 0..5 {
            let started = Instant::now();
            let h = direct_store
                .open(&direct_path, &tc_store::OpenOptions::default())
                .unwrap();
            open_direct.push(millis(started.elapsed()));
            h.wait_idle();
            h.close().unwrap();

            let sink = progress();
            let initial = sink.snapshot();
            let seen: Arc<Mutex<Option<Duration>>> = Arc::default();
            let started = Instant::now();
            let watcher = {
                let (sink, seen) = (sink.clone(), seen.clone());
                std::thread::spawn(move || {
                    while started.elapsed() < Duration::from_secs(30) {
                        if sink.snapshot() != initial {
                            *seen.lock().unwrap() = Some(started.elapsed());
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(1));
                    }
                })
            };
            let h = {
                let (store, folder) = (seam_store.clone(), seam_folder.clone());
                tokio::task::spawn_blocking(move || store.open(&folder, sink).unwrap())
                    .await
                    .unwrap()
            };
            open_seam.push(millis(started.elapsed()));
            watcher.join().unwrap();
            if let Some(at) = *seen.lock().unwrap() {
                first_report.push(millis(at));
            }
            h.wait_idle();
            h.close().await.unwrap();
        }
        report("M5 open, whole Bible, warm: tc-store", "ms", &open_direct);
        report("M5 open, whole Bible, warm: seam", "ms", &open_seam);
        report("M5 the seam's first forwarded report", "ms", &first_report);
    }

    // ---- Both open from here.
    let direct = direct_store
        .open(&direct_path, &tc_store::OpenOptions::default())
        .unwrap();
    direct.wait_idle();
    let verses = psalms(direct.snapshot().fold(), 400);
    if mode == Mode::All {
        let seam = seam_store.open(&seam_folder, progress()).unwrap();
        seam.wait_idle();

        // ---- M1: snapshots.
        let (mut snap_direct, mut snap_seam) = (Vec::new(), Vec::new());
        for _ in 0..20 {
            let started = Instant::now();
            for _ in 0..100_000 {
                std::hint::black_box(direct.snapshot());
            }
            snap_direct.push(started.elapsed().as_nanos() as f64 / 100_000.0);
            let started = Instant::now();
            for _ in 0..100_000 {
                std::hint::black_box(seam.snapshot());
            }
            snap_seam.push(started.elapsed().as_nanos() as f64 / 100_000.0);
        }
        report("M1 snapshot(): tc-store", "ns", &snap_direct);
        report("M1 snapshot(): seam", "ns", &snap_seam);

        // ---- M2 and M3: saves until the reply, and until the subscriber's event.
        let direct_events = direct.subscribe();
        let mut seam_events = seam.subscribe();
        let (mut save_direct, mut save_seam) = (Vec::new(), Vec::new());
        let (mut event_direct, mut event_seam) = (Vec::new(), Vec::new());
        let mut one_save = None;
        for (i, key) in verses.iter().take(200).enumerate() {
            let shown = direct.mint(key, None).unwrap();
            let started = Instant::now();
            let reply = direct
                .submit(Command::SaveVerse {
                    shown,
                    words: format!("A direct save {i} of run {run}"),
                })
                .wait()
                .unwrap();
            save_direct.push(millis(started.elapsed()));
            assert!(!reply.changes.is_empty());
            loop {
                if let tc_store::StoreEvent::Committed { seq, .. } = direct_events.recv().unwrap()
                    && seq == reply.seq
                {
                    event_direct.push(millis(started.elapsed()));
                    break;
                }
            }

            let token = seam.snapshot().mint(key, None).unwrap();
            let before = seam.counters();
            let started = Instant::now();
            let reply = seam
                .submit(StoreCommand::save_verse(
                    token,
                    format!("A seam save {i} of run {run}"),
                ))
                .await
                .unwrap();
            save_seam.push(millis(started.elapsed()));
            if one_save.is_none() {
                one_save = Some(seam.counters().since(&before));
            }
            assert!(!reply.changes.is_empty());
            loop {
                if let Some(StoreEvent::Committed { seq, .. }) = seam_events.recv().await
                    && seq == reply.seq
                {
                    event_seam.push(millis(started.elapsed()));
                    break;
                }
            }
        }
        report(
            "M2 save a Psalms verse until its reply: tc-store",
            "ms",
            &save_direct,
        );
        report(
            "M2 save a Psalms verse until its reply: seam",
            "ms",
            &save_seam,
        );
        report(
            "M3 save until the subscriber's Committed: tc-store",
            "ms",
            &event_direct,
        );
        report(
            "M3 save until the subscriber's Committed: seam",
            "ms",
            &event_seam,
        );
        let one = one_save.unwrap();
        println!(
            "{}",
            json!({"name": "one seam save's store counters", "files_opened": one.files_opened,
               "files_created": one.files_created, "fsyncs": one.fsyncs, "renames": one.renames})
        );
        drop(seam_events);
        drop(direct_events);
        direct.wait_idle();
        seam.wait_idle();
        seam.close().await.unwrap();
    }

    // ---- M4: editor/saveVerse through App::call, beside direct saves.
    let app_dir = dir.join("bench-app");
    let _ = std::fs::remove_dir_all(&app_dir);
    let app = App::start(
        AppConfig {
            app_data: app_dir.clone(),
            projects: ProjectsFolder::Given(dir.join("projects")),
            door43: "http://127.0.0.1:9".parse().unwrap(),
            build: tc_api::app::BuildInfo {
                version: "bench".to_owned(),
                commit: "bench".to_owned(),
            },
            bundle: None,
            test_hooks: false,
            seed_pace: None,
        },
        AppDeps {
            store: StoreChoice::Given(seam_store.clone()),
            ..AppDeps::production()
        },
    )
    .await
    .unwrap();
    let open = app
        .sessions()
        .open_screen(&seam_folder, progress())
        .await
        .unwrap();
    let session = app.sessions().reader(&open.at).unwrap();
    session.handle().wait_idle();
    let (mut call_direct, mut call_app) = (Vec::new(), Vec::new());
    for (i, key) in verses.iter().skip(200).take(200).enumerate() {
        let shown = direct.mint(key, None).unwrap();
        let started = Instant::now();
        direct
            .submit(Command::SaveVerse {
                shown,
                words: format!("A direct save {i} of run {run}, again"),
            })
            .wait()
            .unwrap();
        call_direct.push(millis(started.elapsed()));

        let minted = session.read().mint(key, None).unwrap();
        let body = serde_json::to_vec(&SaveVerse {
            at: open.at.clone(),
            request: tc_api::ids::RequestId::from_bytes(u128::try_from(i).unwrap().to_be_bytes()),
            shown: tc_app::wire::token::shown(&minted).unwrap(),
            words: format!("An app save {i} of run {run}"),
        })
        .unwrap();
        let started = Instant::now();
        let answer = app.call(SaveVerse::PATH, &body).await;
        call_app.push(millis(started.elapsed()));
        let Answer::Reply { body, .. } = answer else {
            panic!("{answer:?}")
        };
        let reply: Reply<tc_api::write::Ack> = serde_json::from_slice(&body).unwrap();
        let ack = reply.into_result().unwrap();
        assert_eq!(ack.stored, tc_api::write::Stored::Written);
        assert!(
            ack.disk_warning.is_none(),
            "a non-durable save is not a timing success"
        );
    }
    assert_eq!(
        call_app.len(),
        200,
        "the fixture must supply 200 changed Psalms saves"
    );
    report("M4 save until its reply: tc-store", "ms", &call_direct);
    report(
        "M4 editor/saveVerse through App::call until its Ack",
        "ms",
        &call_app,
    );
    app.quit().await;
    let _ = std::fs::remove_dir_all(&app_dir);
    direct.wait_idle();
    direct.close().unwrap();
    if mode == Mode::M4 {
        for root in [&direct_path, &dir.join("projects").join(seam_name)] {
            let problems = tc_burrito::validate_dir(root);
            assert!(problems.is_empty(), "{}: {problems:?}", root.display());
        }
        return;
    }

    // ---- M6: a close after 10 saves.
    let (mut close_direct, mut close_seam) = (Vec::new(), Vec::new());
    for round in 0..5 {
        let h = direct_store
            .open(&direct_path, &tc_store::OpenOptions::default())
            .unwrap();
        h.wait_idle();
        for (i, key) in verses.iter().take(10).enumerate() {
            h.submit(Command::SaveVerse {
                shown: h.mint(key, None).unwrap(),
                words: format!("Closed after {i} of round {round} of run {run}"),
            })
            .wait()
            .unwrap();
        }
        let started = Instant::now();
        h.close().unwrap();
        close_direct.push(millis(started.elapsed()));

        let h = seam_store.open(&seam_folder, progress()).unwrap();
        h.wait_idle();
        for (i, key) in verses.iter().take(10).enumerate() {
            let token = h.snapshot().mint(key, None).unwrap();
            h.submit(StoreCommand::save_verse(
                token,
                format!("Closed after {i} of round {round} of run {run}"),
            ))
            .await
            .unwrap();
        }
        let started = Instant::now();
        h.close().await.unwrap();
        close_seam.push(millis(started.elapsed()));
    }
    report("M6 close after 10 saves: tc-store", "ms", &close_direct);
    report("M6 close after 10 saves: seam", "ms", &close_seam);
}
