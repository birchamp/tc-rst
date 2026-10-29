//! A verse save writes its verse's reflowed alignment in the same batch (APP-DESIGN R25,
//! `docs/STORAGE.md` §5.1): `tc_align::reflow_edit` from the old text on the alignment the
//! actor holds, with its heads as parents, so the aligner never shows words the verse no
//! longer has.

use crate::common::*;
use crate::crash::{Outcome, kill_everywhere, outcome_of};
use tc_journal::command::{AlignmentEdit, Command};
use tc_journal::record::{AlignmentGroup, AlignmentValue, SourceWord, TargetWord};
use tc_journal::{ChapterNo, Record, RecordKey, VerseNo};
use tc_store::{OpenOptions, ProjectHandle};

fn alignment_key(c: u16, v: u16) -> RecordKey {
    RecordKey::Alignment(TIT, ChapterNo::new(c).unwrap(), VerseNo::new(v).unwrap())
}

fn source(word: &str) -> SourceWord {
    SourceWord {
        word: word.into(),
        strong: None,
        lemma: Some(word.into()),
        morph: None,
        occurrence: 1,
        occurrences: 1,
        extra: Default::default(),
    }
}

fn target(word: &str, occurrence: u32, occurrences: u32) -> TargetWord {
    TargetWord {
        word: word.into(),
        occurrence,
        occurrences,
        extra: Default::default(),
    }
}

/// Saves `text` at 1:1 and aligns it: each `(source, targets)` pair one card.
fn aligned(handle: &ProjectHandle, text: &str, cards: &[(&str, Vec<TargetWord>)]) {
    save_verse(handle, 1, 1, text).unwrap();
    let shown = handle.mint(&alignment_key(1, 1), None).unwrap();
    let verse_shown = handle.mint(&verse(1, 1), None).unwrap();
    let value = AlignmentEdit {
        alignments: cards
            .iter()
            .map(|(s, t)| AlignmentGroup {
                top_words: vec![source(s)],
                bottom_words: t.clone(),
                extra: Default::default(),
            })
            .collect(),
        word_bank: Vec::new(),
        done: true,
    };
    handle
        .submit(Command::SaveAlignment {
            shown,
            verse_shown,
            value,
        })
        .wait()
        .unwrap();
}

fn alignment(handle: &ProjectHandle) -> AlignmentValue {
    match handle.snapshot().fold().projected(&alignment_key(1, 1)) {
        Some(Record::Alignment { value, .. }) => value,
        other => panic!("{other:?}"),
    }
}

fn placed(value: &AlignmentValue) -> Vec<String> {
    value
        .alignments
        .iter()
        .flat_map(|g| {
            g.bottom_words
                .iter()
                .map(|w| format!("{}#{}", w.word, w.occurrence))
        })
        .collect()
}

fn bank(value: &AlignmentValue) -> Vec<String> {
    value.word_bank.iter().map(|w| w.word.clone()).collect()
}

fn paul(handle: &ProjectHandle) {
    aligned(
        handle,
        "Paul, a servant of God",
        &[
            ("Παῦλος", vec![target("Paul", 1, 1)]),
            ("δοῦλος", vec![target("a", 1, 1), target("servant", 1, 1)]),
            ("Θεοῦ", vec![target("of", 1, 1), target("God", 1, 1)]),
        ],
    );
}

#[test]
fn a_lossless_edit_keeps_every_card_and_banks_the_new_words() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    paul(&handle);
    let before = alignment(&handle);
    let heads_before = handle.snapshot().fold().heads(&alignment_key(1, 1));
    let reply = save_verse(&handle, 1, 1, "Paul, a servant of God and an apostle").unwrap();
    assert_eq!(
        reply.changes.len(),
        2,
        "the verse and its alignment, one batch"
    );
    let batch = handle
        .snapshot()
        .fold()
        .change(&reply.changes[0])
        .unwrap()
        .batch;
    assert_eq!(
        handle
            .snapshot()
            .fold()
            .change(&reply.changes[1])
            .unwrap()
            .batch,
        batch
    );
    let after = alignment(&handle);
    assert_eq!(placed(&after), placed(&before));
    assert_eq!(bank(&after), ["and", "an", "apostle"]);
    assert_ne!(after.target_verse_md5, before.target_verse_md5);
    // The new alignment continues the one the actor held.
    let alignment_change = reply
        .changes
        .iter()
        .map(|id| handle.snapshot().fold().change(id).unwrap())
        .find(|c| c.key == alignment_key(1, 1))
        .unwrap();
    assert_eq!(alignment_change.parents, heads_before);
    assert_eq!(
        handle.snapshot().fold().heads(&alignment_key(1, 1)).len(),
        1
    );
    handle.close().unwrap();
}

/// tC4 #794: a verse whose text holds its alignment inline (`\zaln-s`) is not reflowed: the
/// save writes the verse alone, and the alignment record stays as it was.
#[test]
fn a_verse_edit_of_an_inline_aligned_verse_writes_no_alignment() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    paul(&handle);
    let before = alignment(&handle);
    let reply = save_verse(
        &handle,
        1,
        1,
        "\\zaln-s |x-strong=\"G39720\" x-lemma=\"Παῦλος\" x-morph=\"Gr,N,,,,,NMS,\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"Παῦλος\"\\*\\w Paul|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\\zaln-e\\*, a servant of God and an apostle",
    )
    .unwrap();
    let keys: Vec<RecordKey> = reply
        .changes
        .iter()
        .map(|id| handle.snapshot().fold().change(id).unwrap().key)
        .collect();
    assert_eq!(keys, [verse(1, 1)]);
    assert_eq!(alignment(&handle), before);
    handle.close().unwrap();
}

#[test]
fn a_link_follows_its_own_copy_of_a_repeated_word() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    aligned(
        &handle,
        "servant of God and of God",
        &[
            ("Θεοῦ", vec![target("God", 1, 2)]),
            ("Κυρίου", vec![target("God", 2, 2)]),
        ],
    );
    // The first "God" deleted and another typed earlier: the verse still holds two.
    save_verse(&handle, 1, 1, "God, servant of and of God").unwrap();
    let after = alignment(&handle);
    assert_eq!(
        placed(&after),
        ["God#2"],
        "Θεοῦ's link does not move to the new God"
    );
    assert_eq!(after.alignments[1].bottom_words[0].occurrences, 2);
    assert!(bank(&after).contains(&"God".to_owned()));
    handle.close().unwrap();
}

#[test]
fn crash_a_verse_save_and_its_reflow_are_one_batch() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    paul(&handle);
    handle.wait_idle();
    handle.close().unwrap();
    let base = rig.fs.crash(false);
    let new = "Paul, a servant of God and an apostle";
    kill_everywhere(
        &base,
        &folder,
        &|h| outcome_of(save_verse(h, 1, 1, new)),
        &|h, outcome| {
            let saved = words(h, 1, 1).as_deref() == Some(new);
            if outcome == Outcome::Saved {
                assert!(saved);
            }
            let reflowed = !alignment(h).word_bank.is_empty();
            assert_eq!(
                saved, reflowed,
                "the verse and its alignment travel together"
            );
        },
    );
}

/// TIT 1:1 as "Dios Dios", each copy on its own card, before tC4 #820's edit.
fn dios_dios(handle: &ProjectHandle) {
    aligned(
        handle,
        "Dios Dios",
        &[
            ("θεοῦ", vec![target("Dios", 1, 2)]),
            ("θεὸς", vec![target("Dios", 2, 2)]),
        ],
    );
}

/// A verse's chapter and verse, and the (card, occurrence) of each link listed for it.
type Listed = (u16, u16, Vec<(usize, u32)>);

/// What a save's reply lists.
fn listed(reply: &tc_store::StoreReply) -> Vec<Listed> {
    reply
        .dropped_links
        .iter()
        .map(|d| {
            assert_eq!(d.book, TIT);
            (
                d.chapter.get(),
                d.verse.get(),
                d.links
                    .iter()
                    .map(|l| (l.card, l.word.occurrence))
                    .collect(),
            )
        })
        .collect()
}

/// Q30 (a): a link a verse save cannot follow goes back to the bank, and the save's reply
/// lists it, so Align can say which links went.
#[test]
fn a_translate_save_that_drops_an_unclear_link_lists_it() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    dios_dios(&handle);
    let reply = save_verse(&handle, 1, 1, "Dios Pablo").unwrap();
    assert_eq!(
        listed(&reply),
        [(1, 1, vec![(0, 1), (1, 2)])],
        "INV-A-DROPPED-LINK-IS-LISTED: the save's reply lists the links it could not follow"
    );
    assert!(placed(&alignment(&handle)).is_empty());
    handle.close().unwrap();
}

/// The same through a section save (§5.2).
#[test]
fn a_section_save_that_drops_an_unclear_link_lists_it() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    dios_dios(&handle);
    let Some(Record::Verse {
        value:
            tc_journal::record::VerseValue::Text {
                marker,
                layout,
                last,
                from,
                ..
            },
        ..
    }) = handle.snapshot().fold().projected(&verse(1, 1))
    else {
        panic!("1:1 is text");
    };
    let reply = handle
        .submit(Command::SaveSection {
            book: TIT,
            shown: vec![(
                handle.mint(&verse(1, 1), None).unwrap(),
                Some(tc_journal::command::SectionValue::Verse(
                    tc_journal::record::VerseValue::Text {
                        marker,
                        words: "Dios Pablo".into(),
                        layout,
                        last,
                        from,
                    },
                )),
            )],
            layout: None,
        })
        .wait()
        .unwrap();
    assert_eq!(listed(&reply), [(1, 1, vec![(0, 1), (1, 2)])]);
    handle.close().unwrap();
}

/// A save from a screen a sync overtook leaves the alignment as it is: the teammate's
/// alignment, made for the teammate's text, is never reflowed from text it was not made for.
/// The verse stands as a conflict, and the alignment keeps the teammate's links.
#[test]
fn a_save_from_a_screen_a_sync_overtook_keeps_the_teammates_alignment() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    paul(&handle);
    let shown = handle.mint(&verse(1, 1), None).unwrap();
    let Some(Record::Verse { value, .. }) = handle.snapshot().fold().projected(&verse(1, 1)) else {
        panic!("1:1");
    };
    let mut theirs = alignment(&handle);
    let verse_heads = handle.snapshot().fold().heads(&verse(1, 1));
    let alignment_heads = handle.snapshot().fold().heads(&alignment_key(1, 1));
    handle.close().unwrap();
    // A teammate's text and the alignment their save reflowed to it.
    let mut text = value.clone();
    if let tc_journal::record::VerseValue::Text { words, .. } = &mut text {
        *words = "Paul, a slave of God".into();
    }
    theirs.alignments[1].bottom_words = vec![target("a", 1, 1)];
    theirs.word_bank = vec![target("slave", 1, 1)];
    crate::commands::teammate_batch(
        &rig.fs,
        &folder,
        tc_journal::DeviceId::local([8u8; 15]),
        1,
        vec![
            (
                Record::Verse {
                    book: TIT,
                    chapter: ChapterNo::new(1).unwrap(),
                    verse: VerseNo::new(1).unwrap(),
                    value: text,
                },
                verse_heads,
            ),
            (
                Record::Alignment {
                    book: TIT,
                    chapter: ChapterNo::new(1).unwrap(),
                    verse: VerseNo::new(1).unwrap(),
                    value: theirs.clone(),
                },
                alignment_heads,
            ),
        ],
    );
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let reply = handle
        .submit(Command::SaveVerse {
            shown,
            words: "Paul, a servant of God and an apostle".into(),
        })
        .wait()
        .unwrap();
    let keys: Vec<RecordKey> = reply
        .changes
        .iter()
        .map(|id| handle.snapshot().fold().change(id).unwrap().key)
        .collect();
    assert_eq!(keys, [verse(1, 1)], "no alignment change");
    assert!(reply.dropped_links.is_empty());
    assert_eq!(alignment(&handle), theirs);
    assert_eq!(handle.snapshot().fold().heads(&verse(1, 1)).len(), 2);
    handle.close().unwrap();
}
