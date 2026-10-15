//! tC4 records of a project imported from translationCore 3 (`import/tc3.ts`): tC3 wrote
//! `"type": "bottomWord"` on every banked word, and tC4 keeps the bank as tC3 wrote it.
//! The records are made from tC4's own tC3 fixture, `cfm_fbt_tit_book.zip`, as tC4's
//! importer makes them, then read, written back and opened.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use serde_json::{Map, Value};
use std::io::Read;
use tc_align::{Aligner, AlignmentValue, TargetVerse, record};

const ZIP: &str = "fixtures/tc4/conformance-fixtures/import/tc3/cfm_fbt_tit_book.zip";
const APP: &str = "apps/translationCore";

fn entry(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Value {
    let mut text = String::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

/// tC4's `normalizeOccurrences`: occurrences as integers, every other field kept.
fn normalized(word: &Value) -> Value {
    let mut word = word.clone();
    for key in ["occurrence", "occurrences"] {
        if let Some(Value::String(s)) = word.get(key) {
            word[key] = Value::from(s.parse::<u64>().unwrap());
        }
    }
    word
}

/// tC4's `unmergeWord`: a placed word without tC3's `type`.
fn unmerged(word: &Value) -> Value {
    let mut word = normalized(word);
    word.as_object_mut().unwrap().remove("type");
    word
}

/// One TIT verse of the fixture: its record as tC4's `alignmentFile` makes it, and its
/// text.
struct Imported {
    reference: String,
    record: Value,
    verse: TargetVerse,
}

fn imported() -> Vec<Imported> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(fixture(ZIP)).unwrap()).unwrap();
    let mut out = Vec::new();
    for chapter in 1..=3 {
        let alignments = entry(
            &mut archive,
            &format!("{APP}/alignmentData/tit/{chapter}.json"),
        );
        let texts = entry(&mut archive, &format!("tit/{chapter}.json"));
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
            let target = TargetVerse::new(&tc_usfm::plain_text(text));
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

fn typed_words(list: &Value) -> usize {
    list.as_array()
        .unwrap()
        .iter()
        .filter(|w| w["type"] == "bottomWord")
        .count()
}

#[test]
fn every_record_of_a_tc3_project_reads_and_writes_back_with_tc3s_word_type() {
    let imported = imported();
    assert_eq!(imported.len(), 46);
    let (mut with_type, mut typed_total) = (0, 0);
    for verse in &imported {
        let at = &verse.reference;
        let value =
            record::from_record(&verse.record, basis(1)).unwrap_or_else(|e| panic!("{at}: {e}"));
        assert_eq!(record::to_record(&value, false), verse.record, "{at}");
        let typed = typed_words(&verse.record["wordBank"]);
        assert_eq!(value.word_extra.len(), typed, "{at}");
        if typed > 0 {
            with_type += 1;
        }
        typed_total += typed;
    }
    assert_eq!((with_type, typed_total), (46, 1108));
}

#[test]
fn a_tc3_record_opens_as_it_is_and_tc3s_word_type_goes_with_the_word() {
    let imported = imported();
    let verse = imported.iter().find(|v| v.reference == "1:1").unwrap();
    let stored = record::from_record(&verse.record, basis(1)).unwrap();
    assert!(typed_words(&verse.record["wordBank"]) > 0);
    let mut aligner = Aligner::open(
        Some(&stored),
        verse.verse.clone(),
        &source_verse(UGNT, "1:1"),
        basis(1),
    )
    .unwrap();
    assert_eq!(aligner.value(), &stored);
    assert!(!aligner.is_stale() && !aligner.is_changed());

    // Placed, the word keeps its tC3 type on the card, as tC4's `linkWord` moves the
    // whole bank object; sent back, it keeps it in the bank.
    let word = aligner.value().word_bank[0].clone();
    aligner.place(0, &word).unwrap();
    let placed = record::to_record(aligner.value(), false);
    let card = placed["alignments"][0]["bottomWords"].as_array().unwrap();
    assert_eq!(card.last().unwrap()["word"], word.text.as_str());
    assert_eq!(card.last().unwrap()["type"], "bottomWord");
    assert_eq!(
        typed_words(&placed["wordBank"]) + 1,
        typed_words(&verse.record["wordBank"])
    );
    aligner.unplace(0, &word).unwrap();
    let back = record::to_record(aligner.value(), false);
    assert_eq!(back["wordBank"], verse.record["wordBank"]);
    let reread: AlignmentValue = record::from_record(&back, basis(1)).unwrap();
    assert_eq!(&reread.word_extra, &stored.word_extra);
}
