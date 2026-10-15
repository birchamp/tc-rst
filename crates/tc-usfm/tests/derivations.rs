//! The version-1 derivations against golden outputs of the JavaScript libraries tC4 pins
//! (`docs/STORAGE.md` §7.7, §16.5 item 4): `fixtures/derivations/`, made by
//! `tools/gen-derivation-vectors.cjs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use md5::{Digest, Md5};
use proptest::prelude::*;
use std::path::PathBuf;
use tc_usfm::{SelectionCount, plain_text_v1, selections_v1, tokenize_v1};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn vectors(name: &str) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(fixtures().join("derivations").join(name)).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    value["cases"].as_array().unwrap().clone()
}

fn md5_hex(text: &str) -> String {
    Md5::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Compares every case that usfm-js finishes; returns how many it could not finish.
fn check_plain_text(cases: &[serde_json::Value]) -> usize {
    let mut failures = Vec::new();
    let mut unfinished = 0;
    for case in cases {
        let content = case["content"].as_str().unwrap();
        match case["text"].as_str() {
            Some(expected) => {
                let ours = plain_text_v1(content);
                if ours != expected {
                    failures.push(format!(
                        "{content:?}\n  ours    {ours:?}\n  usfm-js {expected:?}"
                    ));
                }
            }
            None => unfinished += 1,
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
    unfinished
}

#[test]
fn plain_text_v1_is_the_text_tc4_hashes() {
    let cases = vectors("plain-text-v1.json");
    assert!(cases.len() > 1500);
    check_plain_text(&cases);
}

/// The hash tC4 stored for the alignment of TIT 1:1 in its sample project.
#[test]
fn plain_text_v1_gives_tc4s_sample_hash() {
    let usfm = std::fs::read_to_string(fixtures().join("tc4/sample-burrito/ingredients/TIT.usfm"))
        .unwrap();
    let parts = tc_usfm::codec::decompose(&usfm).unwrap();
    let Some(tc_usfm::codec::VersePart::Text(verse)) = parts.chapters[&1].verses.get(&1) else {
        panic!("TIT 1:1 has text");
    };
    let content = tc_usfm::codec::join_verse(&verse.words, &verse.layout);
    assert_eq!(
        md5_hex(&plain_text_v1(&content)),
        "f8aee622daff1496bccc7c2e3b358b43"
    );
}

#[test]
fn plain_text_v1_keeps_tc4s_quirks() {
    // Poetry line breaks stay; the text of `\w` words inside `\zaln` milestones does not.
    assert_eq!(
        plain_text_v1("line one\n\\q2 line two\n\\q1\n"),
        "line one\nline two"
    );
    assert_eq!(
        plain_text_v1("\\zaln-s |x-content=\"a\"\\*\\w one\\w*\\zaln-e\\*, two"),
        ", two"
    );
    // A heading is not displayable text in usfm-js.
    assert_eq!(plain_text_v1("words\n\\s1 Heading"), "words");
    // Text the library cannot finish parsing falls back to the trimmed content.
    assert_eq!(plain_text_v1("  "), "");
}

#[test]
fn tokenize_v1_lists_words_and_numbers() {
    let cases = vectors("tokenize-v1.json");
    for case in &cases {
        let text = case["text"].as_str().unwrap();
        let expected: Vec<&str> = case["words"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w.as_str().unwrap())
            .collect();
        assert_eq!(tokenize_v1(text), expected, "{text:?}");
    }
    assert_eq!(
        tokenize_v1("Pablo, siervo 1:2 de—Dios"),
        ["Pablo", "siervo", "1", "2", "de", "Dios"]
    );
}

#[test]
fn selections_v1_follows_validate_verse_selections() {
    let cases = vectors("selections-v1.json");
    let mut unfinished = 0;
    for case in &cases {
        let text = case["text"].as_str().unwrap();
        let selections: Vec<SelectionCount<'_>> = case["selections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| SelectionCount {
                text: s["text"].as_str().unwrap(),
                occurrences: u32::try_from(s["occurrences"].as_i64().unwrap()).unwrap_or(u32::MAX),
            })
            .collect();
        // tC4 reads a validator that throws as "not stale" (`decisionIsStale`).
        let changed = if case["throws"].as_bool() == Some(true) {
            unfinished += 1;
            false
        } else {
            case["changed"].as_bool().unwrap()
        };
        assert_eq!(
            !selections_v1(text, &selections),
            changed,
            "{text:?} {selections:?}"
        );
    }
    assert!(unfinished > 0);
    let some = |text: &'static str, occurrences| SelectionCount { text, occurrences };
    assert!(selections_v1(
        "de Dios y de",
        &[some("de", 2), some("Dios", 1)]
    ));
    assert!(!selections_v1("de Dios", &[some("de", 2)]));
    assert!(selections_v1("anything", &[]));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// The derivations are total: any text, USFM-like or not, has a value.
    #[test]
    fn derivations_never_panic(text in "(\\\\[a-z+*]{0,6}[0-9]?|[ \\n\\r\\t|*\"=-]|\\PC){0,40}") {
        let plain = plain_text_v1(&text);
        prop_assert_eq!(plain.trim(), plain.as_str());
        let words = tokenize_v1(&plain);
        let selections: Vec<SelectionCount<'_>> =
            words.iter().map(|w| SelectionCount { text: w, occurrences: 1 }).collect();
        let _ = selections_v1(&text, &selections);
    }
}

/// The long differential run: `TC_USFM_DERIVATION_VECTORS` names a file written by
/// `tools/gen-derivation-vectors.cjs --random N --seed S --out FILE`.
#[test]
#[ignore = "needs a generated corpus; see the doc comment"]
fn plain_text_v1_matches_a_generated_corpus() {
    let Some(path) = std::env::var_os("TC_USFM_DERIVATION_VECTORS") else {
        return;
    };
    let text = std::fs::read_to_string(path).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    let cases = value["cases"].as_array().unwrap();
    let unfinished = check_plain_text(cases);
    eprintln!(
        "{} cases, {unfinished} that usfm-js cannot finish",
        cases.len()
    );
}
