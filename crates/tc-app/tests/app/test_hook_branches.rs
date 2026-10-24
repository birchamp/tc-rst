//! Source scans (UI-2): release builds and tests run the same business code, and test doubles
//! of the outside jobs live only as ports.

use std::path::{Path, PathBuf};

fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Each file under `src` (as `a/b.rs`) with its text.
fn sources() -> Vec<(String, String)> {
    let root = src();
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    files.sort();
    files
        .into_iter()
        .map(|f| {
            let rel = f
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            (rel, std::fs::read_to_string(&f).unwrap())
        })
        .collect()
}

/// The in-body test branches each file may still hold, and why. The numbers may only fall.
const ALLOWED: &[(&str, usize, &str)] = &[
    (
        "app.rs",
        15,
        "builds the test service's state and its hooks",
    ),
    ("dispatch.rs", 8, "routes the test service's endpoints"),
    (
        "upgrades.rs",
        3,
        "the scripted plans of the upgrade journeys",
    ),
    ("align/suggest.rs", 2, "the suggestion trainer's test pause"),
    ("seed.rs", 2, "the seed job's crash points"),
    ("fixes.rs", 1, "the guided fix's scripted fetch"),
    ("understand.rs", 1, "the open job's test pause"),
];

fn is_branch(line: &str) -> bool {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = line.trim_start();
    indent >= 8
        && rest.starts_with("#[cfg(")
        && (rest.contains("test-hooks") || rest.contains("fixture-backend"))
}

/// INV-RELEASE-AND-TESTS-RUN-THE-SAME-CODE: a `cfg` test branch inside a function body makes a
/// test pass on code a release build never runs.
#[test]
fn no_test_hook_branch_inside_a_business_function() {
    let mut over = Vec::new();
    for (file, text) in sources() {
        let lines: Vec<usize> = text
            .lines()
            .enumerate()
            .filter(|(_, l)| is_branch(l))
            .map(|(i, _)| i + 1)
            .collect();
        let allowed = ALLOWED
            .iter()
            .find(|(f, ..)| *f == file)
            .map_or(0, |(_, n, _)| *n);
        if lines.len() > allowed {
            over.push(format!(
                "src/{file}: {} in-body test branches (lines {lines:?}), {allowed} allowed",
                lines.len()
            ));
        }
    }
    assert!(
        over.is_empty(),
        "INV-RELEASE-AND-TESTS-RUN-THE-SAME-CODE: a test-hook branch inside a function body: {over:#?}"
    );
}

/// Scripted doubles of other services, not of an outside job this scan covers, and why.
const OTHER_DOUBLES: &[(&str, &str, &str)] = &[(
    "upgrades.rs",
    "struct ScriptedResources",
    "the resources service's scripted offers and plans (C8), not sync, share, exchange or import",
)];

/// INV-ONE-PORT-PER-JOB: a scripted double of an outside job exists only as a port.
#[test]
fn scripted_doubles_live_only_in_ports() {
    let found: Vec<String> = sources()
        .into_iter()
        .filter(|(file, _)| !file.starts_with("ports/"))
        .map(|(file, text)| {
            let text = OTHER_DOUBLES
                .iter()
                .filter(|(f, ..)| *f == file)
                .fold(text, |t, (_, name, _)| t.replace(name, "allowed double"));
            (file, text)
        })
        .flat_map(|(file, text)| {
            text.lines()
                .enumerate()
                .filter(|(_, l)| l.contains("struct Scripted"))
                .map(|(i, _)| format!("src/{file}:{}", i + 1))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        found.is_empty(),
        "INV-ONE-PORT-PER-JOB: a scripted double outside src/ports: {found:#?}"
    );
}
