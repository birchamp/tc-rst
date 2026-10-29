//! Evidence for tC4 #179 (fixed-in-4.1 check, 2026-10-29). Goes at
//! `crates/tc-journal/tests/repro_179.rs` (it uses `tests/support/mod.rs`). Run:
//! `tc-heavy cargo test -p tc-journal --test repro_179 -- --nocapture`
//!
//! The issue's steps on the fold's public API: a verse reads `nuevo`, then one edit arrives
//! whose base names a change nobody has. Two clock orders: the orphan older than the
//! standing text (what `loss_unknown_parent_verse_edit_keeps_standing_text` builds by the
//! test clocks), and the orphan newer (the case the issue is about: a teammate's later edit).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, reason = "test code")]

mod support;

use support::*;
use tc_journal::Change;
use tc_journal::fold::FoldState;

fn report(label: &str, state: &FoldState) -> Option<String> {
    let key = verse_key(1, 1);
    let shown = state.projected(&key).as_ref().and_then(words_of);
    let conflicts: Vec<_> = state.conflicts().cloned().collect();
    println!("--- {label}");
    println!("heads of 1:1: {}", state.heads(&key).len());
    println!("projected 1:1: {shown:?}");
    for c in &conflicts {
        println!(
            "conflict kind {:?} incomplete_history {} candidates {}",
            c.kind,
            c.incomplete_history,
            c.candidates.len()
        );
    }
    let usfm = usfm(state);
    let line = usfm.lines().find(|l| l.contains("\\v 1 ")).unwrap_or("").to_owned();
    println!("USFM verse 1 line: {line:?}");
    shown
}

fn scenario(orphan_newer: bool) -> (FoldState, Change) {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[2]);
    let base = latest(&book, &verse_key(1, 1));
    let nuevo = maria.one(verse(1, 1, text("nuevo", 1)), vec![base.id()]);
    let mut ghost_dev = Dev::new(3);
    let ghost = ghost_dev.one(verse(1, 1, text("never arrived", 1)), vec![nuevo.id()]);
    if orphan_newer {
        juan.at = maria.at + 3_600_000;
    }
    let orphan = juan.one(verse(1, 1, text("Juan's edit", 1)), vec![ghost.id()]);
    let mut all = book.clone();
    all.push(nuevo);
    all.push(orphan.clone());
    (assert_converges(&all), orphan)
}

#[test]
fn repro_179_orphan_older_than_the_standing_text() {
    let (state, _) = scenario(false);
    let shown = report("orphan edit older than 'nuevo'", &state);
    assert_eq!(shown.as_deref(), Some("nuevo"));
}

#[test]
fn repro_179_orphan_newer_than_the_standing_text() {
    let (state, _) = scenario(true);
    let shown = report("orphan edit newer than 'nuevo'", &state);
    assert_eq!(state.heads(&verse_key(1, 1)).len(), 2, "standing head kept");
    assert_ne!(shown.as_deref(), Some("___"), "projects the empty stub");
    assert!(shown.is_some(), "verse shows nothing");
    // The issue's ask: the standing text keeps projecting.
    assert_eq!(shown.as_deref(), Some("nuevo"), "the standing text was displaced");
}
