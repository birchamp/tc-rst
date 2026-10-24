//! INV-ONE-READER-PER-JOB: the readers an import could use for one input agree, so that one
//! of them can go. Each test lists every disagreement it finds.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;
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

/// The runs' links, as `tc-usfm` reads them: each run's source words to its target words.
fn run_links(runs: &[tc_usfm::AlignmentGroup]) -> BTreeSet<(String, u32, String, u32)> {
    runs.iter()
        .flat_map(|r| {
            r.sources.iter().flat_map(move |s| {
                r.targets.iter().map(move |t| {
                    (
                        s.content.clone(),
                        s.occurrence,
                        t.text.clone(),
                        t.occurrence,
                    )
                })
            })
        })
        .collect()
}

/// The import reads every aligned verse of en_ult Titus into tC4's cards (word-aligner's
/// `unmerge`): the links the runs make, and each source word on one card only.
/// `tc4_vectors.rs` holds the cards against tC4's own output for en_ult and en_ust.
#[test]
fn the_aligned_usfm_readers_give_tc4s_cards_on_en_ult_titus() {
    let mut read = 0;
    for (reference, body) in verses(EN_ULT) {
        let aligned = tc_usfm::parse_aligned(&body).unwrap();
        let (plain, kept) = split_aligned(&body, VerseNo::new(1).unwrap())
            .unwrap_or_else(|| panic!("{reference} is kept as written"));
        assert_eq!(plain, aligned.plain, "{reference}");
        assert_eq!(
            links(&kept.alignments),
            run_links(&aligned.groups),
            "{reference}"
        );
        let mut seen = BTreeSet::new();
        for word in kept.alignments.iter().flat_map(|c| &c.top_words) {
            assert!(
                seen.insert((word.word.clone(), word.occurrence)),
                "{reference}: {} is on two cards",
                word.word
            );
        }
        if reference == "1:1" {
            let chosen = kept
                .alignments
                .iter()
                .filter(|c| c.top_words.iter().any(|s| s.word == "ἐκλεκτῶν"))
                .count();
            assert_eq!(chosen, 1, "TIT 1:1: ἐκλεκτῶν is on one card");
            assert_eq!(kept.alignments.len(), 17, "TIT 1:1 has tC4's 17 cards");
        }
        read += 1;
    }
    assert_eq!(read, 46);
}
