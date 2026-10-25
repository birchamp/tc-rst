//! The text tC4's tC3 importer makes of a book's chapter files, usfm-js 3.4.3's
//! `toUSFM(…, { forcedNewLines: true })`, against `fixtures/usfm/tc3-usfm-vectors.json`:
//! every book of the tC3 fixture zips and hand-written books.

// Helpers outside `#[test]` functions read fixtures and may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use md5::{Digest, Md5};
use serde_json::Value;
use std::io::Read;
use std::path::Path;
use tc_usfm::tc3_book_usfm;

type Chapters = Vec<(String, Vec<(String, String)>)>;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn vectors() -> Value {
    let text = std::fs::read_to_string(root().join("fixtures/usfm/tc3-usfm-vectors.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

/// A header object as its string-valued fields.
fn header(value: &Value) -> Vec<(String, String)> {
    value
        .as_object()
        .unwrap()
        .iter()
        .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_owned())))
        .collect()
}

fn pairs(value: &Value) -> Vec<(String, String)> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p[0].as_str().unwrap().to_owned(),
                p[1].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn first_difference(name: &str, got: &str, want: &str) -> Option<String> {
    if got == want {
        return None;
    }
    let at = got
        .bytes()
        .zip(want.bytes())
        .position(|(a, b)| a != b)
        .unwrap_or(got.len().min(want.len()));
    let around = |s: &str| {
        let from = s.floor_char_boundary(at.saturating_sub(40));
        let to = s.ceil_char_boundary((at + 40).min(s.len()));
        format!("{:?}", &s[from..to])
    };
    Some(format!(
        "{name}: first difference at byte {at}\n  got  {}\n  want {}",
        around(got),
        around(want)
    ))
}

#[test]
fn hand_written_books_match_tc4() {
    let vectors = vectors();
    let mut failures = Vec::new();
    let cases = vectors["cases"].as_array().unwrap();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let headers: Vec<_> = case["headers"]
            .as_array()
            .unwrap()
            .iter()
            .map(header)
            .collect();
        let chapters: Chapters = case["chapters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| (c[0].as_str().unwrap().to_owned(), pairs(&c[1])))
            .collect();
        let got = tc3_book_usfm(&headers, &chapters);
        let want = case["usfm"].as_str();
        match (got.as_deref(), want) {
            (Some(got), Some(want)) => failures.extend(first_difference(name, got, want)),
            (None, None) => {}
            (got, want) => failures.push(format!("{name}: got {got:?}, want {want:?}")),
        }
    }
    assert!(cases.len() > 30);
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

/// A book of a tC3 project zip: its headers and its chapter files, as tC4's `bookUsfm`
/// reads them. serde_json orders a file's keys by text; JavaScript's order, which the
/// function applies, is the same for keys that are numbers and `front`.
fn zip_book(zip: &str, book: &str) -> (Vec<Vec<(String, String)>>, Chapters) {
    let file = std::fs::File::open(root().join(zip)).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().to_owned())
        .collect();
    let mut read = |name: &str| -> Value {
        let mut text = String::new();
        archive
            .by_name(name)
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        serde_json::from_str(&text).unwrap()
    };
    let headers = read(&format!("{book}/headers.json"))
        .as_array()
        .unwrap()
        .iter()
        .map(header)
        .collect();
    let chapters = names
        .iter()
        .filter_map(|n| {
            let stem = n.strip_prefix(&format!("{book}/"))?.strip_suffix(".json")?;
            (!stem.is_empty() && stem.bytes().all(|b| b.is_ascii_digit())).then_some((n, stem))
        })
        .map(|(n, stem)| {
            let verses = read(n)
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
                .collect();
            (stem.parse::<u64>().unwrap().to_string(), verses)
        })
        .collect();
    (headers, chapters)
}

#[test]
fn tc3_fixture_books_match_tc4() {
    let vectors = vectors();
    let books = vectors["books"].as_array().unwrap();
    assert_eq!(books.len(), 4);
    for book in books {
        let name = book["book"].as_str().unwrap();
        let (headers, chapters) = zip_book(book["zip"].as_str().unwrap(), name);
        let got = tc3_book_usfm(&headers, &chapters).unwrap();
        if let Some(want) = book["usfm"].as_str()
            && let Some(diff) = first_difference(name, &got, want)
        {
            panic!("{diff}");
        }
        assert_eq!(
            got.len() as u64,
            book["length"].as_u64().unwrap(),
            "{name}: length"
        );
        assert_eq!(
            Md5::digest(got.as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            book["md5"].as_str().unwrap(),
            "{name}: MD5"
        );
    }
}
