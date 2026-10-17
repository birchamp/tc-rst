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
    // A second window: read-only, and the person copies the project.
    let reader = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    assert!(matches!(reader.facts().mode, tc_store::Mode::ReadOnly(_)));
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

/// R7: dismissing a notice removes it and the byte copy it kept (§3.6).
#[test]
fn a_dismissed_notice_goes_with_its_copy() {
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
    assert!(
        rig.fs.peek(&copy).is_none(),
        "the copy goes with the notice"
    );
    assert_eq!(rig.fs.peek(&usfm), Some(edited), "the edited file stays");
    handle.close().unwrap();
}
