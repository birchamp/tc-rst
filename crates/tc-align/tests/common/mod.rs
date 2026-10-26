//! Helpers shared by the integration tests: fixture paths, the tC4 vectors, and tC4's
//! §5.1 JSON read into values.

// Helpers outside `#[test]` functions read fixtures and may fail loudly.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    dead_code,
    reason = "test code: a failed unwrap is a failed test, and not every test binary uses each shared helper"
)]

use serde_json::{Map, Value};
use std::io::Read;
use std::path::PathBuf;
use tc_align::{
    Alignment as _, AlignmentValue, Basis, SourceVerse, SourceWord, TargetVerse, TargetWord,
};
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
    let value = tc4_record(&sample_alignments()["chapters"]["1"]["1"]).unwrap();
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
    tc4_record(record_json).unwrap()
}

/// A tC3 or tC4 record of TIT 1:1 as an import reads it, held to the rules every edit keeps.
pub fn tc4_record(record: &Value) -> Result<AlignmentValue, tc_align::Error> {
    let (_, _, value) = tc_journal::fold::decode_alignment_record(
        record,
        "1:1",
        1,
        1,
        1,
        tc_journal::fold::Writer::Tc4,
    )
    .map_err(|e| tc_align::Error::Record(e.to_string()))?;
    value.well_formed()?;
    Ok(value)
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

/// tC4's tC3 import fixture: translationCore 3's alignment data of Titus.
pub const TC3_ZIP: &str = "fixtures/tc4/conformance-fixtures/import/tc3/cfm_fbt_tit_book.zip";
const TC3_APP: &str = "apps/translationCore";

fn zip_entry(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Value {
    let mut text = String::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

/// tC4's `normalizeOccurrences`: occurrences as integers, every other field kept.
pub fn normalized(word: &Value) -> Value {
    let mut word = word.clone();
    for key in ["occurrence", "occurrences"] {
        if let Some(Value::String(s)) = word.get(key) {
            word[key] = Value::from(s.parse::<u64>().unwrap());
        }
    }
    word
}

/// tC4's `unmergeWord`: a placed word without tC3's `type`.
pub fn unmerged(word: &Value) -> Value {
    let mut word = normalized(word);
    word.as_object_mut().unwrap().remove("type");
    word
}

/// One TIT verse of the fixture: its record as tC4's `alignmentFile` makes it, and its
/// text.
pub struct Imported {
    pub reference: String,
    pub record: Value,
    pub verse: TargetVerse,
}

pub fn imported() -> Vec<Imported> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(fixture(TC3_ZIP)).unwrap()).unwrap();
    let mut out = Vec::new();
    for chapter in 1..=3 {
        let alignments = zip_entry(
            &mut archive,
            &format!("{TC3_APP}/alignmentData/tit/{chapter}.json"),
        );
        let texts = zip_entry(&mut archive, &format!("tit/{chapter}.json"));
        // tC3's `front` holds no verse, and tC4 keys no record to it.
        let mut verses: Vec<_> = alignments
            .as_object()
            .unwrap()
            .iter()
            .filter_map(|(v, data)| Some((v.parse::<u16>().ok()?, v, data)))
            .collect();
        verses.sort_by_key(|(n, _, _)| *n);
        for (_, verse, data) in verses {
            let text = texts[verse.as_str()].as_str().unwrap();
            let target = TargetVerse::new(text);
            let words = |list: &Value, f: fn(&Value) -> Value| {
                Value::Array(list.as_array().unwrap().iter().map(f).collect())
            };
            let mut record = Map::new();
            record.insert(
                "alignments".into(),
                Value::Array(
                    data["alignments"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|a| {
                            serde_json::json!({
                                "topWords": words(&a["topWords"], normalized),
                                "bottomWords": words(&a["bottomWords"], unmerged),
                            })
                        })
                        .collect(),
                ),
            );
            record.insert("wordBank".into(), words(&data["wordBank"], normalized));
            record.insert("invalid".into(), Value::Bool(false));
            record.insert(
                "targetVerseMd5".into(),
                Value::from(target.md5().to_string()),
            );
            record.insert("sourceVersion".into(), Value::from(SOURCE_NT));
            out.push(Imported {
                reference: format!("{chapter}:{verse}"),
                record: Value::Object(record),
                verse: target,
            });
        }
    }
    out
}

/// Cards in the order tC4 saves them after reading aligned USFM with the original verse
/// (word-aligner's `orderAlignments`): a card goes where its first source word stands, a
/// phrase's other words take no place of their own, a source word nothing is aligned to gets
/// an empty card, and a card whose first word the verse does not have goes last.
pub fn in_tc4_source_order(
    source: &SourceVerse,
    mut unordered: Vec<tc_align::AlignmentGroup>,
) -> Vec<tc_align::AlignmentGroup> {
    let same = |a: &SourceWord, b: &SourceWord| a.word == b.word && a.occurrence == b.occurrence;
    let mut ordered: Vec<tc_align::AlignmentGroup> = Vec::new();
    for word in source.words() {
        if let Some(at) = unordered
            .iter()
            .position(|g| g.top_words.first().is_some_and(|s| same(s, word)))
        {
            ordered.push(unordered.remove(at));
            continue;
        }
        let holds = |g: &tc_align::AlignmentGroup| g.top_words.iter().any(|s| same(s, word));
        if !unordered.iter().any(holds) && !ordered.iter().any(holds) {
            ordered.push(tc_align::edit::card(word.clone()));
        }
    }
    ordered.extend(unordered);
    ordered
}
