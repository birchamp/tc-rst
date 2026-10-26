//! INV-PURE-CRATES-DO-NO-IO, INV-ATOMIC-WRITES, INV-ONE-PARSER-OWNER and
//! INV-LOCAL-SERVER-IS-CLOSED, by scanning the product source of every crate.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const PURE: &[&str] = &[
    "tc-core",
    "tc-usfm",
    "tc-versification",
    "tc-helps",
    "tc-journal",
    "tc-check",
    "tc-align",
    "tc-obs",
    "tc-export",
    "tc-api",
];

fn crate_dirs() -> Vec<(String, PathBuf)> {
    common::members()
        .into_iter()
        .map(|m| (m.name, m.manifest.parent().unwrap().to_path_buf()))
        .collect()
}

#[test]
fn pure_crates_do_no_io() {
    let mut problems = Vec::new();
    for (name, dir) in crate_dirs()
        .iter()
        .filter(|(n, _)| PURE.contains(&n.as_str()))
    {
        for file in common::product_sources(dir) {
            let text = std::fs::read_to_string(&file).unwrap();
            for f in tc_arch::scan_io(&common::rel(&file), &text) {
                problems.push(format!(
                    "INV-PURE-CRATES-DO-NO-IO: {}:{} {} ({name} is pure logic)",
                    f.path, f.line, f.what
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn only_listed_files_write_files() {
    let allowed: BTreeMap<String, String> = common::allow_lines("fs_writes.txt")
        .into_iter()
        .map(|l| (l[0].clone(), l.get(1).cloned().unwrap_or_default()))
        .collect();
    assert!(
        allowed.values().all(|r| !r.trim().is_empty()),
        "every allowed file names its reason"
    );
    let mut writers = BTreeSet::new();
    let mut problems = Vec::new();
    for (name, dir) in crate_dirs() {
        if name == "tc-arch" {
            continue;
        }
        for file in common::product_sources(&dir) {
            let text = std::fs::read_to_string(&file).unwrap();
            let path = common::rel(&file);
            let found = tc_arch::scan_writes(&path, &text);
            if found.is_empty() {
                continue;
            }
            writers.insert(path.clone());
            if !allowed.contains_key(&path) {
                for f in found {
                    problems.push(format!(
                        "INV-ATOMIC-WRITES: {}:{} {}; write project files through tc_store::atomic, or list the file with its reason in crates/tc-arch/allow/fs_writes.txt",
                        f.path, f.line, f.what
                    ));
                }
            }
        }
    }
    for path in allowed.keys().filter(|p| !writers.contains(*p)) {
        problems.push(format!("INV-ATOMIC-WRITES: stale entry in crates/tc-arch/allow/fs_writes.txt: {path} writes no file any more"));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn low_level_types_stay_in_their_owning_crate() {
    let dirs = crate_dirs();
    let mut problems = Vec::new();
    for rule in common::allow_lines("imports.txt") {
        let [root, items, owner, readers, why] = &rule[..] else {
            panic!(
                "crates/tc-arch/allow/imports.txt: a rule needs five tab-separated fields: {rule:?}"
            );
        };
        assert!(
            !why.trim().is_empty(),
            "the rule for {root} names its reason"
        );
        let items: Vec<&str> = items.split(',').map(str::trim).collect();
        let readers: Vec<&str> = if readers == "-" {
            Vec::new()
        } else {
            readers.split(',').map(str::trim).collect()
        };
        let mut used_by = BTreeSet::new();
        for (name, dir) in &dirs {
            if name == owner || name == "tc-arch" {
                continue;
            }
            for file in common::product_sources(dir) {
                let text = std::fs::read_to_string(&file).unwrap();
                let path = common::rel(&file);
                let found = if items == ["*"] {
                    let (parent, module) = root.rsplit_once("::").unwrap();
                    tc_arch::scan_names(&path, &text, parent, &[module])
                } else {
                    tc_arch::scan_names(&path, &text, root, &items)
                };
                if found.is_empty() {
                    continue;
                }
                if let Some(reader) = readers.iter().find(|r| **r == name.as_str() || **r == path) {
                    used_by.insert(reader.to_string());
                    continue;
                }
                for f in found {
                    problems.push(format!(
                        "INV-ONE-PARSER-OWNER: {}:{} {}; {root} belongs to {owner}, call its functions instead",
                        f.path, f.line, f.what
                    ));
                }
            }
        }
        for reader in readers.iter().filter(|r| !used_by.contains(**r)) {
            problems.push(format!("INV-ONE-PARSER-OWNER: stale reader in crates/tc-arch/allow/imports.txt: {reader} no longer uses {root}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn every_route_is_behind_guard() {
    let dir = common::root().join("crates/tc-server/src");
    let mut problems = Vec::new();
    let mut routers = 0;
    for file in common::rust_files(&dir) {
        let text = std::fs::read_to_string(&file).unwrap();
        let path = common::rel(&file);
        routers += tc_arch::code_lines(&text)
            .iter()
            .filter(|(_, l)| l.contains("Router::new()"))
            .count();
        if path.ends_with("/lib.rs") {
            for f in tc_arch::scan_routes(&path, &text, "security::guard") {
                problems.push(format!(
                    "INV-LOCAL-SERVER-IS-CLOSED: {}:{} {}",
                    f.path, f.line, f.what
                ));
            }
        }
    }
    if routers != 1 {
        problems.push(format!("INV-LOCAL-SERVER-IS-CLOSED: crates/tc-server/src has {routers} Router::new() calls; only the guarded one may serve"));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
