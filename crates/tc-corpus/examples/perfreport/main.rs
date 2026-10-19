//! The performance report of docs/PERFORMANCE.md (unit P): every budget the storage core and
//! the domain crates own, measured end to end on the whole-Bible project with Psalms in use,
//! printed as a table of action, budget, median, p95 and verdict. Not shipped.
//!
//! ```text
//! perfreport setup DIR                 the whole-Bible project (and the bundled resources) under DIR
//! perfreport run DIR [--ratios FILE] [--only a,b] [--rounds N] [--strict]
//! perfreport ci DIR [--tolerance 3]    sets up when DIR is empty, runs the CI rows, exits 1 on a fail
//! perfreport r9 DIR [--only a,b]       saves and reads while background work runs; J8
//! perfreport memory DIR                M2: private bytes a minute after the last action, both heaps
//! perfreport growth DIR                M3: memory against history, 1.2 million changes
//! perfreport years DIR [--days 750] [--open-every 1]   three simulated years
//! perfreport child CASE DIR [args]     one measurement in a process of its own (prints ROW lines)
//! ```
//!
//! Build and run it alone, so the store is the shipped one (no test holds):
//! `cargo run --release -p tc-corpus --example perfreport -- run DIR`.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod align_rows;
mod check_rows;
mod common;
mod machine;
mod memory;
mod r9;
mod ratios;
mod rows;
mod store_rows;
mod table;
mod years;

use common::{collect, merge, run_child};
use ratios::Ratios;
use std::path::{Path, PathBuf};
use table::{Measured, Mode, Verdict};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        usage();
    };
    let dir = args.get(1).map(PathBuf::from).unwrap_or_else(|| usage());
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let flags = Flags::of(&args[2..]);
    let code = match command {
        "setup" => {
            setup(&dir);
            0
        }
        "run" => run(&dir, &flags, false),
        "ci" => run(&dir, &flags, true),
        "r9" => r9_command(&dir, &flags),
        "memory" => memory_command(&dir, &flags),
        "growth" => growth_command(&dir, &flags),
        "years" => years_command(&dir, &flags),
        "child" => {
            // `child CASE DIR`: the case is the second word, the folder the third.
            let case = args[1].clone();
            let dir = PathBuf::from(args.get(2).unwrap_or_else(|| usage()));
            let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
            let rest = Flags::of(&args[3..]);
            common::emit(&child(&case, &dir, &rest));
            0
        }
        _ => usage(),
    };
    std::process::exit(code);
}

fn usage() -> ! {
    eprintln!(
        "perfreport setup DIR | run DIR [--ratios FILE] [--only a,b] [--rounds N] [--strict] | \
         ci DIR [--tolerance 3] | r9 DIR [--only a,b] | memory DIR | growth DIR | years DIR | child CASE DIR"
    );
    std::process::exit(2);
}

#[derive(Default)]
struct Flags {
    ratios: Option<PathBuf>,
    only: Option<Vec<String>>,
    rounds: usize,
    strict: bool,
    tolerance: u32,
    days: u32,
    open_every: u32,
    months: Option<usize>,
}

impl Flags {
    fn of(args: &[String]) -> Self {
        let mut f = Self {
            rounds: 1,
            tolerance: 3,
            days: 750,
            open_every: 1,
            ..Self::default()
        };
        let mut it = args.iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--ratios" => f.ratios = it.next().map(PathBuf::from),
                "--only" => {
                    f.only = it.next().map(|v| v.split(',').map(str::to_owned).collect());
                }
                "--rounds" => f.rounds = it.next().and_then(|v| v.parse().ok()).unwrap_or(1),
                "--tolerance" => f.tolerance = it.next().and_then(|v| v.parse().ok()).unwrap_or(3),
                "--strict" => f.strict = true,
                "--days" => f.days = it.next().and_then(|v| v.parse().ok()).unwrap_or(750),
                "--open-every" => {
                    f.open_every = it.next().and_then(|v| v.parse().ok()).unwrap_or(1)
                }
                "--months" => f.months = it.next().and_then(|v| v.parse().ok()),
                _ => {}
            }
        }
        f
    }

    fn wants(&self, case: &str) -> bool {
        self.only
            .as_ref()
            .is_none_or(|o| o.iter().any(|c| c == case))
    }
}

fn setup(dir: &Path) {
    println!("{}", machine::machine_line());
    std::fs::create_dir_all(dir).expect("the report folder");
    store_rows::setup(dir, &tc_corpus::Spec::whole_bible(1), "bible");
}

/// One case, run in this process (the parent starts it as a child).
fn child(case: &str, dir: &Path, flags: &Flags) -> Vec<Measured> {
    let _ = flags;
    match case {
        "open-cache" => store_rows::open_once(dir, "cache"),
        "open-nocache" => store_rows::open_once(dir, "nocache"),
        "open-cold" => store_rows::open_once(dir, "cold"),
        "editor" => store_rows::editor(dir),
        "materialize" => store_rows::materialize(dir),
        "whole" => store_rows::whole(dir),
        "checkpoint" => store_rows::checkpoint(dir),
        "close" => store_rows::close(dir),
        "flush" => store_rows::flush(dir),
        "r9-materialize" => r9::materialize(dir),
        "r9-whole" => r9::whole(dir),
        "r9-checkpoint" => r9::checkpoint(dir),
        "r9-repack" => r9::repack(dir),
        "r9-merge-proxy" => r9::merge_proxy(dir),
        "r9-consolidation" => r9::consolidation(dir),
        "r9-upgrade" => r9::upgrade(dir),
        "r9-j8" => r9::j8(dir),
        "memory-nocache" => memory::nocache(dir),
        "memory-app" => memory::app_case(dir),
        "align-edit" => align_rows::align_edit(),
        "align-suggest" => align_rows::align_suggest(),
        "check-open" => check_rows::check_open(dir),
        "check-bible" => check_rows::check_bible(dir),
        "years-prepare" => years::prepare(dir, flags.days),
        "years-run" => years::run(dir, flags.open_every, flags.months),
        other => {
            eprintln!("unknown case {other}");
            std::process::exit(2);
        }
    }
}

/// A step of a report: a case, run `repeat` times in children of their own.
struct Step {
    case: &'static str,
    repeat: usize,
    ci: bool,
}

const STEPS: &[Step] = &[
    Step {
        case: "open-cache",
        repeat: 20,
        ci: true,
    },
    Step {
        case: "open-nocache",
        repeat: 5,
        ci: true,
    },
    Step {
        case: "open-cold",
        repeat: 5,
        ci: true,
    },
    Step {
        case: "editor",
        repeat: 1,
        ci: true,
    },
    Step {
        case: "materialize",
        repeat: 1,
        ci: true,
    },
    Step {
        case: "whole",
        repeat: 1,
        ci: true,
    },
    Step {
        case: "checkpoint",
        repeat: 1,
        ci: true,
    },
    Step {
        case: "close",
        repeat: 1,
        ci: true,
    },
    Step {
        case: "flush",
        repeat: 1,
        ci: true,
    },
    Step {
        case: "check-open",
        repeat: 20,
        ci: true,
    },
    Step {
        case: "check-bible",
        repeat: 1,
        ci: true,
    },
];

fn project_exists(dir: &Path) -> bool {
    common::project_folder(dir).join(".tc").exists()
}

fn run(dir: &Path, flags: &Flags, ci: bool) -> i32 {
    println!("{}", machine::machine_line());
    if tc_store::TEST_HOOKS {
        println!("WARNING: tc-store was built with its test holds; build perfreport alone");
    }
    if ci && !project_exists(dir) {
        setup(dir);
    }
    let ratios = match (&flags.ratios, ci) {
        (Some(path), _) => Ratios::read(path).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(2);
        }),
        (None, true) => Ratios::identity("CI: raw figures"),
        (None, false) => Ratios::identity("no ratios given: this machine's figures"),
    };
    let mut measured: Vec<Measured> = Vec::new();
    for round in 0..flags.rounds {
        println!(
            "round {} of {}, load {}",
            round + 1,
            flags.rounds,
            machine::load_average()
        );
        for step in STEPS
            .iter()
            .filter(|s| flags.wants(s.case) && (!ci || s.ci))
        {
            for _ in 0..step.repeat {
                match run_child(step.case, dir, &[]) {
                    Ok(rows) => merge(&mut measured, rows),
                    Err(e) => eprintln!("{e}"),
                }
            }
        }
    }
    report(dir, &measured, &ratios, ci, flags)
}

fn flush_median(measured: &[Measured]) -> Option<f64> {
    measured
        .iter()
        .find(|m| m.id == "flush")
        .map(|m| table::median(&m.samples))
}

fn report(dir: &Path, measured: &[Measured], ratios: &Ratios, ci: bool, flags: &Flags) -> i32 {
    let mode = if ci {
        Mode::Ci(flags.tolerance)
    } else {
        Mode::Laptop
    };
    let (text, verdicts) =
        table::render(rows::ROWS, measured, mode, ratios, flush_median(measured));
    println!("{text}");
    println!("ratios: {}", ratios.source);
    for m in measured.iter().filter(|m| !m.note.is_empty()) {
        println!("  {}: {}", m.id, m.note);
    }
    let results = dir.join("results");
    let _ = std::fs::create_dir_all(&results);
    let stamp = common::run_id();
    let _ = std::fs::write(
        results.join(format!("{stamp}.tsv")),
        table::tsv(rows::ROWS, measured, mode, ratios, flush_median(measured)),
    );
    let json: Vec<serde_json::Value> = measured
        .iter()
        .map(|m| {
            serde_json::json!({"id": m.id, "samples": m.samples, "note": m.note, "fsyncs": m.fsyncs})
        })
        .collect();
    let _ = std::fs::write(
        results.join(format!("{stamp}.json")),
        serde_json::to_string_pretty(&json).unwrap_or_default(),
    );
    let failed: Vec<&str> = verdicts
        .iter()
        .filter(|(_, v)| *v == Verdict::Fail)
        .map(|(id, _)| id.as_str())
        .collect();
    if failed.is_empty() {
        0
    } else {
        println!("rows over the limit: {}", failed.join(", "));
        if ci || flags.strict { 1 } else { 0 }
    }
}

fn r9_command(dir: &Path, flags: &Flags) -> i32 {
    println!("{}", machine::machine_line());
    let cases = [
        "r9-materialize",
        "r9-whole",
        "r9-checkpoint",
        "r9-repack",
        "r9-merge-proxy",
        "r9-consolidation",
        "r9-upgrade",
        "r9-j8",
    ];
    let mut measured = Vec::new();
    for case in cases.iter().filter(|c| flags.wants(c)) {
        match run_child(case, dir, &[]) {
            Ok(rows) => merge(&mut measured, rows),
            Err(e) => eprintln!("{e}"),
        }
    }
    let _ = collect;
    let ratios = Ratios::read_or_identity(flags.ratios.as_deref());
    report(dir, &measured, &ratios, false, flags)
}

/// Both heaps: glibc's default (what a Linux user has) and one arena (`MALLOC_ARENA_MAX=1`,
/// the figure the laptop's allocator is compared with: the review measured it at 0.97 to 1.07
/// of the one-heap figure).
const HEAPS: [(&str, Option<&str>); 2] = [("one heap", Some("1")), ("default heap", None)];

fn memory_rows(dir: &Path, project: &str) -> Vec<Measured> {
    let mut rows = Vec::new();
    for (_, arenas) in HEAPS {
        let mut envs: Vec<(&str, &str)> = vec![("PERFREPORT_PROJECT", project)];
        if let Some(n) = arenas {
            envs.push(("MALLOC_ARENA_MAX", n));
        }
        for case in ["memory-nocache", "memory-app"] {
            match common::run_child_env(case, dir, &[], &envs) {
                Ok(more) => merge(&mut rows, more),
                Err(e) => eprintln!("{e}"),
            }
        }
    }
    rows
}

fn memory_command(dir: &Path, flags: &Flags) -> i32 {
    println!("{}", machine::machine_line());
    let rows = memory_rows(dir, "bible");
    let ratios = Ratios::read_or_identity(flags.ratios.as_deref());
    report(dir, &rows, &ratios, false, flags)
}

/// M3: steady memory of the whole Bible at today's history and at twice it (what
/// `Spec::whole_bible(1).with_history(750, 2.0)` makes: about 1.2 million changes), per added
/// change.
fn growth_command(dir: &Path, flags: &Flags) -> i32 {
    println!("{}", machine::machine_line());
    let twice = common::project_folder_named(dir, "bible-2x");
    if !twice.join(".tc").exists() {
        store_rows::setup(
            dir,
            &tc_corpus::Spec::whole_bible(1).with_history(750, 2.0),
            "bible-2x",
        );
    }
    let one = memory_rows(dir, "bible");
    let two = memory_rows(dir, "bible-2x");
    let mut rows = one.clone();
    for (label, suffix) in [("one heap", ""), ("default heap", "_default_heap")] {
        let steady = |rows: &[Measured]| {
            rows.iter()
                .find(|m| m.id == format!("mem.steady{suffix}"))
                .and_then(|m| m.samples.first().copied())
        };
        let changes = |rows: &[Measured]| {
            rows.iter()
                .filter(|m| m.id == "mem.changes")
                .map(|m| m.samples[0])
                .fold(0.0, f64::max)
        };
        let (Some(a), Some(b)) = (steady(&one), steady(&two)) else {
            continue;
        };
        let added = changes(&two) - changes(&one);
        let bytes = (b - a) * 1024.0 * 1024.0 / added.max(1.0);
        rows.push(Measured::new(
            &format!("mem.growth{suffix}"),
            vec![bytes],
            format!(
                "{label}: {a:.0} MiB at {:.0} changes, {b:.0} MiB at {:.0}: {bytes:.0} bytes for each of the {added:.0} added changes",
                changes(&one),
                changes(&two)
            ),
        ));
    }
    let ratios = Ratios::read_or_identity(flags.ratios.as_deref());
    report(dir, &rows, &ratios, false, flags)
}

/// Three simulated years: the days are prepared once in a child (it holds the generated
/// corpus), then played into a project in a child of their own.
fn years_command(dir: &Path, flags: &Flags) -> i32 {
    println!("{}", machine::machine_line());
    let days = flags.days.to_string();
    let prepared = dir.join("years/prepared.json");
    let have = std::fs::read_to_string(&prepared)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .is_some_and(|v| v["days"].as_u64() == Some(u64::from(flags.days)));
    let mut rows = Vec::new();
    if !have {
        match run_child("years-prepare", dir, &["--days", &days]) {
            Ok(more) => merge(&mut rows, more),
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        }
    }
    let mut args: Vec<String> = vec!["--open-every".into(), flags.open_every.to_string()];
    if let Some(n) = flags.months {
        args.push("--months".into());
        args.push(n.to_string());
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match run_child("years-run", dir, &args) {
        Ok(more) => merge(&mut rows, more),
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    }
    let ratios = Ratios::read_or_identity(flags.ratios.as_deref());
    let failed = rows
        .iter()
        .any(|m| m.id == "years.failures" && m.samples.first().is_some_and(|n| *n > 0.0));
    let code = report(dir, &rows, &ratios, false, flags);
    if failed { 1 } else { code }
}
