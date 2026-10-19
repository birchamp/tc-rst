//! The machine a figure was measured on (PERFORMANCE.md rule 5: the CPU, whether it has SHA
//! and AVX-512 instructions, the commit), read without privileges on Linux and Windows.

use std::process::Command;

/// Whether the CPU has SHA instructions (the reference laptop has none).
pub fn has_sha() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("sha")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

pub fn has_avx512() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("avx512f")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

fn avx2() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

pub fn load_average() -> String {
    std::fs::read_to_string("/proc/loadavg").map_or_else(
        |_| "unavailable".into(),
        |s| s.split_whitespace().take(3).collect::<Vec<_>>().join(" "),
    )
}

/// The CPUs this process may run on (`taskset` restricts them), Linux only.
fn allowed_cpus() -> String {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("Cpus_allowed_list:"))
                .map(|l| l.trim_start_matches("Cpus_allowed_list:").trim().to_owned())
        })
        .unwrap_or_else(|| "unavailable".into())
}

pub fn commit() -> String {
    Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map_or_else(
            || "unknown".into(),
            |o| String::from_utf8_lossy(&o.stdout).trim().to_owned(),
        )
}

/// One line for the top of every report and every case.
pub fn machine_line() -> String {
    let mut system = sysinfo::System::new();
    system.refresh_cpu_list(sysinfo::CpuRefreshKind::nothing());
    system.refresh_memory();
    let cpu = system
        .cpus()
        .first()
        .map_or_else(|| "unknown CPU".to_owned(), |c| c.brand().trim().to_owned());
    format!(
        "machine: {cpu}, {} logical CPUs (allowed {}), sha {}, avx2 {}, avx512f {}, {:.1} GiB, \
         {} {}, load {}, RAYON_NUM_THREADS={}, MALLOC_ARENA_MAX={}, test-hooks {}, commit {}",
        std::thread::available_parallelism().map_or(0, |n| n.get()),
        allowed_cpus(),
        has_sha(),
        avx2(),
        has_avx512(),
        system.total_memory() as f64 / (1u64 << 30) as f64,
        sysinfo::System::name().unwrap_or_default(),
        sysinfo::System::os_version().unwrap_or_default(),
        load_average(),
        std::env::var("RAYON_NUM_THREADS").unwrap_or_else(|_| "unset".into()),
        std::env::var("MALLOC_ARENA_MAX").unwrap_or_else(|_| "unset".into()),
        tc_store::TEST_HOOKS,
        commit(),
    )
}

/// Private bytes of this process in MiB: Linux `Private_Clean + Private_Dirty` of
/// `smaps_rollup`; `None` elsewhere (the Windows runner samples the private working set from
/// outside at each `MARK` line).
pub fn private_mib() -> Option<f64> {
    let rollup = std::fs::read_to_string("/proc/self/smaps_rollup").ok()?;
    let kb = |field: &str| -> f64 {
        rollup
            .lines()
            .find(|l| l.starts_with(field))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    Some((kb("Private_Clean:") + kb("Private_Dirty:")) / 1024.0)
}

/// A `/proc/self/status` field in MiB (`VmHWM:`, `RssAnon:`, `VmRSS:`).
pub fn status_mib(field: &str) -> Option<f64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find(|l| l.starts_with(field))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|kb| kb.parse::<f64>().ok())
        .map(|kb| kb / 1024.0)
}

/// Prints `MARK name` and waits, so the Windows runner can read the process's private working
/// set from outside; on Linux the figure is read here.
#[allow(dead_code)]
pub fn mark(name: &str) -> Option<f64> {
    if let Some(mib) = private_mib() {
        return Some(mib);
    }
    println!("MARK {name}");
    std::thread::sleep(std::time::Duration::from_secs(3));
    None
}
