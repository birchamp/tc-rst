//! One verse open in the Align workspace, as tC4's J5 journey drives it
//! (`e2e/j05-align-verse`): place and unplace words, Mark valid, and suggestions that are
//! proposals until accepted (D72).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use tc_align::Alignment;

use common::*;
use tc_align::suggest::{self, Model, RawLink};
use tc_align::{Aligner, AlignmentValue, MarkValidRefused, TargetVerse, TargetWord};
use tc_core::Testament;
use tc_journal::json::Json;

/// TIT 1:1 of en_ult as a draft, against UGNT TIT 1:1.
fn open(stored: Option<&AlignmentValue>, text: &str) -> Aligner {
    Aligner::open(
        stored,
        None,
        TargetVerse::new(text),
        &source_verse(UGNT, "1:1"),
        basis(1),
    )
    .unwrap()
}

const TEXT: &str = "Paul, a servant of God and an apostle of Jesus Christ";

fn banked(aligner: &Aligner, text: &str) -> TargetWord {
    aligner
        .value()
        .word_bank
        .iter()
        .find(|w| w.word == text)
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
        aligner.value().alignments[0].bottom_words,
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
        .position(|s| s.text == aligner.value().alignments[card].top_words[0].word)
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
fn clear_discards_standing_suggestions() {
    let mut aligner = open(None, TEXT);
    proposal_for(&mut aligner, 0, "Paul");
    assert_eq!(aligner.proposals().len(), 1);
    aligner.clear();
    assert!(aligner.proposals().is_empty());
    assert_eq!(aligner.shown_bank().len(), aligner.value().word_bank.len());
}

#[test]
fn a_saved_value_is_not_saved_again_until_it_changes() {
    let mut aligner = open(None, TEXT);
    aligner.place(0, &banked(&aligner, "Paul")).unwrap();
    assert!(aligner.is_changed());
    aligner.mark_saved();
    assert!(!aligner.is_changed());
    aligner.unplace(0, &banked_on_card(&aligner, 0)).unwrap();
    assert!(aligner.is_changed());
}

fn banked_on_card(aligner: &Aligner, card: usize) -> TargetWord {
    aligner.value().alignments[card].bottom_words[0].clone()
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
    assert_eq!(
        aligner.value().alignments[0].bottom_words[0].word,
        "servant"
    );
}

/// Paul and servant placed on TIT 1:1, saved against `TEXT`.
fn paul_and_servant() -> AlignmentValue {
    let mut first = open(None, TEXT);
    first.place(0, &banked(&first, "Paul")).unwrap();
    first.place(1, &banked(&first, "servant")).unwrap();
    first.into_value()
}

fn sorted_words(words: impl IntoIterator<Item = TargetWord>) -> Vec<(String, u32)> {
    let mut out: Vec<_> = words.into_iter().map(|w| (w.word, w.occurrence)).collect();
    out.sort();
    out
}

#[test]
fn a_stored_alignment_made_for_other_text_opens_reflowed_and_says_what_it_dropped() {
    let stored = paul_and_servant();
    let edited = "Paul, a slave of God and an apostle of Jesus Christ";
    let mut aligner = open(Some(&stored), edited);
    assert!(
        aligner.is_stale(),
        "the draft changed since the alignment was made"
    );
    assert_eq!(aligner.value().placed(), 1);
    assert_eq!(aligner.dropped().len(), 1);
    assert_eq!(aligner.dropped()[0].word.word, "servant");
    assert_eq!(aligner.dropped()[0].top_words[0].word, "δοῦλος");
    assert!(
        !aligner.is_changed(),
        "a carry-over that dropped a link is not saved until the translator acts"
    );
    // The stored value is untouched: it stays as it was, kept, and computes invalid.
    assert_eq!(stored.placed(), 2);
    assert!(stored.is_stale_against(&TargetVerse::new(edited)));
    aligner.keep_reflow();
    assert!(aligner.is_changed());
    assert!(!aligner.is_stale(), "kept: the notice clears");

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
    assert!(aligner.is_stale(), "saved on leaving, and shown as changed");
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
    stored.span_end = tc_journal::ids::VerseNo::new(2).unwrap();
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
        assert_ne!(p.word.word, "Paul", "a placed word is never proposed");
    }
}

#[test]
fn a_verse_with_no_original_words_or_no_target_words_does_not_open() {
    use tc_align::{SourceVerse, Unavailable};
    let refused = |verse: &str, source: &SourceVerse| {
        Aligner::open(None, None, TargetVerse::new(verse), source, basis(1)).unwrap_err()
    };
    // tC4 shows its "missing" notice: no cards to place on, and no Mark valid that
    // vouches for a verse nothing could be aligned in.
    assert_eq!(
        refused(TEXT, &SourceVerse::default()),
        Unavailable::NoSource
    );
    assert_eq!(
        refused("", &source_verse(UGNT, "1:1")),
        Unavailable::NoWords
    );
    assert_eq!(
        refused("— .", &source_verse(UGNT, "1:1")),
        Unavailable::NoWords
    );
    let stored = paul_and_servant();
    assert_eq!(
        Aligner::open(
            Some(&stored),
            None,
            TargetVerse::new(TEXT),
            &SourceVerse::default(),
            basis(1)
        )
        .unwrap_err(),
        Unavailable::NoSource
    );
}

/// Owner ruling Q37 (tC4 #814): a record flagged for re-review opens as it is, keeps the flag
/// through an edit, and only Mark valid clears it.
#[test]
fn a_record_flagged_for_re_review_opens_as_it_is_and_keeps_the_flag_until_mark_valid() {
    let mut flagged = paul_and_servant();
    flagged.marked_invalid = true;
    let mut aligner = open(Some(&flagged), TEXT);
    assert_eq!(aligner.value(), &flagged);
    aligner.place(2, &banked(&aligner, "God")).unwrap();
    assert!(aligner.value().marked_invalid, "an edit keeps the flag");
    assert!(!aligner.value().done);
    aligner.mark_valid().unwrap();
    assert!(!aligner.value().marked_invalid, "Mark valid clears it");
}

#[test]
fn a_stored_word_held_in_two_places_opens_in_one() {
    // Placed on card 0 and also left in the bank: the bank copy goes, nothing is lost.
    let mut both = paul_and_servant();
    let paul = both.alignments[0].bottom_words[0].clone();
    both.word_bank.push(paul.clone());
    let mut aligner = open(Some(&both), TEXT);
    assert!(!aligner.value().word_bank.contains(&paul));
    assert_eq!(
        aligner.value().alignments[0].bottom_words,
        std::slice::from_ref(&paul)
    );
    assert!(aligner.dropped().is_empty());
    assert!(
        !aligner.is_changed(),
        "the stored value is not rewritten unasked"
    );
    assert!(aligner.place(1, &paul).is_err(), "never on a second card");
    assert_eq!(
        aligner.value().total(),
        TargetVerse::new(TEXT).words().len()
    );
    assert!(tc_align::usfm::weave_verse(TEXT, aligner.value()).is_ok());

    // Placed on two cards: kept on the first, the second link reported.
    let mut twice = paul_and_servant();
    twice.alignments[2].bottom_words.push(paul.clone());
    let aligner = open(Some(&twice), TEXT);
    assert_eq!(aligner.dropped().len(), 1);
    assert_eq!(
        (aligner.dropped()[0].card, &aligner.dropped()[0].word),
        (2, &paul)
    );
    assert_eq!(aligner.value().well_formed(), Ok(()));
}

#[test]
fn clear_after_a_carry_over_puts_the_bank_back_in_verse_order() {
    let mut first = open(None, "the cat saw the dog");
    first.place(0, &banked(&first, "cat")).unwrap();
    let stored = first.into_value();
    let edited = "the cat saw the big dog";
    let mut aligner = open(Some(&stored), edited);
    assert_eq!(aligner.value().placed(), 1);
    aligner.clear();
    assert_eq!(aligner.value().word_bank, TargetVerse::new(edited).words());
    assert_eq!(aligner.value().placed(), 0);
}

#[test]
fn a_verse_with_no_alignment_shows_its_bank_in_verse_order_from_the_first_view() {
    // tC4's record of an unaligned verse banks every first occurrence before every
    // second one; the owner's Q15 ruling shows the bank in verse order from the start.
    let text = "the cat saw the dog";
    let verse = TargetVerse::new(text);
    let aligner = open(None, text);
    assert_eq!(aligner.value().word_bank, verse.words());
    assert_ne!(verse.bank_order(), verse.words());
    assert!(!aligner.is_changed(), "the order alone is not saved");
}

#[test]
fn accept_all_places_every_standing_proposal_as_clicks_would() {
    let mut aligner = open(None, TEXT);
    let input = aligner.session_input();
    let at = |word: &str| input.target().iter().position(|t| t.text == word).unwrap();
    let raw: Vec<RawLink> = [(0, "Paul"), (1, "servant"), (2, "God")]
        .into_iter()
        .map(|(source, word)| RawLink {
            source: vec![source],
            target: vec![at(word)],
            confidence: 0.9,
        })
        .collect();
    aligner.show_suggestions(&raw);
    assert_eq!(aligner.proposals().len(), 3);
    let mut by_hand = open(None, TEXT);
    for (card, word) in [(0, "Paul"), (1, "servant"), (2, "God")] {
        by_hand.place(card, &banked(&by_hand, word)).unwrap();
    }
    assert_eq!(aligner.accept_all(), 3);
    assert_eq!(aligner.value(), by_hand.value());
    assert!(aligner.proposals().is_empty());
    aligner.mark_valid().unwrap();
}

#[test]
fn equal_verses_and_aligners_print_alike() {
    // A verse indexes where its words stand, for the verse-order sort after each edit.
    // That index is not part of the verse: two equal verses, and two aligners opened on
    // the same verse, print the same in an assertion message on every run.
    let text = "a b c d e f g h i j k l m n o p";
    let verse = TargetVerse::new(text);
    for _ in 0..4 {
        let again = TargetVerse::new(text);
        assert_eq!(again, verse);
        assert_eq!(format!("{again:?}"), format!("{verse:?}"));
    }
    assert_eq!(
        format!("{:?}", open(None, TEXT)),
        format!("{:?}", open(None, TEXT))
    );
}

#[test]
fn a_shown_word_the_hash_leaves_out_is_carried_over_when_it_changes() {
    // `plain_text_v1` leaves out the text of a `\qs` written over lines as the ULT writes
    // it, so the hash of these two verses is the same; the aligner shows and banks the
    // Selah all the same, as tC4 does.
    let before = "Paul, a servant of God\n\\qs\nSelah\n\\qs*\n";
    let after = "Paul, a servant of God\n\\qs\nPause\n\\qs*\n";
    assert_eq!(
        TargetVerse::new(before).md5(),
        TargetVerse::new(after).md5()
    );
    let mut aligner = open(None, before);
    assert!(aligner.verse().text().ends_with("Selah"));
    let selah = banked(&aligner, "Selah");
    aligner.place(0, &selah).unwrap();
    let stored = aligner.value().clone();

    let reopened = open(Some(&stored), after);
    let held: Vec<&str> = reopened
        .value()
        .all_target_words()
        .map(|w| w.word.as_str())
        .collect();
    assert!(held.contains(&"Pause") && !held.contains(&"Selah"));
    assert_eq!(reopened.dropped().len(), 1);
    assert_eq!(reopened.dropped()[0].word, selah);
    assert!(!reopened.is_changed(), "leaving keeps the stored value");
}

#[test]
fn a_source_book_gives_every_verse_the_words_from_book_gives() {
    let text = read(UGNT);
    let book = tc_usfm::Book::parse(&text);
    let cut = tc_align::SourceBook::parse(&text);
    let mut seen = 0;
    for segment in book.segments() {
        let Some(unit) = segment.as_verse() else {
            continue;
        };
        let (Some(chapter), Some(verses)) = (unit.chapter(), unit.verses()) else {
            continue;
        };
        assert_eq!(
            cut.verse(chapter, verses).unwrap(),
            tc_align::SourceVerse::from_book(&book, chapter, verses).unwrap(),
            "{chapter}:{verses:?}"
        );
        assert!(cut.has(chapter, verses));
        seen += 1;
    }
    assert!(seen > 40, "{seen} verses of Titus");
    let span = tc_core::VerseSpan::range(1, 2).unwrap();
    assert_eq!(
        cut.verse(1, span).unwrap().words().len(),
        source_verse(UGNT, "1:1").words().len() + source_verse(UGNT, "1:2").words().len()
    );
    assert!(!cut.display_text(1, span).is_empty());
    assert!(!cut.has(9, span));
}

/// tC4 #409: a span aligned verse by verse numbers each source word within its own verse and
/// names that verse in `x-ref`. Such a value, from a file added as a book, opens with every link
/// where it was: the pin fit (#790) numbers words across the span, and would read 1:3's "Θεοῦ"
/// (the first of its verse, the third of the span) as 1:1's.
#[test]
fn an_imported_span_with_x_ref_opens_with_every_link() {
    let source = source_verse(UGNT, "1:1-3");
    let text = "Paul, a servant of God, revealed his word at the right time by the command of God";
    let open_span = |stored: Option<&AlignmentValue>| {
        Aligner::open(stored, None, TargetVerse::new(text), &source, basis(3)).unwrap()
    };
    let card_of = |aligner: &Aligner, word: &str, occurrence: u32| {
        aligner
            .value()
            .alignments
            .iter()
            .position(|g| {
                g.top_words
                    .iter()
                    .any(|w| w.word == word && w.occurrence == occurrence)
            })
            .unwrap()
    };
    let mut aligner = open_span(None);
    for (on, at, said, n) in [
        ("Παῦλος", 1, "Paul", 1),
        ("Θεοῦ", 1, "God", 1),
        ("Θεοῦ", 3, "God", 2),
    ] {
        let card = card_of(&aligner, on, at);
        let word = aligner
            .value()
            .word_bank
            .iter()
            .find(|w| w.word == said && w.occurrence == n)
            .cloned()
            .unwrap();
        aligner.place(card, &word).unwrap();
    }
    let mut stored = aligner.into_value();
    // 1:3's words as a verse-by-verse aligner writes them.
    let before_1_3 =
        source_verse(UGNT, "1:1").words().len() + source_verse(UGNT, "1:2").words().len();
    let own = source_verse(UGNT, "1:3");
    for group in &mut stored.alignments {
        for word in &mut group.top_words {
            let at = source
                .words()
                .iter()
                .position(|s| s.word == word.word && s.occurrence == word.occurrence)
                .unwrap();
            if let Some(mine) = at.checked_sub(before_1_3).map(|i| &own.words()[i]) {
                word.occurrence = mine.occurrence;
                word.occurrences = mine.occurrences;
                word.extra.insert("x-ref".to_owned(), Json::from("1:3"));
            }
        }
    }
    let reopened = open_span(Some(&stored));
    assert!(reopened.dropped().is_empty(), "{:?}", reopened.dropped());
    assert_eq!(reopened.value().alignments, stored.alignments);
    assert_eq!(reopened.value().placed(), 3);
    assert!(!reopened.is_changed(), "nothing to save");
}
