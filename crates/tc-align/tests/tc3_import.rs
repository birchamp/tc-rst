//! tC4 records of a project imported from translationCore 3 (`import/tc3.ts`): tC3 wrote
//! `"type": "bottomWord"` on every banked word, and tC4 keeps the bank as tC3 wrote it.
//! The records are made from tC4's own tC3 fixture, `cfm_fbt_tit_book.zip`, as tC4's
//! importer makes them, then read, written back and opened.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use tc_align::Alignment;

use common::*;
use serde_json::Value;
use tc_align::Aligner;

fn typed_words(list: &Value) -> usize {
    list.as_array()
        .unwrap()
        .iter()
        .filter(|w| w["type"] == "bottomWord")
        .count()
}

#[test]
fn every_record_of_a_tc3_project_reads_and_writes_back_with_tc3s_word_type() {
    let imported = imported();
    assert_eq!(imported.len(), 46);
    let (mut with_type, mut typed_total) = (0, 0);
    for verse in &imported {
        let at = &verse.reference;
        let value = tc4_record(&verse.record).unwrap_or_else(|e| panic!("{at}: {e}"));
        let typed = typed_words(&verse.record["wordBank"]);
        assert_eq!(
            value.all_target_words().filter(|w| typed_word(w)).count(),
            typed,
            "{at}"
        );
        if typed > 0 {
            with_type += 1;
        }
        typed_total += typed;
    }
    assert_eq!((with_type, typed_total), (46, 1108));
}

#[test]
fn a_tc3_record_opens_as_it_is_and_tc3s_word_type_goes_with_the_word() {
    let imported = imported();
    let verse = imported.iter().find(|v| v.reference == "1:1").unwrap();
    let stored = tc4_record(&verse.record).unwrap();
    assert!(typed_words(&verse.record["wordBank"]) > 0);
    let mut aligner = Aligner::open(
        Some(&stored),
        None,
        verse.verse.clone(),
        &source_verse(UGNT, "1:1"),
        basis(1),
    )
    .unwrap();
    assert_eq!(aligner.value(), &stored);
    assert!(!aligner.is_stale() && !aligner.is_changed());

    // Placed, the word keeps its tC3 type on the card, as tC4's `linkWord` moves the
    // whole bank object; sent back, it keeps it in the bank.
    let word = aligner.value().word_bank[0].clone();
    aligner.place(0, &word).unwrap();
    let placed = aligner.value().alignments[0].bottom_words.last().unwrap();
    assert_eq!(placed.word, word.word);
    assert!(typed_word(placed));
    assert_eq!(
        aligner
            .value()
            .word_bank
            .iter()
            .filter(|w| typed_word(w))
            .count()
            + 1,
        typed_words(&verse.record["wordBank"])
    );
    aligner.unplace(0, &word).unwrap();
    assert_eq!(aligner.value().word_bank, stored.word_bank);
}

/// A word that carries tC3's `"type": "bottomWord"`.
fn typed_word(word: &tc_align::TargetWord) -> bool {
    word.extra.get("type") == Some(&tc_journal::json::Json::from("bottomWord"))
}
