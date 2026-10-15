//! The port against its oracles: outputs that proskomma-core 0.11.3 and tC4's own
//! `mapReference` produced (`fixtures/versification/oracle/generate.mjs`).
//!
//! Each oracle file lists only the results that differ from "unchanged", over a stated
//! input domain. These tests run every input of the domain, so a result that should be
//! unchanged but is not fails as well.

// Test helpers fail the test by panicking.
#![allow(clippy::unwrap_used, clippy::panic)]

use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use tc_core::VerseSpan;
use tc_versification::{
    BookCode, MapOutcome, MappingTable, RefPart, RowReference, Scheme, SchemeReference,
    Unplaceable, VerseRef, Versification, map_reference, map_verse,
};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(fixtures().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn oracle(name: &str) -> Value {
    serde_json::from_str(&read(&format!("versification/oracle/{name}"))).unwrap()
}

/// The schemes the oracle names: `json:<name>` for tC4's scheme files, `vrs:<path>` for
/// the `.vrs` fixtures.
fn versification(name: &str) -> Versification {
    if let Some(n) = name.strip_prefix("json:") {
        let rel = if n == "syn" {
            "tc4/test-fixtures/vrs-synthetic/syn.json".to_owned()
        } else {
            format!("tc4/test-fixtures/vrs/{n}.json")
        };
        Versification::from_json(&read(&rel)).unwrap()
    } else if let Some(path) = name.strip_prefix("vrs:") {
        Versification::from_vrs(&read(&format!("versification/vrs/{path}.vrs"))).unwrap()
    } else {
        panic!("unknown scheme {name}")
    }
}

/// A scheme a pair names: a bare name is one of tC4's JSON schemes, `vrs:<path>` a `.vrs`
/// fixture.
fn scheme(name: &str) -> Scheme {
    let source = if name.starts_with("vrs:") {
        name.to_owned()
    } else {
        format!("json:{name}")
    };
    Scheme::new(name, versification(&source)).unwrap()
}

fn book(code: &str) -> BookCode {
    BookCode::new(code).unwrap()
}

fn strings(v: &Value) -> Vec<&str> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect()
}

/// `B c:v`, or `B c:v1..v2` for a run, whose end comes back as `Some(v2)`.
fn parse_lhs(lhs: &str) -> (BookCode, u16, u16, Option<u16>) {
    let (b, cv) = lhs.split_once(' ').unwrap();
    let (c, v) = cv.split_once(':').unwrap();
    let (v1, run_end) = match v.split_once("..") {
        Some((a, z)) => (a.parse().unwrap(), Some(z.parse().unwrap())),
        None => (v.parse().unwrap(), None),
    };
    (book(b), c.parse().unwrap(), v1, run_end)
}

/// `B c:v[,c:v...]`: one hop's targets, all in book B.
fn parse_hop(text: &str) -> Vec<VerseRef> {
    let (b, cvs) = text.split_once(' ').unwrap();
    cvs.split(',')
        .map(|cv| {
            let (c, v) = cv.split_once(':').unwrap();
            VerseRef::new(book(b), c.parse().unwrap(), v.parse().unwrap())
        })
        .collect()
}

/// Expands `moved` lines into input -> landings (one list per hop).
fn expand_moves(lines: &[&str]) -> HashMap<VerseRef, Vec<Vec<VerseRef>>> {
    let mut out = HashMap::new();
    for line in lines {
        let (lhs, rhs) = line.split_once('=').unwrap();
        let (b, c, v1, run_end) = parse_lhs(lhs);
        let hops: Vec<Vec<VerseRef>> = rhs.split(';').map(parse_hop).collect();
        match run_end {
            None => {
                out.insert(VerseRef::new(b, c, v1), hops);
            }
            Some(v2) => {
                let start = hops[0][0];
                for v in v1..=v2 {
                    let target = VerseRef::new(start.book, start.chapter, start.verse + (v - v1));
                    out.insert(VerseRef::new(b, c, v), vec![vec![target]]);
                }
            }
        }
    }
    out
}

#[test]
fn vrs_parsing_matches_proskomma_vrs2json() {
    let oracle = oracle("vrs2json.json");
    let files = oracle["files"].as_object().unwrap();
    assert_eq!(files.len(), 10);
    for (rel, expected) in files {
        let v = Versification::from_vrs(&read(&format!("versification/{rel}"))).unwrap();
        let got: Vec<(String, Vec<String>)> = v
            .mappings()
            .iter()
            .map(|m| {
                (
                    m.source().to_string(),
                    m.targets().iter().map(ToString::to_string).collect(),
                )
            })
            .collect();
        let want: Vec<(String, Vec<String>)> = expected
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, t)| {
                (
                    k.clone(),
                    strings(t).into_iter().map(str::to_owned).collect(),
                )
            })
            .collect();
        assert_eq!(got, want, "{rel}");
    }
}

#[test]
fn vrs_verse_counts_match_the_json_conversion() {
    for name in ["eng", "lxx", "org", "rsc", "rso", "vul"] {
        let from_vrs = versification(&format!("vrs:proskomma/{name}"));
        let from_json = versification(&format!("json:{name}"));
        let vrs_books: Vec<BookCode> = from_vrs.books().collect();
        let json_books: Vec<BookCode> = from_json.books().collect();
        // The JSON conversion dropped three `vul` books whose `.vrs` lines end in a space.
        let dropped: Vec<BookCode> = vrs_books
            .iter()
            .copied()
            .filter(|b| !json_books.contains(b))
            .collect();
        let expected_dropped: Vec<BookCode> = if name == "vul" {
            vec![book("EST"), book("5EZ"), book("EZA")]
        } else {
            vec![]
        };
        assert_eq!(dropped, expected_dropped, "{name}");
        for b in json_books {
            if name == "vul" && b == book("6EZ") {
                // `6EZ 1:63 12:78`: the JSON conversion made 78 the count of chapter 2.
                let mut gap = vec![0; 12];
                gap[0] = 63;
                gap[11] = 78;
                assert_eq!(from_vrs.verse_counts(b), Some(gap.as_slice()));
                assert_eq!(from_json.verse_counts(b), Some([63, 78].as_slice()));
                continue;
            }
            assert_eq!(
                from_vrs.verse_counts(b),
                from_json.verse_counts(b),
                "{name} {b}"
            );
        }
    }
}

#[test]
fn every_table_lookup_matches_proskomma_map_verse() {
    let oracle = oracle("hops.json");
    let domain: Vec<(BookCode, u16, u16)> = strings(&oracle["domain"])
        .into_iter()
        .map(|d| {
            let (b, c, last, _) = parse_lhs(d);
            (b, c, last)
        })
        .collect();
    let tables = oracle["tables"].as_object().unwrap();
    assert_eq!(tables.len(), 17);
    let mut checked = 0usize;
    let mut moved = 0usize;
    for (name, directions) in tables {
        let v = versification(name);
        for (direction, result) in directions.as_object().unwrap() {
            let table = match direction.as_str() {
                "forward" => MappingTable::forward(v.mappings()),
                "reverse" => MappingTable::reverse(v.mappings()),
                other => panic!("unknown direction {other}"),
            };
            if let Some(err) = result.get("error") {
                assert!(
                    table.is_err(),
                    "{name} {direction}: Proskomma refused it ({err})"
                );
                continue;
            }
            let table = table.unwrap_or_else(|e| panic!("{name} {direction}: {e}"));
            let expected = expand_moves(&strings(&result["moved"]));
            moved += expected.len();
            for &(b, c, last) in &domain {
                for verse in 0..=last {
                    let input = VerseRef::new(b, c, verse);
                    let want = expected
                        .get(&input)
                        .map_or_else(|| vec![input], |h| h[0].clone());
                    let hop = table.apply(input);
                    assert_eq!(
                        hop.targets().collect::<Vec<_>>(),
                        want,
                        "{name} {direction} {input}"
                    );
                    checked += 1;
                }
            }
        }
    }
    assert!(moved > 50_000, "the oracle moved only {moved} verses");
    assert!(checked > 1_000_000, "checked only {checked} lookups");
}

#[test]
fn scheme_to_scheme_mapping_matches_proskomma_two_hops() {
    let oracle = oracle("compose.json");
    let pairs = oracle["pairs"].as_object().unwrap();
    let inputs = oracle["inputs"].as_object().unwrap();
    assert_eq!(pairs.len(), 52);
    let mut cache: HashMap<&str, Scheme> = HashMap::new();
    let mut fan_outs = 0usize;
    for (pair, lines) in pairs {
        let (from_name, to_name) = pair.split_once('>').unwrap();
        for name in [from_name, to_name] {
            cache.entry(name).or_insert_with(|| scheme(name));
        }
        let (from, to) = (&cache[from_name], &cache[to_name]);
        let expected = expand_moves(&strings(lines));
        fan_outs += expected.values().filter(|hops| hops.len() > 1).count();
        let mut checked = 0u64;
        for b in from.versification().books() {
            for (i, &last) in from
                .versification()
                .verse_counts(b)
                .unwrap()
                .iter()
                .enumerate()
            {
                let c = u16::try_from(i + 1).unwrap();
                for verse in 0..=last + 1 {
                    let input = VerseRef::new(b, c, verse);
                    let want = expected
                        .get(&input)
                        .cloned()
                        .unwrap_or_else(|| vec![vec![input]]);
                    let got: Vec<Vec<VerseRef>> = map_verse(from, to, input)
                        .landings
                        .into_iter()
                        .map(|l| l.targets)
                        .collect();
                    assert_eq!(got, want, "{pair} {input}");
                    checked += 1;
                }
            }
        }
        assert_eq!(
            Some(checked),
            inputs[pair].as_u64(),
            "{pair}: the oracle's input domain"
        );
    }
    // Proskomma's .vrs copies keep one-to-many entries, so some verses have several
    // pivot verses in org and come back through several reverse hops.
    assert!(fan_outs > 0, "no input had more than one pivot verse");
}

fn reason(text: &str) -> Unplaceable {
    match text {
        "verse-zero" => Unplaceable::VerseZero,
        "no-chapter" => Unplaceable::NoChapter,
        "past-chapter-end" => Unplaceable::PastChapterEnd,
        "ambiguous" => Unplaceable::Ambiguous,
        "span-split" => Unplaceable::SpanSplit,
        "malformed-reference" => Unplaceable::MalformedReference,
        other => panic!("unexpected tC4 reason {other}"),
    }
}

fn span(text: &str) -> VerseSpan {
    text.parse().unwrap()
}

/// `B c:verse` (mapped) or `!reason[ B c:v,B c:v...]` (refused).
fn parse_outcome(text: &str) -> MapOutcome {
    if let Some(refusal) = text.strip_prefix('!') {
        let (why, cands) = refusal.split_once(' ').unwrap_or((refusal, ""));
        let candidates = if cands.is_empty() {
            vec![]
        } else {
            cands.split(',').map(|c| c.parse().unwrap()).collect()
        };
        return MapOutcome::Unplaceable {
            reason: reason(why),
            candidates,
        };
    }
    let (b, cv) = text.split_once(' ').unwrap();
    let (c, v) = cv.split_once(':').unwrap();
    MapOutcome::Mapped(SchemeReference {
        book: book(b),
        chapter: c.parse().unwrap(),
        verses: span(v),
    })
}

fn verses_of(first: u16, width: u16) -> VerseSpan {
    VerseSpan::range(first, first + width - 1).unwrap()
}

#[test]
fn map_reference_matches_tc4() {
    let oracle = oracle("tc4-map-reference.json");
    let pairs = oracle["pairs"].as_object().unwrap();
    assert_eq!(pairs.len(), 12);
    let mut refusals = 0usize;
    for (pair, sections) in pairs {
        let (from_name, to_name) = pair.split_once('>').unwrap();
        let (from, to) = (scheme(from_name), scheme(to_name));
        let books: Vec<BookCode> = from
            .versification()
            .books()
            .filter(|b| to.versification().verse_counts(*b).is_some())
            .collect();
        for width in 1..=3u16 {
            let mut expected: HashMap<(BookCode, u16, u16), MapOutcome> = HashMap::new();
            for line in strings(&sections[format!("width{width}").as_str()]) {
                let (lhs, rhs) = line.split_once('=').unwrap();
                let (b, c, v1, run_end) = parse_lhs(lhs);
                let outcome = parse_outcome(rhs);
                let Some(v2) = run_end else {
                    expected.insert((b, c, v1), outcome);
                    continue;
                };
                let MapOutcome::Mapped(start) = outcome else {
                    panic!("{pair}: a run must map: {line}")
                };
                for v in v1..=v2 {
                    let mapped = SchemeReference {
                        verses: verses_of(start.verses.start() + (v - v1), width),
                        ..start
                    };
                    expected.insert((b, c, v), MapOutcome::Mapped(mapped));
                }
            }
            refusals += expected
                .values()
                .filter(|o| matches!(o, MapOutcome::Unplaceable { .. }))
                .count();
            for &b in &books {
                for (i, &last) in from
                    .versification()
                    .verse_counts(b)
                    .unwrap()
                    .iter()
                    .enumerate()
                {
                    let c = u16::try_from(i + 1).unwrap();
                    for v in (1..=last).take_while(|v| v + width - 1 <= last) {
                        let input = SchemeReference {
                            book: b,
                            chapter: c,
                            verses: verses_of(v, width),
                        };
                        let want = expected
                            .remove(&(b, c, v))
                            .unwrap_or(MapOutcome::Mapped(input));
                        assert_eq!(
                            map_reference(&from, &to, &input.into()),
                            want,
                            "{pair} {input}"
                        );
                    }
                }
            }
            assert!(
                expected.is_empty(),
                "{pair} width {width}: oracle inputs outside the domain"
            );
        }
        for line in strings(&sections["chapter"]) {
            let (lhs, rhs) = line.split_once('=').unwrap();
            let (b, cv) = lhs.split_once(' ').unwrap();
            let (c, v) = cv.split_once(':').unwrap();
            let input = SchemeReference {
                book: book(b),
                chapter: c.parse().unwrap(),
                verses: span(v),
            };
            assert_eq!(
                map_reference(&from, &to, &input.into()),
                parse_outcome(rhs),
                "{pair} {input}"
            );
        }
    }
    assert!(
        refusals > 5_000,
        "the oracle refused only {refusals} references"
    );
}

/// tC4's outcome text, as `generate.mjs` writes it.
fn outcome_text(outcome: &MapOutcome) -> String {
    match outcome {
        MapOutcome::SameFrame(r) => format!("{r} unmapped"),
        MapOutcome::Mapped(r) => r.to_string(),
        MapOutcome::Unplaceable { reason, candidates } => {
            let name = match reason {
                Unplaceable::VerseZero => "verse-zero",
                Unplaceable::NoChapter => "no-chapter",
                Unplaceable::PastChapterEnd => "past-chapter-end",
                Unplaceable::Ambiguous => "ambiguous",
                Unplaceable::SpanSplit => "span-split",
                Unplaceable::MalformedReference => "malformed-reference",
            };
            let candidates: Vec<String> = candidates.iter().map(ToString::to_string).collect();
            if candidates.is_empty() {
                format!("!{name}")
            } else {
                format!("!{name} {}", candidates.join(","))
            }
        }
    }
}

#[test]
fn row_reference_forms_match_tc4() {
    let oracle = oracle("tc4-ref-forms.json");
    let pairs = oracle["pairs"].as_object().unwrap();
    assert_eq!(pairs.len(), 6);
    let mut same_frame = 0usize;
    let mut malformed = 0usize;
    for (pair, lines) in pairs {
        let (from_name, to_name) = pair.split_once('>').unwrap();
        let (from, to) = (scheme(from_name), scheme(to_name));
        for line in strings(lines) {
            let (input, want) = line.split_once('=').unwrap();
            let (b, cv) = input.split_once(' ').unwrap();
            let (c, v) = cv.split_once(':').unwrap();
            let reference = RowReference {
                book: book(b),
                chapter: RefPart::parse(c),
                verse: RefPart::parse(v),
            };
            let got = map_reference(&from, &to, &reference);
            same_frame += usize::from(matches!(got, MapOutcome::SameFrame(_)));
            malformed += usize::from(want == "!malformed-reference");
            assert_eq!(outcome_text(&got), want, "{pair} {input}");
        }
    }
    assert!(same_frame >= 30, "only {same_frame} same-frame inputs");
    assert!(malformed >= 50, "only {malformed} malformed inputs");
}

#[test]
fn same_frame_passes_through_untouched() {
    // A composed eng -> org -> eng round trip moves NEH 7:68 to 7:69; the same frame must
    // not map at all.
    let eng = scheme("eng");
    let input: SchemeReference =
        SchemeReference::from("NEH 7:68".parse::<tc_core::Reference>().unwrap());
    assert_eq!(
        map_reference(&eng, &eng, &input.into()),
        MapOutcome::SameFrame(input.into())
    );
    let composed = map_verse(&eng, &eng, "NEH 7:68".parse().unwrap());
    assert_eq!(
        composed.targets().collect::<Vec<_>>(),
        vec!["NEH 7:69".parse().unwrap()]
    );
    assert!(!composed.is_lossless());
}
