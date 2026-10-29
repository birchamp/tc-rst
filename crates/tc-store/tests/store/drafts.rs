//! Drafts of every editor field (`docs/STORAGE.md` §5.14, APP-DESIGN R6, R15, R23): written
//! beside the actor, offered at the next open while their text differs from what the record
//! holds, kept as the save of their field with their own tokens, discarded on request, and
//! carried by a copy of a read-only project (R7).

use crate::common::*;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tc_journal::command::{AlignmentEdit, Command, Draft, DraftField, SectionValue};
use tc_journal::record::{AlignmentGroup, CommentTarget, SourceWord, TargetWord, VerseValue};
use tc_journal::{ChapterNo, Record, RecordKey, VerseNo};
use tc_store::fault::FaultFs;
use tc_store::{Hooks, OpenOptions, Point, ProjectHandle};

fn draft(handle: &ProjectHandle, field: DraftField, keys: &[RecordKey], text: &str) {
    let shown = keys.iter().map(|k| handle.mint(k, None).unwrap()).collect();
    handle
        .submit(Command::Draft {
            draft: Draft {
                field,
                shown,
                text: text.into(),
            },
            at: tc_journal::WallTime(0),
        })
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
    writer.close().unwrap();
    let fs = rig.fs.clone();
    drop(rig);
    // The projects folder is found to be cloud-synced: read-only, and the person copies the
    // project.
    let cloud = cloud_synced_rig(fs.clone());
    let reader = cloud.store.open(&folder, &OpenOptions::default()).unwrap();
    assert!(matches!(
        reader.facts().mode,
        tc_store::Mode::ReadOnly(tc_store::ReadOnlyReason::UnsafeFolder(_))
    ));
    assert!(
        reader.offered_drafts().is_empty(),
        "a read-only open offers nothing"
    );
    reader.close().unwrap();
    let copy = cloud.store.copy_project(&folder, "Titus copy").unwrap();
    drop(cloud);
    let rig = crate::common::rig(fs);
    let handle = rig.store.open(&copy, &OpenOptions::default()).unwrap();
    let offered = handle.offered_drafts();
    assert_eq!(offered.len(), 1, "{offered:?}");
    assert_eq!(offered[0].draft.text, "typed before the copy");
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
    // Bytes changed under the app's feet, with the file's size and time as they were: held
    // as storage damage, with a copy in .tc/external/.
    settle_entries(&rig, &folder);
    let usfm = folder.join("ingredients/TIT.usfm");
    let edited = damage_file(&rig, &folder, "ingredients/TIT.usfm");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let notice = handle
        .notices()
        .into_iter()
        .find(|n| n.copy.is_some())
        .expect("a held file keeps a copy");
    let copy = folder.join(notice.copy.clone().unwrap());
    assert!(rig.fs.peek(&copy).is_some());
    handle.dismiss_notice(notice.id).wait().unwrap();
    assert!(handle.notices().iter().all(|n| n.id != notice.id));
    assert!(rig.fs.peek(&copy).is_some(), "the file is still held");
    assert_eq!(rig.fs.peek(&usfm), Some(edited), "the edited file stays");
    handle.close().unwrap();
}

fn copied_metadata_notice() -> (
    Rig,
    std::path::PathBuf,
    tc_store::ProjectHandle,
    tc_store::StoreNotice,
    Vec<u8>,
) {
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
    assert_eq!(rig.fs.peek(&copy), Some(edited.as_bytes().to_vec()));
    (rig, folder, handle, notice, edited.into_bytes())
}

/// R7, §3.6: dismissing a notice removes the byte copy it kept, when the copy is the only
/// trace of bytes the app replaced. A dismissal the disk refuses keeps both for a retry.
#[test]
fn a_failed_dismissal_keeps_the_notice_and_its_copy_for_retry() {
    let (rig, folder, handle, notice, _) = copied_metadata_notice();
    let copy = folder.join(notice.copy.clone().unwrap());
    rig.fs.set_busy(&copy, true);
    assert!(handle.dismiss_notice(notice.id).wait().is_err());
    assert!(rig.fs.peek(&copy).is_some());
    assert!(
        handle
            .notices()
            .iter()
            .any(|n| n.id == notice.id && n.copy == notice.copy),
        "a failed dismissal must keep its original notice and exact copy route"
    );
    rig.fs.set_busy(&copy, false);
    handle.dismiss_notice(notice.id).wait().unwrap();
    assert!(handle.notices().iter().all(|n| n.id != notice.id));
    assert!(
        rig.fs.peek(&copy).is_none(),
        "the copy goes with the notice"
    );
    handle.close().unwrap();
}

#[test]
fn a_copy_stat_denial_cannot_turn_dismissal_into_success() {
    let (rig, folder, handle, notice, original) = copied_metadata_notice();
    let copy = folder.join(notice.copy.as_ref().unwrap());
    rig.fs.set_unreadable(&copy, true);
    assert!(
        handle.dismiss_notice(notice.id).wait().is_err(),
        "copy stat/read denial cannot be treated as absence"
    );
    assert!(handle.notices().iter().any(|n| n.id == notice.id));
    assert_eq!(rig.fs.peek(&copy), Some(original));
    rig.fs.set_unreadable(&copy, false);
    handle.dismiss_notice(notice.id).wait().unwrap();
    assert!(rig.fs.peek(&copy).is_none());
    handle.close().unwrap();
}

#[test]
fn dismissing_an_older_copy_never_deletes_its_unseen_replacement() {
    let (rig, folder, handle, notice, original) = copied_metadata_notice();
    let copy = folder.join(notice.copy.as_ref().unwrap());
    let replacement =
        b"a different person's retained bytes must not be dismissed with the old image";
    rig.fs.put_unseen(&copy, replacement, None);
    assert!(
        handle.dismiss_notice(notice.id).wait().is_err(),
        "dismissing an old image cannot authorize destroying unseen replacement bytes"
    );
    assert_eq!(rig.fs.peek(&copy).as_deref(), Some(replacement.as_slice()));
    assert!(handle.notices().iter().any(|n| n.id == notice.id));
    rig.fs.put_unseen(&copy, &original, None);
    handle.dismiss_notice(notice.id).wait().unwrap();
    assert!(rig.fs.peek(&copy).is_none());
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

/// A completed write is the barrier, so the crash does not depend on the writer's throttle.
fn wait_for_draft_write(handle: &ProjectHandle, previous: u64) {
    let until = Instant::now() + Duration::from_secs(10);
    while handle.counters().drafts_written <= previous {
        assert!(Instant::now() < until, "the draft writer did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn link_group(source: &str, target: &str) -> AlignmentGroup {
    AlignmentGroup {
        top_words: vec![SourceWord {
            word: source.into(),
            strong: None,
            lemma: None,
            morph: None,
            occurrence: 1,
            occurrences: 1,
            extra: Default::default(),
        }],
        bottom_words: vec![TargetWord {
            word: target.into(),
            occurrence: 1,
            occurrences: 1,
            extra: Default::default(),
        }],
        extra: Default::default(),
    }
}

/// R26: the record and writer complete before Ack; no align actor can refresh the draft.
#[test]
fn a_newer_alignment_draft_survives_a_crash_before_the_saves_ack() {
    let hooks = Arc::new(Hooks::default());
    let rig = rig_with_hooks(FaultFs::new(), hooks.clone());
    let folder = create_titus(&rig, "Titus");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    handle.wait_idle();
    let key = alignment_key();
    let shown = handle.mint(&key, None).unwrap();
    let verse_shown = handle.mint(&verse(1, 1), None).unwrap();
    let first = AlignmentEdit {
        alignments: vec![link_group("Παῦλος", "Paul")],
        word_bank: Vec::new(),
        done: false,
    };
    let mut newer = first.clone();
    newer.alignments.push(link_group("δοῦλος", "servant"));
    hooks.hold(Point::BeforeDurablePending);
    hooks.hold(Point::BeforeCommandReply);
    let saving = handle.submit(Command::SaveAlignment {
        shown: shown.clone(),
        verse_shown: verse_shown.clone(),
        value: first.clone(),
    });
    hooks.wait_reached(Point::BeforeDurablePending, 1);
    let writes = handle.counters().drafts_written;
    handle
        .submit(Command::Draft {
            draft: Draft {
                field: DraftField::Alignment,
                shown: vec![shown.clone(), verse_shown.clone()],
                text: newer.to_json(),
            },
            // Equal wall clocks must not make a newer draft look older than the save.
            at: tc_journal::WallTime(0),
        })
        .wait()
        .unwrap();
    wait_for_draft_write(&handle, writes);
    hooks.release(Point::BeforeDurablePending);
    hooks.wait_reached(Point::BeforeCommandReply, 1);

    // This other field forces the writer past the save notification and into a durable
    // write while Ack is held. It cannot recreate the alignment draft if the save erased it.
    let writes = handle.counters().drafts_written;
    draft(
        &handle,
        DraftField::Words,
        &[verse(1, 2)],
        "another unsaved verse",
    );
    wait_for_draft_write(&handle, writes);
    let after = rig.fs.crash(true);
    hooks.release(Point::BeforeCommandReply);
    saving.wait().unwrap();
    handle.close().unwrap();

    let (rig, handle) = reopen(after, &folder);
    let saved = handle.snapshot().fold().projected(&key).unwrap();
    assert!(
        matches!(saved, Record::Alignment { value, .. } if value.alignments == first.alignments)
    );
    let offers = handle.offered_drafts();
    let offered = offers
        .iter()
        .find(|offer| offer.draft.field == DraftField::Alignment)
        .expect("the newer durable links must be offered before any actor Ack");
    assert_eq!(offered.draft.shown, vec![shown, verse_shown]);
    assert_eq!(
        AlignmentEdit::from_json(&offered.draft.text),
        Some(newer.clone())
    );
    let kept = handle
        .submit(Command::KeepDraft {
            draft: offered.draft.clone(),
            section: Vec::new(),
        })
        .wait()
        .unwrap();
    // Ruling H5: no conflict. The newest by (at, id) wins; the test clock starts again
    // at reopen, so here the save does. Both versions stay heads, in History.
    assert!(kept.new_conflicts.is_empty(), "no alignment conflict");
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    let heads = fold.heads(&key);
    assert_eq!(heads.len(), 2, "the intervening save stays a head");
    let history = fold.history(&key);
    assert!(heads.iter().all(|head| history.contains(head)));
    assert!(heads.iter().any(|head| {
        matches!(fold.body(head).map(|body| body.record),
            Some(Record::Alignment { value, .. }) if value.alignments == newer.alignments)
    }));
    let newest = heads
        .iter()
        .max_by_key(|h| (fold.body(h).unwrap().at, **h))
        .unwrap();
    assert_eq!(fold.projected(&key), fold.body(newest).map(|b| b.record));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// Both permitted removals are observable on disk, rather than hidden by the open's filter.
#[test]
fn a_save_clears_an_older_different_draft_and_a_newer_equal_draft() {
    let hooks = Arc::new(Hooks::default());
    let rig = rig_with_hooks(FaultFs::new(), hooks.clone());
    let folder = create_titus(&rig, "Titus");
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    draft(&handle, DraftField::Words, &[verse(1, 3)], "older draft");
    save_verse(&handle, 1, 3, "later saved text").unwrap();
    handle.wait_idle();

    hooks.hold(Point::BeforeDurablePending);
    let saving = handle.submit(Command::SaveVerse {
        shown: handle.mint(&verse(1, 4), None).unwrap(),
        words: "saved café".into(),
    });
    hooks.wait_reached(Point::BeforeDurablePending, 1);
    draft(
        &handle,
        DraftField::Words,
        &[verse(1, 4)],
        "saved cafe\u{301}",
    );
    hooks.release(Point::BeforeDurablePending);
    saving.wait().unwrap();
    handle.wait_idle();
    let bytes = rig.fs.peek(&folder.join(".tc/drafts.json")).unwrap();
    let entries: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert!(entries.is_empty(), "{entries:#?}");
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

#[test]
fn cancelling_a_current_section_draft_is_durable_and_preserves_its_offer() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let cancelled = Draft {
        field: DraftField::Section { book: TIT },
        shown: vec![handle.mint(&verse(1, 3), None).unwrap()],
        text: "3 a cancelled section".into(),
    };
    let at = tc_journal::WallTime(123);
    let offered = handle.offer_draft(cancelled.clone(), true, at).unwrap();
    handle
        .submit(Command::Draft {
            draft: cancelled.clone(),
            at,
        })
        .wait()
        .unwrap();
    assert!(handle.discard_current_draft(cancelled, at));
    assert_eq!(handle.offered_drafts()[0].id, offered);
    let fs = rig.fs.crash(true);
    drop(handle);
    let (rig, handle) = reopen(fs, &folder);
    let drafts = handle.offered_drafts();
    assert_eq!(drafts.len(), 1, "only the independent offer remains");
    assert_eq!(drafts[0].draft.text, "3 a cancelled section");
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

#[test]
fn a_failed_current_draft_cancel_reports_failure_and_can_be_retried() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let cancelled = Draft {
        field: DraftField::Section { book: TIT },
        shown: vec![handle.mint(&verse(1, 3), None).unwrap()],
        text: "3 cancel after the disk recovers".into(),
    };
    let at = tc_journal::WallTime(321);
    handle
        .submit(Command::Draft {
            draft: cancelled.clone(),
            at,
        })
        .wait()
        .unwrap();
    handle.wait_idle();
    rig.fs
        .fail_writes(Some(std::io::ErrorKind::PermissionDenied));
    assert!(!handle.discard_current_draft(cancelled.clone(), at));
    let bytes = rig.fs.peek(&folder.join(".tc/drafts.json")).unwrap();
    let entries: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(entries[0]["text"], cancelled.text);
    rig.fs.fail_writes(None);
    assert!(handle.discard_current_draft(cancelled, at));
    let fs = rig.fs.crash(true);
    drop(handle);
    let (rig, handle) = reopen(fs, &folder);
    assert!(handle.offered_drafts().is_empty());
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// Applies `ops` (a put or a cancel of version n) to one section draft; after a crash, the
/// draft offered is the last one put that no cancel of the same text and time removed.
fn stale_cancel_case(ops: Vec<(bool, u8)>) -> Result<(), proptest::test_runner::TestCaseError> {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let shown = vec![handle.mint(&verse(1, 3), None).unwrap()];
    let mut current: Option<(String, i64)> = None;
    for (put, version) in ops {
        let text = format!("3 typed version {version}");
        let at = i64::from(version);
        let draft = Draft {
            field: DraftField::Section { book: TIT },
            shown: shown.clone(),
            text: text.clone(),
        };
        if put {
            handle
                .submit(Command::Draft {
                    draft,
                    at: tc_journal::WallTime(at),
                })
                .wait()
                .unwrap();
            current = Some((text, at));
        } else {
            proptest::prop_assert!(handle.discard_current_draft(draft, tc_journal::WallTime(at)));
            if current.as_ref() == Some(&(text, at)) {
                current = None;
            }
        }
    }
    let fs = crashed_after_drafts(&rig, handle);
    let (rig, handle) = reopen(fs, &folder);
    let offered: Vec<_> = handle
        .offered_drafts()
        .into_iter()
        .map(|d| (d.draft.text, d.at.0))
        .collect();
    proptest::prop_assert_eq!(offered, current.into_iter().collect::<Vec<_>>());
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
    Ok(())
}

fn cancel_ops() -> impl proptest::strategy::Strategy<Value = Vec<(bool, u8)>> {
    proptest::collection::vec((proptest::bool::ANY, 0u8..4), 1..24)
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(8))]
    #[test]
    fn stale_cancel_never_discards_newer_or_different_current_text(ops in cancel_ops()) {
        stale_cancel_case(ops)?;
    }
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(128))]
    #[test]
    #[ignore = "nightly: 128 cases; the gate runs stale_cancel_never_discards_newer_or_different_current_text at 8"]
    fn stale_cancel_never_discards_newer_or_different_current_text_nightly(ops in cancel_ops()) {
        stale_cancel_case(ops)?;
    }
}

const DRAFTS: &str = ".tc/drafts.json";

fn unreadable_notices(handle: &ProjectHandle) -> Vec<String> {
    handle
        .notices()
        .into_iter()
        .filter_map(|n| match n.notice {
            tc_journal::fold::Notice::FileUnreadable { path, .. } => Some(path.to_string()),
            _ => None,
        })
        .collect()
}

/// A drafts file the app cannot understand (damaged, cut short, a newer app's layout) is
/// never written over: it is moved aside under a name no earlier one holds, the person is
/// told where, and this session's drafts are written again.
#[test]
fn an_unreadable_drafts_file_is_moved_aside_and_the_person_is_told() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.close().unwrap();
    let damaged: [&[u8]; 3] = [
        b"\x00\x00garbage",
        br#"[{"field":"words","shown":[],"text":"half a dra"#,
        br#"{"version":9,"entries":[{"text":"newer layout"}]}"#,
    ];
    let mut fs = rig.fs.copy();
    for (n, bytes) in damaged.iter().enumerate() {
        fs.put(&folder.join(DRAFTS), bytes, None);
        let (rig, handle) = reopen(fs, &folder);
        let kept = format!(".tc/drafts.unreadable-{}.json", n + 1);
        assert_eq!(unreadable_notices(&handle), aside(1..=n + 1));
        assert_eq!(rig.fs.peek(&folder.join(&kept)).as_deref(), Some(*bytes));
        draft(&handle, DraftField::Words, &[verse(1, 2)], "typed after");
        let fs_after = crashed_after_drafts(&rig, handle);
        let bytes = fs_after.peek(&folder.join(DRAFTS)).unwrap();
        let entries: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            entries[0]["text"], "typed after",
            "drafts are written again"
        );
        for (i, earlier) in damaged[..=n].iter().enumerate() {
            let path = folder.join(format!(".tc/drafts.unreadable-{}.json", i + 1));
            assert_eq!(
                fs_after.peek(&path).as_deref(),
                Some(*earlier),
                "an earlier kept file is never replaced"
            );
        }
        fs = fs_after;
    }
    let (rig, handle) = reopen(fs, &folder);
    assert_eq!(texts_offered(&handle), ["typed after"]);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

fn aside(numbers: std::ops::RangeInclusive<usize>) -> Vec<String> {
    numbers
        .map(|n| format!(".tc/drafts.unreadable-{n}.json"))
        .collect()
}

/// A drafts file moved aside is told again at every later open, even when the session that
/// moved it typed nothing, so `drafts.json` was never written again. Dismissing the notice
/// hides it for the session and keeps the file: it may hold the only trace of unsaved text.
#[test]
fn a_moved_aside_drafts_file_is_told_at_every_open_until_it_is_gone() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.close().unwrap();
    let fs = rig.fs.copy();
    fs.put(&folder.join(DRAFTS), b"\x00garbage", None);
    let (rig, handle) = reopen(fs, &folder);
    assert_eq!(unreadable_notices(&handle), aside(1..=1));
    handle.close().unwrap();
    let (rig, handle) = reopen(rig.fs.copy(), &folder);
    assert_eq!(unreadable_notices(&handle), aside(1..=1));
    let notice = handle.notices().into_iter().next().unwrap();
    handle.dismiss_notice(notice.id).wait().unwrap();
    assert!(unreadable_notices(&handle).is_empty());
    handle.close().unwrap();
    let kept = folder.join(&aside(1..=1)[0]);
    assert_eq!(rig.fs.peek(&kept).as_deref(), Some(&b"\x00garbage"[..]));
    let (rig, handle) = reopen(rig.fs.copy(), &folder);
    assert_eq!(unreadable_notices(&handle), aside(1..=1));
    handle.close().unwrap();
    // The person dealt with it: moved or deleted the file outside the app.
    rig.fs.delete(&kept).unwrap();
    let (_rig, handle) = reopen(rig.fs.copy(), &folder);
    assert!(unreadable_notices(&handle).is_empty());
    handle.close().unwrap();
}

/// Every file moved aside is told, in order, and a good `drafts.json` beside them is still
/// offered; a read-only open tells of them too, and leaves them as they are.
#[test]
fn moved_aside_drafts_files_are_told_beside_offered_drafts_and_when_read_only() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    draft(
        &handle,
        DraftField::Words,
        &[verse(1, 2)],
        "last session's text",
    );
    let fs = crashed_after_drafts(&rig, handle);
    fs.put(&folder.join(&aside(2..=2)[0]), b"second", None);
    fs.put(&folder.join(&aside(10..=10)[0]), b"tenth", None);
    let (rig, handle) = reopen(fs, &folder);
    assert_eq!(
        unreadable_notices(&handle),
        [aside(2..=2), aside(10..=10)].concat()
    );
    assert_eq!(texts_offered(&handle), ["last session's text"]);
    handle.close().unwrap();
    let fs = rig.fs.copy();
    drop(rig);
    let cloud = cloud_synced_rig(fs);
    let reader = cloud.store.open(&folder, &OpenOptions::default()).unwrap();
    assert!(matches!(reader.facts().mode, tc_store::Mode::ReadOnly(_)));
    assert_eq!(
        unreadable_notices(&reader),
        [aside(2..=2), aside(10..=10)].concat()
    );
    assert!(reader.offered_drafts().is_empty());
    reader.close().unwrap();
    assert_eq!(
        cloud.fs.peek(&folder.join(&aside(2..=2)[0])).as_deref(),
        Some(&b"second"[..])
    );
}

/// Another program holds the drafts file so that it can be neither read nor moved (a
/// scanner on Windows): it is left as it is, nothing writes over it this session, and the
/// next open, once it is free, offers the drafts in it.
#[test]
fn a_held_drafts_file_is_left_untouched_and_offered_at_the_next_open() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    draft(
        &handle,
        DraftField::Words,
        &[verse(1, 2)],
        "last session's text",
    );
    let fs = crashed_after_drafts(&rig, handle);
    let path = folder.join(DRAFTS);
    let before = fs.peek(&path).unwrap();
    fs.set_unreadable(&path, true);
    fs.set_busy(&path, true);
    let (rig, handle) = reopen(fs, &folder);
    assert_eq!(unreadable_notices(&handle), [DRAFTS]);
    assert!(handle.offered_drafts().is_empty());
    draft(
        &handle,
        DraftField::Words,
        &[verse(1, 3)],
        "this session's text",
    );
    handle.wait_idle();
    assert_eq!(
        rig.fs.peek(&path),
        Some(before.clone()),
        "the held file is untouched"
    );
    handle.close().unwrap();
    assert_eq!(rig.fs.peek(&path), Some(before));
    rig.fs.set_unreadable(&path, false);
    rig.fs.set_busy(&path, false);
    let fs = rig.fs.copy();
    let (rig, handle) = reopen(fs, &folder);
    assert!(unreadable_notices(&handle).is_empty());
    assert_eq!(texts_offered(&handle), ["last session's text"]);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// A drafts file that cannot be read, though it could be moved, stays where it is: its bytes
/// may be sound, and the next open offers them.
#[test]
fn a_drafts_file_that_cannot_be_read_is_not_moved_aside() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    draft(
        &handle,
        DraftField::Words,
        &[verse(1, 2)],
        "last session's text",
    );
    let fs = crashed_after_drafts(&rig, handle);
    let path = folder.join(DRAFTS);
    let before = fs.peek(&path).unwrap();
    fs.set_unreadable(&path, true);
    let (rig, handle) = reopen(fs, &folder);
    assert_eq!(unreadable_notices(&handle), [DRAFTS]);
    draft(
        &handle,
        DraftField::Words,
        &[verse(1, 3)],
        "this session's text",
    );
    handle.wait_idle();
    handle.close().unwrap();
    assert_eq!(
        rig.fs.peek(&path),
        Some(before),
        "the file is neither moved nor replaced"
    );
    rig.fs.set_unreadable(&path, false);
    let (_rig, handle) = reopen(rig.fs.copy(), &folder);
    assert_eq!(texts_offered(&handle), ["last session's text"]);
    handle.close().unwrap();
}

/// An entry of a field this version does not know, written by a newer app, stays in the file
/// while this version writes its own drafts beside it.
#[test]
fn a_newer_apps_draft_entry_is_kept_in_the_file() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.close().unwrap();
    let newer = serde_json::json!({
        "field": "someNewerField",
        "shown": [],
        "text": "typed in a newer app",
        "at": 7,
        "offered": true
    });
    let fs = rig.fs.copy();
    fs.put(
        &folder.join(DRAFTS),
        &serde_json::to_vec(&[&newer]).unwrap(),
        None,
    );
    let (rig, handle) = reopen(fs, &folder);
    assert!(unreadable_notices(&handle).is_empty());
    draft(&handle, DraftField::Words, &[verse(1, 2)], "typed here");
    handle.wait_idle();
    let bytes = rig.fs.peek(&folder.join(DRAFTS)).unwrap();
    let entries: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert!(entries.contains(&newer), "{entries:#?}");
    assert!(entries.iter().any(|e| e["text"] == "typed here"));
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}
