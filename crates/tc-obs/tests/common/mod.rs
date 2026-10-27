// Each test file uses a different subset of these helpers, and test helpers may panic.
#![allow(
    dead_code,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test code: a failed unwrap is a failed test, and not every test binary uses each shared helper"
)]

use std::io::Read;
use std::path::PathBuf;

use md5::{Digest, Md5};

pub fn repo_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

pub fn fixture(rel: &str) -> String {
    let path = repo_path("fixtures").join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub fn md5_hex(text: &str) -> String {
    Md5::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The files of a burrito export in `bundle/en/`, by path inside the zip.
pub fn bundle_file(zip: &str, inner: &str) -> String {
    let path = repo_path("bundle/en").join(zip);
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    let mut text = String::new();
    archive
        .by_name(inner)
        .unwrap_or_else(|e| panic!("{zip}: {inner}: {e}"))
        .read_to_string(&mut text)
        .unwrap();
    text
}

/// The stories of one set, 1 to 50: the template, sample-burrito-obs, or the en_obs v9
/// export.
pub fn stories(set: &str) -> Vec<String> {
    match set {
        "template" => (1..=50)
            .map(|n| {
                fixture(&format!(
                    "tc4/conformance-fixtures/text_stories/ingredients/content/{n:02}.md"
                ))
            })
            .collect(),
        "sample" => (1..=50)
            .map(|n| {
                fixture(&format!(
                    "tc4/sample-burrito-obs/ingredients/content/{n:02}.md"
                ))
            })
            .collect(),
        "en_obs" => {
            let path = repo_path("bundle/en/en_obs_v9.sb.zip");
            let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
            (1..=50)
                .map(|n| {
                    let mut text = String::new();
                    archive
                        .by_name(&format!("en_obs/ingredients/content/{n:02}.md"))
                        .unwrap()
                        .read_to_string(&mut text)
                        .unwrap();
                    text
                })
                .collect()
        }
        other => panic!("no story set {other}"),
    }
}

pub const SETS: [&str; 3] = ["template", "sample", "en_obs"];
