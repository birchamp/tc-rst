//! Measures first-launch seeding of the whole bundled suite (docs/PERFORMANCE.md rule 5 and
//! "Start-up", unit A9). Build the bundled store first:
//!
//! ```text
//! cargo run --release -p tc-app --example bundle_store -- --from bundle/en --out target/a9/bundle-full
//! cargo run --release -p tc-app --example seedbench -- --bundle target/a9/bundle-full \
//!   --first 5 --repeat 20 --close 5 --evict scripts/tools/evict-page-cache.py \
//!   --out target/a9/seedbench.json
//! ```
//!
//! `--evict <script>` runs `python3 -I <script> <bundle>` before each first launch and before the
//! reads, so the bundled files come from the disk, as at a person's first launch, and not from
//! the memory the build of the bundle left them in.
//!
//! Three things are timed, each with the real `App` and the real disk, under `--root`
//! (default `target/a9/bench`):
//!
//! - a **first launch**: `App::start` (the plan) and the time to the `Startup(Ready)` frame, in
//!   a process of its own so the peak resident memory (`VmHWM`) is that of one run;
//! - a **repeat launch**: `App::start` on the seeded app data, with the counts of content files
//!   opened and written, which must be 0;
//! - a **close**: `App::quit` 500 ms into a seed.
//!
//! A fourth figure bounds the close: the seed asks to stop between files and before each
//! install, but it reads and checks a whole install before it writes the first file of its copy,
//! so a close waits for the install being read. **reads** times that stretch for every bundled
//! install, with the files in the system's cache (a cold disk reads slower).
//!
//! The driver runs the first launches as child processes (`--one first`) and the other three in
//! one process each, and writes the median and p95 of each to `--out`.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tc_api::app::BuildInfo;
use tc_api::event::{Event, StartupState};
use tc_app::config::{AppConfig, ProjectsFolder};
use tc_app::events::Next;
use tc_app::reports::ReportOp;
use tc_app::{App, AppDeps};
use tc_resources::{ResourceStore, content_files_opened, content_files_written};

fn config(root: &Path, bundle: &Path) -> Result<AppConfig, String> {
    let projects = root.join("projects");
    std::fs::create_dir_all(&projects).map_err(|e| format!("{}: {e}", projects.display()))?;
    Ok(AppConfig {
        app_data: root.join("app-data"),
        projects: ProjectsFolder::Given(projects),
        door43: "http://127.0.0.1:9"
            .parse()
            .map_err(|e| format!("a url: {e}"))?,
        build: BuildInfo {
            version: "seedbench".to_owned(),
            commit: "0".to_owned(),
        },
        bundle: Some(bundle.to_owned()),
        #[cfg(feature = "test-hooks")]
        test_hooks: false,
        #[cfg(feature = "test-hooks")]
        seed_pace: None,
    })
}

fn ms(duration: Duration) -> f64 {
    (duration.as_secs_f64() * 1_000_000.0).round() / 1_000.0
}

/// The peak resident memory of this process, in KiB.
fn peak_memory_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

async fn start(root: &Path, bundle: &Path) -> Result<App, String> {
    App::start(config(root, bundle)?, AppDeps::production())
        .await
        .map_err(|e| e.to_string())
}

/// One first launch on a fresh app data: the plan, then the seed until `Ready`.
async fn first_launch(root: &Path, bundle: &Path) -> Result<Value, String> {
    let _ = std::fs::remove_dir_all(root);
    let began = Instant::now();
    let app = start(root, bundle).await?;
    let plan = began.elapsed();
    // The plan runs on this thread: it must have opened and written no content file.
    let planned = (content_files_opened(), content_files_written());
    let StartupState::Preparing(first) = app.startup() else {
        return Err(format!("nothing to seed: {:?}", app.startup()));
    };
    let mut frames = app.events().subscribe(0);
    let mut reports = 0;
    loop {
        match tokio::time::timeout(Duration::from_secs(900), frames.next()).await {
            Ok(Next::Frame(frame)) => match frame.event {
                Event::Startup(StartupState::Ready) => break,
                Event::Startup(StartupState::Failed { reason }) => return Err(reason),
                Event::Startup(StartupState::Preparing(_)) => reports += 1,
                _ => {}
            },
            other => return Err(format!("the event stream gave {other:?}")),
        }
    }
    let ready = began.elapsed();
    let report = app
        .reports()
        .all()
        .into_iter()
        .find(|report| report.op == ReportOp::Seed)
        .ok_or("no seed report")?;
    app.quit().await;
    Ok(json!({
        "planMs": ms(plan),
        "readyMs": ms(ready),
        "plannedFiles": first.total,
        "planOpened": planned.0,
        "planWritten": planned.1,
        "startupFrames": reports,
        "facts": report.facts,
        "peakMemoryKib": peak_memory_kib(),
    }))
}

/// `runs` repeat launches on a seeded app data.
async fn repeat_launches(root: &Path, bundle: &Path, runs: usize) -> Result<Value, String> {
    let mut times = Vec::new();
    let (mut opened, mut written) = (0, 0);
    for _ in 0..runs {
        let before = (content_files_opened(), content_files_written());
        let began = Instant::now();
        let app = start(root, bundle).await?;
        times.push(ms(began.elapsed()));
        opened += content_files_opened() - before.0;
        written += content_files_written() - before.1;
        if app.startup() != StartupState::Ready || app.jobs().live() != 0 {
            return Err(format!("a repeat launch said {:?}", app.startup()));
        }
        app.quit().await;
    }
    Ok(json!({"startMs": times, "contentFilesOpened": opened, "contentFilesWritten": written}))
}

/// `runs` closes, each 500 ms into a first launch.
async fn closes(root: &Path, bundle: &Path, runs: usize) -> Result<Value, String> {
    let mut times = Vec::new();
    let mut whole = Vec::new();
    for _ in 0..runs {
        let _ = std::fs::remove_dir_all(root);
        let app = start(root, bundle).await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        // A close that finds the seed done measures nothing.
        if !matches!(app.startup(), StartupState::Preparing(_)) {
            return Err(format!(
                "the seed ended before the close: {:?}",
                app.startup()
            ));
        }
        let began = Instant::now();
        app.quit().await;
        times.push(ms(began.elapsed()));
        let store = ResourceStore::open_read_only(root.join("app-data").join("resources"))
            .map_err(|e| e.to_string())?;
        let installs = store.installed().map_err(|e| e.to_string())?.installs;
        for install in &installs {
            install.verify().map_err(|e| e.to_string())?;
        }
        whole.push(installs.len());
    }
    Ok(json!({"quitMs": times, "wholeInstallsAtTheStop": whole}))
}

/// `runs` times, the time to read and check each bundled install: the longest stretch of a seed
/// that a close cannot interrupt.
fn install_reads(bundle: &Path, runs: usize) -> Result<Value, String> {
    let store = ResourceStore::open_read_only(bundle).map_err(|e| e.to_string())?;
    let installs = store.installed().map_err(|e| e.to_string())?.installs;
    let mut rows = Vec::new();
    for install in &installs {
        let mut times = Vec::new();
        for _ in 0..runs {
            let began = Instant::now();
            install.verify().map_err(|e| e.to_string())?;
            times.push(ms(began.elapsed()));
        }
        let bytes: u64 = install.receipt().files.iter().map(|file| file.size).sum();
        rows.push(json!({
            "install": install.dcs_pin().map_or_else(
                || install.dir().display().to_string(),
                |pin| pin.to_string(),
            ),
            "files": install.files().count(),
            "bytes": bytes,
            "firstReadMs": times.first().copied().unwrap_or_default(),
            "readMs": summary(&times),
        }));
    }
    Ok(json!({"installs": rows}))
}

/// Nearest-rank percentile of a list of times.
fn percentile(times: &[f64], p: f64) -> f64 {
    let mut sorted = times.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = (p * sorted.len() as f64).ceil() as usize;
    sorted
        .get(rank.saturating_sub(1))
        .copied()
        .unwrap_or_default()
}

fn summary(times: &[f64]) -> Value {
    json!({
        "n": times.len(),
        "median": percentile(times, 0.5),
        "p95": percentile(times, 0.95),
        "max": percentile(times, 1.0),
    })
}

fn numbers(values: &[Value], key: &str) -> Vec<f64> {
    values.iter().filter_map(|v| v[key].as_f64()).collect()
}

#[derive(Clone)]
struct Args {
    one: Option<String>,
    bundle: PathBuf,
    root: PathBuf,
    runs: usize,
    first: usize,
    repeat: usize,
    close: usize,
    evict: Option<PathBuf>,
    out: Option<PathBuf>,
}

fn parse() -> Result<Args, String> {
    let mut args = Args {
        one: None,
        bundle: PathBuf::new(),
        root: PathBuf::from("target/a9/bench"),
        runs: 1,
        first: 5,
        repeat: 20,
        close: 5,
        evict: None,
        out: None,
    };
    let mut given = std::env::args().skip(1);
    while let Some(flag) = given.next() {
        let mut value = || given.next().ok_or_else(|| format!("{flag} needs a value"));
        let number = |text: String| text.parse::<usize>().map_err(|e| format!("{flag}: {e}"));
        match flag.as_str() {
            "--one" => args.one = Some(value()?),
            "--bundle" => args.bundle = PathBuf::from(value()?),
            "--root" => args.root = PathBuf::from(value()?),
            "--runs" => args.runs = number(value()?)?,
            "--first" => args.first = number(value()?)?,
            "--repeat" => args.repeat = number(value()?)?,
            "--close" => args.close = number(value()?)?,
            "--evict" => args.evict = Some(PathBuf::from(value()?)),
            "--out" => args.out = Some(PathBuf::from(value()?)),
            other => return Err(format!("unknown flag {other}")),
        }
    }
    if args.bundle.as_os_str().is_empty() {
        return Err("--bundle is required".to_owned());
    }
    Ok(args)
}

/// Runs this program again for one measurement and reads its one line of JSON.
fn child(args: &Args, mode: &str, root: &Path) -> Result<Value, String> {
    if let Some(script) = &args.evict
        && matches!(mode, "first" | "reads")
    {
        let status = Command::new("python3")
            .arg("-I")
            .arg(script)
            .arg(&args.bundle)
            .output()
            .map_err(|e| format!("{}: {e}", script.display()))?;
        if !status.status.success() {
            return Err(String::from_utf8_lossy(&status.stderr).into_owned());
        }
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let output = Command::new(exe)
        .args(["--one", mode, "--bundle"])
        .arg(&args.bundle)
        .arg("--root")
        .arg(root)
        .args(["--runs", &args.runs.to_string()])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().last().ok_or("no output")?;
    serde_json::from_str(line).map_err(|e| format!("{e}: {line}"))
}

async fn drive(args: Args) -> Result<(), String> {
    let first_root = args.root.join("first");
    let mut firsts = Vec::new();
    for _ in 0..args.first {
        firsts.push(child(&args, "first", &first_root)?);
    }
    // The last first launch left a seeded app data: the repeat launches use it.
    let repeat = Args {
        runs: args.repeat,
        ..args.clone()
    };
    let repeats = child(&repeat, "repeat", &first_root)?;
    let close = Args {
        runs: args.close,
        ..args.clone()
    };
    let closes = child(&close, "close", &args.root.join("close"))?;
    let reads = child(
        &Args {
            runs: 5,
            ..args.clone()
        },
        "reads",
        &args.root,
    )?;

    let repeat_times: Vec<f64> = repeats["startMs"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default();
    let quit_times: Vec<f64> = closes["quitMs"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default();
    let peaks: Vec<f64> = numbers(&firsts, "peakMemoryKib");
    let report = json!({
        "planMs": summary(&numbers(&firsts, "planMs")),
        "firstLaunchToReadyMs": summary(&numbers(&firsts, "readyMs")),
        "repeatLaunchStartMs": summary(&repeat_times),
        "quitDuringSeedMs": summary(&quit_times),
        "peakMemoryKib": summary(&peaks),
        "firstLaunches": firsts,
        "repeatLaunches": repeats,
        "closes": closes,
        "installReads": reads,
    });
    let text = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    if let Some(out) = &args.out {
        std::fs::write(out, format!("{text}\n")).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    println!("{text}");
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    let result = async {
        let args = parse()?;
        match args.one.clone().as_deref() {
            Some("first") => {
                let value = first_launch(&args.root, &args.bundle).await?;
                println!("{value}");
                Ok(())
            }
            Some("repeat") => {
                let value = repeat_launches(&args.root, &args.bundle, args.runs).await?;
                println!("{value}");
                Ok(())
            }
            Some("close") => {
                let value = closes(&args.root, &args.bundle, args.runs).await?;
                println!("{value}");
                Ok(())
            }
            Some("reads") => {
                println!("{}", install_reads(&args.bundle, args.runs)?);
                Ok(())
            }
            Some(other) => Err(format!("unknown mode {other}")),
            None => drive(args).await,
        }
    }
    .await;
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("seedbench: {message}");
            ExitCode::FAILURE
        }
    }
}
