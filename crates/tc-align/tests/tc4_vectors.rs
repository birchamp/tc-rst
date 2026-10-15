//! The bootstrap, aligned USFM and record against what tC4's own code returns for the
//! real fixtures (`fixtures/align/*-vectors.json`, made by `tools/gen-align-vectors.cjs`):
//! en_ult and en_ust Titus with UGNT Titus, and UHB Jonah (right to left).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use serde_json::Value;
use std::num::NonZeroU16;
use tc_align::usfm::{from_aligned, usfm_groups, weave_verse};
use tc_align::{AlignmentValue, Current, Group, TargetVerse, VerseState, record, verse_status};
use tc_usfm::parse_aligned;

#[test]
fn bootstrap_matches_tc4_for_greek_and_hebrew_verses_and_a_span() {
    let cases = vectors("bootstrap-vectors.json");
    assert_eq!(cases.len(), 46 + 17 + 1);
    let mut hebrew = 0;
    for case in cases {
        let reference = case["ref"].as_str().unwrap();
        let file = case["source"].as_str().unwrap();
        let source = source_verse(file, reference);
        let verse = TargetVerse::new(case["text"].as_str().unwrap());
        let span_end = parse_ref(reference).1.end();
        let value = AlignmentValue::bootstrap(&source, &verse, basis(span_end));
        let expected = &case["record"];
        let at = format!("{file} {reference}");
        assert_eq!(cards(&value), json_cards(&expected["alignments"]), "{at}");
        assert_eq!(value.word_bank, target_words(&expected["wordBank"]), "{at}");
        assert_eq!(
            value.target_verse_md5.to_string(),
            expected["targetVerseMd5"].as_str().unwrap(),
            "{at}"
        );
        if file == UHB && reference.starts_with("1:") {
            hebrew += 1;
            // Right to left, with word joiners inside words, on both sides.
            assert!(
                value.word_bank.iter().any(|w| w.text.contains('\u{2060}')),
                "{at}"
            );
            assert!(
                source
                    .words()
                    .iter()
                    .any(|w| w.content.contains('\u{2060}')),
                "{at}"
            );
        }
    }
    assert_eq!(hebrew, 17);
}

#[test]
fn aligned_en_ult_and_en_ust_read_into_the_cards_tc4_reads_with_ugnt() {
    let mut checked = 0;
    for case in vectors("extract-vectors.json")
        .iter()
        .filter(|c| c["withOriginal"].is_object())
    {
        let file = case["file"].as_str().unwrap();
        let reference = case["ref"].as_str().unwrap();
        let aligned = parse_aligned(&verse_body(file, reference)).unwrap();
        let extracted = from_aligned(&aligned, Some(&source_verse(UGNT, reference)));
        let value = with(extracted.alignments, extracted.word_bank);
        let expected = &case["withOriginal"];
        let at = format!("{file} {reference}");
        assert_eq!(cards(&value), json_cards(&expected["alignments"]), "{at}");
        assert_eq!(value.word_bank, target_words(&expected["wordBank"]), "{at}");
        checked += 1;
    }
    assert_eq!(checked, 92);
}

#[test]
fn partly_aligned_verses_read_with_tc4s_bank_order() {
    let weave = weave_cases();
    let mut checked = 0;
    for case in vectors("extract-vectors.json")
        .iter()
        .filter(|c| c["alone"].is_object())
    {
        let name = case["ref"].as_str().unwrap();
        let woven = weave
            .iter()
            .find(|c| c["source"] == name)
            .and_then(|c| c["woven"].as_str())
            .unwrap();
        let extracted = from_aligned(&parse_aligned(woven).unwrap(), None);
        let value = with(extracted.alignments, extracted.word_bank);
        let expected = &case["alone"];
        assert_eq!(cards(&value), json_cards(&expected["alignments"]), "{name}");
        assert_eq!(
            value.word_bank,
            target_words(&expected["wordBank"]),
            "{name}"
        );
        checked += 1;
    }
    assert_eq!(checked, 136);
}

fn weave_cases() -> Vec<Value> {
    let v: Value = serde_json::from_str(&read("fixtures/usfm/weave-vectors.json")).unwrap();
    v["cases"].as_array().unwrap().clone()
}

fn with(alignments: Vec<Group>, word_bank: Vec<tc_align::TargetWord>) -> AlignmentValue {
    let mut value = AlignmentValue::bootstrap(
        &tc_align::SourceVerse::default(),
        &TargetVerse::new(""),
        basis(1),
    );
    value.alignments = alignments;
    value.word_bank = word_bank;
    value
}

/// Read every published verse of en_ult and en_ust Titus into cards as tC4 does, weave
/// them back, and get the bytes tC4's export writes: the published bytes for 87 of the
/// 92 verses, and for the other 5 the same links with each card's words in one run.
#[test]
fn export_of_read_back_alignments_writes_tc4s_bytes() {
    let mut identical_to_published = 0;
    let mut checked = 0;
    for case in weave_cases()
        .iter()
        .filter(|c| c["aligned"].is_string() && c["source"].as_str().unwrap().contains("en_u"))
    {
        let aligned_text = case["aligned"].as_str().unwrap();
        let plain = case["plain"].as_str().unwrap();
        let tc4 = case["woven"].as_str().unwrap_or(aligned_text);
        let aligned = parse_aligned(aligned_text).unwrap();
        assert_eq!(aligned.plain, plain);
        let extracted = from_aligned(&aligned, None);
        assert!(extracted.word_bank.is_empty());
        let value = with(extracted.alignments, extracted.word_bank);
        let woven = weave_verse(plain, &value).unwrap();
        assert_eq!(woven, tc4, "{}", case["source"]);
        if woven == aligned_text {
            identical_to_published += 1;
        }
        // The same links come back.
        assert_eq!(
            links(&parse_aligned(&woven).unwrap().groups),
            links(&aligned.groups),
            "{}",
            case["source"]
        );
        checked += 1;
    }
    assert_eq!(checked, 92);
    assert_eq!(identical_to_published, 87);
}

/// A word by text and occurrence.
type WordId = (String, u32);

/// Each target word and the source words above it.
fn links(groups: &[tc_usfm::AlignmentGroup]) -> Vec<(WordId, Vec<WordId>)> {
    let mut out: Vec<_> = groups
        .iter()
        .filter(|g| !g.sources.is_empty())
        .flat_map(|g| {
            let sources: Vec<_> = g
                .sources
                .iter()
                .map(|s| (s.content.clone(), s.occurrence))
                .collect();
            g.targets
                .iter()
                .map(move |t| ((t.text.clone(), t.occurrence), sources.clone()))
        })
        .collect();
    out.sort();
    out
}

#[test]
fn cards_without_words_write_nothing() {
    let source = source_verse(UGNT, "1:1");
    let verse = TargetVerse::new("Paul, a servant of God");
    let mut value = AlignmentValue::bootstrap(&source, &verse, basis(1));
    assert!(usfm_groups(&value).is_empty());
    let paul = value.word_bank[0].clone();
    value.link(0, &paul).unwrap();
    let groups = usfm_groups(&value);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].sources[0].content, "Παῦλος");
    let woven = weave_verse("Paul, a servant of God", &value).unwrap();
    assert!(woven.starts_with(
        "\\zaln-s |x-strong=\"G39720\" x-lemma=\"Παῦλος\" x-morph=\"Gr,N,,,,,NMS,\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"Παῦλος\"\\*\\w Paul|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\\zaln-e\\*,"
    ));
}

#[test]
fn the_sample_projects_record_reads_writes_back_byte_for_byte_and_is_valid() {
    let file: Value = serde_json::from_str(&read(
        "fixtures/tc4/sample-burrito/ingredients/checking/alignments/TIT.json",
    ))
    .unwrap();
    let stored = &file["chapters"]["1"]["1"];
    let value = record::from_record(stored, basis(1)).unwrap();
    assert_eq!(value.alignments.len(), 10);
    assert_eq!(value.word_bank.len(), 21);
    assert!(!value.done && !value.marked_invalid);
    assert_eq!(
        serde_json::to_string(&record::to_record(&value, false)).unwrap(),
        serde_json::to_string(stored).unwrap()
    );
    // Its hash is the md5 of the verse's plain text, so it is valid for that verse.
    let body = verse_body("fixtures/tc4/sample-burrito/ingredients/TIT.usfm", "1:1");
    let verse = TargetVerse::new(&tc_usfm::plain_text(&body));
    let now = Current {
        run_end: NonZeroU16::MIN,
        text_md5: verse.md5(),
        source_sha: None,
    };
    assert!(value.is_valid(&now));
    assert_eq!(
        verse_status(Some(&value), Some(&now)).state,
        VerseState::Todo
    );
    // Its words are the verse's words.
    let mut held: Vec<_> = value.all_target_words().cloned().collect();
    let mut words = verse.words().to_vec();
    held.sort_by(|a, b| (&a.text, a.occurrence).cmp(&(&b.text, b.occurrence)));
    words.sort_by(|a, b| (&a.text, a.occurrence).cmp(&(&b.text, b.occurrence)));
    assert_eq!(held, words);
}

#[test]
fn a_record_that_would_lose_data_is_refused() {
    let file: Value = serde_json::from_str(&read(
        "fixtures/tc4/sample-burrito/ingredients/checking/alignments/TIT.json",
    ))
    .unwrap();
    let mut stored = file["chapters"]["1"]["1"].clone();
    stored["wordBank"][0]["disabled"] = Value::Bool(true);
    assert!(record::from_record(&stored, basis(1)).is_err());
    let mut stored = file["chapters"]["1"]["1"].clone();
    stored["alignments"][0]["bottomWords"][0]["occurrence"] = Value::from(0);
    assert!(record::from_record(&stored, basis(1)).is_err());
    let mut stored = file["chapters"]["1"]["1"].clone();
    stored["targetVerseMd5"] = Value::from("F8AEE622DAFF1496BCCC7C2E3B358B43");
    assert!(record::from_record(&stored, basis(1)).is_err());
    // Strings of digits are occurrences, as tC4 normalizes them; unknown fields are kept.
    let mut stored = file["chapters"]["1"]["1"].clone();
    stored["alignments"][0]["topWords"][0]["occurrence"] = Value::from("1");
    stored["note"] = Value::from("kept");
    let value = record::from_record(&stored, basis(1)).unwrap();
    assert_eq!(value.alignments[0].sources[0].occurrence, 1);
    assert_eq!(record::to_record(&value, true)["note"], "kept");
}
