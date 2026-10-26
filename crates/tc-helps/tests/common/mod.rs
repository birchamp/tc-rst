// Each test file uses a different subset of these helpers, and test helpers may panic.
#![allow(
    dead_code,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test, and not every test binary uses each shared helper"
)]

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

/// A text file of the repository: `bundle/en/<export>.zip:<entry>` reads one entry of a
/// burrito export, any other path a file. Paths are relative to the repository root.
pub fn repo_text(source: &str) -> String {
    use std::io::Read;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let Some((zip_path, entry)) = source.split_once(".zip:") else {
        let path = root.join(source);
        return std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    };
    let path = root.join(format!("{zip_path}.zip"));
    let file = std::fs::File::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut archive = zip::ZipArchive::new(file).unwrap_or_else(|e| panic!("{source}: {e}"));
    let mut text = String::new();
    archive
        .by_name(entry)
        .unwrap_or_else(|e| panic!("{source}: {e}"))
        .read_to_string(&mut text)
        .unwrap_or_else(|e| panic!("{source}: {e}"));
    text
}

/// Every `<repo>/ingredients/<BOOK>.tsv` of a burrito export in `bundle/en/`, such as
/// `en_tn_v91.sb.zip`, by book code, in code order.
pub fn export_tables(zip_name: &str) -> Vec<(String, String)> {
    use std::io::Read;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../bundle/en")
        .join(zip_name);
    let file = std::fs::File::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut archive = zip::ZipArchive::new(file).unwrap_or_else(|e| panic!("{zip_name}: {e}"));
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .unwrap_or_else(|e| panic!("{zip_name}: {e}"));
        let name = entry.name().to_owned();
        let parts: Vec<&str> = name.split('/').collect();
        let [_, "ingredients", file] = parts[..] else {
            continue;
        };
        let Some(code) = file.strip_suffix(".tsv") else {
            continue;
        };
        let code = code.to_owned();
        let mut text = String::new();
        entry
            .read_to_string(&mut text)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        out.push((code, text));
    }
    out.sort();
    out
}
