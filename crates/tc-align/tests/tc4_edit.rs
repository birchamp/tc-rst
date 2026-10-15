//! tC4's alignment editing tests, ported: `test/align-edit.test.ts` (C2.11, C2.12, #129)
//! and `test/align-done.test.ts` (#271, D73). The verses and words are tC4's.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{basis, show};
use std::num::NonZeroU16;
use tc_align::{
    AlignmentValue, Current, EditError, SourceVerse, SourceWord, TargetVerse, TargetWord,
    VerseState, verse_status,
};

fn greek(text: &str, strong: &str, lemma: &str, morph: &str) -> SourceWord {
    SourceWord {
        content: text.to_owned(),
        strong: Some(strong.to_owned()),
        lemma: Some(lemma.to_owned()),
        morph: Some(morph.to_owned()),
        occurrence: 1,
        occurrences: 1,
        extra: Vec::new(),
    }
}

fn orig() -> SourceVerse {
    SourceVerse::new(vec![
        greek("Παῦλος", "G39720", "Παῦλος", "Gr,N"),
        greek("δοῦλος", "G14010", "δοῦλος", "Gr,N"),
    ])
}

fn orig3() -> SourceVerse {
    let mut words = orig().words().to_vec();
    words.push(greek("Θεοῦ", "G23160", "θεός", "Gr,N"));
    SourceVerse::new(words)
}

const TARGET: &str = "Pablo, siervo de Dios y de Dios";

fn boot_with(source: &SourceVerse, text: &str) -> (AlignmentValue, TargetVerse) {
    let verse = TargetVerse::new(text);
    (AlignmentValue::bootstrap(source, &verse, basis(1)), verse)
}

fn boot() -> (AlignmentValue, TargetVerse) {
    boot_with(&orig(), TARGET)
}

fn word(text: &str, occurrence: u32, occurrences: u32) -> TargetWord {
    TargetWord {
        text: text.to_owned(),
        occurrence,
        occurrences,
    }
}

fn bank_word(value: &AlignmentValue, text: &str, occurrence: u32) -> TargetWord {
    value
        .word_bank
        .iter()
        .find(|w| w.text == text && w.occurrence == occurrence)
        .cloned()
        .unwrap_or_else(|| panic!("{text} {occurrence} is not banked"))
}

fn sources(value: &AlignmentValue, card: usize) -> Vec<&str> {
    value.alignments[card]
        .sources
        .iter()
        .map(|w| w.content.as_str())
        .collect()
}

fn targets(value: &AlignmentValue, card: usize) -> Vec<&str> {
    value.alignments[card]
        .targets
        .iter()
        .map(|w| w.text.as_str())
        .collect()
}

// ---- C2.12 bootstrap: an unaligned verse starts usable, not empty ----

#[test]
fn bootstrap_offers_one_card_per_original_word_each_with_no_target_words() {
    let (value, _) = boot();
    assert_eq!(value.alignments.len(), 2);
    assert!(value.alignments.iter().all(|g| g.targets.is_empty()));
    assert_eq!(sources(&value, 0), ["Παῦλος"]);
}

#[test]
fn bootstrap_banks_every_target_word() {
    let (value, _) = boot();
    assert_eq!(value.word_bank.len(), 7);
    assert!(value.word_bank.iter().any(|w| w.text == "Pablo"));
}

#[test]
fn bootstrap_records_the_draft_it_was_built_against() {
    let (value, verse) = boot();
    assert_eq!(value.target_verse_md5, verse.md5());
    assert_eq!(value.target_verse_md5.to_string().len(), 32);
    assert_eq!(value.source_version, common::SOURCE_NT);
    assert!(!value.marked_invalid);
    assert!(!value.is_stale_against(&verse));
}

#[test]
fn bootstrap_distinguishes_repeated_words_by_occurrence() {
    let (value, _) = boot();
    let dios: Vec<_> = value
        .word_bank
        .iter()
        .filter(|w| w.text == "Dios")
        .collect();
    assert_eq!(dios.len(), 2);
    assert_eq!(
        dios.iter().map(|w| w.occurrence).collect::<Vec<_>>(),
        [1, 2]
    );
    assert!(dios.iter().all(|w| w.occurrences == 2));
}

#[test]
fn bootstrap_banks_in_tc4_order_every_first_occurrence_then_every_second() {
    let (value, _) = boot_with(&orig(), "de Dios y de Pablo");
    assert_eq!(
        show(&value.word_bank),
        ["de/1/2", "Dios/1/1", "y/1/1", "Pablo/1/1", "de/2/2"]
    );
}

// ---- C2.11 link / unlink: a word is in exactly one place ----

#[test]
fn the_two_dios_occurrences_link_independently() {
    let (mut value, _) = boot();
    value.link(0, &bank_word(&value, "Dios", 1)).unwrap();
    assert_eq!(
        value.word_bank.iter().filter(|w| w.text == "Dios").count(),
        1
    );
    value.link(1, &bank_word(&value, "Dios", 2)).unwrap();
    assert_eq!(
        value.word_bank.iter().filter(|w| w.text == "Dios").count(),
        0
    );
    assert_eq!(value.alignments[0].targets[0].occurrence, 1);
    assert_eq!(value.alignments[1].targets[0].occurrence, 2);
}

#[test]
fn a_word_that_is_not_banked_cannot_be_linked() {
    let (mut value, _) = boot();
    let pablo = bank_word(&value, "Pablo", 1);
    value.link(0, &pablo).unwrap();
    let once = value.clone();
    assert_eq!(value.link(1, &pablo), Err(EditError::NotInBank(pablo)));
    assert_eq!(value, once);
}

#[test]
fn an_out_of_range_card_is_refused_not_a_crash() {
    let (mut value, _) = boot();
    let before = value.clone();
    assert_eq!(
        value.link(99, &bank_word(&before, "Pablo", 1)),
        Err(EditError::NoSuchCard(99))
    );
    assert_eq!(value, before);
}

#[test]
fn unlinking_a_word_that_is_not_there_is_refused() {
    let (mut value, verse) = boot();
    let before = value.clone();
    let pablo = bank_word(&value, "Pablo", 1);
    assert!(matches!(
        value.unlink(0, &pablo, &verse),
        Err(EditError::NotOnCard { card: 0, .. })
    ));
    assert_eq!(value, before);
}

#[test]
fn unlinking_puts_the_word_back_where_the_bootstrap_had_it() {
    let (mut value, verse) = boot();
    let bootstrap_bank = value.word_bank.clone();
    for (card, text, occurrence) in [(0, "Pablo", 1), (1, "de", 2), (0, "y", 1)] {
        value
            .link(card, &bank_word(&value, text, occurrence))
            .unwrap();
    }
    for (card, text, occurrence) in [(1, "de", 2), (0, "Pablo", 1), (0, "y", 1)] {
        let w = word(text, occurrence, if text == "de" { 2 } else { 1 });
        value.unlink(card, &w, &verse).unwrap();
    }
    assert_eq!(value.word_bank, bootstrap_bank);
}

// ---- #129 merge / split: phrase alignments stay original-anchored ----

#[test]
fn merge_concatenates_sources_and_targets_in_card_order_at_the_lower_card() {
    let (mut value, _) = boot();
    value.link(0, &bank_word(&value, "Pablo", 1)).unwrap();
    value.link(1, &bank_word(&value, "siervo", 1)).unwrap();
    let total = value.total();
    value.merge(1, 0).unwrap();
    assert_eq!(value.alignments.len(), 1);
    assert_eq!(sources(&value, 0), ["Παῦλος", "δοῦλος"]);
    assert_eq!(targets(&value, 0), ["Pablo", "siervo"]);
    assert_eq!(value.total(), total, "no word entered or left the verse");
}

#[test]
fn merge_with_the_same_or_an_out_of_range_card_changes_nothing() {
    let (mut value, _) = boot();
    let before = value.clone();
    assert_eq!(value.merge(0, 0), Err(EditError::NotAdjacent(0, 0)));
    assert_eq!(value.merge(0, 99), Err(EditError::NoSuchCard(99)));
    assert_eq!(value, before);
}

#[test]
fn split_restores_single_cards_with_the_placed_words_on_the_first() {
    let (mut value, _) = boot();
    value.link(0, &bank_word(&value, "Pablo", 1)).unwrap();
    value.merge(0, 1).unwrap();
    value.split(0).unwrap();
    assert_eq!(value.alignments.len(), 2);
    assert_eq!(sources(&value, 0), ["Παῦλος"]);
    assert_eq!(targets(&value, 0), ["Pablo"]);
    assert!(value.alignments[1].targets.is_empty());
}

#[test]
fn split_on_a_single_card_changes_nothing() {
    let (mut value, _) = boot();
    let before = value.clone();
    assert_eq!(value.split(0), Err(EditError::NotAPhrase(0)));
    assert_eq!(value, before);
}

#[test]
fn move_word_moves_a_placed_word_between_cards_and_leaves_the_bank() {
    let (mut value, _) = boot();
    value.link(0, &bank_word(&value, "Pablo", 1)).unwrap();
    let bank = value.word_bank.clone();
    value.move_word(0, 1, &word("Pablo", 1, 1)).unwrap();
    assert!(value.alignments[0].targets.is_empty());
    assert_eq!(targets(&value, 1), ["Pablo"]);
    assert_eq!(
        value.word_bank, bank,
        "the bank is identical, order included"
    );
}

#[test]
fn a_move_to_a_missing_card_or_onto_itself_changes_nothing() {
    let (mut value, _) = boot();
    value.link(0, &bank_word(&value, "Pablo", 1)).unwrap();
    let before = value.clone();
    let pablo = word("Pablo", 1, 1);
    assert_eq!(value.move_word(0, 0, &pablo), Err(EditError::SameCard));
    assert_eq!(
        value.move_word(0, 99, &pablo),
        Err(EditError::NoSuchCard(99))
    );
    assert_eq!(value, before);
}

#[test]
fn a_non_adjacent_merge_is_refused_so_phrases_stay_in_verse_order() {
    let (mut value, _) = boot_with(&orig3(), TARGET);
    assert_eq!(value.alignments.len(), 3);
    let before = value.clone();
    assert_eq!(value.merge(0, 2), Err(EditError::NotAdjacent(0, 2)));
    assert_eq!(value.merge(2, 0), Err(EditError::NotAdjacent(2, 0)));
    assert_eq!(value, before);
}

#[test]
fn adjacent_merges_keep_verse_order_whichever_way_they_are_dragged() {
    let (value, _) = boot_with(&orig3(), TARGET);
    let mut down = value.clone();
    down.merge(2, 1).unwrap();
    assert_eq!(sources(&down, 1), ["δοῦλος", "Θεοῦ"]);
    let mut up = value;
    up.merge(0, 1).unwrap();
    assert_eq!(sources(&up, 0), ["Παῦλος", "δοῦλος"]);
}

#[test]
fn move_word_of_a_word_not_on_the_from_card_changes_nothing() {
    let (mut value, _) = boot();
    let before = value.clone();
    assert!(matches!(
        value.move_word(0, 1, &word("Pablo", 1, 1)),
        Err(EditError::NotOnCard { card: 0, .. })
    ));
    assert_eq!(value, before);
}

#[test]
fn clear_starts_the_verse_over() {
    let (fresh, verse) = boot_with(&orig3(), TARGET);
    let mut value = fresh.clone();
    value.link(2, &bank_word(&value, "Dios", 2)).unwrap();
    value.link(0, &bank_word(&value, "Pablo", 1)).unwrap();
    value.merge(0, 1).unwrap();
    value.clear(&verse);
    assert_eq!(value.alignments, fresh.alignments);
    assert_eq!(value.word_bank, fresh.word_bank);
}

// ---- #271 (D73): all aligned is done; Mark valid; any edit takes it back ----

const TEXT: &str = "Pablo siervo";

/// Pablo placed on Παῦλος; "siervo" still in the bank.
fn half_aligned() -> (AlignmentValue, TargetVerse) {
    let (mut value, verse) = boot_with(&orig(), TEXT);
    value.link(0, &bank_word(&value, "Pablo", 1)).unwrap();
    value.target_verse_md5 = verse.md5();
    (value, verse)
}

fn current(verse: &TargetVerse) -> Current<'_> {
    Current {
        run_end: NonZeroU16::MIN,
        text_md5: verse.md5(),
        source_sha: None,
    }
}

#[test]
fn fully_aligned_only_when_the_bank_is_empty_and_every_card_has_a_word() {
    let (half, _) = half_aligned();
    assert!(!half.is_fully_aligned());
    let mut full = half.clone();
    full.link(1, &bank_word(&full, "siervo", 1)).unwrap();
    assert!(full.is_fully_aligned());
    // A bootstrap (nothing placed) is not aligned even if the bank were empty.
    let (mut empty, _) = boot_with(&orig(), TEXT);
    empty.word_bank.clear();
    assert!(!empty.is_fully_aligned());
}

#[test]
fn placing_the_last_word_sets_done() {
    let (mut value, verse) = half_aligned();
    value.settle(&verse);
    assert!(!value.done);
    value.link(1, &bank_word(&value, "siervo", 1)).unwrap();
    value.settle(&verse);
    assert!(value.done);
}

#[test]
fn mark_done_sets_done_with_words_in_the_bank_and_vouches_for_the_current_text() {
    let (stale, _) = half_aligned();
    let new_text = TargetVerse::new("Pablo, siervo");
    assert!(stale.is_stale_against(&new_text));
    let mut marked = stale.clone();
    marked.mark_done(&new_text);
    assert!(marked.done);
    assert_eq!(marked.word_bank.len(), 1);
    assert!(!marked.is_stale_against(&new_text));
    // A record flagged for re-review is re-reviewed by Mark valid.
    let mut flagged = stale;
    flagged.marked_invalid = true;
    flagged.mark_done(&new_text);
    assert!(!flagged.marked_invalid);
    assert_eq!(
        verse_status(Some(&flagged), Some(&current(&new_text))).state,
        VerseState::Valid
    );
}

#[test]
fn a_reflow_recomputes_done() {
    let (mut full, verse) = half_aligned();
    full.link(1, &bank_word(&full, "siervo", 1)).unwrap();
    full.settle(&verse);
    assert!(full.done);
    // "siervo" becomes "esclavo": its link drops, the bank is not empty: not done.
    let dropped = tc_align::reflow(&full, &TargetVerse::new("Pablo esclavo")).unwrap();
    assert!(!dropped.value.done);
    // Only punctuation changed: every word still placed: still done.
    let kept = tc_align::reflow(&full, &TargetVerse::new("Pablo, siervo.")).unwrap();
    assert!(kept.value.done);
}

#[test]
fn the_rail_reads_valid_from_done_only_and_staleness_outranks_it() {
    let (half, verse) = half_aligned();
    let now = current(&verse);
    let mut full_no_flag = half.clone();
    full_no_flag
        .link(1, &bank_word(&full_no_flag, "siervo", 1))
        .unwrap();
    let state = |v: Option<&AlignmentValue>, c: Option<&Current<'_>>| verse_status(v, c).state;
    assert_eq!(state(Some(&full_no_flag), Some(&now)), VerseState::Todo);
    let mut settled = full_no_flag.clone();
    settled.settle(&verse);
    assert_eq!(state(Some(&settled), Some(&now)), VerseState::Valid);
    let mut marked = half.clone();
    marked.mark_done(&verse);
    assert_eq!(state(Some(&marked), Some(&now)), VerseState::Valid);
    let other = TargetVerse::new("Pablo siervo de Dios");
    assert_eq!(
        state(Some(&marked), Some(&current(&other))),
        VerseState::Invalid
    );
    let mut flagged = marked;
    flagged.marked_invalid = true;
    assert_eq!(state(Some(&flagged), Some(&now)), VerseState::Invalid);
    assert_eq!(state(None, Some(&now)), VerseState::Todo);
    assert_eq!(state(None, None), VerseState::Undrafted);
    let status = verse_status(Some(&half), Some(&now));
    assert_eq!((status.placed, status.total), (1, 2));
}
