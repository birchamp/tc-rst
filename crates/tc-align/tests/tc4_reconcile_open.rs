//! A verse opens with the words of its current text and says when its alignment needs
//! review: tC4's `test/align-reconcile-open.test.ts` (#787, at tC4 main 9ad4d5c) ported on
//! the conformance sample, with the one review rule of the rail, the notice and the reflow
//! (#796), the verse that holds its alignment markup (#794) and the cards fitted to the
//! pinned original-language verse (#790).
//!
//! The rebuild saves a carry-over that loses nothing when the person leaves the verse
//! (APP-DESIGN AD19, a declared difference): tC4 writes nothing at open. Every other case
//! here writes nothing at open, as in tC4.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use tc_align::usfm::weave_verse;
use tc_align::{
    Aligner, Alignment, AlignmentValue, Basis, NoReflow, ResourceRef, SourceVerse, SourceWord,
    TargetVerse, TargetWord, Unavailable, VerseState, needs_review, reflow, verse_status,
};
use tc_journal::ids::VerseNo;
use unicode_normalization::UnicodeNormalization;

/// The sample's Greek words of TIT 1:1, from its record, as tC4's test builds `ORIG`.
fn orig() -> SourceVerse {
    let (stored, _) = sample_tit_1_1();
    SourceVerse::new(
        stored
            .alignments
            .iter()
            .flat_map(|g| g.top_words.clone())
            .collect(),
    )
}

/// T1, the sample's TIT 1:1, and T2, the same with "siervo" changed to "esclavo".
fn texts() -> (String, String) {
    let (_, t1) = sample_tit_1_1();
    let t2 = t1.replace("siervo", "esclavo");
    (t1, t2)
}

fn open_with(stored: Option<&AlignmentValue>, text: &str, source: &SourceVerse) -> Aligner {
    Aligner::open(stored, None, TargetVerse::new(text), source, basis(1)).unwrap()
}

fn open(stored: Option<&AlignmentValue>, text: &str) -> Aligner {
    open_with(stored, text, &orig())
}

fn banked(aligner: &Aligner, text: &str) -> TargetWord {
    aligner
        .value()
        .word_bank
        .iter()
        .find(|w| w.word == text)
        .cloned()
        .unwrap_or_else(|| panic!("{text} is not banked"))
}

fn bank_words(value: &AlignmentValue) -> Vec<&str> {
    value.word_bank.iter().map(|w| w.word.as_str()).collect()
}

/// `word|occurrence|occurrences` of every word, sorted.
fn multiset<'a>(words: impl IntoIterator<Item = &'a TargetWord>) -> Vec<String> {
    let mut out: Vec<String> = words
        .into_iter()
        .map(|w| format!("{}|{}|{}", w.word, w.occurrence, w.occurrences))
        .collect();
    out.sort();
    out
}

/// The issue's steps: open a never-saved 1:1, place "Pablo", then unplace it. The value
/// holds one card per Greek word, none holding a word.
fn unplaced_pablo() -> AlignmentValue {
    let (t1, _) = texts();
    let mut aligner = open(None, &t1);
    let pablo = banked(&aligner, "Pablo");
    aligner.place(0, &pablo).unwrap();
    aligner.unplace(0, &pablo).unwrap();
    let value = aligner.into_value();
    assert!(!value.alignments.is_empty());
    assert_eq!(value.placed(), 0);
    value
}

/// "Pablo" placed on Παῦλος, saved against T1.
fn placed_record() -> AlignmentValue {
    let (t1, _) = texts();
    let mut aligner = open(None, &t1);
    aligner.place(0, &banked(&aligner, "Pablo")).unwrap();
    aligner.into_value()
}

// ---- #787 a stored record is reconciled to the text when the verse opens ----

#[test]
fn a_stale_value_opens_with_the_draft_changed_notice() {
    // B1, the issue steps: place and unplace Pablo, change siervo to esclavo, reopen.
    let (_, t2) = texts();
    let aligner = open(Some(&unplaced_pablo()), &t2);
    assert!(bank_words(aligner.value()).contains(&"esclavo"));
    assert!(!bank_words(aligner.value()).contains(&"siervo"));
    assert_eq!(aligner.value().word_bank, TargetVerse::new(&t2).words());
    assert!(aligner.is_stale());
    // B4: opening writes nothing.
    assert!(!aligner.is_changed());
}

#[test]
fn mark_valid_with_nothing_placed_then_a_text_edit_opens_with_the_notice() {
    // B2.
    let (t1, t2) = texts();
    let mut aligner = open(None, &t1);
    aligner.mark_valid().unwrap();
    let done = aligner.into_value();
    assert!(done.done);
    let aligner = open(Some(&done), &t2);
    assert!(bank_words(aligner.value()).contains(&"esclavo"));
    assert!(!bank_words(aligner.value()).contains(&"siervo"));
    assert!(aligner.is_stale());
    assert!(!aligner.is_changed());
}

#[test]
fn a_record_whose_hash_is_current_but_whose_words_are_not_opens_with_the_notice_and_the_right_words()
 {
    // B3: the hash check alone passes.
    let (_, t2) = texts();
    let mut damaged = unplaced_pablo();
    damaged.target_verse_md5 = TargetVerse::new(&t2).md5();
    assert!(!damaged.is_stale_against(&TargetVerse::new(&t2)));
    assert!(bank_words(&damaged).contains(&"siervo"));
    let aligner = open(Some(&damaged), &t2);
    assert!(!bank_words(aligner.value()).contains(&"siervo"));
    assert!(bank_words(aligner.value()).contains(&"esclavo"));
    assert!(aligner.is_stale());
}

#[test]
fn the_links_whose_word_is_still_in_the_text_are_kept_and_the_others_dropped() {
    let (t1, t2) = texts();
    let mut first = open(None, &t1);
    first.place(0, &banked(&first, "Pablo")).unwrap();
    first.place(1, &banked(&first, "siervo")).unwrap();
    let aligner = open(Some(&first.into_value()), &t2);
    let value = aligner.value();
    assert_eq!(multiset(&value.alignments[0].bottom_words), ["Pablo|1|1"]);
    assert!(value.alignments[1].bottom_words.is_empty());
    assert_eq!(
        multiset(value.all_target_words()),
        multiset(TargetVerse::new(&t2).words())
    );
    assert_eq!(aligner.dropped().len(), 1);
    assert_eq!(aligner.dropped()[0].word.word, "siervo");
}

#[test]
fn one_edit_on_a_reconciled_verse_saves_exactly_the_verse_words_and_the_export_weaves_it() {
    // B5.
    let (_, t2) = texts();
    let mut aligner = open(Some(&unplaced_pablo()), &t2);
    aligner.place(0, &banked(&aligner, "Pablo")).unwrap();
    assert!(aligner.is_changed());
    assert!(
        aligner.is_stale(),
        "an edit keeps the notice until Mark valid (Q37)"
    );
    let after = aligner.value();
    assert_eq!(
        multiset(after.all_target_words()),
        multiset(TargetVerse::new(&t2).words())
    );
    assert_eq!(after.target_verse_md5, TargetVerse::new(&t2).md5());
    let woven = weave_verse(&t2, after).unwrap();
    assert!(woven.contains("\\zaln-s") && woven.contains("esclavo"));

    // Control: the same edit on the stored value as it was keeps "siervo", so it needs
    // review, and the export does not weave it (tC4's drops the alignment).
    let mut stored = unplaced_pablo();
    let pablo = stored
        .word_bank
        .iter()
        .find(|w| w.word == "Pablo")
        .cloned()
        .unwrap();
    stored.link(0, &pablo).unwrap();
    stored.settle(&TargetVerse::new(&t2));
    assert_ne!(
        multiset(stored.all_target_words()),
        multiset(TargetVerse::new(&t2).words())
    );
    let verse = TargetVerse::new(&t2);
    assert!(needs_review(
        &stored,
        &basis(1).aligned_run(&verse),
        &verse,
        None
    ));
    assert!(!needs_review(
        after,
        &basis(1).aligned_run(&verse),
        &verse,
        None
    ));
}

#[test]
fn a_record_that_matches_its_text_opens_as_it_is() {
    // B6: every verse of the conformance sample, against its own Greek words.
    let file = sample_alignments();
    let mut opened = 0;
    for (chapter, verses) in file["chapters"].as_object().unwrap() {
        for (verse, record) in verses.as_object().unwrap() {
            let at = format!("{chapter}:{verse}");
            let stored = tc_align::record::from_record(record, basis(1)).unwrap();
            let source = SourceVerse::new(
                stored
                    .alignments
                    .iter()
                    .flat_map(|g| g.top_words.clone())
                    .collect(),
            );
            let aligner = open_with(Some(&stored), &verse_body(SAMPLE_TIT, &at), &source);
            let pairs = |v: &AlignmentValue| {
                v.alignments
                    .iter()
                    .map(|g| {
                        let top: Vec<_> = g.top_words.iter().map(|w| w.word.clone()).collect();
                        (top, multiset(&g.bottom_words))
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(pairs(aligner.value()), pairs(&stored), "{at}");
            assert_eq!(
                multiset(aligner.value().all_target_words()),
                multiset(stored.all_target_words()),
                "{at}"
            );
            assert!(!aligner.is_stale(), "{at}");
            assert!(!aligner.is_changed(), "{at}");
            opened += 1;
        }
    }
    assert!(opened > 0);
}

#[test]
fn a_verse_that_was_never_saved_opens_with_its_bank_in_verse_order() {
    // B7: every drafted verse of the sample Titus.
    let drafted: Vec<_> = verses(SAMPLE_TIT)
        .into_iter()
        .filter(|(_, body)| !TargetVerse::new(body).words().is_empty() && !body.contains("___"))
        .collect();
    assert!(drafted.len() > 1);
    for (at, body) in &drafted {
        let aligner = open_with(None, body, &orig());
        assert_eq!(
            aligner.value().word_bank,
            TargetVerse::new(body).words(),
            "{at}"
        );
    }
    // The sample Jonah span, against the real Hebrew.
    let jonah = "fixtures/tc4/sample-burrito/ingredients/JON.usfm";
    let span: Vec<_> = verses(jonah)
        .into_iter()
        .filter(|(_, body)| !body.contains("___"))
        .collect();
    assert_eq!(
        span.iter().map(|(at, _)| at.as_str()).collect::<Vec<_>>(),
        ["2:9-10"]
    );
    let basis = Basis::run(
        VerseNo::new(9).unwrap(),
        VerseNo::new(10).unwrap(),
        "dcs::unfoldingWord/hbo_uhb@v2.1.30".into(),
        None,
    );
    let body = &span[0].1;
    let aligner = Aligner::open(
        None,
        None,
        TargetVerse::new(body),
        &source_verse(UHB, "2:9-10"),
        basis,
    )
    .unwrap();
    assert_eq!(aligner.value().word_bank, TargetVerse::new(body).words());
}

#[test]
fn a_new_word_a_removed_word_a_dropped_link_and_a_count_change_each_need_review() {
    let (t1, t2) = texts();
    let (stored, _) = sample_tit_1_1();
    let current = open(Some(&stored), &t1);
    assert!(!current.is_stale());
    assert!(!open(Some(&placed_record()), &t1).is_stale());
    let saulo = t1.replace("Pablo", "Saulo");
    // A second "Pablo": the first is still there, but its count is now 2.
    let twice = format!("Pablo {t1}");
    for text in [&t2, &saulo, &twice] {
        assert!(open(Some(&placed_record()), text).is_stale(), "{text}");
        // The words alone, with a hash forged to match the new text.
        let mut forged = placed_record();
        forged.target_verse_md5 = TargetVerse::new(text).md5();
        assert!(open(Some(&forged), text).is_stale(), "{text}");
    }
}

#[test]
fn the_carry_over_changes_only_the_words() {
    let (_, t2) = texts();
    let mut flagged = placed_record();
    flagged.marked_invalid = true;
    flagged.done = true;
    let aligner = open(Some(&flagged), &t2);
    let tops = |v: &AlignmentValue| {
        v.alignments
            .iter()
            .map(|g| g.top_words.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(tops(aligner.value()), tops(&flagged));
    assert_eq!(
        multiset(aligner.value().all_target_words()),
        multiset(TargetVerse::new(&t2).words())
    );
    assert!(aligner.is_stale());
    assert!(
        !aligner.is_changed(),
        "a value marked invalid is not saved unasked"
    );
}

#[test]
fn a_record_with_a_placed_word_and_an_empty_bank_reflows() {
    // tC4 reads a record with no `wordBank` key as one with an empty bank.
    let (_, t2) = texts();
    let mut no_bank = placed_record();
    no_bank.word_bank.clear();
    let out = reflow(&no_bank, &TargetVerse::new(&t2)).unwrap();
    assert!(!out.value.marked_invalid);
    assert_eq!(
        multiset(&out.value.alignments[0].bottom_words),
        ["Pablo|1|1"]
    );
    let rest: Vec<_> = TargetVerse::new(&t2)
        .words()
        .iter()
        .filter(|w| w.word != "Pablo")
        .cloned()
        .collect();
    assert_eq!(out.value.word_bank, rest);
}

// ---- #796 one review rule for the rail, the open verse and the reflow ----

/// "Pablo, siervo de Dios" fully aligned and marked valid against `text`'s hash.
fn valid_4_of_4(text: &str) -> AlignmentValue {
    let source = SourceVerse::new(orig().words()[..3].to_vec());
    let mut aligner = open_with(None, "Pablo, siervo de Dios", &source);
    aligner.place(0, &banked(&aligner, "Pablo")).unwrap();
    aligner.place(1, &banked(&aligner, "siervo")).unwrap();
    aligner.place(2, &banked(&aligner, "de")).unwrap();
    aligner.place(2, &banked(&aligner, "Dios")).unwrap();
    aligner.mark_valid().unwrap();
    let mut value = aligner.into_value();
    value.target_verse_md5 = TargetVerse::new(text).md5();
    value
}

#[test]
fn a_record_holding_a_word_the_verse_lacks_is_invalid_on_the_rail() {
    let esclavo = TargetVerse::new("Pablo, esclavo de Dios");
    let run = basis(1).aligned_run(&esclavo);
    let siervo = valid_4_of_4("Pablo, esclavo de Dios");
    assert!(siervo.is_valid(&run), "the hash alone vouches for it");
    let status = verse_status(Some(&siervo), Some((&run, &esclavo)), None);
    assert_eq!(
        (status.state, status.placed, status.total),
        (VerseState::Invalid, 4, 4)
    );
    // The control: the same record against its own verse is valid.
    let own = TargetVerse::new("Pablo, siervo de Dios");
    let fine = valid_4_of_4("Pablo, siervo de Dios");
    let run = basis(1).aligned_run(&own);
    assert_eq!(
        verse_status(Some(&fine), Some((&run, &own)), None).state,
        VerseState::Valid
    );
    // The store's reflow carries it to the verse's words.
    let carried = reflow(&siervo, &esclavo).unwrap();
    assert_eq!(carried.dropped.len(), 1);
    assert_eq!(carried.dropped[0].word.word, "siervo");
}

#[test]
fn after_a_source_upgrade_the_open_verse_shows_the_notice() {
    let (t1, _) = texts();
    let pin = |sha: &str| ResourceRef {
        repo_path: "git.door43.org/unfoldingWord/el-x-koine_ugnt".into(),
        sha: sha.parse().unwrap(),
    };
    let old = pin("0123456789abcdef0123456789abcdef01234567");
    let new = pin("89abcdef0123456789abcdef0123456789abcdef");
    let mut stored = placed_record();
    stored.source = Some(old.clone());
    let at = |source: ResourceRef| {
        let mut basis = basis(1);
        basis.source = Some(source);
        Aligner::open(Some(&stored), None, TargetVerse::new(&t1), &orig(), basis).unwrap()
    };
    assert!(!at(old).is_stale());
    let mut upgraded = at(new.clone());
    assert!(upgraded.is_stale());
    assert_eq!(upgraded.value(), &stored, "the value opens as it is");
    // The rail says the same.
    let verse = TargetVerse::new(&t1);
    let mut basis = basis(1);
    basis.source = Some(new);
    let run = basis.aligned_run(&verse);
    assert_eq!(
        verse_status(Some(&stored), Some((&run, &verse)), None).state,
        VerseState::Invalid
    );
    // Mark valid clears the notice.
    upgraded.mark_valid().unwrap();
    assert!(!upgraded.is_stale());
}

#[test]
fn a_verse_edit_of_a_word_the_hash_leaves_out_reflows_its_alignment() {
    // The hash leaves out a `\qs` written over lines, so "Selah" to "Pause" keeps it.
    let before = "Pablo, siervo de Dios\n\\qs\nSelah\n\\qs*\n";
    let after = TargetVerse::new("Pablo, siervo de Dios\n\\qs\nPause\n\\qs*\n");
    let mut aligner = open_with(None, before, &orig());
    aligner.place(0, &banked(&aligner, "Selah")).unwrap();
    let stored = aligner.into_value();
    assert!(!stored.is_stale_against(&after));
    let run = basis(1).aligned_run(&after);
    assert_eq!(
        verse_status(Some(&stored), Some((&run, &after)), None).state,
        VerseState::Invalid
    );
    let carried = reflow(&stored, &after).unwrap();
    assert_eq!(carried.dropped.len(), 1);
    assert!(bank_words(&carried.value).contains(&"Pause"));
}

/// The links on the card of `greek`'s `occurrence`.
fn links_on(aligner: &Aligner, greek: &str, occurrence: u32) -> Vec<String> {
    let card = aligner
        .value()
        .alignments
        .iter()
        .find(|g| {
            g.top_words
                .iter()
                .any(|w| w.word == greek && w.occurrence == occurrence)
        })
        .unwrap();
    show(&card.bottom_words)
}

#[test]
fn a_link_to_a_retyped_copy_of_its_word_opens_dropped_and_is_not_saved_on_leaving() {
    // A sync, a kept version or an outside edit: the first Dios deleted and another typed
    // earlier, so the verse still holds two.
    let (stored, t1) = sample_tit_1_1();
    let moved = t1.replacen("Pablo, siervo de Dios", "Pablo, Dios, siervo de", 1);
    let aligner = open(Some(&stored), &moved);
    assert!(links_on(&aligner, "Θεοῦ", 1).is_empty(), "not the new Dios");
    assert!(links_on(&aligner, "Θεοῦ", 2).is_empty());
    let dropped: Vec<_> = aligner.dropped().iter().map(|d| d.word.clone()).collect();
    assert_eq!(show(&dropped), ["Dios/1/2", "Dios/2/2"]);
    assert!(aligner.is_stale());
    assert!(
        !aligner.is_changed(),
        "an unclear carry-over waits for the translator"
    );
    // With the text the record was made against, each link follows its own word.
    let aligner = Aligner::open(
        Some(&stored),
        Some(&TargetVerse::new(&t1)),
        TargetVerse::new(&moved),
        &orig(),
        basis(1),
    )
    .unwrap();
    assert!(links_on(&aligner, "Θεοῦ", 1).is_empty());
    assert_eq!(links_on(&aligner, "Θεοῦ", 2), ["Dios/2/2"]);
    assert_eq!(aligner.dropped().len(), 1);
    assert!(!aligner.is_changed());
}

// ---- #794 a verse that holds its alignment markup ----

#[test]
fn a_verse_that_holds_alignment_markup_does_not_open() {
    let aligned = verse_body(ULT, "1:1");
    assert!(aligned.contains("\\zaln-s"));
    let verse = TargetVerse::new(&aligned);
    assert!(verse.holds_alignment_markup());
    assert!(!verse.words().is_empty(), "it has words to show");
    let refused = |source: &SourceVerse| {
        Aligner::open(None, None, TargetVerse::new(&aligned), source, basis(1)).unwrap_err()
    };
    assert_eq!(
        refused(&source_verse(UGNT, "1:1")),
        Unavailable::AlignedInText
    );
    // It outranks the other reasons.
    assert_eq!(refused(&SourceVerse::default()), Unavailable::AlignedInText);
    // No reflow writes over it.
    assert_eq!(
        reflow(&placed_record(), &verse),
        Err(NoReflow::AlignedInText)
    );
    // A verse that only mentions the marker's name in a word is not one.
    assert!(!TargetVerse::new("a \\zaln-sx b").holds_alignment_markup());
}

// ---- #790 the cards are the pinned original-language words ----

fn pairs_of(value: &AlignmentValue) -> Vec<(Vec<String>, Vec<String>)> {
    value
        .alignments
        .iter()
        .map(|g| {
            (
                g.top_words.iter().map(|w| w.word.clone()).collect(),
                g.bottom_words.iter().map(|w| w.word.clone()).collect(),
            )
        })
        .collect()
}

#[test]
fn the_sample_opens_with_all_17_ugnt_words() {
    let (stored, t1) = sample_tit_1_1();
    let ugnt = source_verse(UGNT, "1:1");
    assert_eq!(
        stored
            .alignments
            .iter()
            .map(|g| g.top_words.len())
            .sum::<usize>(),
        11
    );
    assert_eq!(ugnt.words().len(), 17);
    let aligner = open_with(Some(&stored), &t1, &ugnt);
    let cards = &aligner.value().alignments;
    // One card per UGNT word, in UGNT order: the record's ten, then the six it lacks.
    let order: Vec<_> = cards
        .iter()
        .flat_map(|g| &g.top_words)
        .map(|w| &w.word)
        .collect();
    let expected: Vec<_> = ugnt.words().iter().map(|w| &w.word).collect();
    assert_eq!(order, expected);
    assert_eq!(&pairs_of(aligner.value())[..10], &pairs_of(&stored)[..]);
    assert!(aligner.dropped().is_empty());
}

#[test]
fn opening_a_repaired_verse_writes_nothing() {
    let (stored, t1) = sample_tit_1_1();
    let mut aligner = open_with(Some(&stored), &t1, &source_verse(UGNT, "1:1"));
    assert!(!aligner.is_changed());
    assert!(!aligner.is_stale());
    // The fit rides on the first save.
    aligner.place(10, &banked(&aligner, "y")).unwrap();
    assert!(aligner.is_changed());
    // Ten cards, Ἰησοῦ Χριστοῦ one of them, and six more.
    assert_eq!(aligner.value().alignments.len(), 16);
}

#[test]
fn a_link_to_a_source_word_the_pin_lacks_is_dropped_and_shown() {
    let (mut stored, t1) = sample_tit_1_1();
    stored.alignments[0].top_words[0].word = "Σαῦλος".into();
    let aligner = open_with(Some(&stored), &t1, &source_verse(UGNT, "1:1"));
    assert_eq!(aligner.dropped().len(), 1);
    assert_eq!(aligner.dropped()[0].top_words[0].word, "Σαῦλος");
    assert_eq!(aligner.dropped()[0].word.word, "Pablo");
    let value = aligner.value();
    assert!(bank_words(value).contains(&"Pablo"));
    assert!(
        value
            .alignments
            .iter()
            .all(|g| g.top_words.iter().all(|w| w.word != "Σαῦλος"))
    );
    assert_eq!(value.alignments[0].top_words[0].word, "Παῦλος");
    assert!(value.alignments[0].bottom_words.is_empty());
    assert!(
        !aligner.is_changed(),
        "a link taken off waits for the translator"
    );
    assert_eq!(value.alignments.len(), 16);
}

#[test]
fn a_done_record_with_a_source_word_the_pin_lacks_is_invalid_on_the_rail_and_opens_stale() {
    // An imported record names no pin, so only the pinned words can tell.
    let (mut stored, t1) = sample_tit_1_1();
    assert_eq!(stored.source, None);
    stored.done = true;
    let ugnt = source_verse(UGNT, "1:1");
    let verse = TargetVerse::new(&t1);
    let run = basis(1).aligned_run(&verse);
    let rail =
        |value: &AlignmentValue| verse_status(Some(value), Some((&run, &verse)), Some(&ugnt));
    assert_eq!(rail(&stored).state, VerseState::Valid);
    assert!(!open_with(Some(&stored), &t1, &ugnt).is_stale());
    stored.alignments[0].top_words[0].word = "Σαῦλος".into();
    assert_eq!(rail(&stored).state, VerseState::Invalid);
    assert!(needs_review(&stored, &run, &verse, Some(&ugnt)));
    let aligner = open_with(Some(&stored), &t1, &ugnt);
    assert_eq!(aligner.dropped().len(), 1);
    assert!(
        aligner.is_stale(),
        "the open takes a link off: the notice shows"
    );
}

#[test]
fn an_imported_nfc_hebrew_record_keeps_its_links() {
    // tC4 4.0 stores Hebrew source words in NFC; the pinned UHB writes them otherwise.
    let source = source_verse(UHB, "1:1");
    let nfc = |w: &SourceWord| w.word.nfc().collect::<String>();
    assert!(source.words().iter().any(|w| nfc(w) != w.word));
    let text = "a b c d e f g h i j k l m n o p q r s t";
    let mut aligner = open_with(None, text, &source);
    let cards = aligner.value().alignments.len();
    for card in 0..cards {
        let word = aligner.value().word_bank[0].clone();
        aligner.place(card, &word).unwrap();
    }
    let mut stored = aligner.into_value();
    for word in stored
        .alignments
        .iter_mut()
        .flat_map(|g| g.top_words.iter_mut())
    {
        word.word = word.word.nfc().collect();
    }
    let placed = stored.placed();
    let aligner = open_with(Some(&stored), text, &source);
    assert!(aligner.dropped().is_empty());
    assert_eq!(aligner.value().placed(), placed);
    assert_eq!(aligner.value().alignments.len(), source.words().len());
    assert_eq!(aligner.value(), &stored, "kept as written");
}
