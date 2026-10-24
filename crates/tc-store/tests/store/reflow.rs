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
fn a_lossy_edit_drops_only_the_words_the_verse_lost() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    paul(&handle);
    save_verse(&handle, 1, 1, "Paul, of God").unwrap();
    let after = alignment(&handle);
    assert_eq!(placed(&after), ["Paul#1", "of#1", "God#1"]);
    assert!(bank(&after).is_empty());
    handle.close().unwrap();
}

#[test]
fn a_right_to_left_verse_reflows_by_its_words() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    aligned(
        &handle,
        "פולוס עבד האלהים",
        &[
            ("Παῦλος", vec![target("פולוס", 1, 1)]),
            ("δοῦλος", vec![target("עבד", 1, 1)]),
        ],
    );
    save_verse(&handle, 1, 1, "פולוס עבד האלהים ושליח").unwrap();
    let after = alignment(&handle);
    assert_eq!(placed(&after), ["פולוס#1", "עבד#1"]);
    assert_eq!(bank(&after), ["האלהים", "ושליח"]);
    handle.close().unwrap();
}

#[test]
fn repeated_words_keep_their_occurrences() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    aligned(
        &handle,
        "God and God",
        &[
            ("Θεοῦ", vec![target("God", 1, 2)]),
            ("καὶ", vec![target("and", 1, 1)]),
        ],
    );
    // A new first "God": the old first occurrence is now the second.
    save_verse(&handle, 1, 1, "God, God and God").unwrap();
    let after = alignment(&handle);
    for w in after.alignments.iter().flat_map(|g| &g.bottom_words) {
        if w.word == "God" {
            assert_eq!(w.occurrences, 3);
        }
    }
    assert_eq!(
        after
            .alignments
            .iter()
            .map(|g| g.bottom_words.len())
            .sum::<usize>()
            + after.word_bank.len(),
        4,
        "every word of the verse is on a card or in the bank once"
    );
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
