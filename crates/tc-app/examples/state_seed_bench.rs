//! Times STORAGE §13.2's state seed on a parity state (docs/PERFORMANCE.md rule 5, unit B1a).
//! The fixture backend seeds every project of a scenario when `tc4 serve --scenario` starts,
//! so the seed's time is part of that start. Build a state first:
//!
//! ```text
//! node parity/app/state.mjs --scenario sample --out target/b1a/state-sample
//! cargo run --release -p tc-app --features fixture-backend --example state_seed_bench -- \
//!   --state target/b1a/state-sample --runs 20
//! ```
//!
//! For each project folder under `repos/_local_/_local_/` it times one `Tc4Folder::read`, then
//! `--runs` seeds (to the result, passed or refused), and prints one JSON line per project:
//! the outcome, the batches and changes or the mismatches, and the read's time with the
//! seeds' median and p95 in milliseconds.

use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use serde_json::json;
use tc_app::backend::seed::{SeedRefusal, Tc4Folder, fixture_project_id, tc4_state_seed};

fn millis(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

/// The `q`-quantile of sorted samples, by the nearest rank.
fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let rank = (q * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut state: Option<PathBuf> = None;
    let mut runs = 20usize;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--state" => state = args.next().map(PathBuf::from),
            "--runs" => runs = args.next().ok_or("--runs needs a number")?.parse()?,
            other => return Err(format!("unknown argument {other}").into()),
        }
    }
    let state = state.ok_or("--state <a build-state.mjs output folder>")?;
    let projects = state.join("repos/_local_/_local_");
    let mut folders: Vec<PathBuf> = std::fs::read_dir(&projects)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()?;
    folders.sort();
    for dir in folders {
        let start = Instant::now();
        let folder = Tc4Folder::read(&dir)?;
        let read_ms = millis(start);
        let project = fixture_project_id(folder.name());
        let mut times = Vec::with_capacity(runs);
        let mut outcome = json!(null);
        for _ in 0..runs {
            let start = Instant::now();
            let result = tc4_state_seed(&folder, project);
            times.push(millis(start));
            outcome = match result {
                Ok(seed) => json!({
                    "outcome": "passed",
                    "batches": seed.report.counts.batches,
                    "changes": seed.report.counts.changes,
                }),
                Err(SeedRefusal::Mismatches(found)) => json!({
                    "outcome": "refused",
                    "mismatches": found.len(),
                }),
                Err(other) => json!({ "outcome": "refused", "reason": other.to_string() }),
            };
        }
        times.sort_by(f64::total_cmp);
        let line = json!({
            "project": folder.name(),
            "result": outcome,
            "read_ms": read_ms,
            "runs": runs,
            "seed_median_ms": quantile(&times, 0.5),
            "seed_p95_ms": quantile(&times, 0.95),
            "seed_min_ms": times.first(),
            "seed_max_ms": times.last(),
        });
        writeln!(std::io::stdout().lock(), "{line}")?;
    }
    Ok(())
}
