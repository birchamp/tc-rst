// Shared test helpers. Each test binary uses some of them, and a failed unwrap here is a
// broken fixture, which should fail the test.
#![allow(dead_code, clippy::unwrap_used)]

use serde_json::Value;
use std::path::PathBuf;

pub fn fixture(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/tc4")
        .join(relative)
}

pub fn bible_dir() -> PathBuf {
    fixture("sample-burrito")
}

pub fn obs_dir() -> PathBuf {
    fixture("sample-burrito-obs")
}

pub fn read_metadata(dir: &std::path::Path) -> Value {
    let bytes = std::fs::read(dir.join("metadata.json")).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Copies a project folder, so a test can change files without touching the fixtures.
pub fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}
