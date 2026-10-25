//! The lint that keeps whole-fold walks out of tc-app's code (APP-DESIGN §8.1): a count fails
//! only on a path a test runs, the lint on every path, so it must name every public read of
//! `FoldState` that walks the whole project, and no file of the library may turn it off but
//! the two that read a whole fold on purpose.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tc_app::backend::ProjectStore;
use tc_app::backend::memory::MemoryBackend;
use tc_app::counters::{AppCounters, AppCounts, CountingSnapshot};
use tc_journal::fold::DerivedFile;
use tc_journal::{BookId, ToolId};

use crate::common::{folder, open_progress, sample_bible};

/// Every public read of `FoldState` that walks every key, change, book, batch or conflict of
/// a fold, or projects a whole book, for everything it is asked or only for some of it.
const WHOLE_FOLD_WALKS: [&str; 23] = [
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
    // A book's text projects the book; `conflicts.json` walks every conflict.
    "render",
    "scope_digest",
    "scope_digests",
    // S2: a book's parts and digests project the book; the carried versions, the metadata
    // heads and an outside edit's plan or undo walk the records of a whole file.
    "book_parts",
    "render_book_with_digests",
    "render_with_digests",
    "carried_versions",
    "metadata_heads",
    "plan_outside_edit",
    "undo_outside",
];

/// Every other public method of `FoldState`, by what it costs at most.
const COSTS_LESS: &[(&str, &[&str])] = &[
    (
        "a number the fold keeps",
        &[
            "len",
            "is_empty",
            "counters",
            "conflict_count",
            "conflict_counts",
            "values_unreadable",
        ],
    ),
    (
        "one record or one change",
        &[
            "key_state",
            "heads",
            "projected",
            "projected_change",
            "conflict",
            "change",
            "body",
            "raw",
            "history",
            // The moves into one check: what it touches, never the project (H5).
            "moved_into",
            "shown_heads",
            "review_token",
            "token",
            "placement",
            "change_at",
            "holds",
            "shown_value",
        ],
    ),
    (
        "one chapter, one book, one story, or the stories of an OBS project",
        &[
            "chapters_of",
            "chapter_keys",
            "decisions_of",
            "alignments_of",
            "comments_of",
            "story_decisions",
            "story_comments_of",
            "book_keys",
            "book_heads_seen",
            "book_files",
            "projects_file",
            "chapter_usfm",
            "check_texts",
            "check_texts_at",
            "decisions",
            "decisions_at",
            "file_heads",
            "scope_keys",
            "keep_removed",
        ],
    ),
    (
        "the books, the devices, the project's details, or a short list",
        &[
            "book",
            "books",
            "projected_books",
            "person_labels",
            "anomalies",
            "detached",
            "carried_files",
            "metadata_source_nfc",
            "metadata_scope_digest",
        ],
    ),
    (
        "one conflict, or the conflicts the caller names",
        &[
            "resolution",
            "keep_both_carried",
            "outside_resolution",
            "keep_passage",
            "keep_separate",
            "keep_joined",
            "restore_book",
            "combine",
            "combine_passage",
            "per_unit",
            "keep_both_comments",
            "apply_to_moved_check",
            "rename_carried",
            "bulk_resolution",
        ],
    ),
    (
        "no read: it builds or changes a fold, which only the memory backend does",
        &[
            "new",
            "fold",
            "fold_from",
            "fold_from_with_steps",
            "add",
            "own_counters",
            "set_source",
        ],
    ),
];

/// The files of tc-app's library that may turn the lint off: the counted walks, the seed
/// check, which reads a whole fold once when a fixture loads, and the seed's unit tests.
const MAY_TURN_IT_OFF: [&str; 3] = [
    "src/backend/seed/check.rs",
    "src/backend/seed/tests.rs",
    "src/counters.rs",
];

fn manifest() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The names of the `pub fn`s in tc-journal's `impl FoldState` blocks.
fn public_fold_methods() -> BTreeSet<String> {
    let mut files = Vec::new();
    rust_files(&manifest().join("../tc-journal/src"), &mut files);
    let mut names = BTreeSet::new();
    for file in files {
        let source = std::fs::read_to_string(&file).unwrap();
        let mut inside = false;
        for line in source.lines() {
            if line.starts_with("impl FoldState {") {
                inside = true;
            } else if inside && line.starts_with('}') {
                inside = false;
            } else if inside
                && let Some(rest) = line
                    .strip_prefix("    pub fn ")
                    .or_else(|| line.strip_prefix("    pub const fn "))
            {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                names.insert(name);
            }
        }
    }
    names
}

#[test]
fn the_lint_refuses_every_whole_fold_walk() {
    let config =
        std::fs::read_to_string(manifest().join("clippy.toml")).expect("tc-app's clippy.toml");
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

/// A public method of `FoldState` that neither list names, a new one or one the lists missed,
/// fails here until someone says what it costs.
#[test]
fn every_public_method_of_the_fold_is_refused_or_costs_less() {
    let found = public_fold_methods();
    let refused: BTreeSet<String> = WHOLE_FOLD_WALKS.iter().map(|s| (*s).to_owned()).collect();
    let less: Vec<String> = COSTS_LESS
        .iter()
        .flat_map(|(_, names)| names.iter().map(|s| (*s).to_owned()))
        .collect();
    let less_set: BTreeSet<String> = less.iter().cloned().collect();
    assert_eq!(less.len(), less_set.len(), "a method listed twice");
    let both: Vec<&String> = refused.intersection(&less_set).collect();
    assert!(both.is_empty(), "refused and allowed: {both:?}");
    let listed: BTreeSet<String> = refused.union(&less_set).cloned().collect();
    let unlisted: Vec<&String> = found.difference(&listed).collect();
    assert!(
        unlisted.is_empty(),
        "public methods of FoldState in neither list: {unlisted:?}"
    );
    let gone: Vec<&String> = listed.difference(&found).collect();
    assert!(gone.is_empty(), "listed but not in tc-journal: {gone:?}");
}

/// The lint holds only where nothing turns it off: an `#[allow]` or `#[expect]` in any other
/// file (as on a key listing in the open, which no count sees, §8.2) fails here.
#[test]
fn only_the_counted_walks_and_the_seed_check_turn_the_lint_off() {
    let mut files = Vec::new();
    rust_files(&manifest().join("src"), &mut files);
    let turning_off: BTreeSet<String> = files
        .iter()
        .filter(|file| {
            let source = std::fs::read_to_string(file).unwrap();
            ["disallowed_methods", "clippy::style", "clippy::all"]
                .iter()
                .any(|lint| source.contains(lint))
        })
        .map(|file| {
            file.strip_prefix(manifest())
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    let allowed: BTreeSet<String> = MAY_TURN_IT_OFF.iter().map(|s| (*s).to_owned()).collect();
    assert_eq!(turning_off, allowed);
}

/// `CountingSnapshot::render` counts a file by what it walks: a book's text as `book_usfm`,
/// the conflict list as `conflict_walks`, any other file as `files_rendered`.
#[test]
fn a_rendered_file_counts_by_what_it_walks() {
    let backend = MemoryBackend::new();
    backend.add(folder("rendered"), sample_bible()).unwrap();
    let handle = backend.open(&folder("rendered"), open_progress()).unwrap();
    let counters = Arc::new(AppCounters::default());
    let read = CountingSnapshot::new(handle.snapshot(), counters.clone());
    let cases = [
        (
            DerivedFile::Usfm(BookId::Titus),
            AppCounts {
                book_usfm: 1,
                ..AppCounts::default()
            },
        ),
        (
            DerivedFile::Conflicts,
            AppCounts {
                conflict_walks: 1,
                ..AppCounts::default()
            },
        ),
        (
            DerivedFile::Alignments(BookId::Titus),
            AppCounts {
                files_rendered: 1,
                ..AppCounts::default()
            },
        ),
        (
            DerivedFile::Decisions(ToolId::TranslationWords, BookId::Titus),
            AppCounts {
                files_rendered: 1,
                ..AppCounts::default()
            },
        ),
        (
            DerivedFile::Settings,
            AppCounts {
                files_rendered: 1,
                ..AppCounts::default()
            },
        ),
    ];
    for (file, cost) in cases {
        let before = counters.read();
        let bytes = read.render(&file);
        assert_eq!(counters.read().since(&before), cost, "{file:?}");
        assert_eq!(bytes, handle.snapshot().fold().render(&file), "{file:?}");
    }
    assert!(read.render(&DerivedFile::Usfm(BookId::Titus)).is_some());
}
