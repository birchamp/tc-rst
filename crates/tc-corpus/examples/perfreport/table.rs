//! The report's rows, statistics and verdicts: pure code, tested with
//! `cargo test -p tc-corpus --example perfreport`.
//!
//! A row has a budget (`docs/PERFORMANCE.md`), a class that says which laptop ratio scales it
//! (`docs/evidence/storage-scale-*/laptop-ratios.json`), and the samples a case measured. The
//! CI verdict (rule 5) judges the raw p95 against a multiple of the budget; the laptop verdict
//! judges the p95 times the class's ratio against the budget, and says "not shown" for a row
//! whose cost is the disk's flush, which no CPU ratio scales.

use crate::ratios::Ratios;
use std::fmt::Write as _;

/// What scales a row from this machine to the reference laptop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Hashing: the laptop has no SHA instructions.
    Sha,
    /// An open with a usable fold cache.
    OpenCache,
    /// An open without one.
    OpenNoCache,
    /// Work on one thread.
    Single,
    /// Work that waits for the drive's flushes: no ratio scales it.
    Disk,
    /// Memory.
    Memory,
    /// A timer's interval (progress within 300 ms): no ratio.
    Timer,
    /// A count: no ratio.
    Count,
    /// Work on several threads that also waits for the drive's flushes (a materialize, a
    /// checkpoint, a close, a seeding): the CPU part scales, the flushes do not.
    Mixed,
}

/// Which statistic of the samples a budget is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stat {
    P95,
    Median,
    Max,
    Value,
}

/// A row's limit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Limit {
    Ms(f64),
    MiB(f64),
    BytesPerChange(f64),
    /// A count that must not exceed the number (0 for "reads no file").
    Count(u64),
    /// A figure with no budget.
    Info,
    /// A budget of the owner's that this binary cannot measure here.
    NotBuilt(&'static str),
}

/// One row of the report, fixed in `rows.rs`.
#[derive(Debug, Clone, Copy)]
pub struct RowSpec {
    pub id: &'static str,
    pub area: &'static str,
    pub action: &'static str,
    pub scale: &'static str,
    pub owner: &'static str,
    pub limit: Limit,
    pub on: Stat,
    pub class: Class,
    /// Run by `perfreport ci`.
    pub ci: bool,
}

/// What a case measured for one row.
#[derive(Debug, Clone)]
pub struct Measured {
    pub id: String,
    /// Milliseconds (or MiB, bytes per change, a count) per run.
    pub samples: Vec<f64>,
    /// What the run did: counters, remarks.
    pub note: String,
    /// Flushes one run waited for, when the case knows (for the split of a mixed row).
    pub fsyncs: Option<f64>,
}

impl Measured {
    pub fn new(id: &str, samples: Vec<f64>, note: impl Into<String>) -> Self {
        Self {
            id: id.to_owned(),
            samples,
            note: note.into(),
            fsyncs: None,
        }
    }

    pub fn with_fsyncs(mut self, fsyncs: f64) -> Self {
        self.fsyncs = Some(fsyncs);
        self
    }
}

pub fn sorted(samples: &[f64]) -> Vec<f64> {
    let mut v: Vec<f64> = samples.iter().copied().filter(|x| !x.is_nan()).collect();
    v.sort_by(|a, b| a.total_cmp(b));
    v
}

/// The sample at the rank `round((n - 1) p)` of the sorted samples: for 20 runs the p95 is the
/// nineteenth, for 5 runs the fifth.
pub fn percentile(samples: &[f64], p: f64) -> f64 {
    let v = sorted(samples);
    if v.is_empty() {
        return f64::NAN;
    }
    v[((v.len() as f64 - 1.0) * p).round() as usize]
}

pub fn median(samples: &[f64]) -> f64 {
    let v = sorted(samples);
    match v.len() {
        0 => f64::NAN,
        n if n % 2 == 1 => v[n / 2],
        n => (v[n / 2 - 1] + v[n / 2]) / 2.0,
    }
}

pub fn p95(samples: &[f64]) -> f64 {
    percentile(samples, 0.95)
}

pub fn max(samples: &[f64]) -> f64 {
    sorted(samples).last().copied().unwrap_or(f64::NAN)
}

/// The figure a budget is on.
pub fn statistic(on: Stat, samples: &[f64]) -> f64 {
    match on {
        Stat::P95 => p95(samples),
        Stat::Median => median(samples),
        Stat::Max => max(samples),
        Stat::Value => samples.last().copied().unwrap_or(f64::NAN),
    }
}

/// A row's verdict.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Pass,
    /// Over the budget but within the tolerance (CI's three times).
    Tolerated,
    Fail,
    NotShown(String),
    Info,
}

impl Verdict {
    pub fn word(&self) -> String {
        match self {
            Self::Pass => "pass".into(),
            Self::Tolerated => "over, within tolerance".into(),
            Self::Fail => "FAIL".into(),
            Self::NotShown(why) => format!("not shown ({why})"),
            Self::Info => "-".into(),
        }
    }
}

fn limit_value(limit: Limit) -> Option<f64> {
    match limit {
        Limit::Ms(v) | Limit::MiB(v) | Limit::BytesPerChange(v) => Some(v),
        Limit::Count(n) => Some(n as f64),
        Limit::Info | Limit::NotBuilt(_) => None,
    }
}

/// CI's verdict (PERFORMANCE.md rule 5): the budget's figure of the raw samples against
/// `tolerance` times the budget. A count is not tolerated: it passes or fails.
pub fn verdict_ci(spec: &RowSpec, samples: &[f64], tolerance: f64) -> Verdict {
    if let Limit::NotBuilt(why) = spec.limit {
        return Verdict::NotShown(why.into());
    }
    let Some(limit) = limit_value(spec.limit) else {
        return Verdict::Info;
    };
    if samples.is_empty() {
        return Verdict::NotShown("not measured".into());
    }
    let value = statistic(spec.on, samples);
    if matches!(spec.limit, Limit::Count(_)) {
        return if value <= limit {
            Verdict::Pass
        } else {
            Verdict::Fail
        };
    }
    if spec.class == Class::Timer {
        // "Within 300 ms": the first report comes at 300 ms or a little after.
        return if value <= limit * 1.1 {
            Verdict::Pass
        } else if value <= limit * tolerance {
            Verdict::Tolerated
        } else {
            Verdict::Fail
        };
    }
    if value <= limit {
        Verdict::Pass
    } else if value <= limit * tolerance {
        Verdict::Tolerated
    } else {
        Verdict::Fail
    }
}

fn ratio_of(class: Class, ratios: &Ratios) -> Option<f64> {
    match class {
        Class::Sha => Some(ratios.sha),
        Class::OpenCache => Some(ratios.open_cache),
        Class::OpenNoCache => Some(ratios.open_nocache),
        Class::Mixed => Some(ratios.multi),
        Class::Single => Some(ratios.single),
        Class::Memory => Some(ratios.memory),
        Class::Timer | Class::Count => Some(1.0),
        Class::Disk => ratios.disk,
    }
}

/// The laptop's figure of a row: this machine's, times the class's ratio. `None` for a row the
/// drive's flush decides until the laptop has measured one. For a mixed row the figure is the
/// CPU part's estimate (the flushes taken out), which is a lower bound.
pub fn laptop_estimate(
    spec: &RowSpec,
    samples: &[f64],
    fsyncs: Option<f64>,
    flush_ms: Option<f64>,
    ratios: &Ratios,
) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    let value = statistic(spec.on, samples);
    match spec.class {
        Class::Disk => ratios.disk.map(|r| value * r),
        Class::Mixed => {
            let flush = fsyncs.zip(flush_ms).map_or(0.0, |(n, f)| n * f);
            let cpu = (value - flush).max(0.0);
            Some(cpu * ratios.multi)
        }
        class => ratio_of(class, ratios).map(|r| value * r),
    }
}

/// The verdict on the laptop: the estimate against the budget. A disk row, or a mixed row,
/// is "not shown" whatever the estimate is, because the flushes of the laptop are unknown; a
/// mixed row whose CPU part alone is over the budget fails.
pub fn verdict_laptop(
    spec: &RowSpec,
    samples: &[f64],
    fsyncs: Option<f64>,
    flush_ms: Option<f64>,
    ratios: &Ratios,
) -> Verdict {
    if let Limit::NotBuilt(why) = spec.limit {
        return Verdict::NotShown(why.into());
    }
    let Some(limit) = limit_value(spec.limit) else {
        return Verdict::Info;
    };
    if samples.is_empty() {
        return Verdict::NotShown("not measured".into());
    }
    if matches!(spec.limit, Limit::Count(_)) {
        return verdict_ci(spec, samples, 1.0);
    }
    match laptop_estimate(spec, samples, fsyncs, flush_ms, ratios) {
        None => Verdict::NotShown("the laptop's flush is not measured".into()),
        Some(estimate) if spec.class == Class::Mixed => {
            if estimate > limit {
                Verdict::Fail
            } else {
                Verdict::NotShown("the laptop's flushes are not measured".into())
            }
        }
        Some(estimate) if spec.class == Class::Timer => {
            if estimate <= limit * 1.1 {
                Verdict::Pass
            } else {
                Verdict::Fail
            }
        }
        Some(estimate) => {
            if estimate <= limit {
                Verdict::Pass
            } else {
                Verdict::Fail
            }
        }
    }
}

/// Three significant figures.
pub fn sig3(v: f64) -> String {
    if v.is_nan() {
        return "-".into();
    }
    if v == 0.0 {
        return "0".into();
    }
    let digits = 2 - v.abs().log10().floor() as i32;
    if digits <= 0 {
        format!("{v:.0}")
    } else {
        format!("{v:.*}", digits as usize)
    }
}

/// How the table is printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Raw p95 against the tolerance (CI).
    Ci(u32),
    /// Estimated against the laptop.
    Laptop,
}

/// The table, one row per line.
pub fn render(
    specs: &[RowSpec],
    measured: &[Measured],
    mode: Mode,
    ratios: &Ratios,
    flush_ms: Option<f64>,
) -> (String, Vec<(String, Verdict)>) {
    let mut out = String::new();
    let mut verdicts = Vec::new();
    let _ = writeln!(
        out,
        "{:<24} {:<46} {:>9} {:>5} {:>9} {:>9} {:>10} {:<30}",
        "id", "action", "budget", "runs", "median", "p95", "laptop est", "verdict"
    );
    for spec in specs {
        let m = measured.iter().find(|m| m.id == spec.id);
        let samples: &[f64] = m.map_or(&[], |m| m.samples.as_slice());
        let fsyncs = m.and_then(|m| m.fsyncs);
        let verdict = match mode {
            Mode::Ci(t) => verdict_ci(spec, samples, f64::from(t)),
            Mode::Laptop => verdict_laptop(spec, samples, fsyncs, flush_ms, ratios),
        };
        let budget = match spec.limit {
            Limit::Ms(v) => format!("{} ms", sig3(v)),
            Limit::MiB(v) => format!("{} MiB", sig3(v)),
            Limit::BytesPerChange(v) => format!("{} B/ch", sig3(v)),
            Limit::Count(n) => format!("<= {n}"),
            Limit::Info => "-".into(),
            Limit::NotBuilt(_) => "not built".into(),
        };
        let estimate = laptop_estimate(spec, samples, fsyncs, flush_ms, ratios)
            .map_or_else(|| "-".to_owned(), sig3);
        let _ = writeln!(
            out,
            "{:<24} {:<46} {:>9} {:>5} {:>9} {:>9} {:>10} {:<30}",
            spec.id,
            truncate(spec.action, 46),
            budget,
            samples.len(),
            if samples.is_empty() {
                "-".into()
            } else {
                sig3(median(samples))
            },
            if samples.is_empty() {
                "-".into()
            } else {
                sig3(p95(samples))
            },
            estimate,
            verdict.word()
        );
        verdicts.push((spec.id.to_owned(), verdict));
    }
    (out, verdicts)
}

/// One tab-separated line per row: everything the table prints and the registry knows.
pub fn tsv(
    specs: &[RowSpec],
    measured: &[Measured],
    mode: Mode,
    ratios: &Ratios,
    flush_ms: Option<f64>,
) -> String {
    let mut out = String::from(
        "id\tarea\taction\tscale\towner\tbudget\tclass\tci\truns\tmedian\tp95\tmax\tlaptop_estimate\tverdict\tnote\n",
    );
    for spec in specs {
        let m = measured.iter().find(|m| m.id == spec.id);
        let samples: &[f64] = m.map_or(&[], |m| m.samples.as_slice());
        let fsyncs = m.and_then(|m| m.fsyncs);
        let verdict = match mode {
            Mode::Ci(t) => verdict_ci(spec, samples, f64::from(t)),
            Mode::Laptop => verdict_laptop(spec, samples, fsyncs, flush_ms, ratios),
        };
        let cell = |v: f64| {
            if v.is_nan() {
                String::new()
            } else {
                format!("{v:.4}")
            }
        };
        let _ = writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}\t{:?}\t{:?}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            spec.id,
            spec.area,
            spec.action,
            spec.scale,
            spec.owner,
            spec.limit,
            spec.class,
            spec.ci,
            samples.len(),
            cell(median(samples)),
            cell(p95(samples)),
            cell(max(samples)),
            laptop_estimate(spec, samples, fsyncs, flush_ms, ratios).map_or_else(String::new, cell),
            verdict.word(),
            m.map_or("", |m| m.note.as_str()).replace(['\t', '\n'], " ")
        );
    }
    out
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        let mut t: String = s.chars().take(n - 1).collect();
        t.push('~');
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(limit: Limit, class: Class) -> RowSpec {
        RowSpec {
            id: "x",
            area: "",
            action: "",
            scale: "",
            owner: "",
            limit,
            on: Stat::P95,
            class,
            ci: true,
        }
    }

    fn ratios() -> Ratios {
        Ratios {
            sha: 6.0,
            open_cache: 1.5,
            open_nocache: 1.9,
            multi: 2.5,
            single: 3.0,
            memory: 1.0,
            disk: None,
            source: "test".into(),
        }
    }

    #[test]
    fn p95_of_twenty_is_the_nineteenth() {
        let samples: Vec<f64> = (1..=20).map(f64::from).collect();
        assert_eq!(p95(&samples), 19.0);
        assert_eq!(median(&samples), 10.5);
        let five: Vec<f64> = (1..=5).map(f64::from).collect();
        assert_eq!(p95(&five), 5.0);
    }

    #[test]
    fn a_row_fails_ci_only_above_three_times_its_budget() {
        let s = spec(Limit::Ms(10.0), Class::Single);
        assert_eq!(verdict_ci(&s, &[9.0; 20], 3.0), Verdict::Pass);
        assert_eq!(verdict_ci(&s, &[25.0; 20], 3.0), Verdict::Tolerated);
        assert_eq!(verdict_ci(&s, &[30.0; 20], 3.0), Verdict::Tolerated);
        assert_eq!(verdict_ci(&s, &[30.1; 20], 3.0), Verdict::Fail);
    }

    #[test]
    fn a_count_is_never_tolerated() {
        let s = spec(Limit::Count(0), Class::Count);
        assert_eq!(verdict_ci(&s, &[0.0], 3.0), Verdict::Pass);
        assert_eq!(verdict_ci(&s, &[1.0], 3.0), Verdict::Fail);
    }

    #[test]
    fn a_disk_row_is_not_shown_without_a_laptop_flush_ratio() {
        let s = spec(Limit::Ms(25.0), Class::Disk);
        assert!(matches!(
            verdict_laptop(&s, &[10.0; 20], None, None, &ratios()),
            Verdict::NotShown(_)
        ));
        let mut with = ratios();
        with.disk = Some(2.0);
        assert_eq!(
            verdict_laptop(&s, &[10.0; 20], None, None, &with),
            Verdict::Pass
        );
        assert_eq!(
            verdict_laptop(&s, &[13.0; 20], None, None, &with),
            Verdict::Fail
        );
    }

    #[test]
    fn a_mixed_row_takes_its_flushes_out_before_scaling() {
        let s = spec(Limit::Ms(1000.0), Class::Mixed);
        // 300 ms here, 11 flushes of 10 ms: a CPU part of 190 ms, 475 ms on the laptop.
        let e = laptop_estimate(&s, &[300.0; 5], Some(11.0), Some(10.0), &ratios());
        assert_eq!(e, Some(475.0));
        // Never called met: the laptop's flushes are unknown.
        assert!(matches!(
            verdict_laptop(&s, &[300.0; 5], Some(11.0), Some(10.0), &ratios()),
            Verdict::NotShown(_)
        ));
        // A CPU part that alone misses is a miss.
        assert_eq!(
            verdict_laptop(&s, &[600.0; 5], Some(11.0), Some(10.0), &ratios()),
            Verdict::Fail
        );
    }

    #[test]
    fn a_class_scales_by_its_own_ratio() {
        let r = ratios();
        let single = spec(Limit::Ms(16.0), Class::Single);
        assert_eq!(
            laptop_estimate(&single, &[4.0; 20], None, None, &r),
            Some(12.0)
        );
        assert_eq!(
            verdict_laptop(&single, &[6.0; 20], None, None, &r),
            Verdict::Fail
        );
        let open = spec(Limit::Ms(3000.0), Class::OpenCache);
        assert_eq!(
            verdict_laptop(&open, &[1900.0; 20], None, None, &r),
            Verdict::Pass
        );
    }

    #[test]
    fn three_significant_figures() {
        assert_eq!(sig3(0.01234), "0.0123");
        assert_eq!(sig3(12.34), "12.3");
        assert_eq!(sig3(1236.4), "1236");
        assert_eq!(sig3(0.0), "0");
    }
}
