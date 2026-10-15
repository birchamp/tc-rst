//! The parts of the port that the public functions do not show: `removeMarker` and where
//! the library throws.

use super::js::{js, to_string};
use super::usfmjs::{Thrown, remove_marker, to_json_chunk};

fn vectors() -> serde_json::Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/derivations/plain-text-v1.json"
    );
    let text = std::fs::read_to_string(path).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn remove_marker_matches_usfm_js_on_every_vector() {
    check_remove_marker(&vectors());
}

/// The long differential run of `tests/derivations.rs`, for `removeMarker`.
#[test]
#[ignore = "needs a generated corpus named by TC_USFM_DERIVATION_VECTORS"]
fn remove_marker_matches_a_generated_corpus() {
    let Some(path) = std::env::var_os("TC_USFM_DERIVATION_VECTORS") else {
        return;
    };
    let text = std::fs::read_to_string(path).unwrap();
    check_remove_marker(&serde_json::from_str(&text).unwrap());
}

fn check_remove_marker(vectors: &serde_json::Value) {
    let mut failures = Vec::new();
    let mut thrown = 0;
    for case in vectors["cases"].as_array().unwrap() {
        let content = case["content"].as_str().unwrap();
        let ours = remove_marker(&js(content));
        match (&ours, case["removed"].as_str()) {
            (Ok(ours), Some(theirs)) if to_string(ours) == theirs => {}
            (Err(Thrown), None) => thrown += 1,
            _ => failures.push(format!(
                "{content:?}: ours {ours:?}, usfm-js {:?}",
                case["removed"]
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of them:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(
        thrown > 0,
        "the vectors hold inputs on which usfm-js throws"
    );
}

#[test]
fn a_parse_that_usfm_js_cannot_finish_is_reported() {
    let parsed = to_json_chunk(&js("\\v 1 words"));
    assert!(!parsed.thrown);
    assert_eq!(parsed.verses.len(), 1);
}
