//! A book file another program changed is kept, held and asked about (owner ruling OE 3,
//! `docs/STORAGE-DELTA-2026-10-21.md` §2.3, §2.4, §2.11): nothing in it enters the project
//! until the person chooses *Import*; *Not now* writes the file from the project and keeps the
//! copy listed; every kept copy has its own entry until it is imported or dismissed; a crash at
//! any write point of the hold loses nothing and asks again.

use crate::common::*;
use crate::crash::{Outcome, kill_everywhere_full};
use proptest::prelude::*;
use std::path::{Path, PathBuf};
use tc_journal::fold::{ConflictKind, DerivedFile, Notice};
use tc_journal::{BookId, Md5};
use tc_store::fault::FaultFs;
use tc_store::{Kept, KeptBase, Leave, OpenOptions, ProjectHandle};

const USFM: &str = "ingredients/TIT.usfm";
const RUTH: &str = "ingredients/RUT.usfm";

fn open(rig: &Rig, folder: &Path) -> ProjectHandle {
    let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle
}

fn leave(handle: &ProjectHandle) {
    handle.leave(Leave::Book(TIT)).wait().unwrap();
    handle.wait_idle();
}

fn usfm_path(folder: &Path) -> PathBuf {
    folder.join(USFM)
}

/// `bytes` with the text of TIT c:v replaced.
fn set_verse(bytes: &[u8], c: u16, v: u16, text: &str) -> Vec<u8> {
    let s = String::from_utf8(bytes.to_vec()).unwrap();
    let chapter = s.find(&format!("\n\\c {c}\n")).unwrap();
    let marker = format!("\\v {v} ");
    let at = chapter + s[chapter..].find(&marker).unwrap() + marker.len();
    let end = at + s[at..].find('\n').unwrap();
    format!("{}{}{}", &s[..at], text, &s[end..]).into_bytes()
}

fn conflicts(handle: &ProjectHandle) -> usize {
    handle
        .snapshot()
        .fold()
        .conflicts()
        .filter(|c| {
            matches!(
                c.kind,
                ConflictKind::VerseText | ConflictKind::VerseStructure
            )
        })
        .count()
}

fn recorded(handle: &ProjectHandle) -> usize {
    handle
        .notices()
        .into_iter()
        .filter(|n| matches!(n.notice, Notice::OutsideEditRecorded { .. }))
        .count()
}

fn waiting(handle: &ProjectHandle) -> Vec<String> {
    handle.waiting_books().into_iter().map(|w| w.path).collect()
}

/// A project with 1:1 "Paul, a servant" and 1:2 "in hope" saved and written, closed; and the
/// bytes of its book file.
fn saved_and_closed() -> (Rig, PathBuf, Vec<u8>) {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    save_verse(&handle, 1, 2, "in hope").unwrap();
    leave(&handle);
    handle.close().unwrap();
    let written = rig.fs.peek(&usfm_path(&folder)).unwrap();
    (rig, folder, written)
}

/// The kept copy's bytes are the bytes the entry names.
fn assert_copy(rig: &Rig, folder: &Path, kept: &Kept, bytes: &[u8]) {
    assert_eq!(kept.md5, Md5::of(bytes).to_hex());
    assert_eq!(
        rig.fs.peek(&folder.join(&kept.copy)).as_deref(),
        Some(bytes),
        "the copy holds the bytes found"
    );
}

/// INV-A-CHANGED-BOOK-IS-ASKED-NOT-TAKEN: a book file changed while the app was closed is kept,
/// left as it is, and waits: the project keeps its text, and nothing is recorded. The question
/// comes back after a reopen, from the manifest.
#[test]
fn a_changed_book_file_is_kept_held_and_waits_for_an_answer() {
    let (rig, folder, written) = saved_and_closed();
    let edited = set_verse(&written, 1, 2, "in hope of eternal life");
    rig.fs.put(&usfm_path(&folder), &edited, None);
    let handle = open(&rig, &folder);
    assert_eq!(words(&handle, 1, 2).as_deref(), Some("in hope"));
    assert_eq!(recorded(&handle), 0, "{:?}", handle.notices());
    assert_eq!(
        rig.fs.peek(&usfm_path(&folder)),
        Some(edited.clone()),
        "held as it is"
    );
    let books = handle.waiting_books();
    assert_eq!(books.len(), 1, "{books:?}");
    assert_eq!(books[0].path, USFM);
    assert_eq!(books[0].file, DerivedFile::Usfm(BookId::Titus));
    assert!(!books[0].added);
    assert_eq!(
        books[0].kept.base,
        KeptBase::Placed(Md5::of(&written).to_hex())
    );
    assert_copy(&rig, &folder, &books[0].kept, &edited);
    assert_eq!(handle.kept_copies(), vec![books[0].kept.clone()]);
    // A held file is never written over while it waits.
    save_verse(&handle, 1, 3, "which God promised").unwrap();
    leave(&handle);
    assert_eq!(rig.fs.peek(&usfm_path(&folder)), Some(edited.clone()));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);

    let handle = open(&rig, &folder);
    assert_eq!(waiting(&handle), vec![USFM.to_owned()], "asked again");
    assert_eq!(handle.kept_copies().len(), 1, "no second copy");
    assert_eq!(rig.fs.peek(&usfm_path(&folder)), Some(edited));
    handle.close().unwrap();
}

/// The window's focus finds the change made while the project is open, and holds it at once.
#[test]
fn a_change_found_at_focus_is_held() {
    let (rig, folder, written) = saved_and_closed();
    let handle = open(&rig, &folder);
    let edited = set_verse(&written, 1, 2, "in hope of eternal life");
    rig.fs.put(&usfm_path(&folder), &edited, None);
    let report = handle.focus().wait().unwrap();
    handle.wait_idle();
    assert_eq!(report.found, vec![USFM.to_owned()]);
    assert_eq!(waiting(&handle), vec![USFM.to_owned()]);
    assert_eq!(words(&handle, 1, 2).as_deref(), Some("in hope"));
    assert_eq!(rig.fs.peek(&usfm_path(&folder)), Some(edited));
    handle.close().unwrap();
}

/// *Not now*: the file shows the project again; the copy stays listed until it is imported or
/// dismissed, also after a reopen, and the given-up bytes raise no second copy.
#[test]
fn not_now_keeps_the_projects_text_and_lists_the_copy() {
    let (rig, folder, written) = saved_and_closed();
    let edited = set_verse(&written, 1, 2, "in hope of eternal life");
    rig.fs.put(&usfm_path(&folder), &edited, None);
    let handle = open(&rig, &folder);
    handle.not_now(USFM).wait().unwrap();
    handle.wait_idle();
    assert!(waiting(&handle).is_empty());
    assert_eq!(words(&handle, 1, 2).as_deref(), Some("in hope"));
    assert_eq!(rig.fs.peek(&usfm_path(&folder)), Some(written.clone()));
    let kept = handle.kept_copies();
    assert_eq!(kept.len(), 1);
    assert_copy(&rig, &folder, &kept[0], &edited);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);

    let handle = open(&rig, &folder);
    assert!(waiting(&handle).is_empty());
    assert_eq!(handle.kept_copies(), kept, "listed after a reopen");
    handle.close().unwrap();
}

/// *Not now* while another program still has the file and refuses the rewrite: the given-up
/// bytes are the app's own to write over later, so the next look keeps no second copy and asks
/// nothing (delta §2.5 N3a).
#[test]
fn not_now_on_a_file_another_program_holds_asks_nothing_again() {
    let (rig, folder, written) = saved_and_closed();
    let edited = set_verse(&written, 1, 2, "in hope of eternal life");
    rig.fs.put(&usfm_path(&folder), &edited, None);
    let handle = open(&rig, &folder);
    rig.fs.set_busy(&usfm_path(&folder), true);
    handle.not_now(USFM).wait().unwrap();
    handle.wait_idle();
    assert_eq!(rig.fs.peek(&usfm_path(&folder)), Some(edited.clone()));
    handle.close().unwrap();
    rig.fs.set_busy(&usfm_path(&folder), false);
    let handle = open(&rig, &folder);
    assert!(waiting(&handle).is_empty(), "{:?}", handle.waiting_books());
    assert_eq!(handle.kept_copies().len(), 1, "no second copy");
    assert_eq!(rig.fs.peek(&usfm_path(&folder)), Some(written));
    handle.close().unwrap();
}

/// *Import* compares the copy with the version it was found against: a verse the file changed
/// comes in with no question, and a verse only the project changed since keeps the project's
/// text, also after *Not now* (the base's heads are kept while the entry lives). The entry
/// ends and the file shows the project.
#[test]
fn import_takes_in_what_the_file_changed_since_its_base() {
    for not_now_first in [false, true] {
        let (rig, folder, written) = saved_and_closed();
        let edited = set_verse(&written, 1, 2, "in hope of eternal life");
        rig.fs.put(&usfm_path(&folder), &edited, None);
        let handle = open(&rig, &folder);
        // The project changes 1:1 after the file was written from it.
        save_verse(&handle, 1, 1, "Paul, a slave of God").unwrap();
        if not_now_first {
            handle.not_now(USFM).wait().unwrap();
            handle.wait_idle();
        }
        let copy = handle.kept_copies()[0].copy.clone();
        let report = handle.import_kept(&copy).wait().unwrap();
        handle.wait_idle();
        assert_eq!(
            report.imported, 1,
            "{report:?} (not now first: {not_now_first})"
        );
        assert_eq!(report.to_review, 0, "{report:?}");
        assert_eq!(
            words(&handle, 1, 2).as_deref(),
            Some("in hope of eternal life")
        );
        assert_eq!(
            words(&handle, 1, 1).as_deref(),
            Some("Paul, a slave of God")
        );
        assert_eq!(conflicts(&handle), 0);
        assert!(handle.kept_copies().is_empty());
        assert!(waiting(&handle).is_empty());
        let file = String::from_utf8(rig.fs.peek(&usfm_path(&folder)).unwrap()).unwrap();
        assert!(
            file.contains("in hope of eternal life") && file.contains("Paul, a slave of God"),
            "{not_now_first}: {file}\n{:?}\n{}",
            handle.notices(),
            String::from_utf8_lossy(&rig.fs.peek(&folder.join(".tc/materialized.json")).unwrap())
        );
        handle.close().unwrap();
        assert_valid_burrito(&rig.fs, &folder);
        let handle = open(&rig, &folder);
        assert!(waiting(&handle).is_empty());
        assert!(handle.kept_copies().is_empty());
        handle.close().unwrap();
    }
}

/// A verse changed on both sides is a conflict to review, and the project's text stays shown.
#[test]
fn import_of_a_verse_changed_on_both_sides_is_a_conflict() {
    let (rig, folder, written) = saved_and_closed();
    let edited = set_verse(&written, 1, 2, "in hope of eternal life");
    rig.fs.put(&usfm_path(&folder), &edited, None);
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 2, "in the hope").unwrap();
    let copy = handle.waiting_books()[0].kept.copy.clone();
    let report = handle.import_kept(&copy).wait().unwrap();
    handle.wait_idle();
    assert_eq!((report.imported, report.to_review), (0, 1), "{report:?}");
    assert_eq!(words(&handle, 1, 2).as_deref(), Some("in the hope"));
    assert_eq!(conflicts(&handle), 1);
    handle.close().unwrap();
}

/// INV-EVERY-KEPT-COPY-HAS-ITS-ENTRY: a second save of a held file is kept under its own entry
/// with the same base; the question imports the newest, and the first stays listed.
#[test]
fn a_second_save_of_a_held_file_keeps_the_first_copy_listed() {
    let (rig, folder, written) = saved_and_closed();
    let first = set_verse(&written, 1, 2, "in hope of eternal life");
    rig.fs.put(&usfm_path(&folder), &first, None);
    let handle = open(&rig, &folder);
    rig.fs.advance(std::time::Duration::from_secs(5));
    let second = set_verse(&first, 1, 3, "which God promised");
    rig.fs.put(&usfm_path(&folder), &second, None);
    handle.focus().wait().unwrap();
    handle.wait_idle();
    let kept = handle.kept_copies();
    assert_eq!(kept.len(), 2, "{kept:?}");
    assert_copy(&rig, &folder, &kept[0], &first);
    assert_copy(&rig, &folder, &kept[1], &second);
    assert_eq!(kept[0].base, kept[1].base);
    let books = handle.waiting_books();
    assert_eq!(books.len(), 1, "one question names the book once");
    assert_eq!(books[0].kept, kept[1], "the newest copy");
    handle.import_kept(&kept[1].copy).wait().unwrap();
    handle.wait_idle();
    assert_eq!(words(&handle, 1, 3).as_deref(), Some("which God promised"));
    assert_eq!(handle.kept_copies(), vec![kept[0].clone()]);
    handle.close().unwrap();
    let handle = open(&rig, &folder);
    assert_eq!(handle.kept_copies(), vec![kept[0].clone()]);
    handle.close().unwrap();
}

/// Line ends and blanks at line ends only: nothing is asked, and the file is written again.
#[test]
fn a_line_ends_only_change_asks_nothing() {
    let (rig, folder, written) = saved_and_closed();
    let crlf = String::from_utf8(written.clone())
        .unwrap()
        .replace('\n', "  \r\n");
    rig.fs.put(&usfm_path(&folder), crlf.as_bytes(), None);
    let handle = open(&rig, &folder);
    assert!(waiting(&handle).is_empty());
    assert!(handle.kept_copies().is_empty());
    assert_eq!(rig.fs.peek(&usfm_path(&folder)), Some(written));
    handle.close().unwrap();
}

/// A book file the decoder cannot read is still kept and asked about, with the reason, and
/// *Import* refuses it with words while the entry stays.
#[test]
fn an_unreadable_book_file_is_asked_with_its_reason() {
    let (rig, folder, written) = saved_and_closed();
    let text = String::from_utf8(written).unwrap();
    let broken = text.replacen("\\v 3 ", "\\v 2 ", 1).into_bytes();
    rig.fs.put(&usfm_path(&folder), &broken, None);
    let handle = open(&rig, &folder);
    let books = handle.waiting_books();
    assert_eq!(books.len(), 1);
    assert!(books[0].kept.unreadable.is_some());
    assert!(handle.import_kept(&books[0].kept.copy).wait().is_err());
    assert_eq!(handle.kept_copies().len(), 1);
    assert_eq!(rig.fs.peek(&usfm_path(&folder)), Some(broken));
    handle.close().unwrap();
}

const RUTH_USFM: &str = "\\id RUT\n\\usfm 3.0\n\\h Ruth\n\\mt Ruth\n\\c 1\n\\p\n\\v 1 In the days when the judges ruled\n\\v 2 The name of the man was Elimelech\n";

/// A book file of a book the project lacks is asked about (delta §2.11); *Not now* moves it out
/// of the folder and keeps the copy; *Import* of the copy adds the book.
#[test]
fn a_book_file_for_a_book_the_project_lacks_is_asked() {
    let (rig, folder, _) = saved_and_closed();
    rig.fs.put(&folder.join(RUTH), RUTH_USFM.as_bytes(), None);
    let handle = open(&rig, &folder);
    let books = handle.waiting_books();
    assert_eq!(books.len(), 1, "{books:?}");
    assert_eq!(books[0].path, RUTH);
    assert!(books[0].added);
    assert!(!handle.snapshot().fold().book(BookId::Ruth).projects());
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);

    let handle = open(&rig, &folder);
    handle.not_now(RUTH).wait().unwrap();
    handle.wait_idle();
    assert_eq!(rig.fs.peek(&folder.join(RUTH)), None, "moved out");
    let kept = handle.kept_copies();
    assert_eq!(kept.len(), 1);
    assert_eq!(
        rig.fs.peek(&folder.join(&kept[0].copy)).as_deref(),
        Some(RUTH_USFM.as_bytes())
    );
    assert_valid_burrito(&rig.fs, &folder);
    let report = handle.import_kept(&kept[0].copy).wait().unwrap();
    handle.wait_idle();
    assert!(report.book_added, "{report:?}");
    assert_eq!(report.imported, 2, "{report:?}");
    assert!(handle.snapshot().fold().book(BookId::Ruth).projects());
    let ruth = handle.snapshot().fold().book_usfm(BookId::Ruth).unwrap();
    assert!(ruth.contains("In the days when the judges ruled"), "{ruth}");
    assert!(handle.kept_copies().is_empty());
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// *Dismiss* ends an entry and removes its copy while its bytes are the ones kept; a copy whose
/// book still waits is not dismissed.
#[test]
fn dismiss_removes_a_copy_only_after_the_answer() {
    let (rig, folder, written) = saved_and_closed();
    let edited = set_verse(&written, 1, 2, "in hope of eternal life");
    rig.fs.put(&usfm_path(&folder), &edited, None);
    let handle = open(&rig, &folder);
    let copy = handle.kept_copies()[0].copy.clone();
    assert!(handle.dismiss_kept(&copy).wait().is_err(), "it waits");
    assert!(rig.fs.peek(&folder.join(&copy)).is_some());
    handle.not_now(USFM).wait().unwrap();
    handle.wait_idle();
    handle.dismiss_kept(&copy).wait().unwrap();
    assert!(handle.kept_copies().is_empty());
    assert_eq!(rig.fs.peek(&folder.join(&copy)), None);
    handle.close().unwrap();
}

/// A held book file's question survives a crash at every write point of the hold: the reopened
/// project has the changed file as it was, the project's text, and the book waiting with a copy
/// that holds the changed bytes (delta §2.6 X1, X2).
#[test]
fn crash_at_every_write_point_of_the_hold_loses_nothing() {
    let (rig, folder, written) = saved_and_closed();
    let base = rig.fs.crash(false);
    drop(rig);
    let edited = set_verse(&written, 1, 2, "in hope of eternal life");
    let path = usfm_path(&folder);
    let setup = |fs: &FaultFs, _: &ProjectHandle| fs.put(&path, &edited, None);
    let action = |handle: &ProjectHandle| {
        let _ = handle.focus().wait();
        handle.wait_idle();
        Outcome::NotSaved
    };
    let points = kill_everywhere_full(&base, &folder, &setup, &action, &|handle, fs, _| {
        handle.wait_idle();
        assert_eq!(
            fs.peek(&path),
            Some(edited.clone()),
            "the file was written over"
        );
        assert_eq!(words(handle, 1, 2).as_deref(), Some("in hope"));
        let books = handle.waiting_books();
        assert_eq!(books.len(), 1, "{books:?}");
        assert_eq!(books[0].kept.md5, Md5::of(&edited).to_hex());
        for kept in handle.kept_copies() {
            assert_eq!(
                fs.peek(&folder.join(&kept.copy))
                    .map(|b| Md5::of(&b).to_hex()),
                Some(kept.md5.clone()),
                "an entry names a copy that does not hold its bytes: {kept:?}"
            );
        }
    });
    assert!(points > 4, "{points} kill points");
}

/// An *Import* killed at any write point imports the copy once or not at all: the verse is the
/// file's after the import, never twice a version, and the copy is kept until the entry ends.
#[test]
fn crash_at_every_write_point_of_an_import_imports_once() {
    let (rig, folder, written) = saved_and_closed();
    let edited = set_verse(&written, 1, 2, "in hope of eternal life");
    rig.fs.put(&usfm_path(&folder), &edited, None);
    let handle = open(&rig, &folder);
    assert_eq!(waiting(&handle), vec![USFM.to_owned()]);
    handle.close().unwrap();
    let base = rig.fs.crash(false);
    drop(rig);
    let action = |handle: &ProjectHandle| {
        let copy = handle.kept_copies()[0].copy.clone();
        let _ = handle.import_kept(&copy).wait();
        handle.wait_idle();
        Outcome::NotSaved
    };
    let points = kill_everywhere_full(&base, &folder, &|_, _| {}, &action, &|handle, fs, _| {
        handle.wait_idle();
        let imported = words(handle, 1, 2).as_deref() == Some("in hope of eternal life");
        if !imported {
            assert_eq!(words(handle, 1, 2).as_deref(), Some("in hope"));
            assert_eq!(waiting(handle), vec![USFM.to_owned()], "asked again");
        }
        for kept in handle.kept_copies() {
            assert_eq!(
                fs.peek(&folder.join(&kept.copy))
                    .map(|b| Md5::of(&b).to_hex()),
                Some(kept.md5.clone())
            );
        }
        // An import run again adds nothing the first run wrote.
        if let Some(book) = handle.waiting_books().first() {
            handle.import_kept(&book.kept.copy).wait().unwrap();
            handle.wait_idle();
            assert_eq!(
                words(handle, 1, 2).as_deref(),
                Some("in hope of eternal life")
            );
        }
        assert_eq!(conflicts(handle), 0);
        let versions = handle
            .snapshot()
            .fold()
            .history(&verse(1, 2))
            .into_iter()
            .filter(|id| {
                handle.snapshot().fold().body(id).is_some_and(|b| {
                    matches!(&b.record, tc_journal::Record::Verse {
                        value: tc_journal::record::VerseValue::Text { words, .. }, ..
                    } if words == "in hope of eternal life")
                })
            })
            .count();
        assert_eq!(versions, 1, "imported once");
    });
    assert!(points > 4, "{points} kill points");
}

// ---------------------------------------------------------------------------------------
// No outside text is lost across save, outside edit, not now, import and reopen
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Op {
    /// The app saves 1:v.
    Save(u16),
    /// Another program writes new words into 1:v of the book file.
    Edit(u16),
    Focus,
    NotNow,
    /// *Import* of the newest, or the oldest, kept copy.
    Import(bool),
    Leave,
    Reopen,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        2 => (1u16..=3).prop_map(Op::Save),
        3 => (1u16..=3).prop_map(Op::Edit),
        2 => Just(Op::Focus),
        2 => Just(Op::NotNow),
        2 => any::<bool>().prop_map(Op::Import),
        1 => Just(Op::Leave),
        1 => Just(Op::Reopen),
    ]
}

/// Whether `text` is a version of any verse of chapter 1 in the project's History.
fn in_history(handle: &ProjectHandle, text: &str) -> bool {
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    (1..=3).any(|v| {
        fold.history(&verse(1, v)).into_iter().any(|id| {
            fold.body(&id).is_some_and(|b| {
                matches!(&b.record, tc_journal::Record::Verse {
                    value: tc_journal::record::VerseValue::Text { words, .. }, ..
                } if words == text)
            })
        })
    })
}

/// The texts of the listed kept copies.
fn copies(rig: &Rig, folder: &Path, handle: &ProjectHandle) -> Vec<String> {
    handle
        .kept_copies()
        .iter()
        .filter_map(|k| rig.fs.peek(&folder.join(&k.copy)))
        .map(|b| String::from_utf8(b).unwrap())
        .collect()
}

fn run_ops(ops: &[Op]) {
    let (rig, folder, _) = saved_and_closed();
    let path = usfm_path(&folder);
    let mut handle = open(&rig, &folder);
    let mut outside: Vec<String> = Vec::new();
    let mut saved: Vec<String> = Vec::new();
    let mut last_put: Option<String> = None;
    for (n, op) in ops.iter().enumerate() {
        rig.fs.advance(std::time::Duration::from_secs(3));
        match op {
            Op::Save(v) => {
                let text = format!("saved {n}");
                save_verse(&handle, 1, *v, &text).unwrap();
                saved.push(text);
            }
            Op::Edit(v) => {
                let text = format!("outside {n}");
                let bytes = rig.fs.peek(&path).unwrap();
                let new = set_verse(&bytes, 1, *v, &text);
                // A text the program wrote over itself before the app looked is retired.
                let copies = copies(&rig, &folder, &handle);
                let after = String::from_utf8(new.clone()).unwrap();
                outside.retain(|t| {
                    let line = format!("{t}\n");
                    after.contains(&line)
                        || copies.iter().any(|c| c.contains(&line))
                        || in_history(&handle, t)
                });
                rig.fs.put(&path, &new, None);
                last_put = Some(Md5::of(&new).to_hex());
                outside.push(text);
            }
            Op::Focus => {
                handle.focus().wait().unwrap();
            }
            Op::NotNow => {
                if let Some(book) = handle.waiting_books().first() {
                    handle.not_now(&book.path).wait().unwrap();
                }
            }
            Op::Import(newest) => {
                let kept = handle.kept_copies();
                let chosen = if *newest { kept.last() } else { kept.first() };
                if let Some(kept) = chosen {
                    handle.import_kept(&kept.copy).wait().unwrap();
                }
            }
            Op::Leave => leave(&handle),
            Op::Reopen => {
                handle.close().unwrap();
                handle = open(&rig, &folder);
            }
        }
        handle.wait_idle();
        // Every outside text is in the file, in a listed copy, or in the project's History.
        let file = String::from_utf8(rig.fs.peek(&path).unwrap_or_default()).unwrap();
        let copies = copies(&rig, &folder, &handle);
        for text in &outside {
            let found = file.contains(&format!("{text}\n"))
                || copies.iter().any(|c| c.contains(&format!("{text}\n")))
                || in_history(&handle, text);
            assert!(
                found,
                "{text} was lost after {op:?} (step {n}) in {ops:?}\nfile: {file}\nkept: {:?}",
                handle.kept_copies()
            );
        }
        for text in &saved {
            assert!(
                in_history(&handle, text),
                "the save {text} was lost in {ops:?}"
            );
        }
        // Nothing of a held file enters the project without an Import.
        for book in handle.waiting_books() {
            let now = rig
                .fs
                .peek(&folder.join(&book.path))
                .map(|b| Md5::of(&b).to_hex());
            assert!(
                now == Some(book.kept.md5.clone()) || now == last_put,
                "a held file was written over in {ops:?}"
            );
        }
    }
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// The history the review of this card found: a copy given up with *Not now*, then a new
/// save of the file imported; the first copy's text is only in its copy, which stays listed.
#[test]
fn a_copy_given_up_stays_listed_after_a_later_import() {
    run_ops(&[
        Op::Edit(1),
        Op::Focus,
        Op::NotNow,
        Op::Edit(2),
        Op::Focus,
        Op::Import(true),
        Op::Reopen,
    ]);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, failure_persistence: None, ..ProptestConfig::default() })]

    /// INV-NOTHING-ENTERS-WITHOUT-IMPORT and no loss: random histories of saves, outside edits,
    /// focus, *Not now*, *Import* and reopens keep every outside text and every save.
    #[test]
    fn no_outside_text_is_lost_across_not_now_import_and_reopen(
        ops in proptest::collection::vec(op(), 1..14)
    ) {
        run_ops(&ops);
    }
}
