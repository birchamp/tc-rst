//! INV-ARCHITECTURE: the crate graph and the manifests follow `docs/ARCHITECTURE.md`
//! "Dependency rules" and `AGENTS.md`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;

/// The normal dependencies each member may have on other members (ARCHITECTURE.md's crate
/// table). A new edge or a new crate is a change to the architecture: add it here and there.
const EDGES: &[(&str, &[&str])] = &[
    ("tc-core", &[]),
    ("tc-usfm", &["tc-core"]),
    ("tc-versification", &["tc-core"]),
    ("tc-burrito", &["tc-core"]),
    ("tc-helps", &["tc-core"]),
    ("tc-api", &["tc-core"]),
    ("tc-door43", &["tc-core"]),
    ("tc-journal", &["tc-core", "tc-usfm", "tc-burrito"]),
    (
        "tc-check",
        &[
            "tc-core",
            "tc-usfm",
            "tc-versification",
            "tc-helps",
            "tc-burrito",
            "tc-journal",
        ],
    ),
    (
        "tc-align",
        &["tc-core", "tc-usfm", "tc-versification", "tc-journal"],
    ),
    ("tc-obs", &["tc-helps"]),
    ("tc-export", &["tc-obs"]),
    ("tc-ui", &["tc-api", "tc-core"]),
    ("tc-door43-fake", &[]),
    (
        "tc-resources",
        &["tc-core", "tc-door43", "tc-helps", "tc-journal"],
    ),
    (
        "tc-store",
        &[
            "tc-core",
            "tc-usfm",
            "tc-burrito",
            "tc-journal",
            "tc-align",
            "tc-obs",
        ],
    ),
    (
        "tc-corpus",
        &[
            "tc-core",
            "tc-usfm",
            "tc-burrito",
            "tc-journal",
            "tc-versification",
        ],
    ),
    (
        "tc-app",
        &[
            "tc-api",
            "tc-align",
            "tc-check",
            "tc-core",
            "tc-burrito",
            "tc-door43",
            "tc-export",
            "tc-helps",
            "tc-journal",
            "tc-obs",
            "tc-resources",
            "tc-store",
            "tc-usfm",
            "tc-versification",
        ],
    ),
    ("tc-server", &["tc-api"]),
    ("tc4", &["tc-api", "tc-app", "tc-door43", "tc-server"]),
    ("tc-arch", &[]),
];

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

fn normal(dep: &common::Dep) -> bool {
    dep.kind.is_none()
}

#[test]
fn crate_edges_follow_the_architecture_table() {
    let members = common::members();
    let names: BTreeSet<_> = members.iter().map(|m| m.name.clone()).collect();
    let mut problems = Vec::new();
    for m in &members {
        let Some((_, allowed)) = EDGES.iter().find(|(n, _)| *n == m.name) else {
            problems.push(format!(
                "INV-ARCHITECTURE: {} has no row in the edge table; add its row here and its line in ARCHITECTURE.md",
                m.name
            ));
            continue;
        };
        for d in m
            .deps
            .iter()
            .filter(|d| normal(d) && names.contains(&d.name))
        {
            if !allowed.contains(&d.name.as_str()) {
                problems.push(format!(
                    "INV-ARCHITECTURE: {} may depend only on {} (ARCHITECTURE.md \"Dependency rules\"); {} adds {}",
                    m.name,
                    if allowed.is_empty() { "no other member".to_owned() } else { allowed.join(", ") },
                    common::rel(&m.manifest),
                    d.name
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn network_libraries_only_in_door43_and_resources() {
    let mut problems = Vec::new();
    for m in common::members() {
        for d in m.deps.iter().filter(|d| normal(d)) {
            if !["reqwest", "hyper", "hyper-util"].contains(&d.name.as_str()) {
                continue;
            }
            let allowed =
                m.name == "tc-door43" || (m.name == "tc-resources" && d.name == "reqwest");
            if !allowed {
                problems.push(format!(
                    "INV-ARCHITECTURE: only tc-door43 reaches the network (tc-resources's reqwest until DOM-1); {} depends on {}",
                    m.name, d.name
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn git_https_only_in_door43() {
    let mut problems = Vec::new();
    for m in common::members() {
        for d in m.deps.iter().filter(|d| d.name == "git2") {
            if d.features.iter().any(|f| f == "https") && m.name != "tc-door43" {
                problems.push(format!(
                    "INV-ARCHITECTURE: git2's https feature reaches the network around the internet switch; {} enables it",
                    m.name
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn pure_crates_have_no_io_dependency() {
    let io = [
        "tokio",
        "reqwest",
        "hyper",
        "hyper-util",
        "git2",
        "zip",
        "axum",
        "notify",
    ];
    let mut problems = Vec::new();
    for m in common::members()
        .iter()
        .filter(|m| PURE.contains(&m.name.as_str()))
    {
        for d in m
            .deps
            .iter()
            .filter(|d| normal(d) && io.contains(&d.name.as_str()))
        {
            problems.push(format!(
                "INV-PURE-CRATES-DO-NO-IO: {} is pure logic and may not depend on {}",
                m.name, d.name
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn every_member_uses_the_workspace_edition_and_lints() {
    let mut problems = Vec::new();
    for m in common::members() {
        let text = std::fs::read_to_string(&m.manifest).unwrap();
        let (edition, lints) = tc_arch::uses_workspace_edition_and_lints(&text);
        if !edition {
            problems.push(format!(
                "INV-ARCHITECTURE: {} does not take the workspace edition",
                common::rel(&m.manifest)
            ));
        }
        if !lints {
            problems.push(format!(
                "INV-ARCHITECTURE: {} does not take the workspace lints",
                common::rel(&m.manifest)
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn shared_dependency_versions_come_from_the_workspace() {
    let root = std::fs::read_to_string(common::root().join("Cargo.toml")).unwrap();
    let mut shared = Vec::new();
    let mut inside = false;
    for line in root.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t == "[workspace.dependencies]";
            continue;
        }
        if inside && let Some((key, _)) = t.split_once('=') {
            shared.push(key.trim().trim_matches('"').to_owned());
        }
    }
    assert!(!shared.is_empty(), "no [workspace.dependencies] found");
    let mut problems = Vec::new();
    for m in common::members() {
        let text = std::fs::read_to_string(&m.manifest).unwrap();
        for f in tc_arch::own_versions(&common::rel(&m.manifest), &text, &shared) {
            problems.push(format!("INV-ARCHITECTURE (AGENTS.md: shared versions live in [workspace.dependencies]): {}:{} {}", f.path, f.line, f.what));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
