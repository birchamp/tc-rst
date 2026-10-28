//! Aligned and plain USFM (D2, INV-NO-ALIGNMENT-DROPPED-SILENTLY): a verse's alignment is
//! woven only when `tc_align::needs_review` is false; any other verse with a record keeps its
//! stored bytes and is named in the export's facts. Built on the tC4 sample burrito's Titus,
//! its one alignment record (TIT 1:1) and the UGNT Titus it was made against.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use std::collections::BTreeMap;

use tc_align::{SourceBook, SourceVerse};
use tc_core::{BookId, Reference, VerseSpan};
use tc_export::{
    BookInput, ExportFacts, ExportInput, Produced, SkipReason, SkippedVerse, VerseAlignment,
    produce,
};
use tc_journal::fold::{Writer, decode_alignment_record};
use tc_journal::ids::{ChapterNo, VerseNo};
use tc_journal::record::AlignmentValue;
use tc_usfm::codec::{BookParts, VersePart, decompose, recompose};

use common::fixture;

fn stored_book() -> BookParts {
    decompose(&fixture("tc4/sample-burrito/ingredients/TIT.usfm")).unwrap()
}

/// The sample burrito's record of TIT 1:1, read as an import reads it.
fn record_1_1() -> AlignmentValue {
    let file: serde_json::Value = serde_json::from_str(&fixture(
        "tc4/sample-burrito/ingredients/checking/alignments/TIT.json",
    ))
    .unwrap();
    decode_alignment_record(&file["chapters"]["1"]["1"], "TIT 1:1", 1, 1, 1, Writer::Tc4)
        .unwrap()
        .2
}

fn pin_1_1() -> SourceVerse {
    SourceBook::parse(&fixture("align/el-x-koine_ugnt-TIT.usfm"))
        .verse(1, VerseSpan::single(VerseNo::new(1).unwrap()))
        .unwrap()
}

fn at(chapter: u16, verse: u16) -> (ChapterNo, VerseNo) {
    (
        ChapterNo::new(chapter).unwrap(),
        VerseNo::new(verse).unwrap(),
    )
}

fn aligned(value: AlignmentValue, verse: u16, pin: Option<SourceVerse>) -> VerseAlignment {
    VerseAlignment {
        anchor: VerseNo::new(verse).unwrap(),
        value,
        pin,
    }
}

fn input(parts: BookParts, alignments: Vec<((u16, u16), VerseAlignment)>) -> BookInput {
    BookInput {
        book: BookId::Titus,
        parts,
        alignments: alignments
            .into_iter()
            .map(|((c, v), a)| (at(c, v), a))
            .collect::<BTreeMap<_, _>>(),
        source_sha: None,
    }
}

fn export(input: &BookInput) -> (String, ExportFacts) {
    let (out, facts) = produce(&ExportInput::AlignedUsfm(input)).unwrap();
    let Produced::File(bytes) = out else {
        panic!("aligned USFM is one file")
    };
    (String::from_utf8(bytes).unwrap(), facts)
}

fn reference(chapter: u16, verse: u16) -> Reference {
    Reference::new(
        BookId::Titus,
        ChapterNo::new(chapter).unwrap(),
        VerseSpan::single(VerseNo::new(verse).unwrap()),
    )
}

fn verse_text(parts: &mut BookParts, chapter: u16, verse: u16) -> &mut String {
    match parts
        .chapters
        .get_mut(&chapter)
        .and_then(|c| c.verses.get_mut(&verse))
    {
        Some(VersePart::Text(text)) => &mut text.words,
        _ => panic!("{chapter}:{verse} is not a verse with text"),
    }
}

#[test]
fn export_reports_and_keeps_a_verse_that_needs_review() {
    // The record vouches for the stored text, so it is woven.
    let (woven, facts) = export(&input(
        stored_book(),
        vec![((1, 1), aligned(record_1_1(), 1, Some(pin_1_1())))],
    ));
    assert!(woven.contains("\\zaln-s"), "{woven}");
    assert!(facts.skipped.is_empty());

    // The translator rewrote a word after aligning: the record no longer vouches for it.
    let mut parts = stored_book();
    let words = verse_text(&mut parts, 1, 1);
    *words = words.replacen("siervo", "servidor", 1);
    let stored = recompose(&parts).usfm;
    let (out, facts) = export(&input(
        parts,
        vec![((1, 1), aligned(record_1_1(), 1, Some(pin_1_1())))],
    ));
    assert_eq!(out, stored, "the verse keeps its stored bytes");
    assert_eq!(
        facts,
        ExportFacts {
            skipped: vec![SkippedVerse {
                reference: reference(1, 1),
                why: SkipReason::NeedsReview,
            }],
            skipped_total: 1,
        }
    );
}

#[test]
fn export_keeps_inline_zaln_bytes_even_with_a_matching_record() {
    // The verse holds the alignment of an aligned file it was added from: here the record's
    // links without its first card, so its bytes differ from what the record would weave.
    let record = record_1_1();
    let mut fewer = record.clone();
    fewer.alignments.remove(0);
    let mut parts = stored_book();
    let words = verse_text(&mut parts, 1, 1);
    *words = tc_align::usfm::weave_verse(words, &fewer).unwrap();
    let stored = recompose(&parts).usfm;

    let (out, facts) = export(&input(
        parts,
        vec![((1, 1), aligned(record, 1, Some(pin_1_1())))],
    ));
    assert_eq!(out, stored, "the inline alignment is exported as stored");
    assert_eq!(
        facts.skipped,
        vec![SkippedVerse {
            reference: reference(1, 1),
            why: SkipReason::InlineAlignment,
        }]
    );
    assert_eq!(facts.skipped_total, 1);
}

#[test]
fn a_record_with_nothing_placed_exports_the_stored_bytes() {
    let mut empty = record_1_1();
    for card in &mut empty.alignments {
        empty.word_bank.append(&mut card.bottom_words);
    }
    let parts = stored_book();
    let stored = recompose(&parts).usfm;
    let (out, facts) = export(&input(parts, vec![((1, 1), aligned(empty, 1, None))]));
    assert_eq!(out, stored, "no verse is wrapped in \\w (tC4 F20)");
    assert!(!out.contains("\\w "));
    assert_eq!(facts, ExportFacts::default(), "nothing was left out");
}

#[test]
fn export_judges_validity_with_needs_review() {
    // The record is valid by its hash, span and pin, but the pinned original-language verse
    // lacks the word its first card stands on (tC4 #790): needs_review says no, so the verse
    // is not woven.
    let pin = pin_1_1();
    let without_first = SourceVerse::new(pin.words()[1..].to_vec());
    let (out, facts) = export(&input(
        stored_book(),
        vec![((1, 1), aligned(record_1_1(), 1, Some(without_first)))],
    ));
    assert!(!out.contains("\\zaln-s"), "{out}");
    assert_eq!(
        facts.skipped,
        vec![SkippedVerse {
            reference: reference(1, 1),
            why: SkipReason::NeedsReview,
        }]
    );
}

#[test]
fn the_export_weaves_no_link_to_a_source_word_the_pin_lacks() {
    // tC4 #816 fits the cards to the pin and weaves the rest; the rebuild weaves none of the
    // verse and names it, so no link leaves the app without a word (docs/PLAN.md, D2).
    let pin = pin_1_1();
    let missing = pin.words()[0].word.clone();
    let without_first = SourceVerse::new(pin.words()[1..].to_vec());
    let (out, facts) = export(&input(
        stored_book(),
        vec![((1, 1), aligned(record_1_1(), 1, Some(without_first)))],
    ));
    assert!(
        !out.contains(&format!("x-content=\"{missing}\"")),
        "a link to {missing} was woven: {out}"
    );
    assert_eq!(facts.skipped_total, 1, "the verse is named");
}

#[test]
fn an_export_names_at_most_20_skipped_verses_with_the_total() {
    // 25 records made for other text: TIT 1:1-16 and 2:1-9.
    let places: Vec<(u16, u16)> = (1..=16)
        .map(|v| (1, v))
        .chain((1..=9).map(|v| (2, v)))
        .collect();
    let mut parts = stored_book();
    let words = verse_text(&mut parts, 1, 1);
    *words = words.replacen("siervo", "servidor", 1);
    let stored = recompose(&parts).usfm;
    let alignments = places
        .iter()
        .map(|&(c, v)| {
            let mut value = record_1_1();
            value.span_end = VerseNo::new(v).unwrap();
            ((c, v), aligned(value, v, None))
        })
        .collect();
    let (out, facts) = export(&input(parts, alignments));
    assert_eq!(out, stored);
    assert_eq!(facts.skipped_total, 25);
    assert_eq!(
        facts.skipped,
        places[..20]
            .iter()
            .map(|&(c, v)| SkippedVerse {
                reference: reference(c, v),
                why: SkipReason::NeedsReview,
            })
            .collect::<Vec<_>>(),
        "the first 20 in book order"
    );
}

#[test]
fn a_verse_the_weave_refuses_keeps_its_bytes_and_is_named() {
    // A glossary `\w` that holds no word leaves the verse's words as the record placed them,
    // so the record still vouches for the verse, but the weave refuses `\w` markup.
    let mut parts = stored_book();
    let words = verse_text(&mut parts, 1, 1);
    words.push_str(r#" \w |x-tw="rc://*/tw/dict/bible/kt/god"\w*"#);
    let stored = recompose(&parts).usfm;
    let (out, facts) = export(&input(
        parts,
        vec![((1, 1), aligned(record_1_1(), 1, Some(pin_1_1())))],
    ));
    assert_eq!(out, stored, "the verse keeps its stored bytes");
    assert_eq!(
        facts,
        ExportFacts {
            skipped: vec![SkippedVerse {
                reference: reference(1, 1),
                why: SkipReason::WeaveRefused,
            }],
            skipped_total: 1,
        }
    );
}

fn plain(input: &BookInput) -> (String, ExportFacts) {
    let (out, facts) = produce(&ExportInput::PlainUsfm(input)).unwrap();
    let Produced::File(bytes) = out else {
        panic!("plain USFM is one file")
    };
    (String::from_utf8(bytes).unwrap(), facts)
}

#[test]
fn a_verse_whose_markup_will_not_strip_keeps_its_bytes_in_plain_usfm() {
    let stored_plain = recompose(&stored_book()).usfm;
    let mut parts = stored_book();
    let words = verse_text(&mut parts, 1, 1);
    *words = tc_align::usfm::weave_verse(words, &record_1_1()).unwrap();
    // Kept as written on import, the inline alignment strips to the stored text.
    let (out, facts) = plain(&input(parts.clone(), Vec::new()));
    assert_eq!(out, stored_plain);
    assert_eq!(facts, ExportFacts::default());

    // An unclosed `\zaln-s`: that verse keeps its bytes and is named, the rest is plain.
    let words = verse_text(&mut parts, 1, 1);
    let end = words.rfind("\\zaln-e\\*").unwrap();
    words.replace_range(end..end + "\\zaln-e\\*".len(), "");
    assert!(
        tc_usfm::strip_alignment(words).is_err(),
        "the probe will not strip"
    );
    let broken = words.clone();
    let mut want = stored_book();
    *verse_text(&mut want, 1, 1) = broken.clone();
    let (out, facts) = plain(&input(parts, Vec::new()));
    assert_eq!(out, recompose(&want).usfm);
    assert!(out.contains(&broken));
    assert_eq!(
        facts,
        ExportFacts {
            skipped: vec![SkippedVerse {
                reference: reference(1, 1),
                why: SkipReason::MarkupKept,
            }],
            skipped_total: 1,
        }
    );
}
