//! Every file in `fixtures/check/` has its provenance in the table of
//! `fixtures/check/README.md`. A GitHub table ends at the first line that is not a row, so
//! a row after a blank line renders as a paragraph and is not in the table.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::{fixture, fixture_path};

/// The first cell of each row of the README's first table, without its backticks.
fn table_files(readme: &str) -> Vec<String> {
    let mut lines = readme.lines().skip_while(|l| !l.starts_with('|'));
    let header = lines.next().unwrap();
    assert!(header.starts_with("| File |"), "{header}");
    let separator = lines.next().unwrap();
    assert!(
        separator
            .chars()
            .all(|c| matches!(c, '|' | '-' | ':' | ' ')),
        "{separator}"
    );
    lines
        .take_while(|l| l.starts_with('|'))
        .map(|row| {
            let cell = row.split('|').nth(1).unwrap().trim();
            cell.trim_matches('`').to_owned()
        })
        .collect()
}

#[test]
fn every_check_fixture_has_a_row_in_the_readme_table() {
    let mut files: Vec<String> = std::fs::read_dir(fixture_path("check"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|name| name != "README.md")
        .collect();
    files.sort();
    let mut rows = table_files(&fixture("check/README.md"));
    rows.sort();
    assert_eq!(rows, files);
}

#[test]
fn a_row_after_a_blank_line_is_not_in_the_table() {
    let readme = "| File | What |\n|---|---|\n| `a.json` | A |\n\n| `b.json` | B |\n";
    assert_eq!(table_files(readme), ["a.json"]);
}
