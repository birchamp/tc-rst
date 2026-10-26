//! The Check rail's model (APP-DESIGN §3.8, AD17): a rail patched row by row equals the rail
//! read whole after the same decisions, and a grouping reads each row once. tc-app holds the
//! groups to tc-check's on Titus and Psalms.
#![cfg(not(target_arch = "wasm32"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

use std::cell::Cell;

use proptest::prelude::*;
use tc_api::check::{RailRow, RailRowPatch, RowFlags};
use tc_api::model::rail::{Filter, GroupLabel, SortMode, groups, patch};

/// A rail of `n` rows with indices in a shuffled order, as By verse lists them.
fn rail(n: u32, flags: &[RowFlags]) -> Vec<RailRow> {
    (0..n)
        .map(|i| RailRow {
            index: (i * 7) % n,
            check_id: "abcd".into(),
            place: format!("TIT 1:{}", i + 1).parse().unwrap(),
            title: format!("check {i}"),
            flags: flags[i as usize],
            term: Some(i % 5),
            category: None,
            marker: None,
        })
        .collect()
}

proptest! {
    /// Random decisions on a rail: the rows they changed, patched in, give the rail read
    /// after them.
    #[test]
    fn patching_the_changed_rows_equals_a_fresh_rail(
        before in proptest::collection::vec(any::<RowFlags>(), 31),
        after in proptest::collection::vec(any::<RowFlags>(), 31),
    ) {
        let held = rail(31, &before);
        let fresh = rail(31, &after);
        let changed: Vec<RailRowPatch> = fresh
            .iter()
            .zip(&held)
            .filter(|(new, old)| new != old)
            .map(|(new, _)| RailRowPatch { row: new.clone() })
            .collect();
        let mut patched = held.clone();
        prop_assert!(patch(&mut patched, &changed));
        prop_assert_eq!(patched, fresh);
    }
}

#[test]
fn a_patch_for_a_row_the_rail_lacks_is_refused() {
    let flags = vec![
        RowFlags {
            dot: tc_api::check::Dot::Todo,
            decided: false,
            invalidated: false,
            invalid: false,
            bookmarked: false,
            commented: false
        };
        3
    ];
    let mut held = rail(3, &flags);
    let mut stranger = held[0].clone();
    stranger.index = 99;
    assert!(!patch(&mut held, &[RailRowPatch { row: stranger }]));
}

/// A-13: grouping by word buckets each row once, never once per term (tC4's grouping filters
/// every row once per term: 3.7 million comparisons on Psalms TWL's 7,683 rows and 485 terms).
#[test]
fn rail_groups_visit_each_row_once() {
    let (rows, terms) = (7_683u32, 485u32);
    let flags = RowFlags {
        dot: tc_api::check::Dot::Todo,
        decided: false,
        invalidated: false,
        invalid: false,
        bookmarked: false,
        commented: false,
    };
    let all: Vec<RailRow> = (0..rows)
        .map(|i| RailRow {
            index: i,
            check_id: "abcd".into(),
            place: "PSA 1:1".parse().unwrap(),
            title: String::new(),
            flags,
            term: Some(i % terms),
            category: None,
            marker: None,
        })
        .collect();
    for mode in [SortMode::ByWord, SortMode::ByVerse, SortMode::ByFigure] {
        let visits = Cell::new(0usize);
        let counted = all.iter().inspect(|_| visits.set(visits.get() + 1));
        let out = groups(counted, terms as usize, Filter::All, mode);
        assert_eq!(visits.get(), rows as usize, "{mode:?}");
        let shown: usize = out.iter().map(|g| g.rows.len()).sum();
        assert_eq!(shown, rows as usize);
        if mode == SortMode::ByWord {
            assert_eq!(out.len(), terms as usize);
            assert_eq!(out[3].label, GroupLabel::Term(3));
        }
    }
}

/// INV-GROUP-COUNTS-IGNORE-THE-FILTER: each term's heading counts every row of its term,
/// decided or not, whatever the filter shows (tC4 `groupStatsOf`).
#[test]
fn term_counts_cover_every_row_whatever_the_filter() {
    let row = |index: u32, term: u32, decided: bool, bookmarked: bool| RailRow {
        index,
        check_id: "abcd".into(),
        place: "TIT 1:1".parse().unwrap(),
        title: String::new(),
        flags: RowFlags {
            dot: tc_api::check::Dot::Todo,
            decided,
            invalidated: false,
            invalid: false,
            bookmarked,
            commented: false,
        },
        term: Some(term),
        category: None,
        marker: None,
    };
    let rows = [
        row(0, 1, true, false),
        row(1, 0, false, true),
        row(2, 1, false, false),
        row(3, 1, true, false),
    ];
    let shown = groups(&rows, 2, Filter::Bookmarked, SortMode::ByFigure);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].label, GroupLabel::Figure(0));
    assert_eq!(
        tc_api::model::rail::term_progress(&rows, 2),
        [(0, 1), (2, 3)]
    );
    let all = groups(&rows, 2, Filter::All, SortMode::ByFigure);
    let labels: Vec<GroupLabel> = all.iter().map(|g| g.label).collect();
    assert_eq!(
        labels,
        [GroupLabel::Figure(1), GroupLabel::Figure(0)],
        "first appearance"
    );
}

fn figure_rows() -> Vec<RailRow> {
    (0..6)
        .map(|i| RailRow {
            index: i,
            check_id: "abcd".into(),
            place: "TIT 1:1".parse().unwrap(),
            title: String::new(),
            flags: RowFlags {
                dot: tc_api::check::Dot::Todo,
                decided: false,
                invalidated: false,
                invalid: false,
                bookmarked: false,
                commented: false,
            },
            term: Some(i % 3),
            category: None,
            marker: None,
        })
        .collect()
}

/// INV-THE-ACTIVE-GROUP-IS-OPEN: a folded group shows its rows only while it holds the active
/// check, and By verse never folds.
#[test]
fn the_active_group_is_open_whatever_was_folded() {
    use tc_api::model::rail::open_groups;
    let rows = figure_rows();
    let terms: Vec<String> = ["a", "b", "c"].map(str::to_owned).to_vec();
    let groups = groups(&rows, 3, Filter::All, SortMode::ByFigure);
    let folded: std::collections::BTreeSet<String> = ["a", "b"].map(str::to_owned).into();
    assert_eq!(
        open_groups(&groups, &terms, &folded, Some("b"), SortMode::ByFigure),
        [false, true, true],
        "b holds the active check"
    );
    let by_verse = groups_by_verse(&rows);
    assert_eq!(
        open_groups(&by_verse, &terms, &folded, None, SortMode::ByVerse),
        [true]
    );
}

fn groups_by_verse(rows: &[RailRow]) -> Vec<tc_api::model::rail::RailGroup> {
    groups(rows, 3, Filter::All, SortMode::ByVerse)
}

/// tC4 #769: Fold all folds every shown group but the active one; once those are folded, the
/// same button unfolds every shown group.
#[test]
fn fold_all_keeps_the_active_group_open() {
    use tc_api::model::rail::fold_all;
    let rows = figure_rows();
    let terms: Vec<String> = ["a", "b", "c"].map(str::to_owned).to_vec();
    let groups = groups(&rows, 3, Filter::All, SortMode::ByFigure);
    let mut folded = std::collections::BTreeSet::new();
    let (ids, fold) = fold_all(&groups, &terms, &folded, Some("b"));
    assert!(fold);
    assert_eq!(ids, ["a", "c"]);
    folded.extend(ids);
    let (ids, fold) = fold_all(&groups, &terms, &folded, Some("b"));
    assert!(!fold, "every other group is folded: Unfold all");
    assert_eq!(ids, ["a", "b", "c"]);
}
