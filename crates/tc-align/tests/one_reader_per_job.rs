//! INV-ONE-READER-PER-JOB: the readers an import could use for one input agree, so that one
//! of them can go. Each test lists every disagreement it finds.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use tc_align::usfm::from_aligned;
use tc_align::{AlignmentGroup, AlignmentValue, record};
use tc_core::BookId;
use tc_journal::alignment::split_aligned;
use tc_journal::fold::decode_alignments;
use tc_journal::ids::VerseNo;
use tc_journal::record::Record;

const SAMPLE: &str = "fixtures/tc4/sample-burrito/ingredients/checking/alignments/TIT.json";
const EN_ULT: &str = "fixtures/tc4/test-fixtures/usfm-corpus/en_ult-TIT-aligned.usfm";

/// The fields of two values that differ, each with both values.
fn differences(a: &AlignmentValue, b: &AlignmentValue) -> Vec<String> {
    let mut out = Vec::new();
    macro_rules! field {
        ($name:ident) => {
            if a.$name != b.$name {
                out.push(format!(
                    "{}: {:?} / {:?}",
                    stringify!($name),
                    a.$name,
                    b.$name
                ));
            }
        };
    }
    field!(span_end);
    field!(alignments);
    field!(word_bank);
    field!(target_verse_md5);
    field!(source_version);
    field!(source);
    field!(done);
    field!(marked_invalid);
    field!(dismissed);
    field!(extra);
    out
}

/// `record` read by the sidecar reader, as a one-verse sidecar of TIT.
fn sidecar(chapter: &str, verse: &str, record: &Value) -> AlignmentValue {
    let file = json!({ "chapters": { chapter: { verse: record } } });
    let records = decode_alignments(BookId::Titus, file.to_string().as_bytes()).unwrap();
    match records.into_iter().next() {
        Some(Record::Alignment { value, .. }) => value,
        other => panic!("{chapter}:{verse}: {other:?}"),
    }
}

fn compare_sidecar(reference: &str, record: &Value, found: &mut Vec<String>) {
    let (chapter, verse) = reference.split_once(':').unwrap();
    let span_end = verse.split('-').next_back().unwrap().parse().unwrap();
    let kept = sidecar(chapter, verse, record);
    match record::from_record(record, basis(span_end)) {
        Ok(other) => found.extend(
            differences(&kept, &other)
                .into_iter()
                .map(|d| format!("{reference} {d}")),
        ),
        Err(e) => found.push(format!("{reference} from_record refuses: {e}")),
    }
}

#[test]
fn the_sidecar_readers_agree_on_every_tc4_record() {
    let mut found = Vec::new();
    let file: Value = serde_json::from_str(&read(SAMPLE)).unwrap();
    let mut records = 0;
    for (chapter, verses) in file["chapters"].as_object().unwrap() {
        for (verse, record) in verses.as_object().unwrap() {
            compare_sidecar(&format!("{chapter}:{verse}"), record, &mut found);
            records += 1;
        }
    }
    for verse in imported() {
        compare_sidecar(&verse.reference, &verse.record, &mut found);
        records += 1;
    }
    println!("{records} records, {} disagreements", found.len());
    for line in &found {
        println!("sidecar {line}");
    }
}

/// Each card's links: every source word (text, occurrence) to every target word.
fn links(cards: &[AlignmentGroup]) -> BTreeSet<(String, u32, String, u32)> {
    cards
        .iter()
        .flat_map(|c| {
            c.top_words.iter().flat_map(move |s| {
                c.bottom_words
                    .iter()
                    .map(move |t| (s.word.clone(), s.occurrence, t.word.clone(), t.occurrence))
            })
        })
        .collect()
}

/// The cards as groups of words, in order.
fn groupings(cards: &[AlignmentGroup]) -> Vec<(Vec<String>, Vec<String>)> {
    cards
        .iter()
        .filter(|c| !c.top_words.is_empty())
        .map(|c| {
            (
                c.top_words
                    .iter()
                    .map(|s| format!("{}{}", s.word, s.occurrence))
                    .collect(),
                c.bottom_words
                    .iter()
                    .map(|t| format!("{}{}", t.word, t.occurrence))
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn the_aligned_usfm_readers_give_tc4s_cards_on_en_ult_titus() {
    let mut found = Vec::new();
    let mut counts = (0, 0, 0);
    for (reference, body) in verses(EN_ULT) {
        counts.0 += 1;
        let aligned = tc_usfm::parse_aligned(&body).unwrap();
        let tc4 = from_aligned(&aligned, None);
        let Some((plain, kept)) = split_aligned(&body, VerseNo::new(1).unwrap()) else {
            found.push(format!(
                "{reference} split_aligned keeps the verse as written"
            ));
            continue;
        };
        if plain != aligned.plain {
            found.push(format!("{reference} plain text differs"));
        }
        if links(&kept.alignments) != links(&tc4.alignments) {
            counts.1 += 1;
            found.push(format!("{reference} links differ"));
        }
        if groupings(&kept.alignments) != groupings(&tc4.alignments) {
            counts.2 += 1;
            found.push(format!(
                "{reference} cards: {} / tC4 {}",
                kept.alignments.len(),
                tc4.alignments.len()
            ));
        }
        if kept.word_bank != tc4.word_bank {
            found.push(format!("{reference} bank order differs"));
        }
        if reference == "1:1" {
            let chosen = kept
                .alignments
                .iter()
                .filter(|c| c.top_words.iter().any(|s| s.word == "ἐκλεκτῶν"))
                .count();
            found.push(format!("1:1 cards naming ἐκλεκτῶν: {chosen}"));
        }
    }
    println!(
        "{} verses, links differ in {}, cards differ in {}",
        counts.0, counts.1, counts.2
    );
    for line in &found {
        println!("usfm {line}");
    }
}
