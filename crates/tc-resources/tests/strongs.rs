//! The Strong's rule against what tC4's code computes (`fixtures/lexicon/strongs-vectors.json`,
//! made by `crates/tc-resources/tools/gen-strongs-vectors.cjs` from word-aligner-lib 1.0.1):
//! the edge cases, and every `strong` attribute of the bundled UHB and UGNT looked up in the
//! bundled lexicons.

// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use common::bundled::{UGL, UHL, bundle};
use serde::Deserialize;
use serde_json::Value;
use tc_resources::strongs::{
    LexiconId, StrongsNumber, StrongsPart, contains_valid_strongs_number, strongs_parts,
};
use tc_resources::{Lexicons, PartEntry, Sha256};

#[derive(Deserialize)]
struct Vectors {
    cases: Vec<Case>,
    corpora: Vec<Corpus>,
}

#[derive(Deserialize)]
struct Case {
    strong: String,
    parts: Vec<(String, String, Value)>,
    valid: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Corpus {
    archive: String,
    distinct: usize,
    occurrences: u64,
    parts_with_number: u64,
    parts_found: u64,
    not_found: BTreeMap<String, u64>,
    sha256: String,
}

fn vectors() -> Vectors {
    serde_json::from_slice(&common::fixture("lexicon/strongs-vectors.json")).unwrap()
}

/// A part as the vectors write it: tC4's lexicon id and entry id, with "out of range" for
/// an id no entry can have.
fn encoded(part: &StrongsPart<'_>) -> (String, String, Value) {
    let number = match part.number {
        None => Value::from(0),
        Some(StrongsNumber::Entry(n)) => Value::from(n),
        Some(StrongsNumber::OutOfRange) => Value::from("out of range"),
    };
    (
        part.text.to_owned(),
        part.lexicon.as_str().to_owned(),
        number,
    )
}

#[test]
fn every_edge_case_gives_what_tc4_gives() {
    let cases = vectors().cases;
    assert!(cases.len() > 90);
    for case in cases {
        let parts: Vec<_> = strongs_parts(&case.strong).iter().map(encoded).collect();
        assert_eq!(parts, case.parts, "{:?}", case.strong);
        assert_eq!(
            contains_valid_strongs_number(&case.strong),
            case.valid,
            "{:?}",
            case.strong
        );
    }
}

/// Every `strong="…"` value in the USFM files of a burrito export, as the generator's
/// regular expression `/strong="([^"]*)"/g` finds them.
fn strong_attributes(archive: &str) -> BTreeMap<String, u64> {
    let bytes = std::fs::read(bundle().join(archive)).unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut counts = BTreeMap::new();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).unwrap();
        if !file.name().ends_with(".usfm") {
            continue;
        }
        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();
        let mut rest = text.as_str();
        while let Some(at) = rest.find("strong=\"") {
            rest = &rest[at + "strong=\"".len()..];
            let Some(end) = rest.find('"') else { break };
            *counts.entry(rest[..end].to_owned()).or_default() += 1;
            rest = &rest[end + 1..];
        }
    }
    counts
}

#[test]
fn every_word_of_the_bundled_originals_finds_what_tc4_finds() {
    let lexicons = Lexicons::from_parts(Ok(UGL.lexicon()), Ok(UHL.lexicon()));
    let corpora = vectors().corpora;
    assert_eq!(corpora.len(), 2);
    for corpus in corpora {
        let counts = strong_attributes(&corpus.archive);
        assert_eq!(counts.len(), corpus.distinct, "{}", corpus.archive);
        let mut lines = String::new();
        let (mut occurrences, mut with_number, mut found) = (0, 0, 0);
        let mut not_found = BTreeMap::new();
        // BTreeMap<String, _> orders by UTF-8 bytes, as the generator sorts.
        for (strong, &n) in &counts {
            occurrences += n;
            let parts: Vec<String> = strongs_parts(strong)
                .iter()
                .map(|p| {
                    let (_, id, entry) = encoded(p);
                    match entry {
                        Value::String(s) => format!("{id}:{s}"),
                        number => format!("{id}:{number}"),
                    }
                })
                .collect();
            lines.push_str(&format!("{strong}\t{}\n", parts.join(" ")));
            for lookup in lexicons.lookup(strong) {
                match lookup.entry {
                    PartEntry::NoNumber => {}
                    PartEntry::Found(_) => {
                        with_number += n;
                        found += n;
                    }
                    PartEntry::NotInLexicon => {
                        with_number += n;
                        *not_found.entry(lookup.part.text.to_owned()).or_default() += n;
                    }
                    PartEntry::Unavailable(why) => panic!("{why}"),
                }
            }
        }
        assert_eq!(
            Sha256::of(lines.as_bytes()).to_hex(),
            corpus.sha256,
            "{}: the parts of some attribute differ from tC4's",
            corpus.archive
        );
        assert_eq!(
            (occurrences, with_number, found),
            (
                corpus.occurrences,
                corpus.parts_with_number,
                corpus.parts_found
            ),
            "{}",
            corpus.archive
        );
        assert_eq!(not_found, corpus.not_found, "{}", corpus.archive);
    }
}

#[test]
fn greek_words_go_to_the_new_testament_lexicon_and_hebrew_words_to_the_old() {
    assert_eq!(LexiconId::Ugl.testament(), tc_core::Testament::New);
    assert_eq!(LexiconId::Uhl.testament(), tc_core::Testament::Old);
    let lexicons = Lexicons::from_parts(Ok(UGL.lexicon()), Ok(UHL.lexicon()));
    let [greek] = lexicons.lookup("G35880")[..] else {
        panic!("one part")
    };
    let PartEntry::Found(article) = greek.entry else {
        panic!("found")
    };
    assert_eq!(article.brief.as_deref(), Some("the"));
    let [_, _, earth] = lexicons.lookup("c:d:H0776")[..] else {
        panic!("three parts")
    };
    let PartEntry::Found(entry) = earth.entry else {
        panic!("found")
    };
    assert_eq!(entry.brief.as_deref(), Some("earth"));
}
