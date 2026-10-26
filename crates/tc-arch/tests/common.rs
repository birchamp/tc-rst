//! The workspace as the architecture tests read it: `cargo metadata` and the files on disk.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test, and not every test binary uses each shared helper"
)]

use std::path::{Path, PathBuf};

pub fn root() -> PathBuf {
    // cargo's own root: canonicalize() gives Windows' `\\?\` form, which the manifest paths
    // cargo reports do not have, so no path would strip.
    PathBuf::from(metadata()["workspace_root"].as_str().unwrap())
}

/// A path relative to the workspace root, with `/` on every platform (the Windows nightly runs
/// these tests too).
pub fn rel(path: &Path) -> String {
    path.strip_prefix(root())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub fn metadata() -> serde_json::Value {
    let out = std::process::Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
        ])
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

pub struct Member {
    pub name: String,
    pub manifest: PathBuf,
    pub deps: Vec<Dep>,
}

pub struct Dep {
    pub name: String,
    /// None for a normal dependency, "dev" or "build" otherwise.
    pub kind: Option<String>,
    pub features: Vec<String>,
}

pub fn members() -> Vec<Member> {
    let meta = metadata();
    meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| Member {
            name: p["name"].as_str().unwrap().to_owned(),
            manifest: PathBuf::from(p["manifest_path"].as_str().unwrap()),
            deps: p["dependencies"]
                .as_array()
                .unwrap()
                .iter()
                .map(|d| Dep {
                    name: d["name"].as_str().unwrap().to_owned(),
                    kind: d["kind"].as_str().map(str::to_owned),
                    features: d["features"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|f| f.as_str().unwrap().to_owned())
                        .collect(),
                })
                .collect(),
        })
        .collect()
}

/// Every `.rs` file under `dir`, sorted.
pub fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// The files a `#[cfg(test)] mod x;` line declares, which hold test code only.
pub fn test_module_files(files: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(f).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            let t = l.trim();
            if let Some(name) = t.strip_prefix("mod ").and_then(|r| r.strip_suffix(';'))
                && i > 0
                && lines[i - 1].trim().starts_with("#[cfg(test)]")
            {
                let dir = f.parent().unwrap();
                let stem = f.file_stem().unwrap();
                let base = if stem == "mod" || stem == "lib" || stem == "main" {
                    dir.to_path_buf()
                } else {
                    dir.join(stem)
                };
                for candidate in [
                    base.join(format!("{name}.rs")),
                    base.join(name).join("mod.rs"),
                ] {
                    if candidate.exists() {
                        out.push(candidate);
                    }
                }
            }
        }
    }
    out
}

/// The product source files of `crate_dir/src`: every `.rs` but the test-only module files.
pub fn product_sources(crate_dir: &Path) -> Vec<PathBuf> {
    let files = rust_files(&crate_dir.join("src"));
    let tests = test_module_files(&files);
    files.into_iter().filter(|f| !tests.contains(f)).collect()
}

/// `path<TAB>...` lines of an allow file, comments and blank lines left out.
pub fn allow_lines(name: &str) -> Vec<Vec<String>> {
    let text = std::fs::read_to_string(root().join("crates/tc-arch/allow").join(name)).unwrap();
    text.lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split('\t').map(str::to_owned).collect())
        .collect()
}
