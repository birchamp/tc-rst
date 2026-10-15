//! A Psalms-sized aligned book parses well within the 100 ms budget in release builds.
//! Run `cargo test --release -p tc-usfm --test performance -- --nocapture` to see times.

// Helpers outside `#[test]` functions read fixtures and may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::time::{Duration, Instant};
use tc_usfm::{Book, Document};

/// Psalms: 150 chapters, 2461 verses, built from the aligned ULT verses of Titus (about
/// 3 KB of USFM per verse, as in the aligned ULT), so about 7.7 MB in all.
fn psalms_sized_book() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/tc4/test-fixtures/en_ult/TIT.usfm");
    let titus = std::fs::read_to_string(path).unwrap();
    let book = Book::parse(&titus);
    let bodies: Vec<&str> = book.verses().map(|u| u.body()).collect();
    let mut out = String::from("\\id PSA EN_ULT\n\\usfm 3.0\n\\h Psalms\n\\mt1 Psalms\n");
    let mut verses = 0;
    for chapter in 1..=150 {
        out.push_str(&format!("\\ts\\*\n\\c {chapter}\n\\d A psalm.\n\\q1\n"));
        let in_chapter = match chapter {
            119 => 176,
            1..=50 => 16,
            _ => 15,
        };
        for verse in 1..=in_chapter {
            if verse % 3 == 0 {
                out.push_str("\\q2\n");
            }
            out.push_str(&format!("\\v {verse} {}", bodies[verses % bodies.len()]));
            verses += 1;
        }
    }
    out
}

#[test]
fn a_psalms_sized_book_parses_quickly() {
    let text = psalms_sized_book();
    // The best of several runs, because other processes share the machine. A debug build
    // only checks that the code runs, so one run is enough there.
    let runs = if cfg!(debug_assertions) { 1 } else { 5 };
    let best = |run: &dyn Fn()| {
        (0..runs)
            .map(|_| {
                let start = Instant::now();
                run();
                start.elapsed()
            })
            .min()
            .unwrap_or(Duration::MAX)
    };
    let book = Book::parse(&text);
    let units = book.verses().count();
    assert_eq!(units, 2461);
    let document = best(&|| {
        Document::parse(&text);
    });
    let segments = best(&|| {
        Book::parse(&text);
    });
    let strip = best(&|| {
        for unit in book.verses() {
            tc_usfm::parse_aligned(unit.body()).unwrap();
        }
    });
    let replace = best(&|| {
        book.replace_body(119, "176".parse().unwrap(), "Done.\n")
            .unwrap();
    });
    eprintln!(
        "{} bytes, {units} verse units: document {document:?}, book segments {segments:?}, \
         strip every verse {strip:?}, replace one verse {replace:?}",
        text.len()
    );
    if !cfg!(debug_assertions) {
        assert!(segments < Duration::from_millis(100), "{segments:?}");
    }
}
