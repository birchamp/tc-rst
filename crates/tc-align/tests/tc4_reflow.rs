//! Keeping alignments across a text edit: tC4's `test/align-reflow.test.ts` (#213),
//! ported, and `fixtures/align/reflow-vectors.json` (tC4's `reflowAlignment` on en_ult
//! Titus records after seeded edits).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use tc_align::Alignment;
use tc_align::bootstrap;

use common::*;
use tc_align::{
    AlignmentValue, NoReflow, SourceVerse, SourceWord, TargetVerse, TargetWord, reflow, reflow_edit,
};

fn greek(text: &str, strong: &str, lemma: &str) -> SourceWord {
    SourceWord {
        word: text.to_owned(),
        strong: Some(strong.to_owned()),
        lemma: Some(lemma.to_owned()),
        morph: Some("Gr,N".to_owned()),
        occurrence: 1,
        occurrences: 1,
        extra: Default::default(),
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
    let mut value = bootstrap(&source, &verse, basis(1));
    for (card, text, occurrence) in [(0, "Pablo", 1), (1, "siervo", 1), (2, "Dios", 2)] {
        let word = value
            .word_bank
            .iter()
            .find(|w| w.word == text && w.occurrence == occurrence)
            .cloned()
            .unwrap();
        value.link(card, &word).unwrap();
    }
    value.settle(&verse);
    value
}

fn placed(value: &AlignmentValue) -> Vec<Vec<String>> {
    value
        .alignments
        .iter()
        .map(|g| show(&g.bottom_words))
        .collect()
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
            .map(|g| g.top_words.clone())
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
    assert_eq!(next.dropped[0].top_words[0].word, "δοῦλος");
    assert_eq!(next.dropped[0].word.word, "siervo");
}

#[test]
fn occurrence_numbers_are_recomputed_for_the_new_text() {
    // Without the earlier text, a verse with one "Dios" fewer does not say which went, so
    // the link of Dios(2) goes back to the bank and is reported (#789, Q30 (a)); tC4 drops
    // it as gone.
    let shorter = TargetVerse::new("Pablo, siervo de y de Dios");
    let next = reflow(&linked(), &shorter).unwrap();
    assert_eq!(
        placed(&next.value),
        [vec!["Pablo/1/1"], vec!["siervo/1/1"], vec![]]
    );
    assert!(show(&next.value.word_bank).contains(&"Dios/1/1".to_owned()));
    assert_eq!(next.dropped.len(), 1);
    // With it, the first "Dios" is the one deleted: the link stays on its own word.
    let edit = reflow_edit(&linked(), &TargetVerse::new(TEXT), &shorter).unwrap();
    assert_eq!(
        placed(&edit.value),
        [["Pablo/1/1"], ["siervo/1/1"], ["Dios/1/1"]]
    );
    assert!(edit.dropped.is_empty());
    // A kept word whose total changed carries the new total.
    let more = TargetVerse::new("Dios: Pablo, siervo de Dios y de Dios");
    let edit = reflow_edit(&linked(), &TargetVerse::new(TEXT), &more).unwrap();
    assert_eq!(placed(&edit.value)[2], ["Dios/3/3"]);
    assert_eq!(
        placed(&reflow(&linked(), &more).unwrap().value)[2],
        Vec::<String>::new()
    );
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
    let fresh = bootstrap(&source, &TargetVerse::new(TEXT), basis(1));
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

/// Every placed word is either still placed or reported dropped, every kept link is on a
/// word of the same text, and the value holds every word of the new text exactly once.
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
            before.alignments[drop.card]
                .bottom_words
                .contains(&drop.word),
            "{at}"
        );
    }
    for (old, new) in before.alignments.iter().zip(&after.value.alignments) {
        for word in &new.bottom_words {
            assert!(
                old.bottom_words.iter().any(|w| w.word == word.word),
                "{at}: no new link"
            );
        }
    }
    let mut held: Vec<TargetWord> = after.value.all_target_words().cloned().collect();
    let mut words = verse.words().to_vec();
    let key = |w: &TargetWord| (w.word.clone(), w.occurrence);
    held.sort_by_key(key);
    words.sort_by_key(key);
    assert_eq!(held, words, "{at}");
}

/// The texts the edit gave another number of copies.
fn recounted(before: &TargetVerse, after: &TargetVerse) -> std::collections::BTreeSet<String> {
    let count = |v: &TargetVerse| {
        let mut n = std::collections::BTreeMap::<String, usize>::new();
        for w in v.words() {
            *n.entry(w.word.clone()).or_default() += 1;
        }
        n
    };
    let (old, new) = (count(before), count(after));
    old.keys()
        .chain(new.keys())
        .filter(|t| old.get(*t) != new.get(*t))
        .cloned()
        .collect()
}

#[test]
fn reflow_matches_tc4_on_edited_en_ult_verses_and_drops_nothing_silently() {
    let mut reflowed = 0;
    let mut refused = 0;
    let mut dropped = 0;
    let mut unclear = 0;
    for case in vectors("reflow-vectors.json") {
        let reference = case["ref"].as_str().unwrap();
        let stored = value_of(&case["record"]);
        let before = TargetVerse::new(case["text"].as_str().unwrap());
        assert!(!stored.is_stale_against(&before), "{reference}");
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
                assert_eq!(reflow_edit(&value, &before, &verse), Err(reason), "{at}");
                refused += 1;
                continue;
            }
            let ours = ours.unwrap_or_else(|e| panic!("{at}: {e:?}"));
            // Without the earlier text, a link whose word the edit gave another number of
            // copies goes back to the bank, reported (#789, Q30 (a)); every other link is
            // tC4's.
            let recounted = recounted(&before, &verse);
            let mut theirs = value_of(expected);
            for group in &mut theirs.alignments {
                let before = group.bottom_words.len();
                group.bottom_words.retain(|w| !recounted.contains(&w.word));
                unclear += before - group.bottom_words.len();
            }
            assert_eq!(cards(&ours.value), cards(&theirs), "{at}");
            assert_eq!(ours.value.done, ours.value.is_fully_aligned(), "{at}");
            assert_eq!(ours.value.target_verse_md5, theirs.target_verse_md5, "{at}");
            assert!(!ours.value.marked_invalid, "{at}");
            assert_no_loss(&value, &ours, &verse, &at);
            // With it, each link follows its own word; where no linked word was recounted,
            // that is tC4's result.
            let edited = reflow_edit(&value, &before, &verse).unwrap();
            assert_no_loss(&value, &edited, &verse, &at);
            if recounted.is_empty() {
                assert_eq!(edited, ours, "{at}");
            }
            dropped += ours.dropped.len();
            reflowed += 1;
        }
    }
    assert_eq!((reflowed, refused), (16 * 4, 16 * 2 + 1));
    assert!(dropped > 0);
    assert!(unclear > 0);
}

#[test]
fn a_word_held_on_two_cards_is_kept_on_the_first_and_reported_dropped_from_the_second() {
    // tC4 would keep both links; one word on two cards is one word aligned twice, which
    // the aligned USFM cannot write, so the second link is reported, not kept.
    let mut twice = linked();
    let pablo = twice.alignments[0].bottom_words[0].clone();
    twice.alignments[2].bottom_words.push(pablo.clone());
    assert!(twice.well_formed().is_err());
    let next = reflow(
        &twice,
        &TargetVerse::new("Pablo, siervo de Dios y apóstol de Dios"),
    )
    .unwrap();
    assert_eq!(
        next.value.alignments[0].bottom_words,
        std::slice::from_ref(&pablo)
    );
    assert!(!next.value.alignments[2].bottom_words.contains(&pablo));
    assert_eq!(next.dropped.len(), 1);
    assert_eq!((next.dropped[0].card, &next.dropped[0].word), (2, &pablo));
    assert_eq!(next.value.well_formed(), Ok(()));
    assert_eq!(twice.placed(), next.value.placed() + next.dropped.len());
}

#[test]
fn a_word_keeps_its_extra_fields_across_a_reflow_while_it_stays_in_the_verse() {
    let mut before = linked();
    let typed = tc_journal::json::JsonMap::from([(
        "type".to_owned(),
        tc_journal::json::Json::from("bottomWord"),
    )]);
    let words = before.word_bank.iter_mut().chain(
        before
            .alignments
            .iter_mut()
            .flat_map(|g| g.bottom_words.iter_mut()),
    );
    for word in words {
        word.extra = typed.clone();
    }
    // "siervo" is gone: its link drops and its fields go with it.
    let next = reflow(
        &before,
        &TargetVerse::new("Pablo, esclavo de Dios y de Dios"),
    )
    .unwrap();
    let keys = |v: &AlignmentValue| -> Vec<String> {
        v.all_target_words()
            .filter(|w| w.extra == typed)
            .map(|w| format!("{}{}", w.word, w.occurrence))
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

// ---- #789 a link follows its own word (the owner's Q30 (a)) ----

/// The sample's TIT 1:1 with Θεοῦ¹ on Dios¹ (card 2) and Θεοῦ² on Dios² (card 9), and its
/// text.
fn sample() -> (AlignmentValue, String) {
    let (stored, text) = sample_tit_1_1();
    assert_eq!(placed(&stored)[2], ["Dios/1/2"]);
    assert_eq!(placed(&stored)[9], ["Dios/2/2"]);
    (stored, text)
}

#[test]
fn deleting_an_earlier_copy_keeps_the_link_on_its_physical_word() {
    // tC4 #789's case: delete "de Dios" after "siervo".
    let (stored, text) = sample();
    let before = TargetVerse::new(&text);
    let after = TargetVerse::new(&text.replacen("siervo de Dios", "siervo", 1));
    let next = reflow_edit(&stored, &before, &after).unwrap();
    assert!(placed(&next.value)[2].is_empty());
    assert_eq!(
        placed(&next.value)[9],
        ["Dios/1/1"],
        "Θεοῦ² keeps its own Dios"
    );
    assert_eq!(next.dropped.len(), 1);
    assert_eq!(
        (next.dropped[0].card, next.dropped[0].word.occurrence),
        (2, 1)
    );
    // Nothing is marked for review (Q30 (a)).
    assert!(!next.value.marked_invalid);
    let run = basis(1).aligned_run(&after);
    assert_ne!(
        tc_align::verse_status(Some(&next.value), Some((&run, &after))).state,
        tc_align::VerseState::Invalid
    );
    // Without the earlier text it is not clear which Dios went: both links go back to the
    // bank, reported, and neither lands on the other card.
    let unclear = reflow(&stored, &after).unwrap();
    assert!(placed(&unclear.value)[2].is_empty() && placed(&unclear.value)[9].is_empty());
    assert_eq!(unclear.dropped.len(), 2);
    assert!(show(&unclear.value.word_bank).contains(&"Dios/1/1".to_owned()));
}

#[test]
fn inserting_an_earlier_copy_renumbers() {
    let (stored, text) = sample();
    let before = TargetVerse::new(&text);
    let after = TargetVerse::new(&text.replacen("Pablo,", "Pablo de Dios,", 1));
    let next = reflow_edit(&stored, &before, &after).unwrap();
    assert_eq!(placed(&next.value)[2], ["Dios/2/3"]);
    assert_eq!(placed(&next.value)[9], ["Dios/3/3"]);
    assert!(show(&next.value.word_bank).contains(&"Dios/1/3".to_owned()));
    assert!(next.dropped.is_empty());
}

#[test]
fn delete_then_readd_never_reattaches_an_old_link() {
    let (stored, text) = sample();
    let t1 = TargetVerse::new(&text);
    // Two saves: "Dios" deleted after "siervo de", then typed back.
    let deleted = TargetVerse::new(&text.replacen("siervo de Dios", "siervo de", 1));
    let first = reflow_edit(&stored, &t1, &deleted).unwrap();
    assert!(placed(&first.value)[2].is_empty());
    assert_eq!(first.dropped.len(), 1);
    let second = reflow_edit(&first.value, &deleted, &t1).unwrap();
    assert!(
        placed(&second.value)[2].is_empty(),
        "the old link stays off"
    );
    assert_eq!(placed(&second.value)[9], ["Dios/2/2"]);
    assert!(show(&second.value.word_bank).contains(&"Dios/1/2".to_owned()));
    // One save that deletes it and types it elsewhere: the new copy is another word.
    let moved =
        TargetVerse::new(&text.replacen("Pablo, siervo de Dios", "Pablo, Dios, siervo de", 1));
    let next = reflow_edit(&stored, &t1, &moved).unwrap();
    assert!(placed(&next.value)[2].is_empty());
    assert_eq!(placed(&next.value)[9], ["Dios/2/2"]);
    assert_eq!(next.dropped.len(), 1);
}

#[test]
fn a_reflow_from_text_the_value_was_not_made_against_counts_instead() {
    let (stored, text) = sample();
    let other = TargetVerse::new("Pablo, siervo de Dios");
    let after = TargetVerse::new(&text.replacen("siervo de Dios", "siervo", 1));
    assert_eq!(
        reflow_edit(&stored, &other, &after),
        reflow(&stored, &after)
    );
}
