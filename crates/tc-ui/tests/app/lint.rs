//! Source scans for rules no type can hold (UI-1): which code may read a job's state, and
//! that no screen keeps its own attempt counter.

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

/// The text without white space, and for each kept character its line, so a call written
/// over two lines (`.jobs` then `.with_untracked(`) is still found.
fn squeezed(text: &str) -> (String, Vec<usize>) {
    let mut out = String::new();
    let mut lines = Vec::new();
    let mut line = 1;
    for c in text.chars() {
        if c == '\n' {
            line += 1;
        }
        if !c.is_whitespace() {
            out.push(c);
            lines.push(line);
        }
    }
    (out, lines)
}

/// The lines of `name`'s body in `text` (from its `fn` line to its closing brace).
fn fn_lines(text: &str, name: &str) -> std::ops::RangeInclusive<usize> {
    let start = text
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("no fn {name}"));
    let open = start + text[start..].find('{').unwrap();
    let mut depth = 0;
    let mut end = open;
    for (i, c) in text[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    let line = |at: usize| text[..at].matches('\n').count() + 1;
    line(start)..=line(end)
}

/// Each place in `src` where a pattern appears, outside the allowed places: `file:line pattern`.
fn found(patterns: &[&str], allowed: impl Fn(&str, &str, usize) -> bool) -> Vec<String> {
    let root = src();
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    files.sort();
    let mut out = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let rel = file
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let (flat, lines) = squeezed(&text);
        for pattern in patterns {
            let pattern: String = pattern.chars().filter(|c| !c.is_whitespace()).collect();
            for (at, _) in flat.match_indices(&pattern) {
                let line = lines[at];
                if !allowed(&rel, &text, line) {
                    out.push(format!("src/{rel}:{line} {pattern}"));
                }
            }
        }
    }
    out
}

/// INV-ONE-JOB-FOLLOWER: only the follower reads a job's state to see it end. `read_jobs`
/// rereads every active job after a gap in the event stream and follows none.
#[test]
fn only_the_follower_reads_jobs() {
    let readers = found(
        &[
            "js::sleep(50)",
            "jobs::Get {",
            "JobsGet {",
            ".jobs.with_untracked(",
        ],
        |file, text, line| {
            file == "app/web/jobs.rs"
                || (file == "app/web/mod.rs" && fn_lines(text, "read_jobs").contains(&line))
        },
    );
    assert!(
        readers.is_empty(),
        "INV-ONE-JOB-FOLLOWER: a job's state is read outside app/web/jobs.rs: {readers:#?}"
    );
}
