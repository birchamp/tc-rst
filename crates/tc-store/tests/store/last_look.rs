//! A last look compares the target bytes even when its size and time did not change.

use crate::common::*;
use tc_journal::Record;
use tc_journal::record::VerseValue;
use tc_store::{Leave, OpenOptions, Point, ProjectHandle, Vfs};

const BOOK: &str = "ingredients/TIT.usfm";

fn assert_verse_history(handle: &ProjectHandle, expected: &str) {
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    assert!(
        fold.history(&verse(1, 1)).iter().any(|id| matches!(
            fold.body(id).map(|body| body.record),
            Some(Record::Verse {
                value: VerseValue::Text { words, .. },
                ..
            }) if words == expected
        )),
        "History lost the verse {expected:?}; notices: {:?}",
        handle.notices()
    );
}

#[test]
fn a_same_stat_edit_after_the_group_read_is_kept_at_the_last_look() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let baseline_words = "baseline verse kept during the final look";
    save_verse(&handle, 1, 1, baseline_words).unwrap();
    handle.leave(Leave::Book(TIT)).wait().unwrap();
    handle.wait_idle();
    assert_valid_burrito(&rig.fs, &folder);

    let path = folder.join(BOOK);
    let original = rig.fs.peek(&path).unwrap();
    let original_text = std::str::from_utf8(&original).unwrap();
    assert_eq!(original_text.matches("baseline").count(), 1);
    let outside_words = baseline_words.replacen("baseline", "external", 1);
    let outside_bytes = original_text
        .replacen("baseline", "external", 1)
        .into_bytes();
    assert_eq!(outside_bytes.len(), original.len());
    let old_stat = rig.fs.metadata(&path).unwrap();
    let inside_words = "translator words saved before the final rename";
    save_verse(&handle, 1, 1, inside_words).unwrap();

    let hooks = rig.store.hooks().clone();
    hooks.hold(Point::BeforeRename);
    rig.fs.clear_log();
    let materialized = handle.leave(Leave::Book(TIT));
    hooks.wait_reached(Point::BeforeRename, 1);
    let group_read_the_book = rig.fs.opened().contains(&path);
    let book_temp_is_ready = files(&rig.fs, &folder).keys().any(|rel| {
        let Some((dir, name)) = rel.rsplit_once('/') else {
            return false;
        };
        dir == "ingredients" && tc_store::atomic::temp_target(name) == Some("TIT.usfm")
    });
    rig.fs
        .put_unseen(&path, &outside_bytes, Some(old_stat.mtime_ns));
    let edit_stat = rig.fs.metadata(&path).unwrap();
    hooks.release(Point::BeforeRename);
    let first_attempt = materialized.wait();
    handle.wait_idle();

    assert!(group_read_the_book, "the edit must follow the M1 read");
    assert!(
        book_temp_is_ready,
        "the edit must precede the book's final look"
    );
    assert_eq!(edit_stat.len, old_stat.len);
    assert_eq!(edit_stat.mtime_ns, old_stat.mtime_ns);
    first_attempt.unwrap();
    handle.focus().wait().unwrap();
    handle.leave(Leave::Book(TIT)).wait().unwrap();
    handle.wait_idle();
    let preserved_bytes = rig.fs.peek(&path).as_deref() == Some(outside_bytes.as_slice())
        || copies_of(&handle, BOOK).iter().any(|copy| {
            rig.fs.peek(&folder.join(copy)).as_deref() == Some(outside_bytes.as_slice())
        });
    assert!(
        preserved_bytes,
        "the final look overwrote the same-size/same-time outside bytes without a copy: {:?}",
        handle.notices()
    );
    // The book waits for the person's answer; *Import* takes the outside verse in beside the
    // translator's, both changed since the file was written: a conflict to review.
    assert_eq!(counts(&import_waiting(&handle)), vec![(0, 1)]);
    assert_verse_history(&handle, &outside_words);
    assert_verse_history(&handle, inside_words);
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();

    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert_verse_history(&handle, &outside_words);
    assert_verse_history(&handle, inside_words);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    assert_commits_valid(&rig.fs, &folder);
}
