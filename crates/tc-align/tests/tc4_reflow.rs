//! Keeping alignments across a text edit: tC4's `test/align-reflow.test.ts` (#213),
//! ported, and `fixtures/align/reflow-vectors.json` (tC4's `reflowAlignment` on en_ult
//! Titus records after seeded edits).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use tc_align::{
    AlignmentValue, NoReflow, SourceVerse, SourceWord, TargetVerse, TargetWord, WordId, reflow,
};

fn greek(text: &str, strong: &str, lemma: &str) -> SourceWord {
    SourceWord {
        content: text.to_owned(),
        strong: Some(strong.to_owned()),
        lemma: Some(lemma.to_owned()),
        morph: Some("Gr,N".to_owned()),
        occurrence: 1,
        occurrences: 1,
        extra: Vec::new(),
    }
}

const TEXT: &str = "Pablo, siervo de Dios y de Dios";

/// Pablo on Παῦλος, siervo on δοῦλος and Dios(2) on θεοῦ.
fn linked() -> AlignmentValue {
    let source = SourceVerse::new(vec![
        greek("Παῦλος", "G39720", "Παῦλος"),
        greek("δοῦλος", "G14010", "δοῦλος"),
        greek("θεοῦ", "G23160", "θεός"),
    ]);
    let verse = TargetVerse::new(TEXT);
    let mut value = AlignmentValue::bootstrap(&source, &verse, basis(1));
    for (card, text, occurrence) in [(0, "Pablo", 1), (1, "siervo", 1), (2, "Dios", 2)] {
        let word = value
            .word_bank
            .iter()
            .find(|w| w.text == text && w.occurrence == occurrence)
            .cloned()
            .unwrap();
        value.link(card, &word).unwrap();
    }
    value.settle(&verse);
    value
}

fn placed(value: &AlignmentValue) -> Vec<Vec<String>> {
    value.alignments.iter().map(|g| show(&g.targets)).collect()
}

#[test]
fn a_link_whose_target_word_is_still_present_is_kept_and_a_new_word_is_banked() {
    let before = linked();
    let text = "Pablo, siervo de Dios y apóstol de Dios";
    let next = reflow(&before, &TargetVerse::new(text)).unwrap();
    assert_eq!(
        placed(&next.value),
        [["Pablo/1/1"], ["siervo/1/1"], ["Dios/2/2"]]
    );
    assert_eq!(
        show(&next.value.word_bank),
        ["de/1/2", "Dios/1/2", "y/1/1", "apóstol/1/1", "de/2/2"]
    );
    // The source side is untouched, and the value now vouches for the new text.
    let sources = |v: &AlignmentValue| {
        v.alignments
            .iter()
            .map(|g| g.sources.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(sources(&next.value), sources(&before));
    assert!(!next.value.marked_invalid);
    assert!(!next.value.is_stale_against(&TargetVerse::new(text)));
    assert!(next.dropped.is_empty());
}

#[test]
fn a_changed_word_loses_its_link_and_its_replacement_is_banked() {
    let next = reflow(
        &linked(),
        &TargetVerse::new("Pablo, esclavo de Dios y de Dios"),
    )
    .unwrap();
    assert_eq!(
        placed(&next.value),
        [vec!["Pablo/1/1"], vec![], vec!["Dios/2/2"]]
    );
    let bank = show(&next.value.word_bank);
    assert!(bank.contains(&"esclavo/1/1".to_owned()));
    assert!(!bank.contains(&"siervo/1/1".to_owned()));
    // The dropped link is reported, with its card and source word.
    assert_eq!(next.dropped.len(), 1);
    assert_eq!(next.dropped[0].card, 1);
    assert_eq!(next.dropped[0].sources[0].content, "δοῦλος");
    assert_eq!(next.dropped[0].word.text, "siervo");
}

#[test]
fn occurrence_numbers_are_recomputed_for_the_new_text() {
    // Removing the first "Dios" makes the old Dios(2) vanish (identity is text and
    // occurrence); the surviving "Dios" is Dios(1), total 1, in the bank.
    let next = reflow(&linked(), &TargetVerse::new("Pablo, siervo de y de Dios")).unwrap();
    assert_eq!(
        placed(&next.value),
        [vec!["Pablo/1/1"], vec!["siervo/1/1"], vec![]]
    );
    assert!(show(&next.value.word_bank).contains(&"Dios/1/1".to_owned()));
    // A kept word whose total changed carries the new total.
    let more = reflow(
        &linked(),
        &TargetVerse::new("Pablo, siervo de Dios y de Dios de Dios"),
    )
    .unwrap();
    assert_eq!(placed(&more.value)[2], ["Dios/2/3"]);
}

#[test]
fn nothing_to_reflow_leaves_the_value_as_it_is() {
    let value = linked();
    assert_eq!(
        reflow(&value, &TargetVerse::new(TEXT)),
        Err(NoReflow::Current)
    );
    let mut flagged = value.clone();
    flagged.marked_invalid = true;
    assert_eq!(
        reflow(&flagged, &TargetVerse::new("Pablo, siervo")),
        Err(NoReflow::MarkedInvalid)
    );
    let source = SourceVerse::new(vec![greek("Παῦλος", "G39720", "Παῦλος")]);
    let fresh = AlignmentValue::bootstrap(&source, &TargetVerse::new(TEXT), basis(1));
    assert_eq!(
        reflow(&fresh, &TargetVerse::new("Pablo, siervo")),
        Err(NoReflow::NothingPlaced)
    );
    assert_eq!(
        reflow(&value, &TargetVerse::new("___")),
        Err(NoReflow::NoWords)
    );
    assert_eq!(
        reflow(&value, &TargetVerse::new("")),
        Err(NoReflow::NoWords)
    );
}

#[test]
fn reflow_never_creates_a_link_the_translator_did_not_make() {
    let before = linked();
    let next = reflow(
        &before,
        &TargetVerse::new("Pablo, siervo de Dios y de Dios y Pablo"),
    )
    .unwrap();
    assert!(next.value.placed() <= before.placed());
    assert!(show(&next.value.word_bank).contains(&"Pablo/2/2".to_owned()));
}

/// Every placed word is either still placed (by text and occurrence) or reported
/// dropped, and the value holds every word of the new text exactly once.
fn assert_no_loss(
    before: &AlignmentValue,
    after: &tc_align::Reflowed,
    verse: &TargetVerse,
    at: &str,
) {
    assert_eq!(
        before.placed(),
        after.value.placed() + after.dropped.len(),
        "{at}"
    );
    for drop in &after.dropped {
        assert!(
            before.alignments[drop.card].targets.contains(&drop.word),
            "{at}"
        );
        assert!(verse.position(&drop.word).is_none(), "{at}");
    }
    let mut held: Vec<TargetWord> = after.value.all_target_words().cloned().collect();
    let mut words = verse.words().to_vec();
    let key = |w: &TargetWord| (w.text.clone(), w.occurrence);
    held.sort_by_key(key);
    words.sort_by_key(key);
    assert_eq!(held, words, "{at}");
}

#[test]
fn reflow_matches_tc4_on_edited_en_ult_verses_and_drops_nothing_silently() {
    let mut reflowed = 0;
    let mut refused = 0;
    let mut dropped = 0;
    for case in vectors("reflow-vectors.json") {
        let reference = case["ref"].as_str().unwrap();
        let stored = value_of(&case["record"]);
        for edit in case["edits"].as_array().unwrap() {
            let name = edit["edit"].as_str().unwrap();
            let at = format!("{reference} {name}");
            let mut value = stored.clone();
            value.marked_invalid = edit["invalid"].as_bool().unwrap_or(false);
            let verse = TargetVerse::new(edit["text"].as_str().unwrap());
            let ours = reflow(&value, &verse);
            let expected = &edit["result"];
            if expected.is_null() {
                let reason = ours.expect_err(&at);
                let wanted = match name {
                    "unchanged" => NoReflow::Current,
                    "no words" => NoReflow::NoWords,
                    "marked invalid" => NoReflow::MarkedInvalid,
                    other => panic!("{at}: tC4 left {other} alone"),
                };
                assert_eq!(reason, wanted, "{at}");
                refused += 1;
                continue;
            }
            let ours = ours.unwrap_or_else(|e| panic!("{at}: {e:?}"));
            let theirs = value_of(expected);
            assert_eq!(cards(&ours.value), cards(&theirs), "{at}");
            assert_eq!(ours.value.word_bank, theirs.word_bank, "{at}");
            assert_eq!(ours.value.done, theirs.done, "{at}");
            assert_eq!(ours.value.target_verse_md5, theirs.target_verse_md5, "{at}");
            assert!(!ours.value.marked_invalid, "{at}");
            assert_no_loss(&value, &ours, &verse, &at);
            dropped += ours.dropped.len();
            reflowed += 1;
        }
    }
    assert_eq!((reflowed, refused), (16 * 4, 16 * 2 + 1));
    assert!(dropped > 0);
}

#[test]
fn a_word_held_on_two_cards_is_kept_on_the_first_and_reported_dropped_from_the_second() {
    // tC4 would keep both links; one word on two cards is one word aligned twice, which
    // the aligned USFM cannot write, so the second link is reported, not kept.
    let mut twice = linked();
    let pablo = twice.alignments[0].targets[0].clone();
    twice.alignments[2].targets.push(pablo.clone());
    assert!(twice.well_formed().is_err());
    let next = reflow(
        &twice,
        &TargetVerse::new("Pablo, siervo de Dios y apóstol de Dios"),
    )
    .unwrap();
    assert_eq!(
        next.value.alignments[0].targets,
        std::slice::from_ref(&pablo)
    );
    assert!(!next.value.alignments[2].targets.contains(&pablo));
    assert_eq!(next.dropped.len(), 1);
    assert_eq!((next.dropped[0].card, &next.dropped[0].word), (2, &pablo));
    assert_eq!(next.value.well_formed(), Ok(()));
    assert_eq!(twice.placed(), next.value.placed() + next.dropped.len());
}

#[test]
fn a_word_keeps_its_extra_fields_across_a_reflow_while_it_stays_in_the_verse() {
    let mut before = linked();
    let mut typed = serde_json::Map::new();
    typed.insert("type".into(), "bottomWord".into());
    for word in before.all_target_words().cloned().collect::<Vec<_>>() {
        before.word_extra.insert(WordId::of(&word), typed.clone());
    }
    // "siervo" is gone: its link drops and its fields go with it.
    let next = reflow(
        &before,
        &TargetVerse::new("Pablo, esclavo de Dios y de Dios"),
    )
    .unwrap();
    let keys = |v: &AlignmentValue| -> Vec<String> {
        v.word_extra
            .keys()
            .map(|k| format!("{}{}", k.text, k.occurrence))
            .collect()
    };
    assert!(keys(&before).contains(&"siervo1".to_owned()));
    assert!(!keys(&next.value).contains(&"siervo1".to_owned()));
    assert!(keys(&next.value).contains(&"Pablo1".to_owned()));
    assert!(
        keys(&next.value).contains(&"de2".to_owned()),
        "banked words keep theirs"
    );
    assert_eq!(next.value.well_formed(), Ok(()));
}
