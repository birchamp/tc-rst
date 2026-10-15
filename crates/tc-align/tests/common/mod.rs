//! Helpers shared by the integration tests: fixture paths, the tC4 vectors, and tC4's
//! §5.1 JSON read into values.

// Helpers outside `#[test]` functions read fixtures and may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use serde_json::Value;
use std::num::NonZeroU16;
use std::path::PathBuf;
use tc_align::{AlignmentValue, Basis, SourceVerse, SourceWord, TargetWord, record};
use tc_core::VerseSpan;
use tc_usfm::Book;

pub fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

pub fn read(rel: &str) -> String {
    std::fs::read_to_string(fixture(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// The `cases` of a vector file in `fixtures/align/`.
pub fn vectors(name: &str) -> Vec<Value> {
    let v: Value = serde_json::from_str(&read(&format!("fixtures/align/{name}"))).unwrap();
    v["cases"].as_array().unwrap().clone()
}

pub const ULT: &str = "fixtures/tc4/test-fixtures/en_ult/TIT.usfm";
pub const UST: &str = "fixtures/tc4/test-fixtures/en_ust/TIT.usfm";
pub const UGNT: &str = "fixtures/align/el-x-koine_ugnt-TIT.usfm";
pub const UHB: &str = "fixtures/tc4/test-fixtures/hbo_uhb/JON.usfm";
pub const SOURCE_NT: &str = "dcs::unfoldingWord/el-x-koine_ugnt@v0.34";

/// `chapter:verse` (a verse or span) into its parts.
pub fn parse_ref(reference: &str) -> (u16, VerseSpan) {
    let (c, v) = reference.split_once(':').unwrap();
    (c.parse().unwrap(), v.parse().unwrap())
}

/// The body of a verse unit of a book file.
pub fn verse_body(file: &str, reference: &str) -> String {
    let (chapter, verses) = parse_ref(reference);
    let text = read(file);
    let book = Book::parse(&text);
    book.verse(chapter, verses).unwrap().body().to_owned()
}

/// Every keyed verse of a book file: its reference and body.
pub fn verses(file: &str) -> Vec<(String, String)> {
    let text = read(file);
    let book = Book::parse(&text);
    book.verses()
        .filter_map(|v| {
            Some((
                format!("{}:{}", v.chapter()?, v.verses()?),
                v.body().to_owned(),
            ))
        })
        .collect()
}

/// The original-language words behind a reference: a span's member verses in order.
pub fn source_verse(file: &str, reference: &str) -> SourceVerse {
    let (chapter, verses) = parse_ref(reference);
    SourceVerse::span(
        verses
            .verses()
            .map(|v| SourceVerse::from_usfm(&verse_body(file, &format!("{chapter}:{v}"))).unwrap()),
    )
}

pub fn basis(span_end: u16) -> Basis {
    Basis {
        span_end: NonZeroU16::new(span_end).unwrap(),
        source_version: SOURCE_NT.to_owned(),
        source: None,
    }
}

/// A tC4 §5.1 record read as a value.
pub fn value_of(record_json: &Value) -> AlignmentValue {
    record::from_record(record_json, basis(1)).unwrap()
}

/// A word list of a tC4 record (`topWords`, `bottomWords` or `wordBank`) as target words.
pub fn target_words(list: &Value) -> Vec<TargetWord> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|w| TargetWord {
            text: w["word"].as_str().unwrap().to_owned(),
            occurrence: u32::try_from(w["occurrence"].as_u64().unwrap()).unwrap(),
            occurrences: u32::try_from(w["occurrences"].as_u64().unwrap()).unwrap(),
        })
        .collect()
}

/// What tC4's records say of a source word: text, occurrence, occurrences, Strong's
/// number, lemma, morphology.
pub type SourceFacts = (
    String,
    u32,
    u32,
    Option<String>,
    Option<String>,
    Option<String>,
);

pub fn source_facts(word: &SourceWord) -> SourceFacts {
    (
        word.content.clone(),
        word.occurrence,
        word.occurrences,
        word.strong.clone(),
        word.lemma.clone(),
        word.morph.clone(),
    )
}

pub fn json_source_facts(word: &Value) -> SourceFacts {
    let s = |k: &str| word[k].as_str().map(str::to_owned);
    (
        s("word").unwrap(),
        u32::try_from(word["occurrence"].as_u64().unwrap()).unwrap(),
        u32::try_from(word["occurrences"].as_u64().unwrap()).unwrap(),
        s("strong"),
        s("lemma"),
        s("morph"),
    )
}

/// A value's cards as (source facts, target words), for comparing with a tC4 record.
pub fn cards(value: &AlignmentValue) -> Vec<(Vec<SourceFacts>, Vec<TargetWord>)> {
    value
        .alignments
        .iter()
        .map(|g| {
            (
                g.sources.iter().map(source_facts).collect(),
                g.targets.clone(),
            )
        })
        .collect()
}

pub fn json_cards(alignments: &Value) -> Vec<(Vec<SourceFacts>, Vec<TargetWord>)> {
    alignments
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            (
                a["topWords"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(json_source_facts)
                    .collect(),
                target_words(&a["bottomWords"]),
            )
        })
        .collect()
}

/// `word/occurrence/occurrences`, as tC4's tests print words.
pub fn show(words: &[TargetWord]) -> Vec<String> {
    words
        .iter()
        .map(|w| format!("{}/{}/{}", w.text, w.occurrence, w.occurrences))
        .collect()
}
