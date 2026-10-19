//! Times what the memory backend and the session registry cost (docs/PERFORMANCE.md rule 5,
//! unit B1b): the corpora every operation-count run folds, a snapshot, an open and a close, a
//! chapter read on Psalms and on ten times Psalms, and what a `tc4 serve --scenario` start
//! pays before its address line. No budget applies; the numbers size the harness and check
//! rule 1 with a clock beside the count tests.
//!
//! ```text
//! cargo run --release -p tc-app --features fixture-backend,test-hooks --example memory_bench -- --runs 20
//! ```
//!
//! Prints one JSON line per measurement: its name, the runs, and the median and p95.

// A benchmark stops at its first failure, with the reason: nothing to keep going for.
#![allow(clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use serde_json::json;
use tc_api::jobs::{OpenPhase, Phase};
use tc_app::App;
use tc_app::AppDeps;
use tc_app::backend::memory::{MemoryBackend, MemoryProject};
use tc_app::backend::scenario::{ScenarioState, install_resources};
use tc_app::backend::{ProjectHandle, ProjectStore};
use tc_app::config::{AppConfig, ProjectsFolder};
use tc_app::jobs::ProgressSink;
use tc_corpus::{Corpus, Spec};
use tc_journal::BookId;
use tc_journal::fold::{FoldInput, FoldState, ProjectKind};
use tc_resources::ResourceStore;

fn millis(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
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

fn progress() -> ProgressSink {
    ProgressSink::new(Phase::Open(OpenPhase::ReadingChanges))
}

fn fold_corpus(spec: &Spec) -> MemoryProject {
    let corpus = Corpus::generate(spec).unwrap_or_else(|e| panic!("the corpus builds: {e}"));
    let fold = FoldState::fold(
        ProjectKind::Bible,
        corpus
            .journal
            .lines()
            .into_iter()
            .map(|line| FoldInput::from(line).prepare()),
    );
    let mut project = MemoryProject::empty(corpus.journal.project, ProjectKind::Bible);
    project.fold = fold;
    project
}

fn time_corpus(name: &str, spec: &Spec, runs: usize) -> MemoryProject {
    let mut samples = Vec::new();
    let mut project = None;
    for _ in 0..runs {
        let start = Instant::now();
        project = Some(fold_corpus(spec));
        samples.push(millis(start));
    }
    report(name, "ms", &samples);
    project.expect("at least one run")
}

/// The first Psalms chapter alike in both corpora, as the count tests choose it.
fn common_chapter(small: &FoldState, large: &FoldState) -> u16 {
    small
        .chapters_of(BookId::Psalms)
        .into_iter()
        .find(|&c| {
            let keys = small.chapter_keys(BookId::Psalms, c);
            keys == large.chapter_keys(BookId::Psalms, c)
                && keys.comments.is_empty()
                && !keys.verses.is_empty()
        })
        .expect("a common chapter")
}

async fn app_over(dir: &Path, backend: MemoryBackend) -> App {
    let projects = dir.join("projects");
    std::fs::create_dir_all(&projects).expect("a projects folder");
    let config = AppConfig {
        app_data: dir.join("app-data"),
        projects: ProjectsFolder::Given(projects),
        door43: "http://127.0.0.1:9".parse().expect("a url"),
        build: tc_api::app::BuildInfo {
            version: "0".to_owned(),
            commit: "bench".to_owned(),
        },
        bundle: None,
        #[cfg(feature = "test-hooks")]
        test_hooks: false,
        #[cfg(feature = "test-hooks")]
        seed_pace: None,
    };
    App::start(
        config,
        AppDeps {
            store: Arc::new(backend),
            ..AppDeps::production()
        },
    )
    .await
    .expect("the app starts")
}

fn chapter_read(handle: &dyn ProjectHandle, chapter: u16) {
    let snapshot = handle.snapshot();
    let read = tc_app::counters::CountingSnapshot::new(
        snapshot,
        Arc::new(tc_app::counters::AppCounters::default()),
    );
    let keys = read.chapter_keys(BookId::Psalms, chapter);
    for key in keys.front.iter().chain(&keys.verses) {
        read.key_state(key);
        read.mint(key, None).expect("a token");
    }
    read.chapter_usfm(BookId::Psalms, chapter)
        .expect("the chapter");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runs = 20usize;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--runs" => runs = args.next().ok_or("--runs needs a number")?.parse()?,
            other => return Err(format!("unknown argument {other}").into()),
        }
    }
    // Five runs for anything over a second (PERFORMANCE rule 5).
    let psalms = time_corpus(
        "corpus: generate and fold Psalms",
        &Spec::psalms(1),
        runs.min(5),
    );
    let larger = time_corpus(
        "corpus: generate and fold ten times Psalms",
        &Spec::scaled(1, 10),
        runs.min(5),
    );
    let chapter = common_chapter(&psalms.fold, &larger.fold);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    for (label, project) in [("Psalms", &psalms), ("ten times Psalms", &larger)] {
        let backend = MemoryBackend::new();
        let folder = tc_api::ids::ProjectRef::parse("psalms")?;
        backend.add(folder.clone(), project.clone())?;
        let handle = backend
            .open(&folder, progress())
            .map_err(|e| format!("{e:?}"))?;

        // A snapshot and its clone, per call, averaged over a million calls per run.
        let mut samples = Vec::new();
        for _ in 0..runs {
            let start = Instant::now();
            for _ in 0..1_000_000 {
                let snapshot = handle.snapshot();
                std::hint::black_box(snapshot.clone());
            }
            samples.push(start.elapsed().as_nanos() as f64 / 1_000_000.0);
        }
        report(
            &format!("snapshot and clone, {label}"),
            "ns per call",
            &samples,
        );

        // The first reads of a process are slower (caches, the allocator after the corpus was
        // made, a shared machine): the first is reported apart, and a hundred warm the read
        // before the timed runs, so the two corpora are compared on equal terms.
        let start = Instant::now();
        chapter_read(handle.as_ref(), chapter);
        report(
            &format!("first chapter read of Psalms {chapter}, {label}"),
            "ms",
            &[millis(start)],
        );
        for _ in 0..100 {
            chapter_read(handle.as_ref(), chapter);
        }
        let mut samples = Vec::new();
        for _ in 0..runs {
            let start = Instant::now();
            chapter_read(handle.as_ref(), chapter);
            samples.push(millis(start));
        }
        report(
            &format!("chapter read of Psalms {chapter}, {label}"),
            "ms",
            &samples,
        );
        drop(handle);

        // An open for the screen, then going Home until the session is closed.
        let dir = tempfile::tempdir()?;
        let samples = runtime.block_on(async {
            let app = app_over(dir.path(), backend.clone()).await;
            let mut samples = Vec::new();
            for _ in 0..runs {
                let start = Instant::now();
                let opened = app
                    .sessions()
                    .open_screen(&folder, progress())
                    .await
                    .expect("it opens");
                app.sessions().leave_screen(&opened.at).expect("it leaves");
                app.sessions().settled(&folder).await;
                samples.push(millis(start));
            }
            samples
        });
        report(
            &format!("open the screen, go Home, closed: {label}"),
            "ms",
            &samples,
        );
    }

    // What `tc4 serve --scenario` pays before its address line.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let state = ScenarioState::from_repository(&root, "sample")?;
    let mut seeds = Vec::new();
    let mut installs = Vec::new();
    for _ in 0..runs {
        let start = Instant::now();
        let (_, report) = MemoryBackend::from_scenario(&state)?;
        seeds.push(millis(start));
        assert_eq!(report.loaded.len(), 1);
        let dir = tempfile::tempdir()?;
        let store = ResourceStore::open(dir.path().join("resources"))?;
        let resources = state.resources()?;
        let start = Instant::now();
        let installed = install_resources(&resources, &store)?;
        installs.push(millis(start));
        assert_eq!(installed.len(), 12);
    }
    report("scenario sample: seed both projects", "ms", &seeds);
    report(
        "scenario sample: install 12 resources into a fresh store",
        "ms",
        &installs,
    );
    Ok(())
}
