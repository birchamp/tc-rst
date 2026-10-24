//! "Update a book from USFM" with a file chosen from anywhere (`docs/STORAGE.md` §13.6, no base,
//! `rulings/2026-10-22-H20.md`): nothing newer is overwritten, nothing is removed, and the
//! project's text stays the text shown until the person chooses.

use crate::common::*;
use tc_journal::command::Command;
use tc_journal::fold::DerivedFile;
use tc_journal::outside::UpdatePart;
use tc_journal::{Cause, ImportSource, Record, RecordKey};
use tc_store::{CommandError, ProjectHandle, Refusal, StoreReply};

/// Both parts of the update, the filling first; the reply of each, merged.
fn update(handle: &ProjectHandle, usfm: &str) -> Result<StoreReply, CommandError> {
    let part = |part| {
        handle
            .submit(Command::UpdateBook {
                book: TIT,
                bytes: usfm.as_bytes().to_vec(),
                file_name: "TIT.usfm".to_owned(),
                part,
            })
            .wait()
    };
    let mut reply = part(UpdatePart::Fill)?;
    let review = part(UpdatePart::Review)?;
    reply.changes.extend(review.changes);
    reply.new_conflicts.extend(review.new_conflicts);
    Ok(reply)
}

/// The project's Titus as its book file shows it now.
fn book_file(handle: &ProjectHandle) -> String {
    let bytes = handle
        .snapshot()
        .fold()
        .render(&DerivedFile::Usfm(TIT))
        .unwrap();
    String::from_utf8(bytes).unwrap()
}

fn conflicted(handle: &ProjectHandle, key: &RecordKey) -> bool {
    handle
        .snapshot()
        .fold()
        .conflicts()
        .any(|c| c.key == *key || c.also_keys.contains(key))
}

/// INV-UPDATE-NEVER-OVERWRITES-NEWER-WORK: a verse with words on both sides that differ is a
/// conflict with no parents, and the project's words stay shown. A verse empty in the project
/// takes the file's words with no question; a verse empty in the file leaves the project's.
#[test]
fn update_a_book_conflicts_a_verse_changed_on_both_sides() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &Default::default()).unwrap();
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    save_verse(&handle, 1, 3, "but at the right time").unwrap();
    let file = book_file(&handle)
        .replace("\\v 1 Paul, a servant of God", "\\v 1 Paul, a slave of God")
        .replacen("\\v 2 ___", "\\v 2 in hope of eternal life", 1)
        .replace("\\v 3 but at the right time", "\\v 3 ___");
    let reply = update(&handle, &file).unwrap();

    assert!(conflicted(&handle, &verse(1, 1)));
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("Paul, a servant of God"),
        "the project's words stay shown until the person chooses"
    );
    assert!(!conflicted(&handle, &verse(1, 2)));
    assert_eq!(
        words(&handle, 1, 2).as_deref(),
        Some("in hope of eternal life")
    );
    assert!(!conflicted(&handle, &verse(1, 3)));
    assert_eq!(
        words(&handle, 1, 3).as_deref(),
        Some("but at the right time")
    );
    assert_eq!(
        reply.kept,
        vec![verse(1, 3)],
        "left as it was: empty in the file"
    );

    let fold = handle.snapshot().fold().clone();
    for id in &reply.changes {
        let body = fold.body(id).unwrap();
        let wanted_unknown = body.record.key() == verse(1, 1);
        assert!(
            matches!(&body.cause, Cause::Import(ImportSource::Usfm { file_name, unknown_base })
                if &**file_name == "TIT.usfm" && *unknown_base == wanted_unknown),
            "{:?} {:?}",
            body.record.key(),
            body.cause
        );
    }
    handle.close().unwrap();
}

/// INV-UPDATE-NEVER-REMOVES: a verse the file lacks is never removed: it is left as it was
/// and listed.
#[test]
fn a_verse_the_file_lacks_is_never_removed() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &Default::default()).unwrap();
    save_verse(&handle, 1, 4, "To Titus").unwrap();
    let shown = book_file(&handle);
    let at = shown.find("\\v 4 ").unwrap();
    let end = at + 1 + shown[at + 1..].find("\\v ").unwrap();
    let file = format!("{}{}", &shown[..at], &shown[end..]);
    assert_eq!(
        file.matches("\\v 4 ").count() + 1,
        shown.matches("\\v 4 ").count()
    );
    let reply = update(&handle, &file).unwrap();
    assert!(reply.kept.contains(&verse(1, 4)));
    assert!(!conflicted(&handle, &verse(1, 4)));
    assert_eq!(words(&handle, 1, 4).as_deref(), Some("To Titus"));
    handle.close().unwrap();
}

/// A file that does not read, or that is another book, writes nothing and says why.
#[test]
fn update_a_book_from_an_unreadable_file_writes_nothing_and_names_the_line() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &Default::default()).unwrap();
    let before = handle.snapshot().fold().record_keys().len();
    let twice = "\\id TIT\n\\c 1\n\\p\n\\v 1 One.\n\\v 1 One again.\n";
    match update(&handle, twice) {
        Err(CommandError::Refused(Refusal::Book(why))) => {
            assert!(why.starts_with("the file does not read"), "{why}");
        }
        other => panic!("{other:?}"),
    }
    let other_book = book_file(&handle).replacen("\\id TIT", "\\id PHM", 1);
    match update(&handle, &other_book) {
        Err(CommandError::Refused(Refusal::Book(why))) => {
            assert_eq!(why, "the file is Philemon, not Titus");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(handle.snapshot().fold().record_keys().len(), before);
    handle.close().unwrap();
}

/// INV-UPDATE-KEEPS-ALIGNMENTS (U4): an aligned file's verse that the project held empty comes
/// in with its alignment; a verse in conflict gets none.
#[test]
fn an_aligned_usfm_file_updates_its_alignments() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &Default::default()).unwrap();
    save_verse(&handle, 1, 2, "something else").unwrap();
    // en_ult Titus, every verse aligned: 1:1 fills an empty verse; 1:2 differs.
    let reply = update(&handle, &ult("TIT")).unwrap();
    let fold = handle.snapshot().fold().clone();
    let alignment = |v: u16| {
        fold.projected(&RecordKey::Alignment(
            TIT,
            tc_journal::ChapterNo::new(1).unwrap(),
            tc_journal::VerseNo::new(v).unwrap(),
        ))
    };
    assert!(matches!(alignment(1), Some(Record::Alignment { .. })));
    assert!(
        alignment(2).is_none(),
        "a verse in conflict gets no alignment"
    );
    assert!(conflicted(&handle, &verse(1, 2)));
    assert!(!words(&handle, 1, 1).unwrap().contains("\\zaln"));
    assert!(!reply.changes.is_empty());
    handle.close().unwrap();
}

/// §13.6 U3: a change the update made with no parents is never the text shown, even though it
/// is the newest version of the verse (F7 ranks it last).
#[test]
fn a_no_base_import_never_becomes_the_provisional_text() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &Default::default()).unwrap();
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    let file = book_file(&handle).replace("Paul, a servant of God", "Paul, a slave of God");
    update(&handle, &file).unwrap();
    assert!(conflicted(&handle, &verse(1, 1)));
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("Paul, a servant of God")
    );
    handle.close().unwrap();
}

/// L148: a file exported before the project's last edits reverts nothing: its older verse
/// is a conflict with the newer one, and the newer one stays shown.
#[test]
fn updating_from_an_old_export_reverts_nothing() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &Default::default()).unwrap();
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    let old_export = book_file(&handle);
    save_verse(&handle, 1, 1, "Paul, a slave of God").unwrap();
    update(&handle, &old_export).unwrap();
    assert_eq!(
        words(&handle, 1, 1).as_deref(),
        Some("Paul, a slave of God")
    );
    handle.close().unwrap();
}

/// What a verse holds on one side of an update.
#[derive(Debug, Clone)]
enum Side {
    Empty,
    Words(&'static str),
    /// The file lacks the verse.
    Missing,
}

fn side(project: bool) -> impl proptest::strategy::Strategy<Value = Side> {
    use proptest::prelude::*;
    let words = prop::sample::select(vec!["grace and peace", "to Titus", "a true son", "in hope"]);
    if project {
        prop_oneof![Just(Side::Empty), words.prop_map(Side::Words)].boxed()
    } else {
        prop_oneof![
            Just(Side::Empty),
            Just(Side::Missing),
            words.prop_map(Side::Words)
        ]
        .boxed()
    }
}

/// `file` with TIT 1:v's text replaced by `side`, or the verse cut out.
fn with_verse(file: &str, v: u16, side: &Side) -> String {
    let marker = format!("\\v {v} ");
    let chapter_two = file.find("\\c 2").unwrap_or(file.len());
    let Some(at) = file[..chapter_two].find(&marker) else {
        return file.to_owned();
    };
    let end = at + 1 + file[at + 1..].find("\\v ").unwrap_or(file.len() - at - 1);
    let end = end.min(chapter_two.max(at + 1));
    let text = match side {
        Side::Empty => format!("{marker}___\n"),
        Side::Words(w) => format!("{marker}{w}\n"),
        Side::Missing => String::new(),
    };
    format!("{}{}{}", &file[..at], text, &file[end..])
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig { cases: 24, ..Default::default() })]

    /// INV-UPDATE-NEVER-OVERWRITES-NEWER-WORK and INV-UPDATE-NEVER-REMOVES over random projects
    /// and files: a verse with words keeps them shown and keeps its heads; an empty verse takes
    /// the file's words; no verse is removed.
    #[test]
    fn update_a_book_conserves(
        project in proptest::collection::vec(side(true), 6),
        file in proptest::collection::vec(side(false), 6),
    ) {
        let (rig, folder) = titus();
        let handle = rig.store.open(&folder, &Default::default()).unwrap();
        for (i, s) in project.iter().enumerate() {
            if let Side::Words(w) = s {
                save_verse(&handle, 1, i as u16 + 1, w).unwrap();
            }
        }
        let before = handle.snapshot().fold().clone();
        let mut text = book_file(&handle);
        for (i, s) in file.iter().enumerate() {
            text = with_verse(&text, i as u16 + 1, s);
        }
        update(&handle, &text).unwrap();
        let after = handle.snapshot().fold().clone();
        for (i, (p, f)) in project.iter().zip(&file).enumerate() {
            let v = i as u16 + 1;
            let key = verse(1, v);
            let shown = words(&handle, 1, v);
            proptest::prop_assert!(shown.is_some(), "1:{} was removed", v);
            match (p, f) {
                (Side::Words(w), _) => {
                    proptest::prop_assert_eq!(shown.as_deref(), Some(*w));
                    let heads = after.heads(&key);
                    for h in before.heads(&key) {
                        proptest::prop_assert!(heads.contains(&h), "1:{} lost a head", v);
                    }
                }
                (_, Side::Words(w)) => {
                    proptest::prop_assert_eq!(shown.as_deref(), Some(*w));
                    proptest::prop_assert!(!conflicted(&handle, &key));
                }
                _ => {}
            }
        }
        handle.close().unwrap();
    }
}
