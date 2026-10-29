//! Repro for #27 (docs/evidence/fixed-in-4.1-2026-10-29).
//! Goes at crates/tc-store/tests/repro_27.rs. Run:
//!   tc-heavy cargo test -p tc-store --test repro_27 -- --nocapture
//!
//! Two writers save from the same shown state (each holds a token minted before the other's
//! save). Nothing is refused: different parts of a check merge; the same part of a check takes
//! the newest and keeps the older in History (H5); verse text keeps both as a conflict.

#![allow(clippy::unwrap_used, clippy::disallowed_methods)]

#[path = "store/common.rs"]
#[allow(dead_code)]
mod common;

use common::*;
use tc_journal::command::{Command, DecisionBasis, DecisionChange, DecisionEdit};
use tc_journal::keys::{CheckId, CheckKey, CheckTarget, RefPart, ToolId};
use tc_journal::record::{ResourceRef, UserStatus};
use tc_journal::{GitOid, Json, JsonMap, Record, RecordKey};
use tc_store::OpenOptions;

fn tn_key() -> RecordKey {
    RecordKey::Check(CheckKey {
        tool: ToolId::TranslationNotes,
        check_id: CheckId::parse("abcd").unwrap(),
        target: CheckTarget::Verse {
            book: TIT,
            chapter: RefPart::parse("1").unwrap(),
            verse: RefPart::parse("2").unwrap(),
        },
        occurrence: 1,
    })
}

fn edit(changes: Vec<DecisionChange>) -> DecisionEdit {
    DecisionEdit {
        basis: Some(DecisionBasis {
            context: JsonMap::from([("checkId".to_owned(), Json::from("abcd"))]),
            category: Some("figures".into()),
        }),
        resource: ResourceRef {
            repo_path: "git.door43.org/unfoldingWord/en_tn".into(),
            sha: GitOid::parse(&format!("{:040x}", 7)).unwrap(),
        },
        changes,
    }
}

#[test]
fn repro_27_two_writers_from_one_shown_state_are_never_refused() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let key = tn_key();
    let save = |shown, verse_shown, changes| {
        handle
            .submit(Command::SaveDecision {
                shown,
                verse_shown,
                value: edit(changes),
            })
            .wait()
    };
    let first = save(
        handle.mint(&key, None).unwrap(),
        handle.mint(&verse(1, 2), None).unwrap(),
        vec![DecisionChange::Status(Some(UserStatus::Valid))],
    );
    println!("27: first decision = {:?}", first.as_ref().map(|r| &r.stored));
    first.unwrap();

    // 1. Two writers, different parts of the check: a status and a bookmark.
    let (a, b) = (
        handle.mint(&key, None).unwrap(),
        handle.mint(&key, None).unwrap(),
    );
    let vs = || handle.mint(&verse(1, 2), None).unwrap();
    let ra = save(a, vs(), vec![DecisionChange::Status(Some(UserStatus::Invalid))]);
    let rb = save(b, vs(), vec![DecisionChange::Bookmarked(true)]);
    println!("27 merge: writer A {:?}; writer B (stale token) {:?}", ra.as_ref().map(|r| (&r.stored, r.new_conflicts.len())), rb.as_ref().map(|r| (&r.stored, r.new_conflicts.len())));
    assert!(ra.is_ok() && rb.is_ok(), "neither save is refused");
    let fold = handle.snapshot();
    let projected = fold.fold().projected(&key);
    println!("27 merge: conflicts = {}; check now = {projected:?}", fold.fold().conflict_count());
    assert_eq!(fold.fold().conflict_count(), 0);
    let Some(Record::Check { value, .. }) = projected else { panic!() };
    let shown = format!("{value:?}");
    assert!(shown.contains("Invalid") && shown.contains("bookmarked: true"), "{shown}");

    // 2. Two writers, the same part: the newest wins, the older stays in History (H5).
    let (c, d) = (
        handle.mint(&key, None).unwrap(),
        handle.mint(&key, None).unwrap(),
    );
    save(c, vs(), vec![DecisionChange::Status(Some(UserStatus::Valid))]).unwrap();
    let rd = save(d, vs(), vec![DecisionChange::Status(Some(UserStatus::Todo))]);
    println!("27 same part: writer D (stale token) {:?}", rd.as_ref().map(|r| (&r.stored, r.new_conflicts.len())));
    rd.unwrap();
    let fold = handle.snapshot();
    let projected = format!("{:?}", fold.fold().projected(&key));
    let history = fold.fold().history(&key).len();
    println!("27 same part: conflicts = {}; History versions = {history}; check now = {projected}", fold.fold().conflict_count());
    assert_eq!(fold.fold().conflict_count(), 0);
    assert!(projected.contains("Todo"), "{projected}");
    assert!(history >= 4, "every version stays in History");

    // 3. Verse text from a stale token: both kept as a conflict, never a refusal or a revert.
    let old = handle.mint(&verse(1, 3), None).unwrap();
    save_verse(&handle, 1, 3, "first").unwrap();
    let rv = handle
        .submit(Command::SaveVerse { shown: old, words: "second".into() })
        .wait()
        .unwrap();
    let heads = handle.snapshot().fold().heads(&verse(1, 3)).len();
    println!("27 verse: new conflicts = {}; heads of 1:3 = {heads}", rv.new_conflicts.len());
    assert_eq!(heads, 2);
    handle.close().unwrap();
}
