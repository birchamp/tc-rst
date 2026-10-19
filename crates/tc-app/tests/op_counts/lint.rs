//! The lint that keeps whole-fold walks out of tc-app's code (APP-DESIGN §8.1): a count fails
//! only on a path a test runs, the lint on every path, so it must name every public read of
//! `FoldState` that walks the whole project.

/// Every public read of `FoldState` that walks every key, change, book, batch or conflict of
/// a fold, or projects every book.
const WHOLE_FOLD_WALKS: [&str; 13] = [
    "record_keys",
    "frontier",
    "device_stats",
    "derived_files",
    "batch_changes",
    "conflicts",
    "conflicts_of",
    "unreadable_records",
    "retry_unreadable",
    "conflicts_rebuilt",
    "metadata_source",
    "render_book",
    "book_usfm",
];

#[test]
fn the_lint_refuses_every_whole_fold_walk() {
    let config = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/clippy.toml"))
        .expect("tc-app's clippy.toml");
    let refused: Vec<&str> = config
        .lines()
        .filter_map(|line| line.split("path = \"tc_journal::fold::FoldState::").nth(1))
        .filter_map(|rest| rest.split('"').next())
        .collect();
    let missing: Vec<&str> = WHOLE_FOLD_WALKS
        .into_iter()
        .filter(|walk| !refused.contains(walk))
        .collect();
    assert!(missing.is_empty(), "not refused: {missing:?}");
}
