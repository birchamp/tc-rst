//! Drafts of every editor field (`docs/STORAGE.md` §5.14, APP-DESIGN R6, R15, R23): written
//! beside the actor, offered at the next open while their text differs from what the record
//! holds, kept as the save of their field with their own tokens, discarded on request, and
//! carried by a copy of a read-only project (R7).

use crate::common::*;
use tc_journal::command::{AlignmentEdit, Command, Draft, DraftField, SectionValue};
use tc_journal::record::{AlignmentGroup, CommentTarget, SourceWord, TargetWord, VerseValue};
use tc_journal::{ChapterNo, Record, RecordKey, VerseNo};
use tc_store::fault::FaultFs;
use tc_store::{OpenOptions, ProjectHandle};

fn draft(handle: &ProjectHandle, field: DraftField, keys: &[RecordKey], text: &str) {
    let shown = keys.iter().map(|k| handle.mint(k, None).unwrap()).collect();
    handle
        .submit(Command::Draft(Draft {
            field,
            shown,
            text: text.into(),
        }))
        .wait()
        .unwrap();
}

/// The project's files after `handle` wrote its drafts and the process stopped without a
/// close: the drafts are all a crash leaves of unsaved text.
fn crashed_after_drafts(rig: &Rig, handle: ProjectHandle) -> FaultFs {
    handle.wait_idle();
    let after = rig.fs.crash(false);
    drop(handle);
    after
}

fn reopen(fs: FaultFs, folder: &std::path::Path) -> (Rig, ProjectHandle) {
    let rig = rig_as(fs, APP, "host-1");
    let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    (rig, handle)
}

fn alignment_key() -> RecordKey {
    RecordKey::Alignment(TIT, ChapterNo::new(1).unwrap(), VerseNo::new(1).unwrap())
}

/// R6: the section editor's draft carries the token of every verse it displayed; *Keep*
/// saves the section with those tokens, so a verse changed since becomes a conflict.
#[test]
fn a_section_draft_carries_every_displayed_token_and_is_kept_as_the_section_save() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    draft(
        &handle,
        DraftField::Section { book: TIT },
        &[verse(1, 5), verse(1, 6)],
        "\\v 5 five, typed \\v 6 six, typed",
    );
    let fs = crashed_after_drafts(&rig, handle);
    let (_rig, handle) = reopen(fs, &folder);
    let offered = handle.offered_drafts();
    assert_eq!(offered.len(), 1, "{offered:?}");
    let section = offered[0].draft.clone();
    assert_eq!(section.field, DraftField::Section { book: TIT });
    let keys: Vec<RecordKey> = section.shown.iter().map(|t| t.key().clone()).collect();
    assert_eq!(keys, vec![verse(1, 5), verse(1, 6)]);
    // 1:6 changed since the draft's screen was drawn.
    save_verse(&handle, 1, 6, "six, saved elsewhere").unwrap();
    let five = match handle.snapshot().fold().projected(&verse(1, 5)) {
        Some(Record::Verse {
            value:
                VerseValue::Text {
                    marker,
                    layout,
                    last,
                    from,
                    ..
                },
            ..
        }) => VerseValue::Text {
            marker,
            words: "five, typed".into(),
            layout,
            last,
            from,
        },
        other => panic!("{other:?}"),
    };
    let six = match handle.snapshot().fold().projected(&verse(1, 6)) {
        Some(Record::Verse {
            value:
                VerseValue::Text {
                    marker,
                    layout,
                    last,
                    from,
                    ..
                },
            ..
        }) => VerseValue::Text {
            marker,
            words: "six, typed".into(),
            layout,
            last,
            from,
        },
        other => panic!("{other:?}"),
    };
    let reply = handle
        .submit(Command::KeepDraft {
            draft: section.clone(),
            section: vec![
                (section.shown[0].clone(), Some(SectionValue::Verse(five))),
                (section.shown[1].clone(), Some(SectionValue::Verse(six))),
            ],
        })
        .wait()
        .unwrap();
    assert_eq!(words(&handle, 1, 5).as_deref(), Some("five, typed"));
    assert!(
        !reply.new_conflicts.is_empty(),
        "the verse changed since is a conflict, never a revert"
    );
    assert!(
        handle.offered_drafts().is_empty(),
        "a kept draft is no longer offered"
    );
    handle.close().unwrap();
}

/// R15: the comment box's draft, for a new comment (no token) and for a revision of a comment
/// it showed (the comment's token, R24).
#[test]
fn a_comment_box_draft_is_kept_as_a_new_comment_or_as_a_revision() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let target = CommentTarget::Verse {
        book: TIT,
        chapter: ChapterNo::new(1).unwrap(),
        verse: VerseNo::new(3).unwrap(),
    };
    handle
        .submit(Command::AddComment {
            target: target.clone(),
            text: "first text".into(),
        })
        .wait()
        .unwrap();
    let comment = handle
        .snapshot()
        .fold()
        .record_keys()
        .into_iter()
        .find(|k| matches!(k, RecordKey::Comment(_)))
        .unwrap();
    // A new comment typed on 1:4, and a revision of the comment on 1:3, both unsaved.
    let new_target = CommentTarget::Verse {
        book: TIT,
        chapter: ChapterNo::new(1).unwrap(),
        verse: VerseNo::new(4).unwrap(),
    };
    draft(
        &handle,
        DraftField::UserComment {
            target: new_target.clone(),
        },
        &[],
        "a new comment, typed",
    );
    draft(
        &handle,
        DraftField::UserComment {
            target: target.clone(),
        },
        std::slice::from_ref(&comment),
        "first text, revised",
    );
    let fs = crashed_after_drafts(&rig, handle);
    let (_rig, handle) = reopen(fs, &folder);
    let offered = handle.offered_drafts();
    assert_eq!(offered.len(), 2, "{offered:?}");
    for o in &offered {
        assert!(o.verified);
        handle
            .submit(Command::KeepDraft {
                draft: o.draft.clone(),
                section: Vec::new(),
            })
            .wait()
            .unwrap();
    }
    let fold = handle.snapshot().fold().clone();
    let texts: Vec<String> = fold
        .record_keys()
        .into_iter()
        .filter_map(|k| match fold.projected(&k) {
            Some(Record::Comment { value, .. }) => Some(value.text),
            _ => None,
        })
        .collect();
    assert!(
        texts.contains(&"a new comment, typed".to_owned()),
        "{texts:?}"
    );
    assert!(
        texts.contains(&"first text, revised".to_owned()),
        "{texts:?}"
    );
    assert_eq!(
        fold.history(&comment).len(),
        2,
        "the earlier text stays in History"
    );
    assert!(handle.offered_drafts().is_empty());
    handle.close().unwrap();
}

/// R23: the aligner's unsaved links survive a crash as a draft and are kept as an alignment
/// save; a check comment's draft is kept the same way.
#[test]
fn an_alignment_draft_and_a_check_comment_draft_survive_a_crash() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    let links = AlignmentEdit {
        alignments: vec![AlignmentGroup {
            top_words: vec![SourceWord {
                word: "Παῦλος".into(),
                strong: None,
                lemma: None,
                morph: None,
                occurrence: 1,
                occurrences: 1,
                extra: Default::default(),
            }],
            bottom_words: vec![TargetWord {
                word: "Paul".into(),
                occurrence: 1,
                occurrences: 1,
                extra: Default::default(),
            }],
            extra: Default::default(),
        }],
        word_bank: Vec::new(),
        done: false,
    };
    draft(
        &handle,
        DraftField::Alignment,
        &[alignment_key(), verse(1, 1)],
        &links.to_json(),
    );
    let fs = crashed_after_drafts(&rig, handle);
    let (_rig, handle) = reopen(fs, &folder);
    let offered = handle.offered_drafts();
    assert_eq!(offered.len(), 1, "{offered:?}");
    assert_eq!(
        AlignmentEdit::from_json(&offered[0].draft.text).as_ref(),
        Some(&links)
    );
    handle
        .submit(Command::KeepDraft {
            draft: offered[0].draft.clone(),
            section: Vec::new(),
        })
        .wait()
        .unwrap();
    match handle.snapshot().fold().projected(&alignment_key()) {
        Some(Record::Alignment { value, .. }) => {
            assert_eq!(value.alignments.len(), 1);
            assert_eq!(value.alignments[0].bottom_words[0].word, "Paul");
        }
        other => panic!("{other:?}"),
    }
    assert!(handle.offered_drafts().is_empty());
    handle.close().unwrap();
}

/// A discarded draft is gone, and a draft whose text the record now holds is not offered.
#[test]
fn a_discarded_or_saved_draft_is_not_offered_again() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    draft(&handle, DraftField::Words, &[verse(1, 7)], "discard me");
    draft(&handle, DraftField::Words, &[verse(1, 8)], "saved later");
    let fs = crashed_after_drafts(&rig, handle);
    let (rig, handle) = reopen(fs, &folder);
    assert_eq!(handle.offered_drafts().len(), 2);
    let discard = handle
        .offered_drafts()
        .into_iter()
        .find(|o| o.draft.text == "discard me")
        .unwrap();
    handle.discard_draft(discard.id);
    // The other text reaches the record by a save from another screen.
    save_verse(&handle, 1, 8, "saved later").unwrap();
    let fs = crashed_after_drafts(&rig, handle);
    let (_rig, handle) = reopen(fs, &folder);
    assert!(
        handle.offered_drafts().is_empty(),
        "{:?}",
        handle.offered_drafts()
    );
    handle.close().unwrap();
}

/// R7: the copy offered by a read-only open carries the drafts, and the copy's first open
/// offers them.
#[test]
fn a_copy_of_a_read_only_project_carries_its_drafts() {
    let (rig, folder) = titus();
    let writer = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    draft(
        &writer,
        DraftField::Words,
        &[verse(1, 9)],
        "typed before the copy",
    );
    writer.wait_idle();
    // The folder reaches another computer that finds the lock naming this one: read-only,
    // and the person copies the project.
    let other = rig_as(rig.fs.other_machine(), "/app-desktop", "host-2");
    let reader = other.store.open(&folder, &OpenOptions::default()).unwrap();
    assert!(matches!(
        reader.facts().mode,
        tc_store::Mode::ReadOnly(tc_store::ReadOnlyReason::OpenOnAnotherComputer { .. })
    ));
    assert!(
        reader.offered_drafts().is_empty(),
        "a read-only open offers nothing"
    );
    reader.close().unwrap();
    let copy = rig.store.copy_project(&folder, "Titus copy").unwrap();
    writer.close().unwrap();
    let handle = rig.store.open(&copy, &OpenOptions::default()).unwrap();
    let offered = handle.offered_drafts();
    assert_eq!(offered.len(), 1, "{offered:?}");
    assert_eq!(offered[0].draft.text, "typed before the copy");
    handle.close().unwrap();
}

/// R7: dismissing a notice removes it and the byte copy it kept (§3.6), when the copy is the
/// only trace of bytes the app replaced.
#[test]
fn a_dismissed_notice_goes_with_its_copy() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    handle.leave(tc_store::Leave::Book(TIT)).wait().unwrap();
    handle.close().unwrap();
    // The project name edited in a text editor: `metadata.json` is written from the
    // project, and the bytes found are kept in a copy.
    let path = folder.join("metadata.json");
    let text = String::from_utf8(rig.fs.peek(&path).unwrap()).unwrap();
    let edited = text.replacen("Titus test translation", "Titus, renamed", 1);
    rig.fs.put(&path, edited.as_bytes(), None);
    rig.fs
        .delete(&folder.join(".tc/materialized.json"))
        .unwrap();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let notice = handle
        .notices()
        .into_iter()
        .find(|n| n.copy.is_some())
        .expect("an outside edit keeps a copy");
    let copy = folder.join(notice.copy.clone().unwrap());
    assert_eq!(rig.fs.peek(&copy), Some(edited.into_bytes()));
    handle.dismiss_notice(notice.id).wait().unwrap();
    assert!(handle.notices().iter().all(|n| n.id != notice.id));
    assert!(
        rig.fs.peek(&copy).is_none(),
        "the copy goes with the notice"
    );
    handle.close().unwrap();
}

/// A held file's copy is the other trace of the person's bytes, and the notice comes back at
/// the next open: dismissing it hides it for the session and keeps the copy (§6.2).
#[test]
fn a_dismissed_notice_of_a_held_file_keeps_its_copy() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    save_verse(&handle, 1, 1, "Paul, a servant").unwrap();
    handle.leave(tc_store::Leave::Book(TIT)).wait().unwrap();
    handle.close().unwrap();
    // An edit made outside the app: held, with a copy in .tc/external/.
    let usfm = folder.join("ingredients/TIT.usfm");
    let mut edited = rig.fs.peek(&usfm).unwrap();
    edited.extend_from_slice(b"\n");
    rig.fs.put(&usfm, &edited, None);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let notice = handle
        .notices()
        .into_iter()
        .find(|n| n.copy.is_some())
        .expect("a held edit keeps a copy");
    let copy = folder.join(notice.copy.clone().unwrap());
    assert!(rig.fs.peek(&copy).is_some());
    handle.dismiss_notice(notice.id).wait().unwrap();
    assert!(handle.notices().iter().all(|n| n.id != notice.id));
    assert!(rig.fs.peek(&copy).is_some(), "the file is still held");
    assert_eq!(rig.fs.peek(&usfm), Some(edited), "the edited file stays");
    handle.close().unwrap();
}

fn texts_offered(handle: &ProjectHandle) -> Vec<String> {
    let mut texts: Vec<String> = handle
        .offered_drafts()
        .into_iter()
        .map(|o| o.draft.text)
        .collect();
    texts.sort();
    texts
}

/// APP-DESIGN §6.6 (C1b, finding 1): one field can hold two offered texts, the last session's
/// draft and a text taken in from the page's buffer. *Keep* of one leaves the other offered,
/// at a reload and after a crash, until the person answers it.
#[test]
fn keeping_one_of_two_offered_texts_of_a_field_leaves_the_other() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    draft(
        &handle,
        DraftField::Words,
        &[verse(1, 4)],
        "from drafts.json",
    );
    let fs = crashed_after_drafts(&rig, handle);
    let (rig, handle) = reopen(fs, &folder);
    let buffered = Draft {
        field: DraftField::Words,
        shown: vec![handle.mint(&verse(1, 4), None).unwrap()],
        text: "from the buffer".into(),
    };
    let id = handle
        .offer_draft(buffered.clone(), true, tc_journal::WallTime(1))
        .unwrap();
    assert_eq!(
        handle.offer_draft(buffered.clone(), true, tc_journal::WallTime(2)),
        Some(id),
        "one text, one entry"
    );
    assert_eq!(
        texts_offered(&handle),
        ["from drafts.json", "from the buffer"]
    );
    handle
        .submit(Command::KeepDraft {
            draft: buffered,
            section: Vec::new(),
        })
        .wait()
        .unwrap();
    assert_eq!(words(&handle, 1, 4).as_deref(), Some("from the buffer"));
    assert_eq!(texts_offered(&handle), ["from drafts.json"]);
    let fs = crashed_after_drafts(&rig, handle);
    let (_rig, handle) = reopen(fs, &folder);
    assert_eq!(texts_offered(&handle), ["from drafts.json"]);
    handle.close().unwrap();
}

/// Finding 9: *Discard* of an offered text removes that entry, written before it returns,
/// and never the field's draft typed after the offer.
#[test]
fn discard_leaves_the_fields_draft_typed_since() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    draft(
        &handle,
        DraftField::Words,
        &[verse(1, 5)],
        "offered, discarded",
    );
    let fs = crashed_after_drafts(&rig, handle);
    let (rig, handle) = reopen(fs, &folder);
    let offered = handle.offered_drafts();
    draft(
        &handle,
        DraftField::Words,
        &[verse(1, 5)],
        "typed after the offer",
    );
    handle.wait_idle();
    assert!(handle.discard_draft(offered[0].id));
    // No flush: the discard was written before it returned.
    let fs = rig.fs.crash(false);
    drop(handle);
    let (_rig, handle) = reopen(fs, &folder);
    assert_eq!(texts_offered(&handle), ["typed after the offer"]);
    handle.close().unwrap();
}

/// APP-DESIGN §6.6: a draft records the app's run that typed it, so the app can tell whether
/// that run's page kept newer typing (finding 5 is judged there, with the shell's mirror); an
/// open of a later run offers it, and an open of the same run keeps it as that page's own.
#[test]
fn a_draft_records_its_run_and_only_a_later_run_is_offered_it() {
    let (rig, folder) = titus();
    let run = |name: &str| OpenOptions {
        run: Some(name.to_owned()),
        ..OpenOptions::default()
    };
    let handle = rig.store.open(&folder, &run("run-1")).unwrap();
    draft(&handle, DraftField::Words, &[verse(1, 6)], "typed in run 1");
    let fs = crashed_after_drafts(&rig, handle);
    let rig = rig_as(fs, APP, "host-1");
    let handle = rig.store.open(&folder, &run("run-1")).unwrap();
    assert!(
        handle.offered_drafts().is_empty(),
        "the same run's own typing"
    );
    let fs = crashed_after_drafts(&rig, handle);
    let rig = rig_as(fs, APP, "host-1");
    let handle = rig.store.open(&folder, &run("run-2")).unwrap();
    let offered = handle.offered_drafts();
    assert_eq!(offered.len(), 1, "{offered:?}");
    assert_eq!(offered[0].run.as_deref(), Some("run-1"));
    handle.close().unwrap();
}

/// An offered text is marked so in the file at the open that offers it: a save of the field
/// in that session, which the person made without seeing it, never supersedes it.
#[test]
fn an_offered_draft_outlives_the_next_sessions_save_of_its_field() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    draft(&handle, DraftField::Words, &[verse(1, 7)], "never answered");
    let fs = crashed_after_drafts(&rig, handle);
    let (rig, handle) = reopen(fs, &folder);
    assert_eq!(texts_offered(&handle), ["never answered"]);
    save_verse(&handle, 1, 7, "typed without answering").unwrap();
    let fs = crashed_after_drafts(&rig, handle);
    let (_rig, handle) = reopen(fs, &folder);
    assert_eq!(texts_offered(&handle), ["never answered"]);
    handle.close().unwrap();
}
