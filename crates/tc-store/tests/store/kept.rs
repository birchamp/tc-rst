//! A book file another program changed is kept, held and asked about (owner ruling OE 3,
//! `docs/STORAGE-DELTA-2026-10-21.md` §2.3, §2.4, §2.11): nothing in it enters the project
//! until the person chooses *Import*; *Not now* writes the file from the project and keeps the
//! copy listed; every kept copy has its own entry until it is imported or dismissed; a crash at
//! any write point of the hold loses nothing and asks again.

use crate::common::*;
use crate::crash::{Outcome, kill_everywhere_between, kill_everywhere_full};
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
        assert_eq!(
            rig.fs.peek(&folder.join(&copy)),
            Some(edited.clone()),
            "the copy stays in .tc/external"
        );
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

/// `bytes` with the book's `\toc1` line replaced.
fn set_toc1(bytes: &[u8], text: &str) -> Vec<u8> {
    let s = String::from_utf8(bytes.to_vec()).unwrap();
    let at = s.find("\\toc1 ").unwrap() + "\\toc1 ".len();
    let end = at + s[at..].find('\n').unwrap();
    format!("{}{}{}", &s[..at], text, &s[end..]).into_bytes()
}

fn header(handle: &ProjectHandle) -> String {
    match handle
        .snapshot()
        .fold()
        .projected(&tc_journal::RecordKey::BookHeader(TIT))
    {
        Some(tc_journal::record::Record::BookHeader { value, .. }) => value.0,
        other => panic!("no header: {other:?}"),
    }
}

/// A header the translator never touched (`\toc1` changed in another program) comes in with
/// no question, and the project's header record holds the file's text.
#[test]
fn import_of_a_header_only_change_takes_it_in() {
    let (rig, folder, written) = saved_and_closed();
    let edited = set_toc1(&written, "Paul's letter to Titus");
    rig.fs.put(&usfm_path(&folder), &edited, None);
    let handle = open(&rig, &folder);
    let copy = handle.waiting_books()[0].kept.copy.clone();
    let report = handle.import_kept(&copy).wait().unwrap();
    handle.wait_idle();
    assert_eq!((report.imported, report.to_review), (1, 0), "{report:?}");
    assert!(
        header(&handle).contains("\\toc1 Paul's letter to Titus"),
        "{}",
        header(&handle)
    );
    let file = String::from_utf8(rig.fs.peek(&usfm_path(&folder)).unwrap()).unwrap();
    assert!(file.contains("\\toc1 Paul's letter to Titus"), "{file}");
    assert!(handle.kept_copies().is_empty());
    handle.close().unwrap();
}

/// A header changed on both sides (two saves of the file, the newer imported first) is a
/// conflict whose two versions each hold their header text.
#[test]
fn import_of_a_header_changed_on_both_sides_is_a_conflict_with_both_texts() {
    let (rig, folder, written) = saved_and_closed();
    let first = set_toc1(&written, "First title");
    rig.fs.put(&usfm_path(&folder), &first, None);
    let handle = open(&rig, &folder);
    rig.fs.advance(std::time::Duration::from_secs(5));
    let second = set_toc1(&written, "Second title");
    rig.fs.put(&usfm_path(&folder), &second, None);
    handle.focus().wait().unwrap();
    handle.wait_idle();
    let kept = handle.kept_copies();
    assert_eq!(kept.len(), 2, "{kept:?}");
    handle.import_kept(&kept[1].copy).wait().unwrap();
    handle.wait_idle();
    let report = handle.import_kept(&kept[0].copy).wait().unwrap();
    handle.wait_idle();
    assert_eq!((report.imported, report.to_review), (0, 1), "{report:?}");
    let snapshot = handle.snapshot();
    let conflict = snapshot
        .fold()
        .conflicts()
        .find(|c| c.kind == ConflictKind::BookHeader)
        .expect("a header conflict");
    let mut texts: Vec<String> = conflict
        .candidates
        .iter()
        .map(|c| match &c.value {
            tc_journal::record::Record::BookHeader { value, .. } => value.0.clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    texts.sort();
    assert_eq!(texts.len(), 2, "{texts:?}");
    assert!(texts[0].contains("\\toc1 First title"), "{texts:?}");
    assert!(texts[1].contains("\\toc1 Second title"), "{texts:?}");
    handle.close().unwrap();
}

/// A second *Import* of the same copy (two windows, or a reload during an Import) is refused
/// with `NotKept` once the first ended the entry, never answered "0 imported, 0 to review"
/// while the first made a conflict.
#[test]
fn a_second_import_of_an_ended_copy_is_refused() {
    let (rig, folder, written) = saved_and_closed();
    let edited = set_verse(&written, 1, 2, "in hope of eternal life");
    rig.fs.put(&usfm_path(&folder), &edited, None);
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 2, "in the hope").unwrap();
    let copy = handle.kept_copies()[0].copy.clone();
    let first = handle.import_kept(&copy);
    let second = handle.import_kept(&copy);
    let first = first.wait().unwrap();
    assert_eq!((first.imported, first.to_review), (0, 1), "{first:?}");
    let second = second.wait();
    assert!(
        matches!(&second, Err(tc_store::StoreError::NotKept { copy: c }) if *c == copy),
        "{second:?}"
    );
    handle.wait_idle();
    let third = handle.import_kept(&copy).wait();
    assert!(
        matches!(third, Err(tc_store::StoreError::NotKept { .. })),
        "{third:?}"
    );
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

/// Line ends and blanks at line ends only: nothing is asked, the file is written again, a copy
/// is kept and the whitespace notice is shown.
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
    // H22 part 4: a copy is kept, and the notice is shown.
    let copies: Vec<Vec<u8>> = files(&rig.fs, &folder)
        .into_iter()
        .filter(|(p, _)| p.starts_with(".tc/external/"))
        .map(|(_, b)| b)
        .collect();
    assert_eq!(copies, vec![crlf.clone().into_bytes()]);
    assert!(
        handle
            .notices()
            .iter()
            .any(|n| matches!(n.notice, Notice::WhitespaceOnlyEdit { .. })),
        "{:?}",
        handle.notices()
    );
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
        // A copy a killed hold left unnamed and the hold made again lists twice once the
        // manifest is lost.
        assert_eq!(handle.kept_copies().len(), 1, "{:?}", handle.kept_copies());
        let copies = fs
            .files_under(&folder.join(".tc/external"))
            .into_values()
            .filter(|bytes| *bytes == edited)
            .count();
        assert_eq!(copies, 1, "the changed bytes are kept once");
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

/// INV-A-KEPT-COPY-OUTLIVES-ITS-MANIFEST: a held book's `kept` entry lives in
/// `.tc/materialized.json`. When that file is lost or cannot be read, the next open lists the
/// copy again (no base), with *Import*, whether the file still holds the changed bytes or
/// *Not now* wrote it over.
#[test]
fn a_kept_copy_is_listed_again_after_the_manifest_is_lost() {
    for (not_now_first, corrupt) in [(false, false), (true, false), (true, true)] {
        let (rig, folder, written) = saved_and_closed();
        let edited = set_verse(&written, 1, 2, "in hope of eternal life");
        rig.fs.put(&usfm_path(&folder), &edited, None);
        let handle = open(&rig, &folder);
        if not_now_first {
            handle.not_now(USFM).wait().unwrap();
            handle.wait_idle();
        }
        let kept = handle.kept_copies();
        assert_eq!(kept.len(), 1);
        handle.close().unwrap();
        let manifest = folder.join(".tc/materialized.json");
        if corrupt {
            // A `kept` value an older build cannot read fails the whole manifest.
            let text = String::from_utf8(rig.fs.peek(&manifest).unwrap()).unwrap();
            assert!(text.contains("\"changed\""), "{text}");
            let newer = text.replace("\"changed\"", "\"aNewerWhy\"");
            rig.fs.put(&manifest, newer.as_bytes(), None);
        } else {
            rig.fs.delete(&manifest).unwrap();
        }

        let handle = open(&rig, &folder);
        let listed = handle.kept_copies();
        assert_eq!(
            listed.len(),
            1,
            "{listed:?} (not now first: {not_now_first}, corrupt: {corrupt})"
        );
        assert_eq!(listed[0].path, USFM);
        assert_copy(&rig, &folder, &listed[0], &edited);
        let report = handle.import_kept(&listed[0].copy).wait().unwrap();
        handle.wait_idle();
        assert_eq!(report.imported + report.to_review, 1, "{report:?}");
        let shown = words(&handle, 1, 2).as_deref() == Some("in hope of eternal life");
        assert!(
            shown || conflicts(&handle) == 1,
            "the file's 1:2 is neither shown nor to review: {report:?}"
        );
        assert!(handle.kept_copies().is_empty());
        handle.close().unwrap();
        assert_valid_burrito(&rig.fs, &folder);
    }
}

/// A file held against HEAD (`.tc/` knew nothing of it) is compared at *Import* with the
/// HEAD recorded when it was held, not with the HEAD of the time of the Import: a verse the
/// translator saved and committed after the hold keeps the translator's text (to review at
/// most), never the file's older words with no question.
#[test]
fn import_of_a_head_based_copy_never_replaces_newer_text() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 2, "in hope").unwrap();
    save_verse(&handle, 1, 3, "which God promised").unwrap();
    leave(&handle);
    let first = handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert!(first.commit.is_some(), "{first:?}");
    handle.close().unwrap();
    let written = rig.fs.peek(&usfm_path(&folder)).unwrap();
    rig.fs.put(
        &usfm_path(&folder),
        &set_verse(
            &set_verse(&written, 1, 2, "in hope of eternal life"),
            1,
            3,
            "which God promised long ago",
        ),
        None,
    );
    rig.fs
        .delete(&folder.join(".tc/materialized.json"))
        .unwrap();
    let handle = open(&rig, &folder);
    let books = handle.waiting_books();
    assert_eq!(books.len(), 1, "{books:?}");
    assert!(
        matches!(books[0].kept.base, KeptBase::Head(Some(_))),
        "{books:?}"
    );
    save_verse(&handle, 1, 3, "which God, who never lies, promised").unwrap();
    let second = handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert!(second.commit.is_some(), "HEAD moves: {second:?}");
    handle.close().unwrap();
    let handle = open(&rig, &folder);
    assert_eq!(handle.waiting_books(), books, "still waits");
    let report = handle.import_kept(&books[0].kept.copy).wait().unwrap();
    handle.wait_idle();
    assert_eq!(
        words(&handle, 1, 3).as_deref(),
        Some("which God, who never lies, promised"),
        "{report:?}"
    );
    assert!(
        report.to_review >= 1,
        "the file's 1:3 is to review: {report:?}"
    );
    handle.close().unwrap();
}

/// The same bytes held again after a second loss of the manifest, once HEAD moved with the
/// translator's saves: the new entry has no base (the copy was kept before, against an older
/// HEAD), so its Import never replaces the newer text without a question.
#[test]
fn bytes_held_again_after_a_second_lost_manifest_have_no_base() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 3, "which God promised").unwrap();
    leave(&handle);
    handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    handle.close().unwrap();
    let written = rig.fs.peek(&usfm_path(&folder)).unwrap();
    rig.fs.put(
        &usfm_path(&folder),
        &set_verse(&written, 1, 3, "which God promised long ago"),
        None,
    );
    let manifest = folder.join(".tc/materialized.json");
    rig.fs.delete(&manifest).unwrap();
    let handle = open(&rig, &folder);
    assert!(matches!(
        handle.waiting_books()[0].kept.base,
        KeptBase::Head(Some(_))
    ));
    save_verse(&handle, 1, 3, "which God, who never lies, promised").unwrap();
    let moved = handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert!(moved.commit.is_some(), "{moved:?}");
    handle.close().unwrap();
    rig.fs.advance(std::time::Duration::from_secs(5));
    rig.fs.delete(&manifest).unwrap();

    let handle = open(&rig, &folder);
    let books = handle.waiting_books();
    assert_eq!(books.len(), 1, "{books:?}");
    assert_eq!(books[0].kept.base, KeptBase::None, "{books:?}");
    let report = handle.import_kept(&books[0].kept.copy).wait().unwrap();
    handle.wait_idle();
    assert_eq!(
        words(&handle, 1, 3).as_deref(),
        Some("which God, who never lies, promised"),
        "{report:?}"
    );
    handle.close().unwrap();
}

/// After a lost manifest, a copy an earlier *Import* took in is not listed again; a copy
/// never imported still is.
#[test]
fn an_imported_copy_is_not_listed_again_after_the_manifest_is_lost() {
    let (rig, folder, written) = saved_and_closed();
    let first = set_verse(&written, 1, 2, "in hope of eternal life");
    rig.fs.put(&usfm_path(&folder), &first, None);
    let handle = open(&rig, &folder);
    rig.fs.advance(std::time::Duration::from_secs(5));
    let second = set_verse(&written, 1, 3, "which God promised");
    rig.fs.put(&usfm_path(&folder), &second, None);
    handle.focus().wait().unwrap();
    handle.wait_idle();
    let kept = handle.kept_copies();
    assert_eq!(kept.len(), 2, "{kept:?}");
    handle.import_kept(&kept[1].copy).wait().unwrap();
    handle.wait_idle();
    assert_eq!(handle.kept_copies(), vec![kept[0].clone()]);
    leave(&handle);
    handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    handle.close().unwrap();
    rig.fs
        .delete(&folder.join(".tc/materialized.json"))
        .unwrap();

    let handle = open(&rig, &folder);
    let listed: Vec<String> = handle.kept_copies().into_iter().map(|k| k.copy).collect();
    assert_eq!(listed, vec![kept[0].copy.clone()]);
    handle.close().unwrap();
}

/// The hold and its *Not now* killed at any write point, and then `.tc/materialized.json`
/// lost: the changed bytes are listed once, in a copy that holds them, whether the file still
/// holds them or *Not now* wrote it from the project (the copy is then all that tells).
#[test]
#[ignore = "nightly: a crash then a lost manifest; crash_at_every_write_point_of_the_hold_loses_nothing (one copy kept), crash_at_every_write_point_of_not_now_loses_nothing and a_kept_copy_is_listed_again_after_the_manifest_is_lost catch its bugs in the gate"]
fn crash_at_every_write_point_of_the_hold_then_a_lost_manifest_still_lists_the_copy_nightly() {
    let (rig, folder, written) = saved_and_closed();
    let base = rig.fs.crash(false);
    drop(rig);
    let edited = set_verse(&written, 1, 2, "in hope of eternal life");
    let path = usfm_path(&folder);
    let manifest = folder.join(".tc/materialized.json");
    let setup = |fs: &FaultFs, _: &ProjectHandle| fs.put(&path, &edited, None);
    let action = |handle: &ProjectHandle| {
        let _ = handle.focus().wait();
        handle.wait_idle();
        let _ = handle.not_now(USFM).wait();
        handle.wait_idle();
        Outcome::NotSaved
    };
    let between = |fs: &FaultFs| {
        let _ = fs.delete(&manifest);
    };
    let written_over = std::cell::Cell::new(0u64);
    let points = kill_everywhere_between(
        &base,
        &folder,
        &setup,
        &action,
        &between,
        &|handle, fs, _| {
            handle.wait_idle();
            assert_eq!(words(handle, 1, 2).as_deref(), Some("in hope"));
            let listed: Vec<Kept> = handle
                .kept_copies()
                .into_iter()
                .filter(|k| k.md5 == Md5::of(&edited).to_hex())
                .collect();
            assert_eq!(
                listed.len(),
                1,
                "the changed bytes are listed once: {:?}",
                handle.kept_copies()
            );
            assert_eq!(listed[0].path, USFM);
            assert_eq!(
                fs.peek(&folder.join(&listed[0].copy)),
                Some(edited.clone()),
                "the copy holds the bytes found"
            );
            if fs.peek(&path) != Some(edited.clone()) {
                written_over.set(written_over.get() + 1);
            }
        },
    );
    assert!(points > 4, "{points} kill points");
    assert!(written_over.get() > 0, "Not now never wrote the file over");
}

/// The base heads a live `placed:` entry names are in its path's heads file.
fn assert_bases_kept(fs: &FaultFs, folder: &Path, handle: &ProjectHandle) {
    let heads = fs
        .peek(&folder.join(".tc/heads/ingredients%2FTIT.usfm.bin"))
        .and_then(|b| tc_store::manifest::HeadsFile::parse(&b));
    for kept in handle.kept_copies() {
        if let KeptBase::Placed(md5) = &kept.base {
            assert!(
                heads.as_ref().is_some_and(|h| h.get(md5).is_some()),
                "a live entry's base heads left the heads file: {kept:?}"
            );
        }
    }
}

/// An *Import* killed at any write point imports the copy once or not at all: a verse only the
/// file changed is the file's after the import, never twice a version; a verse changed on both
/// sides keeps the project's text and is one conflict (a kill can land between the import's two
/// batches); the copy is kept, with its base, until the entry ends.
#[test]
fn crash_at_every_write_point_of_an_import_imports_once() {
    let (rig, folder, written) = saved_and_closed();
    let edited = set_verse(
        &set_verse(&written, 1, 2, "in hope of eternal life"),
        1,
        1,
        "Paul, a servant of God",
    );
    rig.fs.put(&usfm_path(&folder), &edited, None);
    let handle = open(&rig, &folder);
    assert_eq!(waiting(&handle), vec![USFM.to_owned()]);
    save_verse(&handle, 1, 1, "Paul, a slave of God").unwrap();
    handle.close().unwrap();
    let base = rig.fs.crash(false);
    drop(rig);
    let action = |handle: &ProjectHandle| {
        let copy = handle.kept_copies()[0].copy.clone();
        let _ = handle.import_kept(&copy).wait();
        handle.wait_idle();
        Outcome::NotSaved
    };
    let versions = |handle: &ProjectHandle, v: u16, text: &str| {
        handle
            .snapshot()
            .fold()
            .history(&verse(1, v))
            .into_iter()
            .filter(|id| {
                handle.snapshot().fold().body(id).is_some_and(|b| {
                    matches!(&b.record, tc_journal::Record::Verse {
                        value: tc_journal::record::VerseValue::Text { words, .. }, ..
                    } if words == text)
                })
            })
            .count()
    };
    let points = kill_everywhere_full(&base, &folder, &|_, _| {}, &action, &|handle, fs, _| {
        handle.wait_idle();
        assert_eq!(words(handle, 1, 1).as_deref(), Some("Paul, a slave of God"));
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
        assert_bases_kept(fs, &folder, handle);
        // An import run again adds nothing the first run wrote.
        if let Some(book) = handle.waiting_books().first() {
            handle.import_kept(&book.kept.copy).wait().unwrap();
            handle.wait_idle();
            assert_eq!(
                words(handle, 1, 2).as_deref(),
                Some("in hope of eternal life")
            );
        }
        assert_eq!(words(handle, 1, 1).as_deref(), Some("Paul, a slave of God"));
        assert_eq!(conflicts(handle), 1, "one conflict for 1:1");
        assert_eq!(
            versions(handle, 2, "in hope of eternal life"),
            1,
            "imported once"
        );
        assert_eq!(
            versions(handle, 1, "Paul, a servant of God"),
            1,
            "to review once"
        );
    });
    assert!(points > 4, "{points} kill points");
}

/// *Not now* killed at any write point: the project keeps its text, and the changed bytes are
/// listed once, in a copy that holds them, whether the file still holds them (it waits) or was
/// written from the project.
#[test]
fn crash_at_every_write_point_of_not_now_loses_nothing() {
    let (rig, folder, written) = saved_and_closed();
    let edited = set_verse(&written, 1, 2, "in hope of eternal life");
    rig.fs.put(&usfm_path(&folder), &edited, None);
    let handle = open(&rig, &folder);
    assert_eq!(waiting(&handle), vec![USFM.to_owned()]);
    handle.close().unwrap();
    let base = rig.fs.crash(false);
    drop(rig);
    let path = usfm_path(&folder);
    let action = |handle: &ProjectHandle| {
        let _ = handle.not_now(USFM).wait();
        handle.wait_idle();
        Outcome::NotSaved
    };
    let points = kill_everywhere_full(&base, &folder, &|_, _| {}, &action, &|handle, fs, _| {
        handle.wait_idle();
        assert_eq!(words(handle, 1, 2).as_deref(), Some("in hope"));
        let listed: Vec<Kept> = handle
            .kept_copies()
            .into_iter()
            .filter(|k| k.md5 == Md5::of(&edited).to_hex())
            .collect();
        assert_eq!(listed.len(), 1, "{:?}", handle.kept_copies());
        assert_eq!(fs.peek(&folder.join(&listed[0].copy)), Some(edited.clone()));
        if fs.peek(&path) == Some(edited.clone()) {
            assert_eq!(waiting(handle), vec![USFM.to_owned()], "asked again");
        } else {
            assert!(waiting(handle).is_empty());
        }
        assert_bases_kept(fs, &folder, handle);
    });
    assert!(points > 2, "{points} kill points");
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

const GATE_CASES: u32 = 16;
const NIGHTLY_CASES: u32 = 256;

proptest! {
    #![proptest_config(ProptestConfig { cases: GATE_CASES, failure_persistence: None, ..ProptestConfig::default() })]

    /// INV-NOTHING-ENTERS-WITHOUT-IMPORT and no loss: random histories of saves, outside edits,
    /// focus, *Not now*, *Import* and reopens keep every outside text and every save.
    #[test]
    fn no_outside_text_is_lost_across_not_now_import_and_reopen(
        ops in proptest::collection::vec(op(), 1..14)
    ) {
        run_ops(&ops);
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: NIGHTLY_CASES, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    #[ignore = "nightly: more cases of no_outside_text_is_lost_across_not_now_import_and_reopen, which runs in the gate"]
    fn no_outside_text_is_lost_across_not_now_import_and_reopen_nightly(
        ops in proptest::collection::vec(op(), 1..14)
    ) {
        run_ops(&ops);
    }
}

/// A project whose save of 1:1 "saved before the file was deleted" the journal lost (its
/// pending file deleted before a crash) while the book file the app wrote still shows it.
fn lost_work() -> (Rig, PathBuf) {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "saved before the file was deleted").unwrap();
    leave(&handle);
    let pending = pending_files(&rig.fs, &folder);
    assert_eq!(pending.len(), 1, "{pending:?}");
    rig.fs.delete(&folder.join(&pending[0])).unwrap();
    let fs = rig.fs.crash(false);
    drop(handle);
    (rig_as(fs, APP, "host-1"), folder)
}

/// The one `why: lost` entry the open of `folder` lists, with its copy checked.
fn handle_kept(rig: &Rig, folder: &Path) -> Vec<Kept> {
    let handle = open(rig, folder);
    let kept = handle.kept_copies();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(kept[0].why, tc_store::KeptWhy::Lost);
    assert!(matches!(kept[0].base, KeptBase::Placed(_)), "{kept:?}");
    let copy = rig.fs.peek(&folder.join(&kept[0].copy)).unwrap();
    assert!(String::from_utf8_lossy(&copy).contains("saved before the file was deleted"));
    handle.close().unwrap();
    kept
}

/// H22 part 1: lost work is asked about until the person answers, after a reload too; *Not
/// now* answers it for good and writes nothing else: the copy stays listed with *Bring it
/// back*, and *Dismiss* is refused, since the copy may be the work's only one.
#[test]
fn lost_work_is_asked_until_not_now_and_is_never_dismissed() {
    let (rig, folder) = lost_work();
    let kept = handle_kept(&rig, &folder);
    let handle = open(&rig, &folder);
    let books = handle.waiting_books();
    assert_eq!(books.len(), 1, "asked again after a reload: {books:?}");
    assert_eq!(books[0].kept.why, tc_store::KeptWhy::Lost);
    let before = files(&rig.fs, &folder);
    handle.not_now(USFM).wait().unwrap();
    handle.wait_idle();
    assert!(waiting(&handle).is_empty());
    assert_eq!(handle.kept_copies().len(), 1);
    let changed: Vec<String> = files(&rig.fs, &folder)
        .into_iter()
        .filter(|(p, b)| before.get(p) != Some(b))
        .map(|(p, _)| p)
        .collect();
    assert_eq!(
        changed,
        vec![".tc/materialized.json".to_owned()],
        "only the answer is written"
    );
    let dismissed = handle.dismiss_kept(&kept[0].copy).wait();
    assert!(
        matches!(&dismissed, Err(tc_store::StoreError::LostWorkKept { copy }) if *copy == kept[0].copy),
        "{dismissed:?}"
    );
    assert!(
        rig.fs.peek(&folder.join(&kept[0].copy)).is_some(),
        "the copy stays"
    );
    handle.close().unwrap();
    let handle = open(&rig, &folder);
    assert!(waiting(&handle).is_empty(), "never asked again");
    assert_eq!(handle.kept_copies().len(), 1, "still listed");
    let report = handle.import_kept(&kept[0].copy).wait().unwrap();
    handle.wait_idle();
    assert_eq!((report.imported, report.to_review), (1, 0), "{report:?}");
    handle.close().unwrap();
}

/// After a lost manifest, lost work is listed as lost again (its mark beside the copy), and
/// *Bring it back* still restores it.
#[test]
fn lost_work_stays_lost_work_after_the_manifest_is_lost() {
    let (rig, folder) = lost_work();
    handle_kept(&rig, &folder);
    rig.fs
        .delete(&folder.join(".tc/materialized.json"))
        .unwrap();
    let handle = open(&rig, &folder);
    let kept = handle.kept_copies();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(kept[0].why, tc_store::KeptWhy::Lost, "{kept:?}");
    let report = handle.import_kept(&kept[0].copy).wait().unwrap();
    handle.wait_idle();
    assert_eq!((report.imported, report.to_review), (1, 0), "{report:?}");
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before the file was deleted")
    );
    handle.close().unwrap();
}

/// A write that fails while lost work is kept (the `.lost` mark among them), then a lost
/// manifest: the save is still in the book file and asked about again, or listed as lost work,
/// which cannot be dismissed. Never an outside change Dismiss could delete.
#[test]
fn a_failed_write_while_keeping_lost_work_never_makes_it_dismissable() {
    let mut reached = 0;
    for n in 0..40 {
        let (rig, folder) = lost_work();
        rig.fs.fail_op(n, std::io::ErrorKind::StorageFull);
        if let Ok(handle) = rig.store.open(&folder, &OpenOptions::default()) {
            handle.wait_idle();
            handle.close().ok();
        }
        let _ = rig.fs.delete(&folder.join(".tc/materialized.json"));
        let handle = open(&rig, &folder);
        let saved = "saved before the file was deleted";
        let in_file =
            String::from_utf8_lossy(&rig.fs.peek(&usfm_path(&folder)).unwrap()).contains(saved);
        for k in handle.kept_copies() {
            let copy = rig.fs.peek(&folder.join(&k.copy)).unwrap_or_default();
            if String::from_utf8_lossy(&copy).contains(saved) && !in_file {
                assert_eq!(k.why, tc_store::KeptWhy::Lost, "fail point {n}: {k:?}");
                reached += 1;
            }
        }
        handle.close().ok();
    }
    assert!(reached > 0, "no fail point left the save only in a copy");
}

/// H22 part 1, row N4: a save the journal lost (its pending file deleted before a crash) that
/// the book file the app wrote still shows. The text is kept in a copy listed with
/// `why: lost` and asked about once; nothing comes back on its own; the file is written from
/// the project. *Bring it back* (the copy's *Import*) restores the save, and the entry ends.
#[test]
fn a_lost_work_copy_offers_bring_it_back() {
    let (rig, folder) = lost_work();
    let kept = handle_kept(&rig, &folder);
    let handle = open(&rig, &folder);
    assert_eq!(waiting(&handle), vec![USFM.to_owned()], "asked once");
    assert_ne!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before the file was deleted"),
        "nothing comes back on its own"
    );
    let file = String::from_utf8(rig.fs.peek(&usfm_path(&folder)).unwrap()).unwrap();
    assert!(
        !file.contains("saved before the file was deleted"),
        "written from the project"
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);

    let handle = open(&rig, &folder);
    assert_eq!(handle.kept_copies(), kept, "listed again after a reopen");
    let report = handle.import_kept(&kept[0].copy).wait().unwrap();
    handle.wait_idle();
    assert_eq!((report.imported, report.to_review), (1, 0), "{report:?}");
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("saved before the file was deleted")
    );
    assert!(handle.kept_copies().is_empty());
    let file = String::from_utf8(rig.fs.peek(&usfm_path(&folder)).unwrap()).unwrap();
    assert!(file.contains("saved before the file was deleted"), "{file}");
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// H22 part 4: a project opened read-only (here a full disk) keeps no copy, asks nothing and
/// writes nothing; it shows a notice that names the changed book file. Opened normally, the
/// book is asked about.
#[test]
fn a_read_only_open_asks_nothing_and_shows_a_notice() {
    let (rig, folder, written) = saved_and_closed();
    let edited = set_verse(&written, 1, 2, "in hope of eternal life");
    rig.fs.put(&usfm_path(&folder), &edited, None);
    let before = files(&rig.fs, &folder);
    rig.fs.fail_writes(Some(std::io::ErrorKind::StorageFull));
    let handle = open(&rig, &folder);
    assert!(
        matches!(handle.facts().mode, tc_store::Mode::ReadOnly(_)),
        "{:?}",
        handle.facts().mode
    );
    assert!(waiting(&handle).is_empty());
    assert!(handle.kept_copies().is_empty());
    let told: Vec<String> = handle
        .notices()
        .into_iter()
        .filter_map(|n| match n.notice {
            Notice::ChangedWhileReadOnly { path } => Some(path.as_str().to_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(told, vec![USFM.to_owned()], "{:?}", handle.notices());
    handle.close().unwrap();
    assert_eq!(files(&rig.fs, &folder), before, "nothing was written");
    rig.fs.fail_writes(None);
    let handle = open(&rig, &folder);
    assert_eq!(waiting(&handle), vec![USFM.to_owned()]);
    handle.close().unwrap();
}

/// H22 part 4: with a full disk the copy cannot be made, so a changed book file is left exactly
/// as it is and nothing is listed; once there is space, the next look keeps and asks.
#[test]
fn a_full_disk_leaves_a_changed_book_file_as_it_is_until_the_copy_can_be_made() {
    let (rig, folder, written) = saved_and_closed();
    let handle = open(&rig, &folder);
    let edited = set_verse(&written, 1, 2, "in hope of eternal life");
    rig.fs.put(&usfm_path(&folder), &edited, None);
    rig.fs.fail_writes(Some(std::io::ErrorKind::StorageFull));
    let _ = handle.focus().wait();
    handle.wait_idle();
    assert_eq!(rig.fs.peek(&usfm_path(&folder)), Some(edited.clone()));
    assert!(
        handle.kept_copies().is_empty(),
        "{:?}",
        handle.kept_copies()
    );
    assert_eq!(words(&handle, 1, 2).as_deref(), Some("in hope"));
    rig.fs.fail_writes(None);
    handle.close().unwrap();
    assert_eq!(rig.fs.peek(&usfm_path(&folder)), Some(edited.clone()));
    let handle = open(&rig, &folder);
    let books = handle.waiting_books();
    assert_eq!(books.len(), 1, "{books:?}");
    assert_copy(&rig, &folder, &books[0].kept, &edited);
    assert_eq!(rig.fs.peek(&usfm_path(&folder)), Some(edited));
    handle.close().unwrap();
}

/// A lost save over a verse that had text: *Bring it back* restores it with no question while
/// the project holds nothing newer for the verse, and makes it a conflict once the person
/// saved the verse again after the loss (H22 part 1).
#[test]
fn bring_it_back_restores_a_lost_save_over_text_unless_newer_work_exists() {
    for newer in [false, true] {
        let (rig, folder) = titus();
        let handle = open(&rig, &folder);
        save_verse(&handle, 1, 1, "the text before").unwrap();
        leave(&handle);
        handle
            .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
            .wait()
            .unwrap();
        let before = pending_files(&rig.fs, &folder);
        save_verse(&handle, 1, 1, "the lost save").unwrap();
        leave(&handle);
        let pending: Vec<String> = pending_files(&rig.fs, &folder)
            .into_iter()
            .filter(|p| !before.contains(p))
            .collect();
        assert_eq!(pending.len(), 1, "{pending:?}");
        rig.fs.delete(&folder.join(&pending[0])).unwrap();
        let fs = rig.fs.crash(false);
        drop(handle);
        let rig = rig_as(fs, APP, "host-1");
        let handle = open(&rig, &folder);
        assert_eq!(words(&handle, 1, 1).as_deref(), Some("the text before"));
        let kept = handle.kept_copies();
        assert_eq!(kept.len(), 1, "{kept:?}");
        if newer {
            save_verse(&handle, 1, 1, "saved after the loss").unwrap();
        }
        let report = handle.import_kept(&kept[0].copy).wait().unwrap();
        handle.wait_idle();
        if newer {
            assert_eq!((report.imported, report.to_review), (0, 1), "{report:?}");
            assert_eq!(
                words(&handle, 1, 1).as_deref(),
                Some("saved after the loss")
            );
            assert_eq!(conflicts(&handle), 1);
        } else {
            assert_eq!((report.imported, report.to_review), (1, 0), "{report:?}");
            assert_eq!(words(&handle, 1, 1).as_deref(), Some("the lost save"));
            assert_eq!(conflicts(&handle), 0);
        }
        handle.close().unwrap();
    }
}
