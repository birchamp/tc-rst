//! tC4's card order tests, ported: `test/align-card-order.test.ts` (#754) at d310bf3. A
//! card keeps its target words in verse order automatically, as tC3 did; there is no
//! drop-position preview. Then the same rule through the aligner, on every edit and when a
//! verse opens, and a value stored in another order kept as it was until it is edited
//! (Q19).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;
use tc_align::Alignment;
use tc_align::bootstrap;

use common::*;
use tc_align::usfm::weave_verse;
use tc_align::{Aligner, AlignmentValue, SourceVerse, SourceWord, TargetVerse, TargetWord};

fn greek(text: &str, strong: &str, lemma: &str) -> SourceWord {
    SourceWord {
        word: text.to_owned(),
        strong: Some(strong.to_owned()),
        lemma: Some(lemma.to_owned()),
        morph: Some("Gr,N".to_owned()),
        occurrence: 1,
        occurrences: 1,
        extra: tc_journal::json::JsonMap::new(),
    }
}

fn orig() -> SourceVerse {
    SourceVerse::new(vec![
        greek("πίστιν", "G41020", "πίστις"),
        greek("ἐκλεκτῶν", "G15880", "ἐκλεκτός"),
    ])
}

const TEXT: &str = "según la fe de los escogidos y la verdad";

fn boot() -> (AlignmentValue, TargetVerse) {
    let verse = TargetVerse::new(TEXT);
    (bootstrap(&orig(), &verse, basis(1)), verse)
}

fn bank(value: &AlignmentValue, text: &str, occurrence: u32) -> TargetWord {
    value
        .word_bank
        .iter()
        .find(|w| w.word == text && w.occurrence == occurrence)
        .cloned()
        .unwrap_or_else(|| panic!("{text} {occurrence} is not banked"))
}

fn card(value: &AlignmentValue, i: usize) -> Vec<String> {
    keys(&value.alignments[i].bottom_words)
}

fn keys(words: &[TargetWord]) -> Vec<String> {
    words
        .iter()
        .map(|w| format!("{}/{}", w.word, w.occurrence))
        .collect()
}

// ---- #754 cardsInVerseOrder ----

#[test]
fn la_fe_de_los_dropped_as_fe_la_los_reads_la_fe_los() {
    let (mut value, verse) = boot();
    for w in ["fe", "la", "los"] {
        value.link(0, &bank(&value, w, 1)).unwrap();
    }
    assert_eq!(
        card(&value, 0),
        ["fe/1", "la/1", "los/1"],
        "drop order, before the sort"
    );
    value.cards_in_verse_order(&verse);
    assert_eq!(card(&value, 0), ["la/1", "fe/1", "los/1"]);
}

#[test]
fn repeated_words_go_by_occurrence_la1_before_la2() {
    let (mut value, verse) = boot();
    value.link(1, &bank(&value, "la", 2)).unwrap();
    value.link(1, &bank(&value, "la", 1)).unwrap();
    value.cards_in_verse_order(&verse);
    assert_eq!(card(&value, 1), ["la/1", "la/2"]);
}

#[test]
fn only_the_target_words_move_and_on_every_card() {
    let (mut value, verse) = boot();
    for (c, w) in [(0, "los"), (0, "según"), (1, "verdad"), (1, "y")] {
        value.link(c, &bank(&value, w, 1)).unwrap();
    }
    let mut sorted = value.clone();
    sorted.cards_in_verse_order(&verse);
    assert_eq!(card(&sorted, 0), ["según/1", "los/1"]);
    assert_eq!(card(&sorted, 1), ["y/1", "verdad/1"]);
    let sources = |v: &AlignmentValue| -> Vec<Vec<SourceWord>> {
        v.alignments.iter().map(|g| g.top_words.clone()).collect()
    };
    assert_eq!(sources(&sorted), sources(&value));
    assert_eq!(sorted.word_bank, value.word_bank);
}

#[test]
fn a_word_the_text_no_longer_has_goes_last_in_its_card() {
    let (mut value, _) = boot();
    value.link(0, &bank(&value, "fe", 1)).unwrap();
    value.link(0, &bank(&value, "según", 1)).unwrap();
    let edited = TargetVerse::new(&TEXT.replace("según", "conforme a"));
    assert!(!keys(edited.words()).contains(&"según/1".to_owned()));
    value.cards_in_verse_order(&edited);
    assert_eq!(card(&value, 0), ["fe/1", "según/1"]);
}

// ---- #754 load: a stored out-of-order card shows in verse order, the file is not rewritten ----

#[test]
fn a_stored_out_of_order_card_opens_in_verse_order_with_nothing_to_save() {
    let (mut value, verse) = boot();
    for w in ["los", "fe", "la"] {
        value.link(0, &bank(&value, w, 1)).unwrap();
    }
    value.settle(&verse);
    let stored = value.clone();
    let aligner = Aligner::open(Some(&stored), None, verse, &orig(), basis(1)).unwrap();
    assert_eq!(card(aligner.value(), 0), ["la/1", "fe/1", "los/1"]);
    assert!(!aligner.is_changed(), "the store sees no write");
    assert_eq!(
        card(&stored, 0),
        ["los/1", "fe/1", "la/1"],
        "the stored value is as it was"
    );
}

// ---- #754 export: the card order does not change the aligned USFM ----

#[test]
fn weaving_a_card_in_drop_order_or_in_verse_order_gives_the_same_bytes() {
    let (value, content) = sample_tit_1_1();
    // "de" (the first) on the Παῦλος card next to "Pablo": two words in one card.
    let de = bank(&value, "de", 1);
    let with_card = |bottom: Vec<TargetWord>| {
        let mut v = value.clone();
        v.alignments[0].bottom_words = bottom;
        v.word_bank.retain(|w| w != &de);
        v
    };
    let pablo = value.alignments[0].bottom_words.clone();
    let in_order = weave_verse(
        &content,
        &with_card([pablo.clone(), vec![de.clone()]].concat()),
    )
    .unwrap();
    let reversed = weave_verse(&content, &with_card([vec![de.clone()], pablo].concat())).unwrap();
    assert_eq!(reversed, in_order);
    assert!(in_order.contains("\\zaln-s"));
}

/// TIT 1:1 of tC4's sample project: its stored record and the verse's content.

// ---- The aligner keeps every card in verse order after every edit ----

#[test]
fn every_edit_leaves_each_card_in_verse_order_wherever_the_word_was_dropped() {
    let verse = TargetVerse::new(TEXT);
    let mut aligner = Aligner::open(None, None, verse, &orig(), basis(1)).unwrap();
    let word = |a: &Aligner, w: &str, o: u32| bank(a.value(), w, o);
    for w in ["los", "fe", "la"] {
        aligner.place(0, &word(&aligner, w, 1)).unwrap();
    }
    assert_eq!(card(aligner.value(), 0), ["la/1", "fe/1", "los/1"], "place");
    aligner.place(1, &word(&aligner, "verdad", 1)).unwrap();
    aligner.move_word(0, 1, &common::word("la", 1, 2)).unwrap();
    assert_eq!(card(aligner.value(), 1), ["la/1", "verdad/1"], "move");
    aligner.place(1, &word(&aligner, "según", 1)).unwrap();
    aligner.merge(1, 0).unwrap();
    assert_eq!(
        card(aligner.value(), 0),
        ["según/1", "la/1", "fe/1", "los/1", "verdad/1"],
        "merge"
    );
    let many = [
        word(&aligner, "y", 1),
        word(&aligner, "la", 2),
        word(&aligner, "de", 1),
    ];
    aligner.split(0).unwrap();
    aligner.place_many(1, &many).unwrap();
    assert_eq!(
        card(aligner.value(), 1),
        ["de/1", "y/1", "la/2"],
        "place_many"
    );
}

// ---- Q19: a value stored in another order is kept as it was until it is edited ----

#[test]
fn tc4s_sample_record_opens_in_verse_order_and_is_saved_in_it_only_after_an_edit() {
    let (stored, content) = sample_tit_1_1();
    let verse = TargetVerse::new(&content);
    // tC4 4.0 wrote this bank in its bootstrap order: every first occurrence, then every
    // second.
    assert_eq!(
        keys(&stored.word_bank[13..17]),
        ["piedad/1", "de/2", "y/2", "la/2"]
    );
    let open = || {
        Aligner::open(
            Some(&stored),
            None,
            verse.clone(),
            &source_verse(UGNT, "1:1"),
            basis(1),
        )
        .unwrap()
    };
    let aligner = open();
    assert!(!aligner.is_stale());
    let shown = &aligner.value().word_bank;
    let mut expected = stored.word_bank.clone();
    verse.sort_in_verse_order(&mut expected);
    assert_eq!(shown, &expected);
    assert_ne!(shown, &stored.word_bank);
    assert!(!aligner.is_changed(), "order alone is not saved");
    let mut as_shown = stored.clone();
    as_shown.word_bank = expected;
    // The six UGNT words the record has no card for get theirs, after its ten (#790).
    assert_eq!(&aligner.value().alignments[..10], &stored.alignments[..]);
    as_shown.alignments = aligner.value().alignments.clone();
    assert_eq!(
        aligner.value(),
        &as_shown,
        "only the order and the fit differ"
    );

    // Mark valid saves the value, in verse order.
    let mut marked = open();
    marked.mark_valid().unwrap();
    assert!(marked.is_changed());
    assert_eq!(marked.value().word_bank, as_shown.word_bank);

    // So does any edit.
    let mut edited = open();
    let de = bank(edited.value(), "de", 1);
    edited.place(4, &de).unwrap();
    edited.unplace(4, &de).unwrap();
    assert!(edited.is_changed());
    assert_eq!(edited.value().word_bank, as_shown.word_bank);
}
