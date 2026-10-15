//! tC4's scheme resolution ladder (tC4 `test/versification.test.ts`), against the
//! bundled schemes.

// Test helpers fail the test by panicking.
#![allow(clippy::unwrap_used, clippy::panic)]

use serde_json::Value;
use tc_versification::{
    SchemeResolution, StandardScheme, UNRECORDED_SCHEME, VrsRegister, canonical_scheme,
    is_scheme_name, resolve_scheme,
};

fn standard() -> Vec<(&'static str, &'static str)> {
    StandardScheme::ALL
        .into_iter()
        .map(|s| (s.name(), s.json()))
        .collect()
}

fn unrecorded(json: &str) -> Option<VrsRegister<'_>> {
    Some(VrsRegister {
        name: UNRECORDED_SCHEME,
        json,
    })
}

fn fingerprint(name: &str) -> SchemeResolution {
    SchemeResolution::Fingerprint(name.to_owned())
}

/// The same document with its keys in alphabetical order and pretty-printed, as a
/// published Septuagint burrito carries `org`.
fn reserialized(json: &str) -> String {
    let doc: Value = serde_json::from_str(json).unwrap();
    let Value::Object(map) = doc else {
        panic!("not an object")
    };
    let mut entries: Vec<(String, Value)> = map.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    serde_json::to_string_pretty(&Value::Object(entries.into_iter().collect())).unwrap()
}

/// A seventh scheme: eng's psalm counts plus a superscription, the French LSG case.
const LSG: &str = r#"{"maxVerses":{"PSA":["6","12","9"],"JHN":["51"]},"mappedVerses":{"PSA 3:1-9":"PSA 3:0-8"},"excludedVerses":[],"partialVerses":{}}"#;

#[test]
fn the_placeholder_is_not_a_scheme_name() {
    assert!(!is_scheme_name(UNRECORDED_SCHEME));
    assert!(!is_scheme_name(""));
    assert!(is_scheme_name("eng"));
    assert!(is_scheme_name("lsg"));
}

#[test]
fn a_recorded_name_wins() {
    let register = VrsRegister {
        name: "lxx",
        json: "{}",
    };
    assert_eq!(
        resolve_scheme(Some(register), &standard()),
        SchemeResolution::Recorded("lxx".into())
    );
}

#[test]
fn a_recorded_name_the_caller_lacks_falls_through_to_the_fingerprint() {
    let register = VrsRegister {
        name: "lsg",
        json: StandardScheme::Lxx.json(),
    };
    assert_eq!(
        resolve_scheme(Some(register), &standard()),
        fingerprint("lxx")
    );
}

#[test]
fn a_project_tc4_did_not_create_is_fingerprinted() {
    for which in StandardScheme::ALL {
        assert_eq!(
            resolve_scheme(unrecorded(which.json()), &standard()),
            fingerprint(which.name())
        );
    }
}

#[test]
fn identity_is_meaning_not_bytes() {
    let org = StandardScheme::Org.json();
    let other = reserialized(org);
    assert_ne!(other, org);
    assert_eq!(canonical_scheme(&other), canonical_scheme(org));
    assert_eq!(
        resolve_scheme(unrecorded(&other), &standard()),
        fingerprint("org")
    );
    // The string and array forms of a mapping mean the same; numbers compare by value.
    assert_eq!(
        canonical_scheme(r#"{"mappedVerses":{"GEN 31:55":"GEN 32:1"},"n":1}"#),
        canonical_scheme(r#"{"n":1.0,"mappedVerses":{"GEN 31:55":["GEN 32:1"]}}"#)
    );
    assert_ne!(
        canonical_scheme(r#"{"maxVerses":{"GEN":["31"]}}"#),
        canonical_scheme(r#"{"maxVerses":{"GEN":["32"]}}"#)
    );
}

#[test]
fn anything_else_is_unknown_never_eng() {
    let known = standard();
    assert_eq!(
        resolve_scheme(unrecorded(r#"{"maxVerses":{"GEN":["99"]}}"#), &known),
        SchemeResolution::Unknown
    );
    assert_eq!(resolve_scheme(None, &known), SchemeResolution::Unknown);
    assert_eq!(
        resolve_scheme(unrecorded("not json"), &known),
        SchemeResolution::Unknown
    );
    assert_eq!(
        resolve_scheme(unrecorded("null"), &known),
        SchemeResolution::Unknown
    );
    assert_eq!(SchemeResolution::Unknown.name(), None);
}

#[test]
fn a_seventh_scheme_resolves_like_any_other() {
    let mut known = standard();
    known.push(("lsg", LSG));
    let register = VrsRegister {
        name: "lsg",
        json: "{}",
    };
    assert_eq!(
        resolve_scheme(Some(register), &known),
        SchemeResolution::Recorded("lsg".into())
    );
    assert_eq!(resolve_scheme(unrecorded(LSG), &known), fingerprint("lsg"));
    assert_eq!(fingerprint("lsg").name(), Some("lsg"));
}

#[test]
fn candidates_are_tried_in_name_order() {
    let known = [("zzz", LSG), ("aaa", LSG)];
    assert_eq!(resolve_scheme(unrecorded(LSG), &known), fingerprint("aaa"));
}
