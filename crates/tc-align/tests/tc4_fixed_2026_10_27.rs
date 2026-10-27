//! tC4 Align issues fixed in tC4 before 2026-10-27, replayed on the rebuild's aligner: a verse
//! opened as the app opens it (`Aligner::open` with no earlier text, the stored record and the
//! pinned original-language verse), the edits a translator makes, and the reflow a Translate
//! save runs. Each test follows the tC4 issue's own steps on the fixtures.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::*;
use tc_align::suggest::{Model, RawLink, TrainingVerse};
use tc_align::usfm::weave_verse;
use tc_align::{
    Aligner, Alignment, AlignmentValue, Click, EditError, NoReflow, SourceBook, SourceVerse,
    SourceWord, TargetVerse, TargetWord, Unavailable, VerseState, needs_review, reflow,
    reflow_edit, verse_status,
};
use tc_core::{Testament, VerseNo, VerseSpan};
use unicode_normalization::UnicodeNormalization;

/// The pinned UGNT TIT 1:1, as the app hands it to the aligner.
fn ugnt() -> SourceVerse {
    source_verse(UGNT, "1:1")
}

/// The verse opened as the app opens it: no earlier text (`tc-app` `align/open.rs`).
fn open(stored: Option<&AlignmentValue>, text: &str, source: &SourceVerse) -> Aligner {
    Aligner::open(stored, None, TargetVerse::new(text), source, basis(1)).unwrap()
}

fn bank_texts(aligner: &Aligner) -> Vec<String> {
    aligner
        .shown_bank()
        .into_iter()
        .map(|w| w.word)
        .collect::<Vec<_>>()
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

fn bank_index(aligner: &Aligner, text: &str) -> usize {
    aligner
        .shown_bank()
        .iter()
        .position(|w| w.word == text)
        .unwrap_or_else(|| panic!("{text} is not shown in the bank"))
}

/// The card holding the original word `word` with `occurrence`.
fn card_of(value: &AlignmentValue, word: &str, occurrence: u32) -> usize {
    value
        .alignments
        .iter()
        .position(|g| {
            g.top_words
                .iter()
                .any(|s| s.word == word && s.occurrence == occurrence)
        })
        .unwrap_or_else(|| panic!("no card for {word} {occurrence}"))
}

fn on_card(value: &AlignmentValue, card: usize) -> Vec<String> {
    show(&value.alignments[card].bottom_words)
}

/// Every word of the verse placed, one per card, round robin; the rest on the last card.
fn place_every_word(aligner: &mut Aligner) {
    let cards = aligner.value().alignments.len();
    let mut card = 0;
    while let Some(word) = aligner.value().word_bank.first().cloned() {
        aligner.place(card.min(cards - 1), &word).unwrap();
        card += 1;
    }
}

// ---- #768 ----

/// tC4 #768 "Align cards lose their English label after a save: saved Hebrew is NFC, the ULT
/// is not". Steps: Genesis 1:1 with hbo_uhb, align it, save (tC4's seal makes the Hebrew card
/// words NFC), leave, come back: every card must still be the pinned word, with its link, so
/// that the gateway label (tc-app `Gateway::label`, which compares in NFC) finds its ULT words.
/// The rebuild's store keeps words as given, so an NFC record reaches it from a tC4 4.0
/// project opened from Door43; the test makes the record NFC as tC4's seal does.
#[test]
fn tc4_768_genesis_1_1_saved_with_nfc_hebrew_reopens_with_every_card_and_link() {
    let hebrew = SourceBook::parse(&read("fixtures/tc4/test-fixtures/hbo_uhb/GEN-1-1.usfm"))
        .verse(1, VerseSpan::single(VerseNo::new(1).unwrap()))
        .unwrap();
    let nfc = |s: &str| s.nfc().collect::<String>();
    assert!(
        hebrew.words().iter().any(|w| nfc(&w.word) != w.word),
        "the pinned Hebrew is not all NFC, or the test shows nothing"
    );
    let text = "In the beginning God created the heavens and the earth";
    let mut aligner = open(None, text, &hebrew);
    let cards = aligner.value().alignments.len();
    assert_eq!(cards, hebrew.words().len());
    place_every_word(&mut aligner);
    let mut saved = aligner.into_value();
    for word in saved
        .alignments
        .iter_mut()
        .flat_map(|g| g.top_words.iter_mut())
    {
        word.word = nfc(&word.word);
    }
    let back = open(Some(&saved), text, &hebrew);
    assert!(back.dropped().is_empty(), "{:?}", back.dropped());
    assert!(!back.is_stale());
    assert_eq!(back.value().alignments.len(), cards);
    assert_eq!(back.value().placed(), saved.placed());
    // Each card is still its pinned word (compared in NFC, as the label compares).
    for (card, pinned) in back.value().alignments.iter().zip(hebrew.words()) {
        assert_eq!(nfc(&card.top_words[0].word), nfc(&pinned.word));
        assert_eq!(card.top_words[0].occurrence, pinned.occurrence);
    }
}

// ---- #787 ----

/// tC4 #787 "Align: a verse whose text changed opens with its old word bank, and the next
/// edit saves it as current". Steps (sample Titus 1:1): place "Pablo", click it again to
/// unalign; in Translate change "siervo" to "esclavo" (no reflow reaches a record with
/// nothing placed); reopen Align. The bank must hold "esclavo" and not "siervo", opening must
/// write nothing, and after one edit the saved record holds exactly the verse's words. With a
/// link standing, the reopen also shows "The draft changed".
#[test]
fn tc4_787_reopen_after_siervo_became_esclavo_banks_esclavo_not_siervo() {
    let (_, t1) = sample_tit_1_1();
    let t2 = t1.replacen("siervo", "esclavo", 1);
    let mut first = open(None, &t1, &ugnt());
    let pablo = banked(&first, "Pablo");
    first.place(0, &pablo).unwrap();
    first.unplace(0, &pablo).unwrap();
    let unplaced = first.into_value();
    assert_eq!(unplaced.placed(), 0);

    let mut reopened = open(Some(&unplaced), &t2, &ugnt());
    let bank = bank_texts(&reopened);
    assert!(bank.contains(&"esclavo".to_owned()), "{bank:?}");
    assert!(!bank.contains(&"siervo".to_owned()), "{bank:?}");
    assert!(!reopened.is_changed(), "opening writes nothing");
    let esclavo = banked(&reopened, "esclavo");
    reopened.place(1, &esclavo).unwrap();
    let saved = reopened.value();
    let mut held: Vec<String> = show(&saved.all_target_words().cloned().collect::<Vec<_>>());
    let mut verse: Vec<String> = show(TargetVerse::new(&t2).words());
    held.sort();
    verse.sort();
    assert_eq!(
        held, verse,
        "the saved record holds exactly the verse's words"
    );
    assert!(weave_verse(&t2, saved).is_ok());

    // With "Pablo" placed, the same edit of the text: the reopen says the draft changed.
    let mut linked = open(None, &t1, &ugnt());
    linked.place(0, &banked(&linked, "Pablo")).unwrap();
    let linked = linked.into_value();
    let reopened = open(Some(&linked), &t2, &ugnt());
    assert!(reopened.is_stale(), "The draft changed shows");
    assert!(bank_texts(&reopened).contains(&"esclavo".to_owned()));
    assert!(!bank_texts(&reopened).contains(&"siervo".to_owned()));
}

/// tC4 #787, second defect: "A verse that was never saved opens with its bank in word-aligner
/// unmerge order: repeated words go to the end. Titus 1:1 ends '… piedad de Dios y la de la de
/// de'." The bank of a never-saved verse must be in verse order.
#[test]
fn tc4_787_a_never_saved_titus_1_1_opens_with_its_bank_in_verse_order() {
    let (_, t1) = sample_tit_1_1();
    let aligner = open(None, &t1, &ugnt());
    let in_verse: Vec<String> = TargetVerse::new(&t1)
        .words()
        .iter()
        .map(|w| w.word.clone())
        .collect();
    assert_eq!(bank_texts(&aligner), in_verse);
}

// ---- #788 ----

/// tC4 #788 "Align: a card click places a selected word that a suggestion moved". Steps
/// (sample Titus 1:1): select "Pablo", the suggestion puts "Pablo" on Παῦλος, click another
/// card. Nothing may be placed and the cards must not look armed. With "Pablo" and "siervo"
/// selected, a drag and a card click place the same shown words.
#[test]
fn tc4_788_a_card_click_places_no_word_a_suggestion_moved() {
    let (_, t1) = sample_tit_1_1();
    let pablo_on_paulos = RawLink {
        source: vec![0],
        target: vec![0],
        confidence: 0.9,
    };
    let mut aligner = open(None, &t1, &ugnt());
    aligner.click_bank_word(bank_index(&aligner, "Pablo"), Click::Plain);
    assert!(aligner.is_armed());
    aligner.show_suggestions(std::slice::from_ref(&pablo_on_paulos));
    assert_eq!(aligner.proposals().len(), 1);
    assert_eq!(aligner.proposals()[0].word.word, "Pablo");
    assert!(aligner.selection().is_empty());
    assert!(!aligner.is_armed(), "the cards show no armed border");
    let before = aligner.value().clone();
    assert_eq!(aligner.place_selection(3), Err(EditError::NothingToPlace));
    assert_eq!(aligner.value(), &before, "nothing was placed");

    let mut aligner = open(None, &t1, &ugnt());
    aligner.click_bank_word(bank_index(&aligner, "Pablo"), Click::Plain);
    aligner.click_bank_word(bank_index(&aligner, "siervo"), Click::Toggle);
    aligner.show_suggestions(&[pablo_on_paulos]);
    let shown = show(&aligner.selection());
    assert_eq!(shown, ["siervo/1/1"]);
    let dragged = show(&aligner.dragged_words(bank_index(&aligner, "siervo")));
    assert_eq!(dragged, shown, "a drag carries what a click places");
    assert_eq!(aligner.place_selection(1), Ok(1));
    assert_eq!(on_card(aligner.value(), 1), ["siervo/1/1"]);
    assert!(
        aligner.value().word_bank.iter().any(|w| w.word == "Pablo"),
        "Pablo stays a suggestion, not placed by the click"
    );
}

// ---- #789 ----

/// tC4 #789 "Align: after a verse edit adds or removes an earlier copy of a repeated word, a
/// link moves to another copy with no warning". Steps (sample Titus 1:1, Θεοῦ¹ on the Dios of
/// "siervo de Dios", Θεοῦ² on the Dios of "escogidos de Dios"): delete "de Dios" after
/// "siervo". Here the text changes outside a Translate save (a sync, a kept version, an
/// outside edit), so Align opens the stored record against the new text with no earlier text,
/// as the app does. The Θεοῦ¹ card must never get the other Dios; a link that cannot follow
/// its word must be listed, and the verse must say the draft changed.
#[test]
fn tc4_789_dios_never_moves_to_the_other_theou_card_when_the_text_changes_outside_align() {
    let (stored, t1) = sample_tit_1_1();
    let first = card_of(&stored, "Θεοῦ", 1);
    let second = card_of(&stored, "Θεοῦ", 2);
    assert_eq!(on_card(&stored, first), ["Dios/1/2"]);
    assert_eq!(on_card(&stored, second), ["Dios/2/2"]);
    let t2 = t1.replacen("siervo de Dios", "siervo", 1);

    let aligner = open(Some(&stored), &t2, &ugnt());
    let value = aligner.value();
    let first = card_of(value, "Θεοῦ", 1);
    let second = card_of(value, "Θεοῦ", 2);
    assert!(
        on_card(value, first).is_empty(),
        "Θεοῦ¹ took the escogidos Dios: {:?}",
        on_card(value, first)
    );
    let kept = on_card(value, second);
    assert!(kept.is_empty() || kept == ["Dios/1/1"], "{kept:?}");
    let listed: Vec<_> = aligner
        .dropped()
        .iter()
        .map(|d| (d.top_words[0].word.clone(), d.top_words[0].occurrence))
        .collect();
    assert!(listed.contains(&("Θεοῦ".to_owned(), 1)), "{listed:?}");
    if kept.is_empty() {
        assert!(listed.contains(&("Θεοῦ".to_owned(), 2)), "{listed:?}");
    }
    assert!(aligner.is_stale(), "the verse says the draft changed");
    assert!(
        !aligner.is_changed(),
        "a dropped link waits for the translator"
    );

    // The Translate save's reflow, which has the earlier text: Dios follows its own word.
    let next = reflow_edit(&stored, &TargetVerse::new(&t1), &TargetVerse::new(&t2)).unwrap();
    assert!(on_card(&next.value, first).is_empty());
    assert_eq!(on_card(&next.value, second), ["Dios/1/1"]);
    assert_eq!(next.dropped.len(), 1);
    assert_eq!(next.dropped[0].top_words[0].occurrence, 1);
}

// ---- #790 ----

/// tC4 #790 "Align: stored source cards are never compared with the pinned original-language
/// verse". Steps: open the sample Titus 1:1 (its record holds 10 cards with 11 Greek words;
/// UGNT v0.34 has 17): all 17 must show, in UGNT order, and opening writes nothing. A record
/// whose link stands on a Greek word the pin lacks (Σαῦλος for Παῦλος, as from another source
/// version) loses that link to the bank, listed, with the notice, and the rail shows the
/// verse needs review.
#[test]
fn tc4_790_sample_titus_1_1_shows_all_17_ugnt_words_and_a_foreign_source_link_is_listed() {
    let (stored, t1) = sample_tit_1_1();
    let pin = ugnt();
    assert_eq!(pin.words().len(), 17);
    let aligner = open(Some(&stored), &t1, &pin);
    let shown: Vec<(String, u32)> = aligner
        .value()
        .alignments
        .iter()
        .flat_map(|g| &g.top_words)
        .map(|w| (w.word.clone(), w.occurrence))
        .collect();
    let pinned: Vec<(String, u32)> = pin
        .words()
        .iter()
        .map(|w| (w.word.clone(), w.occurrence))
        .collect();
    assert_eq!(shown, pinned);
    assert_eq!(aligner.value().placed(), stored.placed());
    assert!(!aligner.is_changed(), "opening writes nothing");

    let mut foreign = stored.clone();
    foreign.alignments[0].top_words[0].word = "Σαῦλος".into();
    let aligner = open(Some(&foreign), &t1, &pin);
    assert!(
        aligner
            .value()
            .alignments
            .iter()
            .all(|g| g.top_words.iter().all(|w| w.word != "Σαῦλος"))
    );
    assert!(bank_texts(&aligner).contains(&"Pablo".to_owned()));
    assert_eq!(aligner.dropped().len(), 1);
    assert_eq!(aligner.dropped()[0].word.word, "Pablo");
    assert!(aligner.is_stale());
    let verse = TargetVerse::new(&t1);
    let run = basis(1).aligned_run(&verse);
    assert!(needs_review(&foreign, &run, &verse, Some(&pin)));
}

// ---- #793 ----

/// tC4 #793 "Suggestions: Hebrew source words exist in two Unicode forms, so suggestion
/// memory misses them". Steps: train the memory on saved Jonah 1:1-12 records whose Hebrew
/// is NFC (as tC4's seal stores them), then ask it about Jonah 1:13-17 opened fresh in Align
/// against the pinned hbo_uhb words as they are. It must suggest as well as when asked in NFC.
#[test]
fn tc4_793_memory_trained_on_nfc_jonah_suggests_for_fresh_raw_hbo_uhb_verses() {
    let ult = bundle_entry("en_ult_v91.sb.zip", "en_ult/ingredients/JON.usfm");
    let uhb = SourceBook::parse(&read(UHB));
    let chapter_1: Vec<(u16, String)> = codec_verses(&ult)
        .into_iter()
        .filter_map(|(r, body)| Some((r.strip_prefix("1:")?.parse().ok()?, body)))
        .collect();
    let mut model = Model::empty(Testament::Old);
    for (v, body) in chapter_1.iter().filter(|(v, _)| *v <= 12) {
        let nfc: String = body.nfc().collect();
        if let Some(verse) = TrainingVerse::from_aligned_usfm(format!("JON 1:{v}"), &nfc).unwrap() {
            model.append_verse(&verse, true);
        }
    }
    assert!(model.verses() >= 10);
    let (mut raw_found, mut nfc_found, mut raw_words) = (0, 0, 0);
    for (v, body) in chapter_1.iter().filter(|(v, _)| (13..=17).contains(v)) {
        let plain = tc_usfm::strip_alignment(body).unwrap();
        let raw = uhb
            .verse(1, VerseSpan::single(VerseNo::new(*v).unwrap()))
            .unwrap();
        raw_words += raw
            .words()
            .iter()
            .filter(|w| w.word.nfc().collect::<String>() != w.word)
            .count();
        let in_nfc = SourceVerse::new(
            raw.words()
                .iter()
                .map(|w| SourceWord {
                    word: w.word.nfc().collect(),
                    lemma: w.lemma.as_ref().map(|l| l.nfc().collect()),
                    ..w.clone()
                })
                .collect(),
        );
        let asked = |source: &SourceVerse| {
            let aligner = open(None, &plain, source);
            model.predict(&aligner.session_input()).len()
        };
        raw_found += asked(&raw);
        nfc_found += asked(&in_nfc);
    }
    assert!(raw_words > 0, "the raw Hebrew holds non-NFC words");
    assert!(nfc_found > 0);
    assert_eq!(raw_found, nfc_found);
}

// ---- #794 ----

/// tC4 #794 "Align and export: do not write alignments for a verse that holds imported
/// alignment markup". Steps: the en_ult Titus added as a book keeps `\zaln` inline in 1:1;
/// open it in Align (it must not open: no edit, no Mark valid can reach it), and a Translate
/// save's reflow of it must write no alignment.
#[test]
fn tc4_794_titus_1_1_holding_inline_zaln_opens_no_aligner_and_reflows_nothing() {
    let aligned = verse_body(ULT, "1:1");
    assert_eq!(aligned.matches("\\zaln-s").count(), 19);
    assert_eq!(
        Aligner::open(None, None, TargetVerse::new(&aligned), &ugnt(), basis(1)).unwrap_err(),
        Unavailable::AlignedInText
    );
    let (stored, t1) = sample_tit_1_1();
    let verse = TargetVerse::new(&aligned);
    assert_eq!(reflow(&stored, &verse), Err(NoReflow::AlignedInText));
    assert_eq!(
        reflow_edit(&stored, &TargetVerse::new(&t1), &verse),
        Err(NoReflow::AlignedInText)
    );
}

// ---- #796 ----

/// tC4 #796 "derive the word bank from the words the export checks". Steps: the verse
/// "Pablo, siervo\f + \ft o esclavo\f* de Dios": the bank must show "siervo" and "de" (not
/// "siervode"), and once every word is placed and the verse marked valid the export's weave
/// takes it, keeping the footnote.
#[test]
fn tc4_796_footnote_after_siervo_banks_two_words_and_the_aligned_verse_weaves() {
    let text = "Pablo, siervo\\f + \\ft o esclavo\\f* de Dios";
    let mut aligner = open(None, text, &ugnt());
    let bank = bank_texts(&aligner);
    assert_eq!(bank, ["Pablo", "siervo", "de", "Dios"]);
    place_every_word(&mut aligner);
    aligner.mark_valid().unwrap();
    let value = aligner.value();
    let verse = TargetVerse::new(text);
    let run = basis(1).aligned_run(&verse);
    assert_eq!(
        verse_status(Some(value), Some((&run, &verse)), Some(&ugnt())).state,
        VerseState::Valid
    );
    let woven = weave_verse(text, value).unwrap();
    assert!(woven.contains("\\zaln-s"), "{woven}");
    assert!(woven.contains("\\f + \\ft o esclavo\\f*"), "{woven}");
}

// ---- #820 ----

/// tC4 #820 "Flag ambiguous repeated-word replacements for alignment review". Steps: a verse
/// "Dios Dios"; place only the first Dios; in Translate save the verse as "Dios Pablo". The
/// text cannot say which Dios stayed, so the link must not be kept silently: either the verse
/// is flagged for review or the link goes back to the bank and is listed (the rebuild's
/// declared Q30 (a)).
#[test]
fn tc4_820_dios_dios_first_linked_saved_as_dios_pablo_keeps_no_unsure_link() {
    let greek = |word: &str, occurrence: u32| SourceWord {
        word: word.into(),
        strong: Some("G23160".into()),
        lemma: Some("θεός".into()),
        morph: None,
        occurrence,
        occurrences: 1,
        extra: Default::default(),
    };
    let source = SourceVerse::new(vec![greek("θεοῦ", 1), greek("θεὸς", 1)]);
    let mut aligner = open(None, "Dios Dios", &source);
    let first = aligner
        .value()
        .word_bank
        .iter()
        .find(|w| w.word == "Dios" && w.occurrence == 1)
        .cloned()
        .unwrap();
    aligner.place(0, &first).unwrap();
    let value = aligner.into_value();
    assert_eq!(on_card(&value, 0), ["Dios/1/2"]);

    let next = reflow_edit(
        &value,
        &TargetVerse::new("Dios Dios"),
        &TargetVerse::new("Dios Pablo"),
    )
    .unwrap();
    let kept = next.value.placed() > 0;
    assert!(
        !kept || next.value.marked_invalid,
        "an unsure link was kept silently: {:?}",
        on_card(&next.value, 0)
    );
    if !kept {
        assert_eq!(next.dropped.len(), 1, "the dropped link is listed");
        assert_eq!(
            show(std::slice::from_ref(&next.dropped[0].word)),
            ["Dios/1/2"]
        );
    }
}
