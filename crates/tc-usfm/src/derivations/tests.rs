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

/// 100,000 nested levels fit in a verse of 500 KB. The walk keeps its own stack, so it
/// neither overflows the thread's stack (the fold computes validity with it, and a crash
/// there would stop every open of the project) nor slows down with depth. usfm-js runs
/// out of stack a few thousand levels down, at a depth that depends on the engine;
/// version 1 is the walk with no limit, so the deep result is the shallow one, which these
/// strings are (usfm-js 3.4.3 at depths 3, 10 and 50).
#[test]
fn deep_nesting_has_the_value_of_shallow_nesting() {
    const DEEP: usize = 100_000;
    // Deep enough to overflow the recursive port in a debug build, and quicker to parse.
    const DEEPER_THAN_USFM_JS: usize = 20_000;
    let removed = |text: String| remove_marker(&js(&text)).map(|t| to_string(&t));
    let add = |n: usize| format!("words {}x", "\\add ".repeat(n));
    let zaln = |n: usize| format!("{}\\w x\\w*", "\\zaln-s |x-strong=\"G1\"\\*".repeat(n));
    let zaln_words = |n: usize| "\\zaln-s |x-strong=\"G1\"\\*\\w x\\w*".repeat(n);
    let closed = |n: usize| {
        format!(
            "{}\\w x\\w*{} y",
            "\\zaln-s |x\\*".repeat(n),
            "\\zaln-e\\*".repeat(n)
        )
    };
    for (n, m) in [(3, 3), (DEEP, DEEPER_THAN_USFM_JS)] {
        assert_eq!(removed(add(n)), Ok("words x  ".into()));
        assert_eq!(removed("\\add ".repeat(n)), Ok("   ".into()));
        assert_eq!(super::plain_text_v1(&add(n)), "words");
        assert_eq!(removed(zaln(m)), Ok("x".into()));
        assert_eq!(removed("a \\f + \\fq ".repeat(m)), Ok("a ".into()));
        assert_eq!(removed(closed(m)), Ok("x y".into()));
        assert_eq!(super::plain_text_v1(&closed(m)), "y");
    }
    let words = removed(zaln_words(DEEPER_THAN_USFM_JS)).unwrap();
    assert_eq!(words.len(), 2 * DEEPER_THAN_USFM_JS - 1);
    assert!(words.split(' ').all(|w| w == "x"));
    let some = |text, occurrences| super::SelectionCount { text, occurrences };
    // `false` is a value the walk computed: a throw would have said the selections fit.
    assert!(!super::selections_v1(&add(DEEP), &[some("x", 2)]));
}

/// An object the walk meets again below itself would make the library recurse forever,
/// which ends in a RangeError: the selections fit. No parse makes one, so the heap is
/// built by hand.
#[test]
fn a_walk_that_never_ends_is_a_throw() {
    assert_eq!(super::usfmjs::walks_of_cycles(), [Err(Thrown), Err(Thrown)]);
}
