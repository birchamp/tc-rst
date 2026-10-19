//! Helpers shared by the integration tests: fixture paths, the tC4 vectors, and tC4's
//! §5.1 JSON read into values.

// Helpers outside `#[test]` functions read fixtures and may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use serde_json::Value;
use std::path::PathBuf;
use tc_align::{AlignmentValue, Basis, SourceVerse, SourceWord, TargetWord, record};
use tc_core::VerseSpan;
use tc_journal::ids::VerseNo;
use tc_journal::json::{Json, JsonMap};
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
    let text = read(file);
    SourceVerse::from_book(&Book::parse(&text), chapter, verses).unwrap()
}

pub const SAMPLE_TIT: &str = "fixtures/tc4/sample-burrito/ingredients/TIT.usfm";
pub const SAMPLE_TIT_ALIGNMENTS: &str =
    "fixtures/tc4/sample-burrito/ingredients/checking/alignments/TIT.json";

/// tC4's conformance sample: its TIT alignment file.
pub fn sample_alignments() -> Value {
    serde_json::from_str(&read(SAMPLE_TIT_ALIGNMENTS)).unwrap()
}

/// tC4's conformance sample TIT 1:1: its record, as a value, and the verse's content.
pub fn sample_tit_1_1() -> (AlignmentValue, String) {
    let value = record::from_record(&sample_alignments()["chapters"]["1"]["1"], basis(1)).unwrap();
    (value, verse_body(SAMPLE_TIT, "1:1"))
}

/// A file of an archive of the bundled resources (`bundle/en`).
pub fn bundle_entry(archive: &str, entry: &str) -> String {
    use std::io::Read;
    let file = std::fs::File::open(fixture(&format!("bundle/en/{archive}"))).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let mut text = String::new();
    zip.by_name(entry)
        .unwrap_or_else(|e| panic!("{archive} {entry}: {e}"))
        .read_to_string(&mut text)
        .unwrap();
    text
}

/// Every verse of a book as the codec cuts it: `chapter:verse` and its content.
pub fn codec_verses(raw: &str) -> Vec<(String, String)> {
    use tc_usfm::codec;
    let parts = codec::decompose(raw).unwrap();
    let mut out = Vec::new();
    for (c, chapter) in &parts.chapters {
        for (v, part) in &chapter.verses {
            if let codec::VersePart::Text(verse) = part {
                out.push((
                    format!("{c}:{}", codec::label(*v, verse.last)),
                    codec::join_verse(&verse.words, &verse.layout),
                ));
            }
        }
    }
    out
}

/// A verse or span from verse 1 to `span_end`, not renumbered.
pub fn basis(span_end: u16) -> Basis {
    Basis::run(
        VerseNo::new(1).unwrap(),
        VerseNo::new(span_end).unwrap(),
        SOURCE_NT.to_owned(),
        None,
    )
}

/// A target word with no other field.
pub fn word(text: &str, occurrence: u32, occurrences: u32) -> TargetWord {
    TargetWord {
        word: text.to_owned(),
        occurrence,
        occurrences,
        extra: JsonMap::new(),
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
            word: w["word"].as_str().unwrap().to_owned(),
            occurrence: u32::try_from(w["occurrence"].as_u64().unwrap()).unwrap(),
            occurrences: u32::try_from(w["occurrences"].as_u64().unwrap()).unwrap(),
            extra: w
                .as_object()
                .unwrap()
                .iter()
                .filter(|(k, _)| !matches!(k.as_str(), "word" | "occurrence" | "occurrences"))
                .map(|(k, v)| (k.clone(), Json::try_from(v).unwrap()))
                .collect(),
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
        word.word.clone(),
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
                g.top_words.iter().map(source_facts).collect(),
                g.bottom_words.clone(),
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
        .map(|w| format!("{}/{}/{}", w.word, w.occurrence, w.occurrences))
        .collect()
}
