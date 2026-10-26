//! INV-ONE-READER-PER-JOB: the readers an import could use for one input agree, so that one
//! of them can go. Each test lists every disagreement it finds.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use tc_align::{AlignmentGroup, AlignmentValue};
use tc_core::BookId;
use tc_journal::alignment::split_aligned;
use tc_journal::fold::decode_alignments;
use tc_journal::ids::VerseNo;
use tc_journal::record::Record;

const SAMPLE: &str = "fixtures/tc4/sample-burrito/ingredients/checking/alignments/TIT.json";
const EN_ULT: &str = "fixtures/tc4/test-fixtures/usfm-corpus/en_ult-TIT-aligned.usfm";

/// `record` read by the sidecar reader, as a one-verse sidecar of TIT this app wrote.
fn sidecar(chapter: &str, verse: &str, record: &Value) -> AlignmentValue {
    let file = json!({ "chapters": { chapter: { verse: record } } });
    let records = decode_alignments(BookId::Titus, file.to_string().as_bytes()).unwrap();
    match records.into_iter().next() {
        Some(Record::Alignment { value, .. }) => value,
        other => panic!("{chapter}:{verse}: {other:?}"),
    }
}

/// Every tC4 and tC3 record reads, as tC4 wrote it, into the value the removed second reader
/// (`tc_align::record::from_record`) gave (`tests/expected/tc4_records.txt`), and, with
/// `invalid` false in all of them, as this app's own sidecar reads it.
#[test]
fn the_sidecar_readers_agree_on_every_tc4_record() {
    let file: Value = serde_json::from_str(&read(SAMPLE)).unwrap();
    let mut records = Vec::new();
    for (chapter, verses) in file["chapters"].as_object().unwrap() {
        for (verse, record) in verses.as_object().unwrap() {
            records.push((format!("{chapter}:{verse}"), record.clone()));
        }
    }
    records.extend(imported().into_iter().map(|v| (v.reference, v.record)));
    let expected = read("crates/tc-align/tests/expected/tc4_records.txt");
    let expected: Vec<&str> = expected.lines().collect();
    assert_eq!(records.len(), expected.len());
    for ((reference, record), expected) in records.iter().zip(expected) {
        let value = tc4_record(record).unwrap_or_else(|e| panic!("{reference}: {e}"));
        assert_eq!(format!("{reference} {value:?}"), expected);
        let (chapter, verse) = reference.split_once(':').unwrap();
        // The sidecar's span is its verse key's; the helper reads every record at 1:1.
        let own = AlignmentValue {
            span_end: value.span_end,
            ..sidecar(chapter, verse, record)
        };
        assert_eq!(own, value, "{reference}");
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

/// tC4 stores a person's "invalid" mark as `invalid`; this app's sidecar writes `invalid`
/// computed and keeps the mark in `markedInvalid`. A field the reader cannot keep whole is
/// refused, never read as nothing.
#[test]
fn each_writers_invalid_reads_as_that_writer_meant_it() {
    let file: Value = serde_json::from_str(&read(SAMPLE)).unwrap();
    let mut record = file["chapters"]["1"]["1"].clone();
    record["invalid"] = Value::Bool(true);
    assert!(tc4_record(&record).unwrap().marked_invalid);
    assert!(!sidecar("1", "1", &record).marked_invalid);
    record["markedInvalid"] = Value::Bool(true);
    assert!(sidecar("1", "1", &record).marked_invalid);

    let mut strong = file["chapters"]["1"]["1"].clone();
    strong["alignments"][0]["topWords"][0]["strong"] = json!(39720);
    assert!(tc4_record(&strong).is_err());
    let mut done = file["chapters"]["1"]["1"].clone();
    done["done"] = json!("yes");
    assert!(tc4_record(&done).is_err());
}
