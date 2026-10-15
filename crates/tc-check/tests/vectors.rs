//! The JavaScript libraries behind tC4's Check screens, answered by this crate: every
//! vector of `fixtures/check/check-vectors.json` (see `fixtures/check/README.md`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use serde_json::Value;
use tc_burrito::ScopeRange;
use tc_check::item::quote_words;
use tc_check::selection::{self, Selection};
use tc_check::{RefPart, carry, rail, scope};

fn vectors() -> Value {
    serde_json::from_str(&common::fixture("check/check-vectors.json")).unwrap()
}

fn text(v: &Value) -> &str {
    v.as_str().unwrap()
}

fn selections(v: &Value) -> Vec<Selection> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| Selection {
            text: text(&s["text"]).to_owned(),
            occurrence: u32::try_from(s["occurrence"].as_u64().unwrap()).unwrap(),
            occurrences: u32::try_from(s["occurrences"].as_u64().unwrap()).unwrap(),
        })
        .collect()
}

fn indices(v: &Value) -> Vec<usize> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|i| usize::try_from(i.as_u64().unwrap()).unwrap())
        .collect()
}

#[test]
fn scope_answers_match_bible_reference_range() {
    let v = vectors();
    let cases = v["scope"].as_array().unwrap();
    assert!(cases.len() > 2000);
    let mut wrong = Vec::new();
    for case in cases {
        let range: ScopeRange = text(&case["range"]).parse().unwrap();
        let chapter = RefPart::new(text(&case["chapter"])).unwrap();
        let verse = RefPart::new(text(&case["verse"])).unwrap();
        let got = scope::in_scope(std::slice::from_ref(&range), &chapter, &verse);
        if got != case["inScope"].as_bool().unwrap() {
            wrong.push(format!("{range} contains {chapter}:{verse} -> {got}"));
        }
    }
    assert!(wrong.is_empty(), "{} wrong: {wrong:#?}", wrong.len());
}

#[test]
fn selections_record_restore_and_revalidate_as_tc4_does() {
    let v = vectors();
    let cases = v["selections"].as_array().unwrap();
    assert!(cases.len() > 100);
    let mut checked = 0;
    let mut recounted = 0;
    for case in cases {
        let verse = text(&case["text"]);
        let picked = selection::from_indices(verse, &indices(&case["indices"])).unwrap();
        let tc4 = selections(&case["selections"]);
        // The same words at the same occurrences as tC4 records, so the same words light
        // up when the decision is opened again.
        let words = |s: &[Selection]| -> Vec<(String, u32)> {
            s.iter().map(|s| (s.text.clone(), s.occurrence)).collect()
        };
        assert_eq!(words(&picked), words(&tc4), "{verse:?}");
        assert_eq!(
            selection::indices(verse, &picked),
            indices(&case["restored"])
        );
        // The count is the one `validateVerseSelections` reads. tC4's recorder counted
        // words, which differs for a word that is also part of a longer one, and its own
        // validator then reads that selection as changed on the verse it was made on.
        for (ours, theirs) in picked.iter().zip(&tc4) {
            if ours.occurrences != theirs.occurrences {
                assert!(ours.occurrences > theirs.occurrences);
                assert!(selection::validate(verse, std::slice::from_ref(theirs)).changed);
                recounted += 1;
            }
        }
        assert!(!selection::validate(verse, &picked).changed, "{verse:?}");
        // The validator answers as the library does, on tC4's own records.
        for validation in case["validations"].as_array().unwrap() {
            let edited = text(&validation["text"]);
            let got = selection::validate(edited, &tc4);
            assert_eq!(
                got.changed,
                validation["changed"].as_bool().unwrap(),
                "{} of {verse:?}",
                text(&validation["edit"])
            );
            assert_eq!(got.valid, selections(&validation["valid"]));
            checked += 1;
        }
    }
    for case in v["validations"].as_array().unwrap() {
        let got = selection::validate(text(&case["text"]), &selections(&case["selections"]));
        assert_eq!(got.changed, case["changed"].as_bool().unwrap(), "{case}");
        assert_eq!(got.valid, selections(&case["valid"]));
        checked += 1;
    }
    assert!(checked > 600);
    assert!(recounted > 0);
}

#[test]
fn quote_words_match_tn_quote_words() {
    let v = vectors();
    for case in v["quoteWords"].as_array().unwrap() {
        let got: Vec<Value> = quote_words(text(&case["quote"]))
            .into_iter()
            .map(|w| serde_json::json!({"word": w.word, "occurrence": w.occurrence}))
            .collect();
        assert_eq!(Value::Array(got), case["words"], "{}", case["quote"]);
    }
}

#[test]
fn quotes_compare_as_same_orig_quote_does() {
    let v = vectors();
    let cases = v["sameQuote"].as_array().unwrap();
    let differing = cases
        .iter()
        .filter(|c| !c["same"].as_bool().unwrap())
        .count();
    assert!(differing > 10 && differing < cases.len() - 10);
    for case in cases {
        assert_eq!(
            carry::same_quote(text(&case["a"]), text(&case["b"])),
            case["same"].as_bool().unwrap(),
            "{case}"
        );
    }
}

#[test]
fn terms_sort_as_locale_compare_does() {
    let v = vectors();
    let expected: Vec<&str> = v["terms"].as_array().unwrap().iter().map(text).collect();
    let mut got = expected.clone();
    got.reverse();
    got.sort_by(|a, b| rail::locale_cmp(a, b));
    assert_eq!(got, expected);
}
