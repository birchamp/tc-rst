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

/// INV-A-STALE-REPLY-NEVER-SHOWS: screens use `Latest`, never a counter of their own. Allowed:
/// id sources (fix.rs `NEXT_ID`, mod.rs `next_listener`, saves.rs `next_hook`), mod.rs's
/// `state_frames` (a count of applied frames that dates a `hello`), and app_ctx.rs's
/// `sign_in_flows` (the flow id the sign-in step keeps in its own state).
#[test]
fn no_hand_made_attempt_counters() {
    let allowed = [
        ("app/web/fix.rs", "NEXT_ID"),
        ("app/web/mod.rs", "next_listener"),
        ("app/web/mod.rs", "state_frames"),
        ("app/web/saves.rs", "next_hook"),
        ("app/web/app_ctx.rs", "sign_in_flows"),
    ];
    let counters = found(&["Cell<u64>"], |file, text, line| {
        let at = text.lines().nth(line - 1).unwrap_or("");
        !file.starts_with("app/web/")
            || allowed
                .iter()
                .any(|(f, name)| *f == file && at.contains(name))
    });
    assert!(
        counters.is_empty(),
        "INV-A-STALE-REPLY-NEVER-SHOWS: a hand-made attempt counter; use app::latest::Latest: {counters:#?}"
    );
}

/// INV-ONE-LEAVE-PATH (UI-3): every move away from a screen or a project runs `leave`, which
/// runs the save gate, then the checkpoint, then the switch. Allowed: `leave` itself; saves.rs,
/// the gate and the window-close guard (D1's quit flow takes that over); the test-hooks probe
/// page (mod.rs `ProbeRoot`, probe.rs), which drives the gate directly to test it; and the three
/// *Restore this version* of History (HIST-1), which save every field before writing over the
/// record, so a save in flight never lands over the restore (owner ruling H5).
#[test]
fn only_leave_calls_the_gate() {
    let callers = found(&[".gate()"], |file, text, line| {
        file == "app/web/leave.rs"
            || file == "app/web/saves.rs"
            || file == "app/web/probe.rs"
            || (file == "app/web/mod.rs" && fn_lines(text, "ProbeRoot").contains(&line))
            || (file == "app/web/check.rs" && fn_lines(text, "restore_check").contains(&line))
            || (file == "app/web/align.rs" && fn_lines(text, "restore_alignment").contains(&line))
            || (file == "views/comment_box.rs"
                && fn_lines(text, "comment_history").contains(&line))
    });
    assert!(
        callers.is_empty(),
        "INV-ONE-LEAVE-PATH: the save gate is called outside Actions::leave: {callers:#?}"
    );
}

/// INV-NET-REFUSALS-HANDLED-ONCE (UI-3): the internet question and the sign-in step are asked
/// for in one function, `Actions::net_refusal` in app_ctx.rs, which `run_net` and the job ends
/// call; no screen matches those refusals itself.
#[test]
fn only_run_net_handles_net_refusals() {
    let handlers = found(
        &["ApiError::NeedsInternet", "ApiError::NeedsSignIn"],
        |file, text, line| {
            !file.starts_with("app/web/")
                || (file == "app/web/app_ctx.rs" && fn_lines(text, "net_refusal").contains(&line))
        },
    );
    assert!(
        handlers.is_empty(),
        "INV-NET-REFUSALS-HANDLED-ONCE: a network refusal is handled outside Actions::net_refusal: {handlers:#?}"
    );
}

/// INV-EVERY-REFUSAL-RENDERS (UI-3): a refusal is put into words by `i18n::refusal_text` alone,
/// which fills every placeholder; no screen reads a refusal's values itself.
#[test]
fn only_refusal_text_reads_vars() {
    let readers = found(&[".vars()"], |file, _, _| file == "i18n.rs");
    assert!(
        readers.is_empty(),
        "INV-EVERY-REFUSAL-RENDERS: a refusal's values are read outside i18n::refusal_text: {readers:#?}"
    );
}
