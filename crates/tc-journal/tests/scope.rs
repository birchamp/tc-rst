//! The scope digest of a derived file (`docs/STORAGE.md` §6.2): an open takes a file as
//! converged without reading or projecting it when its digest is the one its manifest entry
//! keeps. That is safe only if the digest changes whenever the file's bytes would: these
//! tests check it over random team histories, after every step.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
mod sim;
mod support;

use common::run_with_census;
use proptest::prelude::*;
use sim::{Sim, arb_script};
use std::collections::BTreeSet;
use tc_journal::fold::{DerivedFile, FoldState};

fn class(file: &DerivedFile) -> &'static str {
    match file {
        DerivedFile::Usfm(_) => "usfm",
        DerivedFile::Alignments(_) => "alignments",
        DerivedFile::Decisions(..) => "decisions",
        DerivedFile::Comments(_) => "comments",
        DerivedFile::Conflicts => "conflicts",
        _ => "project",
    }
}

/// For every file either fold derives, a digest that did not change means the bytes did not.
fn check_step(
    before: &FoldState,
    after: &FoldState,
    census: &common::Census,
) -> Result<(), TestCaseError> {
    let files: BTreeSet<DerivedFile> = before
        .derived_files()
        .into_iter()
        .chain(after.derived_files())
        .collect();
    let files: Vec<DerivedFile> = files.into_iter().collect();
    let old = before.scope_digests(&files);
    let new = after.scope_digests(&files);
    for (i, file) in files.iter().enumerate() {
        prop_assert_eq!(old[i], before.scope_digest(file), "{:?}", file);
        if old[i] == new[i] {
            prop_assert_eq!(
                before.render(file),
                after.render(file),
                "{:?}: the digest stayed {:?} but the bytes changed",
                file,
                old[i]
            );
            census.saw(match class(file) {
                "usfm" => "same digest: usfm",
                "alignments" => "same digest: alignments",
                "decisions" => "same digest: decisions",
                "comments" => "same digest: comments",
                "conflicts" => "same digest: conflicts",
                _ => "same digest: project",
            });
        } else if before.render(file) != after.render(file) {
            census.saw(match class(file) {
                "usfm" => "new bytes: usfm",
                "alignments" => "new bytes: alignments",
                "decisions" => "new bytes: decisions",
                "comments" => "new bytes: comments",
                "conflicts" => "new bytes: conflicts",
                _ => "new bytes: project",
            });
        }
    }
    Ok(())
}

#[test]
fn a_file_whose_scope_digest_is_unchanged_renders_the_same_bytes() {
    run_with_census(
        64,
        arb_script(),
        &[
            "same digest: usfm",
            "same digest: alignments",
            "same digest: decisions",
            "same digest: comments",
            "same digest: conflicts",
            "same digest: project",
            "new bytes: usfm",
            "new bytes: alignments",
            "new bytes: decisions",
            "new bytes: comments",
            "new bytes: conflicts",
            "new bytes: project",
        ],
        |script, census| {
            let mut sim = Sim::new(script.devices);
            let mut before = sim.devices[0].state.clone();
            for op in &script.ops {
                sim.step(op);
                let after = sim.devices[0].state.clone();
                check_step(&before, &after, census)?;
                before = after;
            }
            sim.sync_all();
            let after = sim.devices[0].state.clone();
            check_step(&before, &after, census)?;
            Ok(())
        },
    );
}

/// The digests are a function of the change set (I1): a fold of the same changes in another
/// order, or one change at a time, gives every file the same digest. An open compares the
/// digest its own fold computes with the one the materialization's fold wrote; a digest that
/// followed arrival order would project files that did not change.
#[test]
fn a_files_scope_digest_is_a_function_of_the_change_set() {
    use tc_journal::fold::ProjectKind;
    run_with_census(
        64,
        arb_script(),
        &["comments", "decisions", "project"],
        |script, census| {
            let sim = Sim::run(&script, false);
            let union = sim.union();
            let forward = FoldState::fold(ProjectKind::Bible, union.clone());
            let mut reversed_inputs = union.clone();
            reversed_inputs.reverse();
            let reversed = FoldState::fold(ProjectKind::Bible, reversed_inputs);
            let mut one_by_one = FoldState::fold(ProjectKind::Bible, Vec::new());
            for (i, input) in union.into_iter().enumerate().rev() {
                if i % 2 == 0 {
                    one_by_one.add([input]);
                } else {
                    one_by_one.add(std::iter::once(input));
                }
            }
            let files: Vec<DerivedFile> = forward.derived_files();
            let expected = forward.scope_digests(&files);
            for other in [&reversed, &one_by_one] {
                prop_assert_eq!(&other.scope_digests(&files), &expected);
                prop_assert_eq!(
                    other.metadata_scope_digest(),
                    forward.metadata_scope_digest()
                );
            }
            for file in &files {
                census.saw(match class(file) {
                    "comments" => "comments",
                    "decisions" => "decisions",
                    _ => "project",
                });
            }
            Ok(())
        },
    );
}

/// The heads file lists the token rule's heads of exactly the records the digest covers.
#[test]
fn a_file_s_heads_are_the_token_rule_s_heads_of_its_scope() {
    let script = sim::Script {
        devices: 3,
        ops: Vec::new(),
        last_books_removed: false,
    };
    let sim = Sim::run(&script, false);
    let state = &sim.devices[0].state;
    for file in state.derived_files() {
        let heads = state.file_heads(&file);
        let keys: Vec<_> = heads.iter().map(|(k, _)| k.clone()).collect();
        if file != DerivedFile::Conflicts {
            let scope = state.scope_keys(&file);
            for key in &keys {
                assert!(scope.contains(key), "{file:?}: {key:?}");
            }
        }
        for (key, listed) in heads {
            assert_eq!(listed, state.shown_heads(&key, None), "{file:?}: {key:?}");
        }
    }
}

/// The range reads (APP-DESIGN R1) find exactly what a scan of every record finds, over
/// random team histories with renumbers, joins, removals, comments and carried-over checks.
#[test]
fn range_reads_find_what_a_scan_finds() {
    use tc_journal::keys::{CheckTarget, RecordKey, ToolId};
    use tc_journal::record::{CommentTarget, Record};
    run_with_census(
        64,
        arb_script(),
        &["verses", "alignments", "decisions", "comments"],
        |script, census| {
            let sim = Sim::run(&script, false);
            let state = &sim.devices[0].state;
            let all = state.record_keys();
            let books: BTreeSet<_> = all.iter().filter_map(|k| k.book()).collect();
            for book in books {
                let mut verses = Vec::new();
                let mut alignments = Vec::new();
                for chapter in 0..=200u16 {
                    let found = state.chapter_keys(book, chapter);
                    verses.extend(found.verses);
                    alignments.extend(found.alignments);
                }
                let scan_verses: Vec<_> = all
                    .iter()
                    .filter(|k| matches!(k, RecordKey::Verse(b, ..) if *b == book))
                    .cloned()
                    .collect();
                prop_assert_eq!(&verses, &scan_verses);
                if !verses.is_empty() {
                    census.saw("verses");
                }
                let scan_alignments: Vec<_> = all
                    .iter()
                    .filter(|k| matches!(k, RecordKey::Alignment(b, ..) if *b == book))
                    .cloned()
                    .collect();
                prop_assert_eq!(&alignments, &scan_alignments);
                prop_assert_eq!(&state.alignments_of(book), &scan_alignments);
                if !alignments.is_empty() {
                    census.saw("alignments");
                }
                for tool in [
                    ToolId::TranslationNotes,
                    ToolId::TranslationWords,
                    ToolId::TranslationQuestions,
                ] {
                    let scan: Vec<_> = all
                        .iter()
                        .filter(|k| matches!(k, RecordKey::Check(c) if c.tool == tool
                            && matches!(&c.target, CheckTarget::Verse { book: b, .. } if *b == book)))
                        .cloned()
                        .collect();
                    prop_assert_eq!(&state.decisions_of(tool, book), &scan);
                    if !scan.is_empty() {
                        census.saw("decisions");
                    }
                }
                let scan_comments: Vec<_> = all
                    .iter()
                    .filter(|k| {
                        matches!(k, RecordKey::Comment(_))
                            && state.history(k).iter().any(|id| {
                                matches!(
                                    state.body(id).map(|b| b.record),
                                    Some(Record::Comment { value, .. }) if match &value.target {
                                        CommentTarget::Verse { book: b, .. } => *b == book,
                                        CommentTarget::Check(c) => c.book() == Some(book),
                                        CommentTarget::Frame { .. } => false,
                                    }
                                )
                            })
                    })
                    .cloned()
                    .collect();
                prop_assert_eq!(&state.comments_of(book), &scan_comments);
                if !scan_comments.is_empty() {
                    census.saw("comments");
                }
            }
            Ok(())
        },
    );
}
