//! A right-to-left verse through every step: UHB Jonah 1 aligned to its own words (Hebrew
//! with word joiners and maqaf), edited, merged into a phrase, written as aligned USFM,
//! read back, stored as tC4's record, and reflowed after a text edit.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use tc_align::usfm::{from_aligned, weave_verse};
use tc_align::{AlignmentValue, SourceVerse, TargetVerse, record, reflow};

/// Jonah 1's verses as tC4 hashed them, with their UHB words.
fn jonah_1() -> Vec<(String, TargetVerse, SourceVerse)> {
    vectors("bootstrap-vectors.json")
        .into_iter()
        .filter(|c| c["source"] == UHB && c["ref"].as_str().unwrap().starts_with("1:"))
        .map(|c| {
            let reference = c["ref"].as_str().unwrap().to_owned();
            let verse = TargetVerse::new(c["text"].as_str().unwrap());
            let source = source_verse(UHB, &reference);
            (reference, verse, source)
        })
        .collect()
}

/// Every word placed on the card of the same Hebrew word.
fn aligned_to_itself(verse: &TargetVerse, source: &SourceVerse) -> AlignmentValue {
    let mut value = AlignmentValue::bootstrap(source, verse, basis(1));
    for card in 0..value.alignments.len() {
        let wanted = value.alignments[card].sources[0].clone();
        let word = value
            .word_bank
            .iter()
            .find(|w| w.text == wanted.content && w.occurrence == wanted.occurrence)
            .cloned()
            .unwrap_or_else(|| panic!("{} is banked", wanted.content));
        value.link(card, &word).unwrap();
    }
    value.settle(verse);
    value
}

#[test]
fn hebrew_verses_align_word_for_word_and_read_back_from_aligned_usfm() {
    let verses = jonah_1();
    assert_eq!(verses.len(), 17);
    for (reference, verse, source) in &verses {
        let mut value = aligned_to_itself(verse, source);
        assert!(value.is_fully_aligned(), "{reference}");
        assert!(value.done, "{reference}");
        // A phrase across the maqaf, such as דְּבַר־יְהוָ֔ה in 1:1.
        value.merge(1, 2).unwrap();
        value.settle(verse);
        assert_eq!(value.alignments[1].sources.len(), 2, "{reference}");
        assert!(value.done, "{reference}");

        let woven = weave_verse(verse.text(), &value).unwrap();
        let aligned = tc_usfm::parse_aligned(&woven).unwrap();
        assert_eq!(aligned.plain, verse.text(), "{reference}");
        let back = from_aligned(&aligned, Some(source));
        assert_eq!(back.alignments, value.alignments, "{reference}");
        assert!(back.word_bank.is_empty(), "{reference}");

        let json = record::to_record(&value, false);
        assert_eq!(record::from_record(&json, basis(1)).unwrap(), value);
    }
}

#[test]
fn a_hebrew_verse_edited_keeps_its_other_links_and_reports_the_lost_one() {
    let (reference, verse, source) = jonah_1().swap_remove(0);
    assert_eq!(reference, "1:1");
    let value = aligned_to_itself(&verse, &source);
    // Drop the last word, לֵ⁠אמֹֽר, and its sof pasuq.
    let edited = verse.text().rsplit_once(' ').unwrap().0;
    let next = reflow(&value, &TargetVerse::new(edited)).unwrap();
    assert_eq!(next.value.placed(), value.placed() - 1);
    assert_eq!(next.dropped.len(), 1);
    assert_eq!(next.dropped[0].word.text, "לֵ\u{2060}אמֹֽר");
    assert_eq!(next.dropped[0].card, value.alignments.len() - 1);
    assert!(
        !next.value.done,
        "the card that lost its word leaves the verse to do"
    );
}
