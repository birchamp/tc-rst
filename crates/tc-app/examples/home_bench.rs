//! Times what Home costs (docs/PERFORMANCE.md rule 5, unit B2): `home/list` and one
//! `home/card` with their summaries kept, on the sample and on ten copies of it; one summary's
//! work (its files and their counts) on the sample, Psalms and ten times Psalms; and
//! `home/languages`: the list's first read and each query's ranking, with the reply's bytes.
//!
//! ```text
//! cargo run --release -p tc-app --features fixture-backend --example home_bench -- --runs 20
//! ```
//!
//! Prints one JSON line per measurement: its name, the runs, and the median and p95.

// A benchmark stops at its first failure, with the reason: nothing to keep going for.
#![allow(clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use tc_api::Endpoint;
use tc_api::event::Event;
use tc_api::home::{Card, Languages, List};
use tc_api::ids::ProjectRef;
use tc_app::backend::memory::{MemoryBackend, MemoryProject};
use tc_app::backend::seed::{Tc4Folder, fixture_project_id, tc4_state_seed};
use tc_app::backend::{ProjectStore, SummaryWatch};
use tc_app::config::{AppConfig, ProjectsFolder};
use tc_app::events::Next;
use tc_app::{Answer, App, AppDeps};
use tc_corpus::{Corpus, Spec};
use tc_journal::fold::{FoldInput, FoldState, ProjectKind};

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

fn report(name: &str, samples: &[f64], extra: serde_json::Value) {
    println!(
        "{}",
        json!({
            "name": name,
            "unit": "ms",
            "runs": samples.len(),
            "median": quantile(samples, 0.5),
            "p95": quantile(samples, 0.95),
            "extra": extra,
        })
    );
}

fn sample_bible() -> MemoryProject {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tc4/sample-burrito");
    let folder = Tc4Folder::read(&root).expect("the sample reads");
    let seed = tc4_state_seed(&folder, fixture_project_id("sample_burrito")).expect("it seeds");
    MemoryProject::from_seed(seed, None)
}

fn corpus(spec: &Spec) -> MemoryProject {
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

fn folder(name: &str) -> ProjectRef {
    ProjectRef::parse(name).expect("a folder name")
}

async fn start(dir: &Path, backend: MemoryBackend) -> App {
    let projects = dir.join("projects");
    std::fs::create_dir_all(&projects).expect("the projects folder");
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
            store: tc_app::StoreChoice::Given(Arc::new(backend)),
            ..AppDeps::production()
        },
    )
    .await
    .expect("the app starts")
}

/// One call through the app's bytes, with the reply's length.
async fn call<E: Endpoint>(app: &App, req: &E) -> usize {
    let body = serde_json::to_vec(req).expect("the request serializes");
    match app.call(E::PATH, &body).await {
        Answer::Reply { body, .. } => body.len(),
        other => panic!("{}: {other:?}", E::PATH),
    }
}

/// Lists Home and waits until every card's summary is in.
async fn warm(app: &App, folders: &[ProjectRef]) {
    let mut events = app.events().subscribe(app.events().head());
    call(app, &List {}).await;
    let mut left: Vec<ProjectRef> = folders.to_vec();
    while !left.is_empty() {
        let next = tokio::time::timeout(Duration::from_secs(120), events.next())
            .await
            .expect("a summary within two minutes");
        if let Next::Frame(frame) = next
            && let Event::HomeCard { project } = frame.event
        {
            left.retain(|f| *f != project);
        }
    }
}

async fn home_reads(name: &str, copies: usize, runs: usize) {
    let dir = tempfile::tempdir().expect("a temporary folder");
    let backend = MemoryBackend::new();
    let project = sample_bible();
    let folders: Vec<ProjectRef> = (0..copies)
        .map(|i| folder(&format!("sample_{i}")))
        .collect();
    for f in &folders {
        backend
            .add(f.clone(), project.clone())
            .expect("the copy is added");
    }
    let app = start(dir.path(), backend).await;
    warm(&app, &folders).await;
    let mut list = Vec::new();
    let mut card = Vec::new();
    let mut bytes = 0;
    for _ in 0..runs {
        let t = Instant::now();
        bytes = call(&app, &List {}).await;
        list.push(millis(t));
        let t = Instant::now();
        call(
            &app,
            &Card {
                project: folders[0].clone(),
            },
        )
        .await;
        card.push(millis(t));
    }
    report(
        &format!("home/list, warm, {name}"),
        &list,
        json!({ "cards": copies, "replyBytes": bytes }),
    );
    report(&format!("home/card, warm, {name}"), &card, json!({}));
}

fn summary_work(name: &str, project: MemoryProject, runs: usize) {
    let backend = MemoryBackend::new();
    let f = folder("p");
    backend
        .add(f.clone(), project)
        .expect("the project is added");
    let peek = backend.peek(&f).expect("a peek");
    let facts =
        tc_app::home::card_facts(&peek.metadata, None, &f, tc_api::home::ProjectKind::Bible);
    let books = facts.books.expect("the books");
    let mut samples = Vec::new();
    for _ in 0..runs {
        let watch = SummaryWatch {
            cancel: Default::default(),
            counters: Default::default(),
        };
        let t = Instant::now();
        let files = backend
            .summary_files(&f, &books, &watch)
            .expect("the files");
        let summary = tc_app::home::summary_of(&files, |_| None);
        samples.push(millis(t));
        assert_eq!(summary.books.len(), books.len());
    }
    report(
        &format!("one summary's work, {name}"),
        &samples,
        json!({ "books": books.len() }),
    );
}

async fn languages(runs: usize) {
    let dir = tempfile::tempdir().expect("a temporary folder");
    let app = start(dir.path(), MemoryBackend::new()).await;
    let t = Instant::now();
    call(
        &app,
        &Languages {
            query: String::new(),
        },
    )
    .await;
    report(
        "home/languages, the list's first read (parse and index)",
        &[millis(t)],
        json!({}),
    );
    for query in ["a", "ha", "hausa", "es-419", "american", "zzzzqqq"] {
        let mut samples = Vec::new();
        let mut bytes = 0;
        for _ in 0..runs {
            let t = Instant::now();
            bytes = call(
                &app,
                &Languages {
                    query: query.to_owned(),
                },
            )
            .await;
            samples.push(millis(t));
        }
        report(
            &format!("home/languages {query:?}"),
            &samples,
            json!({ "replyBytes": bytes }),
        );
    }
}

fn main() {
    let runs: usize = std::env::args()
        .skip_while(|a| a != "--runs")
        .nth(1)
        .and_then(|n| n.parse().ok())
        .unwrap_or(20);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        home_reads("the sample", 1, runs).await;
        home_reads("ten copies of the sample", 10, runs).await;
        languages(runs * 50).await;
    });
    summary_work("the sample (Jonah, Titus)", sample_bible(), runs);
    summary_work("Psalms", corpus(&Spec::psalms(1)), 5);
    summary_work("ten times Psalms", corpus(&Spec::scaled(1, 10)), 5);
}
