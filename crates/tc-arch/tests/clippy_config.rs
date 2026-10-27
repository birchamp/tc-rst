//! INV-PROVEN-PANICS and INV-NO-BAGS-OF-BOOLEANS hold in every crate: clippy reads only the
//! clippy.toml nearest a crate, so a crate's own file must repeat the root's settings.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use std::collections::BTreeMap;
use std::path::Path;

/// Each crate lists the methods and types it forbids for its own reasons.
const OWN_TO_EACH_CRATE: &[&str] = &["disallowed-methods", "disallowed-types"];

/// The `key = value` lines at the top level; an array's continuation lines start with a space,
/// `{`, `]` or `#`, so they never match.
fn top_level_keys(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter(|line| line.starts_with(|c: char| c.is_ascii_lowercase()))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect()
}

#[test]
fn every_crate_clippy_toml_keeps_the_root_keys() {
    let root = common::root();
    let read = |path: &Path| std::fs::read_to_string(path).unwrap();
    let wanted: Vec<(String, String)> = top_level_keys(&read(&root.join("clippy.toml")))
        .into_iter()
        .filter(|(key, _)| !OWN_TO_EACH_CRATE.contains(&key.as_str()))
        .collect();
    assert!(
        wanted.iter().any(|(key, _)| key == "allow-unwrap-in-tests"),
        "the root clippy.toml parsed to {wanted:?}"
    );
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(root.join("crates")).unwrap() {
        let dir = entry.unwrap().path();
        let path = dir.join("clippy.toml");
        if !path.exists() {
            continue;
        }
        let have = top_level_keys(&read(&path));
        for (key, value) in &wanted {
            if have.get(key) != Some(value) {
                missing.push(format!(
                    "INV-PROVEN-PANICS: {} replaces the root clippy.toml but lacks {key} = {value}",
                    common::rel(&path)
                ));
            }
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}
