// Each test file uses a different subset of these helpers, and test helpers may panic.
#![allow(dead_code, clippy::panic)]

use std::path::PathBuf;

pub fn fixture_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(rel)
}

pub fn fixture(rel: &str) -> String {
    let path = fixture_path(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A Door43 resource fixture from tC4: `resource("en_tn@v86/TIT.tsv")`.
pub fn resource(rel: &str) -> String {
    fixture(&format!("tc4/test-fixtures/resources/{rel}"))
}

pub fn aquifer(rel: &str) -> String {
    fixture(&format!("aquifer/{rel}"))
}
