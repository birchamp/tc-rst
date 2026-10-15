//! The weave against tC4's own aligned-USFM export, word-aligner-lib 1.0.1's
//! `addAlignmentsToTargetVerseUsingMerge`, on `fixtures/usfm/weave-vectors.json`: every
//! verse of en_ult and en_ust Titus with the record tC4 reads from it, the sample
//! burrito's stored record, and generated alignments over hand-written verses.

// Helpers outside `#[test]` functions read fixtures and may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::links;
use serde_json::Value;
use std::path::Path;
use tc_usfm::{AlignmentGroup, SourceWord, TargetWord, parse_aligned, weave};

struct Case {
    source: String,
    plain: String,
    groups: Vec<AlignmentGroup>,
    /// What tC4 writes for `plain` and `groups`.
    tc4: String,
    /// The verse as published, for the en_ult and en_ust cases.
    aligned: Option<String>,
}

fn text(value: &Value, key: &str) -> String {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} in {value}"))
        .to_owned()
}

fn count(value: &Value, key: &str) -> u32 {
    u32::try_from(value[key].as_u64().unwrap()).unwrap()
}

/// One tC4 §5.1 alignment becomes one group: its top words are the sources, its bottom
/// words the targets. An alignment without bottom words writes nothing in either weave.
fn group(alignment: &Value) -> AlignmentGroup {
    let optional = |w: &Value, key: &str| w[key].as_str().map(str::to_owned);
    AlignmentGroup {
        sources: alignment["topWords"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| SourceWord {
                content: text(w, "word"),
                strong: optional(w, "strong"),
                lemma: optional(w, "lemma"),
                morph: optional(w, "morph"),
                occurrence: count(w, "occurrence"),
                occurrences: count(w, "occurrences"),
                extra: Vec::new(),
            })
            .collect(),
        targets: alignment["bottomWords"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| TargetWord {
                text: text(w, "word"),
                occurrence: count(w, "occurrence"),
                occurrences: count(w, "occurrences"),
            })
            .collect(),
    }
}

fn cases() -> Vec<Case> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/usfm/weave-vectors.json");
    let vectors: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    vectors["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let aligned = c["aligned"].as_str().map(str::to_owned);
            Case {
                source: text(c, "source"),
                plain: text(c, "plain"),
                groups: c["alignments"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(group)
                    .filter(|g| !g.targets.is_empty())
                    .collect(),
                tc4: c["woven"]
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| aligned.clone())
                    .unwrap(),
                aligned,
            }
        })
        .collect()
}

/// tC4 moves the words of a character span out of it (`\nd Lord\nd*` becomes
/// `\nd \nd*` and then `Lord`), which changes the text; weave keeps them in the span.
fn has_character_span(plain: &str) -> bool {
    plain.contains("\\nd ") || plain.contains("\\add ")
}

#[test]
fn weave_writes_what_tc4_writes() {
    let all = cases();
    assert_eq!(all.len(), 229);
    let mut identical = 0;
    for case in &all {
        let ours =
            weave(&case.plain, &case.groups).unwrap_or_else(|e| panic!("{}: {e}", case.source));
        if has_character_span(&case.plain) {
            assert_ne!(ours, case.tc4, "{}", case.source);
            let tc4 = parse_aligned(&case.tc4).unwrap();
            assert_eq!(
                links(&tc4.groups),
                links(&parse_aligned(&ours).unwrap().groups),
                "{}",
                case.source
            );
        } else if case.plain.contains('\r') {
            // tC4 drops the CR of every CRLF line end; nothing else differs.
            assert_eq!(ours.replace('\r', ""), case.tc4, "{}", case.source);
        } else {
            assert_eq!(ours, case.tc4, "{}", case.source);
            identical += 1;
        }
    }
    assert_eq!(identical, 205);
}

#[test]
fn woven_verses_take_apart_into_their_plain_text_and_meaning() {
    for case in cases() {
        let woven = weave(&case.plain, &case.groups).unwrap();
        let parsed = parse_aligned(&woven).unwrap();
        assert_eq!(parsed.plain, case.plain, "{}", case.source);
        assert_eq!(
            links(&parsed.groups),
            links(&case.groups),
            "{}",
            case.source
        );
    }
}

#[test]
fn strip_reads_published_verses_as_tc4_does() {
    let mut checked = 0;
    for case in cases() {
        let Some(aligned) = &case.aligned else {
            continue;
        };
        let parsed = parse_aligned(aligned).unwrap();
        assert_eq!(parsed.plain, case.plain, "{}", case.source);
        // tC4's record merges the runs of one source chain; the meaning is the same.
        assert_eq!(
            links(&parsed.groups),
            links(&case.groups),
            "{}",
            case.source
        );
        checked += 1;
    }
    assert_eq!(checked, 92);
}
