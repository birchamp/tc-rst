//! The bootstrap, aligned USFM and record against what tC4's own code returns for the
//! real fixtures (`fixtures/align/*-vectors.json`, made by `tools/gen-align-vectors.cjs`):
//! en_ult and en_ust Titus with UGNT Titus, and UHB Jonah (right to left).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use tc_align::Alignment;
use tc_align::bootstrap;

use common::*;
use serde_json::Value;
use tc_align::usfm::{from_aligned, usfm_groups, weave_verse};
use tc_align::{
    AlignedRun, AlignmentGroup, AlignmentValue, Malformed, TargetVerse, VerseState, record,
    verse_status,
};
use tc_journal::json::Json;
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
        let value = bootstrap(&source, &verse, basis(span_end));
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
                value.word_bank.iter().any(|w| w.word.contains('\u{2060}')),
                "{at}"
            );
            assert!(
                source.words().iter().any(|w| w.word.contains('\u{2060}')),
                "{at}"
            );
        }
    }
    assert_eq!(hebrew, 17);
}

#[test]
fn bootstrap_gives_cards_to_original_words_after_a_heading_inside_the_verse_as_tc4_does() {
    // A mid-verse `\s5`, `\r`, `\qa`, `\sp`, `\d`, `\ms`, `\s` or `\cl` and the words after
    // it; a footnote whose words are not verse words; a heading with words between the
    // last verse of chapter 1 and `\c 2`, which tC4 counts in 1:11, and a `\cl` and a
    // heading after `\c 2`, which no verse counts; and spans across both kinds of heading.
    // Chapters 3 and 4: sidebars inside a verse, before the next `\v` and before `\c 4`, and
    // the rarer spans usfm-js hides (`\pro`, `\xt`, `\ior`...), whose words tC4 does not
    // count; a footnote that stays open across a paragraph to its `\f*` or to the next `\v`;
    // and footnotes inside `\add` and `\k-s`, whose words tC4 counts until an end marker or
    // a chunk milestone that matches neither has ended the `\add` or `\k-s`.
    let cases = vectors("heading-vectors.json");
    assert_eq!(cases.len(), 32);
    assert_bootstrap_cards(&cases);
}

#[test]
fn bootstrap_counts_the_words_inside_each_span_as_tc4_does() {
    // A word and then a footnote inside each of the 96 spans usfm-js knows and 11 it does
    // not, such as `\+jmp`: tC4 counts both inside the 68 it shows, neither inside the 28
    // it hides, and only the word inside one it does not know.
    let cases = vectors("span-vectors.json");
    assert_eq!(cases.len(), 107);
    assert_bootstrap_cards(&cases);
}

/// The cards of each case's bootstrap are the cards of tC4's.
fn assert_bootstrap_cards(cases: &[Value]) {
    for case in cases {
        let reference = case["ref"].as_str().unwrap();
        let source = source_verse(case["source"].as_str().unwrap(), reference);
        let verse = TargetVerse::new(case["text"].as_str().unwrap());
        let span_end = parse_ref(reference).1.end();
        let value = bootstrap(&source, &verse, basis(span_end));
        assert_eq!(
            cards(&value),
            json_cards(&case["record"]["alignments"]),
            "{reference}"
        );
    }
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

fn with(alignments: Vec<AlignmentGroup>, word_bank: Vec<tc_align::TargetWord>) -> AlignmentValue {
    let mut value = bootstrap(
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
    let mut value = bootstrap(&source, &verse, basis(1));
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

/// The record reads into the journal's value; the journal projects it back (its projection
/// is checked against this file, field for field, in tc-journal's `screens_and_fold` test).
#[test]
fn the_sample_projects_record_reads_and_is_valid() {
    let file: Value = serde_json::from_str(&read(
        "fixtures/tc4/sample-burrito/ingredients/checking/alignments/TIT.json",
    ))
    .unwrap();
    let stored = &file["chapters"]["1"]["1"];
    let value = record::from_record(stored, basis(1)).unwrap();
    assert_eq!(value.alignments.len(), 10);
    assert_eq!(value.word_bank.len(), 21);
    assert!(!value.done && !value.marked_invalid);
    // Its hash is the md5 of the verse's plain text (plain_text_v1 of its content), so it is
    // valid for that verse.
    let body = verse_body("fixtures/tc4/sample-burrito/ingredients/TIT.usfm", "1:1");
    let verse = TargetVerse::new(&body);
    let now = AlignedRun {
        anchor: 1,
        run_start: 1,
        run_end: 1,
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
    held.sort_by(|a, b| (&a.word, a.occurrence).cmp(&(&b.word, b.occurrence)));
    words.sort_by(|a, b| (&a.word, a.occurrence).cmp(&(&b.word, b.occurrence)));
    assert_eq!(held, words);
}

#[test]
fn a_record_that_breaks_the_rules_is_refused_and_nothing_it_holds_is_dropped() {
    let file: Value = serde_json::from_str(&read(
        "fixtures/tc4/sample-burrito/ingredients/checking/alignments/TIT.json",
    ))
    .unwrap();
    let original = file["chapters"]["1"]["1"].clone();
    let refused = |edit: &dyn Fn(&mut Value)| {
        let mut stored = original.clone();
        edit(&mut stored);
        record::from_record(&stored, basis(1)).unwrap_err()
    };
    refused(&|r| r["alignments"][0]["bottomWords"][0]["occurrence"] = Value::from(0));
    refused(&|r| r["targetVerseMd5"] = Value::from("F8AEE622DAFF1496BCCC7C2E3B358B43"));
    // A word placed and banked at once, or past its count.
    assert!(matches!(
        refused(&|r| {
            let placed = r["alignments"][0]["bottomWords"][0].clone();
            r["wordBank"].as_array_mut().unwrap().push(placed);
        }),
        tc_align::Error::Malformed(Malformed::TargetWordTwice(_))
    ));
    assert!(matches!(
        refused(&|r| r["wordBank"][0]["occurrence"] = Value::from(9)),
        tc_align::Error::Malformed(Malformed::TargetOccurrence(_))
    ));
    assert!(matches!(
        refused(&|r| r["alignments"][0]["topWords"][0]["occurrence"] = Value::from(2)),
        tc_align::Error::Malformed(Malformed::SourceOccurrence(_))
    ));

    // Strings of digits are occurrences, as tC4 normalizes them; unknown fields are kept,
    // on the record, on a card and on a target word, and the word's go with it to a card.
    let mut stored = original.clone();
    stored["alignments"][0]["topWords"][0]["occurrence"] = Value::from("1");
    stored["note"] = Value::from("kept");
    stored["alignments"][0]["note"] = Value::from("kept");
    stored["wordBank"][0]["disabled"] = Value::Bool(true);
    let mut value = record::from_record(&stored, basis(1)).unwrap();
    assert_eq!(value.alignments[0].top_words[0].occurrence, 1);
    assert_eq!(value.extra.get("note"), Some(&Json::from("kept")));
    assert_eq!(
        value.alignments[0].extra.get("note"),
        Some(&Json::from("kept"))
    );
    assert_eq!(
        value.word_bank[0].extra.get("disabled"),
        Some(&Json::Bool(true))
    );
    let word = value.word_bank[0].clone();
    value.link(1, &word).unwrap();
    let placed = value.alignments[1].bottom_words.last().unwrap();
    assert_eq!(placed.extra.get("disabled"), Some(&Json::Bool(true)));
}
