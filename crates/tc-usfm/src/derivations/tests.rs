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

/// The shown text before tC4 #782 (`verseText` of `src/data/bookModel.js:15` at d310bf3),
/// kept as the reference for verses that nest no alignment: each top-level object gives its
/// `text`, or else the `text` of its children one level down.
fn one_level(content: &str) -> String {
    use super::js::{JsStr, concat, trim};
    use super::usfmjs::{Key, Ref, Val, truthy};
    let parsed = to_json_chunk(&concat(&[&js("\\v 1 "), &js(content)]));
    let fallback = || to_string(trim(&js(content)));
    if parsed.thrown {
        return fallback();
    }
    let heap = &parsed.heap;
    let text_of = |object: Ref| heap.get(object, &Key::Text).filter(|v| truthy(v));
    let mut text: JsStr = Vec::new();
    for (_, verse) in &parsed.verses {
        for object in heap.elems(*verse) {
            if let Some(value) = text_of(*object) {
                text.extend(heap.string_of(Some(value)));
                continue;
            }
            match heap.get(*object, &Key::Children) {
                None => {}
                Some(Val::Ref(list)) if heap.is_array(*list) => {
                    for child in heap.elems(*list) {
                        if let Some(value) = text_of(*child) {
                            text.extend(heap.string_of(Some(value)));
                        }
                    }
                }
                Some(_) => return fallback(),
            }
        }
    }
    to_string(trim(&text))
}

/// tC4's `nests`: an alignment milestone inside another object (another `\zaln`, or a
/// character span such as `\qs … \qs*`), at any depth.
fn nests(content: &str) -> bool {
    use super::js::{concat, eq};
    use super::usfmjs::{Key, Val};
    let parsed = to_json_chunk(&concat(&[&js("\\v 1 "), &js(content)]));
    let heap = &parsed.heap;
    let mut stack: Vec<(usize, bool)> = parsed
        .verses
        .iter()
        .flat_map(|(_, verse)| heap.elems(*verse).iter().map(|o| (*o, false)))
        .collect();
    while let Some((object, inside)) = stack.pop() {
        let is = |key: &Key, value: &str| heap.s(object, key).is_some_and(|s| eq(s, value));
        if inside && is(&Key::Tag, "zaln") && is(&Key::Type, "milestone") {
            return true;
        }
        if let Some(Val::Ref(list)) = heap.get(object, &Key::Children) {
            stack.extend(heap.elems(*list).iter().map(|o| (*o, true)));
        }
    }
    false
}

/// Every verse of a fixture book, as `chapter:verse` and its content.
fn fixture_verses(rel: &str) -> Vec<(String, String)> {
    let path = format!(
        "{}/../../fixtures/tc4/test-fixtures/{rel}",
        env!("CARGO_MANIFEST_DIR")
    );
    let raw = std::fs::read_to_string(&path).unwrap();
    let parts = crate::codec::decompose(&raw).unwrap();
    let mut out = Vec::new();
    for (c, chapter) in &parts.chapters {
        for (v, part) in &chapter.verses {
            if let crate::codec::VersePart::Text(verse) = part {
                out.push((
                    format!("{c}:{}", crate::codec::label(*v, verse.last)),
                    crate::codec::join_verse(&verse.words, &verse.layout),
                ));
            }
        }
    }
    out
}

/// tC4's way 3 of `test/bookModel-nested-alignment.test.ts`: a verse that nests no alignment
/// (plain, footnoted, poetry with line breaks, aligned one level deep) keeps the text it had
/// before #782, so a stored Check selection or an alignment still matches it.
#[test]
fn a_verse_with_no_nested_alignment_keeps_its_one_level_text() {
    let mut compared = 0;
    let mut differ = Vec::new();
    for rel in [
        "usfm-corpus/exotic-poetry-footnotes.usfm",
        "usfm-corpus/exotic-partial-book.usfm",
        "usfm-corpus/sample-TIT-draft.usfm",
        "usfm-corpus/sample-JON-span.usfm",
        "usfm-corpus/en_ult-TIT-aligned.usfm",
        "usfm-corpus/en_ust-TIT-aligned.usfm",
        "en_ult/GEN-1.usfm",
        "en_ult/PSA-3.usfm",
    ] {
        for (reference, content) in fixture_verses(rel) {
            if nests(&content) {
                continue;
            }
            compared += 1;
            if super::shown_text_v1(&content) != one_level(&content) {
                differ.push(format!("{rel} {reference}"));
            }
        }
    }
    assert_eq!(differ, Vec::<String>::new());
    assert!(compared > 50, "{compared}");
    let poem = fixture_verses("usfm-corpus/exotic-poetry-footnotes.usfm");
    let (_, four_five) = poem.iter().find(|(r, _)| r == "1:4-5").unwrap();
    assert!(super::shown_text_v1(four_five).contains('\n'));
    // The defect the walk removes: one level down, Psalm 3:2 loses its aligned "Selah".
    let psalm = fixture_verses("en_ult/PSA-3.usfm");
    let (_, selah) = psalm.iter().find(|(r, _)| r == "3:2").unwrap();
    assert!(nests(selah));
    assert!(!one_level(selah).contains("Selah"));
    assert!(super::shown_text_v1(selah).ends_with("\nSelah"));
}

/// 20,000 nested alignments and 100,000 nested character spans give the value of three: the
/// walk keeps its own stack (the store's reflow and the aligner read the shown text, and a
/// stack overflow would abort the program).
#[test]
fn the_shown_text_has_no_depth_limit() {
    let zaln = |n: usize| {
        format!(
            "a {}\\w x\\w*{} b",
            "\\zaln-s |x-content=\"a\"\\*".repeat(n),
            "\\zaln-e\\*".repeat(n)
        )
    };
    let add = |n: usize| format!("words {}x", "\\add ".repeat(n));
    assert_eq!(super::shown_text_v1(&zaln(3)), "a x b");
    assert_eq!(super::shown_text_v1(&zaln(20_000)), "a x b");
    let shallow = super::shown_text_v1(&add(3));
    assert_eq!(shallow, "words x");
    assert_eq!(super::shown_text_v1(&add(100_000)), shallow);
}

/// A list met again inside itself would make tC4's recursion run until a RangeError. No parse
/// makes one, so the verse is built by hand.
#[test]
fn a_shown_walk_that_never_ends_is_a_throw() {
    assert_eq!(
        super::shown::text(&super::usfmjs::cyclic_verse()),
        Err(Thrown)
    );
}
