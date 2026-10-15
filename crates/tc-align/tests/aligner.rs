//! One verse open in the Align workspace, as tC4's J5 journey drives it
//! (`e2e/j05-align-verse`): place and unplace words, Mark valid, and suggestions that are
//! proposals until accepted (D72).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use tc_align::suggest::{self, Model, RawLink};
use tc_align::{Aligner, AlignmentValue, MarkValidRefused, TargetVerse, TargetWord};
use tc_core::Testament;

/// TIT 1:1 of en_ult as a draft, against UGNT TIT 1:1.
fn open(stored: Option<&AlignmentValue>, text: &str) -> Aligner {
    Aligner::open(
        stored,
        TargetVerse::new(text),
        &source_verse(UGNT, "1:1"),
        basis(1),
    )
}

const TEXT: &str = "Paul, a servant of God and an apostle of Jesus Christ";

fn banked(aligner: &Aligner, text: &str) -> TargetWord {
    aligner
        .value()
        .word_bank
        .iter()
        .find(|w| w.text == text)
        .cloned()
        .unwrap()
}

#[test]
fn a_verse_with_no_alignment_opens_as_one_card_per_original_word_and_every_word_banked() {
    let aligner = open(None, TEXT);
    let value = aligner.value();
    assert_eq!(
        value.alignments.len(),
        source_verse(UGNT, "1:1").words().len()
    );
    assert_eq!(value.word_bank.len(), 11);
    assert_eq!(value.placed(), 0);
    assert!(!aligner.is_stale());
    assert!(!aligner.is_changed());
}

#[test]
fn placing_and_unplacing_conserve_the_words_and_stamp_the_shown_text() {
    let mut aligner = open(None, TEXT);
    let paul = banked(&aligner, "Paul");
    aligner.place(0, &paul).unwrap();
    assert_eq!(
        aligner.value().alignments[0].targets,
        std::slice::from_ref(&paul)
    );
    assert_eq!(aligner.value().total(), 11);
    assert_eq!(
        aligner.value().target_verse_md5,
        TargetVerse::new(TEXT).md5()
    );
    assert!(aligner.is_changed());
    aligner.unplace(0, &paul).unwrap();
    assert_eq!(aligner.value().placed(), 0);
    assert_eq!(aligner.value().word_bank[0], paul, "back in its place");
}

#[test]
fn mark_valid_with_words_left_and_any_edit_takes_it_back() {
    let mut aligner = open(None, TEXT);
    aligner.place(0, &banked(&aligner, "Paul")).unwrap();
    aligner.mark_valid().unwrap();
    assert!(aligner.value().done);
    assert!(!aligner.value().word_bank.is_empty());
    aligner.place(1, &banked(&aligner, "servant")).unwrap();
    assert!(!aligner.value().done);
}

fn proposal_for(aligner: &mut Aligner, card: usize, word: &str) {
    let input = aligner.session_input();
    let target = input.target().iter().position(|t| t.text == word).unwrap();
    let source = input
        .source()
        .iter()
        .position(|s| s.text == aligner.value().alignments[card].sources[0].content)
        .unwrap();
    aligner.show_suggestions(&[RawLink {
        source: vec![source],
        target: vec![target],
        confidence: 0.9,
    }]);
}

#[test]
fn suggestions_write_nothing_block_mark_valid_and_vanish_on_reject() {
    let mut aligner = open(None, TEXT);
    let before = aligner.value().clone();
    proposal_for(&mut aligner, 0, "Paul");
    assert_eq!(aligner.proposals().len(), 1);
    assert_eq!(
        aligner.value(),
        &before,
        "a shown suggestion writes nothing"
    );
    assert!(!aligner.is_changed());
    assert_eq!(aligner.mark_valid(), Err(MarkValidRefused));
    assert!(!aligner.value().done);
    aligner.reject_all();
    assert!(aligner.proposals().is_empty());
    assert_eq!(aligner.value(), &before);
    aligner.mark_valid().unwrap();
}

#[test]
fn an_accepted_suggestion_is_the_same_as_placing_the_word_by_hand() {
    let mut by_hand = open(None, TEXT);
    by_hand.place(0, &banked(&by_hand, "Paul")).unwrap();
    let mut accepted = open(None, TEXT);
    proposal_for(&mut accepted, 0, "Paul");
    accepted.accept(0).unwrap();
    assert_eq!(accepted.value(), by_hand.value());
    assert!(
        accepted.proposals().is_empty(),
        "an accepted proposal is spent"
    );
    assert!(accepted.accept(0).is_err());
}

#[test]
fn proposals_follow_their_card_through_a_merge() {
    let mut aligner = open(None, TEXT);
    proposal_for(&mut aligner, 1, "servant");
    assert_eq!(aligner.proposals()[0].card, 1);
    aligner.merge(0, 1).unwrap();
    assert_eq!(aligner.proposals()[0].card, 0);
    assert_eq!(aligner.accept_all(), 1);
    assert_eq!(aligner.value().alignments[0].targets[0].text, "servant");
}

/// Paul and servant placed on TIT 1:1, saved against `TEXT`.
fn paul_and_servant() -> AlignmentValue {
    let mut first = open(None, TEXT);
    first.place(0, &banked(&first, "Paul")).unwrap();
    first.place(1, &banked(&first, "servant")).unwrap();
    first.into_value()
}

fn sorted_words(words: impl IntoIterator<Item = TargetWord>) -> Vec<(String, u32)> {
    let mut out: Vec<_> = words.into_iter().map(|w| (w.text, w.occurrence)).collect();
    out.sort();
    out
}

#[test]
fn a_stored_alignment_made_for_other_text_opens_reflowed_and_says_what_it_dropped() {
    let stored = paul_and_servant();
    let edited = "Paul, a slave of God and an apostle of Jesus Christ";
    let mut aligner = open(Some(&stored), edited);
    assert!(!aligner.is_stale());
    assert_eq!(aligner.value().placed(), 1);
    assert_eq!(aligner.dropped().len(), 1);
    assert_eq!(aligner.dropped()[0].word.text, "servant");
    assert_eq!(aligner.dropped()[0].sources[0].content, "δοῦλος");
    assert!(
        !aligner.is_changed(),
        "a carry-over that dropped a link is not saved until the translator acts"
    );
    // The stored value is untouched: it stays as it was, kept, and computes invalid.
    assert_eq!(stored.placed(), 2);
    assert!(stored.is_stale_against(&TargetVerse::new(edited)));
    aligner.keep_reflow();
    assert!(aligner.is_changed());

    let mut edited_after = open(Some(&stored), edited);
    edited_after
        .place(1, &banked(&edited_after, "slave"))
        .unwrap();
    assert!(
        edited_after.is_changed(),
        "an edit saves the carried-over value"
    );
}

#[test]
fn a_carry_over_that_drops_nothing_is_saved_as_tc4_saves_it() {
    let stored = paul_and_servant();
    let edited = "Paul, a servant of God, and an apostle of Jesus Christ.";
    let aligner = open(Some(&stored), edited);
    assert!(aligner.dropped().is_empty());
    assert!(aligner.is_changed());
    assert_eq!(aligner.value().placed(), 2);
    assert!(!aligner.is_stale());
}

#[test]
fn a_stale_value_tc4_would_not_reflow_opens_with_the_words_of_the_shown_verse() {
    let edited = "Paul, a slave of God";
    // Nothing placed: the bank of the old text is not offered.
    let mut merged = open(None, TEXT);
    merged.merge(0, 1).unwrap();
    let stored = merged.into_value();
    let aligner = open(Some(&stored), edited);
    assert_eq!(
        sorted_words(aligner.value().all_target_words().cloned()),
        sorted_words(TargetVerse::new(edited).words().to_vec())
    );
    assert_eq!(
        aligner.value().alignments,
        stored.alignments,
        "the phrase is kept"
    );
    assert!(!aligner.is_changed());

    // Marked invalid by tC3 or tC4: shown carried over with what it would drop, not saved.
    let mut flagged = paul_and_servant();
    flagged.marked_invalid = true;
    let aligner = open(Some(&flagged), edited);
    assert_eq!(aligner.dropped().len(), 1);
    assert_eq!(
        sorted_words(aligner.value().all_target_words().cloned()),
        sorted_words(TargetVerse::new(edited).words().to_vec())
    );
    assert!(!aligner.is_changed());
}

#[test]
fn a_value_made_for_another_span_is_set_aside_and_the_verse_starts_over() {
    let mut stored = paul_and_servant();
    stored.span_end = std::num::NonZeroU16::new(2).unwrap();
    let aligner = open(Some(&stored), TEXT);
    assert_eq!(aligner.value(), open(None, TEXT).value());
    assert_eq!(aligner.earlier(), Some(&stored));
    assert!(!aligner.is_changed());
    assert!(open(Some(&paul_and_servant()), TEXT).earlier().is_none());
}

#[test]
fn a_stored_value_with_no_cards_opens_as_the_bootstrap() {
    let mut empty = open(None, TEXT).into_value();
    empty.alignments.clear();
    empty.word_bank.clear();
    empty.marked_invalid = true;
    let aligner = open(Some(&empty), TEXT);
    assert_eq!(aligner.value(), open(None, TEXT).value());
}

#[test]
fn suggestions_from_a_model_trained_on_titus_are_proposals_for_bank_words_only() {
    let training: Vec<_> = verses(ULT)
        .into_iter()
        .skip(1)
        .filter_map(|(r, body)| suggest::TrainingVerse::from_aligned_usfm(r, &body).unwrap())
        .collect();
    let model = Model::train(Testament::New, &training[..6], 0);
    let mut aligner = open(None, TEXT);
    aligner.place(0, &banked(&aligner, "Paul")).unwrap();
    let before = aligner.value().clone();
    let raw = model.predict(&aligner.session_input());
    aligner.show_suggestions(&raw);
    assert!(!aligner.proposals().is_empty());
    assert_eq!(aligner.value(), &before);
    for p in aligner.proposals() {
        assert!(aligner.value().word_bank.contains(&p.word));
        assert_ne!(p.word.text, "Paul", "a placed word is never proposed");
    }
}
