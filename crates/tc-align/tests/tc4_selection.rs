//! tC4's bank selection tests, ported: `test/align-selection.test.ts` (#736) at d310bf3:
//! plain click, Ctrl or Cmd toggle, Shift range in bank order, and words that need not be
//! next to each other. Then the selection in the aligner: several words placed on one
//! card in one edit, only the shown selection placed (Q16), and when the selection is
//! kept or cleared. Last, the occurrence numbers of repeated words (#735).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::*;
use tc_align::selection::{dragged_words, next_selection, shown_selection};
use tc_align::suggest::RawLink;
use tc_align::{Aligner, Click, EditError, Occurrence, SourceWord, TargetVerse, TargetWord};

fn w(text: &str, occurrence: u32) -> TargetWord {
    common::word(text, occurrence, 1)
}

fn bank() -> Vec<TargetWord> {
    ["de", "y", "según", "la", "fe", "los"]
        .into_iter()
        .map(|t| w(t, 1))
        .collect()
}

fn words(selection: Option<Vec<TargetWord>>) -> Option<Vec<String>> {
    selection.map(|s| s.into_iter().map(|w| w.word).collect())
}

fn click(armed: &[TargetWord], index: usize, click: Click) -> Option<Vec<String>> {
    let bank = bank();
    let shown = next_selection(&bank, &shown_selection(&bank, armed), click, index);
    // The rule reads only the shown part of what it is given.
    assert_eq!(shown, next_selection(&bank, armed, click, index));
    words(shown)
}

fn list(texts: &[&str]) -> Option<Vec<String>> {
    Some(texts.iter().map(|t| (*t).to_owned()).collect())
}

// ---- #736 nextSelection: the bank selection after a click ----

#[test]
fn a_plain_click_selects_that_word_alone_and_clears_it_when_it_is_the_only_one() {
    assert_eq!(click(&[], 2, Click::Plain), list(&["según"]));
    assert_eq!(
        click(&[w("de", 1), w("fe", 1)], 2, Click::Plain),
        list(&["según"])
    );
    assert_eq!(click(&[w("según", 1)], 2, Click::Plain), None);
}

#[test]
fn ctrl_or_cmd_adds_a_word_or_takes_it_out_keeping_bank_order() {
    let ctrl = Click::from_modifiers(true, false);
    assert_eq!(ctrl, Click::Toggle);
    assert_eq!(click(&[w("fe", 1)], 0, ctrl), list(&["de", "fe"]));
    assert_eq!(click(&[w("de", 1), w("fe", 1)], 0, ctrl), list(&["fe"]));
    assert_eq!(click(&[w("fe", 1)], 4, ctrl), None);
}

#[test]
fn ctrl_or_cmd_wins_over_shift() {
    assert_eq!(Click::from_modifiers(true, true), Click::Toggle);
    assert_eq!(Click::from_modifiers(false, true), Click::Range);
    assert_eq!(Click::from_modifiers(false, false), Click::Plain);
}

#[test]
fn shift_adds_the_range_from_the_first_selected_word_in_bank_order_either_direction() {
    let shift = Click::from_modifiers(false, true);
    assert_eq!(
        click(&[w("y", 1)], 4, shift),
        list(&["y", "según", "la", "fe"])
    );
    assert_eq!(
        click(&[w("fe", 1)], 1, shift),
        list(&["y", "según", "la", "fe"])
    );
    assert_eq!(click(&[], 3, shift), list(&["la"]));
}

#[test]
fn shift_adds_and_never_takes_out() {
    // "de" anchors the range; "la", outside it, stays selected.
    assert_eq!(
        click(&[w("la", 1), w("de", 1)], 1, Click::Range),
        list(&["de", "y", "la"])
    );
}

#[test]
fn a_selected_word_the_bank_no_longer_shows_does_not_break_shift() {
    // "escogidos" was selected, then proposed on a card: it left the shown bank.
    assert_eq!(click(&[w("escogidos", 1)], 3, Click::Range), list(&["la"]));
    assert_eq!(
        click(&[w("escogidos", 1), w("y", 1)], 3, Click::Range),
        list(&["y", "según", "la"])
    );
}

#[test]
fn repeated_words_are_told_apart_by_occurrence() {
    let bank = [w("de", 1), w("y", 1), w("de", 2)];
    let both = next_selection(
        &bank,
        &shown_selection(&bank, &[w("de", 1)]),
        Click::Toggle,
        2,
    )
    .unwrap();
    assert_eq!(both, [w("de", 1), w("de", 2)]);
    assert_eq!(shown_selection(&bank, &[w("de", 2)]), [w("de", 2)]);
}

#[test]
fn a_click_past_the_shown_bank_changes_nothing() {
    assert_eq!(click(&[w("fe", 1)], 6, Click::Plain), list(&["fe"]));
    assert_eq!(click(&[], 6, Click::Range), None);
}

#[test]
fn a_selected_word_drags_the_selection_and_an_unselected_word_drags_itself() {
    let bank = bank();
    let armed = [w("fe", 1), w("y", 1)];
    assert_eq!(dragged_words(&bank, &armed, 4), [w("y", 1), w("fe", 1)]);
    assert_eq!(dragged_words(&bank, &armed, 2), [w("según", 1)]);
    assert!(dragged_words(&bank, &armed, 6).is_empty());
}

// ---- The selection in the aligner (#736, Q16) ----

const TEXT: &str = "Paul, a servant of God and an apostle of Jesus Christ";

fn open() -> Aligner {
    Aligner::open(
        None,
        None,
        TargetVerse::new(TEXT),
        &source_verse(UGNT, "1:1"),
        basis(1),
    )
    .unwrap()
}

/// Where `text` is in the shown bank.
fn at(aligner: &Aligner, text: &str) -> usize {
    aligner
        .shown_bank()
        .iter()
        .position(|w| w.word == text)
        .unwrap_or_else(|| panic!("{text} is not shown in the bank"))
}

fn select(aligner: &mut Aligner, texts: &[&str]) {
    for (i, text) in texts.iter().enumerate() {
        let click = if i == 0 { Click::Plain } else { Click::Toggle };
        aligner.click_bank_word(at(aligner, text), click);
    }
}

fn texts(words: &[TargetWord]) -> Vec<&str> {
    words.iter().map(|w| w.word.as_str()).collect()
}

/// A standing suggestion of `word` on `card`.
fn suggest(aligner: &mut Aligner, card: usize, word: &str) {
    let input = aligner.session_input();
    let target = input.target().iter().position(|t| t.text == word).unwrap();
    let source = input
        .source()
        .iter()
        .position(|s| s.text == aligner.value().alignments[card].top_words[0].word)
        .unwrap();
    aligner.show_suggestions(&[RawLink {
        source: vec![source],
        target: vec![target],
        confidence: 0.9,
    }]);
}

#[test]
fn a_card_click_places_the_whole_selection_in_one_edit_in_verse_order() {
    let mut aligner = open();
    assert!(!aligner.is_armed());
    select(&mut aligner, &["God", "servant", "a"]);
    assert_eq!(texts(&aligner.selection()), ["a", "servant", "God"]);
    assert!(aligner.is_armed());
    assert!(!aligner.is_changed(), "selecting writes nothing");
    assert_eq!(aligner.place_selection(1), Ok(3));
    assert_eq!(
        texts(&aligner.value().alignments[1].bottom_words),
        ["a", "servant", "God"]
    );
    assert!(aligner.selection().is_empty() && !aligner.is_armed());
    assert!(aligner.is_changed());

    // The same as placing each word by hand.
    let mut by_hand = open();
    for word in ["God", "a", "servant"] {
        let w = by_hand.shown_bank()[at(&by_hand, word)].clone();
        by_hand.place(1, &w).unwrap();
    }
    assert_eq!(aligner.value(), by_hand.value());
}

#[test]
fn only_the_shown_selection_is_placed_and_the_cards_look_armed_only_while_it_is_not_empty() {
    // Q16: "Paul" was selected, then proposed on card 0.
    let mut aligner = open();
    select(&mut aligner, &["Paul"]);
    suggest(&mut aligner, 0, "Paul");
    assert!(aligner.selection().is_empty());
    assert!(!aligner.is_armed());
    let before = aligner.value().clone();
    assert_eq!(aligner.place_selection(1), Err(EditError::NothingToPlace));
    assert_eq!(aligner.value(), &before);
    assert!(!aligner.is_changed());

    let mut aligner = open();
    select(&mut aligner, &["Paul", "servant"]);
    suggest(&mut aligner, 0, "Paul");
    assert_eq!(texts(&aligner.selection()), ["servant"]);
    assert_eq!(aligner.place_selection(2), Ok(1));
    assert_eq!(
        texts(&aligner.value().alignments[2].bottom_words),
        ["servant"]
    );
    assert_eq!(
        aligner.proposals().len(),
        1,
        "the suggestion of Paul stands"
    );
    assert!(aligner.value().word_bank.iter().any(|w| w.word == "Paul"));
}

#[test]
fn a_drag_carries_the_selection_or_the_word_and_its_drop_is_one_edit() {
    let mut aligner = open();
    select(&mut aligner, &["apostle", "an"]);
    let selected = aligner.dragged_words(at(&aligner, "apostle"));
    assert_eq!(texts(&selected), ["an", "apostle"]);
    let alone = aligner.dragged_words(at(&aligner, "God"));
    assert_eq!(texts(&alone), ["God"]);

    // A drop of an unselected word places it and clears the selection, as tC4 does.
    assert_eq!(aligner.place_many(2, &alone), Ok(1));
    assert!(aligner.selection().is_empty());

    // A drop with nothing left to place changes nothing and keeps the selection.
    select(&mut aligner, &["apostle"]);
    let before = aligner.value().clone();
    assert_eq!(
        aligner.place_many(3, &alone),
        Err(EditError::NothingToPlace)
    );
    assert_eq!(aligner.value(), &before);
    assert_eq!(texts(&aligner.selection()), ["apostle"]);

    // A stale payload: the words still banked are placed, the rest skipped.
    let mut payload = alone;
    payload.extend(aligner.dragged_words(at(&aligner, "apostle")));
    assert_eq!(aligner.place_many(3, &payload), Ok(1));
    assert_eq!(
        texts(&aligner.value().alignments[3].bottom_words),
        ["apostle"]
    );
}

#[test]
fn un_align_move_merge_split_and_clear_keep_the_selection() {
    let mut aligner = open();
    let paul = aligner.shown_bank()[at(&aligner, "Paul")].clone();
    let god = aligner.shown_bank()[at(&aligner, "God")].clone();
    aligner.place(0, &paul).unwrap();
    aligner.place(2, &god).unwrap();
    select(&mut aligner, &["servant", "Christ"]);
    aligner.unplace(0, &paul).unwrap();
    aligner.move_word(2, 1, &god).unwrap();
    aligner.merge(0, 1).unwrap();
    aligner.split(0).unwrap();
    assert_eq!(texts(&aligner.selection()), ["servant", "Christ"]);
    aligner.clear();
    assert_eq!(texts(&aligner.selection()), ["servant", "Christ"]);
    aligner.mark_valid().unwrap();
    assert_eq!(texts(&aligner.selection()), ["servant", "Christ"]);
}

#[test]
fn place_accept_and_accept_all_clear_the_selection() {
    let mut aligner = open();
    select(&mut aligner, &["servant"]);
    let paul = aligner.shown_bank()[at(&aligner, "Paul")].clone();
    aligner.place(0, &paul).unwrap();
    assert!(aligner.selection().is_empty(), "place");

    select(&mut aligner, &["servant"]);
    suggest(&mut aligner, 2, "God");
    aligner.accept(0).unwrap();
    assert!(aligner.selection().is_empty(), "confirm a suggestion");

    select(&mut aligner, &["servant"]);
    suggest(&mut aligner, 3, "apostle");
    assert_eq!(aligner.accept_all(), 1);
    assert!(aligner.selection().is_empty(), "accept all");
    assert_eq!(aligner.accept_all(), 0, "nothing left to accept");
}

#[test]
fn another_verse_opens_with_nothing_selected() {
    let aligner = open();
    assert!(aligner.selection().is_empty() && !aligner.is_armed());
}

// ---- #788 a card click places the shown selection: tC4's
// `test/align-shown-selection.test.tsx` (at tC4 main 9ad4d5c) on its inputs, the
// conformance sample's TIT 1:1 record and draft ----

/// The sample record open on its draft, with "escogidos" proposed for the ἐκλεκτῶν card
/// (index 8) when `suggested`, after `selected` were selected.
fn sample_open(selected: &[&str], suggested: bool) -> Aligner {
    let (stored, text) = sample_tit_1_1();
    let source = tc_align::SourceVerse::new(
        stored
            .alignments
            .iter()
            .flat_map(|g| g.top_words.clone())
            .collect(),
    );
    let mut aligner = Aligner::open(
        Some(&stored),
        None,
        TargetVerse::new(&text),
        &source,
        basis(1),
    )
    .unwrap();
    assert_eq!(aligner.value().alignments[8].top_words[0].word, "ἐκλεκτῶν");
    select(&mut aligner, selected);
    if suggested {
        suggest(&mut aligner, 8, "escogidos");
    }
    aligner
}

#[test]
fn b1_the_only_selected_word_became_a_suggestion_so_no_card_takes_a_click() {
    let mut aligner = sample_open(&["escogidos"], true);
    assert!(aligner.shown_bank().iter().all(|w| w.word != "escogidos"));
    assert!(aligner.selection().is_empty());
    assert!(
        !aligner.is_armed(),
        "no card looks armed, every chip shows the idle hint"
    );
    let before = aligner.value().clone();
    assert_eq!(aligner.place_selection(0), Err(EditError::NothingToPlace));
    assert_eq!(aligner.place_selection(4), Err(EditError::NothingToPlace));
    assert_eq!(aligner.value(), &before);
}

#[test]
fn b1_a_suggestion_moved_one_of_two_selected_words_so_a_click_places_only_the_other() {
    let mut aligner = sample_open(&["escogidos", "fe"], true);
    assert_eq!(texts(&aligner.selection()), ["fe"]);
    assert!(aligner.is_armed());
    assert_eq!(aligner.place_selection(0), Ok(1));
    assert_eq!(
        texts(&aligner.value().alignments[0].bottom_words),
        ["Pablo", "fe"]
    );
    // escogidos stays in the bank, still proposed for its own card.
    assert!(
        aligner
            .value()
            .word_bank
            .iter()
            .any(|w| w.word == "escogidos")
    );
    assert_eq!(aligner.proposals().len(), 1);
}

#[test]
fn b2_a_drag_and_a_card_click_with_the_same_shown_selection_place_the_same_words() {
    for suggested in [true, false] {
        let dragging = sample_open(&["escogidos", "fe"], suggested);
        let shown = dragging.selection();
        assert_eq!(
            texts(&shown),
            if suggested {
                vec!["fe"]
            } else {
                vec!["fe", "escogidos"]
            }
        );
        let carried = dragging.dragged_words(at(&dragging, "fe"));
        assert_eq!(carried, shown);
        let mut dropped = dragging;
        dropped.place_many(4, &carried).unwrap();
        let mut clicked = sample_open(&["escogidos", "fe"], suggested);
        clicked.place_selection(4).unwrap();
        assert_eq!(clicked.value(), dropped.value(), "suggested: {suggested}");
    }
}

#[test]
fn b3_a_selection_with_no_suggestion_places_what_linking_each_word_places_and_clears() {
    let mut aligner = sample_open(&["fe", "escogidos"], false);
    assert_eq!(texts(&aligner.selection()), ["fe", "escogidos"]);
    assert!(aligner.is_armed());
    assert_eq!(aligner.place_selection(4), Ok(2));
    assert_eq!(
        texts(&aligner.value().alignments[4].bottom_words),
        ["fe", "escogidos"]
    );
    let mut by_hand = sample_open(&[], false);
    for word in ["fe", "escogidos"] {
        let w = by_hand.shown_bank()[at(&by_hand, word)].clone();
        by_hand.place(4, &w).unwrap();
    }
    assert_eq!(aligner.value(), by_hand.value());
    assert!(aligner.selection().is_empty() && !aligner.is_armed());
}

// ---- #735 repeated words carry their occurrence numbers ----

#[test]
fn a_repeated_word_shows_its_occurrence_and_is_named_word_n() {
    let verse = TargetVerse::new("Pablo, siervo de Dios y apóstol de Jesucristo, de la fe");
    let third_de = &verse.words()[8];
    assert_eq!((third_de.word.as_str(), third_de.occurrence), ("de", 3));
    assert_eq!(third_de.shown_occurrence(), Some(3));
    assert_eq!(third_de.occurrence_name().as_deref(), Some("de 3"));
    let pablo = &verse.words()[0];
    assert_eq!(pablo.shown_occurrence(), None);
    assert_eq!(pablo.occurrence_name(), None);

    let kai = |occurrence, occurrences| SourceWord {
        word: "καὶ".to_owned(),
        strong: None,
        lemma: None,
        morph: None,
        occurrence,
        occurrences,
        extra: tc_journal::json::JsonMap::new(),
    };
    assert_eq!(kai(2, 2).occurrence_name().as_deref(), Some("καὶ 2"));
    assert_eq!(kai(1, 1).shown_occurrence(), None);
}
