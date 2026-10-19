//! Edits made outside the app are recorded (`docs/STORAGE.md` §11.1, §6.2): a derived file
//! another program changed is decoded, compared with what the app wrote, and recorded as
//! changes of its own kind with the app's version as their parents; a destructive or
//! reverting edit is a conflict; the bytes found are kept whole in `.tc/external/`; the
//! folder is a valid burrito after every step.

use crate::common::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tc_journal::fold::{ConflictKind, Notice};
use tc_journal::{BookId, RecordKey};
use tc_store::fault::FaultFs;
use tc_store::{Hooks, Leave, OpenOptions, Point, ProjectHandle};

const USFM: &str = "ingredients/TIT.usfm";

fn open(rig: &Rig, folder: &Path) -> ProjectHandle {
    let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle
}

fn leave(handle: &ProjectHandle) {
    handle.leave(Leave::Book(TIT)).wait().unwrap();
}

fn usfm_path(folder: &Path) -> PathBuf {
    folder.join(USFM)
}

/// `bytes` with the text of TIT c:v replaced. A verse the project has no text for shows
/// `___` in the file.
fn set_verse(bytes: &[u8], c: u16, v: u16, text: &str) -> Vec<u8> {
    let s = String::from_utf8(bytes.to_vec()).unwrap();
    let chapter = s.find(&format!("\n\\c {c}\n")).unwrap();
    let marker = format!("\\v {v} ");
    let at = chapter + s[chapter..].find(&marker).unwrap() + marker.len();
    let end = at + s[at..].find('\n').unwrap();
    format!("{}{}{}", &s[..at], text, &s[end..]).into_bytes()
}

fn recorded(handle: &ProjectHandle) -> Vec<(u32, bool)> {
    handle
        .notices()
        .into_iter()
        .filter_map(|n| match n.notice {
            Notice::OutsideEditRecorded {
                path,
                changes,
                unknown_base,
                ..
            } if path.as_str() == USFM => Some((changes, unknown_base)),
            _ => None,
        })
        .collect()
}

fn verse_conflicts(handle: &ProjectHandle) -> Vec<RecordKey> {
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
        .map(|c| c.key.clone())
        .collect()
}

/// A project with 1:1 saved and written, closed.
fn saved_and_closed() -> (Rig, PathBuf) {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    leave(&handle);
    handle.close().unwrap();
    (rig, folder)
}

// ---------------------------------------------------------------------------------------
// X1 to X3: an edit of a verse
// ---------------------------------------------------------------------------------------

/// The edit made while the app was closed is recorded with the version the app wrote as its
/// parent, so it is no conflict; the bytes found are kept; the file shows the project again.
#[test]
fn an_edit_made_while_the_app_was_closed_is_recorded_and_agrees_with_the_project() {
    let (rig, folder) = saved_and_closed();
    let edited = set_verse(
        &rig.fs.peek(&usfm_path(&folder)).unwrap(),
        1,
        2,
        "in hope of eternal life",
    );
    rig.fs.put(&usfm_path(&folder), &edited, None);
    let handle = open(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 2).as_deref(),
        Some("in hope of eternal life")
    );
    assert_eq!(words(&handle, 1, 1).as_deref(), Some("Paul, a servant"));
    assert_eq!(
        recorded(&handle),
        vec![(1, false)],
        "{:?}",
        handle.notices()
    );
    assert!(verse_conflicts(&handle).is_empty(), "the edit has a parent");
    let copies = copies_of(&handle, USFM);
    assert_eq!(copies.len(), 1);
    assert_eq!(
        rig.fs.peek(&folder.join(&copies[0])),
        Some(edited.clone()),
        "the bytes found are kept whole"
    );
    assert_eq!(
        rig.fs.peek(&usfm_path(&folder)),
        Some(edited),
        "the file shows the project, which now shows the edit"
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);

    // The next open finds the file as the app wrote it: nothing more is recorded.
    let handle = open(&rig, &folder);
    assert!(recorded(&handle).is_empty());
    assert_eq!(copies_of(&handle, USFM).len(), 0);
    handle.close().unwrap();
}

/// L40: an edit made after a save and before the save's materialization is recorded, never
/// overwritten, and the save stays.
#[test]
fn loss_materializer_never_overwrites_outside_edit() {
    let hooks = Arc::new(Hooks::default());
    let rig = rig_with_hooks(FaultFs::new(), hooks.clone());
    rig.store.wait_cleanup();
    let folder = create_titus(&rig, "Titus");
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    hooks.hold(Point::BeforeMaterialize);
    let left = handle.leave(Leave::Book(TIT));
    hooks.wait_reached(Point::BeforeMaterialize, 1);
    let edited = set_verse(
        &rig.fs.peek(&usfm_path(&folder)).unwrap(),
        1,
        2,
        "in hope of eternal life",
    );
    rig.fs.put(&usfm_path(&folder), &edited, None);
    hooks.release(Point::BeforeMaterialize);
    left.wait().unwrap();
    handle.wait_idle();
    assert_eq!(
        words(&handle, 1, 2).as_deref(),
        Some("in hope of eternal life")
    );
    assert_eq!(words(&handle, 1, 1).as_deref(), Some("Paul, a servant"));
    let copies = copies_of(&handle, USFM);
    assert_eq!(copies.len(), 1, "{:?}", handle.notices());
    assert_eq!(rig.fs.peek(&folder.join(&copies[0])), Some(edited));
    let written = String::from_utf8(rig.fs.peek(&usfm_path(&folder)).unwrap()).unwrap();
    assert!(written.contains("Paul, a servant") && written.contains("in hope of eternal life"));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// L41: a file cut down to its first chapter records no deletion that overrides the
/// project's text: each verse with text that it lost is an unknown-base removal that
/// conflicts with that text, which stays what the project shows; the words are written again.
#[test]
fn loss_outside_mass_delete_is_conflict() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    save_verse(&handle, 2, 1, "speak what fits sound teaching").unwrap();
    save_verse(&handle, 3, 1, "remind them to be subject").unwrap();
    leave(&handle);
    handle.close().unwrap();
    let text = String::from_utf8(rig.fs.peek(&usfm_path(&folder)).unwrap()).unwrap();
    let cut = text.find("\n\\c 2\n").unwrap();
    let truncated = text.as_bytes()[..cut + 1].to_vec();
    rig.fs.put(&usfm_path(&folder), &truncated, None);

    let handle = open(&rig, &folder);
    assert_eq!(
        words(&handle, 2, 1).as_deref(),
        Some("speak what fits sound teaching"),
        "the project keeps its text"
    );
    assert_eq!(
        words(&handle, 3, 1).as_deref(),
        Some("remind them to be subject")
    );
    let conflicts = verse_conflicts(&handle);
    assert!(
        conflicts
            .iter()
            .any(|k| matches!(k, RecordKey::Verse(_, c, v) if c.get() == 2 && v.get() == 1)),
        "{conflicts:?}"
    );
    assert!(
        conflicts
            .iter()
            .any(|k| matches!(k, RecordKey::Verse(_, c, v) if c.get() == 3 && v.get() == 1)),
        "{conflicts:?}"
    );
    let copies = copies_of(&handle, USFM);
    assert_eq!(copies.len(), 1);
    assert!(
        rig.fs.peek(&folder.join(&copies[0])) == Some(truncated),
        "the cut file is kept whole"
    );
    // The written file keeps every word the person typed. The blank verses the cut file
    // lost carry no text and follow the file; *Undo the outside edit* brings them back.
    let written = String::from_utf8(rig.fs.peek(&usfm_path(&folder)).unwrap()).unwrap();
    for typed in [
        "Paul, a servant",
        "speak what fits sound teaching",
        "remind them to be subject",
    ] {
        assert!(written.contains(typed), "{typed}: {written}");
    }
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// L42: an older copy of the file put back is a revert, a conflict with the newer text and
/// not a quiet return to the old words.
#[test]
fn loss_outside_revert_is_conflict() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    leave(&handle);
    let older = rig.fs.peek(&usfm_path(&folder)).unwrap();
    save_verse(&handle, 1, 1, "Paul, a slave of God").unwrap();
    leave(&handle);
    handle.close().unwrap();
    let newer = rig.fs.peek(&usfm_path(&folder)).unwrap();
    assert_ne!(older, newer);
    rig.fs.put(&usfm_path(&folder), &older, None);

    let handle = open(&rig, &folder);
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("Paul, a slave of God"),
        "the text the person wrote last is the one shown"
    );
    let conflicts = verse_conflicts(&handle);
    assert_eq!(conflicts.len(), 1, "{conflicts:?}");
    assert_eq!(rig.fs.peek(&usfm_path(&folder)), Some(newer));
    assert_eq!(copies_of(&handle, USFM).len(), 1);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// L61: a file the decoder cannot read (a verse number twice) is told in words with the
/// copy's name, the project keeps its text, and the file is written again so that the folder
/// is a burrito whose files say what the project says.
#[test]
fn loss_unreadable_outside_edit_copy_kept() {
    let (rig, folder) = saved_and_closed();
    let text = String::from_utf8(rig.fs.peek(&usfm_path(&folder)).unwrap()).unwrap();
    let broken = text.replacen("\\v 3 ___", "\\v 2 ___", 1).into_bytes();
    assert_ne!(broken, text.as_bytes());
    rig.fs.put(&usfm_path(&folder), &broken, None);
    let handle = open(&rig, &folder);
    let told = handle.notices().into_iter().find_map(|n| match n.notice {
        Notice::OutsideEditUnreadable { path, copy, reason } if path.as_str() == USFM => {
            Some((copy, reason))
        }
        _ => None,
    });
    let (copy, reason) = told.expect("the person is told");
    assert!(!reason.is_empty());
    assert_eq!(rig.fs.peek(&folder.join(&*copy)), Some(broken));
    assert_eq!(words(&handle, 1, 1).as_deref(), Some("Paul, a servant"));
    assert_eq!(
        rig.fs.peek(&usfm_path(&folder)),
        Some(text.into_bytes()),
        "written again from the project"
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// Line endings and blanks at line ends are no edit: an editor that rewrote the file with CRLF
/// and trailing blanks records nothing, and the file is written again with the app's line
/// endings. The digests of the records compare after the blanks are normalized, so no key is
/// looked at.
#[test]
fn a_whitespace_only_rewrite_records_nothing() {
    let (rig, folder) = saved_and_closed();
    let original = rig.fs.peek(&usfm_path(&folder)).unwrap();
    let text = String::from_utf8(original.clone()).unwrap();
    let crlf = text.replace('\n', "  \r\n").into_bytes();
    rig.fs.put(&usfm_path(&folder), &crlf, None);
    let handle = open(&rig, &folder);
    assert!(recorded(&handle).is_empty(), "{:?}", handle.notices());
    assert_eq!(handle.counters().outside_keys_examined, 0);
    assert!(rig.fs.peek(&usfm_path(&folder)) == Some(original));
    // The person is told the file was rewritten and its line endings kept.
    let told: Vec<u32> = handle
        .notices()
        .into_iter()
        .filter_map(|n| match n.notice {
            Notice::WhitespaceOnlyEdit { path, keys } if path.as_str() == USFM => Some(keys),
            _ => None,
        })
        .collect();
    assert_eq!(told.len(), 1, "{:?}", handle.notices());
    assert!(told[0] > 0);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// One real typo among a file's rewritten line endings records exactly one change.
#[test]
fn a_typo_fixed_in_a_file_with_rewritten_line_ends_records_one_change() {
    let (rig, folder) = saved_and_closed();
    let text = String::from_utf8(rig.fs.peek(&usfm_path(&folder)).unwrap()).unwrap();
    let crlf = text
        .replace('\n', "\r\n")
        .replace("Paul, a servant\r\n", "Paul, a servant of God\r\n");
    rig.fs.put(&usfm_path(&folder), crlf.as_bytes(), None);
    let handle = open(&rig, &folder);
    assert_eq!(
        recorded(&handle),
        vec![(1, false)],
        "{:?}",
        handle.notices()
    );
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("Paul, a servant of God")
    );
    assert!(verse_conflicts(&handle).is_empty());
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

// ---------------------------------------------------------------------------------------
// Sidecars, metadata.json and OBS stories
// ---------------------------------------------------------------------------------------

const TN: &str = "ingredients/checking/translationNotes/TIT.json";
const ALIGNMENTS: &str = "ingredients/checking/alignments/TIT.json";

fn recorded_for(handle: &ProjectHandle, rel: &str) -> Vec<(u32, bool)> {
    handle
        .notices()
        .into_iter()
        .filter_map(|n| match n.notice {
            Notice::OutsideEditRecorded {
                path,
                changes,
                unknown_base,
                ..
            } if path.as_str() == rel => Some((changes, unknown_base)),
            _ => None,
        })
        .collect()
}

fn replaced(rig: &Rig, folder: &Path, rel: &str, from: &str, to: &str) -> Vec<u8> {
    let text = String::from_utf8(rig.fs.peek(&folder.join(rel)).unwrap()).unwrap();
    assert!(text.contains(from), "{rel} has no {from}");
    let edited = text.replacen(from, to, 1).into_bytes();
    rig.fs.put(&folder.join(rel), &edited, None);
    edited
}

/// A decision edited in a file is recorded; a flag the app derives (X3) is the app's to
/// write, so an edit of it alone records nothing and is written over.
#[test]
fn an_outside_edit_of_a_sidecar_records_decisions_and_ignores_flags() {
    let (fs, folder) = crate::commands::rich();
    let (rig, handle) = crate::commands::open(&fs, &folder);
    handle.close().unwrap();
    let before = rig.fs.peek(&folder.join(TN)).unwrap();
    // A flag alone: `invalidated` is derived from the verse text and the judgement.
    replaced(
        &rig,
        &folder,
        TN,
        "\"invalidated\":true",
        "\"invalidated\":false",
    );
    let handle = open(&rig, &folder);
    assert!(
        recorded_for(&handle, TN).is_empty(),
        "{:?}",
        handle.notices()
    );
    assert!(
        rig.fs.peek(&folder.join(TN)) == Some(before.clone()),
        "the flag is written as the app derives it"
    );
    handle.close().unwrap();
    // The status the person chose: a decision.
    let edited = replaced(
        &rig,
        &folder,
        TN,
        "\"userStatus\":\"valid\"",
        "\"userStatus\":\"todo\"",
    );
    let handle = open(&rig, &folder);
    assert_eq!(
        recorded_for(&handle, TN),
        vec![(1, false)],
        "{:?}",
        handle.notices()
    );
    let written = String::from_utf8(rig.fs.peek(&folder.join(TN)).unwrap()).unwrap();
    assert!(written.contains("\"userStatus\":\"todo\""), "{written}");
    let copies = copies_of(&handle, TN);
    assert_eq!(copies.len(), 1);
    assert!(rig.fs.peek(&folder.join(&copies[0])) == Some(edited));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// An alignment edited in a file is recorded with the app's version as its parent.
#[test]
fn an_outside_edit_of_an_alignment_file_is_recorded() {
    let (fs, folder) = crate::commands::rich();
    let (rig, handle) = crate::commands::open(&fs, &folder);
    handle.close().unwrap();
    replaced(
        &rig,
        &folder,
        ALIGNMENTS,
        "\"word\":\"Paul\"",
        "\"word\":\"Paulus\"",
    );
    let handle = open(&rig, &folder);
    assert_eq!(
        recorded_for(&handle, ALIGNMENTS).len(),
        1,
        "{:?}",
        handle.notices()
    );
    assert!(
        handle.snapshot().fold().conflicts().next().is_some(),
        "the project's own conflicts are still listed"
    );
    let written = String::from_utf8(rig.fs.peek(&folder.join(ALIGNMENTS)).unwrap()).unwrap();
    assert!(written.contains("Paulus"), "{written}");
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// L92, the outside half: the project name and a field the app does not model in
/// `metadata.json` are the person's to change there; derived fields are not.
#[test]
fn loss_metadata_unmodelled_fields_kept() {
    let (rig, folder) = saved_and_closed();
    let rel = "metadata.json";
    let text = String::from_utf8(rig.fs.peek(&folder.join(rel)).unwrap()).unwrap();
    let renamed = text.replacen("Titus test translation", "Titus, edited by hand", 1);
    assert_ne!(text, renamed);
    rig.fs.put(&folder.join(rel), renamed.as_bytes(), None);
    let handle = open(&rig, &folder);
    assert_eq!(
        recorded_for(&handle, rel).len(),
        1,
        "{:?}",
        handle.notices()
    );
    let written = String::from_utf8(rig.fs.peek(&folder.join(rel)).unwrap()).unwrap();
    assert!(written.contains("Titus, edited by hand"), "{written}");
    let copies = copies_of(&handle, rel);
    assert_eq!(copies.len(), 1, "{:?} {:?}", copies, handle.notices());
    assert!(rig.fs.peek(&folder.join(&copies[0])) == Some(renamed.into_bytes()));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// L92: a link edited in an Open Bible Stories template is recorded as an edit of the story.
#[test]
fn loss_obs_template_edit_recorded() {
    let (fs, folder) = crate::commands::stories();
    let (rig, handle) = crate::commands::open(&fs, &folder);
    handle.close().unwrap();
    let rel = "ingredients/content/01.md";
    let edited = replaced(
        &rig,
        &folder,
        rel,
        "obs-en-01-02.jpg",
        "obs-en-01-02-new.jpg",
    );
    let handle = open(&rig, &folder);
    assert_eq!(
        recorded_for(&handle, rel).len(),
        1,
        "{:?}",
        handle.notices()
    );
    let written = rig.fs.peek(&folder.join(rel)).unwrap();
    assert!(written == edited, "the story shows its edit");
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

// ---------------------------------------------------------------------------------------
// The heads file as the base, and what an outside edit costs
// ---------------------------------------------------------------------------------------

/// A heads file whose records hold no digests (as S1's did): the edit is recorded just the
/// same, and the cost is the book's, by design (every record is compared).
#[test]
fn a_heads_file_without_digests_still_records_an_outside_edit() {
    let (rig, folder) = saved_and_closed();
    let heads = folder.join(".tc/heads/ingredients%2FTIT.usfm.bin");
    let mut file = tc_store::manifest::HeadsFile::parse(&rig.fs.peek(&heads).unwrap()).unwrap();
    let records = file
        .placements
        .iter_mut()
        .flat_map(|p| p.records.iter_mut());
    let mut digests = 0;
    for record in records {
        digests += usize::from(record.shown.take().is_some());
    }
    assert!(digests > 0);
    rig.fs.put(&heads, &file.to_bytes(), None);
    let edited = set_verse(
        &rig.fs.peek(&usfm_path(&folder)).unwrap(),
        1,
        2,
        "in hope of eternal life",
    );
    rig.fs.put(&usfm_path(&folder), &edited, None);
    let handle = open(&rig, &folder);
    assert_eq!(
        recorded(&handle),
        vec![(1, false)],
        "{:?}",
        handle.notices()
    );
    assert!(
        handle.counters().outside_keys_examined > 3,
        "every record of the book was compared: {:?}",
        handle.counters()
    );
    assert!(verse_conflicts(&handle).is_empty());
    assert_eq!(
        words(&handle, 1, 2).as_deref(),
        Some("in hope of eternal life")
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// With the heads file gone the base is unknown: the changed record is recorded without
/// parents and so conflicts with the project's text, which stays what is shown.
#[test]
fn an_edit_with_no_heads_file_is_an_unknown_base_conflict() {
    let (rig, folder) = saved_and_closed();
    rig.fs
        .delete(&folder.join(".tc/heads/ingredients%2FTIT.usfm.bin"))
        .unwrap();
    let text = String::from_utf8(rig.fs.peek(&usfm_path(&folder)).unwrap()).unwrap();
    let edited = text
        .replacen("Paul, a servant", "Paul, a slave", 1)
        .into_bytes();
    rig.fs.put(&usfm_path(&folder), &edited, None);
    let handle = open(&rig, &folder);
    assert_eq!(recorded(&handle), vec![(1, true)], "{:?}", handle.notices());
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("Paul, a servant"),
        "the project's text is the one shown"
    );
    assert_eq!(verse_conflicts(&handle).len(), 1);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// §14: reconciling an edit costs what it changed. Three verses of `PSA.usfm` are replaced
/// while the app is closed; the open reads that one file and its heads file, decodes it once,
/// compares the three records that differ and writes three changes, whatever the size of the
/// project.
#[test]
fn reconciling_an_outside_edit_costs_what_it_changed() {
    use crate::corpus::{psalms, psalms_x10};
    let mut costs = Vec::new();
    for corpus in [psalms(), psalms_x10()] {
        let fs = corpus.fs.copy();
        // The entries settle once their time stamp's tick is over.
        fs.advance(std::time::Duration::from_secs(5));
        let rig = rig_as(fs.clone(), APP, "host-1");
        let handle = rig
            .store
            .open(&corpus.folder, &OpenOptions::default())
            .unwrap();
        handle.wait_idle();
        let conflicts_before = verse_conflicts_of(&handle, BookId::Psalms).len();
        handle.close().unwrap();
        fs.advance(std::time::Duration::from_secs(5));
        let path = corpus.folder.join("ingredients/PSA.usfm");
        let mut bytes = fs.peek(&path).unwrap();
        for v in 1..=3 {
            bytes = set_verse(&bytes, 1, v, &format!("an outside edit of verse {v}"));
        }
        fs.put(&path, &bytes, None);
        let rig = rig_as(fs.clone(), APP, "host-1");
        let handle = rig
            .store
            .open(&corpus.folder, &OpenOptions::default())
            .unwrap();
        let usable = handle.counters();
        handle.wait_idle();
        let after = handle.counters();
        assert_eq!(usable.derived_read, 1, "{usable:?}");
        assert_eq!(usable.heads_read, 1, "{usable:?}");
        assert_eq!(after.outside_decodes, 1, "{after:?}");
        assert_eq!(after.outside_keys_examined, 3, "{after:?}");
        assert_eq!(
            verse_conflicts_of(&handle, BookId::Psalms).len(),
            conflicts_before,
            "the edit has parents, so it conflicts with nothing"
        );
        costs.push((
            after.outside_decodes,
            after.outside_keys_examined,
            usable.derived_read,
            usable.heads_read,
        ));
        handle.close().unwrap();
    }
    assert_eq!(costs[0], costs[1], "the cost does not follow the size");
}

/// The same edit told by the watcher to a project that is open: one event for the file, and
/// the examination reads that file and its heads file, decodes once, compares the three
/// records that differ and writes three changes, however large the project is; no other path
/// is looked at.
#[test]
fn a_watcher_event_for_three_changed_verses_costs_what_they_changed() {
    use crate::corpus::{psalms, psalms_x10};
    let mut costs = Vec::new();
    for corpus in [psalms(), psalms_x10()] {
        let fs = corpus.fs.copy();
        fs.advance(std::time::Duration::from_secs(5));
        let rig = rig_as(fs.clone(), APP, "host-1");
        let handle = rig
            .store
            .open(&corpus.folder, &OpenOptions::default())
            .unwrap();
        handle.wait_idle();
        fs.advance(std::time::Duration::from_secs(5));
        let path = corpus.folder.join("ingredients/PSA.usfm");
        let mut bytes = fs.peek(&path).unwrap();
        for v in 1..=3 {
            bytes = set_verse(&bytes, 1, v, &format!("an outside edit of verse {v}"));
        }
        let before = handle.counters();
        fs.put(&path, &bytes, None);
        eventually("the watcher never recorded the edit", || {
            handle.notices().iter().any(|n| {
                matches!(
                    &n.notice,
                    Notice::OutsideEditRecorded { path, .. } if path.as_str() == "ingredients/PSA.usfm"
                )
            })
        });
        handle.wait_idle();
        let cost = handle.counters().since(&before);
        // The file the watcher told is read to examine it, with its heads. What the writer then
        // reads and writes is the rewrite of the book's files from the project (each file the
        // ledger is asked about has its heads read once in the session), which the 3 s timer
        // of the batch may repeat when the machine is slow: the counts are the same for both
        // corpora (below).
        assert!(cost.derived_read >= 1, "{cost:?}");
        assert!(cost.heads_read >= 1, "{cost:?}");
        assert_eq!(cost.outside_decodes, 1, "{cost:?}");
        assert_eq!(cost.outside_keys_examined, 3, "{cost:?}");
        let changes: Vec<u32> = handle
            .notices()
            .into_iter()
            .filter_map(|n| match n.notice {
                Notice::OutsideEditRecorded { changes, .. } => Some(changes),
                _ => None,
            })
            .collect();
        assert_eq!(changes, vec![3]);
        costs.push((
            cost.heads_read,
            cost.outside_decodes,
            cost.outside_keys_examined,
        ));
        handle.close().unwrap();
    }
    assert_eq!(costs[0], costs[1], "the cost does not follow the size");
}

fn verse_conflicts_of(handle: &ProjectHandle, book: BookId) -> Vec<RecordKey> {
    handle
        .snapshot()
        .fold()
        .conflicts()
        .filter(|c| matches!(&c.key, RecordKey::Verse(b, ..) if *b == book))
        .map(|c| c.key.clone())
        .collect()
}

// ---------------------------------------------------------------------------------------
// The watcher, the window focus, new files and the held file's choices
// ---------------------------------------------------------------------------------------

/// Waits up to ten seconds for `ready`.
fn eventually(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !ready() {
        assert!(std::time::Instant::now() < deadline, "{what}");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// §11.1: a program that saves a derived file while the app is open is told by the
/// filesystem; the edit is recorded without anyone looking at the window.
#[test]
fn the_watcher_records_an_edit_without_a_focus() {
    let (rig, folder) = saved_and_closed();
    let handle = open(&rig, &folder);
    let edited = set_verse(
        &rig.fs.peek(&usfm_path(&folder)).unwrap(),
        1,
        2,
        "in hope of eternal life",
    );
    rig.fs.put(&usfm_path(&folder), &edited, None);
    eventually("the watcher never recorded the edit", || {
        !recorded(&handle).is_empty()
    });
    handle.wait_idle();
    assert_eq!(recorded(&handle), vec![(1, false)]);
    assert_eq!(
        words(&handle, 1, 2).as_deref(),
        Some("in hope of eternal life")
    );
    assert_eq!(rig.fs.peek(&usfm_path(&folder)), Some(edited));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// A disk that refuses the batch of an outside edit (a scanner holds the pending file past
/// the retries) records nothing and accepts nothing; the byte copy stays, and the writer's own
/// retry records the edit once the disk lets it, with no one looking at the window.
#[test]
fn a_disk_that_refuses_the_batch_of_an_outside_edit_is_tried_again_by_the_writer() {
    let (rig, folder) = saved_and_closed();
    let handle = open(&rig, &folder);
    let edited = set_verse(
        &rig.fs.peek(&usfm_path(&folder)).unwrap(),
        1,
        2,
        "in hope of eternal life",
    );
    rig.fs.hold_new_files(&folder.join(".tc/pending"), None);
    rig.fs.put(&usfm_path(&folder), &edited, None);
    handle.focus().wait().unwrap();
    // The actor tries the held rename ten more times, and gives up; the writer is told the
    // batch was refused and tries again on its own. The project is never idle while the disk
    // refuses, so there is no `wait_idle` here.
    eventually("the actor never tried the edit", || {
        rig.fs.held_waits().len() >= 10
    });
    assert!(recorded(&handle).is_empty(), "the disk refused the batch");
    assert_eq!(
        rig.fs.peek(&usfm_path(&folder)),
        Some(edited.clone()),
        "the file is not written over"
    );
    let kept: Vec<Vec<u8>> = files(&rig.fs, &folder)
        .into_iter()
        .filter(|(p, _)| p.starts_with(".tc/external/") && p.contains("TIT.usfm"))
        .map(|(_, bytes)| bytes)
        .collect();
    assert_eq!(kept, vec![edited.clone()], "the bytes found are kept");
    rig.fs.release_new_files(&folder.join(".tc/pending"));
    eventually("the writer never recorded the edit", || {
        !recorded(&handle).is_empty()
    });
    handle.wait_idle();
    assert_eq!(recorded(&handle), vec![(1, false)]);
    assert_eq!(
        words(&handle, 1, 2).as_deref(),
        Some("in hope of eternal life")
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// The app's own writes are no edits: the watcher is not told of them on the test
/// filesystem, and a real one tells them, which the entries recognize.
#[test]
fn what_the_app_writes_is_never_recorded_as_an_edit() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    for v in 1..=5 {
        save_verse(&handle, 1, v, &format!("saved {v}")).unwrap();
    }
    leave(&handle);
    // The watcher is told the paths the app wrote, as a real filesystem does.
    rig.fs.touch_told(&usfm_path(&folder));
    std::thread::sleep(std::time::Duration::from_millis(200));
    handle.wait_idle();
    assert!(recorded(&handle).is_empty(), "{:?}", handle.notices());
    assert!(copies_of(&handle, USFM).is_empty());
    handle.close().unwrap();
}

/// The window got focus: what the watcher missed is found, and the cost is a stat for each
/// path the app wrote and a read for those that changed.
#[test]
fn a_focus_reads_only_what_is_not_clean_by_stat() {
    let (rig, folder) = saved_and_closed();
    settle_entries(&rig, &folder);
    let handle = open(&rig, &folder);
    let before = handle.counters();
    let report = handle.focus().wait().unwrap();
    assert_eq!(report.read, 0, "{report:?}");
    assert!(report.found.is_empty());
    assert_eq!(
        handle.counters().derived_read,
        before.derived_read,
        "nothing was read"
    );
    let edited = set_verse(
        &rig.fs.peek(&usfm_path(&folder)).unwrap(),
        1,
        2,
        "in hope of eternal life",
    );
    rig.fs.put_unseen(&usfm_path(&folder), &edited, None);
    let report = handle.focus().wait().unwrap();
    assert_eq!(report.read, 1, "{report:?}");
    assert_eq!(report.found, vec![USFM.to_owned()]);
    handle.wait_idle();
    assert_eq!(
        recorded(&handle),
        vec![(1, false)],
        "{:?}",
        handle.notices()
    );
    assert_eq!(
        words(&handle, 1, 2).as_deref(),
        Some("in hope of eternal life")
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// L67: a book file put into `ingredients/` adds the book, its text is the project's, and
/// `metadata.json` lists it.
#[test]
fn loss_local_book_file_added_projects() {
    let (rig, folder) = saved_and_closed();
    let jonah = std::fs::read(
        crate::common::repo_root().join("fixtures/tc4/test-fixtures/sample-burrito/JON.usfm"),
    )
    .unwrap();
    let jonah = set_verse(&jonah, 1, 1, "God spoke to Jonah");
    rig.fs
        .put_unseen(&folder.join("ingredients/JON.usfm"), &jonah, None);
    let handle = open(&rig, &folder);
    assert!(handle.snapshot().fold().book(BookId::Jonah).projects());
    let fold = handle.snapshot().fold().clone();
    let text = fold.book_usfm(BookId::Jonah).expect("the book has text");
    assert!(text.contains("God spoke to Jonah"), "{text}");
    let metadata = String::from_utf8(rig.fs.peek(&folder.join("metadata.json")).unwrap()).unwrap();
    assert!(metadata.contains("ingredients/JON.usfm"), "{metadata}");
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);

    // The same through the window focus, with the project open.
    let (rig, folder) = saved_and_closed();
    let handle = open(&rig, &folder);
    rig.fs
        .put_unseen(&folder.join("ingredients/JON.usfm"), &jonah, None);
    let report = handle.focus().wait().unwrap();
    assert_eq!(report.found, vec!["ingredients/JON.usfm".to_owned()]);
    handle.wait_idle();
    assert!(handle.snapshot().fold().book(BookId::Jonah).projects());
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// §3.3: a file that cannot be an ingredient is moved to `.tc/external/`, never deleted;
/// the files of an operating system go without a word.
#[test]
fn a_file_that_cannot_be_an_ingredient_is_moved_out_and_kept() {
    let (rig, folder) = saved_and_closed();
    let handle = open(&rig, &folder);
    let put = |rel: &str, bytes: &[u8]| rig.fs.put_unseen(&folder.join(rel), bytes, None);
    put("ingredients/Thumbs.db", b"thumbs");
    put("ingredients/notes&drafts.txt", b"a name with an ampersand");
    put("ingredients/tc4-sync/state.json", b"{}");
    let report = handle.focus().wait().unwrap();
    assert_eq!(report.moved.len(), 3, "{report:?}");
    for rel in [
        "ingredients/Thumbs.db",
        "ingredients/notes&drafts.txt",
        "ingredients/tc4-sync/state.json",
    ] {
        assert!(rig.fs.peek(&folder.join(rel)).is_none(), "{rel} is gone");
    }
    let kept: Vec<Vec<u8>> = files(&rig.fs, &folder)
        .into_iter()
        .filter(|(rel, _)| rel.starts_with(".tc/external/"))
        .map(|(_, bytes)| bytes)
        .collect();
    for want in [
        b"thumbs".to_vec(),
        b"a name with an ampersand".to_vec(),
        b"{}".to_vec(),
    ] {
        assert!(kept.contains(&want));
    }
    let told = handle
        .notices()
        .iter()
        .filter(|n| matches!(n.notice, Notice::FileMoved { .. }))
        .count();
    assert_eq!(told, 2, "the junk of the system is moved without a word");
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// *Take in the file as an edit* (§6.2): the held bytes are recorded with no base, each
/// record that differs is a conflict with the project's text, and the hold ends.
#[test]
fn take_in_held_records_as_conflicts_and_ends_the_hold() {
    let (rig, folder) = saved_and_closed();
    settle_entries(&rig, &folder);
    let damaged = damage_text(&rig, &folder, USFM, "a servant", "a SERVANT");
    let handle = open(&rig, &folder);
    let held = copies_of(&handle, USFM);
    assert_eq!(held.len(), 1, "{:?}", handle.notices());
    let taken = handle.take_in_held(USFM).wait().unwrap();
    assert!(taken.batch.is_some(), "{taken:?}");
    assert!(taken.changes >= 1);
    handle.wait_idle();
    assert!(
        !handle.notices().iter().any(|n| matches!(
            &n.notice,
            Notice::StorageDamage { .. } | Notice::OutsideEditHeld { .. }
        )),
        "the hold ended: {:?}",
        handle.notices()
    );
    assert_eq!(verse_conflicts(&handle).len(), 1);
    // The copy of the held bytes stays.
    assert_eq!(rig.fs.peek(&folder.join(&held[0])), Some(damaged));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// Undo an outside edit recorded with a known base (§11.1 X7): the values it replaced come
/// back, with the edit's changes as their parents.
#[test]
fn undo_outside_edit_restores_and_keeps_history() {
    use tc_journal::command::Command;
    let (rig, folder) = saved_and_closed();
    let edited = set_verse(
        &rig.fs.peek(&usfm_path(&folder)).unwrap(),
        1,
        2,
        "in hope of eternal life",
    );
    rig.fs.put(&usfm_path(&folder), &edited, None);
    let handle = open(&rig, &folder);
    let batch = handle
        .notices()
        .into_iter()
        .find_map(|n| match n.notice {
            Notice::OutsideEditRecorded { batch, .. } => Some(batch),
            _ => None,
        })
        .expect("the edit was recorded");
    assert_eq!(
        words(&handle, 1, 2).as_deref(),
        Some("in hope of eternal life")
    );
    let reply = handle
        .submit(Command::UndoOutsideEdit { batch })
        .wait()
        .unwrap();
    assert!(reply.kept.is_empty(), "{:?}", reply.kept);
    assert_eq!(reply.changes.len(), 1);
    assert_ne!(
        words(&handle, 1, 2).as_deref(),
        Some("in hope of eternal life"),
        "the edit is taken back"
    );
    assert!(verse_conflicts(&handle).is_empty(), "the undo has a parent");
    // Nothing was deleted: the edit is still in the history of the verse.
    let history = handle.snapshot().fold().history(&verse(1, 2));
    assert_eq!(history.len(), 3, "the seed, the edit and its undo");
    handle.wait_idle();
    leave(&handle);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// §8.1 `OutsideEditLarge`: *Accept all from the file* and *Keep translationCore's text*
/// each resolve the rows an edit made, in one batch.
#[test]
fn accept_all_from_the_file_and_keep_the_projects_text_resolve_the_rows() {
    use tc_journal::command::{Command, OutsideKeep};
    for keep in [OutsideKeep::File, OutsideKeep::Project] {
        let (rig, folder) = titus();
        let handle = open(&rig, &folder);
        save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
        save_verse(&handle, 1, 2, "in hope").unwrap();
        leave(&handle);
        handle.close().unwrap();
        let heads = folder.join(".tc/heads/ingredients%2FTIT.usfm.bin");
        rig.fs.delete(&heads).unwrap();
        let mut bytes = rig.fs.peek(&usfm_path(&folder)).unwrap();
        bytes = set_verse(&bytes, 1, 1, "Paul, a slave");
        bytes = set_verse(&bytes, 1, 2, "in the hope");
        rig.fs.put(&usfm_path(&folder), &bytes, None);
        let handle = open(&rig, &folder);
        let batch = handle
            .notices()
            .into_iter()
            .find_map(|n| match n.notice {
                Notice::OutsideEditRecorded {
                    batch,
                    unknown_base: true,
                    ..
                } => Some(batch),
                _ => None,
            })
            .expect("an unknown-base edit was recorded");
        let conflicts: Vec<_> = handle
            .snapshot()
            .fold()
            .conflicts()
            .map(|c| (c.id, c.key.clone()))
            .collect();
        assert_eq!(conflicts.len(), 2);
        let listed = conflicts
            .iter()
            .map(|(id, key)| (*id, vec![handle.mint_review(key).unwrap()]))
            .collect();
        let reply = handle
            .submit(Command::ResolveOutsideEdit {
                batch,
                keep,
                conflicts: listed,
            })
            .wait()
            .unwrap();
        assert!(reply.skipped.is_empty(), "{:?}", reply.skipped);
        assert_eq!(reply.changes.len(), 2, "one batch");
        assert_eq!(handle.snapshot().fold().conflicts().count(), 0);
        let want = match keep {
            OutsideKeep::File => ("Paul, a slave", "in the hope"),
            OutsideKeep::Project => ("Paul, a servant", "in hope"),
        };
        assert_eq!(words(&handle, 1, 1).as_deref(), Some(want.0));
        assert_eq!(words(&handle, 1, 2).as_deref(), Some(want.1));
        handle.close().unwrap();
        assert_valid_burrito(&rig.fs, &folder);
    }
}

/// §11.1 on a real folder: the operating system's watcher tells the app (inotify,
/// FSEvents, ReadDirectoryChangesW), and an edit saved by another program is recorded
/// without a focus. Polls for up to ten seconds.
#[test]
fn the_watcher_on_a_real_folder_records_an_edit() {
    use tc_store::{FakePlatform, Store, StoreConfig};
    let dir = tempfile::tempdir().unwrap();
    let store = Store::start(StoreConfig {
        app_data: dir.path().join("app"),
        projects: dir.path().join("projects"),
        vfs: Arc::new(tc_store::StdFs),
        platform: Arc::new(FakePlatform::default()),
        clock: Arc::new(tc_store::ManualClock::at(1_792_195_200_000)),
        threads: 2,
        watch_debounce: std::time::Duration::from_millis(50),
    })
    .unwrap();
    store.wait_cleanup();
    watch_real_burrito(&store);
    let folder = store
        .create(&tc_store::NewProject {
            name: "Titus".into(),
            kind: tc_journal::fold::ProjectKind::Bible,
            records: first_records(),
            books: vec![(
                TIT,
                tc_journal::record::Scope::Whole,
                tc_journal::command::SeedSource { usfm: ult("TIT") },
            )],
        })
        .unwrap();
    let handle = store.open(&folder, &OpenOptions::default()).unwrap();
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    leave(&handle);
    handle.wait_idle();
    let path = usfm_path(&folder);
    let edited = set_verse(
        &std::fs::read(&path).unwrap(),
        1,
        2,
        "in hope of eternal life",
    );
    std::fs::write(&path, &edited).unwrap();
    eventually("the watcher never recorded the edit", || {
        recorded(&handle).len() == 1
    });
    handle.wait_idle();
    assert_eq!(
        words(&handle, 1, 2).as_deref(),
        Some("in hope of eternal life")
    );
    // The app's own writes since then were told too, and recorded nothing.
    save_verse(&handle, 1, 3, "grace").unwrap();
    leave(&handle);
    std::thread::sleep(std::time::Duration::from_millis(300));
    handle.wait_idle();
    assert_eq!(recorded(&handle).len(), 1, "{:?}", handle.notices());
    handle.close().unwrap();
    assert!(store.hooks().burrito_problems().is_empty());
}

/// §6.2: with `.tc/` gone (a copy of the folder without it) the last commit of this app says
/// what the files were made from. An edit of a verse HEAD has is recorded with its parents;
/// a verse saved since HEAD is newer than the file the person started from, so the edit and
/// the save are a conflict and neither replaces the other.
#[test]
fn an_outside_edit_found_without_a_manifest_uses_the_head_base() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    save_verse(&handle, 1, 2, "in hope").unwrap();
    handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    leave(&handle);
    handle.close().unwrap();
    let lose_the_local_state = |fs: &FaultFs| {
        fs.delete(&folder.join(".tc/materialized.json")).unwrap();
        fs.delete(&folder.join(".tc/heads")).unwrap();
    };

    // The edit is of a verse HEAD has: known base, no conflict.
    let copy = rig.fs.copy();
    lose_the_local_state(&copy);
    let edited = set_verse(
        &copy.peek(&usfm_path(&folder)).unwrap(),
        1,
        2,
        "in hope of eternal life",
    );
    copy.put(&usfm_path(&folder), &edited, None);
    let rig_a = rig_as(copy.clone(), APP, "host-1");
    let handle = open(&rig_a, &folder);
    assert_eq!(
        recorded(&handle),
        vec![(1, false)],
        "{:?}",
        handle.notices()
    );
    assert!(verse_conflicts(&handle).is_empty());
    assert_eq!(
        words(&handle, 1, 2).as_deref(),
        Some("in hope of eternal life")
    );
    handle.close().unwrap();
    assert_valid_burrito(&copy, &folder);

    // A save made since HEAD (the process was stopped before it could commit): the person's
    // edit started from the older text.
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 2, "in the hope").unwrap();
    leave(&handle);
    handle.wait_idle();
    let stopped = rig.fs.crash(false);
    drop(handle);
    lose_the_local_state(&stopped);
    let edited = set_verse(
        &stopped.peek(&usfm_path(&folder)).unwrap(),
        1,
        2,
        "in hope of eternal life",
    );
    stopped.put(&usfm_path(&folder), &edited, None);
    let rig_b = rig_as(stopped.clone(), APP, "host-1");
    let handle = open(&rig_b, &folder);
    assert_eq!(verse_conflicts(&handle).len(), 1, "{:?}", handle.notices());
    assert_eq!(words(&handle, 1, 2).as_deref(), Some("in the hope"));
    handle.close().unwrap();
    assert_valid_burrito(&stopped, &folder);
}

/// §3.5: a `project.json` that is missing or does not read is restored from the HEAD tree at
/// open, and the person is told; with no copy there, the folder is not a project.
#[test]
fn a_project_json_that_does_not_read_is_restored_from_head_at_open() {
    let (rig, folder) = titus();
    let handle = open(&rig, &folder);
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    handle
        .checkpoint(tc_store::CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    handle.close().unwrap();
    let rel = "ingredients/tc/project.json";
    let path = folder.join(rel);
    let original = rig.fs.peek(&path).unwrap();

    rig.fs.put(&path, b"{ this is not json", None);
    let handle = open(&rig, &folder);
    assert_eq!(handle.facts().id, project_of_bytes(&original));
    let copies = copies_of(&handle, rel);
    assert_eq!(copies.len(), 1, "{:?}", handle.notices());
    assert_eq!(
        rig.fs.peek(&folder.join(&copies[0])),
        Some(b"{ this is not json".to_vec())
    );
    assert_eq!(rig.fs.peek(&path), Some(original.clone()));
    handle.close().unwrap();

    rig.fs.delete(&path).unwrap();
    let handle = open(&rig, &folder);
    assert!(handle.notices().iter().any(|n| matches!(
        &n.notice,
        Notice::MissingFileRebuilt { path } if path.as_str() == rel
    )));
    assert_eq!(rig.fs.peek(&path), Some(original));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);

    // With no copy in HEAD the folder is no project.
    let (rig, folder) = titus();
    let outside = tc_store::Vfs::outside_dir(&rig.fs, &folder).unwrap();
    let _ = std::fs::remove_dir_all(outside.join(".git"));
    rig.fs.delete(&folder.join(rel)).unwrap();
    let opened = rig.store.open(&folder, &OpenOptions::default());
    assert!(matches!(opened, Err(tc_store::OpenError::NotAProject)));
}

fn project_of_bytes(bytes: &[u8]) -> tc_journal::ProjectId {
    let json: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    tc_journal::ProjectId::parse(json["projectId"].as_str().unwrap()).unwrap()
}

/// Crash rows X1 and X2 (§6.4): a process stopped at any write point of recording an edit
/// leaves a project whose next open shows the edit exactly once, keeps the bytes found, and
/// writes the file from the project; a second open changes nothing.
#[test]
fn crash_outside_edit_recording() {
    use crate::crash::{Outcome, kill_everywhere_full};
    let (rig, folder) = saved_and_closed();
    let base = rig.fs.crash(false);
    let edited = set_verse(
        &base.peek(&usfm_path(&folder)).unwrap(),
        1,
        2,
        "in hope of eternal life",
    );
    let setup = |fs: &FaultFs, _handle: &ProjectHandle| {
        fs.put_unseen(&usfm_path(&folder), &edited, None);
    };
    let action = |handle: &ProjectHandle| {
        let _ = handle.focus().wait();
        handle.wait_idle();
        Outcome::NotSaved
    };
    let points = kill_everywhere_full(&base, &folder, &setup, &action, &|handle, fs, _| {
        assert_eq!(
            words(handle, 1, 2).as_deref(),
            Some("in hope of eternal life"),
            "{:?}",
            handle.notices()
        );
        let fold = handle.snapshot().fold().clone();
        let externals = fold
            .history(&verse(1, 2))
            .iter()
            .filter(|id| {
                matches!(
                    fold.body(id).map(|b| b.cause),
                    Some(tc_journal::Cause::External { .. })
                )
            })
            .count();
        assert_eq!(externals, 1, "recorded once");
        assert!(verse_conflicts(handle).is_empty());
        assert!(
            files(fs, &folder)
                .iter()
                .any(|(rel, bytes)| rel.starts_with(".tc/external/") && *bytes == edited),
            "the bytes the person saved are kept"
        );
        let written = String::from_utf8(fs.peek(&usfm_path(&folder)).unwrap()).unwrap();
        assert!(written.contains("in hope of eternal life"), "{written}");
        assert!(written.contains("Paul, a servant"), "{written}");
    });
    assert!(points > 8, "{points}");
}

// ---------------------------------------------------------------------------------------
// §16.1 `outside_edit_reconcile_conserves`
// ---------------------------------------------------------------------------------------

mod conserves {
    use super::*;
    use proptest::prelude::*;
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::atomic::{AtomicU32, Ordering};
    use tc_journal::command::{Command, SeedSource};
    use tc_journal::record::{Scope, VerseValue};
    use tc_journal::{ChangeId, ChapterNo, DeviceId, Record, VerseNo};

    /// What happens to the project between two opens.
    #[derive(Debug, Clone)]
    pub enum Op {
        /// The person saves in the app.
        Save { verse: u16, text: u8 },
        /// A program changes a verse's words in `TIT.usfm`.
        Edit { verse: u16, text: u8 },
        /// A cloud client puts the file back as it was when the project began.
        Revert,
        /// An editor drops a verse.
        Delete { verse: u16 },
        /// A program fills fifty-five verses of `LUK.usfm` at once.
        Many,
        /// An editor rewrites the line ends and the blanks at their ends.
        Whitespace,
        /// A book file is put into `ingredients/`.
        AddBook,
        /// A file that cannot be read: a verse number twice, or bytes that are no text.
        Undecodable { garbage: bool },
        /// A teammate's change of the verse reaches the folder while the file is edited.
        Race { verse: u16, text: u8 },
        /// A program changes the status of a decision in a sidecar.
        Sidecar,
        /// A text editor renames the project in `metadata.json`.
        Metadata { text: u8 },
        /// A program changes a link in an Open Bible Stories template.
        Story,
    }

    const KINDS: usize = 12;

    fn kind(op: &Op) -> usize {
        match op {
            Op::Save { .. } => 0,
            Op::Edit { .. } => 1,
            Op::Revert => 2,
            Op::Delete { .. } => 3,
            Op::Many => 4,
            Op::Whitespace => 5,
            Op::AddBook => 6,
            Op::Undecodable { .. } => 7,
            Op::Race { .. } => 8,
            Op::Sidecar => 9,
            Op::Metadata { .. } => 10,
            Op::Story => 11,
        }
    }

    /// What a run did: the edits it made, by kind, and the claims it checked, by claim.
    #[derive(Default)]
    pub struct Census {
        executed: [AtomicU32; KINDS],
        checked: [AtomicU32; 11],
    }
    const CONFLICT_LISTED: usize = 0;
    const KNOWN_BASE_RECORDED: usize = 1;
    const NOTHING_RECORDED: usize = 2;
    const BOOK_ADDED: usize = 3;
    const COPY_KEPT: usize = 4;
    const OVER_FIFTY: usize = 5;
    const REVERT_UNKNOWN: usize = 6;
    const DELETE_UNKNOWN: usize = 7;
    const SIDECAR_RECORDED: usize = 8;
    const METADATA_RECORDED: usize = 9;
    const STORY_RECORDED: usize = 10;

    fn op() -> impl Strategy<Value = Op> {
        let verse = 1u16..=6;
        let text = 0u8..4;
        prop_oneof![
            3 => (verse.clone(), text.clone()).prop_map(|(verse, text)| Op::Save { verse, text }),
            4 => (verse.clone(), text.clone()).prop_map(|(verse, text)| Op::Edit { verse, text }),
            2 => Just(Op::Revert),
            2 => verse.clone().prop_map(|verse| Op::Delete { verse }),
            2 => Just(Op::Many),
            2 => Just(Op::Whitespace),
            2 => Just(Op::AddBook),
            2 => any::<bool>().prop_map(|garbage| Op::Undecodable { garbage }),
            3 => (verse, text.clone()).prop_map(|(verse, text)| Op::Race { verse, text }),
            2 => Just(Op::Sidecar),
            2 => text.prop_map(|text| Op::Metadata { text }),
            2 => Just(Op::Story),
        ]
    }

    fn words_of(text: u8) -> String {
        format!("words number {text} of the case")
    }

    fn luke_seed() -> String {
        let mut usfm =
            String::from("\\id LUK source\n\\usfm 3.0\n\\h Luke\n\\mt Luke\n\\c 1\n\\p\n");
        for v in 1..=80 {
            usfm.push_str(&format!("\\v {v} Source {v}.\n"));
        }
        usfm
    }

    fn their_version(fold: &tc_journal::fold::FoldState, verse: u16, words: &str) -> Record {
        let key = super::verse(1, verse);
        let Some(Record::Verse {
            value:
                VerseValue::Text {
                    marker,
                    layout,
                    last,
                    from,
                    ..
                },
            ..
        }) = fold.projected(&key)
        else {
            panic!("1:{verse} has text");
        };
        Record::Verse {
            book: TIT,
            chapter: ChapterNo::new(1).unwrap(),
            verse: VerseNo::new(verse).unwrap(),
            value: VerseValue::Text {
                marker,
                words: words.to_owned(),
                layout,
                last,
                from,
            },
        }
    }

    fn external_of(
        fold: &tc_journal::fold::FoldState,
        id: &ChangeId,
    ) -> Option<(bool, Vec<ChangeId>, Record)> {
        let body = fold.body(id)?;
        match body.cause {
            tc_journal::Cause::External { unknown_base, .. } => {
                Some((unknown_base, body.parents, body.record))
            }
            _ => None,
        }
    }

    /// A program changes the status of a decision in a sidecar: the edit is one change, the
    /// bytes found are kept whole, and the file shows the decision the project holds.
    fn sidecar_edit(rig: &Rig, folder: &Path) {
        let text = String::from_utf8(rig.fs.peek(&folder.join(super::TN)).unwrap()).unwrap();
        let (from, to) = if text.contains("\"userStatus\":\"valid\"") {
            ("valid", "todo")
        } else {
            ("todo", "valid")
        };
        let edited = super::replaced(
            rig,
            folder,
            super::TN,
            &format!("\"userStatus\":\"{from}\""),
            &format!("\"userStatus\":\"{to}\""),
        );
        let handle = open(rig, folder);
        assert_eq!(
            super::recorded_for(&handle, super::TN).len(),
            1,
            "{:?}",
            handle.notices()
        );
        let copies = copies_of(&handle, super::TN);
        assert!(
            copies
                .iter()
                .any(|copy| rig.fs.peek(&folder.join(copy)).as_deref() == Some(edited.as_slice())),
            "the bytes found are kept: {copies:?}"
        );
        // A decision put back as it was is a conflict that leaves the project's own status
        // shown; any other edit is shown.
        let written = String::from_utf8(rig.fs.peek(&folder.join(super::TN)).unwrap()).unwrap();
        assert!(
            written.contains("\"userStatus\":\"todo\"")
                || written.contains("\"userStatus\":\"valid\""),
            "{written}"
        );
        handle.wait_idle();
        assert_valid_burrito(&rig.fs, folder);
        handle.close().unwrap();
    }

    /// A program changes a link in the template of a story: the story records it, and the file
    /// shows it.
    fn story_edit(rig: &Rig, folder: &Path) {
        let rel = "ingredients/content/01.md";
        let text = String::from_utf8(rig.fs.peek(&folder.join(rel)).unwrap()).unwrap();
        let (from, to) = if text.contains("obs-en-01-02-new.jpg") {
            ("obs-en-01-02-new.jpg", "obs-en-01-02.jpg")
        } else {
            ("obs-en-01-02.jpg", "obs-en-01-02-new.jpg")
        };
        let edited = super::replaced(rig, folder, rel, from, to);
        let handle = open(rig, folder);
        assert_eq!(
            super::recorded_for(&handle, rel).len(),
            1,
            "{:?}",
            handle.notices()
        );
        let copies = copies_of(&handle, rel);
        assert!(
            copies
                .iter()
                .any(|copy| rig.fs.peek(&folder.join(copy)).as_deref() == Some(edited.as_slice())),
            "the bytes found are kept: {copies:?}"
        );
        // A link put back as it was is a conflict that leaves the project's own link shown;
        // any other edit is shown.
        if to.ends_with("-new.jpg") {
            assert!(
                rig.fs.peek(&folder.join(rel)).as_deref() == Some(edited.as_slice()),
                "the story shows its edit"
            );
        }
        handle.wait_idle();
        assert_valid_burrito(&rig.fs, folder);
        handle.close().unwrap();
    }

    /// A text editor renames the project in `metadata.json`: the name is recorded and the file
    /// shows it.
    fn rename_in_metadata(rig: &Rig, folder: &Path, name: &str) {
        let rel = "metadata.json";
        let bytes = rig.fs.peek(&folder.join(rel)).unwrap();
        let mut doc: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        doc["identification"]["name"]["en"] = serde_json::Value::String(name.to_owned());
        rig.fs.put(
            &folder.join(rel),
            &serde_json::to_vec_pretty(&doc).unwrap(),
            None,
        );
        let handle = open(rig, folder);
        assert!(
            !super::recorded_for(&handle, rel).is_empty(),
            "{:?}",
            handle.notices()
        );
        let written = String::from_utf8(rig.fs.peek(&folder.join(rel)).unwrap()).unwrap();
        assert!(written.contains(name), "{written}");
        handle.wait_idle();
        assert_valid_burrito(&rig.fs, folder);
        handle.close().unwrap();
    }

    pub fn run_case(ops: &[Op], census: &Census) {
        let (rig, folder) = titus();
        let handle = open(&rig, &folder);
        for v in 1..=6 {
            save_verse(&handle, 1, v, &format!("first words {v}")).unwrap();
        }
        handle
            .submit(Command::AddBook {
                book: BookId::Luke,
                scope: Scope::Whole,
                seed: SeedSource { usfm: luke_seed() },
            })
            .wait()
            .unwrap();
        handle.leave(Leave::Mode).wait().unwrap();
        handle.close().unwrap();
        let original = rig.fs.peek(&usfm_path(&folder)).unwrap();
        let luke = folder.join("ingredients/LUK.usfm");
        let jonah = std::fs::read(
            crate::common::repo_root().join("fixtures/tc4/test-fixtures/sample-burrito/JON.usfm"),
        )
        .unwrap();
        let jonah = set_verse(&jonah, 1, 1, "God spoke to Jonah");
        let mut device_n = 20u8;
        // A project with decisions and one of Open Bible Stories, made when an edit needs them.
        let mut rich: Option<(Rig, PathBuf)> = None;
        let mut stories: Option<(Rig, PathBuf)> = None;
        let mut renamed = 0u32;
        for op in ops {
            census.executed[kind(op)].fetch_add(1, Ordering::Relaxed);
            match op {
                Op::Sidecar => {
                    let (rig, folder) = rich.get_or_insert_with(|| {
                        let (fs, folder) = crate::commands::rich();
                        let (rig, handle) = crate::commands::open(&fs, &folder);
                        handle.close().unwrap();
                        (rig, folder)
                    });
                    sidecar_edit(rig, folder);
                    census.checked[SIDECAR_RECORDED].fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                Op::Story => {
                    let (rig, folder) = stories.get_or_insert_with(|| {
                        let (fs, folder) = crate::commands::stories();
                        let (rig, handle) = crate::commands::open(&fs, &folder);
                        handle.close().unwrap();
                        (rig, folder)
                    });
                    story_edit(rig, folder);
                    census.checked[STORY_RECORDED].fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                Op::Metadata { text } => {
                    renamed += 1;
                    rename_in_metadata(&rig, &folder, &format!("renamed outside {text} {renamed}"));
                    census.checked[METADATA_RECORDED].fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                _ => {}
            }
            // The project as the last session left it.
            let handle = open(&rig, &folder);
            let before_fold = handle.snapshot().fold().clone();
            let before_ids = ids_on_disk(&rig.fs, &folder);
            let before_text: BTreeMap<u16, Option<String>> =
                (1..=6).map(|v| (v, words(&handle, 1, v))).collect();
            if let Op::Save { verse, text } = op {
                save_verse(&handle, 1, *verse, &words_of(*text)).unwrap();
                leave(&handle);
            }
            handle.close().unwrap();
            let tit = rig.fs.peek(&usfm_path(&folder)).unwrap();
            let mut teammate_ids: BTreeSet<ChangeId> = BTreeSet::new();
            let mut put = Vec::new();
            match op {
                // Handled above: these edit another project, or another file of this one.
                Op::Save { .. } | Op::Sidecar | Op::Metadata { .. } | Op::Story => {}
                Op::Edit { verse, text } | Op::Race { verse, text } => {
                    if let Op::Race { .. } = op {
                        device_n += 1;
                        let theirs = their_version(&before_fold, *verse, "a teammate's words");
                        let heads = before_fold.heads(&super::verse(1, *verse));
                        teammate_ids.extend(crate::commands::teammate_batch(
                            &rig.fs,
                            &folder,
                            DeviceId::local([device_n; 15]),
                            1,
                            vec![(theirs, heads)],
                        ));
                    }
                    put.push((
                        usfm_path(&folder),
                        set_verse(&tit, 1, *verse, &words_of(*text)),
                    ));
                }
                Op::Revert => put.push((usfm_path(&folder), original.clone())),
                Op::Delete { verse } => {
                    let text = String::from_utf8(tit.clone()).unwrap();
                    let at = text.find(&format!("\n\\v {verse} ")).unwrap() + 1;
                    let end = at + text[at..].find('\n').unwrap() + 1;
                    put.push((
                        usfm_path(&folder),
                        format!("{}{}", &text[..at], &text[end..]).into_bytes(),
                    ));
                }
                Op::Many => {
                    let mut bytes = rig.fs.peek(&luke).unwrap();
                    for v in 1..=55 {
                        bytes = set_verse(&bytes, 1, v, &format!("many {v}"));
                    }
                    put.push((luke.clone(), bytes));
                }
                Op::Whitespace => {
                    let text = String::from_utf8(tit.clone()).unwrap();
                    put.push((
                        usfm_path(&folder),
                        text.replace('\n', "  \r\n").into_bytes(),
                    ));
                }
                Op::AddBook => put.push((folder.join("ingredients/JON.usfm"), jonah.clone())),
                Op::Undecodable { garbage } => {
                    let bytes = if *garbage {
                        (0..200u32).map(|i| (i % 7 * 37 + 128) as u8).collect()
                    } else {
                        String::from_utf8(tit.clone())
                            .unwrap()
                            .replacen("\\v 3 ", "\\v 2 ", 1)
                            .into_bytes()
                    };
                    put.push((usfm_path(&folder), bytes));
                }
            }
            for (path, bytes) in &put {
                rig.fs.put_unseen(path, bytes, None);
            }

            // The next open finds it.
            let handle = open(&rig, &folder);
            let fold = handle.snapshot().fold().clone();
            let after_ids = ids_on_disk(&rig.fs, &folder);
            // No change held before is gone: it is on disk and folded.
            assert!(
                before_ids.is_subset(&after_ids),
                "{op:?}: a change left the disk"
            );
            for id in &before_ids {
                assert!(
                    fold.change(id).is_some(),
                    "{op:?}: {id:?} is no longer folded"
                );
            }
            let new: Vec<ChangeId> = after_ids
                .difference(&before_ids)
                .filter(|id| !teammate_ids.contains(id))
                .copied()
                .collect();
            let externals: Vec<(ChangeId, bool, Vec<ChangeId>, Record)> = new
                .iter()
                .filter_map(|id| {
                    external_of(&fold, id)
                        .map(|(unknown, parents, record)| (*id, unknown, parents, record))
                })
                .collect();
            for (id, unknown, parents, _) in &externals {
                // A known base names only what the file was made from.
                assert!(
                    parents.iter().all(|p| before_ids.contains(p)),
                    "{op:?}: {id:?} names a parent the file was not made from"
                );
                if *unknown {
                    assert!(parents.is_empty(), "{op:?}: an unknown base names parents");
                }
            }
            // An unknown-base change of a verse that held the person's words is a conflict,
            // and not the text shown.
            for (id, unknown, _, record) in &externals {
                let key = record.key();
                let RecordKey::Verse(_, c, v) = &key else {
                    continue;
                };
                let held = c.get() == 1
                    && before_text
                        .get(&v.get())
                        .is_some_and(|w| w.as_deref().is_some_and(|w| w != "___" && !w.is_empty()));
                if *unknown && held && fold.projected(&key).as_ref() != Some(record) {
                    let conflict = fold
                        .conflicts()
                        .find(|cf| cf.key == key)
                        .unwrap_or_else(|| panic!("{op:?}: no conflict on {key:?}"));
                    assert_ne!(
                        conflict.provisional, *id,
                        "{op:?}: the file's text is provisional"
                    );
                    census.checked[CONFLICT_LISTED].fetch_add(1, Ordering::Relaxed);
                }
            }
            // A destructive edit (a verse dropped) or a reverting one (the file put back as it
            // was) of text the person held is recorded with no parents: the file's version is
            // a conflict and never replaces the text (§11.1 X3).
            let held_text = |key: &RecordKey| match key {
                RecordKey::Verse(_, c, v) => {
                    c.get() == 1
                        && before_text.get(&v.get()).is_some_and(|w| {
                            w.as_deref().is_some_and(|w| w != "___" && !w.is_empty())
                        })
                }
                _ => false,
            };
            let destructive: Vec<&(ChangeId, bool, Vec<ChangeId>, Record)> = externals
                .iter()
                .filter(|(_, _, _, record)| held_text(&record.key()))
                .collect();
            if let Op::Revert | Op::Delete { .. } = op {
                for (id, unknown, parents, record) in &destructive {
                    assert!(
                        *unknown && parents.is_empty(),
                        "{op:?}: {id:?} is a known-base edit"
                    );
                    let key = record.key();
                    if fold.projected(&key).as_ref() != Some(record) {
                        let conflict = fold
                            .conflicts()
                            .find(|cf| cf.key == key)
                            .unwrap_or_else(|| panic!("{op:?}: no conflict on {key:?}"));
                        assert_ne!(
                            conflict.provisional, *id,
                            "{op:?}: the file's text is provisional"
                        );
                    }
                }
                if !destructive.is_empty() {
                    let claim = if matches!(op, Op::Revert) {
                        REVERT_UNKNOWN
                    } else {
                        DELETE_UNKNOWN
                    };
                    census.checked[claim].fetch_add(1, Ordering::Relaxed);
                }
            }
            match op {
                Op::Edit { .. }
                    if !externals.is_empty()
                        && externals.iter().all(|(_, unknown, ..)| !unknown) =>
                {
                    census.checked[KNOWN_BASE_RECORDED].fetch_add(1, Ordering::Relaxed);
                }
                Op::Whitespace => {
                    assert!(new.is_empty(), "{op:?}: line ends are no edit");
                    census.checked[NOTHING_RECORDED].fetch_add(1, Ordering::Relaxed);
                }
                Op::AddBook => {
                    assert!(fold.book(BookId::Jonah).projects(), "{op:?}");
                    census.checked[BOOK_ADDED].fetch_add(1, Ordering::Relaxed);
                }
                Op::Undecodable { .. } => {
                    assert!(new.is_empty(), "{op:?}: nothing is recorded");
                    let bytes = &put[0].1;
                    assert!(
                        files(&rig.fs, &folder)
                            .iter()
                            .any(|(rel, b)| rel.starts_with(".tc/external/") && b == bytes),
                        "{op:?}: the bytes are kept"
                    );
                    census.checked[COPY_KEPT].fetch_add(1, Ordering::Relaxed);
                }
                Op::Many if externals.len() > 50 => {
                    assert!(
                        externals.iter().all(|(_, unknown, ..)| *unknown),
                        "{op:?}: more than fifty keys are recorded with no parents"
                    );
                    census.checked[OVER_FIFTY].fetch_add(1, Ordering::Relaxed);
                }
                _ => {}
            }
            handle.wait_idle();
            // Whatever the file held, the folder is the project again.
            assert_valid_burrito(&rig.fs, &folder);
            assert_commits_valid(&rig.fs, &folder);
            let written = rig.fs.peek(&usfm_path(&folder)).unwrap();
            let want = fold
                .render(&tc_journal::fold::DerivedFile::Usfm(TIT))
                .unwrap();
            assert!(
                written == want || matches!(op, Op::Race { .. } | Op::Edit { .. }),
                "{op:?}: TIT.usfm is not what the project says"
            );
            handle.close().unwrap();
        }
    }

    /// One of each kind of edit, with the destructive ones after edits they destroy.
    pub fn every_kind() -> Vec<Op> {
        vec![
            Op::Edit { verse: 2, text: 1 },
            Op::Delete { verse: 3 },
            Op::Whitespace,
            Op::AddBook,
            Op::Undecodable { garbage: true },
            Op::Undecodable { garbage: false },
            Op::Many,
            Op::Race { verse: 4, text: 2 },
            Op::Revert,
            Op::Save { verse: 1, text: 3 },
            Op::Sidecar,
            Op::Sidecar,
            Op::Metadata { text: 1 },
            Op::Story,
            Op::Story,
        ]
    }

    pub fn census(census: &Census) {
        let executed: Vec<u32> = census
            .executed
            .iter()
            .map(|c| c.load(Ordering::Relaxed))
            .collect();
        let checked: Vec<u32> = census
            .checked
            .iter()
            .map(|c| c.load(Ordering::Relaxed))
            .collect();
        assert!(
            executed.iter().all(|n| *n > 0),
            "a kind of edit never occurred: {executed:?}"
        );
        assert!(
            checked.iter().all(|n| *n > 0),
            "a claim was never checked: {checked:?} (kinds {executed:?})"
        );
    }

    /// 48 random sequences of edits; the census then fails the test when a kind of edit never
    /// occurred or a claim was never checked, so a property that stopped exercising a class
    /// cannot pass.
    pub fn property() {
        let config = ProptestConfig {
            cases: 48,
            max_shrink_iters: 0,
            failure_persistence: None,
            ..ProptestConfig::default()
        };
        let mut runner = proptest::test_runner::TestRunner::new(config);
        let counted = Census::default();
        let result = runner.run(&proptest::collection::vec(op(), 1..4), |ops| {
            run_case(&ops, &counted);
            Ok(())
        });
        if let Err(e) = result {
            panic!("{e}");
        }
        // The random cases may miss a class by chance (a revert that follows an edit of its
        // own); one run of every kind in a project makes sure each is reached and checked,
        // by the same code.
        run_case(&every_kind(), &counted);
        census(&counted);
    }
}

#[test]
fn outside_edit_reconcile_conserves() {
    conserves::property();
}

/// Every kind of edit in one project, in order.
#[test]
fn every_kind_of_outside_edit_in_one_project_conserves() {
    let counted = conserves::Census::default();
    conserves::run_case(&conserves::every_kind(), &counted);
    // Every class of edit was reached and every claim about it was checked.
    conserves::census(&counted);
}

// ---------------------------------------------------------------------------------------
// D7: a program that keeps the file open saves it twice
// ---------------------------------------------------------------------------------------

/// Paratext on Windows keeps the file open: the app records the first save but cannot write
/// the file back, and the program saves the file again, from its own text. The second save
/// continues the first (§11.1 D7); it is no conflict between the person's own edits.
#[test]
fn two_saves_from_a_program_that_keeps_the_file_open_are_no_conflict() {
    let (rig, folder) = saved_and_closed();
    let handle = open(&rig, &folder);
    let usfm = usfm_path(&folder);
    let written = rig.fs.peek(&usfm).unwrap();
    rig.fs.set_busy(&usfm, true);
    // The program writes CRLF line ends, which the app does not copy back, so the file stays
    // different from the project's and is rewritten once the program lets go.
    let crlf = |bytes: &[u8]| {
        String::from_utf8(bytes.to_vec())
            .unwrap()
            .replace('\n', "\r\n")
    };
    let first_text = set_verse(&written, 1, 1, "Paul, a slave of God");
    let first = crlf(&first_text).into_bytes();
    rig.fs.put_unseen(&usfm, &first, None);
    handle.focus().wait().unwrap();
    handle.wait_idle();
    assert_eq!(recorded(&handle), vec![(1, false)]);
    assert_eq!(
        rig.fs.peek(&usfm),
        Some(first.clone()),
        "the file cannot be written while the program holds it"
    );
    let second_text = set_verse(&first_text, 1, 1, "Paul, a slave of God and an apostle");
    let second = crlf(&second_text).into_bytes();
    rig.fs.put_unseen(&usfm, &second, None);
    handle.focus().wait().unwrap();
    handle.wait_idle();
    rig.fs.set_busy(&usfm, false);
    // The writer's own retry reads the second save and records it.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while recorded(&handle).len() < 2 {
        assert!(
            std::time::Instant::now() < deadline,
            "never recorded: {:?}",
            handle.notices()
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    handle.wait_idle();
    assert_eq!(
        recorded(&handle),
        vec![(1, false), (1, false)],
        "both with the base of the bytes before them"
    );
    assert!(
        verse_conflicts(&handle).is_empty(),
        "{:?}",
        verse_conflicts(&handle)
    );
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("Paul, a slave of God and an apostle")
    );
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}
