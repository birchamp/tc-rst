//! The version-1 derivations against golden outputs of the JavaScript libraries tC4 pins
//! (`docs/STORAGE.md` §7.7, §16.5 item 4): `fixtures/derivations/`, made by
//! `tools/gen-derivation-vectors.cjs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod census;

use census::run_with_census;
use md5::{Digest, Md5};
use proptest::prelude::*;
use std::path::PathBuf;
use std::time::{Duration, Instant};
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

/// The derivations are total: any text, USFM-like or not, has a value.
#[test]
fn derivations_never_panic() {
    run_with_census(
        512,
        "(\\\\[a-z+*]{0,6}[0-9]?|[ \\n\\r\\t|*\"=-]|\\PC){0,40}",
        &["no text", "text", "selections fit", "selections do not fit"],
        |text, census| {
            let plain = plain_text_v1(&text);
            prop_assert_eq!(plain.trim(), plain.as_str());
            census.saw(if plain.is_empty() { "no text" } else { "text" });
            let words = tokenize_v1(&plain);
            let selections: Vec<SelectionCount<'_>> = words
                .iter()
                .map(|w| SelectionCount {
                    text: w,
                    occurrences: 1,
                })
                .collect();
            census.saw(if selections_v1(&text, &selections) {
                "selections fit"
            } else {
                "selections do not fit"
            });
            Ok(())
        },
    );
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

/// Verses of about `bytes` bytes built to be slow: a quadratic step in the port shows here
/// as a time that grows with the square of the verse's length. Each of these took
/// quadratic time before: usfm-js builds its texts one piece at a time, which is cheap in
/// JavaScript and was a copy of the whole text per piece here.
fn hostile_verses(bytes: usize) -> Vec<(&'static str, String)> {
    let fill = |piece: &str| piece.repeat(bytes / piece.len());
    let attributes: String = (0..bytes / 8).map(|i| format!("a{i}=\"1\" ")).collect();
    vec![
        ("unclosed footnotes", fill("\\f + \\ft ")),
        (
            "closed markers in a footnote",
            format!("\\f + {}\\f*", fill("\\fq x\\fq* ")),
        ),
        ("lines of text", fill("a\n")),
        (
            "a word with many attributes",
            format!("\\w x|{attributes}\\w*"),
        ),
        (
            "attributes over many lines",
            format!("\\w a|b\n{}", fill("c\n")),
        ),
        ("nested markers with no text", fill("\\add ")),
        (
            "a long selection",
            format!("{}\u{0}{}b", "a".repeat(bytes), "a".repeat(bytes / 4)),
        ),
    ]
}

/// The time `selections_v1` takes on the verse (it parses the verse as `plain_text_v1`
/// does, then walks it), the best of `runs`.
fn time_derivations(verse: &str, runs: usize) -> Duration {
    // The text after U+0000 is a selection to count in the text before it.
    let (text, selection) = verse.split_once('\u{0}').unwrap_or((verse, "x"));
    let selections = [SelectionCount {
        text: selection,
        occurrences: 1,
    }];
    (0..runs)
        .map(|_| {
            let start = Instant::now();
            let _ = selections_v1(text, &selections);
            start.elapsed()
        })
        .min()
        .unwrap_or(Duration::MAX)
}

/// Eight times the verse takes about eight times as long, never the 64 times of a
/// quadratic step. The fold computes validity on every open, so one slow verse would
/// stall the project for everyone who opens it.
#[test]
fn derivations_take_time_linear_in_the_verse() {
    let small = 25_000;
    let mut slow = Vec::new();
    for ((name, short), (_, long)) in hostile_verses(small)
        .into_iter()
        .zip(hostile_verses(8 * small))
    {
        let base = time_derivations(&short, 3).max(Duration::from_millis(2));
        // A busy machine can stretch a run; a quadratic step is slow in every run.
        let grown = (0..3)
            .map(|_| time_derivations(&long, 1))
            .find(|t| *t <= base * 16)
            .unwrap_or_else(|| time_derivations(&long, 1));
        if grown > base * 16 {
            slow.push(format!(
                "{name}: {} bytes took {base:?}, {} bytes took {grown:?}",
                short.len(),
                long.len()
            ));
        }
    }
    assert!(slow.is_empty(), "{slow:#?}");
}

/// A verse near the 4 MiB limit of a change line (§4.6) still takes seconds, not minutes.
/// A debug build checks a quarter of it.
#[test]
fn a_verse_of_several_megabytes_takes_seconds() {
    let (bytes, limit) = if cfg!(debug_assertions) {
        (1 << 20, Duration::from_secs(60))
    } else {
        (4 << 20, Duration::from_secs(10))
    };
    let piece = "\\f + \\ft \\fq x\\fq* a\n";
    let verse = piece.repeat(bytes / piece.len());
    let took = time_derivations(&verse, 1);
    assert!(took < limit, "{} bytes took {took:?}", verse.len());
}

/// What tC4's Check screen and aligner read of each verse of a draft
/// (`fixtures/usfm/check-text-vectors.json`, made by `tools/gen-check-text-vectors.mjs` with
/// tC4's own `verseTextIndex`): whether the verse is drafted, by the body tC4's indexer finds
/// ([`tc_usfm::codec::is_drafted`]), and the text it shows ([`tc_usfm::shown_text_v1`]), for
/// every verse of tC4's plain drafts and of drafts written for poetry, superscriptions,
/// lists, notes and CRLF line ends. The one difference is a verse whose `\v` stands inside a
/// line, which tC4 does not index and the rebuild reads (§7.7).
#[test]
fn the_check_text_is_what_tc4_reads() {
    use std::collections::BTreeMap;
    use tc_usfm::codec;
    let text = std::fs::read_to_string(fixtures().join("usfm/check-text-vectors.json")).unwrap();
    let file: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut by_book: BTreeMap<&str, BTreeMap<(String, String), &serde_json::Value>> =
        BTreeMap::new();
    for case in file["cases"].as_array().unwrap() {
        by_book
            .entry(case["book"].as_str().unwrap())
            .or_default()
            .insert(
                (
                    case["chapter"].as_str().unwrap().to_owned(),
                    case["verse"].as_str().unwrap().to_owned(),
                ),
                case,
            );
    }
    let (mut drafted, mut undrafted, mut mid_line) = (0, 0, 0);
    let mut wrong = Vec::new();
    for (book, cases) in &by_book {
        let raw = match file["drafts"].get(*book) {
            Some(draft) => draft.as_str().unwrap().to_owned(),
            None => std::fs::read_to_string(fixtures().join(book.trim_start_matches("fixtures/")))
                .unwrap(),
        };
        let parts = codec::decompose(&raw).unwrap_or_else(|e| panic!("{book}: {e}"));
        let mut seen = 0;
        for (c, chapter) in &parts.chapters {
            for (v, part) in &chapter.verses {
                let codec::VersePart::Text(verse) = part else {
                    continue;
                };
                let key = (c.to_string(), codec::label(*v, verse.last));
                let content = codec::join_verse(&verse.words, &verse.layout);
                let Some(case) = cases.get(&key) else {
                    // tC4's indexer reads a `\v` only at the start of a line, so it indexes
                    // no verse whose marker stands after `\q1 ` or after text; the rebuild
                    // reads it from its marker (§7.7).
                    let ours = codec::is_drafted(verse.marker.term(), &content)
                        .then(|| tc_usfm::shown_text_v1(&content));
                    let expected = match (*book, key.1.as_str()) {
                        ("midline", "13") => {
                            Some("He will bless those fearing Yahweh,\nthe small with the great.")
                        }
                        ("midline", "14") => None,
                        ("midline", "15") => Some("May you be blessed"),
                        ("midline", "16") => Some("by Yahweh."),
                        _ => {
                            wrong.push(format!("{book} {key:?}: tC4 does not index it"));
                            continue;
                        }
                    };
                    if ours.as_deref() != expected {
                        wrong.push(format!("{book} {key:?}: {ours:?}"));
                    }
                    mid_line += 1;
                    continue;
                };
                seen += 1;
                let ours = codec::is_drafted(verse.marker.term(), &content);
                if ours != case["drafted"].as_bool().unwrap() {
                    wrong.push(format!("{book} {key:?}: drafted {ours} in {content:?}"));
                    continue;
                }
                if !ours {
                    undrafted += 1;
                    continue;
                }
                drafted += 1;
                let shown = tc_usfm::shown_text_v1(&content);
                if shown != case["text"].as_str().unwrap() {
                    wrong.push(format!("{book} {key:?}: {shown:?} != {}", case["text"]));
                }
            }
        }
        if seen != cases.len() {
            wrong.push(format!(
                "{book}: tC4 indexes {} verses, the codec {seen}",
                cases.len()
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    assert_eq!((drafted, undrafted, mid_line), (99, 351, 4));
}
