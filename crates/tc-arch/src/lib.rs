//! Pure scans of source and manifest text for the architecture rules of
//! `docs/ARCHITECTURE.md` and `AGENTS.md`. The tests in `tests/` read the files and run these.

/// One place a rule is broken: a file, a 1-based line and what was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: String,
    pub line: usize,
    pub what: String,
}

/// The lines of `text` that are product code: comment lines, and the bodies of inline
/// `#[cfg(test)] mod x { ... }` blocks, are left out.
pub fn code_lines(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut test_attr = false;
    let mut skip_depth: Option<i64> = None;
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if let Some(depth) = skip_depth.as_mut() {
            *depth += braces(line);
            if *depth <= 0 {
                skip_depth = None;
            }
            continue;
        }
        if trimmed.starts_with("//") {
            continue;
        }
        if trimmed.starts_with("#[cfg(test)]") {
            test_attr = true;
            continue;
        }
        if test_attr && trimmed.starts_with("mod ") && trimmed.contains('{') {
            let depth = braces(line);
            if depth > 0 {
                skip_depth = Some(depth);
            }
            test_attr = false;
            continue;
        }
        if !trimmed.starts_with("#[") {
            test_attr = false;
        }
        out.push((i + 1, line));
    }
    out
}

fn braces(line: &str) -> i64 {
    line.chars()
        .map(|c| match c {
            '{' => 1,
            '}' => -1,
            _ => 0,
        })
        .sum()
}

/// Whether `needle` occurs in `hay` with no identifier character just before it.
fn has_word(hay: &str, needle: &str) -> bool {
    hay.match_indices(needle).any(|(at, _)| {
        !hay[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

/// The names inside every `use <root>::{ ... }` group of `code`, across lines, top level only.
fn use_group_names(code: &str, root: &str) -> Vec<String> {
    let mut names = Vec::new();
    let opener = format!("{root}::{{");
    let mut rest = code;
    while let Some(at) = rest.find(&opener) {
        let body = &rest[at + opener.len()..];
        let mut depth = 1;
        let mut end = body.len();
        for (i, c) in body.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i;
                        break;
                    }
                }
                _ => {}
            }
        }
        let mut top = String::new();
        let mut d = 0;
        for c in body[..end].chars() {
            match c {
                '{' => d += 1,
                '}' => d -= 1,
                _ if d == 0 => top.push(c),
                _ => {}
            }
        }
        names.extend(
            top.split(',')
                .map(|n| n.trim().split("::").next().unwrap_or("").trim().to_owned())
                .filter(|n| !n.is_empty()),
        );
        rest = &body[end.min(body.len())..];
    }
    names
}

const IO_MARKS: [&str; 9] = [
    "std::fs",
    "std::net",
    "std::process",
    "std::env::var",
    "env::var(",
    "tokio::",
    "reqwest",
    "SystemTime::now",
    "Instant::now",
];

/// INV-PURE-CRATES-DO-NO-IO: the I/O, process, environment and clock uses in `text`.
pub fn scan_io(path: &str, text: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    for (line, code) in code_lines(text) {
        for mark in IO_MARKS {
            if has_word(code, mark) {
                out.push(Finding {
                    path: path.to_owned(),
                    line,
                    what: format!("uses {}", mark.trim_end_matches('(')),
                });
            }
        }
    }
    let code: String = code_lines(text)
        .iter()
        .map(|(_, l)| *l)
        .collect::<Vec<_>>()
        .join("\n");
    for name in use_group_names(&code, "std") {
        if ["fs", "net", "process", "env"].contains(&name.as_str()) {
            out.push(Finding {
                path: path.to_owned(),
                line: 0,
                what: format!("uses std::{name} (in a use group)"),
            });
        }
    }
    out
}

const WRITERS: [&str; 10] = [
    "write",
    "rename",
    "remove_file",
    "remove_dir",
    "remove_dir_all",
    "copy",
    "create_dir",
    "create_dir_all",
    "hard_link",
    "set_permissions",
];

/// INV-ATOMIC-WRITES: the filesystem mutators `text` calls.
pub fn scan_writes(path: &str, text: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    for (line, code) in code_lines(text) {
        for name in WRITERS {
            if has_word(code, &format!("fs::{name}(")) {
                out.push(Finding {
                    path: path.to_owned(),
                    line,
                    what: format!("calls fs::{name}"),
                });
            }
        }
        for call in ["File::create", "File::options(", "OpenOptions::new("] {
            if has_word(code, call) {
                out.push(Finding {
                    path: path.to_owned(),
                    line,
                    what: format!("calls {}", call.trim_end_matches('(')),
                });
            }
        }
    }
    out
}

/// INV-ONE-PARSER-OWNER: where `text` names one of `items` of `root` (a crate path such as
/// `tc_usfm`), by a direct path or in a `use root::{...}` group.
pub fn scan_names(path: &str, text: &str, root: &str, items: &[&str]) -> Vec<Finding> {
    let mut out = Vec::new();
    let lines = code_lines(text);
    for (line, code) in &lines {
        for item in items {
            if has_word(code, &format!("{root}::{item}")) {
                let next = code
                    .split(&format!("{root}::{item}"))
                    .nth(1)
                    .and_then(|s| s.chars().next());
                if next.is_none_or(|c| !(c.is_alphanumeric() || c == '_')) {
                    out.push(Finding {
                        path: path.to_owned(),
                        line: *line,
                        what: format!("names {root}::{item}"),
                    });
                }
            }
        }
    }
    let code = lines.iter().map(|(_, l)| *l).collect::<Vec<_>>().join("\n");
    for name in use_group_names(&code, root) {
        if items.contains(&name.as_str()) {
            out.push(Finding {
                path: path.to_owned(),
                line: 0,
                what: format!("names {root}::{name} (in a use group)"),
            });
        }
    }
    out
}

/// INV-LOCAL-SERVER-IS-CLOSED: in the router's source, a `.route(` after the guard layer, or a
/// number of `Router::new()` other than one.
pub fn scan_routes(path: &str, text: &str, guard: &str) -> Vec<Finding> {
    let lines = code_lines(text);
    let mut out = Vec::new();
    let routers: Vec<_> = lines
        .iter()
        .filter(|(_, l)| l.contains("Router::new()"))
        .collect();
    if routers.len() != 1 {
        out.push(Finding {
            path: path.to_owned(),
            line: routers.first().map_or(0, |(n, _)| *n),
            what: format!(
                "has {} Router::new() calls; the guard covers exactly one router",
                routers.len()
            ),
        });
    }
    let Some((guard_line, _)) = lines
        .iter()
        .find(|(_, l)| l.contains(".layer(") && l.contains(guard))
    else {
        out.push(Finding {
            path: path.to_owned(),
            line: 0,
            what: format!("has no .layer(...) with {guard}"),
        });
        return out;
    };
    for (line, code) in &lines {
        if line > guard_line
            && (code.contains(".route(")
                || code.contains(".fallback(")
                || code.contains(".merge(")
                || code.contains(".nest("))
        {
            out.push(Finding {
                path: path.to_owned(),
                line: *line,
                what: format!("adds a route after the {guard} layer (line {guard_line}), so the guard never sees it"),
            });
        }
    }
    out
}

/// The dependency names a manifest gives its own `version`, in any dependency section, among
/// `shared` (the names `[workspace.dependencies]` lists).
pub fn own_versions(path: &str, manifest: &str, shared: &[String]) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut in_deps = false;
    for (i, line) in manifest.lines().enumerate() {
        let t = line.trim();
        if t.starts_with('[') {
            in_deps = t.ends_with("dependencies]");
            continue;
        }
        if !in_deps || t.starts_with('#') || t.is_empty() {
            continue;
        }
        let Some((key, value)) = t.split_once('=') else {
            continue;
        };
        let name = key
            .trim()
            .split('.')
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches('"');
        if shared.iter().any(|s| s == name)
            && !key.contains("workspace")
            && !value.contains("workspace")
        {
            out.push(Finding {
                path: path.to_owned(),
                line: i + 1,
                what: format!("gives {name} its own version; use `{name}.workspace = true`"),
            });
        }
    }
    out
}

/// Whether a manifest takes the workspace edition and lints.
pub fn uses_workspace_edition_and_lints(manifest: &str) -> (bool, bool) {
    let edition = manifest.lines().any(|l| {
        let t = l.trim().replace(' ', "");
        t == "edition.workspace=true" || t == "edition={workspace=true}"
    });
    let mut in_lints = false;
    let mut lints = false;
    for l in manifest.lines() {
        let t = l.trim().replace(' ', "");
        if t.starts_with('[') {
            in_lints = t == "[lints]";
            if t == "[lints]" || t == "lints.workspace=true" {
                continue;
            }
        }
        if (in_lints && t == "workspace=true") || t == "lints.workspace=true" {
            lints = true;
        }
    }
    (edition, lints)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_io_scan_finds_a_planted_std_fs_call() {
        let found = scan_io("x.rs", "fn f() {\n    let _ = std::fs::read(\"x\");\n}\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].line, found[0].what.as_str()), (2, "uses std::fs"));
        let grouped = scan_io(
            "x.rs",
            "use std::{\n    collections::BTreeMap,\n    fs,\n};\n",
        );
        assert_eq!(grouped.len(), 1, "{grouped:?}");
    }

    #[test]
    fn the_io_scan_skips_a_cfg_test_module() {
        let text = "fn f() {}\n#[cfg(test)]\nmod tests {\n    fn g() { let _ = std::time::Instant::now(); }\n}\nfn h() {}\n";
        assert!(scan_io("x.rs", text).is_empty());
        assert!(scan_io("x.rs", "// std::fs in a comment\n").is_empty());
    }

    #[test]
    fn the_write_scan_finds_a_planted_rename() {
        let found = scan_writes("x.rs", "fn f() {\n    std::fs::rename(a, b).unwrap();\n}\n");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].what, "calls fs::rename");
        assert!(
            scan_writes("x.rs", "fn f() { vfs::write(a); }\n").is_empty(),
            "vfs:: is not fs::"
        );
    }

    #[test]
    fn the_name_scan_finds_a_type_in_a_use_group() {
        let text = "use tc_usfm::{\n    parse,\n    Node,\n};\nfn f(n: tc_usfm::Document) {}\nfn g() { tc_usfm::NodeKind; }\n";
        let found = scan_names("x.rs", text, "tc_usfm", &["Document", "Node"]);
        let whats: Vec<_> = found.iter().map(|f| f.what.as_str()).collect();
        assert_eq!(
            whats,
            [
                "names tc_usfm::Document",
                "names tc_usfm::Node (in a use group)"
            ]
        );
    }

    #[test]
    fn the_route_scan_finds_a_route_after_the_guard() {
        let ok = "Router::new()\n    .route(\"/\", get(a))\n    .layer(from_fn(guard))\n";
        assert!(scan_routes("lib.rs", ok, "guard").is_empty());
        let late = "Router::new()\n    .layer(from_fn(guard))\n    .route(\"/x\", get(b))\n";
        assert_eq!(scan_routes("lib.rs", late, "guard").len(), 1);
    }

    #[test]
    fn a_shared_dependency_with_its_own_version_is_found() {
        let manifest = "[dependencies]\nserde.workspace = true\nfutures-util = { version = \"0.3\" }\n[dev-dependencies]\nproptest = { workspace = true }\n";
        let shared = [
            "serde".to_owned(),
            "futures-util".to_owned(),
            "proptest".to_owned(),
        ];
        let found = own_versions("Cargo.toml", manifest, &shared);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].line, 3);
    }
}
